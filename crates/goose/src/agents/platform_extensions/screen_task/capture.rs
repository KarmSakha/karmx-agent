//! Screen capture: `/usr/sbin/screencapture`, cropped to the front window's
//! AX frame. All geometry the rest of the module sees is in screen *points*.

use super::items::Frame;
use anyhow::{bail, Context};
use std::path::{Path, PathBuf};

/// The capture on disk plus the geometry needed to map pixels ↔ points.
pub struct Capture {
    pub path: PathBuf,
    /// Window (or full-display) rect the image covers, in screen points.
    pub region: Frame,
    /// image pixels per screen point (2.0 on Retina).
    pub scale: f64,
    /// Cropped image size in pixels — drives `where` hints for Jev.
    pub image_w: f64,
    pub image_h: f64,
}

/// Capture the frontmost app: full screen grabbed once, then cropped to the
/// window's AX frame (plus a thin strip above it for the menu bar, like
/// clicker's crop). Falls back to the whole display when no window resolves.
pub fn grab(window: Option<Frame>) -> anyhow::Result<Capture> {
    let path = std::env::temp_dir().join(format!("karmx-screen-{}.png", std::process::id()));
    let output = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", path.to_string_lossy().as_ref()])
        .output()
        .context("could not run screencapture")?;
    if !output.status.success() {
        bail!(
            "Screen capture failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    crop(&path, window)
}

fn crop(path: &Path, window: Option<Frame>) -> anyhow::Result<Capture> {
    let mut img = image::open(path).context("could not read capture")?;
    let display = display_bounds();
    let scale = img.width() as f64 / display.w.max(1.0);
    let region = window.unwrap_or(display);
    let (px, pw) = span(region.x, region.w, display.x, display.w, scale, img.width());
    let (py, ph) = span(
        region.y,
        region.h,
        display.y,
        display.h,
        scale,
        img.height(),
    );
    img = img.crop_imm(px, py, pw.max(1), ph.max(1));
    img.save(path).context("could not write crop")?;
    Ok(Capture {
        path: path.to_path_buf(),
        region,
        scale,
        image_w: pw as f64,
        image_h: ph as f64,
    })
}

/// Project `origin,len` (points) into `display` then to pixels, clamped.
fn span(origin: f64, len: f64, d_origin: f64, d_len: f64, scale: f64, px_max: u32) -> (u32, u32) {
    let lo = ((origin - d_origin).max(0.0).min(d_len) * scale) as u32;
    let hi = ((origin + len - d_origin).max(0.0).min(d_len) * scale) as u32;
    (
        lo.min(px_max),
        hi.saturating_sub(lo).min(px_max - lo.min(px_max)),
    )
}

/// Main display bounds in points (top-left origin like AX).
fn display_bounds() -> Frame {
    #[cfg(target_os = "macos")]
    {
        let d = core_graphics::display::CGDisplay::main();
        let b = d.bounds();
        Frame {
            x: b.origin.x,
            y: b.origin.y,
            w: b.size.width,
            h: b.size.height,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Frame {
            x: 0.0,
            y: 0.0,
            w: 2560.0,
            h: 1440.0,
        }
    }
}
