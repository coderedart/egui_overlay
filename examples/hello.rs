use egui_backend::UserApp;
use egui_overlay::start_overlay;
use egui_render_wgpu::WgpuBackend;

pub struct HelloWorld {
    pub text: String,
}
impl UserApp<egui_window_glfw_passthrough::GlfwWindow, WgpuBackend> for HelloWorld {
    fn run(
        &mut self,
        egui_context: &egui_backend::egui::Context,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwWindow,
        _: &mut WgpuBackend,
    ) {
        egui_backend::egui::Window::new("hello window").show(egui_context, |ui| {
            ui.text_edit_multiline(&mut self.text);
        });

        if egui_context.wants_pointer_input() || egui_context.wants_keyboard_input() {
            glfw_backend.window.set_mouse_passthrough(false);
        } else {
            glfw_backend.window.set_mouse_passthrough(true);
        }
    }
}

fn main() {
    start_overlay(HelloWorld {
        text: "hello world".to_string(),
    });
}
