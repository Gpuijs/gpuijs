use gpui::{Bounds, ContentMask, Corners, point, px, size};

#[test]
fn a_rounded_mask_keeps_the_corners_it_shares() {
    let card = ContentMask { bounds: Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.))), corner_radii: Corners::all(px(12.)) };
    let header = ContentMask { bounds: Bounds::new(point(px(0.), px(0.)), size(px(100.), px(30.))), ..Default::default() };
    let both = header.intersect(&card);
    assert_eq!(both.bounds, header.bounds);
    assert_eq!((both.corner_radii.top_left, both.corner_radii.top_right), (px(12.), px(12.)));
    assert_eq!((both.corner_radii.bottom_left, both.corner_radii.bottom_right), (px(0.), px(0.)));
}
