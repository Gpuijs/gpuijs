//! Wire format between the Bun sidecar and the host: frames of a JSON `Msg`, of a commit in
//! the binary form `js/src/wire.js` writes (`decode_commit`), or of a canvas's pixels (`decode_pixels`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Msg {
    /// Replaces the app's stylesheet.
    Css { text: String },
    /// Replaces the `<style>` elements libraries added to `<head>`, in document order.
    Styles { sheets: Vec<String> },
    /// One React commit; applied atomically.
    Commit { ops: Vec<Op> },
    /// JS is blocked until the next layout's measured rects reach it through the sync pipe.
    Sync,
    /// JS has done all it does for the input events it was sent (their commits came before this).
    Settled,
    /// JS has `requestAnimationFrame` callbacks: it hears `frame` as the display's next frame begins.
    Frame,
    /// A benchmark request answered (see `bench`): a queried element's rect, or, without one,
    /// that JS has handled an injected input.
    Bench {
        req: u64,
        #[serde(default)]
        rect: Option<serde_json::Value>,
    },
    /// `new Window(options)`: always JS's first message; the window opens with these.
    Window {
        #[serde(default)]
        options: crate::native::WindowSpec,
    },
    /// `play()`, `pause()`, `currentTime = s`, `volume = v`, `playbackRate = r` on a `<video>` or
    /// `<audio>`: `op` is `play`, `pause`, `seek`, `volume` or `rate`.
    Media {
        id: u64,
        op: String,
        #[serde(default)]
        value: f64,
    },
    /// The caret and selected text in the focused editing host, which the host draws (see `editing`).
    Selection(crate::editing::Selection),
    /// A `<canvas>`'s pixels, after JS drew on it.
    #[serde(skip)]
    Pixels {
        id: u64,
        image: Arc<gpui::RenderImage>,
    },
    /// Menus, window and app calls, dialogs (see `native`).
    #[serde(untagged)]
    Native(crate::native::Call),
}

/// Externally tagged, e.g. `{"el":[2,"div",{...}]}`.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    El(u64, String, Props),
    Txt(u64, String),
    Set(u64, Props),
    Settext(u64, String),
    Append(u64, u64),
    Insert(u64, u64, u64),
    Remove(u64, u64),
    /// Unlinks without freeing: library-built nodes (`removeChild`) may be attached again.
    Detach(u64, u64),
    /// JS moved focus (`el.focus()`), or cleared it with `null`.
    Focus(Option<u64>),
    /// The SVG shape under the pointer (JS hit-tests inside drawings; gpui only sees the box), or none.
    Hover(Option<u64>),
    /// JS scrolled a scroll container (`scrollTop = y`, `scrollTo(x, y)`), in whole px.
    Scroll(u64, u32, u32),
}

#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Props {
    pub class: String,
    pub id: Option<String>,
    /// Inline style as CSS declaration text.
    pub style: Option<String>,
    /// Names of the events JS listens for, e.g. `click`.
    pub events: Vec<String>,
    /// `data-*`, `aria-*`, `role`, `type`... for attribute selectors.
    pub attrs: HashMap<String, String>,
    pub value: Option<String>,
    pub placeholder: Option<String>,
    pub disabled: bool,
    pub src: Option<String>,
    /// Last input `change` seq JS had seen when it rendered `value`.
    pub seq: Option<u64>,
    /// JS observes this node's size (`ResizeObserver`); report its bounds when they change.
    pub measure: bool,
    /// A `<dialog>` opened with `showModal()`: it shows in the top layer.
    pub modal: bool,
}

impl Props {
    pub fn listens(&self, event: &str) -> bool {
        self.events.iter().any(|e| e == event)
    }
}

/// The ops of a binary commit; see `js/src/wire.js` for the layout.
pub fn decode_commit(bytes: &[u8]) -> Result<Vec<Op>> {
    let mut r = Reader { bytes, at: 0 };
    let count = r.u32()? as usize;
    // Bounded by what the bytes can hold, so a corrupt count can't allocate wildly.
    let mut ops = Vec::with_capacity(count.min(bytes.len() / 5));
    for _ in 0..count {
        ops.push(r.op()?);
    }
    if r.at != bytes.len() {
        bail!("{} bytes after the last op", bytes.len() - r.at);
    }
    Ok(ops)
}

/// A canvas's pixels: its id, width and height as u32s, then straight-alpha RGBA rows.
pub fn decode_pixels(bytes: &[u8]) -> Result<Msg> {
    let mut r = Reader { bytes, at: 0 };
    let (id, width, height) = (r.id()?, r.u32()?, r.u32()?);
    let mut rgba = r.take(width as usize * height as usize * 4)?.to_vec();
    if r.at != bytes.len() {
        bail!("{} bytes after a {width}x{height} canvas", bytes.len() - r.at);
    }
    // gpui's images are BGRA.
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, rgba).context("canvas size")?;
    let image = gpui::RenderImage::new(vec![image::Frame::new(buffer)]);
    Ok(Msg::Pixels { id, image: Arc::new(image) })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.with_context(|| format!("commit ends inside a field at byte {}", self.at))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn id(&mut self) -> Result<u64> {
        Ok(self.u32()?.into())
    }

    fn str(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(std::str::from_utf8(self.take(len)?)?.to_owned())
    }

    fn props(&mut self) -> Result<Props> {
        let mask = self.u16()?;
        let has = |bit: u16| mask & (1 << bit) != 0;
        let mut p = Props::default();
        if has(0) {
            p.class = self.str()?;
        }
        if has(1) {
            p.id = Some(self.str()?);
        }
        if has(2) {
            p.style = Some(self.str()?);
        }
        if has(3) {
            let n = self.u8()?;
            p.events = (0..n).map(|_| self.str()).collect::<Result<_>>()?;
        }
        if has(4) {
            let n = self.u16()?;
            for _ in 0..n {
                let k = self.str()?;
                p.attrs.insert(k, self.str()?);
            }
        }
        if has(5) {
            p.value = Some(self.str()?);
        }
        if has(6) {
            p.placeholder = Some(self.str()?);
        }
        p.disabled = has(7);
        if has(8) {
            p.src = Some(self.str()?);
        }
        if has(9) {
            p.seq = Some(self.u32()?.into());
        }
        p.measure = has(10);
        p.modal = has(11);
        Ok(p)
    }

    fn maybe_id(&mut self) -> Result<Option<u64>> {
        Ok(match self.u8()? {
            0 => None,
            _ => Some(self.id()?),
        })
    }

    fn op(&mut self) -> Result<Op> {
        Ok(match self.u8()? {
            1 => Op::El(self.id()?, self.str()?, self.props()?),
            2 => Op::Txt(self.id()?, self.str()?),
            3 => Op::Set(self.id()?, self.props()?),
            4 => Op::Settext(self.id()?, self.str()?),
            5 => Op::Append(self.id()?, self.id()?),
            6 => Op::Insert(self.id()?, self.id()?, self.id()?),
            7 => Op::Remove(self.id()?, self.id()?),
            8 => Op::Detach(self.id()?, self.id()?),
            9 => Op::Focus(self.maybe_id()?),
            10 => Op::Hover(self.maybe_id()?),
            11 => Op::Scroll(self.id()?, self.u32()?, self.u32()?),
            code => bail!("unknown op code {code} at byte {}", self.at - 1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commit() {
        let msg: Msg = serde_json::from_str(
            r#"{"t":"commit","ops":[{"el":[2,"div",{"class":"a b","events":["click"]}]},{"append":[1,2]}]}"#,
        )
        .unwrap();
        let Msg::Commit { ops } = msg else { panic!() };
        assert_eq!(ops[1], Op::Append(1, 2));
        let Op::El(2, tag, props) = &ops[0] else {
            panic!()
        };
        assert_eq!(tag, "div");
        assert!(props.listens("click"));
    }

    #[test]
    fn decodes_the_binary_commit_wire_js_writes() {
        // The bytes `js/src/wire.test.js` checks the encoder writes.
        let hex = "0b000000010200000003000000646976ff0f03000000612062010000007809000000636f6c6f723a7265640105000000636c69636b010006000000646174612d6b02000000c3a90100000076010000007005000000732e706e6703000000020300000002000000686903020000000000040300000002000000796f0501000000020000000602000000030000000400000007010000000200000008020000000300000009000a01050000000b020000000a0000002c010000";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let ops = decode_commit(&bytes).unwrap();
        let props = Props {
            class: "a b".into(),
            id: Some("x".into()),
            style: Some("color:red".into()),
            events: vec!["click".into()],
            attrs: [("data-k".to_owned(), "é".to_owned())].into(),
            value: Some("v".into()),
            placeholder: Some("p".into()),
            disabled: true,
            src: Some("s.png".into()),
            seq: Some(3),
            measure: true,
            modal: true,
        };
        assert_eq!(
            ops,
            [
                Op::El(2, "div".into(), props),
                Op::Txt(3, "hi".into()),
                Op::Set(2, Props::default()),
                Op::Settext(3, "yo".into()),
                Op::Append(1, 2),
                Op::Insert(2, 3, 4),
                Op::Remove(1, 2),
                Op::Detach(2, 3),
                Op::Focus(None),
                Op::Hover(Some(5)),
                Op::Scroll(2, 10, 300),
            ]
        );
        assert!(decode_commit(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn decodes_canvas_pixels_as_bgra() {
        let mut bytes = [7u32, 1, 1].map(u32::to_le_bytes).concat();
        bytes.extend([255, 0, 0, 128]);
        let Msg::Pixels { id, image } = decode_pixels(&bytes).unwrap() else {
            panic!()
        };
        assert_eq!((id, image.as_bytes(0)), (7, Some(&[0u8, 0, 255, 128][..])));
        assert!(decode_pixels(&bytes[..bytes.len() - 1]).is_err());
    }
}
