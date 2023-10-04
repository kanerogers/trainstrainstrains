use std::collections::HashMap;

use crate::metal_context::MetalContext;
use common::yakui;
use yakui::{paint::Vertex as YakuiVertex, ManagedTextureId};

pub struct YakuiMetal {
    pub text_pipeline: metal::RenderPipelineState,
    pub texture_pipeline: metal::RenderPipelineState,
    pub vertex_buffer: metal::Buffer,
    pub index_buffer: metal::Buffer,
    initial_textures_synced: bool,
    /// Textures owned by yakui
    yakui_managed_textures: HashMap<ManagedTextureId, metal::Texture>,
    dummy_texture: metal::Texture,
}

impl YakuiMetal {
    pub fn new(context: &MetalContext) -> Self {
        let device = &context.device;
        let library_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/shaders/shaders.metallib");

        let library = device.new_library_with_file(library_path).unwrap();
        let text_pipeline =
            prepare_pipeline_state(&device, &library, "yakui_vertex", "yakui_text_fragment");
        let texture_pipeline =
            prepare_pipeline_state(&device, &library, "yakui_vertex", "yakui_texture_fragment");

        let vertex_buffer = device.new_buffer(
            1000 * std::mem::size_of::<Vertex>() as u64,
            metal::MTLResourceOptions::CPUCacheModeDefaultCache
                | metal::MTLResourceOptions::StorageModeShared,
        );

        let index_buffer = device.new_buffer(
            1000 * std::mem::size_of::<u32>() as u64,
            metal::MTLResourceOptions::CPUCacheModeDefaultCache
                | metal::MTLResourceOptions::StorageModeShared,
        );

        let dummy_texture = create_dummy_texture(device);

        Self {
            dummy_texture,
            vertex_buffer,
            index_buffer,
            text_pipeline,
            texture_pipeline,
            yakui_managed_textures: Default::default(),
            initial_textures_synced: false,
        }
    }

    pub fn paint(
        &mut self,
        context: &MetalContext,
        yak: &mut yakui::Yakui,
        drawable: &metal::MetalDrawableRef,
        command_buffer: &metal::CommandBufferRef,
    ) {
        let paint = yak.paint();

        self.update_textures(context, paint);

        // If there's nothing to paint, well.. don't paint!
        let layers = paint.layers();
        if layers.iter().all(|layer| layer.calls.is_empty()) {
            return;
        }

        let draw_calls = self.build_draw_calls(paint);

        self.render(&draw_calls, drawable, command_buffer);
    }

    fn update_textures(&mut self, context: &MetalContext, paint: &yakui::paint::PaintDom) {
        use yakui::paint::TextureChange;
        if !self.initial_textures_synced {
            self.initial_textures_synced = true;
            for (id, texture) in paint.textures() {
                let texture = texture_from_yakui_texture(context, texture);
                texture.set_label(&format!("Yakui texture {id:?}"));
                self.yakui_managed_textures.insert(id, texture);
            }

            return;
        }

        for (id, change) in paint.texture_edits() {
            match change {
                TextureChange::Added => {
                    let texture = paint.texture(id).unwrap();
                    let texture = texture_from_yakui_texture(context, texture);
                    self.yakui_managed_textures.insert(id, texture);
                }

                TextureChange::Removed => {
                    if let Some(_removed) = self.yakui_managed_textures.remove(&id) {
                        //TODO
                    }
                }

                TextureChange::Modified => {
                    if let Some(_old) = self.yakui_managed_textures.remove(&id) {
                        //TODO
                    }
                    let new = paint.texture(id).unwrap();
                    let texture = texture_from_yakui_texture(context, new);
                    self.yakui_managed_textures.insert(id, texture);
                }
            }
        }
    }

    fn build_draw_calls(&self, paint: &yakui::paint::PaintDom) -> Vec<DrawCall> {
        let mut vertices: Vec<Vertex> = Default::default();
        let mut indices: Vec<u32> = Default::default();
        let mut draw_calls: Vec<DrawCall> = Default::default();

        let calls = paint.layers().iter().flat_map(|layer| &layer.calls);

        for call in calls {
            let base = vertices.len() as u32;
            let index_offset = indices.len() as u32;
            let index_count = call.indices.len() as u32;

            for index in &call.indices {
                indices.push(*index as u32 + base);
            }
            for vertex in &call.vertices {
                vertices.push(vertex.into())
            }

            let texture = call.texture.and_then(|id| match id {
                yakui::TextureId::Managed(managed) => {
                    let texture = self.yakui_managed_textures.get(&managed)?;
                    Some(texture.as_ref())
                }
                yakui::TextureId::User(_bits) => {
                    todo!()
                }
            });

            draw_calls.push(DrawCall {
                index_offset,
                index_count,
                clip: call.clip,
                texture,
                pipeline: call.pipeline,
            });
        }

        unsafe {
            let indices_on_gpu: &mut [u32] = std::slice::from_raw_parts_mut(
                self.index_buffer.contents() as *mut _,
                indices.len(),
            );

            for (i, index) in indices.iter().enumerate() {
                indices_on_gpu[i] = *index;
            }

            // self.vertex_buffer.did_modify_range(metal::NSRange {
            //     location: 0,
            //     length: (vertices.len() * std::mem::size_of::<Vertex>()) as _,
            // });

            let vertices_on_gpu: &mut [Vertex] = std::slice::from_raw_parts_mut(
                self.vertex_buffer.contents() as *mut _,
                vertices.len(),
            );

            for (i, vertex) in vertices.iter().enumerate() {
                vertices_on_gpu[i] = *vertex;
            }
        }

        draw_calls
    }

    fn render(
        &self,
        draw_calls: &[DrawCall],
        drawable: &metal::MetalDrawableRef,
        command_buffer: &metal::CommandBufferRef,
    ) {
        let render_pass_descriptor = metal::RenderPassDescriptor::new();
        prepare_render_pass_descriptor(&render_pass_descriptor, drawable.texture());

        let encoder = command_buffer.new_render_command_encoder(&render_pass_descriptor);
        encoder.set_vertex_buffer(0, Some(&self.vertex_buffer), 0);

        for call in draw_calls {
            let pipeline_state = match call.pipeline {
                yakui::paint::Pipeline::Main => &self.texture_pipeline,
                yakui::paint::Pipeline::Text => &self.text_pipeline,
                _ => todo!(),
            };
            encoder.set_render_pipeline_state(pipeline_state);
            let texture = call.texture.unwrap_or(&self.dummy_texture);
            encoder.set_fragment_texture(0, Some(texture));
            encoder.draw_indexed_primitives_instanced_base_instance(
                metal::MTLPrimitiveType::Triangle,
                call.index_count as _,
                metal::MTLIndexType::UInt32,
                &self.index_buffer,
                (call.index_offset as usize * std::mem::size_of::<u32>()) as _,
                1,
                0,
                0,
            );
        }
        encoder.end_encoding();
    }
}

fn create_dummy_texture(device: &metal::Device) -> metal::Texture {
    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_width(1);
    descriptor.set_height(1);
    descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
    let texture = device.new_texture(&descriptor);
    let data: [u8; 4] = [1, 1, 1, 1];
    texture.replace_region(
        metal::MTLRegion {
            origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: metal::MTLSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        },
        0,
        data.as_ptr() as *const _,
        4,
    );
    texture.set_label("Dummy texture");

    texture
}

fn texture_from_yakui_texture(
    context: &MetalContext,
    yak_texture: &yakui::paint::Texture,
) -> metal::Texture {
    let device = &context.device;

    let descriptor = metal::TextureDescriptor::new();
    let width = yak_texture.size().y as u64;
    let height = yak_texture.size().x as u64;

    descriptor.set_height(width);
    descriptor.set_width(height);
    descriptor.set_pixel_format(yak_to_mtl(yak_texture.format()));
    let texture = device.new_texture(&descriptor);

    log::debug!("Created texture {texture:?}");

    let stride = width * get_stride(yak_texture.format());

    texture.replace_region(
        metal::MTLRegion {
            origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: metal::MTLSize {
                width,
                height,
                depth: 1,
            },
        },
        0,
        yak_texture.data().as_ptr() as _,
        stride,
    );

    texture
}

fn get_stride(format: yakui::paint::TextureFormat) -> u64 {
    match format {
        yakui::paint::TextureFormat::Rgba8Srgb => 4,
        yakui::paint::TextureFormat::R8 => 1,
        _ => todo!(),
    }
}

fn yak_to_mtl(format: yakui::paint::TextureFormat) -> metal::MTLPixelFormat {
    match format {
        yakui::paint::TextureFormat::Rgba8Srgb => metal::MTLPixelFormat::RGBA8Unorm_sRGB,
        yakui::paint::TextureFormat::R8 => metal::MTLPixelFormat::R8Unorm,
        _ => todo!(),
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
) {
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();

    color_attachment.set_texture(Some(colour_texture));
    color_attachment.set_load_action(metal::MTLLoadAction::DontCare);
    color_attachment.set_store_action(metal::MTLStoreAction::Store);
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct Vertex {
    position: yakui::geometry::Vec2,
    texcoord: yakui::geometry::Vec2,
    color: yakui::geometry::Vec4,
}

impl From<&YakuiVertex> for Vertex {
    fn from(y: &YakuiVertex) -> Self {
        Self {
            position: y.position,
            texcoord: y.texcoord,
            color: y.color,
        }
    }
}

#[derive(Debug)]
/// A single draw call to render a yakui mesh
struct DrawCall<'a> {
    index_offset: u32,
    index_count: u32,
    clip: Option<yakui::geometry::Rect>,
    texture: Option<&'a metal::TextureRef>,
    pipeline: yakui::paint::Pipeline,
}
