use egui::{FullOutput, Window};
use egui_overlay::{GlfwWindow, WgpuRenderer};
use wgpu::{
    CommandEncoderDescriptor, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor,
};

fn main() {
    let mut glfw_window = GlfwWindow::new().expect("failed to init window");
    let mut wgpu_renderer =
        pollster::block_on(WgpuRenderer::new(&glfw_window)).expect("failed to init wgpu");
    let mut ctx = egui::Context::default();

    while !glfw_window.window.should_close() {
        glfw_window.tick();
        wgpu_renderer.pre_tick(&glfw_window);
        // use wgpu to draw whatever you want. here we just clear the surface. we only do this IF the framebuffer exists, otherwise, something's gone wrong
        // don't take out the framebuffer either, it is used by egui render pass later
        {
            let mut encoder =
                wgpu_renderer
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor {
                        label: Some("clear pass encoder"),
                    });
            {
                if let Some((fb, fbv)) = wgpu_renderer.framebuffer_and_view.as_ref() {
                    encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("clear render pass"),
                        color_attachments: &[RenderPassColorAttachment {
                            view: fbv,
                            resolve_target: None,
                            ops: Operations {
                                // transparent color
                                load: LoadOp::Clear(wgpu::Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 0.0,
                                }),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                }
            }
            wgpu_renderer
                .queue
                .submit(std::iter::once(encoder.finish()));
        }
        // for people who want to only do egui, just stay between begin_frame and end_frame functions. that's where you dela with gui.
        // now, we can do our own things with egui
        // take the input from glfw_window
        ctx.begin_frame(glfw_window.raw_input.take());
        Window::new("Tigers are Cats too").show(&ctx, |ui| {
            ui.label("MEOW MEOW");
            ctx.style_ui(ui);
        });
        // now at the end of all gui stuff, we end the frame to get platform output and textures_delta and shapes
        let FullOutput {
            platform_output,
            textures_delta,
            shapes,
            ..
        } = ctx.end_frame();
        let shapes = ctx.tessellate(shapes); // need to convert shapes into meshes to draw
                                             // in platform output, we only care about two things. first is whether some text has been copied, which needs ot be put into clipbaord
        if !platform_output.copied_text.is_empty() {
            glfw_window
                .window
                .set_clipboard_string(&platform_output.copied_text);
        }
        // here we draw egui to framebuffer and submit it finally
        wgpu_renderer
            .tick(textures_delta, shapes, &glfw_window)
            .expect("failed to draw for some reason");
        // based on whether egui wants the input or not, we will set the overlay to be passthrough or not.
        if ctx.wants_keyboard_input() || ctx.wants_pointer_input() {
            glfw_window.window.set_mouse_passthrough(false);
        } else {
            glfw_window.window.set_mouse_passthrough(true);
        }

    }
}
