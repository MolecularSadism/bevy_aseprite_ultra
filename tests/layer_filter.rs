use aseprite_loader::loader::AsepriteFile;
use bevy::{
    asset::{AssetPlugin, LoadState},
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;

const LAYERS_ASE: &str = "layers.aseprite";
const LAYERS_ASE_PATH: &str = "assets/layers.aseprite";
const ASSET_DIR: &str = "assets";

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: ASSET_DIR.to_string(),
            ..default()
        },
        AsepriteLoaderPlugin,
    ));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();
    app
}

fn load_until_ready(app: &mut App, handle: &Handle<Aseprite>) {
    for _ in 0..1000 {
        app.update();
        match app
            .world()
            .resource::<AssetServer>()
            .get_load_state(handle.id())
        {
            Some(LoadState::Loaded) => return,
            Some(LoadState::Failed(e)) => panic!("asset failed to load: {e:?}"),
            _ => {}
        }
    }
    panic!("asset did not load within 1000 update cycles");
}

fn atlas_pixel_data(app: &App, handle: &Handle<Aseprite>) -> Vec<u8> {
    let world = app.world();
    let atlas_handle = world
        .resource::<Assets<Aseprite>>()
        .get(handle)
        .unwrap()
        .atlas_image
        .clone();
    world
        .resource::<Assets<Image>>()
        .get(&atlas_handle)
        .unwrap()
        .data
        .clone()
        .unwrap_or_default()
}

/// Extract the raw RGBA bytes for a single composite frame from the shared atlas.
///
/// The atlas is shared across composite, all-layers, and per-layer renders.
/// Checking the whole atlas image is meaningless when testing a custom layer
/// filter — use this instead to inspect only the composite frame cells.
fn composite_frame_pixel_data(app: &App, handle: &Handle<Aseprite>, frame: usize) -> Vec<u8> {
    let world = app.world();
    let ase = world.resource::<Assets<Aseprite>>().get(handle).unwrap();

    let atlas_index = ase.atlas_index(frame).expect("composite has frames");
    let layout = world
        .resource::<Assets<TextureAtlasLayout>>()
        .get(&ase.atlas_layout)
        .unwrap();
    let rect = layout.textures[atlas_index];

    let image = world
        .resource::<Assets<Image>>()
        .get(&ase.atlas_image)
        .unwrap();
    let data = image.data.as_ref().unwrap();
    let atlas_width = image.texture_descriptor.size.width as usize;

    let x0 = rect.min.x as usize;
    let y0 = rect.min.y as usize;
    let w = (rect.max.x - rect.min.x) as usize;
    let h = (rect.max.y - rect.min.y) as usize;

    let mut pixels = Vec::with_capacity(w * h * 4);
    for row in y0..y0 + h {
        let start = (row * atlas_width + x0) * 4;
        pixels.extend_from_slice(&data[start..start + w * 4]);
    }
    pixels
}

#[test]
fn empty_layer_filter_produces_blank_atlas() {
    let mut app = make_app();

    let handle: Handle<Aseprite> = app
        .world_mut()
        .resource::<AssetServer>()
        .load_with_settings(LAYERS_ASE, |s: &mut AsepriteLoaderSettings| {
            s.visible_layers = Some(vec![]);
        });

    load_until_ready(&mut app, &handle);

    // The atlas is shared with all-layers and per-layer renders, so the full
    // atlas image is never blank. Check only the composite frame's pixel region.
    let pixels = composite_frame_pixel_data(&app, &handle, 0);
    assert!(
        pixels.iter().all(|&b| b == 0),
        "empty layer filter should produce a blank composite frame"
    );
}

#[test]
fn single_layer_filter_differs_from_all_layers() {
    let bytes = std::fs::read(LAYERS_ASE_PATH).unwrap();
    let raw = AsepriteFile::load(&bytes).unwrap();
    let visible: Vec<String> = raw
        .layers()
        .iter()
        .filter(|l| l.visible)
        .map(|l| l.name.clone())
        .collect();
    assert!(
        visible.len() >= 2,
        "layers.aseprite needs at least 2 visible layers for this test"
    );

    let mut app_all = make_app();
    let mut app_one = make_app();

    let handle_all: Handle<Aseprite> = app_all
        .world_mut()
        .resource::<AssetServer>()
        .load_with_settings(LAYERS_ASE, |s: &mut AsepriteLoaderSettings| {
            s.visible_layers = None;
        });

    let first = visible[0].clone();
    let handle_one: Handle<Aseprite> = app_one
        .world_mut()
        .resource::<AssetServer>()
        .load_with_settings(LAYERS_ASE, move |s: &mut AsepriteLoaderSettings| {
            s.visible_layers = Some(vec![first.clone()]);
        });

    load_until_ready(&mut app_all, &handle_all);
    load_until_ready(&mut app_one, &handle_one);

    let data_all = atlas_pixel_data(&app_all, &handle_all);
    let data_one = atlas_pixel_data(&app_one, &handle_one);

    assert_ne!(
        data_all, data_one,
        "single-layer composite should differ from all-layers composite"
    );
}
