//! Pixel-for-pixel guard for refactors that must not change the UI: render the screenshots,
//! keep them as the baseline, change the code, render again and compare.
//!
//! ```text
//! cargo test --bin eve-spai uitest_screenshots -- --ignored
//! cargo test --bin eve-spai uitest_pixel_baseline -- --ignored
//! # refactor
//! cargo test --bin eve-spai uitest_screenshots -- --ignored
//! cargo test --bin eve-spai uitest_pixel_diff -- --ignored --nocapture
//! ```

use super::harness::{out_dir, shot_dir};

/// The showcase scenes load images on threads, so how many frames they run varies and a curve's
/// anti-aliasing shifts by a few pixels. More than this is a real change.
const SHOWCASE_NOISE: usize = 100;

fn shots(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".png") && !n.ends_with(".debug.png"))
        .collect();
    v.sort();
    v
}

/// Differing pixels between two PNGs, or why they cannot be compared.
fn differ(a: &std::path::Path, b: &std::path::Path) -> Result<usize, String> {
    let a = image::open(a).map_err(|e| e.to_string())?.to_rgba8();
    let b = image::open(b).map_err(|e| e.to_string())?.to_rgba8();
    if a.dimensions() != b.dimensions() {
        return Err(format!("size {:?} became {:?}", a.dimensions(), b.dimensions()));
    }
    Ok(a.pixels().zip(b.pixels()).filter(|(p, q)| p != q).count())
}

#[test]
#[ignore = "copies target/uishots to target/uishots-base; run with --ignored"]
fn uitest_pixel_baseline() {
    let (src, dst) = (shot_dir(), out_dir("uishots-base"));
    for n in shots(&dst) {
        std::fs::remove_file(dst.join(n)).unwrap();
    }
    let names = shots(&src);
    assert!(!names.is_empty(), "no screenshots: run uitest_screenshots first");
    for n in &names {
        std::fs::copy(src.join(n), dst.join(n)).unwrap();
    }
    println!("baseline: {} screenshots", names.len());
}

#[test]
#[ignore = "compares target/uishots with target/uishots-base; run with --ignored"]
fn uitest_pixel_diff() {
    let (now, base) = (shot_dir(), out_dir("uishots-base"));
    let names = shots(&base);
    assert!(!names.is_empty(), "no baseline: run uitest_pixel_baseline first");
    let (mut bad, mut noise) = (Vec::new(), Vec::new());
    for n in &names {
        let (a, b) = (base.join(n), now.join(n));
        if !b.exists() {
            bad.push(format!("{n}: missing"));
            continue;
        }
        match differ(&a, &b) {
            Ok(0) => {}
            Ok(px) if n.starts_with("showcase_") && px <= SHOWCASE_NOISE => noise.push(format!("{n}: {px} pixels")),
            Ok(px) => bad.push(format!("{n}: {px} pixels")),
            Err(e) => bad.push(format!("{n}: {e}")),
        }
    }
    let added: Vec<_> = shots(&now).into_iter().filter(|n| !names.contains(n)).collect();
    println!("{} compared, {} differ, {} new: {added:?}", names.len(), bad.len(), added.len());
    if !noise.is_empty() {
        println!("within the showcase noise: {}", noise.join(", "));
    }
    assert!(bad.is_empty(), "screenshots changed:\n{}", bad.join("\n"));
}
