//! Keyboard listening: Raw Input (default, no hook) plus an optional low-level hook.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RIDEV_INPUTSINK,
    RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub const WM_APP_TAP: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static TARGET: AtomicIsize = AtomicIsize::new(0);

pub unsafe fn register(hwnd: HWND) -> bool {
    let dev = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x06, // keyboard
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    RegisterRawInputDevices(&dev, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) != 0
}

/// Returns (virtual key, is_key_down) for keyboard events only.
pub unsafe fn on_raw_input(lp: LPARAM) -> Option<(u16, bool)> {
    // RAWINPUT must be 8-byte aligned, so use a properly aligned buffer instead of [u8].
    let mut raw: RAWINPUT = std::mem::zeroed();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    let n = GetRawInputData(
        lp as HRAWINPUT,
        RID_INPUT,
        &mut raw as *mut RAWINPUT as *mut c_void,
        &mut size,
        std::mem::size_of::<windows_sys::Win32::UI::Input::RAWINPUTHEADER>() as u32,
    );
    if n == u32::MAX || size == 0 {
        return None;
    }
    let ri = &raw;
    if ri.header.dwType != RIM_TYPEKEYBOARD {
        return None;
    }
    let k = &ri.data.keyboard;
    if k.VKey == 0 || k.VKey == 0xFF {
        return None; // fake keys (print screen, media) carry no useful vk
    }
    let down = k.Message == WM_KEYDOWN || k.Message == WM_SYSKEYDOWN;
    let up = k.Message == WM_KEYUP || k.Message == WM_SYSKEYUP;
    if down || up {
        Some((k.VKey, down))
    } else {
        None
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lp as *const KBDLLHOOKSTRUCT);
        let down = wp as u32 == WM_KEYDOWN || wp as u32 == WM_SYSKEYDOWN;
        let up = wp as u32 == WM_KEYUP || wp as u32 == WM_SYSKEYUP;
        let target = TARGET.load(Ordering::Relaxed);
        if target != 0 && (down || up) {
            PostMessageW(
                target as HWND,
                WM_APP_TAP,
                info.vkCode as usize,
                down as isize,
            );
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wp, lp)
}

pub unsafe fn install_hook(hwnd: HWND) -> bool {
    TARGET.store(hwnd as isize, Ordering::Relaxed);
    if HOOK.load(Ordering::Relaxed) != 0 {
        return true;
    }
    let h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), std::ptr::null_mut(), 0);
    if h.is_null() {
        return false;
    }
    HOOK.store(h as isize, Ordering::Relaxed);
    true
}

pub unsafe fn remove_hook() {
    let h = HOOK.swap(0, Ordering::Relaxed);
    if h != 0 {
        UnhookWindowsHookEx(h as HHOOK);
    }
}
