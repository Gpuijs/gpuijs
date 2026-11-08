//! Tree + computed styles -> gpui elements, rebuilt on every render.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AbsoluteLength, AnyElement, AnyWindowHandle, AppContext, Bounds, Context, DefiniteLength, Div,
    Entity, FocusHandle, Focusable, HighlightStyle, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, Length, MouseButton, ParentElement, Pixels, Refineable,
    Render, RenderImage, SharedString, Stateful, StatefulInteractiveElement, StyleRefinement,
    Styled, StyledImage, StyledText, Subscription, TextStyleRefinement, WeakEntity, Window,
    WindowAppearance, canvas, div, img, point, px, ScrollDelta, ScrollHandle,
};
use gpui::{DispatchPhase, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent};
use serde_json::json;

use crate::bench::{self, Replies};
use crate::css::cascade::{
    Changes, Computed, PSEUDO, PSEUDO_TEXT, StyleCache, pseudo_id, restyle_in_place,
};
use crate::css::props::{AppRegion, Disp, GENERATED, REM, TableRole, Ws, counter_style};
use crate::css::{Env, Pseudo, Stylesheet, warn_once};
use crate::cull::{self, Cull, Kid, List};
use crate::editing::{self, Segment};
use crate::input::TextInput;
use crate::media::{self, Player};
use crate::sticky::Sticky;
use crate::native;
use crate::protocol::{Msg, Op};
use crate::svg;
use crate::transition::Look;
use crate::tree::{self, BODY, HTML, IdMap, IdSet, Tree};

/// A culled list's children left out of this build: each run's first child maps to the run.
/// A list's left-out children: each run's index among its children, and the run.
type Runs = Vec<(usize, Vec<u64>)>;

/// A benchmark driver's inputs on their way to the screen (see `bench`).
struct BenchState {
    replies: Replies,
    /// Injected, waiting for JS to have handled them: when each was injected.
    injected: HashMap<u64, Instant>,
    /// Handled by JS: when each was injected, and since when the window holds all it changed.
    handled: Vec<(u64, Instant, Instant)>,
    /// When the last commit was applied.
    last_commit: Option<Instant>,
    /// Recent draws: when each began, and the frame number it is presented in.
    draws: std::collections::VecDeque<(Instant, u64)>,
}

/// An SVG's pixels (none if it can't be drawn) and the frame that last drew it.
type RasterizedSvg = (Option<Arc<RenderImage>>, Cell<u64>);
type Rasterize = Arc<dyn Fn(&Image) -> anyhow::Result<Arc<RenderImage>> + Send + Sync>;

/// Bigger SVGs than this (CSS px²) are redrawn off the UI thread when they change (see `svg`).
const SVG_SYNC_AREA: f32 = 128.0 * 128.0;
/// A layer of a big drawing with this little markup and no text draws in a ms or two: now.
const SVG_SYNC_MARKUP: usize = 8 * 1024;
/// An `<svg>` whose picture changes again within this long is animating.
const SVG_ANIMATING: Duration = Duration::from_millis(200);
/// How long a part of a big drawing that changed stays a layer of its own: long enough to outlast
/// pauses between hovers, since merging it back means redrawing its neighbours once more.
const SVG_HOT: Duration = Duration::from_secs(5);

/// Writes events back to the JS side. A thread does the writing: JS stops reading the socket while
/// it waits on a layout sync, and a full socket must not block the UI thread that answers the sync.
#[derive(Clone)]
pub struct Bridge(std::sync::mpsc::Sender<String>, Rc<Cell<bool>>);

/// Events that aren't input: JS sends no `settled` for them (see `Msg::Settled`).
const NOT_INPUT: [&str; 13] = [
    "frame",
    "resize",
    "scroll",
    "benchQuery",
    "benchSettle",
    "visibilitychange",
    "appearance",
    "menu",
    "menu-closed",
    "dialog",
    "window",
    "app",
    "media",
];

/// A scroll container's handle before its offset first went to JS.
fn unreported() -> (ScrollHandle, gpui::Point<Pixels>) {
    (ScrollHandle::new(), point(px(f32::NAN), px(f32::NAN)))
}

/// Where the pointer is, as a DOM event's `clientX`/`clientY`.
fn at(window: &Window) -> serde_json::Value {
    let p = window.mouse_position();
    json!({ "clientX": f32::from(p.x), "clientY": f32::from(p.y) })
}

/// `at`, plus the innermost node under the press: the DOM target, which JS bubbles the press from
/// up to the listener that reports it.
fn pressed_at(window: &Window, reported: &Reported) -> serde_json::Value {
    let mut extra = at(window);
    if let Some(target) = reported.down_target.get() {
        extra["target"] = json!(target);
    }
    extra
}

impl Bridge {
    fn new(mut stream: UnixStream) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in rx {
                if let Err(e) = writeln!(stream, "{line}") {
                    eprintln!("[gpuijs] failed to send a message to js: {e}");
                }
            }
        });
        Bridge(tx, Rc::default())
    }

    /// JS was sent input it hasn't reported done with (`Msg::Settled`).
    fn unsettled(&self) -> bool {
        self.1.get()
    }

    fn settled(&self) {
        self.1.set(false);
    }

    fn send(&self, msg: serde_json::Value) {
        // Fails only once the writer has stopped, which it never does on its own.
        let _ = self.0.send(msg.to_string());
    }

    /// Pointer events `document` should see that no element reported.
    fn document(&self, window: &Window, events: &[&str]) {
        self.document_from(window, None, events);
    }

    /// Pointer events no element reported, dispatched from `target` (else `<body>`).
    fn document_from(&self, window: &Window, target: Option<u64>, events: &[&str]) {
        let mut extra = at(window);
        extra["events"] = json!(events);
        if let Some(target) = target {
            extra["target"] = json!(target);
        }
        self.event(HTML, "document", extra);
    }

    pub fn event(&self, id: u64, kind: &str, mut extra: serde_json::Value) {
        extra["t"] = json!("event");
        extra["id"] = json!(id);
        extra["type"] = json!(kind);
        if !NOT_INPUT.contains(&kind) {
            self.1.set(true);
        }
        self.send(extra);
    }
}

/// An SVG's markup drawn to pixels, or none (warned about once) if it can't be.
fn rasterize_svg(rasterize: &Rasterize, markup: &str) -> Option<Arc<RenderImage>> {
    let image = Image::from_bytes(ImageFormat::Svg, markup.as_bytes().to_vec());
    let rendered = rasterize(&image);
    if let Err(e) = &rendered {
        warn_once(&format!("svg that cannot be drawn: {e}"));
    }
    rendered.ok()
}

/// The text `new` adds to `old` when it only adds at the end: to the last sheet, or as new sheets.
fn appended(old: &[String], new: &[String]) -> Option<String> {
    let (last, before) = old.split_last()?;
    let grown = new.get(before.len())?;
    // Split between rules only, or the added text would start inside one.
    let between_rules = last.trim_end().is_empty() || last.trim_end().ends_with('}');
    if new.len() < old.len()
        || before != &new[..before.len()]
        || !grown.starts_with(last.as_str())
        || !between_rules
    {
        return None;
    }
    let mut text = grown[last.len()..].to_owned();
    for sheet in &new[old.len()..] {
        text.push('\n');
        text.push_str(sheet);
    }
    Some(text)
}

/// After each `E` reaches the window: what the host changed for it alone (a scroll) goes to the GPU
/// now; input JS heard of goes once JS has settled (see `Msg::Settled`).
fn present_after<E: gpui::MouseEvent>(window: &mut Window, bridge: &Bridge) {
    let bridge = bridge.clone();
    window.on_mouse_event(move |_: &E, phase, window, cx| {
        if phase == DispatchPhase::Capture {
            let bridge = bridge.clone();
            window.defer(cx, move |window, cx| {
                if !bridge.unsettled() {
                    window.draw_and_present_now(cx);
                }
            });
        }
    });
}

/// Advance widths of printable ASCII in the UI font, in ems, regular and bold: JS measures text
/// synchronously from them (Recharts sizes axis labels that way), so they go out before anything.
fn text_metrics(cx: &gpui::App) -> serde_json::Value {
    let text = cx.text_system();
    let widths = |weight| {
        let id = text.resolve_font(&gpui::Font {
            weight,
            ..gpui::font(".SystemUIFont")
        });
        (' '..='~')
            .map(|c| {
                text.advance(id, px(100.0), c)
                    .map_or(0.55, |s| f32::from(s.width) / 100.0)
            })
            .collect::<Vec<f32>>()
    };
    json!({ "t": "metrics", "regular": widths(gpui::FontWeight::NORMAL), "bold": widths(gpui::FontWeight::BOLD) })
}

/// Per-press flags: whether this mouse event already went to JS (see `Root::boxed`).
#[derive(Default)]
struct Reported {
    down: Cell<bool>,
    up: Cell<bool>,
    click: Cell<bool>,
    /// An element reported the current pointer move; `document` hears it from JS then.
    moved: Cell<bool>,
    /// The innermost node under the current press: the target of a press no element reported.
    down_target: Cell<Option<u64>>,
    /// The press is on a native `<select>`, which toggles its own list.
    on_select: Cell<bool>,
    /// The innermost `app-region` box under the press took it (see `AppRegion`).
    region: Cell<bool>,
    /// The innermost `contextmenu` listener under a right press reported it.
    context: Cell<bool>,
    /// The innermost `dblclick` listener under a second click reported it.
    dblclick: Cell<bool>,
    /// The innermost `wheel` listener under the pointer reported the current wheel event.
    wheel: Cell<bool>,
    /// The press is in editable content, which keeps the focus it takes.
    editing: Cell<bool>,
    /// The last input was a key, not a press: focus shows as `:focus-visible` then, as in a
    /// browser (a menu opened by mouse shows no ring on its focused item).
    keyboard: Cell<bool>,
}

/// Pointer and focus state, tracked here rather than by gpui so state styles can be combined and animated.
#[derive(Default)]
struct UiState {
    hovered: bool,
    focused: bool,
    /// When the state last changed and what was on screen at that moment, for transitions.
    changed: Option<(Instant, Look)>,
}

struct Input {
    entity: Entity<TextInput>,
    focus: FocusHandle,
    /// The field's `inert` (see `TextInput`).
    inert: Rc<Cell<bool>>,
    placeholder_color: Rc<Cell<Option<gpui::Hsla>>>,
    grows: Rc<Cell<bool>>,
    _subscriptions: [Subscription; 2],
}

pub struct Root {
    _keystrokes: Subscription,
    this: WeakEntity<Root>,
    tree: Tree,
    sheet: Stylesheet,
    /// The app's stylesheet alone, and the `<style>`s libraries injected, which `sheet` combines.
    app_sheet: Stylesheet,
    injected: Vec<String>,
    // Shared: style-sharing twins (a list's rows) point at one computed style.
    styles: IdMap<std::rc::Rc<Computed>>,
    /// Cascade results for the current stylesheet and environment.
    style_cache: StyleCache,
    /// What commits changed since the last restyle.
    changes: Changes,
    states: IdMap<UiState>,
    /// Elements whose hover restyles a descendant (`group-hover:`), so their hover is tracked:
    /// how many styled nodes name each.
    groups: IdMap<u32>,
    /// The group each styled node names, if any.
    group_of: IdMap<u64>,
    /// Styled nodes with a `transition`.
    transitioning: IdSet,
    /// What removed nodes' styles held, dropped with the tree's garbage once a draw has answered JS.
    garbage: Vec<Box<dyn std::any::Any>>,
    /// Set while building when a transition is still running.
    animating: Cell<bool>,
    dirty: bool,
    env: Option<Env>,
    bridge: Bridge,
    /// Fonts' `1ch` as a fraction of their size (`None`: not installed), looked up as the cascade asks.
    font_ch: RefCell<HashMap<String, Option<f32>>>,
    inputs: IdMap<Input>,
    /// `<canvas>` pixels, as JS last sent them.
    canvases: IdMap<Arc<RenderImage>>,
    /// Pictures no longer shown (a canvas's past frames), given back to the GPU at the next render.
    stale_images: Vec<Arc<RenderImage>>,
    /// `<video>` and `<audio>` elements, with a player once they have a source.
    media: IdMap<Option<Player>>,
    /// Rasterizable SVG images by markup; gpui caches their pixels by the image's hash.
    /// Rasterized SVGs by markup, with the frame that last drew each; others are freed after a frame.
    svgs: RefCell<HashMap<String, RasterizedSvg>>,
    /// gpui's SVG rasterizer (its type isn't exported, so it's held by this closure).
    rasterize: Rasterize,
    /// The markup of the layers each `<svg>` last showed, in `svgs`: what it keeps showing while
    /// its new picture is drawn off the UI thread.
    svg_shown: RefCell<IdMap<Vec<String>>>,
    /// When each `<svg>`'s picture last changed: one changing again soon after is animating.
    svg_changed: RefCell<IdMap<Instant>>,
    /// Pictures to draw off the UI thread (an svg's layers to draw, and all it will show),
    /// queued while building; `svg_drawing` are underway.
    svg_jobs: RefCell<Vec<(u64, Vec<String>, Vec<String>)>>,
    svg_drawing: RefCell<IdSet>,
    /// A big drawing's top-level parts: a hash of each one's markup and when it last changed.
    svg_parts: RefCell<IdMap<(u64, Option<Instant>)>>,
    frame: Cell<u64>,
    /// resvg's font database is loaded, so SVGs with text can be drawn without blocking.
    svg_fonts_ready: bool,
    scale: Cell<f32>,
    /// When each element's current `@keyframes` animation started, by animation name.
    animation_clocks: RefCell<IdMap<(String, Instant)>>,
    /// Newly shown overlays JS positions from their size (floating-ui popovers): kept invisible
    /// until that size reached JS and JS answered, as a browser lays out before it paints.
    /// Value: when first built, and whether it may show.
    reveal: RefCell<IdMap<(Instant, bool)>>,
    /// Fixed boxes with fixed ones inside, as of the last build (see `fixed_layers`).
    fixed_wrappers: RefCell<IdSet>,
    /// Bounds last reported to JS per measured node.
    measured: Rc<RefCell<IdMap<Bounds<Pixels>>>>,
    /// Scroll containers' offsets, which JS sets (`scrollTop`) and hears change (`scroll`), with
    /// the offset last reported.
    scrolls: Rc<RefCell<IdMap<(ScrollHandle, gpui::Point<Pixels>)>>>,
    /// Those changed since the last sync, as JS sees them (rect, padding, border): the answer
    /// carries them. JS has the others already (each change is also sent as a `resize` event).
    rects: Rc<RefCell<IdMap<serde_json::Value>>>,
    /// JS is blocked until the next layout's rects come through `sync_pipe`.
    sync_pending: Rc<Cell<bool>>,
    sync_pipe: Rc<RefCell<std::fs::File>>,
    /// Query containers' laid-out sizes, which `@container` rules read.
    container_sizes: Rc<RefCell<IdMap<(f32, f32)>>>,
    /// Transformed ancestors of fixed boxes, their containing blocks (see `fixed_containing_block`),
    /// with where each was drawn last.
    containing_blocks: Rc<RefCell<IdMap<Option<Bounds<Pixels>>>>>,
    /// Containers whose width changed at the last layout; their subtrees restyle.
    resized_containers: Rc<RefCell<Vec<u64>>>,
    /// The `<select>` whose option list is showing.
    open_select: Option<u64>,
    /// Which of the current press's events already went to JS.
    reported: Rc<Reported>,
    /// Focus JS asked for, applied at the next render where the window is at hand.
    pending_focus: Option<Option<u64>>,
    /// The SVG shape under the pointer and its ancestors inside the drawing: their `:hover` paints apply.
    svg_hover: Vec<u64>,
    /// A non-input element JS focused; it shows `:focus` styles until focus moves.
    js_focused: Option<u64>,
    /// Editable paragraphs as laid out in the last frame (see `editing`).
    editables: RefCell<Vec<editing::Para>>,
    /// The selection JS last sent, which the editable paragraphs draw.
    edit: editing::Selection,
    /// gpui's focus while an editing host has it, so typed and composed text comes to `Root`.
    edit_focus: FocusHandle,
    /// The IME's text being composed, as a UTF-16 range in the caret's text node.
    marked: Option<Range<usize>>,
    /// Whether JS was last told the window is visible (`document.visibilityState`).
    visible: bool,
    /// Input-to-pixels benchmarking, when a driver is attached (see `bench`).
    bench: Option<BenchState>,
    /// Long lists' off-screen children, measured and left out of the build (see `cull`).
    cull: Rc<RefCell<Cull>>,
    /// Elements gpui reports hovered: they stay built until it reports them left.
    hovered: Rc<RefCell<IdSet>>,
    /// Elements mid-transition or animation in the last build: they stay built, as their size may change.
    animated: RefCell<IdSet>,
    /// What may not be culled this build: the above and focus, with their ancestors.
    keep: IdSet,
    /// The list being built and the child before the one being built in it, for that child's probe.
    in_list: Cell<Option<(u64, u64)>>,
}

impl Root {
    pub fn new(
        stream: UnixStream,
        sync_pipe: std::fs::File,
        mut rx: UnboundedReceiver<Msg>,
        window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let bridge = Bridge::new(stream);
        native::init(&bridge, window, cx);
        let shell = bridge.clone();
        cx.spawn(async move |this, cx| {
            let mut queued = None;
            loop {
                let msg = match queued.take() {
                    Some(msg) => msg,
                    None => match rx.next().await {
                        Some(msg) => msg,
                        None => break,
                    },
                };
                let msg = match msg {
                    Msg::Native(call) => {
                        let _ = cx.update(|cx| native::handle(call, window, &shell, cx));
                        continue;
                    }
                    msg => msg,
                };
                let sync = matches!(msg, Msg::Sync);
                let settled = matches!(msg, Msg::Settled);
                // JS's animation callbacks run as the display's next frame begins, as a browser
                // runs them, whether or not anything is redrawn then; what they change is drawn in
                // the frame after.
                if matches!(msg, Msg::Frame) {
                    let bridge = shell.clone();
                    let _ = cx.update_window(window, |_, window, _| {
                        window.on_next_frame(move |_, _| bridge.event(HTML, "frame", json!({})));
                    });
                    continue;
                }
                let alive = this.update(cx, |root, cx| {
                    if sync {
                        root.sync_pending.set(true);
                        // A window that draws no frames (hidden) still answers, with the rects it has.
                        cx.spawn(async |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(250))
                                .await;
                            let _ = this.update(cx, |root, _| root.answer_sync());
                        })
                        .detach();
                    }
                    // Between frames: `*` keeps it out of the next frame's total.
                    gpui::profile::time("apply*", || root.handle(msg));
                    if sync {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
                // JS waits for this layout: draw it now, not at the next display refresh (it is
                // presented then, as usual).
                if sync {
                    // Not shown until JS has used it (its next message, or 100ms): Radix Select
                    // places its list from this layout, which has the list where it mounted.
                    let _ = cx.update_window(window, |_, window, cx| {
                        window.draw_settled(cx).clear();
                        window.hold_present(Some(Instant::now() + Duration::from_millis(100)));
                    });
                    continue;
                }
                let _ = cx.update_window(window, |_, window, _| window.hold_present(None));
                // JS is done with an input: all it changed (and the host's own changes for it)
                // goes to the GPU now, not at the next refresh.
                if settled {
                    let _ = cx.update_window(window, |_, window, cx| window.draw_and_present_now(cx));
                    continue;
                }
                // JS often reads layout right after a commit. A display refresh drawing the commit
                // first would hold that read up by a frame, so it is drawn when the read comes, or
                // at the refresh after a moment without one.
                let wait = cx.background_executor().timer(Duration::from_millis(4));
                match futures::future::select(rx.next(), wait).await {
                    futures::future::Either::Left((next, _)) => match next {
                        Some(next) => queued = Some(next),
                        None => break,
                    },
                    futures::future::Either::Right(_) => {}
                }
                if !matches!(queued, Some(Msg::Sync)) {
                    let _ = this.update(cx, |_, cx| cx.notify());
                }
            }
            // The JS side went away; there is nothing left to drive the UI.
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
        let bench = bench::listen().map(|(mut commands, replies)| {
            gpui::profile::track_frames();
            let this = cx.weak_entity();
            cx.spawn(async move |_, cx| {
                while let Some(command) = commands.next().await {
                    let _ = cx.update_window(window, |_, window, cx| {
                        Root::bench_command(&this, command, window, cx)
                    });
                }
            })
            .detach();
            BenchState {
                replies,
                injected: HashMap::new(),
                handled: Vec::new(),
                last_commit: None,
                draws: Default::default(),
            }
        });
        bridge.send(text_metrics(cx));
        let keys = bridge.clone();
        // Every keystroke in the window, focused or not, as `document` sees keydown in a browser
        // (Escape to dismiss, shortcuts). Inputs that stop a key keep it to themselves.
        let _keystrokes = cx.observe_keystrokes(move |root: &mut Root, e, _, _| {
            root.reported.keyboard.set(true);
            let (k, m) = (&e.keystroke, &e.keystroke.modifiers);
            let key = dom_key(&k.key, k.key_char.as_deref());
            let mut init = json!({ "key": key, "metaKey": m.platform, "ctrlKey": m.control, "shiftKey": m.shift, "altKey": m.alt });
            // In editable content, where the keys that move by lines take the caret.
            if let Some((node, offset)) = root.caret_move(&key, m.platform) {
                init["caretTo"] = json!([node, offset]);
            }
            keys.event(HTML, "keydown", init);
        });
        Root {
            _keystrokes,
            this: cx.weak_entity(),
            tree: Tree::default(),
            sheet: Stylesheet::parse("").expect("UA stylesheet parses"),
            app_sheet: Stylesheet::parse("").expect("UA stylesheet parses"),
            injected: Vec::new(),
            styles: IdMap::default(),
            style_cache: StyleCache::default(),
            changes: Changes::default(),
            states: IdMap::default(),
            groups: IdMap::default(),
            group_of: IdMap::default(),
            transitioning: IdSet::default(),
            garbage: Vec::new(),
            animating: Cell::new(false),
            dirty: true,
            env: None,
            bridge,
            font_ch: RefCell::default(),
            inputs: IdMap::default(),
            canvases: IdMap::default(),
            stale_images: Vec::new(),
            media: IdMap::default(),
            svgs: RefCell::default(),
            rasterize: {
                let renderer = cx.svg_renderer();
                // The first SVG with text loads the system font database (~0.5 s). That happens
                // off the UI thread; SVGs with text wait for it (see `svg`) and then redraw.
                let warm = renderer.clone();
                let loading = cx.background_executor().spawn(async move {
                    let text = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text>a</text></svg>"#;
                    let _ = Image::from_bytes(ImageFormat::Svg, text.to_vec()).to_image_data(warm);
                });
                cx.spawn(async move |this, cx| {
                    loading.await;
                    let _ = this.update(cx, |root, cx| {
                        root.svg_fonts_ready = true;
                        cx.notify();
                    });
                })
                .detach();
                Arc::new(move |image| image.to_image_data(renderer.clone()))
            },
            svg_shown: RefCell::default(),
            svg_changed: RefCell::default(),
            svg_jobs: RefCell::default(),
            svg_drawing: RefCell::default(),
            svg_parts: RefCell::default(),
            frame: Cell::new(0),
            svg_fonts_ready: false,
            scale: Cell::new(1.0),
            measured: Rc::default(),
            scrolls: Rc::default(),
            rects: Rc::default(),
            sync_pending: Rc::default(),
            sync_pipe: Rc::new(RefCell::new(sync_pipe)),
            container_sizes: Rc::default(),
            containing_blocks: Rc::default(),
            resized_containers: Rc::default(),
            animation_clocks: RefCell::default(),
            reveal: RefCell::default(),
            fixed_wrappers: RefCell::default(),
            open_select: None,
            reported: Rc::default(),
            pending_focus: None,
            svg_hover: Vec::new(),
            js_focused: None,
            editables: RefCell::default(),
            edit: editing::Selection::default(),
            edit_focus: cx.focus_handle(),
            marked: None,
            visible: true,
            bench,
            cull: Rc::default(),
            hovered: Rc::default(),
            animated: RefCell::default(),
            keep: IdSet::default(),
            in_list: Cell::new(None),
        }
    }

    /// One benchmark driver command (see `bench`). Runs outside any entity update: injected
    /// input runs the window's listeners, which update the root.
    fn bench_command(
        this: &WeakEntity<Root>,
        command: bench::Command,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let Some(root) = this.upgrade() else {
            return;
        };
        let id = command.id;
        match command.what {
            bench::What::Query { selector, index } => root.update(cx, |root, _| {
                let query = json!({ "req": id, "selector": selector, "index": index });
                root.bridge.event(HTML, "benchQuery", query);
            }),
            bench::What::Input { input } => {
                // Hidden, it draws nothing (minimised, covered, the display asleep).
                if !window.is_window_visible() {
                    if let Some(bench) = &root.read(cx).bench {
                        bench
                            .replies
                            .send(json!({ "id": id, "error": "the gpuijs window is hidden" }));
                    }
                    return;
                }
                let focused = root
                    .read(cx)
                    .inputs
                    .values()
                    .find(|i| i.focus.is_focused(window))
                    .map(|i| i.entity.clone());
                let field = focused.is_some();
                let start = bench::inject(&input, focused, window, cx);
                // Typed into focused editable content as the platform's input handler would.
                if let (bench::Input::Key { text }, false) = (&input, field) {
                    root.update(cx, |root, cx| {
                        if root.edit.host.is_some() {
                            gpui::EntityInputHandler::replace_text_in_range(root, None, text, window, cx);
                        }
                    });
                }
                root.update(cx, |root, _| {
                    if let Some(bench) = &mut root.bench {
                        bench.injected.insert(id, start);
                    }
                    // Sent after the events the input caused: JS answers once it has handled them.
                    root.bridge.event(HTML, "benchSettle", json!({ "req": id }));
                });
            }
        }
    }

    /// The window was shown or hidden (minimised, covered, another Space). Hidden, it gets no
    /// frames, so CSS animations wait; JS hears it to pause `requestAnimationFrame`, as browsers
    /// do for background tabs.
    pub fn visibility_changed(&mut self, window: &Window) {
        let visible = window.is_window_visible();
        if visible != self.visible {
            self.visible = visible;
            self.bridge
                .event(HTML, "visibilitychange", json!({ "hidden": !visible }));
        }
    }

    /// Tells JS's `Window` what happened to the window (`focus`, `resize`...), with its new state.
    pub fn window_event(&self, name: &str, window: &Window) {
        self.bridge
            .event(HTML, "window", native::window_state(name, window));
    }

    /// Tells JS's `app` what happened (`activate`: the dock icon was clicked).
    pub fn app_event(&self, name: &str) {
        self.bridge.event(HTML, "app", json!({ "name": name }));
    }

    /// The cascade is the app's stylesheet followed by injected ones; a bad one is skipped, not fatal.
    fn restyle_sheets(&mut self) {
        let mut sheet = self.app_sheet.clone();
        for text in &self.injected {
            match sheet.with_injected(text) {
                Ok(next) => sheet = next,
                Err(e) => eprintln!("[gpuijs] injected <style> rejected: {e:#}"),
            }
        }
        self.sheet = sheet;
        self.style_cache = StyleCache::default();
        self.changes.all = true;
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Css { text } => match Stylesheet::parse(&text) {
                Ok(sheet) => {
                    self.app_sheet = sheet;
                    self.restyle_sheets();
                }
                Err(e) => eprintln!("[gpuijs] stylesheet rejected: {e:#}"),
            },
            Msg::Styles { sheets } => {
                // Libraries like Emotion append rules as new styles render: when only the end grew,
                // only what was added is parsed, after the rest as in source order.
                let added = appended(&self.injected, &sheets);
                self.injected = sheets;
                match added.map(|text| self.sheet.with_injected(&text)) {
                    Some(Ok(sheet)) => {
                        self.sheet = sheet;
                        self.style_cache = StyleCache::default();
                        self.changes.all = true;
                    }
                    _ => self.restyle_sheets(),
                }
            }
            Msg::Sync => {}
            Msg::Settled => self.bridge.settled(),
            Msg::Frame => unreachable!("handled before it reaches the root"),
            // The window opened with these; a second `new Window` throws in JS.
            Msg::Window { .. } => {}
            Msg::Native(_) => unreachable!("handled before it reaches the root"),
            // New pixels change no style or layout.
            Msg::Pixels { id, image } => {
                match self.tree.nodes.contains_key(&id) {
                    true => self.stale_images.extend(self.canvases.insert(id, image)),
                    false => self.stale_images.push(image),
                }
                return;
            }
            Msg::Selection(selection) => {
                if selection.caret.map(|c| c.0) != self.edit.caret.map(|c| c.0) {
                    self.marked = None;
                }
                self.edit = selection;
                return;
            }
            Msg::Media { id, op, value } => {
                let Some(Some(player)) = self.media.get_mut(&id) else {
                    return;
                };
                match op.as_str() {
                    "play" => player.play(),
                    "pause" => player.pause(),
                    "seek" => player.seek(value),
                    "volume" => player.set_volume(value),
                    "rate" => player.set_rate(value),
                    _ => warn_once(&format!("media call {op}")),
                }
                return;
            }
            Msg::Bench { req, rect } => {
                let Some(bench) = &mut self.bench else {
                    return;
                };
                match bench.injected.remove(&req) {
                    // JS has handled an injected input; its commits came before this. Its effects
                    // show from the first draw after the input and those commits.
                    Some(start) => {
                        let effects = bench.last_commit.filter(|&c| c > start).unwrap_or(start);
                        bench.handled.push((req, start, effects));
                    }
                    None => bench.replies.send(json!({ "id": req, "rect": rect })),
                }
            }
            Msg::Commit { ops } => {
                if let Some(bench) = &mut self.bench {
                    bench.last_commit = Some(Instant::now());
                }
                // A commit after an overlay's size was reported is JS's answer to it.
                let measured = self.measured.borrow();
                for (id, (first, ready)) in self.reveal.borrow_mut().iter_mut() {
                    if !*ready && measured.contains_key(id) {
                        *ready = true;
                        if first.elapsed() >= Duration::from_millis(100) {
                            continue; // Already shown by the timeout; its animation is underway.
                        }
                        // Its enter animation plays from now, when it's actually seen.
                        let mut clocks = self.animation_clocks.borrow_mut();
                        let mut stack = vec![*id];
                        while let Some(n) = stack.pop() {
                            clocks.remove(&n);
                            stack.extend(self.tree.get(n).children.iter().copied());
                        }
                    }
                }
                drop(measured);
                let mut cull = self.cull.borrow_mut();
                // What a commit changes must be laid out again before it can be culled.
                let mut touched = Vec::new();
                for op in ops {
                    // A node moved among its siblings keeps its ancestors, so its style and size.
                    let moved_from = match &op {
                        Op::Append(_, child) | Op::Insert(_, child, _) => {
                            self.tree.nodes.get(child).and_then(|n| n.parent)
                        }
                        _ => None,
                    };
                    match &op {
                        Op::Set(id, _) | Op::Settext(id, _) => touched.push(*id),
                        Op::Append(parent, child) | Op::Insert(parent, child, _) => {
                            touched.push(*parent);
                            touched.extend(moved_from);
                            if moved_from != Some(*parent) {
                                cull.kids.remove(child);
                            }
                        }
                        Op::Remove(parent, _) | Op::Detach(parent, _) => touched.push(*parent),
                        _ => {}
                    }
                    match &op {
                        Op::Focus(target) => {
                            self.pending_focus = Some(*target);
                            continue;
                        }
                        Op::Scroll(id, x, y) => {
                            let mut scrolls = self.scrolls.borrow_mut();
                            let (handle, _) = scrolls.entry(*id).or_insert_with(unreported);
                            // gpui's offsets move the content: scrolled down is negative.
                            handle.set_offset(point(px(-(*x as f32)), px(-(*y as f32))));
                            continue;
                        }
                        Op::Hover(target) => {
                            self.svg_hover =
                                std::iter::successors(*target, |&n| self.tree.get(n).parent)
                                    .take_while(|&n| self.tree.get(n).tag != "svg")
                                    .collect();
                            continue;
                        }
                        Op::Set(id, _) => self.changes.props.push(*id),
                        Op::Append(parent, child) | Op::Insert(parent, child, _) => {
                            self.changes.children.push(*parent);
                            if moved_from != Some(*parent) {
                                self.changes.attached.push(*child);
                                // The parent it left lost a child (`:last-child` there may move).
                                self.changes.children.extend(moved_from);
                            }
                        }
                        // A removed subtree is freed, and the tree reports it (`take_freed`).
                        Op::Remove(parent, _) => self.changes.children.push(*parent),
                        Op::Detach(parent, child) => {
                            self.changes.children.push(*parent);
                            let mut stack = vec![*child];
                            while let Some(n) = stack.pop() {
                                if let Some(node) = self.tree.nodes.get(&n) {
                                    self.changes.gone.push(n);
                                    stack.extend(&node.children);
                                }
                            }
                        }
                        Op::El(id, tag, _) if matches!(tag.as_str(), "video" | "audio") => {
                            self.media.insert(*id, None);
                        }
                        // New nodes are styled when attached; text needs no restyle.
                        Op::El(..) | Op::Txt(..) | Op::Settext(..) => {}
                    }
                    self.tree.apply_deferred(op);
                }
                self.tree.settle();
                let freed = self.tree.take_freed();
                for id in &freed {
                    self.scrolls.borrow_mut().remove(id);
                    self.svg_shown.get_mut().remove(id);
                    self.svg_parts.get_mut().remove(id);
                    self.svg_changed.get_mut().remove(id);
                    self.media.remove(id);
                    self.stale_images.extend(self.canvases.remove(id));
                }
                self.changes.gone.extend(freed);
                cull.invalidate(&self.tree, touched, &|n| self.cullable(n));
                drop(cull);
                self.sync_media();
            }
        }
        self.dirty = true;
    }

    /// Gives each `<video>` and `<audio>` a player for its source (its `src`, else its first
    /// `<source>`'s) and its `loop` and `muted` attributes; `autoplay` starts a new one.
    fn sync_media(&mut self) {
        let tree = &self.tree;
        for (&id, slot) in &mut self.media {
            let node = tree.get(id);
            let source = node.children.iter().map(|&c| tree.get(c)).find(|c| c.tag == "source" && c.props.src.is_some());
            let src = node.props.src.as_ref().or(source.and_then(|s| s.props.src.as_ref()));
            if slot.as_ref().map(|p| &p.src) != src {
                *slot = src.and_then(|src| Player::open(src, node.tag == "video"));
                if let Some(player) = slot.as_mut().filter(|_| node.props.attrs.contains_key("autoplay")) {
                    player.play();
                    self.bridge.event(id, "media", json!({ "event": "play" }));
                }
            }
            if let Some(player) = slot {
                player.looping = node.props.attrs.contains_key("loop");
                player.set_muted(node.props.attrs.contains_key("muted"));
            }
        }
    }

    /// Reports what media did since the last render to JS, and keeps frames coming while any
    /// plays or loads.
    // shortcut: a playing video rebuilds the whole page each frame; give it its own view if pages get slow.
    fn poll_media(&mut self, window: &mut Window) {
        let mut busy = false;
        for (&id, player) in self.media.iter_mut().filter_map(|(id, p)| Some((id, p.as_mut()?))) {
            let mut events = Vec::new();
            busy |= player.poll(&mut events);
            for event in events {
                self.bridge.event(id, "media", event);
            }
        }
        if busy {
            window.request_animation_frame();
        }
    }

    /// Plays or pauses from the media controls, as JS's `play()`/`pause()` would.
    fn toggle_media(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(Some(player)) = self.media.get_mut(&id) else {
            return;
        };
        let event = match player.playing() {
            true => {
                player.pause();
                "pause"
            }
            false => {
                player.play();
                "play"
            }
        };
        self.bridge.event(id, "media", json!({ "event": event }));
        cx.notify();
    }

    /// Keeps one native input entity per `<input>` node, pushing new props into it.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tree = &self.tree;
        self.inputs.retain(|id, _| tree.nodes.contains_key(id));
        self.states.retain(|id, _| tree.nodes.contains_key(id));
        self.animation_clocks
            .borrow_mut()
            .retain(|id, _| tree.nodes.contains_key(id));
        self.reveal
            .borrow_mut()
            .retain(|id, _| tree.nodes.contains_key(id));
        self.measured
            .borrow_mut()
            .retain(|id, _| tree.nodes.contains_key(id));
        self.hovered
            .borrow_mut()
            .retain(|id| tree.nodes.contains_key(id));
        // Inputs there were, and new ones: those among the nodes attached since the last restyle.
        let mut ids: Vec<u64> = self.inputs.keys().copied().collect();
        let mut stack = self.changes.attached.clone();
        while let Some(n) = stack.pop() {
            let Some(node) = tree.nodes.get(&n) else {
                continue;
            };
            // Checkboxes and radios are boxes; JS toggles them on click.
            let checkable = matches!(
                node.props.attrs.get("type").map(String::as_str),
                Some("checkbox" | "radio" | "range")
            );
            if matches!(node.tag.as_str(), "input" | "textarea")
                && !checkable
                && !self.inputs.contains_key(&n)
            {
                ids.push(n);
            }
            stack.extend(&node.children);
        }
        for id in ids {
            let props = tree.get(id).props.clone();
            match self.inputs.get(&id) {
                Some(input) => input
                    .entity
                    .update(cx, |input, cx| input.set_props(props, cx)),
                None => {
                    let bridge = self.bridge.clone();
                    let multiline = tree.get(id).tag == "textarea";
                    let entity = cx.new(|cx| TextInput::new(id, props, multiline, bridge, cx));
                    let (focus, inert) = (entity.read(cx).focus_handle(cx), entity.read(cx).inert.clone());
                    let placeholder_color = entity.read(cx).placeholder_color.clone();
                    let grows = entity.read(cx).grows.clone();
                    // JS learns of focus changes too, for onFocus/onBlur and document.activeElement.
                    let _subscriptions = [
                        cx.on_focus(&focus, window, move |root, _, cx| {
                            root.bridge.event(id, "focus", json!({}));
                            root.set_state(id, |s| s.focused = true, cx)
                        }),
                        cx.on_blur(&focus, window, move |root, _, cx| {
                            root.bridge.event(id, "blur", json!({}));
                            root.set_state(id, |s| s.focused = false, cx)
                        }),
                    ];
                    self.inputs.insert(
                        id,
                        Input {
                            entity,
                            focus,
                            inert,
                            placeholder_color,
                            grows,
                            _subscriptions,
                        },
                    );
                }
            }
        }
    }

    /// The element for a node in page flow; `position: fixed` boxes are built by `fixed_layers`.
    fn build(&self, id: u64) -> Option<AnyElement> {
        let c = self.styles.get(&id)?;
        if c.fixed {
            return None;
        }
        let el = self.build_box(id)?;
        // A positioned box with a positive z-index paints over the page after it (a header's
        // dropdown over the cards below), keeping its place in layout and its ancestors' clip.
        // shortcut: positioned boxes without one still paint in tree order; lift them too if a page needs it.
        Some(match c.z_index {
            Some(z) if z > 0 && c.base.position.is_some() && !self.in_stacking_context(id) => gpui::deferred(el)
                .with_priority(z_priority(z))
                .clipped()
                .into_any_element(),
            _ => el,
        })
    }

    /// Whether an ancestor groups its descendants' painting, as a CSS stacking context: a fixed
    /// box, a positioned one with a z-index, or one see-through, transformed or animating. A
    /// z-index inside one stacks within it, so the box isn't lifted out of it (a menu's sticky
    /// subheaders fade and grow in with the menu). Ancestors are built first, so `animated`
    /// already holds those animating in this frame.
    fn in_stacking_context(&self, id: u64) -> bool {
        let animated = self.animated.borrow();
        std::iter::successors(self.tree.get(id).parent, |&a| self.tree.get(a).parent).any(|a| {
            self.styles.get(&a).is_some_and(|s| {
                let b = &s.base;
                s.fixed
                    || (s.z_index.is_some() && b.position.is_some())
                    || b.opacity.is_some_and(|o| o < 1.0)
                    || b.rotate.is_some()
                    || b.scale.is_some()
                    || b.skew.is_some()
                    || b.translate.is_some()
                    || animated.contains(&a)
            })
        })
    }

    fn build_box(&self, id: u64) -> Option<AnyElement> {
        let node = self.tree.get(id);
        let c = self.styles.get(&id)?;
        if c.disp == Disp::None {
            return None;
        }
        if let Some(input) = self.inputs.get(&id) {
            input.inert.set(c.no_pointer);
            input.placeholder_color.set(c.placeholder);
            input.grows.set(c.field_grows);
            let el = self.boxed(id, c).child(input.entity.clone());
            if c.no_pointer {
                return Some(el.into_any_element());
            }
            // Clicking anywhere in the box focuses the field, and shows the focus style on this press
            // rather than after gpui reports the focus change at the end of the next frame.
            let (focus, this) = (input.focus.clone(), self.this.clone());
            let el = el.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus);
                let _ = this.update(cx, |root, cx| root.set_state(id, |s| s.focused = true, cx));
                // The root would otherwise treat this as a click outside and blur.
                cx.stop_propagation();
            });
            return Some(el.into_any_element());
        }
        if node.tag == "select" {
            return Some(self.select(id, c));
        }
        if node.tag == "input" && node.props.attrs.get("type").is_some_and(|t| t == "range") {
            return Some(self.range(id, c));
        }
        if matches!(node.tag.as_str(), "progress" | "meter") {
            return Some(self.gauge(id, c));
        }
        if node.tag == "canvas" {
            return Some(self.canvas(id, c));
        }
        if matches!(node.tag.as_str(), "video" | "audio") {
            return Some(self.media_element(id, c));
        }
        if node.tag == "svg" {
            return Some(self.svg(id, c));
        }
        if node.tag == "table" || c.table.role == Some(TableRole::Table) {
            return Some(self.table(id, c));
        }
        if node.tag == "img" {
            let Some(src) = &node.props.src else {
                return None;
            };
            // shortcut: local file paths and data: URLs only; URLs need an http client registered with gpui.
            let source = data_image(src).map_or_else(|| PathBuf::from(src).into(), ImageSource::from);
            let mut image = img(source)
                .object_fit(c.image.fit)
                .object_position(point(c.image.at.0, c.image.at.1));
            *image.style() = c.base.clone();
            // gpui clips overflow to rectangles, so an image takes the rounded corners of the box
            // clipping it (an avatar's `overflow-hidden rounded-full`) unless it has its own.
            let own = &image.style().corner_radii;
            let unrounded = [
                own.top_left,
                own.top_right,
                own.bottom_right,
                own.bottom_left,
            ]
            .iter()
            .all(Option::is_none);
            if unrounded
                && let Some(parent) = node.parent.and_then(|p| self.styles.get(&p))
                && parent.base.overflow.x == Some(gpui::Overflow::Hidden)
            {
                image.style().corner_radii = parent.base.corner_radii.clone();
            }
            return Some(image.into_any_element());
        }
        let mut el = self.boxed(id, c);
        // Sticky boxes aren't offset in flow; `Sticky` moves them while scrolled.
        let sticky_top = c.sticky.then(|| el.style().inset.top.take()).flatten();
        let el = match self.marker(id) {
            // An inside marker leads the item's content, which keeps to its right.
            // shortcut: wrapped lines hang under the content, not back under the marker.
            Some((marker, true)) => {
                el.style().display = Some(gpui::Display::Flex);
                let content = self.children(div().id(("list-item", id)).flex_1().min_w_0(), id, c.disp);
                el.items_baseline()
                    .child(div().flex_none().whitespace_nowrap().child(marker))
                    .child(content)
            }
            _ => self.children(el, id, c.disp),
        };
        let el = el.into_any_element();
        Some(match sticky_top {
            Some(Length::Definite(DefiniteLength::Absolute(top))) => {
                Sticky::new(el, top.to_pixels(px(REM))).into_any_element()
            }
            // shortcut: only `top` sticks; percent, `bottom`, `left` and `right` are ignored.
            _ => el,
        })
    }

    /// `position: fixed` boxes, lifted out of the scrolling page into layers that fill the window
    /// (their containing block). Each layer carries the text style the box inherits where it sits.
    /// Unblocks JS waiting on a sync with every rect measured so far, once per request.
    fn answer_sync(&self) {
        if !self.sync_pending.replace(false) {
            return;
        }
        let reply = json!({ "rects": std::mem::take(&mut *self.rects.borrow_mut()) });
        if let Err(e) = writeln!(self.sync_pipe.borrow_mut(), "{reply}") {
            eprintln!("[gpuijs] failed to answer a layout sync: {e}");
        }
    }

    /// Takes freed nodes' styles out of `styles`, to be dropped later (see `drop_garbage`). When
    /// most go (a cleared list), the rest move to a fresh map instead: a `Computed` is over a
    /// kilobyte, and moving tens of thousands out one by one took longer than the restyle.
    fn remove_freed_styles(&mut self, gone: &[u64]) {
        let tree = &self.tree;
        let freed = |id: u64| !tree.nodes.contains_key(&(id & (PSEUDO_TEXT - 1)));
        if gone.len() * 2 > self.styles.len() {
            let mut old = std::mem::take(&mut self.styles);
            let kept: Vec<u64> = old.keys().copied().filter(|&id| !freed(id)).collect();
            self.styles.reserve(old.capacity());
            for id in kept {
                self.styles.insert(id, old.remove(&id).unwrap());
            }
            self.garbage.push(Box::new(old));
            return;
        }
        let mut removed = Vec::new();
        for &id in gone.iter().filter(|&&id| freed(id)) {
            for key in [
                id,
                pseudo_id(id, Pseudo::Before),
                pseudo_id(id, Pseudo::After),
            ] {
                removed.extend(self.styles.remove(&key));
            }
        }
        self.garbage.push(Box::new(removed));
    }

    /// Frees what removed nodes left; called once JS no longer waits on this draw.
    fn drop_garbage(&mut self) {
        drop(self.tree.take_garbage());
        self.garbage.clear();
    }

    fn fixed_layers(&self) -> Vec<AnyElement> {
        let fixed = |id: u64| {
            let c = self.styles.get(&id)?;
            (c.disp != Disp::None).then_some(c.fixed)
        };
        let z_of = |id: u64| self.styles.get(&id).and_then(|c| c.z_index);
        let children_of = |id: u64| self.tree.get(id).children.clone();
        let mut order = Vec::new();
        fixed_order(HTML, None, &fixed, &z_of, &children_of, &mut order);
        let ids: IdSet = order.iter().map(|&(id, _)| id).collect();
        *self.fixed_wrappers.borrow_mut() = order
            .iter()
            .filter_map(|&(id, _)| {
                std::iter::successors(self.tree.get(id).parent, |&a| self.tree.get(a).parent)
                    .find(|a| ids.contains(a))
            })
            .collect();
        order
            .into_iter()
            .filter_map(|(id, z)| {
                let el = self.build_box(id)?;
                let node = self.tree.get(id);
                // A modal dialog is in the top layer: over everything, against the window.
                let modal = node.tag == "dialog" && node.props.modal;
                let mut text = TextStyleRefinement::default();
                let ancestors: Vec<u64> =
                    std::iter::successors(self.tree.get(id).parent, |&a| self.tree.get(a).parent)
                        .collect();
                for a in ancestors.into_iter().rev() {
                    if let Some(t) = self.styles.get(&a).and_then(|c| c.base.text.as_ref()) {
                        text.refine(t);
                    }
                }
                let mut layer = div().absolute().top_0().left_0().size_full();
                // Inside a transformed ancestor it is placed against that box, not the window.
                if !modal
                    && let Some(Some(b)) = self
                    .fixed_containing_block(id)
                    .and_then(|a| self.containing_blocks.borrow().get(&a).copied())
                {
                    let s = layer.style();
                    s.inset.left = Some(b.origin.x.into());
                    s.inset.top = Some(b.origin.y.into());
                    s.size.width = Some(b.size.width.into());
                    s.size.height = Some(b.size.height.into());
                }
                if modal {
                    layer = layer.child(self.backdrop(id));
                }
                let mut layer = layer.child(el);
                layer.style().text = Some(text);
                // Higher z-index paints later, equal ones in `fixed_order` (gpui's sort is stable),
                // among the page's raised boxes too (a z-99999 modal over a z-99999 header).
                let priority = match modal {
                    true => TOP_LAYER,
                    false => z_priority(z),
                };
                Some(gpui::deferred(layer).with_priority(priority).into_any_element())
            })
            .collect()
    }

    /// What a modal dialog covers the page with: its `::backdrop`'s look over the whole window,
    /// taking the pointer from the page. A click on it is the dialog's, as in a browser ("click
    /// outside to close" compares `event.target` with the dialog).
    fn backdrop(&self, dialog: u64) -> Stateful<Div> {
        let mut el = div().id(("backdrop", dialog)).occlude();
        if let Some(c) = self.styles.get(&dialog).and_then(|c| c.backdrop_box.as_ref()) {
            *el.style() = c.base.clone();
        }
        let el = el.absolute().top_0().left_0().size_full();
        if !self.tree.get(dialog).props.listens("click") {
            return el;
        }
        let bridge = self.bridge.clone();
        el.on_click(move |_, window, _| bridge.event(dialog, "click", at(window)))
    }

    /// A fixed box's containing block when it isn't the window: its nearest transformed ancestor, as
    /// in CSS (MUI X puts a sparkline's tooltip in the data grid's translated rows).
    /// shortcut: `filter`, `perspective`, `will-change` and `contain` make none here.
    fn fixed_containing_block(&self, id: u64) -> Option<u64> {
        std::iter::successors(self.tree.get(id).parent, |&a| self.tree.get(a).parent).find(|a| {
            self.styles.get(a).is_some_and(|c| {
                let b = &c.base;
                b.translate.is_some() || b.rotate.is_some() || b.scale.is_some() || b.skew.is_some()
            })
        })
    }

    /// A `<progress>` or `<meter>`: its track filled to its value, in Chrome's colors; the fallback
    /// content inside isn't shown. An indeterminate progress shows the empty track.
    fn gauge(&self, id: u64, c: &Computed) -> AnyElement {
        let node = self.tree.get(id);
        let num = |v: Option<&String>| v.and_then(|v| v.trim().parse::<f32>().ok());
        let attr = |k: &str| num(node.props.attrs.get(k));
        let meter = node.tag == "meter";
        let min = if meter { attr("min").unwrap_or(0.0) } else { 0.0 };
        let max = attr("max").filter(|m| *m > min).unwrap_or(min + 1.0);
        let value = num(node.props.value.as_ref()).map(|v| v.clamp(min, max));
        let el = self.boxed(id, c);
        let Some(value) = value else {
            return el.child(gauge_track(if meter { 0.3 } else { 0.4 })).into_any_element();
        };
        let color = match meter {
            false => 0x0075ff,
            // The spec's regions: optimum, suboptimum, and even less good.
            true => {
                let low = attr("low").map_or(min, |l| l.clamp(min, max));
                let high = attr("high").map_or(max, |h| h.clamp(low, max));
                let optimum = attr("optimum").map_or((min + max) / 2.0, |o| o.clamp(min, max));
                let region = |v: f32| if v < low { 0u8 } else if v > high { 2 } else { 1 };
                match region(optimum).abs_diff(region(value)) {
                    0 => 0x107c10,
                    1 => 0xffb900,
                    _ => 0xd83b01,
                }
            }
        };
        let bar = div().h_full().w(gpui::relative((value - min) / (max - min))).bg(gpui::rgb(color));
        el.child(gauge_track(if meter { 0.3 } else { 0.4 }).child(bar)).into_any_element()
    }

    /// A `<canvas>`: the pixels JS last drew, stretched over its box (`object-fit` applies), which
    /// is its `width` × `height` attributes in CSS px unless styled.
    fn canvas(&self, id: u64, c: &Computed) -> AnyElement {
        let attrs = &self.tree.get(id).props.attrs;
        let attr = |k: &str, default| attrs.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        let mut el = self.boxed(id, c);
        natural_size(el.style(), (attr("width", 300.0), attr("height", 150.0)));
        match self.canvases.get(&id) {
            Some(image) => {
                let pixels = img(ImageSource::Render(image.clone())).object_fit(c.image.fit).size_full();
                el.child(pixels).into_any_element()
            }
            None => el.into_any_element(),
        }
    }

    /// A `<video>`: its current frame, letterboxed in its box as browsers draw video, with a
    /// control bar over it under `controls`. An `<audio>` shows only the bar, and only then.
    fn media_element(&self, id: u64, c: &Computed) -> AnyElement {
        let node = self.tree.get(id);
        let video = node.tag == "video";
        let player = self.media.get(&id).and_then(Option::as_ref);
        let attr = |k: &str| node.props.attrs.get(k).and_then(|v| v.trim().parse::<f32>().ok());
        // Chrome's sizes: a video's own (300×150 until it is known), its audio bar's.
        let size = match video {
            true => player.and_then(Player::natural_size).unwrap_or((300.0, 150.0)),
            false => (300.0, 54.0),
        };
        // One attribute alone sizes the other side at the video's ratio, as in a browser.
        let size = match (attr("width"), attr("height")) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * size.1 / size.0),
            (None, Some(h)) => (h * size.0 / size.1, h),
            (None, None) => size,
        };
        let mut el = self.boxed(id, c);
        natural_size(el.style(), size);
        if let Some(frame) = player.and_then(|p| p.frame.clone()) {
            // CSS's initial `fill` would stretch it; a video keeps its shape unless styled to.
            let fit = match c.image.fit {
                gpui::ObjectFit::Fill => gpui::ObjectFit::Contain,
                fit => fit,
            };
            el = el.child(gpui::surface(frame).object_fit(fit).size_full());
        }
        if node.props.attrs.contains_key("controls") {
            el = el.child(self.media_controls(id, player, video));
        }
        el.into_any_element()
    }

    /// Play/pause, the time, and how far it has played, in white over a video's foot or on
    /// Chrome's grey audio bar.
    // shortcut: no seeking, volume or fullscreen; add them when an app relies on native controls.
    fn media_controls(&self, id: u64, player: Option<&Player>, video: bool) -> Div {
        let playing = player.is_some_and(Player::playing);
        let (now, total) = player.map_or((0.0, f64::NAN), |p| (p.current_time(), p.duration()));
        let played = if total > 0.0 { (now / total).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let (ink, track) = match video {
            true => (gpui::rgb(0xffffff), gpui::rgba(0xffffff66)),
            false => (gpui::rgb(0x000000), gpui::rgba(0x0000004d)),
        };
        let this = self.this.clone();
        let toggle = div()
            .id(("media-toggle", id))
            .cursor_pointer()
            .child(if playing { "❚❚" } else { "▶" })
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |root, cx| root.toggle_media(id, cx));
            });
        let bar = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(14.0))
            .text_size(px(13.0))
            .text_color(ink)
            .child(toggle)
            .child(format!("{} / {}", media::clock(now), media::clock(total)))
            .child(div().flex_1().h(px(4.0)).rounded_full().bg(track).child(div().h_full().rounded_full().w(gpui::relative(played)).bg(ink)));
        match video {
            true => bar.absolute().left_0().right_0().bottom_0().h(px(36.0)).bg(gpui::rgba(0x00000080)),
            false => bar.size_full().rounded_full().bg(gpui::rgb(0xf1f3f4)),
        }
    }

    /// An `<input type=range>`: its track filled up to the thumb, in Chrome's colors. Pressing or
    /// dragging along it sets the value, reported as `change` (React's `onChange` and `onInput`).
    fn range(&self, id: u64, c: &Computed) -> AnyElement {
        let node = self.tree.get(id);
        let num = |k: &str| node.props.attrs.get(k).and_then(|v| v.trim().parse::<f32>().ok());
        let (min, max) = (num("min").unwrap_or(0.0), num("max").unwrap_or(100.0));
        let max = if max > min { max } else { min + 1.0 };
        let step = num("step").filter(|s| *s > 0.0).unwrap_or(1.0);
        let value = node.props.value.as_deref().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or((min + max) / 2.0);
        let at = ((value - min) / (max - min)).clamp(0.0, 1.0);
        let blue = gpui::rgb(0x0075ff);
        let thumb = px(14.5);
        let track = gauge_track(0.47).child(div().h_full().w(gpui::relative(at)).bg(blue));
        // The thumb's center travels the track less its own width.
        let knob = div()
            .absolute()
            .top(gpui::relative(0.5))
            .mt(-thumb / 2.0)
            .left(gpui::relative(at))
            .ml(-thumb * at)
            .size(thumb)
            .rounded_full()
            .bg(blue);
        let mut el = self.boxed(id, c).child(track).child(knob);
        if node.props.disabled {
            return el.into_any_element();
        }
        let set = move |bridge: &Bridge, bounds: Bounds<Pixels>, x: Pixels, last: &Cell<f32>| {
            let f = ((x - bounds.left() - thumb / 2.0) / (bounds.size.width - thumb).max(px(1.0))).clamp(0.0, 1.0);
            let v = (min + ((f * (max - min)) / step).round() * step).clamp(min, max);
            if v != last.replace(v) {
                bridge.event(id, "change", json!({ "value": format!("{v}") }));
            }
        };
        let (bridge, last) = (self.bridge.clone(), Rc::new(Cell::new(value)));
        let bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
        let dragging = Rc::new(Cell::new(false));
        let measure = bounds.clone();
        el = el
            .child(gpui::canvas(move |b, _, _| measure.set(Some(b)), |_, _, _, _| {}).absolute().size_full())
            .on_mouse_down(MouseButton::Left, {
                let (bridge, last, bounds, dragging) = (bridge.clone(), last.clone(), bounds.clone(), dragging.clone());
                move |e, _, _| {
                    if let Some(b) = bounds.get() {
                        dragging.set(true);
                        set(&bridge, b, e.position.x, &last);
                    }
                }
            })
            .on_mouse_move({
                let (bounds, dragging) = (bounds.clone(), dragging.clone());
                move |e, _, _| match e.pressed_button {
                    Some(MouseButton::Left) if dragging.get() => {
                        if let Some(b) = bounds.get() {
                            set(&bridge, b, e.position.x, &last);
                        }
                    }
                    _ => dragging.set(false),
                }
            });
        el.into_any_element()
    }

    /// A native `<select>`: the box shows the chosen option; a click opens the list as an overlay
    /// and picking one reports `change`, like the browser's own popup.
    fn select(&self, id: u64, c: &Computed) -> AnyElement {
        let node = self.tree.get(id);
        let options: Vec<(String, String)> = node
            .children
            .iter()
            .flat_map(|&ch| match self.tree.get(ch).tag.as_str() {
                "optgroup" => self.tree.get(ch).children.clone(),
                _ => vec![ch],
            })
            .filter(|&o| self.tree.get(o).tag == "option")
            .map(|o| {
                let o = self.tree.get(o);
                let label: String = o
                    .children
                    .iter()
                    .filter_map(|&t| self.tree.get(t).text.clone())
                    .collect();
                let value = o.props.value.clone().unwrap_or_else(|| label.clone());
                (value, c.case.apply(&label).into_owned())
            })
            .collect();
        let chosen = options
            .iter()
            .find(|(v, _)| Some(v) == node.props.value.as_ref())
            .or(options.first())
            .map_or(String::new(), |(_, label)| label.clone());
        let this = self.this.clone();
        let mut el = self.boxed(id, c);
        // The UA sheet's chevron, unless `appearance: none` took it away.
        if let Some(layer) = bg_image(c) {
            el = el.child(layer);
        }
        let mut el = el.child(StyledText::new(chosen));
        // A select's own box lays out its label like a browser does: at the start, not as flex content.
        el.style().justify_content = Some(gpui::JustifyContent::FlexStart);
        if !node.props.disabled {
            // Tells the root this press is the select's own, so it doesn't close the list first.
            let reported = self.reported.clone();
            el = el.on_mouse_down(MouseButton::Left, move |_, _, _| {
                reported.on_select.set(true)
            });
            el = el.on_click(move |_, _, cx| {
                let _ = this.update(cx, |root, cx| {
                    root.open_select = (root.open_select != Some(id)).then_some(id);
                    cx.notify();
                });
            });
        }
        if self.open_select == Some(id) {
            let mut list = div()
                .id(("options", id))
                .flex()
                .flex_col()
                .min_w(px(120.0))
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(c.color.opacity(0.15))
                .bg(c.backdrop)
                .text_color(c.color)
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (value, label) in options {
                let (this, bridge) = (self.this.clone(), self.bridge.clone());
                let hover = c.color.opacity(0.08);
                list = list.child(
                    div()
                        .id(SharedString::from(format!("option-{id}-{value}")))
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .hover(move |s| s.bg(hover))
                        .child(label)
                        .on_click(move |_, _, cx| {
                            bridge.event(id, "change", json!({ "value": value }));
                            let _ = this.update(cx, |root, cx| {
                                root.open_select = None;
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                );
            }
            // Anchored at the select's bottom-left corner, drawn above everything, kept inside the window.
            let popup = gpui::deferred(gpui::anchored().child(div().mt_1().child(list)))
                .with_priority(usize::MAX);
            el = el.child(div().absolute().left_0().top_full().child(popup));
        }
        el.into_any_element()
    }

    /// A `<table>` as a grid with one track per column: `auto` sizes columns by content like table
    /// layout does, and a cell's percent or px width sizes its whole column (the `max-w-0 w-2/3`
    /// truncating-column trick included). Rows have no box of their
    /// own: their border and background go on each cell.
    /// shortcut: no rowspan, and row hover styles don't apply; revisit if a table needs them.
    fn table(&self, id: u64, c: &Computed) -> AnyElement {
        use gpui::{GridLocation, GridPlacement, GridTrack};
        let shown = |id: &u64| self.styles.get(id).is_some_and(|c| c.disp != Disp::None);
        let kids = |id: u64| self.tree.get(id).children.iter().copied().filter(shown);
        // By tag, or by `display` (a `display: table` div's `table-row`s).
        let role = |id: u64| match self.tree.get(id).tag.as_str() {
            "tr" => Some(TableRole::Row),
            "thead" | "tbody" | "tfoot" => Some(TableRole::RowGroup),
            "td" | "th" => Some(TableRole::Cell),
            _ => self.styles.get(&id).and_then(|c| c.table.role),
        };
        let mut rows: Vec<u64> = kids(id)
            .flat_map(|ch| match role(ch) {
                Some(TableRole::Row) => vec![ch],
                Some(TableRole::RowGroup) => kids(ch).filter(|&r| role(r) == Some(TableRole::Row)).collect(),
                _ => vec![],
            })
            .collect();
        // Cells right in the table make one row, as CSS wraps them in an anonymous one.
        // shortcut: only when the table has no rows of its own.
        if rows.is_empty() && kids(id).any(|ch| role(ch) == Some(TableRole::Cell)) {
            rows.push(id);
        }
        let attr = |cell: u64, name: &str| {
            let attrs = &self.tree.get(cell).props.attrs;
            attrs.get(name).and_then(|n| n.trim().parse().ok()).unwrap_or(1u16).max(1)
        };
        // Each cell's row and column, past the cells spanning down from rows above.
        let mut taken: Vec<Vec<bool>> = vec![Vec::new(); rows.len()];
        let mut cells = Vec::new();
        for (r, &row) in rows.iter().enumerate() {
            let mut col = 0;
            for cell in kids(row) {
                while taken[r].get(col).copied().unwrap_or(false) {
                    col += 1;
                }
                let (cs, rs) = (attr(cell, "colspan") as usize, (attr(cell, "rowspan") as usize).min(rows.len() - r));
                for line in &mut taken[r..r + rs] {
                    if line.len() < col + cs {
                        line.resize(col + cs, false);
                    }
                    line[col..col + cs].fill(true);
                }
                cells.push((cell, row, r, col, cs, rs));
                col += cs;
            }
        }
        let cols = taken.iter().map(Vec::len).max().unwrap_or(1).max(1);
        // Inherited, as `border-collapse` and `border-spacing` are.
        let up = || {
            std::iter::successors(Some(id), |&n| self.tree.get(n).parent).filter_map(|n| self.styles.get(&n))
        };
        let collapse = up().find_map(|s| s.table.collapse).unwrap_or(false);
        let spacing = match collapse {
            true => (0.0, 0.0),
            false => up().find_map(|s| s.table.spacing).unwrap_or((0.0, 0.0)),
        };
        // `table-layout: fixed` with a width: the first row's widths make the columns, the rest
        // share what is left, and content never widens them.
        let fixed = c.table.fixed && c.base.size.width.is_some_and(|w| w != Length::Auto);
        let mut tracks = vec![if fixed { GridTrack::Fr(1.0) } else { GridTrack::Auto }; cols];
        for &(cell, _, r, col, cs, _) in &cells {
            if fixed && r > 0 {
                continue;
            }
            let width = match self.styles[&cell].base.size.width {
                // Grows toward its share as space allows; tables squeeze percent columns rather
                // than overflow, and they never set the table's min width.
                Some(Length::Definite(DefiniteLength::Fraction(f))) if fixed => Some(GridTrack::Percent(f)),
                Some(Length::Definite(DefiniteLength::Fraction(f))) => Some(GridTrack::MinMax(
                    Box::new(GridTrack::Length(px(0.0).into())),
                    Box::new(GridTrack::Percent(f)),
                )),
                Some(Length::Definite(DefiniteLength::Absolute(a))) => Some(GridTrack::Length(a)),
                _ => None,
            };
            if let (1, Some(w)) = (cs, width) {
                tracks[col] = w;
            }
        }
        // A table wider than its fixed-width columns (`w-full` over TanStack's column sizes) grows
        // them past those widths as auto table layout does: first those whose content is wider
        // (a long description), then all of them.
        if !fixed
            && c.base.size.width.is_some_and(|w| w != Length::Auto)
            && tracks.iter().all(|t| matches!(t, GridTrack::Length(_)))
        {
            for t in &mut tracks {
                *t = GridTrack::MinMax(Box::new(t.clone()), Box::new(GridTrack::Auto));
            }
        }
        let mut el = self.boxed(id, c);
        let s = el.style();
        s.display = Some(gpui::Display::Grid);
        s.grid_template_columns = Some(tracks);
        s.grid_table_columns = Some(true);
        if spacing != (0.0, 0.0) {
            // shortcut: the spacing replaces any padding of the table's own.
            let (h, v) = (px(spacing.0), px(spacing.1));
            s.gap = gpui::SizeRefinement { width: Some(h.into()), height: Some(v.into()) };
            s.padding = gpui::EdgesRefinement { top: Some(v.into()), bottom: Some(v.into()), left: Some(h.into()), right: Some(h.into()) };
        }
        // A caption spans the table above its rows (MUI X Charts' tooltip titles its rows so).
        // shortcut: `caption-side: bottom` is drawn on top too.
        let mut first_row = 1;
        for caption in kids(id).filter(|&k| self.tree.get(k).tag == "caption") {
            if let Some(built) = self.build(caption) {
                let mut wrap = div().child(built);
                wrap.style().grid_location = Some(GridLocation {
                    row: GridPlacement::Line(first_row)..GridPlacement::Auto,
                    column: GridPlacement::Line(1)..GridPlacement::Line(-1),
                });
                // Not spaced from the rows: the spacing is between cells.
                if spacing.1 > 0.0 {
                    wrap.style().margin.bottom = Some(px(-spacing.1).into());
                }
                el = el.child(wrap);
                first_row += 1;
            }
        }
        for (cell, row, r, col, cs, rs) in cells {
            let Some(built) = self.table_cell(cell) else {
                continue;
            };
            let rs_ = &self.styles[&row].base;
            // A row shows its group's background through it (shadcn's `thead` is `bg-muted`).
            let group = self.tree.get(row).parent.filter(|&p| p != id);
            let background = rs_.background.clone().or_else(|| {
                group.and_then(|g| self.styles.get(&g)?.base.background.clone())
            });
            let mut wrap = div().flex().flex_col().child(built);
            let s = wrap.style();
            s.grid_location = Some(GridLocation {
                row: GridPlacement::Line(first_row + r as i16)..GridPlacement::Span(rs as u16),
                column: GridPlacement::Line(col as i16 + 1)..GridPlacement::Span(cs as u16),
            });
            s.background = background;
            s.border_widths = rs_.border_widths.clone();
            s.border_color = rs_.border_color;
            // Collapsed borders: each cell overlaps the borders of those before it.
            if collapse {
                let width = |w: Option<AbsoluteLength>| w.map_or(0.0, |w| f32::from(w.to_pixels(px(REM))));
                let own = &self.styles[&cell].base.border_widths;
                if col > 0 {
                    s.margin.left = Some(px(-width(own.left)).into());
                }
                if r > 0 {
                    s.margin.top = Some(px(-width(own.top)).into());
                }
            }
            // Percent columns may be squeezed below their content (which then truncates);
            // other columns are never narrower than their content, as in table layout.
            if fixed
                || matches!(
                    self.styles[&cell].base.size.width,
                    Some(Length::Definite(DefiniteLength::Fraction(_)))
                )
            {
                s.min_size.width = Some(px(0.0).into());
            }
            el = el.child(wrap);
        }
        el.into_any_element()
    }

    /// A `<td>`/`<th>` filling its column and row, its content placed by `vertical-align` (the
    /// cell's, else its row's or row group's: `middle` by default).
    /// shortcut: the content is laid out as block flow even if the cell sets `display: flex`.
    fn table_cell(&self, id: u64) -> Option<AnyElement> {
        use crate::css::props::VAlign;
        let c = self.styles.get(&id)?;
        let valign = std::iter::successors(Some(id), |&n| self.tree.get(n).parent)
            .take(3)
            .find_map(|n| self.styles.get(&n)?.table.valign)
            .unwrap_or(VAlign::Middle);
        let mut el = self.boxed(id, c);
        let s = el.style();
        s.size.width = None;
        s.min_size.width = None;
        s.max_size.width = None;
        s.flex_grow = Some(1.0);
        s.display = Some(gpui::Display::Flex);
        s.flex_direction = Some(gpui::FlexDirection::Column);
        s.justify_content = Some(match valign {
            VAlign::Middle => gpui::JustifyContent::Center,
            VAlign::Bottom => gpui::JustifyContent::FlexEnd,
            _ => gpui::JustifyContent::FlexStart,
        });
        let content = self.children(div().id(("cell", id)), id, Disp::Block);
        Some(el.child(content).into_any_element())
    }

    /// An inline `<svg>`, rasterized at the window's scale so it stays sharp.
    fn svg(&self, id: u64, c: &Computed) -> AnyElement {
        let px_of = |l: Option<Length>| match l {
            Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(p)))) => {
                Some(f32::from(p))
            }
            _ => None,
        };
        // A browser's default replaced-element size when nothing else says.
        let (iw, ih) = svg::intrinsic_size(&self.tree, id).unwrap_or((300.0, 150.0));
        let (w, h) = (
            px_of(c.base.size.width).unwrap_or(iw),
            px_of(c.base.size.height).unwrap_or(ih),
        );
        let k = self.scale.get();
        // SVG text inherits the page font, as in a browser; resvg would otherwise fall back to a serif.
        let font = std::iter::successors(Some(id), |&n| self.tree.get(n).parent)
            .find_map(|n| self.styles.get(&n)?.base.text.as_ref()?.font_family.clone());
        let paints_of = |n: u64| {
            let c = self.styles.get(&n)?;
            let (fill, stroke) = match &c.hover_paints {
                Some(paints) if self.svg_hover.contains(&n) => paints.clone(),
                _ => (c.fill.clone(), c.stroke.clone()),
            };
            Some(svg::Paints {
                color: c.color,
                fill,
                stroke,
                vars: c.vars.clone(),
                // As shown: a transition may be animating it (an attribute's included, see the cascade).
                opacity: self.shown(n, c).opacity,
                presentation: {
                    // The element's own font size and weight from CSS reach SVG text through
                    // attributes too (MUI X's pie labels are styled so).
                    let mut p = c.presentation.clone();
                    if let Some(t) = &c.base.text {
                        if let Some(AbsoluteLength::Pixels(size)) = t.font_size {
                            p.push(("font-size", format!("{}px", f32::from(size))));
                        }
                        if let Some(weight) = t.font_weight {
                            p.push(("font-weight", weight.0.to_string()));
                        }
                    }
                    p
                },
                hidden: c.disp == Disp::None || c.base.visibility == Some(gpui::Visibility::Hidden),
            })
        };
        // gpui can't rotate or scale a box; an SVG box is turned inside its own image instead.
        let mut style = self.shown(id, c);
        let transform = (style.rotate.is_some() || style.scale.is_some()).then(|| {
            let scale = style.scale.unwrap_or(gpui::point(1.0, 1.0));
            let background = style
                .background
                .take()
                .and_then(|gpui::Fill::Color(b)| b.solid_color())
                .map(|color| {
                    let radius = match style.corner_radii.top_left {
                        Some(AbsoluteLength::Pixels(r)) => f32::from(r),
                        _ => 0.0,
                    };
                    (color, radius)
                });
            svg::BoxTransform {
                rotate: style.rotate.unwrap_or(0.0),
                scale: (scale.x, scale.y),
                background,
            }
        });
        let sizes = ((w * k, h * k), (w, h));
        let font = font.as_ref().map(|f| f.as_ref());
        // A big plain drawing is drawn in layers, so what changes is redrawn without the rest.
        let layers = match (w * h > SVG_SYNC_AREA && transform.is_none())
            .then(|| svg::parts(&self.tree, id, &paints_of, sizes, font))
            .flatten()
        {
            Some(parts) => self.svg_layers(&parts),
            None => vec![svg::markup(&self.tree, id, &paints_of, sizes, font, transform.as_ref())],
        };
        // Nothing to draw (a chart before it's measured, a hidden root): the box keeps its place,
        // events and size reports.
        let empty = || {
            let mut b = self.boxed(id, c);
            b.style().size.width.get_or_insert(px(w).into());
            b.style().size.height.get_or_insert(px(h).into());
            b.into_any_element()
        };
        if w * k < 1.0 || h * k < 1.0 || layers.iter().all(String::is_empty) {
            return empty();
        }
        if !self.svg_fonts_ready && layers.iter().any(|m| m.contains("<text")) {
            // Holds the space until the font database is loaded; then the window redraws.
            return div().w(px(w)).h(px(h)).into_any_element();
        }
        let frame = self.frame.get();
        let mut svgs = self.svgs.borrow_mut();
        let mut shown = self.svg_shown.borrow_mut();
        // A big one that changed (a chart animating, a map) keeps its last picture while the new
        // layers are drawn off the UI thread, which takes tens of ms; a new or small one (an icon)
        // or a layer with little in it (a hover highlight) is drawn now, so nothing pops in or lags.
        // One changing every frame (an animation) is always drawn off the UI thread: drawn here,
        // its layers took a frame's time several times over.
        let changed = shown.get(&id).is_some_and(|old| *old != layers);
        let animating = changed && {
            let mut at = self.svg_changed.borrow_mut();
            let now = Instant::now();
            let recent = at.get(&id).is_some_and(|t| now - *t < SVG_ANIMATING);
            at.insert(id, now);
            recent
        };
        let quick = |m: &str| {
            w * h <= SVG_SYNC_AREA || (!animating && m.len() < SVG_SYNC_MARKUP && !m.contains("<text"))
        };
        let slow: Vec<String> = layers
            .iter()
            .filter(|m| !svgs.contains_key(*m) && !quick(m))
            .cloned()
            .collect();
        let previous = shown
            .get(&id)
            .filter(|old| !slow.is_empty() && old.iter().all(|m| svgs.contains_key(m)));
        // Layers it will show soon are kept too.
        for m in &layers {
            if let Some(entry) = svgs.get(m) {
                entry.1.set(frame);
            }
        }
        let showing = match previous {
            Some(previous) => {
                self.svg_jobs.borrow_mut().push((id, slow, layers.clone()));
                previous.clone()
            }
            None => {
                if shown.get(&id) != Some(&layers) {
                    shown.insert(id, layers.clone());
                }
                layers
            }
        };
        let images: Vec<Arc<RenderImage>> = showing
            .into_iter()
            .filter_map(|m| {
                let entry = svgs
                    .entry(m)
                    .or_insert_with_key(|m| (rasterize_svg(&self.rasterize, m), Cell::new(frame)));
                entry.1.set(frame);
                entry.0.clone()
            })
            .collect();
        drop((svgs, shown));
        let Some(image) = images.first().cloned() else {
            return empty();
        };
        if images.len() > 1 {
            // Stacked in drawing order, each the size of the box.
            let props = &self.tree.get(id).props;
            let mut wrapper = match props.measure || !props.events.is_empty() {
                true => self.boxed(id, c),
                false => {
                    let mut plain = div().id(("svg", id));
                    (style.rotate, style.scale) = (None, None);
                    *plain.style() = style;
                    plain
                }
            };
            wrapper.style().size.width.get_or_insert(px(w).into());
            wrapper.style().size.height.get_or_insert(px(h).into());
            let layers = images.into_iter().map(|image| {
                img(ImageSource::Render(image)).absolute().top_0().left_0().size_full()
            });
            return wrapper.children(layers).into_any_element();
        }
        let (mx, my) = transform
            .as_ref()
            .map_or((0.0, 0.0), |t| t.overflow((w, h)));
        // The image already shows the transform; the box itself must not turn again.
        (style.rotate, style.scale) = (None, None);
        let mut el = img(ImageSource::Render(image));
        let props = &self.tree.get(id).props;
        if (mx, my) == (0.0, 0.0) && (props.measure || !props.events.is_empty()) {
            // JS hit-tests inside the drawing (a chart's bars, a map's regions) and measures it:
            // the box carries the events and the rect reports, the image fills it.
            let mut wrapper = self.boxed(id, c);
            wrapper.style().size.width.get_or_insert(px(w).into());
            wrapper.style().size.height.get_or_insert(px(h).into());
            return wrapper.child(el.size_full()).into_any_element();
        }
        if (mx, my) == (0.0, 0.0) {
            *el.style() = style;
            el.style().size.width.get_or_insert(px(w).into());
            el.style().size.height.get_or_insert(px(h).into());
            return el.into_any_element();
        }
        // The turned corners reach past the box: the box keeps its layout, the image overhangs it.
        let mut wrapper = div();
        *wrapper.style() = style;
        wrapper.style().size.width.get_or_insert(px(w).into());
        wrapper.style().size.height.get_or_insert(px(h).into());
        let image = el
            .absolute()
            .left(px(-mx))
            .top(px(-my))
            .w(px(w + 2.0 * mx))
            .h(px(h + 2.0 * my));
        wrapper.child(image).into_any_element()
    }

    /// A big drawing's layers: runs of its top-level parts alike in whether they changed lately,
    /// so what keeps changing (a chart's hover highlight, an animating series) is redrawn apart
    /// from what doesn't (axes and their labels).
    fn svg_layers(&self, parts: &svg::Parts) -> Vec<String> {
        use std::hash::{Hash, Hasher};
        let now = Instant::now();
        let mut seen = self.svg_parts.borrow_mut();
        let hot: Vec<bool> = parts
            .children
            .iter()
            .map(|(child, markup, _)| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                markup.hash(&mut h);
                let hash = h.finish();
                let (last, changed) = seen.entry(*child).or_insert((hash, None));
                if *last != hash {
                    (*last, *changed) = (hash, Some(now));
                }
                changed.is_some_and(|t| now - t < SVG_HOT)
            })
            .collect();
        let mut layers = Vec::new();
        let mut start = 0;
        for i in 1..=hot.len() {
            if i == hot.len() || hot[i] != hot[start] {
                layers.push(parts.layer(start..i));
                start = i;
            }
        }
        if layers.is_empty() {
            layers.push(parts.layer(0..0));
        }
        layers
    }

    /// gpui can't rotate or scale a box. A plain one (no content or border, a solid background:
    /// sonner's spinner bars) is painted as a transformed path instead, taking `style`'s background.
    fn transformed_fill(&self, id: u64, style: &mut StyleRefinement) -> Option<AnyElement> {
        if style.rotate.is_none() && style.scale.is_none() {
            return None;
        }
        let border = &style.border_widths;
        // Clockwise from the bottom, as `box_outline` lists the corners.
        let widths = [border.bottom, border.left, border.top, border.right]
            .map(|w| w.map_or(0.0, |w| f32::from(w.to_pixels(px(REM)))));
        let radius = style
            .corner_radii
            .top_left
            .map_or(0.0, |r| f32::from(r.to_pixels(px(REM))));
        // shortcut: bordered boxes only when square-cornered (ApexCharts' tooltip arrow).
        let bordered = widths.iter().any(|&w| w > 0.0);
        if !self.tree.get(id).children.is_empty() || (bordered && radius > 0.0) {
            return None;
        }
        let Some(gpui::Fill::Color(background)) = &style.background else {
            return None;
        };
        let color = background.solid_color()?;
        style.background = None;
        let border_color = style.border_color.unwrap_or(color);
        // The canvas draws the borders turned along with the box.
        style.border_widths = Default::default();
        // The canvas draws the transform; the box itself must not scale its new child again.
        let rotate = style.rotate.take().unwrap_or(0.0);
        let scale = style.scale.take().map_or((1.0, 1.0), |s| (s.x, s.y));
        let after = style.translate_after.take();
        let paint = canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let along =
                    |l: DefiniteLength, of: f32| f32::from(l.to_pixels(px(of).into(), px(REM)));
                let offset = after.map_or((0.0, 0.0), |a| (along(a.x, size.0), along(a.y, size.1)));
                let center = bounds.center();
                let points: Vec<_> = box_outline(size, radius, rotate, scale, offset)
                    .into_iter()
                    .map(|(x, y)| gpui::point(center.x + px(x), center.y + px(y)))
                    .collect();
                let mut path = gpui::PathBuilder::fill();
                path.add_polygon(&points, true);
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                // Square corners: each corner is one run of identical points, sides between them.
                let corner = |i: usize| points[(i % 4) * points.len() / 4];
                for (side, &w) in widths.iter().enumerate().filter(|(_, w)| **w > 0.0) {
                    let mut edge = gpui::PathBuilder::stroke(px(w));
                    edge.move_to(corner(side));
                    edge.line_to(corner(side + 1));
                    if let Ok(edge) = edge.build() {
                        window.paint_path(edge, border_color);
                    }
                }
            },
        );
        Some(paint.absolute().size_full().into_any_element())
    }

    /// The node's style for its current hover/focus state, partway through a transition if one runs.
    /// The style the node's state calls for, before any transition or animation.
    fn target<'a>(&self, id: u64, c: &'a Computed) -> &'a StyleRefinement {
        let state = self.states.get(&id);
        // shortcut: a group's hover switches its descendants without a transition.
        let group = c
            .group
            .as_ref()
            .filter(|(g, _)| self.states.get(g).is_some_and(|s| s.hovered));
        let focused = state.is_some_and(|s| s.focused);
        // A text field shows focus however it got it; anything else only after a key.
        // shortcut: hover styles don't combine with `:focus-visible` ones; the latter win.
        if focused
            && (self.reported.keyboard.get() || self.inputs.contains_key(&id))
            && let Some(style) = c.focus_visible.as_deref()
        {
            return style;
        }
        // `:focus-within` rules are among the `:focus` ones: focus inside the element shows them.
        // shortcut: so do its `:focus` rules (a focusable box with focus inside it is rare).
        let within = !focused && c.focus.is_some() && self.focus_inside(id);
        match state.map_or((false, within), |s| (s.hovered, s.focused || within)) {
            (true, true) => c
                .hover_focus
                .as_deref()
                .or(c.focus.as_deref())
                .or(c.hover.as_deref()),
            (false, true) => c.focus.as_deref(),
            (true, false) => c.hover.as_deref(),
            (false, false) => group.map(|(_, style)| &**style),
        }
        .unwrap_or(&c.base)
    }

    /// Whether something inside `id` has focus.
    fn focus_inside(&self, id: u64) -> bool {
        let tree = &self.tree;
        self.states.iter().filter(|(_, s)| s.focused).any(|(&f, _)| {
            std::iter::successors(tree.nodes.get(&f).and_then(|n| n.parent), |&p| tree.nodes.get(&p).and_then(|n| n.parent))
                .any(|p| p == id)
        })
    }

    fn shown(&self, id: u64, c: &Computed) -> StyleRefinement {
        let state = self.states.get(&id);
        let target = self.target(id, c);
        let mut style = target.clone();
        if let (Some(t), Some((since, from))) =
            (&c.transition, state.and_then(|s| s.changed.as_ref()))
            && !t.is_done(since.elapsed())
        {
            Look::mix(
                from,
                &Look::of(target).resolved(c.color),
                t.progress(since.elapsed()),
                t,
            )
            .apply(&mut style);
            self.animating.set(true);
            self.animated.borrow_mut().insert(id);
        }
        let mut clocks = self.animation_clocks.borrow_mut();
        match &c.animation {
            Some(animation) => {
                // An animation starts when the element first shows with it, and again if it changes.
                let now = Instant::now();
                let (name, start) = clocks
                    .entry(id)
                    .or_insert_with(|| (animation.name.clone(), now));
                if *name != animation.name {
                    (*name, *start) = (animation.name.clone(), now);
                }
                let elapsed = now.duration_since(*start).as_secs_f32();
                if let Some(look) = animation.sample(&Look::of(&style), elapsed) {
                    look.apply(&mut style);
                }
                if !animation.is_done(elapsed) {
                    self.animating.set(true);
                    self.animated.borrow_mut().insert(id);
                }
            }
            None => {
                clocks.remove(&id);
            }
        }
        style
    }

    fn set_state(&mut self, id: u64, update: impl FnOnce(&mut UiState), cx: &mut Context<Self>) {
        let shown = self
            .styles
            .get(&id)
            .map(|c| Look::of(&self.shown(id, c)).resolved(c.color));
        let state = self.states.entry(id).or_default();
        let before = (state.hovered, state.focused);
        update(state);
        // gpui reports focus a frame after we already applied it; a repeat must not restart the transition.
        if before == (state.hovered, state.focused) {
            return;
        }
        state.changed = shown.map(|look| (Instant::now(), look));
        self.cull
            .borrow_mut()
            .invalidate(&self.tree, [id], &|n| self.cullable(n));
        cx.notify();
    }

    /// Focus set from JS: a native input takes keyboard focus; other elements show `:focus` styles.
    fn apply_focus(&mut self, target: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.js_focused.take() {
            self.set_state(prev, |s| s.focused = false, cx);
        }
        match target.and_then(|id| self.inputs.get(&id).map(|i| i.focus.clone())) {
            Some(handle) => window.focus(&handle),
            None => {
                // An editing host takes typed text through `Root`'s input handler.
                match target.is_some_and(|id| self.is_editing_host(id)) {
                    true => window.focus(&self.edit_focus),
                    false => window.blur(),
                }
                if let Some(id) = target {
                    self.js_focused = Some(id);
                    self.set_state(id, |s| s.focused = true, cx);
                }
            }
        }
    }

    /// An editing host's presses place the caret (a double press a word, a triple its block),
    /// Shift and drags extend the selection; while focused it holds gpui's focus for typing.
    fn editing_events(&self, id: u64, mut el: Stateful<Div>) -> Stateful<Div> {
        let (this, reported) = (self.this.clone(), self.reported.clone());
        el = el.on_mouse_down(MouseButton::Left, move |e, _, cx| {
            reported.editing.set(true);
            let Some(root) = this.upgrade() else { return };
            let root = root.read(cx);
            if let Some((node, offset)) = root.caret_at(id, e.position) {
                let init = json!({ "node": node, "offset": offset, "extend": e.modifiers.shift, "detail": e.click_count });
                root.bridge.event(id, "caret", init);
            }
        });
        let this = self.this.clone();
        let last = Cell::new(None);
        el = el.on_mouse_move(move |e, _, cx| {
            let Some(root) = this.upgrade() else { return };
            let root = root.read(cx);
            if e.pressed_button != Some(MouseButton::Left) || root.edit.host != Some(id) {
                return;
            }
            let point = root.caret_at(id, e.position);
            if let Some((node, offset)) = point
                && last.replace(point) != point
            {
                root.bridge.event(id, "caret", json!({ "node": node, "offset": offset, "drag": true }));
            }
        });
        if self.js_focused == Some(id) {
            el = el.track_focus(&self.edit_focus);
        }
        el
    }

    fn blur_inputs(&mut self, cx: &mut Context<Self>) {
        let focused: Vec<u64> = self
            .states
            .iter()
            .filter(|(_, s)| s.focused)
            .map(|(&id, _)| id)
            .collect();
        for id in focused {
            self.set_state(id, |s| s.focused = false, cx);
        }
    }

    /// A div carrying the node's own box: styles, state styles and events.
    fn boxed(&self, id: u64, c: &Computed) -> Stateful<Div> {
        let props = &self.tree.get(id).props;
        let mut el = div().id(("n", id));
        *el.style() = visible_shadows(self.shown(id, c));
        // A `display: contents` parent has no box to pass its text style down through.
        // shortcut: text directly inside one doesn't get it; only its elements do.
        let contents = std::iter::successors(self.tree.get(id).parent, |&p| self.tree.get(p).parent)
            .map_while(|p| self.styles.get(&p).filter(|s| s.disp == Disp::Contents))
            .collect::<Vec<_>>();
        if !contents.is_empty() {
            let mut text = TextStyleRefinement::default();
            for s in contents.iter().rev() {
                if let Some(t) = &s.base.text {
                    text.refine(t);
                }
            }
            if let Some(own) = &el.style().text {
                text.refine(own);
            }
            el.style().text = Some(text);
        }
        // A fixed box (a modal's overlay, a drawer, a popover) takes the pointer where it is, as
        // the topmost box does in a browser: what it covers gets no hover or press. Its own
        // contents are in front of it and stay interactive; `document` listens at window level.
        // A transparent one around other fixed boxes (MUI's modal root around its backdrop) draws
        // nothing over them, so it doesn't either: they stack just under it (see `fixed_order`),
        // and a press on its empty area is the backdrop's. Presses on its content bubble to it in JS.
        let wrapper = c.fixed
            && self.fixed_wrappers.borrow().contains(&id)
            && paints_nothing(el.style());
        // `pointer-events: none` (MUI's outline over a field): no hitbox, so presses, hovers and
        // the cursor are those of what is behind it.
        let no_pointer = c.no_pointer || wrapper;
        if c.fixed && !no_pointer {
            el = el.occlude();
        }
        if no_pointer {
            el.style().mouse_cursor = None;
        }
        if let Some((list, prev)) = self.in_list.take()
            && self.tree.get(id).parent == Some(list)
        {
            el = self.kid_probe(el, id, c, list, prev);
        }
        if let Some(paint) = self.transformed_fill(id, el.style()) {
            el = el.child(paint);
        }
        if id == HTML {
            // The root fills the window and scrolls, like a browser viewport.
            el = el.size_full().overflow_y_scroll();
            if el.style().background.is_none() {
                el.style().background = self
                    .styles
                    .get(&BODY)
                    .and_then(|b| b.base.background.clone())
                    // A browser's canvas is white when the page paints nothing.
                    .or(Some(gpui::white().into()));
            }
        }
        // A scroll container's offset goes to JS as it changes: `scroll` events and `scrollTop`.
        let scroller = [el.style().overflow.x, el.style().overflow.y].contains(&Some(gpui::Overflow::Scroll));
        if scroller {
            let handle = self.scrolls.borrow_mut().entry(id).or_insert_with(unreported).0.clone();
            el = el.track_scroll(&handle);
            let (scrolls, bridge) = (self.scrolls.clone(), self.bridge.clone());
            let probe = canvas(
                move |_, _, _| {
                    if let Some((handle, last)) = scrolls.borrow_mut().get_mut(&id) {
                        let now = handle.offset();
                        if now != *last {
                            // The first report only tells JS the box scrolls (`scrollIntoView` looks for one).
                            let init = f32::from(last.x).is_nan();
                            *last = now;
                            let (left, top) = (-f32::from(now.x), -f32::from(now.y));
                            bridge.event(id, "scroll", json!({ "scrollLeft": left, "scrollTop": top, "init": init }));
                        }
                    }
                },
                |_, _, _, _| {},
            );
            el = el.child(probe.absolute().top_0().left_0().size_0());
        }
        // Absolute boxes in a static body are placed against the viewport (the initial containing
        // block), not its height: a centered `top-1/2` 404 sat above the window in a 0px body.
        // shortcut: a body taller than the window still places them against its own height.
        if id == BODY && c.base.position.is_none() && el.style().min_size.height.is_none() {
            el.style().min_size.height = Some(gpui::relative(1.).into());
        }
        if c.fixed {
            let mut reveal = self.reveal.borrow_mut();
            // Timed from when the box first shows: one measured later (a field read inside an open
            // modal) has been seen already, and hiding it would also drop its pointer listeners.
            let (first, ready) = *reveal.entry(id).or_insert((Instant::now(), false));
            // shortcut: 100 ms cap in case JS never answers (its position didn't change).
            if props.measure && !ready && first.elapsed() < Duration::from_millis(100) {
                el.style().visibility = Some(gpui::Visibility::Hidden);
                self.animating.set(true);
            }
        }
        if let Some(a) = c.active.as_deref().cloned().map(visible_shadows)
            && !no_pointer
        {
            el = el.active(move |_| a);
        }
        // A right press opens a context menu (`menu.popup()` in an `onContextMenu`); JS bubbles it.
        if props.listens("contextmenu") && !no_pointer {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            el = el.on_mouse_down(MouseButton::Right, move |_, window, _| {
                if !reported.context.replace(true) {
                    bridge.event(id, "contextmenu", at(window));
                }
            });
        }
        // A custom title bar: the innermost `app-region` box under a press decides. Dragging one
        // moves the window, and a double-click does what the system's title bar does.
        if let Some(region) = c.app_region
            && !no_pointer
        {
            let reported = self.reported.clone();
            el = el.on_mouse_down(MouseButton::Left, move |e, window, _| {
                if reported.region.replace(true) || region == AppRegion::NoDrag {
                    return;
                }
                match e.click_count {
                    2 => window.titlebar_double_click(),
                    _ => window.start_window_move(),
                }
            });
        }
        // JS bubbles events itself, so for each press only the innermost listener reports; the root
        // resets these flags as each event starts. Propagation is never stopped, so ancestors' gpui
        // clicks and the root's blur still see the press.
        if props.listens("click") && !props.disabled && !no_pointer {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            let up_too = props.listens("mouseup");
            el = el.on_click(move |_, window, _| {
                // gpui runs click before mouse-up listeners; browsers send pointerup first.
                if !reported.up.replace(true) {
                    match up_too {
                        true => bridge.event(id, "mouseup", pressed_at(window, &reported)),
                        false => bridge.document_from(
                            window,
                            reported.down_target.get(),
                            &["pointerup", "mouseup"],
                        ),
                    }
                }
                if !reported.click.replace(true) {
                    bridge.event(id, "click", pressed_at(window, &reported));
                }
            });
        }
        // gpui counts clicks: the second of a pair is a double-click.
        if props.listens("dblclick") && !props.disabled && !no_pointer {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            el = el.on_click(move |e, window, _| {
                if e.click_count() == 2 && !reported.dblclick.replace(true) {
                    bridge.event(id, "dblclick", pressed_at(window, &reported));
                }
            });
        }
        // A DOM wheel event's deltas: positive scrolls down and right, the opposite of gpui's.
        // shortcut: reported, not cancelable; `preventDefault` doesn't stop the host's scrolling.
        if props.listens("wheel") && !no_pointer {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            el = el.on_scroll_wheel(move |e, window, _| {
                if reported.wheel.replace(true) {
                    return;
                }
                let (dx, dy, mode) = match e.delta {
                    ScrollDelta::Pixels(p) => (-f32::from(p.x), -f32::from(p.y), 0),
                    ScrollDelta::Lines(p) => (-p.x, -p.y, 1),
                };
                let mut init = at(window);
                init["deltaX"] = json!(dx);
                init["deltaY"] = json!(dy);
                init["deltaMode"] = json!(mode);
                bridge.event(id, "wheel", init);
            });
        }
        if c.container {
            let (sizes, resized) = (
                self.container_sizes.clone(),
                self.resized_containers.clone(),
            );
            // `@container` rules read the laid-out size, so a new width restyles the subtree and
            // the frame is drawn again before it shows, as a browser does it within the same one.
            let probe = canvas(
                move |bounds, window, _| {
                    let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let old = sizes.borrow_mut().insert(id, size);
                    if old.is_none_or(|(width, _)| (width - size.0).abs() >= 0.5) {
                        resized.borrow_mut().push(id);
                        window.redraw_before_present();
                    }
                },
                |_, _, _, _| {},
            );
            el = el.child(probe.absolute().top_0().left_0().size_full());
        }
        if self.containing_blocks.borrow().contains_key(&id) {
            let blocks = self.containing_blocks.clone();
            // Where it is drawn, translation included, for the fixed boxes it contains; a change is
            // drawn again before it shows, as `@container` sizes are.
            let probe = canvas(
                move |bounds, window, _| {
                    let old = blocks.borrow_mut().insert(id, Some(bounds));
                    if old != Some(Some(bounds)) {
                        window.redraw_before_present();
                    }
                },
                |_, _, _, _| {},
            );
            el = el.child(probe.absolute().top_0().left_0().size_full());
        }
        if props.measure {
            let (bridge, measured, rects) = (
                self.bridge.clone(),
                self.measured.clone(),
                self.rects.clone(),
            );
            let (padding, border) = box_edges(&c.base);
            // An absolute child sees the box's laid-out bounds without affecting its layout: its
            // padding box, which the borders grow to the border box JS reads.
            let probe = canvas(
                move |bounds, _, _| {
                    if measured.borrow_mut().insert(id, bounds) != Some(bounds) {
                        let (o, s) = (bounds.origin, bounds.size);
                        let [top, right, bottom, left] = border;
                        let rect = json!({ "x": f32::from(o.x) - left, "y": f32::from(o.y) - top, "width": f32::from(s.width) + left + right, "height": f32::from(s.height) + top + bottom, "padding": padding, "border": border });
                        rects.borrow_mut().insert(id, rect.clone());
                        bridge.event(id, "resize", json!({ "rect": rect }));
                    }
                },
                |_, _, _, _| {},
            );
            el = el.child(probe.absolute().top_0().left_0().size_full());
        }
        if no_pointer {
            return el;
        }
        // Every box hears presses, innermost first, so `document` gets the real target of one no
        // element reports (a menu's click-outside check sees the press land inside the menu).
        let listens_down = props.listens("mousedown") && !props.disabled;
        let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
        el = el.on_mouse_down(MouseButton::Left, move |_, window, _| {
            if reported.down_target.get().is_none() {
                reported.down_target.set(Some(id));
            }
            if listens_down && !reported.down.replace(true) {
                bridge.event(id, "mousedown", pressed_at(window, &reported));
            }
        });
        if self.is_editing_host(id) {
            el = self.editing_events(id, el);
        }
        if props.listens("mouseup") && !props.disabled {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            el = el.on_mouse_up(MouseButton::Left, move |_, window, _| {
                if !reported.up.replace(true) {
                    bridge.event(id, "mouseup", pressed_at(window, &reported));
                }
            });
        }
        if props.listens("mousemove") {
            let (bridge, reported) = (self.bridge.clone(), self.reported.clone());
            // Bubbles from the innermost hovered element; only that one reports the move.
            el = el.on_mouse_move(move |_, window, _| {
                if !reported.moved.replace(true) {
                    bridge.event(id, "mousemove", at(window));
                }
            });
        }
        let js_hover = props.listens("mouseenter") || props.listens("mouseleave");
        let styled_hover = c.hover.is_some() || self.groups.contains_key(&id);
        if js_hover || styled_hover {
            let (this, bridge) = (self.this.clone(), self.bridge.clone());
            let tracked = self.hovered.clone();
            el = el.on_hover(move |&hovered, window, cx| {
                match hovered {
                    true => tracked.borrow_mut().insert(id),
                    false => tracked.borrow_mut().remove(&id),
                };
                if js_hover {
                    let kind = if hovered { "mouseenter" } else { "mouseleave" };
                    bridge.event(id, kind, at(window));
                }
                if styled_hover {
                    let _ = this.update(cx, |root, cx| {
                        root.set_state(id, |s| s.hovered = hovered, cx)
                    });
                }
            });
        }
        el
    }

    /// For a long list laid out as a vertical stack: adds the probe that measures it and returns
    /// which of its children to leave out of this build (see `cull`).
    fn list_runs(&self, el: &mut Stateful<Div>, id: u64, disp: Disp, kids: &[u64]) -> Option<Runs> {
        use gpui::{FlexDirection, FlexWrap, JustifyContent};
        let s = &self.styles.get(&id)?.base;
        let vertical = match disp {
            // Free space shared out between items would change if a run became one item.
            Disp::Flex | Disp::InlineFlex => {
                s.flex_direction == Some(FlexDirection::Column)
                    && matches!(s.flex_wrap, None | Some(FlexWrap::NoWrap))
                    && matches!(
                        s.justify_content,
                        None | Some(JustifyContent::Start | JustifyContent::FlexStart)
                    )
            }
            Disp::Grid | Disp::InlineGrid => false,
            _ => self.tree.get(id).tag != "button",
        };
        if !vertical || kids.len() < cull::MIN_KIDS {
            return None;
        }
        // Unmeasured children may stand in with an estimate if they are boxes in flow.
        let estimable = |k: u64| {
            let blockified = matches!(
                disp,
                Disp::Flex | Disp::InlineFlex | Disp::Grid | Disp::InlineGrid
            );
            self.flows(k) && (blockified || !self.is_inline(k)) && self.cullable(k)
        };
        let view = self.env.map_or(0.0, |e| e.height);
        let found =
            self.cull
                .borrow_mut()
                .runs(id, kids, &|k| self.keep.contains(&k), &estimable, view);
        let cull = self.cull.clone();
        let guessed = found.guessed;
        let probe = canvas(
            move |bounds, window, _| {
                let mask = window.content_mask().bounds;
                let origin = f32::from(bounds.origin.y);
                let mut c = cull.borrow_mut();
                let list = List {
                    frame: c.frame,
                    origin,
                    visible: (
                        f32::from(mask.top()) - origin,
                        f32::from(mask.bottom()) - origin,
                    ),
                    width: f32::from(bounds.size.width),
                };
                c.lists.insert(id, list);
                // Rows left out at a guessed height: lay out again with the built ones' measured.
                if guessed {
                    window.redraw_before_present();
                }
            },
            |_, _, _, _| {},
        );
        el.extend([probe
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element()]);
        Some(found.runs)
    }

    /// In a culled list, the spacer for the run of left-out children starting at `i`, and the
    /// index to go on from after it (`next`: the first run not passed yet). Otherwise notes the
    /// child's place in the list for its probe (see `kid_probe`).
    fn culled(
        &self,
        list: Option<(u64, &Runs)>,
        next: &mut usize,
        i: usize,
        kids: &[u64],
    ) -> Option<(AnyElement, usize)> {
        let (list, runs) = list?;
        while runs.get(*next).is_some_and(|(start, _)| *start < i) {
            *next += 1;
        }
        if let Some((start, run)) = runs.get(*next)
            && *start == i
        {
            *next += 1;
            return Some((self.spacer(list, run.clone()), i + run.len()));
        }
        let prev = i.checked_sub(1).map_or(0, |p| kids[p]);
        self.in_list.set(Some((list, prev)));
        None
    }

    /// Measures a list's child at each layout, so it can be left out while off screen. Children
    /// whose size can change without a commit aren't measured: an image without a set size (it
    /// loads), and anything JS observes (its reported rect must stay current).
    fn kid_probe(
        &self,
        el: Stateful<Div>,
        id: u64,
        c: &Computed,
        list: u64,
        prev: u64,
    ) -> Stateful<Div> {
        // Moved off its place in flow, it isn't where a spacer for it would be.
        let i = &c.base.inset;
        let offset = c.base.position == Some(gpui::Position::Relative)
            && [i.top, i.right, i.bottom, i.left]
                .iter()
                .any(Option::is_some);
        let transformed =
            c.base.translate.is_some() || c.base.rotate.is_some() || c.base.scale.is_some();
        if offset || transformed || !self.cullable(id) {
            self.cull.borrow_mut().kids.remove(&id);
            return el;
        }
        let (_, border) = box_edges(&c.base);
        let cull = self.cull.clone();
        let probe = canvas(
            move |bounds, _, _| {
                let mut c = cull.borrow_mut();
                let Some(l) = c.lists.get(&list).filter(|l| l.frame == c.frame).copied() else {
                    return;
                };
                let kid = Kid {
                    top: f32::from(bounds.origin.y) - border[0] - l.origin,
                    height: f32::from(bounds.size.height) + border[0] + border[2],
                    list_width: l.width,
                    prev,
                    estimated: false,
                };
                c.kids.insert(id, kid);
            },
            |_, _, _, _| {},
        );
        el.child(probe.absolute().top_0().left_0().size_full())
    }

    /// A box in its parent's flow, which a list's spacer can stand in for.
    fn flows(&self, id: u64) -> bool {
        self.styles.get(&id).is_some_and(|c| {
            c.disp != Disp::None && !c.fixed && c.base.position != Some(gpui::Position::Absolute)
        })
    }

    fn cullable(&self, id: u64) -> bool {
        let node = self.tree.get(id);
        if node.is_text() {
            return true;
        }
        if node.props.measure {
            return false;
        }
        let sized = |c: &Computed| c.base.size.width.is_some() && c.base.size.height.is_some();
        if node.tag == "img" && !self.styles.get(&id).is_some_and(|c| sized(c)) {
            return false;
        }
        node.children.iter().all(|&ch| self.cullable(ch))
    }

    /// Stands in for a run of culled children: their extent, with the outer margins of the first
    /// and last. If content above moved it, they move along; if it shows, the frame is redrawn
    /// with them built.
    fn spacer(&self, list: u64, run: Vec<u64>) -> AnyElement {
        let (_, height) = self.cull.borrow().extent(&run);
        let margin = |id: &u64| self.styles.get(id).map(|c| c.base.margin.clone());
        let (first, last) = (margin(&run[0]), margin(run.last().unwrap()));
        let mut el = div().flex_shrink_0().h(px(height));
        el.style().margin.top = first.and_then(|m| m.top);
        el.style().margin.bottom = last.and_then(|m| m.bottom);
        let cull = self.cull.clone();
        let probe = canvas(
            move |bounds, window, _| {
                let mut c = cull.borrow_mut();
                if let Some(l) = c.lists.get(&list).filter(|l| l.frame == c.frame).copied() {
                    c.moved(&run, f32::from(bounds.origin.y) - l.origin);
                }
                if bounds.intersects(&window.content_mask().bounds) {
                    window.redraw_before_present();
                }
            },
            |_, _, _, _| {},
        );
        el.child(probe.absolute().top_0().left_0().size_full())
            .into_any_element()
    }

    fn children(&self, mut el: Stateful<Div>, id: u64, disp: Disp) -> Stateful<Div> {
        let node = self.tree.get(id);
        let mut kids = self.kids(id);
        let flex_or_grid = matches!(
            disp,
            Disp::Flex | Disp::InlineFlex | Disp::Grid | Disp::InlineGrid
        );
        let order = |k: &u64| self.styles.get(k).map_or(0, |s| s.order);
        if flex_or_grid && kids.iter().any(|k| order(k) != 0) {
            // Stable: items with the same `order` keep tree order.
            kids.sort_by_key(order);
        }
        if let Some((marker, false)) = self.marker(id) {
            // An outside marker ends where the item's box begins.
            let mut m = div().absolute().top_0().whitespace_nowrap().child(marker);
            m.style().inset.right = Some(gpui::relative(1.).into());
            el = el.child(m);
        }
        // Border sides of different colors, mitred at the corners as CSS draws them.
        if let Some(c) = self.styles.get(&id)
            && let Some(colors) = c.border_sides
        {
            let w = |l: Option<AbsoluteLength>| l.map_or(0.0, |l| f32::from(l.to_pixels(px(REM))));
            let b = &c.base.border_widths;
            let [t, r, bm, l] = [w(b.top), w(b.right), w(b.bottom), w(b.left)];
            let mut layer = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let o = bounds.origin;
                    let (wd, ht) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let at = |x: f32, y: f32| o + point(px(x), px(y));
                    let sides = [
                        [at(0., 0.), at(wd, 0.), at(wd - r, t), at(l, t)],
                        [at(wd, 0.), at(wd, ht), at(wd - r, ht - bm), at(wd - r, t)],
                        [at(wd, ht), at(0., ht), at(l, ht - bm), at(wd - r, ht - bm)],
                        [at(0., ht), at(0., 0.), at(l, t), at(l, ht - bm)],
                    ];
                    for (quad, color) in sides.into_iter().zip(colors) {
                        if color.is_transparent() {
                            continue;
                        }
                        let mut path = gpui::PathBuilder::fill();
                        path.move_to(quad[0]);
                        for p in &quad[1..] {
                            path.line_to(*p);
                        }
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color);
                        }
                    }
                },
            )
            .absolute();
            // Over the border box, from inside the border the box's children start at.
            let s = layer.style();
            s.inset.top = Some(px(-t).into());
            s.inset.right = Some(px(-r).into());
            s.inset.bottom = Some(px(-bm).into());
            s.inset.left = Some(px(-l).into());
            el = el.child(layer);
        }
        // Gradient layers the element's own background can't draw, under any image.
        if let Some(c) = self.styles.get(&id)
            && !c.bg_layers.is_empty()
        {
            let (layers, radii) = (c.bg_layers.clone(), c.base.corner_radii.clone());
            let layer = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let rem = window.rem_size();
                    let r = |l: Option<AbsoluteLength>| l.map_or(px(0.0), |l| l.to_pixels(rem));
                    let radii = gpui::Corners {
                        top_left: r(radii.top_left),
                        top_right: r(radii.top_right),
                        bottom_right: r(radii.bottom_right),
                        bottom_left: r(radii.bottom_left),
                    };
                    crate::background::paint(&layers, bounds, radii, window);
                },
            );
            el = el.child(layer.absolute().top_0().left_0().size_full());
        }
        if let Some(layer) = self.styles.get(&id).and_then(|c| bg_image(c)) {
            el = el.child(layer);
        }
        let runs = self.list_runs(&mut el, id, disp, &kids);
        let list = runs.as_ref().map(|runs| (id, runs));
        if flex_or_grid {
            // Flex and grid items are blockified; each run of text is one anonymous item.
            return el.children(self.items(&kids, false, list));
        }
        // Block flow: consecutive inline-level children share a line box.
        let mut out = Vec::new();
        let mut line = Vec::new();
        // Out-of-flow boxes stay direct children, so this element (not a line box) is their
        // containing block; painted last, as positioned boxes are, in z-index order.
        let mut out_of_flow = Vec::new();
        let (mut i, mut next_run) = (0, 0);
        while i < kids.len() {
            let c = kids[i];
            i += 1;
            let absolute = self
                .styles
                .get(&c)
                .is_some_and(|s| s.base.position == Some(gpui::Position::Absolute));
            if absolute {
                let z = self.styles.get(&c).and_then(|s| s.z_index).unwrap_or(0);
                out_of_flow.extend(self.build(c).map(|el| (z, el)));
            } else if self.is_inline(c) {
                line.push(c);
            } else {
                out.extend(self.line(&std::mem::take(&mut line)));
                // A positioned box paints over the out-of-flow boxes before it that aren't
                // z-indexed above it (tree order, as in CSS: a `relative` label over an
                // `absolute` divider line).
                if let Some(s) = self.styles.get(&c)
                    && s.base.position == Some(gpui::Position::Relative)
                {
                    let z = s.z_index.unwrap_or(0);
                    out_of_flow.sort_by_key(|(z, _)| *z);
                    let below = out_of_flow.partition_point(|(oz, _)| *oz <= z);
                    out.extend(out_of_flow.drain(..below).map(|(_, el)| el));
                }
                if let Some((spacer, end)) = self.culled(list, &mut next_run, i - 1, &kids) {
                    out.push(spacer);
                    i = end;
                    continue;
                }
                let fit = self.styles.get(&c).is_some_and(|s| s.fit_block);
                out.extend(self.build(c).map(|el| match fit {
                    // A row lets the box shrink to its content; block layout would stretch it.
                    true => div().flex().child(el).into_any_element(),
                    false => el,
                }));
                self.in_list.set(None);
            }
        }
        out.extend(self.line(&line));
        out_of_flow.sort_by_key(|(z, _)| *z);
        let out_of_flow = out_of_flow.into_iter().map(|(_, el)| el);
        if node.tag == "button" {
            // Browsers center a button's content vertically (a radio's dot in its `size-4` box).
            return el
                .flex()
                .flex_col()
                .justify_center()
                .child(div().children(out))
                .children(out_of_flow);
        }
        el.children(out).children(out_of_flow)
    }

    /// Brings what is kept per styled node (`groups`, `transitioning`, generated boxes) up to
    /// date with `styles` for a node restyled or gone.
    fn track_style(&mut self, id: u64) {
        let c = self.styles.get(&id);
        let group = c.and_then(|c| Some(c.group.as_ref()?.0));
        let transition = c.is_some_and(|c| c.transition.is_some());
        let old = match group {
            Some(g) => self.group_of.insert(id, g),
            None => self.group_of.remove(&id),
        };
        if old != group {
            if let Some(o) = old
                && let Some(n) = self.groups.get_mut(&o)
            {
                *n -= 1;
                if *n == 0 {
                    self.groups.remove(&o);
                }
            }
            if let Some(g) = group {
                *self.groups.entry(g).or_default() += 1;
            }
        }
        match transition {
            true => self.transitioning.insert(id),
            false => self.transitioning.remove(&id),
        };
        for kind in [Pseudo::Before, Pseudo::After] {
            self.sync_pseudo_box(pseudo_id(id, kind));
        }
    }

    /// `track_style` for every node, after a full restyle.
    fn track_all_styles(&mut self) {
        self.groups.clear();
        self.group_of.clear();
        self.transitioning.clear();
        let ids: Vec<u64> = self
            .styles
            .keys()
            .copied()
            .filter(|id| id & PSEUDO == 0)
            .collect();
        for id in ids {
            self.track_style(id);
        }
        // Generated boxes of nodes no longer styled (detached ones) go too.
        let styles = &self.styles;
        self.tree
            .nodes
            .retain(|&id, _| id & PSEUDO == 0 || styles.contains_key(&(id & !PSEUDO_TEXT)));
    }

    /// `content` text with its `attr()`s and `counter()`s (see `GENERATED`) filled in for the
    /// generated box of `owner` (its `::after` when `after`).
    // shortcut: filled in when the box is styled; counters don't follow later sibling changes.
    fn generated(&self, owner: u64, after: bool, text: &str) -> String {
        let (open, shut) = GENERATED;
        let mut out = String::new();
        let mut parts = text.split(open);
        out.push_str(parts.next().unwrap_or(""));
        for part in parts {
            let (marker, rest) = part.split_once(shut).unwrap_or((part, ""));
            let words: Vec<&str> = marker.split(' ').collect();
            match words[..] {
                ["attr", name] => out.extend(self.tree.nodes.get(&owner).and_then(|n| n.props.attrs.get(name)).cloned()),
                ["counter", name, style] => {
                    let n = counter_value(&self.tree, &self.styles, owner, after, name);
                    let label = counter_style(style).label(n).unwrap_or_default();
                    out.push_str(label.trim_end_matches([' ', '.']));
                }
                _ => {}
            }
            out.push_str(rest);
        }
        out
    }

    /// A `::before`/`::after` box as a host-only node under its element, with its `content` as a
    /// text child, or gone when it has no style. They aren't among its children, so selectors and
    /// JS never see them; `kids` lays them out.
    fn sync_pseudo_box(&mut self, id: u64) {
        let text_id = id | PSEUDO_TEXT;
        let Some(c) = self.styles.get(&id) else {
            self.tree.nodes.remove(&id);
            self.tree.nodes.remove(&text_id);
            return;
        };
        let owner = id & (PSEUDO_TEXT - 1);
        let mut text = c.content.clone().unwrap_or_default();
        if text.contains(GENERATED.0) {
            text = self.generated(owner, id == pseudo_id(owner, Pseudo::After), &text);
        }
        let node = tree::Node {
            tag: "::pseudo".into(),
            children: if text.is_empty() {
                vec![]
            } else {
                vec![text_id]
            },
            parent: Some(owner),
            ..Default::default()
        };
        self.tree.nodes.insert(id, node);
        if !text.is_empty() {
            let text = tree::Node {
                text: Some(text),
                parent: Some(id),
                ..Default::default()
            };
            self.tree.nodes.insert(text_id, text);
        }
    }

    /// An element's children for layout: its `::before` box, its children, its `::after` box.
    fn kids(&self, id: u64) -> Vec<u64> {
        // A generated box has none of its own (and its id would name itself).
        let generated = |kind| {
            Some(pseudo_id(id, kind))
                .filter(|p| id & PSEUDO == 0 && self.tree.nodes.contains_key(p))
        };
        let children = &self.tree.get(id).children;
        let mut out = Vec::new();
        for c in generated(Pseudo::Before)
            .into_iter()
            .chain(children.iter().copied())
            .chain(generated(Pseudo::After))
        {
            // `display: contents` generates no box: its children are laid out in its place (MUI
            // X's grid viewport, around its header and the absolutely placed rows).
            match self.styles.get(&c).is_some_and(|s| s.disp == Disp::Contents) {
                true => out.extend(self.kids(c)),
                false => out.push(c),
            }
        }
        out
    }

    fn is_inline(&self, id: u64) -> bool {
        self.tree.get(id).is_text()
            || self
                .styles
                .get(&id)
                .is_some_and(|c| c.disp.is_inline_level())
    }

    /// Plain text and spans become one wrapped paragraph; anything boxier is laid out in a wrapping row.
    fn line(&self, ids: &[u64]) -> Option<AnyElement> {
        if ids.is_empty() {
            return None;
        }
        if ids.iter().all(|&id| self.is_phrasing(id)) {
            // One span of its own size is drawn at that size below, wrapping as text does.
            let mixed = match ids {
                [only] if self.styles.get(only).is_some_and(|c| c.table.valign.is_none() && !inline_box(&c.base)) => {
                    self.kids(*only).into_iter().any(|k| self.needs_words(k))
                }
                _ => ids.iter().any(|&id| self.needs_words(id)),
            };
            if mixed {
                return Some(self.word_boxes(ids));
            }
            // In editable content, what shows where is kept for the caret (see `editing`).
            let host = self.tree.get(ids[0]).parent.and_then(|n| self.editing_host(n));
            let mut p = Paragraph { segments: host.map(|_| Vec::new()), ..Default::default() };
            for &id in ids {
                self.runs(id, HighlightStyle::default(), &mut p);
            }
            // `text-shadow`: the text again under it, offset, all in the shadow's color.
            let parent = self.tree.get(ids[0]).parent;
            let shadow = std::iter::successors(parent, |&n| self.tree.get(n).parent)
                .find_map(|n| self.styles.get(&n)?.text_shadow)
                .flatten();
            let copy = shadow.and_then(|_| {
                let mut copy = p.clone();
                copy.highlights.iter_mut().for_each(|(_, h)| h.color = None);
                copy.finish()
            });
            let segments = p.segments.take();
            let p = p.finish()?;
            let p = match (host, segments) {
                (Some(host), Some(segments)) => self.editable_text(host, ids[0], p, segments),
                _ => p.into_any_element(),
            };
            let p = match (shadow, copy) {
                (Some((color, x, y)), Some(copy)) => {
                    let mut under = div().absolute().left(px(x)).top(px(y)).size_full().child(copy);
                    under.style().text = Some(TextStyleRefinement { color: Some(color), ..Default::default() });
                    div().relative().child(under).child(p).into_any_element()
                }
                _ => p.into_any_element(),
            };
            // gpui's runs share one font size, the block's; a line that is one span of its own size
            // (the 404 page's `text-[10rem]` digits) takes the span's.
            // shortcut: a span of another size among other text still shows at the block's size.
            if let [only] = ids
                && let Some(text) = self.styles.get(only).and_then(|c| c.base.text.clone())
                && text.font_size.is_some()
            {
                let mut line = div().child(p);
                line.style().text = Some(text);
                return Some(line.into_any_element());
            }
            return Some(p.into_any_element());
        }
        // shortcut: approximates an inline formatting context; replace if inline boxes must break across lines.
        let items = self.items(ids, true, None);
        if items.is_empty() {
            return None;
        }
        let row = div().flex().flex_row().flex_wrap();
        // Boxes alone on a line sit on its strut's baseline, which `line-height` puts mid-line:
        // centered in a line at least that tall, as single-line boxes come out (a chip in a data
        // grid cell whose line height is the row's).
        let boxes = ids.iter().all(|&id| self.tree.get(id).text.is_none());
        let strut = boxes
            .then(|| self.tree.get(ids[0]).parent.and_then(|p| self.line_height(p)))
            .flatten();
        let row = match strut {
            Some(h) => row.items_center().min_h(px(h)),
            None => row.items_baseline(),
        };
        Some(row.children(items).into_any_element())
    }

    /// An editable paragraph: its layout kept for hit-testing, over the caret and selected text.
    fn editable_text(&self, host: u64, first: u64, text: StyledText, segments: Vec<Segment>) -> AnyElement {
        let layout = text.layout().clone();
        let segments = Rc::new(segments);
        let mut marks = self.edit.marks(host, &segments);
        marks.caret_color = std::iter::successors(Some(first), |&n| self.tree.get(n).parent)
            .find_map(|n| self.styles.get(&n)?.base.text.as_ref()?.color);
        self.editables.borrow_mut().push(editing::Para { host, layout: layout.clone(), segments });
        let marks = canvas(|_, _, _| {}, move |_, _, window, _| editing::paint(&layout, &marks, window))
            .absolute()
            .size_full();
        div().relative().child(marks).child(text).into_any_element()
    }

    /// Whether `id` is in editable content: inside an element whose `contenteditable` is
    /// `true`, empty or `plaintext-only`, with no `false` in between.
    fn editable(&self, id: u64) -> bool {
        if self.tree.editable.is_empty() {
            return false;
        }
        for n in std::iter::successors(Some(id), |&n| self.tree.nodes.get(&n)?.parent) {
            if !self.tree.editable.contains(&n) {
                continue;
            }
            let value = self.tree.get(n).props.attrs.get("contenteditable").map(|v| v.to_ascii_lowercase());
            match value.as_deref() {
                Some("false") => return false,
                Some("inherit") | None => {}
                Some(v) => return matches!(v, "" | "true" | "plaintext-only"),
            }
        }
        false
    }

    /// The outermost editable element around `id`: it takes focus and the caret.
    fn editing_host(&self, id: u64) -> Option<u64> {
        if !self.editable(id) {
            return None;
        }
        let mut host = id;
        while let Some(parent) = self.tree.get(host).parent
            && self.editable(parent)
        {
            host = parent;
        }
        Some(host)
    }

    fn is_editing_host(&self, id: u64) -> bool {
        self.tree.editable.contains(&id) && self.editing_host(id) == Some(id)
    }

    /// Where a press at `at` puts the caret in `host`'s text.
    fn caret_at(&self, host: u64, at: gpui::Point<Pixels>) -> Option<(u64, usize)> {
        editing::hit(&self.editables.borrow(), host, at)
    }

    /// Where a key moves the caret, for the keys that need the layout: up and down a line, and to
    /// the line's ends (Home/End, ⌘← and ⌘→). JS moves it by characters and words itself.
    fn caret_move(&self, key: &str, command: bool) -> Option<(u64, usize)> {
        let (host, caret) = (self.edit.host?, self.edit.caret?);
        let mv = match (key, command) {
            ("ArrowUp", false) => editing::LineMove::Up,
            ("ArrowDown", false) => editing::LineMove::Down,
            ("Home", _) | ("ArrowLeft", true) => editing::LineMove::Start,
            ("End", _) | ("ArrowRight", true) => editing::LineMove::End,
            _ => return None,
        };
        editing::line_target(&self.editables.borrow(), host, caret, mv)
    }

    /// The line height text in `id` gets, in px: set there or inherited, a factor of the font
    /// size there (`normal` is 1.2). `None` if nothing sets one.
    fn line_height(&self, id: u64) -> Option<f32> {
        let texts = || {
            std::iter::successors(Some(id), |&a| self.tree.get(a).parent)
                .filter_map(|a| self.styles.get(&a)?.base.text.as_ref())
        };
        let to_px = |l: &AbsoluteLength| match l {
            AbsoluteLength::Pixels(p) => f32::from(*p),
            AbsoluteLength::Rems(r) => r.0 * REM,
        };
        match texts().find_map(|t| t.line_height)? {
            DefiniteLength::Absolute(l) => Some(to_px(&l)),
            DefiniteLength::Fraction(f) => {
                let font = texts().find_map(|t| t.font_size).map_or(REM, |l| to_px(&l));
                Some(f * font)
            }
            DefiniteLength::Calc(..) | DefiniteLength::Expr(_) => None,
        }
    }

    /// Elements as boxes, with adjacent text nodes (React splits `a {b} c`) merged into one paragraph.
    /// In a line box (`spaces`), whitespace next to an element is kept as a space, as browsers do.
    fn items(&self, ids: &[u64], spaces: bool, list: Option<(u64, &Runs)>) -> Vec<AnyElement> {
        let space = || StyledText::new(" ").into_any_element();
        let mut items = Vec::new();
        let mut text = Paragraph::default();
        let mut trailing_space = false;
        let mut out_of_flow = Vec::new();
        let (mut i, mut next_run) = (0, 0);
        while i < ids.len() {
            let id = ids[i];
            i += 1;
            match &self.tree.get(id).text {
                Some(t) => {
                    if spaces
                        && text.text.is_empty()
                        && !items.is_empty()
                        && t.starts_with(char::is_whitespace)
                    {
                        items.push(space());
                    }
                    text.push(&self.cased(id, t), self.ws(id), HighlightStyle::default());
                    trailing_space = t.ends_with(char::is_whitespace);
                }
                None if self.tree.get(id).tag == "br" => text.line_break(None),
                None => {
                    let had_text = !text.text.is_empty();
                    items.extend(
                        std::mem::take(&mut text)
                            .finish()
                            .map(IntoElement::into_any_element),
                    );
                    if spaces && had_text && trailing_space {
                        items.push(space());
                    }
                    trailing_space = false;
                    let position = self.styles.get(&id).and_then(|s| s.base.position);
                    let z = self.styles.get(&id).and_then(|s| s.z_index).unwrap_or(0);
                    if position == Some(gpui::Position::Absolute) {
                        // Out of flow: painted over the in-flow items, in z-index order, as in
                        // block flow (an input's icon before it in the tree, `z-10`).
                        out_of_flow.extend(self.build(id).map(|el| (z, el)));
                        continue;
                    }
                    if position == Some(gpui::Position::Relative) {
                        out_of_flow.sort_by_key(|(z, _)| *z);
                        let below = out_of_flow.partition_point(|(oz, _)| *oz <= z);
                        items.extend(out_of_flow.drain(..below).map(|(_, el)| el));
                    }
                    if let Some((spacer, end)) = self.culled(list, &mut next_run, i - 1, ids) {
                        items.push(spacer);
                        i = end;
                        continue;
                    }
                    items.extend(self.build(id));
                    self.in_list.set(None);
                }
            }
        }
        items.extend(text.finish().map(IntoElement::into_any_element));
        out_of_flow.sort_by_key(|(z, _)| *z);
        items.extend(out_of_flow.into_iter().map(|(_, el)| el));
        items
    }

    /// Text, or a plain `display: inline` element whose content is also phrasing.
    fn is_phrasing(&self, id: u64) -> bool {
        let node = self.tree.get(id);
        if node.is_text() {
            return true;
        }
        let Some(c) = self.styles.get(&id) else {
            return true;
        };
        c.disp == Disp::None
            || (c.disp == Disp::Inline
                // Out-of-flow boxes (e.g. `sr-only`) aren't part of the line's text.
                && c.base.position != Some(gpui::Position::Absolute)
                && !node.props.listens("click")
                && !matches!(node.tag.as_str(), "input" | "textarea" | "img")
                && c.hover.is_none()
                && self.kids(id).into_iter().all(|ch| self.is_phrasing(ch)))
    }

    /// A text node's text under its element's `text-transform`.
    fn cased<'a>(&self, id: u64, text: &'a str) -> std::borrow::Cow<'a, str> {
        let parent = self.tree.get(id).parent.and_then(|p| self.styles.get(&p));
        parent.map_or(text.into(), |c| c.case.apply(text))
    }

    /// A text node's `white-space`, its element's.
    fn ws(&self, id: u64) -> Ws {
        let parent = self.tree.get(id).parent.and_then(|p| self.styles.get(&p));
        parent.map_or(Ws::Collapse, |c| c.ws)
    }

    fn runs(&self, id: u64, style: HighlightStyle, p: &mut Paragraph) {
        let node = self.tree.get(id);
        if let Some(text) = &node.text {
            p.push_node(Some(id), &self.cased(id, text), self.ws(id), style);
            return;
        }
        let Some(c) = self.styles.get(&id) else {
            return;
        };
        if c.disp == Disp::None {
            return;
        }
        if node.tag == "br" {
            return p.line_break(Some(id));
        }
        if let (Some(t), Some(parent)) = (&c.base.text, node.parent)
            && t.font_family.is_some()
            && self.font_of(id).1 != self.font_of(parent).1
        {
            warn_once("font-family on inline text inside a paragraph");
        }
        let style = c
            .base
            .text
            .as_ref()
            .map_or(style, |t| style.highlight(highlight(t)));
        for child in self.kids(id) {
            self.runs(child, style, p);
        }
    }

    /// A list item's marker text: `list-style-type` set here or inherited (`disc` initially),
    /// numbered by the list items before it, from the list's `start`; and whether it is inside.
    fn marker(&self, id: u64) -> Option<(String, bool)> {
        if !self.styles.get(&id)?.list.item {
            return None;
        }
        let up = || std::iter::successors(Some(id), |&n| self.tree.get(n).parent).filter_map(|n| self.styles.get(&n));
        let marker = up().find_map(|s| s.list.marker.clone()).unwrap_or(crate::css::props::Marker::Disc);
        let inside = up().find_map(|s| s.list.inside).unwrap_or(false);
        let parent = self.tree.get(id).parent?;
        let start = self.tree.get(parent).props.attrs.get("start").and_then(|v| v.trim().parse().ok()).unwrap_or(1);
        let before = self
            .kids(parent)
            .into_iter()
            .take_while(|&k| k != id)
            .filter(|k| self.styles.get(k).is_some_and(|s| s.list.item && s.disp != Disp::None))
            .count() as i32;
        let value = self.tree.get(id).props.attrs.get("value").and_then(|v| v.trim().parse().ok());
        Some((marker.label(value.unwrap_or(start + before))?, inside))
    }

    /// Inline text in `id` that one gpui text can't draw: another font size, raised or lowered
    /// (`sub`, `super`), or in a span with a box of its own (a background, padding or border).
    fn needs_words(&self, id: u64) -> bool {
        use crate::css::props::VAlign;
        let node = self.tree.get(id);
        let (Some(c), Some(parent)) = (self.styles.get(&id), node.parent) else {
            return false;
        };
        if node.is_text() || c.disp == Disp::None {
            return false;
        }
        matches!(c.table.valign, Some(VAlign::Sub | VAlign::Super))
            || inline_box(&c.base)
            || (c.base.text.as_ref().is_some_and(|t| t.font_size.is_some())
                && self.font_of(id).0 != self.font_of(parent).0)
            || self.kids(id).into_iter().any(|k| self.needs_words(k))
    }

    /// Phrasing that one gpui text can't draw (see `needs_words`): each word is its own box in a
    /// wrapping row, on a shared baseline, so lines still break between words. A span's box
    /// paints behind its words, its start and end edges on its first and last.
    fn word_boxes(&self, ids: &[u64]) -> AnyElement {
        use crate::css::props::VAlign;
        struct Segment {
            text: String,
            style: TextStyleRefinement,
            /// How far it is lowered, in px.
            shift: f32,
            /// The spans with boxes it is in, outermost first.
            boxes: Vec<u64>,
        }
        fn walk(r: &Root, id: u64, parent: &Segment, out: &mut Vec<Segment>) {
            let node = r.tree.get(id);
            let here = |text: String| Segment { text, style: parent.style.clone(), shift: parent.shift, boxes: parent.boxes.clone() };
            if let Some(text) = &node.text {
                return out.push(here(r.cased(id, text).into_owned()));
            }
            let Some(c) = r.styles.get(&id) else { return };
            if c.disp == Disp::None {
                return;
            }
            if node.tag == "br" {
                return out.push(here("\n".into()));
            }
            let mut seg = here(String::new());
            if let Some(t) = &c.base.text {
                seg.style.refine(t);
            }
            let parent_px = node.parent.map_or(REM, |p| r.font_of(p).0.map_or(REM, |l| f32::from(l.to_pixels(px(REM)))));
            seg.shift += match c.table.valign {
                Some(VAlign::Sub) => 0.2 * parent_px,
                Some(VAlign::Super) => -0.34 * parent_px,
                _ => 0.0,
            };
            if inline_box(&c.base) {
                seg.boxes.push(id);
            }
            for k in r.kids(id) {
                walk(r, k, &seg, out);
            }
        }
        let mut segments = Vec::new();
        let root = Segment { text: String::new(), style: TextStyleRefinement::default(), shift: 0.0, boxes: Vec::new() };
        for &id in ids {
            walk(self, id, &root, &mut segments);
        }
        // Words with the space after them; `None` ends the row (`<br>`).
        let mut words: Vec<Option<Segment>> = Vec::new();
        for seg in segments {
            if seg.text == "\n" {
                words.push(None);
                continue;
            }
            let after_word = matches!(words.last(), Some(Some(w)) if !w.text.ends_with(' '));
            if seg.text.starts_with(char::is_whitespace) && after_word {
                match words.last_mut() {
                    // Within the same spans, the space ends the word before; past a span's end, it
                    // is outside the span's box.
                    Some(Some(w)) if w.boxes == seg.boxes => w.text.push(' '),
                    _ => words.push(Some(Segment { text: " ".into(), style: seg.style.clone(), shift: 0.0, boxes: Vec::new() })),
                }
            }
            let ends_spaced = seg.text.ends_with(char::is_whitespace);
            let mut parts = seg.text.split_whitespace().peekable();
            while let Some(word) = parts.next() {
                let spaced = parts.peek().is_some() || ends_spaced;
                let text = if spaced { format!("{word} ") } else { word.to_owned() };
                words.push(Some(Segment { text, style: seg.style.clone(), shift: seg.shift, boxes: seg.boxes.clone() }));
            }
        }
        let (mut first, mut last) = (HashMap::new(), HashMap::new());
        for (i, w) in words.iter().enumerate() {
            for &b in w.iter().flat_map(|w| &w.boxes) {
                first.entry(b).or_insert(i);
                last.insert(b, i);
            }
        }
        let items = words.into_iter().enumerate().map(|(i, w)| {
            // A full-width empty box ends the row.
            let Some(w) = w else {
                return div().w_full().h_0().into_any_element();
            };
            let mut el = div().flex_none().relative().child(StyledText::new(w.text));
            let s = el.style();
            s.text = Some(w.style);
            if w.shift != 0.0 {
                s.inset.top = Some(px(w.shift).into());
            }
            for b in &w.boxes {
                let c = &self.styles[b].base;
                s.background = c.background.clone().or(s.background.take());
                s.border_color = c.border_color.or(s.border_color);
                // Vertical padding and borders paint around the line without moving it.
                let (top, bottom) = (c.padding.top, c.padding.bottom);
                s.padding.top = top;
                s.padding.bottom = bottom;
                s.margin.top = top.map(|t| negate(t).into());
                s.margin.bottom = bottom.map(|b| negate(b).into());
                s.border_widths.top = c.border_widths.top;
                s.border_widths.bottom = c.border_widths.bottom;
                if first[b] == i {
                    s.padding.left = c.padding.left;
                    s.border_widths.left = c.border_widths.left;
                    s.corner_radii.top_left = c.corner_radii.top_left;
                    s.corner_radii.bottom_left = c.corner_radii.bottom_left;
                    s.margin.left = c.margin.left;
                }
                if last[b] == i {
                    s.padding.right = c.padding.right;
                    s.border_widths.right = c.border_widths.right;
                    s.corner_radii.top_right = c.corner_radii.top_right;
                    s.corner_radii.bottom_right = c.corner_radii.bottom_right;
                    s.margin.right = c.margin.right;
                }
            }
            el.into_any_element()
        });
        let align = std::iter::successors(ids.first().and_then(|&i| self.tree.get(i).parent), |&n| self.tree.get(n).parent)
            .find_map(|n| self.styles.get(&n)?.base.text.as_ref()?.text_align);
        let row = div().flex().flex_row().flex_wrap().items_baseline();
        let row = match align {
            Some(gpui::TextAlign::Center) => row.justify_center(),
            Some(gpui::TextAlign::Right) => row.justify_end(),
            _ => row,
        };
        row.children(items).into_any_element()
    }

    /// The font size and family in effect at `id`: the nearest ones set at or above it.
    fn font_of(&self, id: u64) -> (Option<AbsoluteLength>, Option<SharedString>) {
        let texts = std::iter::successors(Some(id), |&n| self.tree.get(n).parent)
            .filter_map(|n| self.styles.get(&n)?.base.text.as_ref());
        let size = texts.clone().find_map(|t| t.font_size);
        (size, texts.into_iter().find_map(|t| t.font_family.clone()))
    }
}

/// Typing into editable content: the IME reads the caret's text node, and what it inserts goes to
/// JS (`textinput`, `composition`), which edits the DOM and sends the new selection back.
impl gpui::EntityInputHandler for Root {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let units: Vec<u16> = self.tree.nodes.get(&self.edit.caret?.0)?.text.as_ref()?.encode_utf16().collect();
        let range = range.start.min(units.len())..range.end.min(units.len());
        adjusted.replace(range.clone());
        Some(String::from_utf16_lossy(&units[range]))
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<gpui::UTF16Selection> {
        let (node, offset) = self.edit.caret?;
        // What is selected in the caret's node, when the selection is all in it.
        let range = match self.edit.ranges.as_slice() {
            [(n, a, b)] if *n == node => *a..*b,
            _ => offset..offset,
        };
        Some(gpui::UTF16Selection { range, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone()
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: &mut Window, _: &mut Context<Self>) {
        let Some(host) = self.edit.host else { return };
        // A replacement range outside a composition: an accent picked for the letter before.
        let replace = range.filter(|_| self.marked.is_none()).map(|r| [r.start, r.end]);
        self.marked = None;
        self.bridge.event(host, "textinput", json!({ "text": text, "replace": replace }));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let Some(host) = self.edit.host else { return };
        let start = (self.marked.as_ref().map(|m| m.start))
            .or(range.map(|r| r.start))
            .or(self.edit.caret.map(|c| c.1))
            .unwrap_or(0);
        self.marked = Some(start..start + text.encode_utf16().count());
        self.bridge.event(host, "composition", json!({ "text": text }));
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        editing::caret_bounds(&self.editables.borrow(), self.edit.caret?)
    }

    fn character_index_for_point(&mut self, _: gpui::Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An injected input's time runs until the GPU has finished the first frame drawn with all
        // it changed: a recent draw's, or this one's.
        if let Some(bench) = &mut self.bench {
            bench
                .draws
                .push_back((Instant::now(), gpui::profile::frames_submitted() + 1));
            if bench.draws.len() > 32 {
                bench.draws.pop_front();
            }
            for (id, start, effects) in std::mem::take(&mut bench.handled) {
                let frame = bench
                    .draws
                    .iter()
                    .find(|(drawn, _)| *drawn >= effects)
                    .map(|d| d.1);
                let Some(frame) = frame else { continue };
                let replies = bench.replies.clone();
                cx.spawn(async move |_, cx| {
                    let deadline = Instant::now() + Duration::from_secs(2);
                    let ms = loop {
                        if let Some(done) = gpui::profile::frame_completed_at(frame) {
                            break Some(done.duration_since(start).as_secs_f64() * 1000.);
                        }
                        if Instant::now() > deadline {
                            break None;
                        }
                        cx.background_executor()
                            .timer(Duration::from_micros(250))
                            .await;
                    };
                    replies.send(json!({ "id": id, "ms": ms }));
                })
                .detach();
            }
        }
        let size = window.viewport_size();
        let env = Env {
            width: size.width.into(),
            height: size.height.into(),
            dark: matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        };
        let resized = std::mem::take(&mut *self.resized_containers.borrow_mut());
        if !resized.is_empty() {
            self.changes.props.extend(resized);
            self.dirty = true;
        }
        // JS's `matchMedia("(prefers-color-scheme: dark)")`, from the first frame on.
        if self.env.map(|e| e.dark) != Some(env.dark) {
            self.bridge
                .event(HTML, "appearance", json!({ "dark": env.dark }));
        }
        if self.dirty || self.env != Some(env) {
            self.sync_inputs(window, cx);
            if self.env != Some(env) {
                self.style_cache = StyleCache::default();
                self.changes.all = true;
            }
            let changes = std::mem::take(&mut self.changes);
            if !changes.is_empty() {
                // What transitioning elements show now, so a restyle that changes them (a class
                // or attribute flip, the theme) transitions from there, as in a browser. Elements
                // whose props changed too: a transition can arrive with the change it animates
                // (vaul ends a drag with `transition: none` and sets one with the new transform).
                let changed: IdSet = changes.props.iter().copied().collect();
                let before: Vec<(u64, Look, Look)> = changed
                    .union(&self.transitioning)
                    .filter_map(|&id| Some((id, self.styles.get(&id)?)))
                    .map(|(id, c)| {
                        (
                            id,
                            Look::of(&self.shown(id, c)).resolved(c.color),
                            Look::of(self.target(id, c)).resolved(c.color),
                        )
                    })
                    .collect();
                self.remove_freed_styles(&changes.gone);
                let (cache, text) = (&self.font_ch, cx.text_system());
                // One family at a time, as stylesheets name them: listing every installed family
                // up front took ~450ms of launch.
                let fonts = &|f: &str| {
                    *cache.borrow_mut().entry(f.to_owned()).or_insert_with(|| {
                        let id = text.font_id(&gpui::font(f.to_owned())).ok()?;
                        Some(text.advance(id, px(100.0), '0').map_or(0.5, |s| f32::from(s.width) / 100.0))
                    })
                };
                let restyled = gpui::profile::time("restyle", || {
                    restyle_in_place(
                        &self.tree,
                        &self.sheet,
                        &env,
                        fonts,
                        &self.style_cache,
                        &self.container_sizes.borrow(),
                        &mut self.styles,
                        &changes,
                    )
                });
                let mut cull = self.cull.borrow_mut();
                if changes.all {
                    cull.kids.clear();
                }
                cull.invalidate(&self.tree, restyled.changed, &|n| {
                    self.flows(n) && self.cullable(n)
                });
                for id in &changes.gone {
                    cull.kids.remove(id);
                    cull.lists.remove(id);
                }
                drop(cull);
                let styles = &self.styles;
                self.container_sizes
                    .borrow_mut()
                    .retain(|id, _| styles.get(id).is_some_and(|c| c.container));
                if changes.all {
                    self.track_all_styles();
                } else {
                    let ids = restyled.visited.into_iter().chain(changes.gone);
                    for id in ids {
                        self.track_style(id);
                    }
                }
                let now = Instant::now();
                for (id, shown, old_target) in before {
                    let Some((c, tr)) = self
                        .styles
                        .get(&id)
                        .and_then(|c| Some((c, c.transition.as_ref()?)))
                    else {
                        continue;
                    };
                    if old_target.moves(&Look::of(self.target(id, c)).resolved(c.color), tr) {
                        self.states.entry(id).or_default().changed = Some((now, shown));
                    }
                }
            }
            self.env = Some(env);
            self.dirty = false;
        }
        if let Some(target) = self.pending_focus.take() {
            self.apply_focus(target, window, cx);
        }
        self.poll_media(window);
        for image in self.stale_images.drain(..) {
            let _ = window.drop_image(image);
        }
        self.animating.set(false);
        self.scale.set(window.scale_factor());
        self.frame.set(self.frame.get() + 1);
        self.cull.borrow_mut().frame = self.frame.get();
        let focused = self
            .states
            .iter()
            .filter(|(_, s)| s.focused)
            .map(|(&id, _)| id);
        // gpui reports focus a frame late; an input it has focused already must stay rendered,
        // or gpui finds the focus gone and blurs it.
        let native = self
            .inputs
            .iter()
            .filter(|(_, i)| i.focus.is_focused(window));
        let keep: Vec<u64> = focused
            .chain(native.map(|(&id, _)| id))
            .chain(self.hovered.borrow().iter().copied())
            .chain(std::mem::take(&mut *self.animated.borrow_mut()))
            .chain(self.js_focused)
            .chain(self.open_select)
            .chain(self.svg_hover.iter().copied())
            .chain(self.reported.down_target.get())
            .collect();
        self.keep = cull::with_ancestors(&self.tree, keep);
        // Containing blocks to measure this draw: the fixed boxes' transformed ancestors.
        {
            let wanted: IdSet = self
                .styles
                .iter()
                .filter(|(_, c)| c.fixed && c.disp != Disp::None)
                .filter_map(|(&id, _)| self.fixed_containing_block(id))
                .collect();
            let mut blocks = self.containing_blocks.borrow_mut();
            blocks.retain(|id, _| wanted.contains(id));
            for id in wanted {
                blocks.entry(id).or_insert(None);
            }
        }
        self.editables.borrow_mut().clear();
        let page = gpui::profile::time("build", || self.build(HTML))
            .unwrap_or_else(|| div().into_any_element());
        let layers = gpui::profile::time("build", || self.fixed_layers());
        // Images this frame didn't draw (an animation's past frames) give back their GPU memory.
        let frame = self.frame.get();
        self.svgs.borrow_mut().retain(|_, (image, drawn)| {
            let keep = drawn.get() == frame;
            if let (false, Some(image)) = (keep, image) {
                let _ = window.drop_image(image.clone());
            }
            keep
        });
        // Changed SVGs drawn off the UI thread, one at a time each: when its layers are ready it
        // shows them, and the next frame starts on that SVG's newest markup if it moved on meanwhile.
        for (id, layers, all) in self.svg_jobs.take() {
            if !self.svg_drawing.borrow_mut().insert(id) {
                continue;
            }
            let rasterize = self.rasterize.clone();
            let drawing = cx.background_executor().spawn(async move {
                let images: Vec<_> = layers.iter().map(|m| rasterize_svg(&rasterize, m)).collect();
                layers.into_iter().zip(images).collect::<Vec<_>>()
            });
            cx.spawn(async move |this, cx| {
                let drawn = drawing.await;
                let _ = this.update(cx, |root, cx| {
                    root.svg_drawing.borrow_mut().remove(&id);
                    let mut svgs = root.svgs.borrow_mut();
                    for (markup, image) in drawn {
                        svgs.insert(markup, (image, Cell::new(root.frame.get())));
                    }
                    // The newest picture drawn shows until the next is, even if the svg moved on
                    // meanwhile (an animation's next frame), rather than the one before it.
                    // (Not for an svg freed meanwhile, which `svg_shown` dropped.)
                    let mut shown = root.svg_shown.borrow_mut();
                    if shown.contains_key(&id) && all.iter().all(|m| svgs.contains_key(m)) {
                        shown.insert(id, all);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let (down, up, pressed, moved, wheel) = (
            self.reported.clone(),
            self.reported.clone(),
            self.reported.clone(),
            self.reported.clone(),
            self.reported.clone(),
        );
        let (this, bridge) = (self.this.clone(), self.bridge.clone());
        let (bridge_up, bridge_move) = (self.bridge.clone(), self.bridge.clone());
        let bridge_present = self.bridge.clone();
        // `document`'s view of the mouse: window-level listeners, so boxes that block the pointer
        // (overlays) can't hide events from it. Registered first, they see every event first in
        // the capture phase and last in the bubble phase.
        // A focused editing host's typed and composed text comes to `Root` (see `EntityInputHandler`).
        let typing = self
            .js_focused
            .filter(|&id| self.is_editing_host(id))
            .and_then(|_| self.this.upgrade())
            .map(|root| (self.edit_focus.clone(), root));
        let document = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                if let Some((focus, root)) = typing {
                    window.handle_input(&focus, gpui::ElementInputHandler::new(bounds, root), cx);
                }
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| match phase {
                    DispatchPhase::Capture => {
                        down.keyboard.set(false);
                        down.down.set(false);
                        down.down_target.set(None);
                        down.on_select.set(false);
                        down.region.set(false);
                        down.context.set(false);
                        down.editing.set(false);
                        // `document` hears every press (Radix dismisses on outside presses). JS
                        // tells it right after an element's own events; ones no element reported
                        // are sent here once the press has gone everywhere.
                        let (down, bridge) = (down.clone(), bridge.clone());
                        window.defer(cx, move |window, _| {
                            if !down.down.get() {
                                let target = down.down_target.get();
                                bridge.document_from(window, target, &["pointerdown", "mousedown"]);
                            }
                        });
                    }
                    // A press that gets here missed every input: blur, as a browser does.
                    DispatchPhase::Bubble => {
                        if pressed.editing.get() {
                            return;
                        }
                        window.blur();
                        let _ = this.update(cx, |root, cx| {
                            root.blur_inputs(cx);
                            // Editable content loses focus to a press outside it; JS hears it.
                            if let Some(host) = root.js_focused.filter(|&id| root.is_editing_host(id)) {
                                root.js_focused = None;
                                root.bridge.event(host, "blur", json!({}));
                            }
                            // A press outside an open select's list dismisses it.
                            if !pressed.on_select.get() && root.open_select.take().is_some() {
                                cx.notify();
                            }
                        });
                    }
                });
                let bridge = bridge_up;
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    up.up.set(false);
                    up.click.set(false);
                    up.dblclick.set(false);
                    let (up, bridge) = (up.clone(), bridge.clone());
                    window.defer(cx, move |window, _| {
                        let mut events = Vec::new();
                        if !up.up.get() {
                            events.extend(["pointerup", "mouseup"]);
                        }
                        // shortcut: a release no element turned into a click still clicks `document`,
                        // even after a drag; fine for dismiss logic, the only listener this serves.
                        if !up.click.get() {
                            events.push("click");
                        }
                        // From the pressed box: a click there may check a box or its label's radio.
                        if !events.is_empty() {
                            bridge.document_from(window, up.down_target.get(), &events);
                        }
                    });
                });
                // `document` tracks the pointer too (Radix keeps a tooltip open while the pointer
                // travels from its trigger to it). Moves no element reported are sent here, and
                // the flag is cleared for the next one.
                let bridge = bridge_move;
                window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, _| {
                    if phase == DispatchPhase::Bubble && !moved.moved.replace(false) {
                        bridge.document(window, &["pointermove", "mousemove"])
                    }
                });
                let wheel = wheel.clone();
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, _| {
                    if phase == DispatchPhase::Capture {
                        wheel.wheel.set(false);
                    }
                });
                present_after::<MouseDownEvent>(window, &bridge_present);
                present_after::<MouseUpEvent>(window, &bridge_present);
                present_after::<MouseMoveEvent>(window, &bridge_present);
                present_after::<ScrollWheelEvent>(window, &bridge_present);
            },
        );
        let root = div()
            .relative()
            .size_full()
            .child(document.absolute().size_full())
            .child(page)
            .children(layers)
            // Prepaints after every probe above, raised boxes' included, so a sync gets this layout's rects.
            .child({
                let this = self.this.clone();
                gpui::deferred(canvas(
                    move |_, window, cx| {
                        // A draw to be redone has stale rects; the redraw answers.
                        if window.redraw_pending() {
                            return;
                        }
                        let _ = this.update(cx, |root, _| {
                            root.answer_sync();
                            root.drop_garbage();
                        });
                    },
                    |_, _, _, _| {},
                ))
                .with_priority(usize::MAX)
            });
        if self.animating.get() {
            window.request_animation_frame();
        }
        root
    }
}

/// A replaced element's natural size for what CSS leaves auto: both, or the other side at its ratio.
fn natural_size(style: &mut StyleRefinement, (width, height): (f32, f32)) {
    let auto = |l: &Option<Length>| matches!(l, None | Some(Length::Auto));
    match (auto(&style.size.width), auto(&style.size.height)) {
        (true, true) => {
            style.size.width = Some(px(width).into());
            style.size.height = Some(px(height).into());
        }
        (false, false) => {}
        _ if height > 0.0 => style.aspect_ratio = Some(width / height),
        _ => {}
    }
}

/// gpui's key name -> the DOM's `KeyboardEvent.key`.
fn dom_key(key: &str, typed: Option<&str>) -> String {
    let named = match key {
        "escape" => "Escape",
        "enter" => "Enter",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "space" => " ",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        _ => return typed.filter(|t| !t.is_empty()).unwrap_or(key).to_owned(),
    };
    named.to_owned()
}

/// Transparent shadows are placeholders kept for transitions; there is nothing to paint.
/// Whether a box draws nothing of its own: no background, border or shadow.
fn paints_nothing(s: &StyleRefinement) -> bool {
    let b = &s.border_widths;
    s.background
        .as_ref()
        .is_none_or(|f| f.color().is_none_or(|c| c.is_transparent()))
        && [b.top, b.right, b.bottom, b.left]
            .iter()
            .all(|w| w.is_none_or(|w| w.is_zero()))
        && [&s.box_shadow, &s.inset_shadow]
            .iter()
            .all(|v| v.as_ref().is_none_or(|v| v.iter().all(|sh| sh.color.a == 0.0)))
}

fn visible_shadows(mut s: StyleRefinement) -> StyleRefinement {
    for shadows in [&mut s.box_shadow, &mut s.inset_shadow].into_iter().flatten() {
        shadows.retain(|s| s.color.a > 0.0);
    }
    s
}

/// The `position: fixed` boxes under `id` (`fixed` is `None` for one not shown), in document order
/// with the z-index each one's layer stacks by. One inside another is still placed against the
/// window and unclipped, but stacks in that one's context, just before it: a modal's backdrop
/// above the page and under its panel.
/// shortcut: a nested fixed box paints under all of its ancestor's content, even what precedes it.
fn fixed_order(
    id: u64,
    outer: Option<i32>,
    fixed: &dyn Fn(u64) -> Option<bool>,
    z_of: &dyn Fn(u64) -> Option<i32>,
    children_of: &dyn Fn(u64) -> Vec<u64>,
    out: &mut Vec<(u64, i32)>,
) {
    let Some(is_fixed) = fixed(id) else {
        return;
    };
    let z = match is_fixed {
        true => Some(outer.unwrap_or_else(|| layer_z(id, z_of, children_of))),
        false => outer,
    };
    for kid in children_of(id) {
        fixed_order(kid, z, fixed, z_of, children_of, out);
    }
    if let (true, Some(z)) = (is_fixed, z) {
        out.push((id, z));
    }
}

/// A deferred draw's priority for a z-index, in the same order.
/// The top layer's paint priority: over every z-index.
// shortcut: modal dialogs stack in document order, not the order they were opened.
const TOP_LAYER: usize = z_priority(i32::MAX) + 1;

const fn z_priority(z: i32) -> usize {
    (z as i64 - i32::MIN as i64) as usize
}

/// The z-index a fixed layer stacks by. One without its own takes the nearest z-index inside it:
/// Radix copies its content's `getComputedStyle(content).zIndex` onto the fixed popper wrapper,
/// which gpuijs can't answer from JS, so a tooltip's `z-50` would otherwise sink below a `z-50`
/// toolbar painted before it.
fn layer_z(
    id: u64,
    z_of: &dyn Fn(u64) -> Option<i32>,
    children_of: &dyn Fn(u64) -> Vec<u64>,
) -> i32 {
    let mut queue = std::collections::VecDeque::from([id]);
    while let Some(n) = queue.pop_front() {
        if let Some(z) = z_of(n) {
            return z;
        }
        queue.extend(children_of(n));
    }
    0
}

/// The outline of a `w`×`h` box with corner radius `r`, about its center, as CSS
/// `rotate(deg) scale(sx, sy) translate(offset)` places it.
fn box_outline(
    (w, h): (f32, f32),
    r: f32,
    deg: f32,
    (sx, sy): (f32, f32),
    (ox, oy): (f32, f32),
) -> Vec<(f32, f32)> {
    const ARC_STEPS: usize = 6;
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (hw, hh) = (w / 2.0 - r, h / 2.0 - r);
    // Corners clockwise from bottom-right (y points down), each an arc about its own center.
    let corners = [(hw, hh), (-hw, hh), (-hw, -hh), (hw, -hh)];
    let (sin, cos) = deg.to_radians().sin_cos();
    let mut out = Vec::with_capacity(4 * (ARC_STEPS + 1));
    for (i, (cx, cy)) in corners.into_iter().enumerate() {
        for step in 0..=ARC_STEPS {
            let a = (i as f32 + step as f32 / ARC_STEPS as f32) * std::f32::consts::FRAC_PI_2;
            let (x, y) = ((cx + r * a.cos() + ox) * sx, (cy + r * a.sin() + oy) * sy);
            out.push((x * cos - y * sin, x * sin + y * cos));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Paragraph, appended, box_outline, counter_value, data_image, decode_base64, fixed_order, layer_z, z_priority};
    use crate::css::props::Ws;
    use gpui::HighlightStyle;

    #[test]
    fn paragraphs_keep_whitespace_as_white_space_says() {
        let text = |parts: &[(&str, Ws)]| {
            let mut p = Paragraph::default();
            for (t, ws) in parts {
                p.push(t, *ws, HighlightStyle::default());
            }
            p.text
        };
        assert_eq!(text(&[(" a \n  b ", Ws::Collapse)]), "a b ");
        assert_eq!(text(&[(" a  \n  b", Ws::Lines)]), "a\nb");
        assert_eq!(text(&[("  a \n\tb", Ws::Preserve)]), "  a \n\tb");
        assert_eq!(text(&[("x ", Ws::Collapse), ("  y", Ws::Preserve)]), "x   y");
    }

    #[test]
    fn a_line_break_ends_the_line_and_its_trailing_space() {
        let mut p = Paragraph::default();
        p.push("multi ", Ws::Collapse, HighlightStyle::default());
        p.line_break(None);
        p.push(" line", Ws::Collapse, HighlightStyle::default());
        assert_eq!(p.text, "multi\nline");
    }

    #[test]
    fn editable_paragraphs_map_each_nodes_offsets_to_bytes() {
        use crate::editing::Segment;
        let mut p = Paragraph { segments: Some(Vec::new()), ..Default::default() };
        p.push_node(Some(1), "a  b", Ws::Collapse, HighlightStyle::default());
        p.push_node(Some(2), "é", Ws::Collapse, HighlightStyle::default());
        p.line_break(Some(3));
        p.push_node(Some(4), "c", Ws::Collapse, HighlightStyle::default());
        assert_eq!(p.text, "a bé\nc");
        let segments = p.segments.clone().unwrap();
        // The collapsed second space shares the first's byte; "é" is two bytes, one UTF-16 unit.
        assert_eq!(segments[0], Segment { node: 1, bytes: vec![0, 1, 2, 2, 3] });
        assert_eq!(segments[1], Segment { node: 2, bytes: vec![3, 5] });
        assert_eq!(segments[2], Segment { node: 3, bytes: vec![5] });
        assert_eq!(segments[3], Segment { node: 4, bytes: vec![6, 7] });
        // A block of only a line break keeps its one empty line.
        let mut empty = Paragraph::default();
        empty.line_break(None);
        assert!(empty.finish().is_some());
    }

    #[test]
    fn only_text_added_at_the_end_is_parsed_alone() {
        let s = |v: &[&str]| v.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        let old = s(&[".a{}", ".b{}"]);
        assert_eq!(appended(&old, &s(&[".a{}", ".b{}.c{}"])).as_deref(), Some(".c{}"));
        assert_eq!(appended(&old, &s(&[".a{}", ".b{}", ".d{}"])).as_deref(), Some("\n.d{}"));
        // A rule added to an earlier sheet comes before later ones: everything is parsed again.
        assert_eq!(appended(&old, &s(&[".a{}.c{}", ".b{}"])), None);
        assert_eq!(appended(&old, &s(&[".a{}"])), None);
        assert_eq!(appended(&s(&[".a{color:"]), &s(&[".a{color:red}"])), None);
        assert_eq!(appended(&[], &s(&[".a{}"])), None);
    }

    fn extent(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        let xs = points.iter().map(|p| p.0);
        let ys = points.iter().map(|p| p.1);
        (
            xs.clone().fold(f32::MAX, f32::min),
            xs.fold(f32::MIN, f32::max),
            ys.clone().fold(f32::MAX, f32::min),
            ys.fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn z_priorities_keep_z_order() {
        let p = [i32::MIN, -1, 0, 1, 99999, i32::MAX].map(z_priority);
        assert!(p.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn fixed_layers_without_a_z_index_take_their_contents() {
        // 1 > 2 > (3 z-50, 4 z-10); 5 has its own z-index 7 and a child 6 at z-90.
        let z_of = |id| match id {
            3 => Some(50),
            4 => Some(10),
            5 => Some(7),
            6 => Some(90),
            _ => None,
        };
        let children_of = |id| match id {
            1 => vec![2],
            2 => vec![3, 4],
            5 => vec![6],
            _ => vec![],
        };
        assert_eq!(layer_z(1, &z_of, &children_of), 50);
        assert_eq!(layer_z(5, &z_of, &children_of), 7);
        assert_eq!(layer_z(9, &z_of, &children_of), 0);
    }

    #[test]
    fn nested_fixed_boxes_stack_just_before_their_fixed_ancestor() {
        // TailAdmin's modal: 1 > (2 page, 3 fixed z-99999 > (4 fixed backdrop, 5 panel)), 6 fixed,
        // 7 hidden fixed.
        let fixed = |id| match id {
            3 | 4 | 6 => Some(true),
            7 => None,
            _ => Some(false),
        };
        let z_of = |id| (id == 3).then_some(99999);
        let children_of = |id| match id {
            1 => vec![2, 3, 6, 7],
            3 => vec![4, 5],
            _ => vec![],
        };
        let mut order = Vec::new();
        fixed_order(1, None, &fixed, &z_of, &children_of, &mut order);
        assert_eq!(order, [(4, 99999), (3, 99999), (6, 0)]);
    }

    #[test]
    fn counters_count_from_their_reset_in_document_order() {
        use crate::protocol::{Op, Props};
        use crate::tree::{BODY, Tree};
        // ol.c (from 4) > li, li > (ol.c > li, li), li: the inner list counts on its own.
        let css = ".c { counter-reset: n 4 } .i::before { counter-increment: n; content: counter(n) }";
        let sheet = crate::css::Stylesheet::parse(css).unwrap();
        let mut tree = Tree::default();
        let el = |id, class: &str| Op::El(id, "div".into(), Props { class: class.into(), ..Default::default() });
        for (id, class, parent) in [(2, "c", BODY), (3, "i", 2), (4, "i", 2), (5, "c", 4), (6, "i", 5), (7, "i", 5), (8, "i", 2)] {
            tree.apply(el(id, class));
            tree.apply(Op::Append(parent, id));
        }
        let env = crate::css::Env { width: 800.0, height: 600.0, dark: false };
        let styles = crate::css::cascade::compute(&tree, &sheet, &env, &|_| Some(0.5));
        let at = |id| counter_value(&tree, &styles, id, false, "n");
        assert_eq!([3, 4, 6, 7, 8].map(at), [5, 6, 5, 6, 7]);
    }

    #[test]
    fn data_urls_decode() {
        assert_eq!(decode_base64("aGk/Pw==").unwrap(), b"hi??");
        assert_eq!(decode_base64("YWJj").unwrap(), b"abc");
        // @tailwindcss/forms' checkbox tick.
        let tick = data_image("data:image/svg+xml,%3csvg viewBox='0 0 16 16' fill='white'%3e%3c/svg%3e").unwrap();
        assert_eq!(tick.format, gpui::ImageFormat::Svg);
        assert_eq!(tick.bytes, b"<svg viewBox='0 0 16 16' fill='white'></svg>");
        let png = data_image("data:image/png;base64,YWJj").unwrap();
        assert_eq!((png.format, png.bytes.as_slice()), (gpui::ImageFormat::Png, &b"abc"[..]));
        assert!(data_image("data:text/plain,hi").is_none());
    }

    #[test]
    fn box_outline_rotates_after_translating() {
        let near = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
            [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)]
                .iter()
                .all(|(x, y)| (x - y).abs() < 1e-3)
        };
        let plain = box_outline((4.0, 2.0), 1.0, 0.0, (1.0, 1.0), (0.0, 0.0));
        assert!(near(extent(&plain), (-2.0, 2.0, -1.0, 1.0)));
        // `rotate(90deg) translate(4px)`: the box ends up below its center, turned upright.
        let turned = box_outline((4.0, 2.0), 0.0, 90.0, (1.0, 1.0), (4.0, 0.0));
        assert!(near(extent(&turned), (-1.0, 1.0, 2.0, 6.0)));
    }
}

/// Counter `name`'s value at `owner`'s generated box: from the nearest `counter-reset` whose
/// scope it is in (the element, a previous sibling, or theirs up the tree), plus every
/// increment since, in document order.
// shortcut: a counter nobody resets counts from 0 at the root rather than at the element.
fn counter_value(tree: &Tree, styles: &IdMap<std::rc::Rc<Computed>>, owner: u64, after: bool, name: &str) -> i32 {
    let find = |x: u64, reset: bool| {
        let c = styles.get(&x)?.counters.as_ref()?;
        let list = if reset { &c.reset } else { &c.increment };
        list.iter().rev().find(|(n, _)| n == name).map(|(_, v)| *v)
    };
    let inc = |x: u64| find(x, false).unwrap_or(0);
    let own = |x: u64, with_after: bool| {
        inc(x) + inc(pseudo_id(x, Pseudo::Before)) + if with_after { inc(pseudo_id(x, Pseudo::After)) } else { 0 }
    };
    let children = |x: u64| tree.nodes.get(&x).map(|n| n.children.clone()).unwrap_or_default();
    // A subtree's increments, past any that start a counter of their own.
    fn subtree(x: u64, own: &dyn Fn(u64, bool) -> i32, reset: &dyn Fn(u64) -> bool, children: &dyn Fn(u64) -> Vec<u64>) -> i32 {
        if reset(x) {
            return 0;
        }
        own(x, true) + children(x).into_iter().map(|k| subtree(k, own, reset, children)).sum::<i32>()
    }
    let resets = |x: u64| find(x, true).is_some();
    let inside = |x: u64| children(x).into_iter().map(|k| subtree(k, &own, &resets, &children)).sum::<i32>();
    let mut total = own(owner, false) + if after { inside(owner) + inc(pseudo_id(owner, Pseudo::After)) } else { 0 };
    let mut at = owner;
    loop {
        if let Some(r) = find(at, true) {
            return r + total;
        }
        let Some(parent) = tree.nodes.get(&at).and_then(|n| n.parent) else {
            return total;
        };
        let siblings = children(parent);
        let before = siblings.iter().position(|&s| s == at).unwrap_or(0);
        for &s in siblings[..before].iter().rev() {
            if let Some(r) = find(s, true) {
                return r + own(s, true) + inside(s) + total;
            }
            total += subtree(s, &own, &resets, &children);
        }
        at = parent;
        total += own(at, false);
    }
}

/// A background image as a layer under the content, over the background color.
fn bg_image(c: &Computed) -> Option<gpui::Img> {
    let (url, at) = c.bg_image.as_ref()?;
    let mut layer = img(css_image(url)?)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .object_fit(at.fit)
        .object_position(point(at.at.0, at.at.1));
    layer.style().corner_radii = c.base.corner_radii.clone();
    Some(layer)
}

/// A progress, meter or range track: `height` of the box tall, centered, fully rounded, clipping
/// what fills it.
fn gauge_track(height: f32) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(gpui::relative((1.0 - height) / 2.0))
        .h(gpui::relative(height))
        .rounded_full()
        .overflow_hidden()
        .bg(gpui::rgb(0xefefef))
        .border(px(0.5))
        .border_color(gpui::rgb(0xb2b2b2))
}

/// A CSS `url()` image: a `data:` URL's, or a file's (see `css_asset`).
fn css_image(url: &str) -> Option<ImageSource> {
    match url.starts_with("data:") {
        true => data_image(url).map(ImageSource::from),
        false => css_asset(url).map(ImageSource::from),
    }
}

/// A `data:` URL's image, percent-encoded or base64; `None` (warned about) if gpui can't decode it.
fn data_image(url: &str) -> Option<Arc<Image>> {
    let (head, body) = url.strip_prefix("data:")?.split_once(',')?;
    let (head, base64) = match head.strip_suffix(";base64") {
        Some(head) => (head, true),
        None => (head, false),
    };
    let mime = head.split(';').next().unwrap_or("").trim();
    let Some(format) = ImageFormat::from_mime_type(mime) else {
        warn_once(&format!("data: URL of type {mime}"));
        return None;
    };
    let bytes = match base64 {
        true => decode_base64(body)?,
        false => percent_decode(body),
    };
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn percent_decode(text: &str) -> Vec<u8> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%').then(|| std::str::from_utf8(b.get(i + 1..i + 3)?).ok()).flatten();
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    out
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').map(value).collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &d)| n | (d as u32) << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// A CSS `url()` as a local file, as JS's `assetPath` resolves `<img src>`: root-relative URLs are
/// under the app's public/ dir (beside the packaged app's executable once built).
// shortcut: remote and `data:` URLs aren't loaded; JS downloads only `<img src>`.
fn css_asset(url: &str) -> Option<PathBuf> {
    if url.contains("://") {
        warn_once("url() that is remote");
        return None;
    }
    let public = std::env::var_os("GPUIJS_PUBLIC")
        .map(PathBuf::from)
        .or_else(|| Some(std::env::current_exe().ok()?.parent()?.parent()?.join("Resources/public")))?;
    let file = public.join(url.trim_start_matches('/'));
    match file.exists() {
        true => Some(file),
        false => Some(PathBuf::from(url)).filter(|f| f.exists()),
    }
}

/// Inline text with CSS whitespace collapsing and per-span highlight runs.
#[derive(Clone, Default)]
struct Paragraph {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    /// Some text kept its whitespace (`white-space: pre…`), so the end isn't trimmed.
    kept: bool,
    /// Has a `<br>`: a paragraph of only line breaks still takes a line.
    broke: bool,
    /// In editable content: where each node's text landed, to map caret offsets (see `editing`).
    segments: Option<Vec<Segment>>,
}

impl Paragraph {
    fn push(&mut self, text: &str, ws: Ws, style: HighlightStyle) {
        self.push_node(None, text, ws, style);
    }

    fn push_node(&mut self, node: Option<u64>, text: &str, ws: Ws, style: HighlightStyle) {
        let start = self.text.len();
        let mut bytes = Vec::new();
        let map = node.is_some() && self.segments.is_some();
        for c in text.chars() {
            if map {
                bytes.extend(std::iter::repeat_n(self.text.len(), c.len_utf16()));
            }
            let keep = match ws {
                Ws::Collapse => false,
                Ws::Lines => c == '\n',
                Ws::Preserve => true,
            };
            if keep || !c.is_whitespace() {
                if c == '\n' && ws == Ws::Lines {
                    self.text.truncate(self.text.trim_end_matches(' ').len());
                }
                self.text.push(c);
            } else if !self.text.is_empty() && !self.text.ends_with([' ', '\n']) {
                self.text.push(' ');
            }
        }
        self.kept |= ws != Ws::Collapse;
        if self.text.len() > start && style != HighlightStyle::default() {
            self.highlights.push((start..self.text.len(), style));
        }
        if let (Some(node), Some(segments)) = (node.filter(|_| map), &mut self.segments) {
            bytes.push(self.text.len());
            segments.push(Segment { node, bytes });
        }
    }

    /// `<br>`: the line ends here, whatever the white-space.
    fn line_break(&mut self, node: Option<u64>) {
        self.text.truncate(self.text.trim_end_matches(' ').len());
        if let (Some(node), Some(segments)) = (node, &mut self.segments) {
            segments.push(Segment { node, bytes: vec![self.text.len()] });
        }
        self.text.push('\n');
        self.broke = true;
    }

    fn finish(mut self) -> Option<StyledText> {
        // As in browsers, a final newline doesn't start another line: preformatted text's, or a
        // last `<br>`'s (`a<br><br>` is two lines).
        let len = match self.kept {
            true => self.text.strip_suffix('\n').unwrap_or(&self.text).len(),
            false => {
                let text = self.text.trim_end_matches(' ');
                text.strip_suffix('\n').unwrap_or(text).len()
            }
        };
        self.text.truncate(len);
        if self.text.is_empty() {
            if !self.broke {
                return None;
            }
            // An empty line's height (an editor's empty `<p><br></p>`).
            self.text.push('\u{200B}');
        }
        for (range, _) in &mut self.highlights {
            range.end = range.end.min(len);
        }
        self.highlights.retain(|(r, _)| !r.is_empty());
        Some(StyledText::new(SharedString::from(self.text)).with_highlights(self.highlights))
    }
}

/// An inline element that draws a box around its text: a background, padding or border.
fn inline_box(s: &StyleRefinement) -> bool {
    let (p, b) = box_edges(s);
    let painted = s.background.as_ref().is_some_and(|f| f.color().is_none_or(|c| !c.is_transparent()));
    painted || p.iter().chain(&b).any(|&e| e > 0.0)
}

/// `-l`, for a padding that must not move the line.
fn negate(l: DefiniteLength) -> DefiniteLength {
    match l {
        DefiniteLength::Absolute(AbsoluteLength::Pixels(p)) => DefiniteLength::Absolute(AbsoluteLength::Pixels(-p)),
        DefiniteLength::Absolute(AbsoluteLength::Rems(r)) => DefiniteLength::Absolute(AbsoluteLength::Rems(gpui::Rems(-r.0))),
        // shortcut: percent padding moves the line.
        _ => DefiniteLength::Absolute(AbsoluteLength::Pixels(px(0.0))),
    }
}

fn highlight(t: &TextStyleRefinement) -> HighlightStyle {
    HighlightStyle {
        color: t.color,
        font_weight: t.font_weight,
        font_style: t.font_style,
        background_color: t.background_color,
        underline: t.underline,
        strikethrough: t.strikethrough,
        fade_out: None,
    }
}

/// Padding and border widths in px (top, right, bottom, left), for `getComputedStyle`; lengths
/// other than px and rem count as 0.
fn box_edges(s: &StyleRefinement) -> ([f32; 4], [f32; 4]) {
    let abs = |l: Option<AbsoluteLength>| match l {
        Some(AbsoluteLength::Pixels(p)) => f32::from(p),
        Some(AbsoluteLength::Rems(r)) => r.0 * crate::css::props::REM,
        None => 0.0,
    };
    let def = |l: Option<DefiniteLength>| match l {
        Some(DefiniteLength::Absolute(a)) => abs(Some(a)),
        _ => 0.0,
    };
    let (p, b) = (&s.padding, &s.border_widths);
    (
        [def(p.top), def(p.right), def(p.bottom), def(p.left)],
        [abs(b.top), abs(b.right), abs(b.bottom), abs(b.left)],
    )
}
