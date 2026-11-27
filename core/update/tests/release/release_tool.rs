// SPDX-License-Identifier: MIT
//! The release tool signs downloads and writes a manifest that the
//! application's checks accept; the key comes from the environment only.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use mitcad_update::{Channel, Manifest, TrustedKeys, check, to_hex, verify_download};

fn tool() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mitcad-release"));
    command
        .env_remove("MITCAD_RELEASE_KEY")
        .env_remove("MITCAD_RELEASE_KEY_FILE");
    command
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn work_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A key made for this test: 32 bytes of the time and the process, hex.
fn test_seed() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut seed = [0u8; 32];
    seed[..16].copy_from_slice(&nanos.to_le_bytes());
    seed[16..20].copy_from_slice(&std::process::id().to_le_bytes());
    seed[31] = 0x5a;
    to_hex(&seed)
}

#[test]
fn signs_a_release_the_application_accepts() {
    let dir = work_dir("release-tool");
    let seed = test_seed();
    let installer = dir.join("mitcad-0.2.0-windows-x64.exe");
    let appimage = dir.join("Mitcad-0.2.0-x86_64.AppImage");
    let dmg = dir.join("Mitcad-0.2.0-macos-arm64.dmg");
    fs::write(&installer, b"an installer for the test").unwrap();
    fs::write(&appimage, b"an AppImage for the test").unwrap();
    fs::write(&dmg, b"a macOS disk image for the test").unwrap();

    let output = tool()
        .arg("public-key")
        .env("MITCAD_RELEASE_KEY", &seed)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let public = String::from_utf8(output.stdout).unwrap().trim().to_string();
    assert_eq!(public.len(), 64);
    // The key from a file gives the same public key.
    let key_file = dir.join("release.key");
    fs::write(&key_file, format!("{seed}\n")).unwrap();
    let output = tool()
        .arg("public-key")
        .env("MITCAD_RELEASE_KEY_FILE", &key_file)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), public);

    let manifest = dir.join("update-manifest.json");
    let output = tool()
        .args(["manifest", "--version", "v0.2.0", "--notes"])
        .arg("https://example.invalid/releases/tag/v0.2.0")
        .args(["--asset", "windows-x64"])
        .arg(&installer)
        .arg("https://example.invalid/v0.2.0/mitcad-0.2.0-windows-x64.exe")
        .args(["--asset", "linux-x64"])
        .arg(&appimage)
        .arg("https://example.invalid/v0.2.0/Mitcad-0.2.0-x86_64.AppImage")
        .args(["--asset", "macos-arm64"])
        .arg(&dmg)
        .arg("https://example.invalid/v0.2.0/Mitcad-0.2.0-macos-arm64.dmg")
        .arg("--out")
        .arg(&manifest)
        .env("MITCAD_RELEASE_KEY_FILE", &key_file)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains(&public));
    let data = fs::read(&manifest).unwrap();
    let read = Manifest::read(&data, &TrustedKeys::parse(&public).unwrap()).unwrap();
    assert_eq!(read.version, "0.2.0");
    assert_eq!(read.date.len(), 10);
    assert_eq!(read.assets.len(), 3);
    verify_download(&data, &public, "windows-x64", &installer).unwrap();
    verify_download(&data, &public, "linux-x64", &appimage).unwrap();
    verify_download(&data, &public, "macos-arm64", &dmg).unwrap();
    assert!(verify_download(&data, &public, "macos-arm64", &appimage).is_err());
    let offer: serde_json::Value = serde_json::from_str(
        &check(&data, &public, "0.1.0", "macos-arm64", Channel::Stable, "").unwrap(),
    )
    .unwrap();
    assert_eq!(offer["status"], "available");
    assert_eq!(offer["asset"]["url"], read.assets["macos-arm64"].url);
    // An asset's signature is bound to its platform, even in a valid manifest.
    let mut wrong_platform = read.clone();
    wrong_platform
        .assets
        .insert("macos-arm64".into(), read.assets["linux-x64"].clone());
    let seed_key = mitcad_update::signing_key(&seed).unwrap();
    assert!(
        verify_download(
            wrong_platform.signed(&seed_key).as_bytes(),
            &public,
            "macos-arm64",
            &appimage
        )
        .is_err()
    );
    assert!(verify_download(&data, &public, "linux-x64", &installer).is_err());
    assert!(
        check(&data, &public, "0.1.0", "linux-x64", Channel::Stable, "")
            .unwrap()
            .contains("\"status\":\"available\"")
    );

    let output = tool()
        .args(["verify", "--manifest"])
        .arg(&manifest)
        .args(["--key", &public, "--asset", "windows-x64"])
        .arg(&installer)
        .args(["--current", "0.1.0"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("Available"));
    fs::write(&installer, b"an installer for the test, changed").unwrap();
    let output = tool()
        .args(["verify", "--manifest"])
        .arg(&manifest)
        .args(["--key", &public, "--asset", "windows-x64"])
        .arg(&installer)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        text(&output).contains("bytes instead of"),
        "{}",
        text(&output)
    );
}

/// mitcad#938: a manifest expires 90 days after it is signed unless
/// `--expires` says otherwise, and `renew` signs the latest release's
/// manifest again with a new expiry, its downloads as they were.
#[test]
fn manifests_expire_and_are_renewed() {
    let dir = work_dir("release-tool-expiry");
    let seed = test_seed();
    let installer = dir.join("mitcad-0.2.0-windows-x64.exe");
    fs::write(&installer, b"an installer for the expiry test").unwrap();
    let public = {
        let output = tool()
            .arg("public-key")
            .env("MITCAD_RELEASE_KEY", &seed)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    let keys = TrustedKeys::parse(&public).unwrap();
    let sign = |extra: &[&str], out: &PathBuf| {
        tool()
            .args(["manifest", "--version", "0.2.0", "--notes"])
            .arg("https://example.invalid/releases/tag/v0.2.0")
            .args(extra)
            .args(["--asset", "windows-x64"])
            .arg(&installer)
            .arg("https://example.invalid/v0.2.0/mitcad-0.2.0-windows-x64.exe")
            .arg("--out")
            .arg(out)
            .env("MITCAD_RELEASE_KEY", &seed)
            .output()
            .unwrap()
    };

    let manifest = dir.join("update-manifest.json");
    let output = sign(&[], &manifest);
    assert!(output.status.success(), "{}", text(&output));
    let read = Manifest::read(&fs::read(&manifest).unwrap(), &keys).unwrap();
    let today = mitcad_update::today();
    assert_eq!(
        read.expires.as_deref().and_then(mitcad_update::parse_date),
        Some(today + mitcad_update::MANIFEST_VALIDITY_DAYS),
        "{}",
        text(&output)
    );
    assert_eq!(read.date, mitcad_update::date_text(today));

    // An expiry given; a malformed one is a usage error.
    let output = sign(&["--expires", "2099-12-31"], &manifest);
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        text(&output).contains("expires 2099-12-31"),
        "{}",
        text(&output)
    );
    let output = sign(&["--expires", "next year"], &dir.join("bad.json"));
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(!dir.join("bad.json").exists());

    // Renewed: the same release and downloads, a new expiry, signed again.
    let renewed = dir.join("renewed.json");
    let output = tool()
        .args(["renew", "--manifest"])
        .arg(&manifest)
        .args(["--expires", "2100-01-31", "--out"])
        .arg(&renewed)
        .env("MITCAD_RELEASE_KEY", &seed)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let before = Manifest::read(&fs::read(&manifest).unwrap(), &keys).unwrap();
    let after = Manifest::read(&fs::read(&renewed).unwrap(), &keys).unwrap();
    assert_eq!(after.expires.as_deref(), Some("2100-01-31"));
    assert_eq!(
        (&after.version, &after.date, &after.notes, &after.assets),
        (&before.version, &before.date, &before.notes, &before.assets)
    );
    verify_download(
        &fs::read(&renewed).unwrap(),
        &public,
        "windows-x64",
        &installer,
    )
    .unwrap();

    // Only a manifest of the release key is renewed.
    let output = tool()
        .args(["renew", "--manifest"])
        .arg(&manifest)
        .arg("--out")
        .arg(dir.join("foreign.json"))
        .env("MITCAD_RELEASE_KEY", "11".repeat(32))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(text(&output).contains("signature"), "{}", text(&output));
    assert!(!dir.join("foreign.json").exists());
}

#[test]
fn needs_a_key_from_the_environment() {
    let dir = work_dir("release-tool-no-key");
    let output = tool()
        .args([
            "manifest",
            "--version",
            "0.2.0",
            "--notes",
            "https://example.invalid/notes",
            "--out",
        ])
        .arg(dir.join("update-manifest.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output).contains("MITCAD_RELEASE_KEY"),
        "{}",
        text(&output)
    );
    assert!(!dir.join("update-manifest.json").exists());
    let output = tool()
        .arg("public-key")
        .env("MITCAD_RELEASE_KEY", "1234")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let output = tool()
        .args(["manifest", "--key", "secret"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let output = tool().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
