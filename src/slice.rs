use crate::animation::{AseFrame, AseTag, resolve_frame};
use crate::layers::SpriteLayerOf;
use crate::loader::{Aseprite, SliceMeta};
use bevy::{
    ecs::component::Mutable,
    prelude::*,
    sprite::{Anchor, BorderRect, TextureSlicer},
    sprite_render::Material2d,
    ui::{UiSystems, widget::NodeImageMode},
};

/// Convert aseprite 9-patch data to a Bevy [`TextureSlicer`].
///
/// Aseprite stores the center rectangle as `Vec4(x, y, width, height)` relative
/// to the slice origin. Bevy needs border insets from each edge.
pub fn nine_patch_to_slicer(nine_patch: Vec4, slice_size: Vec2) -> TextureSlicer {
    let left = nine_patch.x;
    let top = nine_patch.y;
    let right = slice_size.x - nine_patch.x - nine_patch.z;
    let bottom = slice_size.y - nine_patch.y - nine_patch.w;
    TextureSlicer {
        border: BorderRect {
            min_inset: Vec2::new(left, top),
            max_inset: Vec2::new(right, bottom),
        },
        ..default()
    }
}

pub struct AsepriteSlicePlugin;

impl Plugin for AsepriteSlicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            render_slice::<ImageNode>.before(UiSystems::Prepare),
        );
        app.add_systems(PostUpdate, render_slice::<Sprite>);
        app.register_type::<AseSlice>();
    }
}

/// Any component that implements this trait can be used as a render target for
/// [`AseSlice`]. The plugin ships with implementations for [`Sprite`],
/// [`ImageNode`], and [`MeshMaterial2d`] (plus [`MeshMaterial3d`] with the `3d`
/// feature).
///
/// Implement this trait on your own material to use slice data in custom shaders.
///
/// # Examples
///
/// ```rust,ignore
/// impl RenderSlice for MyMaterial {
///     type Extra<'e> = Res<'e, Time>;
///     fn render_slice(
///         &mut self,
///         aseprite: &Aseprite,
///         slice_meta: &SliceMeta,
///         extra: &mut Self::Extra<'_>,
///     ) {
///         self.image = aseprite.atlas_image.clone();
///         self.texture_min = slice_meta.rect.min.as_uvec2();
///         self.texture_max = slice_meta.rect.max.as_uvec2();
///         self.time = extra.elapsed_secs();
///     }
/// }
/// ```
pub trait RenderSlice {
    /// An extra system parameter used in rendering. Use a tuple if many are required.
    type Extra<'e>;
    fn render_slice(
        &mut self,
        aseprite: &Aseprite,
        slice_meta: &SliceMeta,
        extra: &mut Self::Extra<'_>,
    );
}

impl RenderSlice for ImageNode {
    type Extra<'e> = ();
    fn render_slice(&mut self, aseprite: &Aseprite, slice_meta: &SliceMeta, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: slice_meta.atlas_id,
        });
        // The centre the artist dragged out in Aseprite is the border, always:
        // a nine-patch is a property of the art, not of what draws it. A call
        // site that already asked to be sliced keeps the rest of its slicer —
        // how the middle and sides scale, and the corner cap — because those
        // have no representation in the file.
        if let Some(border) = slice_meta.border() {
            self.image_mode = NodeImageMode::Sliced(match &self.image_mode {
                NodeImageMode::Sliced(slicer) => TextureSlicer {
                    border,
                    ..slicer.clone()
                },
                _ => TextureSlicer {
                    border,
                    ..default()
                },
            });
        }
    }
}

impl RenderSlice for Sprite {
    type Extra<'e> = ();
    fn render_slice(&mut self, aseprite: &Aseprite, slice_meta: &SliceMeta, _extra: &mut ()) {
        self.image = aseprite.atlas_image.clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout.clone(),
            index: slice_meta.atlas_id,
        });
        if let Some(border) = slice_meta.border() {
            self.image_mode = SpriteImageMode::Sliced(match &self.image_mode {
                SpriteImageMode::Sliced(slicer) => TextureSlicer {
                    border,
                    ..slicer.clone()
                },
                _ => TextureSlicer {
                    border,
                    ..default()
                },
            });
        }
    }
}

impl<M: Material2d + RenderSlice> RenderSlice for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(
        &mut self,
        aseprite: &Aseprite,
        slice_meta: &SliceMeta,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice_meta, &mut extra.1);
    }
}

impl<M: UiMaterial + RenderSlice> RenderSlice for MaterialNode<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(
        &mut self,
        aseprite: &Aseprite,
        slice_meta: &SliceMeta,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice_meta, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderSlice> RenderSlice for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(
        &mut self,
        aseprite: &Aseprite,
        slice_meta: &SliceMeta,
        extra: &mut Self::Extra<'_>,
    ) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice_meta, &mut extra.1);
    }
}

/// Renders a named slice region from an aseprite asset.
///
/// Placed on child entities by [`AseTexture`](crate::layers::AseTexture) when
/// a slice is configured. Supports pivot offsets and 9-patch data.
/// When combined with [`AnimationLayer`](crate::animation::AnimationLayer),
/// the slice can be animated (frame-specific slice keys).
#[derive(Component, Reflect, Default, Debug, Clone)]
#[reflect]
pub struct AseSlice {
    pub name: String,
    pub aseprite: Handle<Aseprite>,
}

impl AseSlice {
    /// Create a new `AseSlice`.
    pub fn new(aseprite: Handle<Aseprite>, name: impl Into<String>) -> Self {
        AseSlice {
            name: name.into(),
            aseprite,
        }
    }
}

pub fn render_slice<T: RenderSlice + Component<Mutability = Mutable>>(
    mut slices: Query<(
        &mut T,
        Ref<AseSlice>,
        Option<Ref<AseFrame>>,
        Option<Ref<AseTag>>,
        Option<&SpriteLayerOf>,
        Option<&mut Anchor>,
    )>,
    parent_frames: Query<(Ref<AseFrame>, Option<Ref<AseTag>>)>,
    aseprites: Res<Assets<Aseprite>>,
    mut extra: <T as RenderSlice>::Extra<'_>,
) {
    let asset_change = aseprites.is_changed();

    for (mut target, slice, local_frame, local_tag, parent_ref, maybe_anchor) in &mut slices {
        let parent = parent_ref.and_then(|p| parent_frames.get(p.0).ok());

        // `AseAnimation` advances `AseFrame`/`AseTag` every tick — locally, or
        // on the `AseTexture` parent for layered children — without ever
        // touching `AseSlice` itself. Watching only `slice.is_changed()`
        // would therefore render the frame this slice first resolved to and
        // then never update again for the lifetime of the animation.
        let frame_or_tag_changed = local_frame.as_ref().is_some_and(Ref::is_changed)
            || local_tag.as_ref().is_some_and(Ref::is_changed)
            || parent.as_ref().is_some_and(|(frame, _)| frame.is_changed())
            || parent
                .as_ref()
                .is_some_and(|(_, tag)| tag.as_ref().is_some_and(Ref::is_changed));

        if !asset_change && !slice.is_changed() && !frame_or_tag_changed {
            continue;
        }
        let Some(aseprite) = aseprites.get(&slice.aseprite) else {
            continue;
        };
        let Some(slice_meta) = aseprite.slices.get(&slice.name) else {
            #[cfg(debug_assertions)]
            {
                let source = slice
                    .aseprite
                    .path()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| format!("<handle {:?}>", slice.aseprite.id()));
                warn!(
                    "slice {:?} does not exist in aseprite '{}' (available: {:?})",
                    slice.name,
                    source,
                    aseprite.slices.keys().collect::<Vec<_>>(),
                );
            }
            continue;
        };

        // Resolve AseFrame / AseTag: prefer local, then fall back to the parent's.
        let maybe_frame = local_frame
            .as_deref()
            .copied()
            .or_else(|| parent.as_ref().map(|(f, _)| **f));
        let maybe_tag = local_tag
            .as_deref()
            .or_else(|| parent.as_ref().and_then(|(_, t)| t.as_deref()));

        // A slice is defined in canvas coordinates, so its crop rect is the
        // same on every frame — only *which frame's rendered image* it crops
        // into changes as the animation plays. Re-point atlas_id at the
        // current frame's own position every time, then layer an explicit
        // per-frame key (rect/pivot/9-patch) on top when Aseprite's slice
        // timeline defines one for this exact frame.
        let effective_meta = if let Some(frame) = maybe_frame {
            let absolute = resolve_frame(aseprite, frame, maybe_tag);
            let frame_idx = usize::from(absolute);
            let atlas_id = slice_meta.atlas_id_for_frame(frame_idx);
            if let Some(key) = slice_meta.keys.iter().find(|k| k.frame == frame_idx) {
                SliceMeta {
                    rect: key.rect,
                    atlas_id,
                    pivot: key.pivot.or(slice_meta.pivot),
                    nine_patch: key.nine_patch.or(slice_meta.nine_patch),
                    keys: vec![],
                    frame_atlas_ids: vec![],
                }
            } else {
                SliceMeta {
                    atlas_id,
                    ..slice_meta.clone()
                }
            }
        } else {
            slice_meta.clone()
        };

        if let Some(mut anchor) = maybe_anchor {
            *anchor = Anchor::from(&effective_meta);
        }

        target.render_slice(aseprite, &effective_meta, &mut extra);
    }
}
