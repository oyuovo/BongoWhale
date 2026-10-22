use std::path::Path;

use image::{Rgba, RgbaImage};

/// A rectangle of interest used to keep drift noise out of an extraction.
pub(crate) struct Box2 {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Box2 {
    pub(crate) fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Box2 { x0, y0, x1, y1 }
    }

    fn has(&self, x: u32, y: u32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

fn load(p: &Path) -> Result<RgbaImage, String> {
    image::open(p).map(|i| i.to_rgba8()).map_err(|e| format!("{}: {}", p.display(), e))
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let p = img.get_pixel(x, y);
    [p[0], p[1], p[2], p[3]]
}

fn diff(a: &RgbaImage, b: &RgbaImage, x: u32, y: u32) -> u32 {
    let pa = px(a, x, y);
    let pb = px(b, x, y);
    let mut d = 0u32;
    for k in 0..3 {
        d += (pa[k] as i32 - pb[k] as i32).unsigned_abs();
    }
    // alpha differences count too: the arm can cover background
    d + (pa[3] as i32 - pb[3] as i32).unsigned_abs() * 3
}

/// Pixels that changed a lot -- solid new shapes, not thin drift edges.
fn changed_mask(a: &RgbaImage, b: &RgbaImage, thresh: u32) -> Vec<bool> {
    let (w, h) = a.dimensions();
    let mut m = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            if diff(a, b, x, y) > thresh {
                m[(y * w + x) as usize] = true;
            }
        }
    }
    m
}

fn erode(m: &[bool], w: u32, h: u32, r: i32) -> Vec<bool> {
    let mut o = vec![false; m.len()];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !m[(y * w as i32 + x) as usize] {
                continue;
            }
            let mut keep = true;
            'outer: for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    if !m[(ny * w as i32 + nx) as usize] {
                        keep = false;
                        break 'outer;
                    }
                }
            }
            o[(y * w as i32 + x) as usize] = keep;
        }
    }
    o
}

fn dilate(m: &[bool], w: u32, h: u32, r: i32) -> Vec<bool> {
    let mut o = vec![false; m.len()];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut hit = false;
            'outer: for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    if m[(ny * w as i32 + nx) as usize] {
                        hit = true;
                        break 'outer;
                    }
                }
            }
            o[(y * w as i32 + x) as usize] = hit;
        }
    }
    o
}

/// Keep only the biggest 4-connected blob: drops motion lines and stray specks.
fn largest_blob(m: &[bool], w: u32, h: u32) -> Vec<bool> {
    let mut seen = vec![false; m.len()];
    let mut best: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..m.len() {
        if !m[i] || seen[i] {
            continue;
        }
        let mut cur: Vec<usize> = Vec::new();
        seen[i] = true;
        stack.push(i);
        while let Some(idx) = stack.pop() {
            cur.push(idx);
            let x = (idx as u32) % w;
            let y = (idx as u32) / w;
            let mut push = |nx: i64, ny: i64| {
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    return;
                }
                let ni = (ny as u32 * w + nx as u32) as usize;
                if m[ni] && !seen[ni] {
                    seen[ni] = true;
                    stack.push(ni);
                }
            };
            push(x as i64 - 1, y as i64);
            push(x as i64 + 1, y as i64);
            push(x as i64, y as i64 - 1);
            push(x as i64, y as i64 + 1);
        }
        if cur.len() > best.len() {
            best = cur;
        }
    }
    let mut o = vec![false; m.len()];
    for i in best {
        o[i] = true;
    }
    o
}

fn apply_box(m: &mut [bool], w: u32, b: &Box2) {
    for y in 0..w {
        for x in 0..w {
            if !b.has(x, y) {
                m[(y * w + x) as usize] = false;
            }
        }
    }
}

/// Cut the masked pixels out of `src` onto a transparent canvas.
fn extract(src: &RgbaImage, mask: &[bool], w: u32, h: u32) -> RgbaImage {
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] {
                let p = px(src, x, y);
                out.put_pixel(x, y, Rgba([p[0], p[1], p[2], p[3]]));
            }
        }
    }
    out
}

/// A one-pixel soft edge so extracted layers do not cut like scissors.
fn feather(img: &RgbaImage, mask: &[bool], w: u32, h: u32) -> RgbaImage {
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] {
                continue;
            }
            let mut near = false;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                if mask[(ny as u32 * w + nx as u32) as usize] {
                    near = true;
                }
            }
            if near {
                let p = px(&img, x, y);
                if p[3] > 0 {
                    out.put_pixel(x, y, Rgba([p[0], p[1], p[2], 110]));
                }
            }
        }
    }
    out
}


// ---- 配准：先找到把 src 对齐到 base 的缩放和平移，再差分 ----

/// Resample `src` into base space: base(x, y) ~ src(cx + (x-cx)/s + dx, ...).
pub(crate) fn warp(src: &RgbaImage, inv_scale: f32, dx: i32, dy: i32, w: u32, h: u32) -> RgbaImage {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let sx = cx + (x as f32 - cx) * inv_scale + dx as f32;
            let sy = cy + (y as f32 - cy) * inv_scale + dy as f32;
            if sx < 1.0 || sy < 1.0 || sx >= w as f32 - 1.0 || sy >= h as f32 - 1.0 {
                continue;
            }
            let x0 = sx.floor() as u32;
            let y0 = sy.floor() as u32;
            let fx = sx - x0 as f32;
            let fy = sy - y0 as f32;
            let mut acc = [0f32; 4];
            for (ox, oy, wgt) in [
                (0u32, 0u32, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let p = px(src, x0 + ox, y0 + oy);
                for k in 0..4 {
                    acc[k] += p[k] as f32 * wgt;
                }
            }
            out.put_pixel(x, y, Rgba([acc[0] as u8, acc[1] as u8, acc[2] as u8, acc[3] as u8]));
        }
    }
    out
}

fn score_t(
    src: &RgbaImage,
    base: &RgbaImage,
    inv: f32,
    dx: i32,
    dy: i32,
    region: &Box2,
    w: u32,
    h: u32,
) -> u64 {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let mut s = 0u64;
    let mut y = region.y0;
    while y < region.y1 {
        let mut x = region.x0;
        while x < region.x1 {
            let sx = cx + (x as f32 - cx) * inv + dx as f32;
            let sy = cy + (y as f32 - cy) * inv + dy as f32;
            if sx >= 1.0 && sy >= 1.0 && sx < w as f32 - 1.0 && sy < h as f32 - 1.0 {
                let x0 = sx as u32;
                let y0 = sy as u32;
                let fx = sx - x0 as f32;
                let fy = sy - y0 as f32;
                let b = px(base, x, y);
                for k in 0..3 {
                    let p00 = px(src, x0, y0)[k] as f32;
                    let p10 = px(src, x0 + 1, y0)[k] as f32;
                    let p01 = px(src, x0, y0 + 1)[k] as f32;
                    let p11 = px(src, x0 + 1, y0 + 1)[k] as f32;
                    let top = p00 + (p10 - p00) * fx;
                    let bot = p01 + (p11 - p01) * fx;
                    let v = (top + (bot - top) * fy) as i32;
                    s += (v - b[k] as i32).unsigned_abs() as u64;
                }
            }
            x += 3;
        }
        y += 3;
    }
    s
}

/// Coarse-to-fine search for the transform that best aligns a frame to the base.
pub(crate) fn best_transform(src: &RgbaImage, base: &RgbaImage, w: u32, h: u32, region: &Box2) -> (f32, i32, i32) {
    let mut best = (1.0f32, 0i32, 0i32, u64::MAX);
    for si in -4..=4 {
        let inv = 1.0 + si as f32 * 0.005; // +-2%
        for dx in (-10..=10).step_by(2) {
            for dy in (-10..=10).step_by(2) {
                let sc = score_t(src, base, inv, dx, dy, region, w, h);
                if sc < best.3 {
                    best = (inv, dx, dy, sc);
                }
            }
        }
    }
    // refine around the winner
    let (inv0, dx0, dy0, _) = best;
    let mut refined = (inv0, dx0, dy0, u64::MAX);
    for si in -3..=3 {
        let inv = inv0 + si as f32 * 0.0015;
        for dx in (dx0 - 2)..=(dx0 + 2) {
            for dy in (dy0 - 2)..=(dy0 + 2) {
                let sc = score_t(src, base, inv, dx, dy, region, w, h);
                if sc < refined.3 {
                    refined = (inv, dx, dy, sc);
                }
            }
        }
    }
    (refined.0, refined.1, refined.2)
}



/// The raised arm is white sleeve + skin hand + navy cuff, all distinct from the
/// blue hair behind it. Navy only counts when it touches the sleeve, otherwise
/// the character's dark leg straps get picked up too.
fn arm_by_color(src: &RgbaImage, b: &Box2, w: u32, h: u32) -> (RgbaImage, u32) {
    let mut body = vec![false; (w * h) as usize];
    let mut navy = vec![false; (w * h) as usize];
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            let p = px(src, x, y);
            if p[3] < 60 {
                continue;
            }
            let (r, g, bl) = (p[0] as i32, p[1] as i32, p[2] as i32);
            let i = (y * w + x) as usize;
            let white = r > 195 && g > 190 && bl > 175;
            let skin = r > 220 && g > 175 && bl > 155 && r - bl > 25;
            let navy_px = r < 55 && g < 55 && bl < 100;
            if white || skin {
                body[i] = true;
            } else if navy_px {
                navy[i] = true;
            }
        }
    }
    // the cuff is navy attached to the sleeve; keep only that navy
    let near = dilate(&body, w, h, 5);
    for i in 0..navy.len() {
        if navy[i] && near[i] {
            body[i] = true;
        }
    }
    let mut m = body;
    m = dilate(&m, w, h, 1);
    m = erode(&m, w, h, 1);
    m = largest_blob(&m, w, h);
    m = dilate(&m, w, h, 1);
    let count = m.iter().filter(|v| **v).count() as u32;
    (feather(&extract(src, &m, w, h), &m, w, h), count)
}

/// Copy a soft-edged rectangle from one frame onto another. Used for the eyes:
/// after alignment the two frames are identical except for the eye line, so a
/// feathered patch is visually seamless and much less fragile than diffing.
fn region_patch(src: &RgbaImage, base: &RgbaImage, b: &Box2, w: u32, h: u32) -> RgbaImage {
    let mut out = base.clone();
    let feather_px = 7.0f32;
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            let dx = (x as f32 - b.x0 as f32).min(b.x1 as f32 - x as f32);
            let dy = (y as f32 - b.y0 as f32).min(b.y1 as f32 - y as f32);
            let a = ((dx.min(dy)) / feather_px).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let p = px(src, x, y);
            let q = px(&out, x, y);
            let mix = |i: usize, j: usize| -> u8 {
                (p[i] as f32 * a + q[j] as f32 * (1.0 - a)) as u8
            };
            out.put_pixel(
                x,
                y,
                Rgba([
                    mix(0, 0),
                    mix(1, 1),
                    mix(2, 2),
                    (p[3] as f32 * a + q[3] as f32 * (1.0 - a)) as u8,
                ]),
            );
        }
    }
    out
}


fn layer_from_diff(
    src: &RgbaImage,
    base: &RgbaImage,
    w: u32,
    h: u32,
    b: &Box2,
    thresh: u32,
    clean: bool,
) -> (RgbaImage, u32) {
    let mut m = changed_mask(src, base, thresh);
    apply_box(&mut m, w, b);
    if clean {
        m = erode(&m, w, h, 1);
        m = dilate(&m, w, h, 1);
        m = largest_blob(&m, w, h);
        m = dilate(&m, w, h, 2);
    }
    let count = m.iter().filter(|v| **v).count() as u32;
    let img = feather(&extract(src, &m, w, h), &m, w, h);
    (img, count)
}


/// Compose a few candidate tap frames offline so the motion can be judged
/// before any engine work.
fn write_motion_test(base: &RgbaImage, arm: &RgbaImage, w: u32, h: u32, out: &Path) {
    let shifts = [0i32, -18, 26, 46];
    let alphas = [1.0f32, 1.0, 0.85, 0.35];
    let mut sheet = RgbaImage::new(w * 4, h);
    for (i, (dy, a)) in shifts.iter().zip(alphas).enumerate() {
        let mut f = base.clone();
        // body squash for the impact frames
        let squash = if *dy > 20 { 1.0 } else { 1.0 };
        let _ = squash;
        for y in 0..h {
            for x in 0..w {
                let p = px(arm, x, y);
                if p[3] == 0 {
                    continue;
                }
                let ty = y as i32 + dy;
                if ty < 0 || ty >= h as i32 {
                    continue;
                }
                let q = px(&f, x, ty as u32);
                let a2 = (p[3] as f32 / 255.0) * a;
                let mix = |i: usize, j: usize| -> u8 {
                    (p[i] as f32 * a2 + q[j] as f32 * (1.0 - a2)) as u8
                };
                f.put_pixel(x, ty as u32, Rgba([mix(0, 0), mix(1, 1), mix(2, 2), 255]));
            }
        }
        image::imageops::overlay(&mut sheet, &f, (i as i64) * w as i64, 0);
    }
    let _ = sheet.save(out);
}

pub fn rig(input: &Path, output: &Path) -> Result<(), String> {
    let base_path = input.join("idle").join("00.png");
    let base = load(&base_path)?;
    let (w, h) = base.dimensions();

    let eyes_half_src = load(&input.join("blink").join("00.png"))?;
    let eyes_shut_src = load(&input.join("blink").join("01.png"))?;
    let tap_l = load(&input.join("tap_left").join("00.png"))?;
    let tap_r = load(&input.join("tap_right").join("00.png"))?;

    // The lap/legs never change pose, so they are the alignment reference.
    let anchor = Box2 { x0: 150, y0: 350, x1: 380, y1: 470 };
    let face = Box2 { x0: 130, y0: 200, x1: 390, y1: 345 };
    let left_box = Box2 { x0: 55, y0: 235, x1: 175, y1: 365 };
    let right_box = Box2 { x0: 335, y0: 235, x1: 460, y1: 365 };

    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;

    let (sh, sdx, sdy) = best_transform(&eyes_half_src, &base, w, h, &anchor);
    let eyes_half_src = warp(&eyes_half_src, sh, sdx, sdy, w, h);
    let (tt, tdx, tdy) = best_transform(&eyes_shut_src, &base, w, h, &anchor);
    let eyes_shut_src = warp(&eyes_shut_src, tt, tdx, tdy, w, h);
    let (lh, ldx, ldy) = best_transform(&tap_l, &base, w, h, &anchor);
    let tap_l = warp(&tap_l, lh, ldx, ldy, w, h);
    let (rh, rdx, rdy) = best_transform(&tap_r, &base, w, h, &anchor);
    let tap_r = warp(&tap_r, rh, rdx, rdy, w, h);
    println!(
        "align: blink/0 s={:.3} dx={} dy={} | blink/1 s={:.3} dx={} dy={} | tapL s={:.3} dx={} dy={} | tapR s={:.3} dx={} dy={}",
        sh, sdx, sdy, tt, tdx, tdy, lh, ldx, ldy, rh, rdx, rdy
    );

    let (arm_l, n_l) = layer_from_diff(&tap_l, &base, w, h, &left_box, 95, true);
    let (arm_r, n_r) = layer_from_diff(&tap_r, &base, w, h, &right_box, 95, true);
    // eyes: after alignment the frames only differ on the eyelids, so a feathered
    // patch of the whole eye band is seamless and far more robust than a diff.
    let eye_box = Box2 { x0: 178, y0: 232, x1: 342, y1: 302 };
    let eyes_half = region_patch(&eyes_half_src, &base, &eye_box, w, h);
    let eyes_shut = region_patch(&eyes_shut_src, &base, &eye_box, w, h);

    base.save(output.join("base.png")).map_err(|e| e.to_string())?;
    eyes_half.save(output.join("eyes_half.png")).map_err(|e| e.to_string())?;
    eyes_shut.save(output.join("eyes_shut.png")).map_err(|e| e.to_string())?;
    arm_l.save(output.join("arm_l.png")).map_err(|e| e.to_string())?;
    arm_r.save(output.join("arm_r.png")).map_err(|e| e.to_string())?;

    let manifest = format!(
        "{{\n  \"canvas\": {},\n  \"layers\": {{\n    \"base\": [0, 0],\n    \"eyes_half\": [0, 0],\n    \"eyes_shut\": [0, 0],\n    \"arm_l\": [0, 0],\n    \"arm_r\": [0, 0]\n  }}\n}}\n",
        w
    );
    std::fs::write(output.join("rig.json"), manifest).map_err(|e| e.to_string())?;

    // QA sheet: base, then each layer over a checker so pixels are visible
    let mut qa = RgbaImage::new(w * 3, h * 2);
    image::imageops::overlay(&mut qa, &base, 0, 0);
    image::imageops::overlay(&mut qa, &eyes_half, w as i64, 0);
    image::imageops::overlay(&mut qa, &eyes_shut, (w * 2) as i64, 0);
    image::imageops::overlay(&mut qa, &arm_l, 0, h as i64);
    image::imageops::overlay(&mut qa, &arm_r, w as i64, h as i64);
    qa.save(output.join("_rig_preview.png")).map_err(|e| e.to_string())?;

    write_motion_test(&base, &arm_l, w, h, &output.join("_motion_test.png"));

    println!("rig layers -> {} (arm_l {}px, arm_r {}px)", output.display(), n_l, n_r);
    Ok(())
}
