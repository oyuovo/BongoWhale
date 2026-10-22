use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

/// Offscreen 32-bit ARGB surface we draw into and push to the layered window.
pub struct Surface {
    pub hdc: HDC,
    hbm: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    pub w: i32,
    pub h: i32,
}

unsafe impl Send for Surface {}

impl Surface {
    pub fn new(w: i32, h: i32) -> Option<Surface> {
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let hdc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            if hdc.is_null() {
                return None;
            }
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbm = CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if hbm.is_null() || bits.is_null() {
                DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, hbm as HGDIOBJ);
            Some(Surface { hdc, hbm, old, bits: bits as *mut u8, w, h })
        }
    }

    pub fn bytes(&self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h * 4) as usize) }
    }

    pub fn blit(&mut self, src: &[u8]) {
        let n = ((self.w * self.h * 4) as usize).min(src.len());
        self.bytes()[..n].copy_from_slice(&src[..n]);
    }

    /// Copy with a global alpha multiplier.
    ///
    /// UpdateLayeredWindow ignores SourceConstantAlpha as soon as the blend uses
    /// AC_SRC_ALPHA, so window opacity has to be baked into the pixels.
    /// Premultiplied BGRA scales every channel by the same factor.
    pub fn blit_alpha(&mut self, src: &[u8], alpha: f32) {
        if alpha >= 0.999 {
            self.blit(src);
            return;
        }
        let a = alpha.clamp(0.0, 1.0);
        let dst = self.bytes();
        let n = dst.len().min(src.len());
        let mul = (a * 256.0) as u32;
        for i in 0..n {
            dst[i] = ((src[i] as u32 * mul) >> 8) as u8;
        }
    }

    pub fn present(&self, hwnd: HWND) -> bool {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
            let mut rect: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rect);
            let bf = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                // must stay 255: AC_SRC_ALPHA takes the per-pixel alpha instead
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let dst = POINT { x: rect.left, y: rect.top };
            let size = SIZE { cx: self.w, cy: self.h };
            let src = POINT { x: 0, y: 0 };
            UpdateLayeredWindow(
                hwnd,
                std::ptr::null_mut(),
                &dst,
                &size,
                self.hdc,
                &src,
                0,
                &bf,
                ULW_ALPHA,
            ) != 0
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            DeleteObject(self.hbm as HGDIOBJ);
            DeleteDC(self.hdc);
        }
    }
}
