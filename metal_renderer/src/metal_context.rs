pub use crate::MetalRenderer;

use cocoa::{appkit::NSView, base::id as cocoa_id};
use core_graphics_types::geometry::CGSize;

use common::winit;
use metal::*;
use objc::runtime::YES;
use std::mem;
use winit::platform::macos::WindowExtMacOS;

pub struct MetalContext {
    pub device: metal::Device,
    pub command_queue: metal::CommandQueue,
    pub layer: metal::MetalLayer,
    pub window: winit::window::Window,
    pub depth_texture: metal::Texture,
}

impl MetalContext {
    pub fn new(window: winit::window::Window) -> Self {
        let device = Device::system_default().expect("no device found");
        let layer = MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_presents_with_transaction(false);

        unsafe {
            let view = window.ns_view() as cocoa_id;
            view.setWantsLayer(YES);
            view.setLayer(mem::transmute(layer.as_ref()));
        }

        let draw_size = window.inner_size();
        layer.set_drawable_size(CGSize::new(draw_size.width as f64, draw_size.height as f64));
        let command_queue = device.new_command_queue();

        let depth_texture = create_depth_texture(&device, draw_size);

        MetalContext {
            device,
            command_queue,
            layer,
            window,
            depth_texture,
        }
    }

    pub fn resized(&self, size: winit::dpi::PhysicalSize<u32>) {
        self.layer.set_drawable_size(CGSize {
            width: size.width as _,
            height: size.height as _,
        })
    }
}

fn create_depth_texture(
    device: &metal::Device,
    draw_size: winit::dpi::PhysicalSize<u32>,
) -> metal::Texture {
    let depth_texture_desc = TextureDescriptor::new();
    depth_texture_desc.set_pixel_format(MTLPixelFormat::Depth32Float);
    depth_texture_desc.set_width(draw_size.width as _);
    depth_texture_desc.set_height(draw_size.height as _);
    depth_texture_desc.set_storage_mode(MTLStorageMode::Private);
    depth_texture_desc.set_usage(MTLTextureUsage::RenderTarget);

    device.new_texture(&depth_texture_desc)
}
