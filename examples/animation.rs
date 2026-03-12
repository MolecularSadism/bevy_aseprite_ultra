use bevy::{image::ImageSamplerDescriptor, prelude::*};
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, events)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("walk-right"),
        Transform::from_translation(Vec3::new(15., 0., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("walk-up"),
        Transform::from_translation(Vec3::new(0., 0., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("walk-down"),
        Transform::from_translation(Vec3::new(-15., 0., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::default()
            .with_direction(AnimationDirection::Reverse)
            .with_repeat(AnimationRepeat::Count(1)),
        Transform::from_translation(Vec3::new(0., -20., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("walk-right"),
        AseFlip { x: true, y: false },
        Transform::from_translation(Vec3::new(15., -20., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("ball.aseprite")).sprite(),
        AseAnimation::tag("squash"),
        Transform::from_translation(Vec3::new(0., 20., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("ghost_slices.aseprite"))
            .with_slice("ghost_red")
            .sprite(),
        Transform::from_translation(Vec3::new(50., 0., 0.)),
    ));

    cmd.spawn((
        AseTexture::baked(server.load("ghost_slices.aseprite"))
            .with_slice("ghost_blue")
            .sprite(),
        AseFlip { x: true, y: false },
        Transform::from_translation(Vec3::new(80., 0., 0.)),
    ));
}

fn events(mut events: MessageReader<AnimationEvents>, mut cmd: Commands) {
    for event in events.read() {
        match event {
            AnimationEvents::Finished(entity) => cmd.entity(*entity).despawn(),
            AnimationEvents::LoopCycleFinished(_entity) => (),
        };
    }
}
