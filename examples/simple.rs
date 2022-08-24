use egui_overlay::{start, UserApp};

pub struct MyApp {
    pub name: String,
    pub age: u32,
}
impl UserApp for MyApp {
    fn run(&mut self, etx: &egui::Context) {
        egui::Window::new("my app window").show(etx, |ui| {
            ui.label("simple label");
            ui.text_edit_singleline(&mut self.name);
            ui.add(egui::DragValue::new(&mut self.age));
        });
    }
}

fn main() {
    let app = MyApp {
        name: String::new(),
        age: 0,
    };
    start(app);
}
