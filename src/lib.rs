mod egui_rend3;
mod render_backend;
mod window_backend;
use egui_rend3::EguiRenderOutput;
use render_backend::GfxBackend;
pub use window_backend::*;

pub trait UserApp {
    fn run(&mut self, etx: &egui::Context);
}

pub fn start(mut app: impl UserApp) {
    let etx = egui::Context::default();
    let mut glfw_backend = GlfwWindow::new().expect("failed to create glfw window");
    let mut gfx_backend = GfxBackend::new(&glfw_backend);
    while !glfw_backend.window.should_close() {
        glfw_backend.tick();

        let frame = rend3::util::output::OutputFrame::Surface {
            surface: gfx_backend.surface.clone(),
        };
        // Ready up the renderer
        let (cmd_bufs, ready) = gfx_backend.renderer.ready();

        // Build a rendergraph
        let mut graph = rend3::graph::RenderGraph::new();
        let surface_handle = graph.add_surface_texture();
        etx.begin_frame(glfw_backend.raw_input.take());
        app.run(&etx);
        let output = etx.end_frame();
        let egui_render_output = EguiRenderOutput {
            meshes: etx.tessellate(output.shapes),
            textures_delta: output.textures_delta,
            scale: glfw_backend.scale,
            window_size: Some(glfw_backend.window_size),
            fb_size: glfw_backend.fb_size,
        };
        gfx_backend.egui_render_routine.add_to_graph(
            &mut graph,
            egui_render_output,
            surface_handle,
        );
        graph.execute(&gfx_backend.renderer, frame, cmd_bufs, &ready);
        if etx.wants_pointer_input() || etx.wants_keyboard_input() {
            glfw_backend.window.set_mouse_passthrough(false);
        } else {
            glfw_backend.window.set_mouse_passthrough(true);
        }
    }
}
