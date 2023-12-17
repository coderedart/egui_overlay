use std::time::Duration;

use egui::{Context, PlatformOutput};
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
    let mut glfw_backend = GlfwBackend::new(GlfwConfig {
        // this closure will be called before creating a window
        glfw_callback: Box::new(|gtx| {
            // some defualt hints. it is empty atm, but in future we might add some convenience hints to it.
            (egui_window_glfw_passthrough::GlfwConfig::default().glfw_callback)(gtx);
            // scale the window size based on monitor scale. as 800x600 looks too small on a 4k screen, compared to a hd screen in absolute pixel sizes.
            gtx.window_hint(egui_window_glfw_passthrough::glfw::WindowHint::ScaleToMonitor(true));
        }),
        #[cfg(not(target_os = "macos"))]
        opengl_window: Some(true), // opengl for non-macos, for faster compilation and less wgpu bloat. also, drivers are better with gl transparency than vk
        #[cfg(target_os = "macos")]
        opengl_window: Some(false), // macos doesn't support opengl.
        transparent_window: Some(true),
        ..Default::default()
    });
    // always on top
    glfw_backend.window.set_floating(true);
    // disable borders/titlebar
    glfw_backend.window.set_decorated(false);

    let latest_size = glfw_backend.window.get_framebuffer_size();
    let latest_size = [latest_size.0 as _, latest_size.1 as _];

    // for non-macos, we just use three_d because its much faster compile times and opengl transparency being more reliable than vulkan transparency
    #[cfg(not(target_os = "macos"))]
    let default_gfx_backend = {
        use raw_window_handle::HasRawWindowHandle;
        let handle = glfw_backend.window.raw_window_handle();
        DefaultGfxBackend::new(
            egui_render_three_d::ThreeDConfig {
                ..Default::default()
            },
            |s| glfw_backend.get_proc_address(s),
            handle,
            latest_size,
        )
    };
    // macos doesn't have opengl, so wgpu/metal for that.
    #[cfg(target_os = "macos")]
    let default_gfx_backend = DefaultGfxBackend::new(
        egui_render_wgpu::WgpuConfig {
            ..Default::default()
        },
        Some(&glfw_backend.window),
        latest_size,
    );
    let overlap_app = OverlayApp {
        user_data,
        egui_context: Default::default(),
        default_gfx_backend,
        glfw_backend,
    };
    overlap_app.enter_event_loop();
}

/// Implement this trait for your struct containing data you need. Then, call [`start`] fn with that data
pub trait EguiOverlay {
    fn gui_run(
        &mut self,
        egui_context: &Context,
        default_gfx_backend: &mut DefaultGfxBackend,
        glfw_backend: &mut GlfwBackend,
    );
    fn run(
        &mut self,
        egui_context: &Context,
        default_gfx_backend: &mut DefaultGfxBackend,
        glfw_backend: &mut GlfwBackend,
    ) -> Option<(PlatformOutput, Duration)> {
        let input = glfw_backend.take_raw_input();
        // takes a closure that can provide latest framebuffer size.
        // because some backends like vulkan/wgpu won't work without reconfiguring the surface after some sort of resize event unless you give it the latest size
        default_gfx_backend.prepare_frame(|| {
            let latest_size = glfw_backend.window.get_framebuffer_size();
            [latest_size.0 as _, latest_size.1 as _]
        });
        egui_context.begin_frame(input);
        self.gui_run(egui_context, default_gfx_backend, glfw_backend);

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = egui_context.end_frame();
        let meshes = egui_context.tessellate(shapes, pixels_per_point);
        let repaint_after = viewport_output
            .into_iter()
            .map(|f| f.1.repaint_delay)
            .collect::<Vec<Duration>>()[0];

        default_gfx_backend.render_egui(meshes, textures_delta, glfw_backend.window_size_logical);
        if glfw_backend.is_opengl() {
            use egui_window_glfw_passthrough::glfw::Context;
            glfw_backend.window.swap_buffers();
        } else {
            // for wgpu backend
            #[cfg(target_os = "macos")]
            default_gfx_backend.present()
        }
        Some((platform_output, repaint_after))
    }
}

pub struct OverlayApp<T: EguiOverlay> {
    pub user_data: T,
    pub egui_context: Context,
    pub default_gfx_backend: DefaultGfxBackend,
    pub glfw_backend: GlfwBackend,
}

impl<T: EguiOverlay> OverlayApp<T> {
    pub fn enter_event_loop(mut self) {
        // polls for events and returns if there's some activity.
        // But if there is no event for the specified duration, it will return anyway.
        // used by "reactive" apps which don't do anything unless there's some event.
        tracing::info!("entering glfw event loop");
        let mut wait_events_duration = std::time::Duration::ZERO;
        let callback = move || {
            let Self {
                user_data,
                egui_context,
                default_gfx_backend,
                glfw_backend,
            } = &mut self;
            glfw_backend
                .glfw
                .wait_events_timeout(wait_events_duration.as_secs_f64());

            // gather events
            glfw_backend.tick();

            if glfw_backend.resized_event_pending {
                let latest_size = glfw_backend.window.get_framebuffer_size();
                default_gfx_backend.resize_framebuffer([latest_size.0 as _, latest_size.1 as _]);
                glfw_backend.resized_event_pending = false;
            }
            // run userapp gui function. let user do anything he wants with window or gfx backends
            if let Some((platform_output, timeout)) =
                user_data.run(egui_context, default_gfx_backend, glfw_backend)
            {
                wait_events_duration = timeout.min(std::time::Duration::from_secs(1));
                if !platform_output.copied_text.is_empty() {
                    glfw_backend
                        .window
                        .set_clipboard_string(&platform_output.copied_text);
                }
                glfw_backend.set_cursor(platform_output.cursor_icon);
            } else {
                wait_events_duration = std::time::Duration::ZERO;
            }
            #[cfg(not(target_os = "emscripten"))]
            glfw_backend.window.should_close()
        };

        // on emscripten, just keep calling forever i guess.
        #[cfg(target_os = "emscripten")]
        set_main_loop_callback(callback);

        #[cfg(not(target_os = "emscripten"))]
        {
            let mut callback = callback;
            loop {
                // returns if loop should close.
                if callback() {
                    tracing::warn!("event loop is exiting");
                    break;
                }
            }
        }
    }
}
