//! What rendering a slice costs per entity, per pass.
//!
//! `render_slice` runs over every slice entity in the world whenever the
//! aseprite assets change, which any load flips. What it hands the render
//! target is one frame's geometry, so the pass has no reason to touch the
//! heap — and a per-entity allocation here is one every slice in the scene
//! pays on a frame that already has an asset landing on it.

use bevy::{asset::AssetPlugin, prelude::*};
use bevy_aseprite_ultra::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

/// The system allocator, counting the allocations that pass through it.
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A two-frame sheet whose slice carries a timeline: a key on frame 1, and one
/// atlas position per frame. Both are lists, which is what a whole-struct copy
/// of the slice would have to duplicate.
fn sheet() -> Aseprite {
    Aseprite::builder()
        .with_slice_meta(
            "Panel",
            SliceMeta {
                rect: Rect::new(0.0, 0.0, 8.0, 8.0),
                atlas_id: 0,
                pivot: None,
                nine_patch: None,
                keys: vec![SliceKeyMeta {
                    frame: 1,
                    rect: Rect::new(0.0, 0.0, 4.0, 4.0),
                    pivot: None,
                    nine_patch: None,
                }],
                frame_atlas_ids: vec![0, 1],
            },
        )
        .with_frame_indices([0, 1])
        .with_frame_durations([std::time::Duration::from_millis(100); 2])
        .build()
}

/// An app drawing `entities` sprites off the same slice, run once so every
/// first-pass allocation is already behind it.
fn scene(entities: usize) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), AsepriteUltraPlugin));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();

    let handle = app
        .world_mut()
        .resource_mut::<Assets<Aseprite>>()
        .add(sheet());
    for _ in 0..entities {
        app.world_mut().spawn((
            Sprite::default(),
            AseSlice::new(handle.clone(), "Panel"),
            AseFrame(0),
        ));
    }
    app.update();
    app
}

/// The cheapest pass the app can manage with the assets marked changed, which
/// is what a landing asset does to every slice entity at once.
fn allocations_per_pass(app: &mut App) -> usize {
    (0..4)
        .map(|_| {
            app.world_mut()
                .resource_mut::<Assets<Aseprite>>()
                .set_changed();
            let before = ALLOCATIONS.load(Ordering::Relaxed);
            app.update();
            ALLOCATIONS.load(Ordering::Relaxed) - before
        })
        .min()
        .expect("four passes")
}

#[test]
fn rendering_a_slice_does_not_allocate_per_entity() {
    const FEW: usize = 16;
    const MANY: usize = 272;

    let few = allocations_per_pass(&mut scene(FEW));
    let many = allocations_per_pass(&mut scene(MANY));

    let per_entity = (many.saturating_sub(few)) as f64 / (MANY - FEW) as f64;
    assert!(
        per_entity < 0.5,
        "{per_entity} allocations per slice entity per pass \
         ({few} for {FEW} entities, {many} for {MANY})",
    );
}
