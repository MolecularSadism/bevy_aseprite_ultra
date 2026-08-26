use crate::animation::AnimationDirection;
use crate::error::AsepriteError;
use crate::layers::{LayerEntry, LayerId};
use aseprite_loader::{
    binary::chunks::layer::LayerType,
    loader::{AsepriteFile, LayerSelection},
};
use bevy::{
    asset::{AssetLoader, RenderAssetUsages, io::Reader},
    image::ImageSampler,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite::{Anchor, BorderRect},
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
    /// Read through [`slice`](Self::slice) / [`slices`](Self::slices).
    pub(crate) slices: HashMap<String, SliceMeta>,
    /// Read through [`tag`](Self::tag) / [`tags`](Self::tags).
    pub(crate) tags: HashMap<String, TagMeta>,
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
    /// Aseprite editor, renders in front), each carrying the layer's
    /// file-defined visibility. Read through [`layer_ids`](Self::layer_ids) /
    /// [`visible_layer_ids`](Self::visible_layer_ids); what a given entity
    /// draws is chosen per entity on its
    /// [`AseTexture`](crate::layers::AseTexture), never on the shared asset.
    #[cfg_attr(feature = "asset_processing", serde(with = "layer_serde"))]
    pub(crate) layers: Vec<LayerEntry>,
}

impl Aseprite {
    /// This variant's atlas position for `frame`, clamped to the last frame.
    ///
    /// Clamping is deliberate: an animation whose tag outlives the frames of a
    /// per-layer variant keeps drawing that layer's final frame instead of
    /// vanishing. Returns `None` for an asset with no frames at all.
    #[must_use]
    pub fn atlas_index(&self, frame: usize) -> Option<usize> {
        let last = self.frame_indicies.len().checked_sub(1)?;
        Some(self.frame_indicies[frame.min(last)])
    }

    /// In-crate shim over [`atlas_index`](Self::atlas_index) for the render
    /// paths that still expect an index unconditionally. Not public: index `0`
    /// for a frameless asset is a sentinel, which the public accessor avoids.
    pub(crate) fn get_atlas_index(&self, frame: usize) -> usize {
        self.atlas_index(frame).unwrap_or_default()
    }

    /// The named slice, or `None` when this variant defines none by that name.
    ///
    /// Slice names are file-wide, so every sub-asset of a file carries the
    /// same set; only the atlas positions behind them differ.
    #[must_use]
    pub fn slice(&self, name: &str) -> Option<&SliceMeta> {
        self.slices.get(name)
    }

    /// Every slice of this variant, by name, in arbitrary order.
    pub fn slices(&self) -> impl Iterator<Item = (&str, &SliceMeta)> {
        self.slices.iter().map(|(name, meta)| (name.as_str(), meta))
    }

    /// The named animation tag, or `None` when the file defines none by that
    /// name. Tags are file-wide, like slices.
    #[must_use]
    pub fn tag(&self, name: &str) -> Option<&TagMeta> {
        self.tags.get(name)
    }

    /// Every animation tag of the file, by name, in arbitrary order.
    pub fn tags(&self) -> impl Iterator<Item = (&str, &TagMeta)> {
        self.tags.iter().map(|(name, meta)| (name.as_str(), meta))
    }

    /// All layer IDs in front-to-back order.
    pub fn layer_ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.iter().map(|e| e.id)
    }

    /// Layer IDs marked visible in the file, in front-to-back order.
    ///
    /// This is the file's own state and never changes after load. To show,
    /// hide or reorder layers at runtime, drive the entity's
    /// [`AseTexture`](crate::layers::AseTexture) — the asset is shared by every
    /// entity that references it, so it is not where per-entity state belongs.
    pub fn visible_layer_ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.iter().filter(|e| e.visible).map(|e| e.id)
    }
}

/// Layers round-trip through the `asset_processing` cache as names, since
/// [`LayerId`] interns its string and carries no serde impl of its own.
#[cfg(feature = "asset_processing")]
mod layer_serde {
    use super::{LayerEntry, LayerId};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        layers: &[LayerEntry],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        layers
            .iter()
            .map(|entry| (entry.id.as_str(), entry.visible))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<LayerEntry>, D::Error> {
        Ok(Vec::<(String, bool)>::deserialize(deserializer)?
            .into_iter()
            .map(|(name, visible)| LayerEntry::new(LayerId::new(&name), visible))
            .collect())
    }
}

/// Metadata for a single animation tag in the aseprite file.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct TagMeta {
    pub direction: AnimationDirection,
    pub range: std::ops::RangeInclusive<u16>,
    pub repeat: u16,
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
/// Contains the slice rectangle, its default (frame 0) position in the atlas,
/// optional pivot offset, and optional 9-patch insets for UI scaling.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "asset_processing", derive(Serialize, Deserialize))]
pub struct SliceMeta {
    pub rect: Rect,
    pub atlas_id: usize,
    pub pivot: Option<Vec2>,
    pub nine_patch: Option<Vec4>,
    pub keys: Vec<SliceKeyMeta>,
    /// The slice's own atlas position for each frame of its aseprite variant
    /// (composite/all/per-layer), parallel to the variant's own frame list.
    /// A slice is defined in canvas coordinates, so its crop rect is
    /// identical across frames — only *which frame's rendered image* it
    /// crops into changes. Empty for slices loaded before this field existed
    /// (e.g. deserialized from an older `asset_processing` cache); callers
    /// should fall back to `atlas_id` in that case (see
    /// [`Self::atlas_id_for_frame`]).
    pub frame_atlas_ids: Vec<usize>,
}

impl SliceMeta {
    /// The slice's size in pixels, as authored on the canvas.
    ///
    /// This is the art's own size, independent of whatever ends up drawing
    /// it, so it is the natural size a UI node should fall back to.
    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.rect.size()
    }

    /// The nine-patch border insets, or `None` when the slice has no centre.
    ///
    /// The file stores the centre rectangle; the insets are the distance from
    /// each edge to it. The loader drops centres that do not fit the slice, so
    /// these are never negative.
    #[must_use]
    pub fn border(&self) -> Option<BorderRect> {
        self.nine_patch
            .map(|np| crate::slice::nine_patch_to_slicer(np, self.rect.size()).border)
    }

    /// This slice's atlas position for a specific absolute frame number.
    ///
    /// Falls back to `atlas_id` (the slice's frame-0 position) when
    /// `frame_atlas_ids` has no entry for `frame` — out of range, or empty
    /// because it was loaded before this field existed.
    #[must_use]
    pub fn atlas_id_for_frame(&self, frame: usize) -> usize {
        self.frame_atlas_ids
            .get(frame)
            .copied()
            .unwrap_or(self.atlas_id)
    }
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

/// One layer of a file, resolved against the group tree it sits in.
struct ResolvedLayer {
    /// Id addressing exactly this layer.
    id: LayerId,
    /// Whether the layer is visible in the source file.
    visible: bool,
    /// Which layers this entry draws: itself, or — for a group, which holds no
    /// cels of its own — every drawable layer beneath it.
    selection: LayerSelection,
}

/// Resolves a file's layers into ids and render selections.
///
/// Two things make the flat name list Aseprite exports unusable as an index.
/// Names are unique only within a group, so several colour groups may each
/// hold a child called `Main`; and a group is a container, so rendering one by
/// name yields an empty image. Duplicated names are therefore qualified with
/// their group path, and a group's selection covers its whole subtree.
///
/// The returned order matches [`AsepriteFile::layers`].
fn resolve_layers(raw: &AsepriteFile) -> Vec<ResolvedLayer> {
    // `AsepriteFile::layers` keeps normal and group chunks, in file order, so
    // the same filter over the raw chunks lines the two lists up index for index.
    let chunks: Vec<_> = raw
        .file
        .layers
        .iter()
        .filter(|chunk| matches!(chunk.layer_type, LayerType::Normal | LayerType::Group))
        .collect();
    let layers = raw.layers();

    // Ancestor chain, rebuilt as the walk descends: a layer at child level `n`
    // hangs under the last layer seen at level `n - 1`.
    let mut ancestry: Vec<Vec<usize>> = Vec::with_capacity(layers.len());
    let mut open_groups: Vec<usize> = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        open_groups.truncate(usize::from(chunk.child_level));
        ancestry.push(open_groups.clone());
        if chunk.layer_type == LayerType::Group {
            open_groups.push(index);
        }
    }

    let path_of = |index: usize| -> String {
        ancestry[index]
            .iter()
            .chain(std::iter::once(&index))
            .map(|&i| layers[i].name.as_str())
            .collect::<Vec<_>>()
            .join("/")
    };

    layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let ambiguous = layers
                .iter()
                .enumerate()
                .any(|(other, candidate)| other != index && candidate.name == layer.name);
            let id = if ambiguous {
                LayerId::new(&path_of(index))
            } else {
                LayerId::new(&layer.name)
            };

            // A group draws through its descendants; anything else draws itself.
            let mask = (0..layers.len())
                .map(|other| other == index || ancestry[other].contains(&index))
                .collect();

            ResolvedLayer {
                id,
                visible: layer.visible,
                selection: LayerSelection::Mask(mask),
            }
        })
        .collect()
}

/// Selection covering every layer named in `names`, groups included.
///
/// A name is matched against both the bare layer name and the qualified path
/// [`resolve_layers`] falls back to, so a settings entry keeps working whether
/// or not the name it picks turns out to be ambiguous.
fn union_selection(layers: &[ResolvedLayer], names: &[String]) -> LayerSelection {
    let mut mask = vec![false; layers.len()];
    for layer in layers {
        if !names.iter().any(|name| layer.id == LayerId::new(name)) {
            continue;
        }
        let LayerSelection::Mask(selected) = &layer.selection else {
            continue;
        };
        for (slot, picked) in mask.iter_mut().zip(selected) {
            *slot |= picked;
        }
    }
    LayerSelection::Mask(mask)
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
    /// Edge length, in pixels, of the square the packed atlas may not exceed.
    ///
    /// One file packs its canvas once per frame per variant — composite, all,
    /// and one per layer — so a large canvas with many frames and layers can
    /// exhaust the default. Overflowing the cap fails the load with an
    /// `AsepriteError` rather than cropping silently; raise it here when a
    /// file legitimately needs more than the GPU-safe default of 4096.
    pub max_atlas_size: u32,
}

impl Default for AsepriteLoaderSettings {
    fn default() -> Self {
        Self {
            sampler: ImageSampler::nearest(),
            visible_layers: None,
            max_atlas_size: DEFAULT_MAX_ATLAS_SIZE,
        }
    }
}

/// Widest atlas guaranteed to be within a GPU's texture size limits.
const DEFAULT_MAX_ATLAS_SIZE: u32 = 4096;

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
        reader.read_to_end(&mut bytes).await?;

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
        let resolved_layers = resolve_layers(&raw);
        let composite_selection = match &settings.visible_layers {
            Some(names) => union_selection(&resolved_layers, names),
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

        for layer in resolved_layers {
            layer_entries.push(LayerEntry::new(layer.id, layer.visible));

            let ids = render_frames(&raw, &layer.selection, &settings.sampler, &mut all_images)?;
            per_layer_ids.push((layer.id, ids));
        }

        // Aseprite stores layers bottom-to-top; reverse so index 0 = topmost
        // layer in the editor (front-to-back order).
        layer_entries.reverse();

        // ----------------------------- build shared atlas
        let mut atlas_builder = TextureAtlasBuilder::default();
        atlas_builder.max_size(UVec2::splat(settings.max_atlas_size));
        for (id, image) in &all_images {
            atlas_builder.add_texture(Some(*id), image);
        }
        let (mut layout, source, image) = atlas_builder.build()?;

        // The packer places every texture it accepts, so a missing id means the
        // frames outgrew `max_atlas_size` — a file problem the game must hear
        // about, not an index to guess at.
        let resolve_indices = |ids: &[AssetId<Image>]| -> Result<Vec<usize>, AsepriteError> {
            ids.iter()
                .map(|id| {
                    source
                        .texture_ids
                        .get(id)
                        .copied()
                        .ok_or(AsepriteError::AtlasOverflow {
                            textures: all_images.len(),
                            max_size: settings.max_atlas_size,
                        })
                })
                .collect()
        };

        let composite_indicies = resolve_indices(&composite_ids)?;
        let all_indicies = resolve_indices(&all_composite_ids)?;

        // Pre-resolve per-layer indices while source is still available
        let mut per_layer_resolved: Vec<(LayerId, Vec<usize>)> =
            Vec::with_capacity(per_layer_ids.len());
        for (id, ids) in &per_layer_ids {
            per_layer_resolved.push((*id, resolve_indices(ids)?));
        }

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

        // A centre with no area cannot divide anything, and one reaching past
        // the key's own bounds would invert an inset; neither describes a
        // nine-patch, so both read as "this key sets no centre" rather than as
        // a degenerate slicer. An inset of exactly zero is a legal edgeless
        // border and stays.
        let nine_patch_of = |name: &str, key: &aseprite_loader::binary::chunks::slice::SliceKey| {
            let np = key.nine_patch.filter(|np| np.width > 0 && np.height > 0)?;
            let left = i64::from(np.x);
            let top = i64::from(np.y);
            let right = i64::from(key.width) - left - i64::from(np.width);
            let bottom = i64::from(key.height) - top - i64::from(np.height);
            if left < 0 || top < 0 || right < 0 || bottom < 0 {
                warn!(
                    "slice {name:?} frame {}: nine-patch centre ({}, {}, {}x{}) does not fit its \
                     {}x{} bounds — insets left {left}, top {top}, right {right}, bottom {bottom}. \
                     Ignoring the centre; fix it in Aseprite.",
                    key.frame_number, np.x, np.y, np.width, np.height, key.width, key.height,
                );
                return None;
            }
            Some(Vec4::new(
                np.x as f32,
                np.y as f32,
                np.width as f32,
                np.height as f32,
            ))
        };

        let raw_slice_data: Vec<RawSlice> = raw
            .slices()
            .iter()
            .filter_map(|slice| {
                // A slice's geometry lives entirely in its keys; one without
                // any describes no region, so there is nothing to register.
                let Some(slice_key) = slice.slice_keys.first() else {
                    warn!(
                        "slice {:?} in {source_path} has no keys and was skipped; \
                         re-save it from Aseprite with the slice placed on a frame",
                        slice.name,
                    );
                    return None;
                };
                let min = Vec2::new(slice_key.x as f32, slice_key.y as f32);
                let max = min + Vec2::new(slice_key.width as f32, slice_key.height as f32);

                let pivot = slice_key.pivot.map(|p| Vec2::new(p.x as f32, p.y as f32));

                let keys: Vec<SliceKeyMeta> = slice
                    .slice_keys
                    .iter()
                    .map(|key| {
                        let k_min = Vec2::new(key.x as f32, key.y as f32);
                        let k_max = k_min + Vec2::new(key.width as f32, key.height as f32);
                        SliceKeyMeta {
                            frame: key.frame_number as usize,
                            rect: Rect::from_corners(k_min, k_max),
                            pivot: key.pivot.map(|p| Vec2::new(p.x as f32, p.y as f32)),
                            nine_patch: nine_patch_of(slice.name, key),
                        }
                    })
                    .collect();

                // A key written before its centre was dragged out carries an
                // empty one. Every frame of the slice falls back to the first
                // centre that is actually a centre, so a partly-annotated
                // timeline slices the same way from end to end.
                let nine_patch = keys.iter().find_map(|key| key.nine_patch);

                Some(RawSlice {
                    name: slice.name.to_owned(),
                    rect: Rect::from_corners(min, max),
                    canvas_min: min.as_uvec2(),
                    canvas_max: max.as_uvec2(),
                    pivot,
                    nine_patch,
                    keys,
                })
            })
            .collect();

        // Build a SliceMeta map for a specific variant by offsetting each of
        // its frames' canvas-relative slice rects to that frame's position in
        // the packed atlas. A slice's canvas rect is the same across every
        // frame (it's defined once, in canvas coordinates); only the
        // underlying frame image being cropped changes, so this registers one
        // atlas entry per frame, mirroring `frame_indicies` (see
        // `SliceMeta::atlas_id_for_frame`).
        let build_slices =
            |indices: &[usize], layout: &mut TextureAtlasLayout| -> HashMap<String, SliceMeta> {
                raw_slice_data
                    .iter()
                    .map(|raw| {
                        let frame_atlas_ids: Vec<usize> = indices
                            .iter()
                            .map(|&frame_index| {
                                let frame_rect = layout.textures[frame_index];
                                let atlas_rect = URect::from_corners(
                                    frame_rect.min + raw.canvas_min,
                                    frame_rect.min + raw.canvas_max,
                                );
                                layout.add_texture(atlas_rect)
                            })
                            .collect();
                        (
                            raw.name.clone(),
                            SliceMeta {
                                rect: raw.rect,
                                atlas_id: frame_atlas_ids.first().copied().unwrap_or_default(),
                                pivot: raw.pivot,
                                nine_patch: raw.nine_patch,
                                keys: raw.keys.clone(),
                                frame_atlas_ids,
                            },
                        )
                    })
                    .collect()
            };

        let composite_slices = build_slices(&composite_indicies, &mut layout);
        let all_slices = build_slices(&all_indicies, &mut layout);

        let mut per_layer_data: Vec<(LayerId, Vec<usize>, HashMap<String, SliceMeta>)> = Vec::new();
        for (layer_id, layer_indicies) in per_layer_resolved {
            let slices = build_slices(&layer_indicies, &mut layout);
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
                    direction: tag.direction.into(),
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

        // ----------------------------- variants (one shape, three uses)
        // Every variant shares the file's tags, frame timings and atlas; only
        // its slice positions and frame indices differ.
        let variant = |slices: HashMap<String, SliceMeta>, frame_indicies: Vec<usize>| Aseprite {
            slices,
            tags: tags.clone(),
            frame_durations: frame_durations.clone(),
            atlas_layout: atlas_layout.clone(),
            atlas_image: atlas_image.clone(),
            frame_indicies,
            source_path: source_path.clone(),
            layers: layer_entries.clone(),
        };

        load_context.add_labeled_asset("all".into(), variant(all_slices, all_indicies));

        for (layer_id, layer_indicies, layer_slices) in per_layer_data {
            load_context.add_labeled_asset(
                layer_id.as_str().into(),
                variant(layer_slices, layer_indicies),
            );
        }

        // The default asset: every layer the settings leave visible, composited.
        Ok(variant(composite_slices, composite_indicies))
    }

    fn extensions(&self) -> &[&str] {
        &["aseprite", "ase"]
    }
}
