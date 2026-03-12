use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

pub struct LayerTogglePlugin;

impl Plugin for LayerTogglePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayerState>()
            .add_systems(Update, toggle_layer);
    }
}

#[derive(Resource, Default, Clone, Copy)]
pub enum LayerState {
    #[default]
    AllVisible,
    Layer1Hidden,
    OnlyLayer1,
    FilterVisible,
    FilterOnlyLayer1,
}

impl LayerState {
    pub fn next(self) -> Self {
        match self {
            Self::AllVisible => Self::Layer1Hidden,
            Self::Layer1Hidden => Self::OnlyLayer1,
            Self::OnlyLayer1 => Self::FilterVisible,
            Self::FilterVisible => Self::FilterOnlyLayer1,
            Self::FilterOnlyLayer1 => Self::AllVisible,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::AllVisible => "[Space] All layers visible (visibility)",
            Self::Layer1Hidden => "[Space] Layer 1 hidden (visibility)",
            Self::OnlyLayer1 => "[Space] Only Layer 1 (visibility)",
            Self::FilterVisible => "[Space] LayerFilter::Visible (mutate)",
            Self::FilterOnlyLayer1 => "[Space] LayerFilter: only Layer 1 (mutate)",
        }
    }

    pub fn visibility(self, is_layer1: bool) -> Visibility {
        match self {
            Self::Layer1Hidden if is_layer1 => Visibility::Hidden,
            Self::OnlyLayer1 if !is_layer1 => Visibility::Hidden,
            _ => Visibility::Inherited,
        }
    }
}

#[derive(Component)]
pub struct HintText;

/// Stores the original LayerFilter so we can restore it.
#[derive(Component, Clone)]
pub struct DefaultFilter(pub LayerFilter);

/// Press Space to cycle through visibility toggles and LayerFilter mutations.
fn toggle_layer(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<LayerState>,
    parents: Query<&SpriteLayers>,
    mut layers: Query<(&LayerId, &mut Visibility), With<SpriteLayerOf>>,
    mut textures: Query<(&mut AseTexture, &DefaultFilter)>,
    mut hint: Query<&mut Text, With<HintText>>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    *state = state.next();

    let layer1 = LayerId::new("Layer 1");

    match *state {
        // Visibility-based states: restore original filters, toggle child visibility
        LayerState::AllVisible | LayerState::Layer1Hidden | LayerState::OnlyLayer1 => {
            for (mut tex, default) in &mut textures {
                tex.layers = default.0.clone();
            }
            for sprite_layers in &parents {
                for layer_entity in sprite_layers.iter() {
                    let Ok((id, mut vis)) = layers.get_mut(layer_entity) else {
                        continue;
                    };
                    *vis = state.visibility(*id == layer1);
                }
            }
        }
        // Mutation-based states: mutate the LayerFilter on the component
        LayerState::FilterVisible => {
            for (mut tex, _) in &mut textures {
                tex.layers = LayerFilter::Visible;
            }
        }
        LayerState::FilterOnlyLayer1 => {
            for (mut tex, _) in &mut textures {
                tex.layers = LayerFilter::Include(vec![layer1]);
            }
        }
    }

    for mut text in &mut hint {
        **text = state.hint().into();
    }
}
