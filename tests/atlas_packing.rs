//! How a file's frames share the packed atlas.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture};

/// Frames that render the same pixels share one atlas entry, across frames
/// and across variants; a frame that differs keeps its own.
#[test]
fn identical_frames_share_one_atlas_entry() {
    let fixture = Fixture {
        canvas: (4, 4),
        frames: 3,
        frame_duration: 100,
        layers: vec![
            support::Layer::normal("still", 0),
            support::Layer::normal("blink", 0),
        ],
        cels: vec![
            Cel {
                frame: 0,
                layer_index: 0,
                position: (0, 0),
                colour: [255, 0, 0, 255],
            },
            Cel {
                frame: 1,
                layer_index: 0,
                position: (0, 0),
                colour: [255, 0, 0, 255],
            },
            Cel {
                frame: 2,
                layer_index: 0,
                position: (0, 0),
                colour: [255, 0, 0, 255],
            },
            Cel {
                frame: 2,
                layer_index: 1,
                position: (2, 2),
                colour: [0, 0, 255, 255],
            },
        ],
        slices: Vec::new(),
    };

    let (app, handles) = support::load("identical_frames", &fixture, &["", "still"]);
    let assets = app.world().resource::<Assets<Aseprite>>();
    let composite = assets.get(&handles[0]).expect("composite loaded");
    let still = assets.get(&handles[1]).expect("layer loaded");

    assert_eq!(composite.atlas_index(0), composite.atlas_index(1));
    assert_ne!(
        composite.atlas_index(1),
        composite.atlas_index(2),
        "the frame the blinking layer draws on keeps its own entry"
    );
    assert_eq!(still.atlas_index(0), still.atlas_index(2));
    assert_eq!(
        still.atlas_index(0),
        composite.atlas_index(0),
        "a layer drawing the same pixels as the composite shares its entry"
    );
}
