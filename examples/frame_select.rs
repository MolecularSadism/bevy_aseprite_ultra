// Frame-selection example.
//
// Exercises the AseFrame component as a frame cursor independent from
// AseAnimation:
//   - Left:  layered sprite, paused. Scrub the parent's AseFrame with
//            Left/Right to confirm frame selection works without a driver.
//   - Right: layered sprite where the "Swoosh" layer child carries its own
//            AseAnimation, animating independently of the parent (which is
//            also animating, on a different tag/speed via the parent's
//            AseAnimation).
//
// Controls:
//   [Space]      Pause / resume the left fighter's parent driver
//   [Left/Right] Step the left fighter's AseFrame by -1 / +1 while paused
//   [R]          Reset the left fighter's AseFrame to 0

use bevy::{color::palettes::css, image::ImageSamplerDescriptor, prelude::*};
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (attach_per_layer_driver, handle_input, update_hint))
        .run();
}

#[derive(Component)]
struct Scrubbable;

#[derive(Component)]
struct PerLayerDemo;

#[derive(Component)]
struct HintText;

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    // Left: scrubbable, animation paused so the manual cursor is the only
    // driver of the visible frame.
    let mut paused = AseAnimation::default().with_repeat(AnimationRepeat::Loop);
    paused.pause();
    cmd.spawn((
        AseTexture::new(server.load("sword_fighter.aseprite")).sprite(),
        paused,
        Scrubbable,
        Transform::from_translation(Vec3::new(-30., 0., 0.)),
    ));

    // Right: parent runs an animation; the "Swoosh" layer child will get its
    // own AseAnimation attached in `attach_per_layer_driver` once layers spawn.
    cmd.spawn((
        AseTexture::new(server.load("sword_fighter.aseprite")).sprite(),
        AseAnimation::default()
            .with_repeat(AnimationRepeat::Loop)
            .with_speed(0.25),
        PerLayerDemo,
        Transform::from_translation(Vec3::new(30., 0., 0.)),
    ));

    // Hint UI.
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
            Text::new("[Space] pause/resume left   [Left]/[Right] scrub frame   [R] reset"),
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
            Text::new("left: paused @ frame 0   |   right: parent slow, Swoosh fast"),
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

/// Once the per-layer-demo entity's children exist, give the "Swoosh" layer
/// child its own AseAnimation so it runs independently of the parent driver.
/// AseAnimation requires AseFrame, so the child gets a local frame cursor; the
/// renderer prefers the child's AseFrame over the parent's via the
/// SpriteLayerOf fallback chain.
fn attach_per_layer_driver(
    mut cmd: Commands,
    demos: Query<&SpriteLayers, With<PerLayerDemo>>,
    children: Query<(Entity, &LayerId), Without<AseAnimation>>,
) {
    let swoosh = LayerId::new("Swoosh");
    for layers in &demos {
        for child in layers.iter() {
            if let Ok((entity, id)) = children.get(child) {
                if *id == swoosh {
                    cmd.entity(entity).insert(
                        AseAnimation::default()
                            .with_repeat(AnimationRepeat::Loop)
                            .with_speed(2.0),
                    );
                }
            }
        }
    }
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut scrubbable: Query<(&mut AseAnimation, &mut AseFrame), With<Scrubbable>>,
) {
    for (mut anim, mut frame) in &mut scrubbable {
        if keys.just_pressed(KeyCode::Space) {
            if anim.playing {
                anim.pause();
            } else {
                anim.start();
            }
        }
        if keys.just_pressed(KeyCode::ArrowRight) {
            frame.0 = frame.0.saturating_add(1);
        }
        if keys.just_pressed(KeyCode::ArrowLeft) {
            frame.0 = frame.0.saturating_sub(1);
        }
        if keys.just_pressed(KeyCode::KeyR) {
            frame.0 = 0;
        }
    }
}

fn update_hint(
    scrubbable: Query<(&AseAnimation, &AseFrame), With<Scrubbable>>,
    mut hint: Query<&mut Text, With<HintText>>,
) {
    let Ok((anim, frame)) = scrubbable.single() else {
        return;
    };
    let status = format!(
        "left: {} @ frame {}   |   right: parent slow, Swoosh fast",
        if anim.playing { "playing" } else { "paused" },
        frame.0,
    );
    for mut text in &mut hint {
        **text = status.clone();
    }
}
