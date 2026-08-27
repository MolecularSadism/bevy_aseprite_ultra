//! Tag ranges against the frames a file actually has.
//!
//! Deleting frames in Aseprite leaves any tag that covered them reaching past
//! the end of the file. Every frame a tag names has to resolve to one the file
//! can time, so the loader clamps the range to the frames that exist.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer, Tag};

const WHITE: [u8; 4] = [255, 255, 255, 255];

/// Four frames of one-pixel art, the smallest file a tag can overrun.
fn four_frames() -> Fixture {
    Fixture {
        canvas: (4, 4),
        frames: 4,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: (0..4)
            .map(|frame| Cel {
                frame,
                layer_index: 0,
                position: (0, 0),
                colour: WHITE,
            })
            .collect(),
        slices: Vec::new(),
    }
}

fn loaded(name: &str, tags: &[Tag]) -> (App, Handle<Aseprite>) {
    let (app, handles) = support::load_tagged(name, &four_frames(), tags, &[""]);
    let handle = handles.into_iter().next().expect("one handle");
    (app, handle)
}

#[test]
fn a_tag_reaching_past_the_last_frame_is_clamped_to_it() {
    let (app, handle) = loaded(
        "tag_overrun",
        &[Tag {
            name: "overrun",
            range: (2, 9),
        }],
    );
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let aseprite = aseprites.get(&handle).expect("composite loaded");

    assert_eq!(
        aseprite.tag("overrun").map(|tag| tag.range.clone()),
        Some(2..=3),
        "a tag over a four-frame file ends on frame 3",
    );
}

/// A tag whose whole range was deleted out from under it lands on the last
/// frame rather than on a range no frame satisfies.
#[test]
fn a_tag_entirely_past_the_end_lands_on_the_last_frame() {
    let (app, handle) = loaded(
        "tag_past_end",
        &[Tag {
            name: "gone",
            range: (6, 9),
        }],
    );
    let aseprites = app.world().resource::<Assets<Aseprite>>();

    assert_eq!(
        aseprites
            .get(&handle)
            .expect("composite loaded")
            .tag("gone")
            .map(|tag| tag.range.clone()),
        Some(3..=3),
    );
}

#[test]
fn a_tag_within_the_file_keeps_the_range_it_was_authored_with() {
    let (app, handle) = loaded(
        "tag_in_range",
        &[Tag {
            name: "walk",
            range: (1, 3),
        }],
    );
    let aseprites = app.world().resource::<Assets<Aseprite>>();

    assert_eq!(
        aseprites
            .get(&handle)
            .expect("composite loaded")
            .tag("walk")
            .map(|tag| tag.range.clone()),
        Some(1..=3),
    );
}

/// The frame an animation resolves to is the one it waits on, so every frame
/// a clamped tag can reach has a duration to wait for. Without one the tick
/// finds nothing to time and the entity stops advancing.
#[test]
fn every_frame_a_tag_resolves_to_has_a_duration() {
    let (app, handle) = loaded(
        "tag_resolves",
        &[Tag {
            name: "overrun",
            range: (2, 9),
        }],
    );
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let aseprite = aseprites.get(&handle).expect("composite loaded");

    let tag = AseTag::new("overrun");
    for offset in 0..16u16 {
        let absolute = resolve_frame(aseprite, AseFrame(offset), Some(&tag));
        assert!(
            aseprite
                .frame_durations()
                .get(usize::from(absolute))
                .is_some(),
            "offset {offset} resolved to frame {absolute}, which the file does not have",
        );
    }
}
