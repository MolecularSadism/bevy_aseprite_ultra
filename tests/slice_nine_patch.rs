//! Nine-patch centres across a slice's timeline.
//!
//! Aseprite writes the centre on every key of a nine-patch slice, so a key
//! added before the centre was dragged out carries an empty one. An empty
//! centre divides the slice into nothing, so it reads as "no centre set here"
//! and the slice falls back to the first key that sets a real one.

mod support;

use bevy::prelude::*;
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
    let slice = aseprites.get(&handles[0]).expect("composite loaded").slices["Panel"].clone();

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
    let slice = &aseprites.get(&handles[0]).expect("composite loaded").slices["Panel"];

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

#[test]
fn an_image_mode_the_caller_set_survives_the_authored_nine_patch() {
    let (mut app, handles) = support::load_with(
        "nine_patch_explicit_mode",
        &annotated(),
        &[""],
        AsepriteUltraPlugin,
    );
    let node = app
        .world_mut()
        .spawn((
            ImageNode {
                image_mode: NodeImageMode::Stretch,
                ..default()
            },
            AseSlice::new(handles[0].clone(), "Panel"),
        ))
        .id();
    app.update();

    assert!(
        matches!(
            app.world().get::<ImageNode>(node).unwrap().image_mode,
            NodeImageMode::Stretch
        ),
        "the call site's own image mode outranks the centre the artist authored",
    );
}
