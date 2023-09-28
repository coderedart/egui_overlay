use egui::{ClippedPrimitive, TexturesDelta};
use egui_render_glow::{GlowBackend, GlowConfig};
use raw_window_handle::RawWindowHandle;
pub use three_d;
use three_d::Context;
pub struct ThreeDBackend {
    pub context: Context,
    pub glow_backend: GlowBackend,
}

#[derive(Default)]
pub struct ThreeDConfig {
    pub glow_config: GlowConfig,
}

impl ThreeDBackend {
    pub fn new(
        config: ThreeDConfig,
        get_proc_address: impl FnMut(&str) -> *const std::ffi::c_void,
        handle: RawWindowHandle,
        framebuffer_size: [u32; 2],
    ) -> Self {
        let glow_backend = GlowBackend::new(
            config.glow_config,
            get_proc_address,
            handle,
            framebuffer_size,
        );

        #[cfg(all(target_arch = "wasm32", not(target_os = "emscripten")))]
        {
            use three_d::HasContext;
            let supported_extension = (glow_backend.glow_context).supported_extensions();

            assert!(supported_extension.contains("EXT_color_buffer_float"));

            assert!(supported_extension.contains("OES_texture_float"));

            assert!(supported_extension.contains("OES_texture_float_linear"));
        }

        Self {
            context: Context::from_gl_context(glow_backend.glow_context.clone())
                .expect("failed to create threed context"),
            glow_backend,
        }
    }
    pub fn prepare_frame(&mut self, latest_fb_size: [u32; 2]) {
        self.glow_backend.prepare_frame(latest_fb_size);
    }
    pub fn render_egui(
        &mut self,
        meshes: Vec<ClippedPrimitive>,
        textures_delta: TexturesDelta,
        logical_screen_size: [f32; 2],
    ) {
        self.glow_backend
            .render_egui(meshes, textures_delta, logical_screen_size);
    }

    pub fn resize_framebuffer(&mut self, fb_size: [u32; 2]) {
        self.glow_backend.resize_framebuffer(fb_size);
    }
}
