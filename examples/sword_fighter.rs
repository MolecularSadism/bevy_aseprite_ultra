// Sword Fighter layered animation example.
//
// Asset credit: sword_fighter.aseprite by xzany (itch.io) / mattz_21 (Discord)
//
// Layers: "Character" and "Swoosh"
//
// Controls:
//   [1] Start / Stop animation
//   [2] Toggle Swoosh visibility
//   [3] Swap layer order (Character ↔ Swoosh)

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
        .init_resource::<SwordFighterState>()
        .add_systems(Startup, setup)
        .add_systems(Update, handle_input)
        .run();
}

/// Marker for the sword fighter entity.
#[derive(Component)]
struct SwordFighter;

/// Marker for the hint text.
#[derive(Component)]
struct HintText;

#[derive(Resource)]
struct SwordFighterState {
    playing: bool,
    swoosh_visible: bool,
    layers_swapped: bool,
}

impl Default for SwordFighterState {
    fn default() -> Self {
        Self {
            playing: true,
            swoosh_visible: true,
            layers_swapped: false,
        }
    }
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    cmd.spawn((
        AseTexture::new(server.load("sword_fighter.aseprite")).sprite(),
        AseAnimation::default()
            .with_repeat(AnimationRepeat::Loop),
        SwordFighter,
    ));

    // UI hint
    cmd.spawn(Node {
        width: Val::Percent(100.),
        height: Val::Percent(100.),
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::FlexEnd,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|root| {
        root.spawn((
            Text::new("[1] Start/Stop  [2] Toggle Swoosh  [3] Swap Layer Order"),
            TextFont {
                font_size: 18.,
                ..default()
            },
            TextColor(css::GRAY.into()),
            Node {
                margin: UiRect::bottom(Val::Px(10.)),
                ..default()
            },
        ));
        root.spawn((
            Text::new("Playing | Swoosh: visible | Order: normal"),
            TextFont {
                font_size: 16.,
                ..default()
            },
            TextColor(css::WHITE.into()),
            Node {
                margin: UiRect::bottom(Val::Px(16.)),
                ..default()
            },
            HintText,
        ));
    });
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<SwordFighterState>,
    mut fighters: Query<(&mut AseAnimation, &SpriteLayers), With<SwordFighter>>,
    mut layer_children: Query<(&LayerId, &mut Visibility, &mut Transform), With<SpriteLayerOf>>,
    mut hint: Query<&mut Text, With<HintText>>,
) {
    let swoosh = LayerId::new("Swoosh");
    let mut changed = false;

    // [1] Start / Stop
    if keys.just_pressed(KeyCode::Digit1) {
        state.playing = !state.playing;
        for (mut anim, _) in &mut fighters {
            if state.playing {
                anim.start();
            } else {
                anim.pause();
            }
        }
        changed = true;
    }

    // [2] Toggle Swoosh visibility
    if keys.just_pressed(KeyCode::Digit2) {
        state.swoosh_visible = !state.swoosh_visible;
        for (_, layers) in &fighters {
            for child in layers.iter() {
                if let Ok((id, mut vis, _)) = layer_children.get_mut(child) {
                    if *id == swoosh {
                        *vis = if state.swoosh_visible {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                    }
                }
            }
        }
        changed = true;
    }

    // [3] Swap layer order
    if keys.just_pressed(KeyCode::Digit3) {
        state.layers_swapped = !state.layers_swapped;
        for (_, layers) in &fighters {
            // Collect current z values, then swap them
            let mut entries: Vec<(Entity, f32)> = Vec::new();
            for child in layers.iter() {
                if let Ok((_, _, transform)) = layer_children.get(child) {
                    entries.push((child, transform.translation.z));
                }
            }
            if entries.len() == 2 {
                let z0 = entries[0].1;
                let z1 = entries[1].1;
                if let Ok((_, _, mut t)) = layer_children.get_mut(entries[0].0) {
                    t.translation.z = z1;
                }
                if let Ok((_, _, mut t)) = layer_children.get_mut(entries[1].0) {
                    t.translation.z = z0;
                }
            }
        }
        changed = true;
    }

    if changed {
        let status = format!(
            "{} | Swoosh: {} | Order: {}",
            if state.playing { "Playing" } else { "Paused" },
            if state.swoosh_visible { "visible" } else { "hidden" },
            if state.layers_swapped { "swapped" } else { "normal" },
        );
        for mut text in &mut hint {
            **text = status.clone();
        }
    }
}
