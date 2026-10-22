//! Settings window: a self-drawn panel in the pet's own palette.
//!
//! Everything on screen is a row list. To add a setting: add a row, give it an
//! id, handle that id in set_slider / toggle. No layout code needed.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetDC, InvalidateRect, ReleaseDC, UpdateWindow, HDC, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::ui::{self, Canvas, Fonts, MemDC, Rect};

const W: i32 = 480;
const H: i32 = 740;
const PAD: i32 = 28;

const ID_SCALE: u32 = 1;
const ID_OPACITY: u32 = 2;
const ID_SLEEP: u32 = 3;
const ID_HOVER_HIDE: u32 = 10;
const ID_REACT_MOUSE: u32 = 11;
const ID_HOOK: u32 = 12;
const ID_TOPMOST: u32 = 20;
const ID_CLICK_THROUGH: u32 = 21;
const ID_AUTOSTART: u32 = 22;
const ID_CLOSE: u32 = 31;
const ID_RESET: u32 = 30;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const c_void, size: u32) -> i32;
}

enum Row {
    Section(&'static str),
    Slider { id: u32, label: &'static str, value: f32, min: f32, max: f32, text: String },
    Toggle { id: u32, label: &'static str, on: bool },
    Gap(i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HitKind {
    Slider,
    Toggle,
    Button,
}

#[derive(Clone, Copy)]
struct Hit {
    id: u32,
    kind: HitKind,
    rect: Rect,
    min: f32,
    max: f32,
}

struct State {
    mem: MemDC,
    fonts: Fonts,
    hits: Vec<Hit>,
    hover: Option<u32>,
    active: Option<u32>,
    close_rect: Rect,
}

fn rows(cfg: &crate::config::Config) -> Vec<Row> {
    vec![
        Row::Section("外观"),
        Row::Slider {
            id: ID_SCALE,
            label: "大小",
            value: cfg.scale,
            min: 0.5,
            max: 2.5,
            text: format!("{}%", (cfg.scale * 100.0).round() as i32),
        },
        Row::Slider {
            id: ID_OPACITY,
            label: "不透明度",
            value: cfg.opacity,
            min: 0.2,
            max: 1.0,
            text: format!("{}%", (cfg.opacity * 100.0).round() as i32),
        },
        Row::Section("行为"),
        Row::Slider {
            id: ID_SLEEP,
            label: "多久没人理就打瞌睡",
            value: cfg.sleep_after_secs as f32,
            min: 15.0,
            max: 600.0,
            text: format!("{} 秒", cfg.sleep_after_secs),
        },
        Row::Toggle { id: ID_HOVER_HIDE, label: "鼠标停留时淡出让开", on: cfg.hide_on_hover },
        Row::Toggle { id: ID_REACT_MOUSE, label: "鼠标点击也拍一下", on: cfg.react_to_mouse },
        Row::Toggle { id: ID_HOOK, label: "用键盘钩子（部分游戏需要）", on: cfg.keyboard_hook },
        Row::Section("系统"),
        Row::Toggle { id: ID_TOPMOST, label: "总在最前", on: cfg.always_on_top },
        Row::Toggle { id: ID_CLICK_THROUGH, label: "完全点击穿透", on: cfg.click_through },
        Row::Toggle { id: ID_AUTOSTART, label: "开机自启", on: crate::autostart::is_enabled() },
        Row::Gap(10),
    ]
}

fn draw_slider(c: &mut Canvas, f: &Fonts, hdc: HDC, r: Rect, label: &str, text: &str, frac: f32, hot: bool) {
    f.text(hdc, Rect::new(r.x, r.y, r.w - 90, 20), label, f.label, ui::TEXT, ui::DT_LEFT | ui::DT_VCENTER);
    f.text(hdc, Rect::new(r.right() - 90, r.y, 90, 20), text, f.small, ui::TEXT_DIM, ui::DT_RIGHT | ui::DT_VCENTER);
    let track = Rect::new(r.x, r.y + 32, r.w, 8);
    c.round_rect(track, 4, ui::TRACK);
    let fw = (track.w as f32 * frac).max(9.0) as i32;
    c.round_rect(Rect::new(track.x, track.y, fw, track.h), 4, ui::ACCENT);
    let kx = track.x as f32 + track.w as f32 * frac;
    let ky = track.y as f32 + 4.0;
    if hot {
        c.circle(kx, ky, 11.0, ui::ACCENT_HOVER);
        c.circle(kx, ky, 8.0, ui::KNOB);
    } else {
        c.circle(kx, ky, 9.0, ui::KNOB);
    }
}

fn draw_toggle(c: &mut Canvas, f: &Fonts, hdc: HDC, r: Rect, label: &str, on: bool, hot: bool) {
    f.text(hdc, Rect::new(r.x, r.y, r.w - 70, r.h), label, f.label, ui::TEXT, ui::DT_LEFT | ui::DT_VCENTER);
    let sw = Rect::new(r.right() - 48, r.y + (r.h - 24) / 2, 48, 24);
    c.round_rect(sw, 12, if on { ui::ACCENT } else { ui::TRACK });
    let kx = if on { sw.right() as f32 - 12.0 } else { sw.x as f32 + 12.0 };
    c.circle(kx, sw.y as f32 + 12.0, if hot { 9.5 } else { 8.5 }, ui::KNOB);
}

fn draw_button(c: &mut Canvas, f: &Fonts, hdc: HDC, r: Rect, label: &str, primary: bool, hot: bool) {
    let fill = if primary {
        if hot { ui::ACCENT_HOVER } else { ui::ACCENT }
    } else if hot {
        ui::TRACK
    } else {
        ui::CARD
    };
    let col = if primary || hot { ui::TEXT } else { ui::TEXT_DIM };
    c.round_rect(r, 10, fill);
    f.text(hdc, Rect::new(r.x, r.y, r.w, r.h), label, f.button, col, ui::DT_CENTER | ui::DT_VCENTER);
}

unsafe fn render(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let Some(a) = crate::app_ptr() else { return };
    let cfg = a.cfg.clone();
    let dc = GetDC(hwnd);
    let State { mem, fonts, hits, hover, close_rect, active } = &mut *st;
    let mem_hdc = mem.hdc;
    let mut new_hits: Vec<Hit> = Vec::new();
    {
        let px = mem.bits();
        let mut c = Canvas::new(px, W, H);
        c.fill(ui::BG);
        c.vgrad(Rect::new(0, 0, W, 104), ui::HEADER, ui::BG);
        c.round_rect(Rect::new(0, 103, W, 1), 0, ui::TRACK);

        if *hover == Some(ID_CLOSE) {
            c.round_rect(*close_rect, 8, ui::TRACK);
        }
        fonts.text(mem_hdc, Rect::new(close_rect.x, close_rect.y - 2, close_rect.w, close_rect.h), "×", fonts.title, ui::TEXT_DIM, ui::DT_CENTER | ui::DT_VCENTER);

        // character portrait on the left of the header
        if let Some(icon) = &a.base.icon {
            draw_icon(&mut c, icon, Rect::new(PAD, 22, 60, 60));
        }
        fonts.text(mem_hdc, Rect::new(PAD + 72, 30, 280, 26), "鲸鱼娘 Bongo Cat", fonts.title, ui::TEXT, ui::DT_LEFT | ui::DT_VCENTER);
        fonts.text(mem_hdc, Rect::new(PAD + 72, 58, 280, 20), "桌面宠物 · 设置", fonts.small, ui::TEXT_DIM, ui::DT_LEFT | ui::DT_VCENTER);

        let mut y = 128;
        for row in rows(&cfg) {
            match row {
                Row::Section(t) => {
                    y += 6;
                    c.round_rect(Rect::new(PAD, y, W - PAD * 2, 1), 0, ui::TRACK);
                    y += 7;
                    fonts.text(mem_hdc, Rect::new(PAD, y, 240, 18), t, fonts.small, ui::ACCENT, ui::DT_LEFT | ui::DT_VCENTER);
                    y += 25;
                }
                Row::Slider { id, label, value, min, max, text } => {
                    let r = Rect::new(PAD, y, W - PAD * 2, 50);
                    let frac = ((value - min) / (max - min)).clamp(0.0, 1.0);
                    draw_slider(&mut c, fonts, mem_hdc, r, label, &text, frac, *hover == Some(id) || *active == Some(id));
                    new_hits.push(Hit { id, kind: HitKind::Slider, rect: Rect::new(r.x - 6, r.y, r.w + 12, 44), min, max });
                    y += 54;
                }
                Row::Toggle { id, label, on } => {
                    let r = Rect::new(PAD, y, W - PAD * 2, 40);
                    draw_toggle(&mut c, fonts, mem_hdc, r, label, on, *hover == Some(id));
                    new_hits.push(Hit { id, kind: HitKind::Toggle, rect: r, min: 0.0, max: 0.0 });
                    y += 40;
                }
                Row::Gap(g) => y += g,
            }
        }

        let by = H - 76;
        let bw = (W - PAD * 2 - 12) / 2;
        let reset = Rect::new(PAD, by, bw, 44);
        let done = Rect::new(PAD + bw + 12, by, bw, 44);
        draw_button(&mut c, fonts, mem_hdc, reset, "复位位置", false, *hover == Some(ID_RESET));
        draw_button(&mut c, fonts, mem_hdc, done, "完成", true, *hover == Some(ID_CLOSE));
        new_hits.push(Hit { id: ID_RESET, kind: HitKind::Button, rect: reset, min: 0.0, max: 0.0 });
        new_hits.push(Hit { id: ID_CLOSE, kind: HitKind::Button, rect: done, min: 0.0, max: 0.0 });
    }
    *hits = new_hits;
    mem.flush(hwnd);
    ReleaseDC(hwnd, dc);
}

fn draw_icon(c: &mut Canvas, frame: &crate::sprite::Frame, r: Rect) {
    let s = frame.scaled(r.w as u32, r.h as u32);
    let px = &s.px;
    for y in 0..r.h {
        for x in 0..r.w {
            let i = ((y as u32 * s.w + x as u32) * 4) as usize;
            let a = px[i + 3];
            if a == 0 {
                continue;
            }
            let cov = a as f32 / 255.0;
            c.blend_px(r.x + x, r.y + y, [px[i], px[i + 1], px[i + 2], 255], cov);
        }
    }
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if p.is_null() { None } else { Some(&mut *p) }
}

unsafe fn hit_at(st: &State, x: i32, y: i32) -> Option<Hit> {
    if st.close_rect.has(x, y) {
        return Some(Hit { id: ID_CLOSE, kind: HitKind::Button, rect: st.close_rect, min: 0.0, max: 0.0 });
    }
    st.hits.iter().copied().find(|h| h.rect.has(x, y))
}

fn set_slider(id: u32, value: f32) {
    let Some(a) = crate::app_ptr() else { return };
    let mut resize = false;
    match id {
        ID_SCALE => {
            a.cfg.scale = value.clamp(0.5, 2.5);
            resize = true;
        }
        ID_OPACITY => a.cfg.opacity = value.clamp(0.2, 1.0),
        ID_SLEEP => a.cfg.sleep_after_secs = (value.round() as u64).clamp(15, 600),
        _ => {}
    }
    a.apply_config(resize);
}

fn toggle(id: u32) {
    let Some(a) = crate::app_ptr() else { return };
    match id {
        ID_HOVER_HIDE => a.cfg.hide_on_hover = !a.cfg.hide_on_hover,
        ID_REACT_MOUSE => a.cfg.react_to_mouse = !a.cfg.react_to_mouse,
        ID_HOOK => a.cfg.keyboard_hook = !a.cfg.keyboard_hook,
        ID_TOPMOST => a.cfg.always_on_top = !a.cfg.always_on_top,
        ID_CLICK_THROUGH => a.cfg.click_through = !a.cfg.click_through,
        ID_AUTOSTART => {
            a.cfg.start_with_windows = !a.cfg.start_with_windows;
            crate::autostart::set(a.cfg.start_with_windows);
        }
        _ => return,
    }
    a.apply_config(false);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(hwnd, &mut ps);
            render(hwnd);
            EndPaint(hwnd, &mut ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let x = (lp as i32) & 0xFFFF;
            let y = ((lp as i32) >> 16) & 0xFFFF;
            let x = x as i16 as i32;
            let y = y as i16 as i32;
            if let Some(st) = state(hwnd) {
                if let Some(active) = st.active {
                    if let Some(h) = st.hits.iter().find(|h| h.id == active) {
                        let frac = ((x - h.rect.x) as f32 / h.rect.w as f32).clamp(0.0, 1.0);
                        set_slider(active, h.min + frac * (h.max - h.min));
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                        return 0;
                    }
                }
                if let Some(h) = hit_at(st, x, y) {
                    if st.hover != Some(h.id) {
                        st.hover = Some(h.id);
                        SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND));
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                } else if st.hover.is_some() {
                    st.hover = None;
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lp as i32) & 0xFFFF;
            let y = ((lp as i32) >> 16) & 0xFFFF;
            let x = x as i16 as i32;
            let y = y as i16 as i32;
            if y < 104 && !(x > W - PAD - 40 && y < 52) {
                // drag the window by its header
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
                return 0;
            }
            if let Some(st) = state(hwnd) {
                if let Some(h) = hit_at(st, x, y) {
                    match h.kind {
                        HitKind::Slider => {
                            st.active = Some(h.id);
                            let frac = ((x - h.rect.x) as f32 / h.rect.w as f32).clamp(0.0, 1.0);
                            set_slider(h.id, h.min + frac * (h.max - h.min));
                            SetCapture(hwnd);
                        }
                        HitKind::Toggle => toggle(h.id),
                        HitKind::Button => match h.id {
                            ID_CLOSE => {
                                DestroyWindow(hwnd);
                            }
                            ID_RESET => {
                                if let Some(a) = crate::app_ptr() {
                                    a.reset_position();
                                }
                            }
                            _ => toggle(h.id),
                        },
                    }
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(st) = state(hwnd) {
                if st.active.is_some() {
                    st.active = None;
                    ReleaseCapture();
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }
        WM_KEYDOWN => {
            if wp == 0x1B {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !st.is_null() {
                drop(Box::from_raw(st));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn open(parent: HWND) {
    unsafe {
        let existing = FindWindowW(crate::util::wide("BongoWhaleSettings").as_ptr(), std::ptr::null());
        if !existing.is_null() {
            SetForegroundWindow(existing);
            return;
        }
        let hinst = GetModuleHandleW(std::ptr::null());
        let cls = crate::util::wide("BongoWhaleSettings");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
        };
        RegisterClassW(&wc);

        let Some(st) = State::new() else { return };
        let ptr = Box::into_raw(Box::new(st));

        // centre on whichever monitor the pet is on
        let mon = windows_sys::Win32::Graphics::Gdi::MonitorFromWindow(
            parent,
            windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        );
        let mut mi: windows_sys::Win32::Graphics::Gdi::MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::MONITORINFO>() as u32;
        let wa = if windows_sys::Win32::Graphics::Gdi::GetMonitorInfoW(mon, &mut mi) != 0 {
            mi.rcWork
        } else {
            let mut r: RECT = std::mem::zeroed();
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut _ as *mut _, 0);
            r
        };
        let x = ((wa.left + wa.right) - W) / 2;
        let y = ((wa.top + wa.bottom) - H) / 2;

        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            cls.as_ptr(),
            crate::util::wide("鲸鱼娘 Bongo Cat").as_ptr(),
            WS_POPUP,
            x,
            y,
            W,
            H,
            parent,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        if h.is_null() {
            drop(Box::from_raw(ptr));
            return;
        }
        SetWindowLongPtrW(h, GWLP_USERDATA, ptr as isize);
        let round: i32 = 2;
        DwmSetWindowAttribute(h, 33, &round as *const i32 as *const c_void, 4);
        ShowWindow(h, SW_SHOW);
        UpdateWindow(h);
        SetForegroundWindow(h);
    }
}

impl State {
    fn new() -> Option<State> {
        Some(State {
            mem: MemDC::new(W, H)?,
            fonts: Fonts::new(),
            hits: Vec::new(),
            hover: None,
            active: None,
            close_rect: Rect::new(W - PAD - 24, 20, 24, 24),
        })
    }
}
