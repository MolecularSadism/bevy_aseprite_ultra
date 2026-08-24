//! Builds Aseprite files in memory for tests.
//!
//! Writing the bytes here keeps a fixture's shape — which layers nest where,
//! which slice key sets a centre — visible in the test that depends on it,
//! and keeps binary art out of the repository.

#![allow(dead_code)]

use bevy::{
    app::Plugins,
    asset::{AssetPlugin, LoadState},
    image::Image,
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;
use std::path::PathBuf;

/// A layer of the file being built.
pub struct Layer {
    pub name: &'static str,
    pub group: bool,
    pub child_level: u16,
}

impl Layer {
    pub fn normal(name: &'static str, child_level: u16) -> Self {
        Self {
            name,
            group: false,
            child_level,
        }
    }

    pub fn group(name: &'static str, child_level: u16) -> Self {
        Self {
            name,
            group: true,
            child_level,
        }
    }
}

/// A one-pixel cel, which is all these fixtures need to tell layers apart.
pub struct Cel {
    pub frame: usize,
    pub layer_index: u16,
    pub position: (i16, i16),
    pub colour: [u8; 4],
}

/// A slice key. `centre` is Aseprite's nine-patch centre, in slice-local
/// coordinates; `None` leaves the key without one.
pub struct SliceKey {
    pub frame: u32,
    pub bounds: (i32, i32, u32, u32),
    pub centre: Option<(i32, i32, u32, u32)>,
}

pub struct Slice {
    pub name: &'static str,
    pub keys: Vec<SliceKey>,
}

/// The file to build.
pub struct Fixture {
    pub canvas: (u16, u16),
    pub frames: usize,
    pub frame_duration: u16,
    pub layers: Vec<Layer>,
    pub cels: Vec<Cel>,
    pub slices: Vec<Slice>,
}

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

fn layer_chunk(layer: &Layer) -> Vec<u8> {
    let mut body = Vec::new();
    u16le(&mut body, 1); // flags: visible
    u16le(&mut body, u16::from(layer.group)); // 0 normal, 1 group
    u16le(&mut body, layer.child_level);
    u16le(&mut body, 0); // default width, ignored
    u16le(&mut body, 0); // default height, ignored
    u16le(&mut body, 0); // blend mode: normal
    body.push(255); // opacity
    body.extend_from_slice(&[0; 3]); // reserved
    u16le(&mut body, layer.name.len() as u16);
    body.extend_from_slice(layer.name.as_bytes());
    body
}

fn cel_chunk(cel: &Cel) -> Vec<u8> {
    let mut body = Vec::new();
    u16le(&mut body, cel.layer_index);
    body.extend_from_slice(&cel.position.0.to_le_bytes());
    body.extend_from_slice(&cel.position.1.to_le_bytes());
    body.push(255); // opacity
    u16le(&mut body, 0); // cel type: raw image data
    body.extend_from_slice(&0i16.to_le_bytes()); // z-index
    body.extend_from_slice(&[0; 5]); // reserved
    u16le(&mut body, 1); // width
    u16le(&mut body, 1); // height
    body.extend_from_slice(&cel.colour);
    body
}

fn slice_chunk(slice: &Slice) -> Vec<u8> {
    let mut body = Vec::new();
    u32le(&mut body, slice.keys.len() as u32);
    // Nine-patch keys are a per-slice flag, so a slice with any centre writes
    // the field on every key — an absent centre is written as an empty one,
    // which is how the files Aseprite exports encode it too.
    let nine_patch = slice.keys.iter().any(|key| key.centre.is_some());
    u32le(&mut body, u32::from(nine_patch));
    u32le(&mut body, 0); // reserved
    u16le(&mut body, slice.name.len() as u16);
    body.extend_from_slice(slice.name.as_bytes());
    for key in &slice.keys {
        u32le(&mut body, key.frame);
        body.extend_from_slice(&key.bounds.0.to_le_bytes());
        body.extend_from_slice(&key.bounds.1.to_le_bytes());
        u32le(&mut body, key.bounds.2);
        u32le(&mut body, key.bounds.3);
        if nine_patch {
            let centre = key.centre.unwrap_or((0, 0, 0, 0));
            body.extend_from_slice(&centre.0.to_le_bytes());
            body.extend_from_slice(&centre.1.to_le_bytes());
            u32le(&mut body, centre.2);
            u32le(&mut body, centre.3);
        }
    }
    body
}

/// Serialises the fixture to Aseprite's binary format.
pub fn build(fixture: &Fixture) -> Vec<u8> {
    let mut frames = Vec::new();
    for index in 0..fixture.frames {
        let mut chunks = Vec::new();
        let mut count = 0u32;

        // Layers and slices are file-wide, so they ride on the first frame.
        if index == 0 {
            for layer in &fixture.layers {
                chunk(&mut chunks, 0x2004, &layer_chunk(layer));
                count += 1;
            }
            for slice in &fixture.slices {
                chunk(&mut chunks, 0x2022, &slice_chunk(slice));
                count += 1;
            }
        }
        for cel in fixture.cels.iter().filter(|cel| cel.frame == index) {
            chunk(&mut chunks, 0x2005, &cel_chunk(cel));
            count += 1;
        }

        u32le(&mut frames, (chunks.len() + 16) as u32);
        u16le(&mut frames, 0xF1FA); // frame magic
        u16le(&mut frames, 0); // old chunk count, superseded by the DWORD below
        u16le(&mut frames, fixture.frame_duration);
        frames.extend_from_slice(&[0; 2]); // reserved
        u32le(&mut frames, count);
        frames.extend_from_slice(&chunks);
    }

    let mut file = Vec::new();
    u32le(&mut file, (frames.len() + 128) as u32);
    u16le(&mut file, 0xA5E0); // file magic
    u16le(&mut file, fixture.frames as u16);
    u16le(&mut file, fixture.canvas.0);
    u16le(&mut file, fixture.canvas.1);
    u16le(&mut file, 32); // colour depth: RGBA
    u32le(&mut file, 1); // flags: layer opacity valid
    u16le(&mut file, 100); // speed, deprecated
    u32le(&mut file, 0);
    u32le(&mut file, 0);
    file.push(0); // transparent index
    file.extend_from_slice(&[0; 3]); // ignored
    u16le(&mut file, 0); // colour count
    file.push(1); // pixel width
    file.push(1); // pixel height
    file.extend_from_slice(&0i16.to_le_bytes()); // grid x
    file.extend_from_slice(&0i16.to_le_bytes()); // grid y
    u16le(&mut file, 16); // grid width
    u16le(&mut file, 16); // grid height
    file.extend_from_slice(&[0; 84]); // reserved
    file.extend_from_slice(&frames);
    file
}

/// Writes the fixture into a scratch asset directory and loads the given
/// labels off it, returning the app and one handle per label. An empty label
/// loads the unlabelled composite.
pub fn load(name: &str, fixture: &Fixture, labels: &[&str]) -> (App, Vec<Handle<Aseprite>>) {
    load_with(name, fixture, labels, AsepriteLoaderPlugin)
}

/// [`load`], with the aseprite plugins the test needs in place of the bare
/// loader — `AsepriteUltraPlugin` when the test drives components rather than
/// reading asset data.
pub fn load_with<P: Plugins<M>, M>(
    name: &str,
    fixture: &Fixture,
    labels: &[&str],
    plugins: P,
) -> (App, Vec<Handle<Aseprite>>) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).expect("scratch asset dir");
    let file = format!("{name}.aseprite");
    std::fs::write(dir.join(&file), build(fixture)).expect("write fixture");

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: dir.to_string_lossy().into_owned(),
            ..default()
        },
    ));
    app.add_plugins(plugins);
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();

    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<Aseprite>> = labels
        .iter()
        .map(|label| {
            if label.is_empty() {
                server.load(file.clone())
            } else {
                server.load(format!("{file}#{label}"))
            }
        })
        .collect();

    for _ in 0..1000 {
        app.update();
        if let Some(error) =
            handles
                .iter()
                .find_map(|handle| match server.get_load_state(handle.id()) {
                    Some(LoadState::Failed(error)) => Some(error),
                    _ => None,
                })
        {
            panic!("fixture failed to load: {error:?}");
        }
        let assets = app.world().resource::<Assets<Aseprite>>();
        if handles.iter().all(|handle| assets.get(handle).is_some()) {
            return (app, handles);
        }
    }
    panic!("fixture did not load within 1000 update cycles");
}

/// The pixels of a sub-asset's first frame, read back out of the packed atlas.
pub fn first_frame_pixels(app: &App, handle: &Handle<Aseprite>) -> Vec<[u8; 4]> {
    let world = app.world();
    let aseprite = world
        .resource::<Assets<Aseprite>>()
        .get(handle)
        .expect("sub-asset loaded");
    let layout = world
        .resource::<Assets<TextureAtlasLayout>>()
        .get(&aseprite.atlas_layout)
        .expect("atlas layout");
    let image = world
        .resource::<Assets<Image>>()
        .get(&aseprite.atlas_image)
        .expect("atlas image");
    let rect = layout.textures[aseprite.get_atlas_index(0)];
    let width = image.width();
    let data = image.data.as_ref().expect("atlas pixel data");
    (rect.min.y..rect.max.y)
        .flat_map(|y| (rect.min.x..rect.max.x).map(move |x| (x, y)))
        .map(|(x, y)| {
            let start = ((y * width + x) * 4) as usize;
            [
                data[start],
                data[start + 1],
                data[start + 2],
                data[start + 3],
            ]
        })
        .collect()
}
