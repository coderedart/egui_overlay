pub use egui_backend;
pub use egui_backend::egui;
use egui_backend::{egui::Context, BackendConfig, GfxBackend, UserApp, WindowBackend};

#[cfg(not(target_os = "macos"))]
pub use egui_render_three_d;
#[cfg(not(target_os = "macos"))]
use egui_render_three_d::ThreeDBackend as DefaultGfxBackend;

// mac doesn't support opengl. so, use wgpu.
#[cfg(target_os = "macos")]
pub use egui_render_wgpu;
#[cfg(target_os = "macos")]
use egui_render_wgpu::WgpuBackend as DefaultGfxBackend;

pub use egui_window_glfw_passthrough;
use egui_window_glfw_passthrough::{GlfwBackend, GlfwConfig};
/// After implementing [`EguiOverlay`], just call this function with your app data
pub fn start<T: EguiOverlay + 'static>(user_data: T) {
    let mut glfw_backend = GlfwBackend::new(
        GlfwConfig {
            glfw_callback: Box::new(|gtx| {
                (egui_window_glfw_passthrough::GlfwConfig::default().glfw_callback)(gtx);
                gtx.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::ScaleToMonitor(true),
                );
            }),
            ..Default::default()
        },
        BackendConfig {
            #[cfg(not(target_os = "macos"))]
            is_opengl: true,
            #[cfg(target_os = "macos")]
            is_opengl: false,
            opengl_config: Default::default(),
            transparent: Some(true),
        },
    );
    glfw_backend.set_always_on_top(true);
    glfw_backend.window.set_decorated(false);
    let default_gfx_backend = DefaultGfxBackend::new(&mut glfw_backend, Default::default());
    let overlap_app = OverlayApp {
        user_data,
        egui_context: Default::default(),
        default_gfx_backend,
        glfw_backend,
    };
    GlfwBackend::run_event_loop(overlap_app);
}
/// Implement this trait for your struct containing data you need. Then, call [`start`] fn with that data
pub trait EguiOverlay {
    fn gui_run(
        &mut self,
        egui_context: &Context,
        default_gfx_backend: &mut DefaultGfxBackend,
        glfw_backend: &mut GlfwBackend,
    );
}
pub struct OverlayApp<T: EguiOverlay> {
    pub user_data: T,
    pub egui_context: Context,
    pub default_gfx_backend: DefaultGfxBackend,
    pub glfw_backend: GlfwBackend,
}

impl<T: EguiOverlay> OverlayApp<T> {}

impl<T: EguiOverlay> UserApp for OverlayApp<T> {
    type UserGfxBackend = DefaultGfxBackend;

    type UserWindowBackend = GlfwBackend;

    fn get_all(
        &mut self,
    ) -> (
        &mut Self::UserWindowBackend,
        &mut Self::UserGfxBackend,
        &egui::Context,
    ) {
        (
            &mut self.glfw_backend,
            &mut self.default_gfx_backend,
            &self.egui_context,
        )
    }

    fn gui_run(&mut self) {
        let OverlayApp {
            user_data,
            egui_context,
            default_gfx_backend: wgpu_backend,
            glfw_backend,
        } = self;
        user_data.gui_run(egui_context, wgpu_backend, glfw_backend);
    }
}
