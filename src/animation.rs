use crate::layers::{AseTexture, SpriteLayerOf, TagId};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use aseprite_loader::binary::chunks::tags::AnimationDirection as RawDirection;
use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::component::Mutable,
    image::TextureAtlas,
    prelude::*,
    sprite::Sprite,
    sprite_render::Material2d,
    ui::{UiSystems, widget::ImageNode},
};
use std::{collections::VecDeque, ops::RangeInclusive, time::Duration};

pub struct AsepriteAnimationPlugin;
impl Plugin for AsepriteAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AnimationEvent>();
        app.add_message::<AnimationFrameChanged>();
        app.add_systems(PreUpdate, update_aseprite_animation);
        app.add_systems(
            PreUpdate,
            emit_animation_frame_changed.after(update_aseprite_animation),
        );

        app.add_systems(
            PostUpdate,
            (
                render_children_animation::<ImageNode>.before(UiSystems::Prepare),
                render_children_animation::<Sprite>,
            ),
        );
        app.add_observer(next_frame);

        app.register_type::<AseAnimation>();
        app.register_type::<AnimationDirection>();
        app.register_type::<AnimationEvent>();
        app.register_type::<AnimationFrameChanged>();
        app.register_type::<AnimationFrameCursor>();
        app.register_type::<AnimationLayer>();
        app.register_type::<AnimationRepeat>();
        app.register_type::<AnimationState>();
        app.register_type::<AseFrame>();
        app.register_type::<AseTag>();
        app.register_type::<ManualTick>();
        app.register_type::<NextFrame>();
        app.register_type::<PlayDirection>();
        app.register_type::<TagId>();
    }
}

/// Any component that implements this trait can be used as a render target for
/// aseprite frames. The plugin ships with implementations for [`Sprite`],
/// [`ImageNode`], [`MeshMaterial2d`], and [`MaterialNode`] (plus `MeshMaterial3d`
/// with the `3d` feature).
///
/// Implement this trait on your own material to drive custom shaders from the
/// current [`AseFrame`].
pub trait RenderAnimation {
    /// An extra system parameter used in rendering. Use a tuple if many are required.
    type Extra<'e>;
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, extra: &mut Self::Extra<'_>);
}

impl RenderAnimation for ImageNode {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, _extra: &mut ()) {
        self.image = aseprite.atlas_image().clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout().clone(),
            index: aseprite.get_atlas_index(usize::from(frame)),
        });
    }
}

impl RenderAnimation for Sprite {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, _extra: &mut ()) {
        self.image = aseprite.atlas_image().clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout().clone(),
            index: aseprite.get_atlas_index(usize::from(frame)),
        });
    }
}

impl<M: Material2d + RenderAnimation> RenderAnimation for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, frame, &mut extra.1);
    }
}

impl<M: UiMaterial + RenderAnimation> RenderAnimation for MaterialNode<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, frame, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderAnimation> RenderAnimation for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, frame, &mut extra.1);
    }
}

/// The current frame index into an aseprite asset.
///
/// This is the single source of truth read by renderers ([`Sprite`],
/// [`ImageNode`], [`AseSlice`](crate::slice::AseSlice), custom materials). It is
/// independent of animation: set it manually for static frame selection, or
/// add [`AseAnimation`] to have a driver advance it over time.
///
/// On entities with [`AseTexture`](crate::layers::AseTexture), spawned layer
/// children each carry their own `AseFrame`; the parent's frame is copied to
/// children that lack their own [`AseAnimation`] driver, so per-layer animation
/// works by attaching `AseAnimation` to a specific layer child.
#[derive(Component, Default, Reflect, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct AseFrame(pub u16);

impl AseFrame {
    #[must_use]
    pub fn new(frame: u16) -> Self {
        AseFrame(frame)
    }
}

/// Selects a named aseprite tag for tag-relative frame addressing.
///
/// When present alongside [`AseFrame`], the frame index is interpreted as an
/// **offset into the tag's range** rather than an absolute frame index. The
/// renderer resolves the absolute frame as `tag.range.start + AseFrame.0`,
/// clamped to the tag's range.
///
/// When absent (or when the tag name does not resolve in the asset), the
/// renderer reads [`AseFrame`] as an absolute frame index — existing behavior.
///
/// This enables picking "frame N of tag T" without spawning [`AseAnimation`].
/// Useful for terrain tiles and other static assets where tags label variants
/// rather than animation sequences.
///
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// // First frame of the "Rock" tag — no animation.
/// cmd.spawn((
///     AseTexture::new(server.load("tiles.aseprite")).sprite(),
///     AseTag::new("Rock"),
///     AseFrame::new(0),
/// ));
/// # }
/// ```
///
/// On entities with [`AseTexture`], the parent's `AseTag` propagates to layer
/// children that do not have their own (same pattern as [`AseFrame`]).
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component, Debug, PartialEq)]
pub struct AseTag(pub TagId);

impl AseTag {
    #[must_use]
    pub fn new(name: impl Into<TagId>) -> Self {
        AseTag(name.into())
    }
}

/// Resolve the absolute frame index a renderer should use, given the entity's
/// [`AseFrame`] and optional [`AseTag`].
///
/// - No tag, or tag does not exist in the asset: returns `frame.0` as-is.
/// - Tag exists: returns `tag.range.start + frame.0`, clamped to the tag's range.
pub fn resolve_frame(aseprite: &Aseprite, frame: AseFrame, tag: Option<&AseTag>) -> u16 {
    let Some(tag) = tag else {
        return frame.0;
    };
    let Some(meta) = aseprite.tag(tag.0) else {
        return frame.0;
    };
    let start = *meta.range.start();
    let end = *meta.range.end();
    start.saturating_add(frame.0).min(end)
}

// ---- Components ----

/// The primary animation component. Add alongside [`AseTexture`] to enable
/// animation. The tick logic runs once on the parent entity and frame state
/// is propagated to all child render entities.
///
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// # fn example(mut cmd: Commands, server: Res<AssetServer>) {
/// cmd.spawn((
///     AseTexture::new(server.load("player.aseprite")).sprite(),
///     AseAnimation::tag("walk-right"),
/// ));
/// # }
/// ```
#[derive(Component, Debug, Clone, Reflect)]
#[require(AnimationState)]
#[require(AseFrame)]
#[reflect(Component, Default, Debug)]
pub struct AseAnimation {
    pub tag: Option<TagId>,
    pub speed: f32,
    pub playing: bool,
    /// Override for repeat behavior. `None` uses the aseprite file's tag repeat
    /// count (falling back to loop when no tag or repeat=0). Set via
    /// [`with_repeat`](Self::with_repeat); reset to file default with
    /// [`use_file_repeat`](Self::use_file_repeat).
    pub repeat: Option<AnimationRepeat>,
    /// Overwrite aseprite direction
    pub direction: Option<AnimationDirection>,
    pub queue: VecDeque<(TagId, Option<AnimationRepeat>)>,
    pub hold_relative_frame: bool,
    pub relative_group: u16,
    pub new_relative_group: u16,
    /// Runtime cycle counter. `None` = infinite loop, `Some(n)` = n cycles remaining.
    /// Initialized by the animation system from `repeat` or the file's tag data.
    pub(crate) remaining_cycles: Option<u32>,
    /// Dirty flag: when true the system will re-resolve `remaining_cycles`.
    pub(crate) needs_repeat_init: bool,
}

impl Default for AseAnimation {
    fn default() -> Self {
        Self {
            tag: None,
            speed: 1.0,
            playing: true,
            repeat: None,
            direction: None,
            queue: VecDeque::new(),
            hold_relative_frame: false,
            relative_group: 0,
            new_relative_group: 0,
            remaining_cycles: None,
            needs_repeat_init: true,
        }
    }
}

impl AseAnimation {
    /// Animation from tag.
    #[must_use]
    pub fn tag(tag: impl Into<TagId>) -> Self {
        Self::default().with_tag(tag)
    }

    /// Animation speed multiplier, default is 1.0.
    #[must_use]
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Animation holds relative frame when tag changes, default is false.
    #[must_use]
    pub fn with_relative_frame_hold(mut self, hold_relative_frame: bool) -> Self {
        self.hold_relative_frame = hold_relative_frame;
        self
    }

    /// Animation with tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<TagId>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Overrides how many times the animation plays. Pass
    /// `AnimationRepeat::Loop` for infinite looping or
    /// `AnimationRepeat::Count(n)` to play exactly `n` times.
    /// The override persists across tag changes until cleared with
    /// [`use_file_repeat`](Self::use_file_repeat).
    #[must_use]
    pub fn with_repeat(mut self, repeat: AnimationRepeat) -> Self {
        self.repeat = Some(repeat);
        self.needs_repeat_init = true;
        self
    }

    /// Clears the repeat override so the animation uses the aseprite file's
    /// tag repeat count.
    #[must_use]
    pub fn use_file_repeat(mut self) -> Self {
        self.repeat = None;
        self.needs_repeat_init = true;
        self
    }

    /// Provides an animation direction, overwrites aseprite direction.
    #[must_use]
    pub fn with_direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Chains an animation after the current one is done. Pass `None` for
    /// repeat to use the file's tag repeat, or `Some(repeat)` to override.
    #[must_use]
    pub fn with_then(mut self, tag: impl Into<TagId>, repeat: Option<AnimationRepeat>) -> Self {
        self.queue.push_back((tag.into(), repeat));
        self
    }

    /// Instantly starts playing a new animation using the file's tag repeat
    /// count. Clears any queued animations and any repeat override.
    pub fn play(&mut self, tag: impl Into<TagId>) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = None;
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new animation with an explicit repeat
    /// override. Clears any queued animations.
    pub fn play_with_repeat(&mut self, tag: impl Into<TagId>, repeat: AnimationRepeat) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = Some(repeat);
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new animation starting with same relative frame
    /// only if the new relative group is the same as the previous one.
    /// Uses the file's tag repeat count.
    pub fn play_with_relative_group(&mut self, tag: impl Into<TagId>, new_relative_group: u16) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.new_relative_group = new_relative_group;
        self.repeat = None;
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new looping animation, overriding the file's
    /// repeat count.
    pub fn play_loop(&mut self, tag: impl Into<TagId>) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = Some(AnimationRepeat::Loop);
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly stops the currently playing animation.
    pub fn stop(&mut self) {
        self.playing = false;
        self.tag = None;
        self.repeat = None;
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Pauses the currently playing animation.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Starts the currently set animation.
    pub fn start(&mut self) {
        self.playing = true;
    }

    /// Chains an animation after the current one is done. Pass `None` for
    /// repeat to use the file's tag repeat, or `Some(repeat)` to override.
    pub fn then(&mut self, tag: impl Into<TagId>, repeat: Option<AnimationRepeat>) {
        self.queue.push_back((tag.into(), repeat));
    }

    /// Clears any queued up animations.
    pub fn clear_queue(&mut self) {
        self.queue.clear()
    }

    fn next(&mut self) {
        if let Some((tag, repeat)) = self.queue.pop_front() {
            self.tag = Some(tag);
            self.repeat = repeat;
            self.needs_repeat_init = true;
        }
    }
}

impl From<&str> for AseAnimation {
    fn from(tag: &str) -> Self {
        Self::default().with_tag(tag)
    }
}

/// Internal component placed on child entities spawned by [`AseTexture`].
///
/// Public so advanced users can query layer children, but not intended for
/// direct construction in typical usage. Each child carries its own per-layer
/// asset handle.
///
/// Can also be used standalone with [`AseAnimation`] for custom material
/// rendering without the parent-child model.
#[derive(Component, Default, Reflect, Clone, Debug)]
#[reflect(Component, Default, Debug)]
pub struct AnimationLayer {
    pub aseprite: Handle<Aseprite>,
}

impl AnimationLayer {
    pub fn new(aseprite: Handle<Aseprite>) -> Self {
        AnimationLayer { aseprite }
    }
}

/// Marker component that disables automatic animation ticking.
///
/// When present, the plugin will not advance frames automatically.
/// Use [`NextFrame`] to manually advance frames, or modify
/// [`AnimationState`] directly.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct ManualTick;

/// Tracks per-animation internal state (relative frame within the active tag,
/// elapsed time within the current frame, and ping-pong direction).
///
/// The authoritative "current frame index into the asset" lives on
/// [`AseFrame`], not here. This struct is only meaningful for entities driven
/// by [`AseAnimation`]; reading it on a manually-set frame is a no-op.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct AnimationState {
    pub relative_frame: u16,
    pub elapsed: std::time::Duration,
    pub current_direction: PlayDirection,
}

/// The current playback direction within a ping-pong animation.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub enum PlayDirection {
    #[default]
    Forward,
    Backward,
}

/// Completion signals broadcast by the animation system.
///
/// Read with `MessageReader<AnimationEvent>`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum AnimationEvent {
    Finished(Entity),
    LoopCycleFinished(Entity),
}

/// Emitted whenever an entity's displayed animation frame changes; `frame` is
/// the tag-relative frame now showing.
///
/// [`AnimationEvent`] only signals completions; this message gives per-frame
/// granularity (footstep frames, attack cast points). It fires on every change
/// of [`AnimationState::relative_frame`] — a step in either direction, a wrap
/// at either end (loop, clip restart, ping-pong bounce), or a multi-frame
/// jump — carrying the frame now showing.
///
/// Match with `frame >= N` rather than `frame == N`: a slow tick can advance a
/// clip past `N` in a single message.
///
/// On a tag change — and on the first observation of an entity — only a clean
/// start at `0` is announced. A nonzero frame seen at that point is either
/// genuinely stale (the asset has not loaded yet, so the counter still holds
/// the previous tag's value) or a frame the animation system deliberately
/// kept: an in-range carry-over, or a continuation via
/// [`AseAnimation::hold_relative_frame`] /
/// [`AseAnimation::play_with_relative_group`]. A kept frame is being
/// displayed, but its entry into the new tag is not announced; reporting
/// resumes with the next change. Consequently a `frame >= N` consumer fires
/// one tick late when a tag entry lands exactly on `N`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct AnimationFrameChanged {
    pub entity: Entity,
    pub frame: u16,
}

/// The emitter's per-entity bookkeeping: the last frame seen and the tag it
/// belonged to.
///
/// Inserted lazily by [`emit_animation_frame_changed`]; consumers react to
/// [`AnimationFrameChanged`] rather than reading this. It is public so hosts
/// can reset the detector: remove it after rewriting [`AnimationState`]
/// wholesale and the emitter re-inserts it on next sight, applying the
/// first-observation rule. Keying the tag off [`AseAnimation::tag`] (rather
/// than Bevy change detection) means unrelated writes to the component never
/// spuriously reset it.
#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
pub struct AnimationFrameCursor {
    /// The frame observed on the previous tick, used to detect any change.
    last_frame: u16,
    /// The tag `last_frame` belongs to; a change means a new clip.
    last_tag: Option<TagId>,
}

impl AnimationFrameCursor {
    fn new(frame: u16, tag: Option<TagId>) -> Self {
        Self {
            last_frame: frame,
            last_tag: tag,
        }
    }

    /// Folds one tick's observation into the cursor, returning the frame to
    /// emit when the displayed frame changed.
    ///
    /// Within a tag, any change (a step in either direction or a wrap) is
    /// reported. Across a tag change only a clean `0` is trusted; a nonzero
    /// frame at that point is either stale (asset not yet loaded) or a
    /// deliberately kept carry-over, and is reported later, once the counter
    /// changes again.
    fn advance(&mut self, current: u16, tag: Option<TagId>) -> Option<u16> {
        let tag_changed = self.last_tag != tag;

        if tag_changed {
            self.last_tag = tag;
            self.last_frame = current;
            return (current == 0).then_some(0);
        }

        if current != self.last_frame {
            self.last_frame = current;
            return Some(current);
        }

        None
    }
}

/// Maintains each animated entity's [`AnimationFrameCursor`] and emits
/// [`AnimationFrameChanged`] whenever the displayed frame changes.
///
/// Runs in [`PreUpdate`] after `update_aseprite_animation` (with a sync
/// point in between), so frame advances — including [`NextFrame`]-driven
/// ones — are reported in the same tick they happen. A cursor is inserted
/// lazily on first sight, reporting the initial frame when it is a clean `0`.
/// Within a tag every change is reported; across a tag change only a clean
/// start at 0 is reported (see [`AnimationFrameChanged`] for the kept-frame
/// caveat), so a leftover frame never counts as progress in the new tag.
pub fn emit_animation_frame_changed(
    mut cmd: Commands,
    mut writer: MessageWriter<AnimationFrameChanged>,
    mut q: Query<(
        Entity,
        &AnimationState,
        &AseAnimation,
        Option<&mut AnimationFrameCursor>,
    )>,
) {
    for (entity, state, animation, cursor) in &mut q {
        let current = state.relative_frame;
        let tag = animation.tag;

        // The first frame an entity is seen on is a change from nothing, so it
        // is announced wherever it falls. Testing it against zero announced
        // only animations that happen to open on their first frame, and a
        // reversed one opens on its last.
        let Some(mut cursor) = cursor else {
            cmd.entity(entity)
                .insert(AnimationFrameCursor::new(current, tag));
            writer.write(AnimationFrameChanged {
                entity,
                frame: current,
            });
            continue;
        };

        if let Some(frame) = cursor.advance(current, tag) {
            writer.write(AnimationFrameChanged { entity, frame });
        }
    }
}

/// Playback direction for an animation.
#[derive(Default, Clone, Copy, PartialEq, Eq, Reflect, Debug)]
#[cfg_attr(
    feature = "asset_processing",
    derive(serde::Serialize, serde::Deserialize)
)]
#[reflect(Default, Debug, PartialEq)]
pub enum AnimationDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}

impl From<RawDirection> for AnimationDirection {
    fn from(direction: RawDirection) -> AnimationDirection {
        match direction {
            RawDirection::Forward => AnimationDirection::Forward,
            RawDirection::Reverse => AnimationDirection::Reverse,
            RawDirection::PingPong => AnimationDirection::PingPong,
            RawDirection::PingPongReverse => AnimationDirection::PingPongReverse,
            unknown => {
                warn!("Unhandled aseprite animation direction {unknown:?}, playing forward");
                AnimationDirection::Forward
            }
        }
    }
}

/// How many times an animation should play.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub enum AnimationRepeat {
    /// Play indefinitely.
    #[default]
    Loop,
    /// Play exactly `n` times (1 = play once, 2 = play twice, …).
    /// A value of 0 is treated the same as 1.
    Count(u32),
}

impl From<u16> for AnimationRepeat {
    fn from(value: u16) -> Self {
        match value {
            0 => AnimationRepeat::Loop,
            n => AnimationRepeat::Count(u32::from(n)),
        }
    }
}

// ---- Systems ----

/// Resolves the aseprite handle for tick/frame logic.
/// Parents have AseTexture, standalone entities have AnimationLayer.
fn resolve_handle<'a>(
    tex: Option<&'a AseTexture>,
    layer: Option<&'a AnimationLayer>,
) -> Option<&'a Handle<Aseprite>> {
    tex.map(|t| &t.aseprite)
        .or_else(|| layer.map(|l| &l.aseprite))
}

/// The whole file as a frame range, used whenever no tag narrows playback.
///
/// A frameless asset yields `0..=0`; the frame lookups downstream skip the
/// entity rather than index into nothing.
fn whole_file_range(aseprite: &Aseprite) -> RangeInclusive<u16> {
    let last = aseprite.frame_durations().len().saturating_sub(1);
    0..=u16::try_from(last).unwrap_or(u16::MAX)
}

/// Ticks animation state on entities with [`AseAnimation`].
/// Works for both parent entities (with [`AseTexture`]) and standalone
/// entities (with [`AnimationLayer`], e.g. for custom materials).
#[allow(clippy::type_complexity)]
pub fn update_aseprite_animation(
    mut cmd: Commands,
    mut animations: Query<(
        Entity,
        &mut AseAnimation,
        &mut AnimationState,
        &mut AseFrame,
        Option<&mut AseTag>,
        Option<&AseTexture>,
        Option<&AnimationLayer>,
        Has<ManualTick>,
    )>,
    aseprites: Res<Assets<Aseprite>>,
    time: Res<Time>,
) {
    for (entity, mut animation, mut state, mut frame, mut ase_tag, tex, layer, is_manual) in
        animations.iter_mut()
    {
        let Some(handle) = resolve_handle(tex, layer) else {
            continue;
        };
        let Some(aseprite) = aseprites.get(handle) else {
            continue;
        };

        let tag_meta = animation.tag.and_then(|t| aseprite.tag(t));

        let range = match (tag_meta, animation.tag) {
            (Some(meta), _) => meta.range.clone(),
            (None, Some(tag)) => {
                warn_once!("Animation tag \"{tag}\" not found, playing the whole file");
                whole_file_range(aseprite)
            }
            (None, None) => whole_file_range(aseprite),
        };

        // Mirror the animation's tag into AseTag (when present) so the renderer
        // can resolve tag-relative frames consistently. AseFrame is treated as
        // tag-relative on this entity.
        //
        // `set_if_neq` rather than a compare behind `as_deref_mut`: reaching
        // the tag through `DerefMut` flags the component before anything has
        // looked at it, and every renderer resolving a tag-relative frame
        // watches that flag.
        let has_tag = ase_tag.is_some();
        if let Some(tag) = ase_tag.as_mut() {
            tag.set_if_neq(AseTag(animation.tag.unwrap_or_default()));
        }

        // Working range/index: absolute when no AseTag, relative when AseTag is present.
        let (lo, hi) = if has_tag {
            (0, range.end().saturating_sub(*range.start()))
        } else {
            (*range.start(), *range.end())
        };
        let working_range = lo..=hi;

        // Where playback enters the range, and which way it leaves: a reversed
        // direction opens at the far end walking down, which is the only thing
        // separating `PingPongReverse` from `PingPong`.
        let direction = animation
            .direction
            .unwrap_or_else(|| tag_meta.map_or(AnimationDirection::Forward, |m| m.direction));
        let opens_backward = matches!(
            direction,
            AnimationDirection::Reverse | AnimationDirection::PingPongReverse
        );
        let entry = if opens_backward { hi } else { lo };

        // Resolve remaining_cycles from override or file when needed.
        // remaining_cycles counts how many more times the animation will restart
        // after the current play: Count(1) → 0 remaining, Count(2) → 1 remaining, etc.
        if animation.needs_repeat_init {
            animation.remaining_cycles = match &animation.repeat {
                Some(AnimationRepeat::Loop) => None,
                Some(AnimationRepeat::Count(n)) => Some(n.saturating_sub(1)),
                None => match tag_meta {
                    Some(meta) if meta.repeat > 0 => Some(u32::from(meta.repeat).saturating_sub(1)),
                    _ => None,
                },
            };
            state.current_direction = if opens_backward {
                PlayDirection::Backward
            } else {
                PlayDirection::Forward
            };
            if opens_backward {
                frame.0 = entry;
                state.relative_frame = hi.saturating_sub(lo);
            }
            animation.needs_repeat_init = false;
        }

        if !working_range.contains(&frame.0) {
            if !animation.hold_relative_frame {
                frame.0 = entry;
                state.relative_frame = hi.saturating_sub(lo) * u16::from(opens_backward);
                animation.relative_group = 0;
                animation.new_relative_group = 0;
            } else {
                if animation.new_relative_group != animation.relative_group {
                    animation.relative_group = animation.new_relative_group;
                    frame.0 = entry;
                    state.relative_frame = hi.saturating_sub(lo) * u16::from(opens_backward);
                    state.elapsed = std::time::Duration::ZERO;
                } else {
                    let span = hi.saturating_sub(lo).saturating_add(1);
                    state.relative_frame %= span;
                    frame.0 = lo.saturating_add(state.relative_frame);
                }
            }
        }

        if is_manual {
            continue;
        }

        if !animation.playing {
            continue;
        }

        state.elapsed += std::time::Duration::from_secs_f32(time.delta_secs() * animation.speed);

        // frame_durations is indexed by absolute frame.
        let absolute_frame = if has_tag {
            range.start().saturating_add(frame.0)
        } else {
            frame.0
        };
        let Some(frame_duration) = aseprite.frame_durations().get(usize::from(absolute_frame))
        else {
            continue;
        };

        if state.elapsed > *frame_duration {
            cmd.trigger(NextFrame { entity });
            // A frame the file gives no duration carries nothing into the next
            // one: the remainder is `elapsed % 0`, and `Duration` refuses the
            // `NaN` that produces.
            state.elapsed = if frame_duration.is_zero() {
                Duration::ZERO
            } else {
                Duration::from_secs_f32(state.elapsed.as_secs_f32() % frame_duration.as_secs_f32())
            };
        }
    }
}

/// Trigger this event to manually advance an animation by one frame.
///
/// Used together with [`ManualTick`] for frame-by-frame control.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct NextFrame {
    pub entity: Entity,
}

#[allow(clippy::type_complexity)]
fn next_frame(
    trigger: On<NextFrame>,
    mut events: MessageWriter<AnimationEvent>,
    mut animations: Query<(
        &mut AnimationState,
        &mut AseFrame,
        &mut AseAnimation,
        Has<AseTag>,
        Option<&AseTexture>,
        Option<&AnimationLayer>,
    )>,
    aseprites: Res<Assets<Aseprite>>,
) {
    let entity = trigger.entity;
    let Ok((mut state, mut frame, mut anim, has_tag, tex, layer)) = animations.get_mut(entity)
    else {
        return;
    };

    let Some(handle) = resolve_handle(tex, layer) else {
        return;
    };
    let Some(aseprite) = aseprites.get(handle) else {
        return;
    };

    let (abs_range, direction) = match anim.tag.and_then(|t| aseprite.tag(t)) {
        Some(meta) => {
            let dir = anim.direction.unwrap_or(meta.direction);
            (meta.range.clone(), dir)
        }
        None => {
            let dir = anim.direction.unwrap_or(AnimationDirection::Forward);
            (whole_file_range(aseprite), dir)
        }
    };

    // Range used for incrementing AseFrame: relative (0-based) when AseTag is
    // present, absolute otherwise.
    let range = if has_tag {
        0..=(abs_range.end().saturating_sub(*abs_range.start()))
    } else {
        abs_range.clone()
    };

    // Helper: handle end-of-cycle logic using remaining_cycles.
    // Returns true if the animation should wrap/continue, false if finished.
    let handle_cycle_end = |anim: &mut AseAnimation,
                            events: &mut MessageWriter<AnimationEvent>,
                            entity: Entity|
     -> bool {
        match anim.remaining_cycles {
            None => {
                events.write(AnimationEvent::LoopCycleFinished(entity));
                true
            }
            Some(0) => {
                if anim.queue.is_empty() {
                    events.write(AnimationEvent::Finished(entity));
                } else {
                    anim.next();
                }
                false
            }
            Some(n) => {
                anim.remaining_cycles = Some(n - 1);
                true
            }
        }
    };

    match direction {
        AnimationDirection::Forward => {
            let next = frame.0.saturating_add(1);

            if next > *range.end() {
                if handle_cycle_end(&mut anim, &mut events, entity) {
                    frame.0 = *range.start();
                    state.relative_frame = 0;
                }
            } else {
                frame.0 = next;
                state.relative_frame = state.relative_frame.saturating_add(1);
            }
        }
        AnimationDirection::Reverse => {
            // The cycle ends on the range's own first frame, not on an
            // underflow past zero: a tag whose range does not start at zero
            // would otherwise never reach the test and would walk out of its
            // own range. Wrapping returns to the last frame, which is where
            // reverse playback begins.
            if frame.0 <= *range.start() {
                if handle_cycle_end(&mut anim, &mut events, entity) {
                    frame.0 = *range.end();
                    state.relative_frame = range.end().saturating_sub(*range.start());
                }
            } else {
                frame.0 -= 1;
                state.relative_frame = state.relative_frame.saturating_sub(1);
            }
        }
        AnimationDirection::PingPong | AnimationDirection::PingPongReverse => {
            // The bounce turns around standing on the range's own ends, so both
            // of them are shown; turning a frame early skipped whichever end the
            // walk was heading for. A one-frame range has nowhere to step, so
            // the turn leaves it where it is.
            match state.current_direction {
                PlayDirection::Forward => {
                    if frame.0 >= *range.end() {
                        if handle_cycle_end(&mut anim, &mut events, entity) {
                            state.current_direction = PlayDirection::Backward;
                            frame.0 = range.end().saturating_sub(1).max(*range.start());
                            state.relative_frame = state.relative_frame.saturating_sub(1);
                        }
                    } else {
                        frame.0 = frame.0.saturating_add(1);
                        state.relative_frame = state.relative_frame.saturating_add(1);
                    }
                }
                PlayDirection::Backward => {
                    if frame.0 <= *range.start() {
                        if handle_cycle_end(&mut anim, &mut events, entity) {
                            state.current_direction = PlayDirection::Forward;
                            frame.0 = range.start().saturating_add(1).min(*range.end());
                            state.relative_frame = state.relative_frame.saturating_add(1);
                        }
                    } else {
                        frame.0 = frame.0.saturating_sub(1);
                        state.relative_frame = state.relative_frame.saturating_sub(1);
                    }
                }
            }
        }
    };
}

// ---- Render systems ----

/// Renders frames on any entity carrying [`AnimationLayer`] + the target
/// render component `T`. The frame index is resolved by preferring the entity's
/// own [`AseFrame`] (used for standalone entities and per-layer overrides) and
/// otherwise falling back to the parent's [`AseFrame`] via [`SpriteLayerOf`].
///
/// Whether the frame is being driven by [`AseAnimation`] or set manually is
/// irrelevant — the renderer just reads whichever [`AseFrame`] is in scope.
#[allow(clippy::type_complexity)]
pub fn render_children_animation<T: RenderAnimation + Component<Mutability = Mutable>>(
    mut targets: Query<
        (
            &AnimationLayer,
            Option<&AseFrame>,
            Option<&AseTag>,
            Option<&SpriteLayerOf>,
            &mut T,
        ),
        // Slice-configured children render their named slice region (and 9-patch)
        // via `render_slice`, which owns the same target component. Skip them
        // here so the full-frame renderer doesn't clobber the slice's atlas
        // index and 9-patch image mode.
        Without<AseSlice>,
    >,
    parent_frames: Query<(&AseFrame, Option<&AseTag>)>,
    aseprites: Res<Assets<Aseprite>>,
    mut extra: <T as RenderAnimation>::Extra<'_>,
) {
    for (layer, local_frame, local_tag, parent_ref, mut target) in &mut targets {
        let parent = parent_ref.and_then(|p| parent_frames.get(p.0).ok());
        let frame = local_frame
            .copied()
            .or_else(|| parent.map(|(f, _)| *f))
            .unwrap_or_default();
        let tag = local_tag.or_else(|| parent.and_then(|(_, t)| t));
        let Some(aseprite) = aseprites.get(&layer.aseprite) else {
            continue;
        };
        let absolute = resolve_frame(aseprite, frame, tag);
        target.render_animation(aseprite, absolute, &mut extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::SpriteLayerOf;
    use bevy::image::TextureAtlasLayout;
    use bevy::platform::collections::HashMap;

    /// Minimal in-memory Aseprite asset for tests. The renderer only needs
    /// `frame_indices` populated for `get_atlas_index`; everything else can
    /// be defaulted.
    fn test_aseprite() -> Aseprite {
        Aseprite {
            slices: HashMap::default(),
            tags: HashMap::default(),
            frame_durations: vec![Duration::from_millis(100); 4],
            atlas_layout: Handle::<TextureAtlasLayout>::default(),
            atlas_image: Handle::<Image>::default(),
            frame_indices: vec![0, 1, 2, 3],
            source_path: String::new(),
            layers: vec![],
        }
    }

    /// Test render target: captures the frame index `render_animation` was
    /// invoked with so tests can assert on the frame-resolution result.
    #[derive(Component, Default, Clone, Debug)]
    struct CapturedFrame {
        last: Option<u16>,
        calls: u32,
    }

    impl RenderAnimation for CapturedFrame {
        type Extra<'e> = ();
        fn render_animation(&mut self, _aseprite: &Aseprite, frame: u16, _: &mut ()) {
            self.last = Some(frame);
            self.calls += 1;
        }
    }

    /// Build an app with the render system and a populated Aseprite asset.
    /// Returns the handle pointing at the inserted asset.
    fn render_app() -> (App, Handle<Aseprite>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Aseprite>();
        app.init_asset::<Image>();
        app.init_asset::<TextureAtlasLayout>();
        app.add_systems(Update, render_children_animation::<CapturedFrame>);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<Aseprite>>()
            .add(test_aseprite());
        (app, handle)
    }

    fn last_frame(app: &App, entity: Entity) -> Option<u16> {
        app.world()
            .get::<CapturedFrame>(entity)
            .and_then(|c| c.last)
    }

    // ---------- Require-component wiring ----------

    /// `AseTexture` requires `AseFrame` — spawning one should populate the
    /// frame cursor automatically.
    #[test]
    fn asetexture_requires_aseframe() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Aseprite>();
        let entity = app
            .world_mut()
            .spawn(crate::layers::AseTexture::new(Handle::default()))
            .id();
        assert!(
            app.world().get::<AseFrame>(entity).is_some(),
            "AseFrame should be auto-inserted by AseTexture's require()"
        );
    }

    /// `AseAnimation` requires both `AnimationState` and `AseFrame`.
    #[test]
    fn aseanimation_requires_aseframe() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app.world_mut().spawn(AseAnimation::default()).id();
        assert!(
            app.world().get::<AseFrame>(entity).is_some(),
            "AseFrame should be auto-inserted by AseAnimation's require()"
        );
        assert!(
            app.world().get::<AnimationState>(entity).is_some(),
            "AnimationState should be auto-inserted by AseAnimation's require()"
        );
    }

    // ---------- Frame resolution in render_children_animation ----------

    /// Standalone entity with its own AseFrame: renderer reads it directly.
    #[test]
    fn render_uses_local_frame() {
        let (mut app, handle) = render_app();
        let entity = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                AseFrame::new(3),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        assert_eq!(last_frame(&app, entity), Some(3));
    }

    /// Child without its own AseFrame falls back to the parent's via
    /// SpriteLayerOf.
    #[test]
    fn render_falls_back_to_parent_frame() {
        let (mut app, handle) = render_app();
        let parent = app.world_mut().spawn(AseFrame::new(7)).id();
        let child = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                SpriteLayerOf(parent),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        assert_eq!(last_frame(&app, child), Some(7));
    }

    /// A slice-configured child carries both `AnimationLayer` (for handle
    /// resolution) and `AseSlice` (rendered by `render_slice`). The frame
    /// renderer must skip it so it doesn't clobber the slice's atlas index /
    /// 9-patch. Regression test for map-screen UI losing slice/9-patch data.
    #[test]
    fn render_skips_slice_targets() {
        let (mut app, handle) = render_app();
        let entity = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle.clone()),
                AseFrame::new(2),
                AseSlice::new(handle, "panel"),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<CapturedFrame>(entity).map(|c| c.calls),
            Some(0),
            "frame renderer must not touch slice-rendered entities"
        );
    }

    /// A child with its own AseFrame overrides the parent's — this is the
    /// per-layer-frame story.
    #[test]
    fn render_prefers_local_over_parent() {
        let (mut app, handle) = render_app();
        let parent = app.world_mut().spawn(AseFrame::new(7)).id();
        let child = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                SpriteLayerOf(parent),
                AseFrame::new(2),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        assert_eq!(last_frame(&app, child), Some(2));
    }

    /// No AseFrame anywhere in scope: renderer falls through to the default
    /// (frame 0) rather than panicking or skipping the entity.
    #[test]
    fn render_defaults_to_frame_zero_when_no_source() {
        let (mut app, handle) = render_app();
        let entity = app
            .world_mut()
            .spawn((AnimationLayer::new(handle), CapturedFrame::default()))
            .id();
        app.update();
        assert_eq!(last_frame(&app, entity), Some(0));
    }

    // ---------- AseTag / resolve_frame ----------

    /// Build a test aseprite with a single tag spanning frames 2..=3.
    fn tagged_aseprite() -> Aseprite {
        use crate::loader::TagMeta;
        let mut ase = test_aseprite();
        ase.tags.insert(
            TagId::new("Rock"),
            TagMeta {
                direction: AnimationDirection::Forward,
                range: 2..=3,
                repeat: 0,
            },
        );
        ase
    }

    fn tagged_render_app() -> (App, Handle<Aseprite>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Aseprite>();
        app.init_asset::<Image>();
        app.init_asset::<TextureAtlasLayout>();
        app.add_systems(Update, render_children_animation::<CapturedFrame>);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<Aseprite>>()
            .add(tagged_aseprite());
        (app, handle)
    }

    /// `resolve_frame` adds the tag's range start to AseFrame.
    #[test]
    fn resolve_frame_offsets_into_tag_range() {
        let ase = tagged_aseprite();
        let tag = AseTag::new("Rock");
        assert_eq!(resolve_frame(&ase, AseFrame::new(0), Some(&tag)), 2);
        assert_eq!(resolve_frame(&ase, AseFrame::new(1), Some(&tag)), 3);
    }

    /// Out-of-range relative frames are clamped to the tag's last frame.
    #[test]
    fn resolve_frame_clamps_past_tag_end() {
        let ase = tagged_aseprite();
        let tag = AseTag::new("Rock");
        assert_eq!(resolve_frame(&ase, AseFrame::new(5), Some(&tag)), 3);
    }

    /// Without an AseTag, AseFrame is absolute.
    #[test]
    fn resolve_frame_passthrough_without_tag() {
        let ase = tagged_aseprite();
        assert_eq!(resolve_frame(&ase, AseFrame::new(2), None), 2);
    }

    /// Unknown tag name falls back to absolute AseFrame.
    #[test]
    fn resolve_frame_unknown_tag_passes_through() {
        let ase = tagged_aseprite();
        let tag = AseTag::new("Missing");
        assert_eq!(resolve_frame(&ase, AseFrame::new(2), Some(&tag)), 2);
    }

    /// Renderer applies AseTag offset locally on the entity.
    #[test]
    fn render_uses_local_tag() {
        let (mut app, handle) = tagged_render_app();
        let entity = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                AseTag::new("Rock"),
                AseFrame::new(1),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        // Rock spans 2..=3, offset 1 -> absolute frame 3.
        assert_eq!(last_frame(&app, entity), Some(3));
    }

    /// Parent's AseTag propagates to children that don't have their own.
    #[test]
    fn render_inherits_parent_tag() {
        let (mut app, handle) = tagged_render_app();
        let parent = app
            .world_mut()
            .spawn((AseFrame::new(0), AseTag::new("Rock")))
            .id();
        let child = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                SpriteLayerOf(parent),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        // Rock starts at 2, offset 0 -> absolute frame 2.
        assert_eq!(last_frame(&app, child), Some(2));
    }

    // ---------- Tag names are interned ids ----------

    /// Every constructor that names a tag interns it, so a `&str` at the call
    /// site and a `TagId` held elsewhere address the same animation.
    #[test]
    fn tag_names_intern_at_every_constructor() {
        let walk = TagId::new("walk");

        assert_eq!(AseTag::new("walk"), AseTag(walk));
        assert_eq!(AseAnimation::tag("walk").tag, Some(walk));
        assert_eq!(AseAnimation::from("walk").tag, Some(walk));
        assert_eq!(AseAnimation::default().with_tag(walk).tag, Some(walk));

        let queued = AseAnimation::tag("walk")
            .with_then("attack", None)
            .with_then(TagId::new("idle"), Some(AnimationRepeat::Count(2)));
        assert_eq!(
            queued.queue.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(),
            vec![TagId::new("attack"), TagId::new("idle")],
        );
    }

    /// The runtime setters intern too, and dequeuing hands the id straight to
    /// the active tag.
    #[test]
    fn playing_and_queueing_carry_the_id() {
        let mut animation = AseAnimation::default();

        animation.play("walk");
        assert_eq!(animation.tag, Some(TagId::new("walk")));

        animation.play_loop(TagId::new("run"));
        assert_eq!(animation.tag, Some(TagId::new("run")));

        animation.then("attack", Some(AnimationRepeat::Count(1)));
        animation.next();
        assert_eq!(animation.tag, Some(TagId::new("attack")));
        assert_eq!(animation.repeat, Some(AnimationRepeat::Count(1)));
    }

    /// The tick system mirrors the animation's tag onto `AseTag`, which is
    /// what lets a slice child resolve tag-relative frames.
    #[test]
    fn the_ticked_tag_is_mirrored_onto_ase_tag() {
        let (mut app, handle) = plugin_app(tagged_aseprite(), 120);
        let entity = app
            .world_mut()
            .spawn((
                AseAnimation::tag("Rock"),
                AseTag::new("stale"),
                AnimationLayer::new(handle),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<AseTag>(entity).map(|tag| tag.0),
            Some(TagId::new("Rock")),
        );
    }

    /// A frame the file gives no duration is stepped past, not divided by.
    ///
    /// The remainder carried into the next frame is `elapsed % duration`, which
    /// for a zero duration is `NaN` — and `Duration::from_secs_f32` panics on
    /// that, taking the game down over a frame an artist left at zero.
    #[test]
    fn a_zero_duration_frame_steps_instead_of_panicking() {
        let mut ase = test_aseprite();
        ase.frame_durations = vec![Duration::ZERO; 4];
        let (mut app, handle) = plugin_app(ase, 16);
        let entity = app
            .world_mut()
            .spawn((AseAnimation::default(), AnimationLayer::new(handle)))
            .id();

        let frames: Vec<u16> = (0..4)
            .map(|_| {
                app.update();
                app.world().get::<AseFrame>(entity).expect("the frame").0
            })
            .collect();

        // Nothing holds a frame with no duration, so each tick moves one on.
        assert_eq!(frames, vec![0, 1, 2, 3]);
    }

    /// The mirror leaves `AseTag` alone once it already names the playing tag.
    ///
    /// Every renderer that resolves a tag-relative frame watches this component
    /// for change, so flagging it on a tick that wrote nothing re-renders every
    /// animated entity in the world for as long as it plays.
    #[test]
    fn the_mirror_does_not_touch_a_tag_that_already_matches() {
        #[derive(Resource, Default)]
        struct Flagged(usize);

        fn count_flagged(mut flagged: ResMut<Flagged>, tags: Query<(), Changed<AseTag>>) {
            flagged.0 += tags.iter().count();
        }

        let (mut app, handle) = plugin_app(tagged_aseprite(), 120);
        app.init_resource::<Flagged>();
        app.add_systems(Last, count_flagged);
        app.world_mut().spawn((
            AseAnimation::tag("Rock"),
            AseTag::new("stale"),
            AnimationLayer::new(handle),
        ));

        // The first tick writes the tag the animation actually plays, so one
        // report is the spawn plus that correction.
        app.update();
        let after_first = app.world().resource::<Flagged>().0;

        // Every tick after it agrees with the animation and has nothing to say.
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Flagged>().0,
            after_first,
            "the mirror flagged AseTag on a tick that wrote nothing",
        );
    }

    /// A tag looked up by the id an animation carries finds the same metadata
    /// the name does — the whole point of keying the map by `TagId`.
    #[test]
    fn a_tag_resolves_from_the_id_the_animation_holds() {
        let aseprite = tagged_aseprite();
        let animation = AseAnimation::tag("Rock");
        let tag = animation.tag.expect("the tag that went in");

        assert_eq!(
            aseprite.tag(tag).map(|meta| meta.range.clone()),
            Some(2..=3)
        );
        assert_eq!(
            resolve_frame(&aseprite, AseFrame::new(1), Some(&AseTag(tag))),
            3,
        );
    }

    // ---------- AnimationFrameChanged ----------

    /// A forward step within one tag reports the new frame.
    #[test]
    fn frame_change_forward_progress_reports_frame() {
        let mut cursor = AnimationFrameCursor::new(0, Some("attack".into()));

        assert_eq!(cursor.advance(1, Some("attack".into())), Some(1));
        assert_eq!(cursor.advance(2, Some("attack".into())), Some(2));
    }

    /// Staying on the same frame reports nothing.
    #[test]
    fn frame_change_same_frame_reports_nothing() {
        let mut cursor = AnimationFrameCursor::new(4, Some("attack".into()));
        assert_eq!(cursor.advance(4, Some("attack".into())), None);
    }

    /// A tick that jumps several frames reports the frame now showing, so a
    /// consumer keyed inside the jump still sees `frame >= target` and cannot
    /// be skipped.
    #[test]
    fn frame_change_jump_reports_new_frame() {
        let mut cursor = AnimationFrameCursor::new(6, Some("attack".into()));
        // A hitch advances the clip 6 -> 13 in one tick; frame 11 sat inside the jump.
        assert_eq!(cursor.advance(13, Some("attack".into())), Some(13));
    }

    /// A wrap back to 0 within the same tag (a loop's last-frame-to-0) is a
    /// real change and is reported.
    #[test]
    fn frame_change_wrap_to_zero_is_reported() {
        let mut cursor = AnimationFrameCursor::new(15, Some("attack".into()));
        assert_eq!(cursor.advance(0, Some("attack".into())), Some(0));
    }

    /// Entering a new tag cleanly at frame 0 is reported.
    #[test]
    fn frame_change_tag_entry_at_zero_is_reported() {
        let mut cursor = AnimationFrameCursor::new(15, Some("attack".into()));
        assert_eq!(cursor.advance(0, Some("windup".into())), Some(0));
        // The untagged full-clip case counts as its own "tag" too.
        let mut cursor = AnimationFrameCursor::new(3, Some("attack".into()));
        assert_eq!(cursor.advance(0, None), Some(0));
    }

    /// A tag change whose frame counter has not reset yet is suppressed (the
    /// stale tick), then the real reset to 0 is reported and progress accrues
    /// normally.
    #[test]
    fn frame_change_stale_tag_change_is_suppressed_until_reset() {
        let mut cursor = AnimationFrameCursor::new(2, Some("idle".into()));
        // Tag has flipped to attack but the counter still shows a leftover frame: suppressed.
        assert_eq!(cursor.advance(9, Some("attack".into())), None);
        // A frozen leftover across another tick still reports nothing.
        assert_eq!(cursor.advance(9, Some("attack".into())), None);
        // The counter finally resets to 0: reported as a normal change.
        assert_eq!(cursor.advance(0, Some("attack".into())), Some(0));
        assert_eq!(cursor.advance(7, Some("attack".into())), Some(7));
    }

    /// End-to-end through the emitter system: cursor is inserted lazily, frame
    /// advances are reported, and a tag switch suppresses the stale frame.
    #[test]
    fn frame_change_system_emits_and_rejects_stale() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<AnimationFrameChanged>();
        app.add_systems(Update, emit_animation_frame_changed);

        let entity = app
            .world_mut()
            .spawn((AseAnimation::tag("walk"), AnimationState::default()))
            .id();

        let drain = |app: &mut App| -> Vec<u16> {
            app.world_mut()
                .resource_mut::<Messages<AnimationFrameChanged>>()
                .drain()
                .map(|m| {
                    assert_eq!(m.entity, entity);
                    m.frame
                })
                .collect()
        };

        // First sight inserts the cursor and reports the clean initial frame.
        app.update();
        assert_eq!(drain(&mut app), vec![0]);

        // A forward step is reported.
        app.world_mut()
            .get_mut::<AnimationState>(entity)
            .unwrap()
            .relative_frame = 1;
        app.update();
        assert_eq!(drain(&mut app), vec![1]);

        // Tag switch with a stale leftover frame: suppressed.
        app.world_mut()
            .get_mut::<AseAnimation>(entity)
            .unwrap()
            .play("attack");
        app.world_mut()
            .get_mut::<AnimationState>(entity)
            .unwrap()
            .relative_frame = 5;
        app.update();
        assert_eq!(drain(&mut app), Vec::<u16>::new());

        // The counter resets to 0: reported, and progress accrues again.
        app.world_mut()
            .get_mut::<AnimationState>(entity)
            .unwrap()
            .relative_frame = 0;
        app.update();
        assert_eq!(drain(&mut app), vec![0]);
        app.world_mut()
            .get_mut::<AnimationState>(entity)
            .unwrap()
            .relative_frame = 1;
        app.update();
        assert_eq!(drain(&mut app), vec![1]);
    }

    /// A tag switch that lands cleanly on 0 is reported immediately.
    #[test]
    fn frame_change_system_reports_clean_tag_entry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<AnimationFrameChanged>();
        app.add_systems(Update, emit_animation_frame_changed);

        let entity = app
            .world_mut()
            .spawn((AseAnimation::tag("walk"), AnimationState::default()))
            .id();
        app.update(); // insert cursor (reports the initial frame 0)
        app.world_mut()
            .resource_mut::<Messages<AnimationFrameChanged>>()
            .clear();

        app.world_mut()
            .get_mut::<AseAnimation>(entity)
            .unwrap()
            .play("attack");
        app.update();

        let frames: Vec<u16> = app
            .world_mut()
            .resource_mut::<Messages<AnimationFrameChanged>>()
            .drain()
            .map(|m| m.frame)
            .collect();
        assert_eq!(frames, vec![0]);
    }

    /// First observation follows the clean-entry rule: frame 0 is reported
    /// (so consumers keyed on frame 0 don't miss the first cycle), a mid-clip
    /// frame is suppressed until it changes.
    #[test]
    fn frame_change_reports_whatever_frame_an_entity_opens_on() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<AnimationFrameChanged>();
        app.add_systems(Update, emit_animation_frame_changed);

        let clean = app
            .world_mut()
            .spawn((AseAnimation::tag("walk"), AnimationState::default()))
            .id();
        let midway = app
            .world_mut()
            .spawn((
                AseAnimation::tag("walk"),
                AnimationState {
                    relative_frame: 5,
                    ..Default::default()
                },
            ))
            .id();

        let drain = |app: &mut App| -> Vec<(Entity, u16)> {
            app.world_mut()
                .resource_mut::<Messages<AnimationFrameChanged>>()
                .drain()
                .map(|m| (m.entity, m.frame))
                .collect()
        };

        // Both are announced: a first observation is a change from nothing, and
        // an animation need not open on frame zero — a reversed one opens on
        // its last frame, and a held relative frame resumes mid-clip.
        app.update();
        assert_eq!(drain(&mut app), vec![(clean, 0), (midway, 5)]);

        // And each reports again once its frame moves.
        app.world_mut()
            .get_mut::<AnimationState>(midway)
            .unwrap()
            .relative_frame = 6;
        app.update();
        assert_eq!(drain(&mut app), vec![(midway, 6)]);
    }

    // ---------- Full plugin schedule (tick -> next_frame -> emitter) ----------

    /// Build an app running the real [`AsepriteAnimationPlugin`] schedule with
    /// a fixed time step per update and a populated Aseprite asset.
    fn plugin_app(ase: Aseprite, step_ms: u64) -> (App, Handle<Aseprite>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_asset::<Aseprite>();
        app.init_asset::<Image>();
        app.init_asset::<TextureAtlasLayout>();
        app.add_plugins(AsepriteAnimationPlugin);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(step_ms),
        ));
        let handle = app.world_mut().resource_mut::<Assets<Aseprite>>().add(ase);
        (app, handle)
    }

    /// Run one update and drain the frame-change messages for `entity`.
    fn step_and_drain(app: &mut App, entity: Entity) -> Vec<u16> {
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AnimationFrameChanged>>()
            .drain()
            .map(|m| {
                assert_eq!(m.entity, entity);
                m.frame
            })
            .collect()
    }

    /// Reverse playback steps the relative frame down every tick, and each
    /// step is reported in the same `update()` it happens — through the real
    /// plugin schedule (tick system, `NextFrameEvent` observer, emitter).
    #[test]
    fn reverse_playback_reports_every_frame() {
        // 4 frames of 100ms each, ticked 120ms per update: one advance per
        // update after the zero-delta first one.
        let (mut app, handle) = plugin_app(test_aseprite(), 120);
        let entity = app
            .world_mut()
            .spawn((
                AseAnimation::default().with_direction(AnimationDirection::Reverse),
                AnimationLayer::new(handle),
            ))
            .id();

        let mut frames = Vec::new();
        for _ in 0..5 {
            frames.extend(step_and_drain(&mut app, entity));
        }

        // Reverse opens on the range's last frame, 3, and walks down; the wrap
        // returns there. The first update has a zero delta and reports only
        // that opening frame.
        assert_eq!(frames, vec![3, 2, 1, 0, 3]);
        let state = app.world().get::<AnimationState>(entity).unwrap();
        assert_eq!(state.relative_frame, 3);
        assert_eq!(app.world().get::<AseFrame>(entity).unwrap().0, 3);
    }

    /// Ping-pong playback on a tagged animation without an `AseTag` (absolute
    /// frame addressing) bounces between the tag's bounds while the relative
    /// frame stays in sync with the absolute one, and every step is reported.
    #[test]
    fn pingpong_playback_reports_every_frame() {
        use crate::loader::TagMeta;
        let mut ase = test_aseprite();
        ase.frame_durations = vec![Duration::from_millis(100); 8];
        ase.frame_indices = vec![0, 1, 2, 3, 4, 5, 6, 7];
        ase.tags.insert(
            TagId::new("walk"),
            TagMeta {
                direction: AnimationDirection::Forward,
                range: 2..=7,
                repeat: 0,
            },
        );

        let (mut app, handle) = plugin_app(ase, 120);
        let entity = app
            .world_mut()
            .spawn((
                AseAnimation::tag("walk").with_direction(AnimationDirection::PingPong),
                AnimationLayer::new(handle),
            ))
            .id();

        let mut frames = Vec::new();
        for _ in 0..10 {
            frames.extend(step_and_drain(&mut app, entity));
            // Without an AseTag, AseFrame is absolute: always the tag's range
            // start plus the relative frame.
            let state = app.world().get::<AnimationState>(entity).unwrap();
            let frame = app.world().get::<AseFrame>(entity).unwrap();
            assert_eq!(frame.0, state.relative_frame + 2);
        }

        // The first update has a zero delta: it enters the tag and reports the
        // clean initial frame 0. Every later update ticks once — up to the
        // range's last frame, 5, then back down. The bounce stands on the end
        // rather than turning before it.
        assert_eq!(frames, vec![0, 1, 2, 3, 4, 5, 4, 3, 2, 1]);
    }

    /// A child's own AseTag overrides the parent's.
    #[test]
    fn render_local_tag_overrides_parent() {
        let (mut app, handle) = tagged_render_app();
        // Parent says no tag (absolute mode); child overrides with Rock.
        let parent = app.world_mut().spawn(AseFrame::new(0)).id();
        let child = app
            .world_mut()
            .spawn((
                AnimationLayer::new(handle),
                SpriteLayerOf(parent),
                AseTag::new("Rock"),
                AseFrame::new(1),
                CapturedFrame::default(),
            ))
            .id();
        app.update();
        assert_eq!(last_frame(&app, child), Some(3));
    }

    // ---------- Totality: missing tags, empty assets, unknown directions ----------

    /// A tag the file does not define plays the whole file instead of taking
    /// the app down. The CHANGELOG promised this; the tick system now honours
    /// it the same way `next_frame` always has.
    #[test]
    fn missing_tag_falls_back_to_the_whole_file() {
        let (mut app, handle) = plugin_app(test_aseprite(), 120);
        let entity = app
            .world_mut()
            .spawn((AseAnimation::tag("wlak"), AnimationLayer::new(handle)))
            .id();

        let mut seen = Vec::new();
        for _ in 0..5 {
            app.update();
            seen.push(app.world().get::<AseFrame>(entity).expect("frame").0);
        }

        assert_eq!(
            seen,
            vec![0, 1, 2, 3, 0],
            "typo'd tag should play frames 0..=3"
        );
    }

    /// An asset with no frames at all — `Aseprite::default()`, which every
    /// `Handle::default()` resolves to — must tick without panicking, in every
    /// direction and with or without a tag.
    #[test]
    fn zero_frame_asset_never_panics() {
        for direction in [
            AnimationDirection::Forward,
            AnimationDirection::Reverse,
            AnimationDirection::PingPong,
            AnimationDirection::PingPongReverse,
        ] {
            let (mut app, handle) = plugin_app(Aseprite::default(), 120);
            let untagged = app
                .world_mut()
                .spawn((
                    AseAnimation::default().with_direction(direction),
                    AnimationLayer::new(handle.clone()),
                ))
                .id();
            let tagged = app
                .world_mut()
                .spawn((
                    AseAnimation::tag("nothing").with_direction(direction),
                    AseTag::new("nothing"),
                    AnimationLayer::new(handle.clone()),
                ))
                .id();

            for _ in 0..4 {
                app.update();
            }

            assert_eq!(app.world().get::<AseFrame>(untagged).unwrap().0, 0);
            assert_eq!(app.world().get::<AseFrame>(tagged).unwrap().0, 0);
        }
    }

    /// A tag one or two frames long leaves ping-pong nothing to bounce
    /// between; the wrap must clamp rather than underflow.
    #[test]
    fn short_tag_pingpong_never_panics() {
        use crate::loader::TagMeta;

        for (name, start, end) in [("solo", 0u16, 0u16), ("pair", 5u16, 6u16)] {
            let mut ase = test_aseprite();
            ase.frame_durations = vec![Duration::from_millis(100); 10];
            ase.frame_indices = (0..10).collect();
            ase.tags.insert(
                TagId::new(name),
                TagMeta {
                    direction: AnimationDirection::PingPong,
                    range: start..=end,
                    repeat: 0,
                },
            );

            let (mut app, handle) = plugin_app(ase, 120);
            app.world_mut()
                .spawn((AseAnimation::tag(name), AnimationLayer::new(handle)));
            for _ in 0..4 {
                app.update();
            }
        }
    }

    /// A frame index the asset cannot resolve — a tag reaching past the end of
    /// the file — skips that entity only. Both entities share an archetype, so
    /// the broken one is visited first.
    #[test]
    fn unresolvable_frame_skips_only_its_own_entity() {
        use crate::loader::TagMeta;

        let mut ase = test_aseprite();
        ase.tags.insert(
            TagId::new("overrun"),
            TagMeta {
                direction: AnimationDirection::Forward,
                range: 2..=9,
                repeat: 0,
            },
        );
        ase.tags.insert(
            TagId::new("walk"),
            TagMeta {
                direction: AnimationDirection::Forward,
                range: 0..=3,
                repeat: 0,
            },
        );

        let (mut app, handle) = plugin_app(ase, 120);
        // Frame 3 of "overrun" is absolute frame 5, past the asset's 4 frames.
        app.world_mut().spawn((
            AseAnimation::tag("overrun"),
            AseTag::new("overrun"),
            AseFrame::new(3),
            AnimationLayer::new(handle.clone()),
        ));
        let healthy = app
            .world_mut()
            .spawn((
                AseAnimation::tag("walk"),
                AseTag::new("walk"),
                AseFrame::new(0),
                AnimationLayer::new(handle),
            ))
            .id();

        for _ in 0..3 {
            app.update();
        }

        assert_eq!(
            app.world().get::<AseFrame>(healthy).unwrap().0,
            2,
            "an unresolvable frame on an earlier entity must not stall later ones"
        );
    }

    /// Directions the aseprite format gains later read as forward playback
    /// rather than aborting the conversion.
    #[test]
    fn unknown_raw_direction_reads_as_forward() {
        assert_eq!(
            AnimationDirection::from(RawDirection::Unknown(42)),
            AnimationDirection::Forward
        );
        assert_eq!(
            AnimationDirection::from(RawDirection::PingPongReverse),
            AnimationDirection::PingPongReverse
        );
    }

    /// `NextFrame` targets an entity, so a `ManualTick` animation advances only
    /// when its own event is triggered.
    #[test]
    fn next_frame_advances_the_targeted_entity_only() {
        let (mut app, handle) = plugin_app(test_aseprite(), 120);
        let driven = app
            .world_mut()
            .spawn((
                AseAnimation::default(),
                ManualTick,
                AnimationLayer::new(handle.clone()),
            ))
            .id();
        let idle = app
            .world_mut()
            .spawn((
                AseAnimation::default(),
                ManualTick,
                AnimationLayer::new(handle),
            ))
            .id();

        app.update();
        app.world_mut().trigger(NextFrame { entity: driven });
        app.update();

        assert_eq!(app.world().get::<AseFrame>(driven).unwrap().0, 1);
        assert_eq!(app.world().get::<AseFrame>(idle).unwrap().0, 0);
    }

    /// Reflected components carry `ReflectComponent`, without which inspectors
    /// cannot show them and dynamic scenes drop them.
    #[test]
    fn components_register_reflect_component() {
        use bevy::ecs::reflect::ReflectComponent;
        use bevy::reflect::TypeRegistry;

        let (app, _) = plugin_app(test_aseprite(), 120);
        let registry = app.world().resource::<AppTypeRegistry>().read();

        let assert_registered = |registry: &TypeRegistry, name: &str| {
            let registration = registry
                .get_with_type_path(name)
                .unwrap_or_else(|| panic!("{name} is not registered"));
            assert!(
                registration.data::<ReflectComponent>().is_some(),
                "{name} is registered without ReflectComponent",
            );
        };

        for name in [
            "bevy_aseprite_ultra::animation::AseAnimation",
            "bevy_aseprite_ultra::animation::AseFrame",
            "bevy_aseprite_ultra::animation::AseTag",
            "bevy_aseprite_ultra::animation::AnimationLayer",
            "bevy_aseprite_ultra::animation::AnimationState",
            "bevy_aseprite_ultra::animation::AnimationFrameCursor",
            "bevy_aseprite_ultra::animation::ManualTick",
        ] {
            assert_registered(&registry, name);
        }
    }

    /// Ping-pong bounces off both ends of the tag and shows every frame on the
    /// way, wherever the tag sits in the file.
    #[test]
    fn ping_pong_bounces_inside_the_tag_wherever_it_sits() {
        use crate::loader::TagMeta;

        for (name, start, end) in [("zero", 0u16, 3u16), ("offset", 5u16, 9u16)] {
            let mut ase = test_aseprite();
            ase.frame_durations = vec![Duration::from_millis(100); 10];
            ase.frame_indices = (0..10).collect();
            ase.tags.insert(
                TagId::new(name),
                TagMeta {
                    direction: AnimationDirection::PingPong,
                    range: start..=end,
                    repeat: 0,
                },
            );
            let (mut app, handle) = plugin_app(ase, 120);
            let entity = app
                .world_mut()
                .spawn((AseAnimation::tag(name), AnimationLayer::new(handle)))
                .id();

            // No `AseTag` component, so frames stay absolute.
            let mut seen = Vec::new();
            for _ in 0..(end - start + 1) * 3 {
                app.update();
                let frame = app.world().get::<AseFrame>(entity).expect("frame").0;
                seen.push(frame);
                assert!(
                    (start..=end).contains(&frame),
                    "{name}: frame {frame} left the tag's {start}..={end}, saw {seen:?}",
                );
            }
            for expected in start..=end {
                assert!(
                    seen.contains(&expected),
                    "{name}: frame {expected} never played, saw {seen:?}",
                );
            }
        }
    }

    /// The far-end start must not depend on the frame happening to begin
    /// outside the range: a tag at 0..=3 already contains frame 0.
    #[test]
    fn ping_pong_reverse_opens_backward_on_a_zero_based_tag() {
        use crate::loader::TagMeta;

        let mut ase = test_aseprite();
        ase.frame_durations = vec![Duration::from_millis(100); 4];
        ase.frame_indices = (0..4).collect();
        ase.tags.insert(
            TagId::new("bounce"),
            TagMeta {
                direction: AnimationDirection::PingPongReverse,
                range: 0..=3,
                repeat: 0,
            },
        );
        let (mut app, handle) = plugin_app(ase, 120);
        let entity = app
            .world_mut()
            .spawn((AseAnimation::tag("bounce"), AnimationLayer::new(handle)))
            .id();

        let mut seen = Vec::new();
        for _ in 0..3 {
            app.update();
            seen.push(app.world().get::<AseFrame>(entity).expect("frame").0);
        }
        assert_eq!(
            seen[0], 3,
            "a reversed ping-pong opens on the range's last frame, saw {seen:?}",
        );
    }

    /// A reversed ping-pong starts at the far end and walks back, which is what
    /// distinguishes it from a plain ping-pong.
    #[test]
    fn ping_pong_reverse_starts_at_the_far_end() {
        use crate::loader::TagMeta;

        let mut ase = test_aseprite();
        ase.frame_durations = vec![Duration::from_millis(100); 10];
        ase.frame_indices = (0..10).collect();
        ase.tags.insert(
            TagId::new("bounce"),
            TagMeta {
                direction: AnimationDirection::PingPongReverse,
                range: 5..=9,
                repeat: 0,
            },
        );
        let (mut app, handle) = plugin_app(ase, 120);
        let entity = app
            .world_mut()
            .spawn((AseAnimation::tag("bounce"), AnimationLayer::new(handle)))
            .id();

        let mut seen = Vec::new();
        for _ in 0..4 {
            app.update();
            seen.push(app.world().get::<AseFrame>(entity).expect("frame").0);
        }
        assert!(
            seen[1] < seen[0] || seen[0] == 9,
            "a reversed ping-pong walks down from the end, saw {seen:?}",
        );
    }

    /// Reverse walks the whole tag and stays inside it, wherever the tag sits
    /// in the file.
    ///
    /// `AseFrame` is relative to the tag, so a tag at 5..=9 has frames 0..=4;
    /// reverse must visit every one of them and wrap back to the last, not
    /// step below zero and strand the animation outside its own range.
    #[test]
    fn reverse_walks_every_frame_of_a_tag_wherever_it_sits() {
        use crate::loader::TagMeta;

        for (name, start, end) in [("zero", 0u16, 3u16), ("offset", 5u16, 9u16)] {
            let span = end - start;
            // No `AseTag` component, so frames stay absolute — the shape every
            // example spawns.
            let mut ase = test_aseprite();
            ase.frame_durations = vec![Duration::from_millis(100); 10];
            ase.frame_indices = (0..10).collect();
            ase.tags.insert(
                TagId::new(name),
                TagMeta {
                    direction: AnimationDirection::Reverse,
                    range: start..=end,
                    repeat: 0,
                },
            );
            let (mut app, handle) = plugin_app(ase, 120);
            let entity = app
                .world_mut()
                .spawn((AseAnimation::tag(name), AnimationLayer::new(handle)))
                .id();

            let mut seen = Vec::new();
            for _ in 0..(span + 1) * 2 {
                app.update();
                let frame = app.world().get::<AseFrame>(entity).expect("frame").0;
                seen.push(frame);
                assert!(
                    (start..=end).contains(&frame),
                    "{name}: frame {frame} left the tag's {start}..={end}, saw {seen:?}",
                );
            }

            for expected in start..=end {
                assert!(
                    seen.contains(&expected),
                    "{name}: frame {expected} never played, saw {seen:?}",
                );
            }
        }
    }
}
