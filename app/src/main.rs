#![windows_subsystem = "windows"]

mod anim;
mod assets;
mod autostart;
mod config;
mod input;
mod keys;
mod placeholder;
mod render;
mod settings;
mod sprite;
mod tray;
mod ui;
mod util;

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTONULL,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use anim::Animator;
use config::Config;
use render::Surface;
use sprite::{ClipId, Sprites};
use tray::{Cmd, Tray};

static APP: AtomicIsize = AtomicIsize::new(0);

const TIMER_ANIM: usize = 1;
const TIMER_HOVER: usize = 2;
const DESIGN_SIZE: f32 = 256.0;

/// Pointer-rest fade: the pet gets out of the way instead of blocking clicks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HoverState {
    Idle,
    Pending(u64),
    Hiding,
    Hidden,
    Showing,
}

pub struct App {
    pub cfg: Config,
    pub(crate) base: Sprites,
    sprites: Sprites,
    surface: Surface,
    anim: Animator,
    hwnd: HWND,
    drag: Option<(i32, i32, i32, i32)>,
    size: i32,
    dpi: f32,
    tray: Option<Tray>,
    hook_on: bool,
    /// Physical keys currently held down, so auto-repeat does not machine-gun.
    pressed: [bool; 256],
    hover: HoverState,
    hover_alpha: f32,
}

pub(crate) fn app_ptr() -> Option<&'static mut App> {
    let p = APP.load(Ordering::Relaxed);
    if p == 0 {
        None
    } else {
        Some(unsafe { &mut *(p as *mut App) })
    }
}

fn topmost() -> HWND {
    -1isize as HWND
}

impl App {
    fn scale_target(&self) -> i32 {
        (DESIGN_SIZE * self.cfg.scale * self.dpi / 96.0).round() as i32
    }

    fn rebuild(&mut self) {
        let size = self.scale_target();
        self.size = size;
        self.sprites = scale_sprites(&self.base, size);
        self.surface = Surface::new(size, size).expect("layered surface");
        let (c, f) = self.anim.current();
        self.draw(c, f);
        unsafe {
            SetWindowPos(self.hwnd, std::ptr::null_mut(), 0, 0, size, size, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    fn draw(&mut self, clip: ClipId, frame: usize) {
        let clip = if self.sprites.get(clip).is_some() { clip } else { ClipId::Idle };
        let Some(c) = self.sprites.get(clip) else { return };
        let Some(f) = c.frames.get(frame.min(c.frames.len().saturating_sub(1))) else { return };
        self.surface.blit_alpha(&f.px, self.opacity_now());
        self.surface.present(self.hwnd);
    }

    fn redraw(&mut self) {
        let (c, f) = self.anim.current();
        self.draw(c, f);
    }

    fn arm_timer(&self) {
        unsafe {
            let d = self.anim.next_deadline(&self.sprites);
            let now = util::now_ms();
            let delta = d.saturating_sub(now).clamp(16, 30_000) as u32;
            SetTimer(self.hwnd, TIMER_ANIM, delta, None);
        }
    }

    fn opacity_now(&self) -> f32 {
        (self.cfg.opacity * self.hover_alpha).clamp(0.05, 1.0)
    }

    fn tap(&mut self, side: Option<keys::Side>) {
        if self.anim.tap(side) {
            self.redraw();
        }
        self.arm_timer();
    }

    /// Physical key transition. Auto-repeat arrives as repeated key-downs for a
    /// key that is already down, so the pressed table filters them out.
    fn key(&mut self, vk: u16, down: bool) {
        let i = vk as usize;
        if down {
            if i < 256 {
                if self.pressed[i] {
                    return;
                }
                self.pressed[i] = true;
            }
            self.tap(Some(keys::side_for(vk)));
        } else if i < 256 {
            self.pressed[i] = false;
        }
    }

    fn cursor_inside(&self) -> bool {
        unsafe {
            let mut p = POINT { x: 0, y: 0 };
            GetCursorPos(&mut p);
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut r);
            p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
        }
    }

    /// The window passes clicks through when the user asked for it, or while the
    /// pointer-rest fade has finished.
    fn sync_ex_style(&self) {
        unsafe {
            let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            if self.cfg.always_on_top {
                ex |= WS_EX_TOPMOST;
            }
            if self.cfg.click_through || matches!(self.hover, HoverState::Hidden) {
                ex |= WS_EX_TRANSPARENT;
            }
            let cur = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32;
            if cur != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex as isize);
                SetWindowPos(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }

    fn begin_hover(&mut self) {
        if !self.cfg.hide_on_hover || self.cfg.click_through {
            return;
        }
        if matches!(self.hover, HoverState::Idle) {
            self.hover = HoverState::Pending(util::now_ms());
            unsafe {
                SetTimer(self.hwnd, TIMER_HOVER, 60, None);
            }
        }
    }

    fn hover_tick(&mut self) {
        const MIN_ALPHA: f32 = 0.18;
        const STEP: f32 = 0.11;
        let now = util::now_ms();
        let inside = self.cursor_inside();
        let mut next = 0u32;
        match self.hover {
            HoverState::Pending(since) => {
                if !inside {
                    self.hover = HoverState::Idle;
                } else if now.saturating_sub(since) >= self.cfg.hover_hide_delay_ms {
                    self.hover = HoverState::Hiding;
                    next = 33;
                } else {
                    next = 60;
                }
            }
            HoverState::Hiding => {
                self.hover_alpha = (self.hover_alpha - STEP).max(MIN_ALPHA);
                if self.hover_alpha <= MIN_ALPHA {
                    self.hover = HoverState::Hidden;
                    self.sync_ex_style();
                    next = 130;
                } else {
                    next = 33;
                }
            }
            HoverState::Hidden => {
                if !inside {
                    self.hover = HoverState::Showing;
                    next = 33;
                } else {
                    next = 130;
                }
            }
            HoverState::Showing => {
                self.hover_alpha = (self.hover_alpha + STEP).min(1.0);
                if self.hover_alpha >= 1.0 {
                    self.hover = HoverState::Idle;
                    self.sync_ex_style();
                } else {
                    next = 33;
                }
            }
            HoverState::Idle => {}
        }
        if self.hover != HoverState::Idle || next != 0 {
            self.redraw();
        }
        unsafe {
            if next == 0 {
                KillTimer(self.hwnd, TIMER_HOVER);
            } else {
                SetTimer(self.hwnd, TIMER_HOVER, next, None);
            }
        }
    }

    /// A saved position can point at a monitor that no longer exists.
    fn ensure_on_screen(&mut self) {
        unsafe {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut r);
            let center = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
            if !MonitorFromPoint(center, MONITOR_DEFAULTTONULL).is_null() {
                return;
            }
            self.cfg.offset_x = 24;
            self.cfg.offset_y = 24;
            self.cfg.anchor = "bottom-right".into();
            self.place();
            self.cfg.save();
        }
    }

    fn work_area(&self) -> RECT {
        unsafe {
            let mon = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut mi) != 0 {
                mi.rcWork
            } else {
                let mut r: RECT = std::mem::zeroed();
                SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut _ as *mut _, 0);
                r
            }
        }
    }

    fn place(&self) {
        let wa = self.work_area();
        let (x, y) = match self.cfg.anchor.as_str() {
            "top-left" => (wa.left + self.cfg.offset_x, wa.top + self.cfg.offset_y),
            "top-right" => (wa.right - self.size - self.cfg.offset_x, wa.top + self.cfg.offset_y),
            "bottom-left" => (wa.left + self.cfg.offset_x, wa.bottom - self.size - self.cfg.offset_y),
            _ => (wa.right - self.size - self.cfg.offset_x, wa.bottom - self.size - self.cfg.offset_y),
        };
        unsafe {
            SetWindowPos(self.hwnd, topmost(), x, y, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW);
        }
    }

    pub(crate) fn reset_position(&mut self) {
        self.cfg.offset_x = 24;
        self.cfg.offset_y = 24;
        self.cfg.anchor = "bottom-right".into();
        self.cfg.save();
        self.place();
    }

    fn save_position(&mut self) {
        unsafe {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut r);
            let wa = self.work_area();
            // A drag can park the pet under the taskbar or off the edge of the
            // screen, where it is effectively lost. Clamp it back into the work
            // area before anything is persisted.
            let x = r.left.clamp(wa.left, (wa.right - self.size).max(wa.left));
            let y = r.top.clamp(wa.top, (wa.bottom - self.size).max(wa.top));
            if x != r.left || y != r.top {
                SetWindowPos(
                    self.hwnd,
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
                GetWindowRect(self.hwnd, &mut r);
            }
            self.cfg.offset_x = (wa.right - r.right).max(0);
            self.cfg.offset_y = (wa.bottom - r.bottom).max(0);
            self.cfg.anchor = "bottom-right".into();
            self.cfg.save();
        }
    }

    fn sync_hook(&mut self) {
        unsafe {
            if self.cfg.keyboard_hook && !self.hook_on {
                self.hook_on = input::install_hook(self.hwnd);
            } else if !self.cfg.keyboard_hook && self.hook_on {
                input::remove_hook();
                self.hook_on = false;
            }
        }
    }

    pub(crate) fn apply_config(&mut self, allow_resize: bool) {
        self.cfg.clamp();
        self.anim.paused = self.cfg.paused;
        self.anim.set_sleep_after(self.cfg.sleep_after_secs);
        self.sync_hook();

        self.sync_ex_style();
        unsafe {
            SetWindowPos(
                self.hwnd,
                if self.cfg.always_on_top { topmost() } else { std::ptr::null_mut() },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }

        if allow_resize && self.dpi > 0.0 {
            let want = self.scale_target();
            if want != self.size {
                self.rebuild();
                self.place();
            } else {
                self.redraw();
            }
        } else {
            self.redraw();
        }
        self.cfg.save();
    }

    fn on_tray(&mut self) {
        let cmd = match &self.tray {
            Some(t) => t.menu(&self.cfg),
            None => Cmd::None,
        };
        match cmd {
            Cmd::Pause => {
                self.cfg.paused = !self.cfg.paused;
                self.anim.paused = self.cfg.paused;
                if !self.cfg.paused {
                    self.anim.poke();
                }
                self.cfg.save();
                self.arm_timer();
            }
            Cmd::Settings => settings::open(self.hwnd),
            Cmd::Topmost => {
                self.cfg.always_on_top = !self.cfg.always_on_top;
                self.apply_config(false);
            }
            Cmd::ClickThrough => {
                self.cfg.click_through = !self.cfg.click_through;
                self.apply_config(false);
            }
            Cmd::Autostart => {
                self.cfg.start_with_windows = !self.cfg.start_with_windows;
                autostart::set(self.cfg.start_with_windows);
                self.cfg.save();
            }
            Cmd::ResetPos => self.reset_position(),
            Cmd::Exit => unsafe {
                DestroyWindow(self.hwnd);
            },
            Cmd::None => {}
        }
    }
}

fn scale_sprites(base: &Sprites, size: i32) -> Sprites {
    let mut out = Sprites::new();
    out.canvas = size as u32;
    out.icon = base.icon.as_ref().map(|f| f.scaled(size as u32, size as u32));
    for id in ClipId::all() {
        if let Some(c) = base.get(id) {
            out.insert(
                id,
                sprite::Clip {
                    fps: c.fps,
                    looping: c.looping,
                    frames: c.frames.iter().map(|f| f.scaled(size as u32, size as u32)).collect(),
                },
            );
        }
    }
    out
}

unsafe fn handle(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = app_ptr() else { return 0 };
    match msg {
        WM_TIMER => {
            if wp == TIMER_ANIM {
                KillTimer(hwnd, TIMER_ANIM);
                let mut dirty = false;
                if let Some((c, f)) = a.anim.tick(&a.sprites) {
                    a.draw(c, f);
                    dirty = true;
                }
                let _ = dirty;
                a.arm_timer();
            } else if wp == TIMER_HOVER {
                KillTimer(hwnd, TIMER_HOVER);
                a.hover_tick();
            }
            0
        }
        WM_INPUT => {
            if let Some((vk, down)) = input::on_raw_input(lp) {
                a.key(vk, down);
            }
            0
        }
        input::WM_APP_TAP => {
            a.key(wp as u16, lp != 0);
            0
        }
        tray::WM_APP_TRAY => {
            let ev = util::loword(lp as usize);
            const WM_RBUTTONUP_C: u32 = 0x0205;
            const WM_CONTEXTMENU_C: u32 = 0x007B;
            const WM_LBUTTONDBLCLK_C: u32 = 0x0203;
            if ev == WM_RBUTTONUP_C || ev == WM_CONTEXTMENU_C {
                a.on_tray();
            } else if ev == WM_LBUTTONDBLCLK_C {
                settings::open(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let mut p = POINT { x: 0, y: 0 };
            GetCursorPos(&mut p);
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            a.drag = Some((p.x, p.y, r.left, r.top));
            if a.cfg.react_to_mouse {
                a.tap(None);
            }
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            a.begin_hover();
            if let Some((ox, oy, wx, wy)) = a.drag {
                let mut p = POINT { x: 0, y: 0 };
                GetCursorPos(&mut p);
                SetWindowPos(hwnd, std::ptr::null_mut(), wx + (p.x - ox), wy + (p.y - oy), 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
            0
        }
        WM_LBUTTONUP => {
            if a.drag.is_some() {
                ReleaseCapture();
                a.drag = None;
                a.save_position();
            }
            0
        }
        WM_RBUTTONUP => {
            a.on_tray();
            0
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            a.dpi = unsafe { GetDpiForWindow(hwnd) as f32 };
            if a.dpi <= 0.0 {
                a.dpi = 96.0;
            }
            a.rebuild();
            a.place();
            a.ensure_on_screen();
            0
        }
        WM_DESTROY => {
            a.cfg.save();
            input::remove_hook();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, APP.load(Ordering::Relaxed) as isize);
            let _ = cs;
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => handle(hwnd, msg, wp, lp),
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        util::log(&format!("PANIC {}", info));
    }));
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // single instance: another Bongo Whale window already exists?
        if !FindWindowW(util::wide("BongoWhaleWindow").as_ptr(), std::ptr::null()).is_null() {
            return;
        }

        let hinst = GetModuleHandleW(std::ptr::null());
        let class_name = util::wide("BongoWhaleWindow");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        let mut cfg = Config::load();
        cfg.clamp();
        cfg.start_with_windows = autostart::is_enabled();

        let base = assets::load().unwrap_or_else(placeholder::build);
        let dpi = 96.0f32;
        let size = (DESIGN_SIZE * cfg.scale * dpi / 96.0).round() as i32;
        let sprites = scale_sprites(&base, size);
        let surface = Surface::new(size, size).expect("layered surface");

        let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if cfg.always_on_top {
            ex |= WS_EX_TOPMOST;
        }
        if cfg.click_through {
            ex |= WS_EX_TRANSPARENT;
        }

        let hwnd = CreateWindowExW(
            ex,
            class_name.as_ptr(),
            util::wide("Bongo Whale").as_ptr(),
            WS_POPUP,
            0,
            0,
            size,
            size,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        let anim = Animator::new(cfg.sleep_after_secs);
        let app_box = Box::new(App {
            cfg,
            base,
            sprites,
            surface,
            anim,
            hwnd,
            drag: None,
            size,
            dpi,
            tray: None,
            hook_on: false,
            pressed: [false; 256],
            hover: HoverState::Idle,
            hover_alpha: 1.0,
        });
        let ptr = Box::into_raw(app_box);
        APP.store(ptr as isize, Ordering::Relaxed);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

        input::register(hwnd);

        // tray icon from the character's own face
        let a = &mut *ptr;
        let icon_frame = a
            .base
            .icon
            .as_ref()
            .map(|f| f.scaled(32, 32))
            .or_else(|| a.base.get(ClipId::Idle).map(|c| c.frames[0].scaled(32, 32)));
        let icon = match icon_frame {
            Some(f) => tray::make_icon(&f, 32),
            None => std::ptr::null_mut(),
        };
        a.tray = Some(Tray::new(hwnd, icon, "鲸鱼娘 Bongo Cat"));
        a.dpi = {
            let d = GetDpiForWindow(hwnd);
            if d == 0 { 96.0 } else { d as f32 }
        };
        a.apply_config(false);
        a.place();
        a.ensure_on_screen();
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        a.arm_timer();

        let mut msg: MSG = std::mem::zeroed();
        loop {
            let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if r == 0 || r == -1 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        a.cfg.save();
        input::remove_hook();
        a.tray = None;
        drop(Box::from_raw(ptr));
    }
}
