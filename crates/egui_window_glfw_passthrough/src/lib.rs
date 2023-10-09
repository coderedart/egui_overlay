//! This crate uses `glfw-passthrough` crate as a window backend for egui.

use egui::{Event, Key, PointerButton, Pos2, RawInput};
pub use glfw;
use glfw::Action;
use glfw::ClientApiHint;
use glfw::Context;
use glfw::Glfw;
use glfw::StandardCursor;
use glfw::WindowEvent;
use glfw::WindowHint;
use std::sync::mpsc::Receiver;
use tracing::info;
/// This is the window backend for egui using [`glfw`]
/// Most of the startup configuration is done inside [`default_glfw_callback()`] and [`default_window_callback()`]
/// These are passed to the `new` function using [`GlfwConfig`].
///
/// https://www.glfw.org/docs/3.3/intro_guide.html#coordinate_systems
/// So, there are two different units used when referring to size/position.
/// 1. physical size. This is size in raw physical pixels of the framebuffer.
/// 2. virtual screen coordinates (units). These may or may not be the same size as the pixels.
/// Almost all sizes glfw gives us are in virtual units except monitor size (millimeters) and framebuffer size (physical pixels).
/// Glfw also allows us to query "Content scale". This is a `float` by which we should be scaling our UI.
/// In simple terms, if we use a scale of 1.0 on a 1080p monitor of 22 inches, a character might take 20 pixels.
/// But on a 1080p phone screen of 6 inches, the user won't be able to read text of 20 pixels. So, usually, the content scale will be around 3.0-4.0.
/// This makes the character take 60-80 pixels, and be a readable size on the tiny hidpi phone screen.
///
/// But egui deals with units based on the winit system of physical/logical units.
/// winit has physical pixels. And it uses content scale, to define "logical points". So, on a screen with a scale of 2.0, logical points are made up of 2 pixels per point.
/// that is why, egui calls it "pixels_per_point" in RawInput struct. This makes it easy for egui to integrate with winit.
///
/// The problem? Glfw's virtual units don't necessarily map to logical points. On some displays, the virtual units could be same as physical pixel units.
/// But at the same time, the content scale could be 4.0. It is the responsibility of user to divide virtual units with physical pixels and
/// figure out the number of pixels per virtual unit. And keep this separate from the content scale.
///
/// So, we will use glfw's physical pixel size and "emulate" logical points to match the egui expectations.
///
pub struct GlfwBackend {
    pub glfw: glfw::Glfw,
    pub events_receiver: Receiver<(f64, WindowEvent)>,
    pub window: glfw::Window,
    /// in virtual units
    pub window_size_virtual: [u32; 2],
    /// in logical points
    pub window_size_logical: [f32; 2],
    /// in physical pixels
    pub framebuffer_size_physical: [u32; 2],
    pub window_position: [i32; 2],
    /// ratio between pixels and virtual units
    pub physical_pixels_per_virtual_unit: f32,
    /// ratio between logical points and physical pixels
    pub scale: f32,
    pub raw_input: RawInput,
    pub cursor_icon: glfw::StandardCursor,
    pub frame_events: Vec<WindowEvent>,
    pub resized_event_pending: bool,
    /// in logical points
    pub cursor_pos: [f32; 2],
    pub cursor_inside_bounds: bool,
}
impl Drop for GlfwBackend {
    fn drop(&mut self) {
        tracing::warn!("dropping glfw backend");
    }
}
/// Signature of Glfw callback function inside [`GlfwConfig`]
/// we provide a default callback for common usecases -> [`default_glfw_callback()`]
pub type GlfwCallback = Box<dyn FnOnce(&mut Glfw)>;
/// This is the signature for window callback inside new function of [`GlfwBackend`]
pub type WindowCallback = Box<dyn FnOnce(&mut glfw::Window)>;

/// The configuration struct for Glfw Backend
/// passed in to [`WindowBackend::new()`] of [`GlfwBackend`]
pub struct GlfwConfig {
    /// title of the window
    pub window_title: String,
    /// size of the window on creation
    pub size: [u32; 2],
    /// whether we want the window to be transparent
    pub transparent_window: Option<bool>,
    /// whether the window will be opengl or non-opengl (vulkan, metal etc..)
    /// It will be opengl window on windows/linux, and non-opengl on linux.
    /// If you want to use this with wgpu/vulkan etc.. or create your own gl context using egl, set this to false
    pub opengl_window: Option<bool>,
    /// This callback is called with `&mut Glfw` just before creating a window
    /// All advanced configuration can be done here. eg: opengl settings such as gl version, depth/stencil bits etc..
    pub glfw_callback: GlfwCallback,
    /// This will be called right after window creation and setting event polling.
    /// you can use this to do things at startup like resizing, changing title, changing to fullscreen etc..
    pub window_callback: WindowCallback,
}
impl Default for GlfwConfig {
    fn default() -> Self {
        Self {
            glfw_callback: Box::new(|_| {}),
            window_callback: Box::new(|_| {}),
            window_title: "glfw window".to_string(),
            transparent_window: None,
            opengl_window: None,
            size: [800, 600],
        }
    }
}

impl GlfwBackend {
    pub fn new(config: GlfwConfig) -> Self {
        let mut glfw_context =
            glfw::init(glfw::FAIL_ON_ERRORS).expect("failed to create glfw context");
        glfw_context.window_hint(WindowHint::ScaleToMonitor(true));

        let GlfwConfig {
            window_title,
            size,
            transparent_window,
            opengl_window,
            glfw_callback,
            window_callback,
        } = config;

        if let Some(transparent) = transparent_window {
            glfw_context.window_hint(WindowHint::TransparentFramebuffer(transparent));
        }
        if let Some(opengl_window) = opengl_window {
            if opengl_window {
                glfw_context.window_hint(WindowHint::ClientApi(ClientApiHint::OpenGl));
            } else {
                glfw_context.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
            }
        }
        (glfw_callback)(&mut glfw_context);

        // create a window
        let (mut window, events_receiver) = glfw_context
            .create_window(
                size[0] as _,
                size[1] as _,
                &window_title,
                glfw::WindowMode::Windowed,
            )
            .expect("failed to create glfw window");
        let api = window.get_client_api();
        if api == glfw::ffi::OPENGL_API || api == glfw::ffi::OPENGL_ES_API {
            window.make_current();
        }
        let should_poll = true;
        // set which events you care about
        window.set_pos_polling(should_poll);
        window.set_size_polling(should_poll);
        window.set_close_polling(should_poll);
        window.set_refresh_polling(should_poll);
        window.set_focus_polling(should_poll);
        window.set_iconify_polling(should_poll);
        window.set_framebuffer_size_polling(should_poll);
        window.set_key_polling(should_poll);
        window.set_char_polling(should_poll);
        window.set_mouse_button_polling(should_poll);
        window.set_cursor_pos_polling(should_poll);
        window.set_cursor_enter_polling(should_poll);
        window.set_scroll_polling(should_poll);
        window.set_drag_and_drop_polling(should_poll);

        #[cfg(not(target_os = "emscripten"))]
        {
            // emscripten doesn't have support for these yet. will get support for content scaling in 3.1.33
            window.set_char_mods_polling(should_poll);
            window.set_maximize_polling(should_poll);
            window.set_content_scale_polling(should_poll);
            window.set_store_lock_key_mods(should_poll);
        }
        #[cfg(not(target_os = "emscripten"))]
        let scale = window.get_content_scale().0;
        #[cfg(target_os = "emscripten")]
        let scale = {
            let scale = unsafe { emscripten_get_device_pixel_ratio() } as f32;
            if scale != 1.0 {
                let width = (800.0 * scale) as i32;
                let height = (600.0 * scale) as i32;
                window.set_size(width, height);
            }
            unsafe { emscripten_set_element_css_size(CANVAS_ELEMENT_NAME, 800.0, 600.0) };
            scale
        };

        (window_callback)(&mut window);

        // collect details and keep them updated
        let (physical_width, physical_height) = window.get_framebuffer_size();
        let (logical_width, logical_height) = (
            physical_width as f32 / scale,
            physical_height as f32 / scale,
        );
        let (virtual_width, virtual_height) = window.get_size();
        let pixels_per_virtual_unit = physical_width as f32 / virtual_width as f32;
        let cursor_pos_virtual_units = window.get_cursor_pos();
        // #[cfg(not(target_os = "emscripten"))]
        let logical_cursor_position = (
            cursor_pos_virtual_units.0 as f32 * pixels_per_virtual_unit / scale,
            cursor_pos_virtual_units.1 as f32 * pixels_per_virtual_unit / scale,
        );

        let size_physical_pixels = [physical_width as u32, physical_height as u32];
        let position = window.get_pos();
        let window_position = [position.0, position.1];
        // set raw input screen rect details so that first frame
        // will have correct size even without any resize event
        let raw_input = RawInput {
            screen_rect: Some(egui::Rect::from_points(&[
                Default::default(),
                [
                    physical_width as f32 / scale,
                    physical_height as f32 / scale,
                ]
                .into(),
            ])),
            pixels_per_point: Some(scale),
            ..Default::default()
        };
        tracing::info!(
            "GlfwBackend created. 
        physical_size: {physical_width}, {physical_height};
        logical_size: {logical_width}, {logical_height};
        virtual_size: {virtual_width}, {virtual_height};
        content_scale: {scale};
        pixels_per_virtual_unit: {pixels_per_virtual_unit};
        "
        );
        Self {
            glfw: glfw_context,
            events_receiver,
            window,
            framebuffer_size_physical: size_physical_pixels,
            scale,
            cursor_pos: [logical_cursor_position.0, logical_cursor_position.1],
            raw_input,
            frame_events: vec![],
            resized_event_pending: true, // provide so that on first prepare frame, renderers can set their viewport sizes
            cursor_icon: StandardCursor::Arrow,
            cursor_inside_bounds: false,
            window_size_logical: [logical_width, logical_height],
            window_size_virtual: [
                virtual_width.try_into().unwrap(),
                virtual_height.try_into().unwrap(),
            ],
            physical_pixels_per_virtual_unit: pixels_per_virtual_unit,
            window_position,
        }
    }

    pub fn take_raw_input(&mut self) -> RawInput {
        self.raw_input.take()
    }

    pub fn is_opengl(&self) -> bool {
        let api = self.window.get_client_api();
        match api {
            glfw::ffi::OPENGL_API | glfw::ffi::OPENGL_ES_API => true,
            glfw::ffi::NO_API => false,
            rest => panic!("invalid client api hint {rest}"),
        }
    }

    pub fn get_proc_address(&mut self, symbol: &str) -> *const core::ffi::c_void {
        if self.is_opengl() {
            self.window.get_proc_address(symbol)
        } else {
            unimplemented!("window is not opengl. cannot use get_proc_address.");
        }
    }

    pub fn get_window_size(&mut self) -> Option<[f32; 2]> {
        #[cfg(target_os = "emscripten")]
        let (width, height) = {
            let mut width = 0.0;
            let mut height = 0.0;
            unsafe {
                assert_eq!(
                    emscripten_get_element_css_size(
                        CANVAS_ELEMENT_NAME,
                        &mut width as *mut _,
                        &mut height as *mut _,
                    ),
                    0
                );
            }
            (width as f32, height as f32)
        };
        #[cfg(not(target_os = "emscripten"))]
        let (width, height) = {
            let (width, height) = self.window.get_framebuffer_size();
            (width as f32, height as f32)
        };
        self.window_size_logical = [width / self.scale, height / self.scale];
        [width, height].into()
    }

    pub fn set_window_size(&mut self, size: [f32; 2]) {
        #[cfg(target_os = "emscripten")]
        {
            self.window
                .set_size((size[0] * self.scale) as i32, (size[1] * self.scale) as i32);
            // change the canvas stye size too.
            unsafe {
                assert_eq!(
                    emscripten_set_element_css_size(
                        CANVAS_ELEMENT_NAME,
                        size[0] as _,
                        size[1] as _
                    ),
                    0
                );
            }
        }
        #[cfg(not(target_os = "emscripten"))]
        self.window.set_size(
            (size[0] * self.scale / self.physical_pixels_per_virtual_unit) as i32,
            (size[1] * self.scale / self.physical_pixels_per_virtual_unit) as i32,
        );
    }
}

impl GlfwBackend {
    #[allow(unused)]
    pub fn tick(&mut self) {
        self.frame_events.clear();
        // whether we got a cursor event in this frame.
        // if false, and the window is passthrough, we will manually get cursor pos and push it
        // otherwise, we do nothing.
        let mut cursor_event = false;
        for (_timestamp, event) in glfw::flush_messages(&self.events_receiver) {
            self.frame_events.push(event.clone());
            // if let &glfw::WindowEvent::CursorPos(..) = &event {
            //     continue;
            // }

            if let Some(ev) = match event {
                glfw::WindowEvent::FramebufferSize(width, height) => {
                    self.framebuffer_size_physical = [width as u32, height as u32];
                    self.resized_event_pending = true;
                    let (virtual_width, virtual_height) = self.window.get_size();
                    self.physical_pixels_per_virtual_unit = width as f32 / virtual_width as f32;
                    // logical size
                    let (logical_width, logical_height) =
                        (width as f32 / self.scale, height as f32 / self.scale);
                    #[cfg(target_os = "emscripten")]
                    let (logical_width, logical_height) = {
                        let mut width = 0.0;
                        let mut height = 0.0;
                        unsafe {
                            assert_eq!(
                                emscripten_get_element_css_size(
                                    CANVAS_ELEMENT_NAME,
                                    &mut width as *mut _,
                                    &mut height as *mut _,
                                ),
                                0
                            );
                        }
                        tracing::info!("window css size emscripten: width {width} height {height}");
                        (width as f32, height as f32)
                    };
                    self.window_size_logical = [logical_width, logical_height];
                    self.raw_input.screen_rect = Some(egui::Rect::from_two_pos(
                        Default::default(),
                        self.window_size_logical.into(),
                    ));

                    tracing::info!(
                        width,
                        height,
                        logical_width,
                        logical_height,
                        self.scale,
                        "framebuffer size changed"
                    );
                    None
                }
                glfw::WindowEvent::Size(width, height) => {
                    let (physical_width, physical_height) = self.window.get_framebuffer_size();
                    self.physical_pixels_per_virtual_unit = physical_width as f32 / width as f32;

                    tracing::info!(
                        width,
                        height,
                        physical_width,
                        physical_height,
                        self.physical_pixels_per_virtual_unit,
                        "window virtual size changed"
                    );
                    None
                }
                glfw::WindowEvent::MouseButton(mb, a, m) => {
                    let emb = Event::PointerButton {
                        pos: Pos2 {
                            x: self.cursor_pos[0],
                            y: self.cursor_pos[1],
                        },
                        button: glfw_to_egui_pointer_button(mb),
                        pressed: glfw_to_egui_action(a).unwrap_or_default(),
                        modifiers: glfw_to_egui_modifers(m),
                    };
                    Some(emb)
                }
                // we scroll 25 pixels at a time
                glfw::WindowEvent::Scroll(x, y) => {
                    Some(Event::Scroll([x as f32 * 25.0, y as f32 * 25.0].into()))
                }
                glfw::WindowEvent::Key(k, _, a, m) => match k {
                    glfw::Key::C => {
                        if glfw_to_egui_action(a).unwrap_or_default()
                            && m.contains(glfw::Modifiers::Control)
                        {
                            Some(Event::Copy)
                        } else {
                            None
                        }
                    }
                    glfw::Key::X => {
                        if glfw_to_egui_action(a).unwrap_or_default()
                            && m.contains(glfw::Modifiers::Control)
                        {
                            Some(Event::Cut)
                        } else {
                            None
                        }
                    }
                    glfw::Key::V => {
                        if glfw_to_egui_action(a).unwrap_or_default()
                            && m.contains(glfw::Modifiers::Control)
                        {
                            Some(Event::Text(
                                self.window.get_clipboard_string().unwrap_or_default(),
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
                .or_else(|| {
                    glfw_to_egui_key(k).map(|key| {
                        let pressed = glfw_to_egui_action(a);
                        let repeat = pressed.is_none();
                        Event::Key {
                            key,
                            pressed: pressed.unwrap_or_default(),
                            modifiers: glfw_to_egui_modifers(m),
                            repeat,
                        }
                    })
                }),
                glfw::WindowEvent::Char(c) => Some(Event::Text(c.to_string())),
                glfw::WindowEvent::ContentScale(x, _) => {
                    tracing::info!(
                        previous_scale = self.scale,
                        current_scale = x,
                        "content scale changed"
                    );
                    self.raw_input.pixels_per_point = Some(x);
                    self.scale = x;
                    self.window_size_logical = [
                        self.framebuffer_size_physical[0] as f32 / self.scale,
                        self.framebuffer_size_physical[1] as f32 / self.scale,
                    ];
                    self.raw_input.screen_rect = Some(egui::Rect::from_two_pos(
                        Default::default(),
                        self.window_size_logical.into(),
                    ));
                    let (virtual_width, virtual_height) = self.window.get_size();
                    let pixels_per_virtual_unit =
                        self.framebuffer_size_physical[0] as f32 / virtual_width as f32;
                    None
                }
                glfw::WindowEvent::Close => {
                    self.window.set_should_close(true);
                    None
                }
                glfw::WindowEvent::Pos(x, y) => {
                    info!(
                        previous_x = self.window_position[0],
                        previous_y = self.window_position[1],
                        current_x = x,
                        current_y = y,
                        "jokolay window position changed"
                    );
                    self.window_position = [x, y];

                    None
                }
                glfw::WindowEvent::FileDrop(f) => {
                    self.raw_input
                        .dropped_files
                        .extend(f.into_iter().map(|p| egui::DroppedFile {
                            path: Some(p),
                            name: "".to_string(),
                            last_modified: None,
                            bytes: None,
                            mime: Default::default(),
                        }));
                    None
                }
                // this is in physical coords for some reason
                glfw::WindowEvent::CursorPos(x, y) => {
                    self.cursor_inside_bounds = true;
                    cursor_event = true;
                    // #[cfg(not(target_arch = "wasm32"))]
                    let (x, y) = (
                        x as f32 * self.physical_pixels_per_virtual_unit / self.scale,
                        y as f32 * self.physical_pixels_per_virtual_unit / self.scale,
                    );
                    self.cursor_pos = [x, y];
                    Some(egui::Event::PointerMoved(self.cursor_pos.into()))
                }
                WindowEvent::CursorEnter(c) => {
                    self.cursor_inside_bounds = c;
                    #[cfg(not(target_os = "emscripten"))]
                    if c {
                        None
                    } else if !self.window.is_mouse_passthrough() {
                        // if window is not passthrough, then we forward the event.
                        Some(Event::PointerGone)
                    } else {
                        // if it is passthrough, then we will let the simulated event take care of this
                        // because the pointer might still be within bounds even if we get cursor left event due to window losing focus due to passthrough
                        None
                    }
                    #[cfg(target_os = "emscripten")]
                    if c {
                        None
                    } else {
                        Some(Event::PointerGone)
                    }
                }
                _rest => None,
            } {
                self.raw_input.events.push(ev);
            }
        }

        let virtual_cursor_pos = self.window.get_cursor_pos();

        // #[cfg(not(target_os = "emscripten"))]
        let logical_cursor_pos = [
            virtual_cursor_pos.0 as f32 * self.physical_pixels_per_virtual_unit / self.scale,
            virtual_cursor_pos.1 as f32 * self.physical_pixels_per_virtual_unit / self.scale,
        ];

        // when there's no cursor event and window is passthrough, then, simulate mouse events
        #[cfg(not(target_os = "emscripten"))]
        if !cursor_event && self.window.is_mouse_passthrough() {
            let window_bounds =
                egui::Rect::from_two_pos(Default::default(), self.window_size_logical.into());
            // if cursor within window bounds
            if window_bounds.contains(logical_cursor_pos.into()) {
                // if cursor position has changed since last frame.
                if logical_cursor_pos != self.cursor_pos {
                    // we will manually push the cursor moved event.
                    self.raw_input.events.push(Event::PointerMoved(
                        [logical_cursor_pos[0], logical_cursor_pos[1]].into(),
                    ));
                }
                self.cursor_inside_bounds = true;
            } else {
                // if present cursor is out of bounds for the first time, we need to simulate a pointer gone event.
                // we use the cursor inside bounds flag to keep track of whether the cursor was active
                if self.cursor_inside_bounds {
                    self.raw_input.events.push(Event::PointerGone);
                    // will only be true if we set a new pointermoved event using window event loop or cursor coming into bounds again.
                    self.cursor_inside_bounds = false;
                }
            }
        }
        self.cursor_pos = logical_cursor_pos;
    }
    pub fn set_cursor(&mut self, cursor: egui::CursorIcon) {
        let cursor = egui_to_glfw_cursor(cursor);
        if cursor != self.cursor_icon {
            self.cursor_icon = cursor;
            self.window.set_cursor(Some(glfw::Cursor::standard(cursor)));
        }
    }
}

/// a function to get the matching egui key event for a given glfw key. egui does not support all the keys provided here.
fn glfw_to_egui_key(key: glfw::Key) -> Option<Key> {
    match key {
        glfw::Key::Space => Some(Key::Space),
        glfw::Key::Num0 => Some(Key::Num0),
        glfw::Key::Num1 => Some(Key::Num1),
        glfw::Key::Num2 => Some(Key::Num2),
        glfw::Key::Num3 => Some(Key::Num3),
        glfw::Key::Num4 => Some(Key::Num4),
        glfw::Key::Num5 => Some(Key::Num5),
        glfw::Key::Num6 => Some(Key::Num6),
        glfw::Key::Num7 => Some(Key::Num7),
        glfw::Key::Num8 => Some(Key::Num8),
        glfw::Key::Num9 => Some(Key::Num9),
        glfw::Key::A => Some(Key::A),
        glfw::Key::B => Some(Key::B),
        glfw::Key::C => Some(Key::C),
        glfw::Key::D => Some(Key::D),
        glfw::Key::E => Some(Key::E),
        glfw::Key::F => Some(Key::F),
        glfw::Key::G => Some(Key::G),
        glfw::Key::H => Some(Key::H),
        glfw::Key::I => Some(Key::I),
        glfw::Key::J => Some(Key::J),
        glfw::Key::K => Some(Key::K),
        glfw::Key::L => Some(Key::L),
        glfw::Key::M => Some(Key::M),
        glfw::Key::N => Some(Key::N),
        glfw::Key::O => Some(Key::O),
        glfw::Key::P => Some(Key::P),
        glfw::Key::Q => Some(Key::Q),
        glfw::Key::R => Some(Key::R),
        glfw::Key::S => Some(Key::S),
        glfw::Key::T => Some(Key::T),
        glfw::Key::U => Some(Key::U),
        glfw::Key::V => Some(Key::V),
        glfw::Key::W => Some(Key::W),
        glfw::Key::X => Some(Key::X),
        glfw::Key::Y => Some(Key::Y),
        glfw::Key::Z => Some(Key::Z),
        glfw::Key::Escape => Some(Key::Escape),
        glfw::Key::Enter => Some(Key::Enter),
        glfw::Key::Tab => Some(Key::Tab),
        glfw::Key::Backspace => Some(Key::Backspace),
        glfw::Key::Insert => Some(Key::Insert),
        glfw::Key::Delete => Some(Key::Delete),
        glfw::Key::Right => Some(Key::ArrowRight),
        glfw::Key::Left => Some(Key::ArrowLeft),
        glfw::Key::Down => Some(Key::ArrowDown),
        glfw::Key::Up => Some(Key::ArrowUp),
        glfw::Key::PageUp => Some(Key::PageUp),
        glfw::Key::PageDown => Some(Key::PageDown),
        glfw::Key::Home => Some(Key::Home),
        glfw::Key::End => Some(Key::End),
        _ => None,
    }
}

pub fn glfw_to_egui_modifers(modifiers: glfw::Modifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: modifiers.contains(glfw::Modifiers::Alt),
        ctrl: modifiers.contains(glfw::Modifiers::Control),
        shift: modifiers.contains(glfw::Modifiers::Shift),
        mac_cmd: false,
        command: modifiers.contains(glfw::Modifiers::Control),
    }
}

pub fn glfw_to_egui_pointer_button(mb: glfw::MouseButton) -> PointerButton {
    match mb {
        glfw::MouseButton::Button1 => PointerButton::Primary,
        glfw::MouseButton::Button2 => PointerButton::Secondary,
        glfw::MouseButton::Button3 => PointerButton::Middle,
        glfw::MouseButton::Button4 => PointerButton::Extra1,
        glfw::MouseButton::Button5 => PointerButton::Extra2,
        _ => PointerButton::Secondary,
    }
}
/// will return true if pressed, false if released and None if repeat
/// this allows us to use `unwrap_or_default` to get pressed as false when we get a key repeat event
pub fn glfw_to_egui_action(a: glfw::Action) -> Option<bool> {
    match a {
        Action::Release => Some(false),
        Action::Press => Some(true),
        Action::Repeat => None,
    }
}
/// This converts egui's cursor  icon into glfw's cursor which can be set by glfw.
/// we can get some sample cursor images and use them in place of missing icons (like diagonal resizing cursor)
pub fn egui_to_glfw_cursor(cursor: egui::CursorIcon) -> glfw::StandardCursor {
    match cursor {
        egui::CursorIcon::Default => StandardCursor::Arrow,
        egui::CursorIcon::Crosshair => StandardCursor::Crosshair,
        egui::CursorIcon::VerticalText | egui::CursorIcon::Text => StandardCursor::IBeam,
        egui::CursorIcon::Grab | egui::CursorIcon::Grabbing => StandardCursor::Hand,
        egui::CursorIcon::ResizeColumn
        | egui::CursorIcon::ResizeWest
        | egui::CursorIcon::ResizeEast
        | egui::CursorIcon::ResizeHorizontal => StandardCursor::HResize,
        egui::CursorIcon::ResizeRow
        | egui::CursorIcon::ResizeNorth
        | egui::CursorIcon::ResizeSouth
        | egui::CursorIcon::ResizeVertical => StandardCursor::VResize,
        _ => StandardCursor::Arrow,
    }
}

#[allow(non_camel_case_types)]
type em_callback_func = unsafe extern "C" fn();

#[allow(unused)]
const CANVAS_ELEMENT_NAME: *const std::ffi::c_char = "#canvas\0".as_ptr() as _;
extern "C" {
    // This extern is built in by Emscripten.
    pub fn emscripten_run_script_int(x: *const std::ffi::c_uchar) -> std::ffi::c_int;
    pub fn emscripten_cancel_main_loop();
    pub fn emscripten_set_main_loop(
        func: em_callback_func,
        fps: std::ffi::c_int,
        simulate_infinite_loop: std::ffi::c_int,
    );
    pub fn emscripten_get_device_pixel_ratio() -> std::ffi::c_double;
    pub fn emscripten_set_element_css_size(
        target: *const std::ffi::c_char,
        width: std::ffi::c_double,
        height: std::ffi::c_double,
    ) -> std::ffi::c_int;
    pub fn emscripten_get_element_css_size(
        target: *const std::ffi::c_char,
        width: *mut std::ffi::c_double,
        height: *mut std::ffi::c_double,
    ) -> std::ffi::c_int;

}

thread_local!(static MAIN_LOOP_CALLBACK: std::cell::RefCell<Option<Box<dyn FnMut()>>>  = std::cell::RefCell::new(None));

pub fn set_main_loop_callback<F: 'static>(callback: F)
where
    F: FnMut(),
{
    MAIN_LOOP_CALLBACK.with(|log| {
        *log.borrow_mut() = Some(Box::new(callback));
    });

    unsafe {
        emscripten_set_main_loop(wrapper::<F>, 0, 1);
    }
    #[allow(clippy::extra_unused_type_parameters)]
    extern "C" fn wrapper<F>()
    where
        F: FnMut(),
    {
        MAIN_LOOP_CALLBACK.with(|z| {
            if let Some(ref mut callback) = *z.borrow_mut() {
                callback();
            }
        });
    }
}
