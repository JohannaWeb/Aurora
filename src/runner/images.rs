use crate::identity::Identity;
use crate::layout::LayoutBox;
use std::collections::HashMap;

pub type ImageCache = HashMap<String, peniko::ImageData>;
pub type SvgCache = HashMap<String, usvg::Tree>;

#[derive(Clone)]
struct ImageSource {
    raw: String,
    resolved: String,
}

pub(crate) fn load_images(
    root: &LayoutBox,
    base_url: Option<&str>,
    identity: &Identity,
) -> ImageCache {
    let mut cache = ImageCache::new();
    load_missing_images(root, base_url, identity, &mut cache);
    cache
}

pub(crate) fn load_missing_images(
    root: &LayoutBox,
    base_url: Option<&str>,
    identity: &Identity,
    cache: &mut ImageCache,
) {
    let mut urls = Vec::new();
    collect_image_srcs(root, base_url, &mut urls);

    for source in urls {
        if !is_svg_url(&source.resolved) {
            if cache.contains_key(&source.resolved) {
                if source.raw != source.resolved {
                    if let Some(image) = cache.get(&source.resolved).cloned() {
                        cache.insert(source.raw, image);
                    }
                }
                continue;
            }
            load_image(&source.resolved, identity, cache);
            if source.raw != source.resolved {
                if let Some(image) = cache.get(&source.resolved).cloned() {
                    cache.insert(source.raw, image);
                }
            }
        }
    }
}

pub(crate) fn load_svgs(root: &LayoutBox, base_url: Option<&str>, identity: &Identity) -> SvgCache {
    let mut cache = SvgCache::new();
    load_missing_svgs(root, base_url, identity, &mut cache);
    cache
}

pub(crate) fn load_missing_svgs(
    root: &LayoutBox,
    base_url: Option<&str>,
    identity: &Identity,
    cache: &mut SvgCache,
) {
    let mut urls = Vec::new();
    collect_image_srcs(root, base_url, &mut urls);

    for source in urls {
        if is_svg_url(&source.resolved) {
            if cache.contains_key(&source.resolved) {
                if source.raw != source.resolved {
                    if let Some(tree) = cache.get(&source.resolved).cloned() {
                        cache.insert(source.raw, tree);
                    }
                }
                continue;
            }
            load_svg(&source.resolved, identity, cache);
            if source.raw != source.resolved {
                if let Some(tree) = cache.get(&source.resolved).cloned() {
                    cache.insert(source.raw, tree);
                }
            }
        }
    }
}

fn is_svg_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.ends_with(".svg") || lower.ends_with(".svgz") || lower.contains("image/svg")
}

fn collect_image_srcs(node: &LayoutBox, base_url: Option<&str>, out: &mut Vec<ImageSource>) {
    if let Some(src) = node.image_src() {
        push_image_source(src, base_url, out);
    }
    if let Some(poster) = node.media_poster() {
        push_image_source(poster, base_url, out);
    }

    for child in node.children() {
        collect_image_srcs(child, base_url, out);
    }
}

fn push_image_source(src: &str, base_url: Option<&str>, out: &mut Vec<ImageSource>) {
    let resolved = if let Some(base) = base_url {
        crate::fetch::resolve_relative_url(base, src).unwrap_or_else(|_| src.to_string())
    } else {
        src.to_string()
    };
    if !out.iter().any(|source| source.resolved == resolved) {
        out.push(ImageSource {
            raw: src.to_string(),
            resolved,
        });
    }
}

fn load_image(url: &str, identity: &Identity, cache: &mut ImageCache) {
    match crate::fetch::fetch_bytes(url, identity) {
        Ok(bytes) => match image::load_from_memory(&bytes) {
            Ok(dyn_img) => {
                cache.insert(url.to_string(), image_data_from_dynamic(dyn_img));
            }
            Err(e) => eprintln!("Aurora: failed to decode image {url}: {e}"),
        },
        Err(e) => eprintln!("Aurora: failed to fetch image {url}: {e}"),
    }
}

fn load_svg(url: &str, identity: &Identity, cache: &mut SvgCache) {
    match crate::fetch::fetch_bytes(url, identity) {
        Ok(bytes) => match usvg::Tree::from_data(&bytes, &usvg::Options::default()) {
            Ok(tree) => {
                cache.insert(url.to_string(), tree);
            }
            Err(e) => eprintln!("Aurora: failed to parse SVG {url}: {e}"),
        },
        Err(e) => eprintln!("Aurora: failed to fetch SVG {url}: {e}"),
    }
}

fn image_data_from_dynamic(dyn_img: image::DynamicImage) -> peniko::ImageData {
    let rgba = dyn_img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    peniko::ImageData {
        data: peniko::Blob::from(rgba.into_raw()),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width,
        height,
    }
}
