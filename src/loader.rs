use crate::error::AsepriteError;
use crate::layers::{LayerEntry, LayerId};
use aseprite_loader::{
    binary::chunks::tags::AnimationDirection,
    loader::{AsepriteFile, LayerSelection},
};
use bevy::{
    asset::{io::Reader, AssetLoader, RenderAssetUsages},
    image::ImageSampler,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite::Anchor,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Registers the [`Aseprite`] asset type and its loader.
///
/// Added automatically by [`AsepriteUltraPlugin`](crate::AsepriteUltraPlugin).
pub struct AsepriteLoaderPlugin;
impl Plugin for AsepriteLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Aseprite>();
        app.register_asset_loader(AsepriteLoader);
    }
}

/// The loaded Aseprite asset. By default (no `#label`), all visible layers are
/// composited into a single atlas. Sub-asset labels provide per-layer access:
///
/// - `"file.aseprite"` — all visible layers composited (default)
/// - `"file.aseprite#all"` — all layers including hidden ones
/// - `"file.aseprite#Layer Name"` — a single named layer
///
/// All variants share the same atlas texture and layout.
#[derive(Asset, Default, TypePath, Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct Aseprite {
    pub slices: HashMap<String, SliceMeta>,
    pub tags: HashMap<String, TagMeta>,
    pub frame_durations: Vec<std::time::Duration>,
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub atlas_layout: Handle<TextureAtlasLayout>,
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub atlas_image: Handle<Image>,
    pub(crate) frame_indicies: Vec<usize>,
    /// The asset path this was loaded from, for constructing sub-asset paths.
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub source_path: String,
    /// All layers in **front-to-back order** (index 0 = topmost layer in the
    /// Aseprite editor, renders in front). Each entry carries the layer's
    /// file-defined visibility. Reorder or toggle `visible` at runtime to
    /// change rendering.
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub layers: Vec<LayerEntry>,
}

impl Aseprite {
    pub fn get_atlas_index(&self, frame: usize) -> usize {
        if self.frame_indicies.len() <= frame {
            return self.frame_indicies.last().cloned().unwrap_or_default();
        }
        self.frame_indicies[frame]
    }

    /// All layer IDs in front-to-back order.
    pub fn layer_ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.iter().map(|e| e.id)
    }

    /// Layer IDs that are currently marked visible, in front-to-back order.
    pub fn visible_layer_ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.iter().filter(|e| e.visible).map(|e| e.id)
    }

    /// Set visibility for a layer by name. Returns `true` if the layer was found.
    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> bool {
        if let Some(entry) = self.layers.iter_mut().find(|e| e.id == id) {
            entry.visible = visible;
            true
        } else {
            false
        }
    }

    /// Move the layer with the given ID to a new index (front-to-back).
    /// Returns `true` if the layer was found and moved.
    pub fn reorder_layer(&mut self, id: LayerId, new_index: usize) -> bool {
        if let Some(old) = self.layers.iter().position(|e| e.id == id) {
            let entry = self.layers.remove(old);
            let idx = new_index.min(self.layers.len());
            self.layers.insert(idx, entry);
            true
        } else {
            false
        }
    }
}

/// Metadata for a single animation tag in the aseprite file.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct TagMeta {
    #[cfg_attr(feature = "asset_processing", serde(with = "AnimationDirectionDef"))]
    pub direction: AnimationDirection,
    pub range: std::ops::RangeInclusive<u16>,
    pub repeat: u16,
}

#[cfg(feature = "asset_processing")]
#[derive(Serialize, Deserialize)]
#[serde(remote = "AnimationDirection")]
enum AnimationDirectionDef {
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
    Unknown(u8),
}

/// Metadata for a single key in a slice's animation timeline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct SliceKeyMeta {
    pub frame: usize,
    pub rect: Rect,
    pub pivot: Option<Vec2>,
    pub nine_patch: Option<Vec4>,
}

/// Metadata for a named slice region in the aseprite file.
///
/// Contains the slice rectangle, its position in the atlas, optional
/// pivot offset, and optional 9-patch insets for UI scaling.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct SliceMeta {
    pub rect: Rect,
    pub atlas_id: usize,
    pub pivot: Option<Vec2>,
    pub nine_patch: Option<Vec4>,
    pub keys: Vec<SliceKeyMeta>,
}

impl From<&SliceMeta> for Anchor {
    fn from(value: &SliceMeta) -> Self {
        match value.pivot {
            Some(pivot) => {
                let size = value.rect.size();
                let uv = (pivot.min(size).max(Vec2::ZERO) / size) - Vec2::new(0.5, 0.5);
                Anchor(uv * Vec2::new(1.0, -1.0))
            }
            None => Anchor::CENTER,
        }
    }
}

/// The [`AssetLoader`] for `.aseprite` / `.ase` files.
///
/// Registered automatically by [`AsepriteLoaderPlugin`].
#[derive(Default, TypePath)]
pub struct AsepriteLoader;

/// Settings for the aseprite asset loader.
///
/// Configure the image sampler and optionally restrict which layers are
/// included in the default (unlabeled) composite.
#[derive(Serialize, Deserialize, Debug)]
pub struct AsepriteLoaderSettings {
    /// The texture sampler to use. Defaults to nearest-neighbor.
    pub sampler: ImageSampler,
    /// When set, only these layers are composited for the default asset.
    /// `None` means all visible layers (the default).
    pub visible_layers: Option<Vec<String>>,
}

impl Default for AsepriteLoaderSettings {
    fn default() -> Self {
        Self {
            sampler: ImageSampler::nearest(),
            visible_layers: None,
        }
    }
}

impl AssetLoader for AsepriteLoader {
    type Asset = Aseprite;
    type Settings = AsepriteLoaderSettings;
    type Error = super::error::AsepriteError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| AsepriteError::ReadError)?;

        let raw = AsepriteFile::load(&bytes)?;
        let source_path = load_context.path().to_string();
        let (width, height) = raw.size();
        let buf_size = width as usize * height as usize * 4;
        let num_frames = raw.frames().len();

        // Collect all rendered images with their IDs, then add to atlas in one pass.
        let mut all_images: Vec<(AssetId<Image>, Image)> = Vec::new();

        // Helper: render all frames with a given layer selection.
        // Returns the AssetIds for each frame rendered.
        let render_frames = |raw: &AsepriteFile,
                             selection: &LayerSelection,
                             sampler: &ImageSampler,
                             images: &mut Vec<(AssetId<Image>, Image)>|
         -> Result<Vec<AssetId<Image>>, AsepriteError> {
            let mut frame_ids = Vec::with_capacity(num_frames);
            for index in 0..num_frames {
                let mut buffer = vec![0u8; buf_size];
                raw.render_frame(index, buffer.as_mut_slice(), selection)?;

                let image = Image {
                    sampler: sampler.clone(),
                    ..Image::new(
                        Extent3d {
                            width: width as u32,
                            height: height as u32,
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        buffer,
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    )
                };
                let id = AssetId::Uuid {
                    uuid: Uuid::new_v4(),
                };
                images.push((id, image));
                frame_ids.push(id);
            }
            Ok(frame_ids)
        };

        // ----------------------------- composite (visible layers or custom selection)
        let composite_selection = match &settings.visible_layers {
            Some(layers) => {
                let names: Vec<&str> = layers.iter().map(|s| s.as_str()).collect();
                raw.select_layers_by_name(&names)
            }
            None => LayerSelection::Visible,
        };
        let composite_ids = render_frames(
            &raw,
            &composite_selection,
            &settings.sampler,
            &mut all_images,
        )?;

        // ----------------------------- "all" composite (all layers including hidden)
        let all_composite_ids = render_frames(
            &raw,
            &LayerSelection::All,
            &settings.sampler,
            &mut all_images,
        )?;

        // ----------------------------- per-layer renders
        let mut layer_entries: Vec<LayerEntry> = Vec::new();
        let mut per_layer_ids: Vec<(LayerId, Vec<AssetId<Image>>)> = Vec::new();

        for layer in raw.layers() {
            let layer_id = LayerId::new(&layer.name);
            layer_entries.push(LayerEntry::new(layer_id, layer.visible));

            let selection = raw.select_layers_by_name(&[&layer.name]);
            let ids = render_frames(
                &raw,
                &selection,
                &settings.sampler,
                &mut all_images,
            )?;
            per_layer_ids.push((layer_id, ids));
        }

        // Aseprite stores layers bottom-to-top; reverse so index 0 = topmost
        // layer in the editor (renders in front), matching Aseprite's visual
        // stacking order.
        layer_entries.reverse();

        // ----------------------------- build shared atlas
        let mut atlas_builder = TextureAtlasBuilder::default();
        atlas_builder.max_size(UVec2::splat(4096));
        for (id, image) in &all_images {
            atlas_builder.add_texture(Some(*id), image);
        }
        let (mut layout, source, image) = atlas_builder.build()?;

        let resolve_indices = |ids: &[AssetId<Image>]| -> Vec<usize> {
            ids.iter()
                .map(|id| source.texture_ids.get(id).cloned().unwrap())
                .collect()
        };

        let composite_indicies = resolve_indices(&composite_ids);
        let all_indicies = resolve_indices(&all_composite_ids);

        // Pre-resolve per-layer indices while source is still available
        let per_layer_resolved: Vec<(LayerId, Vec<usize>)> = per_layer_ids
            .iter()
            .map(|(id, ids)| (*id, resolve_indices(ids)))
            .collect();

        // ----------------------------- raw slice data
        // Collect slice metadata without atlas IDs; each variant (composite,
        // all, per-layer) computes its own atlas IDs relative to its frame
        // position in the packed atlas.
        struct RawSlice {
            name: String,
            rect: Rect,
            canvas_min: UVec2,
            canvas_max: UVec2,
            pivot: Option<Vec2>,
            nine_patch: Option<Vec4>,
            keys: Vec<SliceKeyMeta>,
        }

        let raw_slice_data: Vec<RawSlice> = raw
            .slices()
            .iter()
            .map(|slice| {
                let slice_key = slice.slice_keys.first().unwrap();
                let min = Vec2::new(slice_key.x as f32, slice_key.y as f32);
                let max = min + Vec2::new(slice_key.width as f32, slice_key.height as f32);

                let pivot = slice_key
                    .pivot
                    .map(|p| Vec2::new(p.x as f32, p.y as f32));
                let nine_patch = slice_key.nine_patch.map(|np| {
                    Vec4::new(np.x as f32, np.y as f32, np.width as f32, np.height as f32)
                });

                let keys: Vec<SliceKeyMeta> = slice
                    .slice_keys
                    .iter()
                    .map(|key| {
                        let k_min = Vec2::new(key.x as f32, key.y as f32);
                        let k_max =
                            k_min + Vec2::new(key.width as f32, key.height as f32);
                        SliceKeyMeta {
                            frame: key.frame_number as usize,
                            rect: Rect::from_corners(k_min, k_max),
                            pivot: key
                                .pivot
                                .map(|p| Vec2::new(p.x as f32, p.y as f32)),
                            nine_patch: key.nine_patch.map(|np| {
                                Vec4::new(
                                    np.x as f32,
                                    np.y as f32,
                                    np.width as f32,
                                    np.height as f32,
                                )
                            }),
                        }
                    })
                    .collect();

                RawSlice {
                    name: slice.name.to_owned(),
                    rect: Rect::from_corners(min, max),
                    canvas_min: min.as_uvec2(),
                    canvas_max: max.as_uvec2(),
                    pivot,
                    nine_patch,
                    keys,
                }
            })
            .collect();

        // Build a SliceMeta map for a specific variant by offsetting canvas-
        // relative slice rects to the variant's first frame position in the
        // packed atlas.
        let build_slices =
            |frame_index: usize, layout: &mut TextureAtlasLayout| -> HashMap<String, SliceMeta> {
                let frame_rect = layout.textures[frame_index];
                raw_slice_data
                    .iter()
                    .map(|raw| {
                        let atlas_rect = URect::from_corners(
                            frame_rect.min + raw.canvas_min,
                            frame_rect.min + raw.canvas_max,
                        );
                        let layout_id = layout.add_texture(atlas_rect);
                        (
                            raw.name.clone(),
                            SliceMeta {
                                rect: raw.rect,
                                atlas_id: layout_id,
                                pivot: raw.pivot,
                                nine_patch: raw.nine_patch,
                                keys: raw.keys.clone(),
                            },
                        )
                    })
                    .collect()
            };

        let composite_slices = build_slices(composite_indicies[0], &mut layout);
        let all_slices = build_slices(all_indicies[0], &mut layout);

        let mut per_layer_data: Vec<(LayerId, Vec<usize>, HashMap<String, SliceMeta>)> =
            Vec::new();
        for (layer_id, layer_indicies) in per_layer_resolved {
            let slices = build_slices(layer_indicies[0], &mut layout);
            per_layer_data.push((layer_id, layer_indicies, slices));
        }

        // ----------------------------- labeled sub-assets (shared atlas)
        let atlas_layout = load_context.add_labeled_asset("atlas_layout".into(), layout);
        let atlas_image = load_context.add_labeled_asset("atlas_texture".into(), image);

        // ---------------------------- tags
        let mut tags = HashMap::new();
        raw.tags().iter().for_each(|tag| {
            tags.insert(
                tag.name.clone(),
                TagMeta {
                    direction: tag.direction,
                    range: tag.range.clone(),
                    repeat: tag.repeat.unwrap_or(0),
                },
            );
        });

        // ---------------------------- frames
        let frame_durations: Vec<std::time::Duration> = raw
            .frames()
            .iter()
            .map(|frame| std::time::Duration::from_millis(u64::from(frame.duration)))
            .collect();

        // ----------------------------- "all" sub-asset
        load_context.add_labeled_asset(
            "all".into(),
            Aseprite {
                slices: all_slices,
                tags: tags.clone(),
                frame_durations: frame_durations.clone(),
                atlas_layout: atlas_layout.clone(),
                atlas_image: atlas_image.clone(),
                frame_indicies: all_indicies,
                source_path: source_path.clone(),
                layers: layer_entries.clone(),
            },
        );

        // ----------------------------- per-layer sub-assets
        for (layer_id, layer_indicies, layer_slices) in per_layer_data {
            load_context.add_labeled_asset(
                layer_id.as_str().into(),
                Aseprite {
                    slices: layer_slices,
                    tags: tags.clone(),
                    frame_durations: frame_durations.clone(),
                    atlas_layout: atlas_layout.clone(),
                    atlas_image: atlas_image.clone(),
                    frame_indicies: layer_indicies,
                    source_path: source_path.clone(),
                    layers: layer_entries.clone(),
                },
            );
        }

        // ----------------------------- main asset (composite visible)
        Ok(Aseprite {
            slices: composite_slices,
            tags,
            frame_durations,
            atlas_layout,
            atlas_image,
            frame_indicies: composite_indicies,
            source_path,
            layers: layer_entries,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["aseprite", "ase"]
    }
}
