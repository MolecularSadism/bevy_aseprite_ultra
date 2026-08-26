//! In-memory [`Aseprite`] fixtures for tests that need sheet metadata but no
//! pixels.
//!
//! Gated behind the `testing` feature, which is off by default. This module
//! is the crate's one public surface outside
//! [`prelude`](crate::prelude): a consumer opts into it from a test target
//! rather than picking it up with everything else.

use crate::animation::AnimationDirection;
use crate::layers::{LayerEntry, LayerId};
use crate::loader::{Aseprite, SliceMeta, TagMeta};
use bevy::prelude::*;
use std::ops::RangeInclusive;
use std::time::Duration;

/// Builds an [`Aseprite`] out of slice, layer and tag metadata, with no file
/// behind it.
///
/// Only the asset loader can pair metadata with pixels, so the atlas handles
/// a fixture leaves behind are [`Handle::default`]: they address no texture,
/// and anything rendering through them draws nothing. A test asserting on
/// geometry, layer order or tag ranges wants a fixture; a test asserting on
/// rendered output wants a real file loaded through
/// [`AsepriteLoader`](crate::prelude::AsepriteLoader).
///
/// ```
/// # use bevy::prelude::*;
/// use bevy_aseprite_ultra::prelude::*;
/// use bevy_aseprite_ultra::testing::AsepriteFixture;
///
/// let aseprite = AsepriteFixture::new()
///     .with_layer("Body", true)
///     .with_layer("Hat", false)
///     .with_slice("Panel", Rect::new(0.0, 0.0, 16.0, 16.0), 0)
///     .with_tag("walk", 2..=5)
///     .build();
///
/// assert_eq!(aseprite.visible_layer_ids().count(), 1);
/// assert_eq!(aseprite.slice("Panel").map(SliceMeta::size), Some(Vec2::splat(16.0)));
/// ```
#[derive(Debug, Default, Clone)]
pub struct AsepriteFixture {
    aseprite: Aseprite,
}

impl AsepriteFixture {
    /// An empty fixture: no slices, layers, tags or frames.
    ///
    /// ```
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new().build();
    /// assert_eq!(aseprite.layer_ids().count(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a slice covering `rect` at atlas position `atlas_id`, with
    /// no pivot and no nine-patch centre.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_slice("Icon", Rect::new(2.0, 2.0, 10.0, 10.0), 3)
    ///     .build();
    ///
    /// assert_eq!(aseprite.slice("Icon").map(|slice| slice.atlas_id), Some(3));
    /// ```
    #[must_use]
    pub fn with_slice(self, name: impl Into<String>, rect: Rect, atlas_id: usize) -> Self {
        self.with_slice_meta(name, slice_meta(rect, atlas_id))
    }

    /// Registers a nine-patch slice covering `rect`, whose centre is given in
    /// Aseprite's own `Vec4(x, y, width, height)` form, relative to the slice
    /// origin. Its atlas position is `0`; [`with_slice_meta`](Self::with_slice_meta)
    /// sets another.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_nine_patch_slice(
    ///         "Frame",
    ///         Rect::new(0.0, 0.0, 12.0, 12.0),
    ///         Vec4::new(4.0, 4.0, 4.0, 4.0),
    ///     )
    ///     .build();
    ///
    /// let border = aseprite.slice("Frame").and_then(|slice| slice.border());
    /// assert_eq!(border.map(|border| border.min_inset), Some(Vec2::splat(4.0)));
    /// ```
    #[must_use]
    pub fn with_nine_patch_slice(self, name: impl Into<String>, rect: Rect, centre: Vec4) -> Self {
        self.with_slice_meta(
            name,
            SliceMeta {
                nine_patch: Some(centre),
                ..slice_meta(rect, 0)
            },
        )
    }

    /// Registers `meta` under `name`, for the pivot, keys and per-frame atlas
    /// positions the shorthands leave at their defaults.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_aseprite_ultra::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_slice_meta(
    ///         "Head",
    ///         SliceMeta {
    ///             rect: Rect::new(0.0, 0.0, 8.0, 8.0),
    ///             atlas_id: 0,
    ///             pivot: Some(Vec2::new(4.0, 8.0)),
    ///             nine_patch: None,
    ///             keys: Vec::new(),
    ///             frame_atlas_ids: vec![0, 1],
    ///         },
    ///     )
    ///     .build();
    ///
    /// assert_eq!(aseprite.slice("Head").map(|slice| slice.atlas_id_for_frame(1)), Some(1));
    /// ```
    #[must_use]
    pub fn with_slice_meta(mut self, name: impl Into<String>, meta: SliceMeta) -> Self {
        self.aseprite.slices.insert(name.into(), meta);
        self
    }

    /// Appends a layer carrying the visibility a file would record for it.
    ///
    /// Layers are stored front-to-back, so the first call is the topmost
    /// layer in the Aseprite editor.
    ///
    /// ```
    /// use bevy_aseprite_ultra::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_layer("Hat", true)
    ///     .with_layer("Body", true)
    ///     .build();
    ///
    /// assert_eq!(
    ///     aseprite.layer_ids().collect::<Vec<_>>(),
    ///     vec![LayerId::new("Hat"), LayerId::new("Body")],
    /// );
    /// ```
    #[must_use]
    pub fn with_layer(mut self, name: &str, visible: bool) -> Self {
        self.aseprite
            .layers
            .push(LayerEntry::new(LayerId::new(name), visible));
        self
    }

    /// Registers a tag spanning `range`, inclusive of both ends, played
    /// forward and repeating as the file's own `0` does — indefinitely,
    /// unless the entity overrides it.
    ///
    /// ```
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new().with_tag("idle", 0..=3).build();
    ///
    /// assert_eq!(aseprite.tag("idle").map(|tag| tag.range.clone()), Some(0..=3));
    /// ```
    #[must_use]
    pub fn with_tag(self, name: impl Into<String>, range: RangeInclusive<u16>) -> Self {
        self.with_tag_meta(
            name,
            TagMeta {
                direction: AnimationDirection::Forward,
                range,
                repeat: 0,
            },
        )
    }

    /// Registers `meta` under `name`, for the playback direction and repeat
    /// count [`with_tag`](Self::with_tag) fixes.
    ///
    /// ```
    /// use bevy_aseprite_ultra::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_tag_meta(
    ///         "swing",
    ///         TagMeta {
    ///             direction: AnimationDirection::PingPong,
    ///             range: 1..=4,
    ///             repeat: 2,
    ///         },
    ///     )
    ///     .build();
    ///
    /// assert_eq!(aseprite.tag("swing").map(|tag| tag.repeat), Some(2));
    /// ```
    #[must_use]
    pub fn with_tag_meta(mut self, name: impl Into<String>, meta: TagMeta) -> Self {
        self.aseprite.tags.insert(name.into(), meta);
        self
    }

    /// Sets how long each frame of the file is shown.
    ///
    /// ```
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    /// use std::time::Duration;
    ///
    /// let aseprite = AsepriteFixture::new()
    ///     .with_frame_durations([Duration::from_millis(100); 2])
    ///     .build();
    ///
    /// assert_eq!(aseprite.frame_durations.len(), 2);
    /// ```
    #[must_use]
    pub fn with_frame_durations(mut self, durations: impl IntoIterator<Item = Duration>) -> Self {
        self.aseprite.frame_durations = durations.into_iter().collect();
        self
    }

    /// Sets this variant's frame list: one atlas position per frame, which is
    /// what [`Aseprite::atlas_index`] reads.
    ///
    /// ```
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new().with_frame_indicies([4, 5, 6]).build();
    ///
    /// assert_eq!(aseprite.atlas_index(1), Some(5));
    /// ```
    #[must_use]
    pub fn with_frame_indicies(mut self, indicies: impl IntoIterator<Item = usize>) -> Self {
        self.aseprite.frame_indicies = indicies.into_iter().collect();
        self
    }

    /// Sets the asset path the fixture reports as its origin, which sub-asset
    /// paths are built from.
    ///
    /// ```
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let aseprite = AsepriteFixture::new().with_source_path("player.aseprite").build();
    ///
    /// assert_eq!(aseprite.source_path, "player.aseprite");
    /// ```
    #[must_use]
    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        self.aseprite.source_path = path.into();
        self
    }

    /// Hands over the assembled asset.
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// use bevy_aseprite_ultra::prelude::*;
    /// use bevy_aseprite_ultra::testing::AsepriteFixture;
    ///
    /// let mut assets = Assets::<Aseprite>::default();
    /// let handle = assets.add(AsepriteFixture::new().with_layer("Body", true).build());
    ///
    /// assert!(assets.get(&handle).is_some());
    /// ```
    #[must_use]
    pub fn build(self) -> Aseprite {
        self.aseprite
    }
}

/// A slice with nothing but its rectangle and atlas position set.
///
/// `frame_atlas_ids` stays empty, which is how
/// [`SliceMeta::atlas_id_for_frame`] reads as `atlas_id` on every frame.
fn slice_meta(rect: Rect, atlas_id: usize) -> SliceMeta {
    SliceMeta {
        rect,
        atlas_id,
        pivot: None,
        nine_patch: None,
        keys: Vec::new(),
        frame_atlas_ids: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_come_back_out_by_name() {
        let aseprite = AsepriteFixture::new()
            .with_slice("Panel", Rect::new(0.0, 0.0, 16.0, 8.0), 2)
            .with_slice("Icon", Rect::new(0.0, 0.0, 4.0, 4.0), 7)
            .build();

        let panel = aseprite.slice("Panel").expect("the slice that went in");
        assert_eq!(panel.atlas_id, 2);
        assert_eq!(panel.size(), Vec2::new(16.0, 8.0));
        assert_eq!(panel.border(), None);
        assert_eq!(panel.atlas_id_for_frame(3), 2);

        let mut names: Vec<&str> = aseprite.slices().map(|(name, _)| name).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["Icon", "Panel"]);
    }

    #[test]
    fn a_nine_patch_centre_becomes_border_insets() {
        let aseprite = AsepriteFixture::new()
            .with_nine_patch_slice(
                "Frame",
                Rect::new(0.0, 0.0, 12.0, 10.0),
                Vec4::new(3.0, 2.0, 6.0, 6.0),
            )
            .build();

        let border = aseprite
            .slice("Frame")
            .and_then(SliceMeta::border)
            .expect("a centre makes a border");
        assert_eq!(border.min_inset, Vec2::new(3.0, 2.0));
        assert_eq!(border.max_inset, Vec2::new(3.0, 2.0));
    }

    #[test]
    fn layers_keep_their_order_and_visibility() {
        let aseprite = AsepriteFixture::new()
            .with_layer("Hat", true)
            .with_layer("Body", false)
            .with_layer("Shadow", true)
            .build();

        assert_eq!(
            aseprite.layer_ids().collect::<Vec<_>>(),
            vec![
                LayerId::new("Hat"),
                LayerId::new("Body"),
                LayerId::new("Shadow"),
            ]
        );
        assert_eq!(
            aseprite.visible_layer_ids().collect::<Vec<_>>(),
            vec![LayerId::new("Hat"), LayerId::new("Shadow")]
        );
    }

    #[test]
    fn tags_default_to_forward_playback() {
        let aseprite = AsepriteFixture::new()
            .with_tag("walk", 2..=5)
            .with_tag_meta(
                "swing",
                TagMeta {
                    direction: AnimationDirection::PingPongReverse,
                    range: 0..=1,
                    repeat: 3,
                },
            )
            .build();

        let walk = aseprite.tag("walk").expect("the tag that went in");
        assert_eq!(walk.range, 2..=5);
        assert_eq!(walk.direction, AnimationDirection::Forward);
        assert_eq!(walk.repeat, 0);

        let swing = aseprite.tag("swing").expect("the tag that went in");
        assert_eq!(swing.direction, AnimationDirection::PingPongReverse);
        assert_eq!(swing.repeat, 3);
        assert_eq!(aseprite.tags().count(), 2);
    }

    #[test]
    fn frames_and_source_path_survive_the_build() {
        let aseprite = AsepriteFixture::new()
            .with_frame_durations([Duration::from_millis(50), Duration::from_millis(120)])
            .with_frame_indicies([4, 5])
            .with_source_path("player.aseprite")
            .build();

        assert_eq!(
            aseprite.frame_durations,
            vec![Duration::from_millis(50), Duration::from_millis(120)]
        );
        assert_eq!(aseprite.atlas_index(0), Some(4));
        assert_eq!(aseprite.atlas_index(9), Some(5), "frames clamp to the last");
        assert_eq!(aseprite.source_path, "player.aseprite");
    }
}
