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
    fs::write(&installer, b"an installer for the test").unwrap();
    fs::write(&appimage, b"an AppImage for the test").unwrap();

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
    assert_eq!(read.assets.len(), 2);
    verify_download(&data, &public, "windows-x64", &installer).unwrap();
    verify_download(&data, &public, "linux-x64", &appimage).unwrap();
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
