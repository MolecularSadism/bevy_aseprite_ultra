mod helpers;

use bevy::{
    color::palettes::css,
    image::ImageSamplerDescriptor,
    prelude::*,
};
use bevy_aseprite_ultra::prelude::*;
use helpers::{DefaultFilter, HintText, LayerState, LayerTogglePlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin {
            default_sampler: ImageSamplerDescriptor::nearest(),
        }))
        .add_plugins(AsepriteUltraPlugin)
        .add_plugins(LayerTogglePlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut cmd: Commands, server: Res<AssetServer>) {
    cmd.spawn(Camera2d);

    // Full-screen root
    let root = cmd
        .spawn(Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(20.),
            ..default()
        })
        .id();

    // Title
    cmd.spawn((
        Text::new("Layered Animations in UI"),
        TextFont {
            font_size: 24.,
            ..default()
        },
        TextColor(css::WHITE.into()),
        ChildOf(root),
    ));

    // Row of boxes
    let row = cmd
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexEnd,
                column_gap: Val::Px(30.),
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    // ---- Baked AseAnimation group ----
    let baked_group = cmd
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(10.)),
                border: UiRect::all(Val::Px(2.)),
                border_radius: BorderRadius::all(Val::Px(8.)),
                row_gap: Val::Px(10.),
                ..default()
            },
            BorderColor::all(css::DARK_GRAY),
            ChildOf(row),
        ))
        .id();

    // Layer 1 only via sub-asset label
    cmd.spawn((
        Node {
            width: Val::Px(100.),
            height: Val::Px(100.),
            ..default()
        },
        AseAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite#Layer 1"),
        },
        ImageNode::default(),
        ChildOf(baked_group),
    ));

    // All visible layers composed into one image
    cmd.spawn((
        Node {
            width: Val::Px(100.),
            height: Val::Px(100.),
            ..default()
        },
        AseAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
        },
        ImageNode::default(),
        ChildOf(baked_group),
    ));

    cmd.spawn((
        Text::new("Baked AseAnimation"),
        TextFont {
            font_size: 14.,
            ..default()
        },
        TextColor(css::WHITE.into()),
        ChildOf(baked_group),
    ));

    // ---- AseLayeredAnimation group ----
    let layered_group = cmd
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(10.)),
                border: UiRect::all(Val::Px(2.)),
                border_radius: BorderRadius::all(Val::Px(8.)),
                row_gap: Val::Px(10.),
                ..default()
            },
            BorderColor::all(css::DARK_GRAY),
            ChildOf(row),
        ))
        .id();

    // Layer 1 only
    cmd.spawn((
        Node {
            width: Val::Px(100.),
            height: Val::Px(100.),
            ..default()
        },
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::Include(vec![LayerId::new("Layer 1")]),
            render_target: RenderTarget::Ui,
        },
        DefaultFilter(LayerFilter::Include(vec![LayerId::new("Layer 1")])),
        ChildOf(layered_group),
    ));

    // Layer 1 + Layer 2
    cmd.spawn((
        Node {
            width: Val::Px(100.),
            height: Val::Px(100.),
            ..default()
        },
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::Include(vec![
                LayerId::new("Layer 1"),
                LayerId::new("Layer 2"),
            ]),
            render_target: RenderTarget::Ui,
        },
        DefaultFilter(LayerFilter::Include(vec![
            LayerId::new("Layer 1"),
            LayerId::new("Layer 2"),
        ])),
        ChildOf(layered_group),
    ));

    // All visible layers (Layer 1 + 2 + 3)
    cmd.spawn((
        Node {
            width: Val::Px(100.),
            height: Val::Px(100.),
            ..default()
        },
        AseLayeredAnimation {
            animation: Animation::default(),
            aseprite: server.load("layers.aseprite"),
            layers: LayerFilter::Visible,
            render_target: RenderTarget::Ui,
        },
        DefaultFilter(LayerFilter::Visible),
        ChildOf(layered_group),
    ));

    cmd.spawn((
        Text::new("AseLayeredAnimation"),
        TextFont {
            font_size: 14.,
            ..default()
        },
        TextColor(css::WHITE.into()),
        ChildOf(layered_group),
    ));

    // Bottom hint
    cmd.spawn((
        Text::new(LayerState::AllVisible.hint()),
        TextFont {
            font_size: 18.,
            ..default()
        },
        TextColor(css::GRAY.into()),
        ChildOf(root),
        HintText,
    ));
}
