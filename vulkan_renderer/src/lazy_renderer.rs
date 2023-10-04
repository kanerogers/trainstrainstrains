use crate::{
    buffer::Buffer,
    descriptors::Descriptors,
    vulkan_context::VulkanContext,
    vulkan_texture::{VulkanTexture, VulkanTextureCreateInfo},
    LineVertex, NO_TEXTURE_ID,
};
use common::{glam, thunderdome, Camera, GeometryOffsets};
use components::{GLTFAsset, GLTFModel, Material, MaterialOverrides, Transform, Vertex};

use std::{collections::HashMap, ffi::CStr};

use ash::vk;
use bytemuck::{Pod, Zeroable};
use vk_shader_macros::include_glsl;

const VERTEX_SHADER: &[u32] = include_glsl!("src/shaders/shader.vert");
const FRAGMENT_SHADER: &[u32] = include_glsl!("src/shaders/shader.frag");
const LINE_VERTEX_SHADER: &[u32] = include_glsl!("src/shaders/line.vert");
const LINE_FRAGMENT_SHADER: &[u32] = include_glsl!("src/shaders/line.frag");
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

/// HELLO WOULD YOU LIKE TO RENDER SOME THINGS????
pub struct LazyRenderer {
    /// The render pass used to draw
    render_pass: vk::RenderPass,
    /// One or more framebuffers to draw on. This will match the number of `vk::ImageView` present in [`RenderSurface`]
    framebuffers: Vec<vk::Framebuffer>,
    /// The surface to draw on. Currently only supports present surfaces (ie. the swapchain)
    pub render_surface: RenderSurface,
    /// The pipeline layout used to draw
    mesh_pipeline_layout: vk::PipelineLayout,
    /// The graphics pipeline used to draw meshes
    mesh_pipeline: vk::Pipeline,
    /// The pipeline layout used to draw LINES
    _line_pipeline_layout: vk::PipelineLayout,
    /// The graphics pipeline used to draw lines. It has a funny name.
    line_pipeline: vk::Pipeline,
    /// A single vertex buffer, shared between all draw calls
    pub line_vertex_buffer: Buffer<crate::LineVertex>,
    /// Textures owned by the user
    user_textures: thunderdome::Arena<VulkanTexture>,
    /// A wrapper around the things you need for geometry
    geometry_buffers: GeometryBuffers,
    /// A wrapper around descriptor set functionality
    pub descriptors: Descriptors,
    /// You know. A camera.
    pub camera: Camera,
    materials: thunderdome::Arena<GPUMaterial>,
    asset_cache: HashMap<String, LoadedGLTFModel>,
}

#[derive(Clone)]
/// The surface for lazy-vulkan to draw on. Currently only supports present surfaces (ie. the swapchain)
pub struct RenderSurface {
    /// The resolution of the surface
    pub resolution: vk::Extent2D,
    /// The image format of the surface
    pub format: vk::Format,
    /// The image views to render to. One framebuffer will be created per view
    pub image_views: Vec<vk::ImageView>,
    /// The depth buffers; one per view
    pub depth_buffers: Vec<DepthBuffer>,
}

struct GeometryBuffers {
    /// A single index buffer, shared between all draw calls
    pub index_buffer: Buffer<u32>,
    /// A single vertex buffer, shared between all draw calls
    pub vertex_buffer: Buffer<Vertex>,
    /// Some trivial geometry
    pub geometry_offsets: thunderdome::Arena<GeometryOffsets>,
}

impl GeometryBuffers {
    pub fn new(vulkan_context: &VulkanContext) -> Self {
        Self {
            index_buffer: Buffer::new(vulkan_context, vk::BufferUsageFlags::INDEX_BUFFER, &[]),
            vertex_buffer: Buffer::new(vulkan_context, vk::BufferUsageFlags::VERTEX_BUFFER, &[]),
            geometry_offsets: Default::default(),
        }
    }

    pub fn insert(&mut self, indices: &[u32], vertices: &[Vertex]) -> thunderdome::Index {
        let index_count = indices.len();
        let vertex_count = vertices.len();
        let index_offset = self.index_buffer.len();
        let vertex_offset = self.vertex_buffer.len();

        unsafe {
            self.index_buffer.append(indices);
            self.vertex_buffer.append(vertices);
        }

        self.geometry_offsets.insert(GeometryOffsets::new(
            index_offset,
            index_count,
            vertex_offset,
            vertex_count,
        ))
    }

    pub fn get<'a>(&'a self, index: thunderdome::Index) -> Option<&'a GeometryOffsets> {
        self.geometry_offsets.get(index)
    }

    unsafe fn cleanup(&self, device: &ash::Device) {
        self.index_buffer.cleanup(device);
        self.vertex_buffer.cleanup(device);
    }
}

impl RenderSurface {
    pub fn new(
        vulkan_context: &VulkanContext,
        resolution: vk::Extent2D,
        format: vk::Format,
        image_views: Vec<vk::ImageView>,
    ) -> Self {
        let depth_buffers = create_depth_buffers(vulkan_context, resolution, image_views.len());
        Self {
            resolution,
            format,
            image_views,
            depth_buffers,
        }
    }

    /// Safety:
    ///
    /// After you call this method.. like.. don't use this struct again, basically.
    unsafe fn destroy(&mut self, device: &ash::Device) {
        self.image_views
            .drain(..)
            .for_each(|v| device.destroy_image_view(v, None));
        self.depth_buffers.drain(..).for_each(|d| {
            d.destory(device);
        });
    }
}

impl From<&RenderSurface> for yakui_vulkan::RenderSurface {
    fn from(surface: &RenderSurface) -> Self {
        yakui_vulkan::RenderSurface {
            resolution: surface.resolution,
            format: surface.format,
            image_views: surface.image_views.clone(),
            load_op: vk::AttachmentLoadOp::DONT_CARE,
        }
    }
}

#[derive(Clone)]
pub struct DepthBuffer {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
}
impl DepthBuffer {
    unsafe fn destory(&self, device: &ash::Device) {
        device.destroy_image_view(self.view, None);
        device.destroy_image(self.image, None);
        device.free_memory(self.memory, None);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
/// Push constants!
struct PushConstant {
    material: GPUMaterial,
    view_pos: glam::Vec4,
    mvp: glam::Mat4,
    time_of_day: f32,
}

unsafe impl Zeroable for PushConstant {}
unsafe impl Pod for PushConstant {}

impl PushConstant {
    pub fn new(
        material: GPUMaterial,
        time_of_day: f32,
        view_pos: glam::Vec4,
        mvp: glam::Mat4,
    ) -> Self {
        Self {
            material,
            view_pos,
            mvp,
            time_of_day,
        }
    }
}

#[derive(Debug, Clone)]
struct LoadedGLTFModel {
    primitives: Vec<GPUPrimitive>,
}

#[derive(Debug, Clone)]
struct GPUPrimitive {
    pub geometry: thunderdome::Index,
    pub material: thunderdome::Index,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GPUMaterial {
    pub emissive_texture_id: u32,
    pub metallic_roughness_ao_texture_id: u32,
    pub normal_texture_id: u32,
    pub base_colour_texture_id: u32,
    pub base_colour_factor: glam::Vec4,
}

#[derive(Debug, Clone)]
pub struct DrawCall {
    pub geometry: thunderdome::Index,
    pub material: thunderdome::Index,
    pub transform: glam::Mat4,
    pub material_overrides: Option<MaterialOverrides>,
}

impl LazyRenderer {
    /// Create a new [`LazyRenderer`] instance. Currently only supports rendering directly to the swapchain.
    ///
    /// ## Safety
    /// - `vulkan_context` must have valid members
    /// - the members of `render_surface` must have been created with the same [`ash::Device`] as `vulkan_context`.
    pub fn new(vulkan_context: &VulkanContext, render_surface: RenderSurface) -> Self {
        let device = &vulkan_context.device;
        let descriptors = Descriptors::new(vulkan_context);
        let final_layout = vk::ImageLayout::PRESENT_SRC_KHR;

        let renderpass_attachments = [
            vk::AttachmentDescription {
                format: render_surface.format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout,
                ..Default::default()
            },
            vk::AttachmentDescription {
                format: DEPTH_FORMAT,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                ..Default::default()
            },
        ];

        let color_attachment_refs = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];

        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };

        let dependencies = [
            vk::SubpassDependency {
                src_subpass: vk::SUBPASS_EXTERNAL,
                src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                ..Default::default()
            },
            vk::SubpassDependency {
                src_subpass: vk::SUBPASS_EXTERNAL,
                src_stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                dst_access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                ..Default::default()
            },
        ];

        let subpass = vk::SubpassDescription::builder()
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let renderpass_create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&renderpass_attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(&dependencies);

        let render_pass = unsafe {
            device
                .create_render_pass(&renderpass_create_info, None)
                .unwrap()
        };

        let framebuffers = create_framebuffers(&render_surface, render_pass, device);

        let geometry_buffers = GeometryBuffers::new(vulkan_context);

        let line_vertex_buffer =
            Buffer::new(vulkan_context, vk::BufferUsageFlags::VERTEX_BUFFER, &[]);

        let (mesh_pipeline_layout, mesh_pipeline) =
            create_mesh_pipeline(device, &descriptors, &render_surface, render_pass);
        let (line_pipeline_layout, line_pipeline) =
            create_line_pipeline(device, &render_surface, render_pass);

        Self {
            render_pass,
            descriptors,
            framebuffers,
            render_surface,
            mesh_pipeline_layout,
            mesh_pipeline,
            line_pipeline,
            _line_pipeline_layout: line_pipeline_layout,
            geometry_buffers,
            line_vertex_buffer,
            user_textures: Default::default(),
            camera: Default::default(),
            materials: Default::default(),
            asset_cache: Default::default(),
        }
    }

    /// Render the meshes we've been given
    pub fn _render(
        &self,
        vulkan_context: &VulkanContext,
        framebuffer_index: u32,
        draw_calls: &[DrawCall],
        line_vertices: &[LineVertex],
        time_of_day: f32,
    ) {
        let device = &vulkan_context.device;
        let command_buffer = vulkan_context.draw_command_buffer;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let surface = &self.render_surface;

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[framebuffer_index as usize])
            .render_area(surface.resolution.into())
            .clear_values(&clear_values);

        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: surface.resolution.width as f32,
            height: surface.resolution.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];

        unsafe {
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.mesh_pipeline,
            );
            device.cmd_set_viewport(command_buffer, 0, &viewports);
            let default_scissor = [surface.resolution.into()];

            // We set the scissor first here as it's against the spec not to do so.
            device.cmd_set_scissor(command_buffer, 0, &default_scissor);
            device.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &[self.geometry_buffers.vertex_buffer.handle],
                &[0],
            );
            device.cmd_bind_index_buffer(
                command_buffer,
                self.geometry_buffers.index_buffer.handle,
                0,
                vk::IndexType::UINT32,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.mesh_pipeline_layout,
                0,
                std::slice::from_ref(&self.descriptors.set),
                &[],
            );

            let vp = self.camera.projection * self.camera.matrix();

            for draw_call in draw_calls {
                let mvp = vp * draw_call.transform;
                let mut material = self.materials.get(draw_call.material).unwrap().clone();
                if let Some(material_overrides) = &draw_call.material_overrides {
                    material.base_colour_factor = material_overrides.base_colour_factor;
                }

                device.cmd_push_constants(
                    command_buffer,
                    self.mesh_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::bytes_of(&PushConstant::new(
                        material,
                        time_of_day,
                        self.camera.position.extend(1.),
                        mvp,
                    )),
                );

                let GeometryOffsets {
                    index_count,
                    index_offset,
                    vertex_offset,
                    ..
                } = self.geometry_buffers.get(draw_call.geometry).unwrap();

                // Draw the mesh with the indexes we were provided
                device.cmd_draw_indexed(
                    command_buffer,
                    *index_count,
                    1,
                    *index_offset,
                    *vertex_offset as _,
                    1,
                );
            }

            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.line_pipeline,
            );
            device.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &[self.line_vertex_buffer.handle],
                &[0],
            );

            // most of these attributes are ignored but.. I'm lazy
            // TODO
            // device.cmd_push_constants(
            //     command_buffer,
            //     self.mesh_pipeline_layout,
            //     vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            //     0,
            //     bytemuck::bytes_of(&PushConstant::new(NO_TEXTURE_ID, vp, Default::default())),
            // );
            device.cmd_draw(command_buffer, (line_vertices.len() * 2) as u32, 1, 0, 1);
            device.cmd_end_render_pass(command_buffer);
        }
    }

    /// Add a "user managed" texture to this [`LazyRenderer`] instance. Returns a texture ID that can be used
    /// to refer to the texture in a push constant.
    ///
    /// ## Safety
    /// - `vulkan_context` must be the same as the one used to create this instance
    pub fn add_user_texture(
        &mut self,
        vulkan_context: &VulkanContext,
        texture_create_info: VulkanTextureCreateInfo<&[u8]>,
    ) -> u32 {
        let texture =
            VulkanTexture::new(vulkan_context, &mut self.descriptors, texture_create_info);
        let texture_id = texture.id;

        // TODO: do we even care about doing this, beyond cleanup?
        self.user_textures.insert(texture);
        texture_id
    }

    /// Clean up all Vulkan related handles on this instance. You'll probably want to call this when the program ends, but
    /// before you've cleaned up your [`ash::Device`], or you'll receive warnings from the Vulkan Validation Layers.
    ///
    /// ## Safety
    /// - After calling this function, this instance will be **unusable**. You **must not** make any further calls on this instance
    ///   or you will have a terrible time.
    /// - `device` must be the same [`ash::Device`] used to create this instance.
    pub unsafe fn cleanup(&self, device: &ash::Device) {
        device.device_wait_idle().unwrap();
        self.descriptors.cleanup(device);
        for (_, texture) in &self.user_textures {
            texture.cleanup(device);
        }
        device.destroy_pipeline_layout(self.mesh_pipeline_layout, None);
        device.destroy_pipeline_layout(self.mesh_pipeline_layout, None);
        device.destroy_pipeline(self.mesh_pipeline, None);
        device.destroy_pipeline(self.line_pipeline, None);
        self.geometry_buffers.cleanup(device);
        self.destroy_framebuffers(device);
        device.destroy_render_pass(self.render_pass, None);
    }

    /// Update the surface that this [`LazyRenderer`] instance will render to. You'll probably want to call
    /// this if the user resizes the window to avoid writing to an out-of-date swapchain.
    ///
    /// ## Safety
    /// - Care must be taken to ensure that the new [`RenderSurface`] points to images from a correct swapchain
    /// - You must use the same [`ash::Device`] used to create this instance
    pub fn update_surface(&mut self, render_surface: RenderSurface, device: &ash::Device) {
        unsafe {
            self.render_surface.destroy(device);
            self.destroy_framebuffers(device);
        }
        self.framebuffers = create_framebuffers(&render_surface, self.render_pass, device);
        self.render_surface = render_surface;
    }

    unsafe fn destroy_framebuffers(&self, device: &ash::Device) {
        for framebuffer in &self.framebuffers {
            device.destroy_framebuffer(*framebuffer, None);
        }
    }

    pub fn update_assets(
        &mut self,
        vulkan_context: &VulkanContext,
        world: &mut common::hecs::World,
    ) {
        let mut command_buffer = common::hecs::CommandBuffer::new();
        for (entity, (model, asset)) in world
            .query::<(&GLTFModel, &GLTFAsset)>()
            .without::<&LoadedGLTFModel>()
            .iter()
        {
            let asset_name = asset.name.clone();

            // check our asset cache *first*
            if let Some(cached_asset) = self.asset_cache.get(&asset_name) {
                command_buffer.insert_one(entity, cached_asset.clone());
                continue;
            }

            // not cached, import it
            let mut primitives = Vec::new();
            for primitive in model.primitives.iter() {
                let geometry = self
                    .geometry_buffers
                    .insert(&primitive.indices, &primitive.vertices);
                let material = self.import_material(&primitive.material, vulkan_context);
                primitives.push(GPUPrimitive { geometry, material });
            }
            let loaded_model = LoadedGLTFModel { primitives };

            self.asset_cache.insert(asset_name, loaded_model.clone());
            command_buffer.insert_one(entity, loaded_model);
        }

        command_buffer.run_on(world);
    }

    fn import_material(
        &mut self,
        material: &Material,
        vulkan_context: &VulkanContext,
    ) -> thunderdome::Index {
        let base_colour_texture_id = material
            .base_colour_texture
            .as_ref()
            .map(|t| self.add_user_texture(vulkan_context, t.into()))
            .unwrap_or(NO_TEXTURE_ID);

        let normal_texture_id = material
            .normal_texture
            .as_ref()
            .map(|t| self.add_user_texture(vulkan_context, t.into()))
            .unwrap_or(NO_TEXTURE_ID);

        let metallic_roughness_ao_texture_id = material
            .metallic_roughness_ao_texture
            .as_ref()
            .map(|t| self.add_user_texture(vulkan_context, t.into()))
            .unwrap_or(NO_TEXTURE_ID);

        let emissive_texture_id = material
            .emissive_texture
            .as_ref()
            .map(|t| self.add_user_texture(vulkan_context, t.into()))
            .unwrap_or(NO_TEXTURE_ID);

        let loaded_material = GPUMaterial {
            emissive_texture_id,
            metallic_roughness_ao_texture_id,
            normal_texture_id,
            base_colour_texture_id,
            base_colour_factor: material.base_colour_factor,
        };
        self.materials.insert(loaded_material)
    }

    pub fn build_draw_calls(&self, world: &common::hecs::World) -> Vec<DrawCall> {
        let mut draw_calls = Vec::new();
        for (_, (transform, model, material_overrides)) in world
            .query::<(&Transform, &LoadedGLTFModel, Option<&MaterialOverrides>)>()
            .iter()
        {
            for primitive in &model.primitives {
                draw_calls.push(DrawCall {
                    geometry: primitive.geometry,
                    material: primitive.material,
                    transform: transform.into(),
                    material_overrides: material_overrides.cloned(),
                });
            }
        }
        draw_calls
    }

    pub(crate) unsafe fn unload_assets(&mut self, vulkan_context: &VulkanContext) {
        let device = &vulkan_context.device;
        device.queue_wait_idle(vulkan_context.queue).unwrap();
        // OKIEDOKIE. We'll need to:
        // empty the geometry buffers
        self.geometry_buffers.cleanup(device);
        self.geometry_buffers = GeometryBuffers::new(vulkan_context);
        // empty the materials
        self.materials = Default::default();

        // empty the textures
        for (_, texture) in self.user_textures.drain() {
            texture.cleanup(device);
        }

        // empty the descriptors
        self.descriptors.cleanup(device);
        self.descriptors = Descriptors::new(vulkan_context);
        self.asset_cache = Default::default();
    }
}

fn create_line_pipeline(
    device: &ash::Device,
    render_surface: &RenderSurface,
    render_pass: vk::RenderPass,
) -> (vk::PipelineLayout, vk::Pipeline) {
    let vertex_shader_info = vk::ShaderModuleCreateInfo::builder().code(LINE_VERTEX_SHADER);
    let frag_shader_info = vk::ShaderModuleCreateInfo::builder().code(LINE_FRAGMENT_SHADER);

    let vertex_shader_module = unsafe {
        device
            .create_shader_module(&vertex_shader_info, None)
            .expect("Vertex shader module error")
    };

    let fragment_shader_module = unsafe {
        device
            .create_shader_module(&frag_shader_info, None)
            .expect("Fragment shader module error")
    };

    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&[
                    vk::PushConstantRange {
                        size: std::mem::size_of::<PushConstant>() as _,
                        stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        ..Default::default()
                    },
                ]),
                None,
            )
            .unwrap()
    };

    let shader_entry_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };
    let shader_stage_create_infos = [
        vk::PipelineShaderStageCreateInfo {
            module: vertex_shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::VERTEX,
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
            module: fragment_shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
    ];
    let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: std::mem::size_of::<LineVertex>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];

    let vertex_input_attribute_descriptions = [
        // position
        vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: bytemuck::offset_of!(LineVertex, position) as _,
        },
        // normals
        vk::VertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: bytemuck::offset_of!(LineVertex, colour) as _,
        },
    ];

    let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
        .vertex_binding_descriptions(&vertex_input_binding_descriptions);
    let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::LINE_LIST,
        ..Default::default()
    };
    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: render_surface.resolution.width as f32,
        height: render_surface.resolution.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissors = [render_surface.resolution.into()];
    let viewport_state_info = vk::PipelineViewportStateCreateInfo::builder()
        .scissors(&scissors)
        .viewports(&viewports);

    let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
        line_width: 1.0,
        polygon_mode: vk::PolygonMode::FILL,
        ..Default::default()
    };
    let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::TYPE_1,
        ..Default::default()
    };
    let noop_stencil_state = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::ALWAYS,
        ..Default::default()
    };
    let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: 0,
        depth_write_enable: 0,
        depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
        front: noop_stencil_state,
        back: noop_stencil_state,
        max_depth_bounds: 1.0,
        ..Default::default()
    };
    let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ONE,
        dst_alpha_blend_factor: vk::BlendFactor::ZERO,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    }];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
        .attachments(&color_blend_attachment_states);

    let line_pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&shader_stage_create_infos)
        .vertex_input_state(&vertex_input_state_info)
        .input_assembly_state(&vertex_input_assembly_state_info)
        .viewport_state(&viewport_state_info)
        .rasterization_state(&rasterization_info)
        .multisample_state(&multisample_state_info)
        .depth_stencil_state(&depth_state_info)
        .color_blend_state(&color_blend_state)
        .layout(pipeline_layout)
        .render_pass(render_pass);

    let graphics_pipelines = unsafe {
        device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[line_pipeline_info.build()],
                None,
            )
            .expect("Unable to create graphics pipeline")
    };

    let pipeline = graphics_pipelines[0];
    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }
    (pipeline_layout, pipeline)
}

fn create_mesh_pipeline(
    device: &ash::Device,
    descriptors: &Descriptors,
    render_surface: &RenderSurface,
    render_pass: vk::RenderPass,
) -> (vk::PipelineLayout, vk::Pipeline) {
    let vertex_shader_info = vk::ShaderModuleCreateInfo::builder().code(VERTEX_SHADER);
    let frag_shader_info = vk::ShaderModuleCreateInfo::builder().code(FRAGMENT_SHADER);

    let vertex_shader_module = unsafe {
        device
            .create_shader_module(&vertex_shader_info, None)
            .expect("Vertex shader module error")
    };

    let fragment_shader_module = unsafe {
        device
            .create_shader_module(&frag_shader_info, None)
            .expect("Fragment shader module error")
    };

    let mesh_pipeline_layout = unsafe {
        device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder()
                    .push_constant_ranges(&[vk::PushConstantRange {
                        size: std::mem::size_of::<PushConstant>() as _,
                        stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        ..Default::default()
                    }])
                    .set_layouts(std::slice::from_ref(&descriptors.layout)),
                None,
            )
            .unwrap()
    };

    let shader_entry_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };
    let shader_stage_create_infos = [
        vk::PipelineShaderStageCreateInfo {
            module: vertex_shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::VERTEX,
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
            module: fragment_shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
    ];
    let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: std::mem::size_of::<Vertex>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];

    let vertex_input_attribute_descriptions = [
        // position
        vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: bytemuck::offset_of!(Vertex, position) as _,
        },
        // normals
        vk::VertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: bytemuck::offset_of!(Vertex, normal) as _,
        },
        // UV
        vk::VertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: bytemuck::offset_of!(Vertex, uv) as _,
        },
    ];

    let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
        .vertex_binding_descriptions(&vertex_input_binding_descriptions);
    let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TRIANGLE_LIST,
        ..Default::default()
    };
    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: render_surface.resolution.width as f32,
        height: render_surface.resolution.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissors = [render_surface.resolution.into()];
    let viewport_state_info = vk::PipelineViewportStateCreateInfo::builder()
        .scissors(&scissors)
        .viewports(&viewports);

    let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
        front_face: vk::FrontFace::COUNTER_CLOCKWISE,
        line_width: 1.0,
        polygon_mode: vk::PolygonMode::FILL,
        ..Default::default()
    };
    let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::TYPE_1,
        ..Default::default()
    };
    let noop_stencil_state = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::ALWAYS,
        ..Default::default()
    };
    let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: 1,
        depth_write_enable: 1,
        depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
        front: noop_stencil_state,
        back: noop_stencil_state,
        max_depth_bounds: 1.0,
        ..Default::default()
    };
    let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ONE,
        dst_alpha_blend_factor: vk::BlendFactor::ZERO,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    }];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
        .attachments(&color_blend_attachment_states);

    let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state_info =
        vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_state);

    let mesh_pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&shader_stage_create_infos)
        .vertex_input_state(&vertex_input_state_info)
        .input_assembly_state(&vertex_input_assembly_state_info)
        .viewport_state(&viewport_state_info)
        .rasterization_state(&rasterization_info)
        .multisample_state(&multisample_state_info)
        .depth_stencil_state(&depth_state_info)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state_info)
        .layout(mesh_pipeline_layout)
        .render_pass(render_pass);

    let graphics_pipelines = unsafe {
        device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[mesh_pipeline_info.build()],
                None,
            )
            .expect("Unable to create graphics pipeline")
    };

    let mesh_pipeline = graphics_pipelines[0];
    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }
    (mesh_pipeline_layout, mesh_pipeline)
}

fn create_framebuffers(
    render_surface: &RenderSurface,
    render_pass: vk::RenderPass,
    device: &ash::Device,
) -> Vec<vk::Framebuffer> {
    let framebuffers: Vec<vk::Framebuffer> = render_surface
        .image_views
        .iter()
        .zip(&render_surface.depth_buffers)
        .map(|(&present_image_view, depth_buffer)| {
            let framebuffer_attachments = [present_image_view, depth_buffer.view];
            let frame_buffer_create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(&framebuffer_attachments)
                .width(render_surface.resolution.width)
                .height(render_surface.resolution.height)
                .layers(1);

            unsafe {
                device
                    .create_framebuffer(&frame_buffer_create_info, None)
                    .unwrap()
            }
        })
        .collect();
    framebuffers
}

fn create_depth_buffers(
    vulkan_context: &VulkanContext,
    resolution: vk::Extent2D,
    len: usize,
) -> Vec<DepthBuffer> {
    (0..len)
        .map(|_| {
            let (image, memory) =
                unsafe { vulkan_context.create_image(&[], resolution, DEPTH_FORMAT) };
            let view = unsafe { vulkan_context.create_image_view(image, DEPTH_FORMAT) };

            DepthBuffer {
                image,
                view,
                memory,
            }
        })
        .collect::<Vec<_>>()
}
