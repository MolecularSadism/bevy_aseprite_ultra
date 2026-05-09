//! Lit-sprite rendering: pairs the color atlas with a normal-map atlas in a
//! [`Material2d`] suitable for user-supplied 2D lighting.
//!
//! Activate by adding `.lit()` to an [`AseTexture`](crate::layers::AseTexture)
//! and ensuring the underlying [`Aseprite`] has a non-`None`
//! [`normal_atlas_image`](crate::loader::Aseprite::normal_atlas_image).
//!
//! # Custom lighting
//!
//! [`AseLitMaterial`]'s default fragment shader outputs `color * tint` and
//! samples the normal but ignores it — the crate intentionally does not ship
//! a lighting model. To add lighting, define your own [`Material2d`] that
//! exposes the same bindings (color texture, normal texture, [`AseLitParams`]
//! uniform) and impl [`RenderAnimation`] / [`RenderSlice`] on it; the
//! existing `render_children_animation::<MeshMaterial2d<MyMaterial>>` and
//! `render_slice::<MeshMaterial2d<MyMaterial>>` systems plug it in.

use crate::animation::{AnimationLayer, AnimationState, RenderAnimation};
use crate::layers::{AseFlip, SpriteLayerOf, SpriteLayers};
use crate::loader::Aseprite;
use crate::slice::{RenderSlice, AseSlice};
use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

/// The lit-sprite plugin. Registered automatically by
/// [`AsepriteUltraPlugin`](crate::AsepriteUltraPlugin) when the `lit`
/// feature is enabled.
pub struct AsepriteLitPlugin;

impl Plugin for AsepriteLitPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ase_lit.wgsl");
        app.add_plugins(Material2dPlugin::<AseLitMaterial>::default());
        app.init_resource::<AseLitQuad>();
        app.add_systems(
            PostUpdate,
            (
                crate::animation::render_children_animation::<MeshMaterial2d<AseLitMaterial>>,
                crate::animation::render_animation::<MeshMaterial2d<AseLitMaterial>>,
                crate::slice::render_slice::<MeshMaterial2d<AseLitMaterial>>,
                propagate_flip_lit,
                promote_lit,
            ),
        );
    }
}

/// Per-instance shader parameters for [`AseLitMaterial`]. The
/// [`RenderAnimation`] / [`RenderSlice`] impls keep these in sync with the
/// current animation frame and slice rect; [`propagate_flip_lit`] keeps
/// `flip` in sync with [`AseFlip`].
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct AseLitParams {
    /// Atlas rect in pixel space: xy = min, zw = size.
    pub uv_rect: Vec4,
    /// Per-axis flip: `-1.0` mirrors, `1.0` leaves alone.
    pub flip: Vec2,
    pub _pad: Vec2,
    pub tint: Vec4,
}

impl Default for AseLitParams {
    fn default() -> Self {
        Self {
            uv_rect: Vec4::ZERO,
            flip: Vec2::ONE,
            _pad: Vec2::ZERO,
            tint: Vec4::ONE,
        }
    }
}

/// Two-texture lit aseprite material. Color and normal are sampled from the
/// shared aseprite atlases; rect and flip are driven per-entity by
/// [`AseLitParams`].
///
/// The default fragment shader outputs `color * tint`; users wanting actual
/// lighting should write a custom material with the same binding layout (see
/// the module docs).
#[derive(AsBindGroup, Asset, Clone, Debug, TypePath)]
pub struct AseLitMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub color: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub normal: Handle<Image>,
    #[uniform(4)]
    pub params: AseLitParams,
}

impl Default for AseLitMaterial {
    fn default() -> Self {
        Self {
            color: Handle::default(),
            normal: Handle::default(),
            params: AseLitParams::default(),
        }
    }
}

impl Material2d for AseLitMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_aseprite_ultra/ase_lit.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

impl RenderAnimation for AseLitMaterial {
    type Extra<'e> = ResMut<'e, Assets<TextureAtlasLayout>>;
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        layouts: &mut Self::Extra<'_>,
    ) {
        let Some(layout) = layouts.get(&aseprite.atlas_layout) else {
            return;
        };
        self.color = aseprite.atlas_image.clone();
        if let Some(n) = aseprite.normal_atlas_image.as_ref() {
            self.normal = n.clone();
        }
        let index = aseprite.get_atlas_index(usize::from(state.current_frame));
        let rect = layout.textures[index];
        let min = rect.min.as_vec2();
        let size = (rect.max - rect.min).as_vec2();
        self.params.uv_rect = Vec4::new(min.x, min.y, size.x, size.y);
    }
}

impl RenderSlice for AseLitMaterial {
    type Extra<'e> = ();
    fn render_slice(
        &mut self,
        aseprite: &Aseprite,
        slice_meta: &crate::loader::SliceMeta,
        _: &mut (),
    ) {
        self.color = aseprite.atlas_image.clone();
        if let Some(n) = aseprite.normal_atlas_image.as_ref() {
            self.normal = n.clone();
        }
        let min = slice_meta.rect.min;
        let size = slice_meta.rect.size();
        self.params.uv_rect = Vec4::new(min.x, min.y, size.x, size.y);
    }
}

/// Cached unit-quad mesh handle used for spawning lit children. One quad
/// shared across every lit child entity; per-frame size comes from the
/// child's [`Transform`] scale.
#[derive(Resource)]
pub struct AseLitQuad(pub Handle<Mesh>);

impl FromWorld for AseLitQuad {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self(meshes.add(Mesh::from(Rectangle::new(1.0, 1.0))))
    }
}

/// Marker placed on a layer child by `spawn_layered_children` when lit
/// rendering was requested. The [`promote_lit`] system replaces it with
/// `Mesh2d` + `MeshMaterial2d<AseLitMaterial>` once the underlying
/// [`Aseprite`] / [`TextureAtlasLayout`] resolve in `Assets`.
#[derive(Component, Debug, Clone)]
pub struct LitPending {
    pub aseprite: Handle<Aseprite>,
    pub atlas_index: usize,
    pub flip: Vec2,
}

fn promote_lit(
    mut cmd: Commands,
    mut pending: Query<(Entity, &LitPending, &mut Transform)>,
    aseprites: Res<Assets<Aseprite>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    quad: Res<AseLitQuad>,
    mut materials: ResMut<Assets<AseLitMaterial>>,
) {
    for (entity, pending, mut transform) in pending.iter_mut() {
        let Some(ase) = aseprites.get(&pending.aseprite) else {
            continue;
        };
        let Some(layout) = layouts.get(&ase.atlas_layout) else {
            continue;
        };
        let Some(normal) = ase.normal_atlas_image.as_ref() else {
            continue;
        };
        let rect = layout.textures[pending.atlas_index];
        let size = (rect.max - rect.min).as_vec2();
        let min = rect.min.as_vec2();
        // Mesh2d quad is unit-sized; scale to frame pixel size while keeping
        // the original z translation intact.
        transform.scale.x = size.x;
        transform.scale.y = size.y;
        let params = AseLitParams {
            uv_rect: Vec4::new(min.x, min.y, size.x, size.y),
            flip: pending.flip,
            tint: Vec4::ONE,
            _pad: Vec2::ZERO,
        };
        let mat = materials.add(AseLitMaterial {
            color: ase.atlas_image.clone(),
            normal: normal.clone(),
            params,
        });
        cmd.entity(entity)
            .insert((Mesh2d(quad.0.clone()), MeshMaterial2d(mat)))
            .remove::<LitPending>();
    }
}

/// Propagates [`AseFlip`] to the `flip` field of [`AseLitParams`] on lit
/// children. Mirror of `propagate_flip` for `Sprite`s but writing into the
/// material instead.
fn propagate_flip_lit(
    parents: Query<(&AseFlip, &SpriteLayers), Changed<AseFlip>>,
    children: Query<&MeshMaterial2d<AseLitMaterial>, With<SpriteLayerOf>>,
    mut materials: ResMut<Assets<AseLitMaterial>>,
) {
    for (flip, layers) in &parents {
        for child in layers.iter() {
            let Ok(handle) = children.get(child) else {
                continue;
            };
            let Some(mat) = materials.get_mut(&handle.0) else {
                continue;
            };
            mat.params.flip = Vec2::new(
                if flip.x { -1.0 } else { 1.0 },
                if flip.y { -1.0 } else { 1.0 },
            );
        }
    }
}

// `AseSlice` and `AnimationLayer` re-exports kept reachable for downstream code.
#[allow(unused)]
fn _trait_anchors() -> (AnimationLayer, AseSlice) {
    Default::default()
}
