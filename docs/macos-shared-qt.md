<!-- SPDX-License-Identifier: MIT -->
# macOS with the shared Qt UI

On macOS, MitCad uses the same Qt presentation as on Linux and Windows.
The AppKit frontend (`app/platform/macos/Native*`, `MITCAD_MACOS_NATIVE_UI`)
is not built: configuring with it ON is an error.

**Decision (owner 6.11.2026):** the shared Qt UI is, for now, the only form
of the macOS releases, so that macOS releases keep pace with the other
platforms and development is more efficient: one UI to build, test and
document. Native macOS UI work resumes when MitCad is more mature
elsewhere; [native-macos-ui.md](native-macos-ui.md) keeps its design and
state. New UI is written once, in shared Qt.

## What is shared and what stays native

The CMake option `MITCAD_MACOS_SHARED_QT_UI` (ON) leaves `MITCAD_MAC_UI`
undefined. Code under `#ifdef MITCAD_MAC_UI` is the 0.48 macOS
presentation, built only with the option OFF, for comparison. Code under
`#ifdef Q_OS_MACOS` stays native in both.

Shared with Linux and Windows:

- Qt's portable style, the shared tool buttons, card buttons and
  checked-state outlines, the shared accent colour.
- Preference pages with group boxes and the shared form alignment, chosen
  from the shared page list.
- Qt document tabs (Settings › General › Open designs in), the shared
  Window menu (New Window, Minimize ⌘M, the open designs).
- Application-modal dialogs instead of sheets; the Qt About dialog.
- The shared Add/Remove buttons, picker and dialog layouts.

Native, because the system or the keyboard needs them:

- **Hardware:** the OpenGL core-profile context for OCCT; the XPC render
  service with IOSurface and Metal frames; Cycles with the Metal device and
  Open Image Denoise with Metal and BNNS; thread quality of service and
  App Nap prevention (`ProcessActivity`); the 3D mouse driver.
- **Trackpad:** pinch zoom, rotate, smart zoom, two-finger pan with
  momentum (shared code; macOS delivers the events) and the Trackpad
  navigation scheme as the default.
- **Settings window:** Settings… (⌘,) opens a window without OK and
  Cancel; a change applies as it is made and ⌘W closes the window, as in
  macOS applications (the shared `SettingsWindow` with the page list; Linux
  and Windows keep the Preferences dialog).
- **Keyboard:** Backspace deletes (browser, timeline, projects, sketch);
  ⌘0 fits the view (F keys are media keys); Option is the fine step;
  ⌘ and ⌥ in key names; ⌘, opens Settings; ⌘W closes a tool window.
  The system's keys ⌘M, ⌘H, ⌥⌘H and ⌃⌘F are reserved in Keyboard
  Shortcuts. Return takes a dialog's default button and Close has no
  Alt+C mnemonic (macOS has none).
- **Menus:** the system menu bar holds the menus; Quit, About and Settings
  go to the application menu (menu roles). The window's own copy of the
  menu bar is hidden by default (Settings › Display › Show the menu bar in
  the window).
- **Appearance:** the 3D view's background follows the system's light or
  dark appearance by default (Automatic Background).
- **System:** settings and application paths, updates of the bundle,
  slicer processes, Claude Desktop and Finder.

## Release check

Other platforms: `tools/check-all.sh --full` on Linux, and a warning-free
build with the full `ctest` in the Windows VM. The macOS counterpart is
`tools/release-check-macos.sh`:

| Step | |
|---|---|
| build | The `macos-release` preset with every core; compiler warnings fail. |
| native | The AppKit frontend is off, none of its objects exists and no `app.ui-macos-native*` test is registered. |
| ctest | Every ctest through `tools/test-macos.sh` (`mitcad-testrun --schedule`, `-j` the cores): headless tests and the GUI harnesses beside each other, the window-server tests one at a time under `macos_desktop`. |
| package | `tools/package-macos.sh`: install, signature, `tools/check-bundle-macos.sh`, the `.dmg`. |

```bash
tools/release-check-macos.sh                       # everything
tools/release-check-macos.sh --only native,ctest   # an existing build
tools/release-check-macos.sh -- -R '^app\.ui-macos'
```

The macOS VM is for testing; a release runs on a native Mac, where the
GPU is real (the VM's OpenGL is the software renderer and its Metal device
is paravirtual). The corpus tests need `MITCAD_F3D_CORPUS` and
`MITCAD_FCSTD_CORPUS`; Linux's full check runs them in any case.

### Tests of the 0.48 macOS presentation

Registered only with `MITCAD_MACOS_SHARED_QT_UI=OFF`: `app.ui-macos-tool-buttons`
(the Mac tool button style) and `app.ui-macos-multi-document` (the system
window tabs; `app.ui-macos-document-frames` tests the shared document tabs).
`repo.native-coverage` is not registered: the AppKit frontend is not built.

### First native-Mac release check (the next release)

The VM results below leave open whether three tests fail for the VM or
for the layout. The next release's macOS build, on a native Mac, settles
it: run `tools/release-check-macos.sh` there and record each of them. A
test that passes there was the VM's; one that fails is fixed or adjusted
(the test, the fit's room around the floating cards, or the table
checkbox's outline), and the check itself is adjusted by what the native
run shows (for example a larger test window, or a VM display tall enough
for 1400 x 990 windows).

### Result in the macOS VM (2026-11-06)

All 665 ctests ran (the scheduler's disk floor stopped the first pass; the
rest ran in a second). Not passing:

- `app.ui-macos-move-proxy-parity`: failed in this VM before the branch too
  (software OpenGL, about 2.6 frames a second: every move reaches its exact
  preview).
- `app.ui-macos-sketch-dimension-selection`, `app.ui-macos-sketch-polygon-dimensions`:
  passed with the 0.48 presentation. The shared layout's top cards float
  over the view, and the VM's window is at most 757 px high: the fitted
  body's far corner lies under the orientation cube or a value field. A
  native Mac's screen, a taller VM display, or a fit that leaves the
  floating cards' area free would decide it.
- `app.ui-macos-dialog-gallery`: the unchecked checkbox in a table has a
  fainter outline in the shared style (contrast 69) than the threshold
  measured for Qt's macOS style.

Corpus tests are skipped without `MITCAD_F3D_CORPUS` and
`MITCAD_FCSTD_CORPUS`.
