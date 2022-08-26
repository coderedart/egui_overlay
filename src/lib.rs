use egui_backend::{GfxBackend, UserApp, WindowBackend};
pub use egui_backend;
pub use egui_backend::egui;
pub use egui_render_wgpu;
pub use egui_window_glfw_passthrough;
/// just impl the `UserApp<egui_window_glfw_passthrough::GlfwWindow, egui_render_wgpu::WgpuBackend>` trait
/// for your App and pass it to this function. this will initialize the glfw window and wgpu backend.
/// And enters the event loop running the `UserApp::run` fn that you implemented for your app.
pub fn start_overlay(
    app: impl UserApp<egui_window_glfw_passthrough::GlfwWindow, egui_render_wgpu::WgpuBackend> + 'static,
) {
    let (glfw_backend, window_info_for_gfx) = egui_window_glfw_passthrough::GlfwWindow::new(
        Default::default(),
        egui_backend::GfxApiConfig::Vulkan {},
    );
    let wgpu_backend = egui_render_wgpu::WgpuBackend::new(window_info_for_gfx, Default::default());

    glfw_backend.run_event_loop(wgpu_backend, app);
}
