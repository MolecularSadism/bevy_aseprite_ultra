mod helpers;

use bevy::{
    color::palettes::css,
    image::ImageSamplerDescriptor,
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;
use helpers::{DefaultFilter, HintText, LayerState, LayerTogglePlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_plugins(LayerTogglePlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    // ---- Baked AseSlice (left group) ----

    // Layer 1 only via sub-asset label
    cmd.spawn((
        AseSlice {
            name: "Top".into(),
            aseprite: server.load("sliced_layers.aseprite#Layer 1"),
        },
        Sprite::default(),
        Transform::from_translation(Vec3::new(-22., 10., 0.)),
    ));

    cmd.spawn((
        AseSlice {
            name: "Bottom".into(),
            aseprite: server.load("sliced_layers.aseprite#Layer 1"),
        },
        Sprite::default(),
        Transform::from_translation(Vec3::new(-22., -10., 0.)),
    ));

    // All visible layers composed into one sprite (default)
    cmd.spawn((
        AseSlice {
            name: "Top".into(),
            aseprite: server.load("sliced_layers.aseprite"),
        },
        Sprite::default(),
        Transform::from_translation(Vec3::new(-10., 10., 0.)),
    ));

    cmd.spawn((
        AseSlice {
            name: "Bottom".into(),
            aseprite: server.load("sliced_layers.aseprite"),
        },
        Sprite::default(),
        Transform::from_translation(Vec3::new(-10., -10., 0.)),
    ));

    // ---- AseLayeredSlice (right group) ----

    // Layer 1 only
    cmd.spawn((
        AseLayeredSlice {
            name: "Top".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Include(vec![LayerId::new("Layer 1")]),
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Include(vec![LayerId::new("Layer 1")])),
        Transform::from_translation(Vec3::new(10., 10., 0.)),
    ));

    cmd.spawn((
        AseLayeredSlice {
            name: "Bottom".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Include(vec![LayerId::new("Layer 1")]),
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Include(vec![LayerId::new("Layer 1")])),
        Transform::from_translation(Vec3::new(10., -10., 0.)),
    ));

    // Layer 1 + Layer 2
    cmd.spawn((
        AseLayeredSlice {
            name: "Top".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Include(vec![
                LayerId::new("Layer 1"),
                LayerId::new("Layer 2"),
            ]),
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Include(vec![
            LayerId::new("Layer 1"),
            LayerId::new("Layer 2"),
        ])),
        Transform::from_translation(Vec3::new(25., 10., 0.)),
    ));

    cmd.spawn((
        AseLayeredSlice {
            name: "Bottom".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Include(vec![
                LayerId::new("Layer 1"),
                LayerId::new("Layer 2"),
            ]),
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Include(vec![
            LayerId::new("Layer 1"),
            LayerId::new("Layer 2"),
        ])),
        Transform::from_translation(Vec3::new(25., -10., 0.)),
    ));

    // All visible layers (Layer 1 + 2 + 3)
    cmd.spawn((
        AseLayeredSlice {
            name: "Top".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Visible,
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Visible),
        Transform::from_translation(Vec3::new(40., 10., 0.)),
    ));

    cmd.spawn((
        AseLayeredSlice {
            name: "Bottom".into(),
            aseprite: server.load("sliced_layers.aseprite"),
            layers: LayerFilter::Visible,
            render_target: RenderTarget::Sprite,
        },
        DefaultFilter(LayerFilter::Visible),
        Transform::from_translation(Vec3::new(40., -10., 0.)),
    ));

    // ---- UI ----
    cmd.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Percent(100.),
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::FlexEnd,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|root| {
        // Row of group boxes
        root.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(40.),
            margin: UiRect::bottom(Val::Px(20.)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new("Baked AseSlice"),
                TextFont {
                    font_size: 16.,
                    ..default()
                },
                TextColor(css::WHITE.into()),
            ));
            row.spawn((
                Text::new("AseLayeredSlice"),
                TextFont {
                    font_size: 16.,
                    ..default()
                },
                TextColor(css::WHITE.into()),
            ));
        });

        // Bottom hint text
        root.spawn((
            Text::new(LayerState::AllVisible.hint()),
            TextFont {
                font_size: 18.,
                ..default()
            },
            TextColor(css::GRAY.into()),
            Node {
                margin: UiRect::bottom(Val::Px(16.)),
                ..default()
            },
            HintText,
        ));
    });
}
