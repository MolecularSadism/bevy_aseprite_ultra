//! Demonstrates the `lit` feature: pairs an aseprite color atlas with a
//! normal-map atlas and exposes both via `AseLitMaterial`.
//!
//! Run with: `cargo run --features lit --example normal_maps`
//!
//! # Authoring
//!
//! - **In-file (default):** add layers named `<layer>.normal` next to your
//!   color layers (`body` ↔ `body.normal`). The loader strips them from
//!   color paths and packs them into `Aseprite::normal_atlas_image`.
//! - **Sibling file:** save a parallel `<stem>.normal.aseprite` next to
//!   `<stem>.aseprite`. Used as a fallback for layers that have no in-file
//!   companion.
//!
//! # Lighting
//!
//! The bundled shader does a 2D half-Lambert against the tangent-space normal
//! using `AseLitParams::sun_dir` / `sun_color` / `ambient`. With those fields
//! at their defaults (`ambient = 1`, `sun_color = 0`) the output is still
//! `color * tint`. To drive shading, write the lighting fields per frame
//! (e.g., from your scene's directional source). Users wanting a different
//! lighting model can ship their own `Material2d` with the same binding
//! layout and impl `RenderAnimation` / `RenderSlice` on it.

use bevy::{image::ImageSamplerDescriptor, prelude::*};
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, log_normal_atlas_status)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn((Camera2d, Transform::default().with_scale(Vec3::splat(0.15))));

    // Lit, layered. With no `.normal` companion layers in player.aseprite this
    // call falls back to unlit `Sprite` rendering until the artist adds them.
    cmd.spawn((
        AseTexture::new(server.load("player.aseprite")).sprite().lit(),
        AseAnimation::tag("walk-right"),
        AseFlip::default(),
        Transform::from_translation(Vec3::new(0., 0., 0.)),
    ));
}

/// Logs once whether the loaded aseprite carries a normal-map atlas. Useful
/// for verifying the loader picked up `.normal` layers or a sibling file.
fn log_normal_atlas_status(
    mut events: MessageReader<AssetEvent<Aseprite>>,
    aseprites: Res<Assets<Aseprite>>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        let Some(ase) = aseprites.get(*id) else {
            continue;
        };
        match &ase.normal_atlas_image {
            Some(_) => info!("aseprite '{}' loaded with normal-map atlas", ase.source_path),
            None => info!(
                "aseprite '{}' loaded without a normal-map atlas (no `.normal` layers and no sibling `<stem>.normal.<ext>` file)",
                ase.source_path
            ),
        }
    }
}
