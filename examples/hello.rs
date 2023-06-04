use egui_backend::{egui::DragValue, WindowBackend};
use egui_overlay::EguiOverlay;
use egui_render_three_d::ThreeDBackend;

fn main() {
    egui_overlay::start(HelloWorld {
        text: "hello world".to_string(),
    });
}

pub struct HelloWorld {
    pub text: String,
}
impl EguiOverlay for HelloWorld {
    fn gui_run(
        &mut self,
        egui_context: &egui_backend::egui::Context,
        _three_d_backend: &mut ThreeDBackend,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
    ) {
        egui_backend::egui::Window::new("hello window").show(egui_context, |ui| {
            ui.text_edit_multiline(&mut self.text);
        });

        // just some controls to show how you can use glfw_backend
        egui_backend::egui::Window::new("controls").show(egui_context, |ui| {
            // sometimes, you want to see the borders to understand where the overlay is.
            if ui.button("toggle borders").clicked() {
                let dec = glfw_backend.window.is_decorated();
                glfw_backend.window.set_decorated(!dec);
            }
            // how to change size.
            // WARNING: don't use drag value, because window size changing while dragging ui messes things up.
            let size = glfw_backend.window.get_size();
            let mut size = [size.0 as f32, size.1 as f32];
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("width: ");
                ui.add_enabled(false, DragValue::new(&mut size[0]));
                if ui.button("inc").clicked() {
                    size[0] += 10.0;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    size[0] -= 10.0;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("height: ");
                ui.add_enabled(false, DragValue::new(&mut size[1]));
                if ui.button("inc").clicked() {
                    size[1] += 10.0;
                    changed = true;
                }
                if ui.button("dec").clicked() {
                    size[1] -= 10.0;
                    changed = true;
                }
            });
            if changed {
                glfw_backend.set_window_size(size);
            }
        });
        // here you decide if you want to be passthrough or not.
        if egui_context.wants_pointer_input() || egui_context.wants_keyboard_input() {
            glfw_backend.window.set_mouse_passthrough(false);
        } else {
            glfw_backend.window.set_mouse_passthrough(true);
        }
        egui_context.request_repaint();
    }
}
