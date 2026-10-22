//! A tiny self-drawn UI kit for the settings window.
//!
//! The window is opaque and double buffered: background panel and widgets are
//! rasterised into a 32-bit DIB with our own anti-aliased primitives, then the
//! text is drawn on top with GDI, and the result is blitted to the screen in one
//! BitBlt. Controls are described as rows, so adding a setting means adding one
//! row and one id -- no new layout code.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::*;

pub const DT_LEFT: u32 = 0x0000;
pub const DT_CENTER: u32 = 0x0001;
pub const DT_RIGHT: u32 = 0x0002;
pub const DT_VCENTER: u32 = 0x0004;
pub const DT_SINGLELINE: u32 = 0x0020;
pub const DT_NOPREFIX: u32 = 0x0800;

#[link(name = "user32")]
extern "system" {
    fn DrawTextW(hdc: HDC, text: *const u16, len: i32, rc: *mut RECT, flags: u32) -> i32;
}

// ---- palette ----
// Written in RGB and converted, because the DIB is BGRA and writing the two
// orders by hand is exactly the mistake that turns a navy panel brown.
pub const fn c(r: u8, g: u8, b: u8) -> [u8; 4] {
    [b, g, r, 255]
}

pub const BG: [u8; 4] = c(0x1B, 0x21, 0x40); // deep navy, the character's outline colour
pub const HEADER: [u8; 4] = c(0x24, 0x2C, 0x56);
pub const CARD: [u8; 4] = c(0x25, 0x2E, 0x58);
pub const ACCENT: [u8; 4] = c(0x4D, 0x6B, 0xFE); // DeepSeek blue
pub const ACCENT_HOVER: [u8; 4] = c(0x6E, 0x88, 0xFF);
pub const TRACK: [u8; 4] = c(0x33, 0x3D, 0x69);
pub const TEXT: [u8; 4] = c(0xF7, 0xF3, 0xEC);
pub const TEXT_DIM: [u8; 4] = c(0xA9, 0xB4, 0xE0);
pub const KNOB: [u8; 4] = c(0xF7, 0xF3, 0xEC);

pub fn rgb(c: [u8; 4]) -> u32 {
    (c[2] as u32) | ((c[1] as u32) << 8) | ((c[0] as u32) << 16)
}

/// Offscreen 32-bit surface + its memory DC.
pub struct MemDC {
    pub hdc: HDC,
    hbm: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    pub w: i32,
    pub h: i32,
}

impl MemDC {
    pub fn new(w: i32, h: i32) -> Option<MemDC> {
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let hdc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            if hdc.is_null() {
                return None;
            }
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = std::ptr::null_mut();
            let hbm = CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if hbm.is_null() || bits.is_null() {
                DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, hbm as HGDIOBJ);
            Some(MemDC { hdc, hbm, old, bits: bits as *mut u8, w, h })
        }
    }

    pub fn bits(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h * 4) as usize) }
    }

    pub fn flush(&mut self, hwnd: HWND) {
        unsafe {
            let dc = GetDC(hwnd);
            BitBlt(dc, 0, 0, self.w, self.h, self.hdc, 0, 0, SRCCOPY);
            ReleaseDC(hwnd, dc);
        }
    }
}

impl Drop for MemDC {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            DeleteObject(self.hbm as HGDIOBJ);
            DeleteDC(self.hdc);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn has(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

/// Anti-aliased software rasteriser writing straight into the DIB bits.
pub struct Canvas<'a> {
    w: i32,
    h: i32,
    px: &'a mut [u8],
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u8], w: i32, h: i32) -> Self {
        Canvas { w, h, px }
    }

    #[inline]
    pub fn blend_px(&mut self, x: i32, y: i32, c: [u8; 4], cov: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || cov <= 0.0 {
            return;
        }
        let a = (cov * (c[3] as f32 / 255.0)).clamp(0.0, 1.0);
        if a <= 0.002 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        for k in 0..3 {
            let d = self.px[i + k] as f32;
            self.px[i + k] = (c[k] as f32 * a + d * (1.0 - a)).round() as u8;
        }
        self.px[i + 3] = 255;
    }

    pub fn fill(&mut self, c: [u8; 4]) {
        for y in 0..self.h {
            for x in 0..self.w {
                self.blend_px(x, y, c, 1.0);
            }
        }
    }

    pub fn round_rect(&mut self, r: Rect, radius: i32, c: [u8; 4]) {
        let rad = radius.max(0) as f32;
        let cx = r.x as f32 + r.w as f32 / 2.0;
        let cy = r.y as f32 + r.h as f32 / 2.0;
        let hx = r.w as f32 / 2.0 - rad;
        let hy = r.h as f32 / 2.0 - rad;
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let dx = (x as f32 + 0.5 - cx).abs() - hx;
                let dy = (y as f32 + 0.5 - cy).abs() - hy;
                let d = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - rad;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.blend_px(x, y, c, cov);
            }
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, rad: f32, c: [u8; 4]) {
        let x0 = (cx - rad - 2.0).floor() as i32;
        let x1 = (cx + rad + 2.0).ceil() as i32;
        let y0 = (cy - rad - 2.0).floor() as i32;
        let y1 = (cy + rad + 2.0).ceil() as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cov = (rad - d + 0.5).clamp(0.0, 1.0);
                self.blend_px(x, y, c, cov);
            }
        }
    }

    pub fn vgrad(&mut self, r: Rect, top: [u8; 4], bottom: [u8; 4]) {
        for y in r.y..r.bottom() {
            let t = (y - r.y) as f32 / (r.h.max(1) as f32);
            let c = [
                (top[0] as f32 + (bottom[0] as f32 - top[0] as f32) * t) as u8,
                (top[1] as f32 + (bottom[1] as f32 - top[1] as f32) * t) as u8,
                (top[2] as f32 + (bottom[2] as f32 - top[2] as f32) * t) as u8,
                255,
            ];
            for x in r.x..r.right() {
                self.blend_px(x, y, c, 1.0);
            }
        }
    }
}

// ---- text ----

pub struct Fonts {
    pub title: HFONT,
    pub label: HFONT,
    pub small: HFONT,
    pub button: HFONT,
}

impl Fonts {
    pub fn new() -> Fonts {
        unsafe {
            let face = crate::util::wide("Microsoft YaHei UI");
            let mk = |h: i32, w: i32| {
                CreateFontW(
                    -h, 0, 0, 0, w, 0, 0, 0,
                    DEFAULT_CHARSET as u32,
                    OUT_TT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (DEFAULT_PITCH | FF_DONTCARE) as u32,
                    face.as_ptr(),
                )
            };
            Fonts { title: mk(19, 600), label: mk(14, 400), small: mk(12, 400), button: mk(14, 600) }
        }
    }

    pub fn text(&self, dc: HDC, r: Rect, s: &str, font: HFONT, color: [u8; 4], align: u32) {
        unsafe {
            let old = SelectObject(dc, font as HGDIOBJ);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, rgb(color));
            let mut w: Vec<u16> = s.encode_utf16().collect();
            let mut rc = RECT { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() };
            DrawTextW(dc, w.as_mut_ptr(), w.len() as i32, &mut rc, DT_SINGLELINE | DT_NOPREFIX | align);
            SelectObject(dc, old);
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.title as HGDIOBJ);
            DeleteObject(self.label as HGDIOBJ);
            DeleteObject(self.small as HGDIOBJ);
            DeleteObject(self.button as HGDIOBJ);
        }
    }
}
