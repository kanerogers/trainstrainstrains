use crate::{metal_context::MetalContext, yakui_metal::YakuiMetal};
use common::{
    glam::{self, Mat4, Vec2, Vec3, Vec4},
    winit, yakui, Camera, Geometry, Mesh, Renderer,
};
use metal::CommandBufferRef;

impl Renderer for MetalRenderer {
    fn init(window: winit::window::Window) -> Self {
        let metal_context = MetalContext::new(window);
        MetalRenderer::new(metal_context)
    }

    fn render(&mut self, meshes: &[Mesh], camera: Camera, yak: &mut yakui::Yakui) {
        self.camera = camera;
        let context = &self.context;
        let drawable = match context.layer.next_drawable() {
            Some(drawable) => drawable,
            None => return,
        };

        let command_buffer = context.command_queue.new_command_buffer();
        self._render(meshes, drawable, command_buffer);

        self.yakui_metal
            .paint(context, yak, drawable, command_buffer);

        command_buffer.present_drawable(drawable);
        command_buffer.commit();
    }

    fn resized(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.context.resized(size);
    }
}

pub struct MetalRenderer {
    vertex_buffer: metal::Buffer,
    index_buffer: metal::Buffer,
    pipeline_state: metal::RenderPipelineState,
    geometry_offsets: GeometryOffsets,
    uniform_buffer: metal::Buffer,
    depth_stencil_state: metal::DepthStencilState,
    context: MetalContext,
    yakui_metal: YakuiMetal,
    pub camera: Camera,
}

const MAX_INSTANCES: usize = 10_000;

impl MetalRenderer {
    pub fn new(context: MetalContext) -> Self {
        let device = &context.device;
        let library_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/shaders/shaders.metallib");

        let library = device.new_library_with_file(library_path).unwrap();
        let triangle_pipeline_state =
            prepare_pipeline_state(&device, &library, "triangle_vertex", "triangle_fragment");

        let (indices, vertices, geometry_offsets) = create_initial_geometry();

        let vertex_buffer = {
            device.new_buffer_with_data(
                vertices.as_ptr() as *const _,
                (vertices.len() * std::mem::size_of::<Vertex>()) as u64,
                metal::MTLResourceOptions::CPUCacheModeDefaultCache
                    | metal::MTLResourceOptions::StorageModeManaged,
            )
        };

        let index_buffer = device.new_buffer_with_data(
            indices.as_ptr() as *const _,
            (indices.len() * std::mem::size_of::<u32>()) as u64,
            metal::MTLResourceOptions::CPUCacheModeDefaultCache
                | metal::MTLResourceOptions::StorageModeManaged,
        );

        let uniform_buffer = device.new_buffer(
            (MAX_INSTANCES * std::mem::size_of::<Uniforms>()) as _,
            metal::MTLResourceOptions::StorageModeShared,
        );

        let depth_stencil_desc = metal::DepthStencilDescriptor::new();
        depth_stencil_desc.set_depth_compare_function(metal::MTLCompareFunction::Less);
        depth_stencil_desc.set_depth_write_enabled(true);
        let depth_stencil_state = device.new_depth_stencil_state(&depth_stencil_desc);

        Self {
            vertex_buffer,
            index_buffer,
            yakui_metal: YakuiMetal::new(&context),
            pipeline_state: triangle_pipeline_state,
            geometry_offsets,
            uniform_buffer,
            camera: Default::default(),
            depth_stencil_state,
            context,
        }
    }

    fn _render(
        &self,
        meshes: &[Mesh],
        drawable: &metal::MetalDrawableRef,
        command_buffer: &metal::CommandBufferRef,
    ) {
        let context = &self.context;
        let render_pass_descriptor = metal::RenderPassDescriptor::new();

        prepare_render_pass_descriptor(
            &render_pass_descriptor,
            drawable.texture(),
            &context.depth_texture,
        );

        let encoder = command_buffer.new_render_command_encoder(&render_pass_descriptor);

        encoder.set_render_pipeline_state(&self.pipeline_state);
        encoder.set_vertex_buffer(0, Some(&self.vertex_buffer), 0);
        encoder.set_vertex_buffer(1, Some(&self.uniform_buffer), 0);
        encoder.set_depth_stencil_state(&self.depth_stencil_state);

        let screen_size = context.layer.drawable_size();
        let aspect_ratio = screen_size.width / screen_size.height;
        let perspective =
            glam::Mat4::perspective_rh(60_f32.to_radians(), aspect_ratio as f32, 0.01, 1000.);

        let uniforms = unsafe {
            std::slice::from_raw_parts_mut(
                self.uniform_buffer.contents() as *mut Uniforms,
                MAX_INSTANCES,
            )
        };
        for (instance_base, mesh) in meshes.iter().enumerate() {
            let geometry_offset = &self.geometry_offsets.get(mesh.geometry);
            let uniform = &mut uniforms[instance_base];
            uniform.mvp = perspective * self.camera.matrix() * mesh.transform;
            uniform.colour = mesh.colour.unwrap_or(Vec3::ONE).extend(1.);

            encoder.draw_indexed_primitives_instanced_base_instance(
                metal::MTLPrimitiveType::Triangle,
                geometry_offset.index_count as _,
                metal::MTLIndexType::UInt32,
                &self.index_buffer,
                (geometry_offset.index_offset as usize * std::mem::size_of::<u32>()) as _,
                1,
                geometry_offset.vertex_offset as _,
                instance_base as _,
            );
        }

        encoder.end_encoding();
    }
}

fn create_initial_geometry() -> (Vec<u32>, Vec<Vertex>, GeometryOffsets) {
    let mut vertices = vec![];
    let mut indices = vec![];

    let (plane_vertices, plane_indices) = generate_mesh(Geometry::Plane);
    let plane = IndexBufferEntry::new(plane_indices.len(), indices.len(), vertices.len());
    vertices.extend(plane_vertices);
    indices.extend(plane_indices);

    let (cube_vertices, cube_indices) = generate_mesh(Geometry::Cube);
    let cube = IndexBufferEntry::new(cube_indices.len(), indices.len(), vertices.len());
    vertices.extend(cube_vertices);
    indices.extend(cube_indices);

    let (sphere_vertices, sphere_indices) = generate_mesh(Geometry::Sphere);
    let sphere = IndexBufferEntry::new(sphere_indices.len(), indices.len(), vertices.len());
    vertices.extend(sphere_vertices);
    indices.extend(sphere_indices);

    let offsets = GeometryOffsets {
        plane,
        cube,
        sphere,
    };

    log::debug!("Created geometry offsets: {:?}", offsets);

    (indices, vertices, offsets)
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec4,
    pub normal: Vec4,
    pub uv: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexBufferEntry {
    pub index_count: u32,
    pub index_offset: u32,
    pub vertex_offset: u32,
}

impl IndexBufferEntry {
    pub fn new(index_count: usize, index_offset: usize, vertex_offset: usize) -> Self {
        Self {
            index_count: index_count as _,
            index_offset: index_offset as _,
            vertex_offset: vertex_offset as _,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GeometryOffsets {
    plane: IndexBufferEntry,
    cube: IndexBufferEntry,
    sphere: IndexBufferEntry,
}
impl GeometryOffsets {
    fn get(&self, geometry: Geometry) -> IndexBufferEntry {
        match geometry {
            Geometry::Plane => self.plane,
            Geometry::Sphere => self.sphere,
            Geometry::Cube => self.cube,
        }
    }
}

pub fn generate_mesh(geometry: Geometry) -> (Vec<Vertex>, Vec<u32>) {
    match geometry {
        Geometry::Plane => {
            let vertices = vec![
                Vertex {
                    position: Vec4::new(-1.0, -1.0, 0.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, -1.0, 0.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, 0.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, 0.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
            ];

            let indices = vec![0, 1, 2, 0, 2, 3];

            (vertices, indices)
        }

        Geometry::Sphere => {
            // Simplified UV Sphere
            let mut vertices = vec![];
            let mut indices = vec![];
            let sectors = 10;
            let stacks = 10;
            let radius = 1.0;
            let pi = std::f32::consts::PI;

            for i in 0..=stacks {
                let stack_angle = pi / 2.0 - i as f32 / stacks as f32 * pi; // starting from pi/2 to -pi/2
                let xy = radius * stack_angle.cos(); // r * cos(u)
                let z = radius * stack_angle.sin(); // r * sin(u)

                for j in 0..=sectors {
                    let sector_angle = j as f32 / sectors as f32 * pi * 2.0; // starting from 0 to 2pi

                    // vertex position (x, y, z)
                    let x = xy * sector_angle.cos(); // r * cos(u) * cos(v)
                    let y = xy * sector_angle.sin(); // r * cos(u) * sin(v)
                    vertices.push(Vertex {
                        position: Vec4::new(x, y, z, 1.0),
                        normal: Vec4::new(x, y, z, 0.0).normalize(), // normalized
                        uv: Vec2::new(j as f32 / sectors as f32, i as f32 / stacks as f32), // normalized
                    });

                    // indices
                    if i != 0 && j != 0 {
                        let a = (sectors + 1) * i + j; // current top right
                        let b = a - 1; // current top left
                        let c = a - (sectors + 1); // previous top right
                        let d = a - (sectors + 1) - 1; // previous top left
                        indices.push(a as u32);
                        indices.push(b as u32);
                        indices.push(c as u32);
                        indices.push(b as u32);
                        indices.push(d as u32);
                        indices.push(c as u32);
                    }
                }
            }

            (vertices, indices)
        }
        Geometry::Cube => {
            let vertices = vec![
                // Front face
                Vertex {
                    position: Vec4::new(-1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, 1.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
                // Right face
                Vertex {
                    position: Vec4::new(1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
                // Back face
                Vertex {
                    position: Vec4::new(1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 0.0, -1.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
                // Left face
                Vertex {
                    position: Vec4::new(-1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(-1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(-1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(-1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(-1.0, 0.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
                // Top face
                Vertex {
                    position: Vec4::new(-1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, 1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, 1.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
                // Bottom face
                Vertex {
                    position: Vec4::new(-1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, -1.0, -1.0, 1.0),
                    normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 0.0),
                },
                Vertex {
                    position: Vec4::new(1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
                    uv: Vec2::new(1.0, 1.0),
                },
                Vertex {
                    position: Vec4::new(-1.0, -1.0, 1.0, 1.0),
                    normal: Vec4::new(0.0, -1.0, 0.0, 0.0),
                    uv: Vec2::new(0.0, 1.0),
                },
            ];

            let indices = vec![
                0, 1, 2, 2, 3, 0, // front
                4, 5, 6, 6, 7, 4, // right
                8, 9, 10, 10, 11, 8, // back
                12, 13, 14, 14, 15, 12, // left
                16, 17, 18, 18, 19, 16, // top
                20, 21, 22, 22, 23, 20, // bottom
            ];

            (vertices, indices)
        }
    }
}

fn prepare_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    vertex_shader: &str,
    fragment_shader: &str,
) -> metal::RenderPipelineState {
    let vert = library.get_function(vertex_shader, None).unwrap();
    let frag = library.get_function(fragment_shader, None).unwrap();

    let pipeline_state_descriptor = metal::RenderPipelineDescriptor::new();
    pipeline_state_descriptor.set_vertex_function(Some(&vert));
    pipeline_state_descriptor.set_fragment_function(Some(&frag));
    pipeline_state_descriptor
        .set_depth_attachment_pixel_format(metal::MTLPixelFormat::Depth32Float);
    let colour_attachment = pipeline_state_descriptor
        .color_attachments()
        .object_at(0)
        .unwrap();
    colour_attachment.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);

    colour_attachment.set_blending_enabled(true);
    colour_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    colour_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    colour_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    colour_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    colour_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    colour_attachment
        .set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&pipeline_state_descriptor)
        .unwrap()
}

fn prepare_render_pass_descriptor(
    descriptor: &metal::RenderPassDescriptorRef,
    colour_texture: &metal::TextureRef,
    depth_texture: &metal::TextureRef,
) {
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();

    color_attachment.set_texture(Some(colour_texture));
    color_attachment.set_load_action(metal::MTLLoadAction::Clear);
    color_attachment.set_clear_color(metal::MTLClearColor::new(0.2, 0.2, 0.25, 1.0));
    color_attachment.set_store_action(metal::MTLStoreAction::Store);

    let depth_attachment = descriptor.depth_attachment().unwrap();
    depth_attachment.set_texture(Some(depth_texture));
    depth_attachment.set_clear_depth(1.0);
    depth_attachment.set_load_action(metal::MTLLoadAction::Clear);
    depth_attachment.set_store_action(metal::MTLStoreAction::DontCare);
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Uniforms {
    pub mvp: Mat4,
    pub colour: Vec4,
}
