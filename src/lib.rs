pub use egui_backend;
pub use egui_backend::egui;
use egui_backend::{egui::Context, BackendConfig, GfxBackend, UserApp, WindowBackend};
pub use egui_render_three_d;
use egui_render_three_d::ThreeDBackend;
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
            is_opengl: true,
            opengl_config: Default::default(),
            transparent: Some(true),
        },
    );
    glfw_backend.set_always_on_top(true);
    glfw_backend.window.set_decorated(false);
    let three_d_backend = ThreeDBackend::new(&mut glfw_backend, Default::default());
    let overlap_app = OverlayApp {
        user_data,
        egui_context: Default::default(),
        three_d_backend,
        glfw_backend,
    };
    GlfwBackend::run_event_loop(overlap_app);
}
/// Implement this trait for your struct containing data you need. Then, call [`start`] fn with that data
pub trait EguiOverlay {
    fn gui_run(
        &mut self,
        egui_context: &Context,
        three_d_backend: &mut ThreeDBackend,
        glfw_backend: &mut GlfwBackend,
    );
}
pub struct OverlayApp<T: EguiOverlay> {
    pub user_data: T,
    pub egui_context: Context,
    pub three_d_backend: ThreeDBackend,
    pub glfw_backend: GlfwBackend,
}

impl<T: EguiOverlay> OverlayApp<T> {}

impl<T: EguiOverlay> UserApp for OverlayApp<T> {
    type UserGfxBackend = ThreeDBackend;

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
            &mut self.three_d_backend,
            &self.egui_context,
        )
    }

    fn gui_run(&mut self) {
        let OverlayApp {
            user_data,
            egui_context,
            three_d_backend: wgpu_backend,
            glfw_backend,
        } = self;
        user_data.gui_run(egui_context, wgpu_backend, glfw_backend);
    }
}
