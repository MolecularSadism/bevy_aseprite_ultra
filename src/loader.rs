use crate::error::AsepriteError;
use crate::layers::LayerId;
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
    /// All layer names in z-order (bottom to top).
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub layer_names: Vec<LayerId>,
    /// Layer names that are marked visible in the aseprite file.
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub visible_layer_names: Vec<LayerId>,
}

impl Aseprite {
    pub fn get_atlas_index(&self, frame: usize) -> usize {
        if self.frame_indicies.len() <= frame {
            return self.frame_indicies.last().cloned().unwrap_or_default();
        }
        self.frame_indicies[frame]
    }
}

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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct SliceKeyMeta {
    pub frame: usize,
    pub rect: Rect,
    pub pivot: Option<Vec2>,
    pub nine_patch: Option<Vec4>,
}

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

#[derive(Default, TypePath)]
pub struct AsepriteLoader;

#[derive(Serialize, Deserialize, Debug)]
pub struct AsepriteLoaderSettings {
    pub sampler: ImageSampler,
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
        let mut layer_names: Vec<LayerId> = Vec::new();
        let mut visible_layer_names: Vec<LayerId> = Vec::new();
        let mut per_layer_ids: Vec<(LayerId, Vec<AssetId<Image>>)> = Vec::new();

        for layer in raw.layers() {
            let layer_id = LayerId::new(&layer.name);
            layer_names.push(layer_id);
            if layer.visible {
                visible_layer_names.push(layer_id);
            }

            let selection = raw.select_layers_by_name(&[&layer.name]);
            let ids = render_frames(
                &raw,
                &selection,
                &settings.sampler,
                &mut all_images,
            )?;
            per_layer_ids.push((layer_id, ids));
        }

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

        // ----------------------------- slices
        let mut slices = HashMap::new();
        raw.slices().iter().for_each(|slice| {
            let slice_key = slice.slice_keys.first().unwrap();

            let min = Vec2::new(slice_key.x as f32, slice_key.y as f32);
            let max = min + Vec2::new(slice_key.width as f32, slice_key.height as f32);

            let pivot = match slice_key.pivot {
                Some(pivot) => Some(Vec2::new(pivot.x as f32, pivot.y as f32)),
                None => None,
            };

            let nine_patch = match slice_key.nine_patch {
                Some(nine_patch) => Some(Vec4::new(
                    nine_patch.x as f32,
                    nine_patch.y as f32,
                    nine_patch.width as f32,
                    nine_patch.height as f32,
                )),
                None => None,
            };

            let layout_id =
                layout.add_texture(URect::from_corners(min.as_uvec2(), max.as_uvec2()));

            let mut keys = Vec::new();
            for key in &slice.slice_keys {
                let k_min = Vec2::new(key.x as f32, key.y as f32);
                let k_max = k_min + Vec2::new(key.width as f32, key.height as f32);

                let k_pivot = key.pivot.map(|p| Vec2::new(p.x as f32, p.y as f32));

                let k_nine_patch = key.nine_patch.map(|np| {
                    Vec4::new(np.x as f32, np.y as f32, np.width as f32, np.height as f32)
                });

                keys.push(SliceKeyMeta {
                    frame: key.frame_number as usize,
                    rect: Rect::from_corners(k_min, k_max),
                    pivot: k_pivot,
                    nine_patch: k_nine_patch,
                });
            }

            slices.insert(
                slice.name.into(),
                SliceMeta {
                    rect: Rect::from_corners(min, max),
                    atlas_id: layout_id,
                    pivot,
                    nine_patch,
                    keys,
                },
            );
        });

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
                slices: slices.clone(),
                tags: tags.clone(),
                frame_durations: frame_durations.clone(),
                atlas_layout: atlas_layout.clone(),
                atlas_image: atlas_image.clone(),
                frame_indicies: all_indicies,
                source_path: source_path.clone(),
                layer_names: layer_names.clone(),
                visible_layer_names: visible_layer_names.clone(),
            },
        );

        // ----------------------------- per-layer sub-assets
        for (layer_id, ids) in &per_layer_ids {
            let layer_indicies = resolve_indices(ids);
            load_context.add_labeled_asset(
                layer_id.as_str().into(),
                Aseprite {
                    slices: slices.clone(),
                    tags: tags.clone(),
                    frame_durations: frame_durations.clone(),
                    atlas_layout: atlas_layout.clone(),
                    atlas_image: atlas_image.clone(),
                    frame_indicies: layer_indicies,
                    source_path: source_path.clone(),
                    layer_names: layer_names.clone(),
                    visible_layer_names: visible_layer_names.clone(),
                },
            );
        }

        // ----------------------------- main asset (composite visible)
        Ok(Aseprite {
            slices,
            tags,
            frame_durations,
            atlas_layout,
            atlas_image,
            frame_indicies: composite_indicies,
            source_path,
            layer_names,
            visible_layer_names,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["aseprite", "ase"]
    }
}
