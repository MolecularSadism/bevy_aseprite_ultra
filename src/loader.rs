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
///
/// # Normal maps
///
/// Layers ending in `.normal` are treated as normal-map companions to the
/// like-named color layer (`body` ↔ `body.normal`). They are filtered out of
/// every color path (composite, "all", per-layer, `Aseprite::layers`) and
/// packed into a parallel [`normal_atlas_image`](Self::normal_atlas_image)
/// that shares the same [`atlas_layout`](Self::atlas_layout) and frame
/// indices as the color atlas.
///
/// As a fallback, a sibling file named `<stem>.normal.<ase|aseprite>` next to
/// the main file can supply normals for the whole sprite. See
/// [`AsepriteLoaderSettings::normal_map`].
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
    /// Optional normal-map atlas. Same extent as `atlas_image` and addressed
    /// by the same `atlas_layout` / `frame_indicies` — every atlas index that
    /// is valid in the color atlas is valid here. Frames with no normal-map
    /// source are flat blue (128, 128, 255, 255).
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub normal_atlas_image: Option<Handle<Image>>,
    pub(crate) frame_indicies: Vec<usize>,
    /// The asset path this was loaded from, for constructing sub-asset paths.
    #[cfg_attr(feature = "asset_processing", serde(skip))]
    pub source_path: String,
    /// All color layers in **front-to-back order** (index 0 = topmost layer
    /// in the Aseprite editor, renders in front). `.normal` companion layers
    /// are filtered out and never appear here. Each entry carries the
    /// layer's file-defined visibility. Reorder or toggle `visible` at
    /// runtime to change rendering.
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

    /// Set visibility for a layer by name on the **asset** (affects all entities).
    /// For per-entity visibility, use
    /// [`AseTexture::toggle_layer_on`](crate::layers::AseTexture::toggle_layer_on) /
    /// [`toggle_layer_off`](crate::layers::AseTexture::toggle_layer_off) instead.
    ///
    /// Returns `true` if the layer was found.
    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> bool {
        if let Some(entry) = self.layers.iter_mut().find(|e| e.id == id) {
            entry.visible = visible;
            true
        } else {
            false
        }
    }

    /// Move the layer with the given ID to a new index (front-to-back).
    /// Index 0 = topmost layer (renders in front).
    ///
    /// This modifies the **asset** directly, affecting all entities that
    /// reference it. For per-entity overrides, use
    /// [`AseTexture::layer_order`](crate::layers::AseTexture::layer_order) or
    /// [`AseTexture::reorder_layer`](crate::layers::AseTexture::reorder_layer)
    /// instead.
    ///
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
    /// Where the loader looks for normal-map data. Defaults to [`Auto`]:
    /// in-file `<layer>.normal` companion layers take precedence; otherwise
    /// a sibling `<stem>.normal.<ase|aseprite>` file is used if present.
    ///
    /// [`Auto`]: NormalMapMode::Auto
    #[serde(default)]
    pub normal_map: NormalMapMode,
}

/// How the loader sources normal-map pixels.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NormalMapMode {
    /// In-file `.normal` companion layers preferred; sibling
    /// `<stem>.normal.<ase|aseprite>` file used as fallback if present.
    /// No error if neither is found — `normal_atlas_image` stays `None`.
    #[default]
    Auto,
    /// Only use in-file `.normal` companion layers.
    LayerSuffixOnly,
    /// Only use a sibling `<stem>.normal.<ase|aseprite>` file. Fails to load
    /// if no sibling file is found.
    SiblingOnly,
    /// Skip normal-map loading entirely. `.normal` layers are still filtered
    /// out of color paths.
    Disabled,
}

impl Default for AsepriteLoaderSettings {
    fn default() -> Self {
        Self {
            sampler: ImageSampler::nearest(),
            visible_layers: None,
            normal_map: NormalMapMode::Auto,
        }
    }
}

/// Suffix marking a layer as the normal-map companion of its like-named
/// color layer (e.g. `body.normal` ↔ `body`).
pub const NORMAL_LAYER_SUFFIX: &str = ".normal";

fn is_normal_layer_name(name: &str) -> bool {
    name.ends_with(NORMAL_LAYER_SUFFIX) && name.len() > NORMAL_LAYER_SUFFIX.len()
}

/// Strip the `.normal` suffix from a normal-map layer name.
/// Returns `None` if the name is not a normal-map layer.
fn color_prefix_of(name: &str) -> Option<&str> {
    if is_normal_layer_name(name) {
        Some(&name[..name.len() - NORMAL_LAYER_SUFFIX.len()])
    } else {
        None
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

        // ----------------------------- partition layers into color vs normal-map.
        // `.normal` layers are companion normal-maps; never user-visible as
        // color layers. `color_layer_names` keeps file order (bottom-to-top
        // here, before the later reverse).
        let mut color_layer_names: Vec<String> = Vec::new();
        let mut color_layer_visible: Vec<bool> = Vec::new();
        // color_prefix -> normal_layer_name (e.g. "body" -> "body.normal").
        let mut in_file_normal_for: HashMap<String, String> = HashMap::new();
        let mut orphan_normals: Vec<String> = Vec::new();
        for layer in raw.layers() {
            if let Some(prefix) = color_prefix_of(&layer.name) {
                in_file_normal_for.insert(prefix.to_owned(), layer.name.to_owned());
                if !raw.layers().iter().any(|l| l.name == prefix) {
                    orphan_normals.push(layer.name.to_owned());
                }
            } else {
                color_layer_names.push(layer.name.to_owned());
                color_layer_visible.push(layer.visible);
            }
        }
        for n in &orphan_normals {
            warn!(
                "aseprite '{}': '.normal' layer '{}' has no matching color layer; ignored",
                source_path, n
            );
        }

        // ----------------------------- optional sibling normal-map file.
        let sibling_bytes: Option<Vec<u8>> = match settings.normal_map {
            NormalMapMode::Disabled | NormalMapMode::LayerSuffixOnly => None,
            NormalMapMode::Auto | NormalMapMode::SiblingOnly => {
                let cand = sibling_normal_paths(&source_path);
                let mut found: Option<Vec<u8>> = None;
                for p in cand {
                    if let Ok(b) = load_context.read_asset_bytes(&p).await {
                        found = Some(b);
                        break;
                    }
                }
                if found.is_none() && settings.normal_map == NormalMapMode::SiblingOnly {
                    return Err(AsepriteError::ReadError);
                }
                found
            }
        };
        let sibling_raw: Option<AsepriteFile> = match sibling_bytes.as_deref() {
            Some(b) => match AsepriteFile::load(b) {
                Ok(f) => {
                    let (sw, sh) = f.size();
                    if sw != width || sh != height || f.frames().len() != num_frames {
                        warn!(
                            "aseprite '{}': sibling normal-map file dimensions or frame count mismatch; ignoring",
                            source_path
                        );
                        None
                    } else {
                        Some(f)
                    }
                }
                Err(_) => {
                    warn!(
                        "aseprite '{}': sibling normal-map file failed to parse; ignoring",
                        source_path
                    );
                    None
                }
            },
            None => None,
        };

        let want_normal_atlas = settings.normal_map != NormalMapMode::Disabled
            && (!in_file_normal_for.is_empty() || sibling_raw.is_some());

        // Collect all rendered images with their IDs, then add to atlas in one pass.
        let mut all_images: Vec<(AssetId<Image>, Image)> = Vec::new();
        // Parallel to all_images: the raw RGBA bytes for the matching normal-map
        // frame, if any. None = leave the corresponding atlas region flat blue.
        let mut all_normal_bytes: Vec<Option<Vec<u8>>> = Vec::new();

        // Render all frames for a given color selection, paired with the
        // matching normal-map render (in-file companion layers preferred,
        // sibling file as fallback).
        //
        // `color_layers_for_normal` lists the color layer names whose normal
        // companions should be combined for this selection. For composite
        // ("Visible"/"All") it is the visible-or-all color layer list; for
        // per-layer it is `[layer_name]`.
        let render_frames =
            |raw: &AsepriteFile,
             selection: &LayerSelection,
             color_layers_for_normal: &[&str],
             sampler: &ImageSampler,
             images: &mut Vec<(AssetId<Image>, Image)>,
             normals: &mut Vec<Option<Vec<u8>>>|
             -> Result<Vec<AssetId<Image>>, AsepriteError> {
                // Build the in-file `.normal` selection for this color layer set.
                let in_file_normal_names: Vec<&str> = color_layers_for_normal
                    .iter()
                    .filter_map(|c| in_file_normal_for.get(*c).map(|s| s.as_str()))
                    .collect();
                let in_file_normal_sel = (!in_file_normal_names.is_empty())
                    .then(|| raw.select_layers_by_name(&in_file_normal_names));

                // Sibling-file selection: only fill in for color layers that
                // had no in-file `.normal` companion.
                let sibling_sel = sibling_raw.as_ref().and_then(|sib| {
                    let sib_layer_names: Vec<&str> = color_layers_for_normal
                        .iter()
                        .filter(|c| !in_file_normal_for.contains_key(**c))
                        .filter(|c| sib.layers().iter().any(|l| l.name == **c))
                        .copied()
                        .collect();
                    (!sib_layer_names.is_empty())
                        .then(|| (sib, sib.select_layers_by_name(&sib_layer_names)))
                });

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

                    if !want_normal_atlas {
                        normals.push(None);
                        continue;
                    }

                    let mut acc: Option<Vec<u8>> = None;
                    if let Some(sel) = in_file_normal_sel.as_ref() {
                        let mut nbuf = vec![0u8; buf_size];
                        if raw.render_frame(index, nbuf.as_mut_slice(), sel).is_ok() {
                            acc = Some(nbuf);
                        }
                    }
                    if let Some((sib, sel)) = sibling_sel.as_ref() {
                        let mut sbuf = vec![0u8; buf_size];
                        if sib.render_frame(index, sbuf.as_mut_slice(), sel).is_ok() {
                            acc = Some(match acc {
                                Some(prev) => composite_over(&sbuf, &prev),
                                None => sbuf,
                            });
                        }
                    }
                    normals.push(acc);
                }
                Ok(frame_ids)
            };

        // ----------------------------- composite (visible color layers or custom selection)
        let composite_color_names: Vec<String> = match &settings.visible_layers {
            Some(layers) => layers
                .iter()
                .filter(|s| !is_normal_layer_name(s))
                .filter(|s| color_layer_names.iter().any(|c| c == *s))
                .cloned()
                .collect(),
            None => color_layer_names
                .iter()
                .zip(color_layer_visible.iter())
                .filter(|(_, v)| **v)
                .map(|(n, _)| n.clone())
                .collect(),
        };
        let composite_color_refs: Vec<&str> =
            composite_color_names.iter().map(|s| s.as_str()).collect();
        let composite_selection = raw.select_layers_by_name(&composite_color_refs);
        let composite_ids = render_frames(
            &raw,
            &composite_selection,
            &composite_color_refs,
            &settings.sampler,
            &mut all_images,
            &mut all_normal_bytes,
        )?;

        // ----------------------------- "all" composite (all color layers, including hidden)
        let all_color_refs: Vec<&str> =
            color_layer_names.iter().map(|s| s.as_str()).collect();
        let all_selection = raw.select_layers_by_name(&all_color_refs);
        let all_composite_ids = render_frames(
            &raw,
            &all_selection,
            &all_color_refs,
            &settings.sampler,
            &mut all_images,
            &mut all_normal_bytes,
        )?;

        // ----------------------------- per-layer renders (color layers only)
        let mut layer_entries: Vec<LayerEntry> = Vec::new();
        let mut per_layer_ids: Vec<(LayerId, Vec<AssetId<Image>>)> = Vec::new();

        for (name, visible) in color_layer_names.iter().zip(color_layer_visible.iter()) {
            let layer_id = LayerId::new(name);
            layer_entries.push(LayerEntry::new(layer_id, *visible));

            let selection = raw.select_layers_by_name(&[name.as_str()]);
            let ids = render_frames(
                &raw,
                &selection,
                &[name.as_str()],
                &settings.sampler,
                &mut all_images,
                &mut all_normal_bytes,
            )?;
            per_layer_ids.push((layer_id, ids));
        }

        // Aseprite stores layers bottom-to-top; reverse so index 0 = topmost
        // layer in the editor (front-to-back order).
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

        // ----------------------------- normal atlas (same extent + same packing as color)
        let normal_image_data: Option<Image> = if want_normal_atlas {
            let extent = image.texture_descriptor.size;
            let atlas_w = extent.width as usize;
            let atlas_h = extent.height as usize;
            let mut buf = vec![0u8; atlas_w * atlas_h * 4];
            // Default-fill flat blue: (128, 128, 255, 255) tangent-space "up".
            for px in buf.chunks_exact_mut(4) {
                px[0] = 128;
                px[1] = 128;
                px[2] = 255;
                px[3] = 255;
            }
            for (i, (color_id, _)) in all_images.iter().enumerate() {
                let Some(bytes) = all_normal_bytes.get(i).and_then(|n| n.as_ref()) else {
                    continue;
                };
                let Some(atlas_index) = source.texture_ids.get(color_id) else {
                    continue;
                };
                let rect = layout.textures[*atlas_index];
                let rw = (rect.max.x - rect.min.x) as usize;
                let rh = (rect.max.y - rect.min.y) as usize;
                if rw != width as usize || rh != height as usize {
                    // Frame size mismatch (shouldn't happen — all renders are
                    // canvas-sized). Skip rather than corrupt the atlas.
                    continue;
                }
                for row in 0..rh {
                    let src_off = row * (width as usize) * 4;
                    let dst_off =
                        ((rect.min.y as usize + row) * atlas_w + rect.min.x as usize) * 4;
                    buf[dst_off..dst_off + rw * 4]
                        .copy_from_slice(&bytes[src_off..src_off + rw * 4]);
                }
            }
            Some(Image {
                sampler: settings.sampler.clone(),
                ..Image::new(
                    extent,
                    TextureDimension::D2,
                    buf,
                    // Normal maps must NOT be sampled in sRGB space.
                    TextureFormat::Rgba8Unorm,
                    RenderAssetUsages::default(),
                )
            })
        } else {
            None
        };

        // ----------------------------- labeled sub-assets (shared atlas)
        let atlas_layout = load_context.add_labeled_asset("atlas_layout".into(), layout);
        let atlas_image = load_context.add_labeled_asset("atlas_texture".into(), image);
        let normal_atlas_image: Option<Handle<Image>> = normal_image_data
            .map(|img| load_context.add_labeled_asset("normal_atlas_texture".into(), img));

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
                normal_atlas_image: normal_atlas_image.clone(),
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
                    normal_atlas_image: normal_atlas_image.clone(),
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
            normal_atlas_image,
            frame_indicies: composite_indicies,
            source_path,
            layers: layer_entries,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["aseprite", "ase"]
    }
}

/// Standard "over" composite of `top` (premultiplied src) atop `bottom`.
/// Both buffers are RGBA8 in straight (non-premultiplied) alpha; output
/// matches.
fn composite_over(top: &[u8], bottom: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(top.len());
    for i in (0..top.len()).step_by(4) {
        let ta = top[i + 3] as f32 / 255.0;
        if ta >= 1.0 {
            out.extend_from_slice(&top[i..i + 4]);
            continue;
        }
        if ta <= 0.0 {
            out.extend_from_slice(&bottom[i..i + 4]);
            continue;
        }
        let ba = bottom[i + 3] as f32 / 255.0;
        let oa = ta + ba * (1.0 - ta);
        let blend = |t: u8, b: u8| -> u8 {
            if oa <= 0.0 {
                return 0;
            }
            let v = (t as f32 * ta + b as f32 * ba * (1.0 - ta)) / oa;
            v.clamp(0.0, 255.0) as u8
        };
        out.push(blend(top[i], bottom[i]));
        out.push(blend(top[i + 1], bottom[i + 1]));
        out.push(blend(top[i + 2], bottom[i + 2]));
        out.push((oa * 255.0).clamp(0.0, 255.0) as u8);
    }
    out
}

/// Candidate sibling-file paths for a main aseprite at `source_path`.
/// Returns `<stem>.normal.aseprite` and `<stem>.normal.ase` in that order.
fn sibling_normal_paths(source_path: &str) -> Vec<String> {
    let path = std::path::Path::new(source_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = path.parent();
    let mk = |ext: &str| -> String {
        let name = format!("{}.normal.{}", stem, ext);
        match parent {
            Some(p) if !p.as_os_str().is_empty() => p.join(&name).to_string_lossy().into_owned(),
            _ => name,
        }
    };
    vec![mk("aseprite"), mk("ase")]
}
