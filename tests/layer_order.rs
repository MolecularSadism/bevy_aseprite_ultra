//! Layer stacking order and per-layer visibility.
//!
//! The asset owns a default order; an `AseTexture` may override it for one
//! entity. Both halves are checked here against the children that actually
//! render, not only against the structs that describe them.

mod support;

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer};

fn make_aseprite_with_layers(names: &[&str]) -> Aseprite {
    let mut ase = Aseprite::default();
    for &name in names {
        ase.layers.push(LayerEntry::new(LayerId::new(name), true));
    }
    ase
}

// ---------------------------------------------------------------- //
// Aseprite: the asset's own order
// ---------------------------------------------------------------- //

#[test]
fn layer_ids_returns_front_to_back_order() {
    let ase = make_aseprite_with_layers(&["top", "middle", "bottom"]);
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], LayerId::new("top"));
    assert_eq!(ids[2], LayerId::new("bottom"));
}

#[test]
fn visible_layer_ids_filters_hidden() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    ase.layers[1].visible = false;
    let visible: Vec<_> = ase.visible_layer_ids().collect();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0], LayerId::new("a"));
    assert_eq!(visible[1], LayerId::new("c"));
}

#[test]
fn reorder_layer_moves_to_front() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    assert!(ase.reorder_layer(LayerId::new("c"), 0));
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids[0], LayerId::new("c"));
    assert_eq!(ids[1], LayerId::new("a"));
    assert_eq!(ids[2], LayerId::new("b"));
}

#[test]
fn reorder_layer_moves_to_back() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    assert!(ase.reorder_layer(LayerId::new("a"), 99)); // clamped to end
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids[0], LayerId::new("b"));
    assert_eq!(ids[1], LayerId::new("c"));
    assert_eq!(ids[2], LayerId::new("a"));
}

#[test]
fn reorder_nonexistent_layer_returns_false() {
    let mut ase = make_aseprite_with_layers(&["a", "b"]);
    assert!(!ase.reorder_layer(LayerId::new("nope"), 0));
}

#[test]
fn set_layer_visible_toggles() {
    let mut ase = make_aseprite_with_layers(&["a", "b"]);
    assert!(ase.set_layer_visible(LayerId::new("a"), false));
    assert!(!ase.layers[0].visible);
    assert!(ase.set_layer_visible(LayerId::new("a"), true));
    assert!(ase.layers[0].visible);
}

// ---------------------------------------------------------------- //
// AseTexture: the per-entity override
// ---------------------------------------------------------------- //

/// The override starts as the asset's own order, so a single move lands
/// against the other layers rather than into an empty list.
#[test]
fn texture_reorder_seeds_the_override_from_the_asset() {
    let ase = make_aseprite_with_layers(&["a", "b", "c"]);
    let mut tex = AseTexture::new(Handle::default());
    assert!(tex.reorder_layer(&ase, LayerId::new("c"), 0));
    assert_eq!(
        tex.layer_order.as_deref(),
        Some([LayerId::new("c"), LayerId::new("a"), LayerId::new("b")].as_slice()),
    );
}

#[test]
fn texture_reorder_clamps_past_the_end() {
    let ase = make_aseprite_with_layers(&["a", "b", "c"]);
    let mut tex = AseTexture::new(Handle::default());
    assert!(tex.reorder_layer(&ase, LayerId::new("a"), 99));
    assert_eq!(
        tex.layer_order.as_deref(),
        Some([LayerId::new("b"), LayerId::new("c"), LayerId::new("a")].as_slice()),
    );
}

#[test]
fn texture_reorder_reports_an_unknown_layer() {
    let ase = make_aseprite_with_layers(&["a", "b"]);
    let mut tex = AseTexture::new(Handle::default());
    assert!(!tex.reorder_layer(&ase, LayerId::new("nope"), 0));
    assert_eq!(
        tex.layer_order, None,
        "a move that found nothing must leave the entity on the asset order",
    );
}

/// A second move builds on the first rather than re-seeding from the asset.
#[test]
fn texture_reorder_accumulates() {
    let ase = make_aseprite_with_layers(&["a", "b", "c"]);
    let mut tex = AseTexture::new(Handle::default());
    assert!(tex.reorder_layer(&ase, LayerId::new("c"), 0));
    assert!(tex.reorder_layer(&ase, LayerId::new("b"), 0));
    assert_eq!(
        tex.layer_order.as_deref(),
        Some([LayerId::new("b"), LayerId::new("c"), LayerId::new("a")].as_slice()),
    );
}

#[test]
fn show_and_hide_report_no_effect_outside_include() {
    let hat = LayerId::new("hat");
    for filter in [LayerFilter::Visible, LayerFilter::All] {
        let mut tex = AseTexture::new(Handle::default()).with_layers(filter.clone());
        assert!(!tex.show_layer(hat), "{filter:?} names no layers to add to");
        assert!(!tex.hide_layer(hat), "{filter:?} names no layers to remove");
        assert_eq!(tex.layers, filter, "a refused call must change nothing");
    }
}

#[test]
fn show_and_hide_edit_the_include_list() {
    let hat = LayerId::new("hat");
    let mut tex = AseTexture::new(Handle::default()).with_layers(LayerFilter::Include(vec![]));

    assert!(tex.show_layer(hat));
    assert!(tex.show_layer(hat), "showing a shown layer is idempotent");
    assert_eq!(tex.layers, LayerFilter::Include(vec![hat]));

    assert!(tex.hide_layer(hat));
    assert!(tex.hide_layer(hat), "hiding a hidden layer is idempotent");
    assert_eq!(tex.layers, LayerFilter::Include(vec![]));
}

// ---------------------------------------------------------------- //
// The children that actually render
// ---------------------------------------------------------------- //

/// Three one-pixel layers, written bottom-to-top as Aseprite stores them.
fn stack() -> Fixture {
    Fixture {
        canvas: (1, 1),
        frames: 1,
        frame_duration: 100,
        layers: vec![
            Layer::normal("a", 0),
            Layer::normal("b", 0),
            Layer::normal("c", 0),
        ],
        cels: (0..3)
            .map(|layer_index| Cel {
                frame: 0,
                layer_index,
                position: (0, 0),
                colour: [255, 255, 255, 255],
            })
            .collect(),
        slices: Vec::new(),
    }
}

/// Steps the app until the texture has spawned its layer children.
fn spawn_stack(name: &str, tex: AseTexture) -> (App, Entity, Aseprite) {
    let (mut app, handles) = support::load_with(name, &stack(), &[""], AsepriteUltraPlugin);
    let aseprite = app
        .world()
        .resource::<Assets<Aseprite>>()
        .get(&handles[0])
        .expect("composite loaded")
        .clone();
    let parent = app
        .world_mut()
        .spawn(AseTexture {
            aseprite: handles[0].clone(),
            ..tex
        })
        .id();
    for _ in 0..10 {
        app.update();
        if app
            .world()
            .entity(parent)
            .get::<Children>()
            .is_some_and(|children| children.len() == 3)
        {
            return (app, parent, aseprite);
        }
    }
    panic!("the texture never spawned its three layer children");
}

/// The layer children of `parent`, deepest z first — the order they draw in.
fn front_to_back(app: &mut App, parent: Entity) -> Vec<LayerId> {
    let world = app.world_mut();
    let mut query = world.query::<(&LayerId, &Transform, &SpriteLayerOf)>();
    let mut children: Vec<(LayerId, f32)> = query
        .iter(world)
        .filter(|(_, _, of)| of.0 == parent)
        .map(|(id, transform, _)| (*id, transform.translation.z))
        .collect();
    children.sort_by(|a, b| b.1.total_cmp(&a.1));
    children.into_iter().map(|(id, _)| id).collect()
}

fn visibility_of(app: &mut App, parent: Entity, layer: LayerId) -> Visibility {
    let world = app.world_mut();
    let mut query = world.query::<(&LayerId, &Visibility, &SpriteLayerOf)>();
    query
        .iter(world)
        .find(|(id, _, of)| **id == layer && of.0 == parent)
        .map(|(_, visibility, _)| *visibility)
        .expect("layer child exists")
}

#[test]
fn children_stack_in_the_assets_front_to_back_order() {
    let (mut app, parent, _) =
        spawn_stack("layer_order_default", AseTexture::new(Handle::default()));
    assert_eq!(
        front_to_back(&mut app, parent),
        vec![LayerId::new("c"), LayerId::new("b"), LayerId::new("a")],
        "the file's topmost layer draws in front",
    );
}

/// The regression the old no-op `reorder_layer` hid: a move on the component
/// has to reach the z of the children already spawned.
#[test]
fn reorder_layer_restacks_the_spawned_children() {
    let (mut app, parent, aseprite) =
        spawn_stack("layer_order_reorder", AseTexture::new(Handle::default()));

    app.world_mut().increment_change_tick();
    let mut tex = app
        .world_mut()
        .get_mut::<AseTexture>(parent)
        .expect("texture on the parent");
    assert!(tex.reorder_layer(&aseprite, LayerId::new("a"), 0));
    app.update();

    assert_eq!(
        front_to_back(&mut app, parent),
        vec![LayerId::new("a"), LayerId::new("c"), LayerId::new("b")],
        "the bottom layer was moved to the front",
    );
}

#[test]
fn a_spawn_time_order_override_reaches_the_children() {
    let order = vec![LayerId::new("b"), LayerId::new("c"), LayerId::new("a")];
    let (mut app, parent, _) = spawn_stack(
        "layer_order_builder",
        AseTexture::new(Handle::default()).with_layer_order(order.clone()),
    );
    assert_eq!(front_to_back(&mut app, parent), order);
}

#[test]
fn hide_layer_hides_the_matching_child() {
    let (a, b, c) = (LayerId::new("a"), LayerId::new("b"), LayerId::new("c"));
    let (mut app, parent, _) = spawn_stack(
        "layer_order_hide",
        AseTexture::new(Handle::default()).with_layers(LayerFilter::Include(vec![a, b, c])),
    );
    assert_eq!(visibility_of(&mut app, parent, b), Visibility::Inherited);

    app.world_mut().increment_change_tick();
    let mut tex = app.world_mut().get_mut::<AseTexture>(parent).unwrap();
    assert!(tex.hide_layer(b));
    app.update();

    assert_eq!(visibility_of(&mut app, parent, b), Visibility::Hidden);
    assert_eq!(visibility_of(&mut app, parent, a), Visibility::Inherited);
    assert_eq!(visibility_of(&mut app, parent, c), Visibility::Inherited);
}

#[test]
fn show_layer_brings_a_hidden_child_back() {
    let (a, c) = (LayerId::new("a"), LayerId::new("c"));
    let (mut app, parent, _) = spawn_stack(
        "layer_order_show",
        AseTexture::new(Handle::default()).with_layers(LayerFilter::Include(vec![a, c])),
    );
    let b = LayerId::new("b");
    assert_eq!(visibility_of(&mut app, parent, b), Visibility::Hidden);

    app.world_mut().increment_change_tick();
    let mut tex = app.world_mut().get_mut::<AseTexture>(parent).unwrap();
    assert!(tex.show_layer(b));
    app.update();

    assert_eq!(visibility_of(&mut app, parent, b), Visibility::Inherited);
}
