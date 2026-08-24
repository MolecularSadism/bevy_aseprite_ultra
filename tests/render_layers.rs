//! Render layers on a texture's children.
//!
//! An `AseTexture` parent draws nothing itself. A camera filtering to the
//! parent's layer must still find the art.

mod support;

use bevy::{camera::visibility::RenderLayers, prelude::*};
use bevy_aseprite_ultra::prelude::*;
use support::{Cel, Fixture, Layer};

const LAYER: usize = 3;

fn one_pixel() -> Fixture {
    Fixture {
        canvas: (1, 1),
        frames: 1,
        frame_duration: 100,
        layers: vec![Layer::normal("Main", 0)],
        cels: vec![Cel {
            frame: 0,
            layer_index: 0,
            position: (0, 0),
            colour: [255, 255, 255, 255],
        }],
        slices: Vec::new(),
    }
}

/// Steps the app until the texture has spawned its children.
fn children_of(app: &mut App, parent: Entity) -> Vec<Entity> {
    for _ in 0..10 {
        app.update();
        if let Some(children) = app.world().entity(parent).get::<Children>() {
            return children.iter().collect();
        }
    }
    panic!("the texture never spawned a child");
}

#[test]
fn children_inherit_the_parents_render_layers() {
    let (mut app, handles) =
        support::load_with("render_layers", &one_pixel(), &[""], AsepriteUltraPlugin);

    let parent = app
        .world_mut()
        .spawn((
            AseTexture::baked(handles[0].clone()).sprite(),
            RenderLayers::layer(LAYER),
        ))
        .id();

    let children = children_of(&mut app, parent);
    assert!(!children.is_empty(), "expected at least one render child");
    for child in children {
        assert_eq!(
            app.world().entity(child).get::<RenderLayers>(),
            Some(&RenderLayers::layer(LAYER)),
            "a child left on the default layer is invisible to the parent's camera",
        );
    }
}

#[test]
fn a_changed_parent_layer_reaches_the_children() {
    let (mut app, handles) =
        support::load_with("render_layers", &one_pixel(), &[""], AsepriteUltraPlugin);

    let parent = app
        .world_mut()
        .spawn((
            AseTexture::baked(handles[0].clone()).sprite(),
            RenderLayers::layer(LAYER),
        ))
        .id();
    let children = children_of(&mut app, parent);

    app.world_mut()
        .entity_mut(parent)
        .insert(RenderLayers::layer(LAYER + 1));
    app.update();

    for child in children {
        assert_eq!(
            app.world().entity(child).get::<RenderLayers>(),
            Some(&RenderLayers::layer(LAYER + 1)),
        );
    }
}
