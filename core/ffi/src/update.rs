// SPDX-License-Identifier: MIT
//! Automatic updates for C++ (mitcad#9, `mitcad_update`): the application
//! fetches the release manifest and the download with Qt Network; reading
//! and verifying them, and choosing what is offered, is done here.

use std::path::Path;

use mitcad_update::{Channel, UpdateError};

#[cxx::bridge(namespace = "mitcad::updates")]
mod ffi {
    extern "Rust" {
        /// Reads a manifest file, verifies its signature with `keys` (public
        /// keys in hex, separated by white space; none refuses every
        /// manifest), and tells what it offers version `current` of
        /// `platform` ("windows-x64", "linux-x64") on the channel, the
        /// skipped version (or "") set apart: JSON {"status": "available" |
        /// "skipped" | "current", "version", "date", "notes", "prerelease",
        /// "asset": {"url", "size", "sha256", "signature"} or null}.
        fn update_check(
            manifest: &[u8],
            keys: &str,
            current: &str,
            platform: &str,
            prerelease: bool,
            skipped: &str,
        ) -> Result<String>;
        /// Verifies the manifest again, then the downloaded file at `path`
        /// against its download for `platform`: signature, size, SHA-256.
        fn update_verify_download(
            manifest: &[u8],
            keys: &str,
            platform: &str,
            path: &str,
        ) -> Result<()>;
        /// The pre-release channel: the manifest's address from GitHub's
        /// list of releases (the newest that is not a draft).
        fn update_prerelease_manifest_url(releases: &[u8]) -> Result<String>;
        /// Whether `version` is newer than `than`; false when either is no
        /// version.
        fn update_is_newer(version: &str, than: &str) -> bool;
    }
}

fn update_check(
    manifest: &[u8],
    keys: &str,
    current: &str,
    platform: &str,
    prerelease: bool,
    skipped: &str,
) -> Result<String, UpdateError> {
    let channel = if prerelease {
        Channel::Prerelease
    } else {
        Channel::Stable
    };
    mitcad_update::check(manifest, keys, current, platform, channel, skipped)
}

fn update_verify_download(
    manifest: &[u8],
    keys: &str,
    platform: &str,
    path: &str,
) -> Result<(), UpdateError> {
    mitcad_update::verify_download(manifest, keys, platform, Path::new(path))
}

fn update_prerelease_manifest_url(releases: &[u8]) -> Result<String, UpdateError> {
    mitcad_update::prerelease_manifest_url(releases)
}

fn update_is_newer(version: &str, than: &str) -> bool {
    mitcad_update::is_newer(version, than).unwrap_or(false)
}
