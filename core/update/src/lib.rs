// SPDX-License-Identifier: MIT
//! Mitcad's automatic updates (mitcad#9, `docs/updates.md`): the release
//! manifest signed with the release's Ed25519 key, which release a running
//! version is offered, the checks of a downloaded file, and the signing for
//! the release tool `mitcad-release`. The network is the application's (Qt
//! Network); nothing here opens a connection.
//!
//! The manifest, `update-manifest.json`, is a release asset next to the
//! installers:
//!
//! ```json
//! {
//! "manifest": {
//!   "format": 1,
//!   "version": "0.2.0",
//!   "date": "2026-10-06",
//!   "notes": "https://github.com/mitcad/Mitcad/releases/tag/v0.2.0",
//!   "assets": {
//!     "windows-x64": {
//!       "url": "https://github.com/mitcad/Mitcad/releases/download/v0.2.0/mitcad-0.2.0-windows-x64.exe",
//!       "size": 123456789,
//!       "sha256": "<64 hex digits>",
//!       "signature": "<128 hex digits>"
//!     }
//!   }
//! },
//! "signature": "<128 hex digits>"
//! }
//! ```
//!
//! The outer `signature` signs [`MANIFEST_CONTEXT`] followed by the exact
//! bytes of the `manifest` value as the file has them, so no canonical form
//! of JSON is needed; an asset's `signature` signs its [`file_statement`]:
//! the version, the platform, and the size and SHA-256 of the file. Keys,
//! signatures and digests are lowercase hex; a private key is the 32-byte
//! Ed25519 seed. The platforms are `windows-x64` (the NSIS installer) and
//! `linux-x64` (the AppImage); a manifest may list any or none, and a
//! version without a download for the platform is only announced.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};

/// The manifest format this version reads and writes.
pub const MANIFEST_FORMAT: u32 = 1;
/// What the manifest's signature signs before its bytes, so that it can
/// never pass for a file's signature or the other way round.
pub const MANIFEST_CONTEXT: &[u8] = b"mitcad-update-manifest\n";
/// The first line of a file statement.
const FILE_CONTEXT: &str = "mitcad-update-file";
/// The manifest's name among a release's assets.
pub const MANIFEST_ASSET: &str = "update-manifest.json";
/// The largest download a manifest may describe (4 GiB).
pub const MAX_ASSET_SIZE: u64 = 4 << 30;

/// Why an update was not offered or a download was refused. The texts are
/// for the user ("The update check failed: ...").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// The build has no release key (the placeholder), so nothing can be
    /// verified and every manifest is refused.
    NoReleaseKey,
    /// A key is not an Ed25519 key in 64 hex digits.
    BadKey(String),
    /// The manifest is not signed with a trusted key, or was changed.
    ManifestSignature,
    /// The manifest, or GitHub's list of releases, is not one this version
    /// reads.
    Manifest(String),
    /// A version is not a semantic version.
    Version(String),
    /// A downloaded file is not the release's.
    File(String),
    /// Reading a file failed.
    Io(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::NoReleaseKey => write!(
                f,
                "this build of Mitcad has no release key, so it cannot verify updates"
            ),
            UpdateError::BadKey(why) => write!(f, "invalid key: {why}"),
            UpdateError::ManifestSignature => write!(
                f,
                "the update manifest's signature does not match Mitcad's release key"
            ),
            UpdateError::Manifest(why) => write!(f, "{why}"),
            UpdateError::Version(text) => write!(f, "'{text}' is not a version"),
            UpdateError::File(why) => write!(f, "the download is not the release's: {why}"),
            UpdateError::Io(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for UpdateError {}

pub type Result<T> = std::result::Result<T, UpdateError>;

// ---------------------------------------------------------------------------
// Hex

pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 15)] as char);
    }
    text
}

/// `N` bytes from exactly `2 * N` hex digits (either case).
fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digits = text.as_bytes();
    if digits.len() != 2 * N {
        return None;
    }
    let value = |digit: u8| (digit as char).to_digit(16).map(|v| v as u8);
    let mut bytes = [0u8; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = (value(digits[2 * i])? << 4) | value(digits[2 * i + 1])?;
    }
    Some(bytes)
}

fn is_lower_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ---------------------------------------------------------------------------
// Keys

/// The public keys an update is checked with: the release key built into
/// the application, and in tests a key of their own.
#[derive(Clone, Debug, Default)]
pub struct TrustedKeys(Vec<VerifyingKey>);

impl TrustedKeys {
    /// Keys in 64 hex digits, separated by white space or commas; none is
    /// an empty set, which verifies nothing.
    pub fn parse(text: &str) -> Result<Self> {
        let mut keys = Vec::new();
        for word in text.split(|c: char| c.is_whitespace() || c == ',') {
            if word.is_empty() {
                continue;
            }
            let bytes = from_hex::<32>(word)
                .ok_or_else(|| UpdateError::BadKey(format!("'{word}' is not 64 hex digits")))?;
            let key = VerifyingKey::from_bytes(&bytes)
                .map_err(|_| UpdateError::BadKey(format!("'{word}' is not an Ed25519 key")))?;
            keys.push(key);
        }
        Ok(TrustedKeys(keys))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether one of the keys made `signature` (128 hex digits) of
    /// `message`. Strict verification: no malleable or weak-key signatures.
    fn verifies(&self, message: &[u8], signature: &str) -> bool {
        let Some(bytes) = from_hex::<64>(signature) else {
            return false;
        };
        let signature = Signature::from_bytes(&bytes);
        self.0
            .iter()
            .any(|key| key.verify_strict(message, &signature).is_ok())
    }
}

/// A private release key from its 64 hex digits (the Ed25519 seed, for
/// example from `openssl rand -hex 32`).
pub fn signing_key(text: &str) -> Result<SigningKey> {
    from_hex::<32>(text.trim())
        .map(|seed| SigningKey::from_bytes(&seed))
        .ok_or_else(|| UpdateError::BadKey("a private key is 64 hex digits".into()))
}

/// The public key of a private one, in 64 hex digits.
pub fn public_key(key: &SigningKey) -> String {
    to_hex(key.verifying_key().as_bytes())
}

fn sign(key: &SigningKey, message: &[u8]) -> String {
    to_hex(&key.sign(message).to_bytes())
}

// ---------------------------------------------------------------------------
// Versions

/// A semantic version; a leading `v` (a release tag's) is ignored.
pub fn parse_version(text: &str) -> Result<Version> {
    let trimmed = text.trim();
    Version::parse(trimmed.strip_prefix('v').unwrap_or(trimmed))
        .map_err(|_| UpdateError::Version(text.to_string()))
}

/// Whether `version` is newer than `than` (semantic version precedence:
/// build metadata does not count).
pub fn is_newer(version: &str, than: &str) -> Result<bool> {
    Ok(parse_version(version)?.cmp_precedence(&parse_version(than)?) == Ordering::Greater)
}

// ---------------------------------------------------------------------------
// The manifest

/// A download of a release for one platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// HTTPS only.
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Of the [`file_statement`].
    pub signature: String,
}

/// What a release offers: its version, date and notes, and its downloads
/// by platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub version: String,
    /// The release date, `YYYY-MM-DD`.
    pub date: String,
    /// The release notes, the release's page (HTTPS).
    pub notes: String,
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    manifest: &'a RawValue,
    signature: String,
}

/// What a file's signature signs: the version, the platform, and the
/// file's size and SHA-256.
pub fn file_statement(version: &str, platform: &str, size: u64, sha256: &str) -> String {
    format!(
        "{FILE_CONTEXT}\nversion {version}\nplatform {platform}\nsize {size}\nsha256 {sha256}\n"
    )
}

/// The size and SHA-256 (hex) of a file, read in pieces.
pub fn digest_file(path: &Path) -> Result<(u64, String)> {
    let failed =
        |e: std::io::Error| UpdateError::Io(format!("cannot read {}: {e}", path.display()));
    let mut file = File::open(path).map_err(failed)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(failed)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, to_hex(&hasher.finalize())))
}

fn https(url: &str) -> bool {
    url.len() > "https://".len()
        && url
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
}

impl Manifest {
    /// Reads a manifest file and checks its signature with `keys` before
    /// anything else of it; then that it is a manifest this version reads.
    pub fn read(data: &[u8], keys: &TrustedKeys) -> Result<Manifest> {
        if keys.is_empty() {
            return Err(UpdateError::NoReleaseKey);
        }
        let envelope: Envelope<'_> = serde_json::from_slice(data)
            .map_err(|e| UpdateError::Manifest(format!("not an update manifest ({e})")))?;
        let mut message = MANIFEST_CONTEXT.to_vec();
        message.extend_from_slice(envelope.manifest.get().as_bytes());
        if !keys.verifies(&message, &envelope.signature) {
            return Err(UpdateError::ManifestSignature);
        }
        let manifest: Manifest = serde_json::from_str(envelope.manifest.get())
            .map_err(|e| UpdateError::Manifest(format!("the update manifest is invalid ({e})")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        let invalid =
            |why: String| Err(UpdateError::Manifest(format!("the update manifest {why}")));
        if self.format != MANIFEST_FORMAT {
            return invalid(format!(
                "has format {}, which this Mitcad does not read",
                self.format
            ));
        }
        parse_version(&self.version)?;
        if !https(&self.notes) {
            return invalid("has no HTTPS address for the release notes".into());
        }
        for (platform, asset) in &self.assets {
            if platform.is_empty()
                || !platform
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return invalid(format!("names an invalid platform '{platform}'"));
            }
            if !https(&asset.url) {
                return invalid(format!("has no HTTPS address for {platform}"));
            }
            if asset.size == 0 || asset.size > MAX_ASSET_SIZE {
                return invalid(format!("gives {platform} a size of {} bytes", asset.size));
            }
            if !is_lower_hex(&asset.sha256, 64) || !is_lower_hex(&asset.signature, 128) {
                return invalid(format!("has an invalid digest or signature for {platform}"));
            }
        }
        Ok(())
    }

    /// The release's version (checked when read).
    pub fn release_version(&self) -> Version {
        parse_version(&self.version).unwrap_or_else(|_| Version::new(0, 0, 0))
    }

    /// The manifest file: this manifest signed with `key`.
    pub fn signed(&self, key: &SigningKey) -> String {
        let text = serde_json::to_string_pretty(self).expect("a manifest is JSON");
        let mut message = MANIFEST_CONTEXT.to_vec();
        message.extend_from_slice(text.as_bytes());
        let signature = sign(key, &message);
        format!("{{\n\"manifest\": {text},\n\"signature\": \"{signature}\"\n}}\n")
    }

    /// Checks a downloaded file against the release's download for
    /// `platform`: the signature of what the manifest says of it, then its
    /// size and SHA-256.
    pub fn verify_file(&self, platform: &str, path: &Path, keys: &TrustedKeys) -> Result<()> {
        if keys.is_empty() {
            return Err(UpdateError::NoReleaseKey);
        }
        let asset = self.assets.get(platform).ok_or_else(|| {
            UpdateError::File(format!("the release has no download for {platform}"))
        })?;
        let statement = file_statement(&self.version, platform, asset.size, &asset.sha256);
        if !keys.verifies(statement.as_bytes(), &asset.signature) {
            return Err(UpdateError::File(
                "its signature does not match Mitcad's release key".into(),
            ));
        }
        let (size, sha256) = digest_file(path)?;
        if size != asset.size {
            return Err(UpdateError::File(format!(
                "it has {size} bytes instead of {}",
                asset.size
            )));
        }
        if sha256 != asset.sha256 {
            return Err(UpdateError::File(
                "its SHA-256 differs from the release's".into(),
            ));
        }
        Ok(())
    }
}

/// A release's download for `platform`, signed with `key`: the file's size
/// and SHA-256, and the signature of its statement.
pub fn sign_file(
    version: &str,
    platform: &str,
    path: &Path,
    url: &str,
    key: &SigningKey,
) -> Result<Asset> {
    let (size, sha256) = digest_file(path)?;
    let signature = sign(
        key,
        file_statement(version, platform, size, &sha256).as_bytes(),
    );
    Ok(Asset {
        url: url.to_string(),
        size,
        sha256,
        signature,
    })
}

// ---------------------------------------------------------------------------
// Offers

/// Which releases a user is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Releases only (GitHub's latest release, which is never a
    /// pre-release); a pre-release version in a manifest is ignored.
    Stable,
    /// Pre-releases too: the newest release of GitHub's list.
    Prerelease,
}

/// What a manifest means for the running version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// A newer version.
    Available,
    /// A newer version the user chose to skip.
    Skipped,
    /// Nothing newer on the channel: never a downgrade.
    Current,
}

/// What `manifest` offers version `current` on `channel`; `skipped` is the
/// version the user skipped, or empty.
pub fn assess(
    manifest: &Manifest,
    current: &str,
    channel: Channel,
    skipped: &str,
) -> Result<Status> {
    let current = parse_version(current)?;
    let offered = manifest.release_version();
    if channel == Channel::Stable && !offered.pre.is_empty() {
        return Ok(Status::Current);
    }
    if offered.cmp_precedence(&current) != Ordering::Greater {
        return Ok(Status::Current);
    }
    if parse_version(skipped).is_ok_and(|skip| skip.cmp_precedence(&offered) == Ordering::Equal) {
        return Ok(Status::Skipped);
    }
    Ok(Status::Available)
}

#[derive(Serialize)]
struct Offer<'a> {
    status: Status,
    version: &'a str,
    date: &'a str,
    notes: &'a str,
    prerelease: bool,
    /// The download for the platform, or null: announce only.
    asset: Option<&'a Asset>,
}

/// The application's check (`update_check` of the C++ bridge): reads and
/// verifies a manifest file with the keys (text, as [`TrustedKeys::parse`]
/// takes) and tells what it offers the running version, as JSON
/// `{"status": "available" | "skipped" | "current", "version", "date",
/// "notes", "prerelease", "asset": {"url", "size", "sha256", "signature"}
/// or null}`.
pub fn check(
    manifest: &[u8],
    keys: &str,
    current: &str,
    platform: &str,
    channel: Channel,
    skipped: &str,
) -> Result<String> {
    let manifest = Manifest::read(manifest, &TrustedKeys::parse(keys)?)?;
    let status = assess(&manifest, current, channel, skipped)?;
    let offer = Offer {
        status,
        version: &manifest.version,
        date: &manifest.date,
        notes: &manifest.notes,
        prerelease: !manifest.release_version().pre.is_empty(),
        asset: manifest.assets.get(platform),
    };
    Ok(serde_json::to_string(&offer).expect("an offer is JSON"))
}

/// The application's check of a download (`update_verify_download`): the
/// manifest read and verified again, then the file against its download
/// for `platform`.
pub fn verify_download(manifest: &[u8], keys: &str, platform: &str, path: &Path) -> Result<()> {
    let keys = TrustedKeys::parse(keys)?;
    Manifest::read(manifest, &keys)?.verify_file(platform, path, &keys)
}

/// The pre-release channel's manifest: from GitHub's list of releases
/// (`GET /repos/{owner}/{repo}/releases`), the `update-manifest.json` of
/// the release with the highest version that is not a draft. Its address
/// only; the manifest itself is what is verified.
pub fn prerelease_manifest_url(releases: &[u8]) -> Result<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        assets: Vec<ReleaseAsset>,
    }
    #[derive(Deserialize)]
    struct ReleaseAsset {
        name: String,
        browser_download_url: String,
    }
    let releases: Vec<Release> = serde_json::from_slice(releases).map_err(|e| {
        UpdateError::Manifest(format!("the list of releases is not GitHub's ({e})"))
    })?;
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = parse_version(&release.tag_name).ok()?;
            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == MANIFEST_ASSET && https(&asset.browser_download_url))?;
            Some((version, &asset.browser_download_url))
        })
        .max_by(|a, b| a.0.cmp_precedence(&b.0))
        .map(|(_, url)| url.clone())
        .ok_or_else(|| UpdateError::Manifest("no release has an update manifest".into()))
}

#[cfg(test)]
mod tests;
