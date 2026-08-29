//! Loading a file's composite keeps its per-layer sub-assets resident, so a
//! later `#layer` request is a lookup rather than a fresh decode of the file.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::Aseprite;
use support::{Cel, Fixture, Layer, load};

const FIXTURE: &str = "layer_retention";

/// Two normal, differently named layers, so the file exposes `#Blue`, `#Red`
/// and `#all` sub-assets alongside its composite.
fn two_layer_file() -> Fixture {
    Fixture {
        canvas: (1, 1),
        frames: 1,
        frame_duration: 100,
        layers: vec![Layer::normal("Blue", 0), Layer::normal("Red", 0)],
        cels: vec![
            Cel {
                frame: 0,
                layer_index: 0,
                position: (0, 0),
                colour: [0, 0, 255, 255],
            },
            Cel {
                frame: 0,
                layer_index: 1,
                position: (0, 0),
                colour: [255, 0, 0, 255],
            },
        ],
        slices: vec![],
    }
}

#[test]
fn layers_stay_resident_once_the_composite_has_loaded() {
    // Loads the unlabelled composite alone — no `#layer` handle is ever
    // requested, so the sub-assets are resident only if the file kept them.
    let (app, _composite) = load(FIXTURE, &two_layer_file(), &[""]);

    let server = app.world().resource::<AssetServer>();
    let assets = app.world().resource::<Assets<Aseprite>>();

    for label in ["all", "Blue", "Red"] {
        let handle = server
            .get_handle::<Aseprite>(format!("{FIXTURE}.aseprite#{label}"))
            .unwrap_or_else(|| {
                panic!("#{label} was dropped instead of kept resident with its file")
            });
        assert!(
            assets.get(&handle).is_some(),
            "#{label} has no data despite its file being loaded",
        );
    }
}
