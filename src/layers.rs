use crate::animation::{AseAnimation, AnimationLayer};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use bevy::image::TextureAtlas;
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
        app.add_observer(on_ase_texture_added);
        app.add_systems(
            PreUpdate,
            (
                spawn_layers_on_asset_load,
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

/// A layer entry combining the layer's identity with its file-defined visibility.
///
/// The [`Aseprite`] asset stores layers as `Vec<LayerEntry>` in **front-to-back
/// order** (index 0 = topmost layer in the Aseprite editor, renders in front).
/// Reorder or toggle `visible` at runtime to change rendering without replacing
/// the list.
#[derive(Clone, Debug)]
pub struct LayerEntry {
    pub id: LayerId,
    /// Whether the layer was marked visible in the aseprite file.
    /// Toggle at runtime to show/hide without removing from the list.
    pub visible: bool,
}

impl LayerEntry {
    pub fn new(id: LayerId, visible: bool) -> Self {
        Self { id, visible }
    }
}

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
#[require(ViewVisibility)]
pub struct AseTexture {
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub slice: Option<SliceId>,
    pub baked: bool,
    pub render_target: RenderTarget,
    /// Offset applied relatively to child render entities' transforms (Sprite)
    /// or node positions (UI).
    pub offset: Vec2,
    /// Per-entity layer order override. When `Some`, layers are z-ordered
    /// according to this list (index 0 = front, renders on top) instead of
    /// the asset's default order. Layers not in this list keep their
    /// asset-default z-position. Set to `None` to use the asset order.
    pub layer_order: Option<Vec<LayerId>>,
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
            layer_order: None,
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
            layer_order: None,
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

    /// Set a per-entity layer order override. Layers are ordered front-to-back
    /// (index 0 = topmost, renders in front). Only affects this entity's
    /// z-ordering, not the underlying asset.
    pub fn with_layer_order(mut self, order: Vec<LayerId>) -> Self {
        self.layer_order = Some(order);
        self
    }

    /// Override the z-order for a single layer on this entity.
    ///
    /// Moves the layer to `new_index` in the per-entity order list
    /// (0 = front). Initialises the override from the asset's default
    /// order on first call.
    ///
    /// Mutating `AseTexture` triggers the z-reorder system.
    pub fn reorder_layer(&mut self, layer: LayerId, new_index: usize) {
        let order = self.layer_order.get_or_insert_with(Vec::new);
        if let Some(old) = order.iter().position(|id| *id == layer) {
            let entry = order.remove(old);
            let idx = new_index.min(order.len());
            order.insert(idx, entry);
        }
    }

    /// Initialise `layer_order` from the asset's layer list if not already set.
    /// Call this before [`reorder_layer`](Self::reorder_layer) when you need
    /// the override list pre-populated with the asset's default order.
    pub fn init_layer_order_from(&mut self, aseprite: &Aseprite) {
        if self.layer_order.is_none() {
            self.layer_order = Some(aseprite.layer_ids().collect());
        }
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
        LayerFilter::All => aseprite.layers.iter().map(|e| e.id).collect(),
        LayerFilter::Visible => aseprite
            .layers
            .iter()
            .filter(|e| e.visible)
            .map(|e| e.id)
            .collect(),
        LayerFilter::Include(names) => aseprite
            .layers
            .iter()
            .filter(|e| names.contains(&e.id))
            .map(|e| e.id)
            .collect(),
    }
}

/// Convert a front-to-back index into a z-order value.
/// Index 0 (front) gets the highest z so it renders on top.
fn z_from_index(index: usize, total: usize) -> usize {
    total.saturating_sub(1).saturating_sub(index)
}

/// Observer that fires when [`AseTexture`] is added. Spawns children immediately
/// if the asset is already loaded, eliminating the 1-frame lag from the old
/// polling approach.
fn on_ase_texture_added(
    trigger: On<Add, AseTexture>,
    mut cmd: Commands,
    query: Query<(&AseTexture, Has<AseAnimation>, Option<&AseFlip>)>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    let entity = trigger.entity;
    let Ok((tex, has_anim, flip)) = query.get(entity) else {
        return;
    };

    // Sprite parents need Transform + GlobalTransform for world-space rendering.
    // UI parents need UiTransform + UiGlobalTransform for UI layout.
    // insert_if_new preserves any user-supplied values.
    match &tex.render_target {
        RenderTarget::Sprite => {
            cmd.entity(entity)
                .insert_if_new((Transform::default(), GlobalTransform::default()));
        }
        RenderTarget::Ui => {
            cmd.entity(entity)
                .insert_if_new((UiTransform::default(), UiGlobalTransform::default()));
        }
    }

    let Some(aseprite) = assets.get(&tex.aseprite) else {
        return; // Asset not loaded yet; spawn_layers_on_asset_load will handle it.
    };

    spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, has_anim, flip);
}

/// Spawns children for entities whose asset was not yet loaded when the
/// [`on_ase_texture_added`] observer fired. Only runs when an asset finishes
/// loading, not every frame.
fn spawn_layers_on_asset_load(
    mut cmd: Commands,
    mut events: MessageReader<AssetEvent<Aseprite>>,
    query: Query<(Entity, &AseTexture, Has<AseAnimation>, Option<&AseFlip>), Without<SpriteLayers>>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        for (entity, tex, has_anim, flip) in &query {
            if tex.aseprite.id() == *id {
                let Some(aseprite) = assets.get(&tex.aseprite) else {
                    continue;
                };
                spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, has_anim, flip);
            }
        }
    }
}

/// Updates children when [`AseTexture`] changes.
///
/// In layered mode (non-baked): all layer children are always kept alive.
/// Their [`Visibility`] is toggled based on the current filter and z-ordering
/// is updated from the per-entity [`layer_order`](AseTexture::layer_order)
/// override (or asset default).
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
    mut transforms: Query<&mut Transform, With<SpriteLayerOf>>,
    mut z_indices: Query<&mut ZIndex, With<SpriteLayerOf>>,
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
            spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, has_anim, flip);
        } else {
            let all_layers: Vec<LayerId> = aseprite.layer_ids().collect();

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
                spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, has_anim, flip);
            } else {
                // Fast path: toggle visibility and reapply z-ordering.
                let visible = visible_layers(aseprite, &tex.layers);
                let order = tex.layer_order.as_deref().unwrap_or(&all_layers);
                let total = all_layers.len();

                for child in sprite_layers.iter() {
                    if let Ok(id) = layer_ids.get(child) {
                        let vis = if visible.contains(id) {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                        cmd.entity(child).insert(vis);

                        let z = order
                            .iter()
                            .position(|oid| oid == id)
                            .map(|idx| z_from_index(idx, total))
                            .unwrap_or(0);

                        match &tex.render_target {
                            RenderTarget::Sprite => {
                                if let Ok(mut transform) = transforms.get_mut(child) {
                                    transform.translation.z = z as f32 * 0.001;
                                }
                            }
                            RenderTarget::Ui => {
                                if let Ok(mut zi) = z_indices.get_mut(child) {
                                    zi.0 = z as i32;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Propagates [`AseFlip`] to children's [`Sprite`] and [`ImageNode`].
///
/// Also re-derives each sprite child's [`Transform`] translation from the
/// stored [`AppliedOffset`] so the visual anchor stays correct after a flip:
/// when `flip.x` is true, the effective x offset is negated (and likewise for y).
fn propagate_flip(
    parents: Query<(&AseFlip, &SpriteLayers), Changed<AseFlip>>,
    mut sprites: Query<(&mut Sprite, &mut Transform, &AppliedOffset)>,
    mut image_nodes: Query<&mut ImageNode>,
) {
    for (flip, layers) in &parents {
        for child in layers.iter() {
            if let Ok((mut sprite, mut transform, applied)) = sprites.get_mut(child) {
                sprite.flip_x = flip.x;
                sprite.flip_y = flip.y;
                transform.translation.x = if flip.x { -applied.0.x } else { applied.0.x };
                transform.translation.y = if flip.y { -applied.0.y } else { applied.0.y };
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
///
/// In Sprite mode the delta is sign-flipped when [`AseFlip`] is active so that
/// the visual anchor tracks correctly after a flip.
fn propagate_offset(
    parents: Query<(&AseTexture, &SpriteLayers, Option<&AseFlip>), Changed<AseTexture>>,
    mut sprites: Query<(&mut Transform, &mut AppliedOffset)>,
    mut ui_nodes: Query<(&mut Node, &mut AppliedOffset), Without<Transform>>,
) {
    for (tex, layers, flip) in &parents {
        let new_offset = tex.offset;
        let flip_x = flip.map_or(false, |f| f.x);
        let flip_y = flip.map_or(false, |f| f.y);

        for child in layers.iter() {
            match &tex.render_target {
                RenderTarget::Sprite => {
                    if let Ok((mut transform, mut applied)) = sprites.get_mut(child) {
                        let delta = new_offset - applied.0;
                        transform.translation.x += if flip_x { -delta.x } else { delta.x };
                        transform.translation.y += if flip_y { -delta.y } else { delta.y };
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
    assets: &Assets<Aseprite>,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    has_anim: bool,
    flip: Option<&AseFlip>,
) {
    if tex.baked {
        spawn_baked_child(cmd, parent, aseprite, tex, has_anim, flip);
    } else {
        // Spawn ALL layers; visibility is determined by the filter.
        let visible = visible_layers(aseprite, &tex.layers);
        let default_order: Vec<LayerId> = aseprite.layer_ids().collect();
        let order = tex.layer_order.as_deref().unwrap_or(&default_order);
        spawn_layered_children(
            cmd,
            server,
            assets,
            parent,
            aseprite,
            tex,
            has_anim,
            &default_order,
            order,
            &visible,
            flip,
        );
    }
}

fn spawn_baked_child(
    cmd: &mut Commands,
    parent: Entity,
    aseprite: &Aseprite,
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
            let mut sprite = Sprite {
                image: aseprite.atlas_image.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: aseprite.atlas_layout.clone(),
                    index: aseprite.get_atlas_index(0),
                }),
                ..default()
            };
            if let Some(flip) = flip {
                sprite.flip_x = flip.x;
                sprite.flip_y = flip.y;
            }
            let eff_x = flip.map_or(tex.offset.x, |f| if f.x { -tex.offset.x } else { tex.offset.x });
            let eff_y = flip.map_or(tex.offset.y, |f| if f.y { -tex.offset.y } else { tex.offset.y });
            let offset_translation = Vec3::new(eff_x, eff_y, 0.);
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
            let mut node = ImageNode {
                image: aseprite.atlas_image.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: aseprite.atlas_layout.clone(),
                    index: aseprite.get_atlas_index(0),
                }),
                ..default()
            };
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
/// `layers` is the full layer list (determines which entities are created).
/// `order` is the front-to-back ordering used for z-computation (may differ
/// from `layers` when a per-entity `layer_order` override is set).
/// `visible` is the subset that should start with [`Visibility::Inherited`];
/// all others get [`Visibility::Hidden`].
fn spawn_layered_children(
    cmd: &mut Commands,
    server: &AssetServer,
    assets: &Assets<Aseprite>,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    has_anim: bool,
    layers: &[LayerId],
    order: &[LayerId],
    visible: &[LayerId],
    flip: Option<&AseFlip>,
) {
    let total = layers.len();
    for &layer_id in layers.iter() {
        let z = order
            .iter()
            .position(|id| *id == layer_id)
            .map(|idx| z_from_index(idx, total))
            .unwrap_or(0);
        let visibility = if visible.contains(&layer_id) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        let layer_path = format!("{}#{}", aseprite.source_path, layer_id.as_str());
        let layer_handle: Handle<Aseprite> = server.load(&layer_path);

        // Pre-populate with per-layer asset data if available, else fall back
        // to the parent asset (same atlas, different frame indices).
        let layer_ase = assets.get(&layer_handle).unwrap_or(aseprite);

        let common = (
            ChildOf(parent),
            SpriteLayerOf(parent),
            layer_id,
            Name::new(layer_id.as_str().to_owned()),
            visibility,
        );

        match &tex.render_target {
            RenderTarget::Sprite => {
                let mut sprite = Sprite {
                    image: layer_ase.atlas_image.clone(),
                    texture_atlas: Some(TextureAtlas {
                        layout: layer_ase.atlas_layout.clone(),
                        index: layer_ase.get_atlas_index(0),
                    }),
                    ..default()
                };
                if let Some(flip) = flip {
                    sprite.flip_x = flip.x;
                    sprite.flip_y = flip.y;
                }
                let eff_x = flip.map_or(tex.offset.x, |f| if f.x { -tex.offset.x } else { tex.offset.x });
                let eff_y = flip.map_or(tex.offset.y, |f| if f.y { -tex.offset.y } else { tex.offset.y });
                let translation = Vec3::new(eff_x, eff_y, z as f32 * 0.001);
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
                let mut node = ImageNode {
                    image: layer_ase.atlas_image.clone(),
                    texture_atlas: Some(TextureAtlas {
                        layout: layer_ase.atlas_layout.clone(),
                        index: layer_ase.get_atlas_index(0),
                    }),
                    ..default()
                };
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


#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------ //
    // Helpers
    // ------------------------------------------------------------------ //

    fn flip_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(bevy::app::PreUpdate, propagate_flip);
        app
    }

    fn offset_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(bevy::app::PreUpdate, propagate_offset);
        app
    }

    /// Spawn a parent carrying `AseFlip` and a sprite child carrying
    /// `AppliedOffset` + `Transform`. The relationship hook auto-populates
    /// `SpriteLayers` on the parent.
    fn spawn_flip_fixture(
        app: &mut App,
        flip: AseFlip,
        offset: Vec2,
        initial: Vec3,
    ) -> (Entity, Entity) {
        let parent = app.world_mut().spawn(flip).id();
        let child = app
            .world_mut()
            .spawn((
                SpriteLayerOf(parent),
                Sprite::default(),
                Transform::from_translation(initial),
                AppliedOffset(offset),
            ))
            .id();
        (parent, child)
    }

    /// Spawn a parent carrying `AseTexture` (and optionally `AseFlip`) and a
    /// sprite child carrying `AppliedOffset` + `Transform`.
    fn spawn_offset_fixture(
        app: &mut App,
        flip: Option<AseFlip>,
        offset: Vec2,
        applied: Vec2,
        initial: Vec3,
    ) -> (Entity, Entity) {
        let tex = AseTexture::new(Handle::default()).with_offset(offset);
        let parent = match flip {
            Some(f) => app.world_mut().spawn((tex, f)).id(),
            None => app.world_mut().spawn(tex).id(),
        };
        let child = app
            .world_mut()
            .spawn((
                SpriteLayerOf(parent),
                Transform::from_translation(initial),
                AppliedOffset(applied),
            ))
            .id();
        (parent, child)
    }

    fn translation(app: &App, entity: Entity) -> Vec3 {
        app.world().get::<Transform>(entity).unwrap().translation
    }

    // ------------------------------------------------------------------ //
    // propagate_flip
    // ------------------------------------------------------------------ //

    /// When flip.x is true, the child's translation.x should be the negative
    /// of the stored raw offset.
    #[test]
    fn flip_x_negates_offset_x() {
        let mut app = flip_app();
        let (_, child) = spawn_flip_fixture(
            &mut app,
            AseFlip { x: true, y: false },
            Vec2::new(2.0, 3.0),
            Vec3::ZERO,
        );
        app.update();
        assert_eq!(translation(&app, child).x, -2.0);
        assert_eq!(translation(&app, child).y, 3.0);
    }

    /// When flip.y is true, the child's translation.y should be negated.
    #[test]
    fn flip_y_negates_offset_y() {
        let mut app = flip_app();
        let (_, child) = spawn_flip_fixture(
            &mut app,
            AseFlip { x: false, y: true },
            Vec2::new(2.0, 3.0),
            Vec3::ZERO,
        );
        app.update();
        assert_eq!(translation(&app, child).x, 2.0);
        assert_eq!(translation(&app, child).y, -3.0);
    }

    /// Both axes flipped: both components of the translation should be negated.
    #[test]
    fn flip_xy_negates_both() {
        let mut app = flip_app();
        let (_, child) = spawn_flip_fixture(
            &mut app,
            AseFlip { x: true, y: true },
            Vec2::new(2.0, 3.0),
            Vec3::ZERO,
        );
        app.update();
        assert_eq!(translation(&app, child).x, -2.0);
        assert_eq!(translation(&app, child).y, -3.0);
    }

    /// Toggling flip.x after the first frame should update the child's
    /// translation on the next update.
    #[test]
    fn flip_x_toggle_updates_translation() {
        let mut app = flip_app();
        let (parent, child) = spawn_flip_fixture(
            &mut app,
            AseFlip { x: false, y: false },
            Vec2::new(2.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        app.update();
        assert_eq!(translation(&app, child).x, 2.0, "before toggle");

        // Advance the world tick so the mutation lands in a tick the next
        // system run can detect as Changed.
        app.world_mut().increment_change_tick();
        app.world_mut().get_mut::<AseFlip>(parent).unwrap().x = true;
        app.update();
        assert_eq!(translation(&app, child).x, -2.0, "after toggle");
    }

    // ------------------------------------------------------------------ //
    // propagate_offset
    // ------------------------------------------------------------------ //

    /// When flip.x is active and the offset changes, the delta applied to
    /// translation.x must be negated so the anchor stays correct.
    ///
    /// Seeded with applied=2.0 / new offset=4.0 so the first update triggers
    /// a delta of +2.0 which should be applied as -2.0 (flip active).
    #[test]
    fn offset_delta_with_flip_x_is_negated() {
        let mut app = offset_app();
        // flip.x=true, previous applied offset = 2.0 (translation was -2.0).
        // New tex.offset = 4.0 -> delta = 2.0 -> effective dx = -2.0.
        let (_, child) = spawn_offset_fixture(
            &mut app,
            Some(AseFlip { x: true, y: false }),
            Vec2::new(4.0, 0.0), // tex.offset (new)
            Vec2::new(2.0, 0.0), // AppliedOffset (previous)
            Vec3::new(-2.0, 0.0, 0.0),
        );
        app.update();
        assert_eq!(translation(&app, child).x, -4.0);
    }

    /// Without flip the delta is applied with its natural sign.
    #[test]
    fn offset_delta_without_flip_is_positive() {
        let mut app = offset_app();
        let (_, child) = spawn_offset_fixture(
            &mut app,
            None,
            Vec2::new(4.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        app.update();
        assert_eq!(translation(&app, child).x, 4.0);
    }

    /// Zero offset: no translation change regardless of flip state.
    #[test]
    fn zero_offset_leaves_translation_unchanged() {
        let mut app = offset_app();
        let initial = Vec3::new(0.0, 0.0, 5.0);
        let (_, child) = spawn_offset_fixture(
            &mut app,
            Some(AseFlip { x: true, y: true }),
            Vec2::ZERO,
            Vec2::ZERO,
            initial,
        );
        app.update();
        assert_eq!(translation(&app, child).x, 0.0);
        assert_eq!(translation(&app, child).y, 0.0);
        // z must not be touched by either propagation system
        assert_eq!(translation(&app, child).z, 5.0);
    }
}
