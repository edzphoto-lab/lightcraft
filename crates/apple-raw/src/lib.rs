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
    let (image, native_edge) = decode(fixture.to_str().unwrap(), 768).unwrap();
    assert!(native_edge >= 5900, "unexpected native edge: {native_edge}");
    let edge = image.width.max(image.height);
    assert!((640..=768).contains(&edge), "unexpected preview edge: {edge}");
    assert!(image.data.iter().flatten().all(|v| v.is_finite()));
    assert!(image.data.iter().flatten().any(|v| *v > 0.1));
}
