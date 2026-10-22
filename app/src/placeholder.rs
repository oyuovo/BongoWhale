//! Procedural stand-in art so the app is runnable before real sprites exist.
//! Everything here is replaced by assets/manifest.json once the art pack is built.

use crate::sprite::{Clip, ClipId, Frame, Sprites};

const OUTLINE: [f32; 4] = [0x1B as f32, 0x21 as f32, 0x40 as f32, 1.0];
const HAIR: [f32; 4] = [0x2E as f32, 0x4B as f32, 0xC7 as f32, 1.0];
const HAIR_DARK: [f32; 4] = [0x1C as f32, 0x2E as f32, 0x7A as f32, 1.0];
const HAIR_HI: [f32; 4] = [0x7C as f32, 0x93 as f32, 0xFF as f32, 0.55];
const SKIN: [f32; 4] = [0xFF as f32, 0xE9 as f32, 0xDE as f32, 1.0];
const CLOTH: [f32; 4] = [0xF7 as f32, 0xF3 as f32, 0xEC as f32, 1.0];
const CLOTH_SH: [f32; 4] = [0xDF as f32, 0xD8 as f32, 0xCC as f32, 1.0];
const FIN: [f32; 4] = [0x5B as f32, 0x7C as f32, 0xFF as f32, 1.0];
const FIN_IN: [f32; 4] = [0xC9 as f32, 0xD6 as f32, 0xFF as f32, 1.0];
const ACCENT: [f32; 4] = [0x4D as f32, 0x6B as f32, 0xFE as f32, 1.0];
const BLUSH: [f32; 4] = [0xFF as f32, 0x9A as f32, 0xA8 as f32, 0.55];
const MOUTH: [f32; 4] = [0xE8 as f32, 0x83 as f32, 0x7B as f32, 1.0];
const EYE: [f32; 4] = [0x4D as f32, 0x6B as f32, 0xFE as f32, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const WOOD: [f32; 4] = [0xEF as f32, 0xE3 as f32, 0xD2 as f32, 1.0];
const WOOD_SH: [f32; 4] = [0xD8 as f32, 0xC7 as f32, 0xB0 as f32, 1.0];

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<u8>, // premultiplied BGRA
}

impl Canvas {
    fn new(w: i32, h: i32) -> Self {
        Canvas { w, h, px: vec![0; (w * h * 4) as usize] }
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, col: [f32; 4], cov: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || cov <= 0.0 {
            return;
        }
        let a = (col[3] * cov).clamp(0.0, 1.0);
        if a <= 0.002 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let d = &mut self.px[i..i + 4];
        let inv = 1.0 - a;
        d[0] = ((col[2] * a) + (d[0] as f32) * inv).min(255.0) as u8;
        d[1] = ((col[1] * a) + (d[1] as f32) * inv).min(255.0) as u8;
        d[2] = ((col[0] * a) + (d[2] as f32) * inv).min(255.0) as u8;
        d[3] = ((255.0 * a) + (d[3] as f32) * inv).min(255.0) as u8;
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: [f32; 4]) {
        self.ellipse_a(cx, cy, rx, ry, col, 1.0);
    }

    fn ellipse_a(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: [f32; 4], alpha: f32) {
        let x0 = (cx - rx - 2.0).floor() as i32;
        let x1 = (cx + rx + 2.0).ceil() as i32;
        let y0 = (cy - ry - 2.0).floor() as i32;
        let y1 = (cy + ry + 2.0).ceil() as i32;
        let edge = rx.min(ry).max(1.0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = ((1.0 - d) * edge + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, [col[0], col[1], col[2], col[3] * alpha], cov);
                }
            }
        }
    }

    fn capsule(&mut self, ax: f32, ay: f32, bx: f32, by: f32, ra: f32, rb: f32, col: [f32; 4]) {
        let pad = ra.max(rb) + 2.0;
        let x0 = (ax.min(bx) - pad).floor() as i32;
        let x1 = (ax.max(bx) + pad).ceil() as i32;
        let y0 = (ay.min(by) - pad).floor() as i32;
        let y1 = (ay.max(by) + pad).ceil() as i32;
        let vx = bx - ax;
        let vy = by - ay;
        let len2 = (vx * vx + vy * vy).max(1e-4);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = (((px - ax) * vx + (py - ay) * vy) / len2).clamp(0.0, 1.0);
                let cx = ax + vx * t;
                let cy = ay + vy * t;
                let r = ra + (rb - ra) * t;
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let cov = (r - d + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, col, cov);
                }
            }
        }
    }

    fn finish(&self) -> Frame {
        Frame { w: self.w as u32, h: self.h as u32, px: self.px.clone() }
    }
}

struct Pose {
    body_dy: f32,
    head_dy: f32,
    arm_l: f32, // 0 = on desk, 1 = raised
    arm_r: f32,
    eye: f32,   // 1 open, 0 closed
    mouth: f32, // 0 closed, 1 open
    tail: f32,  // -1..1
}

impl Default for Pose {
    fn default() -> Self {
        Pose { body_dy: 0.0, head_dy: 0.0, arm_l: 0.0, arm_r: 0.0, eye: 1.0, mouth: 0.0, tail: 0.0 }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hand_pos(side: f32, raise: f32) -> (f32, f32) {
    // side: -1 left, +1 right
    let rest = (256.0 + side * 78.0, 462.0);
    let up = (256.0 + side * 102.0, 358.0);
    let t = raise.clamp(0.0, 1.0);
    let cx = (rest.0 + up.0) * 0.5 + side * 6.0;
    let cy = (rest.1 + up.1) * 0.5 - 10.0;
    let x = (1.0 - t).powi(2) * rest.0 + 2.0 * (1.0 - t) * t * cx + t * t * up.0;
    let y = (1.0 - t).powi(2) * rest.1 + 2.0 * (1.0 - t) * t * cy + t * t * up.1;
    (x, y)
}

fn draw(p: &Pose) -> Frame {
    let mut c = Canvas::new(512, 512);
    let hd = p.head_dy;
    let bd = p.body_dy;

    // ---- tail (behind everything) ----
    let tail_lift = p.tail * 14.0;
    let tx = 420.0;
    let ty = 430.0 + tail_lift;
    for pass in 0..2 {
        let col = if pass == 0 { OUTLINE } else { FIN };
        let grow = if pass == 0 { 4.0 } else { 0.0 };
        let steps = 26;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = (1.0 - t).powi(2) * 330.0 + 2.0 * (1.0 - t) * t * 392.0 + t * t * tx;
            let y = (1.0 - t).powi(2) * 470.0 + 2.0 * (1.0 - t) * t * 500.0 + t * t * ty;
            let r = lerp(26.0, 13.0, t) + grow;
            c.ellipse(x, y, r, r, col);
        }
        c.ellipse(tx + 8.0, ty - 20.0, 30.0 + grow, 15.0 + grow, col);
        c.ellipse(tx + 10.0, ty + 12.0, 28.0 + grow, 14.0 + grow, col);
    }
    c.ellipse(tx + 2.0, ty - 2.0, 16.0, 10.0, FIN_IN);

    // ---- back hair ----
    c.ellipse(256.0, 240.0 + hd, 120.0, 124.0, OUTLINE);
    c.ellipse(256.0, 238.0 + hd, 116.0, 120.0, HAIR_DARK);

    // ---- side locks ----
    for side in [-1.0f32, 1.0] {
        let x0 = 256.0 + side * 96.0;
        c.capsule(x0, 300.0 + hd, 256.0 + side * 106.0, 478.0, 30.0, 20.0, OUTLINE);
        c.capsule(x0, 300.0 + hd, 256.0 + side * 106.0, 476.0, 25.0, 15.0, HAIR);
    }

    // ---- body ----
    c.ellipse(256.0, 412.0 + bd, 116.0, 100.0, OUTLINE);
    c.ellipse(256.0, 410.0 + bd, 111.0, 95.0, CLOTH);
    c.ellipse_a(256.0, 452.0 + bd, 96.0, 52.0, CLOTH_SH, 1.0);
    c.ellipse(256.0, 410.0 + bd, 96.0, 82.0, CLOTH);

    // ---- sailor collar ----
    c.ellipse(256.0, 344.0 + bd, 96.0, 34.0, OUTLINE);
    c.ellipse(256.0, 342.0 + bd, 92.0, 31.0, WHITE);
    c.capsule(206.0, 344.0 + bd, 256.0, 372.0 + bd, 4.0, 4.0, ACCENT);
    c.capsule(306.0, 344.0 + bd, 256.0, 372.0 + bd, 4.0, 4.0, ACCENT);

    // ---- whale-fin ears ----
    for side in [-1.0f32, 1.0] {
        c.capsule(256.0 + side * 92.0, 214.0 + hd, 256.0 + side * 158.0, 258.0 + hd, 30.0, 8.0, OUTLINE);
        c.capsule(256.0 + side * 92.0, 214.0 + hd, 256.0 + side * 156.0, 256.0 + hd, 25.0, 5.0, FIN);
        c.capsule(256.0 + side * 108.0, 226.0 + hd, 256.0 + side * 148.0, 252.0 + hd, 10.0, 3.0, FIN_IN);
    }

    // face
    c.ellipse(256.0, 250.0 + hd, 102.0, 104.0, OUTLINE);
    c.ellipse(256.0, 249.0 + hd, 98.0, 100.0, SKIN);

    // bangs: only the forehead, kept above the eyes
    c.ellipse(256.0, 158.0 + hd, 112.0, 66.0, OUTLINE);
    c.ellipse(256.0, 158.0 + hd, 108.0, 63.0, HAIR);
    c.ellipse_a(232.0, 136.0 + hd, 50.0, 14.0, HAIR_HI, 0.7);
    // face-framing forelocks
    for side in [-1.0f32, 1.0] {
        c.capsule(256.0 + side * 88.0, 186.0 + hd, 256.0 + side * 96.0, 286.0, 20.0, 7.0, OUTLINE);
        c.capsule(256.0 + side * 88.0, 186.0 + hd, 256.0 + side * 96.0, 284.0, 15.0, 4.0, HAIR);
    }
    // ahoge
    c.capsule(258.0, 104.0 + hd, 286.0, 66.0 + hd, 7.0, 2.0, OUTLINE);
    c.capsule(258.0, 104.0 + hd, 285.0, 68.0 + hd, 4.5, 1.5, HAIR);

    // eyes
    for side in [-1.0f32, 1.0] {
        let ex = 256.0 + side * 44.0;
        let ey = 262.0 + hd;
        if p.eye > 0.12 {
            let ry = 20.0 * p.eye;
            c.ellipse(ex, ey, 15.0, ry + 2.0, OUTLINE);
            c.ellipse(ex, ey, 13.0, ry, EYE);
            c.ellipse(ex - 4.0, ey - ry * 0.4, 5.0, 5.0, WHITE);
            c.capsule(ex - 16.0, ey - ry - 1.0, ex + 16.0, ey - ry - 1.0, 3.0, 3.0, SKIN);
        } else {
            c.capsule(ex - 15.0, ey, ex + 15.0, ey + 1.0, 3.0, 3.0, OUTLINE);
        }
    }

    // blush
    c.ellipse_a(256.0 - 62.0, 288.0 + hd, 18.0, 9.0, BLUSH, 1.0);
    c.ellipse_a(256.0 + 62.0, 288.0 + hd, 18.0, 9.0, BLUSH, 1.0);

    // mouth
    if p.mouth > 0.15 {
        c.ellipse(256.0, 296.0 + hd, 8.0, 5.0 + 7.0 * p.mouth, MOUTH);
    } else {
        c.capsule(249.0, 296.0 + hd, 263.0, 296.0 + hd, 2.5, 2.5, OUTLINE);
    }

    // ---- desk ----
    c.ellipse(256.0, 524.0, 208.0, 46.0, OUTLINE);
    c.ellipse(256.0, 520.0, 204.0, 42.0, WOOD);
    c.ellipse_a(256.0, 534.0, 204.0, 30.0, WOOD_SH, 1.0);
    c.capsule(56.0, 508.0, 456.0, 508.0, 4.0, 4.0, ACCENT);

    // ---- arms ----
    for (side, raise) in [(-1.0f32, p.arm_l), (1.0f32, p.arm_r)] {
        let (hx, hy) = hand_pos(side, raise);
        let sx = 256.0 + side * 74.0;
        let sy = 386.0 + bd;
        c.capsule(sx, sy, hx, hy, 26.0, 19.0, OUTLINE);
        c.capsule(sx, sy, hx, hy, 21.0, 15.0, CLOTH);
        c.ellipse(hx, hy, 26.0, 25.0, OUTLINE);
        c.ellipse(hx, hy, 22.0, 21.0, SKIN);
    }

    c.finish()
}

fn clip(fps: f32, looping: bool, poses: Vec<Pose>) -> Clip {
    Clip { fps, looping, frames: poses.into_iter().map(|p| draw(&p)).collect() }
}

pub fn build() -> Sprites {
    let base = Pose::default();
    let mut s = Sprites::new();
    s.canvas = 512;

    s.insert(
        ClipId::Idle,
        clip(
            4.0,
            true,
            vec![
                Pose { body_dy: 0.0, tail: -0.4, ..base },
                Pose { body_dy: -2.0, tail: 0.2, ..base },
                Pose { body_dy: 0.0, tail: 1.0, ..base },
                Pose { body_dy: 1.5, tail: 0.2, ..base },
            ],
        ),
    );

    s.insert(ClipId::Blink, clip(14.0, false, vec![Pose { eye: 0.35, ..base }, Pose { eye: 0.0, ..base }]));

    s.insert(
        ClipId::TapL,
        clip(
            20.0,
            false,
            vec![
                Pose { arm_l: 0.92, body_dy: -1.0, eye: 0.7, ..base },
                Pose { arm_l: 0.0, body_dy: 4.0, eye: 0.15, mouth: 0.8, tail: 0.6, ..base },
                Pose { arm_l: 0.35, body_dy: 0.0, eye: 0.6, ..base },
            ],
        ),
    );

    s.insert(
        ClipId::TapR,
        clip(
            20.0,
            false,
            vec![
                Pose { arm_r: 0.92, body_dy: -1.0, eye: 0.7, ..base },
                Pose { arm_r: 0.0, body_dy: 4.0, eye: 0.15, mouth: 0.8, tail: -0.6, ..base },
                Pose { arm_r: 0.35, body_dy: 0.0, eye: 0.6, ..base },
            ],
        ),
    );

    s.insert(
        ClipId::Sleep,
        clip(
            2.0,
            true,
            vec![
                Pose { eye: 0.0, body_dy: 3.0, head_dy: 6.0, mouth: 0.1, tail: -0.6, ..base },
                Pose { eye: 0.0, body_dy: 1.0, head_dy: 3.0, mouth: 0.1, tail: -0.2, ..base },
            ],
        ),
    );

    s.insert(ClipId::Wake, clip(1.0, false, vec![Pose { eye: 0.65, body_dy: -1.0, ..base }]));

    s
}
