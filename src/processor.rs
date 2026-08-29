//! Bakes an aseprite file into a processed asset: the packed atlas as QOI
//! bytes, everything else as msgpack.
//!
//! The cache holds the whole asset — the composite, the `all` variant and
//! every per-layer sub-asset — so a processed build resolves the same
//! `path#layer` sub-assets layered rendering asks the source loader for.

use std::io::Cursor;

use bevy::{
    asset::{
        AssetLoader, AsyncWriteExt,
        processor::LoadTransformAndSave,
        saver::{AssetSaver, SavedAsset},
        transformer::IdentityAssetTransformer,
    },
    image::{CompressedImageFormats, ImageFormatSetting, ImageLoaderSettings, ImageType},
    prelude::*,
    render::renderer::RenderDevice,
};
use image::ImageFormat;
use serde::{Deserialize, Serialize};

use crate::{
    error::AsepriteError,
    loader::{Aseprite, AsepriteLoader},
};

/// Label of the shared atlas layout sub-asset.
const ATLAS_LAYOUT: &str = "atlas_layout";
/// Label of the shared atlas texture sub-asset.
const ATLAS_TEXTURE: &str = "atlas_texture";
/// Bytes of the length prefix in front of the msgpack segment.
const HEADER_LEN: usize = size_of::<u64>();

pub struct AsepriteProcessorPlugin;

impl Plugin for AsepriteProcessorPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_processor::<LoadTransformAndSave<AsepriteLoader, IdentityAssetTransformer<Aseprite>, AsepriteSaver>>(LoadTransformAndSave::new(IdentityAssetTransformer::new(), AsepriteSaver));
        app.set_default_asset_processor::<LoadTransformAndSave<AsepriteLoader, IdentityAssetTransformer<Aseprite>, AsepriteSaver>>("aseprite");
        app.set_default_asset_processor::<LoadTransformAndSave<AsepriteLoader, IdentityAssetTransformer<Aseprite>, AsepriteSaver>>("ase");
    }

    fn finish(&self, app: &mut App) {
        let supported_compressed_formats = match app.world().get_resource::<RenderDevice>() {
            Some(render_device) => CompressedImageFormats::from_features(render_device.features()),

            None => CompressedImageFormats::NONE,
        };

        app.register_asset_loader(ProcessedAsepriteLoader {
            supported_compressed_formats,
        });
    }
}

/// Everything about an aseprite file that is not the atlas image.
///
/// `variants` carries the labeled sub-assets — `"all"` and one per layer — as
/// they were loaded, so the processed loader can register them again. Without
/// them a processed build resolves layer sub-asset paths to nothing and
/// layered entities render blank.
#[derive(Serialize, Deserialize)]
struct ProcessedAseprite {
    root: Aseprite,
    variants: Vec<(String, Aseprite)>,
    atlas_layout: TextureAtlasLayout,
}

#[derive(TypePath)]
struct AsepriteSaver;

impl AssetSaver for AsepriteSaver {
    type Asset = Aseprite;

    type Settings = ();

    type OutputLoader = ProcessedAsepriteLoader;

    type Error = super::error::AsepriteError;

    async fn save(
        &self,
        writer: &mut bevy::asset::io::Writer,
        asset: bevy::asset::saver::SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> Result<<Self::OutputLoader as bevy::asset::AssetLoader>::Settings, Self::Error> {
        let texture_atlas_layout: SavedAsset<TextureAtlasLayout> = asset
            .get_labeled(ATLAS_LAYOUT)
            .ok_or(AsepriteError::MissingSubAsset {
                label: ATLAS_LAYOUT,
            })?;
        let atlas_texture: SavedAsset<Image> =
            asset
                .get_labeled(ATLAS_TEXTURE)
                .ok_or(AsepriteError::MissingSubAsset {
                    label: ATLAS_TEXTURE,
                })?;

        // Every label that is not one of the two shared atlas pieces is an
        // aseprite variant: the `all` composite, or a single layer.
        let labels: Vec<String> = asset
            .iter_labels()
            .filter(|label| *label != ATLAS_LAYOUT && *label != ATLAS_TEXTURE)
            .map(str::to_owned)
            .collect();
        let variants: Vec<(String, Aseprite)> = labels
            .into_iter()
            .filter_map(|label| {
                let variant = asset.get_labeled::<Aseprite, str>(&label)?;
                Some((label, variant.get().clone()))
            })
            .collect();

        let processed = ProcessedAseprite {
            root: asset.get().clone(),
            variants,
            atlas_layout: texture_atlas_layout.get().clone(),
        };

        let msgpack_buf = rmp_serde::to_vec(&processed)?;

        // Write length of msgpack segment
        writer
            .write_all(&((msgpack_buf.len() as u64).to_be_bytes()))
            .await
            .map_err(AsepriteError::Write)?;

        // Write msgpack itself
        writer
            .write_all(&msgpack_buf)
            .await
            .map_err(AsepriteError::Write)?;

        let mut image_buf = Vec::new();
        let mut image_write = Cursor::new(&mut image_buf);

        let dynamic = atlas_texture.clone().try_into_dynamic()?;
        dynamic.write_to(&mut image_write, ImageFormat::Qoi)?;

        writer
            .write_all(&image_buf)
            .await
            .map_err(AsepriteError::Write)?;

        Ok(ImageLoaderSettings {
            format: ImageFormatSetting::Format(bevy::prelude::ImageFormat::Qoi),
            is_srgb: atlas_texture.texture_descriptor.format.is_srgb(),
            sampler: atlas_texture.sampler.clone(),
            asset_usage: atlas_texture.asset_usage,
            texture_format: None,
            array_layout: None,
        })
    }
}

/// Splits a processed file into its msgpack and atlas-image halves.
///
/// The length prefix is written by [`AsepriteSaver`], so a prefix pointing
/// past the end of the file means the cache was truncated or half-written,
/// never that the format disagrees.
fn split_payload(buf: &[u8]) -> Result<(&[u8], &[u8]), AsepriteError> {
    let header: [u8; HEADER_LEN] = buf
        .get(..HEADER_LEN)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(AsepriteError::TruncatedCache {
            expected: HEADER_LEN,
            found: buf.len(),
        })?;

    let msgpack_end = usize::try_from(u64::from_be_bytes(header))
        .unwrap_or(usize::MAX)
        .saturating_add(HEADER_LEN);

    let msgpack = buf
        .get(HEADER_LEN..msgpack_end)
        .ok_or(AsepriteError::TruncatedCache {
            expected: msgpack_end,
            found: buf.len(),
        })?;

    Ok((msgpack, &buf[msgpack_end..]))
}

#[derive(TypePath)]
struct ProcessedAsepriteLoader {
    supported_compressed_formats: CompressedImageFormats,
}

impl AssetLoader for ProcessedAsepriteLoader {
    type Asset = Aseprite;

    type Settings = ImageLoaderSettings;

    type Error = AsepriteError;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;

        let (msgpack, atlas_texture_buf) = split_payload(&buf)?;
        let processed: ProcessedAseprite = rmp_serde::from_slice(msgpack)?;

        let atlas_texture = Image::from_buffer(
            atlas_texture_buf,
            ImageType::Format(bevy::prelude::ImageFormat::Qoi),
            self.supported_compressed_formats,
            settings.is_srgb,
            settings.sampler.clone(),
            settings.asset_usage,
        )?;

        let atlas_layout_handle =
            load_context.add_labeled_asset(ATLAS_LAYOUT.into(), processed.atlas_layout);
        let atlas_texture_handle =
            load_context.add_labeled_asset(ATLAS_TEXTURE.into(), atlas_texture);

        // Handles and the source path cannot survive serialization; every
        // variant is put back onto the one atlas this file just decoded.
        let source_path = load_context.path().to_string();
        let restore = |aseprite: Aseprite| Aseprite {
            atlas_layout: atlas_layout_handle.clone(),
            atlas_image: atlas_texture_handle.clone(),
            source_path: source_path.clone(),
            ..aseprite
        };

        // The composite holds a handle to every labeled variant so Bevy keeps
        // them resident alongside it (see `Aseprite::variant_handles`); the
        // cache does not carry the handles, so they are rebuilt here.
        let variant_handles = processed
            .variants
            .into_iter()
            .map(|(label, variant)| load_context.add_labeled_asset(label, restore(variant)))
            .collect();

        Ok(Aseprite {
            variant_handles,
            ..restore(processed.root)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        loader::{SliceMeta, TagMeta},
        prelude::{AnimationDirection, LayerId, SliceId, TagId},
    };

    /// An aseprite as the loader hands it over: layers, slices, frames — all
    /// of it state the cache has to carry, none of it recoverable at load.
    fn sample() -> Aseprite {
        Aseprite::builder()
            .with_layer("top", true)
            .with_layer("bottom", false)
            .with_frame_indices([0, 1])
            .with_slice_meta(
                "Panel",
                SliceMeta {
                    rect: Rect::new(0.0, 0.0, 4.0, 4.0),
                    atlas_id: 0,
                    pivot: None,
                    nine_patch: None,
                    keys: Vec::new(),
                    frame_atlas_ids: vec![0, 1],
                },
            )
            .with_tag_meta(
                "Swing",
                TagMeta {
                    direction: AnimationDirection::PingPongReverse,
                    range: 0..=1,
                    repeat: 2,
                },
            )
            .build()
    }

    #[test]
    fn layers_and_sub_assets_survive_the_cache() {
        let processed = ProcessedAseprite {
            root: sample(),
            variants: vec![("all".to_owned(), sample())],
            atlas_layout: TextureAtlasLayout::new_empty(UVec2::splat(8)),
        };

        let bytes = rmp_serde::to_vec(&processed).expect("cache serializes");
        let read_back: ProcessedAseprite = rmp_serde::from_slice(&bytes).expect("cache parses");

        assert_eq!(
            read_back.root.layer_ids().collect::<Vec<_>>(),
            vec![LayerId::new("top"), LayerId::new("bottom")],
            "layered rendering has nothing to spawn without the layer list"
        );
        assert_eq!(
            read_back.root.visible_layer_ids().collect::<Vec<_>>(),
            vec![LayerId::new("top")]
        );
        // Slices and tags are keyed by their interned ids, which the cache
        // carries as the names they were interned from; a key that came back
        // as anything else would miss both of these lookups.
        let slice = read_back
            .root
            .slice(SliceId::new("Panel"))
            .expect("the slice is kept");
        assert_eq!(slice.rect, Rect::new(0.0, 0.0, 4.0, 4.0));
        assert_eq!(slice.frame_atlas_ids, vec![0, 1]);
        assert_eq!(
            read_back
                .root
                .slices()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![SliceId::new("Panel")],
        );

        let tag = read_back
            .root
            .tag(TagId::new("Swing"))
            .expect("the tag is kept");
        assert_eq!(tag.direction, AnimationDirection::PingPongReverse);
        assert_eq!(tag.range, 0..=1);
        assert_eq!(tag.repeat, 2);
        assert_eq!(
            read_back.root.tags().map(|(id, _)| id).collect::<Vec<_>>(),
            vec![TagId::new("Swing")],
        );

        let (label, variant) = read_back.variants.first().expect("the sub-asset is kept");
        assert_eq!(label, "all");
        assert_eq!(variant.layer_ids().count(), 2);
    }

    fn cache(msgpack: &[u8], image: &[u8]) -> Vec<u8> {
        let mut buf = (msgpack.len() as u64).to_be_bytes().to_vec();
        buf.extend_from_slice(msgpack);
        buf.extend_from_slice(image);
        buf
    }

    #[test]
    fn splits_a_whole_cache() {
        let buf = cache(b"msgpack", b"image");
        let (msgpack, image) = split_payload(&buf).expect("whole cache splits");
        assert_eq!(msgpack, b"msgpack");
        assert_eq!(image, b"image");
    }

    #[test]
    fn a_cache_without_a_header_errors() {
        let error = split_payload(&[1, 2, 3]).expect_err("short cache must not panic");
        assert!(matches!(
            error,
            AsepriteError::TruncatedCache { expected, found } if expected == HEADER_LEN && found == 3
        ));
    }

    #[test]
    fn a_cache_cut_inside_its_msgpack_errors() {
        let mut buf = cache(b"msgpack", b"image");
        buf.truncate(HEADER_LEN + 3);
        let error = split_payload(&buf).expect_err("truncated cache must not panic");
        assert!(matches!(error, AsepriteError::TruncatedCache { .. }));
    }

    #[test]
    fn an_absurd_length_prefix_errors() {
        let mut buf = u64::MAX.to_be_bytes().to_vec();
        buf.extend_from_slice(b"nowhere near that much");
        let error = split_payload(&buf).expect_err("overflowing prefix must not panic");
        assert!(matches!(error, AsepriteError::TruncatedCache { .. }));
    }

    #[test]
    fn an_empty_cache_errors() {
        let error = split_payload(&[]).expect_err("empty cache must not panic");
        assert!(matches!(
            error,
            AsepriteError::TruncatedCache { found: 0, .. }
        ));
    }
}
