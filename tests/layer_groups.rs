//! Group layers and duplicated layer names.
//!
//! Aseprite names are unique only inside their group, and a group holds no
//! cels of its own. The fixture below is the shape that breaks a flat
//! name-keyed index: two groups, each with a child called `Main`.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// `Alpha > Main` paints the left pixel red, `Beta > Main` the right one blue.
fn grouped() -> Fixture {
    Fixture {
        canvas: (2, 1),
        frames: 1,
        frame_duration: 100,
        layers: vec![
            Layer::group("Alpha", 0),
            Layer::normal("Main", 1),
            Layer::group("Beta", 0),
            Layer::normal("Main", 1),
        ],
        cels: vec![
            Cel {
                frame: 0,
                layer_index: 1,
                position: (0, 0),
                colour: RED,
            },
            Cel {
                frame: 0,
                layer_index: 3,
                position: (1, 0),
                colour: BLUE,
            },
        ],
        slices: Vec::new(),
    }
}

#[test]
fn duplicate_layer_names_are_qualified_by_their_group() {
    let (app, handles) = support::load("groups", &grouped(), &[""]);
    let aseprites = app.world().resource::<Assets<Aseprite>>();
    let aseprite = aseprites.get(&handles[0]).expect("composite loaded");

    let ids: Vec<LayerId> = aseprite.layer_ids().collect();
    // Front-to-back: the file's bottom-to-top order, reversed.
    assert_eq!(
        ids,
        vec![
            LayerId::new("Beta/Main"),
            LayerId::new("Beta"),
            LayerId::new("Alpha/Main"),
            LayerId::new("Alpha"),
        ],
        "each layer needs an id that addresses only itself",
    );
}

#[test]
fn a_group_renders_the_layers_beneath_it() {
    let (app, handles) = support::load("groups", &grouped(), &["Alpha", "Beta", "Alpha/Main"]);

    let alpha = support::first_frame_pixels(&app, &handles[0]);
    let beta = support::first_frame_pixels(&app, &handles[1]);
    let alpha_main = support::first_frame_pixels(&app, &handles[2]);

    assert!(
        alpha.contains(&RED),
        "the Alpha group must draw its child's red pixel, got {alpha:?}",
    );
    assert!(
        !alpha.contains(&BLUE),
        "the Alpha group must not draw Beta's pixel, got {alpha:?}",
    );
    assert!(
        beta.contains(&BLUE) && !beta.contains(&RED),
        "the Beta group must draw only its own child, got {beta:?}",
    );
    assert_eq!(
        alpha, alpha_main,
        "a group with one child renders exactly that child",
    );
}
