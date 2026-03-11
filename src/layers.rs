use crate::animation::{AseAnimation, Animation};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use bevy::prelude::*;
use bevy::ui::widget::ImageNode;
use msg_interned_id::InternedId;

/// Whether layer children render as world sprites or UI nodes.
#[derive(Clone, Debug, Default)]
pub enum RenderTarget {
    #[default]
    Sprite,
    Ui,
}

pub struct AsepriteLayersPlugin;

impl Plugin for AsepriteLayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (
                spawn_animation_layers,
                spawn_slice_layers,
                update_animation_layers,
                update_slice_layers,
            ),
        );
    }
}

/// Type-safe interned layer name. O(1) comparisons, `Copy`.
#[derive(InternedId, Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayerId(bevy::ecs::intern::Interned<str>);

/// Which layers to spawn as children.
#[derive(Clone, Debug)]
pub enum LayerFilter {
    /// All layers including hidden ones.
    All,
    /// Only layers marked visible in the aseprite file.
    Visible,
    /// Only these specific layers.
    Include(Vec<LayerId>),
}

impl Default for LayerFilter {
    fn default() -> Self {
        Self::Visible
    }
}

/// Relationship: this entity is a sprite layer of the target entity.
#[derive(Component)]
#[relationship(relationship_target = SpriteLayers)]
pub struct SpriteLayerOf(pub Entity);

/// Auto-populated collection of layer entities.
#[derive(Component, Default)]
#[relationship_target(relationship = SpriteLayerOf)]
pub struct SpriteLayers(Vec<Entity>);

/// Spawns a child [`AseAnimation`] per layer. Each child automatically gets a
/// [`Sprite`] (via [`AseAnimation`]'s required components), [`ChildOf`] (for
/// transform propagation), [`SpriteLayerOf`], and [`LayerId`].
///
/// The parent entity does **not** need a [`Sprite`] — the children handle rendering.
/// Toggle layer visibility at runtime via [`Visibility`] on the children.
///
/// Mutating this component at runtime will diff existing children: only
/// layers that were added/removed are spawned/despawned.
#[derive(Component, Clone)]
#[require(Visibility)]
pub struct AseLayeredAnimation {
    pub animation: Animation,
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub render_target: RenderTarget,
}

/// Spawns a child [`AseSlice`] per layer. Each child gets a [`Sprite`],
/// [`ChildOf`] (for transform propagation), [`SpriteLayerOf`], and [`LayerId`].
///
/// The parent entity does **not** need a [`Sprite`] — the children handle rendering.
/// Toggle layer visibility at runtime via [`Visibility`] on the children.
///
/// Mutating this component at runtime will diff existing children: only
/// layers that were added/removed are spawned/despawned.
#[derive(Component, Clone)]
pub struct AseLayeredSlice {
    pub name: String,
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub render_target: RenderTarget,
}

fn matching_layers(aseprite: &Aseprite, filter: &LayerFilter) -> Vec<LayerId> {
    match filter {
        LayerFilter::All => aseprite.layer_names.clone(),
        LayerFilter::Visible => aseprite.visible_layer_names.clone(),
        LayerFilter::Include(names) => aseprite
            .layer_names
            .iter()
            .filter(|id| names.contains(id))
            .copied()
            .collect(),
    }
}

fn spawn_animation_layers(
    mut cmd: Commands,
    query: Query<(Entity, &AseLayeredAnimation), Without<SpriteLayers>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, comp) in &query {
        let Some(aseprite) = assets.get(&comp.aseprite) else {
            continue;
        };

        let layers = matching_layers(aseprite, &comp.layers);
        spawn_animation_children(&mut cmd, &server, entity, aseprite, &comp.animation, &layers, &comp.render_target);
    }
}

fn spawn_slice_layers(
    mut cmd: Commands,
    query: Query<(Entity, &AseLayeredSlice), Without<SpriteLayers>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, comp) in &query {
        let Some(aseprite) = assets.get(&comp.aseprite) else {
            continue;
        };

        let layers = matching_layers(aseprite, &comp.layers);
        spawn_slice_children(&mut cmd, &server, entity, aseprite, &comp.name, &layers, &comp.render_target);
    }
}

/// When `AseAnimationLayers` changes, diff children: despawn removed layers, spawn added ones.
fn update_animation_layers(
    mut cmd: Commands,
    query: Query<
        (Entity, &AseLayeredAnimation, &SpriteLayers),
        Changed<AseLayeredAnimation>,
    >,
    layer_ids: Query<&LayerId, With<SpriteLayerOf>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, comp, sprite_layers) in &query {
        let Some(aseprite) = assets.get(&comp.aseprite) else {
            continue;
        };

        let desired = matching_layers(aseprite, &comp.layers);

        // Collect existing layer IDs and despawn children not in the desired set.
        let mut existing: Vec<LayerId> = Vec::new();
        for child in sprite_layers.iter() {
            if let Ok(id) = layer_ids.get(child) {
                if desired.contains(id) {
                    existing.push(*id);
                } else {
                    cmd.entity(child).despawn();
                }
            }
        }

        // Spawn layers that don't exist yet.
        let to_spawn: Vec<LayerId> = desired
            .into_iter()
            .filter(|id| !existing.contains(id))
            .collect();

        if !to_spawn.is_empty() {
            spawn_animation_children(
                &mut cmd,
                &server,
                entity,
                aseprite,
                &comp.animation,
                &to_spawn,
                &comp.render_target,
            );
        }
    }
}

/// When `AseSliceLayers` changes, diff children: despawn removed layers, spawn added ones.
fn update_slice_layers(
    mut cmd: Commands,
    query: Query<
        (Entity, &AseLayeredSlice, &SpriteLayers),
        Changed<AseLayeredSlice>,
    >,
    layer_ids: Query<&LayerId, With<SpriteLayerOf>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, comp, sprite_layers) in &query {
        let Some(aseprite) = assets.get(&comp.aseprite) else {
            continue;
        };

        let desired = matching_layers(aseprite, &comp.layers);

        let mut existing: Vec<LayerId> = Vec::new();
        for child in sprite_layers.iter() {
            if let Ok(id) = layer_ids.get(child) {
                if desired.contains(id) {
                    existing.push(*id);
                } else {
                    cmd.entity(child).despawn();
                }
            }
        }

        let to_spawn: Vec<LayerId> = desired
            .into_iter()
            .filter(|id| !existing.contains(id))
            .collect();

        if !to_spawn.is_empty() {
            spawn_slice_children(
                &mut cmd,
                &server,
                entity,
                aseprite,
                &comp.name,
                &to_spawn,
                &comp.render_target,
            );
        }
    }
}

// ---- helpers ----

fn spawn_animation_children(
    cmd: &mut Commands,
    server: &AssetServer,
    parent: Entity,
    aseprite: &Aseprite,
    animation: &Animation,
    layers: &[LayerId],
    render_target: &RenderTarget,
) {
    for (z, layer_id) in layers.iter().enumerate() {
        let layer_path = format!("{}#{}", aseprite.source_path, layer_id.as_str());
        let ase_animation = AseAnimation {
            animation: animation.clone(),
            aseprite: server.load(&layer_path),
        };
        let common = (
            ChildOf(parent),
            SpriteLayerOf(parent),
            *layer_id,
            Transform::from_translation(Vec3::new(0., 0., z as f32 * 0.001)),
            Name::new(layer_id.as_str().to_owned()),
        );
        match render_target {
            RenderTarget::Sprite => {
                cmd.spawn((common, ase_animation, Sprite::default()));
            }
            RenderTarget::Ui => {
                cmd.spawn((common, ase_animation, ImageNode::default(), Node::default()));
            }
        }
    }
}

fn spawn_slice_children(
    cmd: &mut Commands,
    server: &AssetServer,
    parent: Entity,
    aseprite: &Aseprite,
    slice_name: &str,
    layers: &[LayerId],
    render_target: &RenderTarget,
) {
    for (z, layer_id) in layers.iter().enumerate() {
        let layer_path = format!("{}#{}", aseprite.source_path, layer_id.as_str());
        let ase_slice = AseSlice {
            name: slice_name.to_owned(),
            aseprite: server.load(&layer_path),
        };
        let common = (
            ChildOf(parent),
            SpriteLayerOf(parent),
            *layer_id,
            Transform::from_translation(Vec3::new(0., 0., z as f32 * 0.001)),
            Name::new(layer_id.as_str().to_owned()),
        );
        match render_target {
            RenderTarget::Sprite => {
                cmd.spawn((common, ase_slice, Sprite::default()));
            }
            RenderTarget::Ui => {
                cmd.spawn((common, ase_slice, ImageNode::default(), Node::default()));
            }
        }
    }
}
