use bevy::{
    color::palettes::css,
    image::ImageSamplerDescriptor,
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_layer)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    // ---- Baked AseAnimation (left group) ----

    // All visible layers composed into one sprite (default)
    cmd.spawn((
        AseAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
        },
        Sprite::default(),
        Transform::from_translation(Vec3::new(-30., 0., 0.)),
    ));

    // A specific layer loaded directly via sub-asset label
    cmd.spawn((
        AseAnimation::sprite(
            Animation::default(),
            server.load("layers.aseprite#Layer 1"),
        ),
        Transform::from_translation(Vec3::new(-15., 0., 0.)),
    ));

    // ---- AseLayeredAnimation (right group) ----

    // All visible layers, each spawned as a separate child sprite
    cmd.spawn((
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::Visible,
            render_target: RenderTarget::Sprite,
        },
        Transform::from_translation(Vec3::new(10., 0., 0.)),
    ));

    // Only specific layers as children
    cmd.spawn((
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::Include(vec![LayerId::new("Layer 1")]),
            render_target: RenderTarget::Sprite,
        },
        Transform::from_translation(Vec3::new(25., 0., 0.)),
    ));

    // All layers including hidden ones
    cmd.spawn((
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::All,
            render_target: RenderTarget::Sprite,
        },
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
                Text::new("Baked AseAnimation"),
                TextFont {
                    font_size: 16.,
                    ..default()
                },
                TextColor(css::WHITE.into()),
            ));
            row.spawn((
                Text::new("AseLayeredAnimation"),
                TextFont {
                    font_size: 16.,
                    ..default()
                },
                TextColor(css::WHITE.into()),
            ));
        });

        // Bottom hint text
        root.spawn((
            Text::new("[Space] Toggle Layer 1 visibility on AseLayeredAnimation"),
            TextFont {
                font_size: 18.,
                ..default()
            },
            TextColor(css::GRAY.into()),
            Node {
                margin: UiRect::bottom(Val::Px(16.)),
                ..default()
            },
        ));
    });
}


/// Press Space to toggle "Layer 1" visibility on all AseLayeredAnimation entities.
fn toggle_layer(
    keys: Res<ButtonInput<KeyCode>>,
    parents: Query<&SpriteLayers>,
    mut layers: Query<(&LayerId, &mut Visibility), With<SpriteLayerOf>>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    let target = LayerId::new("Layer 1");

    for sprite_layers in &parents {
        for layer_entity in sprite_layers.iter() {
            let Ok((id, mut vis)) = layers.get_mut(layer_entity) else {
                continue;
            };
            if *id == target {
                *vis = match *vis {
                    Visibility::Hidden => Visibility::Inherited,
                    _ => Visibility::Hidden,
                };
            }
        }
    }
}
