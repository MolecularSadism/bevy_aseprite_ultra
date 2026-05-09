# Plan: Normal Maps for `bevy_aseprite_ultra`

## Goal

Add normal-map support to layered aseprite sprites without doubling child
entity count and without sacrificing the runtime flexibility the crate is
built around (per-layer flip, visibility, reorder, and per-layer animation
ticking).

The library will pack a parallel **normal atlas** that shares the existing
`TextureAtlasLayout` with the color atlas, expose an
`AseLitMaterial` that samples both, and let the user wire their own lighting
pipeline on top.

## Authoring conventions

Two conventions are supported. The artist picks per file; the loader can
combine them in one file if needed.

### B. Layer suffix `.normal` — DEFAULT

Inside a single `.aseprite` file, a layer named `<layer>.normal` is the
normal-map companion to the layer named `<layer>`. Example:

```
body
body.normal
hat
hat.normal
shirt          (no normal — gets the flat-blue default)
```

This is the default because it keeps color and normal in one file (one save,
one hot-reload, frame ranges and tags trivially in sync).

Rules:

- A `.normal` layer is paired with the layer whose name matches the prefix.
- A `.normal` layer with no matching prefix is loaded as a regular layer and
  a warning is logged.
- `.normal` layers are **excluded** from every non-lit code path:
  - the default composite (`LayerSelection::Visible` and the user-supplied
    `visible_layers` filter both have `.normal` filtered out *before* selection).
  - the `"all"` composite sub-asset.
  - the per-layer sub-asset list and `Aseprite::layers` — these enumerate
    only color layers. The artist authoring 12 color layers + 12 normal
    layers should still see 12 entries in `Aseprite::layers`, 12 children
    spawned, etc.
  - layer-name selection (`with_layers(LayerFilter::Include(...))`) refers
    only to color layer ids; `.normal` ids cannot be referenced.

### A. Sibling file `file.normal.ase` / `file.normal.aseprite`

For projects that prefer separate files, the loader accepts a sibling file
named `<stem>.normal.<ase|aseprite>` next to `<stem>.<ase|aseprite>`.

- Same canvas size, frame count, and layer set as the color file.
- Layer names in the sibling file match the color file's layer names exactly
  (no `.normal` suffix needed inside the sibling file).
- If both conventions are present in the same project, B (in-file
  `layer.normal`) takes precedence per layer; A fills in any layer that has
  no in-file companion.

## Loader API

### `AsepriteLoaderSettings` additions

```rust
pub struct AsepriteLoaderSettings {
    pub sampler: ImageSampler,
    pub visible_layers: Option<Vec<String>>,
    /// Normal-map source. Default: `Auto` — pick up `layer.normal` companion
    /// layers (B) and, if present, also load a sibling `<stem>.normal.<ext>`
    /// file (A). `Disabled` skips normal-map work entirely.
    pub normal_map: NormalMapMode,
}

pub enum NormalMapMode {
    /// Default. Use `layer.normal` companions and pick up a sibling
    /// `<stem>.normal.<ase|aseprite>` if it exists. No error if neither is
    /// present — the asset just has `normal_atlas_image: None`.
    Auto,
    /// Only use in-file `layer.normal` companions.
    LayerSuffixOnly,
    /// Only use a sibling `<stem>.normal.<ext>` file. Fails to load if
    /// missing.
    SiblingOnly,
    /// Disable normal-map loading even if companion data exists.
    Disabled,
}
```

`Auto` is the default so that the common case ("artist added some
`.normal` layers") just works without touching the `.meta`.

### `Aseprite` additions

```rust
pub struct Aseprite {
    // ... existing fields unchanged ...
    /// Optional second atlas. Identical extent and packing to `atlas_image`,
    /// indexed by the same `atlas_layout` and `frame_indicies`. Pixels for
    /// frames that have no normal-map source are flat blue (128, 128, 255, 255).
    pub normal_atlas_image: Option<Handle<Image>>,
}
```

No other public field changes. `frame_indicies`, `atlas_layout`, slice
metadata, per-layer sub-assets, tags, frame durations all stay valid for
both atlases simultaneously — that is the whole point of the design.

### Loader behaviour

1. Parse the file. Partition layers into:
   - `color_layers`: layers whose name does **not** end in `.normal`.
   - `normal_layers`: layers whose name ends in `.normal`. Build a map
     `prefix -> normal_layer` for lookup.
   Layers reported back through `Aseprite::layers` and per-layer sub-assets
   come exclusively from `color_layers`.
2. Render color frames exactly as today, but using only `color_layers` for
   every selection (`Visible`, `All`, per-layer, `visible_layers` setting).
3. Build the color atlas (`TextureAtlasBuilder`) — unchanged.
4. If a normal source is in scope (per `NormalMapMode`), allocate a second
   `Image` of the same `Extent3d` as the color atlas, default-filled with
   flat blue. For each color frame already packed at `layout.textures[i]`:
   - Resolve the matching normal source (in-file companion preferred,
     sibling file as fallback).
   - Render that normal frame at canvas size and `memcpy` it row-by-row
     into the second image at the rect the color frame occupies.
   - If no source exists for that frame/layer, leave the rect flat blue.
5. Add `normal_atlas_texture` as a labeled sub-asset; propagate the handle
   into the main asset and every labeled sub-asset (`"all"`, per-layer).

The sibling file is loaded via `load_context.read_asset_bytes` so hot-reload
of either file invalidates the parent asset.

#### Pairing precedence per layer

For a given color layer `L`:
1. If file contains `L.normal`, use it.
2. Else if a sibling file exists and contains a layer named `L`, use that.
3. Else flat blue.

Mismatched canvas size or frame count between sibling and main file: warn
and skip the sibling entirely (in-file companions still apply).

## Render API

### `AseLitMaterial`

A library-exposed `Material2d` the user instantiates lighting around. The
crate does **not** ship a lighting pass — the user is expected to provide
lights, attenuation, and any post-processing. The material outputs sampled
albedo, world-/view-space normal (with flip applied), and the user's tint;
how that gets shaded is up to the user's pipeline.

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct AseLitMaterial {
    #[texture(0)] #[sampler(1)] pub color: Handle<Image>,
    #[texture(2)]                pub normal: Handle<Image>,
    #[uniform(3)]                pub params: AseLitParams,
}

#[derive(ShaderType, Clone, Copy, Default)]
pub struct AseLitParams {
    /// Atlas rect in **uv space** (xy = min, zw = size).
    pub uv_rect: Vec4,
    /// {-1, 1} per axis. Mirrors UVs *and* multiplies normal.xy in the shader.
    pub flip: Vec2,
    /// Multiplied into sampled color.
    pub tint: LinearRgba,
}
```

The bundled WGSL:

- Computes `uv = uv_rect.xy + uv_rect.zw * (flip_aware_local_uv)`.
- Samples color, multiplies tint.
- Samples normal as `n = normal_sample.xyz * 2.0 - 1.0`, then
  `n.xy *= flip` (this is the bit a custom material is mandatory for —
  a sprite-flip that doesn't sign-flip the tangent-space normal renders
  wrong-handed lighting).
- Outputs both via a `MaterialPipeline`-compatible struct the user can
  plug into their lit forward pass, OR writes directly to a G-buffer-style
  MRT if the user enables that feature flag (out of scope for v1; v1
  outputs a single `vec4` per the user's chosen channel layout).

The crate registers the material with `Material2dPlugin::<AseLitMaterial>`
inside `AsepriteUltraPlugin` (gated behind a `lit` cargo feature so users
who don't need it pay nothing).

### Opt-in on `AseTexture`

```rust
impl AseTexture {
    /// When true, layer children are spawned as `Mesh2d` + `AseLitMaterial`
    /// instead of `Sprite`. No-op if the asset has no `normal_atlas_image`.
    pub fn lit(mut self) -> Self { self.lit = true; self }
}
```

```rust
cmd.spawn((
    AseTexture::new(server.load("player.aseprite")).sprite().lit(),
    AseAnimation::tag("walk"),
    AseFlip::default(),
));
```

UI render target (`ImageNode`) and 9-slice (`SpriteImageMode::Sliced`) do
not support custom materials in this v1; they fall back to unlit and log
a one-shot warning.

### Spawning

`spawn_layered_children` branches on
`tex.lit && aseprite.normal_atlas_image.is_some() && tex.render_target == Sprite && tex.slice.is_none()`:

- **Lit branch**: spawn `Mesh2d(unit_quad)` + `MeshMaterial2d<AseLitMaterial>`.
  Material handle is unique per child so animation/flip writes do not
  clobber other entities. Quad mesh is shared (registered once at plugin
  init, sized to 1×1 and scaled via `Transform`).
  Same `Transform`, `Visibility`, `ChildOf`, `SpriteLayerOf`, `LayerId`,
  `AppliedOffset`, `AnimationLayer` components as today.
- **Unlit branch**: existing `Sprite` path — no behaviour change.

### System adaptations

- `propagate_flip`: for lit children, write
  `material.params.flip = vec2(if flip.x {-1.0} else {1.0}, if flip.y {-1.0} else {1.0})`
  in addition to the existing `Sprite` write (one query, one branch).
- `update_layers`: visibility (`Visibility::Hidden`/`Inherited`) and z
  (Transform.z) work identically — no change.
- Animation frame tick (`render_children_animation`): for lit children,
  recompute `params.uv_rect` from `atlas_layout.textures[new_index]` and
  the atlas image extent; for unlit, write `sprite.texture_atlas.index`
  as today.

`AseSlice` reconciliation in `render_slice` follows the same pattern.

## Edge cases & validation

- **`.normal` layer with no matching prefix**: log warning at load, treat
  as regular color layer (so the artist sees their pixels and notices the
  typo).
- **Color layer with no `.normal` companion**: that layer's atlas region in
  `normal_atlas_image` stays flat blue → unlit-looking shading. Not an
  error.
- **Sibling file present but mismatched (size / frame count)**: warn,
  ignore sibling entirely. In-file companions still apply.
- **`NormalMapMode::SiblingOnly` and sibling missing**: load error.
- **Asset processing feature**: `normal_atlas_image` is `#[serde(skip)]`
  like `atlas_image`; the processed-asset path packs and serialises the
  second image alongside the first.
- **Hot reload**: sibling file tracked via `load_context.read_asset_bytes`;
  in-file `.normal` layers are tracked automatically since they're inside
  the parent file.
- **Layer-id collisions**: with `.normal` layers stripped from
  `Aseprite::layers`, no `LayerId` ever has a `.normal` suffix. The
  interner never sees them as user-visible ids.

## Cargo feature

Add `lit` (default off) gating:
- `AseLitMaterial`, the WGSL, `Material2dPlugin` registration.
- The lit branch in `spawn_layered_children` and the per-child material
  writes in `propagate_flip` / animation tick.

The loader builds `normal_atlas_image` regardless of feature (tiny work for
files that have no normal data; for files that do, the atlas is small
overhead and can be inspected by user code).

## Migration

Zero changes for existing users. `.normal` layers are filtered out of every
existing path, so a user who happens to add `.normal` layers to an existing
file before enabling the `lit` feature sees no behaviour change beyond
"those layers stop rendering" (which is the intended outcome — they're not
color anymore).

## Open scope (v2, not in this plan)

- Lit 9-slice (`SpriteImageMode::Sliced`) — needs a sliced quad mesh builder.
- Lit `ImageNode` UI — Bevy UI doesn't support custom materials directly;
  would require a Mesh2d-overlay or a UI material extension.
- Multi-render-target output (G-buffer style) for deferred 2D lighting.

## Work breakdown

1. Loader: layer partition + `.normal` filtering across all selections;
   second `Image` allocation; copy step; sibling-file reader; settings enum.
2. `Aseprite::normal_atlas_image` + propagation into all sub-assets.
3. `lit` cargo feature scaffolding.
4. `AseLitMaterial` + WGSL, exposed via `prelude`.
5. `AseTexture::lit()` + lit branch in `spawn_layered_children`.
6. System adaptations: `propagate_flip`, animation tick, `render_slice`
   awareness (slice + lit warns and falls back).
7. New example `examples/normal_maps.rs` showing user-side lighting wiring.
8. Tests: layer partitioning, `.normal` filtering, atlas-rect equality
   between color and normal images, sibling-file fallback precedence.
