//! Native text field (`<input>`, and `<textarea>` with wrapped lines), adapted from gpui's
//! `examples/input.rs`.
//!
//! The native side owns the text so typing never waits on JS. Each edit sends `change` with an
//! increasing `seq`; a `value` prop from JS is applied only when it changed and is not older than
//! the last edit we sent, which keeps controlled inputs from losing keystrokes or the cursor.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
    actions, div, fill, point, prelude::*, px, relative, size,
};
use serde_json::json;
use unicode_segmentation::*;

use crate::protocol::Props;
use crate::render::Bridge;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

const CONTEXT: &str = "TextInput";

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(CONTEXT);
    vec![
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
    ]
}

pub struct TextInput {
    id: u64,
    bridge: Bridge,
    props: Props,
    /// Seq of the last `change` sent to JS.
    seq: u64,
    focus_handle: FocusHandle,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    /// A `<textarea>`: lines wrap, and Enter and the arrows work across them.
    multiline: bool,
    last_layout: Option<Layout>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// `pointer-events: none`, set by the root as it builds the field's box: presses pass through.
    pub inert: Rc<Cell<bool>>,
    /// The `::placeholder` color, set by the root as it builds the field's box.
    pub placeholder_color: Rc<Cell<Option<gpui::Hsla>>>,
    /// `field-sizing: content`, set likewise.
    pub grows: Rc<Cell<bool>>,
}

impl TextInput {
    pub fn new(id: u64, props: Props, multiline: bool, bridge: Bridge, cx: &mut Context<Self>) -> Self {
        TextInput {
            id,
            bridge,
            content: props.value.clone().unwrap_or_default().into(),
            props,
            seq: 0,
            focus_handle: cx.focus_handle(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            multiline,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            inert: Rc::default(),
            placeholder_color: Rc::default(),
            grows: Rc::default(),
        }
    }

    pub fn set_props(&mut self, props: Props, cx: &mut Context<Self>) {
        if accepts_value(&self.props, &props, self.seq, &self.content) {
            self.content = props.value.clone().unwrap_or_default().into();
            let end = self.content.len();
            self.selected_range =
                self.selected_range.start.min(end)..self.selected_range.end.min(end);
            self.marked_range = None;
            cx.notify();
        }
        self.props = props;
    }

    fn edited(&mut self) {
        self.seq += 1;
        self.bridge.event(
            self.id,
            "change",
            json!({ "value": self.content.as_ref(), "seq": self.seq }),
        );
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        if self.multiline && !k.modifiers.platform && !k.modifiers.control {
            match k.key.as_str() {
                "enter" => self.replace_text_in_range(None, "\n", window, cx),
                // A row up or down, at the same x.
                "up" | "down" => {
                    if let Some(layout) = &self.last_layout {
                        let at = layout.position(self.cursor_offset());
                        let dy = if k.key == "up" { -0.5 } else { 1.5 } * layout.line_height;
                        let ix = layout.index(point(at.x, at.y + dy));
                        if k.modifiers.shift { self.select_to(ix, cx) } else { self.move_to(ix, cx) }
                    }
                }
                _ => {}
            }
        }
        let key = match event.keystroke.key.as_str() {
            "enter" => "Enter",
            "escape" => "Escape",
            "tab" => "Tab",
            "up" => "ArrowUp",
            "down" => "ArrowDown",
            _ => return,
        };
        // Always reported: JS bubbles it, and an ancestor may listen (cmdk's list navigation).
        self.bridge.event(self.id, "keydown", json!({ "key": key }));
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled {
            return;
        }
        window.focus(&self.focus_handle);
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline { text } else { text.replace('\n', " ") };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(layout)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        layout.index(position - bounds.origin).min(self.content.len())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

/// Whether a `value` prop from JS should overwrite the native text.
fn accepts_value(old: &Props, new: &Props, sent_seq: u64, content: &str) -> bool {
    new.value != old.value
        && new.seq.unwrap_or(0) >= sent_seq
        && new.value.as_deref().unwrap_or("") != content
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.edited();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.edited();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let (start, end) = (layout.position(range.start), layout.position(range.end));
        Some(Bounds::from_corners(
            bounds.origin + start,
            bounds.origin + point(end.x, end.y + layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let local = self.last_bounds?.localize(&point)?;
        let utf8_index = self.last_layout.as_ref()?.index(local).min(self.content.len());
        Some(self.offset_to_utf16(utf8_index))
    }
}

/// The shaped text: one wrapped line per `\n`-separated line, with its byte offset and top.
#[derive(Clone)]
struct Layout {
    lines: Vec<(usize, Pixels, WrappedLine)>,
    line_height: Pixels,
}

impl Layout {
    fn line_of(&self, ix: usize) -> Option<&(usize, Pixels, WrappedLine)> {
        self.lines.iter().rev().find(|(start, ..)| *start <= ix)
    }

    /// The top left of the caret before byte `ix`.
    fn position(&self, ix: usize) -> Point<Pixels> {
        let Some((start, top, line)) = self.line_of(ix) else {
            return Point::default();
        };
        let at = line.position_for_index(ix - start, self.line_height).unwrap_or_default();
        point(at.x, at.y + *top)
    }

    /// The byte offset closest to `p`.
    fn index(&self, p: Point<Pixels>) -> usize {
        let Some((start, top, line)) =
            self.lines.iter().rev().find(|(_, top, _)| *top <= p.y).or(self.lines.first())
        else {
            return 0;
        };
        let height = line.size(self.line_height).height;
        let y = (p.y - *top).clamp(px(0.0), (height - px(1.0)).max(px(0.0)));
        start + line.closest_index_for_position(point(p.x, y), self.line_height).unwrap_or_else(|ix| ix)
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    layout: Option<Layout>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let mut style = Style::default();
        // The box's size when it has one; else the field's own, as browsers size them.
        style.size.width = relative(1.).into();
        style.size.height = match input.multiline {
            true => relative(1.).into(),
            false => window.line_height().into(),
        };
        let attr = |k: &str, default: f32| {
            input.props.attrs.get(k).and_then(|v| v.trim().parse::<f32>().ok()).filter(|n| *n > 0.0).unwrap_or(default)
        };
        let text = window.text_style();
        let font_size = text.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        // shortcut: average character widths, 0.55em (sans) and 0.6em (monospace), not the font's.
        let width = match input.multiline {
            // Chrome counts a scrollbar's width in.
            true => font_size * 0.6 * attr("cols", 20.0) + px(15.0),
            false => font_size * 0.55 * attr("size", 20.0),
        };
        let rows = attr("rows", 2.0);
        let (multiline, grows) = (input.multiline, input.grows.get());
        let content = input.content.clone();
        let run = text.to_run(content.len());
        let layout = window.request_measured_layout(style, move |known, _, window, _| {
            let width = known.width.unwrap_or(width);
            let height = known.height.unwrap_or_else(|| match (multiline, grows) {
                (false, _) => line_height,
                (true, false) => line_height * rows,
                (true, true) => {
                    let lines = window
                        .text_system()
                        .shape_text(content.clone(), font_size, std::slice::from_ref(&run), Some(width), None)
                        .map_or(1, |l| l.iter().map(|l| l.wrap_boundaries().len() + 1).sum::<usize>());
                    line_height * lines.max(1) as f32
                }
            });
            size(width, height)
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        // Without a ::placeholder color, the text color at half opacity.
        let (display_text, text_color) = if content.is_empty() {
            let placeholder = input.props.placeholder.clone().unwrap_or_default();
            let color = input.placeholder_color.get().unwrap_or(style.color.opacity(0.5));
            (SharedString::from(placeholder), color)
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: style.letter_spacing,
            break_words: false,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        offset: None,
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        // An `<input>` is one line: a newline in its value shows as a space (same byte length).
        let (text, wrap) = match input.multiline {
            true => (display_text, Some(bounds.size.width)),
            false => (display_text.replace('\n', " ").into(), None),
        };
        let shaped = window
            .text_system()
            .shape_text(text, font_size, &runs, wrap, None)
            .unwrap_or_default();
        let (mut start, mut top, mut lines) = (0, px(0.0), Vec::new());
        for line in shaped {
            let (len, height) = (line.len(), line.size(line_height).height);
            lines.push((start, top, line));
            (start, top) = (start + len + 1, top + height.max(line_height));
        }
        let layout = Layout { lines, line_height };

        let at = |ix: usize| bounds.origin + layout.position(ix);
        let (selection, cursor) = if selected_range.is_empty() {
            let caret = Bounds::new(at(cursor), size(px(1.), line_height));
            (Vec::new(), Some(fill(caret, style.color)))
        } else {
            let (a, b) = (at(selected_range.start), at(selected_range.end));
            let row = |y: Pixels, from: Pixels, to: Pixels| {
                fill(Bounds::from_corners(point(from, y), point(to, y + line_height)), gpui::rgba(0x3b82f640))
            };
            let mut rows = vec![];
            if a.y == b.y {
                rows.push(row(a.y, a.x, b.x));
            } else {
                rows.push(row(a.y, a.x, bounds.right()));
                let mut y = a.y + line_height;
                while y < b.y {
                    rows.push(row(y, bounds.left(), bounds.right()));
                    y += line_height;
                }
                rows.push(row(b.y, bounds.left(), b.x));
            }
            (rows, None)
        };
        PrepaintState {
            layout: Some(layout),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection)
        }
        let layout = prepaint.layout.take().unwrap();
        for (_, top, line) in &layout.lines {
            let origin = bounds.origin + point(px(0.0), *top);
            line.paint(origin, layout.line_height, TextAlign::Left, None, window, cx)
                .ok();
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(layout);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let el = div().size_full().flex().key_context(CONTEXT);
        let el = if self.multiline { el.items_start() } else { el.items_center() };
        if self.props.disabled {
            return el.child(TextElement { input: cx.entity() });
        }
        let el = el
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_key_down(cx.listener(Self::on_key_down));
        if self.inert.get() {
            return el.child(TextElement { input: cx.entity() });
        }
        el.cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(value: &str, seq: u64) -> Props {
        Props {
            value: Some(value.into()),
            seq: Some(seq),
            ..Default::default()
        }
    }

    #[test]
    fn controlled_value_rules() {
        // Typed "a" then "ab" (seq 2); JS's render of seq 1 arrives late and must be ignored.
        assert!(!accepts_value(&props("", 0), &props("a", 1), 2, "ab"));
        // JS caught up: same text, nothing to do.
        assert!(!accepts_value(&props("a", 1), &props("ab", 2), 2, "ab"));
        // JS transformed the latest edit (uppercase): apply it.
        assert!(accepts_value(&props("", 0), &props("A", 1), 1, "a"));
        // JS cleared the field after submit.
        assert!(accepts_value(&props("hello", 5), &props("", 5), 5, "hello"));
        // An unchanged defaultValue-style prop never clobbers typed text.
        assert!(!accepts_value(&props("x", 0), &props("x", 0), 3, "xyz"));
    }
}
