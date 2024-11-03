use egui::DragValue;
use egui_overlay::EguiOverlay;
use egui_render_three_d::{
    three_d::{self, ColorMaterial, Gm, Mesh},
    ThreeDBackend,
};

fn main() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    // if RUST_LOG is not set, we will use the following filters
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or(EnvFilter::new("debug,wgpu=warn,naga=warn")),
        )
        .init();
    egui_overlay::start(HelloWorld {
        text: "hello world".to_string(),
        model: None,
    });
}

pub struct HelloWorld {
    pub text: String,
    pub model: Option<Gm<Mesh, ColorMaterial>>,
}
impl EguiOverlay for HelloWorld {
    fn gui_run(
        &mut self,
        egui_context: &egui::Context,
        three_d_backend: &mut ThreeDBackend,
        glfw_backend: &mut egui_window_glfw_passthrough::GlfwBackend,
    ) {
        use three_d::*;
        // create model if not yet created
        self.model
            .get_or_insert_with(|| create_triangle_model(&three_d_backend.context));
        // draw model
        if let Some(model) = &mut self.model {
            // Create a camera
            let camera = three_d::Camera::new_perspective(
                Viewport::new_at_origo(
                    glfw_backend.framebuffer_size_physical[0],
                    glfw_backend.framebuffer_size_physical[1],
                ),
                vec3(0.0, 0.0, 2.0),
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 1.0, 0.0),
                degrees(45.0),
                0.1,
                10.0,
            );
            // Update the animation of the triangle
            model.animate(glfw_backend.glfw.get_time() as _);

            // Get the screen render target to be able to render something on the screen
            egui_render_three_d::three_d::RenderTarget::<'_>::screen(
                &three_d_backend.context,
                glfw_backend.framebuffer_size_physical[0],
                glfw_backend.framebuffer_size_physical[1],
            )
            // Clear the color and depth of the screen render target. use transparent color.
            .clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 0.0, 1.0))
            // Render the triangle with the color material which uses the per vertex colors defined at construction
            .render(&camera, std::iter::once(model), &[]);
        }
        egui::Window::new("hello window")
            .scroll([true, true])
            .show(egui_context, |ui| {
                ui.text_edit_multiline(&mut self.text);
            });

        // just some controls to show how you can use glfw_backend
        egui::Window::new("controls").show(egui_context, |ui| {
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
            // we need input, so we need the window to be NOT passthrough
            glfw_backend.set_passthrough(false);
        } else {
            // we don't care about input, so the window can be passthrough now
            glfw_backend.set_passthrough(true)
        }
        egui_context.request_repaint();
    }
}

fn create_triangle_model(three_d_context: &three_d::Context) -> Gm<Mesh, ColorMaterial> {
    use three_d::*;

    // Create a CPU-side mesh consisting of a single colored triangle
    let positions = vec![
        vec3(0.5, -0.5, 0.0),  // bottom right
        vec3(-0.5, -0.5, 0.0), // bottom left
        vec3(0.0, 0.5, 0.0),   // top
    ];
    let colors = vec![
        Srgba::RED,   // bottom right
        Srgba::GREEN, // bottom left
        Srgba::BLUE,  // top
    ];
    let cpu_mesh = CpuMesh {
        positions: Positions::F32(positions),
        colors: Some(colors),
        ..Default::default()
    };

    // Construct a model, with a default color material, thereby transferring the mesh data to the GPU
    let mut model = Gm::new(
        Mesh::new(three_d_context, &cpu_mesh),
        ColorMaterial::default(),
    );

    // Add an animation to the triangle.
    model.set_animation(|time| Mat4::from_angle_y(radians(time * 0.005)));
    model
}
