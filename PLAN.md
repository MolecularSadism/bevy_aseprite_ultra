# 9-Slicing Implementation Plan

## Current State

**9-patch data IS already extracted** from aseprite files by the loader (`src/loader.rs`):
- `SliceMeta.nine_patch: Option<Vec4>` — stores `(x, y, width, height)` of the center rectangle
- `SliceKeyMeta.nine_patch: Option<Vec4>` — per-frame animated 9-patch data
- The `aseprite-loader` crate already provides this data; **no fork needed**

**9-patch data is NOT applied** to any render target. The `RenderSlice` implementations for `Sprite` and `ImageNode` set `image` + `texture_atlas` but never configure `SpriteImageMode::Sliced` or `NodeImageMode::Sliced`.

## Conversion: Aseprite → Bevy

Aseprite stores 9-patch as center rectangle: `Vec4(x, y, width, height)` relative to the slice rect.
Bevy uses `BorderRect { min_inset: Vec2, max_inset: Vec2 }` (left/top, right/bottom border widths).

Conversion:
```
let slice_size = slice_meta.rect.size();  // total slice dimensions
let left = nine_patch.x;
let top = nine_patch.y;
let right = slice_size.x - nine_patch.x - nine_patch.z;
let bottom = slice_size.y - nine_patch.y - nine_patch.w;
BorderRect { min_inset: Vec2::new(left, top), max_inset: Vec2::new(right, bottom) }
```

## Render Targets (4 types)

### 1. `Sprite` (2D world)
- Set `sprite.image_mode = SpriteImageMode::Sliced(TextureSlicer { border, ..default() })`
- In `RenderSlice for Sprite::render_slice()` — apply when `nine_patch.is_some()`
- In `spawn_baked_child` (RenderTarget::Sprite) — apply at spawn time when slice has nine_patch
- In `RenderAnimation for Sprite` — no change needed (animations don't use slices directly; the slice system handles it)

### 2. `ImageNode` (UI)
- Set `image_node.image_mode = NodeImageMode::Sliced(TextureSlicer { border, ..default() })`
- In `RenderSlice for ImageNode::render_slice()` — apply when `nine_patch.is_some()`
- In `spawn_baked_child` (RenderTarget::Ui) — apply at spawn time when slice has nine_patch

### 3. `MeshMaterial2d<M>` (custom 2D material)
- 9-slicing is a built-in sprite/UI feature; meshes don't support it natively
- The `nine_patch` data is already passed to `render_slice()` via `SliceMeta` — custom impls can read it
- **No changes needed** — users implementing custom materials can access `slice_meta.nine_patch`

### 4. `MeshMaterial3d<M>` / `MaterialNode<M>` (custom 3D / UI material)
- Same as above — data is available, but 9-slicing is not applicable to raw meshes
- **No changes needed**

## Implementation Steps

### Step 1: Add helper to convert nine_patch Vec4 → TextureSlicer
In `src/slice.rs` or `src/loader.rs`, add:
```rust
fn nine_patch_to_slicer(nine_patch: Vec4, slice_size: Vec2) -> TextureSlicer {
    let left = nine_patch.x;
    let top = nine_patch.y;
    let right = slice_size.x - nine_patch.x - nine_patch.z;
    let bottom = slice_size.y - nine_patch.y - nine_patch.w;
    TextureSlicer {
        border: BorderRect {
            min_inset: Vec2::new(left, top),
            max_inset: Vec2::new(right, bottom),
        },
        ..default()
    }
}
```

### Step 2: Update `RenderSlice for Sprite`
Apply `image_mode` when nine_patch is present:
```rust
impl RenderSlice for Sprite {
    fn render_slice(&mut self, aseprite: &Aseprite, slice_meta: &SliceMeta, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas { ... });
        if let Some(np) = slice_meta.nine_patch {
            self.image_mode = SpriteImageMode::Sliced(
                nine_patch_to_slicer(np, slice_meta.rect.size())
            );
        }
    }
}
```

### Step 3: Update `RenderSlice for ImageNode`
Same pattern with `NodeImageMode::Sliced`.

### Step 4: Update `render_slice` system for change detection
Currently skips if `!asset_change && !slice.is_changed()`. The AnimationState change also matters for animated nine_patch — need to also check state changes. (Already partially handled by the frame-key lookup logic.)

### Step 5: Add an example
Create `examples/nine_slice.rs` demonstrating 9-sliced sprites at different sizes.
This requires an aseprite file with 9-patch data configured (the slice examples may already have one).

## Files to Modify
1. `src/slice.rs` — core changes (helper fn + Sprite/ImageNode impls)
2. `src/layers.rs` — optionally set image_mode at spawn time in `spawn_baked_child`
3. `examples/nine_slice.rs` — new example (if test asset available)
