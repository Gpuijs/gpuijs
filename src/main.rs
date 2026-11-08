mod background;
mod bench;
mod css;
mod cull;
mod editing;
mod input;
mod media;
mod native;
mod protocol;
mod render;
mod sticky;
mod svg;
mod transition;
mod tree;

use std::io::{BufReader, ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use futures::StreamExt;
use gpui::{AppContext, Application};
use serde::Deserialize;

use crate::protocol::Msg;
use crate::render::Root;

/// A packaged app's `Contents/Resources/gpuijs.json`, written by `gpuijs build`.
#[derive(Deserialize)]
struct AppInfo {
    name: String,
    /// The bundle identifier, which names the app's data and cache directories.
    id: String,
}

fn main() -> Result<()> {
    // `gpuijs <bundle.js>` in development, run under Bun; with no argument, a packaged app.
    let (mut sidecar, app) = match std::env::args().nth(1) {
        Some(entry) => {
            let mut bun =
                Command::new(std::env::var("GPUIJS_BUN").unwrap_or_else(|_| "bun".into()));
            bun.arg(entry);
            (bun, None)
        }
        None => packaged()?,
    };
    let socket = std::env::temp_dir().join(format!("gpuijs-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener =
        UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;

    // JS writes its messages to a pipe with blocking writes, so they are out by the time it waits
    // for a layout answer (a socket write can sit in Bun's event loop). Answers come back on a
    // second pipe JS can block on, held open read-write here so neither side's open blocks.
    let fifo = |ext: &str| -> Result<PathBuf> {
        let path = std::env::temp_dir().join(format!("gpuijs-{}.{ext}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        if !Command::new("mkfifo").arg(&path).status()?.success() {
            bail!("mkfifo {} failed", path.display());
        }
        Ok(path)
    };
    let (input, sync) = (fifo("in")?, fifo("sync")?);
    let sync_pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&sync)
        .with_context(|| format!("opening {}", sync.display()))?;

    let mut child = sidecar
        .env("GPUIJS_SOCKET", &socket)
        .env("GPUIJS_IN", &input)
        .env("GPUIJS_SYNC", &sync)
        .spawn()
        .with_context(|| format!("spawning {sidecar:?}"))?;
    // Reads from the start: JS writes its first messages (the stylesheet) as soon as it starts.
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let (child_tx, child_rx) = std::sync::mpsc::channel::<Child>();
    let input_path = input.clone();
    std::thread::spawn(move || {
        // Opens once JS opens its end; ends when JS exits.
        let mut reader = match std::fs::File::open(&input_path) {
            Ok(file) => BufReader::new(file),
            Err(e) => return eprintln!("[gpuijs] opening {}: {e}", input_path.display()),
        };
        // Frames: a u8 kind (0 a JSON `Msg`, 1 a binary commit, 2 a canvas's pixels) and the u32
        // length of the rest.
        let mut head = [0u8; 5];
        while reader.read_exact(&mut head).is_ok() {
            let len = u32::from_le_bytes(head[1..].try_into().unwrap()) as usize;
            // Past this the stream is out of step (or JS sent something absurd): stop reading.
            if len > 1 << 30 {
                eprintln!("[gpuijs] bad frame from js: {len} bytes");
                break;
            }
            let mut body = vec![0; len];
            if reader.read_exact(&mut body).is_err() {
                break;
            }
            let parsed = match head[0] {
                0 => serde_json::from_slice::<Msg>(&body).map_err(anyhow::Error::from),
                1 => protocol::decode_commit(&body).map(|ops| Msg::Commit { ops }),
                _ => protocol::decode_pixels(&body),
            };
            match parsed {
                Ok(msg) => {
                    if tx.unbounded_send(msg).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("[gpuijs] bad message from js: {e}"),
            }
        }
        if let Ok(mut child) = child_rx.recv() {
            match child.wait() {
                Ok(status) => eprintln!("[gpuijs] js process exited: {status}"),
                Err(e) => eprintln!("[gpuijs] js process wait failed: {e}"),
            }
        }
        // Dropping `tx` here ends the UI's message stream, which quits the app.
    });
    let stream = accept(&listener, &mut child);
    let _ = std::fs::remove_file(&socket);
    let stream = stream?;
    let _ = child_tx.send(child);
    // The window opens with what JS's `new Window(options)` (or a bare `render`) asked for.
    let spec = match futures::executor::block_on(rx.next()) {
        Some(Msg::Window { options }) => options,
        Some(other) => bail!("js sent {other:?} before creating its Window"),
        None => bail!("js exited before creating its Window"),
    };

    let application = Application::new();
    // The dock icon clicked: JS's `app` hears `activate`.
    application.on_reopen(|cx| {
        if let Some(root) = cx.windows().first().and_then(|w| w.downcast::<Root>()) {
            let _ = root.update(cx, |root, _, _| root.app_event("activate"));
        }
    });
    application.run(move |cx| {
        cx.bind_keys(input::bindings());
        // `GPUIJS_WINDOW_SIZE=1440x900`, as a browser window's viewport is sized.
        let fixed_size = std::env::var("GPUIJS_WINDOW_SIZE").ok().and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        });
        let options = spec.options(app.map(|a| a.name), fixed_size, cx);
        cx.open_window(options, |window, cx| {
            let handle = window.window_handle();
            cx.new(|cx| {
                cx.observe_window_visibility(window, |root: &mut Root, window, _| {
                    root.visibility_changed(window)
                })
                .detach();
                // gpui doesn't redraw for it: restyle `prefers-color-scheme` and `light-dark()`.
                cx.observe_window_appearance(window, |_, _, cx| cx.notify())
                    .detach();
                // What JS's `Window` reports and emits.
                cx.observe_window_activation(window, |root: &mut Root, window, _| {
                    let name = if window.is_window_active() {
                        "focus"
                    } else {
                        "blur"
                    };
                    root.window_event(name, window)
                })
                .detach();
                cx.observe_window_bounds(window, |root: &mut Root, window, _| {
                    root.window_event("resize", window)
                })
                .detach();
                let root = cx.weak_entity();
                window.on_window_should_close(cx, move |window, cx| {
                    let _ = root.update(cx, |root, _| root.window_event("close", window));
                    true
                });
                let root = Root::new(stream, sync_pipe, rx, handle, cx);
                root.window_event("ready-to-show", window);
                root
            })
        })
        .expect("failed to open window");
        cx.on_window_closed(move |cx| {
            let _ = std::fs::remove_file(&input);
            let _ = std::fs::remove_file(&sync);
            cx.quit()
        })
        .detach();
        cx.activate(true);
    });
    Ok(())
}

/// A packaged app (`gpuijs build`): the host is `Contents/MacOS/<name>`, and the compiled JS sidecar
/// and the files it reads are in `Contents/Resources`. The app keeps its data in the user's Library.
fn packaged() -> Result<(Command, Option<AppInfo>)> {
    let exe = std::env::current_exe()?;
    let resources = exe
        .parent()
        .and_then(Path::parent)
        .context("locating the app bundle")?
        .join("Resources");
    let info = resources.join("gpuijs.json");
    let info: AppInfo = std::fs::read(&info)
        .map_err(anyhow::Error::from)
        .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?))
        .with_context(|| {
            format!(
                "reading {} (usage: gpuijs <bundle.js>, or run an app `gpuijs build` made)",
                info.display()
            )
        })?;
    let library =
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join("Library");

    // Finder starts apps with stdout and stderr on /dev/null: the host's and JS's output goes to a
    // log instead, replaced each launch. The sidecar inherits both.
    let logs = library.join("Logs");
    std::fs::create_dir_all(&logs)?;
    let log = std::fs::File::create(logs.join(format!("{}.log", info.name)))?;
    unsafe extern "C" {
        fn dup2(from: i32, to: i32) -> i32;
    }
    for fd in [1, 2] {
        // SAFETY: both are open descriptors; dup2 only swaps what `fd` refers to.
        if unsafe { dup2(log.as_raw_fd(), fd) } < 0 {
            bail!(
                "redirecting output to the log: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    let mut sidecar = Command::new(resources.join("app"));
    sidecar
        .env("GPUIJS_APP_NAME", &info.name)
        .env("GPUIJS_PUBLIC", resources.join("public"))
        .env(
            "GPUIJS_DATA_DIR",
            library.join("Application Support").join(&info.id),
        )
        .env("GPUIJS_CACHE_DIR", library.join("Caches").join(&info.id));
    let css = resources.join("app.css");
    if css.exists() {
        sidecar.env("GPUIJS_CSS", css);
    }
    Ok((sidecar, Some(info)))
}

/// Waits for the JS side to connect, failing fast if it dies first.
fn accept(listener: &UnixListener, child: &mut Child) -> Result<UnixStream> {
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait()? {
                    bail!("js process exited before connecting: {status}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    }
}
