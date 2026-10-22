use std::path::{Path, PathBuf};

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let assets = Path::new(&manifest).join("..").join("assets");
    println!("cargo:rerun-if-changed={}", assets.display());

    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    if assets.is_dir() {
        collect(&assets, &assets, &mut entries);
    }
    entries.sort();

    let out = std::env::var("OUT_DIR").unwrap();
    let mut code = String::new();
    code.push_str("pub static EMBEDDED: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &entries {
        let abs = abs.to_string_lossy().replace('\\', "/");
        code.push_str(&format!(
            "    ({:?}, include_bytes!(r\"{}\")),\n",
            rel, abs
        ));
    }
    code.push_str("];\n");
    std::fs::write(Path::new(&out).join("embedded.rs"), code).unwrap();
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(root, &p, out);
        } else if p.extension().map(|x| x == "png" || x == "json").unwrap_or(false) {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, p));
        }
    }
}
