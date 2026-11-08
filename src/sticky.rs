//! `position: sticky` with `top`: the box keeps its place in flow and, once its scroll container
//! has scrolled it above `top`, is drawn and hit-tested that much lower, the way gpui moves
//! scrolled content: by an element offset while it is prepainted.

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window, point, px,
};

pub struct Sticky {
    child: AnyElement,
    top: Pixels,
}

impl Sticky {
    pub fn new(child: AnyElement, top: Pixels) -> Self {
        Self { child, top }
    }
}

/// How far a box at `box_top` moves down to stay `top` below the clip's top edge.
// shortcut: not held inside its containing block; a sticky box whose parent scrolls away
// rides along past it (fine for page headers, wrong for sticky section titles).
pub fn shift(clip_top: Pixels, top: Pixels, box_top: Pixels) -> Pixels {
    (clip_top + top - box_top).max(px(0.0))
}

impl IntoElement for Sticky {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Sticky {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // The content mask is the clip of the scroll container the box is in.
        let clip_top = window.content_mask().bounds.top();
        let dy = shift(clip_top, self.top, bounds.top());
        window.with_element_offset(point(px(0.0), dy), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sticky_box_moves_only_once_scrolled_past_its_top() {
        assert_eq!(shift(px(0.0), px(0.0), px(40.0)), px(0.0), "below the edge: in flow");
        assert_eq!(shift(px(0.0), px(0.0), px(-120.0)), px(120.0), "scrolled up: held at the edge");
        assert_eq!(shift(px(50.0), px(8.0), px(30.0)), px(28.0), "kept `top` below the clip");
    }
}
