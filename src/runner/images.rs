use crate::layout::LayoutBox;
use opus::domain::Identity;
use std::collections::HashMap;

pub type ImageCache = HashMap<String, peniko::ImageData>;

pub(crate) fn load_images(
    root: &LayoutBox,
    base_url: Option<&str>,
    identity: &Identity,
) -> ImageCache {
    let mut urls = Vec::new();
    collect_image_srcs(root, base_url, &mut urls);

    let mut cache = ImageCache::new();
    for url in urls {
        load_image(&url, identity, &mut cache);
    }

    cache
}

fn collect_image_srcs(node: &LayoutBox, base_url: Option<&str>, out: &mut Vec<String>) {
    if let Some(src) = node.image_src() {
        let resolved = if let Some(base) = base_url {
            crate::fetch::resolve_relative_url(base, src).unwrap_or_else(|_| src.to_string())
        } else {
            src.to_string()
        };
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }

    for child in node.children() {
        collect_image_srcs(child, base_url, out);
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
