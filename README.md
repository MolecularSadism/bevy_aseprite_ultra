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

    // Sprite animation
    cmd.spawn(AseAnimation::sprite(
        Animation::tag("walk-right"),
        server.load("player.aseprite"),
    ));

    // UI animation
    cmd.spawn(AseAnimation::ui(
        Animation::tag("walk-right"),
        server.load("player.aseprite"),
    ));

    // Static sprite from a named slice
    cmd.spawn(AseSlice::sprite(
        server.load("icons.aseprite"),
        "ghost_red",
    ));

    // Static UI slice
    cmd.spawn(AseSlice::ui(
        server.load("icons.aseprite"),
        "ghost_red",
    ));
}
```

## Animations

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn((
    AseAnimation {
        aseprite: server.load("player.aseprite"),
        animation: Animation::tag("walk-right")
            .with_speed(2.0)
            .with_repeat(AnimationRepeat::Count(3))
            .with_direction(AnimationDirection::PingPong)
            // chain animations — loop animations never finish
            .with_then("idle", AnimationRepeat::Loop),
    },
    Sprite {
        flip_x: true,
        ..default()
    },
));
# }
```

### Animation Events

Listen for one-shot animation completions or loop cycles:

```rust,no_run
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

## Slices

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
cmd.spawn(AseSlice::sprite(
    server.load("icons.aseprite"),
    "ghost_red",
));
# }
```

## Layered Rendering

Spawn per-layer children with `AseLayeredAnimation` or `AseLayeredSlice`.
Each layer becomes a separate child entity with its own `Sprite` (or `ImageNode`),
allowing independent visibility control and composition.

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
// All visible layers as separate children
cmd.spawn(AseLayeredAnimation {
    animation: Animation::tag("idle"),
    aseprite: server.load("character.aseprite"),
    layers: LayerFilter::Visible,
    render_target: RenderTarget::Sprite,
});

// Only specific layers
cmd.spawn(AseLayeredAnimation {
    animation: Animation::tag("idle"),
    aseprite: server.load("character.aseprite"),
    layers: LayerFilter::Include(vec![
        LayerId::new("body"),
        LayerId::new("armor"),
    ]),
    render_target: RenderTarget::Sprite,
});

// Layered slices work the same way
cmd.spawn(AseLayeredSlice {
    name: "ghost_red".into(),
    aseprite: server.load("icons.aseprite"),
    layers: LayerFilter::All,
    render_target: RenderTarget::Sprite,
});
# }
```

### Runtime Layer Control

Mutating the `layers` field at runtime diffs existing children — only
changed layers are spawned or despawned. You can also toggle individual
layer visibility via the `Visibility` component on child entities:

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
fn toggle_armor(
    mut query: Query<&SpriteLayers>,
    children_query: Query<(&LayerId, &mut Visibility)>,
) {
    // each child has a LayerId you can match against
}
```

### Sub-Asset Labels

Load specific layer composites directly via asset labels:

- `"file.aseprite"` — all visible layers composited (default)
- `"file.aseprite#all"` — all layers including hidden ones
- `"file.aseprite#Layer Name"` — a single named layer

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(server: Res<AssetServer>) {
let visible: Handle<Aseprite> = server.load("player.aseprite");
let all: Handle<Aseprite> = server.load("player.aseprite#all");
let body: Handle<Aseprite> = server.load("player.aseprite#body");
# }
```

## Bevy UI

Use animations and slices in Bevy UI with `ImageNode`:

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
# fn example(mut cmd: Commands, server: Res<AssetServer>) {
// UI animation
cmd.spawn((
    Button,
    ImageNode::default(),
    AseAnimation {
        aseprite: server.load("player.aseprite"),
        animation: Animation::tag("walk-right"),
    },
));

// UI slice
cmd.spawn((
    Node {
        width: Val::Px(100.),
        height: Val::Px(100.),
        ..default()
    },
    ImageNode::default(),
    AseSlice {
        name: "ghost_red".into(),
        aseprite: server.load("icons.aseprite"),
    },
));

// Layered UI animation
cmd.spawn(AseLayeredAnimation {
    animation: Animation::tag("idle"),
    aseprite: server.load("character.aseprite"),
    layers: LayerFilter::Visible,
    render_target: RenderTarget::Ui,
});
# }
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

Enable asset processing for production builds:

```rust,no_run
# use bevy::prelude::*;
# use bevy_aseprite_ultra::prelude::*;
App::new()
    .add_plugins(DefaultPlugins.set(AssetPlugin {
        mode: AssetMode::Processed,
        ..Default::default()
    }))
    .add_plugins(AsepriteUltraPlugin)
    .run();
```

Run with `cargo run --features asset_processing`.
