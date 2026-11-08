//! The app's native shell, driven from JS with Electron's API (`js/src/native.js`): the window's
//! options, the application, dock and context menus, window and app operations, and dialogs.

use std::path::{Path, PathBuf};

use gpui::{
    Action, AnyWindowHandle, App, Bounds, DummyKeyboardMapper, KeyBinding, Keystroke, Menu,
    MenuItem, OsAction, PathPromptOptions, PromptLevel, SystemMenuType, TitlebarOptions, Window,
    WindowBounds, WindowOptions, point, px, size,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::input;
use crate::render::Bridge;
use crate::tree::HTML;

/// `new Window(options)`: what the window opens with, in Electron's names.
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowSpec {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub title: Option<String>,
    pub resizable: Option<bool>,
    pub minimizable: Option<bool>,
    /// `hidden` / `hiddenInset`: no title bar, the content under the traffic lights.
    pub title_bar_style: Option<String>,
    pub traffic_light_position: Option<Xy>,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Xy {
    pub x: f32,
    pub y: f32,
}

impl WindowSpec {
    /// `title` when the options name none (a packaged app's name); `fixed_size` wins over theirs.
    pub fn options(
        &self,
        title: Option<String>,
        fixed_size: Option<(f32, f32)>,
        cx: &App,
    ) -> WindowOptions {
        let (width, height) =
            fixed_size.unwrap_or((self.width.unwrap_or(1024.), self.height.unwrap_or(768.)));
        let style = self.title_bar_style.as_deref();
        // shortcut: `hiddenInset` only moves the traffic lights in; Electron also deepens the bar.
        let inset = (style == Some("hiddenInset")).then(|| point(px(20.), px(20.)));
        let min = (self.min_width.is_some() || self.min_height.is_some()).then(|| {
            size(
                px(self.min_width.unwrap_or(0.)),
                px(self.min_height.unwrap_or(0.)),
            )
        });
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(width), px(height)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: self.title.clone().or(title).map(Into::into),
                appears_transparent: matches!(style, Some("hidden" | "hiddenInset")),
                traffic_light_position: self
                    .traffic_light_position
                    .map(|p| point(px(p.x), px(p.y)))
                    .or(inset),
            }),
            is_resizable: self.resizable.unwrap_or(true),
            is_minimizable: self.minimizable.unwrap_or(true),
            window_min_size: min,
            ..Default::default()
        }
    }
}

/// A menu item as JS sends it: Electron's `MenuItem`, with menu roles already expanded.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub role: Option<String>,
    pub accelerator: Option<String>,
    pub checked: bool,
    pub enabled: bool,
    #[serde(default)]
    pub submenu: Vec<Item>,
}

/// What JS asks of the shell.
#[derive(Deserialize, Debug)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Call {
    /// `Menu.setApplicationMenu`: the menu bar's menus.
    Menu { items: Vec<Item> },
    /// `app.dock.setMenu`.
    Dockmenu { items: Vec<Item> },
    /// `menu.popup()`: a context menu at the pointer; `menu-closed` answers once it closes.
    Popup { req: u64, items: Vec<Item> },
    /// A `Window` method: `setTitle`, `minimize`, `maximize`...
    Win {
        op: String,
        #[serde(default)]
        arg: Value,
    },
    /// An `app` method: `quit`, `hide`, `show`...
    App { op: String },
    /// `dialog.showOpenDialog` (`open`), `showSaveDialog` (`save`) or `showMessageBox` (`message`);
    /// `dialog` answers with `req`.
    Dialog {
        req: u64,
        kind: String,
        #[serde(default)]
        options: Value,
    },
}

/// A menu item JS made: JS runs its `click`.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = gpuijs, no_json)]
pub struct MenuClick {
    pub id: u64,
}

/// An Electron `role` the host carries out (`quit`, `minimize`...).
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = gpuijs, no_json)]
pub struct MenuRole {
    pub role: String,
}

const HOST_ROLES: [&str; 9] = [
    "quit",
    "hide",
    "hideOthers",
    "unhide",
    "front",
    "minimize",
    "zoom",
    "close",
    "togglefullscreen",
];

/// Menu actions go back to JS (clicks) or are done here (roles).
pub fn init(bridge: &Bridge, window: AnyWindowHandle, cx: &mut App) {
    let bridge = bridge.clone();
    cx.on_action(move |a: &MenuClick, _| bridge.event(HTML, "menu", json!({ "item": a.id })));
    cx.on_action(move |a: &MenuRole, cx| match a.role.as_str() {
        "quit" => cx.quit(),
        "hide" => cx.hide(),
        "hideOthers" => cx.hide_other_apps(),
        "unhide" => cx.unhide_other_apps(),
        "front" => cx.activate(true),
        "zoom" => window_op(window, "zoom", &Value::Null, cx),
        "togglefullscreen" => window_op(window, "toggleFullScreen", &Value::Null, cx),
        op => window_op(window, op, &Value::Null, cx),
    });
}

pub fn handle(call: Call, window: AnyWindowHandle, bridge: &Bridge, cx: &mut App) {
    match call {
        Call::Menu { items } => {
            let mut keys = Vec::new();
            let menus = items
                .iter()
                .filter_map(|item| match menu_item(item, &mut keys) {
                    MenuItem::Submenu(menu) => Some(menu),
                    _ => None, // Electron's menu bar shows submenus only.
                })
                .collect();
            // The new menu's accelerators replace the old one's.
            cx.clear_key_bindings();
            cx.bind_keys(input::bindings());
            cx.bind_keys(keys);
            cx.set_menus(menus);
        }
        Call::Dockmenu { items } => cx.set_dock_menu(menu_items(&items, &mut Vec::new())),
        Call::Popup { req, items } => {
            let closed = cx.popup_menu(menu_items(&items, &mut Vec::new()));
            let bridge = bridge.clone();
            cx.spawn(async move |_| {
                closed.await;
                bridge.event(HTML, "menu-closed", json!({ "req": req }));
            })
            .detach();
        }
        Call::Win { op, arg } => window_op(window, &op, &arg, cx),
        Call::App { op } => match op.as_str() {
            "quit" => cx.quit(),
            "hide" => cx.hide(),
            "show" | "focus" => cx.activate(true),
            _ => eprintln!("[gpuijs] unknown app call: {op}"),
        },
        Call::Dialog { req, kind, options } => dialog(req, &kind, &options, window, bridge, cx),
    }
}

/// The window's state, which JS's `Window` reports from (`isMaximized()`, `getBounds()`...) and
/// derives events from (`maximize`, `enter-full-screen`...).
pub fn window_state(name: &str, window: &Window) -> Value {
    let b = window.bounds();
    json!({
        "name": name,
        "focused": window.is_window_active(),
        "maximized": window.is_maximized(),
        "fullScreen": window.is_fullscreen(),
        "bounds": {
            "x": f32::from(b.origin.x),
            "y": f32::from(b.origin.y),
            "width": f32::from(b.size.width),
            "height": f32::from(b.size.height),
        },
    })
}

fn window_op(window: AnyWindowHandle, op: &str, arg: &Value, cx: &mut App) {
    let _ = window.update(cx, |_, window, _| match op {
        "setTitle" => window.set_window_title(arg.as_str().unwrap_or_default()),
        "minimize" => window.minimize_window(),
        "zoom" => window.zoom_window(),
        "maximize" if !window.is_maximized() => window.zoom_window(),
        "unmaximize" if window.is_maximized() => window.zoom_window(),
        "toggleFullScreen" => window.toggle_fullscreen(),
        "setFullScreen" if arg.as_bool() != Some(window.is_fullscreen()) => {
            window.toggle_fullscreen()
        }
        "close" => window.remove_window(),
        "focus" => window.activate_window(),
        "setSize" => {
            if let (Some(w), Some(h)) = (arg[0].as_f64(), arg[1].as_f64()) {
                window.resize(size(px(w as f32), px(h as f32)));
            }
        }
        "maximize" | "unmaximize" | "setFullScreen" => {}
        _ => eprintln!("[gpuijs] unknown window call: {op}"),
    });
}

fn menu_items(items: &[Item], keys: &mut Vec<KeyBinding>) -> Vec<MenuItem> {
    items.iter().map(|item| menu_item(item, keys)).collect()
}

/// One item, its accelerator added to `keys`.
fn menu_item(item: &Item, keys: &mut Vec<KeyBinding>) -> MenuItem {
    let label = item.label.clone();
    match (item.kind.as_str(), item.role.as_deref()) {
        ("separator", _) => return MenuItem::separator(),
        (_, Some("services")) => return MenuItem::os_submenu(label, SystemMenuType::Services),
        ("submenu", _) => {
            return MenuItem::submenu(Menu {
                name: label.into(),
                items: menu_items(&item.submenu, keys),
            });
        }
        _ => {}
    }
    let key = item.accelerator.as_deref().and_then(keystroke);
    let mut bind = |action: &dyn Action| {
        if let Some(key) = &key {
            // `key` parsed already, so loading it can't fail.
            let binding = KeyBinding::load(
                key,
                action.boxed_clone(),
                None,
                false,
                None,
                &DummyKeyboardMapper,
            );
            keys.extend(binding.ok());
        }
    };
    // Text editing goes through the text inputs' own actions, which carry their own keys.
    let made = match item.role.as_deref() {
        Some("cut") => MenuItem::os_action(label, input::Cut, OsAction::Cut),
        Some("copy") => MenuItem::os_action(label, input::Copy, OsAction::Copy),
        Some("paste") => MenuItem::os_action(label, input::Paste, OsAction::Paste),
        Some("selectAll") => MenuItem::os_action(label, input::SelectAll, OsAction::SelectAll),
        Some("about") => MenuItem::os_action(label, MenuClick { id: item.id }, OsAction::About),
        Some(role) if HOST_ROLES.contains(&role) => {
            let action = MenuRole {
                role: role.to_owned(),
            };
            bind(&action);
            MenuItem::action(label, action)
        }
        _ => {
            let action = MenuClick { id: item.id };
            bind(&action);
            MenuItem::action(label, action)
        }
    };
    made.checked(item.checked).disabled(!item.enabled)
}

/// An Electron accelerator (`CmdOrCtrl+Shift+S`) as a gpui keystroke (`cmd-shift-s`), or `None`
/// for one gpui can't bind.
pub fn keystroke(accelerator: &str) -> Option<String> {
    let parts: Vec<&str> = accelerator.split('+').collect();
    let (key, modifiers) = parts.split_last()?;
    let mut out = Vec::new();
    for m in modifiers {
        out.push(match m.trim().to_ascii_lowercase().as_str() {
            "command" | "cmd" | "commandorcontrol" | "cmdorctrl" | "super" | "meta" => "cmd",
            "control" | "ctrl" => "ctrl",
            "alt" | "option" | "altgr" => "alt",
            "shift" => "shift",
            _ => return None,
        });
    }
    let key = key.trim().to_ascii_lowercase();
    let key = match key.as_str() {
        "return" | "enter" => "enter",
        "esc" | "escape" => "escape",
        "plus" => "+",
        "space" => "space",
        "tab" | "backspace" | "delete" | "up" | "down" | "left" | "right" | "home" | "end"
        | "pageup" | "pagedown" => key.as_str(),
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => f,
        k if k.chars().count() == 1 => k,
        _ => return None,
    };
    out.push(key);
    let keystroke = out.join("-");
    Keystroke::parse(&keystroke).ok()?;
    Some(keystroke)
}

fn dialog(
    req: u64,
    kind: &str,
    options: &Value,
    window: AnyWindowHandle,
    bridge: &Bridge,
    cx: &mut App,
) {
    let bridge = bridge.clone();
    let reply = move |mut result: Value| {
        result["req"] = json!(req);
        bridge.event(HTML, "dialog", result);
    };
    let text = |key: &str| options[key].as_str().map(str::to_owned);
    match kind {
        "open" => {
            let has = |p: &str| {
                options["properties"]
                    .as_array()
                    .is_some_and(|ps| ps.iter().any(|v| v == p))
            };
            let paths = cx.prompt_for_paths(PathPromptOptions {
                files: has("openFile") || !has("openDirectory"),
                directories: has("openDirectory"),
                multiple: has("multiSelections"),
                prompt: text("buttonLabel").map(Into::into),
            });
            cx.spawn(async move |_| {
                let paths = paths.await.ok().and_then(Result::ok).flatten();
                reply(
                    json!({ "canceled": paths.is_none(), "filePaths": paths.unwrap_or_default() }),
                );
            })
            .detach();
        }
        "save" => {
            let default = text("defaultPath").map(PathBuf::from).unwrap_or_default();
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            // A bare name goes in the home folder, as Electron's does.
            let (dir, name) = match (default.parent(), default.file_name()) {
                (Some(dir), name) if dir != Path::new("") => (dir.to_owned(), name),
                (_, name) => (home, name),
            };
            let path = cx.prompt_for_new_path(&dir, name.and_then(|n| n.to_str()));
            cx.spawn(async move |_| {
                let path = path.await.ok().and_then(Result::ok).flatten();
                reply(json!({ "canceled": path.is_none(), "filePath": path.unwrap_or_default() }));
            })
            .detach();
        }
        "message" => {
            let level = match options["type"].as_str() {
                Some("warning") => PromptLevel::Warning,
                Some("error") => PromptLevel::Critical,
                _ => PromptLevel::Info,
            };
            let mut buttons: Vec<String> = options["buttons"]
                .as_array()
                .map(|bs| {
                    bs.iter()
                        .filter_map(|b| b.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if buttons.is_empty() {
                buttons.push("OK".into());
            }
            let message = text("message").unwrap_or_default();
            let detail = text("detail");
            let answer = window.update(cx, |_, window, cx| {
                let buttons: Vec<&str> = buttons.iter().map(String::as_str).collect();
                window.prompt(level, &message, detail.as_deref(), &buttons, cx)
            });
            cx.spawn(async move |_| {
                let response = match answer {
                    Ok(answer) => answer.await.unwrap_or(0),
                    Err(_) => 0,
                };
                reply(json!({ "response": response, "checkboxChecked": false }));
            })
            .detach();
        }
        _ => eprintln!("[gpuijs] unknown dialog: {kind}"),
    }
}

#[cfg(test)]
mod tests {
    use super::keystroke;

    #[test]
    fn accelerators_become_keystrokes() {
        assert_eq!(
            keystroke("CmdOrCtrl+Shift+S").as_deref(),
            Some("cmd-shift-s")
        );
        assert_eq!(keystroke("Alt+F4").as_deref(), Some("alt-f4"));
        assert_eq!(keystroke("Command+Return").as_deref(), Some("cmd-enter"));
        assert_eq!(keystroke("Ctrl+Plus").as_deref(), Some("ctrl-+"));
        assert_eq!(keystroke("Cmd+,").as_deref(), Some("cmd-,"));
        assert_eq!(keystroke("Hyper+K"), None);
        assert_eq!(keystroke("Cmd+MediaPlayPause"), None);
    }
}
