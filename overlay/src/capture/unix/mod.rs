//! Linux capture: Wayland where the session offers it, X11 otherwise.

mod wayland;
mod x11;

use super::{CaptureError, CapturedImage, device_region, log_timing};

pub fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage, CaptureError> {
    // Same test the overlay window uses, so the two can never disagree: an X11
    // GetImage cannot read a Wayland session's surfaces.
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        wayland::capture_region(x, y, width, height)
    } else {
        x11::capture_region(x, y, width, height)
    }
}
