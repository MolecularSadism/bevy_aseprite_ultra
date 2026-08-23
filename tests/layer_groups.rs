//! Group layers and duplicated layer names.
//!
//! Aseprite names are unique only inside their group, and a group holds no
//! cels of its own. The fixture below is the shape that breaks a flat
//! name-keyed index: two groups, each with a child called `Main`.

use bevy::{
    asset::{AssetPlugin, LoadState},
    image::Image,
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;
use std::path::PathBuf;

const CANVAS: (u16, u16) = (2, 1);
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// Builds a one-frame RGBA file: `Alpha > Main` paints the left pixel red,
/// `Beta > Main` paints the right pixel blue.
fn grouped_fixture() -> Vec<u8> {
    fn u16le(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn chunk(out: &mut Vec<u8>, kind: u16, body: &[u8]) {
        u32le(out, (body.len() + 6) as u32);
        u16le(out, kind);
        out.extend_from_slice(body);
    }

    fn layer_chunk(name: &str, group: bool, child_level: u16) -> Vec<u8> {
        let mut body = Vec::new();
        u16le(&mut body, 1); // flags: visible
        u16le(&mut body, u16::from(group)); // 0 normal, 1 group
        u16le(&mut body, child_level);
        u16le(&mut body, 0); // default width, ignored
        u16le(&mut body, 0); // default height, ignored
        u16le(&mut body, 0); // blend mode: normal
        body.push(255); // opacity
        body.extend_from_slice(&[0; 3]); // reserved
        u16le(&mut body, name.len() as u16);
        body.extend_from_slice(name.as_bytes());
        body
    }

    fn cel_chunk(layer_index: u16, x: i16, y: i16, pixel: [u8; 4]) -> Vec<u8> {
        let mut body = Vec::new();
        u16le(&mut body, layer_index);
        body.extend_from_slice(&x.to_le_bytes());
        body.extend_from_slice(&y.to_le_bytes());
        body.push(255); // opacity
        u16le(&mut body, 0); // cel type: raw image data
        body.extend_from_slice(&0i16.to_le_bytes()); // z-index
        body.extend_from_slice(&[0; 5]); // reserved
        u16le(&mut body, 1); // width
        u16le(&mut body, 1); // height
        body.extend_from_slice(&pixel);
        body
    }

    let mut chunks = Vec::new();
    chunk(&mut chunks, 0x2004, &layer_chunk("Alpha", true, 0));
    chunk(&mut chunks, 0x2004, &layer_chunk("Main", false, 1));
    chunk(&mut chunks, 0x2004, &layer_chunk("Beta", true, 0));
    chunk(&mut chunks, 0x2004, &layer_chunk("Main", false, 1));
    chunk(&mut chunks, 0x2005, &cel_chunk(1, 0, 0, RED));
    chunk(&mut chunks, 0x2005, &cel_chunk(3, 1, 0, BLUE));
    let chunk_count = 6u32;

    let mut frame = Vec::new();
    u32le(&mut frame, (chunks.len() + 16) as u32);
    u16le(&mut frame, 0xF1FA); // frame magic
    u16le(&mut frame, 0); // old chunk count, superseded below
    u16le(&mut frame, 100); // duration ms
    frame.extend_from_slice(&[0; 2]); // reserved
    u32le(&mut frame, chunk_count);
    frame.extend_from_slice(&chunks);

    let mut header = Vec::new();
    u32le(&mut header, (frame.len() + 128) as u32);
    u16le(&mut header, 0xA5E0); // file magic
    u16le(&mut header, 1); // frames
    u16le(&mut header, CANVAS.0);
    u16le(&mut header, CANVAS.1);
    u16le(&mut header, 32); // colour depth: RGBA
    u32le(&mut header, 1); // flags: layer opacity valid
    u16le(&mut header, 100); // speed, deprecated
    u32le(&mut header, 0);
    u32le(&mut header, 0);
    header.push(0); // transparent index
    header.extend_from_slice(&[0; 3]); // ignored
    u16le(&mut header, 0); // colour count
    header.push(1); // pixel width
    header.push(1); // pixel height
    header.extend_from_slice(&0i16.to_le_bytes()); // grid x
    header.extend_from_slice(&0i16.to_le_bytes()); // grid y
    u16le(&mut header, 16); // grid width
    u16le(&mut header, 16); // grid height
    header.extend_from_slice(&[0; 84]); // reserved

    header.extend_from_slice(&frame);
    header
}

/// Writes the fixture into a scratch asset directory and loads every label off
/// it, returning the app plus the handles keyed by label.
fn load_fixture(labels: &[&str]) -> (App, Vec<Handle<Aseprite>>) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("layer_groups");
    std::fs::create_dir_all(&dir).expect("scratch asset dir");
    std::fs::write(dir.join("groups.aseprite"), grouped_fixture()).expect("write fixture");

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: dir.to_string_lossy().into_owned(),
            ..default()
        },
        AsepriteLoaderPlugin,
    ));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();

    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<Aseprite>> = labels
        .iter()
        .map(|label| {
            if label.is_empty() {
                server.load("groups.aseprite")
            } else {
                server.load(format!("groups.aseprite#{label}"))
            }
        })
        .collect();

    for _ in 0..1000 {
        app.update();
        let ready = handles.iter().all(|handle| {
            matches!(
                server.get_load_state(handle.id()),
                Some(LoadState::Loaded) | None
            )
        });
        if ready && handles.iter().all(|h| {
            app.world().resource::<Assets<Aseprite>>().get(h).is_some()
        }) {
            return (app, handles);
        }
        if let Some(LoadState::Failed(error)) = handles
            .iter()
            .find_map(|handle| server.get_load_state(handle.id()))
            .filter(|state| matches!(state, LoadState::Failed(_)))
        {
            panic!("fixture failed to load: {error:?}");
        }
    }
    panic!("fixture did not load within 1000 update cycles");
}

#[test]
fn duplicate_layer_names_are_qualified_by_their_group() {
    let (app, handles) = load_fixture(&[""]);
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
    let (app, handles) = load_fixture(&["Alpha", "Beta", "Alpha/Main"]);
    let world = app.world();
    let aseprites = world.resource::<Assets<Aseprite>>();
    let images = world.resource::<Assets<Image>>();
    let layouts = world.resource::<Assets<TextureAtlasLayout>>();

    let first_frame_pixels = |handle: &Handle<Aseprite>| -> Vec<[u8; 4]> {
        let aseprite = aseprites.get(handle).expect("sub-asset loaded");
        let layout = layouts.get(&aseprite.atlas_layout).expect("atlas layout");
        let image = images.get(&aseprite.atlas_image).expect("atlas image");
        let rect = layout.textures[aseprite.get_atlas_index(0)];
        let width = image.width();
        (rect.min.y..rect.max.y)
            .flat_map(|y| (rect.min.x..rect.max.x).map(move |x| (x, y)))
            .map(|(x, y)| {
                let start = ((y * width + x) * 4) as usize;
                let data = image.data.as_ref().expect("atlas pixel data");
                [
                    data[start],
                    data[start + 1],
                    data[start + 2],
                    data[start + 3],
                ]
            })
            .collect()
    };

    let alpha = first_frame_pixels(&handles[0]);
    let beta = first_frame_pixels(&handles[1]);
    let alpha_main = first_frame_pixels(&handles[2]);

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
