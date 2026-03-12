use crate::animation::AnimationState;
use crate::loader::{Aseprite, SliceMeta};
use bevy::{
    ecs::component::Mutable, prelude::*, sprite::Anchor, sprite_render::Material2d, ui::UiSystems,
};

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
    mut slices: Query<(&mut T, Ref<AseSlice>, Option<&AnimationState>, Option<&mut Anchor>)>,
    aseprites: Res<Assets<Aseprite>>,
    mut extra: <T as RenderSlice>::Extra<'_>,
) {
    let asset_change = aseprites.is_changed();

    for (mut target, slice, maybe_state, maybe_anchor) in &mut slices {
        if !asset_change && !slice.is_changed() {
            continue;
        }
        let Some(aseprite) = aseprites.get(&slice.aseprite) else {
            continue;
        };
        let Some(slice_meta) = aseprite.slices.get(&slice.name) else {
            warn!("slice does not exist {}", slice.name);
            continue;
        };

        // For animated slices, use the frame-specific key if available.
        let effective_meta = if let Some(state) = maybe_state {
            let frame = usize::from(state.current_frame);
            if let Some(key) = slice_meta.keys.iter().find(|k| k.frame == frame) {
                &SliceMeta {
                    rect: key.rect,
                    atlas_id: slice_meta.atlas_id,
                    pivot: key.pivot.or(slice_meta.pivot),
                    nine_patch: key.nine_patch.or(slice_meta.nine_patch),
                    keys: vec![],
                }
            } else {
                slice_meta
            }
        } else {
            slice_meta
        };

        if let Some(mut anchor) = maybe_anchor {
            *anchor = Anchor::from(effective_meta);
        }

        target.render_slice(aseprite, effective_meta, &mut extra);
    }
}
