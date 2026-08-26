#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(rustdoc::redundant_explicit_links)]
#![doc = include_str!("../README.md")]

use bevy::prelude::*;

pub(crate) mod animation;
pub(crate) mod error;
pub(crate) mod layers;
pub(crate) mod loader;
#[cfg(feature = "asset_processing")]
pub(crate) mod processor;
pub(crate) mod slice;

pub mod prelude {
    pub use crate::AsepriteUltraPlugin;
    pub use crate::animation::{
        AnimationDirection, AnimationEvent, AnimationFrameChanged, AnimationFrameCursor,
        AnimationLayer, AnimationRepeat, AnimationState, AseAnimation, AseFrame, AseTag,
        ManualTick, NextFrame, PlayDirection, RenderAnimation, emit_animation_frame_changed,
        render_children_animation, resolve_frame,
    };
    pub use crate::layers::{
        AseFlip, AseRenderTarget, AseTexture, LayerEntry, LayerFilter, LayerId, SliceId,
        SpriteLayerOf, SpriteLayers,
    };
    pub use crate::error::AsepriteError;
    pub use crate::loader::{
        Aseprite, AsepriteLoader, AsepriteLoaderPlugin, AsepriteLoaderSettings, SliceKeyMeta,
        SliceMeta, TagMeta,
    };
    pub use crate::slice::{AseSlice, RenderSlice, nine_patch_to_slicer, render_slice};
}

/// The main plugin. Add this to your [`App`] to enable aseprite loading,
/// animation, slices, and layered rendering.
///
/// Compiled but not run: `DefaultPlugins` builds a winit event loop, which
/// needs a display.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// fn main() {
///     App::new()
///         .add_plugins(DefaultPlugins.set(ImagePlugin {
///             default_sampler: bevy::image::ImageSamplerDescriptor::nearest(),
///         }))
///         .add_plugins(AsepriteUltraPlugin)
///         .add_systems(Startup, setup);
///         // .run();
/// }
///
/// fn setup(mut cmd: Commands, server: Res<AssetServer>) {
///     cmd.spawn(Camera2d);
///
///     // Animated sprite (layered)
///     cmd.spawn((
///         AseTexture::new(server.load("player.aseprite")).sprite(),
///         AseAnimation::tag("walk-right"),
///     ));
///
///     // Static slice
///     cmd.spawn(
///         AseTexture::baked(server.load("icons.aseprite"))
///             .with_slice("ghost_red")
///             .sprite(),
///     );
///
///     // Baked animation
///     cmd.spawn((
///         AseTexture::baked(server.load("player.aseprite")).sprite(),
///         AseAnimation::tag("idle"),
///     ));
/// }
/// ```
pub struct AsepriteUltraPlugin;
impl Plugin for AsepriteUltraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(loader::AsepriteLoaderPlugin);
        app.add_plugins(slice::AsepriteSlicePlugin);
        app.add_plugins(animation::AsepriteAnimationPlugin);
        app.add_plugins(layers::AsepriteLayersPlugin);
        #[cfg(feature = "asset_processing")]
        app.add_plugins(processor::AsepriteProcessorPlugin);
    }
}
