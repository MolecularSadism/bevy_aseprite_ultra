# Bevy Aseprite Ultra

[![License: MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache2-blue.svg)](./LICENSE)
[![Crate](https://img.shields.io/crates/v/bevy_aseprite_ultra.svg)](https://crates.io/crates/bevy_aseprite_ultra)

The ultimate bevy aseprite plugin. Import aseprite files directly into bevy with
100% unbreakable hot reloading. Supports animations, static slices, per-layer
rendering, and custom materials.

| Bevy Version | Plugin Version |
| -----------: | -------------: |
|         0.18 |          0.8.1 |
|         0.17 |          0.7.0 |
|         0.16 |          0.6.1 |
|         0.15 |          0.4.1 |
|         0.14 |          0.2.4 |
|         0.13 |          0.1.0 |

## Supported Aseprite Features

- Animations with tags
- Frame duration, repeat count, and animation direction
- Layer visibility and blend modes
- Static slices with pivot offsets and 9-patch data
- Per-layer sub-asset loading

## Bevy Features

- Hot reload anything, anytime (requires `file_watcher` feature in bevy)
- Component-driven animation control with builder API
- One-shot animations with finish events
- Static sprites via slices — use aseprite for icons, UI, and more
- **Layered rendering** — spawn per-layer children for full composition control
- **Baked rendering** — single composite child for simpler use cases
- Render to custom materials and write shaders on top
- Optional asset processor for production builds

## Quick Start

```rust,no_run
use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: bevy::image::ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn(Camera2d);

    // Animated sprite (layered — one child per visible layer)
    cmd.spawn((
        AseTexture::new(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("walk-right"),
    ));

    // Animated sprite (baked — single composite child)
    cmd.spawn((
        AseTexture::baked(server.load("player.aseprite")).sprite(),
        AseAnimation::tag("idle"),
    ));

    // Static slice (no animation component needed)
    cmd.spawn(
        AseTexture::baked(server.load("icons.aseprite"))
            .with_slice("ghost_red")
            .sprite(),
    );
}
```

## Core Concepts

`AseTexture` is the primary component. It always spawns child entities for
rendering — the parent entity itself does not render. Two modes are available:

- **Layered** (`AseTexture::new`) — one child per layer, for full composition control
- **Baked** (`AseTexture::baked`) — single composite child, simpler and cheaper

Add `AseAnimation` alongside `AseTexture` to enable animation. Without it,
children are fully static with zero per-tick overhead.

## Entity Hierarchy

`AseTexture` uses a parent-child model built on Bevy's relationship system.
The parent entity holds `AseTexture` (and optionally `AseAnimation`) but never
renders directly. Instead, the plugin spawns child entities that carry the
actual render components (`Sprite` or `ImageNode`).

```text
[Parent Entity]
  ├─ AseTexture         (config: asset handle, mode, layers, slice, render target)
  ├─ AseAnimation       (optional: tag, speed, repeat, direction, queue)
  ├─ AnimationState     (auto-added when AseAnimation is present)
  ├─ AseFlip            (optional: propagated to children)
  ├─ SpriteLayers       (auto-populated: tracks child entities)
  │
  ├── [Child: "body"]
  │     ├─ SpriteLayerOf(parent)   ── relationship back to parent
  │     ├─ LayerId("body")         ── type-safe interned layer name
  │     ├─ AnimationLayer          ── per-layer asset handle
  │     ├─ AnimationState          ── frame state (propagated from parent)
  │     ├─ Sprite / ImageNode      ── render component
  │     └─ AseSlice                ── (if slice configured)
  │
  └── [Child: "armor"]
        ├─ SpriteLayerOf(parent)
        ├─ LayerId("armor")
        └─ ...
```

### Relationships

The hierarchy is wired via a custom Bevy relationship pair:

- **`SpriteLayerOf(Entity)`** — placed on each child, points back to the parent
- **`SpriteLayers`** — auto-populated on the parent, collects all child entities

This lets you query children through the parent or find the parent from any child:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
// Query children from parent
fn read_layers(query: Query<&SpriteLayers>) {
    for layers in &query {
        for child_entity in layers.iter() {
            // ...
        }
    }
}

// Query parent from child
fn find_parent(query: Query<&SpriteLayerOf>) {
    for layer_of in &query {
        let parent_entity = layer_of.0;
        // ...
    }
}
```

### Baked vs Layered Children

In **baked** mode, a single child named `"baked"` is spawned. It uses the
composite asset handle (all visible layers flattened into one texture).

In **layered** mode, one child per matching layer is spawned. Each child loads
its own sub-asset (`"file.aseprite#Layer Name"`) and gets a `LayerId` component.
Sprite children are z-ordered via small `Transform` offsets (`z * 0.001`).
UI children use `ZIndex` instead.

### Frame Propagation

Animation ticking runs once on the parent entity. The resulting `AnimationState`
is then propagated to all children via the `propagate_frame` system — children
never tick independently. This means you query and control animation on the
parent, while children just reflect the current frame.

## Animations

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn((
    AseTexture::baked(server.load("player.aseprite")).sprite(),
    AseAnimation::tag("walk-right")
        .with_speed(2.0)
        .with_repeat(AnimationRepeat::Count(3))
        .with_direction(AnimationDirection::PingPong)
        // chain animations — loop animations never finish
        .with_then("idle", AnimationRepeat::Loop),
));
# }
```

### Imperative Animation Control

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
fn switch_animation(mut query: Query<&mut AseAnimation>) {
    for mut anim in &mut query {
        anim.play_loop("run");       // start looping immediately
        // anim.play("attack", AnimationRepeat::Count(1));
        // anim.then("idle", AnimationRepeat::Loop);  // queue next
        // anim.pause();
        // anim.start();
        // anim.stop();
    }
}
```

### Animation Events

Listen for one-shot animation completions or loop cycles:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
fn despawn_on_finish(mut events: MessageReader<AnimationEvents>, mut cmd: Commands) {
    for event in events.read() {
        match event {
            AnimationEvents::Finished(entity) => {
                cmd.entity(*entity).despawn();
            }
            AnimationEvents::LoopCycleFinished(_) => {}
        }
    }
}
```

### Manual Frame Control

Disable automatic ticking with `ManualTick` and advance frames yourself:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn((
    AseTexture::baked(server.load("player.aseprite")).sprite(),
    AseAnimation::tag("walk-right"),
    ManualTick,
));
# }

# fn advance(mut cmd: Commands, query: Query<Entity, With<ManualTick>>) {
// Trigger to advance one frame:
for entity in &query {
    cmd.trigger(NextFrameEvent(entity));
}
# }
```

## Slices

Static sprite regions from named slices. Supports pivot offsets and 9-patch data.

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn(
    AseTexture::baked(server.load("icons.aseprite"))
        .with_slice("ghost_red")
        .sprite(),
);
# }
```

### Runtime Slice Switching

Mutate `AseTexture` to switch slices at runtime:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
fn cycle_slices(mut query: Query<&mut AseTexture>) {
    for mut tex in &mut query {
        tex.slice = Some(SliceId::new("ghost_blue"));
    }
}
```

## Layered Rendering

Use `AseTexture::new` for per-layer children. Each layer becomes a separate
child entity with its own `Sprite` (or `ImageNode`), allowing independent
visibility control and composition.

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
// All visible layers as separate children (default)
cmd.spawn((
    AseTexture::new(server.load("character.aseprite")).sprite(),
    AseAnimation::tag("idle"),
));

// Only specific layers
cmd.spawn((
    AseTexture::new(server.load("character.aseprite"))
        .with_layers(LayerFilter::Include(vec![
            LayerId::new("body"),
            LayerId::new("armor"),
        ]))
        .sprite(),
    AseAnimation::tag("idle"),
));

// Layered slices work the same way
cmd.spawn(
    AseTexture::new(server.load("icons.aseprite"))
        .with_layers(LayerFilter::All)
        .with_slice("ghost_red")
        .sprite(),
);
# }
```

### Runtime Layer Control

Mutating the `layers` field at runtime diffs existing children — only
changed layers are spawned or despawned. You can also toggle individual
layer visibility via the `Visibility` component on child entities:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
fn toggle_armor(
    query: Query<&SpriteLayers>,
    children_query: Query<(&LayerId, &mut Visibility)>,
) {
    // each child has a LayerId you can match against
}
```

### Flipping

Use `AseFlip` to flip all child render entities. The flip state propagates
to all children's `Sprite` or `ImageNode` components automatically:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn((
    AseTexture::baked(server.load("player.aseprite")).sprite(),
    AseAnimation::tag("walk-right"),
    AseFlip { x: true, y: false },
));
# }
```

### Sub-Asset Labels

Load specific layer composites directly via asset labels:

- `"file.aseprite"` — all visible layers composited (default)
- `"file.aseprite#all"` — all layers including hidden ones
- `"file.aseprite#Layer Name"` — a single named layer

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(server: Res<AssetServer>) {
let visible: Handle<Aseprite> = server.load("player.aseprite");
let all: Handle<Aseprite> = server.load("player.aseprite#all");
let body: Handle<Aseprite> = server.load("player.aseprite#body");
# }
```

## Bevy UI

Use `.ui()` instead of `.sprite()` to render as `ImageNode` in Bevy UI:

```rust
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
// UI animation
cmd.spawn((
    Node {
        width: Val::Px(100.),
        height: Val::Px(100.),
        ..default()
    },
    AseTexture::baked(server.load("player.aseprite")).ui(),
    AseAnimation::tag("walk-right"),
));

// UI slice
cmd.spawn((
    Node {
        width: Val::Px(100.),
        height: Val::Px(100.),
        ..default()
    },
    AseTexture::baked(server.load("icons.aseprite"))
        .with_slice("ghost_red")
        .ui(),
));

// Layered UI animation
cmd.spawn((
    Node {
        width: Val::Px(100.),
        height: Val::Px(100.),
        ..default()
    },
    AseTexture::new(server.load("character.aseprite")).ui(),
    AseAnimation::tag("idle"),
));
# }
```

## Custom Materials

Implement `RenderAnimation` or `RenderSlice` on your material to drive
custom shaders with aseprite data:

```rust,ignore
impl RenderAnimation for MyMaterial {
    type Extra<'e> = (Res<'e, Time>, Res<'e, Assets<TextureAtlasLayout>>);
    fn render_animation(
        &mut self,
        aseprite: &Aseprite,
        state: &AnimationState,
        extra: &mut Self::Extra<'_>,
    ) {
        // custom rendering logic
    }
}
```

## Examples

```bash
cargo run --example animation
cargo run --example slices
cargo run --example layers
cargo run --example animated_layers
cargo run --example sliced_layers
cargo run --example ui_layers
cargo run --example ui
cargo run --example move_player
cargo run --example manual
cargo run --example queue
cargo run --example shader
cargo run --example 3d --features 3d
cargo run --example asset_processing --features asset_processing
```

![Example](docs/example.gif)

<small> character animation by [Benjamin](https://github.com/headcr4sh) </small>

## Asset Processing

Simply enable asset processing in your `AssetPlugin` like so:

```rust,no_run
use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            mode: AssetMode::Processed,
            ..Default::default()
        }))
        .add_plugins(AsepriteUltraPlugin)
        .run();
}
```

Then run with the `asset_processing` feature enabled:

```bash
cargo run --features asset_processing
```

Then load your aseprite files in code as usual!
