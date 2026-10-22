use std::collections::HashMap;

/// A ready-to-blit, premultiplied BGRA frame.
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Frame {
    #[allow(dead_code)]
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return 0;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        self.px[i + 3]
    }

    /// Bilinear resize on premultiplied data (safe: premultiplied alpha interpolates correctly).
    pub fn scaled(&self, w: u32, h: u32) -> Frame {
        if w == self.w && h == self.h {
            return Frame { w: self.w, h: self.h, px: self.px.clone() };
        }
        let mut out = vec![0u8; (w * h * 4) as usize];
        let sw = self.w as f32;
        let sh = self.h as f32;
        for y in 0..h {
            let sy = ((y as f32 + 0.5) * sh / h as f32 - 0.5).max(0.0);
            let y0 = sy.floor() as u32;
            let y1 = (y0 + 1).min(self.h - 1);
            let fy = sy - y0 as f32;
            for x in 0..w {
                let sx = ((x as f32 + 0.5) * sw / w as f32 - 0.5).max(0.0);
                let x0 = sx.floor() as u32;
                let x1 = (x0 + 1).min(self.w - 1);
                let fx = sx - x0 as f32;
                let mut px = [0u8; 4];
                for c in 0..4 {
                    let p00 = self.px[((y0 * self.w + x0) * 4) as usize + c] as f32;
                    let p10 = self.px[((y0 * self.w + x1) * 4) as usize + c] as f32;
                    let p01 = self.px[((y1 * self.w + x0) * 4) as usize + c] as f32;
                    let p11 = self.px[((y1 * self.w + x1) * 4) as usize + c] as f32;
                    let top = p00 + (p10 - p00) * fx;
                    let bot = p01 + (p11 - p01) * fx;
                    px[c] = (top + (bot - top) * fy).round().clamp(0.0, 255.0) as u8;
                }
                let i = ((y * w + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&px);
            }
        }
        Frame { w, h, px: out }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipId {
    Idle = 0,
    Blink = 1,
    TapL = 2,
    TapR = 3,
    Sleep = 4,
    Wake = 5,
}

impl ClipId {
    pub fn key(self) -> &'static str {
        match self {
            ClipId::Idle => "idle",
            ClipId::Blink => "blink",
            ClipId::TapL => "tap_left",
            ClipId::TapR => "tap_right",
            ClipId::Sleep => "sleep",
            ClipId::Wake => "wake",
        }
    }
    pub fn all() -> [ClipId; 6] {
        [ClipId::Idle, ClipId::Blink, ClipId::TapL, ClipId::TapR, ClipId::Sleep, ClipId::Wake]
    }
}

pub struct Clip {
    pub fps: f32,
    pub looping: bool,
    pub frames: Vec<Frame>,
}

pub struct Sprites {
    clips: HashMap<&'static str, Clip>,
    pub canvas: u32,
    /// Optional app/tray icon shipped with the pack.
    pub icon: Option<Frame>,
}

impl Sprites {
    pub fn new() -> Self {
        Sprites { clips: HashMap::new(), canvas: 512, icon: None }
    }

    pub fn insert(&mut self, id: ClipId, clip: Clip) {
        self.clips.insert(id.key(), clip);
    }

    pub fn has(&self, id: ClipId) -> bool {
        self.clips.contains_key(id.key())
    }

    pub fn get(&self, id: ClipId) -> Option<&Clip> {
        self.clips.get(id.key())
    }
}
