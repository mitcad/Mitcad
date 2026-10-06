// SPDX-License-Identifier: MIT
//! Manifests and downloads signed with keys made here, from seeds derived
//! from a label: valid ones, tampered manifests and downloads, the wrong
//! key, older versions and pre-releases on the stable channel.

use std::fs;
use std::path::PathBuf;

use super::*;

fn key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&Sha256::digest(label.as_bytes()).into())
}

fn keys(key: &SigningKey) -> TrustedKeys {
    TrustedKeys::parse(&public_key(key)).unwrap()
}

/// A file of the test's own, removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, content: &[u8]) -> TempFile {
        let path =
            std::env::temp_dir().join(format!("mitcad-update-{}-{name}", std::process::id()));
        fs::write(&path, content).unwrap();
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

const INSTALLER: &[u8] = b"the installer of Mitcad 0.2.0, for the tests";

fn manifest(version: &str, file: &TempFile, signer: &SigningKey) -> Manifest {
    let asset = sign_file(
        version,
        "windows-x64",
        &file.0,
        &format!("https://example.invalid/v{version}/mitcad-{version}-windows-x64.exe"),
        signer,
    )
    .unwrap();
    Manifest {
        format: MANIFEST_FORMAT,
        version: version.into(),
        date: "2026-10-06".into(),
        notes: format!("https://example.invalid/releases/tag/v{version}"),
        assets: BTreeMap::from([("windows-x64".to_string(), asset)]),
    }
}

#[test]
fn a_signed_manifest_and_its_download_verify() {
    let release = key("release");
    let file = TempFile::new("valid.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(read.version, "0.2.0");
    assert_eq!(read.assets["windows-x64"].size, INSTALLER.len() as u64);
    read.verify_file("windows-x64", &file.0, &keys(&release))
        .unwrap();
    verify_download(
        signed.as_bytes(),
        &public_key(&release),
        "windows-x64",
        &file.0,
    )
    .unwrap();

    let offer: serde_json::Value = serde_json::from_str(
        &check(
            signed.as_bytes(),
            &public_key(&release),
            "0.1.0",
            "windows-x64",
            Channel::Stable,
            "",
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(offer["status"], "available");
    assert_eq!(offer["version"], "0.2.0");
    assert_eq!(offer["date"], "2026-10-06");
    assert_eq!(offer["prerelease"], false);
    assert_eq!(offer["asset"]["size"], INSTALLER.len());
    // No download for the platform: announced only.
    let offer: serde_json::Value = serde_json::from_str(
        &check(
            signed.as_bytes(),
            &public_key(&release),
            "0.1.0",
            "linux-x64",
            Channel::Stable,
            "",
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(offer["status"], "available");
    assert!(offer["asset"].is_null());
}

#[test]
fn a_tampered_manifest_is_refused() {
    let release = key("release");
    let file = TempFile::new("tampered-manifest.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    let trusted = keys(&release);
    for (from, to) in [
        ("\"version\": \"0.2.0\"", "\"version\": \"0.3.0\""),
        ("example.invalid/v0.2.0", "example.invalid/v0.2.1"),
        ("\"size\": ", "\"size\": 1"),
    ] {
        assert!(signed.contains(from), "{from}");
        let tampered = signed.replacen(from, to, 1);
        assert_eq!(
            Manifest::read(tampered.as_bytes(), &trusted),
            Err(UpdateError::ManifestSignature),
            "{to}"
        );
    }
    // The signature covers the bytes: the same JSON written otherwise fails.
    let envelope: serde_json::Value = serde_json::from_str(&signed).unwrap();
    let compact = serde_json::to_string(&envelope).unwrap();
    assert_eq!(
        Manifest::read(compact.as_bytes(), &trusted),
        Err(UpdateError::ManifestSignature)
    );
    // A signature changed, or missing.
    let signature = envelope["signature"].as_str().unwrap();
    let flipped = format!(
        "{}{}",
        if signature.starts_with('0') { "1" } else { "0" },
        &signature[1..]
    );
    let tampered = signed.replace(signature, &flipped);
    assert_eq!(
        Manifest::read(tampered.as_bytes(), &trusted),
        Err(UpdateError::ManifestSignature)
    );
    assert!(matches!(
        Manifest::read(b"{\"manifest\": {}}", &trusted),
        Err(UpdateError::Manifest(_))
    ));
    assert!(matches!(
        Manifest::read(b"<html>not found</html>", &trusted),
        Err(UpdateError::Manifest(_))
    ));
}

#[test]
fn a_tampered_download_is_refused() {
    let release = key("release");
    let file = TempFile::new("tampered-download.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();

    let mut same_size = INSTALLER.to_vec();
    same_size[3] ^= 1;
    let changed = TempFile::new("changed.exe", &same_size);
    let error = read
        .verify_file("windows-x64", &changed.0, &keys(&release))
        .unwrap_err();
    assert_eq!(
        error,
        UpdateError::File("its SHA-256 differs from the release's".into())
    );
    assert!(
        error
            .to_string()
            .starts_with("the download is not the release's")
    );

    let longer = TempFile::new("longer.exe", &[INSTALLER, b"!"].concat());
    assert_eq!(
        read.verify_file("windows-x64", &longer.0, &keys(&release)),
        Err(UpdateError::File(format!(
            "it has {} bytes instead of {}",
            INSTALLER.len() + 1,
            INSTALLER.len()
        )))
    );
    assert!(matches!(
        read.verify_file("linux-x64", &file.0, &keys(&release)),
        Err(UpdateError::File(_))
    ));
    let missing = std::env::temp_dir().join("mitcad-update-no-such-file");
    assert!(matches!(
        read.verify_file("windows-x64", &missing, &keys(&release)),
        Err(UpdateError::Io(_))
    ));
}

#[test]
fn the_wrong_key_verifies_nothing() {
    let release = key("release");
    let other = key("someone else");
    let file = TempFile::new("wrong-key.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    assert_eq!(
        Manifest::read(signed.as_bytes(), &keys(&other)),
        Err(UpdateError::ManifestSignature)
    );
    // Signed by someone else altogether.
    let forged = manifest("0.2.0", &file, &other).signed(&other);
    assert_eq!(
        Manifest::read(forged.as_bytes(), &keys(&release)),
        Err(UpdateError::ManifestSignature)
    );
    // A manifest signed with the release key whose download someone else
    // signed.
    let mut mixed = manifest("0.2.0", &file, &other);
    let signed = mixed.clone().signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(
        read.verify_file("windows-x64", &file.0, &keys(&release)),
        Err(UpdateError::File(
            "its signature does not match Mitcad's release key".into()
        ))
    );
    // The manifest's signature cannot stand for a file's: their contexts
    // differ.
    let asset = mixed.assets.get_mut("windows-x64").unwrap();
    let mut message = MANIFEST_CONTEXT.to_vec();
    message.extend_from_slice(b"anything");
    asset.signature = sign(&release, &message);
    let read = Manifest::read(mixed.signed(&release).as_bytes(), &keys(&release)).unwrap();
    assert!(
        read.verify_file("windows-x64", &file.0, &keys(&release))
            .is_err()
    );
    // Either of two trusted keys does.
    let both =
        TrustedKeys::parse(&format!("{} {}", public_key(&other), public_key(&release))).unwrap();
    Manifest::read(signed.as_bytes(), &both).unwrap();
}

#[test]
fn without_a_release_key_every_manifest_is_refused() {
    let release = key("release");
    let file = TempFile::new("no-key.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    let none = TrustedKeys::parse("").unwrap();
    assert!(none.is_empty());
    assert_eq!(
        Manifest::read(signed.as_bytes(), &none),
        Err(UpdateError::NoReleaseKey)
    );
    assert_eq!(
        check(
            signed.as_bytes(),
            " \n",
            "0.1.0",
            "windows-x64",
            Channel::Stable,
            ""
        ),
        Err(UpdateError::NoReleaseKey)
    );
    assert!(matches!(
        TrustedKeys::parse("none"),
        Err(UpdateError::BadKey(_))
    ));
    assert!(matches!(
        TrustedKeys::parse(&"0".repeat(63)),
        Err(UpdateError::BadKey(_))
    ));
}

#[test]
fn older_and_equal_versions_are_not_offered() {
    let release = key("release");
    let file = TempFile::new("older.exe", INSTALLER);
    let signed = manifest("0.2.0", &file, &release).signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(
        assess(&read, "0.1.9", Channel::Stable, ""),
        Ok(Status::Available)
    );
    assert_eq!(
        assess(&read, "0.2.0", Channel::Stable, ""),
        Ok(Status::Current)
    );
    assert_eq!(
        assess(&read, "0.2.1", Channel::Stable, ""),
        Ok(Status::Current)
    );
    assert_eq!(
        assess(&read, "1.0.0", Channel::Prerelease, ""),
        Ok(Status::Current)
    );
    // The release of a pre-release that came before it is newer.
    assert_eq!(
        assess(&read, "0.2.0-rc.1", Channel::Stable, ""),
        Ok(Status::Available)
    );
    // Build metadata does not make a version newer.
    assert_eq!(
        assess(&read, "0.2.0+local", Channel::Stable, ""),
        Ok(Status::Current)
    );
    assert!(matches!(
        assess(&read, "main", Channel::Stable, ""),
        Err(UpdateError::Version(_))
    ));
}

#[test]
fn pre_releases_are_offered_on_their_channel_only() {
    let release = key("release");
    let file = TempFile::new("beta.exe", INSTALLER);
    let signed = manifest("0.3.0-beta.1", &file, &release).signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(
        assess(&read, "0.2.0", Channel::Stable, ""),
        Ok(Status::Current)
    );
    assert_eq!(
        assess(&read, "0.2.0", Channel::Prerelease, ""),
        Ok(Status::Available)
    );
    assert_eq!(
        assess(&read, "0.3.0-beta.2", Channel::Prerelease, ""),
        Ok(Status::Current)
    );
    let offer: serde_json::Value = serde_json::from_str(
        &check(
            signed.as_bytes(),
            &public_key(&release),
            "0.2.0",
            "windows-x64",
            Channel::Prerelease,
            "",
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(offer["prerelease"], true);
    read.verify_file("windows-x64", &file.0, &keys(&release))
        .unwrap();
}

#[test]
fn a_skipped_version_is_told_apart() {
    let release = key("release");
    let file = TempFile::new("skipped.exe", INSTALLER);
    let read = Manifest::read(
        manifest("0.2.0", &file, &release)
            .signed(&release)
            .as_bytes(),
        &keys(&release),
    )
    .unwrap();
    assert_eq!(
        assess(&read, "0.1.0", Channel::Stable, "0.2.0"),
        Ok(Status::Skipped)
    );
    assert_eq!(
        assess(&read, "0.1.0", Channel::Stable, "v0.2.0"),
        Ok(Status::Skipped)
    );
    // A version newer than the skipped one is offered.
    assert_eq!(
        assess(&read, "0.1.0", Channel::Stable, "0.1.5"),
        Ok(Status::Available)
    );
    assert_eq!(
        assess(&read, "0.1.0", Channel::Stable, "junk"),
        Ok(Status::Available)
    );
}

#[test]
fn invalid_manifests_are_refused_even_when_signed() {
    let release = key("release");
    let file = TempFile::new("invalid.exe", INSTALLER);
    let good = manifest("0.2.0", &file, &release);
    let refused = |manifest: &Manifest| {
        Manifest::read(manifest.signed(&release).as_bytes(), &keys(&release)).unwrap_err()
    };
    let mut wrong = good.clone();
    wrong.format = 2;
    assert!(refused(&wrong).to_string().contains("format 2"));
    let mut wrong = good.clone();
    wrong.version = "next".into();
    assert_eq!(refused(&wrong), UpdateError::Version("next".into()));
    let mut wrong = good.clone();
    wrong.notes = "http://example.invalid/notes".into();
    assert!(refused(&wrong).to_string().contains("HTTPS"));
    let mut wrong = good.clone();
    wrong.assets.get_mut("windows-x64").unwrap().url = "http://example.invalid/setup.exe".into();
    assert!(
        refused(&wrong)
            .to_string()
            .contains("HTTPS address for windows-x64")
    );
    let mut wrong = good.clone();
    wrong.assets.get_mut("windows-x64").unwrap().size = MAX_ASSET_SIZE + 1;
    assert!(refused(&wrong).to_string().contains("size"));
    let mut wrong = good.clone();
    wrong.assets.get_mut("windows-x64").unwrap().sha256 = "ABC".into();
    assert!(refused(&wrong).to_string().contains("digest"));
    let mut wrong = good;
    let asset = wrong.assets.remove("windows-x64").unwrap();
    wrong.assets.insert("Windows x64".into(), asset);
    assert!(refused(&wrong).to_string().contains("platform"));
}

#[test]
fn the_prerelease_channel_takes_the_newest_release() {
    let releases = br#"[
      {"tag_name": "v0.2.0", "draft": false, "prerelease": false,
       "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "https://example.invalid/v0.2.0/update-manifest.json"},
                  {"name": "mitcad-0.2.0-windows-x64.exe",
                   "browser_download_url": "https://example.invalid/v0.2.0/setup.exe"}]},
      {"tag_name": "v0.4.0", "draft": true,
       "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "https://example.invalid/v0.4.0/update-manifest.json"}]},
      {"tag_name": "v0.3.0-beta.2", "prerelease": true,
       "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "https://example.invalid/v0.3.0-beta.2/update-manifest.json"}]},
      {"tag_name": "v0.3.0-beta.10", "prerelease": true,
       "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "https://example.invalid/v0.3.0-beta.10/update-manifest.json"}]},
      {"tag_name": "v0.3.1", "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "http://example.invalid/v0.3.1/update-manifest.json"}]},
      {"tag_name": "nightly", "assets": [{"name": "update-manifest.json",
                   "browser_download_url": "https://example.invalid/nightly/update-manifest.json"}]},
      {"tag_name": "v9.0.0", "assets": []}
    ]"#;
    assert_eq!(
        prerelease_manifest_url(releases).unwrap(),
        "https://example.invalid/v0.3.0-beta.10/update-manifest.json"
    );
    assert!(prerelease_manifest_url(b"[]").is_err());
    assert!(prerelease_manifest_url(b"{\"message\": \"API rate limit exceeded\"}").is_err());
}

#[test]
fn versions_and_keys_parse() {
    assert!(is_newer("v0.2.0", "0.1.9").unwrap());
    assert!(!is_newer("0.2.0", "0.2.0").unwrap());
    assert!(is_newer("0.2.0", "0.2.0-beta.1").unwrap());
    assert!(is_newer("x", "0.1.0").is_err());
    assert_eq!(parse_version(" v1.2.3 ").unwrap(), Version::new(1, 2, 3));
    let release = key("release");
    let public = public_key(&release);
    assert_eq!(public.len(), 64);
    let seed = to_hex(&release.to_bytes());
    assert_eq!(
        public_key(&signing_key(&format!("{seed}\n")).unwrap()),
        public
    );
    assert_eq!(
        public_key(&signing_key(&seed.to_uppercase()).unwrap()),
        public
    );
    assert!(signing_key("1234").is_err());
    assert_eq!(
        TrustedKeys::parse(&format!(" {public},\n{public} "))
            .unwrap()
            .0
            .len(),
        2
    );
    assert_eq!(from_hex::<2>("0aFf"), Some([0x0a, 0xff]));
    assert_eq!(from_hex::<2>("0aF"), None);
    assert_eq!(from_hex::<2>("0aFg"), None);
}
