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

    // ---- Baked AseTexture (left group) ----

    // Layer 1 only via layer filter on baked texture
    cmd.spawn((
        AseTexture::baked(server.load("layers.aseprite"))
            .with_layers(LayerFilter::Include(vec![LayerId::new("Layer 1")]))
            .sprite(),
        AseAnimation::default(),
        Transform::from_translation(Vec3::new(-22., 0., 0.)),
    ));

    // All visible layers composed into one sprite (default)
    cmd.spawn((
        AseTexture::baked(server.load("layers.aseprite")).sprite(),
        AseAnimation::default(),
        Transform::from_translation(Vec3::new(-10., 0., 0.)),
    ));

    // ---- Layered AseTexture (right group) ----

    // Layer 1 only
    cmd.spawn((
        AseTexture::new(server.load("layers.aseprite"))
            .with_layers(LayerFilter::Include(vec![LayerId::new("Layer 1")]))
            .sprite(),
        AseAnimation::default(),
        DefaultFilter(LayerFilter::Include(vec![LayerId::new("Layer 1")])),
        Transform::from_translation(Vec3::new(10., 0., 0.)),
    ));

    // Layer 1 + Layer 2
    cmd.spawn((
        AseTexture::new(server.load("layers.aseprite"))
            .with_layers(LayerFilter::Include(vec![
                LayerId::new("Layer 1"),
                LayerId::new("Layer 2"),
            ]))
            .sprite(),
        AseAnimation::default(),
        DefaultFilter(LayerFilter::Include(vec![
            LayerId::new("Layer 1"),
            LayerId::new("Layer 2"),
        ])),
        Transform::from_translation(Vec3::new(25., 0., 0.)),
    ));

    // All visible layers (Layer 1 + 2 + 3)
    cmd.spawn((
        AseTexture::new(server.load("layers.aseprite")).sprite(),
        AseAnimation::default(),
        DefaultFilter(LayerFilter::Visible),
        Transform::from_translation(Vec3::new(40., 0., 0.)),
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
                Text::new("Baked AseTexture"),
                TextFont {
                    font_size: 16.,
                    ..default()
                },
                TextColor(css::WHITE.into()),
            ));
            row.spawn((
                Text::new("Layered AseTexture"),
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
