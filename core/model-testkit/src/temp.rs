// SPDX-License-Identifier: MIT
//! Temporary folders of a test's own (mitcad#1017).
//!
//! A test that names its folder in the system's temporary folder after the
//! process alone shares it with every other test of its binary that runs
//! at the same time and uses the same name (a helper several tests call),
//! and an earlier process with the same id may have left one behind. These
//! names add a counter of the process, so no other test touches the
//! folder; [`remove_temp`] cleans up without failing on what is gone
//! already.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A path of the test's own in the system's temporary folder, not created:
/// `mitcad-<name>-<process>-<counter>`. A leftover of the same name is
/// removed. For a test whose code under test creates the folder itself.
pub fn unique_temp_path(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "mitcad-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    remove_temp(&path);
    path
}

/// A new, empty folder of the test's own in the system's temporary folder
/// ([`unique_temp_path`], created).
pub fn unique_temp_dir(name: &str) -> PathBuf {
    let dir = unique_temp_path(name);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

/// Removes a test's temporary folder or file. One that is not there is no
/// error; any other failure is.
pub fn remove_temp(path: &Path) {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    if let Err(e) = result
        && e.kind() != ErrorKind::NotFound
    {
        panic!("cannot remove {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_folder_is_new_and_its_own() {
        let a = unique_temp_dir("testkit-temp");
        let b = unique_temp_dir("testkit-temp");
        assert_ne!(a, b);
        assert!(a.is_dir() && b.is_dir());
        std::fs::write(a.join("file"), b"x").unwrap();
        assert_eq!(std::fs::read_dir(&b).unwrap().count(), 0);
        let path = unique_temp_path("testkit-temp");
        assert!(!path.exists());
        remove_temp(&a);
        remove_temp(&b);
        assert!(!a.exists() && !b.exists());
        // Gone already: no error.
        remove_temp(&a);
        remove_temp(&path);
    }
}
