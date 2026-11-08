//! Guards the vendored taffy fix: `width: 100%; max-width` items in an `align-items: center` column
//! must be measured at their clamped width, or wrapping content overflows (shadcn Card + footer).
use taffy::prelude::*;
use taffy::{Overflow, Point};

#[test]
fn percent_width_with_max_width_is_clamped_before_measuring_height() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let items: Vec<_> = [130.0_f32, 70.0, 100.0, 75.0, 70.0]
        .iter()
        .map(|&w| {
            let size = Size {
                width: length(w),
                height: length(36.0_f32),
            };
            t.new_leaf(Style {
                size,
                flex_shrink: 0.0,
                ..Default::default()
            })
            .unwrap()
        })
        .collect();
    let row = Style {
        flex_wrap: FlexWrap::Wrap,
        gap: Size {
            width: length(8.0_f32),
            height: length(8.0_f32),
        },
        ..Default::default()
    };
    let footer = t.new_with_children(row, &items).unwrap();
    let card_style = Style {
        flex_direction: FlexDirection::Column,
        size: Size {
            width: percent(1.0_f32),
            height: auto(),
        },
        max_size: Size {
            width: length(400.0_f32),
            height: auto(),
        },
        ..Default::default()
    };
    let card = t.new_with_children(card_style, &[footer]).unwrap();
    let root_style = Style {
        flex_direction: FlexDirection::Column,
        align_items: Some(AlignItems::Center),
        size: Size {
            width: length(1024.0_f32),
            height: auto(),
        },
        ..Default::default()
    };
    let root = t.new_with_children(root_style, &[card]).unwrap();
    t.compute_layout(root, Size::MAX_CONTENT).unwrap();
    assert_eq!(
        t.layout(footer).unwrap().size.height,
        80.0,
        "two rows of 36px plus an 8px gap"
    );
    assert_eq!(t.layout(card).unwrap().size.height, 80.0);
}

/// Guards the vendored taffy fix: `gap` must still separate items when an auto margin takes
/// the free space (shadcn sidebar: icon, label, `ms-auto` chevron).
#[test]
fn gap_is_kept_next_to_auto_margins() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let square = || Style {
        size: Size {
            width: length(16.0_f32),
            height: length(16.0_f32),
        },
        ..Default::default()
    };
    let icon = t.new_leaf(square()).unwrap();
    let label = t.new_leaf(square()).unwrap();
    let chevron = t
        .new_leaf(Style {
            margin: Rect {
                left: auto(),
                right: zero(),
                top: zero(),
                bottom: zero(),
            },
            ..square()
        })
        .unwrap();
    let row = Style {
        size: Size {
            width: length(200.0_f32),
            height: auto(),
        },
        gap: Size {
            width: length(8.0_f32),
            height: zero(),
        },
        ..Default::default()
    };
    let row = t.new_with_children(row, &[icon, label, chevron]).unwrap();
    t.compute_layout(row, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(label).unwrap().location.x, 24.0, "icon + gap");
    assert_eq!(t.layout(chevron).unwrap().location.x, 184.0, "flush right");
}

/// A negative margin shrinks an item's outer size but must not collapse its flex container
/// (shadcn's collapsed sidebar: group label `-mt-8` above the menu).
#[test]
fn negative_margin_item_keeps_column_height() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let label = t
        .new_leaf(Style {
            size: Size {
                width: auto(),
                height: length(32.0_f32),
            },
            flex_shrink: 0.0,
            margin: Rect {
                top: length(-32.0_f32),
                ..Rect::zero()
            },
            ..Default::default()
        })
        .unwrap();
    let menu = t
        .new_leaf(Style {
            size: Size {
                width: auto(),
                height: length(68.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let group = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            &[label, menu],
        )
        .unwrap();
    let content = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(96.0_f32),
                    height: length(384.0_f32),
                },
                overflow: Point {
                    x: Overflow::Hidden,
                    y: Overflow::Hidden,
                },
                ..Default::default()
            },
            &[group],
        )
        .unwrap();
    t.compute_layout(content, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(group).unwrap().size.height, 68.0);
    assert_eq!(t.layout(menu).unwrap().location.y, 0.0);
}

/// Guards the vendored taffy fix: an `aspect-ratio` item with a definite cross size keeps the
/// transferred size as its minimum (shadcn's collapsed sidebar logo: `size-8 aspect-square`).
#[test]
fn aspect_ratio_item_is_not_squeezed_below_its_transferred_size() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let icon = t
        .new_leaf(Style {
            size: Size {
                width: length(16.0_f32),
                height: length(16.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let logo = t
        .new_with_children(
            Style {
                size: Size {
                    width: length(32.0_f32),
                    height: length(32.0_f32),
                },
                aspect_ratio: Some(1.0),
                ..Default::default()
            },
            &[icon],
        )
        .unwrap();
    let wide = t
        .new_leaf(Style {
            size: Size {
                width: length(100.0_f32),
                height: length(10.0_f32),
            },
            flex_shrink: 0.0,
            ..Default::default()
        })
        .unwrap();
    let row = t
        .new_with_children(
            Style {
                size: Size {
                    width: length(32.0_f32),
                    height: length(32.0_f32),
                },
                overflow: Point {
                    x: Overflow::Hidden,
                    y: Overflow::Hidden,
                },
                ..Default::default()
            },
            &[logo, wide],
        )
        .unwrap();
    t.compute_layout(row, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(logo).unwrap().size.width, 32.0);
}

/// A relative block child's `top: 50%` resolves against its container's definite height (sonner's
/// spinner centers itself in its 16px icon box this way); with an indefinite height it is ignored.
#[test]
fn block_child_percent_top_resolves_against_definite_height() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let child_style = || Style {
        position: Position::Relative,
        inset: Rect {
            left: percent(0.5_f32),
            right: auto(),
            top: percent(0.5_f32),
            bottom: auto(),
        },
        size: Size {
            width: length(4.0_f32),
            height: length(4.0_f32),
        },
        ..Default::default()
    };
    let block = |size: Size<Dimension>| Style {
        display: Display::Block,
        size,
        ..Default::default()
    };
    let child = t.new_leaf(child_style()).unwrap();
    let sized = block(Size {
        width: length(16.0_f32),
        height: length(16.0_f32),
    });
    let root = t.new_with_children(sized, &[child]).unwrap();
    t.compute_layout(root, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(child).unwrap().location, Point { x: 8.0, y: 8.0 });

    let child = t.new_leaf(child_style()).unwrap();
    let auto_height = block(Size {
        width: length(16.0_f32),
        height: auto(),
    });
    let root = t.new_with_children(auto_height, &[child]).unwrap();
    t.compute_layout(root, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(child).unwrap().location, Point { x: 8.0, y: 0.0 });
}

/// Guards the vendored taffy fix: measuring a block re-lays its children at the measured width, so
/// a final layout cached before that must not be reused (shadcn Recent Sales rows in the
/// dashboard grid kept their text laid out at a ~36px measuring width).
#[test]
fn measuring_drops_the_cached_final_layout() {
    use taffy::{Cache, LayoutOutput, RunMode};
    let mut cache = Cache::new();
    let wide = Size {
        width: Some(310.0_f32),
        height: None,
    };
    let output = |width: f32| {
        LayoutOutput::from_outer_size(Size {
            width,
            height: 76.0,
        })
    };
    cache.store(
        wide,
        Size::MAX_CONTENT,
        RunMode::PerformLayout,
        output(310.0),
    );
    assert!(
        cache
            .get(wide, Size::MAX_CONTENT, RunMode::PerformLayout)
            .is_some()
    );
    let narrow = Size {
        width: Some(36.0_f32),
        height: None,
    };
    cache.store(
        narrow,
        Size::MAX_CONTENT,
        RunMode::ComputeSize,
        output(36.0),
    );
    assert!(
        cache
            .get(wide, Size::MAX_CONTENT, RunMode::PerformLayout)
            .is_none()
    );
}

/// Guards the vendored taffy fix: a final layout cached without a known width isn't reused once the
/// width is known, or a `max-width` column keeps the text it laid out at max-content (shadcn
/// sign-in-2: the description stayed one line tall and the form overlapped it).
#[test]
fn max_width_column_keeps_its_wrapped_text_height() {
    let mut t: TaffyTree<f32> = TaffyTree::new();
    let block = || Style {
        display: Display::Block,
        ..Default::default()
    };
    // Two 300px words, 40px a line.
    let text = t.new_leaf_with_context(Style::default(), 600.0).unwrap();
    let p = t.new_with_children(block(), &[text]).unwrap();
    let header = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            &[p],
        )
        .unwrap();
    let column = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: percent(1.0_f32),
                    height: auto(),
                },
                max_size: Size {
                    width: length(400.0_f32),
                    height: auto(),
                },
                ..Default::default()
            },
            &[header],
        )
        .unwrap();
    let cell = t.new_with_children(block(), &[column]).unwrap();
    let side = t.new_leaf(Style::default()).unwrap();
    let grid = t
        .new_with_children(
            Style {
                display: Display::Grid,
                grid_template_columns: vec![repeat(2, vec![minmax(length(0.0_f32), fr(1.0_f32))])],
                align_items: Some(AlignItems::Center),
                size: Size {
                    width: length(1024.0_f32),
                    height: length(768.0_f32),
                },
                ..Default::default()
            },
            &[cell, side],
        )
        .unwrap();
    t.compute_layout_with_measure(grid, Size::MAX_CONTENT, |known, avail, _, width, _| {
        let Some(&mut width) = width else {
            return Size::ZERO;
        };
        let max = known.width.unwrap_or(match avail {
            Size {
                width: AvailableSpace::Definite(w),
                ..
            } => w,
            Size {
                width: AvailableSpace::MinContent,
                ..
            } => 0.0,
            _ => f32::INFINITY,
        });
        let lines = if max >= width { 1.0 } else { 2.0 };
        Size {
            width: width.min(max.max(width / 2.0)),
            height: 40.0 * lines,
        }
    })
    .unwrap();
    assert_eq!(t.layout(column).unwrap().size.width, 400.0);
    assert_eq!(
        t.layout(text).unwrap().size.height,
        80.0,
        "wrapped at 400px"
    );
    assert_eq!(
        t.layout(header).unwrap().size.height,
        80.0,
        "holds the wrapped text"
    );
}

/// Guards the vendored taffy fix: a `width: 100%` grid item contributes its content width to an
/// auto track, so `justify-content: center` centers it (shadcn sign-in: card left of the logo).
#[test]
fn percent_width_grid_item_is_sized_by_content() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let card = t
        .new_leaf(Style {
            size: Size {
                width: length(384.0_f32),
                height: length(40.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let column = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: percent(1.0_f32),
                    height: auto(),
                },
                padding: length(32.0_f32),
                ..Default::default()
            },
            &[card],
        )
        .unwrap();
    let grid = t
        .new_with_children(
            Style {
                display: Display::Grid,
                justify_content: Some(JustifyContent::Center),
                size: Size {
                    width: length(1024.0_f32),
                    height: length(768.0_f32),
                },
                ..Default::default()
            },
            &[column],
        )
        .unwrap();
    t.compute_layout(grid, Size::MAX_CONTENT).unwrap();
    let layout = t.layout(column).unwrap();
    assert_eq!(layout.size.width, 448.0);
    assert_eq!(layout.location.x, 288.0);
}

#[test]
fn inline_size_containers_keep_wide_content_from_widening_flex_items() {
    // shadcn's SidebarInset: a flex item whose `@container/main` descendant holds a wide table.
    let mut t: TaffyTree<()> = TaffyTree::new();
    let table = t
        .new_leaf(Style {
            size: Size {
                width: length(1000.0_f32),
                height: length(10.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let container = t
        .new_with_children(
            Style {
                contain_inline_size: true,
                overflow: Point {
                    x: Overflow::Scroll,
                    y: Overflow::Visible,
                },
                ..Default::default()
            },
            &[table],
        )
        .unwrap();
    let column = Style {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        ..Default::default()
    };
    let inset = t.new_with_children(column, &[container]).unwrap();
    let row = t
        .new_with_children(
            Style {
                size: Size {
                    width: length(500.0_f32),
                    height: auto(),
                },
                ..Default::default()
            },
            &[inset],
        )
        .unwrap();
    t.compute_layout(row, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(inset).unwrap().size.width, 500.0);
    assert_eq!(t.layout(container).unwrap().size.width, 500.0);
}

/// Guards the vendored `TaffyTree::resolve_calc_value`: gpui's calc handles are `[fraction, px]`.
#[test]
fn calc_handles_resolve_to_a_fraction_plus_pixels() {
    // One op (see `taffy::eval_calc`): push 1 × the basis + 14.
    #[repr(C, align(8))]
    struct Calc([f32; 4]);
    static RIGHT: Calc = Calc([1.0, 0.0, 1.0, 14.0]);
    let mut t: TaffyTree<()> = TaffyTree::new();
    let menu = t
        .new_leaf(Style {
            position: Position::Absolute,
            size: Size {
                width: length(250.0_f32),
                height: length(50.0_f32),
            },
            inset: Rect {
                left: LengthPercentageAuto::auto(),
                right: LengthPercentageAuto::calc(&RIGHT as *const Calc as *const ()),
                top: length(0.0_f32),
                bottom: LengthPercentageAuto::auto(),
            },
            ..Default::default()
        })
        .unwrap();
    let li = t
        .new_with_children(
            Style {
                size: Size {
                    width: length(234.0_f32),
                    height: length(40.0_f32),
                },
                ..Default::default()
            },
            &[menu],
        )
        .unwrap();
    t.compute_layout(li, Size::MAX_CONTENT).unwrap();
    // Its right edge sits 14px left of the parent's left edge.
    assert_eq!(t.layout(menu).unwrap().location.x, -14.0 - 250.0);
}

#[test]
fn block_content_width_counts_margins_of_fixed_width_children() {
    // TailAdmin's radio: an inline wrapper around a `w-5 me-2` dot, before the label text.
    let mut t: TaffyTree<()> = TaffyTree::new();
    let dot = t
        .new_leaf(Style {
            size: Size {
                width: length(20.0_f32),
                height: length(20.0_f32),
            },
            margin: Rect {
                left: zero(),
                right: length(8.0_f32),
                top: zero(),
                bottom: zero(),
            },
            ..Default::default()
        })
        .unwrap();
    let wrapper = t
        .new_with_children(
            Style {
                display: Display::Block,
                ..Default::default()
            },
            &[dot],
        )
        .unwrap();
    let label = t.new_with_children(Style::default(), &[wrapper]).unwrap();
    t.compute_layout(label, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(wrapper).unwrap().size.width, 28.0);
}

/// A grid of `overflow: hidden` items in an auto-height flex column keeps its rows' content height
/// (shadcn dashboard stat cards: `overflow-hidden` Cards in a grid, under a page that scrolls).
#[test]
fn grid_of_scroll_container_items_keeps_content_height_in_a_column() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let cards: Vec<_> = (0..4)
        .map(|_| {
            let content = t
                .new_leaf(Style {
                    size: Size {
                        width: auto(),
                        height: length(171.0_f32),
                    },
                    ..Default::default()
                })
                .unwrap();
            t.new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    overflow: Point {
                        x: Overflow::Hidden,
                        y: Overflow::Hidden,
                    },
                    contain_inline_size: true,
                    ..Default::default()
                },
                &[content],
            )
            .unwrap()
        })
        .collect();
    let grid = t
        .new_with_children(
            Style {
                display: Display::Grid,
                grid_template_columns: vec![repeat(4, vec![minmax(length(0.0_f32), fr(1.0_f32))])],
                ..Default::default()
            },
            &cards,
        )
        .unwrap();
    let below = t
        .new_leaf(Style {
            size: Size {
                width: auto(),
                height: length(775.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let column = t
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(1344.0_f32),
                    height: auto(),
                },
                ..Default::default()
            },
            &[grid, below],
        )
        .unwrap();
    t.compute_layout(
        column,
        Size {
            width: AvailableSpace::Definite(1344.0),
            height: AvailableSpace::Definite(900.0),
        },
    )
    .unwrap();
    assert_eq!(t.layout(grid).unwrap().size.height, 171.0);
    assert_eq!(t.layout(column).unwrap().size.height, 946.0);
}

/// A block-level child with `aspect-ratio` and auto width and height takes its height from the
/// width it stretches to (shadcn's ChartContainer: `aspect-video` in a Card's content).
#[test]
fn stretched_block_child_takes_height_from_aspect_ratio() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    // Recharts' ResponsiveContainer: `width: 100%; height: 100%`.
    let responsive = t
        .new_leaf(Style {
            size: Size { width: percent(1.0_f32), height: percent(1.0_f32) },
            ..Default::default()
        })
        .unwrap();
    let chart = t
        .new_with_children(
            Style { display: Display::Flex, aspect_ratio: Some(16.0 / 9.0), ..Default::default() },
            &[responsive],
        )
        .unwrap();
    let content = t
        .new_with_children(
            Style {
                display: Display::Block,
                padding: Rect { left: length(16.0_f32), right: length(16.0_f32), top: zero(), bottom: zero() },
                ..Default::default()
            },
            &[chart],
        )
        .unwrap();
    let card = t
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size { width: length(752.0_f32), height: auto() },
                overflow: Point { x: Overflow::Hidden, y: Overflow::Hidden },
                ..Default::default()
            },
            &[content],
        )
        .unwrap();
    t.compute_layout(card, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(chart).unwrap().size.height, 405.0);
    assert_eq!(t.layout(responsive).unwrap().size.height, 405.0);
}

/// The shadcn sidebar layout around that grid: a scrolling root, a `min-h-svh` flex row, and
/// `flex-1` (basis 0%) columns. The page grows to its content; nothing in it is squeezed.
#[test]
fn flex_1_page_in_min_height_row_grows_to_content() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let leaf = |t: &mut TaffyTree<()>, h: f32| {
        t.new_leaf(Style {
            size: Size { width: auto(), height: length(h) },
            ..Default::default()
        })
        .unwrap()
    };
    let column = |t: &mut TaffyTree<()>, kids: &[NodeId], flex_1: bool| {
        t.new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                gap: Size { width: length(16.0_f32), height: length(16.0_f32) },
                flex_grow: if flex_1 { 1.0 } else { 0.0 },
                flex_basis: if flex_1 { percent(0.0_f32) } else { auto() },
                ..Default::default()
            },
            kids,
        )
        .unwrap()
    };
    let cards: Vec<_> = (0..4)
        .map(|_| {
            let content = leaf(&mut t, 171.0);
            t.new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    overflow: Point { x: Overflow::Hidden, y: Overflow::Hidden },
                    contain_inline_size: true,
                    min_size: Size { width: length(0.0_f32), height: auto() },
                    ..Default::default()
                },
                &[content],
            )
            .unwrap()
        })
        .collect();
    let grid = t
        .new_with_children(
            Style {
                display: Display::Grid,
                grid_template_columns: vec![repeat(4, vec![minmax(length(0.0_f32), fr(1.0_f32))])],
                ..Default::default()
            },
            &cards,
        )
        .unwrap();
    let heading = leaf(&mut t, 32.0);
    let charts = leaf(&mut t, 775.0);
    let overview = column(&mut t, &[heading, grid, charts], true);
    let page = column(&mut t, &[overview], true);
    let header = leaf(&mut t, 56.0);
    let main = column(&mut t, &[header, page], true);
    let gap = t
        .new_leaf(Style {
            size: Size { width: length(48.0_f32), height: auto() },
            ..Default::default()
        })
        .unwrap();
    let wrapper = t
        .new_with_children(
            Style {
                display: Display::Flex,
                size: Size { width: percent(1.0_f32), height: auto() },
                min_size: Size { width: auto(), height: length(900.0_f32) },
                ..Default::default()
            },
            &[gap, main],
        )
        .unwrap();
    let body = t.new_with_children(Style { display: Display::Block, ..Default::default() }, &[wrapper]).unwrap();
    let html = t
        .new_with_children(
            Style {
                display: Display::Block,
                size: Size { width: percent(1.0_f32), height: percent(1.0_f32) },
                overflow: Point { x: Overflow::Visible, y: Overflow::Scroll },
                ..Default::default()
            },
            &[body],
        )
        .unwrap();
    t.compute_layout(
        html,
        Size { width: AvailableSpace::Definite(1440.0), height: AvailableSpace::Definite(900.0) },
    )
    .unwrap();
    assert_eq!(t.layout(grid).unwrap().size.height, 171.0);
    assert_eq!(t.layout(wrapper).unwrap().size.height, 56.0 + 16.0 + 32.0 + 171.0 + 775.0 + 32.0);
}

/// A grid row holding an `aspect-square` box (a table cell's product image, its picture absolutely
/// positioned inside) is as tall as the box's width makes it.
#[test]
fn grid_row_fits_an_aspect_ratio_box() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let picture = t
        .new_leaf(Style {
            position: Position::Absolute,
            inset: Rect { left: zero(), right: zero(), top: zero(), bottom: zero() },
            ..Default::default()
        })
        .unwrap();
    let square = t
        .new_with_children(
            Style { display: Display::Block, aspect_ratio: Some(1.0), ..Default::default() },
            &[picture],
        )
        .unwrap();
    // gpuijs's table cell (`Root::table_cell`): a flex column wrapper around the <td>, itself a
    // centered flex column around a block of its content.
    // `w_full`: gpuijs's cell content fills the cell, as a table cell's content does.
    let content = t
        .new_with_children(
            Style { display: Display::Block, size: Size { width: percent(1.0_f32), height: auto() }, ..Default::default() },
            &[square],
        )
        .unwrap();
    let td = t
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: Some(JustifyContent::Center),
                flex_grow: 1.0,
                padding: Rect { left: length(8.0_f32), right: length(8.0_f32), top: length(8.0_f32), bottom: length(8.0_f32) },
                ..Default::default()
            },
            &[content],
        )
        .unwrap();
    let cell = t
        .new_with_children(
            Style { display: Display::Flex, flex_direction: FlexDirection::Column, ..Default::default() },
            &[td],
        )
        .unwrap();
    let text = t
        .new_leaf(Style { size: Size { width: length(50.0_f32), height: length(20.0_f32) }, ..Default::default() })
        .unwrap();
    let grid = t
        .new_with_children(
            Style {
                display: Display::Grid,
                grid_template_columns: vec![
                    minmax(length(116.0_f32), auto()),
                    minmax(length(150.0_f32), auto()),
                ],
                size: Size { width: length(600.0_f32), height: auto() },
                ..Default::default()
            },
            &[cell, text],
        )
        .unwrap();
    t.compute_layout(grid, Size::MAX_CONTENT).unwrap();
    let side = t.layout(square).unwrap().size.width;
    assert_eq!(t.layout(square).unwrap().size.height, side);
    assert_eq!(t.layout(grid).unwrap().size.height, side + 16.0);
}

/// Guards the vendored taffy fix: a block's definite height is the basis for its children's percent
/// min heights (the AI chat's `min-h-full` message list centers its empty state in the viewport).
#[test]
fn block_child_percent_min_height_resolves_against_definite_height() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let line = t
        .new_leaf(Style {
            size: Size {
                width: length(50.0_f32),
                height: length(20.0_f32),
            },
            ..Default::default()
        })
        .unwrap();
    let content = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                min_size: Size {
                    width: auto(),
                    height: percent(1.0_f32),
                },
                ..Default::default()
            },
            &[line],
        )
        .unwrap();
    let viewport = t
        .new_with_children(
            Style {
                display: Display::Block,
                overflow: Point {
                    x: Overflow::Hidden,
                    y: Overflow::Scroll,
                },
                flex_grow: 1.0,
                min_size: Size {
                    width: auto(),
                    height: length(0.0_f32),
                },
                ..Default::default()
            },
            &[content],
        )
        .unwrap();
    // A flex column measures the viewport before giving it its height.
    let column = t
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(300.0_f32),
                    height: length(200.0_f32),
                },
                ..Default::default()
            },
            &[viewport],
        )
        .unwrap();
    t.compute_layout(column, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(viewport).unwrap().size.height, 200.0);
    assert_eq!(t.layout(content).unwrap().size.height, 200.0);
}

#[test]
fn an_item_spanning_rows_stretches_over_them() {
    let mut t: TaffyTree<()> = TaffyTree::new();
    let cell = |t: &mut TaffyTree<()>, row: i16, col: i16, rows: u16, h: f32| {
        let leaf = t.new_leaf(Style { size: Size { width: auto(), height: length(h) }, ..Default::default() }).unwrap();
        let wrap = Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            grid_row: Line { start: line(row), end: span(rows) },
            grid_column: Line { start: line(col), end: span(1) },
            ..Default::default()
        };
        t.new_with_children(wrap, &[leaf]).unwrap()
    };
    let tall = cell(&mut t, 1, 2, 2, 20.0);
    let kids = [cell(&mut t, 1, 1, 1, 20.0), tall, cell(&mut t, 2, 1, 1, 20.0)];
    let grid = Style {
        display: Display::Grid,
        grid_template_columns: vec![auto(), auto()],
        ..Default::default()
    };
    let root = t.new_with_children(grid, &kids).unwrap();
    t.compute_layout(root, Size::MAX_CONTENT).unwrap();
    assert_eq!(t.layout(tall).unwrap().size.height, 40.0);
}

/// A grid of auto columns laying out a table, at `width`, over cells of (min-, max-content)
/// widths: the columns' widths.
fn table_columns(width: f32, cells: &[(f32, f32)]) -> Vec<f32> {
    let mut t: TaffyTree<(f32, f32)> = TaffyTree::new();
    let leaves: Vec<_> = cells.iter().map(|&c| t.new_leaf_with_context(Style::default(), c).unwrap()).collect();
    let grid = Style {
        display: Display::Grid,
        grid_table_columns: true,
        grid_template_columns: vec![auto(); cells.len()],
        size: Size { width: length(width), height: auto() },
        ..Default::default()
    };
    let root = t.new_with_children(grid, &leaves).unwrap();
    t.compute_layout_with_measure(root, Size::MAX_CONTENT, |known, available, _, ctx, _| {
        let &mut (min, max) = ctx.unwrap();
        let width = known.width.unwrap_or(match available.width {
            AvailableSpace::MinContent => min,
            AvailableSpace::MaxContent => max,
            AvailableSpace::Definite(w) => w.clamp(min, max),
        });
        Size { width, height: 10.0 }
    })
    .unwrap();
    leaves.iter().map(|&l| t.layout(l).unwrap().size.width).collect()
}

#[test]
fn table_columns_share_space_by_their_content() {
    // Wider than all the content: what is left goes by max-content width, not equally.
    assert_eq!(table_columns(400.0, &[(10.0, 50.0), (10.0, 150.0)]), vec![100.0, 300.0]);
    // Between: each column gets its min-content, then a share of the rest by how far it has to go.
    assert_eq!(table_columns(100.0, &[(10.0, 50.0), (10.0, 130.0)]), vec![30.0, 70.0]);
}

#[test]
fn calc_programs_take_mins_and_maxes() {
    // clamp(10px, 50%, 200px): max(10px, min(50%, 200px)).
    let clamp = [0.0, 0.0, 10.0, 0.0, 0.5, 0.0, 0.0, 0.0, 200.0, 1.0, 2.0, 0.0, 2.0, 2.0, 0.0];
    assert_eq!([10.0, 100.0, 1000.0].map(|b| taffy::eval_calc(&clamp, b)), [10.0, 50.0, 200.0]);
}
