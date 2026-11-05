<!-- SPDX-License-Identifier: MIT -->
# Native UI parity acceptance

Issue #820 combines the corrections in #812–#819 with an integrated Qt/AppKit
comparison. A successful build, a capture or a control found by identifier
does not establish visual or interaction acceptance. Keep Qt as the default
macOS build and release frontend.

## Headless checks before desktop acceptance

Run helper and policy checks without opening a desktop window:

```bash
QT_QPA_PLATFORM=offscreen tools/test-macos.sh --build build/macos-release \
  -R '^(app\.(unit|render_devices|drag_proxy|library_paths)|repo\.(tooltips|spdx))$'
```

The ordinary scheduler can overlap these checks with each other. The model's
Rust tests and CLI fixture preparation also run without the Aqua desktop;
retain their Cargo fixtures and reuse unchanged build outputs. These checks
cover shared widget helpers, device policy, drag calculations, library paths
and repository rules. They do not establish native layout or input acceptance.

The assignment/Bindings test also exposes its saved-data checks separately.
After a GUI workflow has produced a design, raw log and captures, these commands
recheck the model bindings, exact picked face and capture inventory without a
desktop or the shared GUI lock:

```sh
python3 tools/ui-scripts/bindings-parity-verify.py theme \
  <saved-design> <capture-directory> qt light
python3 tools/ui-scripts/bindings-parity-verify.py open-values \
  <saved-design> <raw-log> <capture-directory> qt
```

Use `native` for the AppKit result and `dark` for a dark theme result. This is
retained-output validation; it neither runs the interactive workflow nor
certifies the visual content of the captures. Keep the original GUI report
and its source/binary evidence alongside the saved files.

AppKit layout, menus, focus, WindowServer pointer input and composed screenshots
still need a logged-in macOS desktop. Keep those tests serial under the shared
GUI lock. An offscreen screenshot framebuffer alone does not make an AppKit
window independent of WindowServer, and setting `QT_QPA_PLATFORM=offscreen`
for the native audit would change the behavior being checked.

## Reproduce the surface inventory

Run serially in the approved macOS GUI environment, after the scoped
build and regression tests. Use the same synthetic demo and local fastener
library in both frontends. The capture tool creates isolated settings,
recovery, project and library directories. It preserves earlier evidence by
requiring a new output directory for every invocation.

```sh
python3 tools/ui-macos-parity-audit.py \
  --app <candidate-QtOFF-app-executable> --cli <candidate-cli> \
  --frontend qt --revision <full-tested-source-commit> \
  --build-evidence <Qt-build-source-manifest> --build-evidence <Qt-build-log> \
  --output <new-qt-evidence-directory>
python3 tools/ui-macos-parity-audit.py \
  --app <candidate-NativeON-app-executable> --cli <candidate-cli> \
  --frontend native --revision <full-tested-source-commit> \
  --build-evidence <native-build-source-manifest> --build-evidence <native-build-log> \
  --output <new-native-evidence-directory>
```

This comparison uses the current candidate's default Qt startup in a QtOFF
binary and explicit `--ui=native` in its NativeON binary, built from the same
source commit. It is separate from the earlier released 0.30.0 audit; neither
candidate binary is labelled as that release application.

The script records the supplied source commit, tracked source content hash
when Git metadata is available, binary and script hashes, UTC timestamps,
startup arguments, raw logs, scripts and PNG dimensions/hashes. Supplied build
evidence is copied into the new evidence directory. The caller must verify
that the binaries were built from that commit; a binary hash alone cannot prove
its source revision. Retain the build cache, startup arguments and build log
alongside the report. Record the actual release separately when comparing a
released application with the candidate; explicit Qt mode in a candidate does
not substitute for testing the release application.

The original 28 groups run in both appearances (56 runs per frontend). The
selection group captures main, selected face, Extrude and command search.
The Parameters group also commits invalid units with Return and Tab, verifies
that the dialog and unchanged model survive, and captures each refusal and a
successful correction in both frontends. Review the native cell and in-dialog
reason in the resulting images; the shared model's refusal state alone cannot
certify the native presentation.
The picker group retains its fixture state, then opens the actual BUILD
Profile command's picker with two pictured synthetic profiles. Its open-table
Grade and Offset options exercise the #810 cascade and a committed numeric
edit through each frontend's controls. The Projects group opens the selected
synthetic design with Return in the actual Projects tree before capturing the
Open a Project fixture. Send Feedback is opened and cancelled; nothing is sent.

Each state has a content capture and an own-window screen capture for checking
screen clipping and system chrome. The software-rendered VM canvas can differ
between those captures; inspect the framebuffer before reporting missing
geometry. `--only` selects a diagnostic subset and marks the report incomplete.
`--plan-only` writes scripts and planned records without executing the CLI or
opening the application. Execution stops at the first failed scenario and
preserves its evidence and all unrun inventory. Never overwrite a failed run.

| # | Group ID | Surface and review |
| --- | --- | --- |
| 1 | `appearances` | Appearances: production entry; pictures, normal/narrow bounds and actions |
| 2 | `community` | Community Library: production entry; pictured catalogue, categories and details |
| 3 | `feedback` | Send Feedback: open/cancel only; fields, footer and screen bounds |
| 4 | `insert` | Insert from Library: production entry; catalogue and insertion controls |
| 5 | `libraries` | Libraries: production entry; source rows, checkboxes and actions |
| 6 | `new-project` | New Project: production entry; compact form, labels and normal/narrow fields |
| 7 | `parameters` | Parameters: production entry; hierarchy, invalid Return/Tab, refusal and correction |
| 8 | `parts-fixture` | Components in This Design fixture: separate synthetic presenter and versions/bill of materials |
| 9 | `picker-fixture` | Component Picker fixture, then actual BUILD Profile picker and configuration options |
| 10 | `project-settings` | Project Settings: shared entry adapter with synthetic data; fields and storage choices |
| 11 | `projects` | Actual selected Projects design + Return, then Open a Project fixture |
| 12 | `publish` | Publish to Library: production entry; helper actions and footer |
| 13 | `render` | Render Image: production entry; preview and reachable inputs |
| 14 | `selection` | Main, selected face, Extrude and command search; shared artwork and compact Create controls |
| 15 | `settings-cache` | Settings Cache: production entry; fields, labels and focus |
| 16 | `settings-cloud` | Settings Cloud: production entry; fields, labels and focus |
| 17 | `settings-display` | Settings Display: production entry; fields, labels and focus |
| 18 | `settings-navigation` | Settings Navigation: production entry; slider widths and control artwork |
| 19 | `settings-textures` | Settings Textures: production entry; pictures, fields and labels |
| 20 | `shortcuts` | Keyboard Shortcuts: production entry; normal/narrow hierarchy and filtering |
| 21 | `print` | 3D Print: production entry; compact fields and visible primary/cancel actions |
| 22 | `profiles` | Processes (once Printers and Processes, mitcad#1135): production entry; grouped list, editor fields and actions |
| 23 | `materials` | Materials: production entry; list, fields and assignment actions |
| 24 | `properties` | Properties: production entry; readable results and closing action |
| 25 | `export` | Export: production entry; format choices and action order; no file exported |
| 26 | `find` | Find in Design: production entry; fields, results and document ownership |
| 27 | `overview` | Keyboard and Mouse Overview: production entry; text hierarchy and scrolling |
| 28 | `parts-production` | Components in This Design production entry after actual synthetic fastener insertion |

The JSON report distinguishes `planned`, `running`, `captured`, `failed` and
`not-selected` scenarios. A completed diagnostic subset remains `incomplete`.
Capture success is recorded as `captured`, with visual and interaction review
still `pending`. Exit zero checks execution, expected frontend, all expected
PNG headers/bounds and absence of driver/canvas/Auto Layout failures. It does
not certify legibility, pointer targets, picture correctness or keyboard
behavior. Inspect every pair in both themes and record findings with the
surface, frontend, appearance, screenshot and source commit. Keep harness
failures and product defects separate; repair selectors rather than omitting
the failing scenario. Preserve failed runs before retrying.

## Integrated behavior acceptance

The child issues provide focused regressions. Record their exact names,
revision and results with the surface review. At minimum check:

- #812 and #817: normal/narrow field and slider bounds, real pointer/focus,
  macOS action order, helpers, Return and Escape.
- #813 and #819: pictured catalogue, categories, favourites, search, tile size,
  pointer and keyboard insertion; the actual BUILD command's modeless picker,
  pin/unpin, close/reopen and command/canvas focus recovery. A picker fixture
  capture cannot pass production BUILD acceptance.
- #814: invalid `d3` length expressions `30 deg` with Return and `45 deg` with
  Tab; Parameters stays open, the demo remains 20 mm / 48000 mm³, and the
  refused cell and in-dialog reason stay visibly marked. Correct to `25 mm`
  and verify the error clears. Repeat the paired Qt probe.
- #815: all three face Create actions, compact Extrude, expression entry,
  visible primary/cancel actions and 3D Print.
- #816: focus the actual Projects tree, select a synthetic design and press
  Return; verify the design opens in both frontends. Exercise F2, Delete,
  creation/rename with commit and Escape, context actions and valid/invalid
  within-project pointer moves; verify selection and canvas focus afterward.
- #818: card width dragging/restoration, long names, screen bounds including
  titlebars and the shared Qt Appearances overflow.
- Retain timeline size/marker/error clarity, Browser badges and overlap,
  Projects clipping/layering/disclosure/startup visibility. Cover many long
  entries, resizing, scrolling, relaunch and normal/narrow document windows.
- Run the ordinary Qt modeling path and the native secondary/modeless paths.
  Record actual pointer/keyboard outcomes separately from capture completion.

## Completion record

Do not close #820 with acceptance cells still blank. Link the integrated
commit and warning-free macOS build, scoped test report, paired captures and
reviewed findings. Record maintainer acceptance of the resulting iteration
explicitly. Record physical trackpad, VoiceOver/accessibility and the oldest
supported macOS runtime separately, as passed, failed or pending with a
concrete reason. VM runs do not certify physical input or unavailable runtimes.

## Operator acceptance on 31 October 2026

The remaining manual checks in #917 and #918 use the retained AppKit candidate
`bd12f12593aed709aea599ec1c7a7304c5f950d5`, integrated on main as
`ed89fe274408e8f1bbe34184398a5edfdf35c060`. This candidate includes the later
inventory corrections through #996. Its 4,069 tracked source files were checked
against the Git objects, accounting for the Windows script checkout line endings
required by `.gitattributes`. The retained AppKit and Qt executable, CLI and CMake
cache hashes match their previously built and tested producer records. No rebuild
or full inventory rerun was performed for these operator checks.

The AppKit executable SHA-256 is
`824bf2535261e7620093b73c6ad70ec8612f83c7f6b2ac10d5c88b34e844da43`.
Its CMake cache enables `MITCAD_MACOS_NATIVE_UI`, its launch explicitly selects
`--ui=native`, and its startup log confirms the AppKit shell. The runtime is
macOS 27.0.1, arm64, with the Apple M1 Max renderer. The earlier `a6159618a`
paired inventory remains separate and unchanged.

| Acceptance | Status | Evidence and remaining work |
| --- | --- | --- |
| Maintainer review, #917 | Passed | After both operator test rounds, the maintainer explicitly accepted this tested AppKit candidate. |
| Physical trackpad, #917 | Passed | Using a built-in or directly connected physical trackpad, the operator passed pan, Option-scroll orbit including momentum, pinch/Shift-scroll zoom, secondary-window scrolling and context-menu focus recovery. The production Profile picker passed pictured part selection, Grade/Offset editing and Tab commit, pin/cancel/reuse, unpin/close/reopen, search/Escape and document selection/camera recovery. The controls remained usable after narrowing and restoring the document window. |
| VoiceOver and keyboard, #918 | Tested; failed | Actual VoiceOver read Browser/Projects, command fields, Parameters and its refused-length reason, and the inline fx menu. Edit Formula activation and Escape returned to the same inline field. The production Component Picker is reachable and readable, and closing it returns naturally to the document Part button. Moving from the first tile to the second loses the VoiceOver cursor and falls back to the window close button; reproduced twice and tracked in ready follow-up #1025. After the picker closes, cancelling Profile also leaves a dead Part-button reference; ready follow-up #1026 tracks that separate focus-return defect. This acceptance cell remains failed until both defects are corrected and retested. |
| Oldest supported runtime, #919 | Passed for `a6159618a` | The separate #919 closing record documents macOS 14.4 (23E214), arm64, fresh Qt-OFF and explicit Native-ON builds of frozen `a6159618a`, 11 passing bounded GUI runs and 32 reviewed original captures. That result belongs to its recorded source and producers. The manual candidate above was exercised on macOS 27.0.1. |

Operator testing uses isolated settings and synthetic designs and library components.
The maintainer supplied the physical trackpad input. VoiceOver scenario
preparation used ordinary OS keyboard and pointer events. Qt remains the default
macOS frontend; accepting an experimental candidate does not change that default
or establish the remaining parent acceptance items.

The #918 observations use actual VoiceOver commander commands and its emitted
last-phrase/cursor data, with owned foreground process checks around every
command. Scenario preparation and foreground reactivation between batches do
not count as VoiceOver navigation or focus-return evidence. These observations
do not claim human listening. The earlier `a6159618a` partial observations are
separate from this candidate.

The operator authorized temporary AppleScript control of VoiceOver. After the
operator completed macOS authentication, an independent Accessibility query
verified that the control checkbox was off and `NSWorkspace.isVoiceOverEnabled`
remained true. The owned test application was stopped and the shared GUI lock
was released. The bounded test work in #918 is complete with the failed result
and ready follow-ups #1025 and #1026 recorded above.


## VoiceOver follow-up acceptance on 1 November 2026

The bounded follow-ups #1025 and #1026 were retested with actual VoiceOver on
`03fbb2233642b5e23f97fdffcd768fd79264c0d8` from dev, which includes their
`3afbe0da8` integration. All 4,337 tracked build-mirror files were verified.
The macOS 27.0.1 arm64 build compiled both frontends with
`MITCAD_MACOS_NATIVE_UI=ON` without compiler warnings. Its application SHA-256
is `7eb704a73de0323834e295f3b811ffb8b1eed2da5918bf4e8e74a53a5c0f7d66`.
Runtime `--ui=native` and the startup log establish the AppKit test path.
Ordinary startup without a frontend selector separately reports
`Frontend: qt (default)` and passes normal command creation/cancellation.

| Check | Result |
| --- | --- |
| #1025: live picker tiles | Passed. Actual VoiceOver Right/Left reads Profile 20 test and Profile 30 test, retains their nonempty live bounds, and synchronises to the same keyboard-focused tile. Its selection-reading command reports one selected item in both positions. VoiceOver activation and Return choose the second component through the production handler. Closing/reopening, Search focus/Escape and Grade/Offset navigation remain usable. Actual Grade B activation limits Offset from 8 mm to 5 mm, which VoiceOver reads. |
| #1026: command retirement | Passed. Closing the picker returns naturally to the live Component button. Escape from that button announces the focused `3D view` group with nonempty bounds. The independent repeat never enters the tile collection. Direct cancellation from the command field and from the canvas also retains the live 3D view. No keyboard focus is forced after cancellation; subsequent Browser/view navigation remains usable. |
| Short #918 surface revisit | Passed. Browser rows and disclosure state are read; Return on the selected synthetic Projects design opens that design. Parameters reads the actual name, unit, expression and value, and Escape returns to the live document. The inline fx menu is traversed with actual VoiceOver, its Edit Formula action opens the formula editor, and Escape returns to the same live Width field. |

VoiceOver commander commands, emitted phrases, cursor text/bounds and owned
foreground process checks establish these observations. In-process test input
only prepares the synthetic scenarios; it is not counted as VoiceOver evidence.
One exploratory fx traversal entered Functions and then sent Return without
opening the editor. The retained corrected run uses VoiceOver's actual action
on Edit Formula and verifies the editor and its natural focus return. The
historical paired inventory and physical-input/runtime records above retain
their own sources; no full inventory, human listening or hardware rerun is
claimed here.

The native picker CTest passes in light/dark (134 steps per theme, 42.41 s),
and `app.unit` passes (6.65 s). Four original picker captures were individually
reviewed. The supplemental Qt Docked busy-preview Exit CTest fails before the
expected save question (23.84 s), separately retained in ready follow-up #1125;
that failure is not waived or reported as a passing Qt test.

The bounded follow-up results supersede the failed #1025/#1026 outcomes in the
31 October record; that historical result is preserved. Qt remains the default.
