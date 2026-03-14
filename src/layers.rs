use crate::animation::{AseAnimation, AnimationLayer};
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
                propagate_offset,
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

/// Selects which layers are visible. All layers are always spawned as children;
/// this filter only controls which children have [`Visibility::Inherited`] vs
/// [`Visibility::Hidden`].
///
/// ```rust
/// # use bevy_aseprite_ultra::prelude::*;
/// // All layers visible including hidden ones
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
    /// All layers visible including hidden ones.
    All,
    /// Only layers marked visible in the aseprite file (default).
    #[default]
    Visible,
    /// Only these specific layers visible.
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
/// In layered mode **all** layers from the aseprite file are always spawned as
/// children. The [`layers`](AseTexture::layers) filter only controls which
/// children are visible; it does not affect which entities exist. This avoids
/// entity churn when switching visibility rapidly and makes z-ordering stable
/// (set once at spawn time, never recalculated).
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
    /// Offset applied relatively to child render entities' transforms (Sprite)
    /// or node positions (UI).
    pub offset: Vec2,
}

impl AseTexture {
    /// Layered mode (default). Spawns one child per layer (all layers).
    pub fn new(aseprite: Handle<Aseprite>) -> Self {
        AseTexture {
            aseprite,
            layers: LayerFilter::Visible,
            slice: None,
            baked: false,
            render_target: RenderTarget::Sprite,
            offset: default(),
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
            offset: default(),
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

    /// Set the offset applied to child render entities.
    pub fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    /// Show a layer. Adds it to the [`LayerFilter::Include`] list if not
    /// already present.
    ///
    /// Has no effect when the filter is [`LayerFilter::All`] or
    /// [`LayerFilter::Visible`] (all relevant layers are already shown).
    /// Switch to [`LayerFilter::Include`] first to toggle individual layers.
    ///
    /// Mutating `AseTexture` triggers the visibility update system.
    pub fn toggle_layer_on(&mut self, layer: LayerId) {
        if let LayerFilter::Include(ids) = &mut self.layers {
            if !ids.contains(&layer) {
                ids.push(layer);
            }
        }
    }

    /// Hide a layer. Removes it from the [`LayerFilter::Include`] list.
    ///
    /// Has no effect when the filter is [`LayerFilter::All`] or
    /// [`LayerFilter::Visible`]. Switch to [`LayerFilter::Include`] first to
    /// toggle individual layers.
    ///
    /// Mutating `AseTexture` triggers the visibility update system.
    pub fn toggle_layer_off(&mut self, layer: LayerId) {
        if let LayerFilter::Include(ids) = &mut self.layers {
            ids.retain(|id| *id != layer);
        }
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

/// Tracks the last applied offset so changes can be applied relatively.
#[derive(Component, Default, Clone, Debug)]
struct AppliedOffset(Vec2);

// ---- systems ----

fn visible_layers(aseprite: &Aseprite, filter: &LayerFilter) -> Vec<LayerId> {
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

/// Updates children when [`AseTexture`] changes.
///
/// In layered mode (non-baked): all layer children are always kept alive.
/// Only their [`Visibility`] is toggled based on the current filter. This
/// avoids entity churn when the filter changes rapidly, and z-ordering never
/// needs to be recalculated.
///
/// A full respawn only happens when the underlying aseprite asset changes
/// (different layer set detected).
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
            let all_layers = &aseprite.layer_names;

            // Check whether existing children exactly match the aseprite's full
            // layer set. If not (e.g. aseprite handle changed), do a full respawn.
            let children_match = {
                let count = sprite_layers.iter().count();
                count == all_layers.len()
                    && sprite_layers.iter().all(|child| {
                        layer_ids
                            .get(child)
                            .map(|id| all_layers.contains(id))
                            .unwrap_or(false)
                    })
            };

            if !children_match {
                for child in sprite_layers.iter() {
                    cmd.entity(child).despawn();
                }
                spawn_children(&mut cmd, &server, entity, aseprite, tex, has_anim, flip);
            } else {
                // Fast path: only toggle visibility, no spawning or z-reordering.
                let visible = visible_layers(aseprite, &tex.layers);
                for child in sprite_layers.iter() {
                    if let Ok(id) = layer_ids.get(child) {
                        let vis = if visible.contains(id) {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                        cmd.entity(child).insert(vis);
                    }
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

/// Propagates [`AseTexture::offset`] changes relatively to children.
///
/// Computes the delta between the new offset and the previously applied one,
/// then adds that delta to each child's [`Transform`] (Sprite mode) or
/// [`Node`] position (UI mode). This preserves any other positional changes
/// made by other systems (e.g. z-ordering).
fn propagate_offset(
    parents: Query<(&AseTexture, &SpriteLayers), Changed<AseTexture>>,
    mut sprites: Query<(&mut Transform, &mut AppliedOffset)>,
    mut ui_nodes: Query<(&mut Node, &mut AppliedOffset), Without<Transform>>,
) {
    for (tex, layers) in &parents {
        let new_offset = tex.offset;

        for child in layers.iter() {
            match &tex.render_target {
                RenderTarget::Sprite => {
                    if let Ok((mut transform, mut applied)) = sprites.get_mut(child) {
                        let delta = new_offset - applied.0;
                        transform.translation.x += delta.x;
                        transform.translation.y += delta.y;
                        applied.0 = new_offset;
                    }
                }
                RenderTarget::Ui => {
                    if let Ok((mut node, mut applied)) = ui_nodes.get_mut(child) {
                        node.left = Val::Px(new_offset.x);
                        node.top = Val::Px(new_offset.y);
                        applied.0 = new_offset;
                    }
                }
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
        // Spawn ALL layers; visibility is determined by the filter.
        let visible = visible_layers(aseprite, &tex.layers);
        spawn_layered_children(
            cmd,
            server,
            parent,
            aseprite,
            tex,
            has_anim,
            &aseprite.layer_names,
            &visible,
            flip,
        );
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
            let offset_translation = Vec3::new(tex.offset.x, tex.offset.y, 0.);
            let mut entity_cmd = cmd.spawn((
                common,
                sprite,
                Transform::from_translation(offset_translation),
                AppliedOffset(tex.offset),
            ));
            if has_anim {
                entity_cmd.insert(AnimationLayer::new(tex.aseprite.clone()));
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
            let (left, top) = (tex.offset.x, tex.offset.y);
            let mut entity_cmd = cmd.spawn((
                common,
                node,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    left: Val::Px(left),
                    top: Val::Px(top),
                    ..default()
                },
                AppliedOffset(tex.offset),
            ));
            if has_anim {
                entity_cmd.insert(AnimationLayer::new(tex.aseprite.clone()));
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

/// Spawns one child entity per layer.
///
/// `layers` is the full ordered layer list (determines z-ordering and which
/// entities are created). `visible` is the subset that should start with
/// [`Visibility::Inherited`]; all others get [`Visibility::Hidden`].
///
/// Z-ordering is computed once here from the layer's position in `layers` and
/// never recalculated, since all layers are always present.
fn spawn_layered_children(
    cmd: &mut Commands,
    server: &AssetServer,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    has_anim: bool,
    layers: &[LayerId],
    visible: &[LayerId],
    flip: Option<&AseFlip>,
) {
    for (z, &layer_id) in layers.iter().enumerate() {
        let visibility = if visible.contains(&layer_id) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        let layer_path = format!("{}#{}", aseprite.source_path, layer_id.as_str());
        let layer_handle: Handle<Aseprite> = server.load(&layer_path);

        let common = (
            ChildOf(parent),
            SpriteLayerOf(parent),
            layer_id,
            Name::new(layer_id.as_str().to_owned()),
            visibility,
        );

        match &tex.render_target {
            RenderTarget::Sprite => {
                let mut sprite = Sprite::default();
                if let Some(flip) = flip {
                    sprite.flip_x = flip.x;
                    sprite.flip_y = flip.y;
                }
                let translation = Vec3::new(tex.offset.x, tex.offset.y, z as f32 * 0.001);
                let mut entity_cmd = cmd.spawn((
                    common,
                    sprite,
                    Transform::from_translation(translation),
                    AppliedOffset(tex.offset),
                ));
                if has_anim {
                    entity_cmd.insert(AnimationLayer::new(layer_handle.clone()));
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
                let (left, top) = (tex.offset.x, tex.offset.y);
                let mut entity_cmd = cmd.spawn((
                    common,
                    node,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.),
                        height: Val::Percent(100.),
                        left: Val::Px(left),
                        top: Val::Px(top),
                        ..default()
                    },
                    ZIndex(z as i32),
                    AppliedOffset(tex.offset),
                ));
                if has_anim {
                    entity_cmd.insert(AnimationLayer::new(layer_handle.clone()));
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
