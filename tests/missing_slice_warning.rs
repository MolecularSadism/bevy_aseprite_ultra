//! What a missing slice logs, and how often.
//!
//! An artist typo in a slice name renders nothing, so the warning is the only
//! trace of it. Deduplicating per call site — one warning per system for the
//! life of the process — let the first typo anywhere swallow every other one:
//! a second entity naming a different missing slice, or the same name against
//! a different sheet, failed in silence. Each distinct (sheet, slice) miss
//! warns exactly once.

use bevy::log::{tracing, tracing_subscriber};
use bevy::{asset::AssetPlugin, prelude::*};
use bevy_aseprite_ultra::prelude::*;
use std::sync::{Mutex, Once};
use tracing_subscriber::layer::SubscriberExt;

static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Collects every warning logged anywhere in the process into [`WARNINGS`].
///
/// The systems under test may run on task-pool threads, where a thread-local
/// subscriber would not be seen, so this installs globally — once, shared by
/// every test in this binary. Assertions filter by slice name instead of
/// counting the whole log.
struct CaptureWarnings;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureWarnings {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        struct Message(Option<String>);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = Some(format!("{value:?}"));
                }
            }
        }
        let mut message = Message(None);
        event.record(&mut message);
        if let Some(text) = message.0 {
            WARNINGS.lock().unwrap().push(text);
        }
    }
}

fn install_collector() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureWarnings),
        )
        .expect("no other global subscriber in the test binary");
    });
}

/// Warnings mentioning the missing slice `name` so far.
fn warned(name: &str) -> usize {
    WARNINGS
        .lock()
        .unwrap()
        .iter()
        .filter(|message| message.contains("does not exist") && message.contains(name))
        .count()
}

/// A sheet with one real slice, so a miss has something to list as available.
fn sheet() -> Aseprite {
    Aseprite::builder()
        .with_slice_meta(
            "Panel",
            SliceMeta {
                rect: Rect::new(0.0, 0.0, 8.0, 8.0),
                atlas_id: 0,
                pivot: None,
                nine_patch: None,
                keys: Vec::new(),
                frame_atlas_ids: vec![0],
            },
        )
        .with_frame_indices([0])
        .with_frame_durations([std::time::Duration::from_millis(100)])
        .build()
}

#[test]
fn each_distinct_sheet_and_slice_miss_warns_exactly_once() {
    install_collector();

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), AsepriteUltraPlugin));
    app.init_asset::<Image>();
    app.init_asset::<TextureAtlasLayout>();

    let (first, second) = {
        let mut aseprites = app.world_mut().resource_mut::<Assets<Aseprite>>();
        (aseprites.add(sheet()), aseprites.add(sheet()))
    };
    // Two different missing slices on one sheet, and one of the same names
    // against another sheet: three distinct misses, three warnings.
    app.world_mut()
        .spawn((Sprite::default(), AseSlice::new(first.clone(), "MissAlpha")));
    app.world_mut()
        .spawn((Sprite::default(), AseSlice::new(first.clone(), "MissBeta")));
    app.world_mut().spawn((
        Sprite::default(),
        AseSlice::new(second.clone(), "MissAlpha"),
    ));
    app.update();

    assert_eq!(
        warned("MissAlpha"),
        2,
        "the same missing name warns once per sheet it misses on",
    );
    assert_eq!(
        warned("MissBeta"),
        1,
        "a second missing slice is not swallowed by the first",
    );

    // Re-rendering passes — which a landing asset forces on every slice
    // entity at once — repeat the misses, not the warnings.
    for _ in 0..3 {
        app.world_mut()
            .resource_mut::<Assets<Aseprite>>()
            .set_changed();
        app.update();
    }

    assert_eq!(warned("MissAlpha"), 2, "a repeated miss stays warned");
    assert_eq!(warned("MissBeta"), 1);
}
