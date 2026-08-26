//! The order and visibility a file's layers reach the asset with.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Fixture, Layer};

/// A file that is nothing but its layer stack, written bottom-to-top the way
/// Aseprite stores it.
fn stack(layers: Vec<Layer>) -> Fixture {
    Fixture {
        canvas: (4, 4),
        frames: 1,
        frame_duration: 100,
        layers,
        cels: Vec::new(),
        slices: Vec::new(),
    }
}

fn layers_of(name: &str, fixture: &Fixture) -> (App, Handle<Aseprite>) {
    let (app, handles) = support::load(name, fixture, &[""]);
    let handle = handles.into_iter().next().expect("one handle");
    (app, handle)
}

#[test]
fn layer_ids_run_front_to_back() {
    let fixture = stack(vec![
        Layer::normal("bottom", 0),
        Layer::normal("middle", 0),
        Layer::normal("top", 0),
    ]);
    let (app, handle) = layers_of("layer_order_front_to_back", &fixture);
    let aseprite = app
        .world()
        .resource::<Assets<Aseprite>>()
        .get(&handle)
        .expect("composite loaded");

    assert_eq!(
        aseprite.layer_ids().collect::<Vec<_>>(),
        vec![
            LayerId::new("top"),
            LayerId::new("middle"),
            LayerId::new("bottom"),
        ],
        "index 0 is the layer the editor draws in front"
    );
}

#[test]
fn visible_layer_ids_skip_what_the_file_hid() {
    let fixture = stack(vec![
        Layer::normal("bottom", 0),
        Layer::hidden("middle", 0),
        Layer::normal("top", 0),
    ]);
    let (app, handle) = layers_of("layer_order_visibility", &fixture);
    let aseprite = app
        .world()
        .resource::<Assets<Aseprite>>()
        .get(&handle)
        .expect("composite loaded");

    assert_eq!(
        aseprite.visible_layer_ids().collect::<Vec<_>>(),
        vec![LayerId::new("top"), LayerId::new("bottom")]
    );
    assert_eq!(
        aseprite.layer_ids().count(),
        3,
        "a hidden layer still exists, it is only not visible"
    );
}
