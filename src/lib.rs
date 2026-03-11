#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(rustdoc::redundant_explicit_links)]
#![doc = include_str!("../README.md")]

use bevy::prelude::*;
use bevy::sprite_render::Material2d;

pub(crate) mod animation;
pub(crate) mod error;
pub(crate) mod layers;
pub(crate) mod loader;
#[cfg(feature = "asset_processing")]
pub(crate) mod processor;
pub(crate) mod slice;

pub mod prelude {
    pub use crate::animation::{
        render_animation, Animation, AnimationDirection, AnimationEvents, AnimationRepeat,
        AnimationState, AseAnimation, ManualTick, NextFrameEvent, PlayDirection, RenderAnimation,
    };
    pub use crate::layers::{
        AseLayeredAnimation, AseLayeredSlice, LayerFilter, LayerId, RenderTarget, SpriteLayerOf,
        SpriteLayers,
    };
    pub use crate::loader::{Aseprite, AsepriteLoaderPlugin, AsepriteLoaderSettings, SliceMeta};
    pub use crate::slice::{render_slice, AseSlice, RenderSlice};
    pub use crate::AseBundled;
    pub use crate::AsepriteUltraPlugin;
}

/// Trait for Aseprite components that can be paired with a render target
/// to form a spawnable bundle.
///
/// Implemented for [`AseAnimation`](animation::AseAnimation) and
/// [`AseSlice`](slice::AseSlice). Use the constructor (`new`) to build the
/// component, then call one of these methods to pair it with a render target:
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// // Sprite (2D world)
/// cmd.spawn(AseAnimation::new(Animation::tag("walk"), server.load("player.aseprite")).sprite());
///
/// // UI image node
/// cmd.spawn(AseSlice::new(server.load("icons.aseprite"), "ghost_red").ui());
/// # }
/// ```
pub trait AseBundled: Sized {
    /// Pair with a [`Sprite`] render target (2D world).
    fn sprite(self) -> (Self, Sprite) {
        (self, Sprite::default())
    }

    /// Pair with an [`ImageNode`] render target (UI).
    fn ui(self) -> (Self, ImageNode) {
        (self, ImageNode::default())
    }

    /// Pair with a [`MeshMaterial2d`] render target for custom 2D shaders.
    fn material_2d<M: Material2d>(self, handle: Handle<M>) -> (Self, MeshMaterial2d<M>) {
        (self, MeshMaterial2d(handle))
    }

    /// Pair with a [`MaterialNode`] render target for custom UI shaders.
    fn material_node<M: UiMaterial>(self, handle: Handle<M>) -> (Self, MaterialNode<M>) {
        (self, MaterialNode(handle))
    }

    /// Pair with a [`MeshMaterial3d`] render target for custom 3D shaders.
    #[cfg(feature = "3d")]
    fn material_3d<M: bevy::pbr::Material>(
        self,
        handle: Handle<M>,
    ) -> (Self, MeshMaterial3d<M>) {
        (self, MeshMaterial3d(handle))
    }
}

impl AseBundled for slice::AseSlice {}
impl AseBundled for animation::AseAnimation {}

/// The main plugin. Add this to your [`App`] to enable aseprite loading,
/// animation, slices, and layered rendering.
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
///         .add_systems(Startup, setup)
///         .run();
/// }
///
/// fn setup(mut cmd: Commands, server: Res<AssetServer>) {
///     cmd.spawn(Camera2d);
///
///     // Sprite animation
///     cmd.spawn(AseAnimation::new(
///         Animation::tag("walk-right"),
///         server.load("player.aseprite"),
///     ).sprite());
///
///     // Static sprite slice
///     cmd.spawn(AseSlice::new(
///         server.load("icons.aseprite"),
///         "ghost_red",
///     ).sprite());
///
///     // Layered animation — each layer becomes a child entity
///     cmd.spawn(AseLayeredAnimation {
///         animation: Animation::tag("idle"),
///         aseprite: server.load("character.aseprite"),
///         layers: LayerFilter::Visible,
///         render_target: RenderTarget::Sprite,
///     });
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
