//! What the sample tests share: where testdata/e-invoices is, and the fetched CEN and OpenPeppol
//! examples of `fetch.txt` (from `deploy/fetch-einvoice-samples.sh`, in `target/e-invoice-samples` or
//! `OBOP_EINVOICE_SAMPLES_DIR`).

#![allow(dead_code)] // Each test file uses a part.

use std::path::{Path, PathBuf};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn samples() -> PathBuf {
    repo().join("testdata/e-invoices")
}

pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// One fetched example: its name as `set/path` and where it is on disk.
pub struct Fetched {
    pub name: String,
    /// The path inside its set, such as `cii/examples/CII_example1.xml`.
    pub src: String,
    pub path: PathBuf,
}

/// Every fetched example of `fetch.txt`, or `None` when some are missing. Missing files fail the
/// test when `OBOP_REQUIRE_EINVOICE_SAMPLES=1`, as CI sets it; otherwise this prints why it skips.
pub fn fetched() -> Option<Vec<Fetched>> {
    let dir = match std::env::var_os("OBOP_EINVOICE_SAMPLES_DIR") {
        Some(d) => repo().join(d),
        None => repo().join("target/e-invoice-samples"),
    };
    let required = std::env::var("OBOP_REQUIRE_EINVOICE_SAMPLES").is_ok_and(|v| v == "1");
    let list = String::from_utf8(read(&samples().join("fetch.txt"))).unwrap();
    let (mut found, mut missing) = (Vec::new(), Vec::new());
    for line in list
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        // set, repository, commit, SHA-256, then the path, which may contain spaces.
        let fields: Vec<&str> = line.splitn(5, ' ').collect();
        let (set, src) = (fields[0], fields[4]);
        let path = dir.join(set).join(src);
        let name = format!("{set}/{src}");
        if path.is_file() {
            found.push(Fetched {
                name,
                src: src.to_string(),
                path,
            });
        } else {
            missing.push(name);
        }
    }
    if missing.is_empty() {
        return Some(found);
    }
    let msg = format!(
        "{} fetched e-invoice sample file(s) missing from {} (first: {}); run deploy/fetch-einvoice-samples.sh",
        missing.len(),
        dir.display(),
        missing[0]
    );
    assert!(!required, "{msg} (OBOP_REQUIRE_EINVOICE_SAMPLES=1)");
    eprintln!("skipped: {msg}");
    None
}
