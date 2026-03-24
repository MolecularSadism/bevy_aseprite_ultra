use bevy_aseprite_ultra::prelude::*;

fn make_aseprite_with_layers(names: &[&str]) -> Aseprite {
    let mut ase = Aseprite::default();
    for &name in names {
        ase.layers.push(LayerEntry::new(LayerId::new(name), true));
    }
    ase
}

#[test]
fn layer_ids_returns_front_to_back_order() {
    let ase = make_aseprite_with_layers(&["top", "middle", "bottom"]);
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], LayerId::new("top"));
    assert_eq!(ids[2], LayerId::new("bottom"));
}

#[test]
fn visible_layer_ids_filters_hidden() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    ase.layers[1].visible = false;
    let visible: Vec<_> = ase.visible_layer_ids().collect();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[0], LayerId::new("a"));
    assert_eq!(visible[1], LayerId::new("c"));
}

#[test]
fn reorder_layer_moves_to_front() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    assert!(ase.reorder_layer(LayerId::new("c"), 0));
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids[0], LayerId::new("c"));
    assert_eq!(ids[1], LayerId::new("a"));
    assert_eq!(ids[2], LayerId::new("b"));
}

#[test]
fn reorder_layer_moves_to_back() {
    let mut ase = make_aseprite_with_layers(&["a", "b", "c"]);
    assert!(ase.reorder_layer(LayerId::new("a"), 99)); // clamped to end
    let ids: Vec<_> = ase.layer_ids().collect();
    assert_eq!(ids[0], LayerId::new("b"));
    assert_eq!(ids[1], LayerId::new("c"));
    assert_eq!(ids[2], LayerId::new("a"));
}

#[test]
fn reorder_nonexistent_layer_returns_false() {
    let mut ase = make_aseprite_with_layers(&["a", "b"]);
    assert!(!ase.reorder_layer(LayerId::new("nope"), 0));
}

#[test]
fn set_layer_visible_toggles() {
    let mut ase = make_aseprite_with_layers(&["a", "b"]);
    assert!(ase.set_layer_visible(LayerId::new("a"), false));
    assert!(!ase.layers[0].visible);
    assert!(ase.set_layer_visible(LayerId::new("a"), true));
    assert!(ase.layers[0].visible);
}
