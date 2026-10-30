# Automatic updates

MitCad checks for newer releases. Where it can replace itself, MitCad
installs a newer release with your consent. The user guide also gives
the user-level description, refer to [Updates](user-guide.md#updates).

## Behaviour

MitCad checks for a newer release, shows a notice and, if the
installation permits it, installs the release. This section gives the
check, the notice and the installation on each platform. It also gives
the installations that only announce, a download that does not verify
and the report after an update.

**Check:** MitCad checks for updates at start-up, at most once a day.
The check starts 3 s after the window opens. You can also start a check
from *Help › Check for Updates*. A silent automatic check reports
nothing on failure. It also reports nothing when MitCad is up to date.
The manual check always reports.

**Notice:** when a newer release is available, MitCad shows a bar under
the toolbar. The bar does not block your work. It has the buttons
*Release Notes*, *Install*, *Skip This Version* and *Later*. If you
skip a version, MitCad does not announce that version again
automatically.

**Install on Windows (NSIS installation):** MitCad downloads the
installer to a private folder with a random name in the temp folder,
`%TEMP%\mitcad-update-<random>`, and verifies it. Then the window
closes. MitCad asks you to save your work first. After MitCad has quit,
the installer runs silently into the same folder and scope as the
current installation. Then MitCad starts again.

The installer runs only after a second check. MitCad passes the SHA-256
of the verified manifest to `mitcad-updater.exe` on its command line
(`--sha256`). Right before the installer runs, the updater opens the
installer with read sharing only. While that handle is open, no other
program can write to the file, rename it or delete it. The updater
computes the SHA-256 through that handle (Windows CNG). It runs the
installer only if the hash matches. It runs the installer by the final
path of the open file, with links and junctions resolved. The handle
stays open until the installer has ended. Thus a program that replaces
the file between the first check and the start (for example, at the
UAC prompt of an installation for all users) cannot get its own file
run. If the check fails, the updater does not run the installer. It
starts MitCad again, and MitCad reports the reason from the last line
of the updater's log. The next start of MitCad removes the folder
(`updates/downloadFolders` lists it until then).

The scope comes from the `installation.ini` of the installation (refer
to [development.md](development.md#windows-installer)). An installation
just for the current user updates as the user, without a UAC prompt. An
installation for all users (Program Files) updates elevated. UAC asks
for permission, unless you already have administrator rights.

An installation of an earlier release has no `installation.ini`. Such
an installation was for all users, thus it continues to update there.
The installer recognises the folder of the installation for all users
and elevates itself.

**Install on Linux (AppImage in a writable folder):** MitCad downloads
the new AppImage next to the installed file, as the hidden file
`.<name>.update`. MitCad verifies the download and makes it executable.
After MitCad has quit, the new file replaces the old file in one atomic
step. Then the new MitCad starts.

**Install on macOS (application bundle in a writable folder):** MitCad
downloads the `macos-arm64` disk image to a private temporary folder and
verifies it. Then the window closes. MitCad asks you to save your work
first. After the process has quit, a helper mounts the disk image
read-only, without a Finder window. The helper checks the identifier,
the version and the code signature of the new app. Then it copies
`Mitcad.app` beside the current installation, into a staging folder.

The helper exchanges the installed bundle and the staged bundle in one
atomic step on the same filesystem. The old bundle stays in the staging
folder until the helper detaches the disk image and macOS Launch
Services accepts the relaunch. If a step fails, the helper reverses the
exchange and starts the old version again. If the rollback itself fails,
the log gives the path of the old bundle that the helper kept. If the
filesystem does not support an atomic exchange, the update fails and the
installed bundle does not move.

**Announce only:** some installations get the notice without *Install*:

- The portable `.zip`
- Unbundled development builds
- System packages
- Read-only installations
- On macOS, an app that runs from a read-only disk image
- On macOS, an app on an App Translocation path
- On macOS, a quarantined bundle in Downloads

The startup log gives the reason. On macOS, copy the app to a writable
Applications folder first. MitCad requests no elevation to replace a
bundle that the current user cannot write.

**A download that does not verify:** if the signature, the SHA-256 or
the size of a download does not match, MitCad deletes the download and
reports the problem. MitCad never runs such a download. MitCad refuses a
manifest with a bad signature.

**Report:** after an update, the new MitCad reports the result. The
report comes from the settings in
[Settings and turning checks off](#settings-and-turning-checks-off).

An older production macOS version, released before the bundle updater,
can announce a new release. This occurs when the manifest of the
release lists `macos-arm64`. But you must download and install that
release manually. The asset in the manifest cannot add installation
code to the older application. When a version with the bundle updater
is installed, that version can install subsequent releases
automatically with your consent.

The local UI test simulates an earlier version with the corrected
binary. Thus it does not prove an update from an older production
binary.

The macOS updater keeps the code signature, the resource forks and the
extended attributes of the bundle. It adds no quarantine attribute and
removes none. It does not sign the release again, and it does not change
the Gatekeeper policy. If a code signature is present but invalid, the
updater refuses the bundle before it replaces the old bundle. The
Ed25519 release signature authenticates the disk image. macOS code
signing and Gatekeeper apply independently.

An ad-hoc signature, as development builds use, does not establish a
notarized developer identity. MitCad can verify an unsigned local bundle
as a download, but macOS can still refuse the bundle at launch. The
updater cannot change an unsigned or ad-hoc build into a notarized
application. It cannot give such a build a new Gatekeeper exception.

## Settings and turning checks off

The update settings are on the page *Tools › Preferences › Updates*. The
page has two settings:

- **Check for updates at start-up, once a day**: on by default
- The **channel**: *Releases* (stable, the default) or *Pre-releases
  too*

MitCad stores the settings under these keys:

| Key | Content |
|---|---|
| `updates/automatic` | the check at start-up, on or off |
| `updates/channel` | `stable` or `prerelease` |
| `updates/skipped` | the version that *Skip This Version* selected |
| `updates/lastCheck` | the time of the last check (UTC) |
| `updates/hintShown` | |
| `updates/installing`, `updates/installingFrom`, `updates/installLog` | set during an update |
| `updates/downloadFolders` | Windows: the private download folders that the next start removes |
| `updates/lastResult` | the result after an update: `installed 0.1.0 -> 0.2.0`, or `failed ...: <reason>` |

With the setting off, MitCad makes no request at start-up. *Check for
Updates* still asks when you select it.

An administrator can set update checks to off for every user, for
example for offline or managed installations. Then MitCad makes no
request at all. *Check for Updates* makes none either. Preferences
shows the switch off and gives the reason. The administrator has two
switches:

- The environment variable `MITCAD_NO_UPDATE_CHECK` with any value but
  empty or `0`. This switch works on every platform, for example in
  `/etc/environment` or in the system's environment variables.
- On Windows, the registry value `DisableUpdateCheck` (DWORD, not 0)
  under `HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad`:

  ```bat
  reg add HKLM\SOFTWARE\Policies\Mitcad\Mitcad /v DisableUpdateCheck /t REG_DWORD /d 1 /f
  ```

## What is sent

A check is one HTTPS request for the manifest. On the pre-release
channel, a check is two requests: first GitHub's list of releases, then
the manifest. An installation adds the download. The requests carry the
User-Agent `Mitcad/<version> (<platform>; <channel>)`, for example
`Mitcad/0.2.0 (windows-x64; stable)`. The requests carry nothing that
identifies you or the machine:

- No cookies. MitCad keeps none and sends none.
- No cache
- `Accept-Language: *` instead of your languages
- No telemetry

MitCad uses HTTPS only. It follows a redirect only from HTTPS to HTTPS,
and at most five redirects. GitHub's downloads redirect to its CDN.

## Releases on GitHub

The project publishes releases on GitHub (`mitcad/Mitcad`). Each
release carries these assets:

| Asset | Content |
|---|---|
| `mitcad-<version>-windows-x64.exe` | the NSIS installer (`package`) |
| `mitcad-<version>-windows-x64.zip` | the portable `.zip` (`package`; announce only) |
| `Mitcad-<version>-x86_64.AppImage` | the Linux AppImage (`appimage`) |
| `Mitcad-<version>-macos-arm64.dmg` | the macOS application disk image (`dmg`) |
| `update-manifest.json` | the signed manifest (`update-manifest`) |

The tag is `v<version>`. The version is a semantic version, for example
`0.2.0` or `0.3.0-beta.1`. The two channels read different addresses:

- **stable:** MitCad reads
  `https://github.com/mitcad/Mitcad/releases/latest/download/update-manifest.json`.
  GitHub's *latest* release is never a pre-release or a draft. MitCad
  ignores a pre-release version in it anyway.
- **pre-release:** MitCad reads
  `https://api.github.com/repos/mitcad/Mitcad/releases?per_page=20`.
  This is the REST API, unauthenticated. One request a day stays far
  below the rate limit. MitCad takes the release with the highest
  version that is not a draft and has an `update-manifest.json`. Then
  MitCad gets the manifest of that release and verifies it as on the
  stable channel.

MitCad never offers a version that is not newer than the installed
version. Thus there are no downgrades. Build metadata (`+...`) does not
count in the comparison.

## For developers

The update code has three parts:

- [core/update](../core/update/src/lib.rs): the manifest, the
  verification, the versions and the `mitcad-release` tool. It uses
  `ed25519-dalek`, `sha2` and `semver`.
- The bridge `core/ffi/src/update.rs`
- `app/update/`: the requests, with Qt Network. Thus TLS is Qt's
  platform backend. The first request logs it:
  `Update check: TLS <library> (<backend> backend)`, or
  `Update check: TLS unavailable (…)` with Qt's backends when none works.

**TLS on Linux:** Qt's OpenSSL backend loads `libssl.so.3` and
`libcrypto.so.3` at run time. The AppImage carries both, from one
OpenSSL build (vcpkg's), and Qt finds them in the AppImage before the
system's (mitcad#954). Before, the AppImage carried only `libcrypto`
(for the render worker), and Qt combined it with the system's `libssl`.
Two OpenSSL builds then ran in one process, and on some distributions
(Fedora) the check failed with "TLS initialization failed". The bundled
OpenSSL reads the system's `/etc/ssl/openssl.cnf` (the distribution's
crypto policy), and Qt reads the system's CA certificates itself
(`/etc/ssl/certs`, Fedora's `/etc/pki/tls/certs/ca-bundle.crt` and
others). It loads no engine or provider module: TLS needs none
([development.md](development.md#linux-appimage)).

### The manifest

`update-manifest.json` has this form:

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
  them. No canonical JSON is necessary. If you edit the file in any way,
  the signature breaks. MitCad checks the signature before it reads
  anything else of the manifest.
- `version`, `date` (`YYYY-MM-DD`) and `notes` (the release page,
  HTTPS).
- `assets`: one entry per platform. `windows-x64` is the installer,
  `linux-x64` is the AppImage and `macos-arm64` is the disk image. Each
  entry gives the HTTPS address of the download, its size, its SHA-256
  and the Ed25519 signature of its statement:

  ```text
  mitcad-update-file
  version 0.2.0
  platform windows-x64
  size 42614672
  sha256 <64 hex digits>
  ```

  If a platform has no asset, MitCad shows the notice without
  *Install*.
- Keys, signatures and digests are lowercase hex. `format` is 1. MitCad
  refuses a manifest of another format.

### Signing a release

The release key is an Ed25519 key. Its private half exists only in the
release environment, as a CI secret. The private half is never in the
repository. Its public half is built into MitCad.

1. **Make the key** once and keep it secret. For example,
   `openssl rand -hex 32 > mitcad-release.key` writes the 32-byte seed
   in hex.
2. **Build the public key in.** The command
   `MITCAD_RELEASE_KEY_FILE=mitcad-release.key mitcad-release public-key`
   prints the public key. Put that line of 64 hex digits into
   [packaging/update-key.txt](../packaging/update-key.txt). As an
   alternative, point the CMake cache variable `MITCAD_UPDATE_KEY_FILE`
   at a file with the key. **Until then, the file holds a placeholder.
   A build without a key makes no update request at all.** Its *Check
   for Updates* says "This build of MitCad has no release key, so it
   cannot verify updates". **The core refuses every manifest.** CMake
   reports this when it configures.
3. **Sign the release** after `package`, in the release environment.
   Give the key in `MITCAD_RELEASE_KEY` (the hex digits) or in
   `MITCAD_RELEASE_KEY_FILE` (a file with the hex digits):

   ```bat
   tools\dev-env\msvc.cmd cmake --build --preset dev --target package
   set MITCAD_RELEASE_KEY_FILE=D:\secrets\mitcad-release.key
   tools\dev-env\msvc.cmd cmake --build --preset dev --target update-manifest
   ```

   `update-manifest` runs the release tool `mitcad-release`, built from
   `core/update`. The tool signs the installer as the `windows-x64`
   download at `<MITCAD_RELEASE_URL>/download/v<version>/<file>`.
   `MITCAD_RELEASE_URL` is a CMake cache variable. Its default is
   `https://github.com/mitcad/Mitcad/releases`. The notes are at
   `.../tag/v<version>`.

   The tool makes sure that the checks of the application accept the
   manifest. Then it writes `update-manifest.json` next to the
   installer. The Linux build environment makes the Linux AppImage
   (`cmake --build --preset <release preset> --target appimage`). One
   run of the tool signs a release with several platforms:

   ```sh
   mitcad-release manifest --version 0.2.0 \
     --notes https://github.com/mitcad/Mitcad/releases/tag/v0.2.0 \
     --asset windows-x64 mitcad-0.2.0-windows-x64.exe \
             https://github.com/mitcad/Mitcad/releases/download/v0.2.0/mitcad-0.2.0-windows-x64.exe \
     --asset linux-x64 Mitcad-0.2.0-x86_64.AppImage \
             https://github.com/mitcad/Mitcad/releases/download/v0.2.0/Mitcad-0.2.0-x86_64.AppImage \
     --asset macos-arm64 Mitcad-0.2.0-macos-arm64.dmg \
             https://github.com/mitcad/Mitcad/releases/download/v0.2.0/Mitcad-0.2.0-macos-arm64.dmg \
     --out update-manifest.json
   mitcad-release verify --manifest update-manifest.json --key <public key> \
     --asset windows-x64 mitcad-0.2.0-windows-x64.exe
   ```

   Never give the key on the command line.
4. **Publish** the installers and `update-manifest.json` as the assets
   of the GitHub release.

There is no Authenticode signature yet. Thus Windows SmartScreen can
warn about a downloaded installer that you run by hand. MitCad itself
checks the Ed25519 signature.

To change to a new release key, first build a release with both keys.
`packaging/update-key.txt` takes one key per line, and MitCad accepts a
manifest signed with any of them. Sign that release with the old key.
Then installations can move to the new key.

### Tests

No test contacts any host but the machine itself. The tests point
MitCad at their own server only in a test build
([the test hooks](#the-test-hooks)). The tests are:

- `app.unit` (`app/tests/update_installer.cpp`, every platform): the
  private download folder of Windows and macOS (a random name in the
  temp folder, owner-only on Linux and macOS, a new one after removal,
  nothing else removed as a leftover). It also checks the updater's
  command line with `--sha256`, which MitCad writes and the updater
  reads (`app/update/UpdaterChecks.hpp`). The updater refuses a missing
  or malformed hash. The hash comparison of a file tells a changed file
  apart. An installation for all users only announces an update from a
  test's source, unless MitCad has administrator rights already
  (mitcad#921).
- `app.update_source_release` and `app.update_source_hooks`
  (`app/tests/update_source.cpp`, every platform): the update source
  built as a release and as a test build, whatever the build's own
  option, with every test variable set. The release keeps GitHub's
  addresses, its built-in keys, no extra certificate and its own
  version, and the core refuses a manifest signed with
  `MITCAD_UPDATE_TEST_KEY` (made by `mitcad-release` for the run). The
  test build takes them all, and the core accepts that manifest. Both
  keep the built-in keys apart for the gate of a build without a key.
- `core.update` (ctest, `cargo test -p mitcad-update`): manifests and
  downloads signed with keys that the tests make. The test covers valid
  manifests and downloads, and tampered manifests and downloads. It
  covers the wrong key, no release key, older and equal versions and
  pre-releases on the stable channel. It also covers skipped versions,
  invalid manifests, GitHub's list of releases and the release tool.
- `app.ui-macos-update` (`tools/ui-macos-update-test.sh`, in the macOS
  guest): a local HTTPS manifest and test-owned, ad-hoc signed
  application bundles in a real disk image. The test checks the atomic
  exchange and the rollback. It checks announce only on the mounted
  read-only image and on a synthetic App Translocation path. It checks
  the rejection of a changed download and of a broken code signature,
  and the rollback on a Launch Services refusal. Then it checks the
  offer, the download, the verification and the installation on quit
  and relaunch, with the real version of the build. Only the version of
  the initial process is overridden, to simulate an earlier
  installation. The path fixture checks the translocation policy. It
  does not claim that the OS translocated the fixture. Real OS
  translocation, and a production signed release manifest offered to an
  installed previous release, are separate release acceptance checks.
- `tools/ui-update-test.sh` (Linux, `check-all.sh` runs it): the test
  runs against a local HTTPS server (`tools/update-test-server.py`, with
  a certificate made for the run) and a release key made for the run.
  It checks the three ways to set checks off: the setting, the
  administrator's switch and a build without a release key. It checks
  the notice, what the requests send, skip and later, refused manifests,
  an HTTP redirect and the pre-release channel. It checks a per-user
  AppImage (a script stands for one) that refuses a tampered download
  and then updates itself after it saves the changed design. It checks a
  read-only AppImage that only announces.
- `tools/installer-windows-test.ps1 -MesaDir ...`, and the ctest
  `app.installer-windows` (configured with `MITCAD_MESA_DIR`): the ctest
  runs once `package` has made the installer of the build, and is
  skipped otherwise. The installed `mitcad-updater.exe` does not run an
  installer with a SHA-256 that is not the given one, or without one,
  and it logs the reason as its last line. An installation just for the
  current user refuses a tampered download. The download goes to a
  private folder. Then the installation updates itself from a local manifest
  served over HTTPS, as an installation for all users also does. It
  saves the changed design, starts again as the new version and stays
  registered in its scope only.
- `tools/appimage-test.sh`, and the ctest `app.appimage`: the ctest runs
  once `appimage` has made the AppImage of the build, and is skipped
  otherwise. The real AppImage is signed as the `linux-x64` download
  with a key made for the test and served over HTTPS from 127.0.0.1.
  MitCad offers it with *Install*. The AppImage replaces itself after
  MitCad has quit and starts again as the release. The check uses the
  AppImage's own OpenSSL: the logged TLS library is the bundled
  libcrypto's version, and MitCad has loaded `libssl.so.3` and
  `libcrypto.so.3` from the AppImage only.

#### The test hooks

The tests point MitCad at their server with environment variables. Only
a **test build** reads them: a build configured with the CMake option
`MITCAD_UPDATE_TEST_HOOKS` (mitcad#921). The option is off by default
and in the release presets (`linux-release`, `macos-release`,
`windows-release`), and on in the debug presets (`linux-debug`,
`macos-debug`), which the development builds and `check-all.sh` use.

| Variable | Content |
|---|---|
| `MITCAD_UPDATE_URL` | the stable channel's manifest instead of GitHub's (HTTPS) |
| `MITCAD_UPDATE_RELEASES_URL` | the pre-release channel's list of releases |
| `MITCAD_UPDATE_TEST_KEY` | a public key trusted besides the built-in one |
| `MITCAD_UPDATE_TEST_CA` | a PEM certificate the update requests trust besides the system's |
| `MITCAD_TEST_VERSION` | the version MitCad compares with (the new MitCad an update starts does not get it) |

The policy:

- **A release never reads these variables.** Whoever can set a user's
  environment (on Windows, any program that runs as the user can write
  `HKEY_CURRENT_USER\Environment`) could otherwise point MitCad at a
  server of their own and have a key of their own trusted. MitCad would
  then offer, verify and install their installer as a genuine update,
  and on an installation for all users the UAC prompt that the user
  expects after *Install* would run it with administrator rights. A
  release checks GitHub's addresses with the built-in release keys only,
  and compares its own version.
- MitCad logs where its updates come from at start-up, also with checks
  off: `Update source: manifest at <url>, releases at <url>, N built-in
  release key(s)` (`and the test key` when a test build took one). With
  any of the variables set, a second line says `this test build reads
  ...` or `this build ignores ... (MITCAD_UPDATE_TEST_HOOKS off)`.
- **A test's source never asks for administrator rights.** When a test
  build takes an address, a key or a certificate from the environment,
  an installation for all users only announces a release, unless MitCad
  runs with administrator rights already. Then the update elevates
  nothing (the installer test runs so). A changed version alone
  (`MITCAD_TEST_VERSION`) is not a test's source.
- **A build without a release key makes no update request**, also when
  a test build has `MITCAD_UPDATE_TEST_KEY` set: only the built-in keys
  count.
- A configure that requires the render worker's CUDA device (a release:
  `MITCAD_RENDER_REQUIRED_DEVICES` naming CUDA, as a cache variable or
  from the environment) stops when `MITCAD_UPDATE_TEST_HOOKS` is on. The
  `update-manifest` target of a test build refuses to sign.
- The update tests take the option from the build: the ctest's
  `UI_UPDATE_TEST_HOOKS`, or the build folder's `CMakeCache.txt`. Built
  without it (a release's configuration), `app.appimage`,
  `app.installer-windows`, `app.ui-macos-update` and
  `tools/ui-update-test.sh` check that MitCad ignores the variables: its
  own version, GitHub's addresses, the built-in keys only, no request to
  the test's server. A release AppImage (`appimage-test.sh
  --require-cuda`) with the option on fails.

On Windows the `dev` preset of the build VM inherits `windows-release`
and builds the releases too, so it stays without the option. Configure a
separate build with `-DMITCAD_UPDATE_TEST_HOOKS=ON` to run the update
part of `app.installer-windows`.

The other UI tests set `MITCAD_NO_UPDATE_CHECK=1` (in `ui-test-lib.sh`
and `ui-windows-lib.ps1`). Thus they make no requests.
