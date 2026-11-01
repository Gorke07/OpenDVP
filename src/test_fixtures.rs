//! Large reference files (reverse-engineering captures, vendor sample projects) are not part of the
//! repository. Tests that need them look in `analysis/` next to `Cargo.toml` and are skipped when
//! the files are absent.

pub fn read(rel: &str) -> Option<Vec<u8>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("analysis").join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipped: fixture {} is not present", path.display());
            None
        }
    }
}

/// Bytes of a fixture, or return from the calling test when it is missing.
macro_rules! fixture {
    ($rel:expr) => {
        match crate::test_fixtures::read($rel) {
            Some(bytes) => bytes,
            None => return,
        }
    };
}

/// Return from the calling test when a fixture is missing.
macro_rules! skip_without_fixtures {
    ($rel:expr) => {
        if crate::test_fixtures::read($rel).is_none() {
            return;
        }
    };
}
