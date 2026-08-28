use crate::animation::{AseFrame, AseTag, resolve_frame};
use crate::layers::{SliceId, SpriteLayerOf};
use crate::loader::{Aseprite, SliceMeta, SliceView};
use bevy::{
    ecs::component::Mutable,
    platform::collections::HashSet,
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

/// Fold a slice's own nine-patch centre into whatever slicer the call site
/// already asked for.
///
/// The centre the artist dragged out in Aseprite is the border, always: a
/// nine-patch is a property of the art, not of what draws it. The rest of an
/// existing slicer survives — how the middle and sides scale, and the corner
/// cap — because none of that has a representation in the file.
fn merge_border(existing: Option<&TextureSlicer>, border: BorderRect) -> TextureSlicer {
    match existing {
        Some(slicer) => TextureSlicer {
            border,
            ..slicer.clone()
        },
        None => TextureSlicer {
            border,
            ..default()
        },
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
        app.register_type::<SliceId>();
    }
}

/// Any component that implements this trait can be used as a render target for
/// [`AseSlice`]. The plugin ships with implementations for [`Sprite`],
/// [`ImageNode`], and [`MeshMaterial2d`] (plus `MeshMaterial3d` with the `3d`
/// feature).
///
/// Implement this trait on your own material to use slice data in custom shaders.
///
/// `Extra` is whatever system parameters the implementation needs on top of
/// the asset and the slice; use a tuple when there is more than one.
///
/// # Examples
///
/// ```rust
/// # use bevy::prelude::*;
/// # use bevy_aseprite_ultra::prelude::*;
/// #[derive(Default)]
/// struct MyMaterial {
///     image: Handle<Image>,
///     texture_min: UVec2,
///     texture_max: UVec2,
///     time: f32,
/// }
///
/// impl RenderSlice for MyMaterial {
///     type Extra<'e> = Res<'e, Time>;
///     fn render_slice(
///         &mut self,
///         aseprite: &Aseprite,
///         slice: SliceView,
///         extra: &mut Self::Extra<'_>,
///     ) {
///         self.image = aseprite.atlas_image().clone();
///         self.texture_min = slice.rect.min.as_uvec2();
///         self.texture_max = slice.rect.max.as_uvec2();
///         self.time = extra.elapsed_secs();
///     }
/// }
/// ```
pub trait RenderSlice {
    /// An extra system parameter used in rendering. Use a tuple if many are required.
    type Extra<'e>;
    /// Draws `slice` — one slice on one frame — off `aseprite`'s atlas.
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, extra: &mut Self::Extra<'_>);
}

impl RenderSlice for ImageNode {
    type Extra<'e> = ();
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, _extra: &mut ()) {
        self.image = aseprite.atlas_image().clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout().clone(),
            index: slice.atlas_id,
        });
        if let Some(border) = slice.border() {
            let existing = match &self.image_mode {
                NodeImageMode::Sliced(slicer) => Some(slicer),
                _ => None,
            };
            self.image_mode = NodeImageMode::Sliced(merge_border(existing, border));
        }
    }
}

impl RenderSlice for Sprite {
    type Extra<'e> = ();
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, _extra: &mut ()) {
        self.image = aseprite.atlas_image().clone();
        self.texture_atlas = Some(TextureAtlas {
            layout: aseprite.atlas_layout().clone(),
            index: slice.atlas_id,
        });
        if let Some(border) = slice.border() {
            let existing = match &self.image_mode {
                SpriteImageMode::Sliced(slicer) => Some(slicer),
                _ => None,
            };
            self.image_mode = SpriteImageMode::Sliced(merge_border(existing, border));
        }
    }
}

impl<M: Material2d + RenderSlice> RenderSlice for MeshMaterial2d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice, &mut extra.1);
    }
}

impl<M: UiMaterial + RenderSlice> RenderSlice for MaterialNode<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice, &mut extra.1);
    }
}

#[cfg(feature = "3d")]
impl<M: Material + RenderSlice> RenderSlice for MeshMaterial3d<M> {
    type Extra<'e> = (ResMut<'e, Assets<M>>, <M as RenderSlice>::Extra<'e>);
    fn render_slice(&mut self, aseprite: &Aseprite, slice: SliceView, extra: &mut Self::Extra<'_>) {
        let Some(material) = extra.0.get_mut(&*self) else {
            return;
        };
        material.render_slice(aseprite, slice, &mut extra.1);
    }
}

/// Renders a named slice region from an aseprite asset.
///
/// Placed on child entities by [`AseTexture`](crate::layers::AseTexture) when
/// a slice is configured. Supports pivot offsets and 9-patch data.
/// When combined with [`AnimationLayer`](crate::animation::AnimationLayer),
/// the slice can be animated (frame-specific slice keys).
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq)]
#[reflect(Component)]
pub struct AseSlice {
    pub name: SliceId,
    pub aseprite: Handle<Aseprite>,
}

impl AseSlice {
    /// Create a new `AseSlice`.
    pub fn new(aseprite: Handle<Aseprite>, name: impl Into<SliceId>) -> Self {
        AseSlice {
            name: name.into(),
            aseprite,
        }
    }

    /// The slice this component draws.
    ///
    /// `None` while the sheet is still loading, or when the file defines no
    /// slice by this name. The component carries both halves of the lookup,
    /// so resolving through it is the only way to be sure the sheet and the
    /// name belong together.
    #[must_use]
    pub fn meta<'a>(&self, aseprites: &'a Assets<Aseprite>) -> Option<&'a SliceMeta> {
        aseprites.get(&self.aseprite)?.slice(self.name)
    }

    /// The authored size of the slice this component draws.
    #[must_use]
    pub fn size(&self, aseprites: &Assets<Aseprite>) -> Option<Vec2> {
        self.meta(aseprites).map(SliceMeta::size)
    }

    /// The nine-patch insets of the slice this component draws, or `None`
    /// when the artist gave it no centre.
    #[must_use]
    pub fn border(&self, aseprites: &Assets<Aseprite>) -> Option<BorderRect> {
        self.meta(aseprites)?.border()
    }
}

#[allow(clippy::type_complexity, reason = "an optional-heavy Bevy query")]
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
    mut warned_missing: Local<HashSet<(AssetId<Aseprite>, SliceId)>>,
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
        let Some(slice_meta) = aseprite.slice(slice.name) else {
            // An artist typo is the commonest way to reach this, and release
            // is where nobody can attach a debugger — so warn there too. Once
            // per (sheet, slice), not every frame the animation ticks — and
            // not once per system, which would let the first typo anywhere
            // swallow every other one. The formatting only runs on that first
            // pass.
            if warned_missing.insert((slice.aseprite.id(), slice.name)) {
                warn!(
                    "slice {:?} does not exist in aseprite '{}' (available: {:?})",
                    slice.name.as_str(),
                    slice
                        .aseprite
                        .path()
                        .map(|path| path.to_string())
                        .unwrap_or_else(|| format!("<handle {:?}>", slice.aseprite.id())),
                    aseprite
                        .slices()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>(),
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
        // into changes as the animation plays. The frame's view carries that
        // frame's atlas position, and whatever rect, pivot or centre
        // Aseprite's slice timeline sets on a key for this exact frame.
        let view = match maybe_frame {
            Some(frame) => {
                let absolute = resolve_frame(aseprite, frame, maybe_tag);
                slice_meta.view_at_frame(usize::from(absolute))
            }
            None => SliceView::from(slice_meta),
        };

        if let Some(mut anchor) = maybe_anchor {
            *anchor = Anchor::from(view);
        }

        target.render_slice(aseprite, view, &mut extra);
    }
}
