use std::sync::Arc;

use rend3::{types::glam, *};
use wgpu::Surface;

use crate::{egui_rend3::EguiRenderRoutine, GlfwWindow};

pub struct GfxBackend {
    pub renderer: Arc<Renderer>,
    pub surface: Arc<Surface>,
    pub egui_render_routine: EguiRenderRoutine,
}

impl GfxBackend {
    pub fn new(glfw_backend: &GlfwWindow) -> Self {
        let fb_size = glfw_backend.window.get_framebuffer_size();
        let fb_size = [fb_size.0 as u32, fb_size.1 as u32];
        // create instance + adapter + device
        let iad =
            pollster::block_on(create_iad(None, None, None, None)).expect("failed to create iad");
        // create surface
        let surface = Arc::new(unsafe { iad.instance.create_surface(&glfw_backend.window) });

        let formats = surface.get_supported_formats(&iad.adapter);
        let preferred_format = if formats.contains(&types::TextureFormat::Rgba8UnormSrgb) {
            types::TextureFormat::Rgba8UnormSrgb
        } else if formats.contains(&types::TextureFormat::Bgra8UnormSrgb) {
            types::TextureFormat::Bgra8UnormSrgb
        } else {
            unreachable!("non-transparent surface formats are not supported")
        };

        // Configure the surface to be ready for rendering.
        rend3::configure_surface(
            &surface,
            &iad.device,
            preferred_format,
            glam::UVec2::new(fb_size[0], fb_size[1]),
            rend3::types::PresentMode::Fifo,
        );
        // creat renderer
        let renderer = Renderer::new(
            iad,
            Default::default(),
            Some(fb_size[0] as f32 / fb_size[1] as f32),
        )
        .expect("failed to create renderr");
        let egui_render_routine = EguiRenderRoutine::new(&renderer, preferred_format);
        Self {
            renderer,
            surface,
            egui_render_routine,
        }
    }
}
