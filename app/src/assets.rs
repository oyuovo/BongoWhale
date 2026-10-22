//! Loads the sprite pack: external ./assets first (for iteration), then the copy
//! embedded in the exe at build time (green single-file distribution).

use std::path::PathBuf;

use crate::sprite::{Clip, ClipId, Frame, Sprites};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

#[derive(serde::Deserialize)]
struct Manifest {
    canvas: u32,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    front: Vec<String>,
    #[serde(default)]
    props: Vec<String>,
    clips: std::collections::HashMap<String, ClipDef>,
}

#[derive(serde::Deserialize)]
struct ClipDef {
    fps: f32,
    #[serde(default)]
    looping: bool,
    frames: Vec<String>,
}

fn premultiply(img: &image::RgbaImage) -> Frame {
    let (w, h) = img.dimensions();
    let mut px = vec![0u8; (w * h * 4) as usize];
    for (i, p) in img.pixels().enumerate() {
        let a = p[3] as u32;
        px[i * 4] = ((p[2] as u32 * a) / 255) as u8;
        px[i * 4 + 1] = ((p[1] as u32 * a) / 255) as u8;
        px[i * 4 + 2] = ((p[0] as u32 * a) / 255) as u8;
        px[i * 4 + 3] = p[3];
    }
    Frame { w, h, px }
}

fn decode(bytes: &[u8]) -> Option<Frame> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(premultiply(&img))
}

/// src-over of premultiplied BGRA: character on top of the static prop layer.
fn composite(top: &Frame, bottom: &Frame) -> Frame {
    let mut out = bottom.px.clone();
    for i in (0..top.px.len()).step_by(4) {
        let a = top.px[i + 3] as u32;
        if a == 0 {
            continue;
        }
        let inv = 255 - a;
        for c in 0..3 {
            out[i + c] = (top.px[i + c] as u32 + (out[i + c] as u32 * inv) / 255).min(255) as u8;
        }
        out[i + 3] = (a + (out[i + 3] as u32 * inv) / 255).min(255) as u8;
    }
    Frame { w: top.w, h: top.h, px: out }
}

fn parse(manifest: &str, get: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Sprites> {
    let m: Manifest = serde_json::from_str(manifest).ok()?;
    let mut s = Sprites::new();
    s.canvas = m.canvas;

    // static layers drawn behind the character (desk, keyboard, ...)
    let prop = m
        .props
        .iter()
        .filter_map(|p| get(p).and_then(|b| decode(&b)))
        .reduce(|a, b| composite(&b, &a));
    // layers drawn in front of the character (a desk edge closest to the viewer)
    let front = m
        .front
        .iter()
        .filter_map(|p| get(p).and_then(|b| decode(&b)))
        .reduce(|a, b| composite(&a, &b));

    for (name, def) in &m.clips {
        let Some(id) = ClipId::all().into_iter().find(|c| c.key() == name.as_str()) else {
            continue; // unknown clip in the manifest: ignore, do not fail the whole pack
        };
        let mut frames: Vec<Frame> = def.frames.iter().filter_map(|f| get(f).and_then(|b| decode(&b))).collect();
        if let Some(p) = &prop {
            if frames.iter().all(|f| f.w == p.w && f.h == p.h) {
                frames = frames.iter().map(|f| composite(f, p)).collect();
            }
        }
        if let Some(p) = &front {
            if frames.iter().all(|f| f.w == p.w && f.h == p.h) {
                frames = frames.iter().map(|f| composite(p, f)).collect();
            }
        }
        if frames.is_empty() {
            continue;
        }
        s.insert(id, Clip { fps: def.fps, looping: def.looping, frames });
    }
    s.icon = m.icon.as_ref().and_then(|p| get(p)).and_then(|b| decode(&b));
    if s.has(ClipId::Idle) {
        Some(s)
    } else {
        None
    }
}

fn external_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let near = exe.parent()?.join("assets");
    if near.join("manifest.json").is_file() {
        return Some(near);
    }
    let cwd = std::env::current_dir().ok()?.join("assets");
    if cwd.join("manifest.json").is_file() {
        return Some(cwd);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("assets");
    if dev.join("manifest.json").is_file() {
        return Some(dev);
    }
    None
}

pub fn load() -> Option<Sprites> {
    if let Some(dir) = external_dir() {
        let manifest = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        let s = parse(&manifest, |rel| std::fs::read(dir.join(rel)).ok());
        if s.is_some() {
            return s;
        }
    }
    let manifest = embedded::EMBEDDED.iter().find(|(n, _)| *n == "manifest.json")?.1;
    let manifest = std::str::from_utf8(manifest).ok()?;
    parse(manifest, |rel| {
        embedded::EMBEDDED.iter().find(|(n, _)| *n == rel).map(|(_, b)| b.to_vec())
    })
}
