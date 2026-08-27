use crate::animation::{AnimationLayer, AseFrame};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::image::TextureAtlas;
use bevy::prelude::*;
use bevy::sprite::TextureSlicer;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use msg_interned_id::InternedId;

/// Controls whether layer children render as world [`Sprite`]s or UI
/// [`ImageNode`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum AseRenderTarget {
    /// Render as world sprites (default). Children get [`Sprite`] + [`Transform`].
    #[default]
    Sprite,
    /// Render as UI nodes. Children get [`ImageNode`] + [`Node`] + [`ZIndex`].
    Ui,
}

pub struct AsepriteLayersPlugin;

impl Plugin for AsepriteLayersPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AseTexture>()
            .register_type::<AseFlip>()
            .register_type::<AseRenderTarget>()
            .register_type::<LayerFilter>()
            .register_type::<LayerId>()
            .register_type::<SliceId>()
            .register_type::<TagId>()
            .register_type::<SpriteLayerOf>()
            .register_type::<SpriteLayers>();
        // `LayerId` gets its reflect impls from the `InternedId` derive, which
        // knows nothing about it also being a component.
        app.register_type_data::<LayerId, bevy::ecs::reflect::ReflectComponent>();
        app.add_observer(on_ase_texture_added);
        app.add_systems(
            PreUpdate,
            (
                sync_layers_with_asset,
                update_layers,
                propagate_flip,
                propagate_offset,
            ),
        );
        // After every path that can spawn children — the asset-load system
        // above and the add observer alike — and before visibility is
        // resolved, so a child never renders a frame on the wrong layer.
        app.add_systems(
            PostUpdate,
            propagate_render_layers
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Type-safe interned animation-tag name. O(1) comparisons, `Copy`.
#[derive(InternedId, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TagId(bevy::ecs::intern::Interned<str>);

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
#[derive(Clone, Debug, Default, PartialEq, Eq, Reflect)]
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
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[relationship(relationship_target = SpriteLayers)]
pub struct SpriteLayerOf(pub Entity);

/// Auto-populated collection of layer entities.
#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
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
/// children are visible; it does not affect which entities exist. Editing the
/// component reaches the children it already has, in either mode, so what a
/// game attached to one survives the edit; children are replaced only when
/// the file's own layer set changes.
///
/// Add [`AseAnimation`](crate::animation::AseAnimation) alongside this
/// component to opt into animation ticking. Without it, children are fully
/// static with zero per-tick overhead.
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
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component)]
#[require(Visibility)]
#[require(InheritedVisibility)]
#[require(ViewVisibility)]
#[require(AseFrame)]
pub struct AseTexture {
    pub aseprite: Handle<Aseprite>,
    pub layers: LayerFilter,
    pub slice: Option<SliceId>,
    pub baked: bool,
    pub render_target: AseRenderTarget,
    /// Offset applied relatively to child render entities' transforms (Sprite)
    /// or node positions (UI).
    pub offset: Vec2,
    /// Per-entity layer order override. When `Some`, layers are z-ordered
    /// according to this list (index 0 = front, renders on top) instead of
    /// the asset's default order. Layers the list leaves out draw behind
    /// every layer it names, in the asset's own order. Set to `None` to use
    /// the asset order.
    pub layer_order: Option<Vec<LayerId>>,
}

impl AseTexture {
    /// Layered mode (default). Spawns one child per layer (all layers).
    #[must_use]
    pub fn new(aseprite: Handle<Aseprite>) -> Self {
        AseTexture {
            aseprite,
            layers: LayerFilter::Visible,
            slice: None,
            baked: false,
            render_target: AseRenderTarget::Sprite,
            offset: default(),
            layer_order: None,
        }
    }

    /// Baked mode. Spawns a single composite child.
    #[must_use]
    pub fn baked(aseprite: Handle<Aseprite>) -> Self {
        AseTexture {
            aseprite,
            layers: LayerFilter::Visible,
            slice: None,
            baked: true,
            render_target: AseRenderTarget::Sprite,
            offset: default(),
            layer_order: None,
        }
    }

    /// Set the slice name. Enables slice-based rendering.
    #[must_use]
    pub fn with_slice(mut self, name: impl Into<SliceId>) -> Self {
        self.slice = Some(name.into());
        self
    }

    /// Set the layer filter.
    #[must_use]
    pub fn with_layers(mut self, layers: LayerFilter) -> Self {
        self.layers = layers;
        self
    }

    /// Set the render target.
    #[must_use]
    pub fn with_render_target(mut self, target: AseRenderTarget) -> Self {
        self.render_target = target;
        self
    }

    /// Use [`Sprite`] as the render target (2D world).
    #[must_use]
    pub fn sprite(mut self) -> Self {
        self.render_target = AseRenderTarget::Sprite;
        self
    }

    /// Use [`ImageNode`] as the render target (UI).
    #[must_use]
    pub fn ui(mut self) -> Self {
        self.render_target = AseRenderTarget::Ui;
        self
    }

    /// Set the offset applied to child render entities.
    #[must_use]
    pub fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    /// Set a per-entity layer order override. Layers are ordered front-to-back
    /// (index 0 = topmost, renders in front). Only affects this entity's
    /// z-ordering, not the underlying asset.
    #[must_use]
    pub fn with_layer_order(mut self, order: Vec<LayerId>) -> Self {
        self.layer_order = Some(order);
        self
    }

    /// Override the z-order for a single layer on this entity.
    ///
    /// Moves the layer to `new_index` (0 = front) in this entity's order
    /// list, seeding that list from `aseprite`'s own front-to-back order the
    /// first time one is needed — so an override starts out identical to the
    /// asset and diverges only by the moves asked for here.
    ///
    /// Mutating `AseTexture` triggers the z-reorder system.
    ///
    /// Returns `false` when the layer is not in the order list.
    pub fn reorder_layer(&mut self, aseprite: &Aseprite, layer: LayerId, new_index: usize) -> bool {
        // Seed only for a layer the asset has, so a move that finds nothing
        // leaves the entity on the asset order instead of pinning it to a
        // snapshot of one.
        if self.layer_order.is_none() {
            if !aseprite.layers.iter().any(|entry| entry.id == layer) {
                return false;
            }
            self.layer_order = Some(aseprite.layer_ids().collect());
        }
        let order = self.layer_order.get_or_insert_default();
        let Some(old) = order.iter().position(|id| *id == layer) else {
            return false;
        };
        let entry = order.remove(old);
        order.insert(new_index.min(order.len()), entry);
        true
    }

    /// Show a layer by adding it to the [`LayerFilter::Include`] list.
    ///
    /// Only `Include` names layers one at a time; [`LayerFilter::All`] and
    /// [`LayerFilter::Visible`] carry no list to add to, so switch the filter
    /// first.
    ///
    /// Mutating `AseTexture` triggers the visibility update system.
    ///
    /// Returns `false` when the filter is not `Include`, so the call had no
    /// effect.
    pub fn show_layer(&mut self, layer: LayerId) -> bool {
        let LayerFilter::Include(ids) = &mut self.layers else {
            return false;
        };
        if !ids.contains(&layer) {
            ids.push(layer);
        }
        true
    }

    /// Hide a layer by removing it from the [`LayerFilter::Include`] list.
    ///
    /// Only `Include` names layers one at a time; [`LayerFilter::All`] and
    /// [`LayerFilter::Visible`] carry no list to remove from, so switch the
    /// filter first.
    ///
    /// Mutating `AseTexture` triggers the visibility update system.
    ///
    /// Returns `false` when the filter is not `Include`, so the call had no
    /// effect.
    pub fn hide_layer(&mut self, layer: LayerId) -> bool {
        let LayerFilter::Include(ids) = &mut self.layers else {
            return false;
        };
        ids.retain(|id| *id != layer);
        true
    }
}

/// Flip state that propagates to all child render entities.
///
/// Place on the parent entity alongside [`AseTexture`].
#[derive(Component, Reflect, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub struct AseFlip {
    pub x: bool,
    pub y: bool,
}

/// Tracks the last applied offset so changes can be applied relatively.
#[derive(Component, Default, Clone, Debug)]
struct AppliedOffset(Vec2);

// ---- systems ----

/// Where a single layer child belongs: its z-order and whether the filter
/// shows it.
///
/// Borrowed rather than collected, and consulted by both the spawn path and
/// the update path, so a layer cannot end up placed differently depending on
/// which of the two last ran.
struct LayerPlan<'a> {
    entries: &'a [LayerEntry],
    /// `None` is the asset's own front-to-back order.
    order: Option<&'a [LayerId]>,
    filter: &'a LayerFilter,
}

impl<'a> LayerPlan<'a> {
    fn new(aseprite: &'a Aseprite, tex: &'a AseTexture) -> Self {
        Self {
            entries: &aseprite.layers,
            order: tex.layer_order.as_deref(),
            filter: &tex.layers,
        }
    }

    /// The depth a layer's child draws at, counting up towards the front, or
    /// `None` for a layer this file does not have.
    ///
    /// Both orders read front to back, index 0 first. A layer an order
    /// override leaves out draws behind every layer it names, holding its
    /// place among the other layers left out.
    fn z(&self, layer: LayerId) -> Option<usize> {
        let asset_index = self.entries.iter().position(|entry| entry.id == layer)?;
        let Some(order) = self.order else {
            return Some(self.entries.len() - 1 - asset_index);
        };
        let left_out = |entry: &&LayerEntry| !order.contains(&entry.id);
        Some(match order.iter().position(|id| *id == layer) {
            Some(index) => {
                let behind = self.entries.iter().filter(left_out).count();
                behind + order.len() - 1 - index
            }
            // Entries run front to back, so the ones after this layer are the
            // ones it draws in front of.
            None => self.entries[asset_index + 1..]
                .iter()
                .filter(left_out)
                .count(),
        })
    }

    fn visibility(&self, layer: LayerId) -> Visibility {
        let shown = match self.filter {
            LayerFilter::All => true,
            LayerFilter::Visible => self
                .entries
                .iter()
                .any(|entry| entry.id == layer && entry.visible),
            LayerFilter::Include(ids) => ids.contains(&layer),
        };
        if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    }
}

/// Layer children share one thousandth of a unit of depth, so the stack keeps
/// its internal order without displacing where the parent as a whole sits.
fn z_depth(z: usize) -> f32 {
    z as f32 * 0.001
}

/// A [`Val`] moved by `delta` logical pixels. A position given in anything
/// but pixels has no pixel origin to move from and stays as its writer left it.
fn moved(value: Val, delta: f32) -> Val {
    match value {
        Val::Px(px) => Val::Px(px + delta),
        other => other,
    }
}

/// An offset as a child must apply it: mirroring an axis mirrors the anchor
/// with it, so that component changes sign.
fn flipped_offset(offset: Vec2, flip: Option<&AseFlip>) -> Vec2 {
    let flip = flip.copied().unwrap_or_default();
    Vec2::new(
        if flip.x { -offset.x } else { offset.x },
        if flip.y { -offset.y } else { offset.y },
    )
}

/// Observer that fires when [`AseTexture`] is added. Spawns children immediately
/// if the asset is already loaded, eliminating the 1-frame lag from the old
/// polling approach.
fn on_ase_texture_added(
    trigger: On<Add, AseTexture>,
    mut cmd: Commands,
    query: Query<(&AseTexture, Option<&AseFlip>)>,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    let entity = trigger.entity;
    let Ok((tex, flip)) = query.get(entity) else {
        return;
    };

    // Sprite parents need Transform + GlobalTransform for world-space rendering.
    // UI parents need UiTransform + UiGlobalTransform for UI layout.
    // insert_if_new preserves any user-supplied values.
    match tex.render_target {
        AseRenderTarget::Sprite => {
            cmd.entity(entity)
                .insert_if_new((Transform::default(), GlobalTransform::default()));
        }
        AseRenderTarget::Ui => {
            cmd.entity(entity)
                .insert_if_new((UiTransform::default(), UiGlobalTransform::default()));
        }
    }

    let Some(aseprite) = assets.get(&tex.aseprite) else {
        return; // Asset not loaded yet; spawn_layers_on_asset_load will handle it.
    };

    spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, flip);
}

/// The children's own components, as the paths that reconcile them write
/// through. Gathered in one place so a system that touches children asks for
/// one parameter rather than five.
#[derive(SystemParam)]
struct ChildViews<'w, 's> {
    layer_ids: Query<'w, 's, &'static LayerId, With<SpriteLayerOf>>,
    transforms: Query<'w, 's, &'static mut Transform, With<SpriteLayerOf>>,
    z_indices: Query<'w, 's, &'static mut ZIndex, With<SpriteLayerOf>>,
    sprites: Query<'w, 's, &'static mut Sprite, With<SpriteLayerOf>>,
    image_nodes: Query<'w, 's, &'static mut ImageNode, With<SpriteLayerOf>>,
    animations: Query<'w, 's, &'static mut AnimationLayer, With<SpriteLayerOf>>,
}

/// Whether the existing children are exactly one per layer of the file.
///
/// A layered texture keeps its children through every change but one: the
/// file itself gaining, losing or renaming a layer.
fn children_match(
    sprite_layers: &SpriteLayers,
    aseprite: &Aseprite,
    layer_ids: &Query<&LayerId, With<SpriteLayerOf>>,
) -> bool {
    sprite_layers.iter().count() == aseprite.layers.len()
        && sprite_layers.iter().all(|child| {
            layer_ids
                .get(child)
                .is_ok_and(|id| aseprite.layers.iter().any(|entry| entry.id == *id))
        })
}

/// Puts every existing layer child where the plan says: the filter decides
/// whether it is visible, the order how deep it sits.
fn apply_plan(
    cmd: &mut Commands,
    tex: &AseTexture,
    aseprite: &Aseprite,
    sprite_layers: &SpriteLayers,
    views: &mut ChildViews,
) {
    let plan = LayerPlan::new(aseprite, tex);
    for child in sprite_layers.iter() {
        let Ok(&id) = views.layer_ids.get(child) else {
            continue;
        };
        cmd.entity(child).insert(plan.visibility(id));

        let z = plan.z(id).unwrap_or_default();
        match tex.render_target {
            AseRenderTarget::Sprite => {
                if let Ok(mut transform) = views.transforms.get_mut(child) {
                    transform.translation.z = z_depth(z);
                }
            }
            AseRenderTarget::Ui => {
                if let Ok(mut zi) = views.z_indices.get_mut(child) {
                    zi.0 = z as i32;
                }
            }
        }
    }
}

/// Re-points a baked texture's composite child at what the component now
/// says, in place, so whatever the game hung off that entity survives.
///
/// Returns `false` when there is nothing to reconcile — no single child, or
/// one drawing through the other render target — and the caller must spawn a
/// fresh one.
fn reconcile_baked(
    cmd: &mut Commands,
    tex: &AseTexture,
    aseprite: &Aseprite,
    sprite_layers: &SpriteLayers,
    views: &mut ChildViews,
) -> bool {
    let [child] = sprite_layers.0[..] else {
        return false;
    };
    let (index, slicer) = initial_atlas(aseprite, tex.slice);
    let atlas = Some(TextureAtlas {
        layout: aseprite.atlas_layout().clone(),
        index,
    });

    match tex.render_target {
        AseRenderTarget::Sprite => {
            let Ok(mut sprite) = views.sprites.get_mut(child) else {
                return false;
            };
            sprite.image = aseprite.atlas_image().clone();
            sprite.texture_atlas = atlas;
            sprite.image_mode = match slicer {
                Some(slicer) => SpriteImageMode::Sliced(slicer),
                None => SpriteImageMode::Auto,
            };
        }
        AseRenderTarget::Ui => {
            let Ok(mut node) = views.image_nodes.get_mut(child) else {
                return false;
            };
            node.image = aseprite.atlas_image().clone();
            node.texture_atlas = atlas;
            node.image_mode = match slicer {
                Some(slicer) => NodeImageMode::Sliced(slicer),
                None => NodeImageMode::Auto,
            };
        }
    }

    if let Ok(mut animation) = views.animations.get_mut(child)
        && animation.aseprite != tex.aseprite
    {
        animation.aseprite = tex.aseprite.clone();
    }
    let mut child = cmd.entity(child);
    match tex.slice {
        Some(name) => {
            child.insert(AseSlice {
                name,
                aseprite: tex.aseprite.clone(),
            });
        }
        None => {
            child.remove::<AseSlice>();
        }
    }
    true
}

/// Keeps children in step with the asset behind them.
///
/// Spawns them for an entity whose asset was still loading when the
/// [`on_ase_texture_added`] observer fired, and, when a reload replaces a file
/// the children already came from, gives them the layer set, order and
/// visibility the new file has. A layer set that changed shape needs a fresh
/// set of children; one that only changed pixels keeps the ones it has.
fn sync_layers_with_asset(
    mut cmd: Commands,
    mut events: MessageReader<AssetEvent<Aseprite>>,
    query: Query<(Entity, &AseTexture, Option<&SpriteLayers>, Option<&AseFlip>)>,
    mut views: ChildViews,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    // One pass per asset, not per event: the children a pass spawns are only
    // visible to the query once the commands are applied, so a file that both
    // loaded and changed this frame would otherwise be given two sets.
    let mut changed: Vec<AssetId<Aseprite>> = Vec::new();
    for event in events.read() {
        let (AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id }) = event
        else {
            continue;
        };
        if !changed.contains(id) {
            changed.push(*id);
        }
    }

    for id in changed {
        for (entity, tex, sprite_layers, flip) in &query {
            if tex.aseprite.id() != id {
                continue;
            }
            let Some(aseprite) = assets.get(&tex.aseprite) else {
                continue;
            };
            let Some(sprite_layers) = sprite_layers else {
                spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, flip);
                continue;
            };
            // A baked texture draws through one composite child whatever the
            // file holds, and the render systems follow the atlas for it.
            if tex.baked {
                continue;
            }
            if children_match(sprite_layers, aseprite, &views.layer_ids) {
                apply_plan(&mut cmd, tex, aseprite, sprite_layers, &mut views);
            } else {
                for child in sprite_layers.iter() {
                    cmd.entity(child).despawn();
                }
                spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, flip);
            }
        }
    }
}

/// Updates children when [`AseTexture`] changes.
///
/// Both modes reconcile the children they have rather than replacing them, so
/// a material, a render layer or a marker the game attached to one survives an
/// edit: layered children have their [`Visibility`] and depth reapplied from
/// the filter and the per-entity
/// [`layer_order`](AseTexture::layer_order) override, and the baked composite
/// child is re-pointed at the atlas rect the component now names.
///
/// Children are replaced only when they cannot be reconciled — the aseprite
/// handle now names a different layer set, or the render target changed.
fn update_layers(
    mut cmd: Commands,
    query: Query<(Entity, &AseTexture, &SpriteLayers, Option<&AseFlip>), Changed<AseTexture>>,
    mut views: ChildViews,
    assets: Res<Assets<Aseprite>>,
    server: Res<AssetServer>,
) {
    for (entity, tex, sprite_layers, flip) in &query {
        let Some(aseprite) = assets.get(&tex.aseprite) else {
            continue;
        };

        let reconciled = if tex.baked {
            reconcile_baked(&mut cmd, tex, aseprite, sprite_layers, &mut views)
        } else {
            let matched = children_match(sprite_layers, aseprite, &views.layer_ids);
            if matched {
                apply_plan(&mut cmd, tex, aseprite, sprite_layers, &mut views);
            }
            matched
        };

        if !reconciled {
            for child in sprite_layers.iter() {
                cmd.entity(child).despawn();
            }
            spawn_children(&mut cmd, &server, &assets, entity, aseprite, tex, flip);
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
                let offset = flipped_offset(applied.0, Some(flip));
                transform.translation.x = offset.x;
                transform.translation.y = offset.y;
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
/// [`Node`] position (UI mode), so whatever else writes those — z-ordering, a
/// layout pass, a tween — keeps its work.
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

        for child in layers.iter() {
            match tex.render_target {
                AseRenderTarget::Sprite => {
                    if let Ok((mut transform, mut applied)) = sprites.get_mut(child) {
                        let delta = flipped_offset(new_offset - applied.0, flip);
                        transform.translation.x += delta.x;
                        transform.translation.y += delta.y;
                        applied.0 = new_offset;
                    }
                }
                AseRenderTarget::Ui => {
                    if let Ok((mut node, mut applied)) = ui_nodes.get_mut(child) {
                        let delta = new_offset - applied.0;
                        node.left = moved(node.left, delta.x);
                        node.top = moved(node.top, delta.y);
                        applied.0 = new_offset;
                    }
                }
            }
        }
    }
}

/// Mirrors an [`AseTexture`] parent's render layers onto the children it draws
/// through.
///
/// The parent renders nothing itself, so without this its children fall to the
/// default layer — a camera filtering to some other layer would draw nothing at
/// all, while the layers the parent was excluded from would draw it anyway.
#[allow(clippy::type_complexity, reason = "a change-filtered Bevy query")]
fn propagate_render_layers(
    mut cmd: Commands,
    parents: Query<
        (&RenderLayers, &SpriteLayers),
        Or<(Changed<RenderLayers>, Changed<SpriteLayers>)>,
    >,
) {
    for (layers, children) in &parents {
        for &child in children.0.iter() {
            cmd.entity(child).insert(layers.clone());
        }
    }
}

// ---- helpers ----

/// Resolve the initial texture-atlas index and optional 9-slice data a newly
/// spawned sprite should render with, from the same `Aseprite` whose handle
/// the child will carry downstream.
///
/// - Without a slice: the aseprite's frame 0 atlas rect.
/// - With a slice: the slice's own atlas rect, plus a [`TextureSlicer`] when
///   the slice carries 9-patch data.
///
/// If a slice name is requested but missing from this aseprite, falls back
/// to frame 0 — `render_slice` will emit its own warning at runtime.
fn initial_atlas(ase: &Aseprite, slice: Option<SliceId>) -> (usize, Option<TextureSlicer>) {
    let Some(meta) = slice.and_then(|id| ase.slice(id)) else {
        return (ase.get_atlas_index(0), None);
    };
    let slicer = meta.border().map(|border| TextureSlicer {
        border,
        ..default()
    });
    (meta.atlas_id, slicer)
}

/// One render child, described without reference to whether it will draw as a
/// [`Sprite`] or an [`ImageNode`].
struct ChildSpec {
    name: Name,
    /// `None` for the baked composite, which stands in for every layer at once.
    layer: Option<LayerId>,
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    index: usize,
    slicer: Option<TextureSlicer>,
    z: usize,
    visibility: Visibility,
    /// The aseprite variant this child animates and slices from: the composite
    /// for a baked child, the layer's own sub-asset for a layered one.
    source: Handle<Aseprite>,
}

/// Spawns one render child of `parent`.
///
/// Baked and layered, sprite and UI all come through here, so the flip, the
/// offset, the slicer and the [`AseSlice`] companion are decided once instead
/// of in four places free to drift apart.
fn spawn_render_child(
    cmd: &mut Commands,
    parent: Entity,
    tex: &AseTexture,
    flip: Option<&AseFlip>,
    spec: ChildSpec,
) {
    let common = (
        ChildOf(parent),
        SpriteLayerOf(parent),
        spec.name,
        spec.visibility,
        AppliedOffset(tex.offset),
        AnimationLayer::new(spec.source.clone()),
    );
    let atlas = Some(TextureAtlas {
        layout: spec.layout,
        index: spec.index,
    });
    let flip = flip.copied().unwrap_or_default();

    let mut child = match tex.render_target {
        AseRenderTarget::Sprite => {
            let mut sprite = Sprite {
                image: spec.image,
                texture_atlas: atlas,
                flip_x: flip.x,
                flip_y: flip.y,
                ..default()
            };
            if let Some(slicer) = spec.slicer {
                sprite.image_mode = SpriteImageMode::Sliced(slicer);
            }
            let offset = flipped_offset(tex.offset, Some(&flip));
            cmd.spawn((
                common,
                sprite,
                Transform::from_translation(offset.extend(z_depth(spec.z))),
            ))
        }
        AseRenderTarget::Ui => {
            let mut node = ImageNode {
                image: spec.image,
                texture_atlas: atlas,
                flip_x: flip.x,
                flip_y: flip.y,
                ..default()
            };
            if let Some(slicer) = spec.slicer {
                node.image_mode = NodeImageMode::Sliced(slicer);
            }
            cmd.spawn((
                common,
                node,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    left: Val::Px(tex.offset.x),
                    top: Val::Px(tex.offset.y),
                    ..default()
                },
                ZIndex(spec.z as i32),
            ))
        }
    };

    if let Some(layer) = spec.layer {
        child.insert(layer);
    }
    if let Some(name) = tex.slice {
        child.insert(AseSlice {
            name,
            aseprite: spec.source,
        });
    }
}

/// Spawns the render children of an [`AseTexture`]: one composite child in
/// baked mode, otherwise one child per layer of the file.
///
/// Every layer gets a child even when the filter hides it, so switching the
/// filter costs a visibility write rather than a respawn.
fn spawn_children(
    cmd: &mut Commands,
    server: &AssetServer,
    assets: &Assets<Aseprite>,
    parent: Entity,
    aseprite: &Aseprite,
    tex: &AseTexture,
    flip: Option<&AseFlip>,
) {
    if tex.baked {
        let (index, slicer) = initial_atlas(aseprite, tex.slice);
        spawn_render_child(
            cmd,
            parent,
            tex,
            flip,
            ChildSpec {
                name: Name::new("baked"),
                layer: None,
                image: aseprite.atlas_image().clone(),
                layout: aseprite.atlas_layout().clone(),
                index,
                slicer,
                z: 0,
                visibility: Visibility::Inherited,
                source: tex.aseprite.clone(),
            },
        );
        return;
    }

    let plan = LayerPlan::new(aseprite, tex);
    let source_path = aseprite.source_path();
    for entry in &aseprite.layers {
        let layer_id = entry.id;
        // An asset assembled by `Aseprite::builder` has no file behind it, so
        // there is no path to hang a `#layer` sub-asset off. Its children draw
        // from the asset itself rather than from a request that never resolves.
        let layer_handle: Handle<Aseprite> = if source_path.is_empty() {
            warn_once!(
                "aseprite asset {:?} has no source path; its layer children draw the whole file",
                tex.aseprite.id(),
            );
            tex.aseprite.clone()
        } else {
            server.load(format!("{source_path}#{}", layer_id.as_str()))
        };

        // Per-layer sub-assets load lazily on first `server.load` request, so
        // they are typically not yet in `Assets<Aseprite>` here. Fall back to
        // the parent composite (parent and per-layer share `atlas_image` and
        // `atlas_layout`; only the frame/slice atlas index differs). The
        // `render_children_animation` / `render_slice` systems reconcile the
        // per-layer index once the sub-asset resolves.
        let layer_ase = assets.get(&layer_handle).unwrap_or(aseprite);
        let (index, slicer) = initial_atlas(layer_ase, tex.slice);

        spawn_render_child(
            cmd,
            parent,
            tex,
            flip,
            ChildSpec {
                name: Name::new(layer_id.as_str()),
                layer: Some(layer_id),
                image: layer_ase.atlas_image().clone(),
                layout: layer_ase.atlas_layout().clone(),
                index,
                slicer,
                z: plan.z(layer_id).unwrap_or_default(),
                visibility: plan.visibility(layer_id),
                source: layer_handle,
            },
        );
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

    // ------------------------------------------------------------------ //
    // propagate_offset, UI
    // ------------------------------------------------------------------ //

    /// Spawn a UI parent carrying `AseTexture` and a node child carrying
    /// `AppliedOffset` + `Node`.
    fn spawn_ui_offset_fixture(
        app: &mut App,
        offset: Vec2,
        applied: Vec2,
        initial: (Val, Val),
    ) -> (Entity, Entity) {
        let parent = app
            .world_mut()
            .spawn(AseTexture::new(Handle::default()).ui().with_offset(offset))
            .id();
        let child = app
            .world_mut()
            .spawn((
                SpriteLayerOf(parent),
                Node {
                    left: initial.0,
                    top: initial.1,
                    ..default()
                },
                AppliedOffset(applied),
            ))
            .id();
        (parent, child)
    }

    fn position(app: &App, entity: Entity) -> (Val, Val) {
        let node = app.world().get::<Node>(entity).unwrap();
        (node.left, node.top)
    }

    /// A layout pass, a tween, anything else writing the child's position
    /// keeps its work: the offset moves the node by what changed.
    #[test]
    fn ui_offset_moves_the_node_by_the_delta() {
        let mut app = offset_app();
        let (_, child) = spawn_ui_offset_fixture(
            &mut app,
            Vec2::new(4.0, 1.0),
            Vec2::new(1.0, 1.0),
            (Val::Px(100.0), Val::Px(50.0)),
        );
        app.update();
        assert_eq!(position(&app, child), (Val::Px(103.0), Val::Px(50.0)));
    }

    /// A position given in anything but pixels has no pixel origin to move
    /// from, so it stays as its writer left it.
    #[test]
    fn a_ui_position_off_pixels_is_left_alone() {
        let mut app = offset_app();
        let (_, child) = spawn_ui_offset_fixture(
            &mut app,
            Vec2::new(4.0, 0.0),
            Vec2::ZERO,
            (Val::Percent(25.0), Val::Auto),
        );
        app.update();
        assert_eq!(position(&app, child), (Val::Percent(25.0), Val::Auto));
    }

    // ------------------------------------------------------------------ //
    // LayerPlan::z
    // ------------------------------------------------------------------ //

    /// A file whose layers are `names`, front to back.
    fn file(names: &[&'static str]) -> Aseprite {
        Aseprite {
            layers: names
                .iter()
                .map(|name| LayerEntry::new(LayerId::new(name), true))
                .collect(),
            ..default()
        }
    }

    #[test]
    fn z_counts_up_towards_the_front_of_the_asset_order() {
        let ase = file(&["front", "middle", "back"]);
        let tex = AseTexture::new(Handle::default());
        let plan = LayerPlan::new(&ase, &tex);
        assert_eq!(plan.z(LayerId::new("front")), Some(2));
        assert_eq!(plan.z(LayerId::new("middle")), Some(1));
        assert_eq!(plan.z(LayerId::new("back")), Some(0));
    }

    #[test]
    fn z_is_none_for_a_layer_the_file_does_not_have() {
        let ase = file(&["front"]);
        let tex = AseTexture::new(Handle::default());
        assert_eq!(LayerPlan::new(&ase, &tex).z(LayerId::new("nope")), None);
    }

    #[test]
    fn an_override_puts_the_layers_it_skips_behind_the_ones_it_names() {
        let ase = file(&["a", "b", "c"]);
        let tex = AseTexture::new(Handle::default()).with_layer_order(vec![LayerId::new("c")]);
        let plan = LayerPlan::new(&ase, &tex);
        assert_eq!(plan.z(LayerId::new("c")), Some(2), "the only named layer");
        assert_eq!(
            plan.z(LayerId::new("a")),
            Some(1),
            "in front of b, as in the file"
        );
        assert_eq!(plan.z(LayerId::new("b")), Some(0));
    }

    /// An override may name layers the file does not have; the ones it does
    /// have keep the order it gives them.
    #[test]
    fn an_override_longer_than_the_file_still_orders_it() {
        let ase = file(&["a", "b"]);
        let tex = AseTexture::new(Handle::default()).with_layer_order(vec![
            LayerId::new("x"),
            LayerId::new("y"),
            LayerId::new("a"),
            LayerId::new("b"),
        ]);
        let plan = LayerPlan::new(&ase, &tex);
        assert!(plan.z(LayerId::new("a")) > plan.z(LayerId::new("b")));
    }
}
