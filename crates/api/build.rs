//! Embed the bundled template library (`templates/*.toml`) at build time, so
//! every front end ships the same copy without reading files at run time.
//! Also packs the vendored appliance library (`library/`, D-147) into one
//! gzip-compressed bundle (`housewright_core::appliances::bundle`'s format):
//! about 470 KB of TOML becomes about 90 KB in the binary.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

fn library_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("library directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            library_files(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "toml") {
            println!("cargo:rerun-if-changed={}", p.display());
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, std::fs::read_to_string(&p).unwrap());
        }
    }
}

fn pack_library(manifest: &Path) {
    let root = manifest.join("../../library");
    let mut files = BTreeMap::new();
    library_files(&root, &root, &mut files);
    let json = serde_json::json!({
        "format": "housewright-library-bundle",
        "bundle": 1,
        "files": files,
    });
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(json.to_string().as_bytes()).unwrap();
    let gz = enc.finish().unwrap();
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("library.json.gz");
    std::fs::write(dest, gz).unwrap();
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dir = manifest.join("../../templates");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("templates directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut out = String::from(
        "/// Bundled templates: (file name, TOML text).\npub static BUNDLED: &[(&str, &str)] = &[\n",
    );
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let name = f.file_name().unwrap().to_string_lossy();
        let abs = f.canonicalize().unwrap();
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            abs.display().to_string()
        ));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bundled_templates.rs");
    std::fs::write(dest, out).unwrap();
    pack_library(&manifest);
}
