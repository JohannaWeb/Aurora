//! Visual regression tests.
//!
//! Run:  cargo test --test visual_regression
//! Update baselines: UPDATE_SNAPSHOTS=1 cargo test --test visual_regression

use image::RgbaImage;
use std::path::PathBuf;

/// Max allowed pixel diff ratio before a test fails (1%).
const DIFF_THRESHOLD: f64 = 0.01;
/// Per-channel tolerance — pixels within this distance are considered equal.
const PIXEL_TOLERANCE: i32 = 8;

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/screenshots")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
        .join("index.html")
}

fn updating_snapshots() -> bool {
    std::env::var("UPDATE_SNAPSHOTS").is_ok()
}

/// Render a fixture and compare to its baseline snapshot.
fn assert_snapshot(fixture_name: &str, width: u32, height: u32) {
    assert_snapshot_named(fixture_name, fixture_name, width, height);
}

fn assert_snapshot_named(fixture_name: &str, snapshot_name: &str, width: u32, height: u32) {
    let html_path = fixture_path(fixture_name);
    assert!(
        html_path.exists(),
        "Fixture not found: {}",
        html_path.display()
    );

    let rendered = aurora::render::headless::render_fixture_to_image(
        html_path.to_str().unwrap(),
        width,
        height,
    );

    let snapshot_path = snapshots_dir().join(format!("{}.png", snapshot_name));

    if updating_snapshots() {
        rendered
            .save(&snapshot_path)
            .expect("Failed to save snapshot");
        println!("Updated snapshot: {}", snapshot_path.display());
        return;
    }

    if !snapshot_path.exists() {
        // First run — save the baseline.
        rendered
            .save(&snapshot_path)
            .expect("Failed to save initial snapshot");
        println!("Created initial snapshot: {}", snapshot_path.display());
        return;
    }

    let baseline = image::open(&snapshot_path)
        .unwrap_or_else(|_| panic!("Failed to load baseline: {}", snapshot_path.display()))
        .to_rgba8();

    let (diff_ratio, diff_image) = compute_diff(&rendered, &baseline);

    assert!(
        diff_ratio <= DIFF_THRESHOLD,
        "Visual regression in '{}': {:.2}% pixels differ (threshold {:.2}%)\n\
         Diff saved to: {}\n\
         Run UPDATE_SNAPSHOTS=1 cargo test to update baselines.",
        snapshot_name,
        diff_ratio * 100.0,
        DIFF_THRESHOLD * 100.0,
        save_diff(snapshot_name, &diff_image).display()
    );
}

fn save_diff(name: &str, img: &RgbaImage) -> PathBuf {
    let path = snapshots_dir().join(format!("{}-diff.png", name));
    img.save(&path).expect("Failed to save diff image");
    path
}

/// Returns (ratio, diff_image)
fn compute_diff(actual: &RgbaImage, expected: &RgbaImage) -> (f64, RgbaImage) {
    let (w, h) = actual.dimensions();
    let mut diff_img = RgbaImage::new(w, h);

    if expected.dimensions() != (w, h) {
        return (1.0, diff_img);
    }

    let mut diff_count = 0u64;
    for (x, y, pa) in actual.enumerate_pixels() {
        let pb = expected.get_pixel(x, y);
        let dr = (pa.0[0] as i32 - pb.0[0] as i32).abs();
        let dg = (pa.0[1] as i32 - pb.0[1] as i32).abs();
        let db = (pa.0[2] as i32 - pb.0[2] as i32).abs();

        if dr > PIXEL_TOLERANCE || dg > PIXEL_TOLERANCE || db > PIXEL_TOLERANCE {
            diff_count += 1;
            // Highlight diff in red
            diff_img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        } else {
            // Dim background for context
            let gray = ((pa.0[0] as u32 + pa.0[1] as u32 + pa.0[2] as u32) / 3) as u8;
            let val = (gray as f32 * 0.1) as u8;
            diff_img.put_pixel(x, y, image::Rgba([val, val, val, 255]));
        }
    }

    (diff_count as f64 / (w * h) as f64, diff_img)
}

// --- Tests ---

#[test]
fn snapshot_demo() {
    assert_snapshot("demo", 1200, 900);
}

#[test]
fn snapshot_demo_narrow() {
    assert_snapshot_named("demo", "demo-narrow", 960, 540);
}

#[test]
fn snapshot_google_homepage() {
    assert_snapshot("google-homepage", 1338, 786);
}

#[test]
fn snapshot_wikipedia_rust() {
    assert_snapshot("wikipedia-rust", 1440, 900);
}
