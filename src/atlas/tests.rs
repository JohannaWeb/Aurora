use super::*;

#[test]
fn test_packer() {
    let mut packer = AtlasPacker::new(256, 256);

    let pos1 = packer.pack(16, 16);
    assert!(pos1.is_some());

    let pos2 = packer.pack(16, 16);
    assert!(pos2.is_some());

    assert_eq!(pos1.unwrap().1, pos2.unwrap().1);
}

#[test]
fn test_atlas_registration() {
    let mut atlas = GlyphAtlas::new(512, 512);
    let bitmap = vec![255; 16 * 16];

    atlas.register_glyph('A', &bitmap, 16, 16, 0, 0, 10.0, 0, 0);

    let metrics = atlas.get_glyph('A');
    assert!(metrics.is_some());
    let m = metrics.unwrap();
    assert_eq!(m.width, 16);
    assert_eq!(m.height, 16);
    assert_eq!(m.advance_width, 10.0);
}
