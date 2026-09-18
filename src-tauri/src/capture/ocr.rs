//! OCR fallback for windows UI Automation cannot read.
//!
//! GPU-rendered apps (Warp, WezTerm, Alacritty, Electron with accessibility
//! off, ...) expose no UIA text, so the deep-capture readers return None and
//! the event stays title-only. This module is the last-resort path: it grabs
//! the window's pixels with `PrintWindow` into an IN-MEMORY bitmap and runs
//! the OS's built-in WinRT OCR (`Windows.Media.Ocr`) over it.
//!
//! Privacy contract (deliberate, do not weaken):
//!  - The bitmap NEVER touches disk — it lives only as a heap buffer and a
//!    `SoftwareBitmap` for the duration of one call.
//!  - Only extracted TEXT leaves this module, and callers run it through the
//!    same `PrivacyFilter::sanitize_content` as UIA text.
//!  - OCR runs ONLY as a fallback after UIA fails, and only when the
//!    `capture_ocr_fallback` setting is enabled (default on).

#![cfg(target_os = "windows")]

use windows::core::ComInterface;
use windows::Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    GetWindowDC, ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, PW_RENDERFULLCONTENT};

/// Hard cap so a maximized 4K window doesn't produce a multi-second OCR call.
const MAX_OCR_DIMENSION: i32 = 2560;

/// OCR `hwnd` and return the extracted text (post-processed, capped at
/// `max_chars`), or None when the window can't be captured or yields no text.
pub fn ocr_window_text(hwnd: HWND, max_chars: usize) -> Option<String> {
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return None; // minimized windows have no capturable pixels
    }
    let (native_width, native_height) = window_size(hwnd)?;
    if native_width < 50 || native_height < 50 {
        return None; // tooltips / collapsed windows are noise
    }
    // The window renders at its NATIVE size; `target` is only how big a bitmap
    // we hand the OCR engine. Capturing straight into a clamped bitmap would
    // crop to the top-left instead of shrinking the whole window.
    let (width, height) = clamp_dimensions(native_width, native_height);
    let pixels = capture_bgra(hwnd, native_width, native_height, width, height)?;
    let bitmap = software_bitmap_from_bgra(&pixels, width, height).ok()?;

    let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok()?;
    let result = engine.RecognizeAsync(&bitmap).ok()?.get().ok()?;
    let raw = result.Text().ok()?.to_string_lossy();

    let text = postprocess_ocr_text(&raw, max_chars);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn window_size(hwnd: HWND) -> Option<(i32, i32)> {
    let mut rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    Some((width, height))
}

fn clamp_dimensions(width: i32, height: i32) -> (i32, i32) {
    if width <= MAX_OCR_DIMENSION && height <= MAX_OCR_DIMENSION {
        return (width, height);
    }
    let scale = MAX_OCR_DIMENSION as f64 / width.max(height) as f64;
    (
        ((width as f64) * scale) as i32,
        ((height as f64) * scale) as i32,
    )
}

/// Print the window into an in-memory DIB and return its BGRA pixels
/// (top-down rows, `target_width * target_height * 4` bytes).
/// `PW_RENDERFULLCONTENT` asks the window to render its full content including
/// hardware-accelerated layers, which is what makes GPU terminals readable.
/// Some apps ignore it and return black pixels — the caller treats empty OCR
/// output as "no fallback".
///
/// The window always renders at its native size; when `target_*` is smaller
/// (see `clamp_dimensions`) the native bitmap is downscaled with `StretchBlt`.
/// Blitting the window directly into a smaller bitmap would crop it to the
/// top-left corner instead.
fn capture_bgra(
    hwnd: HWND,
    native_width: i32,
    native_height: i32,
    target_width: i32,
    target_height: i32,
) -> Option<Vec<u8>> {
    unsafe {
        let window_dc = GetWindowDC(hwnd);
        if window_dc.is_invalid() {
            return None;
        }
        let src_dc = CreateCompatibleDC(window_dc);
        let src_bitmap = CreateCompatibleBitmap(window_dc, native_width, native_height);
        let src_previous = SelectObject(src_dc, src_bitmap);

        let printed = PrintWindow(hwnd, src_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT));
        // PrintWindow can silently no-op for some composited windows; BitBlt
        // from the window DC is the backup for those.
        if !printed.as_bool() {
            let _ = BitBlt(
                src_dc,
                0,
                0,
                native_width,
                native_height,
                window_dc,
                0,
                0,
                SRCCOPY,
            );
        }

        // Downscale step, only for windows above the OCR size cap. HALFTONE
        // averages source pixels; COLORONCOLOR (the default) drops whole
        // scanlines and would shred small glyphs. HALFTONE requires resetting
        // the brush origin to avoid brush-alignment artifacts.
        let mut dest: Option<(HDC, HBITMAP, HGDIOBJ)> = None;
        let mut stretch_failed = false;
        if target_width != native_width || target_height != native_height {
            let dest_dc = CreateCompatibleDC(window_dc);
            let dest_bitmap = CreateCompatibleBitmap(window_dc, target_width, target_height);
            let dest_previous = SelectObject(dest_dc, dest_bitmap);
            SetStretchBltMode(dest_dc, HALFTONE);
            let _ = SetBrushOrgEx(dest_dc, 0, 0, None);
            stretch_failed = !StretchBlt(
                dest_dc,
                0,
                0,
                target_width,
                target_height,
                src_dc,
                0,
                0,
                native_width,
                native_height,
                SRCCOPY,
            )
            .as_bool();
            dest = Some((dest_dc, dest_bitmap, dest_previous));
        }

        let (read_dc, read_bitmap) = match &dest {
            Some((dc, bitmap, _)) => (*dc, *bitmap),
            None => (src_dc, src_bitmap),
        };

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: target_width,
            biHeight: -target_height, // negative = top-down row order
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut pixels = vec![0u8; (target_width * target_height * 4) as usize];
        // A failed downscale leaves the destination bitmap undefined, so treat
        // it exactly like a failed read rather than OCR'ing garbage.
        let rows = if stretch_failed {
            0
        } else {
            GetDIBits(
                read_dc,
                read_bitmap,
                0,
                target_height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };

        if let Some((dest_dc, dest_bitmap, dest_previous)) = dest {
            SelectObject(dest_dc, dest_previous);
            let _ = DeleteObject(dest_bitmap);
            let _ = DeleteDC(dest_dc);
        }
        SelectObject(src_dc, src_previous);
        let _ = DeleteObject(src_bitmap);
        let _ = DeleteDC(src_dc);
        let _ = ReleaseDC(hwnd, window_dc);

        if rows == 0 {
            return None;
        }
        Some(pixels)
    }
}

/// Wrap raw BGRA pixels in a WinRT `SoftwareBitmap` for the OCR engine.
fn software_bitmap_from_bgra(
    pixels: &[u8],
    width: i32,
    height: i32,
) -> windows::core::Result<SoftwareBitmap> {
    let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, width, height)?;
    let buffer = bitmap.LockBuffer(BitmapBufferAccessMode::Write)?;
    let reference = buffer.CreateReference()?;
    let byte_access: IMemoryBufferByteAccess = reference.cast()?;

    let needed = (width * height * 4) as usize;
    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut capacity = 0u32;
    unsafe {
        byte_access.GetBuffer(&mut ptr, &mut capacity)?;
        if (capacity as usize) < needed || ptr.is_null() {
            return Err(windows::core::Error::from_win32());
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr, needed);
    }
    // `buffer`/`reference` drop here, releasing the lock before OCR reads it.
    Ok(bitmap)
}

/// Clean raw OCR output into capture-grade text: drop lines with no
/// alphanumeric content (OCR noise artifacts), collapse blank runs, cap
/// length. Pure function — the unit-testable seam of this module.
pub(crate) fn postprocess_ocr_text(raw: &str, max_chars: usize) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut last_was_blank = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !last_was_blank && !lines.is_empty() {
                lines.push("");
            }
            last_was_blank = true;
            continue;
        }
        if !trimmed.chars().any(|ch| ch.is_alphanumeric()) {
            // OCR noise: lines of pure punctuation/symbols. Skipped WITHOUT
            // clearing `last_was_blank`, so a noise line sandwiched between
            // blank runs doesn't leak a second separator into the output.
            continue;
        }
        last_was_blank = false;
        lines.push(trimmed);
    }
    while lines.last() == Some(&"") {
        lines.pop();
    }
    let joined = lines.join("\n");
    joined.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postprocess_drops_noise_lines_and_collapses_blanks() {
        let raw = "cargo test\n\n\n\n||| ---\n   \nCompiling taskflow\n@@##$$";
        let cleaned = postprocess_ocr_text(raw, 2000);
        assert_eq!(cleaned, "cargo test\n\nCompiling taskflow");
    }

    #[test]
    fn postprocess_caps_length_and_handles_empty() {
        assert_eq!(postprocess_ocr_text("", 100), "");
        assert_eq!(postprocess_ocr_text("\n\n|||\n", 100), "");
        let long = "word ".repeat(1000);
        assert_eq!(postprocess_ocr_text(&long, 50).chars().count(), 50);
    }

    #[test]
    fn clamp_dimensions_scales_large_windows() {
        assert_eq!(clamp_dimensions(800, 600), (800, 600));
        let (w, h) = clamp_dimensions(5120, 2880);
        assert_eq!(w, MAX_OCR_DIMENSION);
        assert_eq!(h, 1440);
    }

    #[test]
    fn clamp_dimensions_preserves_aspect_on_4k() {
        // The case that used to be silently cropped to the top-left corner.
        for (w, h) in [(3840, 2160), (2560, 1440), (3440, 1440), (1080, 3840)] {
            let (cw, ch) = clamp_dimensions(w, h);
            assert!(cw <= MAX_OCR_DIMENSION && ch <= MAX_OCR_DIMENSION, "{cw}x{ch}");
            assert!(cw > 0 && ch > 0, "{cw}x{ch}");
            let before = w as f64 / h as f64;
            let after = cw as f64 / ch as f64;
            assert!(
                (before - after).abs() < 0.01,
                "aspect drift for {w}x{h}: {before} -> {after}"
            );
        }
    }
}
