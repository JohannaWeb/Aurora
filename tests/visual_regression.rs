//! Visual regression tests.
//!
//! Run:  cargo test --test visual_regression
//! Update baselines: UPDATE_SNAPSHOTS=1 cargo test --test visual_regression

use std::path::PathBuf;
use image::RgbaImage;

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

    let snapshot_path = snapshots_dir().join(format!("{}.png", fixture_name));

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

    let diff_ratio = pixel_diff_ratio(&rendered, &baseline);
    assert!(
        diff_ratio <= DIFF_THRESHOLD,
        "Visual regression in '{}': {:.2}% pixels differ (threshold {:.2}%)\n\
         Run UPDATE_SNAPSHOTS=1 cargo test to update baselines.",
        fixture_name,
        diff_ratio * 100.0,
        DIFF_THRESHOLD * 100.0
    );
}

/// Returns the ratio of pixels that differ beyond PIXEL_TOLERANCE.
fn pixel_diff_ratio(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let (w, h) = a.dimensions();
    if b.dimensions() != (w, h) {
        // Size mismatch — treat as fully different.
        return 1.0;
    }

    let total = (w * h) as f64;
    let mut diff_count = 0u64;

    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let dr = (pa.0[0] as i32 - pb.0[0] as i32).abs();
        let dg = (pa.0[1] as i32 - pb.0[1] as i32).abs();
        let db = (pa.0[2] as i32 - pb.0[2] as i32).abs();
        if dr > PIXEL_TOLERANCE || dg > PIXEL_TOLERANCE || db > PIXEL_TOLERANCE {
            diff_count += 1;
        }
    }

    diff_count as f64 / total
}

// --- Tests ---

#[test]
fn snapshot_demo() {
    assert_snapshot("demo", 1200, 900);
}

#[test]
fn snapshot_google_homepage() {
    assert_snapshot("google-homepage", 1338, 786);
}

#[test]
fn snapshot_wikipedia_rust() {
    let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
    let rendered = aurora::render::headless::render_url_to_image(url, 1440, 900);
    let path = snapshots_dir().join("wikipedia-rust.png");
    rendered.save(&path).expect("Failed to save screenshot");
    println!("Screenshot saved to {}", path.display());
}
