use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, CreateCompatibleDC, DeleteDC, DeleteObject, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateIconIndirect, CreatePopupMenu, DestroyMenu, DestroyIcon,
    GetCursorPos, PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
};

use crate::config::Config;
use crate::sprite::Frame;
use crate::util::wide;

pub const WM_APP_TRAY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    None,
    Pause,
    Settings,
    Topmost,
    ClickThrough,
    Autostart,
    ResetPos,
    Exit,
}

pub fn make_icon(frame: &Frame, size: u32) -> HICON {
    unsafe {
        let f = frame.scaled(size, size);
        let screen = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), screen);

        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if color.is_null() || bits.is_null() {
            DeleteDC(dc);
            return std::ptr::null_mut();
        }
        std::ptr::copy_nonoverlapping(f.px.as_ptr(), bits as *mut u8, f.px.len());

        let zero = vec![0u8; ((size * size) / 8) as usize];
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, zero.as_ptr() as *const _);

        let ii = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&ii);
        SelectObject(dc, color as _);
        DeleteObject(color as _);
        DeleteObject(mask as _);
        DeleteDC(dc);
        icon
    }
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    added: bool,
}

impl Tray {
    pub fn new(hwnd: HWND, icon: HICON, tip: &str) -> Self {
        let mut t = Tray { hwnd, icon, added: false };
        t.add(tip);
        t
    }

    fn fill(&self, nid: &mut NOTIFYICONDATAW, tip: &str) {
        unsafe {
            std::ptr::write_bytes(nid, 0, 1);
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_APP_TRAY;
            nid.hIcon = self.icon;
            let w = wide(tip);
            let n = w.len().min(nid.szTip.len());
            nid.szTip[..n].copy_from_slice(&w[..n]);
        }
    }

    fn add(&mut self, tip: &str) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            self.fill(&mut nid, tip);
            self.added = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
        }
    }

    #[allow(dead_code)]
    pub fn update_icon(&mut self, icon: HICON, tip: &str) {
        unsafe {
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
            self.icon = icon;
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            self.fill(&mut nid, tip);
            if self.added {
                Shell_NotifyIconW(NIM_MODIFY, &nid);
            }
        }
    }

    pub fn menu(&self, cfg: &Config) -> Cmd {
        unsafe {
            let menu = CreatePopupMenu();
            let pause = if cfg.paused { "继续" } else { "暂停" };
            AppendMenuW(menu, MF_STRING, 1, wide(pause).as_ptr());
            AppendMenuW(menu, MF_STRING, 2, wide("设置…").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, 3, wide("总在最前").as_ptr());
            AppendMenuW(menu, MF_STRING, 4, wide("点击穿透").as_ptr());
            AppendMenuW(menu, MF_STRING, 5, wide("开机自启").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, 6, wide("复位到右下角").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, 7, wide("退出").as_ptr());
            if cfg.always_on_top {
                CheckMenuItem(menu, 3, MF_CHECKED);
            }
            if cfg.click_through {
                CheckMenuItem(menu, 4, MF_CHECKED);
            }
            if cfg.start_with_windows {
                CheckMenuItem(menu, 5, MF_CHECKED);
            }

            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(self.hwnd);
            let id = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                std::ptr::null(),
            ) as i32;
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            match id {
                1 => Cmd::Pause,
                2 => Cmd::Settings,
                3 => Cmd::Topmost,
                4 => Cmd::ClickThrough,
                5 => Cmd::Autostart,
                6 => Cmd::ResetPos,
                7 => Cmd::Exit,
                _ => Cmd::None,
            }
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}
