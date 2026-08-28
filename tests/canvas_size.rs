//! Reading the canvas box off a loaded asset.
//!
//! A sheet with no slices draws whole frames, and every frame is the canvas —
//! so the canvas size is the natural size to draw or measure such a sheet at.
//! Callers should be able to ask the asset for it instead of digging the
//! first frame's rect out of the atlas layout by hand.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer};

const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A 12x10 canvas with no slices at all.
fn fixture() -> Fixture {
    Fixture {
        canvas: (12, 10),
        frames: 2,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: vec![Cel {
            frame: 0,
            layer_index: 0,
            position: (0, 0),
            colour: WHITE,
        }],
        slices: Vec::new(),
    }
}

#[test]
fn canvas_size_is_the_files_own_canvas() {
    let (app, handles) = support::load("canvas_size", &fixture(), &["", "Main"]);
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let layouts = app.world().resource::<Assets<TextureAtlasLayout>>();

    for (handle, variant) in handles.iter().zip(["composite", "layer"]) {
        assert_eq!(
            aseprites
                .get(handle)
                .expect("variant loaded")
                .canvas_size(layouts),
            Some(Vec2::new(12.0, 10.0)),
            "the {variant} variant reports the canvas the file was authored on",
        );
    }
}

/// An asset with no frames has no canvas in the atlas to measure.
#[test]
fn a_frameless_sheet_has_no_canvas_size() {
    let (app, _handles) = support::load("canvas_size_frameless", &fixture(), &[""]);
    let layouts = app.world().resource::<Assets<TextureAtlasLayout>>();

    assert_eq!(Aseprite::builder().build().canvas_size(layouts), None);
}

/// A sheet whose layout has not loaded resolves to nothing rather than
/// panicking.
#[test]
fn an_unloaded_layout_resolves_to_nothing() {
    let (app, _handles) = support::load("canvas_size_unloaded", &fixture(), &[""]);
    let layouts = app.world().resource::<Assets<TextureAtlasLayout>>();

    let dangling = Aseprite::builder().with_frame_indices([0]).build();
    assert_eq!(dangling.canvas_size(layouts), None);
}
