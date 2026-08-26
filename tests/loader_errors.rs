//! What the loader does with files it cannot take at face value.
//!
//! A foundation crate loads art an artist can rename, empty or outgrow at any
//! time, and it does so inside an async task where a panic says almost
//! nothing. Every case here must end in a skipped item or a load error.

mod support;

use bevy::{
    asset::{AssetPlugin, LoadState},
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;
use std::path::PathBuf;
use support::{Fixture, Slice, SliceKey};

fn key(bounds: (i32, i32, u32, u32)) -> SliceKey {
    SliceKey {
        frame: 0,
        bounds,
        centre: None,
    }
}

#[test]
fn a_slice_without_keys_is_skipped_and_the_file_still_loads() {
    let fixture = Fixture {
        canvas: (8, 8),
        frames: 1,
        frame_duration: 100,
        layers: vec![support::Layer::normal("art", 0)],
        cels: Vec::new(),
        slices: vec![
            Slice {
                name: "Keyless",
                keys: Vec::new(),
            },
            Slice {
                name: "Panel",
                keys: vec![key((1, 1, 4, 4))],
            },
        ],
    };

    let (app, handles) = support::load("slice_without_keys", &fixture, &[""]);
    let aseprite = app
        .world()
        .resource::<Assets<Aseprite>>()
        .get(&handles[0])
        .expect("composite loaded");

    assert!(
        aseprite.slice("Keyless").is_none(),
        "a slice with no keys describes no region and is dropped"
    );
    assert!(
        aseprite.slice("Panel").is_some(),
        "the rest of the file still loads"
    );
    assert_eq!(aseprite.slices().count(), 1);
}

#[test]
fn atlas_index_clamps_past_the_last_frame() {
    let fixture = Fixture {
        canvas: (4, 4),
        frames: 3,
        frame_duration: 100,
        layers: vec![support::Layer::normal("art", 0)],
        cels: Vec::new(),
        slices: Vec::new(),
    };
    let (app, handles) = support::load("atlas_index_clamp", &fixture, &[""]);
    let aseprite = app
        .world()
        .resource::<Assets<Aseprite>>()
        .get(&handles[0])
        .expect("composite loaded");

    assert_eq!(aseprite.atlas_index(99), aseprite.atlas_index(2));
    assert!(aseprite.atlas_index(0).is_some());
}

#[test]
fn atlas_index_of_a_frameless_asset_is_none() {
    assert_eq!(Aseprite::default().atlas_index(0), None);
}

#[test]
fn a_file_that_outgrows_the_atlas_cap_fails_to_load() {
    let fixture = Fixture {
        canvas: (64, 64),
        frames: 4,
        frame_duration: 100,
        layers: vec![
            support::Layer::normal("back", 0),
            support::Layer::normal("front", 0),
        ],
        cels: Vec::new(),
        slices: Vec::new(),
    };

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("atlas_cap");
    std::fs::create_dir_all(&dir).expect("scratch asset dir");
    let file = "atlas_cap.aseprite";
    std::fs::write(dir.join(file), support::build(&fixture)).expect("write fixture");

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: dir.to_string_lossy().into_owned(),
            ..default()
        },
    ));
    app.add_plugins(AsepriteLoaderPlugin);
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();

    let server = app.world().resource::<AssetServer>().clone();
    let handle: Handle<Aseprite> = server.load_with_settings(file, |settings| {
        *settings = AsepriteLoaderSettings {
            max_atlas_size: 8,
            ..default()
        };
    });

    for _ in 0..1000 {
        app.update();
        match server.get_load_state(handle.id()) {
            Some(LoadState::Failed(error)) => {
                let message = error.to_string();
                assert!(
                    message.contains("atlas"),
                    "the failure should name the atlas: {message}"
                );
                return;
            }
            Some(LoadState::Loaded) => {
                panic!("frames far larger than the cap should not pack into it")
            }
            _ => {}
        }
    }
    panic!("load never settled");
}
