use vello::kurbo::Affine;
use vello::Scene;

/// Render a parsed usvg tree into a Vello scene at the given position and size.
pub(super) fn render_svg_tree(
    scene: &mut Scene,
    tree: &usvg::Tree,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    let svg_size = tree.size();
    let scale_x = width as f64 / svg_size.width() as f64;
    let scale_y = height as f64 / svg_size.height() as f64;
    let scale = scale_x.min(scale_y);

    let offset_x = x as f64 + (width as f64 - svg_size.width() as f64 * scale) / 2.0;
    let offset_y = y as f64 + (height as f64 - svg_size.height() as f64 * scale) / 2.0;

    let transform = Affine::translate((offset_x, offset_y)) * Affine::scale(scale);

    // vello_svg 0.9.0 requires vello 0.7 / usvg 0.46 which conflicts with our vello 0.9 / usvg 0.47.
    // SVG GPU rendering is stubbed until a compatible vello_svg is available.
    let _ = (tree, transform, scene);
}

/// Parse an SVG string into a usvg tree.
pub fn parse_svg(svg_str: &str) -> Option<usvg::Tree> {
    let options = usvg::Options::default();
    usvg::Tree::from_str(svg_str, &options).ok()
}

/// Parse SVG bytes into a usvg tree.
pub fn parse_svg_bytes(bytes: &[u8]) -> Option<usvg::Tree> {
    let svg_str = std::str::from_utf8(bytes).ok()?;
    parse_svg(svg_str)
}
