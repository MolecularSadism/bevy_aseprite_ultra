//! Structural hot reload: what happens to the children when a reloaded file
//! has a different layer set than the one they were spawned from.

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), AsepriteUltraPlugin));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();
    app
}

/// A metadata-only file with the given layers, all visible.
fn file(layers: &[&'static str]) -> Aseprite {
    layers
        .iter()
        .fold(Aseprite::builder(), |builder, name| {
            builder.with_layer(*name, true)
        })
        .build()
}

/// The names of `parent`'s layer children, sorted.
fn layer_names(app: &mut App, parent: Entity) -> Vec<String> {
    let world = app.world_mut();
    let mut query = world.query::<(&LayerId, &SpriteLayerOf)>();
    let mut names: Vec<String> = query
        .iter(world)
        .filter(|(_, of)| of.0 == parent)
        .map(|(id, _)| id.as_str().to_owned())
        .collect();
    names.sort();
    names
}

/// The entities of `parent`'s layer children, sorted, so churn is visible.
fn child_entities(app: &mut App, parent: Entity) -> Vec<Entity> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &SpriteLayerOf)>();
    let mut children: Vec<Entity> = query
        .iter(world)
        .filter(|(_, of)| of.0 == parent)
        .map(|(entity, _)| entity)
        .collect();
    children.sort();
    children
}

/// Spawns a layered texture over `layers` and steps until its children exist.
fn spawn(app: &mut App, layers: &[&'static str]) -> (Entity, Handle<Aseprite>) {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .add(file(layers));
    let parent = app
        .world_mut()
        .spawn(AseTexture::new(handle.clone()).sprite())
        .id();
    for _ in 0..4 {
        app.update();
    }
    (parent, handle)
}

/// Replaces the asset behind `handle`, as a reload of a re-saved file does.
fn reload(app: &mut App, handle: &Handle<Aseprite>, layers: &[&'static str]) {
    app.world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .insert(handle.id(), file(layers))
        .expect("replace the loaded asset");
    for _ in 0..4 {
        app.update();
    }
}

#[test]
fn a_reload_that_adds_a_layer_adds_its_child() {
    let mut app = app();
    let (parent, handle) = spawn(&mut app, &["Body"]);
    assert_eq!(layer_names(&mut app, parent), ["Body"]);

    reload(&mut app, &handle, &["Body", "Hat"]);

    assert_eq!(
        layer_names(&mut app, parent),
        ["Body", "Hat"],
        "the layer the reload added never got a child",
    );
}

#[test]
fn a_reload_that_drops_a_layer_drops_its_child() {
    let mut app = app();
    let (parent, handle) = spawn(&mut app, &["Body", "Hat"]);
    assert_eq!(layer_names(&mut app, parent), ["Body", "Hat"]);

    reload(&mut app, &handle, &["Body"]);

    assert_eq!(
        layer_names(&mut app, parent),
        ["Body"],
        "the child of the dropped layer outlived it",
    );
}

#[test]
fn a_reload_that_renames_a_layer_renames_its_child() {
    let mut app = app();
    let (parent, handle) = spawn(&mut app, &["Body"]);

    reload(&mut app, &handle, &["Torso"]);

    assert_eq!(layer_names(&mut app, parent), ["Torso"]);
}

/// A reload that only redrew pixels leaves the same layers, and so must leave
/// the same child entities.
#[test]
fn a_reload_that_keeps_the_layer_set_keeps_the_children() {
    let mut app = app();
    let (parent, handle) = spawn(&mut app, &["Body", "Hat"]);
    let before = child_entities(&mut app, parent);

    reload(&mut app, &handle, &["Body", "Hat"]);

    assert_eq!(
        child_entities(&mut app, parent),
        before,
        "a pixel-only reload must not churn the children",
    );
}

/// A reload that only flipped a layer's visibility flag keeps the children it
/// has, and the one it hid stops drawing.
#[test]
fn a_reload_that_hides_a_layer_hides_its_child() {
    let mut app = app();
    let (parent, handle) = spawn(&mut app, &["Body", "Hat"]);
    let before = child_entities(&mut app, parent);
    assert_eq!(
        visibility_of(&mut app, parent, "Hat"),
        Visibility::Inherited
    );

    app.world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .insert(
            handle.id(),
            Aseprite::builder()
                .with_layer("Body", true)
                .with_layer("Hat", false)
                .build(),
        )
        .expect("replace the loaded asset");
    for _ in 0..4 {
        app.update();
    }

    assert_eq!(child_entities(&mut app, parent), before);
    assert_eq!(visibility_of(&mut app, parent, "Hat"), Visibility::Hidden);
    assert_eq!(
        visibility_of(&mut app, parent, "Body"),
        Visibility::Inherited
    );
}

/// The visibility of `parent`'s child for the named layer.
fn visibility_of(app: &mut App, parent: Entity, layer: &str) -> Visibility {
    let layer = LayerId::new(layer);
    let world = app.world_mut();
    let mut query = world.query::<(&LayerId, &Visibility, &SpriteLayerOf)>();
    query
        .iter(world)
        .find(|(id, _, of)| **id == layer && of.0 == parent)
        .map(|(_, visibility, _)| *visibility)
        .expect("the layer has a child")
}
