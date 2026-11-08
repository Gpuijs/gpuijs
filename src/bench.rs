//! Input-to-pixels benchmarking, on when `GPUIJS_BENCH_SOCKET` names a socket path. A driver
//! connects and sends JSON lines; each gets one reply line with the same `id`:
//!
//! - `{"id":1,"cmd":"query","selector":".menu-item","index":2}` →
//!   `{"id":1,"rect":{"x":..,"y":..,"width":..,"height":..,"value":..}}` (`null` when nothing
//!   matches; `value` is a field's text);
//! - `{"id":2,"cmd":"input","input":{"kind":"move","x":10,"y":20}}` → `{"id":2,"ms":4.2}`: from
//!   injecting the input until the GPU finished the first frame drawn after JS handled it.
//!
//! Inputs enter where the platform's do (`Window::simulate_platform_input`); typing also inserts the text
//! into the focused field as macOS's text input does.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui::{
    App, EntityInputHandler, KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
    point, px,
};
use serde::Deserialize;
use serde_json::Value;

use crate::input::TextInput;

#[derive(Deserialize, Debug)]
pub struct Command {
    pub id: u64,
    #[serde(flatten)]
    pub what: What,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum What {
    /// The `index`th match (default the first).
    Query {
        selector: String,
        #[serde(default)]
        index: usize,
    },
    Input {
        input: Input,
    },
}

/// An input, at window coordinates in CSS pixels.
#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Input {
    Move {
        x: f32,
        y: f32,
    },
    /// A press and release; timed from the release, when `click` fires.
    Click {
        x: f32,
        y: f32,
    },
    /// `dy` as a DOM wheel event's `deltaY`: positive scrolls down.
    Wheel {
        x: f32,
        y: f32,
        dy: f32,
    },
    /// One key typed: its key event, then its text into the focused field.
    Key {
        text: String,
    },
}

/// Replies to the driver, once it has connected; shared by the tasks answering it.
#[derive(Clone, Default)]
pub struct Replies(Arc<Mutex<Option<UnixStream>>>);

impl Replies {
    pub fn send(&self, reply: Value) {
        if let Ok(mut stream) = self.0.lock()
            && let Some(stream) = stream.as_mut()
            && let Err(e) = writeln!(stream, "{reply}")
        {
            eprintln!("[gpuijs bench] failed to reply: {e}");
        }
    }
}

/// With `GPUIJS_BENCH_SOCKET` set, listens there, and returns the driver's commands (once it
/// connects, in the background) and where to reply.
pub fn listen() -> Option<(UnboundedReceiver<Command>, Replies)> {
    let path = std::env::var_os("GPUIJS_BENCH_SOCKET")?;
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!(
                "[gpuijs bench] can't listen on {}: {e}",
                path.to_string_lossy()
            );
            return None;
        }
    };
    let replies = Replies::default();
    let writer = replies.clone();
    let (tx, rx) = unbounded();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        if let Ok(mut slot) = writer.0.lock() {
            *slot = stream.try_clone().ok();
        }
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<Command>(&line) {
                Ok(command) => {
                    if tx.unbounded_send(command).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("[gpuijs bench] bad command {line:?}: {e}"),
            }
        }
    });
    Some((rx, replies))
}

/// Sends `input` into the window as the platform would; `focused` is the text field typing goes
/// into. Returns when its timing starts: a click's at the release, as a browser's `click` event.
pub fn inject(
    input: &Input,
    focused: Option<gpui::Entity<TextInput>>,
    window: &mut Window,
    cx: &mut App,
) -> Instant {
    let modifiers = Modifiers::default();
    let mut start = Instant::now();
    match *input {
        Input::Move { x, y } => {
            window.simulate_platform_input(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: point(px(x), px(y)),
                    pressed_button: None,
                    modifiers,
                }),
                cx,
            );
        }
        Input::Click { x, y } => {
            let position = point(px(x), px(y));
            window.simulate_platform_input(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            start = Instant::now();
            window.simulate_platform_input(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                }),
                cx,
            );
        }
        Input::Wheel { x, y, dy } => {
            window.simulate_platform_input(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(x), px(y)),
                    // gpui's deltas move the content: down the page is negative.
                    delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
                    modifiers,
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            );
        }
        Input::Key { ref text } => {
            let keystroke = Keystroke {
                modifiers,
                key: text.to_lowercase(),
                key_char: Some(text.clone()),
            };
            window.simulate_platform_input(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke,
                    is_held: false,
                }),
                cx,
            );
            if let Some(field) = focused {
                field.update(cx, |field, cx| {
                    field.replace_text_in_range(None, text, window, cx)
                });
            }
        }
    }
    start
}
