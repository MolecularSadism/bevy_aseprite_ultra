use crate::animation::{AseAnimation, AnimationLayer, AnimationState};
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
                spawn_layers,
                update_layers,
                propagate_flip,
            ),
        );
    }
}

/// Type-safe interned layer name. O(1) comparisons, `Copy`.
#[derive(InternedId, Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayerId(bevy::ecs::intern::Interned<str>);

/// Type-safe interned slice name. O(1) comparisons, `Copy`.
#[derive(InternedId, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SliceId(bevy::ecs::intern::Interned<str>);

/// Selects which layers to spawn as children.
///
/// ```rust
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
#[derive(Clone, Debug, Default)]
pub enum LayerFilter {
    /// All layers including hidden ones.
    All,
    /// Only layers marked visible in the aseprite file (default).
    #[default]
    Visible,
    /// Only these specific layers.
    Include(Vec<LayerId>),
}

/// Relationship: this entity is a sprite layer of the target entity.
#[derive(Component)]
#[relationship(relationship_target = SpriteLayers)]
pub struct SpriteLayerOf(pub Entity);

/// Auto-populated collection of layer entities.
#[derive(Component, Default)]
#[relationship_target(relationship = SpriteLayerOf)]
pub struct SpriteLayers(Vec<Entity>);

/// The primary component for displaying aseprite assets.
///
/// Always spawns child entities for rendering — the parent entity itself does
/// not render. Use [`baked`](AseTexture::baked) mode for a single composite
/// child, or the default layered mode for per-layer children.
///
/// Add [`AseAnimation`] alongside this component to opt into animation ticking.
/// Without it, children are fully static with zero per-tick overhead.
///
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// // Layered animation (default)
/// cmd.spawn((
///     AseTexture::new(server.load("player.aseprite")).sprite(),
///     AseAnimation::tag("walk"),
/// ));
///
/// // Baked animation (single composite child)
/// cmd.spawn((
///     AseTexture::baked(server.load("player.aseprite")).sprite(),
///     AseAnimation::tag("idle"),
/// ));
///
/// // Static slice (no animation)
/// cmd.spawn(
///     AseTexture::new(server.load("icons.aseprite"))
///         .with_slice("ghost_red")
///         .sprite(),
/// );
/// # }
/// ```
#[derive(Component, Clone, Debug)]
#[require(Visibility)]
#[require(InheritedVisibility)]
pub struct AseTexture {
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub slice: Option<SliceId>,
    pub baked: bool,
    pub render_target: RenderTarget,
}

impl AseTexture {
    /// Layered mode (default). Spawns one child per visible layer.
    pub fn new(aseprite: Handle<Aseprite>) -> Self {
        AseTexture {
            aseprite,
            layers: LayerFilter::Visible,
            slice: None,
            baked: false,
            render_target: RenderTarget::Sprite,
        }
    }

    /// Baked mode. Spawns a single composite child.
    pub fn baked(aseprite: Handle<Aseprite>) -> Self {
        AseTexture {
            aseprite,
            layers: LayerFilter::Visible,
            slice: None,
            baked: true,
            render_target: RenderTarget::Sprite,
        }
    }

    /// Set the slice name. Enables slice-based rendering.
    pub fn with_slice(mut self, name: &str) -> Self {
        self.slice = Some(SliceId::new(name));
        self
    }

    /// Set the layer filter.
    pub fn with_layers(mut self, layers: LayerFilter) -> Self {
        self.layers = layers;
        self
    }

    /// Set the render target.
    pub fn with_render_target(mut self, target: RenderTarget) -> Self {
        self.render_target = target;
        self
    }

    /// Use [`Sprite`] as the render target (2D world).
    pub fn sprite(mut self) -> Self {
        self.render_target = RenderTarget::Sprite;
        self
    }

    /// Use [`ImageNode`] as the render target (UI).
    pub fn ui(mut self) -> Self {
        self.render_target = RenderTarget::Ui;
        self
    }
}

/// Flip state that propagates to all child render entities.
///
/// Place on the parent entity alongside [`AseTexture`].
#[derive(Component, Default, Reflect, Clone, Debug)]
#[reflect]
pub struct AseFlip {
    pub x: bool,
    pub y: bool,
}

// ---- systems ----

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

/// Spawns child entities for newly added [`AseTexture`] components.
fn spawn_layers(
    mut cmd: Commands,
    query: Query<(Entity, &AseTexture, Has<AseAnimation>, Option<&AseFlip>), Without<SpriteLayers>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, tex, has_anim, flip) in &query {
        let Some(aseprite) = assets.get(&tex.aseprite) else {
            continue;
        };

        spawn_children(&mut cmd, &server, entity, aseprite, tex, has_anim, flip);
    }
}

/// Diffs children when [`AseTexture`] changes.
fn update_layers(
    mut cmd: Commands,
    query: Query<
        (Entity, &AseTexture, Has<AseAnimation>, &SpriteLayers, Option<&AseFlip>),
        Changed<AseTexture>,
    >,
    layer_ids: Query<&LayerId, With<SpriteLayerOf>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, tex, has_anim, sprite_layers, flip) in &query {
        let Some(aseprite) = assets.get(&tex.aseprite) else {
            continue;
        };

        if tex.baked {
            for child in sprite_layers.iter() {
                cmd.entity(child).despawn();
            }
            spawn_children(&mut cmd, &server, entity, aseprite, tex, has_anim, flip);
        } else {
            let desired = matching_layers(aseprite, &tex.layers);

            let mut existing: Vec<LayerId> = Vec::new();
            let mut has_non_layer_children = false;
            for child in sprite_layers.iter() {
                if let Ok(id) = layer_ids.get(child) {
                    if desired.contains(id) {
                        existing.push(*id);
                    } else {
                        cmd.entity(child).despawn();
                    }
                } else {
                    cmd.entity(child).despawn();
                    has_non_layer_children = true;
                }
            }

            if has_non_layer_children {
                spawn_children(&mut cmd, &server, entity, aseprite, tex, has_anim, flip);
            } else {
                let to_spawn: Vec<LayerId> = desired
                    .into_iter()
                    .filter(|id| !existing.contains(id))
                    .collect();

                if !to_spawn.is_empty() {
                    spawn_layered_children(
                        &mut cmd, &server, entity, aseprite, tex, has_anim, &to_spawn, flip,
                    );
                }
            }
        }
    }
}

/// Propagates [`AseFlip`] to children's [`Sprite`] and [`ImageNode`].
fn propagate_flip(
    parents: Query<(&AseFlip, &SpriteLayers), Changed<AseFlip>>,
    mut sprites: Query<&mut Sprite>,
    mut image_nodes: Query<&mut ImageNode>,
) {
    for (flip, layers) in &parents {
        for child in layers.iter() {
            if let Ok(mut sprite) = sprites.get_mut(child) {
                sprite.flip_x = flip.x;
                sprite.flip_y = flip.y;
            }
            if let Ok(mut node) = image_nodes.get_mut(child) {
                node.flip_x = flip.x;
                node.flip_y = flip.y;
            }
        }
    }
}

// ---- helpers ----

fn spawn_children(
    cmd: &mut Commands,
    server: &AssetServer,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    has_anim: bool,
    flip: Option<&AseFlip>,
) {
    if tex.baked {
        spawn_baked_child(cmd, parent, tex, has_anim, flip);
    } else {
        let layers = matching_layers(aseprite, &tex.layers);
        spawn_layered_children(cmd, server, parent, aseprite, tex, has_anim, &layers, flip);
    }
}

fn spawn_baked_child(
    cmd: &mut Commands,
    parent: Entity,
    tex: &AseTexture,
    has_anim: bool,
    flip: Option<&AseFlip>,
) {
    let common = (
        ChildOf(parent),
        SpriteLayerOf(parent),
        Name::new("baked"),
    );

    match &tex.render_target {
        RenderTarget::Sprite => {
            let mut sprite = Sprite::default();
            if let Some(flip) = flip {
                sprite.flip_x = flip.x;
                sprite.flip_y = flip.y;
            }
            let mut entity_cmd = cmd.spawn((common, sprite));
            if has_anim {
                entity_cmd.insert((
                    AnimationLayer::new(tex.aseprite.clone()),
                    AnimationState::default(),
                ));
            }
            if let Some(slice_id) = &tex.slice {
                entity_cmd.insert(AseSlice {
                    name: slice_id.as_str().to_owned(),
                    aseprite: tex.aseprite.clone(),
                });
            }
        }
        RenderTarget::Ui => {
            let mut node = ImageNode::default();
            if let Some(flip) = flip {
                node.flip_x = flip.x;
                node.flip_y = flip.y;
            }
            let mut entity_cmd = cmd.spawn((
                common,
                node,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
            ));
            if has_anim {
                entity_cmd.insert((
                    AnimationLayer::new(tex.aseprite.clone()),
                    AnimationState::default(),
                ));
            }
            if let Some(slice_id) = &tex.slice {
                entity_cmd.insert(AseSlice {
                    name: slice_id.as_str().to_owned(),
                    aseprite: tex.aseprite.clone(),
                });
            }
        }
    }
}

fn spawn_layered_children(
    cmd: &mut Commands,
    server: &AssetServer,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    has_anim: bool,
    layers: &[LayerId],
    flip: Option<&AseFlip>,
) {
    for (z, layer_id) in layers.iter().enumerate() {
        let layer_path = format!("{}#{}", aseprite.source_path, layer_id.as_str());
        let layer_handle: Handle<Aseprite> = server.load(&layer_path);

        let common = (
            ChildOf(parent),
            SpriteLayerOf(parent),
            *layer_id,
            Name::new(layer_id.as_str().to_owned()),
        );

        match &tex.render_target {
            RenderTarget::Sprite => {
                let mut sprite = Sprite::default();
                if let Some(flip) = flip {
                    sprite.flip_x = flip.x;
                    sprite.flip_y = flip.y;
                }
                let mut entity_cmd = cmd.spawn((
                    common,
                    sprite,
                    Transform::from_translation(Vec3::new(0., 0., z as f32 * 0.001)),
                ));
                if has_anim {
                    entity_cmd.insert((
                        AnimationLayer::new(layer_handle.clone()),
                        AnimationState::default(),
                    ));
                }
                if let Some(slice_id) = &tex.slice {
                    entity_cmd.insert(AseSlice {
                        name: slice_id.as_str().to_owned(),
                        aseprite: layer_handle,
                    });
                }
            }
            RenderTarget::Ui => {
                let mut node = ImageNode::default();
                if let Some(flip) = flip {
                    node.flip_x = flip.x;
                    node.flip_y = flip.y;
                }
                let mut entity_cmd = cmd.spawn((
                    common,
                    node,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        ..default()
                    },
                    ZIndex(z as i32),
                ));
                if has_anim {
                    entity_cmd.insert((
                        AnimationLayer::new(layer_handle.clone()),
                        AnimationState::default(),
                    ));
                }
                if let Some(slice_id) = &tex.slice {
                    entity_cmd.insert(AseSlice {
                        name: slice_id.as_str().to_owned(),
                        aseprite: layer_handle,
                    });
                }
            }
        }
    }
}
