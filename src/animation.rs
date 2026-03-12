use crate::layers::{AseTexture, SpriteLayers};
use crate::loader::Aseprite;
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
        app.add_systems(PreUpdate, (update_aseprite_animation, propagate_frame).chain());

        app.add_systems(
            PostUpdate,
            (
                render_children_animation::<ImageNode>.before(UiSystems::Prepare),
                render_children_animation::<Sprite>,
                render_animation::<ImageNode>.before(UiSystems::Prepare),
                render_animation::<Sprite>,
            ),
        );
        app.add_observer(next_frame);

        app.register_type::<AseAnimation>();
        app.register_type::<AnimationState>();
        app.register_type::<PlayDirection>();
        app.register_type::<AnimationRepeat>();
    }
}

/// Any component that implements this trait can be used as a render target for
/// aseprite animations. The plugin ships with implementations for [`Sprite`],
/// [`ImageNode`], [`MeshMaterial2d`], and [`MaterialNode`] (plus [`MeshMaterial3d`]
/// with the `3d` feature).
///
/// Implement this trait on your own material to drive custom shaders with
/// aseprite animation data.
pub trait RenderAnimation {
    /// An extra system parameter used in rendering. Use a tuple if many are required.
    type Extra<'e>;
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        extra: &mut Self::Extra<'_>,
    );
}

impl RenderAnimation for ImageNode {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, state: &AnimationState, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: aseprite.get_atlas_index(usize::from(state.current_frame)),
        });
    }
}

impl RenderAnimation for Sprite {
    type Extra<'e> = ();
    fn render_animation(&mut self, aseprite: &Aseprite, state: &AnimationState, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: aseprite.get_atlas_index(usize::from(state.current_frame)),
        });
    }
}

impl<M: Material2d + RenderAnimation> RenderAnimation for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, state, &mut extra.1);
    }
}

impl<M: UiMaterial + RenderAnimation> RenderAnimation for MaterialNode<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, state, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderAnimation> RenderAnimation for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderAnimation>::Extra<'e>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_animation(aseprite, state, &mut extra.1);
    }
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
#[reflect]
pub struct AseAnimation {
    pub tag: Option<String>,
    pub speed: f32,
    pub playing: bool,
    pub repeat: AnimationRepeat,
    /// Overwrite aseprite direction
    pub direction: Option<AnimationDirection>,
    pub queue: VecDeque<(String, AnimationRepeat)>,
    pub hold_relative_frame: bool,
    pub relative_group: u16,
    pub new_relative_group: u16,
}

impl Default for AseAnimation {
    fn default() -> Self {
        Self {
            tag: None,
            speed: 1.0,
            playing: true,
            repeat: AnimationRepeat::Loop,
            direction: None,
            queue: VecDeque::new(),
            hold_relative_frame: false,
            relative_group: 0,
            new_relative_group: 0,
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

    /// Sets a repeat count, default is loop.
    pub fn with_repeat(mut self, repeat: AnimationRepeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Provides an animation direction, overwrites aseprite direction.
    pub fn with_direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Chains an animation after the current one is done.
    pub fn with_then(mut self, tag: impl Into<String>, repeats: AnimationRepeat) -> Self {
        self.queue.push_back((tag.into(), repeats));
        self
    }

    /// Instantly starts playing a new animation, clearing any item left in the queue.
    pub fn play(&mut self, tag: impl Into<String>, repeat: AnimationRepeat) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = repeat;
        self.queue.clear();
    }

    /// Instantly starts playing a new animation starting with same relative frame
    /// only if the new relative group is the same as the previous one.
    pub fn play_with_relative_group(
        &mut self,
        tag: impl Into<String>,
        repeat: AnimationRepeat,
        new_relative_group: u16,
    ) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.new_relative_group = new_relative_group;
        self.repeat = repeat;
        self.queue.clear();
    }

    /// Instantly starts playing a new looping animation.
    pub fn play_loop(&mut self, tag: impl Into<String>) {
        self.playing = true;
        self.tag = Some(tag.into());
        self.repeat = AnimationRepeat::Loop;
        self.queue.clear();
    }

    /// Instantly stops the currently playing animation.
    pub fn stop(&mut self) {
        self.playing = false;
        self.tag = None;
        self.repeat = AnimationRepeat::Loop;
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

    /// Chains an animation after the current one is done.
    pub fn then(&mut self, tag: impl Into<String>, repeats: AnimationRepeat) {
        self.queue.push_back((tag.into(), repeats));
    }

    /// Clears any queued up animations.
    pub fn clear_queue(&mut self) {
        self.queue.clear()
    }

    fn next(&mut self) {
        if let Some((tag, repeat)) = self.queue.pop_front() {
            self.tag = Some(tag);
            self.repeat = repeat;
        }
    }
}

impl From<&str> for AseAnimation {
    fn from(tag: &str) -> Self {
        AseAnimation {
            tag: Some(tag.to_string()),
            speed: 1.0,
            ..Default::default()
        }
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

/// Tracks the current frame and elapsed time of an animation.
///
/// Automatically added to entities with [`AseAnimation`] via required components.
/// You can read this to query the current animation frame, or write to it
/// when using [`ManualTick`] for manual frame control.
#[derive(Component, Debug, Default, Reflect)]
#[reflect]
pub struct AnimationState {
    pub relative_frame: u16,
    pub current_frame: u16,
    pub elapsed: std::time::Duration,
    pub current_direction: PlayDirection,
}

#[allow(unused)]
impl AnimationState {
    pub fn current_frame(&self) -> u16 {
        self.current_frame
    }
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

/// How many times an animation should repeat.
#[derive(Default, Debug, Clone, Reflect)]
#[reflect]
pub enum AnimationRepeat {
    #[default]
    Loop,
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
        Option<&AseTexture>,
        Option<&AnimationLayer>,
        Has<ManualTick>,
    )>,
    aseprites: Res<Assets<Aseprite>>,
    time: Res<Time>,
) -> Result<(), BevyError> {
    for (entity, mut animation, mut state, tex, layer, is_manual) in animations.iter_mut() {
        let Some(handle) = resolve_handle(tex, layer) else {
            continue;
        };
        let Some(aseprite) = aseprites.get(handle) else {
            continue;
        };

        let range = match animation.tag.as_ref() {
            Some(tag) => aseprite
                .tags
                .get(tag)
                .map(|meta| meta.range.clone())
                .context(format!(
                    "Animation tag \"{tag}\" not found in aseprite file",
                ))?,
            None => 0..=(aseprite.frame_durations.len() as u16 - 1),
        };

        if !range.contains(&state.current_frame) {
            if !animation.hold_relative_frame {
                state.current_frame = *range.start();
                state.relative_frame = 0;
                animation.relative_group = 0;
                animation.new_relative_group = 0;
            } else {
                if animation.new_relative_group != animation.relative_group {
                    animation.relative_group = animation.new_relative_group;
                    state.current_frame = *range.start();
                    state.relative_frame = 0;
                    state.elapsed = std::time::Duration::ZERO;
                } else {
                    state.relative_frame =
                        (state.relative_frame) % (*range.end() * range.start() - 1);
                    state.current_frame = *range.start() + state.relative_frame;
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

        let Some(frame_duration) = aseprite
            .frame_durations
            .get(usize::from(state.current_frame))
        else {
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
        &mut AseAnimation,
        Option<&AseTexture>,
        Option<&AnimationLayer>,
    )>,
    aseprites: Res<Assets<Aseprite>>,
) {
    let Ok((mut state, mut anim, tex, layer)) = animations.get_mut(trigger.0) else {
        return;
    };

    let Some(handle) = resolve_handle(tex, layer) else {
        return;
    };
    let Some(aseprite) = aseprites.get(handle) else {
        return;
    };

    let (range, direction) = match anim
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

    match direction {
        AnimationDirection::Forward => {
            let next = state.current_frame + 1;

            if next > *range.end() {
                match anim.repeat {
                    AnimationRepeat::Loop => {
                        state.current_frame = *range.start();
                        state.relative_frame = 0;
                        events.write(AnimationEvents::LoopCycleFinished(trigger.0));
                    }
                    AnimationRepeat::Count(count) => {
                        if count > 0 {
                            state.current_frame = *range.start();
                            state.relative_frame = 0;
                            anim.repeat = AnimationRepeat::Count(count - 1);
                        } else {
                            if anim.queue.is_empty() {
                                events.write(AnimationEvents::Finished(trigger.0));
                            } else {
                                anim.next();
                            }
                        }
                    }
                }
            } else {
                state.current_frame = next;
                state.relative_frame += 1;
            }
        }
        AnimationDirection::Reverse => {
            let next = state.current_frame.checked_sub(1).unwrap_or(*range.end());

            if next == *range.end() {
                match anim.repeat {
                    AnimationRepeat::Loop => {
                        state.current_frame = range.end() - 1;
                        state.relative_frame = range.end() - range.start() - 1;
                        events.write(AnimationEvents::LoopCycleFinished(trigger.0));
                    }
                    AnimationRepeat::Count(count) => {
                        if count > 0 {
                            state.current_frame = range.end() - 1;
                            state.relative_frame = range.end() - range.start() - 1;
                            anim.repeat = AnimationRepeat::Count(count - 1);
                        } else {
                            if anim.queue.is_empty() {
                                events.write(AnimationEvents::Finished(trigger.0));
                            } else {
                                anim.next();
                            }
                        }
                    }
                }
            } else {
                state.current_frame = next;
                state
                    .relative_frame
                    .checked_sub(1)
                    .unwrap_or(range.end() - range.start() - 1);
            }
        }
        AnimationDirection::PingPong | AnimationDirection::PingPongReverse => {
            let (next, relative_next) = match state.current_direction {
                PlayDirection::Forward => (state.current_frame + 1, state.relative_frame + 1),
                PlayDirection::Backward => (
                    state.relative_frame.checked_sub(1).unwrap_or(0),
                    state.current_frame.checked_sub(1).unwrap_or(0),
                ),
            };

            let is_forward = match state.current_direction {
                PlayDirection::Forward => true,
                PlayDirection::Backward => false,
            };

            if next >= *range.end() && is_forward {
                match anim.repeat {
                    AnimationRepeat::Loop => {
                        state.current_direction = PlayDirection::Backward;
                        state.current_frame = range.end() - 2;
                        state.relative_frame = range.end() - range.start() - 2;
                        events.write(AnimationEvents::LoopCycleFinished(trigger.0));
                    }
                    AnimationRepeat::Count(count) => {
                        if count > 0 {
                            state.current_direction = PlayDirection::Backward;
                            state.current_frame = range.end() - 2;
                            state.relative_frame = range.end() - range.start() - 2;
                            anim.repeat = AnimationRepeat::Count(count - 1);
                        } else {
                            if anim.queue.is_empty() {
                                events.write(AnimationEvents::Finished(trigger.0));
                            } else {
                                anim.next();
                            }
                        }
                    }
                };
            } else if next <= *range.start() && !is_forward {
                match anim.repeat {
                    AnimationRepeat::Loop => {
                        state.current_direction = PlayDirection::Forward;
                        state.current_frame = *range.start();
                        state.relative_frame = 0;
                        events.write(AnimationEvents::LoopCycleFinished(trigger.0));
                    }
                    AnimationRepeat::Count(count) => {
                        if count > 0 {
                            state.current_direction = PlayDirection::Forward;
                            state.current_frame = *range.start();
                            state.relative_frame = 0;
                            anim.repeat = AnimationRepeat::Count(count - 1);
                        } else {
                            if anim.queue.is_empty() {
                                events.write(AnimationEvents::Finished(trigger.0));
                            } else {
                                anim.next();
                            }
                        }
                    }
                };
            } else {
                state.current_frame = next;
                state.relative_frame = relative_next;
            }
        }
    };
}

/// Propagates parent [`AnimationState`] to children's render targets.
/// Runs after tick so children always reflect the latest frame.
fn propagate_frame(
    parents: Query<(&AnimationState, &SpriteLayers), With<AseAnimation>>,
    mut child_sprites: Query<&mut AnimationState, Without<AseAnimation>>,
) {
    for (parent_state, layers) in &parents {
        for child in layers.iter() {
            if let Ok(mut child_state) = child_sprites.get_mut(child) {
                child_state.current_frame = parent_state.current_frame;
                child_state.relative_frame = parent_state.relative_frame;
                child_state.elapsed = parent_state.elapsed;
                child_state.current_direction = match &parent_state.current_direction {
                    PlayDirection::Forward => PlayDirection::Forward,
                    PlayDirection::Backward => PlayDirection::Backward,
                };
            }
        }
    }
}

// ---- Render systems ----

/// Renders animation frames on child entities via parent → child iteration.
/// Registered for [`Sprite`] and [`ImageNode`] by default.
/// Register for your custom material type to support material rendering on children.
pub fn render_children_animation<T: RenderAnimation + Component<Mutability = Mutable>>(
    parents: Query<(&AnimationState, &SpriteLayers), With<AseAnimation>>,
    mut children: Query<(&AnimationLayer, &mut T)>,
    aseprites: Res<Assets<Aseprite>>,
    mut extra: <T as RenderAnimation>::Extra<'_>,
) {
    for (state, layers) in &parents {
        for child in layers.iter() {
            if let Ok((layer, mut target)) = children.get_mut(child) {
                let Some(aseprite) = aseprites.get(&layer.aseprite) else {
                    continue;
                };
                target.render_animation(aseprite, state, &mut extra);
            }
        }
    }
}

/// Renders animation frames on standalone entities that have both
/// [`AnimationLayer`] and [`AnimationState`] directly (e.g. custom materials).
pub fn render_animation<T: RenderAnimation + Component<Mutability = Mutable>>(
    mut animations: Query<(&AnimationLayer, &mut T, &AnimationState), Without<SpriteLayers>>,
    aseprites: Res<Assets<Aseprite>>,
    mut extra: <T as RenderAnimation>::Extra<'_>,
) {
    for (layer, mut target, state) in &mut animations {
        let Some(aseprite) = aseprites.get(&layer.aseprite) else {
            continue;
        };
        target.render_animation(aseprite, state, &mut extra);
    }
}
