//! library-sync: bring a pinned version of the appliance library
//! (`holdTheDoorHoid/housewright-library`, CC BY 4.0) into `library/`, check
//! it the way the app will, and build the files a library release publishes.
//!
//! ```text
//! library-sync copy --from ../housewright-library [--version 2026.11.28]
//!     copy a local checkout (must be clean); the commit is recorded
//! library-sync fetch --tag v2026.11.28 [--version ...]
//!     download that tag's tarball from GitHub and copy it
//! library-sync check
//!     check library/ against templates/ (what the tests also do)
//! library-sync bundle --out DIR
//!     write DIR/latest.json and DIR/library-<version>.json.gz from library/,
//!     the two files the app's "Get appliance library updates" reads
//! ```
//!
//! Run from the repository root. It only ever writes inside `library/` (and
//! `--out`). Licensed AGPL-3.0-only.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use housewright_core::appliances::bundle::{self, Bundle, MANIFEST_FORMAT, Manifest};
use housewright_core::appliances::{Library, SnapshotMeta};
use housewright_core::template::Library as Templates;

const REPO: &str = "https://github.com/holdTheDoorHoid/housewright-library";
const OUT: &str = "library";
/// The library's data directories and files, the only things copied.
const DATA_DIRS: &[&str] = &["categories", "brands", "families", "reference"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let result = match args.first().map(String::as_str) {
        Some("copy") => match flag("--from") {
            Some(from) => copy_checkout(Path::new(&from), flag("--version")),
            None => Err("copy needs --from <library checkout>".into()),
        },
        Some("fetch") => match flag("--tag") {
            Some(tag) => fetch_tag(&tag, flag("--version")),
            None => Err("fetch needs --tag <tag>".into()),
        },
        Some("check") => check().map(|l| {
            println!(
                "library {} ({}): {} categories, {} brands, {} families; no problems",
                l.meta.version,
                &l.meta.commit[..l.meta.commit.len().min(12)],
                l.categories.len(),
                l.brands.len(),
                l.families.len()
            );
        }),
        Some("bundle") => match flag("--out") {
            Some(out) => write_release(Path::new(&out)),
            None => Err("bundle needs --out <dir>".into()),
        },
        _ => Err(
            "usage: library-sync copy --from DIR | fetch --tag TAG | check | bundle --out DIR"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("library-sync: {e}");
            ExitCode::FAILURE
        }
    }
}

fn templates() -> Result<Templates, String> {
    Templates::load_dir("templates").map_err(|e| format!("loading templates/: {e}"))
}

/// Every file of a library tree as (relative path, text), plus its LICENSE.
fn read_tree(root: &Path) -> Result<(BTreeMap<String, String>, String), String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(root, &p, out)?;
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if !bundle::path_allowed(&rel) {
                return Err(format!("unexpected file in the library: {rel}"));
            }
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{rel}: {e}"))?;
            out.insert(rel, text);
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    for d in DATA_DIRS {
        walk(root, &root.join(d), &mut files)?;
    }
    let licence =
        std::fs::read_to_string(root.join("LICENSE")).map_err(|e| format!("LICENSE: {e}"))?;
    if !licence.contains("Attribution 4.0 International") {
        return Err("the library's LICENSE is not CC BY 4.0; stopping".into());
    }
    Ok((files, licence))
}

fn snapshot_toml(version: &str, commit: &str, published: &str) -> String {
    format!(
        "# Written by tools/library-sync; do not edit by hand.\n\
         # The appliance library data in this folder is CC BY 4.0 (LICENSE).\n\
         format = 1\n\
         version = \"{version}\"\n\
         commit = \"{commit}\"\n\
         source = \"{REPO}\"\n\
         published = {published}\n\
         licence = \"CC BY 4.0\"\n\
         credit = \"Housewright appliance library contributors\"\n"
    )
}

/// Check and write `library/` from a tree's files.
fn install(
    files: BTreeMap<String, String>,
    licence: String,
    commit: &str,
    published: &str,
    version: Option<String>,
) -> Result<(), String> {
    let version = version.unwrap_or_else(|| published.replace('-', "."));
    if bundle::parse_version(&version).is_none() {
        return Err(format!("'{version}' is not a version (use dotted numbers)"));
    }
    let mut all = files.clone();
    all.insert(
        "SNAPSHOT.toml".into(),
        snapshot_toml(&version, commit, published),
    );
    let b = Bundle::new(all.clone());
    let lib = Library::load(&b, &templates()?, true)?;

    let out = Path::new(OUT);
    if out.exists() {
        for d in DATA_DIRS {
            let p = out.join(d);
            if p.exists() {
                std::fs::remove_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
    }
    for (rel, text) in &all {
        let p = out.join(rel);
        std::fs::create_dir_all(p.parent().expect("has a parent"))
            .map_err(|e| format!("{}: {e}", p.display()))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    std::fs::write(out.join("LICENSE"), licence).map_err(|e| e.to_string())?;
    println!(
        "library/ now holds version {version} (commit {commit}): {} brands, {} families",
        lib.brands.len(),
        lib.families.len()
    );
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("running git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn copy_checkout(from: &Path, version: Option<String>) -> Result<(), String> {
    let mut status_args = vec!["status", "--porcelain", "--"];
    status_args.extend(DATA_DIRS);
    status_args.push("LICENSE");
    let dirty = git(from, &status_args)?;
    if !dirty.is_empty() {
        return Err(format!(
            "the checkout has uncommitted changes to its data; commit or stash them first:\n{dirty}"
        ));
    }
    let commit = git(from, &["rev-parse", "HEAD"])?;
    let published = git(from, &["log", "-1", "--format=%cs"])?;
    let (files, licence) = read_tree(from)?;
    install(files, licence, &commit, &published, version)
}

fn fetch_tag(tag: &str, version: Option<String>) -> Result<(), String> {
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(format!("'{tag}' is not a tag name"));
    }
    let url = format!("{REPO}/archive/refs/tags/{tag}.tar.gz");
    eprintln!("downloading {url}");
    let mut res = ureq::get(&url)
        .header("User-Agent", "housewright-library-sync")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let gz = res
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(&gz[..])
        .take(256 * 1024 * 1024)
        .read_to_end(&mut tar)
        .map_err(|e| format!("not a gzip tarball: {e}"))?;
    let (entries, commit) = untar(&tar)?;
    let commit = commit.ok_or("the tarball does not say which commit it is")?;
    let mut files = BTreeMap::new();
    let mut licence = None;
    for (path, bytes) in entries {
        // GitHub puts everything under "<repo>-<ref>/".
        let Some((_, rel)) = path.split_once('/') else {
            continue;
        };
        let text = || String::from_utf8(bytes.clone()).map_err(|_| format!("{rel} is not text"));
        if rel == "LICENSE" {
            licence = Some(text()?);
        } else if DATA_DIRS.iter().any(|d| rel.starts_with(&format!("{d}/"))) {
            if !bundle::path_allowed(rel) {
                return Err(format!("unexpected file in the library: {rel}"));
            }
            files.insert(rel.to_string(), text()?);
        }
    }
    let licence = licence.ok_or("the tarball has no LICENSE")?;
    if !licence.contains("Attribution 4.0 International") {
        return Err("the library's LICENSE is not CC BY 4.0; stopping".into());
    }
    // Tags are versions ("v2026.12.1"); the first three parts are the date.
    let version = version.unwrap_or_else(|| tag.trim_start_matches('v').to_string());
    let parts = bundle::parse_version(&version)
        .filter(|p| p.len() >= 3)
        .ok_or_else(|| {
            format!("'{version}' is not a dated version like 2026.12.1; pass --version")
        })?;
    let published = format!("{:04}-{:02}-{:02}", parts[0], parts[1], parts[2]);
    install(files, licence, &commit, &published, Some(version))
}

/// A minimal reader for the ustar files GitHub serves: regular files, and
/// the global pax header whose `comment` is the commit hash.
/// (path, bytes) of each file, and the commit hash when given.
type Untarred = (Vec<(String, Vec<u8>)>, Option<String>);

fn untar(t: &[u8]) -> Result<Untarred, String> {
    let field = |b: &[u8]| {
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).to_string()
    };
    let mut out = Vec::new();
    let mut commit = None;
    let mut at = 0;
    while at + 512 <= t.len() {
        let h = &t[at..at + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let name = field(&h[0..100]);
        let size = usize::from_str_radix(field(&h[124..136]).trim(), 8)
            .map_err(|_| "a tar header is damaged".to_string())?;
        let kind = h[156];
        let prefix = field(&h[345..500]);
        let full = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let start = at + 512;
        let end = start
            .checked_add(size)
            .filter(|e| *e <= t.len())
            .ok_or("the tarball is cut short")?;
        let body = &t[start..end];
        match kind {
            b'g' => {
                for line in String::from_utf8_lossy(body).lines() {
                    if let Some((_, kv)) = line.split_once(' ')
                        && let Some(c) = kv.strip_prefix("comment=")
                    {
                        commit = Some(c.trim().to_string());
                    }
                }
            }
            b'0' | 0 => {
                if full.contains("..") {
                    return Err(format!("unsafe path in the tarball: {full}"));
                }
                out.push((full, body.to_vec()));
            }
            _ => {}
        }
        at = start + size.div_ceil(512) * 512;
    }
    Ok((out, commit))
}

fn check() -> Result<Library, String> {
    let (mut files, _) = read_tree(Path::new(OUT))?;
    let snap = std::fs::read_to_string(Path::new(OUT).join("SNAPSHOT.toml"))
        .map_err(|e| format!("library/SNAPSHOT.toml: {e}"))?;
    files.insert("SNAPSHOT.toml".into(), snap);
    Library::load(&Bundle::new(files), &templates()?, true)
}

fn write_release(out: &Path) -> Result<(), String> {
    let lib = check()?;
    let meta: &SnapshotMeta = &lib.meta;
    let (mut files, _) = read_tree(Path::new(OUT))?;
    files.insert(
        "SNAPSHOT.toml".into(),
        std::fs::read_to_string(Path::new(OUT).join("SNAPSHOT.toml")).map_err(|e| e.to_string())?,
    );
    let gz = Bundle::new(files).to_gz();
    let name = format!("library-{}.json.gz", meta.version);
    let manifest = Manifest {
        format: MANIFEST_FORMAT.into(),
        library_format: 1,
        version: meta.version.clone(),
        commit: meta.commit.clone(),
        published: meta.published.clone(),
        bundle: name.clone(),
        sha256: bundle::sha256_hex(&gz),
        size: gz.len() as u64,
    };
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    std::fs::write(out.join(&name), &gz).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    // Check what we wrote reads back.
    Manifest::parse(&json)?.verify(&gz)?;
    std::fs::write(out.join("latest.json"), json + "\n").map_err(|e| e.to_string())?;
    println!(
        "wrote {} ({} bytes) and latest.json",
        out.join(&name).display(),
        gz.len()
    );
    Ok(())
}
