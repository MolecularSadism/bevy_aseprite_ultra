//! Reading a slice's geometry off a loaded asset.
//!
//! Size and nine-patch insets are properties of the art, so callers should be
//! able to ask the slice for them instead of rebuilding them from `rect` and
//! the raw centre.

mod support;

use bevy::prelude::*;
use bevy::sprite::{Anchor, BorderRect};
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer, Slice, SliceKey};

const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A 12x10 panel with a centre, and a 4x4 icon without one.
fn fixture() -> Fixture {
    Fixture {
        canvas: (16, 16),
        frames: 1,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: vec![Cel {
            frame: 0,
            layer_index: 0,
            position: (0, 0),
            colour: WHITE,
        }],
        slices: vec![
            Slice {
                name: "Panel",
                keys: vec![SliceKey {
                    frame: 0,
                    bounds: (0, 0, 12, 10),
                    centre: Some((3, 2, 6, 5)),
                }],
            },
            Slice {
                name: "Icon",
                keys: vec![SliceKey {
                    frame: 0,
                    bounds: (12, 0, 4, 4),
                    centre: None,
                }],
            },
        ],
    }
}

fn loaded(name: &str) -> (App, Handle<Aseprite>) {
    let (app, handles) = support::load(name, &fixture(), &[""]);
    let handle = handles.into_iter().next().expect("one handle");
    (app, handle)
}

#[test]
fn size_is_the_slices_own_rect() {
    let (app, handle) = loaded("slice_geometry_size");
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let aseprite = aseprites.get(&handle).expect("composite loaded");

    assert_eq!(
        aseprite.slice("Panel").expect("Panel").size(),
        Vec2::new(12.0, 10.0),
        "the slice reports the size it was authored at, not the canvas's",
    );
    assert_eq!(
        aseprite.slice("Icon").expect("Icon").size(),
        Vec2::splat(4.0),
    );
}

#[test]
fn border_is_the_authored_centres_insets() {
    let (app, handle) = loaded("slice_geometry_border");
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let panel = aseprites
        .get(&handle)
        .expect("composite loaded")
        .slice("Panel")
        .expect("Panel");

    assert_eq!(
        panel.border(),
        Some(BorderRect {
            min_inset: Vec2::new(3.0, 2.0),
            max_inset: Vec2::new(3.0, 3.0),
        }),
        "a 6x5 centre at (3, 2) of a 12x10 slice leaves 3 left, 2 top, 3 right, 3 bottom",
    );
    assert_eq!(
        panel.border(),
        Some(nine_patch_to_slicer(panel.nine_patch.expect("centre"), panel.size()).border),
        "the accessor agrees with the conversion callers used to write by hand",
    );
}

#[test]
fn a_slice_without_a_centre_has_no_border() {
    let (app, handle) = loaded("slice_geometry_no_border");
    let aseprites = app.world().resource::<Assets<Aseprite>>();

    assert_eq!(
        aseprites
            .get(&handle)
            .expect("composite loaded")
            .slice("Icon")
            .expect("Icon")
            .border(),
        None,
        "a slice nobody gave a centre has no insets to report",
    );
}

#[test]
fn looking_up_an_unknown_slice_finds_nothing() {
    let (app, handle) = loaded("slice_geometry_lookup");
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let aseprite = aseprites.get(&handle).expect("composite loaded");

    assert!(aseprite.slice("Panel").is_some());
    assert!(aseprite.slice("panel").is_none(), "names are exact");
    assert!(aseprite.slice("Missing").is_none());
}

/// The component carries the handle and the name together, so it can answer
/// for its own geometry without a caller pairing the two by hand.
#[test]
fn an_ase_slice_resolves_its_own_geometry() {
    let (app, handle) = loaded("slice_geometry_component");
    let aseprites = app.world().resource::<Assets<Aseprite>>();

    let panel = AseSlice::new(handle.clone(), "Panel");
    assert_eq!(panel.size(aseprites), Some(Vec2::new(12.0, 10.0)));
    assert_eq!(
        panel.border(aseprites),
        Some(BorderRect {
            min_inset: Vec2::new(3.0, 2.0),
            max_inset: Vec2::new(3.0, 3.0),
        }),
    );

    let icon = AseSlice::new(handle.clone(), "Icon");
    assert_eq!(icon.size(aseprites), Some(Vec2::new(4.0, 4.0)));
    assert_eq!(
        icon.border(aseprites),
        None,
        "a slice with no centre reports no insets",
    );

    let missing = AseSlice::new(handle, "Missing");
    assert!(missing.meta(aseprites).is_none());
    assert!(missing.size(aseprites).is_none());
    assert!(missing.border(aseprites).is_none());
}

/// A handle that has not loaded resolves to nothing rather than panicking.
#[test]
fn an_ase_slice_on_an_unloaded_sheet_resolves_to_nothing() {
    let (app, _handle) = loaded("slice_geometry_unloaded");
    let aseprites = app.world().resource::<Assets<Aseprite>>();

    let dangling = AseSlice::new(Handle::default(), "Panel");
    assert!(dangling.meta(aseprites).is_none());
    assert!(dangling.size(aseprites).is_none());
    assert!(dangling.border(aseprites).is_none());
}

/// A slice whose rect collapsed to nothing on some frame still carries the
/// pivot the artist set, and dividing by that empty rect is what an anchor is
/// built from.
fn collapsed(rect: Rect) -> Aseprite {
    Aseprite::builder()
        .with_slice_meta(
            "Collapsed",
            SliceMeta {
                rect,
                atlas_id: 0,
                pivot: Some(Vec2::new(2.0, 2.0)),
                nine_patch: None,
                keys: Vec::new(),
                frame_atlas_ids: Vec::new(),
            },
        )
        .build()
}

#[test]
fn a_zero_area_slice_anchors_at_its_centre() {
    for rect in [
        Rect::new(0.0, 0.0, 0.0, 0.0),
        Rect::new(4.0, 4.0, 4.0, 12.0),
        Rect::new(4.0, 4.0, 12.0, 4.0),
    ] {
        let aseprite = collapsed(rect);
        let meta = aseprite.slice("Collapsed").expect("the slice that went in");
        let anchor = Anchor::from(meta);

        assert!(
            anchor.0.is_finite(),
            "a {rect:?} slice anchored at {:?}",
            anchor.0,
        );
        assert_eq!(anchor, Anchor::CENTER);
    }
}

/// A pivot on a slice with area still places the anchor.
#[test]
fn a_pivot_places_the_anchor_within_the_slice() {
    let aseprite = collapsed(Rect::new(0.0, 0.0, 8.0, 8.0));
    let meta = aseprite.slice("Collapsed").expect("the slice that went in");

    assert_eq!(Anchor::from(meta), Anchor(Vec2::new(-0.25, 0.25)));
}
