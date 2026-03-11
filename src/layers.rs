use crate::animation::{AseAnimation, Animation};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use bevy::prelude::*;
use bevy::ui::widget::ImageNode;
use msg_interned_id::InternedId;

/// Controls whether layer children render as world [`Sprite`]s or UI
/// [`ImageNode`]s.
#[derive(Clone, Debug, Default)]
pub enum RenderTarget {
    /// Render as world sprites (default). Children get [`Sprite`] + [`Transform`].
    #[default]
    Sprite,
    /// Render as UI nodes. Children get [`ImageNode`] + [`Node`] + [`ZIndex`].
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

/// Selects which layers to spawn as children.
///
/// ```rust,no_run
/// # use bevy_aseprite_ultra::prelude::*;
/// // All layers including hidden ones
/// let all = LayerFilter::All;
///
/// // Only layers marked visible in the aseprite file (default)
/// let visible = LayerFilter::Visible;
///
/// // Only specific named layers
/// let specific = LayerFilter::Include(vec![
///     LayerId::new("body"),
///     LayerId::new("hat"),
/// ]);
/// ```
#[derive(Clone, Debug)]
pub enum LayerFilter {
    /// All layers including hidden ones.
    All,
    /// Only layers marked visible in the aseprite file (default).
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

/// Spawns a child [`AseAnimation`] per layer for full composition control.
///
/// Each child entity gets [`ChildOf`], [`SpriteLayerOf`], [`LayerId`], and
/// the appropriate render component ([`Sprite`] or [`ImageNode`]).
/// The parent entity does **not** render — the children handle all rendering.
///
/// Toggle layer visibility at runtime via [`Visibility`] on the children.
/// Mutating this component diffs existing children: only layers that were
/// added or removed are spawned or despawned.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// cmd.spawn(AseLayeredAnimation {
///     animation: Animation::tag("idle"),
///     aseprite: server.load("character.aseprite"),
///     layers: LayerFilter::Visible,
///     render_target: RenderTarget::Sprite,
/// });
/// # }
/// ```
#[derive(Component, Clone)]
#[require(Visibility)]
#[require(InheritedVisibility)]
pub struct AseLayeredAnimation {
    pub animation: Animation,
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub render_target: RenderTarget,
}

/// Spawns a child [`AseSlice`] per layer for full composition control.
///
/// Each child entity gets [`ChildOf`], [`SpriteLayerOf`], [`LayerId`], and
/// the appropriate render component. Works identically to
/// [`AseLayeredAnimation`] but for static slices.
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// cmd.spawn(AseLayeredSlice {
///     name: "ghost_red".into(),
///     aseprite: server.load("icons.aseprite"),
///     layers: LayerFilter::All,
///     render_target: RenderTarget::Sprite,
/// });
/// # }
/// ```
#[derive(Component, Clone)]
#[require(Visibility)]
#[require(InheritedVisibility)]
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
            Name::new(layer_id.as_str().to_owned()),
        );
        match render_target {
            RenderTarget::Sprite => {
                cmd.spawn((
                    common,
                    ase_animation,
                    Sprite::default(),
                    Transform::from_translation(Vec3::new(0., 0., z as f32 * 0.001)),
                ));
            }
            RenderTarget::Ui => {
                cmd.spawn((
                    common,
                    ase_animation,
                    ImageNode::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        ..default()
                    },
                    ZIndex(z as i32),
                ));
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
            Name::new(layer_id.as_str().to_owned()),
        );
        match render_target {
            RenderTarget::Sprite => {
                cmd.spawn((
                    common,
                    ase_slice,
                    Sprite::default(),
                    Transform::from_translation(Vec3::new(0., 0., z as f32 * 0.001)),
                ));
            }
            RenderTarget::Ui => {
                cmd.spawn((
                    common,
                    ase_slice,
                    ImageNode::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        ..default()
                    },
                    ZIndex(z as i32),
                ));
            }
        }
    }
}
