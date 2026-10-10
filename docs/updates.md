# Automatic updates

Mitcad checks for newer releases and, where it can replace itself,
installs one with the user's consent. Code: [core/update](../core/update/src/lib.rs)
(manifest, verification, versions, `mitcad-release`; `ed25519-dalek`,
`sha2`, `semver`), the bridge `core/ffi/src/update.rs`, and `app/update/`
(requests with Qt Network, so TLS is Qt's backend: Schannel on Windows,
the system's OpenSSL on Linux).

## Behaviour

- **Check:** at start-up at most once a day (3 s after the window opens),
  and from *Help › Check for Updates*. A silent automatic check reports
  nothing on failure or when up to date; the manual one always reports.
- **Notice:** a non-blocking bar under the toolbar with *Release Notes*,
  *Install*, *Skip This Version*, *Later*. A skipped version is not
  announced again automatically.
- **Install, Windows (NSIS installation):** the installer is downloaded to
  `%TEMP%\mitcad-update` and verified; the window closes (asking to save);
  after Mitcad has quit (a `QCoreApplication` post routine)
  `mitcad-updater.exe`, copied next to the download because the installer
  replaces `bin`, runs it silently (`/S /D=<install dir>`, UAC prompts as
  usual) and restarts Mitcad.
- **Install, AppImage in a writable folder:** downloaded next to the
  running file as a hidden `.<name>.update`, verified, made executable,
  and after quit renamed atomically over the old file and started.
- **Announce only:** the portable `.zip`, development builds, system
  packages and read-only installations.
- A download whose signature, SHA-256 or size does not match is deleted
  and reported, never run; a manifest with a bad signature is refused.
- After an update the new Mitcad reports the result (from the settings
  below).

## Settings and turning checks off

*Tools › Preferences › Updates*: **Check for updates at start-up, once a
day** (on by default) and the **channel**: *Releases* (stable, default)
or *Pre-releases too*. The settings (`QSettings`) are
`updates/automatic`, `updates/channel` (`stable`, `prerelease`),
`updates/skipped` (the version *Skip This Version* chose),
`updates/lastCheck` (UTC) and `updates/hintShown`; during an update
`updates/installing`, `updates/installingFrom` and `updates/installLog`,
and afterwards `updates/lastResult` (`installed 0.1.0 -> 0.2.0`, or
`failed ...: <reason>`).

With the setting off Mitcad makes no request at start-up; *Check for
Updates* still asks when the user chooses it.

An administrator turns update checks off for every user, for offline or
managed installations; Mitcad then makes no request at all, *Check for
Updates* neither, and Preferences shows the switch off and why:

- the environment variable `MITCAD_NO_UPDATE_CHECK` (any value but empty
  or `0`), on Windows and Linux, for example in `/etc/environment` or the
  system's environment variables;
- on Windows, the registry value `DisableUpdateCheck` (DWORD, not 0) under
  `HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad`:

  ```bat
  reg add HKLM\SOFTWARE\Policies\Mitcad\Mitcad /v DisableUpdateCheck /t REG_DWORD /d 1 /f
  ```

## What is sent

A check is one HTTPS request for the manifest (the pre-release channel:
two, GitHub's list of releases first); an installation adds the download.
The requests carry the User-Agent `Mitcad/<version> (<platform>;
<channel>)`, for example `Mitcad/0.2.0 (windows-x64; stable)`, and
nothing that identifies the user or the machine: no cookies (none are
kept or sent), no cache, `Accept-Language: *` instead of the user's
languages, no telemetry. HTTPS only; a redirect is followed only from
HTTPS to HTTPS (GitHub's downloads redirect to its CDN), at most five.

## Releases on GitHub

Releases are published on GitHub (`mitcad/Mitcad`). Each release carries
as assets:

| Asset | |
|---|---|
| `mitcad-<version>-windows-x64.exe` | the NSIS installer (`package`) |
| `mitcad-<version>-windows-x64.zip` | the portable `.zip` (`package`; announce only) |
| `Mitcad-<version>-x86_64.AppImage` | the Linux AppImage (`appimage`) |
| `update-manifest.json` | the signed manifest (`update-manifest`) |

The tag is `v<version>`; the version is a semantic version
(`0.2.0`, `0.3.0-beta.1`). The channels:

- **stable:** `https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json`.
  GitHub's *latest* release is never a pre-release or a draft; a
  pre-release version in it would be ignored anyway.
- **pre-release:** `https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20`
  (the REST API, unauthenticated: one request a day stays far below the
  rate limit). The release with the highest version that is not a draft
  and has an `update-manifest.json` is taken, and its manifest is fetched
  and verified as on the stable channel.

A version not newer than the running one is never offered (no
downgrades); build metadata (`+...`) does not count.

## The manifest

`update-manifest.json`:

```json
{
"manifest": {
  "format": 1,
  "version": "0.2.0",
  "date": "2026-10-06",
  "notes": "https://github.com/mitcad/Mitcad/releases/tag/v0.2.0",
  "assets": {
    "windows-x64": {
      "url": "https://github.com/mitcad/Mitcad/releases/download/v0.2.0/mitcad-0.2.0-windows-x64.exe",
      "size": 42614672,
      "sha256": "<64 hex digits>",
      "signature": "<128 hex digits>"
    }
  }
},
"signature": "<128 hex digits>"
}
```

- `signature`: the Ed25519 signature of `mitcad-update-manifest\n`
  followed by the exact bytes of the `manifest` value as the file has
  them (no canonical JSON is needed; editing the file in any way breaks
  it). It is checked before anything else of the manifest is read.
- `version`, `date` (`YYYY-MM-DD`), `notes` (the release page, HTTPS).
- `assets`: per platform (`windows-x64`: the installer, `linux-x64`: the
  AppImage) the download's HTTPS address, size, SHA-256 and the Ed25519
  signature of its statement:

  ```text
  mitcad-update-file
  version 0.2.0
  platform windows-x64
  size 42614672
  sha256 <64 hex digits>
  ```

  A platform without an asset gets the notice without *Install*; so does
  macOS (`macos-arm64`), whose new version is installed from its disk
  image.
- Keys, signatures and digests are lowercase hex. `format` is 1; a
  manifest of another format is refused.

## Signing a release

The release key is an Ed25519 key. Its private half exists only in the
release environment (a CI secret) and never in the repository; its public
half is built into Mitcad.

1. **Make the key** once, for example `openssl rand -hex 32 >
   mitcad-release.key` (the 32-byte seed in hex), and keep it secret.
2. **Build the public key in:** `MITCAD_RELEASE_KEY_FILE=mitcad-release.key
   mitcad-release public-key` prints it; put that line of 64 hex digits
   into [packaging/update-key.txt](../packaging/update-key.txt) (or point
   the CMake cache variable `MITCAD_UPDATE_KEY_FILE` at a file with it).
   **Until then the file holds a placeholder: a build without a key makes
   no update request at all** (*Check for Updates* says "This build of
   Mitcad has no release key, so it cannot verify updates"), **and the
   core refuses every manifest**; CMake says so when it configures.
3. **Sign the release** after `package`, in the release environment, with
   the key in `MITCAD_RELEASE_KEY` (the hex digits) or
   `MITCAD_RELEASE_KEY_FILE` (a file of them):

   ```bat
   tools\dev-env\msvc.cmd cmake --build --preset dev --target package
   set MITCAD_RELEASE_KEY_FILE=D:\secrets\mitcad-release.key
   tools\dev-env\msvc.cmd cmake --build --preset dev --target update-manifest
   ```

   `update-manifest` runs the release tool `mitcad-release` (built from
   `core/update`), which signs the installer as the `windows-x64` download
   at `<MITCAD_RELEASE_URL>/download/v<version>/<file>` (the CMake cache
   variable, default `https://github.com/mitcad/Mitcad/releases`; the
   notes at `.../tag/v<version>`) and writes `update-manifest.json` next
   to it, after checking that the application's checks accept it. The
   Linux AppImage is made in the Linux build environment
   (`cmake --build --preset <release preset> --target appimage`). A
   release with several platforms is signed in one run of the tool:

   ```sh
   mitcad-release manifest --version 0.2.0 \
     --notes https://github.com/mitcad/Mitcad/releases/tag/v0.2.0 \
     --asset windows-x64 mitcad-0.2.0-windows-x64.exe \
             https://github.com/mitcad/Mitcad/releases/download/v0.2.0/mitcad-0.2.0-windows-x64.exe \
     --asset linux-x64 Mitcad-0.2.0-x86_64.AppImage \
             https://github.com/mitcad/Mitcad/releases/download/v0.2.0/Mitcad-0.2.0-x86_64.AppImage \
     --out update-manifest.json
   mitcad-release verify --manifest update-manifest.json --key <public key> \
     --asset windows-x64 mitcad-0.2.0-windows-x64.exe
   ```

   The key is never given on the command line.
4. **Publish** the installers and `update-manifest.json` as the GitHub
   release's assets.

There is no Authenticode signature yet: Windows SmartScreen may warn
about a downloaded installer run by hand. The Ed25519 signature is what
Mitcad itself checks. A new release key needs a release built with both
keys first (`packaging/update-key.txt` takes a key per line; Mitcad
accepts a manifest signed with any of them), signed with the old key, so
that installations can move to the new one.

## Tests

No test contacts any host but the machine itself.

- `core.update` (ctest; `cargo test -p mitcad-update`): manifests and
  downloads signed with keys the tests make: valid ones, tampered
  manifests and downloads, the wrong key, no release key, older and equal
  versions, pre-releases on the stable channel, skipped versions, invalid
  manifests, GitHub's list of releases, and the release tool.
- `tools/ui-update-test.sh` (Linux, `check-all.sh` runs it): against a
  local HTTPS server (`tools/update-test-server.py`, a certificate made
  for the run) with a release key made for the run; checks off (the
  setting, the administrator's switch, a build without a release key),
  the notice, what the requests send, skip and later,
  refused manifests and an HTTP redirect, the pre-release channel, a
  per-user AppImage (a script standing for one) that refuses a tampered
  download and then updates itself after saving the changed design, and a
  read-only one that only announces.
- `tools/installer-windows-test.ps1 -MesaDir ...`, and the ctest
  `app.installer-windows` (configured with `MITCAD_MESA_DIR`, run once
  `package` has made the build's installer, skipped otherwise): an
  installed Mitcad refuses a tampered download, then updates itself from
  a local manifest served over HTTPS, saving the changed design, and
  starts again as the new version.
- `tools/appimage-test.sh`, and the ctest `app.appimage` (run once
  `appimage` has made the build's AppImage, skipped otherwise): the real
  AppImage, signed as the `linux-x64` download with a key made for the
  test and served over HTTPS from 127.0.0.1, is offered with *Install*,
  replaces itself once Mitcad has quit and starts again as the release.

The tests point Mitcad at their server with environment variables, which
an installed Mitcad reads too (anyone who can set a user's environment can
also change what Qt loads, so they open no new way in):

| Variable | |
|---|---|
| `MITCAD_UPDATE_URL` | the stable channel's manifest instead of GitHub's (HTTPS) |
| `MITCAD_UPDATE_RELEASES_URL` | the pre-release channel's list of releases |
| `MITCAD_UPDATE_TEST_KEY` | a public key trusted besides the built-in one |
| `MITCAD_UPDATE_TEST_CA` | a PEM certificate the update requests trust besides the system's |
| `MITCAD_TEST_VERSION` | the version Mitcad compares with (the new Mitcad an update starts does not get it) |

The other UI tests set `MITCAD_NO_UPDATE_CHECK=1` (`ui-test-lib.sh`,
`ui-windows-lib.ps1`), so they make no requests.
