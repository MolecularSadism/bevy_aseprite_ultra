use aseprite_loader::loader::{LoadImageError, LoadSpriteError};
use bevy::image::TextureAtlasBuilderError;
use thiserror::Error;

/// Everything loading, saving or reading back an [`Aseprite`](crate::loader::Aseprite)
/// can fail with.
///
/// This is the [`AssetLoader::Error`](bevy::asset::AssetLoader::Error) of both the
/// source loader and — with `asset_processing` — the processed loader, so it is
/// the type a game matches on to tell a broken file from a broken cache.
///
/// New variants are added as new failure modes are surfaced rather than
/// collapsed into an existing one, hence `#[non_exhaustive]`: match with a
/// wildcard arm.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AsepriteError {
    #[error("failed to build atlas")]
    Atlas(#[from] TextureAtlasBuilderError),
    #[error("failed to read aseprite binary")]
    Load(#[from] LoadSpriteError),
    #[error("failed to combine aseprite layers")]
    Composite(#[from] LoadImageError),
    #[error("failed to read byte stream")]
    Read(#[from] std::io::Error),
    /// The atlas packer placed no rect for a frame it was handed, which it only
    /// does when the frames do not fit the size cap.
    #[error(
        "frame dropped while packing the atlas: {textures} rendered frame textures do not \
         fit a {max_size}x{max_size} atlas — raise `AsepriteLoaderSettings::max_atlas_size`, \
         reduce the canvas, or split the file"
    )]
    AtlasOverflow { textures: usize, max_size: u32 },
    #[cfg(feature = "asset_processing")]
    #[error("failed to write to processed asset")]
    Write(#[source] std::io::Error),
    #[cfg(feature = "asset_processing")]
    #[error("failed to serialize aseprite data")]
    Serialize(#[from] rmp_serde::encode::Error),
    #[cfg(feature = "asset_processing")]
    #[error("failed to deserialize processed aseprite data: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
    #[cfg(feature = "asset_processing")]
    #[error("failed to write image data")]
    Encode(#[from] image::ImageError),
    #[cfg(feature = "asset_processing")]
    #[error("failed to read image data")]
    Texture(#[from] bevy::image::TextureError),
    #[cfg(feature = "asset_processing")]
    #[error("atlas image is not in a format that can be encoded")]
    AtlasConversion(#[from] bevy::image::IntoDynamicImageError),
    /// A sub-asset the saver needs is absent, which means the asset it was
    /// handed did not come from this crate's loader.
    #[cfg(feature = "asset_processing")]
    #[error("aseprite asset is missing its {label:?} sub-asset")]
    MissingSubAsset { label: &'static str },
    /// The processed cache does not hold the bytes its own header promises —
    /// a truncated or partially written file. Delete `imported_assets` and let
    /// the processor rebuild it.
    #[cfg(feature = "asset_processing")]
    #[error("processed aseprite cache is truncated: {found} bytes present, {expected} needed")]
    TruncatedCache { expected: usize, found: usize },
}
