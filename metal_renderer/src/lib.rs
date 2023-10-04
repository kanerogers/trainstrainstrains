#[cfg(target_os = "macos")]
mod metal_context;

#[cfg(target_os = "macos")]
mod metal_renderer;

#[cfg(target_os = "macos")]
mod yakui_metal;

#[cfg(target_os = "macos")]
pub use metal_renderer::MetalRenderer;
