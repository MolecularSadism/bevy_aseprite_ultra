## 0.11.0

Breaking. The crate stops panicking on art it does not like, stops failing
silently where a warning would save an artist an afternoon, and names its
types the way an ECS crate should.

### The crate no longer takes the process down

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
