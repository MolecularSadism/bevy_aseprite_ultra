use bevy::math::{Vec2, Vec4};
use bevy_aseprite_ultra::prelude::nine_patch_to_slicer;

#[test]
fn nine_patch_to_slicer_basic() {
    // 9-patch center rect: x=4, y=4, w=8, h=8 on a 16x16 slice
    let np = Vec4::new(4.0, 4.0, 8.0, 8.0);
    let size = Vec2::new(16.0, 16.0);
    let slicer = nine_patch_to_slicer(np, size);

    // left = x = 4, top = y = 4
    // right = 16 - 4 - 8 = 4, bottom = 16 - 4 - 8 = 4
    assert_eq!(slicer.border.min_inset, Vec2::new(4.0, 4.0));
    assert_eq!(slicer.border.max_inset, Vec2::new(4.0, 4.0));
}

#[test]
fn nine_patch_to_slicer_asymmetric() {
    // Asymmetric borders: center at (2, 6, 10, 4) on 20x16 slice
    let np = Vec4::new(2.0, 6.0, 10.0, 4.0);
    let size = Vec2::new(20.0, 16.0);
    let slicer = nine_patch_to_slicer(np, size);

    // left = 2, top = 6
    // right = 20 - 2 - 10 = 8, bottom = 16 - 6 - 4 = 6
    assert_eq!(slicer.border.min_inset, Vec2::new(2.0, 6.0));
    assert_eq!(slicer.border.max_inset, Vec2::new(8.0, 6.0));
}

#[test]
fn nine_patch_to_slicer_zero_borders() {
    // Center rect fills entire slice (zero borders)
    let np = Vec4::new(0.0, 0.0, 32.0, 32.0);
    let size = Vec2::new(32.0, 32.0);
    let slicer = nine_patch_to_slicer(np, size);

    assert_eq!(slicer.border.min_inset, Vec2::ZERO);
    assert_eq!(slicer.border.max_inset, Vec2::ZERO);
}
