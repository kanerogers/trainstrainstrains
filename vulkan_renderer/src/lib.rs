mod buffer;
mod descriptors;
pub mod lazy_renderer;
pub mod vulkan_context;
pub mod vulkan_texture;

use ash::vk;
use common::{glam, hecs, winit, yakui, Camera, Renderer};
use glam::Vec4;
pub use lazy_renderer::LazyRenderer;

pub use crate::vulkan_texture::NO_TEXTURE_ID;
use crate::{lazy_renderer::RenderSurface, vulkan_context::VulkanContext};

#[derive(Default, Debug, Clone, Copy)]
pub struct LineVertex {
    pub position: Vec4,
    pub colour: Vec4,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SwapchainInfo {
    pub image_count: u32,
    pub resolution: vk::Extent2D,
    pub format: vk::Format,
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub struct LazyVulkan {
    context: VulkanContext,
    yakui_vulkan: yakui_vulkan::YakuiVulkan,
    pub renderer: LazyRenderer,
    pub window: winit::window::Window,
    pub surface: Surface,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_loader: ash::extensions::khr::Swapchain,

    pub present_complete_semaphore: vk::Semaphore,
    pub rendering_complete_semaphore: vk::Semaphore,

    pub draw_commands_reuse_fence: vk::Fence,
    pub setup_commands_reuse_fence: vk::Fence,
}

pub struct Surface {
    pub surface: vk::SurfaceKHR,
    pub surface_loader: ash::extensions::khr::Surface,
    pub surface_format: vk::SurfaceFormatKHR,
    pub surface_resolution: vk::Extent2D,
    pub present_mode: vk::PresentModeKHR,
    pub desired_image_count: u32,
}

impl Renderer for LazyVulkan {
    fn init(window: winit::window::Window) -> Self {
        Self::new(window)
    }

    fn render(
        &mut self,
        world: &hecs::World,
        lines: &[common::Line],
        camera: Camera,
        yak: &mut yakui::Yakui,
        time_of_day: f32,
    ) {
        let swapchain_index = self.render_begin();
        self.renderer.camera = camera;
        let context = &self.context;
        let mut line_vertices = Vec::new();
        for line in lines {
            line_vertices.push(LineVertex {
                position: line.start.extend(1.),
                colour: line.colour.extend(1.),
            });
            line_vertices.push(LineVertex {
                position: line.end.extend(1.),
                colour: line.colour.extend(1.),
            })
        }
        unsafe {
            self.renderer
                .line_vertex_buffer
                .overwrite(context, &line_vertices)
        };

        let draw_calls = self.renderer.build_draw_calls(world);

        self.renderer._render(
            context,
            swapchain_index,
            &draw_calls,
            &line_vertices,
            time_of_day,
        );

        self.yakui_vulkan
            .paint(yak, &context.into(), swapchain_index);
        self.render_end(swapchain_index, &[self.present_complete_semaphore]);
    }

    fn resized(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        let new_render_surface = self._resized(size.width, size.height);
        self.yakui_vulkan
            .update_surface((&new_render_surface).into(), &self.context.device);
        self.renderer
            .update_surface(new_render_surface, &self.context.device);
    }

    fn cleanup(&mut self) {
        unsafe {
            self.renderer.cleanup(&self.context.device);
        }
    }

    fn update_assets(&mut self, world: &mut hecs::World) {
        let vulkan_context = &self.context;
        self.renderer.update_assets(vulkan_context, world);
    }

    fn unload_assets(&mut self) {
        let vulkan_context = &self.context;
        unsafe {
            self.renderer.unload_assets(vulkan_context);
        }
    }

    fn window(&'_ self) -> &'_ winit::window::Window {
        &self.window
    }
}

impl LazyVulkan {
    pub fn context(&self) -> &VulkanContext {
        &self.context
    }

    /// Bring up all the Vulkan pomp and ceremony required to render things.
    /// Vulkan Broadly lifted from: https://github.com/ash-rs/ash/blob/0.37.2/examples/src/lib.rs
    fn new(window: winit::window::Window) -> Self {
        let window_resolution = vk::Extent2D {
            width: window.inner_size().width as _,
            height: window.inner_size().height as _,
        };
        let (context, surface) = VulkanContext::new_with_surface(&window, window_resolution);
        let device = &context.device;
        let instance = &context.instance;
        let swapchain_loader = ash::extensions::khr::Swapchain::new(instance, device);
        let (swapchain, swapchain_images, swapchain_image_views) =
            create_swapchain(&context, &surface, &swapchain_loader, None);

        let fence_create_info =
            vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

        let draw_commands_reuse_fence = unsafe {
            device
                .create_fence(&fence_create_info, None)
                .expect("Create fence failed.")
        };
        let setup_commands_reuse_fence = unsafe {
            device
                .create_fence(&fence_create_info, None)
                .expect("Create fence failed.")
        };

        let semaphore_create_info = vk::SemaphoreCreateInfo::default();

        let present_complete_semaphore = unsafe {
            device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap()
        };
        let rendering_complete_semaphore = unsafe {
            device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap()
        };

        let render_surface = RenderSurface::new(
            &context,
            surface.surface_resolution,
            surface.surface_format.format,
            swapchain_image_views,
        );

        let yakui_vulkan =
            yakui_vulkan::YakuiVulkan::new(&(&context).into(), (&render_surface).into());
        let renderer = LazyRenderer::new(&context, render_surface);

        Self {
            window,
            yakui_vulkan,
            context,
            surface,
            swapchain_loader,
            swapchain,
            swapchain_images,
            present_complete_semaphore,
            rendering_complete_semaphore,
            draw_commands_reuse_fence,
            setup_commands_reuse_fence,
            renderer,
        }
    }

    pub fn _resized(&mut self, window_width: u32, window_height: u32) -> RenderSurface {
        unsafe {
            let device = &self.context.device;
            device.device_wait_idle().unwrap();
            self.surface.surface_resolution = vk::Extent2D {
                width: window_width,
                height: window_height,
            };
            let (new_swapchain, _, new_present_image_views) = create_swapchain(
                &self.context,
                &self.surface,
                &self.swapchain_loader,
                Some(self.swapchain),
            );

            self.destroy_swapchain(self.swapchain);
            self.swapchain = new_swapchain;

            RenderSurface::new(
                &self.context,
                self.surface.surface_resolution,
                self.surface.surface_format.format,
                new_present_image_views,
            )
        }
    }

    unsafe fn destroy_swapchain(&self, swapchain: vk::SwapchainKHR) {
        self.swapchain_loader.destroy_swapchain(swapchain, None);
    }

    pub fn render_begin(&self) -> u32 {
        let (present_index, _) = unsafe {
            self.swapchain_loader
                .acquire_next_image(
                    self.swapchain,
                    std::u64::MAX,
                    self.present_complete_semaphore,
                    vk::Fence::null(),
                )
                .unwrap()
        };

        let device = &self.context.device;
        unsafe {
            device
                .wait_for_fences(
                    std::slice::from_ref(&self.draw_commands_reuse_fence),
                    true,
                    std::u64::MAX,
                )
                .unwrap();
            device
                .reset_fences(std::slice::from_ref(&self.draw_commands_reuse_fence))
                .unwrap();
            device
                .reset_command_buffer(
                    self.context.draw_command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();
            device
                .begin_command_buffer(
                    self.context.draw_command_buffer,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();
        }
        present_index
    }

    pub fn render_end(&self, present_index: u32, wait_semaphores: &[vk::Semaphore]) {
        let device = &self.context.device;
        unsafe {
            device
                .end_command_buffer(self.context.draw_command_buffer)
                .unwrap();
            let swapchains = [self.swapchain];
            let image_indices = [present_index];
            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(wait_semaphores)
                .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                .command_buffers(std::slice::from_ref(&self.context.draw_command_buffer))
                .signal_semaphores(std::slice::from_ref(&self.rendering_complete_semaphore));

            device
                .queue_submit(
                    self.context.queue,
                    std::slice::from_ref(&submit_info),
                    self.draw_commands_reuse_fence,
                )
                .unwrap();

            match self.swapchain_loader.queue_present(
                self.context.queue,
                &vk::PresentInfoKHR::builder()
                    .image_indices(&image_indices)
                    .wait_semaphores(std::slice::from_ref(&self.rendering_complete_semaphore))
                    .swapchains(&swapchains),
            ) {
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    println!("Swapchain is suboptimal!")
                }
                Err(e) => panic!("Error presenting: {e:?}"),
                _ => {}
            }
        };
    }
}

fn create_swapchain(
    context: &VulkanContext,
    surface: &Surface,
    swapchain_loader: &ash::extensions::khr::Swapchain,
    previous_swapchain: Option<vk::SwapchainKHR>,
) -> (vk::SwapchainKHR, Vec<vk::Image>, Vec<vk::ImageView>) {
    let device = &context.device;

    let mut swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface.surface)
        .min_image_count(surface.desired_image_count)
        .image_color_space(surface.surface_format.color_space)
        .image_format(surface.surface_format.format)
        .image_extent(surface.surface_resolution)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(surface.present_mode)
        .clipped(true)
        .image_array_layers(1);

    if let Some(old_swapchain) = previous_swapchain {
        swapchain_create_info.old_swapchain = old_swapchain
    }

    let swapchain = unsafe {
        swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .unwrap()
    };

    let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain).unwrap() };
    let present_image_views =
        create_swapchain_image_views(&present_images, surface.surface_format.format, device);

    (swapchain, present_images, present_image_views)
}

pub fn create_swapchain_image_views(
    present_images: &[vk::Image],
    surface_format: vk::Format,
    device: &ash::Device,
) -> Vec<vk::ImageView> {
    present_images
        .iter()
        .map(|&image| {
            let create_view_info = vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            unsafe { device.create_image_view(&create_view_info, None).unwrap() }
        })
        .collect()
}

#[cfg(test)]
impl Drop for LazyVulkan {
    fn drop(&mut self) {
        unsafe {
            let device = &self.context.device;
            device.device_wait_idle().unwrap();
            device.destroy_semaphore(self.present_complete_semaphore, None);
            device.destroy_semaphore(self.rendering_complete_semaphore, None);
            device.destroy_fence(self.draw_commands_reuse_fence, None);
            device.destroy_fence(self.setup_commands_reuse_fence, None);
            device.destroy_command_pool(self.context.command_pool, None);
            self.destroy_swapchain(self.swapchain);
            device.destroy_device(None);
            self.surface
                .surface_loader
                .destroy_surface(self.surface.surface, None);
            self.context.instance.destroy_instance(None);
        }
    }
}
