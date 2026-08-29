## Unreleased

- **A file's per-layer sub-assets stay resident for as long as its composite
  is.** The loader builds every layer variant in the one load pass, but handed
  each labeled sub-asset's handle to no one, so Bevy dropped every one the
  moment the load finished — a labeled sub-asset it holds no live handle to is
  never inserted. The next `load("file.aseprite#Layer")` then forced a
  from-scratch reload of the whole file, re-decoding and re-packing its atlas,
  and resolved to nothing until it finished several frames later. The composite
  now keeps a handle to each variant, so a layer is ready the moment its file
  is rather than after re-decoding it. No API or on-disk cache change.

## 0.12.0

Breaking. The tick system stops panicking on values a caller is invited to
set, a clip ends once instead of repeating its ending, and a tag change
starts the tag.

- **A negative or non-finite `speed` no longer panics.** It reached
  `Duration::from_secs_f32`, which refuses both, taking the schedule down from
  a public field documented only as a multiplier. Backwards playback is
  `AnimationDirection::Reverse`; `speed` is documented as non-negative.
- **`AnimationEvent::Finished` fires once.** A finished animation kept crossing
  its frame duration and kept writing the event, once per frame duration for as
  long as the entity lived.
- **A tag change opens the new tag.** `play`, `then` and the queue left the
  frame wherever the last tag stopped, unless the new one ran backwards. Frame
  carry-over is what `hold_relative_frame` is for, and it means something again.
- **Ping-pong counts a there-and-back as one cycle.** Each end counted, so a
  clip told to play once went out and never came back.
- **`relative_frame` is derived from the frame and its range**, in one place,
  rather than stepped alongside it — it could leave the range of a one-frame
  tag, and went stale on a tag change.
- **A tag reaching past the file's frames is clamped at load**, with a warning
  naming it. It used to walk to the first frame it could not resolve and stop
  there in silence, drawing a plausible last frame forever.
- **`SliceMeta::border()` measures a centre against the rect it belongs to.** A
  slice resized across its timeline had a later key's centre measured against
  frame 0's rect, which produced negative insets the doc promised were
  impossible. `AsepriteBuilder::with_nine_patch_slice` is validated the same way.
- **A slice with no area anchors at its centre** instead of dividing by zero
  into a `NaN` anchor.
- **`RenderSlice::render_slice` takes a `SliceView`** — one slice on one frame,
  `Copy` — instead of a `&SliceMeta` the renderer had to synthesise per entity
  per frame by cloning the slice's whole timeline. `SliceMeta::view_at_frame`
  exposes the same resolution.
- **A static layer child stops reporting a change it never made.**
  `render_children_animation` rewrote every child every frame, which is what
  `AseTexture`'s "zero per-tick overhead" ruled out.
- **A structural hot reload reaches the entities drawing it.** Adding, removing
  or renaming a layer left existing children stale, since nothing watched
  `AssetEvent::Modified`.
- **Baked mode reconciles its child** instead of despawning and respawning it on
  every `AseTexture` write.
- **A layer absent from a `layer_order` override sits behind everything named**,
  rather than sharing a depth with the backmost named layer.
- **A pathless `Aseprite` reuses its parent's handle** for layer children
  instead of asking the asset server for `"#Layer"` — the builder's whole point
  is a sheet with no file behind it.

## 0.11.1

- **The tag mirror stops flagging `AseTag` on every tick.** The tick system
  compared the tag before writing it, but reached it through `DerefMut`, which
  marks a component changed whether or not anything is written. Every renderer
  that resolves a tag-relative frame watches that flag, so an animated entity
  re-rendered its slice on each of its own ticks for as long as it played.
- **A frame with no duration steps instead of panicking.** The remainder
  carried into the next frame is `elapsed % duration`, which for a zero
  duration is `NaN`, and `Duration::from_secs_f32` refuses that — so a frame an
  artist left at zero took the process down from the tick system. Nothing holds
  such a frame now; each tick moves one on.

## 0.11.0

Breaking. The crate stops panicking on art it does not like, stops failing
silently where a warning would save an artist an afternoon, and names its
types the way an ECS crate should.

### An `Aseprite` can be built without a file

- **`Aseprite::builder()`**: `AsepriteBuilder` assembles an `Aseprite` from
  slice, layer and tag metadata, so a downstream crate can test — or
  demonstrate, or benchmark — against sheet metadata without a file on disk.
  It is in `prelude` and needs no feature: with every field of `Aseprite`
  private, building one is the type's own constructor rather than a test-only
  affordance. Only the loader pairs metadata with pixels, so a built asset's
  atlas handles are `Handle::default()` and nothing renders through one.

### Names are interned once and compared as ids

- **`slices` and `tags` are keyed by `SliceId` / `TagId`**, the interned ids
  the render paths already carry, instead of by `String`. Rendering a slice
  hashed the id back into a string on every entity on every frame the asset or
  the animation changed; it now hashes the id itself.
- **`TagId`** joins `SliceId` and `LayerId`: `AseTag`, `AseAnimation::tag` and
  the animation queue all hold one. Every constructor that took a tag name
  still takes `impl Into<TagId>`, so `AseAnimation::tag("walk")` is unchanged.
- Breaking: **`Aseprite`'s fields are private**, read through
  `frame_durations()`, `atlas_layout()`, `atlas_image()` and `source_path()`
  alongside the accessors that already existed. `slice()` and `tag()` take
  `impl Into<SliceId>` / `impl Into<TagId>`, so a `&str` still works;
  `slices()` and `tags()` yield ids rather than `&str`.
- Breaking: `frame_indicies` is spelled `frame_indices`.
- Breaking: **`TagMeta::direction` is the crate's own `AnimationDirection`**,
  converted at the loader boundary. It was `aseprite_loader`'s enum, so
  reading a tag's direction meant depending on `aseprite_loader` to name the
  type. The `AnimationDirectionDef` serde shim is gone with it; a processed
  cache holding a direction the crate does not recognise now fails to load
  rather than silently playing forward.

### The crate no longer takes the process down

- **ping-pong bounces on the range's own ends.** It turned around a frame
  early, so whichever end the walk was heading for never displayed — the same
  skip reverse had. A one-frame range now stays put instead of stepping
  outside itself.
- **`PingPongReverse` opens at the far end walking down.** The play direction
  was never seeded from the animation's direction, so it was indistinguishable
  from `PingPong`. `Reverse` opens there too, rather than showing the first
  frame once and jumping.
- **the frame an animation opens on is announced.** `AnimationFrameChanged`
  reported a first observation only when it landed on frame zero, so an
  animation opening anywhere else — a reversed one, or a held relative frame
  resuming mid-clip — never announced its opening frame at all.
- **a missing animation tag plays the whole file** instead of panicking. The
  tick system returned an error, and bevy's default handler panics, so
  renaming a tag in Aseprite or a typo in `AseAnimation::tag` crashed the
  game. `next_frame` already fell back gracefully; the two halves of the
  feature now agree, as the 0.2.3 entry claimed.
- **one unresolvable frame no longer freezes every other animation** — the
  tick loop skipped the rest of the query instead of that one entity.
- `From<RawDirection> for AnimationDirection` is infallible, a zero- or
  one-frame asset no longer underflows, a slice chunk with no keys is
  skipped with a warning rather than unwrapped, and a truncated processed
  cache errors instead of slicing out of bounds.

### Failures you can see

- **the missing-slice warning ships in release builds.** It named the slice
  and listed the ones that exist, and it was compiled out of exactly the
  builds where you cannot attach a debugger.
- **a nine-patch centre that overflows its slice reads as unset**, with a
  warning naming the slice and every inset. A centre dragged past an edge
  gives that edge a negative inset, which no slicer can express.
- `AsepriteError` keeps the cause it used to discard, drops the `Error`
  suffix from every variant, and is `#[non_exhaustive]`.
- `AsepriteLoaderSettings::max_atlas_size` replaces a hard-coded 4096, so a
  project with large sheets can raise it rather than fail to load.

### Component-based, in ECS language

- `AnimationEvents` → `AnimationEvent`; it carries one event.
- `NextFrameEvent(Entity)` → `NextFrame { entity }`, an `EntityEvent`, so
  `On<NextFrame>` derefs to a named field.
- `RenderTarget` → `AseRenderTarget`, which no longer collides with bevy's
  own — the type you name to point a camera at a texture.
- **every component is `#[reflect(Component)]` and registered**, so an
  inspector can see them and dynamic scenes keep them. They carried a bare
  `#[reflect]`, which adds no type data.
- `AseSlice::name` is a `SliceId` rather than a `String`, matching
  `AseTexture::slice` and dropping four heap allocations per layer child on
  every spawn.

### Reading a slice through the component that owns it

- `AseSlice::meta`, `size` and `border` resolve through the component, which
  holds the sheet and the name together — so a caller sizing a node off the
  art cannot pair one sheet's handle with another's slice name.
  `SliceMeta::size`, `SliceMeta::border` and `Aseprite::slice` are the same
  answers for callers holding the asset.

### Methods that did not do what they said

- `AseTexture::reorder_layer` seeded its override from an empty list, so it
  silently did nothing; it now takes the asset, seeds from the file's own
  order, and returns whether the layer was found.
  `init_layer_order_from` existed only to work around that and is gone.
- `toggle_layer_on`/`toggle_layer_off` set rather than toggled and did
  nothing at all outside `LayerFilter::Include`: now `show_layer`/
  `hide_layer`, each returning whether the call landed.
- `Aseprite::set_layer_visible` and `Aseprite::reorder_layer` are removed.
  Both were documented as affecting every entity using the asset; nothing
  in the crate observed them. Order and visibility are per entity.
- `get_atlas_index` → `atlas_index`, returning `Option` rather than index
  `0` for an asset with no frames.

### Processed assets keep their layers

- The `asset_processing` cache persists the layer table and every labeled
  sub-asset, so `path#all` and `path#LayerName` resolve in a processed
  build. Layered mode previously rendered nothing at all there, silently,
  while the README advertised the processor for production.

### Infrastructure

- CI runs format, clippy, docs and tests across both ends of the feature
  space, with warnings denied in rustc and rustdoc. The crate had none, and
  nineteen clippy and four rustdoc warnings had accumulated; all are fixed.
- The plugin's own doc example built `DefaultPlugins`, which needs a
  display, so `cargo test` was red in every headless environment.
- `anyhow` is no longer a dependency.

## 0.10.0

- **the authored centre is the border**: a slice carrying a nine-patch is sliced on the centre drawn in Aseprite, and a caller that sets its own `border` no longer overrides it — the nine-patch belongs to the art. A caller that already asked to be sliced keeps the rest of its slicer (`center_scale_mode`, `sides_scale_mode`, `max_corner_scale`), which the file cannot express.
- **per-frame `AnimationFrameChanged` message**, and reverse and ping-pong tags now report the frame they are actually on rather than drifting.
- **group layers render their contents**: selecting a group by id draws every layer beneath it, where before it drew nothing (groups hold no cels of their own).
- **layer ids address one layer each**: a name repeated across groups — two colour groups each holding a `Main` — is qualified with its group path (`Blue/Main`). Unique names are unchanged, as are the sub-asset labels derived from them.
- `AsepriteLoaderSettings::visible_layers` accepts group names, and resolves each entry against the same ids.
- **render layers reach the children**: an `AseTexture` renders through child entities, which until now fell to the default layer — a camera filtering to the parent's layer drew nothing, and cameras it was excluded from drew it anyway. Children now mirror the parent's `RenderLayers`, at spawn and on every change.
- **empty nine-patch centres read as unset**: Aseprite writes a centre on every key of a nine-patch slice, so a key added before the centre was dragged out carried a zero-area one and sliced into nothing. Such a key now defers to the first key that sets a real centre, and a slice with no real centre anywhere is not a nine-patch.

## 0.9.0

- updated to bevy 0.18.
- **9-slice support**: slices with 9-patch data now automatically apply `SpriteImageMode::Sliced` / `NodeImageMode::Sliced` to Sprite and ImageNode targets. Use `nine_patch_to_slicer` for custom overrides.
- **z-order layer priority**: layers are stored as `Vec<LayerEntry>` in front-to-back order. Reorder layers per-entity via `AseTexture::layer_order` / `reorder_layer`, or globally via `Aseprite::reorder_layer`.
- optimized slice rendering.
- observer-based child spawning (fixes 1-frame sprite lag).
- layer switching uses visibility toggling instead of entity churn.

## 0.6.1

- feature gated 3d rendering to a "3d" feature flag.

## 0.6

- removed `AseAnimation` trait.
- new `AseAnimation` component instead of `AseSpriteAnimation` and `AseUiAnimation` which renders its animation onto any component which implements `RenderAnimation`.
- new `AseSlice` component instead of `AseSpriteAnimation` and `AseUiAnimation` which renders its slices onto any component which implements `RenderSlice`.
- implementation of `RenderAnimation` and `RenderSlice` for `Sprite` and `ImageNode`. So now, instead of using an `AseSpriteAnimation` component, use an `AseAmination` component and a `Sprite` component (see the animation example).
- implementations of `RenderAnimation` and `RenderSlice` for `MeshMaterial2d` and `MeshMaterial3d` for any `Material2d` or `Material` that also implements `RenderAnimation` or `RenderSlice`. So now, implement `RenderAnimation` for your material and add the `render_animation::<MeshMaterial2d<MyMaterial>>` system (see the shader and 3d examples).
- removed requirement for materials to be components.

## 0.5

- new asset processing feature. compile your aseprite sourefile for shipping. Comes with an example.
- new shader example. Render animations to any custom material.
- updated to bevy 0.16

## 0.4.1

- fixed queue system, added example

## 0.4.0

- fixed speed multiplier
- (internal) decoupled next frame logic
- new manual example
- new `NextFrameEvent` to progress animations with custom logic.

## 0.3.3

- new animation now correctly start at the tag start frame.

## 0.3.2

- replaced `basic-universl` with `png` feature.

## 0.3.1

- changing the slice component now updates the sprite/ui.

## 0.3.0

- updated to bevy 0.15
- changed plugin name to `AsepriteUltraPlugin`.
- removed bundles, switched to required components.
- added `ManualTick` component. Let's you update the animation state following you own logic.
- added `FrameChangedEvent`. Triggering it on an entity ensures a frame re-render. (has to be called manual if in manual control mode).
- replaced `anyhow` with `thiserror`.

## 0.2.4

- aseprite slice component can now be changed at runtime.
- increased max size atlas.

## 0.2.3

- non existing animation tags no longer panic, instead default back to play the whole animation file.

---
