//! Core Image develops the original RAW. LightCraft receives the already tone-mapped pixels
//! as floating-point, linear Rec.2020 for its non-destructive edits and colour-managed output.
use std::ffi::c_void;
use std::ptr::{NonNull, null};

use lightcraft_raster::Rgb32f;
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_foundation::{NSString, NSURL};

#[link(name = "CoreImage", kind = "framework")]
unsafe extern "C" {
    static kCIFormatRGBAf: i32;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGColorSpaceExtendedLinearITUR_2020: *const c_void;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    fn CGImageGetWidth(image: *const c_void) -> usize;
    fn CGImageGetHeight(image: *const c_void) -> usize;
    fn CGImageGetBitsPerPixel(image: *const c_void) -> usize;
    fn CGImageGetBitsPerComponent(image: *const c_void) -> usize;
    fn CGImageGetBytesPerRow(image: *const c_void) -> usize;
    fn CGImageGetBitmapInfo(image: *const c_void) -> u32;
    fn CGImageGetDataProvider(image: *const c_void) -> *const c_void;
    fn CGDataProviderCopyData(provider: *const c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    fn CFDataGetLength(data: *const c_void) -> isize;
}

/// Owns one reference returned by a Core Foundation Create/Copy function.
struct OwnedCf(NonNull<c_void>);

impl OwnedCf {
    fn new(pointer: *mut c_void) -> Option<Self> {
        NonNull::new(pointer).map(Self)
    }

    fn as_ptr(&self) -> *const c_void {
        self.0.as_ptr()
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        // SAFETY: only retained references from Create/Copy are placed in this wrapper.
        unsafe { CFRelease(self.as_ptr()) };
    }
}

pub fn available() -> bool {
    // Looking up the class rather than referencing its symbol permits macOS 11 to use the
    // LightCraft decoder. CIRAWFilter is available from macOS 12.
    AnyClass::get(c"CIRAWFilter").is_some()
}

pub fn decode(path: &str, max_edge: usize) -> Result<(Rgb32f, usize), &'static str> {
    autoreleasepool(|_| {
        let raw_class = AnyClass::get(c"CIRAWFilter").ok_or("CIRAWFilter is unavailable")?;
        let context_class = AnyClass::get(c"CIContext").ok_or("CIContext is unavailable")?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: the documented Core Image selectors and their argument/return encodings.
        // Give the RAW filter the file URL so ImageIO can use the extension to select the camera
        // decoder. Sniffing NEF data without a type hint can select the container's TIFF thumbnail.
        unsafe {
            let raw: Option<Retained<AnyObject>> = msg_send![raw_class, filterWithImageURL: &*url];
            let raw = raw.ok_or("Apple did not recognize the RAW data")?;
            let native: CGSize = msg_send![&raw, nativeSize];
            let edge = native.width.max(native.height);
            if !edge.is_finite() || edge <= 0.0 || max_edge == 0 {
                return Err("Apple reported invalid RAW dimensions");
            }
            if edge > max_edge as f64 {
                let scale = (max_edge as f64 / edge) as f32;
                let _: () = msg_send![&raw, setScaleFactor: scale];
            }
            let image: Option<Retained<AnyObject>> = msg_send![&raw, outputImage];
            let image = image.ok_or("Apple did not produce a RAW image")?;
            let bounds: CGRect = msg_send![&image, extent];
            let pixels = bounds.size.width * bounds.size.height;
            if !pixels.is_finite() || !(1.0..=128_000_000.0).contains(&pixels) {
                return Err("Apple's RAW output exceeds the pixel limit or has invalid dimensions");
            }
            let context: Option<Retained<AnyObject>> = msg_send![context_class, contextWithOptions: null::<AnyObject>()];
            let context = context.ok_or("could not create a Core Image context")?;
            let space = OwnedCf::new(CGColorSpaceCreateWithName(kCGColorSpaceExtendedLinearITUR_2020)).ok_or("could not create linear Rec.2020")?;
            let cg: *mut c_void = msg_send![&context, createCGImage: &*image, fromRect: bounds, format: kCIFormatRGBAf, colorSpace: space.as_ptr()];
            let cg = OwnedCf::new(cg).ok_or("Core Image could not render a float bitmap")?;
            let mut rgb = pixels_from_image(&cg).ok_or("Core Image returned an unreadable float bitmap")?;
            let native_long = edge.round() as usize;
            if rgb.width.max(rgb.height) > max_edge {
                rgb = lightcraft_raster::resample::fit(&rgb, max_edge, max_edge, lightcraft_raster::resample::Filter::Box);
            }
            Ok((rgb, native_long))
        }
    })
}

fn pixels_from_image(image: &OwnedCf) -> Option<Rgb32f> {
    // SAFETY: the caller owns a CGImage from CIContext. The copied data owns its storage
    // independently of the provider and stays alive for every read of the validated slice.
    let (width, height, stride, flags, data, bytes) = unsafe {
        let ptr = image.as_ptr();
        let (width, height) = (CGImageGetWidth(ptr), CGImageGetHeight(ptr));
        let (stride, flags) = (CGImageGetBytesPerRow(ptr), CGImageGetBitmapInfo(ptr));
        if CGImageGetBitsPerPixel(ptr) != 128 || CGImageGetBitsPerComponent(ptr) != 32 || flags & 0x100 == 0 {
            return None;
        }
        let row_len = width.checked_mul(16)?;
        if width == 0 || height == 0 || stride < row_len || width.checked_mul(height)? > 128_000_000 {
            return None;
        }
        let provider = NonNull::new(CGImageGetDataProvider(ptr).cast_mut())?;
        let data = OwnedCf::new(CGDataProviderCopyData(provider.as_ptr()))?;
        let length = usize::try_from(CFDataGetLength(data.as_ptr())).ok()?;
        let start = NonNull::new(CFDataGetBytePtr(data.as_ptr()).cast_mut())?;
        let needed = stride.checked_mul(height - 1)?.checked_add(row_len)?;
        if length < needed || needed > isize::MAX as usize {
            return None;
        }
        (width, height, stride, flags, data, std::slice::from_raw_parts(start.as_ptr(), needed))
    };
    let alpha = flags & 0x1f;
    let offset = if matches!(alpha, 2 | 4 | 6) { 4 } else { 0 };
    let big_endian = flags & 0x7000 == 0x4000; // kCGBitmapByteOrder32Big
    let channel = |p: &[u8], at: usize| {
        let b: [u8; 4] = std::array::from_fn(|i| p[at + i]);
        if big_endian { f32::from_be_bytes(b) } else { f32::from_le_bytes(b) }
    };
    let mut rgb = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = &bytes[y * stride..y * stride + width * 16];
        rgb.extend(row.chunks_exact(16).map(|p| {
            let opacity = match alpha {
                1 => channel(p, 12),
                2 => channel(p, 0),
                _ => 1.0,
            };
            std::array::from_fn(|i| {
                let v = channel(p, offset + i * 4);
                if v.is_finite() && opacity > 0.0 { v / opacity } else { 0.0 }
            })
        }));
    }
    drop(data);
    Some(Rgb32f { width, height, data: rgb })
}
