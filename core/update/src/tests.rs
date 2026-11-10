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
        expires: None,
        notes: format!("https://example.invalid/releases/tag/v{version}"),
        assets: BTreeMap::from([("windows-x64".to_string(), asset)]),
        browser: Vec::new(),
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
            "its signature does not match MitCad's release key".into()
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

/// mitcad#938: a validly signed manifest whose `expires` has passed offers
/// nothing, though its version is newer; clocks a little ahead are
/// tolerated, clocks behind never expire it.
#[test]
fn an_expired_manifest_offers_nothing() {
    let release = key("release");
    let file = TempFile::new("expired.exe", INSTALLER);
    let mut expiring = manifest("0.2.0", &file, &release);
    expiring.expires = Some("2026-11-30".into());
    let signed = expiring.signed(&release);
    assert!(signed.contains("\"expires\": \"2026-11-30\""), "{signed}");
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(read.expires.as_deref(), Some("2026-11-30"));
    let day = |text: &str| parse_date(text).unwrap();

    // Before and on its last day, and within the clock tolerance after it.
    for today in ["2026-10-06", "2026-11-30", "2026-12-14"] {
        assert_eq!(
            assess_on(&read, "0.1.0", Channel::Stable, "", day(today)),
            Ok(Status::Available),
            "{today}"
        );
    }
    // A clock behind (a flat battery): never expired.
    assert_eq!(
        assess_on(&read, "0.1.0", Channel::Stable, "", day("1970-01-01")),
        Ok(Status::Available)
    );
    // Past the tolerance: nothing, though 0.2.0 is newer.
    assert_eq!(
        assess_on(&read, "0.1.0", Channel::Stable, "", day("2026-12-15")),
        Ok(Status::Current)
    );
    assert_eq!(
        assess_on(
            &read,
            "0.1.0",
            Channel::Prerelease,
            "0.2.0",
            day("2027-06-01")
        ),
        Ok(Status::Current)
    );
    // The application's check: current, and told why, with the date.
    let offer: serde_json::Value = serde_json::from_str(
        &check_on(
            signed.as_bytes(),
            &public_key(&release),
            "0.1.0",
            "windows-x64",
            Channel::Stable,
            "",
            day("2027-01-01"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(offer["status"], "current", "{offer}");
    assert_eq!(offer["expired"], true, "{offer}");
    assert_eq!(offer["expires"], "2026-11-30", "{offer}");
    assert_eq!(offer["version"], "0.2.0");
    // Not newer: plainly current, no expiry to tell.
    let offer: serde_json::Value = serde_json::from_str(
        &check_on(
            signed.as_bytes(),
            &public_key(&release),
            "0.2.0",
            "windows-x64",
            Channel::Stable,
            "",
            day("2027-01-01"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(offer["status"], "current", "{offer}");
    assert_eq!(offer["expired"], false, "{offer}");
    // By the real clock: a manifest that expired long ago.
    let mut old = manifest("0.2.0", &file, &release);
    old.date = "2020-01-06".into();
    old.expires = Some("2020-02-01".into());
    let read = Manifest::read(old.signed(&release).as_bytes(), &keys(&release)).unwrap();
    assert_eq!(
        assess(&read, "0.1.0", Channel::Stable, ""),
        Ok(Status::Current)
    );
    // A manifest without an expiry (before mitcad#938) never expires, and
    // its signed bytes have no `expires`.
    let unexpiring = manifest("0.2.0", &file, &release);
    let signed = unexpiring.signed(&release);
    assert!(!signed.contains("expires"), "{signed}");
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    assert_eq!(
        assess_on(&read, "0.1.0", Channel::Stable, "", day("2999-12-31")),
        Ok(Status::Available)
    );
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
    assert_eq!(offer["expires"], serde_json::Value::Null);
    assert_eq!(offer["expired"], false);
}

#[test]
fn expiry_dates_are_checked() {
    let release = key("release");
    let file = TempFile::new("expiry-dates.exe", INSTALLER);
    let refused = |expires: &str| {
        let mut wrong = manifest("0.2.0", &file, &release);
        wrong.expires = Some(expires.into());
        Manifest::read(wrong.signed(&release).as_bytes(), &keys(&release)).unwrap_err()
    };
    for bad in [
        "2026-02-30",
        "2026-13-01",
        "26-11-30",
        "2026-11-3x",
        "soon",
        "",
        "２026-11-30",
    ] {
        assert!(
            refused(bad).to_string().contains("invalid expiry date"),
            "{bad}"
        );
    }
    assert!(
        refused("2026-10-05")
            .to_string()
            .contains("before its release date 2026-10-06")
    );
    // The release day itself is a valid last day.
    let mut same_day = manifest("0.2.0", &file, &release);
    same_day.expires = Some("2026-10-06".into());
    Manifest::read(same_day.signed(&release).as_bytes(), &keys(&release)).unwrap();

    assert_eq!(parse_date("1970-01-01"), Some(0));
    assert_eq!(parse_date("2000-03-01"), Some(11_017));
    assert_eq!(
        parse_date("2024-02-29").map(date_text).as_deref(),
        Some("2024-02-29")
    );
    assert_eq!(parse_date("2100-02-29"), None);
    assert_eq!(
        parse_date("2000-02-29").map(date_text).as_deref(),
        Some("2000-02-29")
    );
    for days in [-1, 0, 59, 365, 11_016, 20_000, 20_755, 2_932_896] {
        assert_eq!(parse_date(&date_text(days)), Some(days), "{days}");
    }
    assert!(today() > parse_date("2026-01-01").unwrap());
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

// ---------------------------------------------------------------------------
// The web browser component's packages (mitcad#1579)

const BROWSER_ZIP: &[u8] = b"the browser component package, for the tests";
const CEF: &str = "156.0.27+ga10dae3+chromium-156.0.8078.88";

fn fingerprint(label: &str) -> String {
    to_hex(&Sha256::digest(label.as_bytes()))
}

fn browser_package(
    abi: u32,
    version: &str,
    file: &TempFile,
    signer: &SigningKey,
) -> BrowserPackage {
    sign_browser_package(
        "windows-x64",
        abi,
        version,
        CEF,
        &fingerprint("module and CEF"),
        &file.0,
        &format!("https://example.invalid/mitcad-browser-{version}-windows-x64.zip"),
        signer,
    )
    .unwrap()
}

fn browser_check_json(
    signed: &str,
    signer: &SigningKey,
    abi: u32,
    installed: &str,
) -> serde_json::Value {
    serde_json::from_str(
        &browser_check(
            signed.as_bytes(),
            &public_key(signer),
            "windows-x64",
            abi,
            installed,
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn a_browser_package_is_offered_for_its_interface_and_verifies() {
    let release = key("release");
    let installer = TempFile::new("browser-installer.exe", INSTALLER);
    let zip = TempFile::new("browser.zip", BROWSER_ZIP);
    let mut release_manifest = manifest("0.60.0", &installer, &release);
    release_manifest.browser = vec![
        browser_package(2, "2026.1110.900", &zip, &release),
        browser_package(2, "2026.1112.1530", &zip, &release),
        browser_package(1, "2026.1101.800", &zip, &release),
    ];
    let signed = release_manifest.signed(&release);
    let read = Manifest::read(signed.as_bytes(), &keys(&release)).unwrap();
    // The highest version of the interface asked for.
    assert_eq!(
        read.browser_for("windows-x64", 2).unwrap().version,
        "2026.1112.1530"
    );
    assert_eq!(
        read.browser_for("windows-x64", 1).unwrap().version,
        "2026.1101.800"
    );
    assert!(read.browser_for("linux-x64", 2).is_none());
    verify_browser_download(
        signed.as_bytes(),
        &public_key(&release),
        "windows-x64",
        2,
        "2026.1112.1530",
        &zip.0,
    )
    .unwrap();

    // None installed, an older one, the same one.
    let offer = browser_check_json(&signed, &release, 2, "");
    assert_eq!(offer["status"], "available");
    assert_eq!(offer["package"]["version"], "2026.1112.1530");
    assert_eq!(offer["package"]["cef"], CEF);
    assert_eq!(offer["package"]["size"], BROWSER_ZIP.len() as u64);
    assert_eq!(offer["abis"], serde_json::json!([1, 2]));
    assert_eq!(
        browser_check_json(&signed, &release, 2, "2026.1110.900")["status"],
        "available"
    );
    assert_eq!(
        browser_check_json(&signed, &release, 2, "2026.1112.1530")["status"],
        "current"
    );
    // A MitCad whose interface no package has: it needs an update itself.
    let other = browser_check_json(&signed, &release, 3, "");
    assert_eq!(other["status"], "abi");
    assert!(other["package"].is_null());
    // A manifest without packages for the platform, or none at all.
    let plain = manifest("0.60.0", &installer, &release).signed(&release);
    assert!(!plain.contains("\"browser\""));
    assert_eq!(
        browser_check_json(&plain, &release, 2, "")["status"],
        "none"
    );
}

#[test]
fn a_tampered_or_mismatching_browser_package_is_refused() {
    let release = key("release");
    let installer = TempFile::new("browser-installer2.exe", INSTALLER);
    let zip = TempFile::new("browser2.zip", BROWSER_ZIP);
    let mut release_manifest = manifest("0.60.0", &installer, &release);
    release_manifest.browser = vec![browser_package(2, "2026.1110.900", &zip, &release)];
    let signed = release_manifest.signed(&release);
    let public = public_key(&release);
    let verify = |abi: u32, version: &str, path: &PathBuf| {
        verify_browser_download(
            signed.as_bytes(),
            &public,
            "windows-x64",
            abi,
            version,
            path,
        )
    };
    // Another file, another interface, another version.
    let other = TempFile::new(
        "browser-other.zip",
        b"another package of the same size!!!!!!!!!!!!",
    );
    assert!(matches!(
        verify(2, "2026.1110.900", &other.0),
        Err(UpdateError::File(_))
    ));
    assert!(matches!(
        verify(3, "2026.1110.900", &zip.0),
        Err(UpdateError::File(_))
    ));
    assert!(matches!(
        verify(2, "2026.1110.901", &zip.0),
        Err(UpdateError::File(_))
    ));
    // Signed with another key than the release's.
    let mut forged = release_manifest.clone();
    forged.browser = vec![browser_package(
        2,
        "2026.1110.900",
        &zip,
        &key("someone else"),
    )];
    let forged = forged.signed(&release);
    assert!(matches!(
        verify_browser_download(
            forged.as_bytes(),
            &public,
            "windows-x64",
            2,
            "2026.1110.900",
            &zip.0
        ),
        Err(UpdateError::File(_))
    ));
    // The package's entry changed after signing the manifest.
    let changed = signed.replace("2026.1110.900", "2026.1110.999");
    assert!(matches!(
        Manifest::read(changed.as_bytes(), &keys(&release)),
        Err(UpdateError::ManifestSignature)
    ));
    // Signed, but invalid: no HTTPS, no fingerprint.
    let mut insecure = release_manifest.clone();
    insecure.browser[0].url = "http://example.invalid/b.zip".into();
    assert!(matches!(
        Manifest::read(insecure.signed(&release).as_bytes(), &keys(&release)),
        Err(UpdateError::Manifest(_))
    ));
    let mut unfingerprinted = release_manifest.clone();
    unfingerprinted.browser[0].fingerprint = String::new();
    assert!(matches!(
        Manifest::read(unfingerprinted.signed(&release).as_bytes(), &keys(&release)),
        Err(UpdateError::Manifest(_))
    ));
}

#[test]
fn a_manifest_with_browser_packages_reads_where_they_are_unknown() {
    // What a MitCad before mitcad#1579 reads of a manifest with packages:
    // the release as before (the field is ignored, the signature covers it).
    #[derive(Deserialize)]
    struct Before {
        version: String,
        assets: BTreeMap<String, Asset>,
    }
    let release = key("release");
    let installer = TempFile::new("browser-installer3.exe", INSTALLER);
    let zip = TempFile::new("browser3.zip", BROWSER_ZIP);
    let mut release_manifest = manifest("0.60.0", &installer, &release);
    release_manifest.browser = vec![browser_package(2, "2026.1110.900", &zip, &release)];
    let signed = release_manifest.signed(&release);
    let envelope: serde_json::Value = serde_json::from_str(&signed).unwrap();
    let before: Before = serde_json::from_value(envelope["manifest"].clone()).unwrap();
    assert_eq!(before.version, "0.60.0");
    assert_eq!(before.assets.len(), 1);
}
