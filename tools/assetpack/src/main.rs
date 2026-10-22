//! assetpack: turns AI-generated art into engine-ready sprite frames.
//!
//!   assetpack pack --in art/raw --out assets
//!
//! Steps per image: optional grid slicing, magenta keying, despill, trim,
//! bottom-center re-anchor on the 512 canvas, then manifest.json.

mod rig;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

const CANVAS: u32 = 512;
const ANCHOR_Y: u32 = 496;
const MAX_FIG_H: u32 = 460;

struct Opts {
    input: PathBuf,
    output: PathBuf,
    key: [u8; 3],
    tol: f32,
    cols: u32,
    rows: u32,
    /// Opt-in per-frame trim + re-anchor. Off by default: uniform framing keeps
    /// states aligned with each other.
    trim: bool,
    /// Composite the props/ layer into every frame. Off by default: a prop drawn
    /// for its own composition rarely lines up with the character's pose, and a
    /// half-hidden prop reads as a rendering bug.
    with_props: bool,
    /// Draw props over the character instead of behind it (a desk in front).
    front_props: bool,
    /// Front prop width as a fraction of the 512 canvas.
    prop_scale: f32,
    /// Top edge of the front prop on the canvas.
    prop_y: i32,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("usage: assetpack pack --in <dir> --out <dir> [--tol 40] [--grid 2x2] [--key FF00FF]");
        return;
    }
    match args[0].as_str() {
        "pack" => {
            let mut o = Opts {
                input: PathBuf::from("art/raw"),
                output: PathBuf::from("assets"),
                key: [255, 0, 255],
                tol: 40.0,
                cols: 1,
                rows: 1,
                trim: false,
                with_props: false,
                front_props: false,
                prop_scale: 0.7,
                prop_y: 430,
            };
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--in" => { o.input = PathBuf::from(&args[i + 1]); i += 2; }
                    "--out" => { o.output = PathBuf::from(&args[i + 1]); i += 2; }
                    "--tol" => { o.tol = args[i + 1].parse().unwrap_or(40.0); i += 2; }
                    "--trim" => { o.trim = true; i += 1; }
                    "--with-props" => { o.with_props = true; i += 1; }
                    "--front-props" => { o.front_props = true; i += 1; }
                    "--prop-scale" => { o.prop_scale = args[i + 1].parse().unwrap_or(0.7); i += 2; }
                    "--prop-y" => { o.prop_y = args[i + 1].parse().unwrap_or(430); i += 2; }
                    "--key" => {
                        let h = args[i + 1].trim_start_matches('#');
                        if h.len() == 6 {
                            o.key = [
                                u8::from_str_radix(&h[0..2], 16).unwrap_or(255),
                                u8::from_str_radix(&h[2..4], 16).unwrap_or(0),
                                u8::from_str_radix(&h[4..6], 16).unwrap_or(255),
                            ];
                        }
                        i += 2;
                    }
                    "--grid" => {
                        let g = &args[i + 1];
                        let mut parts = g.split('x');
                        o.cols = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
                        o.rows = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
                        i += 2;
                    }
                    _ => { i += 1; }
                }
            }
            if let Err(e) = pack(&o) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
        "check" => {
            let mut o = Opts {
                input: PathBuf::from("art/raw"),
                output: PathBuf::from("assets"),
                key: [255, 0, 255],
                tol: 40.0,
                cols: 2,
                rows: 2,
                trim: false,
                with_props: false,
                front_props: false,
                prop_scale: 0.7,
                prop_y: 430,
            };
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--in" => { o.input = PathBuf::from(&args[i + 1]); i += 2; }
                    "--grid" => {
                        let g = &args[i + 1];
                        let mut parts = g.split('x');
                        o.cols = parts.next().and_then(|v| v.parse().ok()).unwrap_or(2);
                        o.rows = parts.next().and_then(|v| v.parse().ok()).unwrap_or(2);
                        i += 2;
                    }
                    _ => { i += 1; }
                }
            }
            if let Err(e) = check(&o) {
                eprintln!("not ready: {}", e);
                std::process::exit(1);
            }
        }
        "rig" => {
            let mut input = PathBuf::from("assets");
            let mut output = PathBuf::from("assets/rig");
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--in" => { input = PathBuf::from(&args[i + 1]); i += 2; }
                    "--out" => { output = PathBuf::from(&args[i + 1]); i += 2; }
                    _ => { i += 1; }
                }
            }
            if let Err(e) = rig::rig(&input, &output) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
        _ => println!("unknown command"),
    }
}

/// States the engine knows about, plus their playback defaults.
fn clip_settings(name: &str) -> Option<(f32, bool)> {
    match name {
        "idle" => Some((4.0, true)),
        "blink" => Some((14.0, false)),
        "tap_left" | "tap_right" => Some((20.0, false)),
        "sleep" => Some((2.0, true)),
        "wake" => Some((10.0, false)),
        _ => None,
    }
}

/// Horizontal centre of the drawn pixels, sampled every other row/column.
fn bbox_center_x(img: &RgbaImage) -> Option<f32> {
    let (mut x0, mut x1) = (u32::MAX, 0u32);
    let mut any = false;
    for y in (0..img.height()).step_by(2) {
        for x in (0..img.width()).step_by(2) {
            if img.get_pixel(x, y)[3] > 8 {
                any = true;
                x0 = x0.min(x);
                x1 = x1.max(x);
            }
        }
    }
    any.then(|| (x0 as f32 + x1 as f32) * 0.5)
}

fn shift_x(img: &RgbaImage, dx: i32) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            let sx = x as i32 - dx;
            if sx >= 0 && (sx as u32) < img.width() {
                let p = *img.get_pixel(sx as u32, y);
                if p[3] > 0 {
                    out.put_pixel(x, y, p);
                }
            }
        }
    }
    out
}

/// A prop drawn over the character: trimmed, scaled to a fraction of the canvas
/// and pinned so only its front edge is on screen.
fn make_front_prop(img: &RgbaImage, o: &Opts) -> RgbaImage {
    let keyed = key_out(img, o);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..keyed.height() {
        for x in 0..keyed.width() {
            if keyed.get_pixel(x, y)[3] > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let mut canvas = RgbaImage::new(CANVAS, CANVAS);
    if x0 == u32::MAX {
        return canvas;
    }
    let crop = image::imageops::crop_imm(&keyed, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    let tw = ((CANVAS as f32 * o.prop_scale).round() as u32).max(1);
    let th = ((crop.height() as f32) * (tw as f32) / (crop.width() as f32)).round().max(1.0) as u32;
    let resized = resize_rgba(&crop, tw, th);
    let ox = (CANVAS as i32 - tw as i32) / 2;
    for y in 0..th as i32 {
        for x in 0..tw as i32 {
            let (cx, cy) = (ox + x, o.prop_y + y);
            if cx >= 0 && cy >= 0 && cx < CANVAS as i32 && cy < CANVAS as i32 {
                let p = *resized.get_pixel(x as u32, y as u32);
                if p[3] > 0 {
                    canvas.put_pixel(cx as u32, cy as u32, p);
                }
            }
        }
    }
    canvas
}

/// Align every state's frames onto the idle pose's geometry.
///
/// Each sheet was generated separately, so the character sits a couple of
/// percent larger/shifted in one sheet than another. Switching states then
/// jumps: that jump is what reads as twitching. Warping each state onto the
/// idle geometry removes it without touching the drawings.
fn align_states(frames: &mut BTreeMap<String, Vec<RgbaImage>>, w: u32, h: u32) {
    let Some(reference) = frames.get("idle").and_then(|v| v.first().cloned()) else {
        return;
    };
    // lap and legs never change pose: a stable registration target
    let anchor = rig::Box2::new(150, 350, 380, 470);
    let names: Vec<String> = frames.keys().cloned().collect();
    for name in names {
        if name == "idle" {
            continue;
        }
        let Some(first) = frames.get(&name).and_then(|v| v.first().cloned()) else {
            continue;
        };
        let (s, dx, dy) = rig::best_transform(&first, &reference, w, h, &anchor);
        if (s - 1.0).abs() < 0.002 && dx.abs() < 2 && dy.abs() < 2 {
            continue;
        }
        if let Some(list) = frames.get_mut(&name) {
            for f in list.iter_mut() {
                *f = rig::warp(f, s, dx, dy, w, h);
            }
        }
        println!("aligned {}: scale {:.3} shift {},{:+} ", name, s, dx, dy);
    }
}

fn pack(o: &Opts) -> Result<(), String> {
    if !o.input.is_dir() {
        return Err(format!("input dir not found: {}", o.input.display()));
    }
    std::fs::create_dir_all(&o.output).map_err(|e| e.to_string())?;

    let mut frames: BTreeMap<String, Vec<RgbaImage>> = BTreeMap::new();
    let mut props: Vec<RgbaImage> = Vec::new();
    let mut front: Vec<RgbaImage> = Vec::new();
    let mut raw_props: Vec<RgbaImage> = Vec::new();

    for entry in std::fs::read_dir(&o.input).map_err(|e| e.to_string())?.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if clip_settings(&name).is_none() && (name != "props" || !(o.with_props || o.front_props)) {
            continue;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&p)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|q| q.extension().map(|x| x == "png").unwrap_or(false))
            .collect();
        files.sort();
        let mut out_imgs: Vec<RgbaImage> = Vec::new();
        for f in files {
            let img = image::open(&f).map_err(|e| format!("{}: {}", f.display(), e))?.to_rgba8();
            // props are single full-canvas images; never grid-slice them
            let (cols, rows) = if name == "props" { (1, 1) } else { (o.cols, o.rows) };
            for cell in slice(&img, cols, rows) {
                if !has_content(&cell, o) {
                    continue; // empty grid slot (e.g. the unused 4th cell)
                }
                if name == "props" {
                    raw_props.push(cell);
                } else {
                    out_imgs.push(process_cell(&cell, o));
                }
            }
        }
        if name == "props" {
            if o.front_props {
                front = raw_props.drain(..).map(|c| make_front_prop(&c, o)).collect();
            } else if o.with_props {
                props = raw_props.drain(..).map(|c| process_cell(&c, o)).collect();
            }
            continue;
        }
        if out_imgs.is_empty() {
            continue;
        }
        frames.insert(name, out_imgs);
    }

    // The sheets are generated one state at a time, so the character can sit a
    // few pixels left or right in one sheet and not another. Align every state
    // to the idle pose's centre so switching states does not jump sideways.
    let reference = frames.get("idle").and_then(|v| bbox_center_x(&v[0]));
    if let Some(rc) = reference {
        for (name, imgs) in frames.iter_mut() {
            if name == "idle" {
                continue;
            }
            if let Some(c) = bbox_center_x(&imgs[0]) {
                let dx = (rc - c).round() as i32;
                if dx.abs() >= 3 {
                    for im in imgs.iter_mut() {
                        *im = shift_x(im, dx);
                    }
                }
            }
        }
    }

    align_states(&mut frames, CANVAS, CANVAS);

    let mut clips: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut preview: Vec<RgbaImage> = Vec::new();
    for (name, imgs) in &frames {
        let mut paths = Vec::new();
        for (i, im) in imgs.iter().enumerate() {
            let rel = format!("{}/{:02}.png", name, i);
            let dst = o.output.join(&rel);
            std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
            im.save(&dst).map_err(|e| e.to_string())?;
            paths.push(rel);
        }
        preview.extend(imgs.iter().cloned());
        clips.insert(name.clone(), paths);
    }
    let mut prop_paths = Vec::new();
    for (i, im) in props.iter().enumerate() {
        let rel = format!("props/{:02}.png", i);
        let dst = o.output.join(&rel);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        im.save(&dst).map_err(|e| e.to_string())?;
        prop_paths.push(rel);
    }
    let mut front_paths = Vec::new();
    for (i, im) in front.iter().enumerate() {
        let rel = format!("front/{:02}.png", i);
        let dst = o.output.join(&rel);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        im.save(&dst).map_err(|e| e.to_string())?;
        front_paths.push(rel);
    }

    // optional app icon: input/icon.png, keyed and clamped to 256
    let mut icon_rel: Option<String> = None;
    let icon_src = o.input.join("icon.png");
    if icon_src.is_file() {
        if let Ok(img) = image::open(&icon_src) {
            let keyed = key_out(&img.to_rgba8(), o);
            let icon = resize_rgba(&keyed, 256, 256);
            icon.save(o.output.join("icon.png")).map_err(|e| e.to_string())?;
            icon_rel = Some("icon.png".into());
        }
    }

    write_manifest(&o.output, &clips, &prop_paths, &front_paths, icon_rel.as_deref())?;
    write_preview(&o.output, &preview)?;
    println!(
        "packed {} clips, {} frames, {} props -> {}",
        clips.len(),
        preview.len(),
        prop_paths.len(),
        o.output.display()
    );
    Ok(())
}

fn slice(img: &RgbaImage, cols: u32, rows: u32) -> Vec<RgbaImage> {
    if cols <= 1 && rows <= 1 {
        return vec![img.clone()];
    }
    let (w, h) = img.dimensions();
    let cw = w / cols;
    let ch = h / rows;
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let mut cell = RgbaImage::new(cw, ch);
            for y in 0..ch {
                for x in 0..cw {
                    cell.put_pixel(x, y, *img.get_pixel(c * cw + x, r * ch + y));
                }
            }
            out.push(cell);
        }
    }
    out
}

fn dist(a: [u8; 3], b: [u8; 3]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

/// Resize RGBA through premultiplied alpha so transparent pixels do not bleed
/// their (keyed-out magenta) colour into the edges.
fn resize_rgba(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut pre = img.clone();
    for p in pre.pixels_mut() {
        let a = p[3] as u32;
        p[0] = ((p[0] as u32 * a) / 255) as u8;
        p[1] = ((p[1] as u32 * a) / 255) as u8;
        p[2] = ((p[2] as u32 * a) / 255) as u8;
    }
    let mut out = image::imageops::resize(&pre, w, h, image::imageops::FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p[3] as u32;
        if a > 0 {
            p[0] = ((p[0] as u32 * 255) / a).min(255) as u8;
            p[1] = ((p[1] as u32 * 255) / a).min(255) as u8;
            p[2] = ((p[2] as u32 * 255) / a).min(255) as u8;
        }
    }
    out
}

fn key_out(img: &RgbaImage, o: &Opts) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut keyed = RgbaImage::new(w, h);
    let has_alpha = img.pixels().any(|p| p[3] < 240);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let mut px = [p[0], p[1], p[2]];
            let mut a = p[3] as f32;
            if !has_alpha {
                let d = dist(px, o.key);
                if d < o.tol {
                    a = 0.0;
                } else if d < o.tol * 1.8 {
                    a = a * ((d - o.tol) / (o.tol * 0.8)).clamp(0.0, 1.0);
                }
                // despill the magenta fringe
                let g = px[1] as f32;
                let spill = (px[0] as f32).min(px[2] as f32) - g;
                if spill > 0.0 {
                    let k = spill.clamp(0.0, 255.0);
                    px[0] = (px[0] as f32 - k).clamp(0.0, 255.0) as u8;
                    px[2] = (px[2] as f32 - k).clamp(0.0, 255.0) as u8;
                }
            }
            keyed.put_pixel(x, y, Rgba([px[0], px[1], px[2], a as u8]));
        }
    }
    keyed
}

/// True when a cell actually contains a drawing rather than a flat background.
fn has_content(cell: &RgbaImage, o: &Opts) -> bool {
    let total = (cell.width() * cell.height()).max(1) as u64;
    let mut drawn = 0u64;
    for p in cell.pixels() {
        if p[3] < 16 {
            drawn += 1;
            continue;
        }
        if dist([p[0], p[1], p[2]], o.key) > o.tol {
            drawn += 1;
        }
    }
    drawn * 200 > total
}
/// One cell -> one 512 canvas frame.
///
/// The framing inside the cell is preserved on purpose: every sheet was drawn
/// with the character at the same spot, so uniform scaling keeps frames aligned
/// across states. Per-frame trimming and re-centring is what would introduce
/// jitter between idle and tap.
fn process_cell(img: &RgbaImage, o: &Opts) -> RgbaImage {
    let keyed = key_out(img, o);
    let mut canvas = resize_rgba(&keyed, CANVAS, CANVAS);
    if !o.trim {
        return canvas;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (CANVAS, CANVAS, 0u32, 0u32);
    for y in 0..CANVAS {
        for x in 0..CANVAS {
            if canvas.get_pixel(x, y)[3] > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return canvas;
    }
    let trimmed = image::imageops::crop_imm(&canvas, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    let (tw, th) = trimmed.dimensions();
    let scale = if th > MAX_FIG_H { MAX_FIG_H as f32 / th as f32 } else { 1.0 };
    let nw = ((tw as f32 * scale).round() as u32).max(1);
    let nh = ((th as f32 * scale).round() as u32).max(1);
    let resized = resize_rgba(&trimmed, nw, nh);
    canvas = RgbaImage::new(CANVAS, CANVAS);
    let ox = (CANVAS as i32 - nw as i32) / 2;
    let oy = ANCHOR_Y as i32 - nh as i32;
    for y in 0..nh as i32 {
        for x in 0..nw as i32 {
            let p = *resized.get_pixel(x as u32, y as u32);
            let (cx, cy) = (ox + x, oy + y);
            if cx >= 0 && cy >= 0 && cx < CANVAS as i32 && cy < CANVAS as i32 {
                canvas.put_pixel(cx as u32, cy as u32, p);
            }
        }
    }
    canvas
}

fn check(o: &Opts) -> Result<(), String> {
    let required: [(&str, usize); 5] = [
        ("idle", 4),
        ("blink", 2),
        ("tap_left", 3),
        ("tap_right", 3),
        ("sleep", 2),
    ];
    let optional: [(&str, usize); 2] = [("wake", 1), ("props", 1)];

    println!("checking {}", o.input.display());
    println!();
    println!("{:<12} {:>8} {:>8}  {}", "state", "have", "want", "notes");
    println!("{:-<12} {:->8} {:->8}  {:-<40}", "", "", "", "");

    let mut missing = 0usize;
    let mut total_frames = 0usize;

    for (name, want) in required.iter().chain(optional.iter()) {
        let is_optional = optional.iter().any(|(n, _)| n == name);
        let dir = o.input.join(name);
        let mut cells = 0usize;
        let mut notes: Vec<String> = Vec::new();
        if dir.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
                .collect();
            files.sort();
            if files.is_empty() {
                notes.push("directory is empty".into());
            }
            for f in &files {
                match image::open(f) {
                    Ok(img) => {
                        let (w, h) = (img.width(), img.height());
                        let img = img.to_rgba8();
                        let is_props = name == &"props";
                        let (cols, rows) = if is_props || (w == 512 && h == 512) {
                            (1, 1)
                        } else {
                            (o.cols, o.rows)
                        };
                        if w % cols != 0 || h % rows != 0 {
                            notes.push(format!("{}: {}x{} does not split {}x{}", f.file_name().unwrap().to_string_lossy(), w, h, cols, rows));
                            continue;
                        }
                        let cw = w / cols;
                        let ch = h / rows;
                        let mut bg_pixels = 0u32;
                        let mut bg_total = 0u32;
                        for r in 0..rows {
                            for c in 0..cols {
                                let cellimg =
                                    image::imageops::crop_imm(&img, c * cw, r * ch, cw, ch).to_image();
                                if !has_content(&cellimg, o) {
                                    continue; // an unused grid slot
                                }
                                cells += 1;
                                for y in (0..ch).step_by(2) {
                                    for x in (0..cw).step_by(2) {
                                        let p = cellimg.get_pixel(x, y);
                                        if p[3] > 200 {
                                            bg_total += 1;
                                            if p[0] > 200 && p[1] < 60 && p[2] > 200 {
                                                bg_pixels += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if bg_total > 0 {
                            let ratio = bg_pixels as f32 / bg_total as f32;
                            if ratio < 0.35 {
                                notes.push(format!(
                                    "{}: only {:.0}% magenta background - is it the right colour?",
                                    f.file_name().unwrap().to_string_lossy(),
                                    ratio * 100.0
                                ));
                            }
                        }
                    }
                    Err(e) => notes.push(format!("{}: {}", f.display(), e)),
                }
            }
        } else {
            notes.push("missing directory".into());
        }

        let ok = cells >= *want;
        if !ok && !is_optional {
            missing += want - cells;
        }
        total_frames += cells;
        let status = if ok { "ok" } else if is_optional { "optional" } else { "MISSING" };
        let mut note = notes.join("; ");
        if note.is_empty() {
            note = status.into();
        } else {
            note = format!("{} ({})", note, status);
        }
        println!("{:<12} {:>8} {:>8}  {}", name, cells, want, note);
    }

    println!();
    if missing == 0 {
        println!("ready: {} frames found. run pack next.", total_frames);
        Ok(())
    } else {
        Err(format!("{} frame(s) still missing", missing))
    }
}
fn write_manifest(
    out: &Path,
    clips: &BTreeMap<String, Vec<String>>,
    props: &[String],
    front: &[String],
    icon: Option<&str>,
) -> Result<(), String> {
    let mut s = String::from("{\n  \"canvas\": 512,\n");
    if let Some(i) = icon {
        s.push_str(&format!("  \"icon\": \"{}\",\n", i));
    }
    s.push_str("  \"front\": [");
    for (i, p) in front.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("\"{}\"", p));
    }
    s.push_str("],\n  \"props\": [");
    for (i, p) in props.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("\"{}\"", p));
    }
    s.push_str("],\n  \"clips\": {\n");
    let n = clips.len();
    for (i, (name, frames)) in clips.iter().enumerate() {
        let (fps, looping) = clip_settings(name).unwrap_or((8.0, false));
        s.push_str(&format!("    \"{}\": {{\"fps\": {}, \"looping\": {}, \"frames\": [", name, fps, looping));
        for (j, f) in frames.iter().enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("\"{}\"", f));
        }
        s.push_str("]}");
        if i + 1 < n {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  }\n}\n");
    std::fs::write(out.join("manifest.json"), s).map_err(|e| e.to_string())
}

fn write_preview(out: &Path, frames: &[RgbaImage]) -> Result<(), String> {
    if frames.is_empty() {
        return Ok(());
    }
    let cols = frames.len().min(4) as u32;
    let rows = ((frames.len() as u32) + cols - 1) / cols;
    let cell = 192u32;
    let mut sheet = RgbaImage::from_pixel(cols * cell, rows * cell, Rgba([28, 32, 44, 255]));
    for (i, f) in frames.iter().enumerate() {
        let small = image::imageops::resize(f, cell, cell, image::imageops::FilterType::Triangle);
        let cx = (i as u32 % cols) * cell;
        let cy = (i as u32 / cols) * cell;
        image::imageops::overlay(&mut sheet, &small, cx as i64, cy as i64);
    }
    sheet.save(out.join("_preview.png")).map_err(|e| e.to_string())
}
