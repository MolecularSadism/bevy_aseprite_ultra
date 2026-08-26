//! Nine-patch centres across a slice's timeline.
//!
//! Aseprite writes the centre on every key of a nine-patch slice, so a key
//! added before the centre was dragged out carries an empty one. An empty
//! centre divides the slice into nothing, so it reads as "no centre set here"
//! and the slice falls back to the first key that sets a real one. A centre
//! that overflows its slice is rejected the same way.

mod support;

use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::NodeImageMode;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer, Slice, SliceKey};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const CENTRE: (i32, i32, u32, u32) = (2, 2, 4, 4);

/// One 8x8 slice over two frames: frame 0's key predates the centre, frame 1's
/// sets it.
fn partly_annotated() -> Fixture {
    Fixture {
        canvas: (8, 8),
        frames: 2,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: (0..2)
            .map(|frame| Cel {
                frame,
                layer_index: 0,
                position: (0, 0),
                colour: WHITE,
            })
            .collect(),
        slices: vec![Slice {
            name: "Panel",
            keys: vec![
                SliceKey {
                    frame: 0,
                    bounds: (0, 0, 8, 8),
                    centre: None,
                },
                SliceKey {
                    frame: 1,
                    bounds: (0, 0, 8, 8),
                    centre: Some(CENTRE),
                },
            ],
        }],
    }
}

#[test]
fn an_empty_centre_falls_back_to_the_one_the_slice_defines() {
    let (app, handles) = support::load("nine_patch_keys", &partly_annotated(), &[""]);
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let slice = aseprites
        .get(&handles[0])
        .expect("composite loaded")
        .slice("Panel")
        .expect("Panel slice")
        .clone();

    let expected = Vec4::new(
        CENTRE.0 as f32,
        CENTRE.1 as f32,
        CENTRE.2 as f32,
        CENTRE.3 as f32,
    );
    assert_eq!(
        slice.nine_patch,
        Some(expected),
        "the slice's centre is the first one a key actually sets",
    );
    assert_eq!(
        slice.keys[0].nine_patch, None,
        "an empty centre sets nothing, so frame 0 defers to the slice's",
    );
    assert_eq!(
        slice.keys[1].nine_patch,
        Some(expected),
        "frame 1 keeps the centre it sets",
    );
}

#[test]
fn a_slice_with_no_centre_anywhere_has_no_nine_patch() {
    let mut fixture = partly_annotated();
    fixture.slices[0].keys[1].centre = None;

    let (app, handles) = support::load("nine_patch_none", &fixture, &[""]);
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let slice = aseprites
        .get(&handles[0])
        .expect("composite loaded")
        .slice("Panel")
        .expect("Panel slice");

    assert_eq!(
        slice.nine_patch, None,
        "a slice nobody gave a centre is not a nine-patch",
    );
}

/// One 8x8 slice with a centre, on a single frame.
fn annotated() -> Fixture {
    let mut fixture = partly_annotated();
    fixture.slices[0].keys[0].centre = Some(CENTRE);
    fixture
}

/// The border the fixture's authored centre works out to.
fn expected_border() -> BorderRect {
    nine_patch_to_slicer(
        Vec4::new(
            CENTRE.0 as f32,
            CENTRE.1 as f32,
            CENTRE.2 as f32,
            CENTRE.3 as f32,
        ),
        Vec2::splat(8.0),
    )
    .border
}

#[test]
fn a_default_image_mode_takes_the_authored_nine_patch() {
    let (mut app, handles) = support::load_with(
        "nine_patch_default_mode",
        &annotated(),
        &[""],
        AsepriteUltraPlugin,
    );
    let node = app
        .world_mut()
        .spawn((
            ImageNode::default(),
            AseSlice::new(handles[0].clone(), "Panel"),
        ))
        .id();
    app.update();

    let expected = nine_patch_to_slicer(
        Vec4::new(
            CENTRE.0 as f32,
            CENTRE.1 as f32,
            CENTRE.2 as f32,
            CENTRE.3 as f32,
        ),
        Vec2::splat(8.0),
    );
    let NodeImageMode::Sliced(slicer) = app
        .world()
        .get::<ImageNode>(node)
        .unwrap()
        .image_mode
        .clone()
    else {
        panic!("a slice with a centre nine-slices a node that asked for no mode of its own");
    };
    assert_eq!(slicer.border, expected.border);
}

/// A call site cannot narrow or widen the border: the centre is the art's.
#[test]
fn the_authored_centre_overrides_a_border_the_caller_set() {
    let (mut app, handles) = support::load_with(
        "nine_patch_caller_border",
        &annotated(),
        &[""],
        AsepriteUltraPlugin,
    );
    let node = app
        .world_mut()
        .spawn((
            ImageNode {
                image_mode: NodeImageMode::Sliced(TextureSlicer {
                    border: BorderRect::all(1.0),
                    ..default()
                }),
                ..default()
            },
            AseSlice::new(handles[0].clone(), "Panel"),
        ))
        .id();
    app.update();

    let NodeImageMode::Sliced(slicer) = app
        .world()
        .get::<ImageNode>(node)
        .unwrap()
        .image_mode
        .clone()
    else {
        panic!("a slice with a centre nine-slices whatever draws it");
    };
    assert_eq!(
        slicer.border,
        expected_border(),
        "the art's centre is the border"
    );
}

/// Aseprite has no way to say "tile the middle", so that part of the slicer
/// stays a call-site decision and has to survive.
#[test]
fn a_callers_scale_mode_survives_the_authored_centre() {
    let (mut app, handles) = support::load_with(
        "nine_patch_caller_scale",
        &annotated(),
        &[""],
        AsepriteUltraPlugin,
    );
    let tile = SliceScaleMode::Tile { stretch_value: 1.0 };
    let node = app
        .world_mut()
        .spawn((
            ImageNode {
                image_mode: NodeImageMode::Sliced(TextureSlicer {
                    center_scale_mode: tile,
                    sides_scale_mode: tile,
                    max_corner_scale: 2.0,
                    ..default()
                }),
                ..default()
            },
            AseSlice::new(handles[0].clone(), "Panel"),
        ))
        .id();
    app.update();

    let NodeImageMode::Sliced(slicer) = app
        .world()
        .get::<ImageNode>(node)
        .unwrap()
        .image_mode
        .clone()
    else {
        panic!("a slice with a centre nine-slices whatever draws it");
    };
    assert_eq!(
        slicer.border,
        expected_border(),
        "the art still owns the border"
    );
    assert_eq!(slicer.center_scale_mode, tile);
    assert_eq!(slicer.sides_scale_mode, tile);
    assert_eq!(slicer.max_corner_scale, 2.0);
}

/// A single-frame, single-slice file: the smallest fixture that can carry a
/// malformed centre.
fn one_slice(bounds: (i32, i32, u32, u32), centre: Option<(i32, i32, u32, u32)>) -> Fixture {
    Fixture {
        canvas: (64, 16),
        frames: 1,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: vec![Cel {
            frame: 0,
            layer_index: 0,
            position: (0, 0),
            colour: WHITE,
        }],
        slices: vec![Slice {
            name: "Panel",
            keys: vec![SliceKey {
                frame: 0,
                bounds,
                centre,
            }],
        }],
    }
}

fn loaded_slice(name: &str, fixture: &Fixture) -> SliceMeta {
    let (app, handles) = support::load(name, fixture, &[""]);
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    aseprites
        .get(&handles[0])
        .expect("composite loaded")
        .slice("Panel")
        .expect("Panel slice")
        .clone()
}

/// A centre dragged past an edge would give that edge a negative inset, which
/// no slicer can express. The art is still there, so the slice loads — it just
/// is not a nine-patch.
#[test]
fn a_centre_taller_than_its_bounds_is_rejected() {
    // 48x1 strip, centre one pixel below the bottom edge: bottom = -1.
    let slice = loaded_slice(
        "nine_patch_overflow_bottom",
        &one_slice((0, 0, 48, 1), Some((13, 1, 22, 1))),
    );

    assert_eq!(slice.size(), Vec2::new(48.0, 1.0), "the slice still loads");
    assert_eq!(slice.nine_patch, None, "a negative inset is not a border");
    assert_eq!(slice.border(), None);
    assert_eq!(slice.keys[0].nine_patch, None);
}

#[test]
fn a_centre_wider_than_its_bounds_is_rejected() {
    // 12x12 panel, centre one pixel past the right edge: right = -1.
    let slice = loaded_slice(
        "nine_patch_overflow_right",
        &one_slice((0, 0, 12, 12), Some((3, 3, 10, 9))),
    );

    assert_eq!(slice.nine_patch, None);
    assert_eq!(slice.border(), None);
}

/// Zero is a legal inset — it means that edge has no border, not that the
/// centre is malformed.
#[test]
fn a_centre_flush_with_an_edge_survives() {
    let slice = loaded_slice(
        "nine_patch_flush_edge",
        &one_slice((0, 0, 12, 12), Some((0, 3, 12, 5))),
    );

    assert_eq!(
        slice.nine_patch,
        Some(Vec4::new(0.0, 3.0, 12.0, 5.0)),
        "a centre spanning the full width borders top and bottom only",
    );
    assert_eq!(
        slice.border(),
        Some(BorderRect {
            min_inset: Vec2::new(0.0, 3.0),
            max_inset: Vec2::new(0.0, 4.0),
        }),
    );
}
