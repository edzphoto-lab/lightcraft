//! Safe Rust entry points for the Apple Core Image RAW decoder.
#![cfg_attr(not(target_os = "macos"), forbid(unsafe_code))]

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native;

#[cfg(target_os = "macos")]
pub use native::{available, decode};

#[cfg(all(test, target_os = "macos"))]
#[test]
#[ignore = "requires a Nikon D600 RAW fixture on a macOS host"]
fn decodes_camera_raw() {
    let fixture = std::env::var_os("LIGHTCRAFT_APPLE_RAW_FIXTURE").expect("set LIGHTCRAFT_APPLE_RAW_FIXTURE");
    let data = std::fs::read(fixture).unwrap();
    let (image, native_edge) = decode(&data, 768).unwrap();
    assert!((6000..=6100).contains(&native_edge));
    assert!((766..=768).contains(&image.width.max(image.height)));
    assert!(image.data.iter().flatten().all(|v| v.is_finite()));
    assert!(image.data.iter().flatten().any(|v| *v > 0.1));
}
