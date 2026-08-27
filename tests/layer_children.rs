//! The render children an `AseTexture` owns: what survives a change to the
//! component, and what a file-less asset gives them to draw from.

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

/// Marks a child so a respawn behind the crate's back is visible.
#[derive(Component)]
struct GameOwned;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), AsepriteUltraPlugin));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();
    app
}

/// A metadata-only file with two layers and two slices at distinct atlas
/// positions, so a slice swap is visible in the child's atlas index.
fn file() -> Aseprite {
    Aseprite::builder()
        .with_layer("Body", true)
        .with_layer("Hat", true)
        .with_slice("left", Rect::new(0.0, 0.0, 8.0, 8.0), 1)
        .with_slice("right", Rect::new(8.0, 0.0, 16.0, 8.0), 2)
        .with_frame_indices([0])
        .build()
}

fn children_of(app: &mut App, parent: Entity) -> Vec<Entity> {
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

/// Spawns `tex` over the fixture and steps until its children exist.
fn spawn(app: &mut App, tex: AseTexture) -> Entity {
    let parent = app.world_mut().spawn(tex).id();
    for _ in 0..4 {
        app.update();
    }
    parent
}

fn texture(app: &mut App) -> AseTexture {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .add(file());
    AseTexture::baked(handle).sprite()
}

fn atlas_index(app: &App, child: Entity) -> Option<usize> {
    app.world()
        .get::<Sprite>(child)?
        .texture_atlas
        .as_ref()
        .map(|atlas| atlas.index)
}

/// Editing the component must reach the composite child without replacing it:
/// whatever the game hung off that entity is the game's, not the crate's.
#[test]
fn a_baked_child_survives_a_texture_edit() {
    let mut app = app();
    let tex = texture(&mut app);
    let parent = spawn(&mut app, tex);

    let before = children_of(&mut app, parent);
    assert_eq!(before.len(), 1, "baked mode draws through one child");
    app.world_mut().entity_mut(before[0]).insert(GameOwned);

    app.world_mut().increment_change_tick();
    app.world_mut()
        .get_mut::<AseTexture>(parent)
        .expect("texture on the parent")
        .offset = Vec2::splat(4.0);
    app.update();

    assert_eq!(
        children_of(&mut app, parent),
        before,
        "the composite child was replaced by an edit that did not need it",
    );
    assert!(
        app.world().get::<GameOwned>(before[0]).is_some(),
        "the edit destroyed what the game attached to the child",
    );
}

/// The edit still has to land: a slice swap moves the composite child onto
/// the new slice's atlas rect.
#[test]
fn a_baked_child_follows_a_slice_swap() {
    let mut app = app();
    let tex = texture(&mut app).with_slice("left");
    let parent = spawn(&mut app, tex);
    let child = children_of(&mut app, parent)[0];
    assert_eq!(atlas_index(&app, child), Some(1));

    app.world_mut().increment_change_tick();
    app.world_mut()
        .get_mut::<AseTexture>(parent)
        .expect("texture on the parent")
        .slice = Some(SliceId::new("right"));
    app.update();

    assert_eq!(
        app.world().get::<AseSlice>(child).map(|slice| slice.name),
        Some(SliceId::new("right")),
    );
    assert_eq!(atlas_index(&app, child), Some(2));
}

/// Dropping the slice puts the child back on the whole frame.
#[test]
fn a_baked_child_drops_the_slice_with_the_texture() {
    let mut app = app();
    let tex = texture(&mut app).with_slice("left");
    let parent = spawn(&mut app, tex);
    let child = children_of(&mut app, parent)[0];

    app.world_mut().increment_change_tick();
    app.world_mut()
        .get_mut::<AseTexture>(parent)
        .expect("texture on the parent")
        .slice = None;
    app.update();

    assert!(app.world().get::<AseSlice>(child).is_none());
    assert_eq!(atlas_index(&app, child), Some(0));
}

/// An asset the builder assembled has no file behind it, so there is no path
/// to hang a `#layer` sub-asset off. The children draw from the asset itself.
#[test]
fn a_file_less_asset_gives_its_children_its_own_handle() {
    let mut app = app();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .add(file());
    let parent = spawn(&mut app, AseTexture::new(handle.clone()).sprite());

    let children = children_of(&mut app, parent);
    assert_eq!(children.len(), 2, "one child per layer");
    for child in children {
        assert_eq!(
            app.world()
                .get::<AnimationLayer>(child)
                .map(|layer| layer.aseprite.id()),
            Some(handle.id()),
            "the child was pointed at a sub-asset that can never resolve",
        );
    }
}
