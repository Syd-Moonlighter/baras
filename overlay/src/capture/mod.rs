//! Platform screen capture used by raid-frame detection.
//!
//! Captures include the overlay itself, so callers blank it first.

pub mod analysis;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(all(unix, not(target_os = "macos")))]
mod unix;
#[cfg(all(unix, not(target_os = "macos")))]
use unix as backend;

#[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
mod backend {
    use super::{CaptureError, CapturedImage};

    pub fn capture_region(
        _x: i32,
        _y: i32,
        _width: u32,
        _height: u32,
    ) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::Unsupported(
            "raid-frame capture is not available on this platform".into(),
        ))
    }
}

/// Capture cost, on its own target so it can be filtered on or off by itself:
/// `RUST_LOG=baras::capture=info`. Fires once per detect press.
#[cfg_attr(
    not(any(target_os = "windows", all(unix, not(target_os = "macos")))),
    allow(dead_code)
)]
fn log_timing(
    backend: &str,
    setup: std::time::Duration,
    captured: std::time::Duration,
    cropped: std::time::Duration,
    total: std::time::Duration,
    region: (u32, u32),
) {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    tracing::info!(
        target: "baras::capture",
        backend,
        setup_ms = ms(setup),
        capture_ms = ms(captured),
        crop_ms = ms(cropped),
        total_ms = ms(total),
        region = format!("{}x{}", region.0, region.1),
        "raid-frame capture"
    );
}

/// Map a global logical rectangle onto an output's capture buffer.
///
/// Buffers are device pixels, overlay geometry is logical. The factor comes from
/// the sizes the compositor reported — `wl_output`'s own scale is an integer and
/// wrong under fractional scaling.
///
/// Clamped to the buffer. `None` when the region lies outside it.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub(crate) fn device_region(
    region: (i32, i32, u32, u32),
    output_origin: (i32, i32),
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let (logical_w, logical_h) = logical_size;
    let (buffer_w, buffer_h) = buffer_size;
    if logical_w == 0 || logical_h == 0 || buffer_w == 0 || buffer_h == 0 {
        return None;
    }

    let scale_x = f64::from(buffer_w) / f64::from(logical_w);
    let scale_y = f64::from(buffer_h) / f64::from(logical_h);

    let left = (i64::from(region.0) - i64::from(output_origin.0)) as f64 * scale_x;
    let top = (i64::from(region.1) - i64::from(output_origin.1)) as f64 * scale_y;
    let right = left + f64::from(region.2) * scale_x;
    let bottom = top + f64::from(region.3) * scale_y;

    let x0 = left.round().clamp(0.0, f64::from(buffer_w)) as u32;
    let y0 = top.round().clamp(0.0, f64::from(buffer_h)) as u32;
    let x1 = right.round().clamp(0.0, f64::from(buffer_w)) as u32;
    let y1 = bottom.round().clamp(0.0, f64::from(buffer_h)) as u32;

    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// A captured region, top-down, 4 bytes per pixel.
///
/// A scaled display returns more pixels than were asked for; downsampling would
/// throw away detail recognition wants. Compare `width` against the requested
/// width to recover the factor.
#[derive(Debug)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    /// RGBA, row-major, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// Pixel at `(x, y)` as `(r, g, b, a)`, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize).checked_mul(4)?;
        let pixel = self.rgba.get(i..i.checked_add(4)?)?;
        Some((pixel[0], pixel[1], pixel[2], pixel[3]))
    }

    /// Copy a sub-rectangle out of this image, clamped to its bounds.
    ///
    /// Returns `None` when the requested rectangle lies entirely outside.
    pub fn crop(&self, x: i32, y: i32, width: u32, height: u32) -> Option<CapturedImage> {
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(4)?;
        if self.rgba.len() < expected {
            return None;
        }

        let x0 = i64::from(x).clamp(0, i64::from(self.width)) as u32;
        let y0 = i64::from(y).clamp(0, i64::from(self.height)) as u32;
        let x1 = (i64::from(x) + i64::from(width)).clamp(0, i64::from(self.width)) as u32;
        let y1 = (i64::from(y) + i64::from(height)).clamp(0, i64::from(self.height)) as u32;

        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        let (w, h) = (x1 - x0, y1 - y0);
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for row in y0..y1 {
            let start = (row as usize * self.width as usize + x0 as usize) * 4;
            let end = start + w as usize * 4;
            rgba.extend_from_slice(&self.rgba[start..end]);
        }

        Some(CapturedImage {
            width: w,
            height: h,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_clamps_without_overflowing() {
        let image = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        };

        let crop = image.crop(-1, -1, u32::MAX, u32::MAX).unwrap();
        assert_eq!((crop.width, crop.height), (2, 2));
    }

    #[test]
    fn malformed_image_is_rejected() {
        let image = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![255; 3],
        };

        assert_eq!(image.pixel(0, 0), None);
        assert!(image.crop(0, 0, 1, 1).is_none());
    }

    /// Scaling works on an unscaled machine and silently reads the wrong pixels
    /// everywhere else, so it is tested here rather than against a compositor.
    #[test]
    fn device_region_scales_the_rectangle() {
        // 1:1 — the region is already in device pixels.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1920, 1080), (1920, 1080)),
            Some((100, 50, 400, 200))
        );

        // 2x HiDPI.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1920, 1080), (3840, 2160)),
            Some((200, 100, 800, 400))
        );

        // 1.5x and 1.25x fractional — the cases wl_output's integer scale gets wrong.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1280, 720), (1920, 1080)),
            Some((150, 75, 600, 300))
        );
        assert_eq!(
            device_region((100, 40, 400, 200), (0, 0), (1536, 864), (1920, 1080)),
            Some((125, 50, 500, 250))
        );
    }

    #[test]
    fn device_region_is_relative_to_its_output() {
        // Second monitor starting at x=1920, captured at 2x.
        assert_eq!(
            device_region((2020, 50, 400, 200), (1920, 0), (1920, 1080), (3840, 2160)),
            Some((200, 100, 800, 400))
        );
    }

    #[test]
    fn device_region_clamps_and_rejects_the_unusable() {
        // Overhanging rectangles keep the part that lies on the output.
        assert_eq!(
            device_region((-50, -50, 200, 200), (0, 0), (1920, 1080), (1920, 1080)),
            Some((0, 0, 150, 150))
        );
        // Entirely off the output.
        assert_eq!(
            device_region((4000, 0, 100, 100), (0, 0), (1920, 1080), (1920, 1080)),
            None
        );
        // A compositor that reported nothing usable.
        assert_eq!(device_region((0, 0, 100, 100), (0, 0), (0, 0), (1920, 1080)), None);
    }

    #[test]
    fn oversized_capture_is_rejected_before_the_backend() {
        assert!(matches!(
            capture_region(0, 0, u32::MAX, 1),
            Err(CaptureError::InvalidRegion(_))
        ));
    }
}

#[derive(Debug)]
pub enum CaptureError {
    /// Display server or graphics API unreachable.
    ConnectionFailed(String),
    /// The platform or compositor does not support region capture.
    Unsupported(String),
    /// Requested region is empty or off-screen.
    InvalidRegion(String),
    /// Capture was attempted but produced nothing usable.
    Failed(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::ConnectionFailed(s) => write!(f, "Capture connection failed: {s}"),
            CaptureError::Unsupported(s) => write!(f, "Capture unsupported: {s}"),
            CaptureError::InvalidRegion(s) => write!(f, "Invalid capture region: {s}"),
            CaptureError::Failed(s) => write!(f, "Capture failed: {s}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Capture a screen region in global desktop coordinates.
///
/// The overlay must be hidden first — see the module docs.
pub fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidRegion(format!(
            "{width}x{height} has no area"
        )));
    }
    if width > i32::MAX as u32
        || height > i32::MAX as u32
        || (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none()
    {
        return Err(CaptureError::InvalidRegion(format!(
            "{width}x{height} is too large"
        )));
    }
    backend::capture_region(x, y, width, height)
}
