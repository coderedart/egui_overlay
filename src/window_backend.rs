use crate::GlfwWindow;
use egui::{Event, Key, PointerButton, Pos2, RawInput, Vec2};
use glfw::Action;

impl GlfwWindow {
    pub fn new() -> Result<Self, String> {
        let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)
            .map_err(|_| "failed to initialize glfw".to_string())?;
        glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

        glfw.window_hint(glfw::WindowHint::Floating(true));

        glfw.window_hint(glfw::WindowHint::TransparentFramebuffer(true));
        glfw.window_hint(glfw::WindowHint::MousePassthrough(true));

        let (mut window, events) = glfw
            .create_window(800, 600, "Overlay Window", glfw::WindowMode::Windowed)
            .ok_or("failed to create window window".to_string())?;

        window.set_all_polling(true);
        window.set_store_lock_key_mods(true);
        let (width, height) = window.get_framebuffer_size();
        let scale = window.get_content_scale();
        let cursor_position = window.get_cursor_pos();
        Ok(GlfwWindow {
            glfw,
            events_receiver: events,
            window,
            size_physical_pixels: [
                width.try_into().map_err(|_| "width not u32".to_string())?,
                height
                    .try_into()
                    .map_err(|_| "height not u32".to_string())?,
            ],
            scale: [scale.0, scale.1],
            cursor_pos_physical_pixels: [cursor_position.0 as f32, cursor_position.1 as f32],
            raw_input: RawInput::default(),
            frame_events: vec![],
        })
    }
    pub fn tick(&mut self) {
        self.glfw.poll_events();
        self.frame_events.clear();
        let cursor_position = self.window.get_cursor_pos();
        let cursor_position = [cursor_position.0 as f32, cursor_position.1 as f32];

        let mut input = RawInput {
            time: Some(self.glfw.get_time()),
            pixels_per_point: Some(self.scale[0]),
            screen_rect: Some(egui::Rect::from_two_pos(
                Default::default(),
                [
                    self.size_physical_pixels[0] as f32 / self.scale[0],
                    self.size_physical_pixels[1] as f32 / self.scale[1],
                ]
                .into(),
            )),
            ..Default::default()
        };
        if cursor_position != self.cursor_pos_physical_pixels {
            self.cursor_pos_physical_pixels = cursor_position;
            input.events.push(Event::PointerMoved(
                [
                    cursor_position[0] / self.scale[0],
                    cursor_position[1] / self.scale[1],
                ]
                .into(),
            ))
        }

        for (_, event) in glfw::flush_messages(&self.events_receiver) {
            if let &glfw::WindowEvent::CursorPos(..) = &event {
                continue;
            }
            self.frame_events.push(event.clone());
            if let Some(ev) = match event {
                glfw::WindowEvent::FramebufferSize(w, h) => {
                    self.size_physical_pixels = [w as u32, h as u32];
                    input.screen_rect = Some(egui::Rect::from_two_pos(
                        Default::default(),
                        [w as f32 / self.scale[0], h as f32 / self.scale[1]].into(),
                    ));

                    None
                }
                glfw::WindowEvent::MouseButton(mb, a, m) => {
                    let emb = Event::PointerButton {
                        pos: Pos2 {
                            x: cursor_position[0] / self.scale[0],
                            y: cursor_position[1] / self.scale[1],
                        },
                        button: glfw_to_egui_pointer_button(mb),
                        pressed: glfw_to_egui_action(a),
                        modifiers: glfw_to_egui_modifers(m),
                    };
                    Some(emb)
                }
                glfw::WindowEvent::CursorPos(..) => None,
                // we scroll 25 pixels at a time
                glfw::WindowEvent::Scroll(x, y) => {
                    Some(Event::Scroll([x as f32 * 25.0, y as f32 * 25.0].into()))
                }
                glfw::WindowEvent::Key(k, _, a, m) => match k {
                    glfw::Key::C => {
                        if glfw_to_egui_action(a) && m.contains(glfw::Modifiers::Control) {
                            Some(Event::Copy)
                        } else {
                            None
                        }
                    }
                    glfw::Key::X => {
                        if glfw_to_egui_action(a) && m.contains(glfw::Modifiers::Control) {
                            Some(Event::Cut)
                        } else {
                            None
                        }
                    }
                    glfw::Key::V => {
                        if glfw_to_egui_action(a) && m.contains(glfw::Modifiers::Control) {
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
                    glfw_to_egui_key(k).map(|key| Event::Key {
                        key,
                        pressed: glfw_to_egui_action(a),
                        modifiers: glfw_to_egui_modifers(m),
                    })
                }),
                glfw::WindowEvent::Char(c) => Some(Event::Text(c.to_string())),
                glfw::WindowEvent::ContentScale(x, y) => {
                    input.pixels_per_point = Some(x);
                    self.scale = [x, y];
                    None
                }
                glfw::WindowEvent::Close => {
                    self.window.set_should_close(true);
                    None
                }

                glfw::WindowEvent::FileDrop(f) => {
                    input
                        .dropped_files
                        .extend(f.into_iter().map(|p| egui::DroppedFile {
                            path: Some(p),
                            name: "".to_string(),
                            last_modified: None,
                            bytes: None,
                        }));
                    None
                }

                _rest => None,
            } {
                input.events.push(ev);
            }
        }
        self.raw_input = input;
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
        _ => PointerButton::Secondary,
    }
}

pub fn glfw_to_egui_action(a: glfw::Action) -> bool {
    match a {
        Action::Release => false,
        Action::Press => true,
        Action::Repeat => true,
    }
}
