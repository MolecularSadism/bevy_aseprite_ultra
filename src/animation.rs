use crate::layers::{AseTexture, SpriteLayerOf};
use crate::loader::Aseprite;
use crate::slice::AseSlice;
use anyhow::Context;
use aseprite_loader::binary::chunks::tags::AnimationDirection as RawDirection;
use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::component::Mutable,
    image::TextureAtlas,
    prelude::*,
    sprite::Sprite,
    sprite_render::Material2d,
    ui::{widget::ImageNode, UiSystems},
};
use std::{collections::VecDeque, time::Duration};

pub struct AsepriteAnimationPlugin;
impl Plugin for AsepriteAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AnimationEvents>();
        app.add_systems(PreUpdate, update_aseprite_animation);

        app.add_systems(
            PostUpdate,
            (
                render_children_animation::<ImageNode>.before(UiSystems::Prepare),
                render_children_animation::<Sprite>,
            ),
        );
        app.add_observer(next_frame);

        app.register_type::<AseAnimation>();
        app.register_type::<AnimationState>();
        app.register_type::<AseFrame>();
        app.register_type::<AseTag>();
        app.register_type::<PlayDirection>();
        app.register_type::<AnimationRepeat>();
    }
}

/// Any component that implements this trait can be used as a render target for
/// aseprite frames. The plugin ships with implementations for [`Sprite`],
/// [`ImageNode`], [`MeshMaterial2d`], and [`MaterialNode`] (plus [`MeshMaterial3d`]
/// with the `3d` feature).
///
/// Implement this trait on your own material to drive custom shaders from the
/// current [`AseFrame`].
pub trait RenderAnimation {
    /// An extra system parameter used in rendering. Use a tuple if many are required.
    type Extra<'e>;
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        frame: u16,
        extra: &mut Self::Extra<'_>,
    );
}

impl RenderAnimation for ImageNode {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: aseprite.get_atlas_index(usize::from(frame)),
        });
    }
}

impl RenderAnimation for Sprite {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, frame: u16, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: aseprite.get_atlas_index(usize::from(frame)),
        });
    }
}

impl<M: Material2d + RenderAnimation> RenderAnimation for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        frame: u16,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, frame, &mut extra.1);
    }
}

impl<M: UiMaterial + RenderAnimation> RenderAnimation for MaterialNode<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        frame: u16,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, frame, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderAnimation> RenderAnimation for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        frame: u16,
        extra: &mut Self::Extra<'_>,
    ) {
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
#[derive(Component, Default, Reflect, Clone, Copy, Debug)]
#[reflect]
pub struct AseFrame(pub u16);

impl AseFrame {
    pub fn new(frame: u16) -> Self {
        AseFrame(frame)
    }

    pub fn get(&self) -> u16 {
        self.0
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
#[derive(Component, Reflect, Clone, Debug)]
#[reflect]
pub struct AseTag(pub String);

impl AseTag {
    pub fn new(name: impl Into<String>) -> Self {
        AseTag(name.into())
    }

    pub fn get(&self) -> &str {
        &self.0
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
    let Some(meta) = aseprite.tags.get(&tag.0) else {
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
#[reflect]
pub struct AseAnimation {
    pub tag: Option<String>,
    pub speed: f32,
    pub playing: bool,
    /// Override for repeat behavior. `None` uses the aseprite file's tag repeat
    /// count (falling back to loop when no tag or repeat=0). Set via
    /// [`with_repeat`](Self::with_repeat); reset to file default with
    /// [`use_file_repeat`](Self::use_file_repeat).
    pub repeat: Option<AnimationRepeat>,
    /// Overwrite aseprite direction
    pub direction: Option<AnimationDirection>,
    pub queue: VecDeque<(String, Option<AnimationRepeat>)>,
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
    pub fn tag(tag: &str) -> Self {
        Self::default().with_tag(tag)
    }

    /// Animation speed multiplier, default is 1.0.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Animation holds relative frame when tag changes, default is false.
    pub fn with_relative_frame_hold(mut self, hold_relative_frame: bool) -> Self {
        self.hold_relative_frame = hold_relative_frame;
        self
    }

    /// Animation with tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Overrides how many times the animation plays. Pass
    /// `AnimationRepeat::Loop` for infinite looping or
    /// `AnimationRepeat::Count(n)` to play exactly `n` times.
    /// The override persists across tag changes until cleared with
    /// [`use_file_repeat`](Self::use_file_repeat).
    pub fn with_repeat(mut self, repeat: AnimationRepeat) -> Self {
        self.repeat = Some(repeat);
        self.needs_repeat_init = true;
        self
    }

    /// Clears the repeat override so the animation uses the aseprite file's
    /// tag repeat count.
    pub fn use_file_repeat(mut self) -> Self {
        self.repeat = None;
        self.needs_repeat_init = true;
        self
    }

    /// Provides an animation direction, overwrites aseprite direction.
    pub fn with_direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Chains an animation after the current one is done. Pass `None` for
    /// repeat to use the file's tag repeat, or `Some(repeat)` to override.
    pub fn with_then(
        mut self,
        tag: impl Into<String>,
        repeat: Option<AnimationRepeat>,
    ) -> Self {
        self.queue.push_back((tag.into(), repeat));
        self
    }

    /// Instantly starts playing a new animation using the file's tag repeat
    /// count. Clears any queued animations and any repeat override.
    pub fn play(&mut self, tag: impl Into<String>) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = None;
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new animation with an explicit repeat
    /// override. Clears any queued animations.
    pub fn play_with_repeat(&mut self, tag: impl Into<String>, repeat: AnimationRepeat) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = Some(repeat);
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new animation starting with same relative frame
    /// only if the new relative group is the same as the previous one.
    /// Uses the file's tag repeat count.
    pub fn play_with_relative_group(
        &mut self,
        tag: impl Into<String>,
        new_relative_group: u16,
    ) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.new_relative_group = new_relative_group;
        self.repeat = None;
        self.needs_repeat_init = true;
        self.queue.clear();
    }

    /// Instantly starts playing a new looping animation, overriding the file's
    /// repeat count.
    pub fn play_loop(&mut self, tag: impl Into<String>) {
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
    pub fn then(&mut self, tag: impl Into<String>, repeat: Option<AnimationRepeat>) {
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
#[reflect]
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
/// Use [`NextFrameEvent`] to manually advance frames, or modify
/// [`AnimationState`] directly.
#[derive(Component)]
pub struct ManualTick;

/// Tracks per-animation internal state (relative frame within the active tag,
/// elapsed time within the current frame, and ping-pong direction).
///
/// The authoritative "current frame index into the asset" lives on
/// [`AseFrame`], not here. This struct is only meaningful for entities driven
/// by [`AseAnimation`]; reading it on a manually-set frame is a no-op.
#[derive(Component, Debug, Default, Reflect)]
#[reflect]
pub struct AnimationState {
    pub relative_frame: u16,
    pub elapsed: std::time::Duration,
    pub current_direction: PlayDirection,
}

#[allow(unused)]
impl AnimationState {
    pub fn relative_frame(&self) -> u16 {
        self.relative_frame
    }
}

/// The current playback direction within a ping-pong animation.
#[derive(Default, Debug, Reflect)]
#[reflect]
pub enum PlayDirection {
    #[default]
    Forward,
    Backward,
}

/// Events emitted by the animation system.
///
/// Use `EventReader<AnimationEvents>` to react to animation completions.
#[derive(Message, Debug, Reflect)]
#[reflect]
pub enum AnimationEvents {
    Finished(Entity),
    LoopCycleFinished(Entity),
}

/// Playback direction for an animation.
#[derive(Default, Clone, Reflect, Debug)]
#[reflect]
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
            _ => panic!("Invalid AnimationDirection"),
        }
    }
}

/// How many times an animation should play.
#[derive(Default, Debug, Clone, Reflect)]
#[reflect]
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

/// Ticks animation state on entities with [`AseAnimation`].
/// Works for both parent entities (with [`AseTexture`]) and standalone
/// entities (with [`AnimationLayer`], e.g. for custom materials).
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
) -> Result<(), BevyError> {
    for (entity, mut animation, mut state, mut frame, mut ase_tag, tex, layer, is_manual) in
        animations.iter_mut()
    {
        let Some(handle) = resolve_handle(tex, layer) else {
            continue;
        };
        let Some(aseprite) = aseprites.get(handle) else {
            continue;
        };

        let tag_meta = animation
            .tag
            .as_ref()
            .map(|t| aseprite.tags.get(t))
            .flatten();

        let range = match animation.tag.as_ref() {
            Some(tag) => tag_meta
                .map(|meta| meta.range.clone())
                .context(format!(
                    "Animation tag \"{tag}\" not found in aseprite file",
                ))?,
            None => 0..=(aseprite.frame_durations.len() as u16 - 1),
        };

        // Mirror the animation's tag into AseTag (when present) so the renderer
        // can resolve tag-relative frames consistently. AseFrame is treated as
        // tag-relative on this entity.
        let has_tag = ase_tag.is_some();
        if let Some(tag) = ase_tag.as_deref_mut() {
            let desired = animation.tag.clone().unwrap_or_default();
            if tag.0 != desired {
                tag.0 = desired;
            }
        }

        // Working range/index: absolute when no AseTag, relative when AseTag is present.
        let (lo, hi) = if has_tag {
            (0, range.end() - range.start())
        } else {
            (*range.start(), *range.end())
        };
        let working_range = lo..=hi;

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
            animation.needs_repeat_init = false;
        }

        if !working_range.contains(&frame.0) {
            if !animation.hold_relative_frame {
                frame.0 = lo;
                state.relative_frame = 0;
                animation.relative_group = 0;
                animation.new_relative_group = 0;
            } else {
                if animation.new_relative_group != animation.relative_group {
                    animation.relative_group = animation.new_relative_group;
                    frame.0 = lo;
                    state.relative_frame = 0;
                    state.elapsed = std::time::Duration::ZERO;
                } else {
                    let span = hi - lo + 1;
                    state.relative_frame = state.relative_frame % span;
                    frame.0 = lo + state.relative_frame;
                }
            }
        }

        if is_manual {
            continue;
        }

        if !animation.playing {
            continue;
        }

        state.elapsed +=
            std::time::Duration::from_secs_f32(time.delta_secs() * animation.speed);

        // frame_durations is indexed by absolute frame.
        let absolute_frame = if has_tag { range.start() + frame.0 } else { frame.0 };
        let Some(frame_duration) = aseprite.frame_durations.get(usize::from(absolute_frame)) else {
            return Ok(());
        };

        if state.elapsed > *frame_duration {
            cmd.trigger(NextFrameEvent(entity));
            state.elapsed =
                Duration::from_secs_f32(state.elapsed.as_secs_f32() % frame_duration.as_secs_f32());
        }
    }
    Ok(())
}

/// Trigger this event to manually advance an animation by one frame.
///
/// Used together with [`ManualTick`] for frame-by-frame control.
#[derive(Event)]
pub struct NextFrameEvent(pub Entity);

fn next_frame(
    trigger: On<NextFrameEvent>,
    mut events: MessageWriter<AnimationEvents>,
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
    let Ok((mut state, mut frame, mut anim, has_tag, tex, layer)) =
        animations.get_mut(trigger.0)
    else {
        return;
    };

    let Some(handle) = resolve_handle(tex, layer) else {
        return;
    };
    let Some(aseprite) = aseprites.get(handle) else {
        return;
    };

    let (abs_range, direction) = match anim
        .tag
        .as_ref()
        .map(|t| aseprite.tags.get(t))
        .flatten()
    {
        Some(meta) => {
            let dir = anim
                .direction
                .clone()
                .unwrap_or(AnimationDirection::from(meta.direction));
            (meta.range.clone(), dir)
        }
        None => {
            let dir = anim
                .direction
                .clone()
                .unwrap_or(AnimationDirection::Forward);
            (0..=(aseprite.frame_durations.len() as u16 - 1), dir)
        }
    };

    // Range used for incrementing AseFrame: relative (0-based) when AseTag is
    // present, absolute otherwise.
    let range = if has_tag {
        0..=(abs_range.end() - abs_range.start())
    } else {
        abs_range.clone()
    };

    // Helper: handle end-of-cycle logic using remaining_cycles.
    // Returns true if the animation should wrap/continue, false if finished.
    let handle_cycle_end = |anim: &mut AseAnimation,
                            events: &mut MessageWriter<AnimationEvents>,
                            entity: Entity|
     -> bool {
        match anim.remaining_cycles {
            None => {
                events.write(AnimationEvents::LoopCycleFinished(entity));
                true
            }
            Some(0) => {
                if anim.queue.is_empty() {
                    events.write(AnimationEvents::Finished(entity));
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
            let next = frame.0 + 1;

            if next > *range.end() {
                if handle_cycle_end(&mut anim, &mut events, trigger.0) {
                    frame.0 = *range.start();
                    state.relative_frame = 0;
                }
            } else {
                frame.0 = next;
                state.relative_frame += 1;
            }
        }
        AnimationDirection::Reverse => {
            let next = frame.0.checked_sub(1).unwrap_or(*range.end());

            if next == *range.end() {
                if handle_cycle_end(&mut anim, &mut events, trigger.0) {
                    frame.0 = range.end() - 1;
                    state.relative_frame = range.end() - range.start() - 1;
                }
            } else {
                frame.0 = next;
                state
                    .relative_frame
                    .checked_sub(1)
                    .unwrap_or(range.end() - range.start() - 1);
            }
        }
        AnimationDirection::PingPong | AnimationDirection::PingPongReverse => {
            let (next, relative_next) = match state.current_direction {
                PlayDirection::Forward => (frame.0 + 1, state.relative_frame + 1),
                PlayDirection::Backward => (
                    state.relative_frame.checked_sub(1).unwrap_or(0),
                    frame.0.checked_sub(1).unwrap_or(0),
                ),
            };

            let is_forward = match state.current_direction {
                PlayDirection::Forward => true,
                PlayDirection::Backward => false,
            };

            if next >= *range.end() && is_forward {
                if handle_cycle_end(&mut anim, &mut events, trigger.0) {
                    state.current_direction = PlayDirection::Backward;
                    frame.0 = range.end() - 2;
                    state.relative_frame = range.end() - range.start() - 2;
                }
            } else if next <= *range.start() && !is_forward {
                if handle_cycle_end(&mut anim, &mut events, trigger.0) {
                    state.current_direction = PlayDirection::Forward;
                    frame.0 = *range.start();
                    state.relative_frame = 0;
                }
            } else {
                frame.0 = next;
                state.relative_frame = relative_next;
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
    /// `frame_indicies` populated for `get_atlas_index`; everything else can
    /// be defaulted.
    fn test_aseprite() -> Aseprite {
        Aseprite {
            slices: HashMap::default(),
            tags: HashMap::default(),
            frame_durations: vec![Duration::from_millis(100); 4],
            atlas_layout: Handle::<TextureAtlasLayout>::default(),
            atlas_image: Handle::<Image>::default(),
            frame_indicies: vec![0, 1, 2, 3],
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
        app.world().get::<CapturedFrame>(entity).and_then(|c| c.last)
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
            "Rock".to_string(),
            TagMeta {
                direction: RawDirection::Forward,
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
}
