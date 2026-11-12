# MitCad user guide

MitCad is an open-source parametric desktop CAD application for Windows,
Linux and macOS (Apple Silicon, macOS 14.4 or newer). It has the MIT
license. This guide tells you how to install MitCad and how to use it.
The chapters that follow describe the window, the commands, sketches,
features, components, files, libraries, import and export, outputs and
preferences; the contents below link to each of them. In MitCad,
**Help › User Guide** opens this guide on GitHub, in the version that
goes with your release (a development build opens the newest one).

The window has the toolbar at the top and the browser
on the left. The timeline is at the bottom and the command panel is on
the right. The orientation cube is in the top right corner of the view.
The toolbar has tabs (Solid, and Sketch in sketch mode) and groups
(Create, Modify, Assemble, Construct, Inspect, Insert, Select). A click
on a group's name opens a menu of all its commands. **S** opens command
search: type the start of a command's name and press Enter.

<!-- contents: written by tools/check-guide-contents.py --write after a heading changes; do not edit by hand -->
## Contents

- [Connect an AI assistant](#connect-an-ai-assistant)
- [Chat with an AI agent](#chat-with-an-ai-agent)
- [Work with an agent through comments](#work-with-an-agent-through-comments)
- [Installation](#installation)
  - [Windows](#windows)
  - [Linux](#linux)
  - [macOS](#macos)
- [Quick start](#quick-start)
- [The window](#the-window)
  - [The Start tab](#the-start-tab)
  - [Layouts](#layouts)
  - [Messages and their history](#messages-and-their-history)
  - [Quick access](#quick-access)
  - [View and navigation](#view-and-navigation)
  - [Selection](#selection)
  - [Command panels](#command-panels)
  - [Long computations](#long-computations)
- [Commands and shortcuts](#commands-and-shortcuts)
  - [Shortcuts](#shortcuts)
  - [The Solid tab](#the-solid-tab)
  - [The context menu](#the-context-menu)
- [Make](#make)
  - [The Make tab](#the-make-tab)
  - [Annotate](#annotate)
  - [Laser cutting](#laser-cutting)
  - [The selected body on the Make tab](#the-selected-body-on-the-make-tab)
- [Create features](#create-features)
  - [Extrude](#extrude)
  - [Thin Extrude](#thin-extrude)
  - [Emboss and Wrap Onto](#emboss-and-wrap-onto)
  - [Helix](#helix)
  - [Sweep](#sweep)
  - [Intersection Curve](#intersection-curve)
  - [Loft](#loft)
  - [Hole](#hole)
  - [Thread](#thread)
  - [Patterns](#patterns)
- [Modify features](#modify-features)
  - [Press Pull](#press-pull)
  - [Move Face](#move-face)
  - [Fillet](#fillet)
  - [Chamfer](#chamfer)
  - [Shell](#shell)
  - [Dogbone](#dogbone)
  - [Tangent Chain](#tangent-chain)
  - [Cutting across components](#cutting-across-components)
  - [Inspect: Measure, Annotations and Section Analysis](#inspect-measure-annotations-and-section-analysis)
  - [Physical Properties and Center of Mass](#physical-properties-and-center-of-mass)
- [Sketches](#sketches)
  - [Sketch mode](#sketch-mode)
  - [Sketch curve tools](#sketch-curve-tools)
  - [Snaps and auto constraints](#snaps-and-auto-constraints)
  - [Dimensions](#dimensions)
  - [Constraints](#constraints)
  - [Projections](#projections)
  - [Equal Spacing](#equal-spacing)
  - [Text, fonts and QR codes](#text-fonts-and-qr-codes)
  - [Selection in sketch mode](#selection-in-sketch-mode)
  - [Copy, Cut and Paste](#copy-cut-and-paste)
  - [Redefine Sketch Plane](#redefine-sketch-plane)
  - [3D sketches](#3d-sketches)
- [Browser, timeline and parameters](#browser-timeline-and-parameters)
  - [Browser](#browser)
  - [Units](#units)
  - [Timeline](#timeline)
  - [Timeline filter and search](#timeline-filter-and-search)
  - [Messages and their levels](#messages-and-their-levels)
  - [Find in Design](#find-in-design)
  - [Parameters and expressions](#parameters-and-expressions)
  - [Formula Editor](#formula-editor)
  - [Parameters as CSV files](#parameters-as-csv-files)
  - [Values that follow parameters](#values-that-follow-parameters)
- [Configurations](#configurations)
  - [1. When you need configurations](#1-when-you-need-configurations)
  - [2. Make the options](#2-make-the-options)
  - [3. Choose in the Configuration bar](#3-choose-in-the-configuration-bar)
  - [4. Add a row](#4-add-a-row)
  - [5. Add the rule](#5-add-the-rule)
  - [6. When a choice breaks a rule](#6-when-a-choice-breaks-a-rule)
  - [More in the Configurations window](#more-in-the-configurations-window)
  - [How MitCad applies rules](#how-mitcad-applies-rules)
- [Appearances and rendering](#appearances-and-rendering)
  - [Appearances](#appearances)
  - [Materials](#materials)
  - [Print settings](#print-settings)
  - [Where print items live](#where-print-items-live)
  - [Textures](#textures)
  - [The rendered view](#the-rendered-view)
  - [Lights](#lights)
  - [Render Image](#render-image)
  - [Render device](#render-device)
- [Components and assemblies](#components-and-assemblies)
  - [Components](#components)
  - [Design policy](#design-policy)
  - [Moving features and bodies](#moving-features-and-bodies)
  - [Joints and constraints](#joints-and-constraints)
  - [Dragging components](#dragging-components)
  - [Properties](#properties)
  - [Motion studies and contact sets](#motion-studies-and-contact-sets)
  - [Motion links](#motion-links)
- [Build tab](#build-tab)
  - [Profile](#profile)
  - [Panel and Linear Component](#panel-and-linear-component)
  - [Connect](#connect)
  - [Modify](#modify)
  - [Output](#output)
  - [Timber](#timber)
  - [Timber frames](#timber-frames)
- [More features](#more-features)
  - [Surface modeling](#surface-modeling)
  - [Patch, Stitch and Thicken](#patch-stitch-and-thicken)
  - [Plastic part features](#plastic-part-features)
  - [Placing a feature](#placing-a-feature)
  - [Gears](#gears)
  - [Pulleys](#pulleys)
  - [Gear Pair and Belt Drive](#gear-pair-and-belt-drive)
  - [Printable bearings](#printable-bearings)
  - [Boundary Fill](#boundary-fill)
  - [Bounding Solid](#bounding-solid)
  - [Envelope](#envelope)
  - [Convert Mesh](#convert-mesh)
  - [Sheet metal](#sheet-metal)
  - [Mortise and Tenon](#mortise-and-tenon)
- [Files and projects](#files-and-projects)
  - [Saving and recovery](#saving-and-recovery)
  - [Several designs at once](#several-designs-at-once)
  - [Versions](#versions)
  - [The Projects panel](#the-projects-panel)
  - [Sharing a project](#sharing-a-project)
  - [Live updates](#live-updates)
  - [Edit locks](#edit-locks)
- [Libraries](#libraries)
  - [Component libraries](#component-libraries)
  - [Component picker](#component-picker)
  - [Community library](#community-library)
- [Import and export](#import-and-export)
  - [File formats](#file-formats)
  - [Import](#import)
  - [Components from the web](#components-from-the-web)
  - [Image references](#image-references)
  - [Export](#export)
  - [Base features](#base-features)
  - [.f3d import](#f3d-import)
  - [Reading an imported timeline](#reading-an-imported-timeline)
  - [FreeCAD import](#freecad-import)
  - [.ipt import](#ipt-import)
  - [.iam import](#iam-import)
  - [IFC import](#ifc-import)
  - [.rvt import](#rvt-import)
  - [SolidCraft import](#solidcraft-import)
  - [DWG import](#dwg-import)
- [Outputs](#outputs)
  - [3D printing](#3d-printing)
  - [USDZ for AR and 3D viewers](#usdz-for-ar-and-3d-viewers)
  - [Drawings](#drawings)
  - [Web viewer](#web-viewer)
- [Preferences and diagnostics](#preferences-and-diagnostics)
  - [Preferences](#preferences)
  - [Language](#language)
  - [Cache of computed results](#cache-of-computed-results)
  - [Updates](#updates)
  - [Feedback and error reports](#feedback-and-error-reports)
  - [Debug log](#debug-log)
- [Command line](#command-line)
- [Terms](#terms)
<!-- end of contents -->

## Connect an AI assistant

With a design open, choose **Help › Connect AI Assistant…**, click the
assistant indicator, or choose **Connect AI Assistant…** in
**Preferences › AI Assistants**.

1. Choose the assistant: **Claude Desktop**, **Claude Code**,
   **ChatGPT / Codex** or **Other**. The dialog starts with the one chosen
   last, or else the first one installed on this computer. In its lines
   ChatGPT / Codex is called Codex (**Ready to connect Codex**).
2. Press the button beside the state line. The line names the next step
   and the button does it:

   | State line | Button | What to do |
   |---|---|---|
   | *Assistant* was not found on this computer | **Get Claude Desktop** or **How to install** | Install it from the page that opens, then come back. If it is installed after all, press **Connect anyway** (Claude Desktop, ChatGPT / Codex) |
   | Ready to connect *assistant* | **Connect** | MitCad sets the connection up |
   | Approve MitCad in Claude Desktop, then start a new chat | **Open in Claude Desktop again** | Approve the extension in Claude Desktop; the button opens it again if needed |
   | Install Claude Desktop first, in red | **Get Claude Desktop** | No application on this computer took the extension and Claude Desktop was not found (also after **Open Claude Desktop** found it gone): install Claude Desktop, then choose it again |
   | Installed in Claude Desktop, not connected: turn the MitCad extension off and on in Claude Desktop, or start a new chat | **Open Claude Desktop** | The extension is installed in Claude Desktop, but Claude Desktop is not connected to MitCad now. In Claude Desktop's **Settings › Extensions**, turn **MitCad** off and on again, or start a new chat; nothing needs to be installed again |
   | Install the extension in Claude Desktop | **Show in Explorer** (**Show in Finder** on macOS), and **Open Claude Desktop** | Claude Desktop is installed but does not open the extension by itself (Claude Desktop from the Microsoft Store). The line below says where: **In Claude Desktop: Settings › Extensions › Advanced settings › Install Extension…, then choose** the saved `mitcad-desktop.mcpb`, which the button shows (do not double-click it). Approve and enable the extension, then start a new chat |
   | Start a new Claude Code session to use MitCad / Restart ChatGPT or start a new Codex session to use MitCad | none | Start a new conversation in the assistant |
   | Reconnect MitCad in Claude Code (/mcp › mitcad › Reconnect) or start a new session | none | After a disconnect, run `/mcp` in Claude Code, choose **mitcad**, then **Reconnect**, or start a new session |
   | Restart ChatGPT or Codex, or start a new session | none | After a disconnect, restart the assistant or start a new session |
   | ✓ Connected: *assistant* | **Disconnect** | Work with the assistant; **Disconnect** keeps its setup, the design and unsaved changes |
   | Blocked in the assistant menu | **Allow** | The assistant is unchecked under **Incoming connections** in the assistant indicator's menu, or you pressed **Disconnect**: MitCad refuses its connections, also after a restart. **Allow** (or checking it again in the menu) lets it connect again; then use the step shown |
   | A short message saying what went wrong, in red | **Try again** | Do what the message says, then try again |

   Connected shows for the assistant that is connected, by its name
   (**Claude Desktop (chat)** and **Claude Desktop (Code tab)**, both
   under **Claude Desktop**, **Claude Code**, **Codex**; another assistant
   by the name it gives itself). With another assistant chosen, a quiet line
   names it (**Also connected: …**), and **Disconnect** then ends only the
   chosen assistant's connection and names it (**Disconnect Claude Code**;
   with **Other**, the assistant's own name).
   For **Other**, **Copy configuration** copies the settings to paste into
   your assistant's settings.
3. The line **Folder for designs:** names the folder the assistant can
   open designs from and save them to (with its subfolders). **Connect**
   (or **Connect anyway**, **Copy configuration**) allows that folder;
   **Change…** picks another first. Without an allowed folder, the
   assistant can only save a design that already has a file. The allowed
   folders are listed in **Preferences › AI Assistants**.

**Advanced** shows what sets the connection up by hand: the
**Configuration** (the settings to paste) and the **Command** that
registers MitCad (**PowerShell command** on Windows), each with its own
**Copy** button, and the **Codex settings file** with **Change…** (for
ChatGPT / Codex; a file other than `config.toml` takes the configuration
only). When several MitCad windows are open, **Copy
Command for This Window** gives a command that connects to this window
only. Saved assistant setups connect to the MitCad window active last when
the connection starts. Switching to another window later keeps the existing
connection with its original design. With several windows open, the state
line says **in this window** or names the other connected design; only the
connected window's assistant indicator shows the agent. The dialog checks
the installations and the connection every second.

Connect never changes unrelated settings: it backs up the assistant's
settings first, changes only MitCad's entry, and asks before it replaces
a connection to another MitCad installation. Settings managed by an
administrator are not changed. See [the MCP guide](mcp.md#connect-from-mitcad)
for the details of each assistant.

With the Linux AppImage, the assistant starts the AppImage file itself
(`MitCad.AppImage cli mcp --connect`). If you move or rename the AppImage
file afterwards, choose **Connect** again.

**The assistant indicator.** Beside the project indicator, the assistant
indicator shows whether AI assistants are connected to the window: **No
assistant** with a hollow dot (the window accepts them), **Claude Code
connected** with a colored dot (one assistant; **2 assistants connected**
with several), or **Assistants unavailable** with a struck-through dot
(the window cannot accept them, for example when MitCad was started with
`--no-mcp`). When you have blocked an assistant in its menu, the indicator
says so as well: **No assistant · 1 blocked** or **1 connected · 1
blocked**. It is in the title row of the Floating layout, in the status
bar of the Docked layout.
Its tooltip gives the details, with the window's instance id and port.

With no assistant connected, a click on the indicator opens **Connect AI
Assistant**; its arrow, or a right click, opens its menu. Otherwise a
click opens the menu. With assistants unavailable, the menu says why
first. The menu has two sections, with a checkmark and a state on each
row:

- **Incoming connections**: the assistants that work in MitCad, such as
  **Claude Desktop (chat)**, **Claude Desktop (Code tab)**, **Claude
  Code** and **Codex**: each one set up with **Connect AI Assistant**, and
  any other assistant that connected since MitCad started. Its state is
  **connected since** *time*, **not connected** or **blocked**. A checked
  assistant may connect. Uncheck it to disconnect it and refuse its
  connections; check it again to let it connect at once. The name and
  version the assistant gives itself are in the row's tooltip.
- **Outgoing connections**: the agents the chat panel can use (each
  Claude Code found, Codex, and the API agents and the custom command you
  turned on in **Preferences › AI Assistants**), with the state the chat
  panel shows. A checked agent is offered in the chat panel. Uncheck it to
  take it out of the panel's list: MitCad then never starts it.

The checkmarks are kept when MitCad restarts. Below them are **Connect AI
Assistant…**, with several MitCad windows open **Copy Command for This
Window** (a command that connects to this window only), and **Folders
Assistants May Use…** (**Preferences › AI Assistants**).

## Chat with an AI agent

**View › Chat** (Ctrl+Shift+L, ⌘⇧L on macOS, or its button beside the
assistant indicator) opens the chat panel: write what you want, for
example "a 30 × 20 × 10 mm box with the top edges rounded 2 mm", and your
own Claude Code or Codex changes the open design through MitCad's tools
while you watch. The panel finds an installed agent by itself; when it is
missing, **Install Claude Code** (**Install Codex**) opens its setup page,
and when it is not signed in, **Sign in to Claude** (**Sign in to
ChatGPT**) starts its own sign-in.

- With both agents installed, the header chooses between them; the other
  one starts a new conversation and stays chosen next time. **Model**
  below it sets the model the agent uses (**Default model**, the ones the
  agent names, or a name you type), from the next message on.

- **Enter** sends, **Shift+Enter** starts a new line; **Selection** (on
  by default) and **View** say what goes with the message. While the
  agent works, the button is **Stop** (Esc).
- The agent's calls are rows in the conversation; calls in a row between
  two of its sentences are one row (**3 calls (1 failed) · *the latest
  call***) that opens to them. Saving, opening,
  exporting and undoing what was there before the message ask first with
  a card: **Allow once**, **Always allow** or **Deny** (**Preferences › AI
  Assistants › Chat** sets what asks).
- Each message is one undo step, **AI: *its first words***: one **Undo**
  takes it back. If you change the design yourself meanwhile, the
  agent's changes stay separate steps.
- The agent can start a new design from the open one ("make a nipple
  with this thread at both ends"): it makes an empty design as **File ›
  New Design** does, in a tab of its own beside the open design (in the
  current project's top folder as **Design*N***, in a folder for designs
  under a name, or untitled), and builds in it; the open design stays as
  it is, and the conversation stays in its tab.
- The conversation is kept with the design in your own data folder (not
  in the design file) and shown again when you open the design.
- Without an agent program, the chat can use the **Anthropic API** with
  your own key, a local model in **Ollama** or **LM Studio**, or another
  OpenAI-compatible endpoint: check it under **Preferences › AI
  Assistants › Chat › API endpoints** and choose it in the panel's header.
  The key is kept in the system's keychain; a local server that is not
  running shows **Get Ollama** (**Get LM Studio**), and a model that
  cannot call tools is reported in the panel.
- An agent program of your own that speaks the Agent Client Protocol can
  be the chat's agent too: check **Custom command** under **Preferences ›
  AI Assistants › Chat › Custom agent**, give its program and arguments,
  and choose **Custom command** in the panel's header. MitCad starts it
  as it is (no adapter is installed); a command that does not answer the
  protocol's handshake is reported in the panel's state line.

If the account cannot use the service (organization restrictions, expired
sign-in, invalid API key or a credit/rate limit), the error stays in the
state line. **How to fix** opens help in the panel; after fixing the
account, **Check again** checks sign-in and one short request without your
design or conversation. **New conversation** (+) starts a fresh agent
process with the latest sign-in. For Claude Code and Codex, **Sign in
Again…** beside **Check again** opens a terminal window that signs the
agent out and in again with its own commands (`claude auth logout`, then
`claude auth login`; `codex logout`, then `codex login`). It first says
what will happen: signing Claude Code out also signs Claude Desktop out,
as they share one sign-in, and signing in again with an account whose
organization has disabled access does not fix that. When you are done in
the terminal, MitCad runs **Check again** by itself. Without a terminal
program, **How to fix** shows the commands with **Copy**.

See [the chat panel](ai-chat.md) for the setup, the approvals, privacy
and the history.

## Work with an agent through comments

A comment is a note pinned to the place it is about: a face, an edge, a
body, a point on a face, a feature, a sketch or a user parameter. Pin
"make this hole an M6 thread" on the hole, "this fillet is too small" on
the fillet, and an AI agent (the chat panel's or one connected over MCP)
finds the open comments, does the work and marks each one addressed with
a note of what it did; you check it and resolve the comment. Comments are
saved with the design, so they wait for the next session.

1. **Add a comment.** **Add Comment** (Shift+N, the Solid tab's Inspect
   group, or the view's context menu on a face, edge, body or point)
   opens a small panel: click what the comment is about in **On** (a
   face, an edge, a vertex, a body, a shown sketch's curve or point, or a
   construction plane, axis or point), write the text in **Comment** and
   press Enter. **At the Point Clicked** pins it to the spot you clicked
   on a face or an edge instead of the whole face or edge. A feature's or
   a sketch's row in the browser or the timeline, a body's row, and a
   user parameter's row in **Edit Parameters…** have **Add Comment…** in
   their context menus too.
2. **The agent addresses it.** Each comment has a numbered pin in the
   view: open ones in full color, addressed ones dimmed. The agent reads
   the open comments, makes the change and writes what it did as the
   comment's resolution, in the same undo step as the change.
3. **You resolve it.** **View › Comments…**, or a click on a pin, opens
   the Comments panel: the comments with their number, text, author,
   what they are on, status and the agent's resolution, and the replies
   under them. **Open**, **Addressed** and **Resolved** choose what it
   lists. A click on a row selects the comment's target and frames it.
   **Resolve** when the work is right (its pin goes), **Reopen** with a
   reply when it is not, **Reply…** to add to it, **Delete** to remove
   it. Every change is one undo step.

A comment whose target is gone (the face was removed by a later change)
keeps its pin at its last place with a warning look; select a new target
and **Retarget** it. **View › Comment Pins** (or its button in the
navigation bar) hides or shows all pins.

By default only you resolve comments: an agent can add comments of its
own, reply and mark them addressed (with a note of what it did), but its
resolve is refused, so every change an agent makes waits for your check.
**Preferences › AI Assistants › Agents may resolve comments** lets an
agent resolve them itself, without marking them addressed first.

## Installation

MitCad runs on Windows, Linux and macOS. On macOS it needs Apple Silicon
and macOS 14.4 or newer. The website is
[mitcad.org](https://mitcad.org/). The downloads are on
[the releases page](https://github.com/mitcad/Mitcad/releases) on
GitHub. MitCad is under active development. A release comes as a Windows
installer, a Linux AppImage and a macOS disk image.

Each release on GitHub has these files:

| File | Content |
|---|---|
| `mitcad-<version>-windows-x64.exe` | The Windows installer |
| `mitcad-<version>-windows-x64.zip` | The portable `.zip` for Windows |
| `Mitcad-<version>-x86_64.AppImage` | The Linux AppImage |
| `Mitcad-<version>-macos-arm64.dmg` | The macOS application disk image |
| `update-manifest.json` | The signed manifest for automatic updates |

MitCad checks once a day for a signed release and offers to install it.
The user or an administrator can set the checks to off. These
installations update themselves:

- The installation of the Windows installer
- An AppImage in a writable folder
- A macOS application bundle in a writable folder

Other installations only announce a new release:

- The portable `.zip`
- Unbundled development builds
- System packages
- Read-only installations
- On macOS, an app in a read-only disk image, on an App Translocation
  path or quarantined in Downloads

The startup log gives the reason. On macOS, copy the app to a writable
Applications folder first. MitCad does not ask for elevation to replace
a bundle that the current user cannot write. Refer to
[Updates](#updates) and [updates.md](updates.md).

### Windows

MitCad needs OpenGL 2.1 or newer on Windows. The installer runs as the
user. It installs in one of two scopes:

- **just for me** (the default): into `%LOCALAPPDATA%\Programs\Mitcad`,
  with the Start menu shortcut, the `.mitcad` file type and the
  uninstall entry under `HKCU`. This scope does not need administrator
  rights, so there is no UAC prompt.
- **for all users**: into `Program Files\Mitcad`, registered under
  `HKLM`. Only this choice asks for elevation: the installer starts
  itself again elevated.

The page *Choose Users* after the license offers the choice. It starts
on *For all users* only when MitCad is installed for all users and not
for the current user. The installer first uninstalls an earlier version
in the same scope. It runs that version's own uninstaller silently, in
place.

A silent install uses these switches:

| Switch | Effect |
|---|---|
| `/S` | Installs just for the current user |
| `/S /AllUsers` | Installs for all users |
| `/CurrentUser` | Says the default (just for the current user) explicitly |
| `/EndPrograms` | Ends the programs that run from the installation's `bin` folder in your session, except MitCad itself, instead of stopping with exit code 3. The updater passes it after you chose End Them and Install |
| `/D=<dir>` | Sets the folder in both scopes. Put it last, without quotes |

A silent install whose `/D` folder is the folder of the installation
for all users stays for all users. The updater of an earlier release
runs such an install.

The installer writes the scope into `<dir>\installation.ini`
(`[Installation]`, `Scope=user` or `all`). The uninstaller and the
automatic updates read this file. An installation without the file is
from an earlier release, which was for all users. The uninstaller
removes only the files and the keys of its own scope. For all users it
elevates as the installer does.

The installer has these exit codes:

| Exit code | Meaning |
|---|---|
| 0 | Done |
| 1 | Canceled |
| 2 | Aborted (also when the installer could not uninstall the earlier version) |
| 3 | Programs from the installation are running (silent install). Nothing was changed. With `/EndPrograms`: programs it could not end, such as MitCad |
| 740 | Elevated, but still without administrator rights |
| 1223 | Elevation refused |

The portable `.zip` has the same files as the installer. It does not
update itself. It only announces a new release.

### Linux

The AppImage is built on Ubuntu 24.04 and needs glibc 2.39 or newer:
Ubuntu 24.04 or newer, or a distribution based on them. It brings its
own Qt and does not use the system's.
Without FUSE, run it with `--appimage-extract-and-run`. At each start
the AppImage writes its desktop file and its icons into `$XDG_DATA_HOME`
if they differ, so docks show the icon. In the desktop file, `Exec` is
the AppImage and `StartupWMClass=Mitcad`. The AppImage has no GTK. Thus
it uses the `xdgdesktopportal` platform theme unless
`QT_QPA_PLATFORMTHEME` is set. An AppImage in a writable folder updates
itself. `MitCad.AppImage cli <arguments>` runs the bundled `mitcad-cli`
([Command line](#command-line)), for example
`MitCad.AppImage cli --version`. The AppImage's OpenSSL uses the
system's OpenSSL configuration (`/etc/ssl/openssl.cnf`, the
distribution's crypto policy) and certificates. If that configuration
does not load in it (an option only the distribution's own OpenSSL
knows), MitCad uses the AppImage's own minimal configuration for its
connections (update checks, downloads) instead, and its log says
`TLS: the system's OpenSSL configuration failed to load (…); using the
AppImage's own`.

### macOS

MitCad needs Apple Silicon, macOS 14.4 or newer and OpenGL 4.1 core.
Apple has deprecated OpenGL, but it is present. The release is an
application disk image (`.dmg`). Copy the app to a writable
Applications folder. An app in a writable folder updates itself. An
app that runs from the read-only disk image, from an App Translocation
path or as a quarantined bundle in Downloads only announces a new
release. The startup log gives the reason.

## Quick start

This procedure makes a body from a sketch and gives it a fillet.

1. Select **Create Sketch** (Alt+S, or command search S). Click the XY
   plane. The view turns to the plane.
2. Press **R** (2-Point Rectangle). Click the first corner at the origin.
   Type the width `40`, press Tab, type the height `25` and press Enter.
   Typed values become dimensions and parameters (`d1`, `d2`).
3. Press **Ctrl+Enter** to finish the sketch.
4. Press **E** (Extrude). Extrude takes the latest profile. Type `20` and
   press Enter. You can also drag the arrow in the view.
5. Press **F** (Fillet). Click the top face. A face is all its edges.
   Type the radius and press Enter.
6. To change a dimension, open **Edit Parameters** (in Modify or in the
   Edit menu). Double-click a value and type `50` or the expression
   `d2 * 2`. MitCad recomputes the design, and the fillet stays on the
   same edges.
7. To edit a feature, double-click it on the timeline. A sketch opens in
   sketch mode. An edit is one undo step (Ctrl+Z).

You can also open a `.f3d` or `.f3z` design with **File › Open** (or
Import). Refer to [.f3d import](#f3d-import) for the import, its
progress dialog and its report.

## The window

The window has the toolbar at the top, the Browser on the left, the
timeline at the bottom and the command panel on the right. The
orientation cube is in the top right corner of the view. The sections
that follow describe the layouts of the window, the view, selection,
the command panels and long computations.

MitCad's version shows in two places: in the window title (for
example `Robot.mitcad - MitCad 0.42.0`, so the taskbar and the window
switcher show it too) and in small, faint text at the lower right of the
view, just above the timeline. The text in the view does not take the
mouse: a click, a drag or the wheel there works in the view. It moves
up, or to the left of a panel along the right edge, so that it never
covers a card or a button, and it is hidden when there is no room. It
is not in pictures of the view (Export Snapshot, screenshots,
thumbnails, renders). To hide it, turn off **Show the version in the
view** in Preferences › General; the window title keeps the version.

To make everything in the window larger, choose **Preferences › Display ›
Interface size**: 100% (the default), 125%, 150%, 175% or 200%. Icons,
texts, the toolbar, the panels, the floating cards, the dialogs and what
MitCad draws over the 3D view (the orientation cube, the navigation bar,
handles) grow together, and the 3D view draws at the higher resolution.
The size takes effect when MitCad starts again. It comes on top of the
display scaling set in your system.

To make only the texts or only the icons smaller or larger, move the
**Text size** and **Icon size** sliders under it: 80% to 200% in steps of
5%, on top of the Interface size. Icon size covers the toolbar, the
Browser and the panels, and also the icons in menus, drop-down lists and
text fields (on macOS the menu bar's menus keep the system's size). **Font** chooses the interface's font:
**System** (the default) or any font installed on the computer, at the
Text size. These too take effect when MitCad starts again.

On macOS, the keys that say Ctrl in this guide are Cmd keys (Cmd+N,
Cmd+S, Cmd+Z and so on). Refer to [Shortcuts](#shortcuts) for the
other keys that differ on macOS.

### The Start tab

**Start** is the first tab of the toolbar, before Solid, Build and Make.
MitCad starts on it, and you can go back to it at any time. Its page
covers the view:

- **New Design** makes an empty design in the units for new designs
  (Preferences › General). **Open Design…**, **Open Project…** and
  **Import File…** do what the File menu's Open, Open Project and Import
  do. Each card shows its keyboard shortcut.
- Below them, your designs in two columns. On the left, **Recent** and
  **Pinned**, then under **Projects** the projects of the Projects panel,
  each with its icon (a Cloud project shows whether it is synced), and
  **Add project**, which starts New Project. The row you chose is
  highlighted, and MitCad shows it again the next time it starts.
- **Recent** shows the files of File › Open Recent as cards: a preview of
  the design, its file name, its type, its folder and when you last
  opened or saved it. Click a card to open the design. The star at the
  top right of a card pins the design; **Pinned** shows only the pinned
  designs. A pinned design stays on the list when ten newer files have
  pushed it out of Open Recent. A file that is no longer where it was
  shows as missing: **Remove** takes it off the list. Right-click a card
  to open, pin or remove it.
- Click a project to browse it: the path at the top (the project, then
  each folder; click a part to go back up), its folders as buttons (click
  one to go in), and its designs as cards with their preview, name, folder
  and when they last changed. Click a card to open the design; it then
  shows under Recent too. A design that has no preview on this computer
  shows the icon of its type: MitCad downloads nothing to make one.
- Type in **Search designs in all projects** to find designs by part of
  their name, in every project and among the recent designs. Each result
  names its project and folder, so designs with the same name tell apart.
  Clear the field to go back to the row you chose. The two buttons beside
  the field show the designs as cards or as a compact list.
- Drop a CAD file anywhere on the page to open it, or to import it into
  the design, as on the window.
- **Resources**: **Component Libraries** opens the component picker,
  **User Guide** this guide, **Keyboard Shortcuts** the Keyboard and
  Mouse Overview and **Connect AI Assistant** the assistant's setup
  ([Connect an AI assistant](#connect-an-ai-assistant)).
- **What's New in** and the version, at the top right, opens the release
  notes of your version in the web browser. **Report an Issue** at the
  bottom opens MitCad's issue list, **Preferences…** the Preferences.
- Clear **Show start page at launch** at the bottom to start on an
  empty design instead. The Start tab stays in the toolbar.

A save makes the preview: a picture of the view as it is when you save.
MitCad keeps the previews in its own data folder, not in your designs or
their folders. A design that was never saved since this version, or a
file you only imported, shows the icon of its type instead.

When you open a design, create a new one, import a file or start a
command of the view (a key such as E for Extrude, a button of another
tab), MitCad goes to the Solid tab. When MitCad starts with a file (a
double-click on a design, `--open`), it opens on the Solid tab. If MitCad
ended without closing, the question about the unsaved designs comes
first, over the Start page ([Saving and recovery](#saving-and-recovery)).

### Layouts

The **Layout** setting on the Display page of Preferences selects how
MitCad arranges the window:

- **Floating** (the default): the panels are cards over a full-window
  3D view.
- **Docked**: the panels are in docks around the 3D view. A right-click
  on a dock's title or on the toolbar opens a menu that shows and hides
  the panels; the toolbar with its tabs always stays.

Tools › Preferences has a page for each kind of setting. Refer to
[Preferences](#preferences).

The layout takes effect when MitCad starts. After a change, MitCad asks
if you want to restart now. Restart Now closes the window as a quit
does and starts MitCad again with the design open. If the design has
unsaved changes, MitCad saves it or asks about it first. Cancel keeps
MitCad open. Later keeps the choice for the next start. On the command
line, `--chrome=docked|floating` overrides the setting for one run.

**On macOS.** MitCad has the same interface as on Windows and Linux.
The menus are in the menu bar at the top of the screen; **Show the menu
bar in the window** on the Display page of Settings shows them under
the title bar too (off by default).

**Hide the menu bar (Windows and Linux).** Clear **Show the menu bar** on
the Display page of Preferences to give the view the menu bar's height,
in either layout. The menus are then under the ☰ button at the left, before
Undo and Redo (in the title bar row of the Floating layout, at the start
of the toolbar in the Docked layout). Press and release Alt to show the
menu bar and go to its File menu, or press Alt with a menu's underlined
letter (Alt+F for File). The menu bar goes away again when its menu
closes or when you press Esc. The menus' shortcuts work while the bar is hidden. Select **Show
the menu bar** to show it again.

In the Floating layout only the menu bar is a bar at the top (on macOS
the system's menu bar), and the 3D view starts directly below it. The
title bar row floats over the top of the view without a card. Its
controls and the design's name each have a small box of their own.
Each of the toolbar's groups is a card of its own in a row below the
title bar row. A click, a drag or the wheel between and beside these
cards works in the view.

The other cards always start below the bar and the floating rows, at
any window size. The parts of the Floating layout are:

- **Title bar row:** Undo and Redo, the workspace tabs as a segmented
  control, the quick-access group (see [Quick access](#quick-access)),
  the design's name and the command search. Under it the toolbar's
  groups are capsules. A click on
  a group's name opens a menu of all its commands. In a narrow window
  the groups at the right show only their names.
  Finish Sketch is in the toolbar and the Sketch Palette while editing a sketch.
- **Cards:** the Browser (upper left, collapsible), the Command panel
  (upper right, only while it has content to show) and the Timeline
  (along the bottom) float over the 3D view. The Projects panel, when
  shown, is a card above the Browser, and the Configuration panel, while
  the design has options, a card between them. View › Browser, View ›
  Timeline and View › Projects show and hide them. The navigation bar
  (Look At, Fit, Visual Style, Layout Grid, Snap to Grid; see
  [View and navigation](#view-and-navigation)) is a small card at the
  bottom center, above the Timeline. Fit and Home put the
  design in the area that the cards leave free. A click beside a card
  reaches the view.
- **The left cards** share the height down to the Timeline: each is as
  high as its content while there is room, and the others get the rest.
  Drag the gap between two cards up or down to give one of them more;
  MitCad keeps that split for the next start. Double-click the gap to go
  back to the automatic split. A collapsed card gives its height to the
  others.
- **Browser background** (Preferences › Display, in this layout only):
  **Card** puts the Browser on its card. **Rows** leaves the card out.
  Then each row, and the Browser's header, has a small box of its own,
  as wide as its content. The design shows between and beside the rows.
  A click, a drag or the wheel there works in the view (select, orbit,
  zoom).

  Only the boxes belong to the Browser. Its scroll bar is at the left,
  beside the rows. The change applies at once.
- **The Browser's height** follows its rows. The Browser grows as
  folders open, down to the Timeline, and only then scrolls. It shrinks
  as folders close.
- **Messages** show in a band right above the Timeline, as wide as it.
  Refer to [Messages and their history](#messages-and-their-history).
  Update and remote notices show in a card under the toolbar's groups. MitCad does not show the remote's
  state. The remote's commands are in File › Project.
- **Settings** on macOS (MitCad › Settings, Cmd+,) is a window with the
  same pages as Preferences elsewhere, in the same list. Changes apply at
  once and are saved automatically. There is no OK. After each change the
  note *Changes are saved automatically* shows under the page for a few
  seconds. Typed cache budgets apply when editing finishes; arrow buttons
  apply immediately. Camera and background controls follow changes made
  through View while Settings is open. Cmd+W closes the active tool window;
  in the design window it closes the design. The background can follow
  the system's appearance.

In the Docked layout the panels are docks: the Browser, the Timeline, the
Command panel and, when shown, the Projects, Configuration and Chat
panels. To hide a dock, click the × on its title row (it shows while the
pointer is over the row), or right-click a dock's title row and clear the
dock's name in the menu. To show it again, choose it in the View menu:
View › Browser, View › Timeline, View › Projects, View › Command or
View › Chat. The Configuration panel shows while the design has options
and cannot be hidden.

### Messages and their history

What MitCad tells you (a step of a command, a fact, a warning, why
something failed) is a message. Each message has a level: an error, a
warning, information or a hint. An error and a warning have their icon
and their word (*Failed*, *Warning*) in their color.

In the Floating layout the messages show in the **message band** right
above the Timeline, as wide as the Timeline. The band shows the whole
text on up to two lines; the tooltip has the rest of a longer one.

- A hint or information shows for 6 seconds, a warning for 10 seconds.
- An error stays until you close it with **×** or start the next
  command (a command with a panel, a sketch tool, or an edit of a
  feature). An action such as Undo or a view leaves it.
- A hint does not replace an error or a warning that shows. The hint
  goes to the history.
- When a message goes away, a dimmed line with the latest message
  stays in the band's place, so the cards above the band do not move.
  Click it to open the history.
- A new or another design (New, Open, Open Recent) empties the band:
  the previous design's message, an error too, and its dimmed line go.
  The history still has them.
- A click that picks nothing, such as a click beside the edges of a
  drawing view, is a hint that fades, not an error.

The **message history** lists this session's messages, the newest
first, with their time, the level's icon and the text. Click a message
about a feature (from the "n failed" summary, for example) to select
that feature on the Timeline and in the Browser. Esc or a click
elsewhere closes the history. MitCad keeps the history only while it
runs; it does not save it.

In the Docked layout the messages show in the status bar until the
next message, with the level's icon, or until another design is new or
opened. The history button at the right of the status bar opens the
same history.

### Quick access

The quick-access group holds one-click buttons for the commands you use
most. In the Floating layout it is in the title bar row, after the
workspace tabs. In the Docked layout it is in the toolbar's row of tabs,
after the tabs. It stays on every tab, in sketch mode and while a drawing is open.

By default the group has New Design…, Open…, Save, Print…, Design
Properties… and Preferences… (MitCad › Settings… on macOS), when your
version of MitCad has them. The tooltip of a button names the command and
its shortcut. A command that you cannot use now is shown dimmed, and its
tooltip tells why. For example, Save is dimmed while the design has no
unsaved changes.

To choose the commands, click the ⋮ at the end of the group, or
right-click the group. The menu lists the commands of the File, Edit,
View and Tools menus. Each command with a check mark is shown in the
group, in the order of the menus. Click a command to add it to the group
or to remove it. **Reset to Default** brings back the default set.
MitCad keeps your choice for the next start.

When the title bar row is too narrow for the group and the whole name of
the design, the group shows only its ⋮. Its menu then lists the group's
commands first.

If you do not need the group, you can hide it. Choose **Hide Quick-Access
Toolbar** at the end of the group's menu, or clear **Show quick-access
toolbar** in Tools › Preferences › Display (MitCad › Settings… › Display
on macOS). The group, its ⋮ too, goes away at once, and the row closes up.
To show it again, select **Show quick-access toolbar**. The group comes
back with the commands you chose before. To find the setting, you can also
search for "quick access".

### View and navigation

The mouse scheme sets how you pan, orbit and zoom. Tools › Preferences
selects the scheme. The schemes are:

- Default: the middle mouse button + drag pans. Shift + the middle
  button orbits. The wheel zooms at the cursor.
- Alt with the left, middle or right button: orbit, pan and zoom.
- The middle button pans, and F4 + the left button orbits.
- The middle button orbits, Ctrl + the middle button pans and Shift +
  the middle button zooms.
- The right button orbits, and the middle button pans.
- **Trackpad** (the default on macOS): a two-finger scroll pans.
  Alt (Option) + scroll orbits. Shift or Ctrl (Cmd) + scroll zooms.
  Alt + left drag and the right button orbit. Shift + Alt + left drag
  and the middle button pan. A right click, or Control + click on a
  Mac, still opens the context menu.

In every scheme a pinch zooms about the fingers. A two-finger rotation
rolls the view (not with a constrained orbit). A two-finger double tap
fits everything. A two-finger orbit has a 1.3× motion gain. Pan, zoom
and mouse drags keep their own sensitivity.

An orbit with the mouse or the cube goes on past the edge of the screen:
the cursor continues from the opposite edge of the same screen and the
view keeps turning, so a long orbit needs no second drag. Where the
system does not let an application move the cursor (Wayland), the orbit
stops at the edge.

The **orientation cube** turns the view. A click on a face, an edge or
a corner of the cube turns the view there. Drag the cube to orbit. The
house returns to the home view. The right button opens the cube's menu:
Home, Camera (Orthographic, Perspective: the camera's projection, as in
View › Camera), Set Current View as Home, Reset Home, Fit.

The cube is dark glass over a dark background and frosted light glass
over a light background. Its face names are printed on it. The edges and
corners are wide bevels between the faces, so that the edge and corner
views are easy to hit. The faces, edges and corners light up in the
accent color under the pointer. The
house is the small capsule above the upper left corner of the cube.

The **arrow keys** turn the view while the view has the keyboard: click
in the view first. Left and Right turn the view round to the left or
right side of the design, and Up and Down round to its top or bottom, as
the arrows of the orientation cube do, but by a step. The step is 15° by
default. Set it in **Arrow key step** on the Navigation page of Tools ›
Preferences. The view turns about the same center as a mouse orbit, and
the zoom does not change. The keys also turn the view in a sketch, except
Up while a drawing tool of a 3D sketch other than Sketch Fillet is
active: there it switches
between On Plane and Normal ([3D sketches](#3d-sketches)). While
the keyboard is in a value or text field, the Browser, a list, a panel or
a dialog, the arrow keys do what they always do there. To use other keys,
change **Turn View Left**, **Turn View Right**, **Turn View Up** and
**Turn View Down** in Tools › Keyboard Shortcuts.

The **Home** key shows the home view and the keys **1** to **7** the
standard views of View › Standard Views, while the view has the keyboard:
1 Front, 2 Back, 3 Left, 4 Right, 5 Top, 6 Bottom and 7 Isometric. They
work in a sketch and while a command's panel is open too. While the
keyboard is in a value field, a digit is typed into the field and Home
moves its text cursor; a number typed over the view while a value box
shows at the cursor goes into that box. To use other keys, change
**Home** and **Front View** to **Isometric View** in Tools › Keyboard
Shortcuts.

**Preferences › Display › Orientation cube** sets how the cube is drawn:

- **Follow the View** (the default): as the camera draws the design. With
  a perspective camera the cube is in perspective too, and in its corner
  you also see the sides that face the middle of the view.
- **Orthographic**: always orthographic, so it keeps its square shape in
  the corner, also with a perspective camera.
- **Perspective**: always in a perspective of its own, as if you looked
  straight at it, also with an orthographic camera.

The cube keeps its size and place in the corner with each choice, and a
click on it turns the view to what you clicked.

**View › Anaglyph 3D** shows the view in stereo for red/cyan 3D glasses
(the red filter over the left eye). It is off by default; select it
again to turn it off. While it is on, the camera is a perspective one;
when you turn it off, the camera goes back to Orthographic or
Perspective as it was before. Choosing **Orthographic** (View › Camera,
the cube's menu or Preferences) turns Anaglyph 3D off. Selecting,
highlights, handles and the orientation cube work as usual. The
**Rendered** visual style is not available while Anaglyph 3D is on:
turning Anaglyph 3D on leaves the rendered view. **Export Snapshot**
saves the normal perspective picture, without the red/cyan colors.
Anaglyph 3D is a user preference; it does not change the design.

Two settings in **Preferences › Display › Anaglyph 3D** set its depth.
The view shows a change at once:

- **Stereo depth** (1–20 %, default 5 %): how far apart the two eyes
  are. More gives more depth; too much is tiring to look at.
- **Convergence** (25–400 %, default 100 %): the distance that lies in
  the plane of the screen, in percent of the distance to the middle of
  the view. What is nearer than it stands out of the screen, and what is
  farther lies behind it.

**Preferences › Display › Small circles** sets how finely the view draws
small holes, circles and arcs:

- **Adaptive** (the default): a small circle gets fewer segments, by its
  size compared with the body, but always at least 12 for a full circle,
  so that small holes still look round. Large circles are drawn as in
  Full Detail. A design with thousands of small holes has far fewer
  triangles to draw, so it opens faster and takes less memory.
- **Full Detail**: every circle as finely as a large one, as in earlier
  versions. Zoomed in close on a small hole, it looks smoother.

When you change it, MitCad draws the open designs again with the new
choice. It changes only what the view shows: exported meshes (STL, 3MF,
OBJ, Send to Slicer) and the model itself stay the same.

The **View menu** has the standard views, Look At, Fit, the camera, the
display styles, the background, Grid and Snaps, and the named views.
The display styles are Shaded, Shaded with Visible Edges Only, Shaded
with Hidden Edges, Wireframe and Wireframe with Hidden Edges. Named
views are stored in the design. MitCad remembers the settings.

**View › Grid and Snaps › Grid Settings…** sets the spacing of the
layout grid. With **Automatic** (the default) the spacing follows the
zoom, in steps that suit the unit of the design:

- mm, cm or m: 1 or 5 times a power of ten mm. Every fifth line is a
  darker major line.
- in: 1/16, 1/8, 1/4, 1/2, 1, 2, 5, 10 in and on. Below an inch every
  fourth line is a major line, so the major lines fall on quarters,
  halves and whole inches.
- ft: the inch steps up to 10 in, then 1, 2, 5, 10 ft and on.

Type a **Fixed spacing** in the unit of the design, for example 0.25 in.
MitCad keeps it as a length, so a change of the design's unit does not
move the grid: 0.25 in shows as 6.35 mm in a design in mm. With Snap to
Grid, sketch points snap to the grid lines that the view shows.

View › Visual Style › Rendered shows the bodies path traced: refer to
[The rendered view](#the-rendered-view) for the render environment, the
lights and Render Image.

To see the parts inside a body or a component, make it transparent:
**Transparent** in its context menu, in the view or in the Browser. In
the shaded styles it shows as a faint surface with its edges, and a
click goes through it to the parts behind it. Refer to
[Browser](#browser).

The **navigation bar** at the bottom of the view holds the View menu's
commands you use all the time:

- **Look At** turns the view straight at the selected plane, planar
  face or sketch. In a sketch it looks at the sketch plane. It is also in
  the context menu of a planar face, a plane and a sketch: refer to [The
  context menu](#the-context-menu).
- **Fit** (F6) fits the design into the view.
- **Visual Style** opens a list of the display styles. The style in use
  is checked.
- **Layout Grid** shows or hides the grid.
- **Snap to Grid** turns grid snap in sketches on or off.
- **Edit Parameters**, after a line at the end of the bar, opens the
  parameter table in every workspace (Solid, Build, Make and the others)
  and in a sketch, so that you do not have to go to Solid › Modify for
  it. Refer to [Parameters and expressions](#parameters-and-expressions).
  While a drawing is open, the same button is in the drawing's bar,
  after **Add Revision…**. It is the first of the commands pinned to the
  bar, and you can unpin it like the others.

**Your commands on the bar.** You can pin any command to the end of the
navigation bar, so that it is one click away in every workspace and in
sketches: for example **Measure** from Solid › Inspect while you work in
Build. Open the command's row menu in a group's menu (the ⋮ button at
the right of its row, a right-click on the row, or the Menu key) or
right-click a command you pinned to the toolbar, and choose **Pin to
Navigation Bar**. The command shows after the line at the end of the bar,
with its icon, its tooltip and its shortcut, and it is unavailable when
the command is. Drag a pinned command along the others to put it
elsewhere. Right-click it for **Unpin from Navigation Bar** and **Change
Keyboard Shortcut…**; the row menu has **Unpin from Navigation Bar**
too. MitCad keeps the pinned commands across sessions. Refer to
**Pinned commands** in [Commands and shortcuts](#commands-and-shortcuts) for the toolbar.

The view buttons are the View menu's own commands: a change in the bar shows in the
View menu, and a change in the View menu, by a shortcut or in the Sketch
Palette shows in the bar. Layout Grid and Snap to Grid show checked while
they are on. Hold the pointer over a button to see its command and its
shortcut.

In the Floating layout the bar is a small card at the bottom center of
the view, above the Timeline. In the Docked layout its buttons are at
the right end of the status bar, at the bottom of the window. It shows in every workspace and in sketches. While a drawing takes
the view, the bar is hidden with the other cards. To hide the bar, clear
**Show the navigation bar** on the Display page of Tools › Preferences.

#### 3D mouse

A 3D mouse (a 6-DOF device such as a SpaceMouse or a Space Navigator)
moves the view. Orbit, pan and zoom work together. The **Navigation
mode** sets what follows the cap:

- **Object** (the default): the design follows the cap. Push the cap to
  the side or lift it, and the design moves the same way. Pull the cap
  toward you, and the design comes closer; push it away to zoom out.
  Tilt, twist or roll the cap, and the design turns the same way, about
  the same center as the mouse's orbit.
- **Camera**: the camera follows the cap. Push the cap to the right, and
  the camera moves right (the design goes left). Push the cap away, and
  the camera moves forward: in perspective it moves toward the design, in
  orthographic it zooms in. Tilt or twist the cap, and the camera turns
  the same way about the orbit's center.
- **Fly**: the camera moves as in Camera mode but turns in place, as
  when you look around inside a building. In orthographic projection Fly
  works as Camera.

With **Lock horizon** on, Z stays upright and the view does not roll:
the roll axis does nothing. Lock horizon belongs to the 3D mouse alone;
the mouse's **Constrained orbit (Z stays up)** does not change it.

In sketch mode the 3D mouse only pans and zooms, in the direction of its
Navigation mode. The first button fits everything and the second button
goes to the home view.

The **3D Mouse** tab of Tools › Preferences › Navigation has the
settings:

- **Use a 3D mouse** turns it on or off.
- **3D mouse speed** sets how fast the view moves.
- **Navigation mode** is Object, Camera or Fly.
- For each axis, **On** lets the axis move the view, and **Reverse**
  turns its direction around from what the Navigation mode does.
- **Lock rotation** and **Lock translation** stop all turning or all
  panning and zooming. **Dominant axis only** uses only the axis that
  you push the most. **Lock horizon** keeps Z upright. These work in
  every Navigation mode.
- The button table sets the command of each button: Fit, Home, a
  standard view, Look At, Orthographic or Perspective, or None.

The tab also shows whether MitCad found a 3D mouse. MitCad needs no
driver from the device's maker:

- On Windows, MitCad reads the device directly. If the maker's driver
  is installed, set it to send nothing of its own to MitCad, so that the
  view does not move twice.
- On Linux, MitCad uses the spacenavd service. Install and start
  spacenavd; MitCad finds it also when it starts later.
- On macOS, MitCad reads the device directly. If the maker's driver is
  installed and running, it can keep the device to itself; then quit
  the driver to use the device in MitCad.

If the view jumps or an axis moves it the wrong way on Windows, turn on
the [debug log](#debug-log), move the cap along one axis at a time, and
attach the log to your report. The log shows what the device sends.

### Selection

A click selects. Ctrl or Shift adds to the selection or removes from
it. A drag from left to right selects what lies inside the window. A
drag from right to left also selects what the window crosses. In a
design with components, a drag that starts on a body drags the
component, or MitCad says why it does not. Refer to
[Components](#components).

The filters of the Select group limit what you can select. Sketch mode
has its own filters. Refer to
[Selection in sketch mode](#selection-in-sketch-mode). A selection that
you make before a command goes into the command's input.

MitCad highlights under the pointer what a click would select. A face
shows see-through in light blue, with its boundary drawn, so that its
shading still shows. In a command, the highlight has the color of the
command's input. An edge or a vertex shows in the same color. Geometry
in front of it hides the highlight.

To select something that lies behind other geometry, press the left
mouse button over it and hold it still for about half a second. A list
opens beside the pointer with everything under it that the filters let
you select, nearest first, for example `Sketch1: Profile` and
`Body1: Face`. Release the button, then point at a row to highlight its
item in the view, and click the row to select it as a click on it would:
Ctrl or Shift adds it, and in a command it goes into the command's
input. Esc or a click outside the list closes it. If the pointer moves
while you hold the button, the drag works as before.

The right mouse button opens the commands that fit the selection.
Refer to [The context menu](#the-context-menu).

The selection card also shows a Create row under the selection's text.
The row has up to four buttons. Each button starts its command with the
selection as its input, as the toolbar does. Extrude is always the first button when it
can take the selection. The other buttons depend on what you select:

| Selection | Buttons |
|---|---|
| Profiles | Extrude, Revolve, Sweep, Loft (two or more profiles only) |
| A planar face | Extrude, Create Sketch, Hole |
| A cylindrical face | Thread, Pulley, Gear, Bearing |
| Sketch curves or edges | Pipe |
| Bodies | Rectangular Pattern, Circular Pattern, Mirror, Combine (one body only) |
| A plane | Create Sketch |

A command is shown only when it can use all of the selection. When no
command can use it, for example a mix of a face and a profile, the row
is hidden. The Make tab has no Create row: the card shows the selected
body's Make section there instead
([The selected body on the Make tab](#the-selected-body-on-the-make-tab)).

### Command panels

A command's panel on the right shows the command's inputs. The view
shows the selections in the color of the input, and the preview
follows the inputs. Distances and angles also have handles in the view.
A press on a handle belongs to the handle: a click on it without a drag
does nothing, even where the handle is drawn over the orientation cube or
over an edge. While dragging, the blue handle follows the pointer continuously. The small
marker labeled **Snapped** and the value field show the rounded value that
will be kept on release. Dragging an Extrude's distance or start offset
also shows the extrusion at once in see-through amber with dashed edges:
a provisional shape, drawn while the exact preview of that value is still
computed, which replaces it when it is ready. The automatic operation and
the bodies it affects follow the exact preview; OK always keeps the exact
result of the last value. Dragging a handle of Move/Copy's Free Move,
Translate, Rotate (the ring about the axis) or Point to Position (an arrow
along each axis to the position) moves the bodies or components with the
pointer at once; the
exact preview is computed when you stop for a moment or release the
button, and OK keeps it. Typing a value keeps the precision you enter.
A preview that fails shows in red, and OK does not accept it. Enter is
OK, and Esc is Cancel. While you edit a feature, the timeline rolls
back to before the feature.

**Hide the preview while you pick.** Hold Ctrl (Cmd on macOS) alone to
hide a feature's preview, for example an Extrude's new bodies or a
Fillet's rounding. The view then shows the bodies as they are without
the command, or, while you edit a feature, as they are before it. The
tool and the handles are hidden too. Picking still works: click faces,
edges or profiles in or out of the command's inputs, and use the
panel's fields. A click while Ctrl is down is a Ctrl+click, which adds
or removes an item as a plain click does in a command. Release Ctrl to
see the preview again, computed for the inputs as they are now. The
preview hides as soon as you press Ctrl. Another key pressed with Ctrl,
as in a shortcut such as Ctrl+Z or Ctrl+S, shows the preview again at
once, and the shortcut runs as usual. The wheel (a Ctrl+scroll zoom) and
the mouse buttons (a Ctrl + middle button pan) leave it hidden.
In a text field where you type, Ctrl does not hide the preview, so
Ctrl+A, Ctrl+C and Ctrl+V edit the text. A value field of the panel
that has the keyboard but in which you have typed nothing yet does not
count: the panel puts the keyboard there after the picks and as it
opens.

**Pick on the preview.** While the preview shows, you can click the
faces and edges it shows. A face or edge that the preview only changed
picks the face or edge of the part it comes from: for example, after a
Fillet's first two edges along the sides of a rib, the rib's foot edge
between them is shorter in the preview, and a click on it adds the foot
edge. The faces and edges that the command itself makes, such as a
fillet's rounding, cannot be picked. A click on one tells you so in the
status bar, and that holding Ctrl (Cmd on macOS) shows the part's own
faces and edges to pick from.

A value field takes an expression with units (`20`, `2 in`, `d1 * 2`).
The value of an expression, or an error, shows below the field; a
number, with or without its sign and unit (`-8 mm`), is its own value
and shows nothing there, so the panel does not move while you drag an
arrow past zero. Numbers take a decimal
comma or a decimal point: `1,5 mm` is `1.5 mm` and shows as `1.5 mm`.
A comma directly between two digits is a decimal comma. Any other comma
separates the arguments of a function, and `;` always does. Thus
`max(a, b)`, `max(1, 5)` and `max(1; 5)` have two arguments, but
`max(1,5)` is `max(1.5)`.

There are no thousands separators: `1,000` is 1, and `1 000` is an
error.

**Stepping a value.** In a value field of a panel, and in the value
fields of a sketch, Up and Down change a number by 1 in the unit that
the field shows: `10 mm` becomes `11 mm`, `0.5 in` becomes `1.5 in`,
`360 deg` becomes `361 deg` and a quantity of `4` becomes `5`. Hold
Shift for steps of 10, or Ctrl for steps of 0.1. On macOS, hold
Option for steps of 0.1. The mouse wheel does the same while the field
has the keyboard. Over a field that you are not editing, the wheel
scrolls the panel and does not change a value. The preview follows each
step, as it does when you type. A step down stops at the smallest value
that the field takes: a quantity stops at 1, and a size that must be
more than zero, such as a diameter or a thickness, stays above 0. A
signed value, such as an Extrude distance, can go below 0. A field that
holds an
expression or a parameter name, such as `d1 * 2`, is not stepped. The
field's tooltip shows these keys.

### Long computations

To open a large design, or to change it, can take a while. The window
continues to draw. After a moment a progress dialog shows the feature
that MitCad computes, how many features there are and the time so far.
**Cancel** (or Esc) stops the computation in a fraction of a second,
also inside a long feature. It also undoes what started the
computation: an open leaves the previous design. A command, a
parameter change, Undo or Redo leaves the design as it was.

A command's panel stays open, and OK tries again. While the progress
dialog shows, the window takes no other input.

## Commands and shortcuts

The shortcuts in this chapter are the defaults. You can change them in
Tools › Keyboard Shortcuts. Click a command's Shortcut cell (or select
its row and press Enter) and press the new key; Delete clears a
shortcut, and the row's right-click menu also has Reset to Default. The
search field filters by command or shortcut (Down goes to the first row
found), and the context list shows the keys of one context: 3D View
(outside sketches), Sketch, or Command Panel (while a command's panel is
open), each command as one row whose key works in that context only. The
selected command stays selected when you change the context. With all
contexts shown, the Context column says where each key works; commands of different contexts
can share a key, a sketch command and a model command for one. A command
that works in several contexts, such as Command Search, Undo or Extrude,
has a row with its contexts (All, or for example 3D View, Sketch) and
expands into a row per context. A key given on the command's row is its
key in every one of them; a key given on a context's row is its own
there, and the command's row then shows Varies. To make S draw a spline
in sketches, for example, give Command Search F3 in its Sketch row and S
to Fit Point Spline: S still opens the search in the 3D view. When two
commands that work in the same context get the same key, OK says so ("S
is used by both Command Search and Fit Point Spline in a sketch") and
selects the row to change. The keys that cannot be changed are listed
in gray and marked (reserved): Ctrl+F under Search Timeline, and Enter
for OK, Esc for Cancel and the others after the commands. A command
cannot take them ("Ctrl+F is reserved for Search Timeline"). Ctrl+F, F2
(Rename) and Delete work while the Browser or the timeline has the
keyboard focus. Ctrl+C, Ctrl+X and Ctrl+V are reserved in the design and
in command panels; in a sketch they are Copy, Cut and Paste of sketch
geometry.
Help › Keyboard and Mouse Overview lists all
shortcuts. Commands without a shortcut are in command search (S) and in
the menus of the groups.

**Command search.** Type a few letters of a command's name, of its English
name in another language, or of a keyword; Enter runs the selected command.
The better matches come first, and among equally good ones the commands
that can run now, then the commands you ran last (from a key, the toolbar,
a menu or the search; MitCad remembers the last ten), then by name: after
you have used Center Rectangle, "center" in a sketch selects it. Words separated by spaces are also matched one by one,
in any order, after the matches of the whole text: "rectangle center"
finds Center Rectangle, and in a language that writes the name as one
word, its parts typed apart find it too. When the best matches are sketch tools and no sketch is
open, a row above them says how to open one ("Open a sketch first: Create
Sketch (Alt+S)") and nothing is selected, so Enter does not start a weaker
match; in a sketch, the commands that need the sketch finished say so the
same way.

**Pinned commands.** A command you use often can be pinned to the
toolbar, so that you need not look for it in a group's menu. In a group's
menu (Create, Modify, …, also in their submenus) each command's row shows
a ⋮ button at its right while the pointer is on the row; click it, or
right-click the row, or choose the row with the arrow keys and press the
Menu key (Shift+F10). Its menu has **Pin to Toolbar**, **Pin to
Navigation Bar** (the navigation bar in [View and navigation](#view-and-navigation)) and
**Change Keyboard Shortcut…**, which opens Tools › Keyboard Shortcuts at
that command (press Enter and the new key). A command you pin shows as a
button in its own group, after the group's own buttons: Draft pinned from
the Modify menu is a button in the Modify group, Sweep a button in the
Create group. The group's menu still lists the command. The pinned
buttons of a group are in the order you pinned them; drag one along them
to put it elsewhere. A command that already has a button in its group
cannot be pinned again (its **Pin to Toolbar** is unavailable). To unpin
a command, choose **Unpin from Toolbar** in its row's menu or right-click
its pinned button. The pins are kept across sessions; commands pinned in
an earlier version of MitCad, which showed them in a separate Pinned
group, now show in their own groups. In a narrow window the groups fold
to their names from the right, Create last; before a group folds, the
buttons you pinned in it go first, the last one first, so that pinning a
command never hides its group's own buttons. A folded pinned command is
still in the group's menu.

### Shortcuts

| Command | Key | Action |
|---|---|---|
| Command Search | S | Find a command by its name or by a keyword |
| New Design, Open, Close | Ctrl+N, Ctrl+O, Ctrl+W | New Design makes a new design. In the current project, which the Projects panel names, the design belongs to that project ([Versions](#versions)). Outside projects it is an untitled design. Open opens a design, or imports a file as a new design. Close closes the design |
| New Project, Open Project, Open from Cloud, Project Settings | | In File › Project. Make, open, clone or configure a Local or Cloud project ([Versions](#versions)) |
| Projects | | Show or hide the Projects panel ([Versions](#versions)) |
| Save, Save As | Ctrl+S, Ctrl+Shift+S | Save a `.mitcad` design. In a project, Save also saves a version |
| Save to Project | | Save the design in a project, as Save As does ([Versions](#versions)) |
| Save with Description | Ctrl+Alt+S | Save, and record a version with a description that you write |
| Version History | Ctrl+Shift+H | Show the versions of the design. Compare, open or restore a version, or save a copy of it |
| Sync | Ctrl+Alt+Y | Get the newer versions of the remote and send your versions (a Cloud project) |
| Print | Ctrl+P | Print the open drawing, the open sketch at 1:1 or fitted to a page, or the Cut List or Bill of Materials window ([Printing](#printing)) |
| Undo, Redo | Ctrl+Z, Ctrl+Y | Undo or redo. This includes a dimension change and a drag of the timeline marker |
| Fit | F6 | Fit the design to the view. If something is selected, Fit shows the selection. If some bodies are isolated, Fit shows only the isolated bodies |
| OK, Cancel | Enter, Esc | Accept or cancel a command. Esc also clears the selection |
| Repeat | Enter | With no command running and outside sketches, start the last command again, as Repeat in the context menu does. Its panel starts with the settings of that command's last OK (the appearance, operation, distance and so on); only the selections are empty. Not while you type in a field, rename a row or work in a list: there Enter keeps its own meaning |
| Hide the preview | Hold Ctrl (Cmd) | While a feature command's panel is open: hide its preview while you pick its inputs, and show it again on release ([Command panels](#command-panels)) |
| Create Sketch | Alt+S (Option+S) | Select a plane or a planar face, or select it before the command. Available outside a sketch |
| Finish Sketch | Ctrl+Enter | Go back to the 3D view. Esc never leaves a sketch unless you turn on Esc leaves an unchanged sketch in Preferences › Sketch: then Esc does the same as Finish Sketch when it has nothing else to cancel and you changed nothing in the sketch, and a new sketch that is still empty is removed |
| Line | L | Draw lines one after another. Drag from the last point, or from the end of a line or an arc, for an arc: tangent when you drag along the curve, normal when you drag across it. Also Midpoint Line ([Sketch curve tools](#sketch-curve-tools)) |
| 2-Point Rectangle | R | Also 3-Point Rectangle, Center Rectangle and the rounded rectangles |
| Center Diameter Circle | C | Also 2-Point, 3-Point, 2-Tangent and 3-Tangent circles |
| Sketch Dimension | D | A length, a distance, an angle, a radius or a diameter. A distance is horizontal, vertical or aligned. A horizontal or vertical distance can also be signed. The angle is the one on the side where you put the value. A dimension can also go to a body edge or corner, in the sketch plane or not, to the axes of the sketch and to its origin |
| Trim | T | Remove the part of a curve between intersections (also Extend) |
| Offset, Project, Move/Copy | O, P, M | In a sketch: offset a curve, make a projection, move. Project is also in the context menu of a face, an edge or a vertex |
| Construction | X | Change the selected curves to construction geometry and back |
| Sketch constraints | Shift+V, Shift+C, Shift+T, Shift+E, Shift+P, Shift+R, Shift+F, Shift+M, Shift+O, Shift+L, Shift+S, Shift+G | In a sketch: Horizontal/Vertical, Coincident, Tangent, Equal, Parallel, Perpendicular, Fix/Unfix, Midpoint, Concentric, Collinear, Symmetry, Curvature ([Constraints](#constraints)) |
| Delete | Delete | Delete the selected sketch geometry, constraints and dimensions |
| Copy, Cut, Paste | Ctrl+C, Ctrl+X, Ctrl+V | In a sketch: copy or cut the selected sketch geometry with its constraints and dimensions, and paste it into this or another sketch ([Copy, Cut and Paste](#copy-cut-and-paste)) |
| Extrude | E | Extrude profiles, also directly from sketch mode, or one planar face of a body. Set the direction, the extent, the taper, a thin wall and the Operation ([Extrude](#extrude), [Thin Extrude](#thin-extrude)) |
| Hole | H | Click a face for the hole positions. Types: simple, counterbore, countersink, counterdrill, taper, thread, clearance for a screw size ([Hole](#hole)) |
| Fillet | F | Round edges or faces, in several radius sets ([Fillet](#fillet)) |
| Press Pull | Q | Offset a face, fillet an edge or extrude a profile ([Press Pull](#press-pull)) |
| Move/Copy | M | Move bodies or components: a free move with handles, a move along axes, a rotation, or a move from point to point |
| Measure | I | Measure a distance, an angle, a length, an area or a position. Also whole bodies ([Inspect: Measure, Annotations and Section Analysis](#inspect-measure-annotations-and-section-analysis)) |
| Add Comment | Shift+N | Pin a note for an AI agent to a face, an edge, a body or a point ([Work with an agent through comments](#work-with-an-agent-through-comments)) |
| Rename | F2 | Rename a browser row. On the timeline, use the context menu |
| Search Timeline | Ctrl+F (Cmd+F) | Available when the timeline or the browser has the keyboard focus. Find features by name, type, component and status ([Timeline filter and search](#timeline-filter-and-search)). Also Edit › Search Timeline |

**Keys on macOS.** On macOS the shortcuts use these keys:

| On Windows and Linux | On macOS |
|---|---|
| Ctrl in a shortcut (Ctrl+N, Ctrl+S, Ctrl+Z, …) | Cmd (Cmd+N, Cmd+S, Cmd+Z, …) |
| Redo: Ctrl+Y and Ctrl+Shift+Z | Redo: Shift+Cmd+Z |
| Fit: F6 | Fit: F6 and Cmd+0 |
| Delete: the Delete key | Delete: the Delete key and the delete key (Backspace) |
| Create Sketch: Alt+S | Create Sketch: Option+S (⌥S in the menus) |

On macOS, Cmd+, opens Settings and Cmd+Q quits MitCad. These two
commands and About are in the application menu. The Window menu has
Minimize (Cmd+M), Zoom and Bring All to Front.

### The Solid tab

The Solid tab holds the commands for solid modeling in groups. The
commands of the Build tab (frames of slot profiles) are in [Build
tab](#build-tab). The other commands of the Solid tab are listed below,
in menu order.

**Create:** the menu has sections with lines between them, and two
submenus.

- Create Sketch
- From profiles:
  - Extrude ([Extrude](#extrude), [Thin Extrude](#thin-extrude))
  - Revolve
  - Sweep ([Sweep](#sweep))
  - Loft ([Loft](#loft))
  - Rib
  - Web
  - Emboss ([Emboss and Wrap Onto](#emboss-and-wrap-onto))
- On the faces of a body:
  - Hole ([Hole](#hole))
  - Thread ([Thread](#thread))
  - Plastic Feature ([Plastic part features](#plastic-part-features))
- Primitives:
  - Box, Cylinder, Sphere, Torus, Coil ([Placing a feature](#placing-a-feature))
  - Helix ([Helix](#helix))
  - Pipe
  - Intersection Curve ([Intersection Curve](#intersection-curve))
- Patterns ([Patterns](#patterns)):
  - Pattern › Rectangular Pattern, Circular Pattern, Pattern on Path,
    Pattern at Points
  - Mirror
- Envelope ([Envelope](#envelope))
- Machine Elements ›
  - Gear ([Gears](#gears))
  - Pulley ([Pulleys](#pulleys))
  - Bearing ([Printable bearings](#printable-bearings))
  - Gear Pair, Belt Drive ([Gear Pair and Belt Drive](#gear-pair-and-belt-drive))

The toolbar has buttons for Create Sketch, Extrude, Revolve and Hole. The
command search finds every command by its name, also the commands in the
submenus.

**Modify:**

- Press Pull ([Press Pull](#press-pull))
- Fillet ([Fillet](#fillet))
- Chamfer ([Chamfer](#chamfer))
- Shell ([Shell](#shell))
- Draft
- Scale
- Combine
- Offset Face
- Replace Face
- Split Face
- Split Body
- Move/Copy
- Move Face ([Move Face](#move-face))
- Align
- Delete Face
- Appearance ([Appearances](#appearances))
- Material ([Materials](#materials))
- Dogbone ([Dogbone](#dogbone))
- Mortise and Tenon ([Mortise and Tenon](#mortise-and-tenon))
- Edit Appearances ([Appearances](#appearances))
- Edit Parameters ([Parameters and
  expressions](#parameters-and-expressions))
- Configurations ([Configurations](#configurations))

Fillet, Chamfer, Shell, Draft and Replace Face obey the *Tangent Chain*
option ([Tangent Chain](#tangent-chain)).

**Split Face** and **Split Body** cut with a **Splitting Tool**: a plane,
a face, a body or sketch curves. With **Extend Splitting Tool** (on by
default), a face is extended along its surface so that it cuts through
the whole body. A plane always cuts through the whole body. While the
option is on, the view shows the tool as the split uses it: an orange,
see-through surface across the bodies, a little past their edges. It
shows where the cut runs and which bodies it goes through. Turn the
option off, and the view shows only the face that you selected.

**Surface:** Surface Extrude, Surface Revolve, Surface Sweep, Surface
Loft, Patch, Stitch, Thicken, Offset Surface, Ruled Surface, Extend and
the other surface features ([Surface modeling](#surface-modeling)).

**Assemble:** everything that builds an assembly ([Joints and
constraints](#joints-and-constraints), [Motion studies and contact
sets](#motion-studies-and-contact-sets), [Motion links](#motion-links)):

- New Component
- Joint
- As-Built Joint
- Joint Origin
- Rigid Group
- Drive Joint
- Assembly Constraint
- Contact Set
- Motion Link
- Animate Joint
- Motion Study
- Insert Component
- Insert from Library

**Construct:**

- Planes: Offset Plane, Plane at Angle, Tangent Plane, Midplane, Plane
  Through Two Edges / Three Points / Edge and Point, Plane Tangent to
  Face at Point, Plane Along Path, Plane Normal to Path at Point, Offset
  Plane Through Point
- Axes: through a cylinder, cone or torus, perpendicular at a point,
  through two planes, through two points, through an edge, normal to a
  face at a point
- Points: at a vertex, through two edges, through three planes, at the
  center of a circle or sphere, at an edge and a plane, along a path

A construction plane, axis or point goes into the active component. It
can use the faces, edges and vertices of another component, its
construction planes, axes and points, and its sketch curves and points:
pick them where an occurrence shows them. The construction feature is linked to
that occurrence, so it follows when the other component moves, and the
other component stays inactive. The preview shows the plane, axis or
point where the finished one will be, also when the active component is
moved or turned.

**Inspect:** Measure, Interference, Section Analysis ([Inspect: Measure,
Annotations and Section
Analysis](#inspect-measure-annotations-and-section-analysis)), Physical
Properties, Center of Mass ([Physical Properties and Center of
Mass](#physical-properties-and-center-of-mass)).

**Insert:** Insert Mesh, Insert DXF, Import.

**Render:** Render Image ([Render Image](#render-image)). Everything about
making the design, by manufacturing method, is on the Make tab
([Make](#make)): Edit Materials, Print Analysis, Send to Slicer and
Export Flat Pattern moved there.

### The context menu

The right mouse button opens the context menu. The menu holds the
commands that fit the selection, Edit (of a feature) and Repeat (the
last command, its panel with the settings of its last OK; the selections
are what you selected before, or empty). The commands are in submenus by kind: **Solid**, **Build**
(on Build components) and **Construct**.

In a submenu, the first commands are the ones that the selection alone
completes. On a face these are Press Pull, Fillet, Chamfer, Shell,
Offset Face and Delete Face. Then **More** holds the commands that need
more selections: Draft, Hole, Joint and Loft. **More** also holds Box,
Cylinder and the other creation commands that would only stand on the
face. Commands for surface bodies (Untrim, Unstitch, Reverse Normal and
others) show on surface bodies only.

**Move/Copy** is an entry of the menu itself, near Edit, when the
selection is bodies or components (not in Solid). It starts with them
selected. Select a body or a component in the Browser, then right-click
it in the view: the selection stays, and the menu is the one of the body
or the component. A face of a body that is not selected selects the
face, and a face has no Move/Copy.

**Look At** shows before Measure when you right-click a planar face, an
origin or construction plane, or a sketch. It turns the view to look
straight at what you right-clicked. This also works for a face of a
component, of a linked design or of an imported model. A curved face and
an edge have no Look At.

**Delete** removes a whole thing, in one undo step. The thing is one
of these:

- A Build member, with what it added
- Its spacing set
- The body
- The component (its occurrence)
- The feature that made the face

## Make

Everything about making a design is on the **Make** tab: the materials,
the annotations a workshop reads, and a group for each manufacturing
method. The tab is always there, after Solid and Build. Work through it
from left to right: choose the materials, annotate, then send the design
to the machine.

### The Make tab

| Group | Commands |
|---|---|
| **Materials** | **Edit Materials…** ([Materials](#materials)), **Material**: the Material tool, as in Solid › Modify ([Materials](#materials)) |
| **Annotate** | **Dimension**, **Note**, **Datum Label**, **Feature Control Frame**, **Hole Callout**, **Material**, **Process Note**, then **Annotations…** ([Annotate](#annotate)) |
| **3D Print** | **Print Analysis** ([Print Analysis](#print-analysis)), **Send to Slicer** ([Sending bodies to a slicer](#sending-bodies-to-a-slicer)), **Printers…** and **Processes…** ([Print settings](#print-settings)), **Plates…** ([Print plates](#print-plates)) |
| **Laser** | **Export Flat Pattern…** ([Sheet metal](#sheet-metal)), **Export Face as DXF…** ([Laser cutting](#laser-cutting)) |

Each command has one place on the toolbar: Edit Materials came from the
Modify group, Print Analysis from the Inspect group and Export Flat
Pattern from the Sheet Metal group of the Solid tab. *Send to Slicer* is
the command that was called *3D Print*. Their keyboard shortcuts stay as
they were, and the command search finds them all. Render Image stays on
the Solid tab, in its Render group. A group for CNC machining comes with CAM.

Right-click a body or a face: the context menu's **Make** submenu has the
Make tab's commands that take it.

### Annotate

The Annotate group keeps annotations in the design, drawn in the view as
on a drawing and listed in the Annotations window ([Inspect: Measure,
Annotations and Section
Analysis](#inspect-measure-annotations-and-section-analysis)). Each
command opens a panel: pick, fill in what it asks, and click **Add**
(or press Enter). Each annotation is one undo step.

- **Dimension**: pick one or two faces, edges, vertices or construction
  planes, axes or points. MitCad keeps the value they have: the
  diameter of a round face, the distance, offset, axis distance or angle
  of two picks.
- **Note**: pick where the arrow points and type the **Text**.
- **Datum Label**: pick a face, an edge, an axis or a plane. *Letter*
  empty takes the next free letter.
- **Feature Control Frame**: pick a face or an edge, then click
  **Tolerance…** for the characteristic, the tolerance and the datums.
- **Hole Callout**: pick a hole's wall or rim; the text comes from the
  hole.
- **Material**: pick a body (a face of it). The note shows the body's
  material and changes when the material changes.
- **Process Note**: pick a body. The note shows the printer and the
  process the body prints with and every print value set on the body,
  its occurrence or its component (`Printer: Bench / Process: Standard /
  Walls: 5`). It follows the print settings.

A material or process note whose body is deleted becomes *broken*, as
any annotation whose pick is gone.

### Laser cutting

**Export Face as DXF…** writes the outline of a flat part for a laser,
waterjet or plasma cutter. Select a planar face of the body, choose the
command in the Laser group (or right-click the face and choose it there)
and save the file. MitCad looks at the body square to that face from
outside and writes its outline at 1:1 in the design's length unit, or in
the unit you choose as the file type ([The unit of a DXF
file](#the-unit-of-a-dxf-file)): the outer outline
on the layer `CUT_OUTER`, the holes on `CUT_INNER`, the lower left corner
of the outline at the origin. The design does not change. For a
sheet-metal part, **Export Flat Pattern…** writes its unfolded flat
pattern with its bend lines, for the same cutters. To lay several parts out on a sheet of
material, make a cut sheet ([Drawings](#drawings)).

### The selected body on the Make tab

On the Make tab, select one body (or a face of it): the card on the right
shows its **Make** section under the selection, where the other tabs show
the Create row. Its rows go from the widest choice to the body's own:

- **Printer**: the printer of the whole design, for every body. Click it
  to open a list of the printers, grouped by vendor, from this design and
  from your libraries (This project, My library, Built-in). Type a part
  of a name to find one and press Enter. The design then prints with it,
  and the Process list puts that printer's processes first. A printer
  from a library is copied into the design. Without one, the row says
  *Slicer default*. The last row, **Printers…**, opens the Printers
  manager on the design's printer.
- **Process**: the process profile the body prints with. Its list works
  the same way: the processes of the design's printer first, then the
  others under their printers, and those for any printer last. The one
  you choose is set for the body; a process from a library is copied into
  the design. **Processes…** opens the Processes manager.
- **Material**: the body's material. Click it to open the Material
  Picker, the same one as in Send to Slicer's material rows: the
  materials with their color swatches, the 3D printing filaments first,
  grouped by where they are (This design, the 3MF files they were
  imported from, This project, My library, Built-in), the body's own
  marked *Current*. Type a part of a name to find one and press Enter:
  the body gets it at once. A material from This project or My library is
  copied into the design in the same undo step. If you gave the body an
  appearance by hand that the material does not offer, MitCad asks
  whether to replace it. While the body has no material of its own, the
  row shows the design's default, for example *Default (Steel)*. The last
  row, **Edit materials…**, opens the Materials window on the body's
  material.
- The print values that differ from the profiles, each with the icon of
  where it is set: the body, the occurrence or the component. Type a new
  value and press Enter to set it on the body.
- **Add Print Value…** adds a setting of the body's own, for example the
  number of walls. It opens a list of the process and printer settings,
  grouped as the Properties window groups them, with the value the body
  prints with now. Type a part of the name to find a setting (point at
  it to see its key), then press Enter or double-click it. The setting
  is set on the body at that value and its field is ready for a new one.
  A setting the profiles do not set (the slicer's default) gets an empty
  field: it is set when you type a value. The settings the body already
  sets are not in the list. Filament settings belong to the material:
  edit them in Edit Materials….
- The **×** after a value set on the body removes it from the body. The
  body then prints with the value it inherits: the row shows the
  occurrence's or the component's value, or goes away when the profile's
  value applies again.
- **Send to Slicer**, the last row, opens Send to Slicer
  ([Sending bodies to a slicer](#sending-bodies-to-a-slicer)) with only
  this body in **Bodies**, whatever else the view shows. The rest of the
  panel is the same as from the toolbar, and you can still add or remove
  bodies there before you press Send. The button is grayed when Send to
  Slicer cannot start.

When the body has no values of its own, the section says so and points
at Add Print Value…. Each change is one undo step. The values added here
are the same as those the Properties window sets on the body: Send to
Slicer and the 3MF export use them, and the Browser's dot marks the
body. The Properties window
([Properties](#properties)) shows all the print settings and edits them
on any level.

In the Browser, on the Make tab, a small dot after a body's name marks a
body whose material or print settings differ from its component's: a
material of its own, or print values set on the body. Point at the dot
to see what differs. The other tabs do not show the dots.

## Create features

The Create group of the Solid tab makes features from sketches and from
other geometry. The sections that follow give the options of these
features.

### Extrude

**Extrude** (E) extrudes profiles. You can start it directly from sketch
mode. In the panel you set the profiles, the direction, the extent, the
taper, a thin wall and the Operation. The thin wall is the Thin Extrude
type ([Thin Extrude](#thin-extrude)). The **Wrap Onto** option of the
Extrude panel wraps profiles onto a cylindrical or conical face ([Emboss
and Wrap Onto](#emboss-and-wrap-onto)). For an Extrude with **Operation:
Cut** in an assembly, refer to [Cutting across
components](#cutting-across-components).

While you make a new extrusion, MitCad selects the Operation from where
the solid lies: Join where it touches a body, Cut where it goes into one,
New Body where it touches none, until you select one yourself. When you
edit an extrusion, its Operation stays as it was saved, also when Flip,
Start or a distance moves the solid into a body or out of it; select
another Operation yourself to change it.

**Two Sides and Start: Offset.** With **Direction: Two Sides**,
**Distance** goes along the sketch normal from the start and **Distance
2** the other way. A negative distance ends on the other side, so the
solid lies on one side of the sketch without touching it: **Distance**
50 mm and **Distance 2** −30 mm give a solid from 30 mm to 50 mm along
the sketch normal. You can also drag an arrow across the sketch plane to
get a negative distance. The two ends must not meet or cross: 50 mm and
−50 mm is refused. A taper still widens or narrows the solid as measured
from the sketch plane. Join, Cut and Intersect work with such a solid
too. **Start: Offset** is another way to get a solid that does not touch
the sketch. The extrusion then starts at that offset from the sketch
plane, and its distances are measured from there. Extrusions saved
before this change, and imported ones, keep their solids: a negative
distance there goes its own way by its size.

**To Object in another component.** The object of To Object or of
Start: Object can be a face or a body of another component, picked
where an occurrence shows it. The extrusion then follows that object
and both occurrences ([Components](#components)).

**To Object at a point.** The object of To Object, and of Start:
Object, can also be a point: a sketch point, a vertex of a body or a
construction point. The extrusion then ends (or starts) on the plane
through the point parallel to the sketch. A design laid out in two
sketches at right angles to each other is so extruded without a
construction plane: extrude a profile of one sketch up to a point of the
other. **Offset** moves that plane as it moves any other object. The
extrusion keeps the point by its name and follows it when the point
moves. A point that is gone fails the extrusion, as a plane that is gone
does. A straight line works too: a sketch line, a straight edge or an
axis that is parallel to the sketch plane (at right angles to the
extrusion). The extrusion ends (or starts) on the plane through the
line parallel to the sketch. A line that is not parallel to the sketch
plane is refused, and the panel says how many degrees off it is. Hole's
To Object takes a point or a line too: the hole ends on the plane
through it at right angles to the hole, and a line must lie in such a
plane. The point or line must be in the extrusion's own component.
Revolve's To Object takes neither a point nor a line.

**A planar face as the profile.** Extrude also takes a planar face of a
body as its profile, with no sketch. Select the face and press **E**.
Or right-click the face and select Solid › Extrude. Or select the face
in Profiles. The face's whole area goes along the face's own normal,
holes included. This also applies to a slanted face. A positive
distance goes out of the material, and MitCad selects Join for you. A
negative distance, or Flip, goes into the material (Cut). New Body is
also available. Two sides, symmetric distances and a start offset from
the face work as for profiles. Tapers, To Object and All do not work.

This works on any body, also on a body imported without its history.
The extrusion keeps its reference to the face. When an earlier feature
moves or resizes the face, the extrusion follows. If the face is gone,
the extrusion fails and says so. It does not take another profile. If
the selected face is not planar, or if it is a face of a mesh body, the
panel refuses it.

A face goes into Profiles alone. While Profiles holds sketch profiles, a
click on a face does not go in, and the status bar says what to do: to
start the extrusion at that face, click Start Object first, then the
face; to extrude the face itself, clear Profiles first.

### Thin Extrude

**Thin Extrude** makes walls along the curves of the profiles. It also
takes sketch curves instead of profiles. Open chains and closed chains
are permitted. To use sketch curves, first set the type to Thin Extrude.
Then select the curves in Profiles.

An open line, an open arc or an open chain of them gets a wall along it.
The wall is square at its ends. Side 1 is on the outside of the bend of
the chain, and Side 2 is on the inside. For a straight chain, Side 1 is
on the right of the first selected curve, in the direction in which you
drew the curve. Center puts half of the wall on each side. A closed
chain gets walls in the same way as the outline of a profile.

### Emboss and Wrap Onto

**Emboss** raises sketch profiles from a face by a depth, or sinks them
into the face. The letters of a text are profiles too. On a planar face
the profiles go straight out of the material or into it. If you select
no face, they go straight out from the sketch plane or into it.

On a cylindrical face, Emboss wraps the profiles round the cylinder. It
lays them out by their length along the cylinder, and does not project
them. Thus the letters keep their widths, and their walls stand square
to the face. A sketch on a plane parallel to the axis of the cylinder
wraps onto the side that faces the sketch. For a plane through the axis,
the profiles wrap onto the side to which the normal of the plane points.
The wall of a bore also takes profiles. Emboss then raises them into
the hole. The profiles must stay in one turn.

On a conical face (a cone or a countersink), Emboss lays the profiles
onto the development of the cone. Thus lengths, angles and areas on the
cone are those of the sketch. Emboss raises the profiles along the
normals of the cone.

When Emboss wraps the profiles, you can move them on the face without
moving the sketch. **Along Axis** moves them along the axis of the
face. A positive value moves them towards +Z, or towards +Y or +X when
the axis lies flat. **Around** turns them about the axis. **Rotation** turns them on
the face about their center. Each value is 0 at first, which keeps the
profiles where the sketch puts them. Each value takes a parameter or an
expression, as other values do. Each value also has a handle in the
view: an arrow along the axis, a ring about the axis, and a ring at the
profiles. Drag a handle to change its value. The preview follows.

On any other face (a sphere, a torus, a free-form face), Emboss projects
the profiles onto the face. It moves each point of a profile along the
normal of the sketch until it meets the face. Then it raises the
projected profile along the normals of the face, or sinks it into the
face, so that the top follows the curve of the face. Select a
**Direction** to project along it instead of along the normal of the
sketch: an axis, a straight edge or a sketch line, or a plane or a
planar face (its normal). If the line of projection meets the face
twice (as on a ball), Emboss uses the side that is nearest in front of
the sketch, else the nearest side behind it. Each profile must fall
fully on the face. A profile that falls partly off the face, or past
the rim that the direction sees, stops the feature, and the message
names that profile.

On a cylindrical or conical face, **Method** selects between the two.
Use **Wrap** when the letters must keep their widths round the face, as
for a label round a rod. Use **Projection** when the profiles must look
as they do in the sketch from the direction of the projection, as for a
logo seen from the front on a curved panel. On a planar face, select a
Direction to project the profiles along it. Without a Direction they go
straight out of the face.

**Taper Angle** tilts the walls of every emboss. At 0 the walls stand
along the normals of the face, so on a cylinder they spread out as the
emboss rises. A positive angle widens the profiles as they rise, as the
Taper Angle of Extrude does. A negative angle narrows them, so the top
is smaller than the foot. For Sink, a negative angle narrows the pocket
towards its floor. On a cylinder or a cone the top moves by the depth
times the tangent of the angle along the face. For a projection the
move is measured as the direction sees the profiles. If the angle closes
a profile (its walls meet before the full depth), the feature stops and
the message names that profile. Drag the ring in the view to change the
angle.

The **Wrap Onto** option of Extrude does the same from the Extrude
panel. Select a cylindrical or conical face. Extrude wraps the profiles
(a text, an inserted SVG logo) onto the face and grows them by the
distance out of the material. With Flip and Cut, Extrude engraves them
into the material. The text stays a sketch text. Thus you can still edit
its string.

### Helix

**Helix** turns sketch profiles about an axis while they rise along the
axis, as a screw moves. Examples are the profile of a thread and a
twisted ridge. Give one of these pairs of values:

- The revolutions and the pitch
- The revolutions and the height
- The height and the pitch

The helix is right-handed or left-handed. A Growth moves the profiles
out from the axis by that distance on each turn. A negative Growth
moves them in. Thus Growth widens the helix into a cone or a spiral.

### Sweep

**Sweep** starts where the path meets the profile. *Distance 1* is
the part of the path beyond the profile that the sweep covers, from 0
to 1 (1 is all of it). If the profile lies inside the path, or at its
far end, the path also runs on before the profile. *Distance 2* then
shows: the part of the path before the profile, also from 0 to 1. Both
are 1 for a new sweep, so it covers the whole path both ways from the
profile. Set Distance 2 to 0 to sweep only forwards from the profile.
On a closed path the sweep goes once round; Distance 1 less than 1
covers that part of it. **Pipe** has Distance 1 only: its section is at
the start of the path.

When the path of a **Pipe** has sharp corners, *Corners* shows. *Mitered*
(the default) runs the sides on until they meet in the corner. *Rounded*
turns the section about the corner, so the outside of the corner is
round. The choice holds for every corner. For a bend of a given radius,
or a bevel, or a different joint at one corner, use *Sketch Fillet* or
*Sketch Chamfer* on that corner of the path in its sketch.

A pipe folds through itself where its path bends more tightly than its
section reaches from the path: a fillet in the sketch whose radius is
less than half the pipe's size, for example. A short straight piece
between two mitered corners, such as a small chamfer, folds the same way
when the two miters overlap, and so does a short arc between two
mitered corners. MitCad builds such a pipe in pieces, cut where it
folds, and joins them into one solid, so that the material the pieces
share counts once, as in a tube bent that tightly; a path that crosses
itself or comes back close to itself is joined the same way. This works
along a path in one plane, along a path of lines and arcs in no plane,
and for a round pipe along any path; a round pipe along a tight bend of
an ellipse or a spline in one plane, or of a helix between straight
legs, is built from the tube its rim sweeps, up to where that tube meets
itself. The pipe's details then note the curve and the numbers. Where it
cannot do that (for example a tight bend of a curve that bends tightest
at more than one point), **Pipe** warns instead,
naming the curve of the path and the numbers: the radius it bends with
and the smallest that works, or the piece's length and the length its
miters need. Make the fillet larger or the pipe smaller, lengthen the
chamfer, or choose *Rounded* corners. Only a pipe along its whole path
is checked. **Sweep** builds a profile that passes through itself in
pieces the same way.

**Sweep** can obey a *Guide Rail* as well as the path. *Rail Point* says
where the rail meets the profile. This is on the plane square to the
path. Or it is at the same fraction of the length of the rail as of
the path. An optional *Second Rail* (sketch curves or edges)
turns and sizes the profile with the line between the two rails.

### Intersection Curve

**Intersection Curve** (Create, after Pipe) makes a 3D curve from two
sketches on planes at an angle to each other, for example a sketch on XZ
and a sketch on XY. Seen along the normal of each sketch, the curve looks
like the curves of that sketch. Select the *First Curves* in one sketch
and the *Second Curves* in the other. MitCad moves each set of curves
along the normal of its own sketch, and the curve is where the two
surfaces that they make meet.

- The curve follows both sketches. When you edit a sketch or change its
  dimensions or parameters, the curve and the features that use it
  change too.
- The curve is a path. Select it in the view or in the timeline for the
  path of Sweep or Pipe, for the rails or the centerline of Loft, for
  Pattern on Path, or for a construction plane or point along a path.
  A point or a plane along the curve is exactly where the given length
  along the curve ends.
- If the curve lies in one plane, Rib and Web can grow from it: select it
  as their *Curves*.
- In a later sketch, **Project** and **Intersect** take the curve. Select
  it in the view, in the Browser or in the timeline. Project adds the
  curve and its ends to the sketch. Intersect adds the points where the
  sketch plane crosses the curve. A sketch that you click in the timeline
  or the Browser still goes in as the whole sketch.
- It shows in the Browser in the Construction folder, with a light bulb.
- If the curve cannot be made, the panel shows the reason: the planes
  of the sketches are parallel, the curves do not meet, or the result
  divides into branches or into separate pieces (with the points where
  that occurs).

### Loft

Each end of a **Loft** has a *Start Condition* or an *End Condition*.
The condition is one of these:

- Free
- For a profile: a Direction. MitCad measures the *Takeoff Angle* from
  the normal of the profile. A positive angle points outwards
- For a face: Tangent or Smooth to the faces next to it
- For a point: a sharp tip, or a rounded (tangent) tip

A face with holes lofts from its outer edges. The loft is solid across
the holes.

### Hole

**Hole** (H) makes holes. **By Pick**, the hole goes where **Place On**
puts it (see [Placing a feature](#placing-a-feature)): click a face, and set
its distances from two edges of the face; or select a point, or a shaft
or a hole to drill along its axis. **From Sketch**, the holes are at
sketch points. A hole is one of these types: simple, counterbore,
countersink, counterdrill, taper or thread; for printed parts it can
also be a press fit (below).

**Flip**, next to the placement, turns the hole round.

**Typing the sizes.** Tab moves from one field of the panel to the next,
in the order the panel shows them: from *Distance 2* on to *Diameter*,
and so on down the panel; Shift+Tab goes back. Fields that are hidden or
disabled are skipped, and Tab never presses OK or Cancel. The
distances from the edges and the depth are also dimensions in the view.
Click a dimension's value (it turns blue under the mouse) and type a new
one there: Enter keeps it, Esc puts the old value back, and Tab goes on
to the next dimension. You can type the same as in the panel's field,
such as `25`, `1 in` or `d1 * 2`; the panel shows the value too, and the
preview follows. A text that is not a value turns the box's border red,
and Enter does nothing until you correct it or press Esc. Enter in a
dimension's box never makes the hole: press Enter again, or click OK,
when you are done.

A hole made on a face in an earlier version opens on its face. If you do
not move it, it is saved as it was. If it has several positions,
**Earlier Positions** keeps them until you choose By Pick.

An imported hole can be placed at distances from two edges of its face,
on a plane, or along a shaft. It opens with that placement: **From Two
Edges** (with *Edge Offset 1* and *Edge Offset 2*), **On a Plane**, or
**Along a Shaft** (with *Offset*, from the face it is measured from).
Change the distances there, or choose By Pick to place it again.

A counterdrilled hole has a counterbore whose floor is a cone down to
the hole. A tapered hole has a Taper Angle. The wall leans in as the
hole goes deeper.

A hole tapped with a taper pipe thread (NPT, or ISO 7-1 *Rc*) is
*Tapered* by default. It then keeps its *Diameter*: that is the tap
drill, which the size fills in. The thread tapers the bore from the
hole's mouth, 1:16 on the diameter, from its minor diameter at the
mouth until it meets the drilled bore. This happens also when the
thread is cosmetic. A hole saved by an earlier version stays as it was
(not tapered, its bore the size's minor diameter) until you check
*Tapered*.

A tapped hole has a *Direction*, right hand or left hand, and can take a
custom thread with a number of starts, as Thread does (below). Its bore
is the thread's minor diameter.

**Clearance holes.** With *Hole Tap Type* **Clearance** the hole is for
an ISO metric screw: choose its *Screw Size* (M1.6 to M24) and its *Fit*
(Fine, Normal or Coarse, the series of ISO 273; Normal by default). The
size fills in the fields:

| Field | What the size gives | M6, Fit Normal |
|---|---|---|
| Diameter | the clearance hole of ISO 273 for the fit | 6.6 mm (Fine 6.4, Coarse 7) |
| Counterbore Diameter and Depth | the counterbore for the socket head of an ISO 4762 screw (DIN 974-1), deep enough to put the head just below the face | 11 mm, 6.4 mm |
| Countersink Diameter and Angle | the countersink for the 90° head of an ISO 10642 screw (or ISO 15065's, where that is larger), so that the head is at the face | 13.44 mm, 90° |
| Counterdrill Diameter, Depth and Angle | as the counterbore, with a 90° cone | 11 mm, 6.4 mm, 90° |

M24 has no countersink in the tables, so a countersink keeps its
values. A tapped hole's countersink is filled in too: just over the
thread's major diameter, at 90° (6.6 mm for M6x1).

A field the size fills keeps following the size, the fit and the hole
type until you type a value in it. Your value then stays when the size
changes, and so does an expression (`d1 + 2`). Choosing a *Hole Tap
Type* fills in every field again, except expressions. When you edit a
clearance hole, it opens with its screw size and fit, and the values you
typed stay as they are. The hole keeps its values as numbers, so a
change to the tables in a later version does not change it. Holes made
in earlier versions open as they were.

**Press fits for printed parts.** A printed round hole cannot hold a pin
or a bearing firmly: a printed surface is only good to about ±0.1 mm.
With *Hole Tap Type* **Press Fit** the hole grips the part that goes in.
*Diameter* is the diameter of that pin or bearing, and *Press Fit Shape*
is one of these:

| Shape | What grips | Use it for |
|---|---|---|
| Hex | the six flats of a hexagon | pins; the corners leave room for the plastic the pin pushes aside |
| Square | the four flats of a square | small pins |
| Crush Ribs | small ribs round a slightly larger bore | pins and bearings in parts you assemble once; the ribs deform as the part goes in |

*Interference* is how much smaller than *Diameter* the flats or the
ribs' tips are, on the diameter. A new hole suggests the printed fit
`press_fit_interference` (0.1 mm, see [Tuning printed
fits](#tuning-printed-fits)), so that you tune it once for your printer.
Crush ribs also have *Ribs* (3 by default) and *Rib Height* (0.3 mm): the
bore is the ribs' tips plus twice the height, so the pin touches only
the ribs. A rib must stand out more than half the interference.
*Rotation* turns the shape about the hole's axis; at 0° the first flat
or rib faces the x axis.

A press-fit hole is a simple hole with a flat bottom, so the panel hides
*Hole Type*, *Drill Point* and *Taper Angle*. It can go to a depth,
through all or up to an object. It has no hole callout in drawings,
because it is not round: dimension its flats or its ribs instead.

A round hole printed lying on its side has a roof with nothing under
it: the roof sags, and the hole comes out out of round and too small.
When a body of the design has a print bed face (**Set as Print Bed
Face**), the Hole panel offers *Print Shape* for such holes:

| Print Shape | The hole's top |
|---|---|
| Round | round, as drawn |
| Teardrop | two sides at 45° that meet at a point, which print without support |
| Flat Roof | the teardrop's sides cut short by a flat roof that bridges the hole, *Roof Sag* above the circle |

The roof points away from the print bed face; without one, the design's
−Z is down. Holes whose axis is not parallel to the bed stay round. The
lower part of the hole keeps its circle, and the hole keeps its
diameter in drawings and callouts: only the solid gets the roof. A new
flat roof's *Roof Sag* suggests the printed fit `hole_roof_sag` (0.4 mm,
see [Tuning printed fits](#tuning-printed-fits)), which you tune to how
far your printer's bridges sag. A counterbore lying on its side gets a
roof of its own. Print shapes do not go with a press fit, a taper or a
modeled thread. Changing the print bed face turns the roofs with it.

### Thread

**Thread** makes a thread that is cosmetic or modeled. The options
below apply to it.

The first cylindrical face suggests the size, whether you select it
before Thread (for example from the view's context menu) or pick it in
the panel. A shaft gets the ISO metric coarse size of its diameter and
class 6g. A hole gets the size whose tap drill it is and class 6H: a
5 mm hole gets M6x1 6H. Once the faces are known, *Class* offers only
the classes of their side, and a new size or designation keeps the
class on that side.

When you thread several holes of the same size one after the other, the
next Thread starts from the settings of your last thread. If the first
cylindrical face has the same diameter (within 0.001 mm) and the same
side (hole or shaft) as the last thread you made with OK, all of that
thread's settings come back: the type, size, class, direction,
*Modeled*, starts, *Tapered*, clearances, *Full Length*, length, offset,
location and end chamfers. A setting that you change before you pick
the face keeps your value. On a face of another size, Thread suggests
the size as before, and the other settings start from their defaults.
Editing a thread and Cancel do not change what is kept. MitCad keeps
these settings on this computer, not in the design.

A taper pipe thread (*Thread Type* NPT, or ISO Taper Pipe Threads with
*R* sizes for shafts and *Rc* sizes for holes) is *Tapered* by default:
1:16 on the diameter. It starts at the free end of the face: the mouth
of a hole, or the end of a shaft. If both ends or neither is free, it
starts at the end that *Thread Location* names. Its size sets the
diameters there. From there they shrink into a hole and grow along a
shaft. Even a cosmetic tapered thread changes the geometry: it tapers
the bore, or the shaft's end, to the thread's cone. MitCad can model
tapered NPT threads; ISO 7-1 threads are cosmetic only. Tapered threads
have no end chamfers, so the panel does not offer *Chamfer Ends*. A
thread saved by an earlier version stays as it was, straight, until you
check *Tapered*.

*Thread Type* **Custom** makes a thread of your own size, for example a
longer pitch than any standard has for a 3D-printed part. Type its
*Major Diameter* and *Pitch*; a shaft you pick fills in its diameter.
Both can be expressions, so the thread follows your parameters. A
custom thread has the 60-degree profile, no designation and no class.
It is cosmetic or modeled like a standard size, with the same
clearances and end chamfers. The pitch must leave the thread a core.

A modeled custom thread can have several *Starts*, as threaded rods and
jar lids do. The grooves are evenly spaced round the axis, a pitch
apart, and each one rises by the lead, the starts times the pitch, in a
turn. A two-start thread of pitch 1.5 mm advances 3 mm a turn. A
trapezoidal size with a lead, such as `Tr10x4(P2)`, has its starts in
its designation (2 here) and is modeled with them.

Trapezoidal threads (ISO 2904, the lead screws of linear drives) can be
modeled. MitCad cuts the 30-degree profile of ISO 2901 with its flat
crests and roots, and the standard's clearance at the roots: the
screw's root is a little deeper than the nut's crest, and the nut's root
a little wider than the screw's crest, so the two clear each other.

A modeled thread runs out as a tap or a die does. Where its face ends
in a cone that opens away from the thread, such as a countersink above
a tapped hole or a chamfer at a shaft's end, the grooves go on into the
cone by a pitch and fade out of it. No flat, unthreaded ring is left at
the cone's root. A floor, a shoulder, or a hole or shaft that goes on
stops the thread flat at the end.

*Chamfer Ends* cuts a cone at the free ends through which the thread
runs out. Examples are the tip of a shaft and the mouth of a hole. Thus
the first turn starts on the cone, without a knife edge. MitCad
chamfers an external thread to below its minor diameter. It
countersinks an internal thread past its major diameter. Ends against
more material get no chamfer, and neither does an end that runs out
into a cone: that cone is its chamfer already. *Ends* selects both
ends, the start or the end. The start is the end from which the thread
location measures. By default it is the high end.

When a modeled thread is shorter than its face, an end inside the face
with *Chamfer Ends* gets a run-out instead. The grooves go on past the
end inside a cone at the chamfer's angle. In a hole the cone narrows
from the major diameter to the hole's diameter; on a shaft it widens
from the minor diameter to the shaft's diameter. So the grooves fade
out of the face, without a flat wall. Without a chamfer there, the
thread ends flat.

A modeled thread in a design saved by an earlier version keeps the flat
ends it had, so that chamfers and fillets on its ends stay as they
were. Once you edit the thread, it gets the run-outs. The same holds
for the modeled thread of a tapped hole: under a countersink it keeps
its flat lip until you edit the hole. An imported
thread can keep flat ends where the program it comes from made them so.
Earlier versions cannot open a design saved with this version that has
a modeled thread.

The size of the chamfer comes from the thread size. The cone is at 45
degrees to the axis. It goes a tenth of a pitch inside the minor
diameter of a shaft, or a tenth of a pitch outside the major diameter
of a hole. This size obeys a change of the thread size. Or you give the
size as an angle and a diameter.

The *Extra clearance* of a modeled thread makes it fit looser. Its
size, its class and its pitch stay. There are two clearances:

- *Radial clearance* is per side, along the radius. It moves the whole
  profile away from the material. Thus it enlarges an internal thread
  and shrinks an external one. It is not a diameter: 0.2 mm changes each
  diameter by 0.4 mm.
- *Flank clearance* is on each flank. MitCad measures it perpendicular
  to the flank, not along the axis, and it is not a scale factor. MitCad
  applies it after the radial clearance. It thins the teeth of an
  external thread or widens the groove of an internal thread. The pitch
  and the flank angle stay.

A fine pitch has small teeth, so it takes less flank clearance. The
crest must stay at least a fiftieth of the pitch wide. An internal M6x1
takes at most 0.0996 mm, an external M6x1 at most 0.0455 mm. A
trapezoidal thread's steeper flanks leave wider crests: an external
Tr10x2 takes up to 0.334 mm. The panel's
default, the parameter `thread_flank_clearance` (0.1 mm), is a fit for
your printer and serves threads of every size: where a thread's pitch is
too fine for it, MitCad uses the largest flank clearance that the thread
takes, and the parameter keeps its value. A number that you type is
used as it is. If it is too large, the thread fails, and the message
says the largest value that fits.

Clearances on both mating bodies add up. MitCad does not change the
other body.

MitCad makes the thread even when it does not suit its face, and the
panel's message (and the thread's warning in the timeline) says what
happened:

- The size is far from the face. A shaft should be about as wide as the
  thread's major diameter, and a hole about as wide as its minor
  diameter, within the class's tolerance and 0.5 mm. For
  example, "M80x2 (Ø80 mm) on face F45:side(c21) (Ø87.1 mm): the face's
  diameter is 7.1 mm larger than the thread's major diameter". Pick a
  size near the face, or change the face.
- A modeled thread cut through the wall behind its face. A thread cuts
  from the face down to its root. If the material behind the face is
  thinner than that, for example a ring whose bore lies inside the
  thread's root, the thread takes the wall with it, and the threaded
  part of the ring is gone: "… the thread cut through the 3.5 mm wall
  behind the face". A hole's wall to the outside is checked the same
  way.

### Patterns

The pattern commands are Rectangular Pattern, Circular Pattern, Pattern
on Path, Pattern at Points (Solid tab, Create › Pattern) and Mirror
(Create, after the Pattern submenu).

**Rectangular Pattern** and **Pattern on Path** have a Distance Type
and a Direction Type:

- Distance Type Spacing: the length is the distance from one copy to
  the next. Distance Type Extent: the length is the distance from the
  first copy to the last. The length row shows Spacing or Extent.
- Direction Type One Direction: the copies go along the direction.
  Direction Type Symmetric: the copies go to the two sides of the
  original. Rectangular Pattern hides Flip for a symmetric direction.

Rectangular Pattern has up to three directions. Direction 2 makes rows
and columns. Direction 3 makes layers: pick a third axis, edge or sketch
line, and one pattern makes a 3D grid of copies. The rows of Direction 2
and Direction 3 show when you pick their direction. The number of copies
is the product of the three quantities (the original is included).

Circular Pattern has Symmetric. With Symmetric, the total angle is
centered on the original. A full turn does not change. When you edit a
pattern, its panel shows the options that the pattern has, also for an
imported pattern.

**Pattern on Path** repeats its objects along a path. The path is a
sketch line, an arc or a circle, or lines and arcs of one sketch that
are joined end to end. MitCad goes along the path from the free end of
the first curve.

**Pattern at Points** repeats Features, Bodies or Components at sketch
points, construction points or vertices. To make the pattern:

1. Select the source objects.
2. Select the Target Points.

Every target receives one more copy with the orientation of the source.
The originals stay. The targets stay linked to their geometry. Thus, if
you move a point, MitCad updates its copy. Components stay occurrences of
the same definition. Features replay their operations. To repeat a hole
and its finishing features as one group, select them together. Then
MitCad repeats the group and does not copy the resulting body. For
Bodies and Features, first activate the component that owns the
sources. Use Components to repeat placed occurrences from other
components.

The preview marks the shared **Automatic anchor**. The anchor goes to
each target. The rules for the anchor are these:

- Components: the center of the bounding box of their origins
- A feature group that contains holes: the center of the bounding box
  of all hole placement points, on their start surface. Finishing
  features do not change it
- Other feature groups: the bounding box of their tools
- A group that contains only finishing features: the modified bodies
- Bodies: the bounding box of all selected bodies

These group rules do not depend on the order of selection. Under
**Additional options**, set Anchor to **Selected point** to select a
different anchor. Set Anchor to **Automatic** to go back to the rules. A
group keeps its relative positions. You cannot select a repeated target
reference twice. Coincident targets, and targets at the anchor, cause
an error. MitCad does not silently omit a copy. Edit the timeline
feature to change the sources, the targets or the anchor.

## Modify features

The Modify group of the Solid tab changes bodies that exist. The
sections that follow give the options of the commands that have them.

### Press Pull

**Press Pull** (Q) acts on the selection:

- It offsets a face
- It rounds an edge
- It extrudes a profile

If you select a face of a fillet, Press Pull opens that fillet.

### Move Face

**Move Face** moves faces of one body together by a rigid motion. You
select the motion as in Move/Copy:

- Free Move with its handles
- Translate
- Translate Along an axis or an edge
- Rotate about an axis or an edge
- Point to Point
- Point to Position

Each face keeps its surface, moved. The faces around it extend or
shorten along their own surfaces up to it. Thus an end face pushed out
makes the block longer. A top face turned about one of its edges tilts
the top. Sometimes the faces around cannot follow the motion: a face
would vanish, split or no longer meet its neighbor. Then MitCad
refuses the motion, and the panel says why.

Rotate's ring turns about the axis that the faces turn about, as in
Move/Copy. So dragging the ring turns the faces the way the ring shows,
even about a circular edge.

The panel's note and the selection card of its timeline item say what
it moves, for example "MoveFace1: 2 faces of Body1, translate (-1.856,
0, -0.361) mm". A double-click on the item edits it. This also applies
to an item that the `.f3d` import made for a file's moved faces.

### Fillet

**Fillet** (F) rounds edges in sets. If you select a face, Fillet rounds
all the edges of that face. Each set has its own Radius Type:

- Constant
- Chord Length: the width of the rounding across the edge
- Variable: a start radius and an end radius, and Mid Radii at
  positions from 0 at the start to 1 at the end
- Asymmetric: a distance on each face. The first distance is on the
  Reference Face
- Spline: control radii along the chain. The Knots run from 0 to 1, and
  each end is repeated degree + 1 times. The number of knots is the
  number of control radii plus the degree plus one. If Knots is empty,
  the knots are evenly spaced, with a degree of 3 at most.

The Radius field holds the first of these values.

*All Edges* rounds every edge of a body. Turn it on, then select the
body in the Body field that replaces Edges. The fillet also rounds the
edges that you add later, for example when you change a sketch. A fillet
with All Edges has only one set. Chamfer has the same option.

A rounding or bevel as large as the face beside it takes that face away:
a chamfer of 2 mm on the inner top edges of a wall that is 2 mm thick
and 2 mm high meets the outer wall and the floor at one edge each. If
the geometry kernel cannot make such a fillet or chamfer at its size,
MitCad makes it 0.1 % smaller and removes the narrow strip that this
leaves; a strip along a long chain of edges (more than 16 faces beside
it) stays, a thousandth of the size wide. The panel and the feature's
details then show this as information, not as a warning.

Where the roundings of two edges of a narrow face together are wider
than the face, they would meet beyond each other and the face would
vanish under them. The geometry kernel can build that only in some
cases, for example both rims of a thin plate. When it cannot, the error
names the face and the two edges, how far each rounding reaches into
the face and how wide the face is, for example "the roundings of the
edges reach 1.73 and 0.577 mm into a face between them, which is only
1.51 mm wide there". An eave 1.5 mm wide with an acute edge at its tip
is such a face for a radius of 1 mm. Use a smaller radius, or leave one
of the two edges out of the fillet.

At a difficult corner, the geometry kernel can build a fillet or chamfer
whose corner folds back on itself or ends in a spike. A rounding of
convex edges only takes material away, so MitCad looks at the faces that
the fillet or chamfer made. If one of them reaches out of the body (or,
for concave edges, into it) farther than 3 % of the radius, the feature
keeps its result and shows a warning that says what is wrong and how far
it reaches, for example "The corner of the fillet reaches 0.035 mm out
of the body … Check the result." Smaller deviations, which cannot be
seen, give no warning. Hold the pointer over the warning (in the
command's panel, on the Timeline, in the Browser or in the message
band): the view paints the faces and the point it is about, until the
pointer moves away. Check the result there; a smaller radius often gives
a clean corner. If the result is not a valid solid at all, the error says
which corner or face is at fault, and the pointer over it paints that
place too.

A fillet or chamfer of a hole's rim that runs over a neighboring face is
built as a ring about the hole's axis. That is no warning by itself: only
a rounding that leaves an edge or a sliver on the face it runs onto
warns.

In Fillet and Chamfer, click a selected face or edge again to remove it,
including while the preview is visible. The Edges input's ✕ clears that
input; a list item's ✕ removes only that item. Near a selected face's
boundary, the hover and click still target its edge. Click the face's
middle to remove the face.

Each set has arrows in the view, on its first edge. The arrows drag the
set's own values, as the fields do. The arrow of a constant Radius
points into the rounding.

A new fillet starts with the Radius of your last fillet, in any design:
the value that the first set had when you clicked OK. The first fillet
starts at 2 mm. If the remembered Radius is an expression that names a
parameter the design does not have, the fillet starts at 2 mm instead.
When you edit a fillet, its own values show, and neither the edit nor
Cancel changes the remembered Radius. MitCad keeps the value on this
computer, not in the design.

### Chamfer

**Chamfer** bevels edges in sets, in the same way as Fillet. Its Corner
Type sets what occurs where three bevelled edges meet:

- Chamfer: a small face of its own
- Miter: the bevels continue until they meet
- Blend: a smooth patch

Each set has arrows in the view, on its first edge. The arrows drag the
set's own values, as the fields do. The arrow of an Equal Distance
points into the middle of the bevel. With Two Distances or Distance and
Angle, the arrow of Distance runs along the face that Distance is
measured on. That face is the Reference Face, or else the edge's first
face. Flip takes the other face. The arrow of Distance 2 runs along the
other face. You type the angle.

A chamfer may be wider than a face beside its edge, such as a narrow
ring between a hole's rim and the cone of a thread's end chamfer. It is
built at its full size: the bevel goes on past the narrow face and is cut
by the faces beyond it, and the narrow face vanishes where the bevel is
wider than it. Where it stays narrower, the face stays. Where a wall rises
past the narrow face (also past a curved one, such as a flange below a
cylinder's band), the bevel ends at the narrow face's edge and the wall's
faces continue down to it along their own surfaces, a thread's flanks
too. Where such a chamfer meets other chamfers at a corner, the corner is
built as without the overflow. A chamfer of an inside corner, such as the
foot of an octagon standing on a disc, adds material: where it runs past
the narrow face beside it and the face beyond falls away, the face beyond
cuts it, so it never hangs out past the body's edge. Where the chamfer is
exactly as wide as the narrow face at one place, so that the face would
end in a point there, the chamfer fails and the error says where; a
slightly smaller distance builds. A chamfer is never made smaller than you
asked: where it cannot be built, the error names the edge, the narrow
face and the face beyond it, and at a corner the corner types that would
build it.

A new chamfer starts with the Distance of your last chamfer, in any
design, as a new fillet starts with the last Radius. Distance 2 and
Angle start with the values of the last chamfer that had them. The first
chamfer starts at 1 mm and 45 degrees.

### Shell

**Shell** hollows a body. In *Faces/Body*, pick the faces to remove:
they become the openings. Pick the body instead for a closed void inside
it. *Direction* puts the wall inside the body (*Inside Thickness*),
outside it (*Outside Thickness*) or both. *Shell Type* Rounded rounds the
outer corners of an outside wall; Sharp keeps them sharp. The faces of
the body keep their names, so later features on them still find them.

A part of the body that is no thicker than the wall stays solid, and the
cavity is only where the wall fits. For example, a cap with a flange
2 mm thick, shelled 3 mm from its open bottom: the cap becomes a cup with
3 mm walls, the cavity goes through the flange, and the flange stays
whole. A plate, a fin, a sleeve or a skirt thinner than twice the wall
stays solid in the same way, flat, curved or tapered, also where its
ends continue the body's sides (a keel as long as the body), where it
stands in a corner on two faces, or where it stands on a curved face. Roundings and
chamfers smaller than the wall do not stop the shell: the inside of the
wall runs past them, keeping at least the wall's thickness from them.

The feature then shows a warning with the faces where this happened:
"the wall does not fit under face …; it is left solid there", or "the
inside of the wall runs past face …". If the wall still comes out
thinner than given somewhere, the warning says where and how thin. If
the wall fits under no face at all, the feature fails with that message.
Make the wall thinner to hollow more of the body.

### Dogbone

**Dogbone** cuts reliefs at the inside corners of a body, for parts cut
with a router (CNC). A round router bit cannot cut a sharp inside
corner: it leaves material of its own radius there, and a square part
then does not fit into the pocket. A dogbone removes that material with a
cylinder of the bit's diameter at each corner, so the bit reaches the
sharp corner.

Pick in *Corners*:

- inside corner edges (straight edges where the material turns inwards);
- faces: the inside corners along the tool axis on each face. A face
  across the tool axis, such as the floor of a pocket, takes the inside
  corners that end on it;
- a body (in the browser): all of its inside corners along the tool axis.

*Tool Diameter* is the bit's diameter (6.35 mm, a quarter inch, by
default). *Extra* moves each relief that much beyond the sharp corner
(0.1 mm by default). *Type* sets where the relief goes:

- Diagonal: along the corner's bisector, into the corner;
- Along a Side (a T-bone): along one face of the corner, so that the
  relief notches the other face only. This is the longer face, or the
  face that you picked.

*Tool Axis* (Z, Y or X of the component) shows when you pick faces or a
body. Edges that you pick are taken in every direction. An edge that is
not straight or not an inside corner makes the feature fail, and the
message gives the reason. The body keeps its name, and you can edit the
values later.

### Tangent Chain

*Tangent Chain* is an option of Fillet, Chamfer, Shell, Draft and
Replace Face. It is on by default. With the option on, Fillet and
Chamfer also take the edges that continue a selected edge smoothly.
Shell, Draft and Replace Face also take the faces that are joined to a
selected face through smooth edges. The hover then shows the whole
chain before the click. With the option off, the hover shows only the
edge or the face under the pointer.

### Cutting across components

For a new **Extrude** with **Operation: Cut**, **Affected components**
lists the components that the current tool intersects. The names
include their assembly paths. Bodies that the root owns show under the
root. Uncheck a component to keep all its bodies unchanged. Or expand
its row to exclude single bodies. Hover over a row, or give it the
focus, to identify its geometry. The list scrolls independently of the
other command fields.

The preview obeys the checked targets. Exclusions stay when you change
the distance, the start, the direction or the profile. They also stay
for targets that are temporarily outside the cut. MitCad automatically
checks new eligible targets that the tool intersects, except what the
view hides. A component starts unchecked if its light bulb is off, if a
light bulb above it is off, or if Isolate leaves it out. A hidden body
of a shown component also starts unchecked.

The rows of hidden targets say **(hidden)**. You can still check them.
When you edit a cut, it keeps its choices, whatever is hidden later.
**Select none** means no targets, and it disables OK. **Select all**
explicitly clears the exclusions. The saved feature keeps its choices
when you edit it, reopen it, undo it or redo it.

Repeated occurrences of a shared component definition show disabled, with
a reason. Linked read-only components also show disabled, with a reason.
This cut cannot change only one occurrence of a shared definition. The
active component is different: a cut made inside it changes its own
definition, like its other features. So it can cut its own bodies even
when the component has several occurrences, and every occurrence shows
the result. Its row says so. Older
cuts keep their **Local bodies** scope when you edit them. When you open
an existing cut, MitCad does not add other component targets.

### Inspect: Measure, Annotations and Section Analysis

**Measure** (I) measures a distance, an angle, a length, an area or a
position. It also measures whole bodies. Use Measure for distances and
angles between items. Measure gives positions and distances in the
coordinates of the design, also for the parts of components. When you
pick two parallel planar faces or planes, the main *Distance* is the
perpendicular gap between their planes, including faces on different
bodies or component occurrences and a face against a construction plane. When
their finite boundaries do not overlap in projection, *Minimum distance*
below it gives the longer distance between their closest points. This
secondary row is omitted when the two values agree within 0.000001 mm.
The view draws the main dimension along the normal, starting at a face's
center when there is no common projection. Other pairs keep their
closest-point distance as the main result.

The Measure panel reads from top to bottom in the order you work: what
you pick, the result, what to do with it, and the history. The result
comes right under the picks. Between two picks their values come first,
the *Distance* in a larger type, right under it its *Δx*, *Δy* and *Δz*, its distinct *Minimum distance*, then the *Angle*, the
*Axis distance* and the *Center distance*; hover over the distance to see
the points it is measured between. Under them each pick
has a group titled by what it is, for example *Selection 1: cylindrical
face*, with its *Diameter*, *Radius*, *Area*, *Length*, *Volume*,
*Position*, *Axis through* and *Axis direction* as they apply. Click a
group's title to fold it. Right-click a value and choose *Copy* to copy
its number and unit. At the bottom is the history. *Close* (or Enter or
Esc) closes the panel. Measure only measures: to keep a measurement in
the design, use *Dimension* on the Make tab ([Annotate](#annotate)).

The *Selection Filter* of the Measure panel says what a click picks.
With *Faces, Edges and Vertices* (the default), the bodies come first:
where a shown sketch's point or line lies on a body's corner, edge or
face, for example a sketch in the middle plane of a body extruded to
both sides, a click picks the body's vertex, edge or face, and the
highlight under the pointer shows it before you click. A sketch's curves
and points are picked where no body is under the pointer; while you edit
a sketch, its own curves and points are picked as before. *Sketch Curves and
Points* picks only sketch geometry, also where it lies on a body.
*Bodies* picks whole bodies.

Measure also works with axes. When you pick a round face, a circle, an
arc or a construction axis, the panel shows its axis: the center and the
direction. The view draws the axis as a line while Measure is open.
Measure gives the angle between any two picks with an axis, an edge
direction or a plane. When one of the picks has an axis, Measure also
shows the *Axis distance*: between two parallel axes their distance,
between skew axes the length of the shortest line between them, from an
axis to a vertex or a parallel plane the distance square to the axis.
Between two circles or arcs, Measure also shows the distance of their
centers.

The Measure panel keeps a *History* of the measurements you take, newest
first. It is folded at the bottom of the panel: click *History* to open
it. Each row is one line: its number, its main value and what you picked
(`3.  6.843 mm   Face ↔ Face`); hover over a row to see all of it. Click
a row to show that measurement again. Right-click a row for *Show*
and *Copy*, which copies the row as tab-separated
text, for example into a spreadsheet. The *⋯* button at the end of the
history's title has *Copy All*, which copies all rows, and *Clear*,
which empties the history. The history stays while the design is open,
also when you close and open Measure again. It is not saved with the
design. A row whose geometry is gone is gray.

**Annotations.** To keep a measurement in the design, use *Dimension*
in the Annotate group of the Make tab ([Annotate](#annotate)): pick one
or two faces, edges, vertices or construction geometry and click *Add*.
The measurement becomes an *annotation*: a dimension drawn in the view
as on a drawing
(ISO 129 style, millimeters). MitCad names it after
what it shows (`Distance1`, `Angle1`, `Offset1`, `Diameter1`, …).
Sketch curves and points cannot be kept: pick the body's corners or
edges instead. The other commands of the Annotate group add other
annotations on one face, edge or vertex: *Note* adds a *leader note*, an
arrow to the pick and a text of one or more lines; *Datum Label*,
*Feature Control Frame* and *Hole Callout* are below. *Annotations…*
opens the Annotations window.

**Hole callouts.** Choose *Hole Callout* in the Annotate group of the
Make tab and pick a hole — its wall, its counterbore's or countersink's
face, or its rim — then click *Add*. MitCad writes the hole as a drawing does, from the hole
feature, in the symbols of ISO 129-1: how many holes the feature has
(`4x`), the drill's diameter (`Ø6.6`) or a tapped hole's thread (`M8x1.25
- 6H`, a custom thread's size and number of starts), `THRU` or the depth
(`↧ 15`), and on a second line the counterbore (`⌴ Ø11 ↧ 6.8`) or the
countersink (`⌵ Ø12 × 90°`). The arrow points at the hole's edge where
you clicked it. The callout follows the hole: change its size, depth or
thread and the text changes too. A round face that is not from a hole
feature gets its diameter. To add a position frame under it, as on a
drawing, select it in the Annotations window and click *Add Frame…*.

**Datum labels and feature control frames.** For geometric tolerances
(ISO 1101), choose *Datum Label* or *Feature Control Frame* in the
Annotate group of the Make tab and pick one face, edge, construction
plane or construction axis (not a vertex or a point: they mark a
feature). *Datum Label* adds a *datum label*: a
letter in a square on a leader that ends in a filled triangle on the
pick (ISO 5459). MitCad
offers the first free letter, A, B, C …; it skips I, O and Q, and each
letter is used once; type another letter in *Letter* to choose it.
*Feature Control Frame* adds a *feature control frame* on a leader
with an arrow to the pick: a row of boxes with the characteristic's symbol
(straightness ⏤, flatness ⏥, circularity ○, cylindricity ⌭, profile of a
line ⌒ or of a surface ⌓, parallelism ∥, perpendicularity ⟂, angularity
∠, position ⌖, concentricity ◎, symmetry ⌯, circular ↗ or total runout
⌰), the tolerance zone's width, and the datum letters it refers to in
order. Tick *Cylindrical zone (Ø)* for the zone of an axis, and choose a
*Material condition* (Ⓜ maximum, Ⓛ least material) where the
characteristic takes them. A form tolerance (straightness, flatness,
circularity, cylindricity) refers to no datum; parallelism,
perpendicularity, angularity, concentricity, symmetry and runout refer to
at least one. To put a frame under a dimension instead, select the
dimension in the Annotations window and click *Add Frame…*: the frame
stands just below the dimension line and moves with the dimension. A
frame that refers to a letter no datum label has, of the design or of a
drawing, shows a warning in the window. When you change a datum label's
letter, the frames that refer to it follow, in the design and in the
drawings. A frame refers to single datum labels: common datums (A-B)
are not supported yet.

**How annotations look.** Annotations are drawn as a technical
drawing's dimensions are (ISO 129-1, ISO 3098, ISO 128, ISO 1101), on a
light and on a dark background alike: technical lettering in the style of
ISO 3098 type B (a bundled font) with no box behind it, crisp thin
lines, small closed filled arrowheads that lie in the annotation's plane
like its text, extension lines that start a little off the edge, corner
or face you picked and run a little past the dimension line. Feature
control frames and datum labels are outlined in the same thin lines, two
text heights high; the datum label stands on a short stem square to its
face, ending in a filled triangle on the face (on a flat face, on its
outline as you see it, the triangle's base along it), a note's or a frame's
leader on a face ends in a dot, one on an edge in an arrow. A selected
annotation is drawn in the selection's blue, one under the mouse in the
hover's blue, with the same thin lines and text; the faces and edges it
measures are not filled in.

- *Color.* A new annotation is drawn in *Auto*: near-black on a light
  background or over a light body, a light gray over a dark background:
  a line takes the one that shows over what it crosses, a text the one
  for what lies behind most of it. In its editor or with *Color…*
  in the Annotations window choose *Auto*, *Black*, *Dark Gray* or
  *Custom…* for any other. Designs saved by older versions, whose
  annotations took orange, blue, green, magenta, violet and amber in
  turn, open with those in *Auto*; a color an annotation has of its own
  stays and is offered in the list. Snapshots and Render Image draw *Auto*
  for the background of the picture itself.
- *Text height.* All texts of a design's annotations are one height in
  millimeters, lying in the annotation's plane, so they grow and shrink
  with the design as you zoom, like the lines. *Auto* takes 2.5, 3.5, 5, 7
  or 10 mm by the size of the design. Choose another with *Text Height…*
  in the Annotations window. A text is drawn to the design's scale: a
  50 mm text is as tall as a 50 mm cube beside it at any zoom, and grows
  as you zoom in. Zoomed far out it stays at least about 9 pixels high,
  so that the values of a whole assembly stay readable; a leader's
  landing then grows with its text. Snapshots and Render Image draw the
  texts the same way; a drawing writes them in its own text height on
  the sheet.
- *Leaders.* A note's or a hole callout's text moved nearer its arrow
  than its landing is long takes a shorter landing, so that the leader
  never bends back past the arrow's point.
- *Reading direction.* A text reads from the bottom or from the right of
  the screen from any side you look at it: never upside down or
  mirrored; a vertical or nearly vertical dimension reads from bottom to
  top.

A dimension lies in a plane, not facing the screen: a length in a plane
of the part that holds it — a flat face at a picked point, or else a
coordinate plane through the points — the one most facing you when you
kept it; a radius in its circle's plane, an angle in the plane of its
sides; a diameter added in the view faces you as a note does, otherwise
in its circle's plane. A length's dimension line is placed outside the
body, on the side towards you when you kept it, six text heights clear
of its outline, so that its extension lines leave the body at its edge;
a length between two flat faces moves along them to their edge on that
side and to their far edge, so that it stands outside the part as on a
drawing. A second dimension parallel to it on the same side goes two
and a half text heights further out. A diameter is a leader from outside
the circle with its arrow on the rim, its text level on a short landing.
Notes, feature control frames and hole callouts put their text out of
the part the shortest way on an oblique leader (up and out to a side, or
down and out), with a short landing level with the first line, without
crossing those placed before it and off the dimension lines; their lines
are set flush left. A datum label stands on its stem off the face. A
frame added to a hole callout hangs under its lines, flush with them. *Automatic* in the editor places it so again. An annotation whose
plane you see edge on fades away. Drag the text of an annotation to move
it, or drag a length's dimension line (an angle's arc) to move it
farther from or nearer the measured points; a place you dragged it to is
kept. The places are kept relative to the picks, so the annotation
follows the design when it changes.

Double-click an annotation in the view to edit it: its name, kind,
prefix (Ø, R), decimals, your own text before or after the value or in
its place (`4 x`, `THRU`), its tolerance, color, group and whether it
is shown. A tolerance is symmetric (`±0.1`), deviations (`+0.2` over
`-0.1`), limits (the largest and the smallest size, `10.1` over `9.9`;
type them as the sizes in the editor), or an ISO 286 fit code (`H7` for
a hole, `g6` for a shaft) whose deviations MitCad can show in brackets.
Fit codes cover sizes up to 500 mm, the grades IT5 to IT11 and the
common holes and shafts (C to S, c to s); a radius takes the fit of its
diameter. A tolerance shows the decimals it needs, whatever the value
shows. A right click on an annotation offers *Edit…*, *Hide*, *Delete*
and *Annotations…*. While a command or a sketch is open, a click on an
annotation goes to the design as any other click.

The editor also shows what the annotation points at and its plane. Click
*Pick Again…* beside a pick to choose another face, edge, vertex or
datum in the view, or *Pick…* beside the plane to lay the annotation in
a planar face's or a construction plane's plane; *Automatic* gives the plane
back to MitCad. The editor closes for the pick, a banner says what to
pick, and the editor opens again after it. Press Esc to cancel the pick.
A radius, a diameter and an angle have no plane to pick: they lie in
their circle's plane or in the plane of their sides. A frame under a
dimension stands in the dimension's place; to put it on a leader, set
*Placed* to *On its leader* and click *Pick Again…* or OK. When it
stood on a vertex, a point or a whole body, MitCad asks you to pick the
face, edge or axis its leader points at first.

The **Annotations** window (*View › Annotations…*, *Annotations…* in
the Annotate group of the Make tab, or the browser's *Annotations*
folder) lists every
annotation: name, kind, value, tolerance, own text, color, group and
whether it is shown. Search it, filter it by kind or group, and sort it
by any column. Select several rows to change their color, group,
decimals or tolerance together, to show or hide them, or to delete them;
each change is one undo step. *Text Height…* sets the height of all the
design's annotation texts (*Auto*, 2.5 to 10 mm, or a height you type). Choose a group in the filter and click
*Hide Group* or *Show Group* to hide or show all of its annotations. A
row you select is highlighted in the view, and so are the faces, edges
or datums it points at, in its color; double-click it to edit it.
When a pick of an annotation is gone, its row shows a warning and says
why; the annotation comes back when the pick does.

The design file saves the annotations and their text height. Designs
saved with measurement objects open with them as annotations, with the
same values. A design
with dimensions kept from the view needs this version of MitCad: Keep
stores the plane and the places you saw them in, which older versions do
not read.

*View › Export Snapshot* saves a picture of the view, 2 or 4 times as
large as the view or a width in pixels, optionally with a transparent
background, with the shown annotations drawn over it: all of them, none,
or the shown annotations of one group. *Render Image* also draws the
shown annotations over the image it saves.

Outside a command, the selection card shows measurements of the
selection. When you select an edge, the card shows its length. For a
circular edge the card also shows the radius and the diameter. For a
selected face the card shows its area. For a body the card shows its
area and its volume. The values update after edits and after undo.

The card lists up to eight items. Hover over the text to show their
full geometry references.

**Section Analysis** cuts the bodies at a plane and shows the area and
the outline length of the cut. Pick a planar face or a plane under
*Faces/Plane*. *Distance* moves the cut along the normal of the plane,
and *Flip* cuts away the other side. To cut at a slant, pick an *Axis*
(a straight edge, a sketch line, a construction axis or an origin axis)
and type an *Angle*, or drag the ring around the axis. The plane turns
about the axis by that angle, and Distance then moves the turned plane
along its own normal. You do not need a construction plane for a
slanted cut. An Angle needs an Axis, and the axis must not be along the
normal of the plane. OK keeps the section in the Analysis folder of the
browser; Edit Section Analysis opens it again with its axis and angle.
A design with a turned section needs this version of MitCad.

When the Distance puts the Section Analysis plane past every body, the
panel shows a warning instead of the zero area: "it cuts them all away"
when the view no longer shows the bodies, "it cuts nothing" when it shows
them whole. Change Distance, or turn on Flip to keep the other side. A
section you keep in the Analysis folder tells it in the tooltip of its
row: "The plane does not cut any shown body." A plane that cuts only a
thin sliver off a body is a cut, and nothing is said.

Sometimes MitCad cannot cut a body at the plane. It then tries again
with a small tolerance, and then with the plane moved by 0.000001 mm,
which you cannot see. A plane through the axis of a revolved body, in
the plane of its profile, needs this. When no try works, MitCad shows the
body whole and tells you so once: the message band shows "Section1 could
not cut Body1; it is shown whole." (with several bodies, how many and the
first names), and the message history keeps the reason for each body.
The row of the section in the Analysis folder turns to the warning color,
and its tooltip names the bodies and the reasons. The warning goes away
when you hide the section or when it cuts the bodies again. The Section
Analysis panel and Hide Above Sketch tell it the same way.

**Section Analysis** draws a cap over every cut face. **Hide Above
Sketch** draws the same caps. Thin parallel lines at 45 degrees cover a
cap. A cap has the color of the component that owns its body. This is
the color of the band of that component in the Timeline. Thus you can
tell the components of an assembly apart on the plane.

The caps of the root component are orange. The lines are a darker shade
of the color of the cap. From one component to the next, the lines
alternate between 45 degrees and 135 degrees. The lines stay in every
visual style. They also stay when MitCad draws the bodies in Component
Colors, by appearance or rendered. Small caps get fewer lines, closer
together. MitCad does not fill large caps with lines.

### Physical Properties and Center of Mass

**Physical Properties** (Inspect group) opens the Properties window.
For each body it shows the Material, the Density, the Mass, the Volume,
the Area, the Center of Mass and the Principal Moments. A body in a
component counts where its occurrence places it, with the material it
has there. If a component is placed more than once, its bodies count
once for each placement: the window shows a group for each, named by the
occurrence, for example *Body1 in Arm:2*. It also shows:

- **Bounding Box**: the overall size of the body along the X, Y and Z
  axes of the design, for example 40 × 25 × 20 mm, where its occurrence
  places it.
- **Faces / Edges / Vertices**: how many faces, edges and vertices the
  body has, for example 6 / 12 / 8 for a block. An edge that two faces
  share counts once. A mesh body (from an STL or OBJ file) has
  triangles, not faces, so the window leaves this row out for it.

Under **Total** the window shows the Mass, the Volume and the Area of
all the bodies together, their Center of Mass, the Bounding Box around
all of them, and the sums of their faces, edges and vertices. With
nothing selected, the total Mass and Center of Mass are the ones that
Center of Mass below shows.

What the window covers:

- Select bodies first (or a face, an edge or a vertex of a body) to see
  only those bodies.
- Select a component in the view or in the browser to see its own
  bodies, the bodies of its sub-components included. The title of the
  window names the component, for example *Properties: Arm*. The window
  counts the bodies only where the occurrence that you selected places
  them.
- Select nothing to see every body of the design.

A body without a material counts as steel (7.85 g/cm³); its Material
row says steel. The Center of Mass rows give the centers in the
coordinates of the design.

MitCad computes the Volume, the Area, the Mass and the Center of Mass
from the exact surfaces of the body, not from the mesh that the view
draws. It integrates each face piece by piece, so the values come within
about a millionth of the exact ones, also for lofts and the shells of
lofts. Every place that shows them gives the same values: this window,
Measure, the Bill of Materials window, the print Estimate and the
properties of the body.

**Center of Mass** in the Inspect group marks the center of mass of the
whole design in the view. The marker is a crossed circle that keeps its
size on screen and shows through the bodies. Every body counts where it
is placed and with the material it has there: a component placed twice
counts twice. The marker follows every change, such as a recompute, a
component that you move or a material that you change. Rest the pointer
on the marker, or on its row in the browser, to see its coordinates in
millimeters, the total mass, and how many bodies count as steel because
they have no material.

While the design has the marker, the browser shows the row **Center of
Mass** right under Origin. Its light bulb shows and hides the marker.
**Delete** in the row's context menu removes the marker; **Center of
Mass** adds it again. The design saves the marker and its light bulb.

## Sketches

This chapter tells how you draw sketch geometry, how you constrain and
dimension it, and how you select it in sketch mode.

### Sketch mode

**Create Sketch** with nothing selected waits for a plane. A banner at
the top of the view says so. The banner stays until you select a plane
or a planar face, or until you press Esc. While **Create Sketch** waits,
other actions also say so.

The view always shows the origin planes for the pick, also when the eye
of the Origin folder in the Browser is off. The eye does not change:
when the pick ends, the view shows the origin as the eye says. The
planes lie behind the bodies for the pick. Where a face is in front of a
plane, or a plane runs through a body, a click selects the face. A click
on a plane where there is no face selects the plane. You can also click
**XY**, **XZ** or **YZ** in the Origin folder of the Browser.

Some faces cannot take a sketch: a cylinder, a thread or another curved
surface, or a face that the design policy leaves out. If you click such
a face, MitCad says why. Then select a planar face or a plane, or make
one with Construct.

A new sketch opens with the view looking at its plane and the design's
Z axis pointing up on the screen. This holds on the XZ and YZ planes and
on every face that is not horizontal. On the XY plane and on horizontal
faces the design's Y axis points up. The sketch's own x and y axes
follow the screen: x points right and y points up. Thus a Horizontal
line is horizontal on the screen. You look at a sketch on XY from the
top, on YZ from the right and on XZ from the front (the faces TOP,
RIGHT and FRONT of the orientation cube), with the design's X axis
pointing right on XY and XZ.
You look at a sketch on a face from outside the face. An extrude of a
new sketch on XZ goes towards the front by default; type a negative
distance or turn on Flip to go towards the back. A sketch that you edit
again keeps the orientation it was made with, also a sketch on XZ made
in an earlier version, which opens from the back.

While you edit a sketch, the timeline at the bottom shows the sketch's
own items instead: its points, curves, constraints and dimensions in the
order you made them, each with its icon. A curve says its kind and
whether it is construction; a dimension shows its value. Click an item to
select it in the view, press Delete to delete it, and double-click a
dimension to change its value. An item in conflict, or one that the
others already imply, has the same mark as in the Sketch Palette. The
list has no marker: a sketch is solved as a whole, so you cannot roll
it back. When you finish the sketch, the timeline comes back.

On a planar face, the face's edges also bound the sketch's profiles.
For example, draw an octagon inside a circular face: the area between
the circle and the octagon is a profile that you can pick in Extrude or
Revolve. A line across the face splits it into two profiles. These areas
shade and highlight like the sketch's other profiles. You do not need
Project to make the face's edge a profile boundary; Project is still
useful when you need a sketch curve to constrain or offset. Construction
curves do not bound profiles, and closed regions drawn outside the face
remain available. The whole face can still be selected as a face.

This also works on a planar Patch made from sketch profiles. Its free
edges bound profiles without adding projected curves to the new sketch.
For example, a circle inside a square Patch gives a disc and the square
outside it. Extrude that outside region with New Body to make a solid;
the supporting Patch remains a surface body. Changes to the source
sketch's dimensions move the Patch's boundaries and update the regions.
The references survive saving and reopening the design. A boundary with
no usable source reference, or one made ambiguous by a later operation,
reports a warning and leaves the sketch's own regions available.

The boundaries follow changes to the body. Redefine Sketch Plane onto
another plane, or offset the sketch off the face, and only the sketch's
own regions remain. A feature using a lost face-bounded profile reports
the missing profile so that you can select a replacement.

The Sketch tab has three groups:

- Create: the sketch curve tools (refer to [Sketch curve tools](#sketch-curve-tools))
- Modify: fillet, chamfer, Trim, Extend, Offset, Move/Copy, Edit
  Pattern, Edit Offset, Edit Text, construction, center line,
  Convert to Sketch Geometry, Delete, Cut, Copy and Paste (refer to
  [Copy, Cut and Paste](#copy-cut-and-paste))
- Constraints: the constraint buttons (refer to
  [Constraints](#constraints))

The sketch palette is on the right of the view. It shows the degrees of
freedom, the options, your most used sketch tools and the constraint
buttons. The options are construction, signed distances, snap to grid,
options to show dimensions, constraints, points and profiles, Hide
Above Sketch and Look At.

*Show points* draws the end points, center points and sketch points.
Turn it off to read a dense sketch more easily: the points are then not
drawn, but the point under the pointer still shows, the tools still snap
to points, you can still drag and select them, and selected points are
drawn. The setting is kept for the next sketches.

*Show profiles* shades the closed regions of the sketch, which Extrude
and Revolve take. Each sketch opens with it on. To have sketches open
with it off, turn off **Show profiles when a sketch opens** in
Preferences › Sketch. *Show profiles* in the palette still turns the
shading on or off, for the sketch that is open only.

The *Create* section of the palette, above *Constraints*, has two rows
of buttons for the sketch tools that you use most, for drawing and for
modifying. Before you have used the tools, it shows Line, Rectangle,
Circle, Arc, Polygon, Slot, Spline, Point, Trim, Extend, Offset, Fillet,
Mirror and Sketch Dimension. MitCad counts each start of a tool in a
sketch, from the toolbar, the palette, a shortcut or the command search,
and keeps the counts in your local settings. The tools that you pinned
on the Sketch tab (refer to *Pinned commands* in
[Commands and shortcuts](#commands-and-shortcuts)) come
first, and the tools that you start most fill the rest. The buttons are
in the order of the toolbar: the Create tools, then the Modify tools.
MitCad chooses them again only when the palette opens for a sketch, so
the buttons do not move while you sketch. The button of the tool that
is on shows checked, as on the toolbar. The tooltips and shortcuts are
the same as on the toolbar. The constraint buttons are on two rows.
When the palette is longer than its panel, it scrolls, and *Finish
Sketch* stays at the bottom of the panel, always in sight.

*Hide Above Sketch* cuts the bodies at the sketch plane. Thus other
parts of a body do not hide a sketch on a face below them. You cannot
select the cut bodies while they are cut. MitCad draws the faces where
it cuts the bodies in a color of their own, with hatches, as in Section
Analysis. The bodies stay cut while the panel of a sketch command is
open and its preview is shown (Offset, Mirror, the patterns, Project,
sketch Fillet and others). The option stays on for the next sketch.

The sketch's X and Y axes run across the view through the sketch's
origin. The origin has a ring. The sketch always shows its origin and
axes, also when the eye of the Origin folder is off. The sketch does not
change the eye, so the Origin folder is as
before when you finish the sketch. The same holds when you edit a sketch
again. The tools snap to the sketch's origin and onto its axes, and
Sketch Dimension and the constraints can use its origin and axes (refer to
[Dimensions](#dimensions)). They belong to the sketch: you cannot select
them outside the sketch or as the geometry of a body.
A sketch can have an origin that is not the design's: a sketch on a
face or a construction plane, or in a component put elsewhere. Such a
sketch names the place of its origin in the design beside the origin.
It also marks the design origin as seen along the sketch's normal.

Above the status badge, the view shows the position of the pointer in
the sketch's coordinates. The position is in the design's length
unit. It is the point where a tool snaps the pointer.

The colors of the geometry tell its state:

| Geometry | Color |
| --- | --- |
| Fully constrained geometry | Dark (light on a dark background) |
| Free geometry | Blue |
| Construction geometry | An orange dashed line |
| Projected geometry | Purple |

MitCad draws fully constrained geometry dark, or light on a dark
background. This also applies to the sketches shown when you do not
edit a sketch. Thus one look tells you which sketches are fully
constrained. Fully constrained curves and center lines have a thin
outline in the background's tone: dark under light lines, light under
dark lines. Thus they stay readable where they cross a body's face. When
you drag a point or a curve, it moves as far as the constraints permit.
When you drag one of several selected curves or points, the whole
selection moves together and keeps its shape, as far as fixed geometry
and constraints to other geometry permit. A drag that starts on geometry that is not selected moves only
that geometry. The drag is one undo step.

When you drag a single point, such as the end of a line, it snaps to
points, curve ends, midpoints and curves as a point you draw does, and
the snap marker shows where. Drop it there and it stays joined: dropped
on a point it becomes coincident with it, dropped on a curve it stays on
that curve. So you can close an open chain of lines by dragging its last
end onto its first. Undo takes back the drag and the join in one step.
When the join would conflict with the sketch's other constraints, the
point is not joined, and the status bar says why. Hold Shift (or Alt,
where your desktop leaves Alt-drags to MitCad) to drag without
snapping.

While a tool is active, the pointer is a slim cross with the tool's
icon. A small square in the center gap of the cross shows that the
point snaps to geometry.

You can type values while you draw. Fields show next to the cursor. Tab
moves between the fields. Enter accepts them. Typed values become
dimensions. This also applies to expressions.

The fields stay clear of the pointer, so a click always places the next
point where the pointer is. A field takes clicks of its own (also on its
**fx** button) after you type into it or move to it with Tab. Until then,
a click on it places the point there, as a click in the view does.

Some commands start with a selection, for example Offset, Fillet and
Extrude. As soon as you have made the necessary selections, such a
command gives the keyboard to its first value field, with its text
selected. Thus you can type the number without a click. Clicks in the
view continue to work. A field that you clicked into keeps the
keyboard.

While the selection takes more items, for example Fillet's edges or the
profiles, the next click in the view adds the next item. In that case,
the labels of dimensions and values show only after you type in the
field. Also, the view does not move when the preview shows a design's
first body.

Sketch mode has these limits:

- You cannot add constraints to the curves of an offset of an ellipse
  or a spline.

#### Printing a template at 1:1

File › Print… (Ctrl+P) prints the open sketch, for example a template to
cut or drill along. The sketch is printed as you see it square on: x to
the right, y up, in black. Construction lines are dashed.

The **Print Sketch** panel comes first:

- **1:1, tiled over pages** prints the sketch at its size. A sketch larger
  than the paper is printed on several pages. Each page overlaps the next
  by 10 mm and says its row and column (*Row 1, column 2*). Dashed lines
  mark both edges of each overlap, with crosses on them. Lay the next page
  over the previous one so that the crosses meet, and tape the pages
  together.
- **Fit to page** prints the sketch as large as it fits on one page.
- **Paper** and **Orientation**: the orientation follows the paper and
  the scale, the one with fewer pages at 1:1, until you choose it.
  MitCad remembers the paper.
- **Dimensions** and **Constraints** add the sketch's dimensions and its
  constraint symbols. Both are off by default.
- **Pages** says how many pages the print takes.

**Print…** opens the system's print dialog for the printer and the
copies. A page range there counts the pages the panel showed. If you
choose another paper in the print dialog, MitCad prints the pages of that
paper that hold the same parts of the sketch. Check the printer's
settings: a printer that scales to fit its
paper (*Fit to page* or *Shrink* in some print dialogs) does not print at
1:1. Measure a known line on the first page before you cut.

### Sketch curve tools

The Create menu of the Sketch tab groups the sketch curve tools:

| Entry | Tools |
| --- | --- |
| Lines | Line (lines end to end, with tangent and normal arcs) and Midpoint Line |
| Point | Point (a sketch point; an entry of its own, not a submenu) |
| Rectangles | 2-Point, 3-Point and Center Rectangle, 2-Point and Center Rounded Rectangle |
| Circles | Center Diameter, 2-Point, 3-Point, 2-Tangent and 3-Tangent Circle |
| Arcs | 3-Point, Center Point and Tangent Arc |
| Other Geometry | Ellipse, the polygons, the slots and the splines |
| Text | Text, QR Code |
| Patterns | Rectangular Pattern, Circular Pattern and Mirror |
| References | Project, Intersect, Clean Up Projections, Clean Up Stale Projections and Project Again (references from the bodies) |
| Sketch Dimension | Sketch Dimension |

On the toolbar, Point and Sketch Dimension are buttons. Point has no
keyboard shortcut; the command search finds it. Lines, Rectangles,
Circles and Arcs are split buttons. The button starts the default tool.
The arrow of the button opens the other variants.

| Split button | Default tool |
| --- | --- |
| Lines | Line |
| Rectangles | 2-Point Rectangle |
| Circles | Center Diameter Circle |
| Arcs | 3-Point Arc |

The keyboard shortcuts and the command search reach every tool as
before.

**Ending a chain or a tool.** The Line tool draws lines end to end until
you end the chain: double-click, press Enter or Esc, click the chain's
last point again, or right-click in the view. A right-click while a
sketch tool is on does what Esc does in that tool, without a menu: the
first right-click ends the chain or the shape in progress and keeps the
tool, so that the next click starts a new chain. A right-click with
nothing in progress ends the tool. A right-button drag is not a click:
when your navigation moves the view with the right button, the drag
orbits or pans the view as before and the chain goes on. With no tool
on, a right-click opens the sketch's context menu as before.

**Arcs from the Line tool.** Press on the last point of the chain, or,
to start a chain, on the end of a line or an arc, and drag: the Line
tool draws an arc instead of a line, and releases it where you release
the button. The way you drag from the point chooses the arc. Drag along
the curve (within 45 degrees of the way it leaves the point) for a
tangent arc, which continues the curve smoothly. Drag across the curve,
or back, for a normal arc, which leaves the point square to the curve on
the side you drag to. The choice is made once the pointer is a little
way from the point; to choose again, move back over the point and out
again. The preview shows the tangent or perpendicular symbol at the
point. From the end of a spline the arc is always tangent. While you
drag, type the Radius, or press Tab and type the Sweep: the arc keeps
that radius (it becomes the arc's radius dimension) or that sweep, and
the pointer places the rest. After the arc, the chain goes on from its
end. Press Esc during the drag to drop the arc and go on with lines. A
click on the end of a line or an arc, without a drag, starts a chain of
lines there as before.

**Midpoint Line.** Midpoint Line (in the Lines split button) draws one
line from its middle: click the midpoint, then one end. The line runs
as far on the other side, and the midpoint stays a sketch point at the
middle of the line (a Midpoint constraint), so the line stays symmetric
about it when you drag an end or change its length. Type the Length (of
the whole line) or the Angle (from the sketch's x axis) as for a line. A
line drawn level or plumb gets Horizontal or Vertical, and the points
that you click on geometry stay on it.

A polygon (Circumscribed, Inscribed or Edge Polygon) stays regular. Its
construction circles keep the sides equal and the corners evenly
spaced. Thus when you drag a corner, the whole polygon rotates, moves or
changes its size, and it does not skew. The same occurs when you rotate
the polygon with Move/Copy (Rotate). MitCad refuses a dimension that
would make the polygon irregular.

A typed Radius becomes a driving dimension and parameter on the
construction circle: the inner circle for Circumscribed Polygon, the
outer circle for Inscribed Polygon. A typed Length in Edge Polygon
dimensions the edge between the first two clicks. The polygon and its
dimension undo together. The Sides count does not add a dimension, and
a size placed only by clicking stays free.

The center and the orientation stay free until you constrain them, as
does a size you have not typed. Fix the center or set its place,
dimension a circle or a side, or make a side horizontal.

**Rounded rectangles.** 2-Point Rounded Rectangle and Center Rounded
Rectangle draw a rectangle with rounded corners, as 2-Point Rectangle
and Center Rectangle do. After the first click, a third field, Corner
Radius, follows Width and Height (Tab moves to it). If you do not type a
radius, the tool uses the last radius that you used with it, at first
2 mm. The result is four lines and four arcs that are tangent to them.
The lines are horizontal and vertical, the arcs are Equal, and one
radius dimension sets all four. Change that dimension and all four
corners change. The sharp corners stay as points on the lines, so a
typed width or height measures the whole rectangle from corner to
corner. Thus a rounded rectangle is fully constrained by its width, its
height, its radius and its position. MitCad refuses a radius of half the
shorter side or more, and tells why.

**Closed splines.** Fit Point Spline draws a smooth curve through the
points that you click. Press Enter or double-click to end it. To close
it, click its first point again (after at least three points). The
preview shows the closed curve and a ring on the first point when a
click there closes the spline.
A closed spline is one curve without ends. It is smooth where it closes,
with no corner. It is one profile, so you can extrude it. Offset makes a
closed curve around it, and Trim keeps the rest of it between the curves
that cross it.

**Insert Point on a spline.** To add a point to a spline, right-click
the spline where the point must go and choose **Insert Point**. You can
also select the spline and then double-click it at that place. This is
one step that Undo takes back.

- A Fit Point Spline gets a new fit point at that place, between its two
  neighbors along the curve. The curve still goes through all its fit
  points. Between the neighbors of the new point, the curve can change
  its shape a little.
- A Control Point Spline gets a new control point, and its shape does
  not change. The control points next to the new one move a little to
  keep the shape. The shape changes only when you drag a point.
- Closed splines work the same way. The existing points keep their
  constraints and dimensions.

**Trim** (Modify) removes the piece of a curve that you click, up to
the curves that cross it. The new ends obey these rules:

- If a new end lies on the end of another curve, or on any existing
  point, the end joins it. Thus when you trim both curves at a crossing,
  one after the other, they end in one point. This also works where two
  curves touch (are tangent).
- If a new end joins a curve that it meets tangentially, the two curves
  get a tangent constraint, as when you draw tangentially. A tangent that
  held the removed piece's end to a curve that no longer touches the
  trimmed curve is removed in its place. Thus when you trim a slot's arcs
  where they touch the sides of a second slot, the arcs stay tangent to
  the sides, and no stale tangent to the old top is left.
- If a new end lies in the middle of a curve, the end stays on that
  curve.
- A projected point holds the new end with a coincident constraint.

**Extend** (Modify) extends a curve at one end. The extended end obeys
these rules:

- If the end reaches the end of another curve, or any existing point,
  the end joins it.
- If the end reaches the middle of a curve, the end stays on that
  curve.
- A projected point holds the extended end with a coincident
  constraint.
- The end keeps its point, its constraints and its dimensions. But the
  dimensions of its length do not stay. A distance to the end measures
  the new place.

A Coincident constraint between the ends of two curves makes the two
ends one point, as if you had drawn both curves from that point. Thus
Offset and chain selection see the corner as joined. If one end is
fixed, MitCad keeps that end. MitCad removes a distance between the two
ends. A point and a curve, a center, a lone point or a projected point
stay a coincident constraint.

**Sketch Fillet** (Modify) rounds corners of lines and circular arcs
with arcs that are tangent to both curves. Select what to round in
Curves or Corners:

- Two lines: the corner where they meet.
- A line and an arc, or two arcs: the corner where they end together.
- More curves, for example a whole rectangle or a whole profile of
  lines and arcs (also selected with a window): every corner where two
  of the curves end together. Where a line runs smoothly into an arc
  (tangent), there is no corner to round.
- Corner points: the corner of the two lines or arcs that end at each
  point. If a line and an arc end together at both of their ends, as in
  a half circle, select the corner point.
- A corner where a curve's end is held on the other curve right next to
  that curve's own end, as Trim leaves corners, counts as a corner too.

Each corner gets an arc on the side where the two curves enclose the
corner. One radius dimension sets them all, and the other arcs are
Equal to the first. Undo takes back all the corners in one step. The
sharp corner stays as a point on both curves, and a length dimension of
a line becomes the distance to that point, so the dimensions keep their
meaning. An arc that is rounded keeps its center and radius. If the
radius does not fit on a curve, MitCad names the curve and the largest
radius that fits. Ellipses and splines cannot be rounded.

**Offset** (Modify) copies a chain of curves at a distance. With Chain
Selection, a click on one curve also takes the curves that are joined
to it end to end. Sometimes a curve lies on another curve, for example
the edge that two projected faces share, or an edge that an earlier
version of MitCad projected again after its face. Offset then uses the curve of the two that is joined to the other
curves, so that the whole outline is offset. If you select both curves,
Offset uses one of them, and the panel tells which. If the curves are
not one chain, the panel tells why: two ends at the same place that are
two points (a Coincident constraint joins them), or three curves that
end at one point. If an open chain stops where another curve ends on a
point of its own, the panel tells that too.

When the offset goes toward the inside of a bend, some curves of the
copy can vanish: an arc whose radius is smaller than the distance, or a
short curve whose neighbors' copies cross before they reach it. Offset
leaves those curves out and cuts or extends the copies on either side to
where they cross, so the copy is the clean outline on its side. For
example, a hexagon with 2 mm rounded corners offset 2 mm or more toward
the inside gives a hexagon with sharp corners. The preview shows the
result as you type the distance, and the panel names the curves that
vanish as a note, for example *corner arcs c4 and c7 vanish at this
distance*. An open chain keeps its ends. If the copies on either side do
not meet, for example the parallel sides of a slot offset inward past
the radius of its ends, the panel names the curve that vanishes and the
offset is not made.

When you later change the distance with Edit Offset, or change the
source curves, so that other curves vanish or come back, MitCad makes
the offset again. The copies that stay keep their names, so dimensions,
constraints and profiles that use them keep working. A copy that comes
back gets a new name. If you had put a constraint or dimension on a copy
that vanishes, MitCad removes it and tells you which. This also happens
when the distance is a parameter you change in the parameters table, or
when projected curves follow a changed body: the sketch shows the offset
made again, and the timeline's information names the curves that
vanish.

The same holds for parts of the copy away from the corners: if the copies
of two walls of a narrow neck pass each other, the walls are left out and
the curves on either side meet. If a narrow notch in a straight side
closes, the two pieces of the side become one line. A closed outline
whose copy falls apart, for example across a waist narrower than twice
the distance, gives one offset with two separate outlines: a dog bone
offset inward past its waist gives two rectangles. The panel tells you
in a note. Each outline is fully constrained when its source is.

An open chain falls apart the same way: where the copies on either side
of what vanished do not meet, the copy ends on both sides and the offset
keeps both parts. If a narrow notch in a circle closes, the two arcs of
the circle become one, and a circle with one notch gives a whole circle.
If the copies of two walls cross each other inside a narrowing neck, for
example a dog bone whose waist is a trapezoid, the copy is split where
they cross and the part beyond is trimmed, so no copies cross. If a part
of the outline is too small to keep any copy of its own, for example a
lobe of a dog bone narrower than twice the distance, the offset leaves
it out, keeps the rest and tells you in a note, for example *the offset
leaves out c5, a part too small for this distance*.

If you offset the copy again, and the first offset later changes so that
some of its curves vanish or come back, MitCad makes the second offset
again from the first one's new curves.

**Rectangular Pattern** (Create › Patterns) copies the curves and points
of Objects in rows and columns. The first direction is Direction: a line
of the sketch that you pick, or the sketch's x or y axis (click the axis
anywhere along the line drawn across the view; the field then names the
axis). When you pick nothing, Direction is the x axis. A sketch that has
only circles or points can thus copy them along y too. The second direction
is not picked: it always runs across Direction, at right angles to it in
the sketch (a quarter turn counterclockwise from Direction, so the y
axis when Direction is the x axis). Quantity and Spacing set the copies
along Direction, Quantity 2 and Spacing 2 those across it. Quantity 2
starts at 2, so the preview shows a grid at once; set it to 1 for a
single row. In the view, an arrow at the first copy along each direction
shows its spacing: drag an arrow to change Spacing or Spacing 2. The
arrows move in steps of the grid; hold Shift or Alt to drag freely.
While Quantity 2 is 1, the second arrow is faint: it only shows where a
second row would go. Flip and Flip 2 copy against a direction, and a
Symmetric Direction Type puts the copies on both sides of the originals
(an odd quantity). Edit Pattern changes the quantities and spacings
later.

**Mirror** (Create › Patterns) copies the curves and points of Objects
to the other side of the Mirror Line. Each copy gets a Symmetry
constraint, so it follows its original when you change the original.
The copies share the points that lie on the line. The Mirror Line can
be a line of the sketch, also a construction line, or the sketch's x
or y axis: click the axis anywhere along the line drawn across the
view. If an axis is not yet in the sketch, MitCad adds it in the same
undo step. If Objects also contains the Mirror Line (for example after
a window selection), MitCad leaves the line out. When you select
objects before you click Mirror, Mirror Line is active at once.
Otherwise, click Mirror Line in the panel after you select the objects.

**Move/Copy** (Modify, **M**) moves or copies the selected curves and
points. Move Type has three choices:

- **Translate** moves them by X and Y Distance. X and Y Distance start
  at 0, so nothing moves when the panel opens; the selection moves as
  you type a distance. You can also drag the handle at the middle of the
  selection: drag an arrow to move along that axis, or the square
  between the arrows to move freely. The handle snaps as the drawing
  tools do: on a point or a curve, the middle of the selection goes
  there; on the grid, the distances go in grid steps. Hold Shift or Alt
  to drag without snaps.
- **Rotate** turns them about a Pivot Point by an Angle (default
  90°). If you do not pick a Pivot Point, they turn about their own
  middle. Horizontal and vertical constraints of the turned geometry
  turn with it: a quarter turn makes a horizontal line vertical. At
  other angles, the lines are held parallel or perpendicular to each
  other instead, and the panel says so.
- **Point to Point** moves them from the Origin Point to the Target
  Point. Pick each in the view: the pointer snaps to points, line ends,
  midpoints, centers, quadrants, intersections and the grid, as when you
  draw. To use a point of another sketch, project it first.

A move keeps the constraints; Create Copy leaves the originals where
they are. When the Target Point snaps to a point or a curve, the Origin
Point (or its copy) is held there with a constraint, as a point you draw
there would be; the same when you drop the handle on a point or a curve
and a point of the selection sits at the handle (a circle's center, a
single point). If you type another distance afterwards, that hold is
dropped.

**Convert to Sketch Geometry** is in Modify and in the context menu of
projected geometry. It makes the selected projected curves and points
ordinary sketch geometry. They become free, and they no longer follow
their source. Projected curves that you do not convert can also use a
corner of a converted curve. Such a corner stays projected, and the
converted curve stays joined to it.

### Snaps and auto constraints

The cursor snaps to points, centers, the midpoints of lines and arcs,
quadrants, intersections, curves and the origin. With the snap to grid
option, it also snaps to the grid. A snap adds a constraint: wherever
the snap marker shows, the point you place there is held there, also
when you drag a point, paste, use Move/Copy, drag a text's frame, or
end an arc on a point. When a
line is nearly horizontal, vertical, perpendicular, parallel or tangent,
it gets that constraint.

**Constraints the drawing already satisfies:** a point that snaps to a
quadrant of a circle, an arc or an ellipse stays there. It is on the
curve, and level or plumb with the center. When the line tool comes
near the point where a line from its start touches a circle or an arc,
the end goes to that point, and the line is tangent there. When you
finish a line, an arc, a rectangle or a slot, MitCad also adds the
constraints that the drawing already satisfies:

- Horizontal or vertical for a line that is exactly level or plumb, or
  that the line tool made so. A slot's center line gets this too.
- Tangent where the end of a line or an arc lies on a circle or an arc
  and runs along it there. An arc also gets a tangent to a line in the
  same way.
- Perpendicular or parallel, as the line tool shows them.

MitCad adds such a constraint only when it is safe: the other
constraints must not already give it, the sketch must still solve, and
no point may move. Otherwise MitCad leaves it out, without a message.
Thus the sketch gets no redundant or conflicting constraints, and the drawing does not change.
All of this is in the same undo step as the new entity. A point that
does not snap (a free click or the grid) gets no such constraint at its
end. The *Auto constraint* section of the palette works as before.

**On the axes:** every drawing tool also snaps onto the sketch's X and Y
axes, along their whole length across the view, with the marker of a
curve. Thus a circle's center placed on an axis stays on it. A click
there holds the point on the axis with a Coincident constraint, as on a
construction line. This is one undo step with the rest of the click.
While the pointer only hovers, MitCad adds nothing. The sketch's own
curves and points and the origin win when they are under the pointer
too. With the snap to grid option, the point keeps to the grid along
the axis. A line drawn along an axis gets only the constraints it needs:
a line from the origin along the X axis is horizontal, and its end stays
on the axis. Horizontal and vertical are the directions of the axes, so
the line tool needs no axis to infer them.

**Point on a body:** the Point tool also snaps to the corners of visible
bodies that lie in the sketch plane. It also snaps to the middles of
their edges in the plane (lines and arcs). This includes the bodies of
other components. The marker and the hint are those of a point or a midpoint.
When a body and the sketch's own geometry are both under the pointer,
the sketch's geometry wins. While the pointer only hovers, MitCad adds
nothing.

A click projects the corner or the edge into the sketch, linked, as
Project does it. Then it adds the point and holds it on the projected
corner or at the middle of the projected edge. This is one undo step.
Thus the point follows the body when the design changes. If the sketch
already has a projection of the same corner or edge, MitCad uses that
projection again. A click on a corner that already has a point does
nothing.

**Auto constraint:** when the line, circle or arc tool has drawn its
first entity, the palette shows an *Auto constraint* section. The
section constrains each next entity of the same tool to a reference
entity. *Link to first* uses the first entity that the tool drew. *Link
to last* uses the entity drawn just before. *Link to last* is the
default.

Circles and arcs offer these constraints:

- *Equal* (radius)
- *Horizontal/Vertical alignment* (the centers level or plumb, whichever
  they are nearer to)
- *Concentric*

Lines offer these constraints:

- *Equal* (length)
- *Parallel*
- *Perpendicular*

Thus *Equal* and *Perpendicular* with *Link to last* draw square corners
with equal sides. MitCad makes the new entity where the checked
constraints put it. The preview shows the entity, with the reference
highlighted. The preview also shows while the pointer is over the
section. The entity gets the constraints in the same undo step. The
choices apply only to the entities drawn next and last, until the tool
ends.

If the sketch refuses a constraint, a dialog opens that lists the
constraint. *Cancel* removes the entity. *Accept* keeps the entity
without that constraint. Tangent circles and tangent arcs take no auto
constraints.

### Dimensions

Sketch Dimension (D) makes a dimension of the selected geometry. The
value fields that show while you draw also make dimensions (refer to
[Sketch mode](#sketch-mode)).

**The first dimension scales the sketch:** you can draw a sketch at any
size and then give it its true size with one dimension. When you add
the first dimension to a sketch, MitCad first scales the whole sketch
about the sketch origin so that the dimension holds. Then it adds the
dimension. For example, draw a rectangle 10 × 5 and give its long side
100: the rectangle becomes 100 × 50. The sketch keeps its shape and its
constraints. Text heights scale too. The scale and the dimension are one
undo step. A message tells the scale factor.

- This applies to a length, a distance, a radius or a diameter. It does
  not apply to an angle or to a driven dimension.
- MitCad scales only a sketch that has no other dimensions, offsets or
  rectangular patterns, no projected, intersected or linked geometry,
  and no fixed points other than the origin. In all other cases the
  dimension moves only the geometry that it controls.
- A value that you type while you draw makes the shape at that size.
  Thus it scales nothing unless the shape is not at that size.
- **Preferences › Sketch › Scale the sketch to its first dimension** sets
  this. It is on by default. Set it to off to keep the drawn size of the
  sketch.

**Angles:** Sketch Dimension (D) on two lines measures the angle on the
side where you put the value. The two lines, extended, make four angles
at their crossing. MitCad measures the angle that the pointer is in.
For a shape with the pointer inside it, that is the inner corner.

The arc and the value of an angle follow the pointer. Thus when you
move the pointer across a line, the dimension switches between the
angle and its supplement. The dimension keeps that side when its value
changes. When you move the value afterwards, only the text moves.

**Signed distances:** a horizontal or vertical distance is normally a
size. It is never negative, and MitCad keeps it on the side where the
point is. With the palette's *Signed distances* option checked, Sketch
Dimension makes such distances signed. The value is then the offset
from the first point selected. It is negative to the left of that point
or below it. MitCad shows the value with its sign (`+25.98`, `-15.00`).

Thus a point that you dimension from a center with `r * cos(a)` and
`r * sin(a)` follows `a` through all four quadrants.

A negative value typed into a new horizontal or vertical distance also
makes it signed. The point goes to that side. The context menu of an
existing distance offers *Make Distance Signed* or *Make Distance
Unsigned*. The point stays where it is. A value on the negative side is
negated: `-30`, or `-(r * 0.6)` for an expression.

**Center or tangent:** a circle or an arc normally counts by its center.
While Sketch Dimension selects, right-click a circle or an arc and
choose *Tangent* to measure from its nearest side instead, or *Center*
to measure from its center. If the circle is not selected yet, the
choice also selects it. Between two circles by their sides, the
dimension is the gap between them; when one circle is inside the other,
it is the gap inside. From one side to the center of another circle or
to a point, it is the gap from that side.

The same menu has *Driving* and *Driven*. A driven dimension only
measures: a click places it, without a value to type, and its value is
shown in parentheses. *Cancel* steps back, as Esc does.

**Dimensions to bodies:** Sketch Dimension (D) also selects the edges of
visible bodies that lie in the sketch plane: lines, arcs, circles,
ellipses and splines. It also selects their corners and the centers of
their circles and arcs. This includes the bodies of other components.
It also works while Hide Above Sketch cuts the bodies. The edges and
corners highlight under the pointer as sketch geometry does. When both
are under the pointer, the sketch geometry wins.

MitCad adds nothing while you select. When it makes the dimension, it
projects the edge or corner into the sketch, linked, as Project does
it. The dimension attaches to the projection. This is one undo step. If
the sketch already has a projection of the same edge or corner, MitCad
uses that projection again.

Projected geometry is fixed. Thus a dimension to sketch geometry moves
that geometry, but a dimension between two projected edges is driven.

**Dimensions to bodies below or above the sketch:** Sketch Dimension (D)
also selects the edges and corners of visible bodies that are not in the
sketch plane. An example is the edge of a plate below a sketch on the top
of a cylinder. You select the edge where the view shows it. An edge
behind a face is not selected. The pointer
highlights the edge with a dashed line, and the curve or point that the
edge gives in the sketch plane. MitCad projects the edge onto the sketch
plane, linked, when it makes the dimension. The dimension attaches to the
projection, as for an edge in the plane:

- A line gives a line, and a circle parallel to the plane gives a circle.
- A corner gives a point.
- An edge that is perpendicular to the plane gives a point.

Edges and corners that you selected before you push D are used too. When
the same place in the view has an edge in the sketch plane and an edge
out of it, the edge in the plane wins. When the design changes, the
projection and the dimension follow the edge.

**The sketch's axes and origin:** Sketch Dimension (D) and the
constraint tools also select the sketch's x and y axes. They select an
axis anywhere along the line drawn across the view. They also select
the origin, also when nothing has been drawn there. Any sketch or body geometry under the
pointer wins.

| Selection | Dimension |
| --- | --- |
| A point and an axis | The point's distance from the axis (its y or x coordinate) |
| A line and an axis | The angle between them |
| The origin and a point | A horizontal, vertical or aligned distance, as between two points |

Constraints use the axes and the origin in these ways:

- Coincident puts a point on an axis or at the origin.
- Parallel, Perpendicular, Collinear and Tangent take an axis as a
  line.
- Symmetry mirrors about an axis.
- Horizontal/Vertical and Midpoint take the origin as a point.

The Mirror Line of Mirror and the Direction of Rectangular Pattern also
take the x and y axes (refer to [Sketch curve tools](#sketch-curve-tools)).

You can also select an axis or the origin before you start a tool, as
you select construction geometry: click it, and Ctrl+click to add other
geometry. For example, select a point and the y axis, then push
Coincident: the point goes onto the axis. Select a point and the x axis,
then push D: the dimension is the point's distance from the axis. A
selected axis also becomes the Mirror Line of Mirror or the Direction of
Rectangular Pattern. Selecting an axis
or the origin adds nothing to the sketch. Delete, Copy, Cut and
Fix/Unfix ignore them, and you cannot drag them.

The first time something refers to an axis or the origin, MitCad adds
it to the sketch as fixed construction geometry. This occurs in the
same undo step. Afterwards MitCad uses it again. It never bounds a profile. It is not
drawn apart from the axes themselves.

A double-click on a dimension edits its value. You can drag a
dimension's text. Until you do, a circle's diameter or radius shows its
value outside the circle at the upper right, or on another side when
another value is already there, such as a concentric circle's
diameter. If a dimension would over-constrain the sketch,
MitCad adds it as driven, in parentheses. If a constraint would
over-constrain the sketch, MitCad refuses it with a message.

The value of a driven dimension is a read-only parameter (`d7`, kind
reference). You can rename it and use it in expressions of features
after the sketch. If you type a value into a driven dimension, it
becomes driving.

You can also use it in the sketch's own dimensions, for example
`(d7 - d2) / 2` for a hole that stays centered when the measured length
changes. This works when that dimension cannot change what `d7`
measures: the measured geometry is separate, or fully constrained by the
other constraints and dimensions. If the dimension would change what it
uses (a circular dependency), MitCad refuses it and names both
dimensions. Patterns, offsets and texts of the same sketch cannot use
it.

**Dimension names:** when you hover a dimension, MitCad shows its
parameter (`d3 = d1 / 2 → 20 mm (Sketch1 distance)`). The value editor
shows the name before the field (`d3 =`). If you type `width = 40`,
MitCad renames the parameter as it sets it. A click on another
dimension while the editor is open inserts that dimension's name. The
palette's *Dimension text* option shows each dimension's value, its
name, or both (`d3 = 20`).

A feature panel's value field shows the name of its own parameter after
the label (`Distance (d5)`). Its tooltip says what the parameter is, as
a dimension's tooltip does (`d5 = d1 / 2 → 20 mm (Extrude2 distance)`).
The field also takes `name = value`.

While a feature panel's value field has the keyboard, the view labels
the dimensions of the sketches it shows. It also labels the values of
the features before the one that the panel adds or edits
(`depth = 12 mm`). The labels are where the handles of those features
would be. The values of a fillet's or chamfer's sets are on the faces
that they made. A click on a label inserts its name at the field's
cursor.

**Imported designs:** a dimension that the file keeps driven gets the
file's own parameter of it as its read-only reference. Thus expressions
that use the parameter follow what the sketch measures.

### Constraints

The constraint buttons are in the Constraints group of the Sketch tab
and on the sketch palette. A constraint button applies to the selected
geometry. If nothing that fits is selected, the button lets you select
the geometry.

In a sketch, each constraint also has a key: Shift and a letter. The key
does what the button does.

| Constraint | Key | Constraint | Key |
|---|---|---|---|
| Horizontal/Vertical | Shift+V | Fix/Unfix | Shift+F |
| Coincident | Shift+C | Midpoint | Shift+M |
| Tangent | Shift+T | Concentric | Shift+O |
| Equal | Shift+E | Collinear | Shift+L |
| Parallel | Shift+P | Symmetry | Shift+S |
| Perpendicular | Shift+R | Curvature | Shift+G |

The keys work only in a sketch. In a value or text field, such as a
dimension's value, Shift and a letter types the capital letter. You can
change the keys in Keyboard Shortcuts, under Sketch. The tooltips of the
buttons and Help › Keyboard and Mouse Overview show them.

Equal, Parallel, Perpendicular, Collinear, Concentric, Tangent and
Coincident on more than two selected entities constrain the first
selected entity with each of the others. For example, you can make
several circles equal to a dimensioned one. Horizontal/Vertical on
several lines makes each line horizontal or vertical. The constraints
are one undo step.

If the selection is only part of what the constraint takes, the button
uses it as the first selections and lets you select the rest. For
example, select two circles, click Symmetry and then click the line or
the sketch's axis to mirror about.

If the sketch refuses some of the constraints, MitCad leaves them out
with a message. The sketch refuses a constraint that is already implied
or that conflicts. MitCad also refuses a constraint that would
over-constrain the sketch, with a message. When you click geometry that
a constraint tool does not take, the status line tells why.

**Conflicting constraints.** When a change cannot hold with the
constraints and dimensions the sketch has, MitCad refuses it and says
what blocks it, for example *the Equal between two lines can't hold with
the fillet*. The message names constraints by their type and the
geometry they are on, and dimensions by their value. Projected and fixed
geometry is called *projected geometry* or *fixed geometry*. While the
message shows, the blocking constraints' symbols, the dimensions and the
curves they are on are drawn in red in the sketch. When you point at the
message in a command panel, they are drawn in the highlight color.
Delete or change the blocking constraint, or choose a value that fits.
If the sketch itself no longer solves, the Sketch Palette shows
*Conflicting constraints* as a link: click it to select the constraints
and dimensions that conflict.

Symmetry of two circles or arcs makes their radii equal. If dimensions
or an Equal constraint already fix both radii, Symmetry mirrors only
their positions. If another constraint on one of the circles then
becomes redundant, MitCad removes it and tells you, but it never removes
a dimension.

**Tangent at an end.** When the two curves share an end, or one of them
is a spline, Tangent and Curvature apply at the end where you pick each
curve. Pick each curve near the end where it should be tangent. While
you pick, the constraint's symbol shows that end. Thus a curve can be
tangent at both of its ends, to two curves or to the same curve twice.
A line or a circle that shares no end with the other curve stays tangent
wherever the two curves touch. A spline whose two ends are at one point
becomes smooth there when you pick it twice for Tangent. A closed spline
is smooth where it closes already. Tangents of older designs keep their
meaning.

Coincident between the ends of two curves joins them to one point
(refer to [Sketch curve tools](#sketch-curve-tools)). The constraint tools also
take the sketch's axes and origin (refer to [Dimensions](#dimensions)).
Equal Spacing keeps intervals equal (refer to
[Equal Spacing](#equal-spacing)).

**Inactive constraints and dimensions.** An inactive constraint or
dimension stays in the sketch, but it does not hold the geometry.
MitCad draws it pink, with dashed lines. Its tooltip tells why it is not
applied, for example "Not applied: not met in the imported file". An
import makes inactive the constraints and dimensions of a sketch that
the file's own geometry does not meet (refer to [.f3d
import](#f3d-import)). An inactive dimension keeps its value and its
parameter.

Right-click a selected inactive constraint or dimension and select
**Activate** to apply it. If the sketch cannot solve with it, or if it
over-constrains the sketch, MitCad does not activate it and shows the
reason. Then change the geometry or delete the constraints that are in
conflict, and try again. **Deactivate** in the same menu makes an
active constraint or dimension inactive. **Delete** removes it. An
equal spacing and its spacing dimension go together: the dimension of an
inactive equal spacing is drawn inactive too, and **Activate** on it
activates both.

### Projections

**Project** (P, in Create) projects references from the bodies into the
sketch. Projected geometry is linked: it follows its source when the
design changes. MitCad draws projected geometry purple. The Point tool
snaps only to edges and corners in the sketch plane. Sketch Dimension
also takes edges and corners that are not in the plane. The Point tool and
Sketch Dimension project corners and edges of bodies as Project does it
(refer to [Snaps and auto constraints](#snaps-and-auto-constraints) and
[Dimensions](#dimensions)).

Projected geometry obeys these rules:

- Projected geometry is fixed. A dimension to sketch geometry moves
  that geometry. A dimension between two projected edges is driven.
- When the source of a projection moves, the sketch geometry that you
  dimensioned or constrained to the projection moves with it. That
  geometry stays on the side of the projection that it had when you last
  edited the sketch, also when the move is as large as the dimension or
  larger. This applies to distance dimensions, tangent distances, offsets
  and tangencies.
- If the sketch already has a projection of the same edge or corner,
  MitCad uses that projection again. This also applies to an edge of a
  face that you projected before, and to a face after one of its edges:
  the sketch gets no second curve on the edge, and the new curves end
  on the points that are already there. Thus the outline stays one
  chain for Offset. A face beside a face that you projected keeps its
  own line on the edge that they share, so that each outline stays one
  chain.
- A face is projected as you see it along the sketch normal. A curved
  face gets its outline: the side of a cylinder on a plane parallel to
  its axis becomes a rectangle, a cone becomes a trapezoid, and a sphere
  becomes a circle. The seam of a round face, where its surface closes,
  is not projected when it is not on the outline. A cylinder seen along
  its axis gives only its circles. With *Projection Link* on, the outline
  follows the face, for example when you change the radius of the
  cylinder.
- **Convert to Sketch Geometry** makes projected geometry ordinary
  sketch geometry (refer to [Sketch curve tools](#sketch-curve-tools)).
- Project has selection rows of its own while it runs (refer to
  [Selection in sketch mode](#selection-in-sketch-mode)).

Project also takes the curves and points of other sketches: lines,
arcs, circles, ellipses, splines and points, also construction and
projected geometry. Click them in the view while Project runs, or select
a whole sketch in the browser to get all its curves. The sketch must come
before the edited sketch in the timeline. If you select geometry of a
later sketch, the panel shows the reason and OK is not available. You can
also select the sketches of other components.

- With *Projection Link* on, the projected curves follow the other
  sketch: when you edit it or change its dimensions or parameters, they
  move with it. If you delete a curve there, its projection keeps its
  last place and the sketch shows a warning, as for a body edge that is
  gone. With *Projection Link* off, you get fixed copies.
- The sketches can be on any planes. On the same plane you get exact
  copies. On a parallel plane you get the same shapes. On a plane at an
  angle, MitCad flattens the curves along the sketch normal, as for
  body edges: a circle becomes an ellipse, and a line across the angle
  becomes shorter.

**Intersect** (Create › References) adds the curves where the sketch
plane cuts the bodies and faces that you select. It adds points where
the sketch plane crosses the edges that you select. For a body, MitCad
cuts every face of the body. For example, a sketch through a shaft gets
the section of the shaft as a circle. Project is different: it flattens
edges onto the sketch plane.

- *Projection Link* is on by default. Then the curves and points follow
  the design when the design changes, as projected geometry does.
- You can also select the faces, edges and bodies of other components.
- If the sketch plane does not cut a selection, the panel shows the
  reason and OK is not available.
- Intersect is also in the context menu of a face or an edge, after
  Project. The command search also finds it with the word "section".

Intersect also takes the curves and points of other sketches. It adds
a point at each place where the sketch plane crosses a curve that you
select. For example, a circle in a sketch on XY crosses a sketch on XZ
through its center at two points, which you can use for a rail of a
loft. Click the curves in the view while Intersect runs, or select a
whole sketch in the browser: MitCad then uses each curve that crosses
the plane. As for Project, the other sketch must come before the edited
sketch in the timeline, and the edited sketch's own geometry is not
available.

- With *Projection Link* on, the points follow the other sketch when
  you edit it or change its dimensions or parameters.
- If the sketch plane does not cross a curve that you select, the panel
  shows the reason and OK is not available.
- Intersect has selection rows of its own while it runs, with a row for
  other sketches' curves and points (refer to
  [Selection in sketch mode](#selection-in-sketch-mode)).

**Clean Up Projections** (Create › References) removes projected curves
that repeat another curve, and projected lines that lie on another
line. It keeps their constraints and dimensions on the curve that
stays. *Merge Overlapping Lines* also takes a projected line that lies
on a longer line, and unlinked lines that overlap. A merged line
follows one source only.

**Clean Up Stale Projections** is also a button on the sketch palette
while the sketch has stale projections. It removes the projected curves
that no longer match their source and that nothing uses. It trims the
projected curves that the profiles still use. It unlinks the
projections that cannot follow.

**Project Again** (Create › References) repairs a linked projection
that no longer follows its source, for example after the source face
changed its shape. It projects the source again and moves the
constraints and dimensions of the old curves to the new curves that
take their place, so that the geometry you drew around the projection
follows it. The sketch palette shows a *Project Again* button beside
*Clean Up Stale Projections* under the warning, and the context menu
of the sketch in the timeline and in the browser has *Project Again*
too.

- A constraint or a dimension moves only when its curves and points
  map clearly to new ones. One that does not stays on the old curve,
  which stays in the sketch as construction geometry, and the panel
  lists it. Check these and constrain the new curves as necessary.
- The new curves keep the names of the curves they replace, so that
  the profiles keep their names where their outline is the same. If a
  profile that a later feature uses changes its name, the panel says so.
- A projection that only moved or changed its size follows its source
  without Project Again.

### Equal Spacing

**Equal Spacing** (Constraints) keeps the intervals between three or
more independent objects equal. It does not copy the objects. The
objects can be points, circles and arcs (their centers) and lines
(their midpoints), mixed as necessary.

Select how MitCad measures the intervals:

| Measurement | Interval |
| --- | --- |
| Horizontal | Along x |
| Vertical | Along y |
| Along a Line | Along a line that you select |
| Point to Point | The distances, as a chain that can bend |

Only successive intervals count (A-B, B-C, C-D). Equal Spacing aligns
nothing, and it makes no size equal. For example, an arc spaced by its
center keeps its radius when the radius is dimensioned.

Horizontal, vertical and along spacings take the objects in their order
along the direction. When two objects are at the same place along the
direction, the panel names them. Then order the list and check Keep
Listed Order.

A point-to-point chain follows the order of the list. Objects selected
in the panel come in the order clicked. For a selection made before the
command, check Chain in Listed Order when the list is correct. The
preview draws the chain with the numbers 1, 2, 3.

Without a dimension, the intervals stay equal, but their size is free.
**Common Spacing Dimension** adds one dimension that sets every
interval. If you select three or more such objects and press Dimension
(D), Equal Spacing Dimension opens directly, with the current mean
interval. You can type over that value. When you edit that one value,
every interval changes. When you delete it, the intervals stay equal.

A double-click on the symbol of the equal spacing opens **Edit Equal
Spacing**. The command is also in the context menu of a right-clicked
symbol. It changes the objects, their order, the measurement and the
dimension. When you delete one of the objects, MitCad takes it out of
the chain. With fewer than three objects left, MitCad removes the
constraint.

Copies of a sketch pattern are spaced by the pattern. Change them with
Edit Pattern.

### Text, fonts and QR codes

Text letters are profiles: the letters are regions, and their openings
are holes. The copies of sketch patterns follow their originals. Edit
Text, Edit Pattern and Edit Offset change texts, patterns and offsets
later.

A double-click on a text opens Edit Text, and so does **Edit Text** in
the context menu of a right-clicked text. Zoomed in, a click inside a
letter or inside a letter's opening (the hole of an "o") selects that
region, not the outline. Edit Text is there too: the double-click and
the context menu edit the text the region belongs to.

**Fonts** (Edit Text): the font chooser lists the bundled Droid Sans,
then the installed fonts, then the font files of the folders of
**Preferences › Fonts**. Each font has a preview. **File…** beside the
chooser selects any font file (`.ttf`, `.otf`, `.ttc`, `.woff`,
`.woff2`).

A text in a font file keeps the file's outlines, its path and its
license note in the design. Thus the design looks the same on a
computer without the file. On such a computer, a new string or style
asks for the file again. MitCad finds a file of the same name in the
font folders by itself.

A font file is one face. Bold leaves it as it is: select the bold file.
Italic slants an upright font.

**Right-to-left and joined scripts:** Hebrew, Arabic and other scripts
join their letters or carry marks. MitCad shapes them as the font says,
with joined forms, ligatures and marks. They are read from right to
left. A line that mixes them with Latin text or numbers is ordered as
it is read. This works with font files and with installed fonts that
have the script. The bundled Droid Sans has neither Hebrew nor Arabic.

Joined letters overlap a little. Thus extrude the whole word.

**QR Code** (Create › Text) makes a QR code of a text or an address.
You set its module size, its error correction and its lower left
corner. The error correction is L, M, Q or H: about 7 to 30 % of the
code can be lost. The dark modules are profiles. Extrude started next
takes all of the code at once. Thus it extrudes or engraves the code as
one.

A double-click on the code opens Edit QR Code (content, module size,
error correction). This also applies to a double-click inside a dark
module or a light square of the code, and to its context menu.

### Selection in sketch mode

While you edit a sketch, the **Selection Filter** button of the Select
group of the Sketch tab says what a click and a window select. Each row
cycles through three states when you click it:

- An empty square: the row cannot be selected
- A filled square: a click selects it
- A square with a cross: a click and a window select it, in both drag
  directions

Each sketch edit starts with these defaults:

| Row | Default |
| --- | --- |
| Sketch geometry, construction geometry, projected geometry | cross |
| Dimensions and constraints, profiles, visible body edges | square |
| Occluded body edges, body faces, body vertices, bodies, components, planes/axes/points | empty |

Thus a window selects the sketch's ordinary, construction and projected
geometry together, and nothing of the bodies. A click on a body edge in
sight selects it. A click on an edge behind faces does not. The view at
the moment decides if an edge is in sight. When you orbit, this
changes. For an edge that is partly hidden, the part that you click or
that is inside the window counts.

The rows only concern the edited sketch. You cannot select the geometry
of other sketches, except while Project or Intersect runs, and you cannot select
hidden bodies. *Reset to
Defaults* restores the rows. The Solid tab's filters are kept apart.
They apply again after the sketch.

**Shift+click** also selects what empty rows hold, for example an edge
behind a face. While Shift is down, the highlight under the pointer
shows this. When you release Shift, the highlight goes back. Ctrl+click
adds or removes. Ctrl+Shift+click does both. Shift with a window adds
to the selection, but it does not go beyond the filter.

Shift with the other mouse buttons and the wheel navigates as before. A
right click offers the commands for what Shift+click would select. Thus
a face still offers Project.

**Click adds to the selection in sketches** (Preferences › Sketch, off
by default) selects several items without Ctrl. While it is on, a click
with no tool running adds an item to the selection, and a click on a
selected item removes it, as Ctrl+click does. A click on empty space or
Esc clears the selection. A right click on an item that is not selected
adds it, and the menu offers the commands for all that is selected. A
window, and the picks of tools and commands such as Sketch Dimension,
the constraints, Project and Intersect, work as before. A double-click
is two clicks: on a line it adds the line and removes it again.

**Project** (P) has rows of its own while it runs, shown in the same
control:

| Row | Setting |
| --- | --- |
| Visible body edges | cross |
| Occluded edges | empty |
| Faces and vertices | square |
| Other sketches' curves and points, intersection curves | cross |

You can change the rows while Project runs. Shift+click
reaches an edge behind faces. Project takes nothing but edges, faces and
vertices of the bodies, curves and points of other sketches, and the
curves of Intersection Curves. OK and Cancel bring back the sketch's rows as they were. The
projected geometry then follows the projected geometry row.

**Intersect** has rows of its own too: the same as Project's, but with
a *Bodies* row (square) in place of the vertices, so that a click can
select a whole body. Other sketches' curves and points are a cross, as
for Project.

### Copy, Cut and Paste

You can copy sketch geometry from one sketch to another sketch, or to
another place in the same sketch. Select points, curves and texts, then
use **Copy** (Ctrl+C) or **Cut** (Ctrl+X). Cut also deletes the
selection. Open the sketch that gets the geometry and use **Paste**
(Ctrl+V). The three commands are in the Modify menu of the Sketch tab and
in the context menu.

Paste shows the geometry at the pointer. The pointer holds the base
point, shown as a dot: the first point of the item that you selected
first. Click where the base point goes, or type X and Y in the boxes and
press Enter. Esc cancels the paste. The coordinates are the sketch's
own, so a sketch on another plane gets the same 2D coordinates. You can
paste into a design that is open in another MitCad window too. Undo
names the steps Cut and Paste. When you click with the base point on a
point or a curve (the snap marker shows it), the base point is held
there with a constraint, as a point you draw there would be: a circle
pasted with its center on a point is coincident with that point and
follows it.

The constraints and dimensions come with the geometry when they
reference only copied geometry or the sketch's origin and axes. The
others are left out, and MitCad says how many. A copy that is
dimensioned to the origin or an axis keeps those distances in the new
sketch too, so a click cannot move it away from them: in that direction
the preview stays where the copy was, and only the other value box is
shown. A copy that is dimensioned to the origin in both directions is
pasted at once where it was. A dimension keeps its expression
when the design has all the parameters that the expression uses.
Otherwise it gets the value. Projected geometry becomes ordinary sketch
geometry. Construction lines, center lines, splines, arcs, ellipses,
points and texts stay what they are.

A 3D sketch's geometry is copied and pasted between 3D sketches, and a 2D
sketch's copy can be pasted into a 3D sketch, but a 3D sketch's copy
cannot be pasted into a 2D sketch (see *Copy, Cut and Paste in a 3D
sketch* under [3D sketches](#3d-sketches)).

In a text box, Ctrl+C, Ctrl+X and Ctrl+V copy, cut and paste the text as
usual.

### Redefine Sketch Plane

**Redefine Sketch Plane** moves a sketch to another plane or planar
face. You can also move the sketch on its plane. Right-click the sketch
in the Browser or the Timeline, or a curve or profile of it in the view,
and select **Redefine Sketch Plane**. While you edit a sketch, the same
command is in the sketch's context menu, and it always moves the sketch
that you edit.

The panel has these inputs:

- **Sketch**: the sketch to move (not shown while you edit a sketch)
- **Plane**: an origin plane, a construction plane or a planar face. It
  starts as **Same Plane**: the sketch stays on its plane until you
  select another one. A face of another component is linked, as with
  Create Sketch.
- **Offset**: the distance along the plane's normal. **Flip** beside it
  turns the sketch over, to the other side of the plane.
- **Move X** and **Move Y**: the distances along the sketch's x and y
  axes on the plane
- **Angle**: the angle about the plane's normal, from the sketch's x
  axis towards its y axis

The values can be expressions with parameters, for example `Gap * 2`.
The sketch keeps them as its own parameters. Thus you can change them
again later in this panel or in the parameters table.

The preview shows the sketch and the features built on it in their new
places. The sketch geometry keeps its own coordinates. Extrudes,
revolves, holes and other features on the sketch follow it. Linked
projections and intersections are made again from the new plane. When
they cannot follow, the sketch palette says so. OK makes one undo step.
While you edit a sketch, the change is undone with the sketch edit.

On a new plane, the sketch opens upright, as a new sketch does. The
plane must be made before the sketch in the Timeline. If you select a
construction plane or a face of a later feature, the panel tells why it
cannot be used. To use such a plane, move the sketch after it in the
Timeline first.

### 3D sketches

A 3D sketch holds points, lines, arcs and splines in space: a route can
leave the plane it starts on, for example for a bent tube. Its frame
follows the plane or face it starts on, as a sketch does, but its points
have a third coordinate along the plane's normal.

**Create a 3D sketch.** While **Create Sketch** waits for a plane, the
command card shows **Sketch type** with **2D** and **3D**. **2D** is the
default. Click **3D**, then select an origin plane, a construction plane
or a planar face of the active component. The banner over the view says
**Create 3D Sketch** while you choose. **Create 3D Sketch** in the
command search starts the same command with **3D** chosen, also on a
plane that you selected first. A face of another component is not
linked into a 3D sketch: activate that component first.

**Edit a 3D sketch.** A 3D sketch opens as a sketch does: double-click
it in the Browser or the Timeline, or select **Edit Sketch** in its
context menu. Editing a sketch earlier in the Timeline rolls the
Timeline back to it until you finish. While you edit a 3D sketch:

- The banner over the view says **3D Sketch** and the sketch's name.
- The view shows the sketch's frame at its origin: X red, Y green and Z
  blue. The camera does not turn to the sketch plane. Orbit, pan and
  zoom work as usual, so that you can see the route from every side.
- The view shows the sketch's points and curves where they are in space.
  A curve from outside the sketch, such as a body edge or a curve of
  another sketch that its points follow, is drawn as reference geometry.
  A control point spline shows its control polygon, dashed. A fit point
  spline shows a tangent handle at an end that is free or held tangent.
- Points and curves that are fully determined are drawn darker, as in a
  2D sketch.
- A sketch turned from a 2D sketch shows one **In plane** badge beside
  each shape whose relations and dimensions it kept from it (above the
  shape, clear of a text in it and of the other badges), and a square
  around each point that stays on the sketch plane.
- Click a point or a curve to select it. The **3D Sketch** palette
  describes what you select, under its relation and dimension buttons
  and scrolled into sight: a point's coordinates in the sketch, a
  curve's points, whether it is determined, and the source of a
  reference curve. The same description shows beside the pointer when
  you hover over a point or a curve.
- The **3D Sketch** palette shows the degrees of freedom, or **Fully
  constrained**, how many points and curves are determined, the
  redundant relations, the linked references and those that failed, and
  why the sketch does not solve.
- **Delete** removes the selected points and curves. A point that a
  curve still uses is not removed: MitCad says which curve uses it.
- **Point**, **Line**, the three arcs and the two splines of the SKETCH
  tab draw in space (see *Draw in a 3D sketch* below), the circles, the
  ellipse, the rectangles, the polygons and **Text** draw on a plane of
  the sketch (see *Draw a shape on a plane* below), and **Sketch
  Fillet** rounds corners of lines (see *Round the corners of a route*
  below). **Copy**, **Cut** and **Paste** work between 3D sketches, and
  bring a 2D sketch's geometry into a 3D sketch (see *Copy, Cut and Paste
  in a 3D sketch* below). The other 2D sketch tools are disabled. Closed
  loops in the sketch plane are profiles, as in a 2D sketch (see
  *Profiles of a 3D sketch* below).
- Esc clears the selection. It does not leave the sketch: **Finish
  Sketch** does. Undo and Redo work as in a 2D sketch.

**Draw in a 3D sketch.** Select **Point**, **Line** (L), **3-Point
Arc**, **Center Point Arc**, **Tangent Arc**, **Fit Point Spline** or
**Control Point Spline** on the SKETCH tab. A mouse click in the view
cannot tell how far away a point is, so the tool places each point on a
temporary plane or along an axis, which the view shows:

- **Plane** chooses **XY**, **YZ** or **ZX** of the sketch. The plane goes
  through the anchor: the sketch's origin at first, then the last point
  that you placed. A dotted line drops from the next point to the plane,
  so that you can see how high it is.
- **Place** chooses **On Plane**, an axis **X**, **Y** or **Z** through
  the anchor, or **Normal**: the point keeps its place in the plane and
  moves along the plane's normal, up or down.
- When you look along the plane or the axis, the pointer cannot place a
  point well. The tool keeps the last place and tells you so. Choose
  another plane, type the values or turn the view. You can orbit, pan
  and zoom at any time; what you have started stays.
- **Rotate the plane** to draw at any angle. Three rings around the
  anchor, one for each of the plane's axes, are the rotation handles:
  drag a ring and the plane, its axes and its normal turn about that
  axis, in whole degrees; the palette's **Rotate** field of that axis
  shows the angle while you drag. Or type an angle in **Rotate X**,
  **Y** or **Z** and press Enter: the plane turns about its own axis by
  that much (a positive angle turns counter-clockwise, seen from the
  axis' tip). **XY**, **YZ** and **ZX**, the axes and **Normal** then
  mean the rotated plane's. **Reset** brings back the sketch's own XY
  plane at the anchor. The rotation only guides where you click: it is
  not saved, it does not change the sketch's own axes, and the next time
  you open the sketch the plane is the sketch's XY again. The typed
  **Delta X**, **Y** and **Z**, **Axis and Length** and **Keep Axis** go
  along the rotated plane's axes, and the shapes on a plane (circles,
  ellipses, rectangles, polygons and text, below) lie on the rotated
  plane: what you draw keeps those directions. **Keep Plane** keeps an
  arc in a plane of the sketch's own axes.
- **Show** **Plane** and **Handles** draw the plane and the rings, or
  hide them when they are in the way; the plane works the same.
- **Keys** change the plane and the place while you draw, without the
  palette. While a drawing tool other than **Sketch Fillet** is active
  and no field has the keyboard, **Tab** moves to the next plane, XY →
  YZ → ZX, **Shift+Tab** to the previous one, and **Up Arrow** switches
  between **On Plane** and **Normal** (from an axis it goes to
  **Normal**). The hint line and the tooltips of the Plane and Place
  buttons name the keys. When a field has the keyboard, Tab and Shift+Tab
  move between the fields as usual. Without such a tool, Up turns the
  view as it always does. The keys are **Next Placement Plane**, **Previous Placement Plane** and
  **Placement Along Normal** under **3D Sketch Tool** in Tools › Keyboard
  Shortcuts, where you can change them.

A point snaps to what is under the pointer, and a badge names it:
**Point** (a point of the sketch: the curves share it), **Origin**,
**Linked Point** (a vertex or an edge end of a body, or a point of another
sketch, also in another component), **Center** of an arc, **Midpoint** of
an edge or a curve, **On Curve**, **On Edge** and **On Face** (a face of a
body under the pointer, also in another component). A linked point keeps
its own height and follows its source when the source changes. Points
win over places on curves, those win over the axes of the plane, and a
face comes last: aim at a face away from its edges to put the point on
it.

A point **On Face** lies on the face and can still slide on it: give it
two more values (two offsets, a length and an axis) to fix where. Flat,
cylindrical, conical, spherical and toroidal faces are followed exactly;
other curved faces too, through the face itself. When the face moves (you
change its feature or an earlier one), the point follows; if the face is
lost, the sketch shows the reference in red and keeps its definition
until you replace or detach it.

Type values in the palette instead of clicking. A field that you leave
empty shows the measured value in gray; a value that you type becomes a
dimension that drives the geometry, also a typed 0:

- **Offsets**: **Delta X**, **Delta Y** and **Delta Z** from the start of
  the segment. The coordinates that you do not type stay free.
- **Axis and Length** (Line): an axis, its sense (+ or -) and the length.
- **Radius** and **Arc Length** (Center Point Arc and Tangent Arc).
- **Normal** (Center Point Arc): the arc lies square to the active
  plane's normal. Click **Pick**, then a line, a straight edge, an axis
  or a planar face: the arc's normal takes the line's direction or the
  face's normal. **Plane** goes back to the active plane. The picked
  direction is taken once; it does not follow the line or the face later.
- **Keep Axis** keeps a line along the axis that you place it on. **Keep
  Plane** keeps an arc in the active plane. **Construction** makes
  construction geometry.

Tab moves from field to field while a field has the keyboard (otherwise
it changes the plane, see **Keys** above). Enter commits the segment,
arc, point or spline. When the values contradict a snap, or a value is wrong, the
palette says why in red and nothing is made.

A line or an arc is made when you place its last point; the points
before it are only shown, so that Esc leaves nothing behind. Each line,
arc, spline or point is one undo step, with its dimensions and
relations. A line continues from its end: click the next end, or
double-click to stop. **Tangent Arc** starts at the end of a line, an
arc or a spline that you click. A spline is made when you press Enter or
double-click; a click on the first point of a fit point spline closes
it.

Esc goes back one step at a time: first the value in a field, then the
points that are not made yet, then the tool. Esc does not remove what
was made: Undo does.

**Round the corners of a route.** **Sketch Fillet** on the SKETCH tab
rounds the corner where two lines meet with an arc, the bend of a tube:

- Type the **Radius** in the palette, then click a corner: it is rounded
  at once. Click the next corner to round it with the same radius.
- Without a typed radius, click a corner, then move the pointer into the
  corner: the arc follows it, and the **Radius** field shows its radius.
  Click to take it. You can also press on the corner and drag.
- To round a corner of construction lines, or one where more lines meet,
  click the two lines one after the other.

Both lines end where the arc touches them; they keep their names, their
direction and their other relations. The arc is tangent to both lines
and has a radius dimension: change it later in the palette's list, and
the arc follows. The corner point stays where the two lines would meet,
with its dimensions, so a typed length still measures to the corner.
Each corner is one undo step. MitCad refuses a radius that is too large
for one of the lines, and two lines that do not meet or that are in
line, and says why. A pipe or a sweep along the route goes through the
arcs. Esc first takes back the typed value, then the corner you
clicked, then ends the tool.

**Draw a shape on a plane.** In a 3D sketch the SKETCH tab's **Center
Diameter Circle** (C), **2-Point Circle**, **3-Point Circle**,
**Ellipse**, **2-Point Rectangle** (R), **Center Rectangle**, **3-Point
Rectangle**, **Circumscribed Polygon**, **Inscribed Polygon**, **Edge
Polygon** and **Text** draw as they do in a 2D sketch, on the plane you
choose in the palette (**XY**, **YZ** or **ZX**, rotated as you rotated
the plane):

- The first click (or the first point typed as Delta X, Y and Z from the
  anchor) places the shape's first point as any point of the sketch: it
  snaps to the sketch's points, the origin, vertices and edges. The shape
  lies in the chosen plane through that point; its other clicks stay in
  it.
- Type the sizes as in a 2D sketch: **Radius**, **Diameter**, **Major
  Radius** and **Minor Radius**, **Width** and **Height** (Tab moves
  between them), **Sides** of a polygon, the **Text** and its **Text
  Height**. A typed size becomes a dimension you can change later in the
  palette's list; a size you only click stays free.
- The shape gets the relations a 2D sketch gives it, in its own plane: a
  rectangle's sides stay along the plane's axes (or square to each other
  for a 3-Point Rectangle), a polygon's corners stay on its construction
  circle with equal sides, and its points stay in one plane (**Plane-Bound**
  in the palette's list). A circle and an ellipse stay in their plane; an
  ellipse's minor radius is solved like any size, so a point you put on
  it or a dimension changes it.
- A text starts where you click; drag to draw its frame instead. Type the
  text and press Enter.
- Each shape is one undo step. Esc first takes back a typed value, then
  the points you clicked, then ends the tool.

**Edit a 3D sketch.** With no drawing tool active, the **3D Sketch**
palette edits what you select:

- **Relations**: select points and curves, then click **Coincident**,
  **Point on Curve**, **Midpoint**, **Tangent**, **Equal**, **Fix**,
  **Unfix**, **Align X**, **Align Y**, **Align Z**, **On Sketch Plane**,
  **Release from Plane**, **Release Plane** or **Detach**. A button that
  does not fit the selection is disabled; its tooltip says what to
  select. Coincident, Tangent, Equal, Fix, Midpoint and Sketch Dimension
  of the SKETCH tab work on the selection too.
- **3D relations**: **Parallel** and **Perpendicular** take two lines, or
  a line and an arc or circle (its normal), anywhere in space: the lines
  need not meet. **Angle** holds the same two at the angle they have now;
  type another angle in its field. **Symmetric** mirrors two points
  across a plane: click it, then click a planar face, a construction
  plane or another sketch, or choose **XY**, **YZ** or **ZX** in the
  palette for a plane of the sketch itself. **On Plane** keeps points on
  such a plane. **Concentric** gives two arcs or circles one center and
  parallel planes, **Same Center** only the center. **Curvature** makes a
  spline continue a line, an arc or another spline smoothly at their
  shared end, its curvature included.
- **Dimensions**: **Length** of a line (its direction stays free),
  **Delta X**, **Delta Y** and **Delta Z** between two points or along a
  line, **Radius**, **Diameter**, **Arc Length**, **Spline Length**,
  **Angle** between two lines (in degrees) and **Tangent Magnitude** at a
  spline's start or end (select the spline and the end point; the end
  must be free or tangent).
- **Relations and dimensions** lists them all. Type a value, an
  expression or a parameter's name in a dimension's field and press
  Enter. **Driven** makes a dimension only measure; **Remove** removes
  the relation or the dimension. A driven dimension's value is a
  parameter too (its name, like `d4`, is in the Parameters list), which
  the sketch's other dimensions may use: `d4 / 2` keeps a length half of
  what `d4` measures. A dimension that would change what it uses is
  refused.
- **Linked references** lists what the sketch follows: a vertex, an edge,
  a face, another sketch's point or curve, also in another component.
  **Replace** then a click on another source of the same kind points the
  reference there; **Detach** frees its points, which stay where they
  are. To attach a point, select it, click **Attach** and click the
  source: a vertex, an edge, a face, another sketch's point or curve, an
  Intersection Curve or the origin. If a
  source is lost (you deleted the edge), the reference turns red and the
  sketch keeps its definition until you replace or detach it.
- **Delete** of a point that curves still use asks first: the palette
  names those curves and **Delete with Dependents** deletes them all.

**Drag** a point, a curve or a spline's tangent handle to move it. Choose
the plane (**XY**, **YZ**, **ZX**) or the axis (**X**, **Y**, **Z**) it
moves on under **Drag on**. The relations and dimensions hold: when they
stop it short of the pointer, a dashed line shows how far. Releasing the
mouse button makes the change, one Undo step; Esc during the drag
cancels it. A spline's tangent handle sets the direction and the length
of the spline's end tangent.

**Copy, Cut and Paste in a 3D sketch.** Select points and curves, then
use **Copy** (Ctrl+C) or **Cut** (Ctrl+X) as in a 2D sketch (see [Copy,
Cut and Paste](#copy-cut-and-paste)); Cut also deletes the selection, one
undo step. Open the 3D sketch that gets the geometry and use **Paste**
(Ctrl+V): the geometry is pasted at once, one undo step, and stays
selected.

- A 3D sketch's copy goes where it was: the same coordinates and height
  in the sketch, also in a sketch on another plane.
- A 2D sketch's copy lies on the plane that the drawing tools chose last,
  **XY**, **YZ** or **ZX** of the sketch (a rotation of the plane does not
  apply), with its base point at the anchor. Its arcs, circles, ellipses
  and texts stay on that plane, and its constraints and dimensions hold
  in it.
- The relations and dimensions come with the geometry when they name only
  copied geometry. The others are left out, and MitCad says how many.
  Linked references (attached points, points on faces, reference curves)
  stay linked to the same sources when the sketch can use them. A source
  that it cannot use, such as a feature after the sketch in the Timeline,
  is left out with what used it, and MitCad says how many.
- A 3D sketch's copy cannot be pasted into a 2D sketch: MitCad says so
  and pastes nothing.

**Finish Sketch** ends the edit and leaves the view as you had it in the
3D sketch, also for a sketch that was a 2D sketch when you opened it. A
new 3D sketch that you leave empty is removed, as an empty 2D sketch is.
The Browser lists 3D sketches in the Sketches folder, with their eye and
their degrees of freedom.

**Turn a 2D sketch into a 3D sketch.** Start a sketch on a face as usual,
then check **3D Sketch** in the Sketch Palette: the sketch becomes a 3D
sketch in place, and you can go on drawing to points, edges and sketches
on other planes and in other components. **Convert to 3D Sketch** in the
Browser's or the timeline's menu of a 2D sketch opens it and does the
same. Finish or Escape the active tool first: the box is disabled while a
tool has input pending.

- The sketch keeps its name, its place in the timeline, its plane and its
  axes, and every point, curve, text, constraint and dimension with its
  value or expression. Its points stay in the sketch plane, so a fully
  constrained sketch stays fully constrained.
- Its constraints and dimensions keep their 2D meaning: they work on the
  sketch seen along its normal, and the view marks each shape that has
  them with one **In plane** badge beside it.
  Relations and dimensions you add afterwards are 3D ones. To lift a point
  out of the plane, select it and click **Release from Plane** (points
  joined to it go with it); **Release Plane** frees an arc's or circle's
  plane.
- Circles, arcs, ellipses and texts stay in planes parallel to the sketch
  plane. A center line becomes a construction line.
- Linked projections keep following their sources. A Pipe or Sweep
  that runs along the sketch's curves keeps its path.
- A sketch on a face can put its points on that face with **Attach**:
  released from the plane, such a point stays on the face and follows it.
- An Extrude, a Revolve or a Sweep that uses the sketch's profiles keeps
  them: the closed loops of a 3D sketch in its sketch plane are profiles,
  as a 2D sketch's are. Patterns, offsets, the sketch's place on its
  plane, texts that follow text parameters, inactive constraints and
  dimensions, and projections of faces, sections and chains of edges all
  come along and keep working in the sketch plane.

**Turn a 3D sketch back into 2D.** While all of a 3D sketch lies in its
sketch plane, uncheck **3D Sketch** in the 3D Sketch Palette, or choose
**Convert to 2D Sketch** in the Browser's or the timeline's menu of the 3D
sketch: it becomes a 2D sketch in place, with every point, curve,
constraint and dimension. Pipes, sweeps, lofts, path patterns and
construction planes and points along its curves keep running the same
way along them. When something lies out of the plane, the box
is disabled and its tooltip names it; release or move it back first. It
is disabled too while a drawing tool runs: Escape the tool first.
**Undo** takes either conversion back.

**Profiles of a 3D sketch.** Closed loops that a 3D sketch draws in its
sketch plane are profiles: **Extrude**, **Revolve** and the other profile
features take them as they take a 2D sketch's, and the 3D Sketch Palette
says how many there are. Loops out of the plane are no profiles.

**Pipe and Sweep along a 3D sketch.** A route of lines, arcs and splines
in a 3D sketch is a path for **Pipe** and **Sweep** (its Path and Guide
Rail) in the same component:

- With **Chain Selection** checked (the default), one click on any curve
  of the route takes the whole route: every curve joined to it end to
  end. Uncheck it and click the curves you want for a part of the route.
  A click on the 3D sketch's row in the Browser, or on its entry in the
  timeline, takes its route whole when the sketch has one route.
- The Path row lists the route as the feature will run along it, the
  curves in order and the points where it starts and ends, for example
  `c3, c11, c5 (p1 to p8)`. The route runs through the first curve you
  clicked, along the way that curve was drawn. **Reverse** (Pipe) runs it
  from its other end; Sweep has **Flip**.
- Curves join only where they share a point, or where their ends are made
  coincident. Ends that only lie close to each other do not join, and
  curves that cross do not meet there. When the clicked curves are not one
  route, the Path row says **Not one path** and the panel says why: where
  the route branches, which curve does not join the others, or that a
  closed curve was clicked with others.
- Closed routes are paths too: one click on a curve of a closed loop
  takes the whole loop, listed for example `c3, c5, c6 (closed, from p1)`,
  and a circle, an ellipse or a closed spline is a closed path by itself
  (`c11 (closed)`). A reference curve (geometry from outside the sketch)
  is not taken: the status bar says why when you click one. Construction
  curves are left out of a route unless you click them.
- Draw the bends of a tube with arcs or the 3D sketch's **Fillet**: tangent
  joins give a smooth pipe. At a sharp corner, Pipe's **Corners** choose a
  mitered or a rounded joint; no bend is added for you.
- The profiles of a Sweep come from a 2D sketch, or from a 3D sketch's
  closed loops in its sketch plane.
- The other inputs that take a path take a 3D sketch's route the same
  way, by clicks in the view or its row in the Browser: a Loft's
  **Centerline** and **Rails**, **Pattern on Path**, **Plane Along Path**,
  **Plane Normal to Path at Point** and **Point Along Path**. A line of a
  3D sketch is an axis or a line wherever one is asked for (a construction
  axis through it, a plane at an angle to it, the axis of a revolve or a
  circular pattern), an arc or a circle gives the axis through its center,
  and a point of a 3D sketch is a point. **Intersection Curve** takes a 3D
  sketch's curves too, swept along the normal of its sketch plane. All of
  them follow the 3D sketch's edits.
- **Rib**, **Web**, **Thin Extrude** along curves and the curves of
  **Surface Extrude**, **Surface Revolve** and **Surface Sweep** grow from
  their sketch's plane. They take a 3D sketch's curves that lie in its
  sketch plane, as they take a 2D sketch's, and follow the 3D sketch's
  edits. A click on a curve that leaves the sketch plane is not taken,
  and the status bar says why; if a curve leaves the plane after an edit,
  the feature fails and names the sketch. Surface Sweep's **Path** takes
  a 3D sketch's route as Sweep's does.
- Edit the 3D sketch and the pipe or sweep follows: a new dimension value,
  a moved point, a changed body that the route is attached to. When the
  route no longer makes one path (you deleted a curve's joint, or added a
  branch), the Pipe or Sweep fails with the reason and the sketch stays
  editable. Editing the Pipe shows its route as you picked it; the 3D
  sketch stays shown while a pipe uses it.

## Browser, timeline and parameters

The Browser shows the structure of the design. The timeline shows its
features in the order of their construction. The parameters hold the
values that the sketches and features use. This chapter tells you how
to read and change each of them.

### Browser

The Browser is a tree. It shows these rows:

- Design Settings (the unit)
- Named Views
- Origin
- The root component
- Every occurrence with its bodies, sketches and construction geometry

The eye before the icon of a row shows and hides that row. You can undo
a change of the eye. A sketch hides by itself when a feature first uses
it. The sketch shows again when you delete the last feature that uses
it. The eye has priority over this automatic behavior. A 3D sketch
stays shown when a Pipe or a Sweep uses it, so that you can see and edit
the route.

While **Isolate** is on, the eyes of bodies and components show what
the view shows. A row that only Isolate hides has a dimmed, closed eye.
Click it to show that body or component as well: it joins the isolated
items. Click the eye of an item that you added this way to hide it
again. Neither click changes the item's own visibility, and each is one
undo step. Click the closed eye of an item that you hid yourself to
show it and add it to the isolated items in one step. **Unisolate**
shows every item again as its own visibility says.

The eyes also work while a command's panel or a sketch is open: the eyes
of bodies, components, sketches, construction geometry, a section
analysis in the Analysis folder and an image reference. The command does
not end. Its inputs, its selection and its preview stay, and the preview
is drawn again for what the view now shows. For example, open Thread on
the inside of a hole and turn on the eye of a section analysis: the
preview of the thread is cut open at the plane of the section. You can
also hide the body that the command works on; what you selected on it
stays selected. Each click of an eye is an undo step of its own, never
part of the step of the command: Cancel (Esc) keeps what you showed or
hid, and after OK the first Undo takes back the command and the next
Undo the eye. The eye of a flat pattern and the eyes of the Analysis
folder while the panel of Section Analysis, Print Analysis, Measure or
Interference is open still wait until the command ends.

The eye of the Origin folder shows the design's origin in the view: its
point, its X, Y and Z axes and the XY, XZ and YZ planes. The design
saves the eye. A new design starts with it on, and looks at the design
from the top with the origin in the middle of the view, so you can find
the origin for your first sketch. When Create Sketch then asks for a
plane, the view first turns to the default view from the front, the
right and above, so that you can click any of the XY, XZ and YZ planes.
After the sketch you see what you make next from that side too. If you
turned the view yourself before Create Sketch, it stays as you chose.
Turn the view with the orientation cube when you want to see the design
from another side. A design that you open keeps the eye as it was saved,
and opens in the default view.

The default view is also the home view of a design that has none of its
own (Home). It is a few degrees off the exact isometric view, so that
the edges of the origin planes do not lie on top of each other and the
three planes are easy to tell apart. View › Standard Views › Isometric
View (key 7) and the front right top corner of the orientation cube
still give the exact isometric view. You do not need the origin to start a sketch: Create
Sketch always shows the origin planes for the pick (refer to [Sketch mode](#sketch-mode)), and every sketch
shows its own origin and axes, whatever the eye of the Origin folder
says.

In the other commands the eye has priority. A command that takes a
plane, an axis or a point, for example Offset Plane, Revolve or the
Start Object of Extrude, does not show a hidden origin, so the origin
planes do not cover the faces that you want to select. To use an origin
plane, axis or point in such a command, click its row (**O**, **X**,
**Y**, **Z**, **XY**, **XZ** or **YZ**) in the Origin folder of the
Browser: the command takes it as it takes a click in the view. Or turn
on the eye of the Origin folder: then the view shows the origin, also
during the commands, and you click it there.

**Show or hide a kind in the whole design.** The **View › Show** menu
has three entries: **Origins**, **Construction** and **Sketches**. Each
entry shows or hides that kind in all the components of the design, also
in linked components:

- **Origins**: the origin of the design and the joint origins.
- **Construction**: the construction planes, axes and points.
- **Sketches**: the sketches.

Use these entries in a large assembly, where it is slow to click the
eyes of each component. The eyes do not change, except the eye of the
Origin folder (see below). The Browser shows the
rows of a hidden kind in gray. When you turn the kind on again, the view
shows what the eyes show. The design saves the three entries. Each
change is an Undo step. The entries also work while a command's panel or
a sketch is open. The sketch that you edit stays in the view, and so do
the sketches that a command shows for its inputs. Create Sketch also
shows the origin planes for the pick while Origins is off. You can find
the entries in the command search and give them keys in Keyboard
Shortcuts.

The origin of the design shows only when **Origins** and the eye of the
Origin folder are both on. So each of them turns the other on when it
shows the origin: **Origins** turns on the eye of the Origin folder, and
the eye turns on **Origins**, in one Undo step. **Origins** has a check
mark only while the view shows the origin of the design. To hide the
origin, turn either one off.

When you edit a feature, the view shows the sketches that the feature
uses, also hidden sketches. A hidden sketch is a little lighter. You can
select its profiles, curves and points for the inputs of the panel. When
you close the panel, the sketch hides again. To keep the sketch in the
view, click its eye while the panel is open. A new command, for example
Extrude, also shows the hidden sketch of the profile that it starts
with.

The radio button after the name of a component activates that
component. Press F2 to rename a row.

**Sketches under their features.** A sketch has a row in the Sketches
folder, and the features that use it are rows under it. You can also see
it the other way round: turn on **Show used sketches under their
features** in Preferences › General. The Browser changes at once, in
every window:

- Each component has a **Features** folder after Bodies. It lists the
  features of the component in the order of the timeline.
- Under each feature are the sketches that it uses. Open the row of the
  feature to see them.
- A sketch that several features use is under each of these features.
  Its icon then has a small mark with two rings, and its tooltip tells how
  many features use it ("Used by 2 features").
- The Sketches folder keeps only the sketches that no feature uses.

The rows under the features are the sketches themselves. Their eye,
selection, rename and context menu (Edit Sketch) are those of the sketch,
so they work the same from each row. When you select one row of a
sketch, all its rows are selected. The context menu of a feature row is
the menu of the feature in the timeline. Turn the setting off to get the
Browser as before. The timeline does not change.

**Find a part from its row.** Move the pointer over the row of a body
and the view highlights that body. Over the row of a component (below
the root), the view highlights all the bodies of that component, also
those of the components in it. The highlight shows through the other
bodies, a little fainter, so you also find a part that is hidden inside
the assembly. The selection, the eyes and the view do not change, and
the highlight goes when the pointer leaves the row. Move the pointer
down the rows to find the parts of a large assembly one after the other.

**Hide in the view.** In the view, the context menu of a face, an edge,
a vertex or a body has the entry **Hide**. It holds two entries:

- **Hide Body** hides the bodies of everything that is selected.
- **Hide Component** hides the occurrences that put these bodies in
  place. The bodies of the root have no such entry.

When there are several bodies or components, the entries count them
("Hide 3 Bodies"). These entries do what the eyes do. One Undo step
undoes them.

**See inside: Transparent.** The context menu of a body or a component
has the entry **Transparent**, in the view and in the Browser. It
applies to all the bodies and components that are selected. On a
component it applies to all that the component holds, also its
components. The view shows a transparent body as a faint surface with
its edges, in the color of its appearance, and you see the other parts
behind it. Thus you look inside an assembly, and its housing stays in
sight. The entry has a check mark while all the selected bodies and
components are transparent. Click it again to make them opaque. The
appearance and the material do not change.

- The eye stays an eye with two states. A click on it shows or hides
  the row, as before. While a transparent body or component is shown,
  its eye is half open.
- A hidden transparent body stays transparent. When you show it again,
  it is transparent.
- A click in the view goes through a transparent body to the body
  behind it. Where there is no other body behind it, the click selects
  the transparent body. In the Browser, a click on its row always
  selects it.
- The design saves the state, as it saves the eyes. Each change is one
  Undo step.
- In the rendered view a transparent body shows its appearance with
  less opacity. In the wireframe styles every body shows its edges only,
  so a transparent body looks as the others do.

**The context menu of a row.** The context menu of a Browser row has
these entries:

- Edit
- Move/Copy (for bodies and components: the rows that are selected
  with the row, or the row alone)
- Show/Hide
- Transparent (for bodies and components: the rows that are selected
  with the row, or the row alone)
- Isolate
- New Component
- Move Features into a Component
- Copy
- Paste
- Paste New
- Move to Component… (for a component: another parent). The joints,
  constraints, rigid groups and moves that put the component in place
  go with it.
- Ground
- Create Components from Bodies
- Find Repeated Parts (for bodies of an imported file)
- Move to Component (for bodies and sketches)
- Create Sketch
- Look At
- Rename
- Delete (for a body: **Delete Body**, or **Delete Bodies** for the
  bodies that are selected with the row)
- Find in Timeline
- Save as Mesh… (for bodies and components)

**Delete Body** in the menu of a body's row deletes only that body. The
Delete key does the same on the selected rows of bodies. The other bodies
of the feature stay. For example, delete one body of an imported file,
and the other bodies of the import stay. MitCad adds a Remove Body
feature to the timeline, as **Delete › Delete Body** in the context menu
of the view does. One Undo step brings the bodies back. To delete the
feature that made the body, use the row of the feature in the timeline.
- Collapse All

**Collapse All** is the last entry of the menu of every row. It folds
the whole tree below the top row. The row that you clicked makes no
difference. Everything below the folded rows is folded too. The top row
(the root) stays open. Thus its children are in sight, and each of them
is folded.

Each row opens with its own arrow. Thus you open the tree again one
level at a time. The arrow of the top row still folds the top row.
Collapse All changes only what the Browser shows. It changes nothing in
the design, and no light bulb changes.

In the floating layout (the default), the **Browser background**
setting in Preferences › Display selects between the Browser's card
and **Rows**. Refer to [Layouts](#layouts).

**When a design opens.** A design that opens from a file opens with
the component that was active when the design was saved. Thus, if you
worked in a component and saved, that component is active again when
you open the design, and a sketch on one of its faces does not ask you
to activate it again. This also applies to a version, a copy and a
recovered design. An import opens with its root active. This is not an
Undo step, and it does not mark the design as modified. A new design starts with its Component1
active, so its first sketch and feature go into Component1. This
applies to the untitled design at the start, to New, to the empty
design after Close, and to New Design in a project.

When the Browser tree is longer than the Browser, the design opens with
the root open. Every row under the root is folded. Thus the components
and folders of the root are in sight, and their arrows are closed. A
tree that fits in the Browser opens as it is. Later changes keep the
rows as you left them.

**Reorient Design.** **Reorient Design…** is in the root's menu and in
the command search. It turns the whole design by 90, 180 or 270 degrees
about the X, Y or Z axis of the origin. Or it turns the design so that
the XZ or YZ plane becomes XY. Use it for a design that was started on the
wrong origin plane.

Reorient Design is not a move of the bodies. The sketches, the
construction planes, axes and points, and every feature turn with the
design. MitCad rewrites their references to the origin planes and axes
to the turned ones. For example, a sketch on XY turned 90 degrees about
X lies on XZ. It faces the other way, and its geometry is turned, not
mirrored. Imported bodies get a Move after their base feature.
Components keep their places relative to the turned design. Named
views, section analyses and the lights put in the design turn too.

The design shows turned while the dialog is open. OK keeps the turn as
one undo step. Cancel takes it back. The names of faces and edges stay.
Thus everything built on them still finds them. Sometimes a feature
would fail, or a body would not land exactly where the turn takes it.
This occurs with a rule of the design axes that a feature cannot carry,
such as the direction in which a cylinder's axis is taken. Then the
dialog names the feature or the body, and OK stays off.

### Units

Each design has a length unit of its own: mm, cm, m, in or ft. The
Browser shows it in the row **Design Settings › Units**. A number that
you type without a unit is in this unit. For example, `1` in a design in
inches is 1 in (25.4 mm).

To change the unit of the open design, double-click the Units row, or
use **Change Active Units**. The change is one undo step.

Every length that you type is read in the unit of the design: the value
fields of the sketch tools (a rectangle's width, a circle's diameter), a
dimension's value, the values of a command panel and of Edit Parameters.
In a design in inches, a rectangle typed as 0.875 by 12.125 is 0.875 in
by 12.125 in (22.225 mm by 307.975 mm). **Add User Parameter**, and Add
Option and Add Column of Configurations, start with the unit of the
design. A dimension that was made while the design had another unit
takes the unit of the design when you type a new value into it: `0.25 *
2` is 0.5 in. Changing the unit does not change the sizes that the design
already has: a bar drawn as 0.875 mm stays 0.875 mm, now shown as 0.03.
The chat's assistant is told the unit of the design with each message.

The values that a command starts with are in the unit of the design,
rounded to a usual size of that unit. In a design in inches, **Extrude**
starts at 0.5 in (10 mm in a design in mm), a sketch chamfer at 0.1 in and
**Sheet Rules** at a thickness of 0.05 in. In cm and m the value is the
same length (10 mm is 1 cm). Standard sizes keep their value in mm, for
example a thread's M10x1.5, a gear's module, a 608 bearing and the
filament diameter of Print Analysis. The Browser's sheet rules row (for
example `SheetRules1 (0.05 in, R 0.1 in, K 0.44)`) and **Measure** also
show lengths, areas, volumes and positions in the unit of the design. The
layout grid steps in inches in a design in in or ft, and Grid Settings
shows its fixed spacing in the unit of the design: refer to [View and
navigation](#view-and-navigation).

A new design starts in the unit of **Preferences › General › Default
units for new designs**. The default is mm. Set it to in, for example,
and every new design starts in inches: the untitled design when MitCad
starts, **New** and **New Design** in a project. The new design is not
modified and has no undo step for its unit.

An untitled design that has nothing in it yet, such as the one that
MitCad starts with, is still a new design: when you change the
preference, it starts again in the new unit. The Start page's New Design
card shows the unit. The preference does not change any other open
design: a design that has a sketch or a body, designs that you open and
files that you import keep their units. For such a design, use Change
Active Units.

### Timeline

The timeline shows the features as icons. Each icon has the color of
its component. A failed feature is red with the error's badge (a red
circle with "!"), and its tooltip says "Failed:" and the error. A
feature with a warning is yellow with the warning's badge (a yellow
triangle with "!"), and its tooltip says "Warning:" and the warning. A
feature with a warning has the same badge and word everywhere: in the
Browser, in the selection's card, in its command's panel (a yellow box
with the warning) and on the status line. A note of an import is
information (a blue "i" badge). Refer to [Reading an imported
timeline](#reading-an-imported-timeline) and [Messages and their
levels](#messages-and-their-levels).

When the features do not fit, a scroll bar shows under them. Turn the
mouse wheel over the timeline to scroll it: down moves to the newer
features on the right, up to the older ones on the left. A sideways
swipe on a trackpad scrolls it too.

Hover over a feature to preview its geometry in light blue without changing
selection. A folded group's cell previews its active members. Dashed boundaries
remain visible through solid bodies, so enclosed details can be located. Leaving
the timeline clears the preview. Selected features use the same indication
through solids and retain their usual blue shading on visible faces.

After a short pause, the tooltip shows the feature's kind, name, component and
main dimensions, using the same labels as its editor. A failed, suppressed or
rolled-back feature says **No geometry to show**. The selection card names the
selected timeline features and their components and shows their main dimensions.

A selected or hovered Fillet or Chamfer also shows the edges it picked, as
blue lines where they were before the feature rounded or bevelled them
away; the parts behind or inside the body are dashed. On a dark background
the lines are a lighter blue with a dark outline. Its own faces are then
shaded amber, so that they stand apart from the lines. The lines only show
where the edges were: they cannot be clicked, and nothing is rolled back or
rebuilt to show them. The selection card names how many edges the feature
picked and its size (for example, **12 edges; Radius: 2 mm**).

**Selection.** A click on a feature selects what the feature made:

- The bodies, the sketch or the construction geometry that the feature
  created
- For a feature that changes a body (fillet, chamfer, hole, shell and
  other such features), the faces that the feature made

Shift+click selects a run of features. These features highlight
nothing:

- A feature after the marker
- A suppressed feature
- A feature whose faces a later feature removed

A double-click on a feature edits it.

**The context menu of a feature.** The context menu of a feature has
these entries:

- Edit
- Auto Fix. This entry shows only for a feature with a warning that a
  sketch change caused, such as "region … found as … after a change in
  Sketch9". MitCad used what it found in place of the old reference.
  Auto Fix writes that into the feature, so the warning goes and the
  bodies stay the same. Select several features, or open the menu of a
  timeline group, to fix them all. One Undo takes it back. A reference
  that is gone or that matches several faces is not fixed: the status bar
  tells you to pick it again. The tooltip of such a feature in the
  timeline and in the Browser says that Auto Fix can fix its warning, and
  a click on the warnings badge offers Auto Fix for all of them.
- Rename
- Suppress
- Delete. MitCad asks you about the dependent features.
- Move to Component. For a component made from bodies, the entry is
  **Move the Features That Made It into the Component**.
- Roll Timeline Marker Here

**Feature names.** Features can have the same name. For example, you can
name a sketch and the extrude made from it both Holder. A name cannot be
empty. New features still get a name that is not in use, such as
Extrude3. Bodies, components and parameters must still have names of
their own. A script or an agent that gives a feature by a name that
several features have must give the feature's id (F2) instead. If only
one of these features is of the kind it needs, for example one sketch,
MitCad uses that feature.

**The marker and the features.** Drag the marker to roll the design
back. Drag a feature to move it. If you drag a feature to a place that
is not permitted, the place shows in red with the reason.

**Failed features and warnings.** The status bar shows "n failed". A
click on it lists the failed features and goes to them. The yellow "n
warnings" is beside it. It lists the features with warnings. The
fallbacks, partial items and placeholders of an import are among them.
It goes to these features in the same way. In the Floating layout,
both are in the header of the Browser ([Layouts](#layouts)).

### Timeline filter and search

**The timeline filter.** The filter button is after the playback
buttons. It shows the features of some components only. The filter has
these choices:

- All Components
- Active Component. The filter obeys the radio button in the Browser.
- A component that you select from the list.

Include Subcomponents adds the components that are put in the chosen
one.

A new window starts at Active Component with Include Subcomponents on.
While the root component is active, the timeline shows all features.
When you activate a component, the timeline shows only the features of
that component and of its subcomponents.

MitCad saves your choice of All Components or Active Component, with
Include Subcomponents. The next window and the next start use it. A
filter on one component that you select from the list applies only to
its window. MitCad does not save it.

The filter button is highlighted while the timeline hides features. Its
tooltip names the filter and tells how many features the timeline
shows.

**Show in Timeline Only** on the Browser row of a component does the
same. The joints and rigid groups that put an occurrence of a shown
component in place stay in the timeline.

The other features take no room. A dotted mark stands for each run of
them. The marker still rolls back every later feature, shown or not.
Its tooltip says how many hidden features it rolled back. The playback
buttons step between the shown features only. A feature that you drag
lands directly before or after a shown neighbor. Find in Timeline on a
hidden feature shows all features again in that window.

**The timeline search.** The search opens a field on the timeline.
Open it in one of these ways:

- Click the magnifier after the filter button.
- Press Ctrl+F (Cmd+F on a Mac) while the timeline or the Browser has
  the keyboard focus.
- Select Edit › Search Timeline.

The search finds the features whose name or type contains every word
that you type. Examples are "fillet2", "rounded edge", "fillet" and
"construction plane". A "quoted phrase" is one word. The search looks
only at the features that the timeline filter shows.

The matches are outlined in orange. They are also listed above the
field with the color and the name of their component. This is
necessary because imported designs repeat names across components.

Move between the matches in these ways:

- Enter or Down goes to the next match.
- Shift+Enter or Up goes to the previous match.
- A click on a listed match goes to that match.

MitCad scrolls the match into sight and selects it. The selection is
the same as the selection of a click on the feature.

You can type filters in the same field. The filter menu of the field
also gives them:

| Filter | Finds |
| --- | --- |
| `in:Part` | The features of the components whose name contains the text. For a name with spaces, type `in:"Two words"`. |
| `is:failed` | The failed features |
| `is:warning` | The features with warnings, as the yellow "n warnings" counts them |
| `is:note` | The features with a note of the import (information); also `is:info` |
| `is:suppressed` | The suppressed features |
| `is:rolled-back` | The rolled-back features |

Filters of one kind are alternatives: `is:failed is:warning` finds
either. Filters of different kinds all apply: `fillet in:Cover
is:warning` finds the fillets with a warning in the Cover components.

Press Esc to close the search. When you clear the search, MitCad brings
back the scroll position and the selection from before the search. But
if you selected something else in the meantime, MitCad keeps that
selection.

### Messages and their levels

Every message of MitCad has a level. A level looks the same everywhere:
on the status line, in the Browser, on the timeline, in the selection's
card and in a command's panel. A color never says the level alone: an
error and a warning also have their icon and their word.

| Level | When | Looks like |
| --- | --- | --- |
| Error | The result is missing: the work stopped. A feature that builds nothing, a file that was not written, conflicting constraints. | A red circle with "!", the word "Failed", red text. In a command's panel, a red box. |
| Warning | It ran, but something is wrong or left out. A fillet that lost some of its edges, a partial import, a broken annotation, redundant constraints. | A yellow triangle with "!", the word "Warning", yellow-brown text (yellow on a dark theme). In a command's panel, a yellow box. |
| Information | A fact that needs nothing from you. A note of an import, a done export. | Plain text; a blue "i" badge on the timeline. |
| Success | The answer to a check that you asked for, such as "up to date". | A green check mark. |

A hint (the next step of a command) and progress ("Computing preview…")
are plain text. States such as suppressed, rolled back or hidden are no
level: MitCad draws them faint or struck through.

An error or a warning in a command's panel stays in its box until you
fix the value or change the selection. An import or an export with
warnings says so once on the status line, with their count and the
first one. The import's report and the log have them all.

### Find in Design

Find in Design is a window beside the Browser. It finds components and
bodies by name, and it acts on all the matches at the same time.
Designs with thousands of elements need this. Open it in one of these
ways:

- Click the magnifier on the title row of the Browser.
- Select Edit › Find in Design.
- Use the command search.

**The search text.** MitCad matches the text anywhere in a name. The
case of the letters makes no difference. `*` stands for any characters,
and `?` stands for one character. Examples are `bracket*left` and
`plat?`.

**The list.** Components and Bodies select what the list shows.
"Visible only" or "Hidden only" narrows the list. The list changes with
the text as you type, after a short pause. It also changes with the
design when the design changes. Each row shows the name, the kind and
the place in the assembly. The window also shows a count such as "238
bodies, 12 components".

The search looks at the names of the design, not at the rows of the
Browser. Thus it also finds the branches that are folded or that were
never opened. A component that is put in place twice gives one row for
each place for each of its bodies. But the body is one body. Thus,
when you hide it, it is hidden in every place.

**Selection in the list.** When you select rows, MitCad selects them in
the view and in the Browser. Select All and Select None select the
rows of the list. A double-click on a row shows the row in the
Browser.

**The buttons.** The buttons act on the selected rows. When no row is
selected, they act on all rows.

| Button | Effect |
| --- | --- |
| **Show** | Shows the rows, as the eyes of the Browser do. One Undo step undoes it. |
| **Hide** | Hides the rows, as the eyes of the Browser do. One Undo step undoes it. |
| **Appearance…** | Starts the Appearance tool with the bodies picked. The bodies of a component are picked for it. |
| **Select** | Makes the results the selection, for every command that takes a selection. |

The debug log records each action with its count, for example "hid 238
bodies".

### Parameters and expressions

A value that you type into a sketch dimension or a feature becomes a
parameter. For example, the width and the height of a rectangle become
the dimensions and the parameters `d1` and `d2`. The depth of an
extrude and the radius of a fillet are parameters too. The values of a
gear, for example its **Module**, **Teeth**, **Pressure Angle** and
**Face Width**, are also parameters. You can type expressions into
them, and you can change them later in the parameters.

**Edit Parameters.** Open **Edit Parameters** from the Modify group,
from the Edit menu, or with its button at the end of the navigation bar
at the bottom of the view, which is there in every workspace and in
sketches (in a drawing, in the drawing's bar). It lists the user parameters and the model
parameters with their expressions and comments. To change a value,
double-click it and type a number such as `50` or an expression such
as `d2 * 2`. MitCad computes the design again. A fillet stays on the
same edges. MitCad refuses a change that makes a cycle or has a wrong
unit. The refused change stays in the cell in red with the reason.
When you rename a parameter, MitCad updates the expressions that use
it.

**Edit Parameters while you edit.** You do not have to finish a
sketch or close a feature panel to change the parameters. In a sketch,
open **Edit Parameters** from Sketch › Modify or from the command
search. The sketch stays open, and its dimensions and geometry follow
each change at once. Undo in the sketch takes a change back. In a
feature panel, click the **Edit Parameters** button at the right of
the panel title, or use the command search. The panel stays open
with your values, expressions and selections. MitCad calculates the
values in the panel and the preview again after each change. If you
rename a parameter, the expressions in the panel use the new name. A
change stays when you cancel the panel. When you click OK in the panel,
the change and the feature are two separate undo steps. You cannot
change the name or the expression of a value of the feature that you
edit in the parameter table. Change it in the panel. Click OK in the
parameter table to go back to the sketch or the panel.

**Aligned numbers.** In the **Expression** and **Value** columns, the
decimal separators of the numbers are on one vertical line. A whole
number such as `3 mm` has its separator after the last digit. The units
are in a column of their own, so a unit of another length does not move
the numbers. A formula such as `d1 * 2` stays a text. MitCad does not
change the value or the expression for this: when you edit a cell, you
see and change the whole text. If a column is too narrow for its
numbers, make it wider.

**Limits.** A parameter can have a minimum, a maximum and a step, so
that you do not need a formula such as `min(max(…))`. Type them into the
**Min**, **Max** and **Step** columns of Edit Parameters. The minimum
and the maximum are expressions in the unit of the parameter and can
use other parameters, for example `Wall * 2`. The step is a positive
value, for example `1` for whole numbers or `0.2 mm`. Clear a cell to
remove the limit.

The value of a parameter with limits is the result of its expression,
limited to the minimum and the maximum and rounded to the step. The
steps start at the minimum (at zero without a minimum). A result
exactly between two steps goes to the upper step. The expression stays
as you typed it. The **Value** cell shows the result and the limited
value with a mark, for example `372 mm → 300 mm ▲max` (also `▼min` and
`◆step`). Its tooltip tells which limit took effect and where the limit
comes from. While you type an expression, the message line below the
table tells you at once if the value will be limited. Select **Only
limited** to see only the parameters whose value a limit changed.

Select **Strict** if a value outside the limits is an error: MitCad
then refuses such a value, as it refuses a wrong unit, and tells you
why. A parameter with a minimum and a maximum has a slider in the
**Slider** column. Drag it, or click its handle and use the arrow keys,
Home or End. Each change is one undo step.

If a feature limits the value that a parameter gives it, for example
the quantity of a pattern (1 to 10000, whole), the Min, Max and Step
cells show these limits in gray until you give the parameter limits of
its own. A feature panel field shows the same mark when its parameter
has limits, and so does the tooltip of a sketch dimension. The limits
are saved with the design and stay when you apply a configuration.

**Expressions with units.** A value field takes an expression with
units, for example `20`, `2 in` or `d1 * 2`. The value or an error
shows below the field. An expression can use the parameters of the
design and functions such as `max`, `sin` and `cos`. For example, a
point dimensioned from a center with `r * cos(a)` and `r * sin(a)`
moves with `a` through all four quadrants.

**Decimal comma and decimal point.** A number takes a decimal comma or
a decimal point. `1,5 mm` is `1.5 mm`, and MitCad shows it as `1.5 mm`.
A comma directly between two digits is a decimal comma. Any other comma
separates the arguments of a function. A `;` always separates the
arguments. Thus `max(a, b)`, `max(1, 5)` and `max(1; 5)` have two
arguments, but `max(1,5)` is `max(1.5)`. There are no thousands
separators: `1,000` is 1, and `1 000` is an error.

**Inch and foot marks.** A `"` directly after a number means inches,
and a `'` means feet, with or without a space between: `.875"` is
0.875 in (22.225 mm, also in a design in mm), `2.5 "` is 2.5 in and `3'`
is 3 ft. Two single quotes, `2''`, are inches too. Feet and inches
together add up: `3' 6"` is 3 ft 6 in (42 in). The value below the
field shows the converted value. Only a number takes a mark; elsewhere
quotes enclose a text, as in `'Width'` or `VALUE(d1; "in")`.

**Formulas like in a spreadsheet.** An expression can use the functions
and the syntax of spreadsheet formulas. All expressions that you wrote
before work as they did.

- The case of a function name does not matter: `FLOOR`, `Floor` and
  `floor` are the same function. `PI` and `pi` are the same too. The case
  of a parameter name and of a unit matters: `d1` and `D1` are two
  parameters, and `MM` is not a unit. A parameter cannot have the name of
  a function in any case, for example `Max` or `Sum`. When an older
  design or an imported file has such a parameter, MitCad gives it a new
  name, for example `Max_1`, tells you so, and changes the expressions
  that use it.
- Inside parentheses a single `=` compares, as in a spreadsheet:
  `IF(GridX = 6; a; b)`. `==` also compares. At the start of a field,
  `width = 40` still names the parameter of the field.
- Rounding: `ROUND(x; 2)` rounds to two decimals, `ROUNDUP`,
  `ROUNDDOWN` and `TRUNC` likewise, and `INT(x)` rounds down to a whole
  number. `FLOOR(d1; 0.5 mm)` and `CEILING(d1; 0.5 mm)` round down or up
  to a multiple of 0.5 mm, `MROUND(d1; 5 mm)` to the nearest multiple.
  The decimals count in the default unit, for example in millimeters.
  Floating point noise does not change the result:
  `ROUNDUP(0.1 + 0.2; 1)` is 0.3.
- Numbers: `MOD(a; b)` is the remainder with the sign of `b`, and `%`
  stays the remainder with the sign of `a`. `QUOTIENT(a; b)` is the whole
  part of `a / b`. `EVEN(x)` and `ODD(x)` round away from zero to an
  even or odd number.
- Many values: `SUM`, `AVERAGE`, `MEDIAN` and `PRODUCT` take any number
  of values, for example `SUM(a; b; 5 mm)`. `CLAMP(x; 2 mm; 8 mm)` keeps
  `x` between two limits.
- Choices: `IFS(x < 10; 1; x < 20; 2; 1; 3)` takes the value after the
  first true condition. `SWITCH(n; 1; 10 mm; 2; 20 mm; 30 mm)` takes the
  value after the first match. When nothing matches, it takes the last
  value, here `30 mm`, if the count of values leaves one over.
  `CHOOSE(2; a; b; c)` takes the second value. `IFERROR(a / b; 0)` gives
  0 when `a / b` is an error, for example a division by zero. An unknown
  name or a wrong unit stays an error.
- Names in a formula: `LET(r; d1 / 2; PI * r ^ 2)` gives `d1 / 2` the
  name `r` for the rest of the formula. Such a name is not a parameter,
  and it cannot be the name of a parameter.
- Units: `VALUE(d1; "mm")` is the number of millimeters in `d1`, without a
  unit: 15 for 15 mm. `RADIANS(30)` is the angle 30 degrees and
  `DEGREES(PI)` the angle 180 degrees, so `SIN(RADIANS(30))` is 0.5.
  `PI()` is `PI`.

The function names are in English in all languages.

**Text parameters.** A parameter with the unit `Text` has a text as its
value instead of a number, for example the label `6qt` on a lid. In
**Add User Parameter**, choose the unit **Text**, or type a text
expression and leave the unit empty. Write a text in single or double
quotes: `'6qt'` or `"6qt"`. To put a quote of the same kind into the
text, type it twice: `'It''s'`. A text can use other parameters:

- `&` joins texts and numbers: `"Box " & GridX & "x" & GridY` gives
  `Box 6x4`. A number with a unit shows its unit: `"W" & Width` gives
  `W15 mm`.
- `TEXT(x; "0.00")` writes a number with two decimals, `TEXT(Width;
  "0.0 in")` in inches. `TEXT(VALUE(Width; "mm"); "0")` gives the number
  without the unit.
- `CONCAT(a; b; …)` joins like `&`. `LEFT(t; 2)` and `RIGHT(t; 2)` take
  the first or the last two characters, and `LEN(t)` counts the
  characters.
- `IF`, `IFS`, `SWITCH` and `CHOOSE` can choose texts, for example
  `IF(GridX > 4; "large"; "small")`, and `=` compares texts.

A text is never a number: MitCad refuses a text parameter in a length,
an angle or a count, and a number as the value of a text parameter. Use
`&` or `TEXT(…)` to make a text from a number. A text parameter has no
limits and no slider. A parameter can still be named `Text`, `Left` or
`Len`: these names are functions only before `(`.

**Sketch text from a parameter.** In **Edit Text**, type a text
parameter, for example `Label`, or a text expression, for example
`'Box ' & GridX`, into **Text Parameter**. The text then shows the value
of the parameter and changes with it, and so does an emboss or an
extrude of the text. An expression becomes a parameter of the sketch.
Clear **Text Parameter** to type the text yourself again.

**Driven dimensions.** The value of a driven dimension is a read-only
parameter (`d7`, of the kind reference). You can rename it, and you can
use it in the expressions of the features after the sketch. If you type
a value into a driven dimension, the dimension becomes driving.
A dimension of the same sketch can use it too (`(d7 - d2) / 2`) when it
cannot change what `d7` measures; MitCad refuses a circular dependency
and names the dimensions.

**Parameter names.** When you hold the pointer over a dimension, MitCad
shows its parameter, for example `d3 = d1 / 2 → 20 mm (Sketch1
distance)`. The value editor shows the name before the field (`d3 =`).
If you type `width = 40`, MitCad renames the parameter and sets its
value at the same time. While the editor is open, a click on another
dimension inserts the name of that dimension. The *Dimension text*
option of the palette shows the value of each dimension, its name, or
both (`d3 = 20`).

The value field of a feature panel shows the name of its own parameter
after the label (`Distance (d5)`). Its tooltip says what the parameter
is, as the tooltip of a dimension does (`d5 = d1 / 2 → 20 mm (Extrude2
distance)`). The field also takes `name = value`.

**Labels in the view.** When you click into the value field of a feature
panel or type in it, the view shows labels on the dimensions of the
sketches that the view shows. A click on a label inserts the name of the
label at the cursor of the field. When a panel opens, its first value
field has the keyboard focus, but the view shows no labels until you
click into the field or type. The values of other features have no
labels: type their names (for example `depth`), with the help of the
parameter suggestions below.

**Parameter suggestions.** When you type part of a parameter name into
a value field, a list shows the parameters that match. This applies to
feature panels, sketch dimensions, the value fields of sketch tools and
the expression cells of Edit Parameters. The list shows the user
parameters first, then the model parameters. Each row shows the name,
the value with its unit and the comment. Use Up and Down to move in the
list. Enter or a click puts the name into the field in place of what you
typed. The rest of the expression stays. Enter in the list does not
click OK. Esc closes only the list. If the field cannot use a
parameter, for example an angle as a length, the row is dimmed and the
list tells you why.

**The fx button.** Click **fx** at the right of a value field to see all
the parameters, also when the field is empty. Type to search the names
and the comments. The text in the field does not change until you choose
a parameter.

**Name a value or make a parameter.** Type `name = value` to give the
field's own parameter a name, for example `width = 30 mm`. When you type
a name that no parameter has, the list offers this form. The list also
offers **Add User Parameter**. It makes a new user parameter that is
independent of the field's own parameter. Give its value or expression
and its unit. MitCad refuses a name that exists already, an invalid
name, a wrong unit or an expression that the design refuses. The editor
then stays open with the reason. When the parameter is made, MitCad puts
its name into the field where you typed it. The new parameter is in
Edit Parameters and in the lists at once. In a sketch, Undo takes it
back. In a feature panel it stays when you cancel the panel. The
right-click menu of a value field has the same actions.

**Remove a name.** A sketch dimension whose parameter you named shows
the name in its value editor, for example `width = 20 mm`. Delete the
`width = ` part and press Enter, or choose **Remove Name** in the fx
list or the field's right-click menu, and the parameter gets its
automatic name back, such as `d1`. The right-click menu of the dimension
in the sketch offers **Remove Name width** too. Other dimensions and
parameters that use `width` change to the new name, in one step that
Undo takes back. Backspace is quicker: the first press deletes the
value, the next removes the whole `width = `, so you can type a new value
or another parameter's name and press Enter. Ctrl+Z in the field brings
the name back. The field's tooltip says so while it shows a name.

#### Tables

A table keeps data that formulas look up, for example the sizes of
screws from a standard. Each column has a name and a unit, and each row
is one size:

| Size | d (mm) | dk (mm) | k (mm) |
|---|---|---|---|
| M3 | 3 | 5.5 | 3 |
| M4 | 4 | 7 | 4 |
| M5 | 5 | 8.5 | 5 |

A formula names a column as `Table.Column`, for example `Screws.dk`, and
looks a value up with a function:

- `XLOOKUP(Size; Screws.Size; Screws.dk)` finds the row whose `Size` is
  the value of the parameter `Size` (for example the text `'M5'`) and
  gives its `dk`: 8.5 mm. A fourth value is used when no row matches,
  for example `XLOOKUP(Size; Screws.Size; Screws.dk; 6 mm)`. A fifth
  value `-1` takes the next smaller row when no row matches exactly, `1`
  the next larger one.
- `MATCH(Size; Screws.Size)` gives the number of the row, and
  `INDEX(Screws.k; 3)` the cell of row 3, so
  `INDEX(Screws.k; MATCH(Size; Screws.Size))` gives `k` of the size.
- `MIN`, `MAX`, `SUM`, `AVERAGE` and `MEDIAN` also take a column of
  numbers: `MAX(Screws.dk)`.

A match is exact by default, and a text matches only with the same
capital and small letters: `'m5'` does not find `M5`. When no row
matches, the formula is an error, unless you give the value for that
case or use `IFERROR`. A number in a table has the unit of its column, so
a value from an `in` column comes into a millimeter parameter in
millimeters.

**Make and edit tables.** In **Edit Parameters**, click the **Tables**
tab. The list at the left shows the tables of the design:

- **New** makes a table. Give it a name, for example `Screws`. A table
  cannot have the name of a parameter. The new table has a column `Key`
  for texts, a column `Value` for lengths and one empty row.
- In the header of the grid, each column has its name and a unit box.
  Type a new name and press Enter: the formulas that use the column
  follow. Open the unit box and choose a length, an angle, **no unit**
  or **Text**; the mouse wheel and the arrow keys do not change it. When
  you change from one length unit to another, the numbers change so that
  the values stay the same: 25.4 mm becomes 1 in. The status bar says
  so, and Undo brings the old numbers back.
- Click a cell and type. A cell of a number column takes a number in the
  unit of the column, or a number with another unit of its kind: `0.25 in`
  in a millimeter column is kept as 6.35. Enter keeps the cell, Enter
  again goes to the cell below, and Delete empties a cell. A cell
  that MitCad cannot take, for example a text in a column of numbers,
  stays in red with the reason, and the table does not change.
- **Add Row**, **Remove Row**, **Add Column**, **Remove Column** and the
  arrow buttons add, remove and move the rows and columns.
- **Rename** renames the table; the formulas follow. **Delete** deletes
  it. MitCad does not delete a table, or remove a column, that a
  parameter uses. The line **Used by** under the grid shows the
  parameters that use the table.

Each change is one step to undo. When you change a cell, the parameters
that use it and the design follow at once. MitCad refuses a change that
would make a formula fail, for example a row removed that a formula
looks up, and says which parameter would fail.

**Tables as CSV files.** **Export CSV…** writes the selected table to a
CSV file. The header has the name of each column with its unit in
brackets, for example `Size [Text],d [mm],dk [mm]`; `[]` is a number
without a unit. **Import CSV…** reads a CSV file into a table, for
example one that you saved from a spreadsheet. Before anything changes,
the **Import Table** window shows the table's name (the file's name;
you can change it), the columns with their units, the rows, the cells
that MitCad cannot take, and the parameters whose values change. A table
of that name is made, or replaced whole. A column without a unit in
brackets is a column of texts, or of numbers when all its cells are
numbers. A cell such as `0.25 in` in a millimeter column is kept in
millimeters. MitCad does not import a file with a cell that it cannot
take, or a file that would make a formula fail. Click **Import** to
import, or **Cancel** to keep the design as it is. The import is one
step to undo.

**Type a table's columns.** When you type the name of a table in a value
field, the suggestions list it with the parameters. Choose it, and the
list goes on with its columns after `Screws.`. The Formula Editor does
the same as you type, and it lists XLOOKUP, MATCH and INDEX with the
other functions.

### Formula Editor

The **fx** list includes searchable functions with descriptions and examples,
as well as the design's parameters. Choosing a function inserts its name and
parentheses; the caret stays inside them. While typing a function call, a
signature such as `FLOOR(value; [step])` highlights the current argument.
Select a sub-expression, or pause the pointer over it, to inspect its value
and unit. Formula errors identify the part that failed.

Choose **Edit Formula…** in the fx list or a value field's context menu for a
larger, multiline editor. In **Edit Parameters**, select a parameter and
choose **Edit Formula…**. The function search includes names, descriptions
and examples. **Indent LET** adds whitespace to make a long formula easier
to read; it does not change the formula's meaning. As you type a name, the
editor lists the parameters, tables and functions that start with it, and
after a table's name and a dot its columns: Up and Down choose, Enter or
Tab puts the choice in, Esc closes the list. Ctrl+Enter is the same as
**OK**.

Templates insert ordinary formula text: **How many fit** inserts
`FLOOR(length / pitch)`, **Spacing for N items** inserts `length / (count - 1)`,
and the count and rounding templates insert `EVEN`, `ODD` or `MROUND`.
Replace the example names with your parameter names and continue typing.
There is no separate builder state: the text remains the formula.

For an existing parameter, **Minimum**, **Maximum**, **Step** and the strict
option edit its actual parameter limits. **Keep within limits** takes you to
these controls rather than inserting a clamping formula. Limits are saved
with the formula as one parameter edit; **Cancel** changes neither. A new
feature value needs to become a saved parameter before it can have limits.
An open feature editor still owns its own parameters, as in Edit Parameters.

### Parameters as CSV files

You can save the parameters to a CSV file and read parameters from one,
for example to edit them in a spreadsheet or to use the same values in
another design.

**Export.** In **Edit Parameters**, click **Export…** and choose a
file. MitCad writes one row for each parameter: `name`, `kind` (user,
model or reference), `unit` (empty for a number without a unit),
`expression`, `value` (a number in the unit of the parameter), `comment`,
`favorite` (`true` or `false`), and the limits of the parameter: `min`,
`max` and `step` as you typed them (empty when the parameter has none)
and `strict` (`true` or `false`). The file is UTF-8 text. Numbers have a
decimal point. A field with a comma is in quotes.

**Import.** Click **Import…** and choose a file. MitCad reads its own
CSV files and also the parameter table CSV of the `.f3d` application
(the header `Name,Unit,Expression,Value,Comments,Favorite`). Before
anything changes, the **Import Parameters** window shows each row of the
file and what the import does with it:

- **Add:** a new user parameter.
- **Change:** a parameter of the design with the same name gets the
  expression and the limits of the row. A user parameter also gets the
  unit, the comment and the star of the row. A parameter of a feature
  keeps its unit, its comment and its star; only its expression and its
  limits change. An empty `min`, `max` or `step` cell removes that limit.
  A file without these columns, such as the parameter table of the
  `.f3d` application, leaves the limits as they are.
- **Unchanged:** the parameter is already as the row has it.
- **Refused:** the row cannot be used, for example because its unit is
  unknown, its expression does not work and it has no value, a limit is
  wrong (for example a minimum above the maximum), or it changes the
  value of a driven dimension. The reason shows in the row.
- Rows with the unit `Text` are text parameters (see *Text parameters*
  under *Parameters and expressions*). When the expression of such a row
  does not work, the text in its `Value` column becomes the parameter's
  text.

The **Limits** column shows the limits of each parameter after the
import, and the **Value** column says when a limit changes the value.
A row can use a parameter of a later row. A name that MitCad cannot use,
such as `mil` (a unit), gets a new name such as `mil_1`, and the
expressions that use it follow. When an expression does not work (an
unknown name, a wrong unit, a circular reference), MitCad uses the number
in the `Value` column and says why. Click **Import** to import, or
**Cancel** to keep the design as it is. The import is one step to undo.
You cannot import while a sketch or a command panel is open.

### Values that follow parameters

A body's appearance and material, the appearance of a face, whether a
feature is suppressed, and whether a body or a component is shown can
follow parameters. One text parameter, for example `Color`, can then
switch the whole design between its variants: change it in Change
Parameters (or apply a configuration) and the looks, the materials and the
features follow.

- **fx** of the **Material** and **Appearance** tools (the button beside
  the picker button): a material or an appearance that follows a
  parameter. Turn it on and choose the parameter in **Varies With**: the
  options of the configuration and the parameters with a list of allowed
  values (with none, the line under it says to add one in
  Configurations…). A row per value appears, named by the value; the
  design's value now reads, for example, *Black (now)*. Each row starts
  as **As above**: such a value gets what the picker button above shows.
  Choose a material or an appearance for the rows that differ, then
  **OK**: the bodies (or faces) follow the parameter, in one Undo step.
  A body that already follows a parameter opens the tool with its rows
  shown, also one made before this tool existed; the panel says
  *varies with Color* for it. Turn **fx** off and press **OK** to set
  what the body shows now by hand. A component's default material does
  not follow parameters: pick its bodies. A name that no appearance or
  material has gives the body a warning (in its tooltip in the
  browser), and the body then has its default look. While a body follows
  a parameter, its **Appearance…** or **Material** in the context menu
  is marked *fx*. An expression of another form, such as `if(Length > 1
  m; 'paint_red'; 'paint_white')`, is written in Edit Parameters'
  Formula Editor and stays as it is until the tool sets a table.
- **Suppress When…** (in the context menu of a feature in the timeline
  and the browser): a condition, for example `Holes = 'No'` or `Length <
  500 mm`. The feature is suppressed while it holds.
- **Show When…** (in the context menu of a body or a component): the body
  or the component is shown while the condition holds.

A body, a component or a feature that follows parameters has a small
*fx* on its icon in the browser and the timeline, and its tooltip shows
the expression. Setting the value by hand (the Material and Appearance
tools without fx, Material ›, Hide, Suppress Features) ends it. **Remove
Condition** in the dialog ends it too and keeps the value it gave. Each change is one
step to undo. A parameter that is followed cannot be deleted; renaming it
changes the expressions.

## Configurations

A configurable design stands for a family of products: a shelf in two
colors and any length from 300 to 2000 mm, a screw in its sizes. Whoever
uses the design chooses a value for each **option**, such as Color and
Length. A **row** is a named, saved combination of the options' values,
with a designation and a part number. **Rules** take away the values and
combinations that are not allowed. A configuration only sets parameters:
an option is a user parameter, and everything that uses it (dimensions,
features, other parameters through their expressions) changes with it.

The words the window uses:

- An option's **possible values** are the values it can have at all: a
  list, a range, the values of a table column, or no limits of its own.
- Its **allowed values** are what is left of them for the current
  choices, once the rules (and **Only these rows**, if it is on) have
  taken values away. Length's possible values are 300–2000 mm; after you
  choose Black, the rule *Black is short* leaves the allowed values
  300–1000 mm.
- A **combination** is the options' values together, such as Black and
  1000 mm. An allowed combination does not need a row of its own.
- An option's **value** is its current value. An option can be **by
  formula**: its value is calculated from its expression, and no chooser
  sets it.

This chapter builds one example from the start to the end, a shelf:

| | |
|---|---|
| Options | **Color**: Black or White. **Length**: 300 mm to 2000 mm, in steps of 10 mm |
| Row | **Standard**: White, 1200 mm |
| Rule | **Black is short**: when Color is Black, Length is at most 1000 mm |

The steps say what you do, in the order you do it. What MitCad does
behind them is in [How MitCad applies rules](#how-mitcad-applies-rules)
at the end, and the window's other controls are in [More in the
Configurations window](#more-in-the-configurations-window).

### 1. When you need configurations

Use configurations when a design is made or sold in sizes or colors, and
the choice must stay valid: a chooser for each option at the top of the
browser, rows with their designation and part number for the bill of materials,
and rules that keep out the combinations that are not made. A design that
is inserted from a library offers the same choosers ([Components with
options](#components-with-options)).

If you only change a length now and then, you do not need them: change
the parameter in **Edit Parameters**
([Parameters and expressions](#parameters-and-expressions)).

A configuration starts from user parameters. The shelf is a board whose
extrusion distance is the user parameter **Length** (`1500 mm`), and a
text parameter **Color** (`'White'`, unit **Text**) that the board's
appearance can follow ([Values that follow
parameters](#values-that-follow-parameters)). Make them in Change
Parameters first.

![The Parameters window with the user parameters Color and Length](images/configurations/shelf-1-parameters.png)

### 2. Make the options

1. Open **Configurations…** from the Modify group of the Solid tab (beside
   **Edit Parameters**) or from **Edit › Configurations…**. The window
   has the tabs **Options**, **Rows**, **Rules** and **Bindings**. Each tab
   starts with a line that says what it is for; the Options tab's says
   "Options are the choices a user of this design makes, such as Color or
   Length. Each option is a user parameter." While the tab is empty, it
   says "No options yet. Add Option… makes a parameter an option."
2. Click **Add Option…**, choose **Color** and click **OK**. Do the same
   for **Length**. (With **New Parameter** in the same list you can make a
   new user parameter here: its name, unit and value.)
3. Give each option the values it can have. Double-click its **Possible
   Values** cell (or select the option and click **Edit Possible
   Values…**):
   - Color: **List**, with `Black` and `White` on lines of their own.
   - Length: **Range**, minimum `300 mm`, maximum `2000 mm`, step
     `10 mm`.
4. Keep the order: Color first, then Length. The options are chosen in
   this order, and an option's value can limit only the options after it,
   so put first the option that decides the most. Drag an option to its
   place, or select it and click **Move Up** or **Move Down**.

The possible values are stored as the parameter's limits, so Edit
Parameters shows them too. **Value** is the option's current value
(1500 mm for Length); it follows every choice you make later.

![The Options tab with Color and Length and their possible values](images/configurations/shelf-2-options.png)

### 3. Choose in the Configuration bar

Close the window. Above the browser, the **Configuration** panel now
has a chooser for each option, in their order: a list for Color, and a
field and a slider for Length. Type `1200` in the Length field and
press Enter: the board is now 1200 mm long.

Each choice is one step to undo, and the design is computed again at
once.

![The Configuration bar with Color White and Length 1200 mm](images/configurations/shelf-3-bar.png)

### 4. Add a row

A row saves a combination under a name, for the bill of materials and for a
quick choice. The Rows tab's line says "A row is a ready-made combination
of the options, with its designation and part number."; while the tab is
empty, it says "No rows yet. Add Row saves the options' values as a row."

1. Open **Configurations…** again and click the **Rows** tab.
2. Click **Add Row**. The new row, **Row1**, has the options' values as
   they are now: White and 1200 mm.
3. Double-click its **Name**, type `Standard` and press Enter.
4. Fill in **Designation**, **Part Number** and **Supplier URL** if the
   bill of materials needs them. Type a text without quotes (`White`) and a
   number with or without its unit (`1200` is `1200 mm` for a length).
5. Select the row and click **Set as Default**: its name turns bold. The
   **default row** is the row a library component is inserted with when
   no other row is chosen, and when MitCad adjusts a text option it
   prefers the default row's value. It is not the same as an option's
   current value.

The first box of the Configuration bar, **Row**, now chooses a row:
**Standard** sets White and 1200 mm at once. When the options match no row, the box
says **No row**. The line above the sheet names the row that the options
match now.

![The Rows tab with the row Standard](images/configurations/shelf-4-rows.png)

### 5. Add the rule

The shelf is not made longer than 1000 mm in black. The rule **Black is
short** says so. The Rules tab's line says "Rules take away values and
combinations that are not allowed. Most designs need none."; while there
are no rules, it says "No rules yet. A rule is, for example: when Color
is Black, Length is at most 1000 mm."

1. Click the **Rules** tab, then **Add** and **Empty rule**.
2. The new rule's name is ready to be typed: type `Black is short` and
   press Enter.
3. Click the **Text** under the blocks (or press Alt+T). While the rule is
   empty, the text shows an example made from your options. Type the
   rule, one line per part, with **Shift+Enter** between the lines:

   ```text
   when Color == 'Black'
   then Length max 1000 mm
   ```

   A text goes in single quotes (`'Black'`), and a number has its unit
   (`1000 mm`).
4. Press **Enter**. MitCad reads the lines and saves the rule, as one
   step to undo. Leaving the text saves it too, and **Esc** brings back
   the saved text instead.

The blocks above the text show the same rule, and the list on the left
shows it without a warning mark. If a line cannot be read (an option
that the design does not have, a number without its unit), MitCad marks
the line, the line under the blocks says where and why (for example
*Line 2, column 17: 1000 needs a unit: Length is in mm (1000 mm)*), and
nothing is saved until you correct it.

You can also start from a template. **Add › Limit a range when…** makes a
complete rule from the design's options at once, `when Color ==
'Black'` and `then Length max 2000 mm`, named *Limit Length when Color is
Black*, and saves it. Change `2000 mm` to `1000 mm` in the text and press
Enter (or double-click the number's block), and rename the rule
(double-click its name, or press F2). The other templates are in
[Rules](#rules).

![The Rules tab with the rule Black is short as text and as blocks](images/configurations/shelf-5-rule.png)

### 6. When a choice breaks a rule

Close the window and choose **Black** in the Configuration bar. Length
was 1200 mm, which is not one of Black's allowed values, so MitCad
adjusts it to the nearest allowed value, 1000 mm. A notice under the
choosers and the status bar say what was adjusted and why: *Length
1200 mm → 1000 mm: Black is short*. The board follows, and the Length
slider now ends at 1000 mm. The choice and its adjustment are one step
to undo.

![The Configuration bar after choosing Black: Length adjusted to 1000 mm, with the notice](images/configurations/shelf-6-moved.png)

- Choosing **White** again keeps 1000 mm: rules only take values away,
  they never set one back. Choose the row **Standard** to get 1200 mm.
- A value that a rule does not allow is not written. In the Rows tab, a
  row with Black whose Length you type as `1500` keeps its old value; the
  cell stays red, and its tooltip names the rule.
- A row that breaks a rule (for example because the rule came later) is
  marked in yellow in the cells the rule is about, with the rule in the
  tooltip.
- When no adjustment can repair a choice (a rule that forbids a
  combination, for example), the choice is refused, and the message names
  the rule.

The shelf is done: two options, a row and a rule.

### More in the Configurations window

Every change in the window is one step to undo. A change that MitCad
refuses stays in its cell in red, and the cell's tooltip and the line
under the tabs say why. You cannot change the configuration while a
sketch or a command panel is open. The **Bindings** tab lists what follows
the options ([Values that follow parameters](#values-that-follow-parameters)).

#### Options

- **Add Option…** makes a user parameter an option, or makes a new one
  with **New Parameter** (its name, unit and value; unit `Text` for a
  text such as a color name). **Remove Option** takes the option out of
  the configuration and out of the rows; the parameter stays.
- **Possible Values** are the values the option can have before rules
  and rows restrict them: a **list** of values (`Black`, `White`;
  `300 mm`, `500 mm`, `800 mm`), a **range** (a minimum, a maximum and a
  step), a **table column**, or **free** (no limits of its own). A text
  option is a list, a table column or free. When rules restrict the
  option now, the cell's tooltip shows its allowed values and the rules.
- A **table column** takes the option's values from a column of one of
  the design's tables ([Tables](#tables)): choose **Table Column** and the
  column, such as `Screws.Size`. The Possible Values cell then shows the
  column with its values (`Screws.Size: M3, M4, M5`). A text option takes
  a text column, a number option a number column. When you edit the
  table (or import it from CSV) and the option's value is no longer in
  the column, MitCad adjusts the option to the nearest value (for a text,
  to the default row's value or the column's first), and the status bar
  says so.
- **Value** is the option's current value: it changes with every choice
  in the Configuration bar, a row you apply and an automatic adjustment.
  Double-click it to type another value; MitCad checks it against the
  rules as a choice in the bar. **Comment** is the parameter's comment.
- **By Formula** marks an option whose value is calculated from its
  expression, a formula of other parameters. Its Value is the calculated
  value, with the expression in the cell's tooltip; you change the
  expression in Edit Parameters, not here. No chooser sets such an
  option, and MitCad never adjusts it; the Configuration bar shows its
  value only.

#### Rows

- **Generate Rows from Table…** adds a row for each value of a table's key
  column, named by the value, that sets an option to it: choose the table,
  its **Key column** and the **Option** (an option, or a user parameter
  that becomes one), and click **Generate**. MitCad offers first the
  column that an option's values come from, and that option. Under
  **Columns** the table's other columns are checked: each becomes a
  parameter column of the rows (see [Only the listed
  rows](#only-the-listed-rows)); uncheck them all to set the key only.
  Running it again after the table changed updates the rows of the same
  name instead of adding them twice, and keeps what you typed in them. It
  is one step to undo.
- **Import CSV…** fills the rows from a CSV file and **Export CSV…** writes
  them to one (see [Only the listed rows](#only-the-listed-rows)).
- **Copy and paste.** Select a block of cells (drag, or Shift+click) and
  press Ctrl+C to copy it as a spreadsheet does; click a cell and press
  Ctrl+V to paste a block copied here or in a spreadsheet, with its top
  left cell there. Rows are added below the last when the block needs
  them. **Delete** empties the selected cells (a row's name stays). A paste
  or a Delete is one step to undo; when MitCad refuses it, the pasted
  cells stay in red with the reason.
- **Delete Row** deletes the selected row. **Apply** sets the options to
  the selected row's values; when MitCad adjusts other options to the
  rules, the line under the tabs and the status bar say so.
- **Only these rows** allows only the combinations listed in the Rows tab
  (see [Only the listed rows](#only-the-listed-rows)).

#### Rules

The list on the left has each rule's name, a check that turns it on or
off, and a warning mark with the reason in its tooltip. **Duplicate**
copies the selected rule and **Delete** removes it. Double-click a name,
or press F2, to rename the rule.

**Add** opens a menu of ready-made rules. Each one but the empty rule is
complete at once, filled in from your options, and saved, and its name
says what it does; you then change its values. For the shelf:

- **Limit a range when…**: `when Color == 'Black'` / `then Length max
  2000 mm`, named *Limit Length when Color is Black*. It needs an option
  that is a number.
- **Allow only some values when…**: `when Color == 'Black'` / `then Length
  only 300 mm`.
- **Forbid a combination…**: `when and(Color == 'Black'; Length == 300 mm)`
  / `then forbid: Black with 300 mm is not made`.
- **Only the combinations of a table…**: `then combination in table Made
  (Color = Color, Length = Length)`. It needs a table in the design.
- **Empty rule**: a rule with nothing in it, to write yourself.

Type the new rule's name at once, or press Esc to keep the one it got.

**The text** under the blocks is the quickest way to write a rule (Alt+T
goes to it). It has a line per part: `when` and a condition, or `always`
(or no first line) for a rule that always applies; a `then` line for each
action; and `else` lines for the actions when the condition does not
hold. A condition compares options and other parameters: `Color ==
'Black'`, `Length > 1000 mm`, `and(Color == 'Black'; Length > 1000 mm)`,
`or(…)`, `not(…)`. The actions are, for example:

| Line | What it does |
|---|---|
| `then Length max 1000 mm` | at most 1000 mm (also `min` and `step`) |
| `then Color only 'Black'; 'White'` | allows only the values listed (also written `allow only`) |
| `then Color not 'Red'` | allows all but the values listed (also written `exclude`) |
| `then require Depth >= Length / 4: the shelf is too shallow` | the combination must meet the condition, else the message shows |
| `then forbid: Red is not made in XL` | the combination is refused with the message |
| `then combination in table Made (Color = Color, Length = Length)` | the options must be a row of the table Made |

Write a text in quotes (`'Black'`) and a length or an angle with its unit
(`1000 mm`). **Enter** saves the rule, **Shift+Enter** starts a new line,
**Esc** brings back the saved text, and leaving the text saves it too. A
line that cannot be read is marked, the line under the blocks says where
and why, and nothing is saved until you correct it. An empty rule's text
shows an example made from your options.

**The blocks** show the same rule, built from a palette of four groups.
**Rule**: *when … then* gives the rule a condition, *then (always)* takes
it away, and *else* adds actions for when the condition does not hold.
**Logic**: *and*, *or*, *not*, *compare* (=, ≠, <, ≤, >, ≥) and *is one
of*. **Values**: an *option* or another *parameter*, a *number with unit*,
a *text*, a *table column* and a *formula*. **Actions**: *allow only*,
*exclude*, *min*, *max*, *step*, *require*, *forbid* and *combination in
table*. Hover over a block to see what it does, with an example, such as
*max*: "Limits an option's largest value. Example: Length max 1000 mm."
An action comes with its values filled in from your options (*max* with
the option's largest value, *allow only* with its first value), so the
rule is complete and saved at once; change the value afterwards.

- Drag a block from the palette onto the rule. The place where it would
  go lights up on the way: a value or logic block fits only an empty
  place (or replaces a value), an action only between or after the
  actions of *then* or *else*. Drag a block within the rule to move it, or
  onto the palette to delete it.
- A new name, number or text block asks for its value at once; Enter
  confirms it and Esc cancels. Double-click a block (or click a field,
  such as an action's option or a comparison's operator) to change it. A
  formula block opens the **Formula Editor**; any part of a condition that
  has no block of its own is shown as a formula block, so every rule
  opens and nothing is lost.
- *Combination in table* asks for the table's name (MitCad completes the
  names of the design's tables); each option is then matched to the
  table's column of the same name. Click its columns to choose another
  column for an option. With a table Made of colors and lengths, only
  the combinations it lists are allowed, and each chooser offers only
  what the rows allow with the options before it.
- An edit in the blocks rewrites the text, and the text rewrites the
  blocks.

A rule with an empty (dashed) place, or an *else* without an action, is
not saved yet: the list marks it, and the line under the rule says, for
example, "Rule 1 and Rule 2 are not saved yet: fill their dashed
places." Hover over a dashed place to see what goes there: *Choose an
option*, *Enter a value*, *Add a condition* or *Add an action*. The rule
is saved as soon as it is complete; Ctrl+Z in the tab takes back such
changes one by one.

Rules can also be set with the `set_configurations` command (from an AI
assistant over MCP, or with `mitcad-cli run`).

#### Options in Edit Parameters

In **Edit Parameters**, an option has the configurations icon before
its name, and its tooltip says that it is an option (and whether it is by
formula). When rules restrict an option's allowed values now, the
restricted minimum, maximum and step show in gray in the **Min**, **Max**
and **Step** columns, as a feature's own limits do, and their tooltips
name the rules. A value that you type there for an option goes through the
same rules as a chooser.

#### The Configuration bar

The bar is a panel of its own on the left, between the Projects panel and
the browser, and shows while the design has options or rows. Collapse it
with its arrow; MitCad remembers that. Its first box, **Row**, chooses a
row. Below it is a chooser for each option, in the options' order: a list
for an option with a list of values, a field and a slider for a range, a
field for a free option, and the value only for an option by formula
(its expression in the tooltip). The choosers offer only the allowed
values; when a rule narrows a range, the slider ends at the new limit.

### How MitCad applies rules

**Rules only narrow.** A rule takes values away or refuses a
combination; it never sets a computed parameter. An option's allowed
values are its possible values less what every rule that applies now
takes away, so the order of the rules does not matter, and two rules
cannot fight over a value. Computed values come from expressions (`if`,
lookups in tables).

**The options' order.** When you change an option (or a row, a parameter
that a condition uses, a table, or parameters from a CSV file), MitCad
checks the options in the Options tab's order and, when necessary,
adjusts the later ones to allowed values:

- An option keeps its value while it is still allowed.
- Otherwise MitCad adjusts it: a range to the nearest allowed value in its
  steps, a list of numbers to the nearest listed value, a list of texts
  to the default row's value when that is allowed, else to the first
  allowed value. When only one value is left, the option takes it.
- An option by formula is never adjusted; a rule that it breaks stays
  broken.

This is the options' order, not an order in which the rules run. The
change and its adjustments are one step to undo, and MitCad tells you
what it adjusted and why (*Length 1500 mm → 1000 mm: Black is short*).

**Refusals.** When a rule still fails after the adjustments (a *require*,
a *forbid*, a *combination in table*, or an option by formula), the change
is refused, and the message names the rule and gives its message.
Nothing changes.

**Where rules hold.** Everywhere a value is chosen: the Configuration
bar, the Configurations window's rows, Edit Parameters, a CSV import,
applying a row, inserting a component from a library, the Build tab's
components, and the commands (MCP, `mitcad-cli`). An inserted component
whose option is bound to the parent design's parameters and breaks one of
the component's rules gets an error status that names the rule; the
parent's change is not refused.

#### Only the listed rows

**Only these rows** (in the Rows tab) allows only the combinations listed
in the Rows tab, as a table of sizes does: a screw comes only in the sizes
listed. A choice that is no listed row is then adjusted to one, and the
notice names *Only these rows*. While it is on, the Rules tab says "Only
these rows is on: only the combinations listed in the Rows tab are
allowed, so rules are not needed." With it off (the default), a
combination must still keep to the options' possible values and the
rules, and a row is a saved combination for a quick choice and the bill
of materials.

**Parameter columns.** With Only these rows on, the rows can set other
parameters as well as the options: these are the parameter columns, after
the options' columns in the Rows tab. A parameter column's parameter is
not an option: nobody chooses it, the row sets it. A library screw is an
example: its option Size chooses the row, and the row sets the columns d,
P, s and k (the diameter, the pitch, the width across flats and the head
height), which the screw's features use.

| Name | Size | d | P | s | k |
|---|---|---|---|---|---|
| M4 | M4 | 4 mm | 0.7 mm | 7 mm | 2.8 mm |
| M5 | M5 | 5 mm | 0.8 mm | 8 mm | 3.5 mm |
| M6 | M6 | 6 mm | 1 mm | 10 mm | 4 mm |

**Filling the table from a CSV file or a table.** A table of sizes
usually exists already, in a standard or a spreadsheet. Bring it in whole
instead of typing it cell by cell:

- **Import CSV…** (in the Rows tab) reads a CSV file with a header line,
  as a spreadsheet saves one: `size,d [mm],P [mm],s [mm],k [mm]`, then a
  line per size. The first column is the option that chooses the row (the
  key, Size here); every other column sets the parameter of its name, and
  the columns `Name`, `Designation`, `Part Number` and `Supplier URL` fill
  those fields of the rows. Units in brackets are optional (`5` in an `mm`
  column is 5 mm; without a unit the column takes the unit of its cells),
  a decimal comma is read as a point, and a parameter's cell may be a formula
  (`d * 0.6`). Parameters the design does not have yet are made, with the
  column's unit. A preview shows what each column becomes (the key option,
  a parameter column, a new parameter), the rows that are added and those
  that are updated, and any cell or value that MitCad refuses, which you
  correct in the file first. **Import** fills the rows as one step to undo:
  a row of the same name is updated, a new size is added, and rows the file
  does not have are kept. Only these rows turns on when the file sets
  parameters besides the options. **Export CSV…** writes
  the rows in the same form, so you can edit them in a spreadsheet and
  import them again.
- **Generate Rows from Table…** does the same from a table of the design
  (Edit Parameters, Tables, where **Import CSV…** brings a CSV in as a
  table): choose the table and its key column, and keep the other columns
  checked. Each row then sets those parameters to the values of its size.
- **Copy and paste** a block from a spreadsheet into the Rows sheet: click
  the top left cell and press Ctrl+V.

Then choosing a size on the Configuration bar sets every parameter column
to that size's values, and the design follows.

- **Add Option…** works with Only these rows and rows too: every row
  takes the parameter's current value, which you then change in each row.
  A parameter column chosen there becomes an option with its rows' values.
- **Add Column…** (in the Rows tab) chooses a user parameter that is no
  option, or **New Parameter** (a name, a unit and a value). Every row
  then sets it: each row's cell starts with the parameter's current value
  or expression, and you type a row's own value into its cell as into an
  option's. Only these rows turns on with the first column (without it,
  rows set options only); the window says so.
- **Remove Column** takes out the column of the selected cell (click a
  cell of the column first). The parameter stays, with the value it has.
- Each is one step to undo. **Apply** (or the Configuration bar's row)
  sets the row's options and its columns' values together.

**The other way: lookups.** A table in Edit Parameters with a formula
per parameter gives the same result: the option Size chooses the size, and
`d` is `XLOOKUP(Size; Screws.Size; Screws.d)` ([Tables](#tables)). Generate
Rows from Table… with every other column unchecked then makes a row per
size that sets only Size. Rows with parameter columns keep each size's
values in the Configurations window; a table with lookups keeps them in
one place that formulas share, and Only these rows can then stay off.
Both ways can live side by side in one design.

#### Check Rules

**Check Rules** (in the Rules tab) goes through the combinations of the
options and reports values that are never allowed, rules that never
apply, choices of earlier options that leave a later option no value,
rows that break a rule, and combinations that cannot be repaired. Up to 100 000
combinations are checked one by one; more are sampled, and the report
says so. With changes not saved yet, it says that the saved rules were
checked.

#### The keyboard

In the Rules tab, Alt+R goes to the list, Alt+A opens Add's templates,
Alt+U duplicates the selected rule, Alt+T goes to the rule's text and
Alt+K checks the rules. Alt+P goes to the palette, where the arrow keys or the first letters
choose a block and Enter puts it at the rule's focus (an empty place, or
after the focused action). Alt+B goes to the blocks: the arrow keys move
the focus, Enter edits it, Delete removes it, and the menu key or
Shift+F10 opens its menu (Edit, Edit as Formula…, Move Up, Move Down,
Delete; on the rule itself Add When, Apply Always, Add Else, Remove
Else).

In the Rows tab, Alt+I is Import CSV… and Alt+X Export CSV…. In the
sheet, Shift with the arrow keys or a click selects a block of cells,
Ctrl+C copies it, Ctrl+V pastes at the current cell and Delete empties
the selected cells. In Generate Rows from Table…, Alt+C goes to the
columns' checks, and Space checks or unchecks one.

## Appearances and rendering

An appearance gives a body or a face its color and its surface values.
The shaded view shows only the base color of an appearance. The
rendered view uses all of its values and the texture with all its maps.
This chapter describes the appearance library, the texture sources, the
rendered view, its lights, Render Image and the render device.

### Appearances

The **Modify** group of the Solid tab holds the **Appearance** tool and
**Edit Appearances**. Assigning is the tool's; the **Appearances**
window that Edit Appearances opens manages the library and assigns
nothing.

**Appearance** (also **Appearance…** in the context menu of a body or a
face) opens a panel: click the picker button, whose swatch and name show
the appearance chosen, and choose in the picker that opens: type part of
a name in its search, then click an appearance or press Enter. It lists
**Default** (no appearance of its own: its material's), then under
*Recent* the last five appearances you gave with the tool, the latest
first (MitCad remembers them for you, also after a restart), then the
appearances of the class of the body's material (for a PLA, the FDM
Plastic ones, then the General Plastic ones), then the rest of the
design's own appearances under *This design*, then the rest of the
library under the headings of their classes, in the order *Metal*,
*General Plastic*, *FDM Plastic*, *Resin*, *Wood*, *Other*. Pick the **Bodies** and the **Faces** in
the view or the Browser; what you selected before goes into them. **OK**
gives them the appearance in one Undo step. A face's appearance
overrides its body's; **Default** for a face takes its own away. With
several bodies selected in the Browser (Ctrl+click), **Appearance…** of
one of them is for all of them; on a body that is not selected, it is
for that body only. **fx** beside the picker button makes the appearance
follow a parameter ([Values that follow
parameters](#values-that-follow-parameters)).
**Edit Appearances…**, the last entry of the picker below a line, opens
the Appearances window on the appearance the picker button shows while
the panel stays open: make a new appearance there or change one, close
the window, and the picker offers it, with the bodies and faces you
picked still picked. Reach it with the mouse, or with the keyboard as the
other entries (Down, or type *Edit*, then Enter).

The Appearances window lists the design's own appearances first, under
*This design*, then the library grouped by class under the headings
*Metal*, *General Plastic*, *FDM Plastic*, *Resin*, *Wood* and *Other*,
each with the number of its appearances. Click a heading (or press Space on
it) to collapse or expand its group; MitCad remembers which groups you
collapsed. Typing in the search narrows every group and hides the groups
with no match.

The library's *FDM Plastic* group holds the basic colors of generic
filaments, with the look of a printed thermoplastic (not metallic, the
slightly rough surface of a print, a little gloss, no clear coat):

| Appearance | Color (sRGB) |
|---|---|
| Filament - White | `#F4F4F1` |
| Filament - Black | `#1F2022` |
| Filament - Gray | `#8C8F92` |
| Filament - Silver | `#B4B7BA` |
| Filament - Red | `#C8102E` |
| Filament - Orange | `#F47B20` |
| Filament - Yellow | `#F5D130` |
| Filament - Green | `#2E9B48` |
| Filament - Blue | `#1F58B5` |
| Filament - Light Blue | `#6CB2E2` |
| Filament - Purple | `#6B3FA0` |
| Filament - Pink | `#F28CB6` |
| Filament - Brown | `#7A4A2A` |
| Filament - Beige (natural) | `#E5D4AE` |
| Filament - Translucent (natural, lets light through) | `#EEF0EC` |

The library shows a preview of each appearance. An appearance has these
values:

- Base color
- Metalness
- Roughness
- Specular
- Transmission
- Index of refraction
- Coat
- Emission
- Opacity

and a **Class**: what it is a look of (Metal, General Plastic, FDM
Plastic, Resin, Wood or Other). The pickers offer the appearances of a
material's class first. The library's appearances have theirs (the
metals Metal, the plastics General Plastic, the filament colors FDM
Plastic, the resins Resin, the woods, plywoods and panels Wood); an
appearance you make is Other until you choose its class, and a
filament's color is FDM Plastic. A design whose appearances have a class
does not open in MitCad versions from before the classes.

The library has these buttons:

- **New** copies the selected appearance into the design. Then you can
  change the values of the copy.
- **Delete** removes an appearance of the design; its bodies get the
  default look.

A face can have an appearance of its own. The face appearance overrides
the appearance of the body. It follows the face as other face
references do. The shaded view shows the base color of bodies and
faces. The rendered view uses every value of the appearance and the
texture with all its maps.

**View › Component Colors** gives each component one color of its own
in the shaded and wireframe views: the color of its band in the timeline
and of its section caps (the root component's bodies take the root's cap
color). The color is stable, and all
occurrences of the component share it. It temporarily replaces the
colors of the bodies and faces. Component Colors is a user preference.
It does not change the appearances, the exports or the design. If you
enable it in the rendered view, MitCad leaves the rendered view. If you
enter the rendered view, MitCad sets Component Colors to off.

### Materials

A material tells what a body is made of: its density, which gives the
mass, its appearance, and its print settings for 3D printing (the
filament type, the temperatures, the cooling, and so on). MitCad has
built-in materials: Steel, Stainless Steel, Cast Iron, Aluminum, Copper,
Brass, Titanium, ABS Plastic, PLA Plastic, Nylon, Polycarbonate, Glass,
Water and Resin - Standard, and in the class *Wood* common woods and
panels: Wood - Pine, Wood - Oak, Wood - Birch, Plywood - birch and MDF -
raw, with the values and looks of the bundled material dataset (below).

A material is either built-in or your own. The materials that the
design's bodies use are in bold, with a dot, and the editor says "Used
in this design". You cannot change
the built-in materials: the editor says *Built-in*, with a lock and a
**Duplicate** button beside it. Duplicate one and change the copy.

**Material** (Solid › Modify, and Make › Materials) gives bodies a
material. Its panel has a picker button with the material's swatch and
name: click it and choose in the picker, which lists **Default** (no
material of its own), the design's materials and those of the libraries
(built-in, My library, the project's), with a search. A material of a
library is copied into the design when you press OK. **Appearance** is
the material's default appearance; when the material offers several,
choose one of them there, as in **Material › PLA Basic › Black** of the
context menu. Pick the **Bodies** in the view or the Browser, or
components for their default material; what you selected before goes
into them, and the line under them says what they are made of now, for
example *3 bodies: PLA Basic, 1 body: PETG*. **OK** sets the material in
one Undo step. When a picked body has an appearance you set yourself
that the material does not offer, **Keep appearances set by hand**
appears (and names the bodies): on, they keep it; off, they get the
material's appearance. One the material offers stays the body's. **fx** beside the
picker button makes the material follow a parameter ([Values that
follow parameters](#values-that-follow-parameters)).
**Material Library…** under the picker button opens the Materials
manager on My library while the panel stays open: make or change a
material there, close the manager, and the picker offers it, with the
bodies you picked still picked. **Edit Materials…**, the last entry of
the picker below a line, does the same on the material the picker button
shows: the Materials manager opens on it, and once you close the manager
the picker offers what you made or changed there.

**Edit Materials** (Make › Materials) opens the **Materials** manager,
which manages the materials and assigns none. Its list groups the
materials by where they live: **This design**, **This project** (in a
project), **My library** and **Built-in**, each with its count (refer to
[Where print items live](#where-print-items-live)). In each group the
materials are listed by their class: *Metals*, *General Plastics*
(molded, cast, machined or sheet plastic), *FDM Plastics* (the
filaments), *Resins* (for resin printers), *Wood* (solid wood and
wood-based panels) and *Other*. Type in the
search above the list to find materials by name in all the groups. The
list is as
wide as the longest name needs; drag the divider to make it narrower,
and a long name then ends in "…" (point at it to see it in full).
Select a material to see it on the right. The **General** tab has its
**Name**, its **Class** (Metal, General Plastic, FDM Plastic, Resin,
Wood or Other: the group it is listed in; a material with print settings is an
FDM plastic, and its Class shows so and cannot change), **Density** (g/cm³) and **Appearances** (see
[A filament's color is its appearance](#a-filaments-color-is-its-appearance)).
An FDM plastic also has its print
settings in the tabs beside General (*Filament*, *Cooling*, …); a
metal, a general plastic, a resin, a wood or another material has none. Type in **Search settings** to find
a setting or a field of General, or turn on **Advanced** to see all of
the print settings. A setting that is
not set is an empty field that reads *Slicer default* in gray: type a
value in it. There is no filament color among them: the color is the
appearance's. **↺** clears a
value. A value that does not fit says what to use under its field, for
example "Use a value of 0 or more". The fields take what the slicers
take for that setting, and the design's slicer decides where slicers
differ: Bambu Studio takes -1 ("automatic") in a few settings that
OrcaSlicer does not.

**Slicer presets.** On the *Filament* tab, **Slicer presets** has a row
for each slicer (Bambu Studio, OrcaSlicer, QIDI Studio, PrusaSlicer):
the name of that slicer's filament preset the material is based on, for
example "Generic PETG HF". Type the name, or open the list to choose one
of the filament presets of the slicer installed on your computer (Bambu
Studio, OrcaSlicer and QIDI Studio) or a name another material of the
design gives. In
the 3MF of Send to Slicer the filament then has that preset's name, and Bambu
Studio, OrcaSlicer and QIDI Studio start from that preset, with the
material's own values on top of it. Without a preset, the slicer gets
only the values that MitCad has, and fills in the rest with its
defaults. Bambu Studio,
OrcaSlicer and QIDI Studio share their names: a row that is empty shows
in gray the name that one of the others gives, which is then used.

Each value is saved when you press Enter or leave the field. There is no
OK button. Undo takes back the change, also in your library.

**New**, **Duplicate** and **Delete** are under the list. New asks the
class first and starts a metal at steel's density, a plastic at PLA's
and a resin at that of the built-in *Resin - Standard*; for a wood or
another material it asks for the density. A new wood looks like oak
until you choose its appearance. A design with a material of the class
FDM Plastic without print settings, or of the class Resin or Wood, does
not open in MitCad versions from before these classes. Duplicate makes
your own copy of a material, with its class, named after it (for
example "PLA Plastic (copy)", then "PLA Plastic (copy 2)"); New names
it "Material 1". New and Duplicate ask where to make the material: This
design (first when a design is open), This project or My library. It is
made there and selected; the Material tool gives it to bodies.

A material is edited where it lives: the line above its settings says
where, for example "Editing: My library — every design of yours can use
it". **Copy to ▾** beside the name copies it to another place (refer to
[Where print items live](#where-print-items-live)). **Delete** always
asks first, and says who uses the material and what they use then, for
example "Delete Shop PETG? Body1 and Body2 then use Default (Steel)."
For a material in your library, Delete removes it from there ("Delete
Shop PETG from My library? This design keeps using it."): the design
keeps its copy as its own.

When you changed a material of your library in another design, MitCad
asks once, when this design uses its copy or you edit the copy: "Shop PETG
was changed in another design. Use the changes or keep this design's
settings?" **Use the changes** takes them into this design; **Keep
this design's settings** keeps them, and MitCad does not ask again
until the material changes again.

**Material** in the context menu of a body (in the Browser or in the
view) picks its material without a window. The submenu lists the
materials of the design, each with its color, and checks the one the
body is made of; below a line come the materials of your libraries (and
the built-in ones) that you assigned last, up to eight, which the design
does not have yet. Pick one: the body gets it, with its appearance, in
one Undo step. A material from a library is copied into the design as it
is assigned. With several bodies selected (in the Browser with
Ctrl+click, or in the view) the pick is for all of them, and the check
mark shows only when they are all made of the same material. If you set
the appearance of a body yourself and the new material does not offer
that appearance, MitCad asks first (when it offers it, the body keeps it
without a question):

- **Replace** gives the body the appearance of the material.
- **Keep** keeps the appearance that you set. The body gets only the
  material.
- **Cancel** changes nothing.

**Material › More…** opens the Material tool with the body picked. When
you selected several bodies (in the Browser with Ctrl+click) and open
the menu on one of them, the tool is for all the selected bodies; on a
body that is not selected, the menu is for that body only. **Default**
in the picker removes the body's own material: the body then uses the
material of its component, as the entry says, for example **Default
(Steel)**.

**Material** in the context menu of a component sets the default
material of the component in the same way: pick a material of the
submenu, or choose **More…** and **Default** in the Material tool's
picker to remove it. The bodies of the
component that have no material of their own are made of it. A body in
a subcomponent uses the nearest component above it that has a default
material. Without any, a body is steel.

#### Mechanical and thermal properties

Under Appearances, the **General** tab of the Materials manager has the
material's properties in three groups:

- **Mechanical:** Young's modulus, Poisson's ratio and yield strength.
- **Thermal:** maximum service temperature and coefficient of thermal
  expansion.
- **Printing** (FDM plastics only): the extruder temperature and the bed
  temperature. These are the same values as the *Nozzle temperature* and
  *Bed temperature* of the Filament tab: change them in either place.

A value that is not given is an empty field that reads *Not given*; it
is never taken as 0. Type a value and press Enter or leave the field to
save it; empty the field to take the value away. Under the fields the
manager says where the values came from, when it knows, and reminds you
that the values are typical: check them against your supplier's data.
MitCad does not compute anything with these values yet. They are shown
on the material, in **Physical Properties** (the values of each body's
material), in a bill of materials (type a property's name, for example
*Young's modulus*, in a Bill of Materials table's **More Columns**) and
they go with the material into a design, a library and the files below.

**Units.** The values are shown in the design's units: in a metric
design the density is in g/cm³, the modulus and the strength in MPa and
the expansion coefficient in µm/(m·°C); in an inch design (Change
Active Units › in) they are in lb/in³, psi and µin/(in·°F).
Temperatures are in °C or °F as you choose in **Preferences › General ›
Temperature unit**, whatever the design's units.

**Browse Material Dataset…** (the last entry of **New ▾**) starts a
material from MitCad's bundled material dataset: typical values of
common steels, stainless steels, cast irons, aluminum, copper and nickel
alloys, titanium and printing materials (PLA, ABS, PETG, ASA, nylon,
polycarbonate, HIPS), and in the class *Wood* ten solid woods (birch,
cedar, cherry, Douglas fir, eucalyptus, maple, oak, pine, teak, walnut)
and seven panels (chipboard, MDF, OSB/3, birch, eucalyptus, pine and
spruce plywood), each with a look of its own. It works offline. Type in
its search to find a material and grade, for example *6061 T6*, select
it to see its values in the design's units, and click **Create**; then
choose where to make it. To make several at once, pick them with Ctrl
and Shift, or click **Select All** to pick every material the search
shows (for example all the plywoods after typing *plywood*): Create
makes one material of each, as one Undo step. A material whose name is
there already (a built-in wood, or one you made before) is skipped, and
the line under the list says how many were made and how many were
skipped. The new material is your own: its values are filled in and you
can change them. It remembers the dataset's material and grade as its
source, and a newer MitCad with an updated dataset never changes it.
The dataset's values come from the material-properties dataset, under
the Apache License 2.0 (refer to the About box and the third-party
notices); those of the woods and panels are MitCad's own, each from the
public source it names as its source (a wood database, OEKOBAUDAT and the
manufacturers' data sheets; refer to [Materials as JSON and CSV
files](material-files.md#the-woods-and-panels)). A wood has its density
and Young's modulus, a panel its density only.

**Import…** and **Export…** under the list read and write materials as
JSON or CSV files (refer to [Materials as JSON and CSV
files](material-files.md) for the formats and an example). Export writes
the materials of the selected material's place (with none selected, the
design's), in the design's units and your temperature unit; the file
says its units. Import asks where to put the materials, as New does.
When the file has a material whose name is there already, MitCad asks
once for the whole file:

- **Keep Existing** keeps the material that is there and skips the
  file's.
- **Replace Values** gives the material that is there the file's
  values; the values the file does not give stay.
- **Keep Both** adds the file's material with a number, for example
  "Steel (2)".

A material of the file that is not valid (a value that is not a number
or is out of its range, a missing name, a new material without a
density) is not imported: the line under the list says which row and
why, for example "Row 4 (Broken) not imported: Yield strength must be
greater than zero, got -3". The others are imported, in one Undo step.

#### One filament, its variants per printer and nozzle

A filament is one material, also when it prints differently on your
printers. Its settings for one printer and nozzle are a **variant** of
the material: the variant holds only the values that differ from the
material's own. You give a body the filament, and the printer you print
with picks the values: the variant of that printer and its nozzle, else
a variant of that printer for any nozzle, else the material's own
values. Send to Slicer's **Materials** row says which variant is used, for
example "Shop PLA for Bench X, 0.4 mm nozzle". A filament whose colors
need different settings (a black and a white that print at different
temperatures) is a material per color, each with its appearance.

A plastic's **Variants** tab, after its print settings, lists its
variants: the printer and nozzle and the values that differ ("Nozzle
temperature 220 °C, Bed temperature 55 °C"). **Add Variant ▾** lists
your printer profiles; choose one to add the variant of its printer and
nozzle. Select a variant to change its values in the fields under the
list: an empty field shows the material's own value in gray, a value you
type is the variant's, and **↺** gives the field back to the material.
**Remove Variant** removes the selected one. The nozzle of a printer is
its **Nozzle diameter** in the Printers manager.

**Import from Slicer** brings a filament's presets in as one material:
"Shop PLA @Bench X" and "Shop PLA @Bench Y 0.6 nozzle" become Shop PLA
with a variant for each printer and nozzle. In **Import Presets** such a
filament is one row with a row per printer and nozzle under it. When the
presets are for several printers, **Printers and nozzles in use** at the
top lists them: untick the ones you do not use, and their variants are
not imported. If your library or design still has a filament's presets
as separate materials from an earlier import, **Merge the separate
materials of 1 filament** under the list makes them one material with
variants; the bodies that used them use the merged one.

#### A filament's color is its appearance

A filament gets its color from its appearance: filaments are matte,
glossy or translucent, so the appearance is the color. A material offers
one or more appearances, for example PLA Basic in Bright Green, Black,
White and Translucent Blue, one of them its default. A body has a
material and one of its appearances; a body you give the material gets
the default.

On the **General** tab, **Appearances** lists every appearance the
material offers, each with its swatch and name; the default says
**Default** in its row. **Add…** beside the list opens the appearance
library: type in **Search appearances**, then click an appearance (or
press Enter for the first match) to add it. The appearances of the
material's class come first, under its name (*FDM Plastic* for a
filament, the design's and MitCad's together); for an FDM plastic or a
resin then the *General Plastic* ones; then the design's other
appearances, under *This design*, and MitCad's, under the headings of
their classes (*Metal*, *General Plastic*, *FDM Plastic*, *Resin*,
*Wood*, *Other*); a search hides the headings with no match; **Edit
Appearances…** at the end opens the Appearances window. Select a row and
click **Make Default** to make it the default, or **Remove** to take it
out of the list (or right-click the row for the same two). When you
remove the default, the next one becomes the default.

In the context menu, a material that offers two or more appearances
opens a submenu of them: **Material › PLA Basic › Black** gives the body
PLA Basic in black, in one Undo step, and checks the appearance the body
has. On a component, it sets the component's default material and the
appearance its bodies print with. When you give a body an appearance
that its material does not offer, with **Appearance…** or this way, the
design's material offers it from then on. The built-in materials cannot
change: their list stays as it is.

A material saved to **My library** or a project's library keeps its
whole list of appearances, also those you made in this design. Another
design that uses it gets the same appearances, the default first. If
that design already has an appearance of its own that the material's
would replace, it keeps its own, and the material's comes in beside it.

Send to Slicer sends the color of the appearance each body prints with: its
own, else that of its component's default material, else the material's
default. Each material and appearance is one filament in the file: two
bodies of PLA Basic in two colors are two filaments with the same
settings and different colors, as the slicer's color map and an
automatic material system use them. Designs from before, whose
materials had a filament color of their own, open with that color as an
appearance of the material's list. The purge table names such
filaments by their colors, for example "PLA Basic (Black)", and works
out the volume between two colors of one material from the colors.

### Print settings

A design can carry its print settings: a **printer** (its bed, its
printable height, its nozzle), a **process** (layer height, walls,
infill, speeds, supports and the other settings of a slicer) and, for
each body, the print settings of its material. Send to Slicer gives them to
the slicer in the 3MF file, so that the slicer opens the bodies with the
right printer, process and filaments.

**Choose a printer and a process.** In **Send to Slicer**, set **Format** to
3MF (the first time, 3MF is chosen; with STL the rows are grayed out:
STL files carry no settings).
The **Printer** and **Process** rows show the design's printer and
process, by the model and the nozzle (for example "X1 Carbon · 0.4 mm
nozzle"; hover over the box for the whole name). Click the box to open
the picker: type in its search field to narrow the list (by any part of
the name, the vendor or the nozzle), then press Return to choose the
first match, or click a row; Down moves into the list, Escape or a click
outside closes it. The printers are grouped by vendor ("Bambu Lab",
"QIDI", …) and the processes by printer (as in the Processes manager:
refer to [Processes by printer](#print-settings)): the processes of the
chosen printer first, then the others under their printers, those for
any printer last under **Any printer** (the Process drop-downs of the
Make tab's body card and of Property Sets list them the same way, the
body's or the level's printer first). Each row of the picker says on
the right where it comes from: **This design**, **This
project**, **My library**, **Built-in** (the groups of the Printers and
Processes managers), or the 3MF file it was imported from, so two
printers of the same name can be told apart. The current choice is
checked. A choice is set at once, as one Undo step. **Slicer default**,
always at the top, takes the design's printer or process away: the
slicer then uses its own. **New printer…** or **New process…**, at the
bottom below a line, makes a new one in this design (named Printer 1 or Process 1, then
Printer 2, …), sets it for the design and opens it to edit (Copy to
puts it in a library). **Edit printers…** (in the Printer picker) or
**Edit processes…** (in the Process picker), the last row, opens the
Printers or the Processes manager on the current choice, without
changing it. The box is the row's only control: there is no button
beside it.

**Choose the materials.** The **Material** row shows what the bodies you
send print with, and is where you choose it. When the bodies have
different materials, there is a row for each, one per filament of the
3MF file: **Filament 1**, **Filament 2**, …, each with how many bodies
print with it (for example "Filament 1: PETG (2 bodies)", "Filament 2:
TPU (1 body)"). A body that has its component's material shows that
material by name. Bodies without a material of their own or of their
components show **No material: choose one**; hover over it to see what
the slicer gets until you choose (the design's default material, such as
Steel). Click a row to open the material picker: type in its search
field to narrow the list, then press Return to choose the first match,
or click a material. The filaments (FDM plastics) come first, then the
other materials, each with its color swatch and grouped by where it
lives: **This design**, the filaments imported from a slicer's 3MF,
**This project**, **My library**, **Built-in**; the row's material is
marked *Current*. The material you choose is given at once to the bodies
of that row only, as one Undo step, as the Material tool gives it (a
material from a library is copied into the design). **Edit materials…**,
the last entry of the picker, opens the Materials window on the row's
material, to edit it. A warning under the rows names the chosen materials that name no
filament preset of the slicer the file is for, for example "Shop PETG
HF names no Bambu Studio filament preset": give them one on the
*Filament* tab of Materials (**Slicer presets**, see
[Materials](#materials)). After **Send**, the message line names each
filament that the design leaves without a value another filament has
(for example "Steel (filament 2) sets no nozzle temperature: written as
generic PLA's"): the slicer takes every filament's values from one
list, so such a filament gets MitCad's generic values of its type, and
the values you set reach their own filament.

**Edit printers and processes.** In **Send to Slicer**, open the
Printer or Process picker and choose its last row, **Edit printers…** or
**Edit processes…**, or use **Printers…** or **Processes…** (Make › 3D
Print). The **Printers** and the **Processes** managers are two windows,
which work as the Materials manager: the list on the left, grouped by
where the items live (**This design**, **This project** in a project,
**My library**, **Built-in**, each with its count, refer to [Where print
items live](#where-print-items-live)), with a search above it; the
settings on the right (a printer's on one page; a process's in the tabs
*Quality*, *Strength*, *Speed*, *Support* and *Others*), and **New**,
**Duplicate** and **Delete** under the list. The picker's last row opens
the design's copy, which it prints with. The design's printer and
process are in bold, with a dot; **Use in this design** makes the
selected one the design's (an item of a library is copied in); for one
the design uses, the editor says "Used in this design" instead. New and
Duplicate ask where to make the item, This design first; one made in
this design is set for the design at once, as one Undo step. Built-in
ones cannot be changed: Duplicate one. **Copy to**, **Delete** and a
change in another design work as for materials.

**Processes by printer.** In the Processes manager, the processes of
each place are grouped by printer: a group per printer ("Bench X",
"Bench Y", …) by name, then **Any printer**, each one level
below its place and folded or opened as the places are. A process is
for the printers its slicer says it is compatible with (when it was
imported from a slicer, also those of the preset it inherits from),
else for the printer its name names after an `@` ("0.20mm Standard
@Bench X" is for Bench X), else for any printer. A process for several
printers is listed under each of them; the place's count counts it
once. The search finds processes in every group, and the group of the
selected process is open. Deleting the design's
own printer or process asks first, and the design then has none: the
slicer uses its own. **Import from Slicer…** at the top works as Import
from slicer… in Send to Slicer (below), into the place **Import into**
beside it shows (This design, the default with a design open, This
project or My library): into the design, the printer and the process
read come in and the design uses them; the list shows the item of the
manager's kind selected; **Details…** beside the line shows the whole
report, and one Undo takes an import into the design back.

**Import from a slicer.** **Import from Slicer…**, the first print row
of Send to Slicer, above Printer and Process, reads a
slicer's settings: a 3MF project of Bambu Studio, OrcaSlicer, QIDI
Studio, PrusaSlicer or UltiMaker Cura, a preset or an exported bundle of
Bambu Studio, OrcaSlicer or QIDI Studio, or a configuration or config
bundle of PrusaSlicer. MitCad reads the settings only, never the
geometry. The printer and the process come into the design, and the
design uses them. A material for each filament is added to Materials,
with an appearance in the filament's color (a neutral light gray when
the file names no color). A preset that builds on one of the slicer's own
presets comes in with the values it takes from them, when that slicer is
installed or the preset is in its folder. A
short line under the button says so, for example "plate.3mf imported",
or "plate.3mf imported. 1 setting not read"; until
you close Send to Slicer, click it for the whole report. The new
materials are in the material rows' pickers; under the rows,
**Assign…** also offers them: it closes Send to Slicer (what it imported stays) and opens the
Material tool with the bodies you print, to give the new materials to
them.

With **Open in** set to Folder only, a 3MF still carries the printer, the
process and the materials, in the form of the slicer of the design's
printer (Bambu Studio's when it has none).

**Settings of components and bodies.** A body, an occurrence or a
component can have process settings of its own, as an object has in a
slicer. **Property Sets…** sums up under **Print settings** what the
selected one prints with: the printer, the process and the material,
and where a value comes from (for a value of its own, its name, such as
"Body1"). The pencil beside Process opens the process, the one beside
Printer the printer and the one beside Material (or a click on the
material) the Materials window on that material, to edit it; to give the
body another material, use the Material tool; a
long name is shortened so its pencil stays in sight, and the whole name
shows as a tooltip. Click the name of the printer or the process itself
to choose another from a drop-down; Escape or a click elsewhere closes
it without a change. Below the root
component, the process shows its own values only, and **Own settings**
names the values of its own, for example "Walls", with them listed
under it and who sets each; its pencil opens them in the same tabs as a process, in a
window such as "Body1 · Own settings": set a value there, or clear it with
**↺**. The root has no own settings: its values are the process. A
design prints on one printer: below the root component it shows as one
line. Values that are not set show *Slicer default* in gray italics.
Send to Slicer shows the own settings of bodies and components in its **Own
settings** row, for example "Body1: Walls", or "None"; its pencil opens the own settings of the body you print, or, with
several, of the one you choose from their names. The
process editor shows them beside each setting, for example "4 on Body1".
Where the occurrences of a component print a body otherwise (one of them
has walls or a material of its own, for example), the 3MF has a part for
each group of occurrences that print alike, named after the body and
the occurrences, such as "Body1 (Box:2)", each with its own settings
and filament; a body that prints alike everywhere stays one part.

**Purge volumes.** When the printer changes from one filament to
another, it purges some of the new filament to clear the old one out of
the nozzle. The **Purge volumes** row of Send to Slicer, under the material rows, says
"Computed", or how many volumes you set ("1 set by hand"); its pencil
opens the table of the filaments you print: a row for each filament it
changes from, a column for each filament it changes to, the volumes in
mm³. A volume is computed from the two materials: from a dark color to
a light one takes more than from a light one to a dark one, and the
**Purge multiplier** of the process (*Others*) multiplies it. Type a
volume in a cell to set it yourself: the cell is then in bold, and the
design keeps it for those two materials. **Reset** gives the selected
cells the computed volume again. Each change is one Undo step.

A material's own values are on its *Filament* tab, under **Purge
volumes**: **Purge into this filament** and **Purge out of this
filament**. Unset, they show the values of the material's color in
gray.

The 3MF gives the table to Bambu Studio, OrcaSlicer, QIDI Studio and
PrusaSlicer; UltiMaker Cura has no such table. Importing a slicer's
project brings its table in as volumes set by hand.

**Filament changes.** On a single extruder you can change to another
filament, or pause (for magnets, nuts or inserts), at a height of the
print. The **Filament changes** section of Send to Slicer, under Purge
volumes, has a row for each change: where it is, what it changes to,
and its **Edit** and **Remove** buttons. **Add change** and **Add
pause** open the **Filament Change** window: type a height above the
print bed in **At** (12, or 12 mm), choose the filament in **Change
to**, or Pause with its **Message**, and press OK. A change can also sit
at a construction plane or a flat face: click **Change At** and then the
plane or face in the view, or right-click a plane or a flat face and
choose **Filament Change Here…** (on a face of a body that is on a print
plate, it opens **Plates…** on that plate instead, where the plate's own
changes are). **Plates…** has the same section for the plate you choose:
until you edit it there, a plate prints with the design's changes. Point
at a row to see its plane or face in the view. Such a change follows the model: a
change at the top face of a base stays there when the base gets
thicker, and one at a construction plane moves with the plane. Its row
shows the plane's or face's name and the height it gives. A plane or
face that is not level on the print bed (after the bodies were turned
onto another face, for example) gives no height: the row says so, and
the change is left out of the file rather than moved. While the panel
is open the view shows the bodies in the colors they will print in, with
a thin ring at each change; closing it restores their normal look. Each
edit is one Undo step, and the design keeps the changes. The 3MF carries
them for PrusaSlicer, Bambu Studio, OrcaSlicer and QIDI Studio; for
UltiMaker Cura the warning names each layer to add them there.

**Show Build Volume** (the View menu) draws the bed of the design's
printer, with its middle at the origin and its surface at Z = 0, and its
printable height. Bodies that do not fit are marked with a dashed box.

STL files carry no settings.

### Where print items live

Materials, printers and processes live in four places, and the
Materials, Printers and Processes managers list them under these
groups:

| Group | Where it is kept | Who sees it |
|---|---|---|
| **This design** | the design's file | this design |
| **This project** | the project's folder (`print-library`) | every design of the project and everyone who works on it |
| **My library** | your library on this computer | every design of yours |
| **Built-in** | MitCad | everyone; read only |

Each group is a heading with its count, for example *My library (3)*. The
search above the list finds items by name in all the groups. The pickers
of Send to Slicer and of the Material tool name the same places.

**Edited where it lives.** Select an item to edit it in its own place;
the line above its settings says where and whom a change reaches, for
example "Editing: this project's library — every design of the project
uses it". A design keeps a copy of each library item it uses: the copy
is listed under **This design**, and its library item under its own
group. A change to the library item reaches this design's copy too; a
change to the copy changes only this design ("Editing: this design only
— My library's Shop PETG stays as it is"). Another design with a copy is
asked once, as for a change in another design: **Use the changes** or
**Keep this design's settings**. Built-in items are read only: select
one and click **Duplicate**.

**Hiding built-ins you do not use.** The built-in printers, processes
and library materials (the filaments) cannot be deleted, but they can be
hidden: select one and click **Hide** beside Delete, or right-click it
and choose **Hide**. A hidden item is no longer listed in the managers
or offered in any picker (the printer and process of Send to Slicer,
the Make tab and Property Sets, the Material tool and the Material
menus). It
is hidden on this computer only, for all your designs; designs and
projects do not change. A design that already uses a hidden item keeps
it, and lists it as its own, under **This design**. To bring one back,
tick **Show hidden** under the list (it shows while something is
hidden): the hidden items come back grayed; select one and click
**Unhide** (or right-click it and choose **Unhide**). MitCad's own
materials such as Steel cannot be hidden, and your own and the project's
items keep **Delete** instead.

**Your own first.** Once My library or This project has a printer of
yours, the Printers manager opens with the **Built-in** group folded
(click its arrow to open it; a search finds in it too), and the printer
pickers list your printers first and the built-in ones last, under a
**Built-in** heading. The Processes manager and the process pickers do
the same once you have a process of your own.

**New, Duplicate and Copy to.** New and Duplicate ask where to make the
item, the default first: **This design** when a design is open, else
**My library**. **Copy to ▾** beside the name copies the selected item
to another place. A copy keeps a link to where it came from only as
before: an item of a library copied into the design is the design's copy
of it, and an item of the design copied into a library becomes that
library's item, with the design's item as its copy. An item copied from
one library into the other keeps its name and no link.

**Your own library.** **Tools › Libraries › Material Library…**,
**Printer Library…** and **Process Library…** open the same managers on
**My library**: its group at the top of the list with its first item
selected, and My library the place New, Duplicate and Import offer
first. They work with or without a design open. With no design open
(the empty *Untitled* window MitCad starts with), the managers list **My
library** (and **This project**, when a project is open) and
**Built-in**, and work on them alone: the window's design does not
change. The Make tab's Edit Materials…, Printers… and Processes… open
the same managers with This design first. Type "My library" in the
command search to find them all.

**Import from Slicer…** at the top of each manager reads a slicer's 3MF
project or preset, as Import from Slicer… in Send to Slicer does, into the
place **Import into** beside it shows (This design with a design open,
else My library; also This project in a project): its printer, its
process and a material for each filament. A short line beside the
button says what came in; **Details…** shows the whole report.

**Presets of an installed slicer.** When Bambu Studio, OrcaSlicer or
QIDI Studio is installed, the button is **Import from Slicer ▾**: choose
**From a file…**, or **Presets of Bambu Studio…** to take the printers,
processes and filaments you made in that slicer, without exporting them
first. The **Import Presets** window lists them under **My printers**,
**My processes** and **My filaments**; tick the ones to import (a
group's box ticks all of it or none). Below them come the slicer's own
presets that you enabled in it, the printers chosen in its setup with
their processes and the filaments chosen there, under **Bambu Studio
printers**, **Bambu Studio processes** and **Bambu Studio filaments**;
these groups are closed at first (open one with its arrow or the Right
key) and their presets are not ticked, since your own presets usually
build on them already. A slicer you have set up but made no presets in
yet is offered too, with its own presets only. If several slicers are
installed, choose one at the top. Type in **Filter presets** above the
list to show only the presets whose name contains the text, for example
"X1" for the processes of that printer; presets you ticked stay ticked
while they are hidden and are still imported, and clearing the field
shows the groups as they were. A preset whose name the library already
has is shown in another color with **Same name:** beside it; point at
it to see what it meets, for example "My library has a process of this
name". These are not ticked at first, and a closed group with one of
them opens. Beside each one, choose **Copy** to import it as a new item
("Fine 2"), or **Override** to update the existing item; choosing one
also ticks the preset. **All same names** above the list sets the
choice of them all; it is empty while they differ. With the keyboard,
the arrow keys move between the presets, Space ticks or unticks one,
Tab reaches its Copy or Override, and Shift+Tab from the list reaches
the filter. The button says how many presets are ticked, for example
**Import 3 Presets**; it brings them in with the
values they take from the slicer's own presets filled in (the
temperatures and the color as much as the rest). Each filament comes in
with an appearance in its color: the slicer keeps a filament's color in
its projects rather than its presets, so a filament preset that sets
only its default color gets that one, and one with no color anywhere
gets a neutral light gray plastic named after it, which you can change
in the Materials window; the slicer then gets the appearance's color.
The line
names a single preset ("Fine imported into My library as Fine 2"), and
the manager shows the first imported item of its kind, selected. MitCad
only reads the slicer's files; it never changes them. Into the design,
the import is one Undo step.

### Textures

A texture gives an appearance a set of images, one for each of its
maps. The selector above the list in the appearance library switches
between the appearances and the texture sources. The sources are
*Local Folders* and *Poly Haven*.

*Local Folders* are the folders that you add in Preferences › Textures.
**Add…** chooses a folder, and **Remove** takes the selected folders off
the list. MitCad also searches the subfolders.

A material in a local folder is a set of PNG or JPEG images. A suffix
in the name of each image says which map the image holds:

| Map | Suffix |
|---|---|
| Base color | `_diff`, `_albedo`, `_basecolor`, `_color` |
| Normal map in the OpenGL convention | `_nor_gl`, `_normal` |
| Roughness | `_rough` |
| Metalness | `_metal` |
| Ambient occlusion | `_ao` |

The base color is necessary. The other maps are optional. MitCad
leaves out normal maps in the DirectX convention (`_nor_dx`). The
images of one material have the same name before the suffix, for
example `oak_diff_2k.jpg` and `oak_nor_gl_2k.png`. A material can also
have a folder of its own. Then the name of each file is the suffix
alone, for example `color.jpg` and `roughness.png`. A resolution in the
name (`_1k`, `_2K`, `_2048`) makes that material available in several
resolutions.

The search field and the category narrow the list. Each texture folder
is one category. The appearances have the categories Library and This
design. The materials show as thumbnails.

To use a material from a local folder:

1. Select the material.
2. Select a resolution. MitCad offers the resolution that you selected
   last, at first 2k. The line next to the resolution says how much
   space the images take.
3. Click **Use**.

The material becomes a new appearance of the design. MitCad embeds all
its images in the design file. Thus the design opens on any computer
without the folder. An image that the design embeds already is kept
once. The texture is 100 mm wide, and its height follows the
proportions of the image. If the source knows the real size of the
material, the texture has that size.

The base color of the new appearance is the average color of the
image. The shaded view shows that color. The new appearance is
selected; the Appearance tool gives it to bodies and faces.

*Poly Haven* is an online source. It gives the textures of
[Poly Haven](https://polyhaven.com). All of them are under the CC0
license (`CC0-1.0`). The license permits any use and asks for no
attribution. Thus you can embed the textures in designs and share the
designs. MitCad searches the textures through the public API of
Poly Haven. MitCad is not made or endorsed by Poly Haven.

MitCad uses the internet only when you permit it. The first time that
you select an online source, MitCad asks. If you click **No**, MitCad
asks for nothing. The setting *Allow texture sources on the internet*
in Preferences › Textures changes the choice later.

When you select a texture of Poly Haven, MitCad shows its resolutions
with their download size. **Use** downloads only the maps of that
resolution. These are the base color, normal, roughness, metalness and
ambient occlusion maps, where the texture has them. The texture has its
real size. Poly Haven gives the size in meters. The source records the
page of the texture and its license.

MitCad keeps the list of textures, the thumbnails and the downloads in
the texture cache. Thus MitCad asks Poly Haven again only now and then.
If Poly Haven cannot be reached, the texture list says why. The other
sources continue to work.

The dialog lists the maps of a texture and where the texture came from.

### The rendered view

**View › Visual Style › Rendered** shows the bodies path traced. The
rendered view uses every value of each appearance and the textures with
all their maps. It shows every body normally, also the bodies of
inactive components that the shaded view shows as ghosts. Render Image
does the same. Screenshots show the view as it is. The ghost display
and its preference are described in
[Components](#components).

If you enter the rendered view, MitCad sets View › Component Colors to
off. If you enable Component Colors, MitCad leaves the rendered view.

The rendered view is not available while View › Anaglyph 3D is on (its
tooltip says so), and turning Anaglyph 3D on leaves it. Render Image
renders the normal picture also then.

**View › Render Environment** sets how the rendered view lights and
shows the design. The command is also in Visual Style and Environment.
It has these settings:

| Setting | Choices |
|---|---|
| The light | Studio, White Studio, Dark Studio, Outdoor with the height and direction of the sun, or an HDR image (`.hdr` or `.exr`) with its rotation and strength |
| Behind the bodies | The background of the view, a color or the environment itself |
| The ground | Shadows, reflections and the height of the ground |
| The film | The exposure, and Standard, Filmic or Neutral |

MitCad keeps these settings in the design and saves them with it. Each
change is a step of Undo.

### Lights

The Lights page of Render Environment adds lights of your own. A light
is a point, spot, area or sun light. MitCad keeps the lights in the
design and saves them with it. Each change is a step of Undo.

The Power of a point, spot or area light is in watts. A light of 5 W
at a distance of 300 mm gives about as much light as the key light of
the studio. The Power of a sun is in W/m².

The Size of a light makes its shadows softer as it grows. For a point
or spot light, Size is the diameter of its ball. For an area light,
Size is its width or the diameter of its disc.

A light that *Follows the camera* has its place and direction relative
to the camera. The x axis points to the right, the y axis up and the z
axis toward you. Thus the light moves with the view.

**Aim at Face** points a light at a face:

1. Click **Aim at Face**.
2. Click a face in the view.

The light goes the given distance out along the normal of the face.
Then it shines at the point that you clicked. Press Esc to cancel.

### Render Image

**Render Image** is in the Render group of the Solid tab and also in View › Visual Style.
It renders the current view, or a named view, to an image file. It uses
the settings of the rendered view: the render environment, the lights
and the render device.

| Option | Choices |
|---|---|
| Size | A preset such as 1920 × 1080, or a width and a height |
| Aspect | From the view, or fixed |
| Samples | A number of samples, or a time limit |
| Denoising | Denoise the image |
| Background | A transparent background |
| Format | PNG with 8 or 16 bits per channel, JPEG, or OpenEXR |

When the aspect is fixed, the view shows the frame of the image.
PNG and JPEG images look as the rendered view does. OpenEXR keeps the
linear light of the render, without the exposure and the view
transform.

Render Image works in the background. You can continue to work. The
dialog shows the image as it refines, the samples and the time left.
**Render** is the default button. **Cancel** stops the render; Escape or
**Close** also stops a pending or running render and closes the dialog.
**Save…** writes the finished image. MitCad saves these choices with the design too.

### Render device

The renderer runs on the CPU or on one GPU. The GPU devices are:

- CUDA on NVIDIA cards, Maxwell and newer
- HIP for AMD, Metal for Apple and oneAPI for Intel, in builds that
  include them

Apple Silicon builds include the CPU and Metal. The Linux and Windows
releases ship CUDA. A build without a GPU device renders on the CPU. A
GPU build also starts on a machine without a GPU or without its driver.
Then it lists the CPU only.

**Preferences › Display › Render device** selects the device. It is a
setting of the machine, not of the design. A design moves between
machines, a GPU does not. The setting offers Automatic, the CPU and
each GPU of this machine. Automatic takes the first GPU. If there is no
GPU, Automatic takes the CPU.

If you selected a device earlier and it is not there now, the setting
shows "Not found: <id>". A change of the setting restarts the worker of
the rendered view on the new device. Render Image uses the same
setting.

`mitcad-cli render` takes `--device auto|cpu|cuda|<id>`. The default is
`auto`. The command does not read the settings of the application.
`mitcad-cli render --list-devices` lists the devices.

**Preferences › Display › Render worker** selects the renderer program.
**Shipped** is the default. **Choose a Worker…** picks a worker that you
built yourself, for example an OptiX worker that uses the ray tracing
cores of NVIDIA RTX cards. MitCad keeps the choice, and the device list
shows the devices of that worker. If the picked worker is not there,
does not start or does not match this MitCad, the shipped worker renders
instead, and the status bar says why. MitCad does not ship OptiX. You
build an OptiX worker with the OptiX SDK that you download from NVIDIA
yourself, for your own use
([Building an OptiX worker yourself](rendering.md#building-an-optix-worker-yourself)).

If the selected device cannot start, the CPU renders instead. This
occurs when the driver or the kernels of the device are missing, or
when the device is not there. If the device fails while it renders,
the CPU also replaces it. For example, the memory of a GPU can run
out. The CPU gets the bodies, the environment, the view and the samples
of the failed device, and renders the same view again. MitCad shows the
reason over the view: "Rendering on the CPU: NVIDIA GeForce GTX 1060
3GB (CUDA) failed (…)". Render Image and `mitcad-cli render` show the
reason as a warning.

A GPU can also denoise on the GPU. If such a GPU fails, MitCad first
tries the same GPU with denoising on the CPU. The message is then
"Denoising on the CPU: Open Image Denoise on NVIDIA GeForce RTX 3080
(CUDA) failed (…)". Only if that fails too, the CPU replaces the GPU
with the message above. A GPU that already denoises on the CPU goes to
the CPU at once. Each of these steps occurs at most once.

Open Image Denoise denoises on the GPU when it has a module for that
GPU. On NVIDIA cards, these are the Turing cards and newer. Otherwise,
it denoises on the CPU.

MitCad does not offer several GPUs together, the CPU together with a
GPU, or OptiX.

The renderer can reach the ray tracing cores of NVIDIA RTX cards only
through NVIDIA OptiX. The license of OptiX does not fit open source
software. It permits distribution only in binary form, only for NVIDIA
hardware and only under an agreement that binds every recipient to its
terms. Open source software that anyone can download, build, change
and pass on cannot obey that. Thus MitCad does not build or ship OptiX.
NVIDIA cards render with the CUDA device, which does not use the ray
tracing cores. As a result, RTX cards render slower than they could.

## Components and assemblies

The Assemble group holds the commands that build an assembly. Its
commands are:

- New Component
- Joint
- As-Built Joint
- Joint Origin
- Rigid Group
- Drive Joint
- Assembly Constraint
- Contact Set
- Motion Link
- Animate Joint
- Motion Study
- Insert Component
- Insert from Library

This chapter tells how components, joints, constraints, properties,
motion studies and motion links work. Refer to [Motion studies and
contact sets](#motion-studies-and-contact-sets) and [Motion
links](#motion-links) for the commands that move an assembly.

### Components

These commands make a component:

- New Component
- The New Component operation of an extrude or a primitive
- Create Components from Bodies
- Find Repeated Parts

Paste makes an occurrence. Paste New makes a new component. Activate a
component with its radio button. Then new features go into it.

**Find Repeated Parts** is in the browser's menu of a body of an imported
file. Some files hold every copy of a part as a body of its own, already
moved into its place. Find Repeated Parts finds the bodies that are the
same part, only moved or turned. Each such part becomes one component,
with an occurrence where each copy was. The first copy keeps its name,
color and appearance, and the component is named after it. A mirror
image is not the same part. A body with another color stays apart too.
Bodies that copy nothing stay as they were. Undo takes it all back in
one step.

Ground fixes an occurrence: joints and drags do not move it. Move/Copy
moves components as a timeline feature, a grounded one too, which stays
grounded at the new place. It is also in the context menu of a component's row and, for a
selected component, in the context menu of the view. A body (or a face)
of another component than the active one, clicked before Move/Copy or
in it, moves as that component: Move/Copy takes the occurrence and
selects *Move Object: Components*. With Point to Point, the Origin Point and the Target Point can
be on any component: a vertex, a sketch point, a construction point or
the center of a circle, picked where the view shows it. MitCad keeps the
occurrence that you picked it in, so the point is where that occurrence
places it, also on the second copy of a component placed twice. When
that occurrence moves later, the move follows it. The Point of Point to
Position and the Axis of Rotate work the same way. Insert Component
inserts another design, linked or as a copy.

**Insert › From an Open Design…** lists the other designs open in this process,
whether they are tabs or separate windows. Choose a design and press **Insert**,
then choose **Linked** or **Copy** as with Insert Component. The insertion reads
that design's saved file. A design with unsaved changes, or one that has never
been saved, asks for **Save** first; **Cancel** leaves both designs unchanged.
Saving uses the source design's normal Save or Save As workflow. The destination
cannot select itself. Insertion and Undo affect the destination only.
MitCad reads a linked design again when you open the design that
holds it; the message band names each linked design it took again
("Electronics is updated from board.mitcad"), and warns about one it could
not read.

**Work with a linked design.** A linked component changes only with its own
file. When you try to edit it (activate it, double-click its feature in
the timeline or the browser, or start a command on it), the message band
says "Electronics is linked from board.mitcad" with
**Open Linked Design**, which opens that design in a tab of its own; just
selecting the component shows nothing. Right-click a linked component's row
in the browser, or the component or one of its bodies in the view, for
**Open Linked Design**, **Update Link** (read that design's file again now)
and **Update All Links** (every linked design's). Each update is one step
that Undo takes back. While the holder is open, MitCad watches the linked
files: when one changes on disk, or you save it in another tab, the
message band says "board.mitcad changed" with **Update**. Nothing is
updated on its own; the message stays until you press Update or close it.
In the Docked layout the message and its button are in the status bar.

**Show a linked design by its envelope.** A linked design of many parts,
such as a machine in a production-line layout, can be heavy. Right-click
its row in the browser and choose **Show as Envelope…**. Type a detail
size in millimeters, or leave the field empty for 1 % of the design's
diagonal (at least 5 mm), and click **OK**. The component then holds only
the [envelope](#envelope) of the design's bodies: one light body in the
shape of its outside, with openings larger than the detail size kept
open. MitCad does not keep the design's bodies in memory or in this
design's file while the component shows its envelope. When the linked
design changes, MitCad makes the envelope again. **Show in Full** on the
same row brings the design's bodies back. Both are one step of Undo. A
library part and a design linked with options cannot show an envelope.

Each occurrence chooses for itself. If a linked design is placed more
than once, **Show as Envelope…** changes only the occurrence whose row
you right-clicked: for example, the machine in front of a production line
in full and the others as envelopes. Such an occurrence goes into a
component of its own that links the same file, such as **Machine
Envelope**, so its row is then **Machine Envelope:2**. It stays where it
is, with its joints. If you leave the detail size empty, the occurrence
takes the detail size that the other occurrences of the design are shown
at. **Show All as Envelope…** changes every occurrence of the component
at once. **Show in Full** brings that one occurrence back to the design's
bodies.

Joints to faces, edges and vertices of the linked design keep their
places while the design is shown by its envelope. MitCad finds the faces
where another occurrence shows the design in full, or uses the places
that the joints had when the design was last shown in full. Joints that
you make while the envelope is shown use the faces of the envelope.

A design that shows a linked design by its envelope needs this version
of MitCad or a later one: an earlier version does not open it. To open
the design in an earlier version, choose **Show in Full** for every
envelope first, and save.

**Save a component as a design.** Save Component as Design… on a
component's row in the browser writes the component as a design file
of its own. The row of the root and the rows of linked and library
components do not have it. The new design's root holds the component:
its features and timeline, its sketches and construction geometry, the
parameters that its expressions use, the components in it, and its
materials, appearances, property sets and design policy. Linked and
library components in it stay linked.

A component can refer to things outside it: a sketch on another
component's face, a projection of another component's edges, a body of
another component that a feature uses. The new design cannot carry
these references. The dialog lists them before you save. In the new
design, each one keeps its current geometry, as a broken link keeps the
geometry that it had. Joints to other components stay in this design.
This design does not change.

Select **Replace with a link to the new file** (off by default) to make
the component in this design a linked component of the new file. Its
occurrences keep their places, joints and grounding. The replacement is
one step of Undo. Click **Save…** to choose the file: the project and
folder as Save As asks, with the component's name as the file's name.
MitCad does not write the component over the design's own file or over
a file that a component of the design is linked from, also when the name
differs only in case: choose another file.

**The active component in the view.** While a component other than
the root is active, MitCad draws everything outside it as a ghost.
Outside it are the root's own bodies, the other components, their
sketches and their construction geometry. A ghost body is see-through
in its own color. Ghost lines and planes are pale. MitCad draws these
normally: the active component's sub-components, the sketch that you
edit and the origin's planes.

You can still pick (click) a ghost as a reference, with the usual
highlights. Examples are a face to sketch on, a face to project and a
measurement. A click takes the active component's own item under the
cursor first. This occurs also where a ghost lies in front of that
item. Thus, to pick a ghost, click where only the ghost lies. When you
activate the root, or undo the activation, MitCad draws everything
normally again.

Two settings on the Display page of Tools › Preferences set each of
these behaviors to off: **Ghost inactive components** and **Pick the
active component first**. The rendered view and Render Image show
every body normally. Screenshots show the view as it is.

**A sketch on another component's face.** A sketch of the active
component can lie on a face or a construction plane of another
component. Use Create Sketch on the face where an occurrence shows it.
The sketch stays linked to that face. It follows the face when the
face changes or when either component moves.

**Activate the component of a picked face.** Create Sketch and the
Construct commands ask a question when you pick a face, a plane or an
edge of a component that is not active. This includes a component that
is placed in the active component. An example is "The face belongs to
M6 rod bracket:1, but Corner brace:1 is active." A second line tells
what each answer does: "If you activate it, the sketch goes into M6 rod
bracket:1. If you keep Corner brace:1 active, the sketch goes into Corner
brace:1." (A Construct command names the plane, axis or point instead.)
The question opens beside the face that you picked, so that the face
stays in sight. Click **Activate M6 rod
bracket:1** (the default) to put the sketch or the construction feature
into that component. MitCad activates the component, and the command
continues with the same pick, at the occurrence that you picked. The activation is one
step of Undo, as the radio button makes. Click **Keep in Corner
brace:1** to put it into the active component, linked to the face, edge
or vertex. Select
**Don't ask again** to stop the question. To start it again, set **Ask
to activate the component of a picked face** on the General page of
Tools › Preferences. MitCad does not ask for the faces and planes of the
active component, for the origin planes, or for a face that the design
policy does not let you sketch on.

**Fillet, Chamfer and other features on another component's body.** A
feature that changes a body goes into that body's own component. When the
first edge, face or body that you pick for Fillet, Chamfer, Shell, Draft,
Offset Face, Replace Face, Split Face, Split Body, Delete Face, Move Face,
Thread, Press Pull, Hole, Dogbone or Combine's target belongs to a
component that is not active, MitCad activates that component without a
question and the command continues with the same pick. The panel says so:
"Fillet goes into Lid (now active)." The activation is one step of Undo of
its own, before the feature: Undo takes back the feature first, then the
activation. Cancel keeps the activation. If you pick the bodies of two
components, the panel shows why the feature cannot be made and a button
**Activate Lid** under it. The button activates that component and keeps
only the picks on its bodies; the panel says how many other picks it
dropped. The setting **Ask to activate the component of a picked face**
does not apply to these features.

Project in such a sketch, or in any sketch, also takes edges, faces
and vertices of other components. These projections are linked when
Projection Link is on. Construction planes, axes and points take the
faces, edges, vertices, construction planes, axes and points, and sketch
curves and points of other components too, linked. Other features
work only on the bodies of their own component.

Extrude and Revolve are an exception for the object an extent ends at.
Extrude's To Object, on either side, and its Start: Object can take a face
or a body of another component. Revolve's To Object can too. Pick the
object where an occurrence shows it; the component of the object stays
inactive. The feature stays linked to that occurrence: it follows the
object when the object changes or when either component moves. When you
edit the feature, the panel shows the object where you picked it. Hole's
To Object takes only an object of the hole's own component. The panel
says so when you pick another component's face or body.

### Design policy

A new design starts with an empty component, Component1. Component1 is
active, so the first sketch and feature go into it. Further new
components are Component2, Component3 and so on. A design made with an
earlier version keeps its first component's name, Part1. By default,
every component and the root can hold sketches, features and
components.
Also by default, sketches and features can use the faces and the
geometry of other components.

For a stricter design, Design Policy… sets what a component can hold
and link to. Find Design Policy… on a component's row in the
browser, or in the command search. Select a preset, or set each
setting on its own:

| Preset | A component can hold |
|---|---|
| **Component** | Sketches and features, no components |
| **Assembly** | Components only |
| **Hybrid** (the default) | Everything |

A blocked setting shows how many existing objects break it. A policy
never changes or removes anything that exists. It only refuses new
work, and gives the reason. A design that you save with a policy keeps
the policy.

### Moving features and bodies

**Modeled bodies in the root.** A design often starts with sketches
and features in the root. Components come later. When you make a
component in a root that has modeled bodies, MitCad asks first. This
occurs with New Component, Create Components from Bodies and the New
Component operation of a feature. The question has three answers:

- **Move into a New Component** puts every modeled body of the root
  into one new component. The features, sketches, construction
  geometry and parameters that made the bodies go with them. The new
  component stays where the bodies are now, grounded, so nothing
  moves. As a result, the root holds only components. You can then
  edit the design and reuse it as a component.
- **Continue as Is** makes the component and keeps the root's features
  where they are.
- **Cancel** makes nothing.

For Create Components from Bodies, the move gives each selected body's
component the features that made that body. The answer to "Don't ask
again for this design" is saved with the design. If the feature that
you add uses what moved, the feature goes into the new component too.
An example is an extrude of a moved sketch.

**Later moves.** The same moves are available later:

- **Move Features into a Component**, on a component's row, moves all
  its modeled features.
- **Move to Component**, on a body, a sketch or a feature in the
  timeline, moves that one item. The features that go with it move
  too.
- **Move the Features That Made It into the Component**, on a
  Component from Bodies step, gives that component its history. It
  then removes the step.

To move only a body, from the timeline marker on:

1. Open Move to Component on the body's row.
2. Check **Move Only the Bodies**.
3. Select a component that exists already.

A Body Transfer step in that component moves the body there. The body
stays where it is in the design. The features that made the body stay
in their component. The later features of the destination work on the
body. A move of the destination takes the body along.

Some features work on bodies that would end up in different
components. Examples are a combine, a cut through both bodies and a
sketch on a face of another body. Such a feature cannot go with only
one of the bodies. MitCad lists such features and offers to keep those
bodies together. Each move is one undo step.

**Move a component to another parent.** If a component is active when
you make a new component, the new one is inside it. You can move it
out. **Move to Component…** on its row in the browser lists the root
and the other components by their full paths. Select one. MitCad moves
the component, with everything in it, under the one that you selected.
This changes the structure only. To move the component in space, use
Move/Copy, which is also in the context menu of its row.

The component stays where it is in the design, also under turned
parents. Its history, parameters, name and grounding stay too.
Sketches, links, joints, cut scopes, section analyses, Isolate,
annotations, and the drawings' views, dimensions and balloons that refer
to it through its old place follow it.

Parents that the component cannot go into are listed grayed, with the
reason:

- Its current parent
- Itself, and what is placed in it
- A component placed more than once, because the move would change
  every copy of it
- A linked component

What the move cannot keep is named instead of changed:

- An occurrence placed by a Move, a captured position, a joint, a
  constraint or a rigid group in its parent
- A joint whose moved component would change
- A link across it, while the timeline moves a parent on the way
- Pattern at Points
- A drawing of a component that shows it, when it would leave that
  component

The move is one undo step. Cancel changes nothing.

### Joints and constraints

**Joint and As-Built Joint.** Joint has an origin A and an origin B.
As-Built Joint has a component A. When you pick origin A, or As-Built
Joint's component A, the component of A becomes see-through. It also
lets clicks through. Thus you can pick origin B behind it.

When you pick B, the preview shows A where the joint puts it. Click
the button of Origin A again to show A normally for another pick.

**Assembly Constraint** holds components against each other in rows. Each
row takes away only some motions. Pick geometry 1 on the component that
moves. Then pick geometry 2 on another component. When the row's type
is Automatic, the type follows the picks:

| Picks | Type |
|---|---|
| Two circles | Insert |
| A circle and a hole's face | Insert (Axis) |
| A cylinder or a sphere on a plane | Tangent |
| Other picks | Align |

You can also select the row's type:

- **Align**: faces flush or facing, points together, or axes
  concentric, with an offset or limits
- **Angle**
- **Center**: between two faces
- **Tangent**
- **Insert**: a pin into a hole. Lock Rotation keeps its turn.
- **Insert (Axis)**: the depth stays free.

Add Constraint adds rows and does not close the panel. MitCad solves
the rows together with the joints. The set is one timeline feature.
Its rows are under it in the Joints folder.

**What a joint connects.** Select a joint: its row in a Joints folder,
or its cell in the timeline. The view then highlights what its two sides
are built on, where the components are. The faces, edges, vertices,
sketch geometry, planes, axes, points or joint origins of side A are
orange. Those of side B are green. Each side's frame shows as a small
triad: x red, y green, z in the side's color. The two components that
the joint joins show in a faint tint.

A side whose frame is given directly shows its frame only. That is a
frame override that moves or turns the side. A side that lies on a fixed
plane, axis or point also shows its frame only. A Build component's mount
frame shows the frame and the component. The selection card names the joint,
its kind and each side, for example "Joint1: revolute, a = Pin:1, face …
of Body2, b = Plate:1, face … of Body1". An as-built joint shows its two
components and its motion origin. A constraint set shows each row's
references. With the pointer over a joint's row, the view previews the
same highlight in lighter colors. When you select something else, the
highlight is replaced. When you clear the selection, the highlight is
cleared.

### Dragging components

A component that its joints and constraints leave free to move can be
dragged in the view with the left mouse button. The joints and
constraints hold while the component moves. The drag is one undo step.
Over such a body, the pointer becomes the move cursor. The status bar
says what the drag moves, for example "Drag to move Pin:1 (Joint1:
rz)".

A drag on a body that cannot be dragged moves nothing. The status bar
says why. The reasons are:

- The component is grounded, or it is in a rigid group with a grounded
  component
- Its joints hold it fixed (a rigid joint)
- It has no joints. Components move only through their joints. Create
  Components from Bodies makes a component of each body of a
  component.
- The body belongs to the root component
- A command is open
- The design is read-only

In a design with components, a drag that starts on a body drags the
component, or the status bar says why it does not. Thus a window
selection starts from empty space, or with Shift held.

### Properties

Components, their occurrences and bodies carry typed properties in
named sets. Examples are `Identity / Mark = W-12` and
`Pset_WallCommon / FireRating = EI60`.

**Property Sets…** shows the properties in a window. Find it in
these places:

- The browser's menu of a body or a component
- The view's menu of a picked body or face
- The command search, for the selection

The window has one section per level: the body's own properties, the
occurrence's and the component's. The component's properties are the
definition that every occurrence shares. An occurrence shows its
component's values as *inherited*. A value set on the occurrence
overrides the component's value for that occurrence only. You can fold
each set.

Each value has a type and a source. The types are text, integer,
number, length, area, volume, mass, angle, yes/no and date. A design
can also have its own types. Types… defines an own type with a name,
a base type, its permitted values, a unit and a description. An
example is FireRating with the values EI30, EI60 and EI90.

The source says where the value came from: you, an import (`rvt`,
`ifc`, `ipt`, `f3d`), a library component or the design (`computed`).

To change a value in place, double-click it or press F2. MitCad reads
lengths and other quantities in the design's units, unless you type a
unit (`1.2 m`, `500 g`). For a value of an own type with permitted
values, or a yes/no value, select the value from a list. If you change
an imported value, MitCad keeps the value that the source gave. The
source column then says *edited*, and the tooltip shows the original.

Add… adds a property to one of the levels. Remove removes one. Type
a value as in the table, and a date as `YYYY-MM-DD`. Each change is
one undo step.

The design calculates some values when they are shown: a body's volume,
area, mass and material, a component's volume and mass, and a Build
member's length. Their source is *Computed*. A library component's
designation, supplier, part number and material have the source
*Library*. These values are never saved in the design file, so they
always follow the design. You cannot change a computed value.

**Design Properties.** File › Design Properties… shows the design's
summary and statistics. You do not need to select anything first. The
**Summary** tab has the Title, Subject, Author, Keywords and Comments
of the design. Comments can have several lines. OK saves the changes
as one undo step. MitCad keeps the summary as the root component's
property set `Summary`, so Property Sets… of the root shows it too. An
`.ipt` or `.iam` file's summary comes into the same set. Drawing title
blocks show the summary's title, author and comments where the sheet
and the component give none. The description of Save with Description
belongs to a version, not to the summary.

The **Statistics** tab shows when the design was created and last
saved, the user name of the last save and the MitCad version that made
it. Save writes them into the design file when the file changes. A
design saved by an earlier MitCad shows *unknown* for Created.
Design Properties… also opens in a read-only window, while a command
is open and in a sketch: the summary then shows without editing, and
the window has only Close.

**Search by property.** In Find in Design, a search with a comparison
finds by property values instead of names: `FireRating = EI60`,
`Pset_WallCommon/FireRating = EI60`, `Mark != W-12`, `Mass > 10 kg`,
`Name ~ wall` (contains).

**Bill of Materials columns.** The Bill of Materials of the Build tab takes property
keys as columns. Type them in Property columns, separated by commas:
`Mark, FireRating, Mass`. The columns are also in its CSV and PDF.

**Imported properties.** Imports bring the properties that their files
hold:

- An `.rvt` element's identity, instance parameters and type
  parameters go on its bodies. Built-in parameters have their common
  names. Other parameters are named `Parameter <id>`.
- An IFC element's property sets and quantities go on its bodies and
  occurrences. Its type's sets go on the component of the type.
- An `.ipt` or `.iam` file's part number, description fields, author
  and user-defined properties go on the design. The parts of an
  assembly put theirs on their components.
- An `.f3d` dump's part numbers and descriptions go on its components.

In an `.rvt` file, lengths, areas, volumes, angles and masses are
imported as those quantities. MitCad shows them in the design's units.
A wall's width of 200 mm shows as 200 mm, not as the file's 0.656
feet. A number whose kind the file does not tell is marked `internal
units`, as the file keeps it.

### Motion studies and contact sets

A **contact set** names components whose bodies can touch but not pass
through each other while joints move. Make one with Assemble › Contact
Set. Pick two or more components in the browser, or bodies that they
place. These motions stop where two bodies of different components of
a set touch:

- A drag of a component
- Drive Joint
- Animate Joint
- The playback of a motion study

Touch means within 0.01 mm. The motion says so. Drive Joint's panel
says "stopped at contact: Body2 (Pin:1) and Body3 (Wall:1) in
Contact1". The others say it in the status bar.

When a joint is driven so and stops, it keeps the value where it
stopped. Bodies that already pass through each other where a motion
starts do not stop it. Thus you can move components apart. In the browser's
**Contact Sets** folder, you can suppress a set, edit it, rename it
and delete it. A suppressed set stops nothing. A design can have
several contact sets.

**Animate Joint** (Assemble) plays a joint's free motion through its
range. It plays the joint selected in the timeline, else the newest
one. It changes nothing in the design.

A **motion study** keys joints along a number of steps. Start one with
Assemble › Motion Study, or with Play on a study in the browser's
**Motion Studies** folder. For each joint and each free motion, the
study holds values at steps, in the joint's units. A cylindrical
joint's slide and turn are two motions. Between two keys the value
goes evenly. Before the first key and after the last key, the value
stays.

The study's panel has:

- The step slider
- The keys of each joint motion, with the value at the current step
- Add Key, which keys the selected joint motion at the current step.
  It takes the typed value, else where the joint is in the design.
- Remove Key
- Play and Pause
- Loop
- The speed

Playback moves the components through the joints, as Drive Joint
does. Constraints and other joints follow. Playback stops at a contact
of a contact set. It changes nothing in the design until **Keep
Position**. Keep Position drives the keyed joints to the shown step,
as one undo step. Studies and contact sets are saved in the design.

### Motion links

A **motion link** (Assemble › Motion Link) makes one joint's free
motion, the follower, follow another joint's free motion, the driver.
Then the two move together whenever either moves: in Drive Joint, in a
drag, in Animate Joint and in a motion study. A contact of a contact
set stops both.

To make a motion link:

1. Pick the driving joint, in the browser or in the timeline.
2. Pick the following joint.
3. For a joint with more than one free motion, select which motion of
   each joint the link uses.
4. Select the type.

The types are:

- **Gears**: two turning joints. The follower turns the driver gear's
  size divided by its own size as far as the driver, the other way.
  For an **Internal Gear** (a ring gear), the follower turns the same
  way. A size is a radius, or any two numbers in proportion, such as
  tooth counts. A size can also be the radius of a round face, edge or
  circle that you pick beside it. Then the size follows the design.
- **Rack and Pinion**: a turn and a slide, either one can drive. The
  rack travels the pinion's radius per radian.
- **Screw**: a turn and a slide. They are a cylindrical joint's own
  two motions, or a revolute joint and a slider. The slide advances
  the lead per turn.
- **Ratio**: the follower's value is the ratio times the driver's
  value, in mm or radians each.
- **Cam**: the cam turns on a revolute joint, the driver. Its follower
  slides on a slider or swings on a revolute joint. The follower
  stays on the profile, and comes from the side that it is on now.

For a cam, also pick:

1. The cam's profile: a flat face of the cam, or a curve of the
   sketch profile that the cam is made from.
2. The follower's shape: a knife edge, a roller with its radius, or a
   flat face for a sliding follower.
3. The follower's tip: the knife's point or the roller's center. A
   swinging follower must have a tip.

**Reverse** turns the follower the other way. **Keep Where They Are**
adds the offset that leaves both joints where they are. It is on by
default, except for a cam. If it is off, the offset is the follower's
value when the driver is at zero. A cam's follower moves to touch the
cam.

Once linked, either joint can be driven. A cam link can be driven only
by the cam. The motions linked to the driven joint let go of their own
positions. The links are in the component's **Motion Links** folder in
the browser and on the timeline. Edit, suppress or delete them there.

## Build tab

The **Build** tab is beside Solid. It builds frames of aluminum slot
profiles with their connectors, panels and Linear Component components. Select a component
and click where it goes. Its length, joints, miters and connectors follow
from the clicks.

The result is ordinary design content. Each placed item is a component
of its own. It has its occurrence, its features and its joint on the
timeline. You can edit it with the Solid tab's commands, for example a
hole sketched on a profile, or a cut. MitCad saves it in the design and
opens it again as it was.

The components come from MitCad's component library. To get the library, add it
and fetch it in Tools › Libraries, or use Update in Browse Components.
Each component lists its supplier and part number. A component placed once stays in
the design. Thus you need no library to place more of it.

You do not need to activate a component first. The Build tab puts its components into
the active component when it can hold components. If it cannot, the Build tab
puts them into the nearest component above it that can. In a new design
that is Component1 itself. Component1 is active and holds components.
The panel shows where (*Adds to: Component1*).

### Profile

**Profile** (Profile group) places one profile. At first the panel shows only
the **Component**, its **Length** and its **Direction**. The Length is the
profile's stock length, or 500 mm. The Component is a tile. It shows the
selected profile's picture, its name, and its supplier and part number.

To select a profile by its picture, click the **Component** tile. When the
tile has the keyboard, Enter or Space also opens it. The
[component picker](#component-picker) opens. In the picker you can:

- Search by name, size, system, slots, supplier or part number
- Narrow the list to one system (`B8 30` is B-type slot 8 on a 30 mm
  grid)
- Find your favorites first
- Find the profiles this design already uses in *In this design*

The picker opens with the system of the profile you selected last. Click
a picture. MitCad selects that profile, and the preview shows it at
once. The view stays yours to click in. The picker closes with the
command unless you pin it. A pinned picker stays open. It shows the components
of the next Build command that selects one (Panel, Linear Component,
Connector). Panel and Linear Component have the same tile.

The first profile goes on a plane or a face. The view says *Click a
plane or a face, or press Enter*. Or press Enter to place it at the
origin.

- On a face, one click places it where you click.
- On a plane, two clicks: the first one selects the plane. Click the
  icon of one of the origin's planes or of a construction plane, or its
  row in the Browser (**XY**, **XZ** or **YZ** in the Origin folder).
  Then the preview follows the mouse on that plane, wherever the mouse
  is in the view. The next click places the profile there. Thus the
  first profile can go anywhere on the plane, for example in the middle
  of a table whose corner is at the origin. Before that click, Enter
  places it at the plane's origin, and Esc deselects the plane. Other
  planes and faces take no click until then.

The view shows the origin's planes when the eye of the Origin folder is
on; else use the Origin folder's rows in the Browser.
The X, Y and Z buttons select the axis it runs along (Z: standing). Drag
the handle at its free end, or type a length. The length also shows in a
box at the cursor.

The next profiles go on a profile you click. Only the profiles' faces
highlight. The planes are hidden until you close the panel.

On a side: a click near an end, on a profile's side, puts the new
profile along that slot. In one profile width of an end, it sits flush
with that end. Near the middle or a grid point of the profile system, it
snaps there, and the preview marks the snap. The new profile never sticks
out past an end. Snaps keep a size on the screen. Thus they also work in
a zoomed-out view.

The click fills **Offset** and **From** (End A, End B, Middle or Middle
of Gap):

- Flush with end B is From End B, Offset 0
- On the middle is From Middle, Offset 0
- On the middle of a gap is From Middle of Gap, Offset 0
- Elsewhere it is the nearer end and the distance from it

A gap is the free stretch of the side between the profiles that stand on
it, or between one of them and an end of the side. The preview marks the
middle of each gap. For example, on a frame side with a cross member in
the middle, click near the middle of either half. The new cross member
goes in the middle of that half. It stays there when the middle member
moves or the side gets longer. When you delete one of the profiles
around it, the profile stays where it is. It is then bound to the nearer
end of the side.

The profile stays bound to that place. When the profile it is on becomes
longer, one flush with end B stays flush with end B. One on the middle
stays on the middle. When you type an offset, MitCad moves the profile
from the same reference. From end A the reference is its near face. From
end B it is its far face, and from the middle or the middle of a gap it
is its center. When you
change **From**, the profile stays where it is, and the panel shows its
offset from the new reference. A new click replaces both.

**Tilt** leans the profile along the profile it stands on. A positive
value leans it towards that profile's end B, a negative value towards
end A. The limit is 80 degrees. 0 stands square. MitCad cuts its foot to
lie on the side. An automatic length ends flush on the face it meets.

On an end: a click on a profile's end face puts the new profile on that
end. The click decides the direction. In the middle of the end face, the
new profile continues straight on. Near an edge, it turns towards that
edge (a corner), in the profile's own axes. With the mouse over an end
face, the preview shows which before you click. MitCad marks the face's
zones (*straight*, *turn +X*, …). **Direction** changes it afterwards.
The options are Straight, or one of the four sides. When the profile
turns, its side is named by the axis it runs along (+X, -Y, …).

A corner lies in the profile's plane with its outer faces flush. By
default (Miter *Auto (none)*) the new profile sits on the end face. Its
end is flush with the outer side, and both ends are square. With Miter
*Shared*, MitCad cuts both profiles on the bisector, and their outer
corners meet. The corner's **Offset** moves the new profile's end from
that outer side towards the turn. A shared miter meets only at 0. Thus at
any other offset, Shared is not offered. **Miter** can also cut only the
existing profile or only the new one.

**Rotate** turns the profile about its axis in 90 degree steps. **Flip**
puts its other end first.

**Placement** has four buttons. They show once there is a profile to go
on:

- *Automatic*: where the click is (the default)
- *On a profile's side*
- *On a profile's end*
- *On a plane*: to place a profile as the first one again

The profile's length runs to the profile in its way and stays bound to
it. When that profile moves, the length follows. The preview highlights
that profile's face. It shows the length at the free end and in a box at
the cursor. The **Length** buttons select one of these:

- To the face it meets
- To its center
- Through it
- A typed length
- To a face you pick, plus an offset

**Tab** steps through the first three. The last one ends the profile at a
face that is not in its way, for example the top of another upright
beside it. Pick that face (or a plane, or a profile's end) in **To
Face**. The profile ends where its axis meets the face's plane, plus the
**Face Offset** (a negative offset ends it short). Uprights on bases of
different heights can thus end level with the first one's top. The length
stays bound: when the first upright gets longer or moves, the others
follow. This mode is also there for the first profile on a plane or a
face. MitCad refuses a face whose plane runs along the profile, or that
is behind the profile's start. When you type a length in the box
at the cursor or in the panel, the length becomes a plain number. When
you drag its handle, the length also becomes a plain number. **Place**
(Enter) places the profile, and the panel stays open for the next one.
After **Place**, nothing is previewed until your next click. When you
place **On a plane** again, the next profile waits for a click on a
plane or a face. It does not show at the origin on top of the profile
placed there. **Close** (Esc) ends the command and keeps what you
placed.

### Panel and Linear Component

**Panel** fills a frame or puts a sheet on a face. Click the inner face
of a frame's profile. The panel fills the frame in its slots, sized to
the frame. A sheet sold in fixed widths keeps its width. It says when it
does not fit. Click an outer face. The panel goes on the face, held by
clamps. On a timber member's face, Panel puts a sheet on that face. A
generic panel takes any thickness.

**Linear Component** places these components:

- Rails, cut to length, with their hole pitch kept from end A
- Carriages. A carriage on its rail slides along it: drag it, or use
  Animate Joint
- Slot-mounted carriage mounts
- Bearings
- Motor plates

### Connect

**Connector** places a connector at a junction. With the mouse over a
profile, MitCad marks the places where it meets other profiles (L, T,
cross, inline). Connected junctions say so. *Show All Junctions* marks
every one. Click a profile near a junction, or select the junction in
the list.

The panel lists the connectors that fit the junction's systems and the
room in the corner. Each shows its picture, supplier and part number.
The panel grays out the connectors that nearly fit, with the reason. The
preview shows the selected connector in place with its nuts and screws.
When you select another one in the list, the preview shows that one.

The **Connector** tile shows the connector's picture, name, supplier and
part number. Click it to show the connectors in the component picker. The
connectors that fit come first. The connectors that nearly fit are
grayed with the reason. A click on a picture previews that connector.
The picker follows the junction you move on to.

**Connect** places the selected connector with its nuts and screws and
the end machining it needs, for example a bore in the profile's end.
Then it moves on to the next junction. When you remove a connector,
MitCad removes what it added.

At a timber junction the connector is a wood fastener or a joint. A wood
fastener has a count of screws or nails in a row along the contact, and
a size. The fastener's *Auto* size takes the diameter from the thinner
member. It takes the length through the first member and well into the
other.

**Fasteners** changes a connector's nuts and screws. You can set them to
another size, remove them, or add them again.

### Modify

**Length** sets a component's length. The length can be bound to a face: to
it, to the profile's center, or through it. It can also be bound to a face
or a plane anywhere, plus an offset. Or it can be detached as a number.

**Ends** sets an end's miter angle and the axis it turns about. *Also the
Component It Meets* also cuts the end that meets it. *End Machining* sets a
bore in the end, or none. The bore has a diameter, a depth and a thread,
for a screw or an automatic connector. The other end's machining stays
as it is.

**Flip** and **Rotate 90°** apply to the components you select. Enter turns
them again.

**Move/Copy** (Solid tab, **M**) moves a placed component. Click it in the
view, then start Move/Copy, or click it in Move/Copy. Each placed
component is a component of its own. Thus Move/Copy takes its occurrence
and selects *Move Object: Components*, and the whole component moves.
The same applies to a body of any other component than the active one.
A first profile that is not on a plane of the component it is in is
grounded instead: one placed with Enter and no plane, or on the root's
plane while a new design's Component1 holds the profiles. Move/Copy
moves it too, and it stays grounded at the new place. The panel says
*Grounded: it stays grounded at the new place.* The profiles placed on
it before the move stay where they are, as with any move of a
component.

### Output

**Cut List** lists the components to cut. Components that are the same are grouped.
Each group shows its quantity, length, end angles, machining, supplier,
part number and mass. The list also shows the stock lengths the parts
come from, with the kerf (the saw cut's width), and the other components.
**Copy as CSV** puts the tables on the clipboard for a spreadsheet.
**Save as CSV…** writes the tables currently shown to a UTF-8 CSV file.
**Copy as Text** puts them on the clipboard as plain text with the
columns lined up, ready to paste into an email. **Save as Text…** writes
the same text to a `.txt` file. Each column is as wide as its widest
entry, and numbers are right-aligned. A blank line separates the tables.
**Save as PDF…** writes them with the design's name and the date.
The CSV and text lines end the way Windows and email programs expect
(CR LF), on every system.

**Bill of Materials** lists components with their quantities, suppliers and part
numbers. A single-body component has one component row. Bodies in the root,
and the bodies of an unstructured component with several bodies, have their
own rows instead; they are not also counted as a component row. Feature-created,
linked and library parts keep their component row, even when made of several
bodies, and purchased items keep their own rows. A component with a part
number of its own (its **Part number** property) is one part too, whatever
its number of bodies: a bearing of nine bodies from Component from Web is one
row, not nine. Remove the part number to list its bodies apart.

Switch on **Group identical parts** to combine separate components and bodies
with equal volume, local-frame bounding-box dimensions and face count, and the
same material. Measurements within 0.000 001 match; a mirrored equal part can
match too. An unsupported measurement leaves the part separate. Hover over a
row to see the names combined into it. The quantity is their total, and the
first part supplies the row's descriptive columns. The option is remembered
for your user and changes only the report: the bodies, occurrences and undo
history stay as they were. Switch it off to see the individual rows again.
Clipboard, CSV file and PDF all use the list as currently shown.
A **Material** property column shows each row's physical material in its actual
placement, including inherited defaults; **Computed/Material** does the same.
A qualified custom field such as **Identity/Material** keeps its own text.

**Print…** (or Ctrl+P) in either window prints the tab shown as its PDF
lays it out. The system's print dialog chooses the printer, the paper
and the copies.

### Timber

The **Timber** group has three tools, split by what they build:

- **Timber** for loose members
- **Wall** for wall frames ([Timber frames](#timber-frames))
- **Floor / Roof** for floors, ceilings and roofs ([Timber
  frames](#timber-frames))

Each tool places members of the timber selected in its Component tile. The
component and the fastener that you selected last carry over from one tool
to the next. Each tool keeps its own other selections.

The **Timber** tool's **Mode** buttons select how it places a member:

- **On a member** (the default): on a member's side or end, or on a
  plane
- **Spacing**: a row of members
- **Two points**: a member between a start and an end

In **On a member** mode, Timber places a member as Profile places a
profile, with the length you typed last.

In **Spacing** mode, Timber makes a row of members. *Spacing On* sets
where the row goes:

- *Between two members* (studs, joists): the row goes on one member's
  side, and each member reaches the other
- *Over the supporting faces* (deck boards, battens): the row runs along
  the **Supporting Faces** you select, such as the joists' top faces,
  and each member goes across all of them

**Spacing** is a center distance or a gap between boards. A center
distance is for studs, joists and rafters (400 mm). A gap is for a deck,
a fence or cladding (5 mm). With a gap, **Last Board** sets the result:
MitCad cuts the last board to what is left, or spreads the gaps evenly.

In **Two points** mode, **Start** is a plane or a face the member lies
on, or another member. Another member's ends, sides and center lines
snap. **End** is where the member ends. MitCad moves the end onto the
start's plane. Shift+click places the member.

**Lie** lays a member placed by two points, over faces or on the ground.
*Flat* puts its thinner side across the plane it lies on. *On edge* puts
the wider side across it. Tab turns it.

**Spacing Set** (Modify) changes a row afterwards. Select one of its
members. Change its distance, count or the members' size. MitCad lays
the members out again.

### Timber frames

**Wall** and **Floor / Roof** make a whole frame in one step. The frame
uses the timber selected in their Component tile. Each tool shows only the
inputs of what it builds. Floor / Roof selects between a floor and a
roof with its **Kind** buttons at the top. It starts with the kind that
you used last.

A **Wall** starts and ends where you click. Click where the wall's center
line starts and ends. For the first wall, you can press Enter instead.
Then the wall runs from the origin, with **Length** along **Direction**.
After **Place**, the panel keeps its values. But it shows no wall until
you click where the next wall starts. Give the wall's **Height** and the
studs' **Spacing** (600 mm). The wall gets a bottom plate and a top
plate. If you select it, it gets two top plates. It gets studs at the
spacing from its start, and one stud at each end.

**Openings** opens the Openings window beside the panel. The window is
described below. Each opening gets king studs, a header, a window's sill
and cripple studs.

A wall can join another wall frame. Click the wall's start or end on a
member of the other wall frame. At that wall's end, the junction is a
corner. Along that wall, it is a T-junction. The other wall runs through
and gets the extra studs. This wall stops at its face.

A wall that you start with a click on another wall's side runs square
to that wall. A click near that wall's end snaps the new wall flush with
the end, a clean corner. A click near the middle centers the new wall.
**From** (End A, End B or Middle) and **Offset** show where the wall
went. When you type them, the wall moves. Offset 0 is flush with that
end, or centered. Without an End click, **Length** *Auto: to the wall it
meets* ends the wall at the first wall frame or other face that its run
reaches. The wall then butts against it. An End that you click has
priority. When you type a Length, Length changes to *Typed length*. The
panel shows what the run met and the length of the wall. It also shows
them when MitCad cannot make the wall.

A plate that is longer than the longest timber is made of pieces. The
pieces are butted end to end, and each joint is over a stud. Where no
stud is near, MitCad adds a backing stud. With two top plates, the
joints are one stud spacing apart. The panel lists the joints, and the
cut list shows the pieces.

A **Floor / Roof** frame of Kind **Floor or ceiling** fills a rectangle.
Click two opposite corners, or give Length and Width. The frame gets rim
joists all round and joists at the spacing (400 mm). It gets a row of
**Blocking** if you want it. MitCad frames stair **Openings** (in the
Openings window) with doubled trimmers and headers.

A **Floor / Roof** frame of Kind **Roof** goes on two walls. Click a
member of each of two parallel wall frames. Select **Gable** or
**Mono-pitch**, the **Pitch** and the **Overhang**. The roof gets
rafters at the spacing (600 mm). The rafters are plumb cut at their
ends. MitCad notches them with a birdsmouth over each wall's top plate.
A gable roof also gets a ridge board.

**The Openings window.** The **Openings** window lists a wall's windows
and doors as rows. Each row has these fields:

- **Kind**: Window or Door
- **From**: End A, the wall's start, or End B, its end
- **Offset**: to the opening's near side
- **Width**
- **Height**
- **Sill**: a window's height above the wall's base

A floor's openings have **Offset Along** and **Offset Across** (from the
floor's first corner), **Width** and **Length**. **Add Window** and
**Add Door** add one opening after the last one. **Remove** takes the
selected opening away. The preview follows each change. Enter or Tab commits a
field. The selected opening is marked in the view. Some rows cannot be
framed: an opening outside the wall, one that overlaps another one, one
too near the wall's end studs, or one whose header is above the top
plate. Such a row says why on the row.

At the top of the window, **Non-load-bearing (partition)** is the
default for a new wall. It frames each opening with one header that lies
flat between its king studs, and no trimmer studs. **Load-bearing** adds
trimmer studs beside the king studs. The trimmer studs carry a header on
edge. A wall made before this selection existed stays load-bearing.
Timber Frame (Modify) opens the same window for a wall or a floor that
is already placed.

**The right-click menu.** The view's right-click menu offers the Timber
tools under **Build**. On a member's face, it offers Timber (the member
goes on that face) and Wall (the wall starts there). With members of
two wall frames selected, it offers Floor / Roof first. Floor / Roof
then starts as a roof on those walls.

With **Fasteners** checked, wood screws or nails go into each junction
of the frame. These options set them:

- **Fastener**: its picture opens the component picker with the library's
  wood screws, ring nails and structural screws
- **Size**: *Auto* sizes them from the members, or select a size such
  as 5.0x90 or a nail's 3.1x90
- **Count**: the count at each junction (2)

The first time, the fasteners are two of the library's countersunk wood
screw. They are sized from the members and driven flush with the wood.
The pan-head connector screw is for metal connectors: it fixes them. The
Timber tools remember your selection. Timber's **Spacing** has the same
**Fasteners** row. It is off at first. When it is on,
MitCad fastens each member of the row to the members it stands on,
reaches or crosses.

**Swap Fastener** (Build, Connect) changes the fasteners you selected
into another fastener or size, where they are, in one undo step. It is
also in the view's right-click menu under Build when fasteners are
selected. Wood screws and nails become another wood fastener. A
connector's nuts and screws become another of their slot system. The
status bar shows the warnings for the new size (too short, too near an
edge or an end).

The preview shows the members and counts them. It shows any warning, for
example a ridge board too shallow, or a birdsmouth too deep. Every member
is an ordinary Build component. It shows in the cut list with the other
timber, grouped by size.

**Timber Frame** (Modify) changes a frame afterwards. Select one of its
members. Change what Wall or Floor / Roof showed for it: its length,
height, spacing, openings, or a roof's pitch. Or change the members'
size. MitCad lays the frame out again in one step. An undo takes it
back. A roof on a wall follows the wall.

The Build commands work through live MCP as the other panels do
([docs/mcp.md](mcp.md)). The model's commands behind them are in
[commands.md](../core/model-api/src/commands.md), *Build: profile
assemblies*.

## More features

### Surface modeling

The Surface group of the Solid tab makes surface bodies. A surface body
has faces but no volume. Later features fill, sew, trim and thicken
surface bodies. [Patch, Stitch and Thicken](#patch-stitch-and-thicken)
describes three of the group's features. This section describes the
other features of the group.

**Surface Extrude**, **Surface Revolve**, **Surface Sweep** and
**Surface Loft** are Extrude, Revolve, Sweep and Loft that make
surfaces. They make the walls without caps. Select profiles and sketch
curves. The boundaries of the profiles become walls. The sketch curves
can be open or closed. Chain Selection adds the curves that are joined
to the curve you select.

Each of the four features makes new surface bodies: one body of the
profiles' walls and one body of the curves' walls. The walls get their
names from the sketch curves they come from. Thus features on the walls
follow when the sketch changes. The extents of the four features are:

| Feature | Extent |
| --- | --- |
| Surface Extrude | A distance one way, both ways, or two distances |
| Surface Revolve | A full turn or an angle about a sketch line, an axis or a straight edge |
| Surface Sweep | Along a path of sketch curves or edges |
| Surface Loft | Through sections in order. Each section is a sketch curve with the curves joined to it, or a profile. A 3D sketch's curves can be sections in any plane. Rails guide a loft of profiles; a rail can be curves of a 3D sketch |

**Offset Surface** copies the faces you select out along their normals
by a distance. The faces can be faces of a solid or of a surface. The
result is a new surface. With a distance of 0, the result is a copy in
place. Tangent Chain adds the faces that are joined smoothly to the
selected faces.

**Ruled Surface** hangs a strip off a chain of edges. The strip has
one of three directions. It goes along the normal of the face beside
the edges. Or it continues that face. Or it goes at an angle from the
normal of that face. At corners, the strips meet mitered or rounded.

**Extend** makes a surface body longer past its open edges by a
distance along the surface. With Natural, the surface continues. The
other options keep the edge tangent to the surface, or also keep its
curvature. Extend works only on a face that is bounded by the natural
sides of its surface, that is, four sides and no holes.

The open edges of a surface body are edges of one face only. You cannot
select open edges themselves. Ruled Surface and Extend take them through
**Open Edges Of**: the faces whose open edges you mean. All open edges
of each selected face are used.

**Trim** cuts a surface body with a plane, a face, a body or sketch
curves, like Split Body. Trim keeps only the pieces you click under
**Keep**. Click the surface on each piece that you want to keep. Trim
does not divide a solid. Use Split Body for a solid. As in Split Body,
**Extend Trimming Tool** extends a face along its surface, and the view
then shows the extended tool as an orange, see-through surface.

**Untrim** gives a face its surface back. Untrim closes the holes of the
face. With Outer Boundary, Untrim also puts the outline of the face back
at the surface's own edges. A plane or a cylinder has no edges of its
own, so the outline goes out to the size of the face. Untrim makes a new
surface. With In Place, Untrim changes the face of the surface body
itself.

**Unstitch** takes the faces you select out of a body as surfaces of
their own. Select a body to take it apart completely. A solid becomes
surfaces.

**Reverse Normal** turns the selected faces of a surface over. Select
the body to turn a whole body over. The normal decides the side to which
Thicken and Offset Surface go.

**Blend Surface** bridges two chains of edges with a new surface. The
chains can be on one body or on two bodies. Each end of the blend has a
continuity:

- G0: the blend leaves its edges straight across
- G1: the blend is tangent to the face beside the edges
- G2: the blend also has the curvature of the face beside the edges.

A bulge makes the blend fuller or flatter. With Stitch, MitCad sews the
blend onto both bodies.

**Boundary Surface** fills a loop of sketch curves and open edges. If
you ask, the surface is tangent to the plane of the sketch or to the
faces beside the edges. Boundary Surface can also run through a
network. The network has curves one way, lofted through in order, and
curves that cross them.

**Match Surface** rebuilds a face of a surface. The side of the face
nearest to a chain of edges of another body then lies on that chain.
That side also meets the face beside the chain smoothly. The opposite
side of the face stays where it was. The face must have four sides and
no holes.

**Fair Surface** smooths B-spline faces in a tolerance. B-spline faces
come from imports, fills and networks. The edges of the faces stay. With
Keep Tangents at Edges, the tangents at the edges also stay.

An import or a script can make a surface with options that the surface
panels do not show. Examples are tapers, an extent up to an object, and
a guide rail. Such a surface opens in no panel. It stays as it is. But the
timeline still suppresses it, rolls it back and deletes it.

The details are in
[commands.md](../core/model-api/src/commands.md#surface-modelling).

### Patch, Stitch and Thicken

A surface body is a body of faces without a volume. Examples are a
surface loft, imported surfaces, and a face that you deleted without a
healed gap. Three features of the Surface group of the Solid tab fill
surface bodies, sew them together and make them solid: **Patch**,
**Stitch** and **Thicken**. The model's commands and the `.solidcraft`
import also make these features. [Surface modeling](#surface-modeling)
describes the other features of the group.

**Patch** fills closed regions of a sketch with flat surface bodies, one
body for each region. Patch can also fill a closed loop of a body's
edges with a new surface body. When the loop lies in a plane, the new
body is a flat face. Otherwise the new body is a surface fitted to the
edges. The loop can be edges you select, or an opening of a surface
body. An opening is the free edges of the body, which MitCad finds by
the faces around the opening.

The patch can meet the faces beside the loop in three ways:

- Only at the edges
- Tangent to the faces (G1)
- Also with the curvature of the faces (G2).

The patch can also pass through points that you give. The body that the
loop is on stays as it is.

**Stitch** sews the faces of bodies together where their edges meet in
a tolerance. The result keeps the name of the first body. Stitch uses
up the other bodies. If the faces close up all round, the result is a
solid. Otherwise the result stays a surface. For example, an open box
stitched with a patch of its opening becomes a solid box.

**Thicken** makes surface bodies solid with a wall along the normals of
their faces. With a negative thickness, the wall goes the other way.
Thicken can also put half of the wall on each side. Like an extrusion,
the result is a new body, or MitCad joins it to, cuts it from or
intersects it with other bodies. The surfaces stay.

The faces that these features make have names that later features
follow. A patch's face gets its name from the patch. The front, back and
side faces of a thickened wall get their names from the surface they
come from. The details are in
[commands.md](../core/model-api/src/commands.md#patch-stitch-and-thicken).

The panels of Patch, Stitch and Thicken accept what the panel shows: a
patch of profiles, or a patch of edges you select with one continuity
for the loop. A script or an import can make other patches: a patch of an
opening, a patch with points, or a patch with edges that each have their
own continuity. Such a patch keeps its definition. But you cannot edit it in the
panel.

### Plastic part features

**Plastic Feature** (Solid tab, Create group) adds the features of
moulded plastic parts. Its **Type** selects the feature:

- **Boss:** a round post where you click a face. You set its height. It
  can have a hole down from its top, ribs around it and a rounded root.
- **Lip** and **Groove:** a band along the rim of an opening. The
  opening is the open end of a shelled body. A lip is raised and a
  groove is cut in. The band goes along the opening or along the outer
  edge. A groove is wider and deeper by its gap. The gap is the
  clearance for the lip that goes into the groove.
- **Snap Fit:** a cantilever arm on a face with a catch at its tip. You
  select the direction in which the catch points.
- **Rest:** a pad on a face. The pad is round or a rectangle, and solid
  or a wall.
- **Plastic Rule:** the design values of a moulded part. The values are
  the nominal wall, the nominal radius, the clearance, the draft, the
  smallest draft, and the smallest and largest wall. Under More, the
  values are the knife edge, the reveal height and the thickness
  variation.

A boss, a rest and a groove name a plastic rule. The rule is the newest
one before the feature, or None. Where the feature's own draft or gap is
0, the feature takes the draft or the clearance of the rule. The
timeline shows a warning when a draft is below the smallest draft of the
rule. The timeline also shows a warning when a wall is outside the walls
of the rule.

A boss, a snap fit and a rest grow out of the face from the point you
click. You can type the coordinates of the point. MitCad measures the
sizes at the face. A draft makes the walls narrower from the face. A lip
or a groove must have a flat face around one opening. MitCad refuses a
face whose inner edges are the feet of features that stand on it. The
model's definitions are in
[commands.md](../core/model-api/src/commands.md#plastic-part-features).

### Placing a feature

Box, Cylinder, Sphere, Torus, Coil, Hole, Gear, Pulley and Bearing are
placed by what you select in the view, in **Place On**. You never type
coordinates. Until you select something, the feature is not placed and
there is nothing to see.

- **A shaft or a hole:** the end edge (a round edge) or the side face.
  The feature goes on the shaft, or into the hole, along its axis. When you
  select the side face, the end nearer to your click is the end the
  **Offset** is measured from. The shaft can be a body of another
  component, for example a shaft from Build or a component from the library:
  select it where you see it.
- **An axis:** an origin axis, a construction axis, or a sketch line,
  circle or arc (its center, square to the sketch).
- **A point:** a sketch point, a vertex or a construction point. For a
  vertex or a construction point, the **Plane** row shows the plane the
  feature lies on: the origin plane that faces you most. Select another
  plane or a flat face there to use it.
- **A face or a plane:** click where the feature goes. The click snaps to
  the corners of the face, the centers of its circles and the middles of
  its edges. Two dimensions in the view measure the point from the two
  nearest straight edges of the face (**Distance 1** and **Distance 2**
  from **Reference 1** and **Reference 2**). On an origin plane they
  measure from the origin axes. To measure from another edge, click that
  edge; it replaces the reference that runs in the same direction. Drag
  a dimension, or click its value in the view and type a new one. Tab
  goes to the next dimension. In the panel, each reference and its
  distance are on one line.

**Offset**, **Flip**, **Angle** and **Align to** are under **More**,
so that the sizes of the feature stay in sight. They show without More when
you use them, and their handles are always in the view. **Offset** moves
the feature along the axis, and **Flip** turns it round. Where the turn of
the feature matters (a box, the teeth of a gear, a keyway), **Angle** turns
it about its axis: drag the ring, or click its value in the view. The
view draws the line the angle is measured from. On a face, this line is
Reference 1, so a box lines up with that edge and grows into the face.
**Align to** selects another edge, or a flat face, for the line; empty,
it says *Reference 1*, and clearing it returns to Reference 1. On an
origin plane the line is the plane's own X axis, Reference 2; when you
then pick another line for a reference, the X axis stays in Align to,
so the feature does not turn. On a shaft, an axis or a point, Angle is
there only once you select an edge or a flat face in **Align to**: the angle is measured from it, and a keyway or the first set
screw lies at that angle. A shaft with a flat is aligned to the flat at
once, so a set screw can be turned onto it straight away. A value of 0
shows in the view only when the mouse is on its handle. When a value box
has to move away from its dimension (off another box or a panel), a thin
line leads back to it.

When you first select where a feature goes, the view moves the feature out
from under the panel if it is hidden there.

A field that says *Auto* when it is empty is worked out by MitCad. The
gray text in the field shows the value it uses. Type a value to use
your own, or clear the field to go back to Auto.

The feature keeps what you selected, not coordinates. When the shaft gets
longer, the sketch moves or the face moves, the feature follows.

A feature made in an earlier version with a plane and a center opens on that
plane, with its distances from the origin axes. If you do not change the
placement, it is saved as it was. If you change it, it is saved with the
new placement. Moving it on the plane does not turn it: it keeps the
plane's own direction until you select a reference or **Align to**.

Some features lie on a **fixed plane**: a plane given by a point and a
direction rather than by a face or a construction plane, as an import or
an AI assistant may make them. **Place On** shows such a plane by where it
lies, for example *Plane at Z 36 mm (fixed)*, and the view draws it. You
edit the sizes and the other values as usual, and the dimensions in the
view move the feature on the plane. The plane stays as it is, and it does
not follow the model. To attach the feature to the model, select a face or
a plane in **Place On**: it replaces the fixed plane.

### Gears

**Gear** (Solid tab, Create › Machine Elements) makes a gear with
involute teeth from the values you give. Its **Type** selects the gear:

- **Spur:** straight teeth.
- **Helical:** teeth along a helix of the **Helix Angle**. The helix is
  right-handed unless you set **Left-Handed**. The module of a helical
  gear is the normal module.
- **Herringbone:** two helical halves of opposite hands. An optional
  **Gap** lies between the halves. MitCad cuts the gap a quarter module
  below the roots.
- **Ring:** an internal gear. Its teeth are inside a rim of the **Rim
  Thickness**. The teeth are straight or helical.
- **Rack:** the teeth in a row. The rack is **Height** high from its
  back to its tips. Its **Length** is *Auto* when you leave it empty:
  the pitch line, the number of teeth times the pitch. A larger Length
  makes a longer bar. The
  teeth are straight or helical.
- **Bevel:** a straight bevel gear for a mate of **Mate Teeth** at the
  **Shaft Angle** (90 degrees). **Make the Pair** also makes the mate,
  put in position to mesh.

Every gear has these values:

| Value | Meaning |
| --- | --- |
| **Module** | The module of the gear |
| **Teeth** | The number of teeth |
| **Pressure Angle** | 20 degrees, 14.5 degrees or 25 degrees |
| **Face Width** | The width of the gear |
| **Backlash** (under **Profile**) | Each tooth is thinner by half of it. Thus two gears with the same value mesh with that backlash |
| **Profile Shift** (under **Profile**) | In modules, away from the axis |

The values of a gear are parameters. You can type expressions, and you
can change the values later in the parameters. A spur, helical,
herringbone or bevel gear can have a **Bore** and a **Hub**. The bore is
plain, or it has an optional **Keyway** of DIN 6885 for its diameter, or
it has a thread size. For a thread size, the bore is the tap drill of
the thread, shown as a cosmetic thread. The hub is on the end face of
the gear. For a bevel gear, the hub is on its back. Its **Hub
Diameter** is *Auto* when you leave it empty: the bore with a 2 mm wall
all round.

**Place On** puts the gear where it goes (see [Placing a
feature](#placing-a-feature)), and **Angle** turns its teeth. On a shaft, the plain bore of a spur, helical,
herringbone or bevel gear has the diameter of the shaft plus the **Bore
Clearance**, and it follows the shaft. A rack and a bevel pair go on a
face or a plane. A rack runs along Reference 1 of the face.

**Mesh With** a gear (one of its faces, or the feature in the timeline)
turns the new gear so that the teeth engage. The gear stays where you
placed it; the panel warns if its axis is not at the center distance of
the mesh. A gear that meshes with a rack lies along the rack at its
**Position** from the middle of the rack's teeth. To make two gears that
fit between two shafts, use [Gear Pair](#gear-pair-and-belt-drive),
next to Gear in the menu.

A gear made in an earlier version at a **Mesh Angle** opens placed where
it is, meshing with its gear. If you do not move it, it is saved as it
was. A gear made in an earlier version beside its gear in a direction
shows that direction in **Toward**.

Gears that mesh share the module and the pressure angle. External
helical gears that mesh have opposite hands. A ring and its pinion have
the same hand. A rack and its pinion have the same hand.

The panel shows the results of the values:

- The pitch diameter, the outside diameter and the root diameter
- The inside diameter of a ring
- The pitch line of a rack
- The cone distance and the pitch angle of a bevel gear
- The center distance of a mesh
- If few teeth are undercut.

Small tooth counts are undercut, below 17 teeth at 20 degrees. The
undercut is the same as when a rack cuts the teeth: the root fillet cuts
into the flank.

To let meshed gears turn together, do these steps:

1. Make each gear a component. Set **New Component** as the operation.
2. Give each gear a joint to its frame: a revolute joint on its bore, or
   a slider for a rack.
3. Press OK on a gear that meshes with another gear. If both gears have
   a joint, MitCad links their joints with a
   [motion link](#motion-links). The motion link is a gear link, an
   internal link or a rack and pinion link.
4. If the joints came after the gears, edit the gear that meshes and
   press OK. MitCad then links the gears.

A gear exports as any body (STL, 3MF, STEP). The design keeps the
definition of the gear. The model's definition is in
[commands.md](../core/model-api/src/commands.md#gears).

### Pulleys

**Pulley** (Solid tab, Create › Machine Elements, after Gear) makes a
timing belt pulley from the values you give. Its **Type** selects the
pulley:

- **Toothed:** a pulley with teeth for a timing belt.
- **Idler:** a smooth wheel of a **Diameter**. The back of the belt runs
  on it.
- **Toothed Idler:** teeth on a bearing bore.

The **Belt Profile** is GT2 2 mm, GT2 3 mm, HTD 3M, HTD 5M, T5 or XL.
MitCad makes the grooves of the profile from its published dimensions.
The pitch diameter is the number of teeth times the pitch, divided by
pi. The belt's pitch line runs on this diameter. The outside diameter
is smaller by twice the pitch line offset of the profile. HTD 5M needs
at least 12 teeth. The other profiles need at least 10 teeth.

Every pulley has these values:

| Value | Meaning |
| --- | --- |
| **Teeth** | The number of teeth (not for an idler) |
| **Belt Width** | The width of the belt |
| **Belt Clearance** | Added to the belt width. The toothed part is this much wider than the belt (1 mm) |
| **Pitch Correction** | Added to the diameter of the teeth. A negative value gives a looser fit of the belt |
| **Flanges** | None, One or Both, with their **Flange Diameter** (*Auto*: the outside diameter plus the depth of the grooves and 1 mm all round) and **Flange Thickness**. One flange is on the side away from the hub. The flanges are larger than the outside diameter |

The values of a pulley are parameters. You can type expressions, and
you can change the values later in the parameters.

A pulley can have a **Bore** and a **Hub**. The bore is plain, or it has
an optional **Keyway** of DIN 6885 for its diameter, or it has a thread
size. For a thread size, the bore is the tap drill of the thread, shown
as a cosmetic thread. The bore goes through the whole pulley. The hub is
on the side away from the plane. Its **Hub Diameter** is *Auto* when
you leave it empty: the bore with a wall all round. **Set Screws** are
1 to 4 tap drills through the hub, 90 degrees apart. Their **Set Screw
Position** is the distance from the outer end of the hub; *Auto* puts
them in the middle of the hub.

**Place On** puts the pulley where it goes, its axis along the axis or
the normal of what you select (see [Placing a feature](#placing-a-feature)). With a keyway or set screws,
the pulley turns from an edge: on a face, Reference 1; on a shaft, an
axis or a point, the edge or flat you select in **Align to** (a
shaft's flat is taken at once). On an origin plane the edge is the
plane's X axis (Reference 2). **Angle** then
turns the keyway and the first set screw from that edge.

**Check Belt**, right under Place On, checks the belt round this pulley
and another one. Select the other pulley: one of its faces, or the
feature in the timeline. Under it, a word in bold says whether the
nearest belt **fits** as the pulleys are, **needs a tensioner** (it is
longer: the axes move apart, or a tensioner takes up the slack), or is
a **belt too short** (the axes must move closer). The lines under it
show the belt round both as the pulleys are placed, to 0.1 mm:

- **Belt length:** the length of the belt at its pitch line, its number
  of teeth, and the teeth in mesh on the smaller pulley.
- **Nearest belt:** the nearest belt you can get, for example *GT2 200
  mm, 6 mm wide*. **Belts** selects where it comes from: the **Library**
  (the closed belts of the component library; for profiles without them,
  every whole number of teeth) or **Whole Teeth** (every whole number
  of teeth).
- **Travel:** how far the axes must move to fit that belt: *apart* or
  *closer*. This is the tension travel your design must allow.

The view draws the belt's pitch line round both pulleys. If the pulleys
are not in one plane, a fourth line says so. The two pulleys must have
the same belt profile and parallel axes. Nothing is made: the pulleys
stay where you placed them. The pulley keeps the check: when you edit it
later, Check Belt still shows the belt. To make two pulleys of a ratio
between two shafts, use [Belt Drive](#gear-pair-and-belt-drive), next to
Pulley in the menu.

A pulley made in an earlier version with **Belt With** opens placed where
it is, with the other pulley as its Check Belt. If you do not move it or
change Check Belt, it is saved as it was.

#### A pulley on a shaft

**Place On** a shaft puts the pulley on it. Select the end edge of the
shaft (a round edge) or its side face. The shaft can be a body of
another component, for example a shaft from Build or a component from the
library: select it where you see it. Then:

- **Offset** is the distance along the shaft from its end to where the
  pulley starts (its first flange). The pulley runs along the shaft from
  there. When you select the side face, the offset is measured from the
  end of the shaft nearer to your click.
- **Flip** measures from the other end of the shaft, and turns the
  pulley to face back along the shaft.
- The plain bore has the diameter of the shaft plus the **Bore
  Clearance** (0.2 mm: the printed-fit parameter
  *shaft_bore_clearance*). The panel shows the bore, and the bore follows when the
  shaft becomes thicker or thinner. A **Keyway** goes into this bore. A
  thread size gives the tap drill of the thread as before.

The pulley follows the shaft when the shaft or its component moves. If
the shaft is too thick for the teeth (the bore must stay 0.5 mm inside
the grooves), the pulley fails with that message. Gears and bearings go
on shafts the same way.

The panel shows the results of the values:

- The pitch diameter and the outside diameter
- The bore from the shaft, on a shaft
- With Check Belt: the belt, in the three lines under Check Belt

The result of a pulley keeps a mount frame `bore` on its axis, as a
pulley from the library has. Joints can use this frame. MitCad makes no
belt body. A pulley exports as any body (STL, 3MF, STEP). The model's
definition is in
[commands.md](../core/model-api/src/commands.md#pulleys).

### Gear Pair and Belt Drive

**Gear Pair** and **Belt Drive** (Solid tab, Create › Machine Elements)
make two bodies that work together between two shafts or axes:

1. Select the **First Axis** and the **Second Axis**: the end edges or
   the side faces of two shafts, holes, construction or origin axes, or
   sketch lines, circles or arcs. The axes must be parallel. Under
   **More**, each axis has an **Offset** along it, so that both bodies
   lie in one plane (its handle is in the view all the time). If they
   do not, the panel says so. Each body grows from its axis in the
   direction of that axis: an end edge points into its shaft, a side
   face from the end nearer your click. If the two axes point opposite
   ways, the bodies would grow apart, and the panel says so too: select
   one shaft nearer its other end.
   A shaft can be in another component, where the view shows it (for
   example a motor placed in the assembly): the bodies go where that
   shaft is. The second gear of a Gear Pair meshes with the first, so
   its shaft must be in the active component; select a shaft of
   another component as the First Axis.
2. Type the **Ratio**, for example *1 : 2*: the teeth of the first
   part to the teeth of the second.
3. For a belt drive, select the **Belt Profile**. For gears, **More**
   shows the **Type** (spur or helical, or both), the **Pressure Angle**
   and the **Modules**, if you want only some of them.
4. The panel lists what fits, the best first. Gears: the module, the
   teeth, under **Ratio** the deviation from the ratio you typed (only
   when a pair has one), and under **Center distance** how the pair fits
   the distance between the axes (*exact*,
   *exact, corrected* when a gear is shifted only against undercut,
   *shifted*, or the helix angle). The view shows the chosen pair as a
   preview, both bodies in sight. A warning icon in the teeth marks a
   pair with an undercut gear; the heading of the last column gives
   the distance between the axes. Belt drives: the teeth, the deviation
   from the ratio (only when a row has one), the nearest belt and its
   tension travel; under the list, in Check Belt's words, whether that
   belt fits, the nearest belt and the travel, and the view draws the
   belt. Click a row to see it in the view, or use the arrow keys. The
   note at the bottom gives a gear pair's profile shifts and an undercut,
   or a belt drive's belt length and teeth in mesh; its center distance
   is in the heading of the Travel column.
5. Set the **Face Width** or the **Belt Width**, then press OK. MitCad
   makes both bodies on their axes. On a shaft, the bore has the
   diameter of the shaft plus the **Bore Clearance**.

The second gear meshes with the first. Made as components with joints
on their bores, the gears are linked as for [Gear](#gears).

### Printable bearings

**Bearing** (Solid tab, Create group, Machine Elements, after Pulley)
makes a ball bearing that you print on a 3D printer. The printer makes
the bearing in one piece: the inner race, the outer race and the
rolling elements come off the printer together, and they turn at once.
Each of them is a body of its own, and there is a clearance between
them, so they do not touch. For a bearing that you buy, use a component from
a library instead.

The **Element** selects the rolling elements:

- **Dumbbell** (the default): a roller with a narrow waist. Each race
  has a ridge that goes into the waist. The roller touches each race
  on two sloped faces, so it centers itself and does not jam. This
  element prints well.
- **Roller:** a cylinder with chamfered ends. The races have lips over
  the chamfers.
- **Ball:** a printed ball. Its top and bottom are cones, so that it
  stands on the print bed and prints without supports. Small balls
  stick to the races or rattle: below 3 mm the panel shows an error.
- **Airsoft:** bought plastic airsoft balls of 6 mm (5.95 mm as the
  default diameter, which you can change). A printed cage holds the
  balls apart. You put the balls in by hand: push the inner race to one
  side against the outer race, put the balls into the wide gap, move
  them round evenly, and press the cage in from the top face. The cage
  is a body of its own; print it on its flat ring.

The **Size** is a standard size: the 60, 62, 63, 68, 69 and 160 series,
the miniature MR sizes, the inch R sizes, and the flanged F and MF
sizes. A size sets the **Bore**, the **Outside Diameter** and the
**Width**. With *Custom* you type these values. A **Flange** on the outer
race is optional; a flanged size has its own flange.

The print values have defaults for FDM printers:

| Value | Meaning |
| --- | --- |
| **Radial Clearance** | The gap across the round faces between the elements and the races (0.4 mm) |
| **Axial Clearance** | The gap along the axis at the sloped faces (0.4 mm) |
| **Element Gap** | The smallest gap between two elements. *Auto*: 0.6 mm (with airsoft balls, room for the cage) |
| **Elements** | The number of elements. *Auto*: the most that fit |
| **Element Diameter** | The elements' diameter. *Auto*: the largest that leaves the races their walls |
| **Chamfer** | The chamfers at the bore and the outside (0.4 mm) |
| **Smallest Wall** | The thinnest wall of a race, an element or the cage (1.2 mm) |
| **Flank Angle** | The slope of the sloped faces from the axis (45 degrees). It is also their overhang when you print with the axis vertical |

The panel shows the result of the values: the pitch diameter, the
elements, the walls and the smallest gap. If the values do not print,
the panel shows an error and MitCad changes nothing by itself. For
example: too many elements for the pitch circle, a wall thinner than
the smallest wall, a clearance below 0.2 mm, an overhang steeper than
45 degrees, a ball that is too small, or airsoft balls that do not fit
the section.

**Place On** puts the bearing where it goes, its axis along the axis or
the normal of what you select (see [Placing a feature](#placing-a-feature)).

**Place On** a shaft or a hole puts the bearing on the shaft or into the
hole. Select the end
edge or the round side face of a shaft, or the rim or the wall of a
hole. On a shaft, MitCad selects the standard size whose bore fits the
shaft; in a hole, the size whose outside diameter fits the hole. The
size fits when its diameter is within the **Fit Clearance** (0.5 mm) of
the shaft or the hole. You can select another size after that. On a
shaft, the **Offset** is the distance from the end of the shaft; in a
hole, it is the depth (with 0, the bearing is flush with the face).
**Flip** measures from the other end. On a side face or a wall, the
end nearer to your click counts. When the shaft or the hole changes,
the bearing follows it.

**Groove Into** cuts the
raceway into a round face of a body instead, for example the bore of a
pulley or a wheel, or the outside of a shaft. That body is then one race.
MitCad makes the other race as a new body, in the same component, and
fills the gap with the elements. **Offset** is the distance along the
face from its end; with 0, the bearing is in the middle of the face.

With **New Component** (the default) the bearing is a component of its
own, named after its size, for example `Bearing 608 (8x22x7)`. With **New
Body** the bodies go into the active component. The result has the
mount frames `bore` (the axis), `face_a` and `face_b` (the two faces)
for joints.

**Print a bearing with 3MF.** In **Send to Slicer**, select the bodies of the
bearing and set **Format** to *3MF*: the slicer
then gets one object in one piece. STL writes one file for each body,
and the slicer must put the parts back together. With airsoft balls,
leave the balls out: you buy them. The **Bill of Materials** shows the balls to
buy, `Airsoft ball 6 mm`, with the number for all the bearings.

The model's definition is in
[commands.md](../core/model-api/src/commands.md#printable-bearings).

### Boundary Fill

**Boundary Fill** makes solids of the spaces that bodies enclose
together. The tool bodies divide space into cells. Each cell lies inside
some of the tools and outside the others. Examples of a cell are the
space that two bodies share where they overlap, and the part of one body that
another body does not reach. You select a cell by a point inside it. You
can select as many cells as you want. When the tools cut the space of a
cell apart, the cell is several separate pieces. MitCad fills the whole
cell.

The operations of Boundary Fill are:

- **New Body** makes a body of each cell, or puts the bodies in a new
  component.
- **Join**, **Cut** and **Intersect** use the cells together as one tool
  on the bodies you select. When you select no bodies, the operation
  applies to all bodies. This is the same as an extrusion. For example,
  cut the space that two bodies share out of a third body, or add that
  space to one body.

**Remove tools** takes the tool bodies out of the design. Otherwise the
tool bodies stay as they were.

The faces of the cells keep the names of the tool faces they lie on.
Thus fillets, sketches and other features on the faces of a cell follow
when the tools change.

If a point lies outside every tool, the feature fails with a message
that says so. If a point lies on the surface of a tool, the feature also
fails with a message that says so. Two points in one cell fill that
cell once.

Note: Boundary Fill has no panel yet. You make it through the model's
commands: scripts, `mitcad-cli run` or MCP
([commands.md](../core/model-api/src/commands.md#boundary-fill)). The
`.solidcraft` import also makes it. You edit, suppress and undo it in
the timeline like any feature.

### Bounding Solid

A **Bounding Solid** is a new body: the box around bodies. The box
stands off the bodies by a margin on every side. Uses are stock sizes,
packaging, clearance envelopes, and a cut that must clear a whole body.

Name the bodies and the margin. If you name no bodies, the box holds
every body of the component at that point of the timeline. A margin of
zero gives the bodies' own box. A negative margin makes the box smaller.
The box is square to the axes of the component. The box holds the exact
shapes of the bodies, round faces included.

A Bounding Solid is a feature like any other. If you change a dimension
of a body in the box, or the margin, the box follows. The bodies
themselves stay as they are. The faces of the box have the names of a
box's faces: bottom, top and four sides. Thus later features keep their
references when the box grows or becomes smaller.

Note: Bounding Solid has no panel yet. You make it through the model's
commands: scripts, `mitcad-cli run` or MCP
([`bounding_solid`](../core/model-api/src/commands.md#bounding_solid)).
The `.solidcraft` import also makes it.

### Envelope

An **Envelope** is a new body: a light outer skin of bodies. Use it as a
placeholder in a layout: a machine of many parts becomes one body of
flat faces in the shape of its outside, not a box.

The **detail size** says what the skin keeps. Gaps, holes and features
smaller than it are closed over. Openings larger than it stay open, for
example a tunnel through a machine where the product goes, or the space
under a table. Spaces that are closed in on every side are filled. If
you give no detail size, MitCad uses 1 % of the diagonal of the bodies,
but at least 5 mm.

Name what the envelope wraps:

- nothing: every body of the component at that point of the timeline,
  with the bodies of the components that are placed in it;
- an occurrence: the bodies of its component (also of a linked design),
  with the components placed in it, where the occurrence puts them;
- bodies: only these bodies (of the component or of the occurrence's
  component).

The skin is made of flat faces that are square to the axes of the
component. It is outside the bodies, at most about half the detail size
from them. Slanted and round faces become steps. The new body has the
appearance of the first body of the source that has one, and its name is
the name of the feature (Envelope1). The bodies query marks it as an
envelope.

An Envelope is a feature like any other. If you change the bodies, the
occurrence or the detail size, the envelope follows. The bodies
themselves stay as they are.

To make an Envelope, choose **Envelope** in the SOLID tab's CREATE menu,
after Mirror. In **Source**, pick a component in the browser, or pick
bodies of one occurrence. If you pick nothing, the envelope wraps the
active component with the components placed in it. Leave **Automatic
Detail Size** on for the automatic size, or turn it off and type a
**Detail Size**. The view shows the envelope as you change the inputs.
Click **OK**. To change an envelope, double-click it in the timeline: the
same panel opens. Scripts, `mitcad-cli run` and MCP make it too
([`envelope`](../core/model-api/src/commands.md#envelope)). To show a
linked design by its envelope, use **Show as Envelope…** in the browser
([Components](#components)).

If you copy a component that has an Envelope of an occurrence in it, the
copy's Envelope wraps the copy's occurrence. If you move an occurrence to
another component (Move to…), an Envelope of it follows. MitCad does not
move an occurrence out of the component that has its Envelope.

### Convert Mesh

**Convert Mesh** changes a mesh body from an STL or OBJ file into a solid
that every feature can use: you can fillet it, cut it, shell it or join
other bodies to it. Use it to change a printed part that you downloaded
as an STL file. It is in the Modify group of the Solid tab. A mesh body's
context menu (in the view and in the browser) also has Convert Mesh at
its top.

Pick the mesh body in the view or in the browser, select the **Type**,
and click OK.

- **Faceted Solid** (the default) works with every closed mesh. Each
  triangle, or each group of neighbouring triangles that lie in one
  plane, becomes a flat face of a new solid body. The mesh body stays in
  the design but is hidden. Show it again with its light bulb.
  - **Weld Tolerance** (0.01 mm by default): corners closer together
    than this become one corner. This closes the small gaps that some
    files have between their triangles.
  - **Merge Angle** (0.5° by default): two neighbouring triangles become
    one flat face when the angle between them is less than this, and
    their corners lie within the weld tolerance of one plane.
  - The mesh must be closed: every edge must be on exactly two
    triangles. Otherwise MitCad refuses it and the panel says how many
    open edges (edges of one triangle only) and non-manifold edges
    (edges of more than two triangles) it has. Repair the mesh in a mesh
    tool, or try a larger weld tolerance. A mesh with a closed shell
    inside another (a hollow part) is refused too.
  - The mesh can have at most 500 000 triangles; above that the panel
    says how many it has. A typical printed part has a few hundred
    thousand triangles or fewer.
  - Round surfaces stay facets: a cylinder of 64 facets becomes 64 flat
    faces.
- **Analytic** works only with a mesh of one profile extruded straight,
  such as a plate, a bracket, a bar or a profile with holes. It changes
  the mesh body itself into a solid with flat and round faces: a hole or
  a rod with 12 or more corners on one circle becomes one round face.
  - The **Tolerance** (0.000001 to 1 mm) is the largest distance that
    the new faces can lie from the triangles. A hole or a rod becomes
    round when each of its facets lies within the tolerance of the
    circle; otherwise it stays flat facets. Straight runs of corners
    become one flat face.
  - The mesh must be closed, every triangle must face out, and it can
    have at most 50 000 triangles. MitCad refuses a mesh that is not one
    profile extruded straight (a tapered, twisted or stepped body, a
    sphere, a freeform scan) and converts nothing.
  - The body keeps its name and its color, and it stays where it was. If
    you suppress or delete the feature, the mesh body comes back.
  - The new faces have names like the faces of an extrusion: by the
    edges of the section in turn
    ([commands.md](../core/model-api/src/commands.md#convert-mesh)).

**Insert Mesh** can convert the mesh at once: select **Convert to
solid** in its dialog. It uses Faceted Solid with the weld tolerance and
the merge angle that you type there; MitCad remembers the choice. The
import and the conversion are one undo step. If MitCad refuses the
conversion, the mesh body stays as it came in, and the status line says
why.

Scripts, `mitcad-cli run` and MCP make the same feature
([`mesh_convert`](../core/model-api/src/commands.md#convert-mesh)), for
example `{"type": "mesh_convert", "body": "F3.b0", "mode": "faceted",
"tolerance": 0.01}`. The `.solidcraft` import makes analytic ones.

### Sheet metal

The Sheet Metal group holds the commands for sheet metal bodies: Face, Flange,
Bend, Fold, corners, cuts, reliefs, Unfold, Refold and the Flat Pattern
([app/COMMANDS.md](../app/COMMANDS.md#sheet-metal)). This section
describes the sheet rules, the Flat Pattern export, and the options of
Flange, Bend, Corner Relief and Sheet Cut. It also describes three sheet
features that have no panel yet.

**Edit Sheet Rules** edits the thickness, the bend radius, the K-factor
and the reliefs. Open it from the context menu of a sheet metal body, or
double-click the rules row under the sheet metal body in the browser. Every
sheet that uses those rules follows the change.

**Export Flat Pattern** writes the Flat Pattern of the selected sheet to
a DXF file. The file has the cut contours and the bend lines of the
sheet, in the design's length unit or in the unit you choose as the file
type ([The unit of a DXF file](#the-unit-of-a-dxf-file)).

The Length of a **Flange** is *Measured* from one of two lines. The
first is the tangent line of the bend. The second is the virtual sharp
corner of the inner or the outer surfaces. *Outer Tangent Length* measures
from the plane that touches the outside of the bend. The Angle can be
more than 180 degrees. A Length of 0 makes the bend alone, without a wall:
a rolled hem.

**Hem** makes the flange a Teardrop, Rope or Double hem. Type the hem's
**Gap**, and for a double hem its **Setback**. The bends then follow from
the gap, the Length and the bend radius. Flip turns the hem to the other
side of the face.

**Width** sets how much of the edge the flange covers. *Whole Edge* covers
all of it. *Offsets from the Ends* leaves the Start Offset and the End
Offset free. *From the Middle* covers **Before the Middle** and **After
the Middle** from the middle of the edge. This width can pass the ends of
the edge. **Own Overhang Root** sets how far the wall past the ends is cut
back; 0 starts it at the end of the bend.

When a Position other than *Edge* sets the bend back into the face and a
side of the face meets the edge at another angle than 90 degrees, an end
without an offset follows that side: the side runs on to the bend at its
own angle, and the bend and the wall reach from side to side there. A
flange so comes out wider than the edge where a side leans outwards and
narrower where it leans inwards.

**Corner** is for a flange that closes against another wall of the
sheet, such as the last side of a box section that is welded. Type the
inside dimension as the Length; the corner sets where the wall ends.
*Open* ends the wall on the inside of the other wall: the walls meet
only along the corner line. *Half Open* lets the wall reach half the
thickness over the end of the other wall. The other wall is then cut
back by a seam of 0.00001 mm under it, as walls that touch over a face
cannot make one solid. Use Corner on the flange that closes the section,
the last one.

The **Profile** row draws the wall with a sketch instead of a rectangle,
for a wall that is slanted, tapered, curved or wider than the edge.
Click **Edit Profile**: a sketch opens on the wall where the flange puts
it, with the rectangle of the Length and Width. Its bottom line lies on
the bend line and is fixed there; that line is the bend. Change the other
lines as in any sketch: slant or taper them, round them, take them past
the ends of the edge, and constrain them to other geometry of the design.
**Finish Sketch** brings you back to the Flange panel, and OK adds the
flange. The sketch stays in the timeline before the flange: edit it later
and the flange and its flat pattern follow. The wall cannot cross the bend
line. With a profile the Length, Width, Hem and Corner rows are hidden.
If you change the flange's Angle or Position later, Edit Profile puts the
sketch back on the wall. **Remove Profile** makes the wall a rectangle
again.

**Bend** makes one bend between two edges. With the Bend Type *Double
Bend*, it makes two opposite bends of the angle with a straight between
them. A double bend joins parallel faces of two sheets.

**Fold** bends a sheet along a sketch line on one of its faces. A fold
cannot bend part of an earlier bend. Suppose an earlier fold's line
crossed both arms of a U- or V-shaped face, and a new fold line runs
between the arms. The earlier bend then joins both sides of the new line,
and MitCad refuses the fold. The message names the earlier feature as the
browser shows it, for example: "This fold would also have to bend
SheetFold1, whose bend joins both sides of the fold line." Move the new
fold before SheetFold1 in the timeline, or make SheetFold1 with a separate
line on each arm. A Bend whose moving face an earlier bend also joins to
the stationary face is refused the same way.

**Corner Relief** takes faces of the two bends that meet. When the bends
leave several flat faces, it also takes the Face they leave. For bends
on different faces, it takes the face held, that is, a face next to
either bend.

**Sheet Cut** cuts sketch profiles through the thickness. The profiles
can come from one or several sketches on a flat face. Sheet Cut also
cuts across bends. It opens the bends for the cut.

An ordinary cut (Extrude, Hole) through a sheet also keeps it a sheet,
across bends too: the sheet still unfolds, refolds and has a Flat
Pattern with the cut in it. In a bend, MitCad makes the cut through the
thickness at right angles to the material, as Sheet Cut does. Where that
differs from the cut's own walls, the feature shows a warning. A feature
that adds material across a bend makes the body an ordinary solid; the
sheet tools after it and the Flat Pattern row in the Browser say which
feature did so.

Three sheet features have no panel yet. You make them through the
model's commands (scripts, `mitcad-cli run`, MCP) and by the
`.solidcraft` import:

- **Contour Flange** bends a sheet along an open chain of sketch lines.
  The lines are one surface of the sheet. Every corner becomes a bend of
  the radius of the rules. The sheet is as wide as you say along the
  normal of the sketch. Select the side on which the material lies and
  the direction of the width.
- **Convert to Sheet Metal** makes a body a sheet. A plate of the
  thickness of the rules becomes a sheet of one panel from the face you
  select. If you also select bend edges, the flat faces reached across
  those edges become panels bent at those edges. Thus a thin-walled
  channel or bracket keeps its walls, and a solid block becomes the
  sheet of its outer faces. MitCad refuses faces that meet at an edge
  that is not a bend.
- **Perforate** cuts a field of round holes over a whole sheet, on a
  staggered or a square grid. A margin keeps the holes clear of the
  edges, the cut-outs and the bends of the sheet. The holes are real:
  the Flat Pattern, its DXF file and every measurement have them.

All three features keep the body a sheet. The sheet unfolds, refolds and
has a Flat Pattern like any other sheet. The details are in
`sheet-metal.md` in the docs folder, a developer document.

**Mirror** and the patterns copy a sheet body as a sheet. A copy that is
a new body takes the sheet tools: Fold, Flange, Cut and the Flat Pattern
work on it as on the original. With **Join**, the copy and the original
become one sheet: where their flat panels meet in one plane, they are
one panel, and where a bend crosses the mirror plane, the bend and its
mirror image are one bend. The other bends stay bends. The Flat Pattern
unfolds the whole part. MitCad refuses the join when the two halves
overlap, do not touch, or meet only at a wall that crosses the mirror
plane at a slant. The message says which. **Combine** with Join does the
same when you join a sheet with its mirrored or patterned copy made as a
new body: the result is one sheet. Where the two sheets cannot be one
sheet, Combine still joins them, and the sheet state says why the result
is no longer a sheet.

This also holds for a sheet with an ordinary fillet or chamfer on it, for
a pattern of a pattern of the sheet body, and for **Join any**, where a
copy becomes one sheet with the one sheet it touches. Later features on a
joined copy keep finding the panels they were made on when you change
the sheet before the mirror or pattern. A scaled copy is not a sheet,
because scaling changes the thickness; MitCad warns about it, and also
when a join turns a sheet into an ordinary solid, for example a copy
that joins two sheets at once where one of them has a bend. Where such a
join leaves only flat material in one plane, the result is still a
sheet.

### Mortise and Tenon

**Mortise and Tenon** (Solid tab, Modify group) joins two bodies, for
example two boards that cross. It adds a tenon to the end of one body
and cuts the mortise for it into the other body. One feature changes
both bodies. The bodies keep their names.

1. Click the **End Face**: the flat face that the tenon stands on, for
   example the end of a rail.
2. Select the **Mortise Body**: the body that the tenon goes into, for
   example a post.
3. Set the size of the tenon:
   - **Width** goes along the longest straight edge of the end face.
     **Turned** puts the width across that edge.
   - **Thickness** goes across the width.
   - **Length** is how far the tenon goes out of the end face. With
     **Through**, the tenon goes to the far face of the mortise body
     and the mortise goes through the body.
   - **Offset Along Width** and **Offset Along Thickness** move the
     tenon from the middle of the end face. At 0 the tenon is in the
     middle. The parts of the end face around the tenon are the
     shoulders.
   - **Count** makes several tenons along the longest edge of the end
     face, for example to join two panels. Each tenon gets its own
     mortise, and all of them are one feature and one undo step. With a
     Count above 1, **Start Offset** is the distance from one end of the
     end face to the first tenon and **End Offset** the distance from
     the last tenon to the other end. MitCad spaces the tenons evenly
     between them. The offset along the longest edge is hidden then.
     Every tenon has the same size, length, clearance and corners.
4. Set the fit:
   - **Clearance** (default 0.1 mm) makes the mortise larger than the
     tenon on each side and at the bottom.
   - **Corners**: **Square**, or **Rounded** for a mortise that a
     router cuts. With Rounded, the corners of the mortise have the
     radius of the **Tool Diameter**. The edges of the tenon are
     rounded to match, so the clearance is the same all around.
5. Click OK.

When you change a size later, MitCad builds both bodies again. MitCad
refuses a tenon that is larger than the end face, a tenon that an
offset moves past the edge of the end face, and a tenon that does not
reach the mortise body. With several tenons, MitCad also refuses a
Start Offset and End Offset that leave no room for a tenon, and tenons
or mortises that overlap or touch. The timeline shows the reason. The model's
definition is in
[commands.md](../core/model-api/src/commands.md#mortise-and-tenon).

## Files and projects

MitCad saves a design as a `.mitcad` file. A design is a single file, or
it is part of a project. A **project** is a folder whose designs keep
their versions. This chapter tells how MitCad saves and recovers
designs, how projects and versions work, and how you share a project
with other people.

### Saving and recovery

A design is a `.mitcad` file. Outside a project, the design is a
single-file design: MitCad stores its imported bodies inside the file.
In a project, MitCad stores the imported bodies in the project's folder
instead (refer to [Versions](#versions)).

A `*` in the window title shows that the design has unsaved changes.
**New**, **Open** and the close of a design ask about unsaved changes
first. If another program changed the file after you opened it, **Save**
asks before it writes over the file. If you open a file that is not a
valid design, MitCad names the place of the error.

### Several designs at once

New Design and Open keep your current design open. They reuse an empty,
unchanged Untitled design, and otherwise add another document. Opening a
file already open brings its document forward. Every document keeps its
own selection, running command, undo history and save questions.

**Window › New Window** (Ctrl+Shift+N, Cmd+Shift+N on macOS) opens a separate
window. The Window menu also lists the open designs, checks the active
one and offers **Minimize**. **Close** closes just that document; with only
one design open it starts an empty Untitled design as before. **Quit**
asks about changed designs one by one; Cancel stops quitting. Closing the
last document ends MitCad.

In **Preferences › General**, **Open in the current window** restores the
previous replacement behavior. **Open designs in: Tabs / Windows** chooses
how new documents appear on Windows and Linux, with Tabs as the default.
The tab bar shows only while two or more designs share a window: with one
design the window looks as it does without tabs, the tabs appear below the
menu bar when a second design opens there, and they go again when one
design is left.

**A design from a newer MitCad.** A newer MitCad can save things into a
design that this MitCad does not know, for example a future note format.
MitCad opens such a design without them, and a warning names what it did
not read: "This design was written by a newer MitCad: future_notes, next_future_note
not read." MitCad cannot write these things back. So before **Save**
writes over the file, MitCad asks:

- **Save and Drop Them** saves over the file. The newer MitCad then no
  longer finds the things that this MitCad did not read.
- **Save As…** saves the design to a new file. The old file stays as it
  is, and the newer MitCad still finds everything in it.
- **Cancel** saves nothing. The file stays as it is.

A save to another file does not ask. `mitcad-cli` saves over such a file
without a question and writes a warning. An agent's `document_save`
(MCP) does not save over such a file: it is refused, and you save the
design in MitCad yourself.

**Autosave** writes the unsaved changes every 5 minutes to a recovery
folder in your user data. It never writes next to your design. The
settings are on the General page of **Tools › Preferences**:

- Autosave on or off
- The interval, 1 to 60 minutes
- **Open Folder**, which opens the recovery folder

When you save, when you close the design and when MitCad ends normally,
MitCad removes the autosaved design. After a crash, MitCad offers the
autosaved designs at the next start. **Recover Unsaved Work** lists
the designs with the time that MitCad autosaved them. The dialog has
three buttons:

- **Restore** opens each selected design in its own document window, as
  the file it came from. All healthy designs are selected initially. Their
  changes are unsaved, and they have no undo history. An empty initial
  window is reused for the first design; an open design is kept.
- **Discard** deletes the selected designs.
- **Later** keeps the designs for the next start, or for **File ›
  Recover Designs**.

If you select **Discard**, MitCad deletes the selected autosaved
designs. You cannot get them back.

### Versions

A **project** is a folder whose designs keep their versions. A project
is **Local** or **Cloud**. In a Local project, the versions are on this
computer. In a Cloud project, the versions are also in a git repository
that other people share (refer to [Sharing a
project](#sharing-a-project)).

Every **Save** (Ctrl+S) of a design in a project records a version.
MitCad names the version after what you did since the last save, for
example `Save bracket.mitcad: Add Fillet1, Change d3`. **File › Save
with Description…** (Ctrl+Alt+S) records a version with a description
that you type. A save without a change records no version. A save after
a change of only the view (Origin folder shown, Isolate) records no
version. Autosave never records a version.

**New Project.** **File › Project › New Project…** makes a project. The
dialog has these fields:

- **Folder:** a full path. The default is Documents/Mitcad/Project1, or
  the first such folder that is free. If the folder is a project
  already, **Create** becomes **Open It**. Designs in the folder become
  the project's designs, in its first version. Other files stay in the
  folder, and MitCad does not version them. A folder inside another git
  repository gets a repository of its own, and a warning says so.
- If the folder already holds files or folders, a warning says that the
  folder and everything below it, its subfolders too, become the
  project. It also says how many designs go into the first version. To
  keep the folder as it is, use **Create in a New Subfolder** instead of
  Create. It makes the project in a new folder inside the chosen one,
  named in the **New subfolder** field (New Project by default).
- **Storage:** Local, or Cloud with the Cloud section (refer to [Sharing
  a project](#sharing-a-project)).
- **Author:** the name and the email address that every version records.
  MitCad fills them in from git's settings, or else from Preferences ›
  Cloud. MitCad keeps them in the project's repository. The email
  address is part of every version.

**Create** makes the folder, its repository and the project's first
version. The project starts empty, or with the designs that its folder
had. MitCad shows the project in the **Projects panel**, ready for **New
Design** (refer to [The Projects panel](#the-projects-panel)). If you
cancel, the folder stays as it was.

**Open Project.** **File › Project › Open Project…** opens a folder. If
the folder is a project, MitCad shows it in the Projects panel. Then it
opens the design that you opened last in that project, or else the
newest design. An empty project opens no design. An older project
without versions gets its versions at once. If the folder is a git
repository, a folder of designs or an empty folder, MitCad goes to New
Project with the folder filled in. A project inside another git
repository opens without versions.

**Open…** and **Open Recent** find the project of a design the same way.
Open Recent lists each design with its project, for example
`bracket.mitcad — Robot arm`.

**Open from Project.** **File › Open from Project…** (right after Open)
opens a design from your projects. You browse the projects as you browse
folders, in a window like a file dialog:

- On the left, the window lists the projects that the Projects panel
  lists, Local and Cloud, with their icons, and their folders. On the
  right, it lists the folder's sub-folders first. Then it lists the
  folder's designs with their version, their edit lock, their last
  change and who made the change. A double-click (or Enter) goes into a
  folder or opens a design. Backspace goes up one folder. The path bar
  above (`Robot arm › parts › brackets`) takes you back to any folder
  on the way.
- **Search** finds designs by a part of their name as you type. It
  searches in all projects, or only in the project shown (**In**). The
  results show their project and folder. Enter opens the first design
  found.
- **Read-only** opens the design as **Open Read-Only…** does, without
  its edit lock. **Another File…** opens the system's file dialog
  instead.
- Only designs are listed. A project's own settings (`.mitcad/`) and
  other files are left out. The window starts where you left it last
  time.

**The project indicator.** The project indicator shows the current
project in the status bar, where the version was. On a Mac, it is beside
the design's name in the title bar row. It shows, for example, `Robot
arm · Local · v14`, `Robot arm · Cloud · ✓` or `Not in a project`. A
Cloud project also shows `↑2` for versions to send, `↓1` for newer
versions, or `Offline, ↑2 waiting`. A click on the indicator opens its
menu: **Sync Now**, **Check for Newer Versions**, **Version History**,
**Project Settings**, **Open in Browser** and **New Design**. For a
design in no project, the menu has **Save to Project…**. It is also in
the File menu, right after Save As.

**Save to Project.** **File › Save to Project…** saves the design in a
project, as Save As does. The window then shows the file in the project.
The file that the design came from stays where it was. A new, untitled
design is saved in the project the first time. A dialog lists your
projects, Local and Cloud, with their folders. Select a project and a
folder in it. Then type the design's name. If the folder already has
that name, the dialog shows it and waits for another name.

**New Folder** in the dialog makes a folder in the selected project or
folder. The new folder shows in the list with its name selected, as New
Folder in the Projects panel does. Type its name and press Enter. Esc
takes the new folder away. If the name is taken or cannot be a folder's
name, the dialog says so below the name and waits for another. The new
folder is then selected, and Save puts the design in it. The folder is
recorded as a version of its own, so it stays even if you cancel the
dialog. The same button is in every dialog that saves a design in a
project: Save As, Save Component as Design and Save Copy As.

**New Project…** in the dialog makes a project first. By default, the
new project is in the design's own folder, when that folder is no
project's and no repository's. Else the new project is in a new folder
named after the design. **Another Folder…** takes a project that the
list does not have. Every save of the design in a project records a
version. Save with Description, Version History and Project Settings
offer Save to Project for a design in no project.

Note: Older versions of MitCad had **File › Project › Move to a
Project…**. It moved the design out of its folder. A shortcut that you
gave it now starts Save to Project.

**Project Settings.** **File › Project › Project Settings…** has these
settings:

- The project's display name
- The project's folder
- The project's storage, Local or Cloud
- On this computer: the author, if MitCad sends each version at once,
  and how often MitCad checks the remote. Preferences › Cloud gives the
  defaults.

The **Display name** is the name that the Projects panel shows for the
project on this computer. It never renames the folder or the repository.
If you clear it, the default name comes back: a Cloud project's
repository address such as `github.com/acme/gripper`, or a Local
project's folder name.

**Save when the file changed outside MitCad.** If the file changed
outside MitCad since you opened it, Save asks first. The change can come
from another program, another MitCad or git. Save offers **Save as New
Version**, **Save As** or **Compare**. Save as New Version records the
outside change as a version first. Thus both changes stay in the
history.

Sometimes MitCad cannot record the outside change: HEAD is detached, a
merge is in progress, or another program locked the repository's index.
Then Save stops with an error and leaves the file as it is. Save also
stops when MitCad cannot read the version status. Save As still keeps
your design in another file. If a design was renamed or moved in its
project outside MitCad, it keeps its view settings. Its next save
records the rename.

**Git.** A project is an ordinary git repository, so git tools work on
it. A design file that was moved out of its project folder has no
imported bodies. `mitcad-cli convert` makes a single-file design of it
again (refer to [Command line](#command-line)). The internals are in
[architecture.md](architecture.md).

**Version History.** **Version History** (File › Version History…,
Ctrl+Shift+H) lists the versions of the open design, newest first. Each
version shows when it was saved, who saved it, its description and what
it changed, for example `d3 20 mm -> 25 mm, 1 feature modified`. The
selected version shows its whole description, its id and a preview of
the view as it was saved on this computer. Below the list, MitCad
compares the selected version with the version before it, or with the
open design. The open design includes its unsaved changes. The
comparison lists parameters, timeline features, sketches, components and
bodies. **Compare Geometry** also compares the volumes and the areas of
the bodies.

Version History has these buttons:

- **Open** opens a version as a new, untitled design, for example
  "bracket v3". The file and its history stay as they are.
- **Restore** makes a version the latest again. It records the design of
  that version as a new version, for example `Restore v3 of
  bracket.mitcad (abc1234)`. The versions since then stay. MitCad never
  rewrites the history.
- **Save Copy As** writes a version to a file that you select.

Before Restore records the new version, MitCad saves your unsaved
changes as a version, or drops them if you select that. MitCad also
records a change made to the file outside MitCad as a version first. If
MitCad cannot record one of these as a version, Restore stops with an
error that says why. Then Restore writes nothing over the file. Unsaved
changes that Restore already saved stay in the file, without a version
yet.

### The Projects panel

The **Projects panel** (View › Projects) lists your projects, their
folders and their designs. It is on the left above the Browser. On a
Mac, it is a card above the Browser. It is hidden until you show it
(File › Open from Project opens a project's design without it); MitCad
then remembers whether you show or hide it.

```
github.com/acme/gripper     (cloud) ↑2
  finger                     v4 (lock)
Robot arm
  parts
    bracket                  v3*
  base                       v12
```

`(cloud)` and `(lock)` stand for small icons.

The panel starts shown when you have a project. You have a project when
you made, opened or cloned one here, or when one is in the projects
folder. The panel starts hidden when you have no project. When you show
or hide the panel yourself (View › Projects), MitCad remembers your
choice.

**Which projects.** The panel lists the projects that you made, opened
or cloned on this computer, newest first. It also lists the projects in
the projects folder (Documents/Mitcad). **Add Existing Project Folder…**
adds a project from another place. **Remove from List** takes a project
out of the panel and deletes nothing.

**Remove Project.** **Remove Project…** makes a project's folder an
ordinary folder again. It is on the project's context menu, in the
panel's Projects menu and in Project Settings. Your designs and other
files stay in the folder, but the project's version history goes. First
a question tells you what happens:

- The folder and its designs stay.
- If MitCad made the project's git repository, the repository and its
  versions are deleted. Older versions and deleted designs cannot be
  brought back.
- If the folder was a git repository before the project, it keeps its
  history. A new version takes the project's files out.
- For a Cloud project, only this computer's copy stops being a project.
  The repository on the server does not change.
- A design that keeps data in the project is saved as a single file
  first, so it opens without the project.
- Open designs of the project close. MitCad asks you to save changes
  first.

MitCad removes only its own files: the `.mitcad` folder, its lines in
`.gitattributes` and `.gitignore` (the files too when nothing else is in
them), and the `.git` folder when MitCad made it. After that, you can
make a project in a subfolder, or in the same folder again.

**Rows.** Each project is a row of its own. Only its folders and designs
are below it, sorted by name. A project shows the display name that you
gave it in Project Settings. Without a display name, a Cloud project
shows its repository's address, for example `github.com/acme/gripper`,
`gitlab.com/group/sub/part` or a shared folder's path. A Local project
shows its folder's name. The icon and the tooltip tell Local from Cloud.
The names get the width. The state beside them is in short marks, and
the tooltip says it in full. A Cloud project shows its sync state as a
cloud icon. The icon has a check when the project is in sync. It is
crossed out when the remote cannot be reached. `↑2` shows versions to
send, and `↓1` shows newer versions.

A design shows its latest version (`v3`). It shows a `*` when it differs
from that version (`v3*`). It shows `new` before its first version. It
shows a lock icon when someone edits it, you elsewhere included. It
shows a pencil when you edit it here. The open design is in bold. A name
too long for the panel is shortened in the middle (`mounting-br…rev-b`).
Its start and its end stay. Its tooltip has the whole name and the
path. Drag the panel's right edge to make the panel wider. MitCad
remembers the width in both layouts. The panel reads the projects again
after a save, after a sync and when MitCad gets the focus back. Thus
changes made with other programs show then.

**Open.** Open a design with a double-click or with Enter. If the open
design has unsaved changes, MitCad asks about them as with Open.

**New Design.** **New Design** adds a design to the selected folder of
the current project. It is in the toolbar, in the context menu, in
**File › New Design…** (Ctrl+N) and in the project indicator's menu.
With no current project, File › New Design opens an untitled design
instead. To add a design:

1. Select **New Design**.
2. Type the name of the design in the panel.
3. Press Enter.

MitCad records the design as a version (`Add parts/bracket.mitcad`) and
opens it. **New Folder** adds a folder the same way (`Add folder
parts`).

The panel proposes a free name, such as `Design4`, and selects it, so
that what you type replaces it. The proposed name is not used in the
folder, also not by a file that the panel does not list yet. If the
Projects panel is hidden or its card is collapsed, New Design shows it
while you type the name. After Enter or Esc, the panel is hidden or
collapsed again. If you hide the panel or collapse its card while the
name editor is open, MitCad does not take the name.

If a name is not possible, for example because a design or a folder of
that name is there already, the name editor stays open. A message below
the row says why. Type another name and press Enter, or press Esc to
stop. Rename works the same way.

**Rename and move.** **Rename** (F2, or the context menu of a design or
a folder) renames a design or a folder in its project. A **drag** onto
another folder or onto the project moves a design or a folder in its
project. Each rename or move is one version, for example `Move
parts/a.mitcad to old/a.mitcad`. A design that is open follows its new
name. You cannot move a design between projects.

**Delete.** **Delete** (Del) asks first. Then it removes a design or a
folder as a new version. The project's history keeps the design or the
folder. A design that is open closes. If no version holds some of its
changes, MitCad loses those changes, and the question says so.

**Project Settings.** **Project Settings…** (the settings button of the
toolbar) opens the settings of the selected project. For a selected
folder or design, it opens the settings of their project. A double-click
on a Cloud project also opens its settings. The settings of another
project open without that project.

**Other commands.** **Version History…**, **Show in Folder** and **Sync
Now** work on the selected design or project. For another project, that
project opens first. **New Project…** and **Open from Cloud…** are in
the panel too.

**Save.** **Save** of a new, untitled design in a project starts in the
folder that is selected in the panel.

You cannot move or delete a design that someone else locked. MitCad
refuses a name that is taken, and you can then type another one. MitCad
sends each change as it sends a saved version.

### Sharing a project

A **Cloud** project shares its versions through a git repository. The
repository is on a git service (GitHub, GitLab, Forgejo or Gitea, or any
server) or in a shared folder. This needs the git program (Git for
Windows on Windows).

You get a Cloud project in three ways:

- **New Project** with Storage **Cloud**
- **Project Settings** of a Local project, with **Cloud** and **Share**.
  Share sends the whole history. The dialog lists the authors whose
  names and email addresses the versions carry.
- **File › Project › Open from Cloud…**, which opens a Cloud project
  that exists

**The Cloud section.** Paste the repository's address into **Repository
address**. MitCad accepts these forms:

- The address that your server's clone button shows:
  `https://git.example.org/team/robot-arm.git`,
  `git@git.example.org:team/robot-arm.git` or
  `ssh://git@git.example.org/team/robot-arm.git`
- The address of the repository's page in the browser:
  `https://git.example.org/team/robot-arm`, also with `/src/…` or
  `/tree/…` after it
- The `git clone …` line
- A folder

Any server works. MitCad reads an address on a host that it does not
know as `owner/repository` on a Forgejo / Gitea style server. MitCad
uses an address with a port, another SSH user or a deeper path as it is
(**Other address**). MitCad refuses a malformed address at once with
"This is not a valid repository address". It does not give the address
to git.

**Details** shows how MitCad read the address and lets you change it:

- The **Service**: GitHub, GitLab, Forgejo / Gitea with its server,
  Shared folder, or Other address
- The **Owner**: you or an organization. On GitLab, also a group with
  its subgroups, `group/sub`.
- The **Repository**: named after the project's folder until you type
  one
- **HTTPS** or **SSH**

The address follows what you change in Details. MitCad reads an address
pasted into the Server, Owner or Repository field like an address pasted
into Repository address. `https://gitlab.com/group/sub/robot-arm.git`
gives the owner `group/sub`. GitHub and Forgejo / Gitea have one owner
level. An address with a part that starts with `~` stays an Other
address. **Create Repository in Browser…** (under Details) opens the
service's page for a new repository. On GitHub, the form is filled in,
also for an organization. On GitLab and Forgejo, select the owner on the
page. MitCad makes an empty shared folder a repository.

**The check of the address.** MitCad checks the address as you type. It
checks the address again when the dialog gets the focus back, for
example after you made the repository in the browser. The result is one
of these:

- Empty and ready
- Files but no project, for example a README. The project goes beside
  the files, and its history after theirs.
- A MitCad project. **Open It Instead** opens it.
- Not found
- A problem of the sign-in or of the network

When MitCad cannot reach the server, **Create as Local for Now** makes a
Local project. You can share it later in Project Settings. For a folder
with files, it makes the project in the new subfolder, unless you
clicked Create for the whole folder before.

**Open from Cloud.** **Open from Cloud** clones a project into a new or
empty folder. The default folder is Documents/Mitcad/<repository>. The
dialog shows a summary of what the repository holds, for example
`Project "robot-arm": 14 versions, latest 2026-10-07 by Your Name`. An
empty repository gets a new, empty project (**Create Project Here**). A
repository with files but no project gets a project beside the files
(**Make It a Project**). A folder that holds a clone of the repository
opens as it is (**Open It**). The project shows in the Projects panel
with its repository's address. It opens the design that you opened last,
or else the newest design.

**The sign-in.** The sign-in is git's. MitCad never asks for, keeps or
passes a password or a token. It refuses an address that has one in it.
Over SSH, git uses your SSH key and its agent. If the server refuses the
key, **Copy Public Key** puts your key on the clipboard for the
service's **SSH Keys Page**. The key goes to your own user account, also
for an organization's repository. Over HTTPS, git uses a credential
helper. Git Credential Manager comes with Git for Windows.

**A project folder from someone else.** A project that you get as a zip
or from a shared drive brings its own git settings (`.git/config`). Some
settings start programs when git connects, for example `core.sshCommand`,
`credential.helper` or `remote.origin.uploadpack`. MitCad does not trust
these settings of a project. If the project's own settings contain such
a setting, MitCad runs no git in the project: it does not fetch, send,
sync or take edit locks there. The project indicator shows `Not synced:
untrusted git settings`, and its tooltip names the settings. Your
versions on this computer are not affected. If you made the setting
yourself, remove it from the project (`git config --local --unset
<setting>`) and keep it in your own git settings (`git config --global`),
which MitCad uses. MitCad also runs no git hooks, and it checks the
repository address that a project's settings give as it checks an
address that you type. When the project's remote is a folder (for
example a repository beside the project on the shared drive), MitCad
runs none of that repository's hooks when it sends to it, and it does
not use the remote if that repository's own settings start programs. Projects that MitCad makes or opens from a
remote do not have such settings.

If this computer does not know the identity of the server yet, MitCad
shows **Trust This Server…**. It shows the fingerprints of the server's
keys. MitCad checks them against the keys that GitHub, GitLab and
Codeberg publish. For another server, compare them with the fingerprints
that its administrator gives. If you trust the server, MitCad adds the
keys to your `~/.ssh/known_hosts`.

**In a Cloud project.** These rules apply in a Cloud project:

- MitCad sends every version that Save records at once. Project Settings
  can set that to off, with Preferences › Cloud as the default. Offline,
  versions wait, and the indicator shows them (`Offline, ↑2 waiting`).
  MitCad sends them when it can reach the repository again.
- MitCad checks the repository for newer versions when a design opens,
  every 10 minutes and at the first change after the design opened.
  Project Settings sets how often, or never. The indicator shows newer
  versions (`↓1`). A newer version of the open design shows a notice
  under the toolbar with **Sync Now**.
- **Sync** (File › Project › Sync, Ctrl+Alt+Y) saves the open design,
  takes the newer versions of the remote and sends your versions after
  them. Thus the history stays one line. If the file of the design
  changed, MitCad opens the design again.
- When you and someone else both changed a design, **Resolve Sync
  Conflicts** asks what to keep: **Keep mine**, **Take theirs** or
  **Save mine as a copy**. Save mine as a copy is the default: the file
  becomes theirs, and MitCad keeps yours next to it as `bracket
  (conflict copy Your Name 2026-10-05 14.03).mitcad`. **Compare** lists
  what differs. MitCad never merges designs, and nothing is lost. The
  versions that are left out stay in a backup of the history.
- **Project Settings** shows the address with **Change…** and **Open in
  Browser**. Change… accepts a new address of the same repository, when
  the repository moved. It refuses another repository. Project Settings
  shows the state with **Sync Now**. **Local** stops the sync: MitCad
  removes the remote, and the versions stay here and in the repository.
  Under **For everyone in this project** are the settings that the
  project's members share. MitCad records them as a version and sends
  them. The MitCad of the other members obeys them after its next sync.
- MitCad does not share designs that are linked from outside the
  project.
- A repository with **several remotes** (configured with git) syncs with
  the remote that its branch follows. Without one, it syncs with
  `origin`, or else with the only remote. When none of these applies,
  the indicator says `No remote chosen`. Then MitCad sends and fetches
  nothing, and Project Settings asks which remote to follow (**Remote**
  and **Follow**). `mitcad-cli remote follow` does the same.

### Live updates

**Live updates** make what other people do arrive at once instead of at
the next check: a version that someone sends, and who has a design open.
They go through an MQTT broker that the project names. Set the broker in
Project Settings › Live updates › Broker, with a topic prefix, `mitcad`
by default. Under Live updates here, **Another broker** or **Off**
changes the setting for this computer only. The versions stay in the git
repository. The broker only says that something changed.

**Trust.** The first time a project wants to connect to a broker, MitCad
asks: "Robot arm uses live updates through broker.example.com. Your name
and the designs you open are sent there." Select **Connect** or **Not
Now**. Anyone who can send versions to the project can change its
broker. Thus nothing connects before you answer. MitCad keeps the answer
for that address. Another address asks again. For a plain-text broker
(`mqtt://`), the question says so: anyone on the network can read its
messages.

**The sign-in.** If a broker wants a user name and a password, a notice
shows under the toolbar: "Live updates need you to sign in to
broker.example.com", with **Sign In** and **Not Now**. The design opens
meanwhile. MitCad keeps the user name and the password in the system's
keychain: Windows Credential Manager, the macOS Keychain, or the Secret
Service or KWallet on Linux. It never keeps them in the project or in
MitCad's settings. They go only to that broker's host and port, and only
over an encrypted connection (`mqtts://`). MitCad refuses a password for
`mqtt://`. Without a keychain, MitCad keeps them until it closes.

**The indicator.** The indicator says `· Live` while the connection is
up. It says `· Live offline (polling)` while the connection is down.
Then MitCad checks the repository on its own again. While live, a
version that someone sends shows at once: `↓1`, and the notice of a
newer version of the open design. While live, MitCad does not check the
repository on the timer.

**Test.** **Test** in Project Settings connects to the broker,
subscribes, publishes one message under the prefix and waits for it. It
lists the steps.

**Preferences › Cloud.** **Tools › Preferences › Cloud** has these
settings:

- The git program that sends and gets the versions of Cloud projects.
  MitCad finds it on `PATH`, on Windows also where Git for Windows
  installs itself, or you select it. The sign-in is git's own. Local
  projects need no git program.
- The default author, used when git has none
- The defaults of projects: if MitCad sends each version at once, and
  how often it checks for newer versions
- If live updates are permitted. When off, no project connects to a
  broker.
- The broker that New Project offers
- **Known brokers**: the brokers for which you answered the trust
  question, with the user that you signed in as. **Sign Out** removes a
  broker's user name and password from the keychain. **Forget** also
  removes the answer, so that the next connection asks again.

### Edit locks

In a Cloud project, a design has one editor at a time. When you open a
design, MitCad takes its **edit lock** on the remote. The lock is a
reference of the repository's own, outside its branches. Anyone else who
opens the design gets it **read-only** and can ask for the lock. Other
git tools do not see the locks. A remote that does not accept such
references has no locks, and Project Settings says so.

Locks are on by default. Project Settings sets them to off for everyone.
Project Settings also sets the idle time (1 to 120 minutes, 10 by
default) and how often MitCad reads the remote (5 s to 10 min, 10 s by
default). While live updates are connected, MitCad reads the remote
every 2 minutes, because live updates tell of every change at once.

**When you open a design.** If the lock is free, the design opens with
the lock, and the indicator says `Editing`. If the remote has a newer
version of the design, MitCad takes that version first, as Sync does. If
someone else edits the design, it opens read-only with "part.mitcad is
being edited by Alex (since 13:40, last active 2 min ago)". Select
**Continue Read-Only** or **Request Edit Access…**. If you opened the
design elsewhere yourself, on another computer or in another window,
select **Take Over** or **Open Read-Only**. With Take Over, the other
window becomes read-only and keeps its changes.

MitCad takes a lock as free when its holder is gone, and tells you whose
it was. The holder is gone in these cases:

- Its MitCad ended and did not release the lock
- It did not answer
- With live updates, its connection was lost and you said so

When MitCad cannot reach the remote, the design opens with `Edit lock
not confirmed (offline)`, and you can edit it. MitCad takes the lock
when the remote answers. If someone else holds the lock by then, the
window becomes read-only. **File › Open Read-Only…** opens a design
without a lock. An older version opened from Version History takes no
lock either.

**Read-only windows.** A read-only window shows and inspects the design
but does not change it. It permits selection, measurement, sections,
Isolate, export, 3D printing, rendering, Version History and its
comparisons. It refuses commands, timeline edits, sketches and
parameters. The banner under the toolbar says why, with **Request Edit
Access…**, which sends a message of up to 200 characters. The banner
shows the holder's answer. With live updates, it also shows who else has
the design open.

When the holder sends a version, the read-only window shows it without a
change to your copy of the file: "Updated to Alex's version saved at
14:05." Your copy follows when you get the lock. **Save As** keeps a
copy. The copy opens with a lock of its own.

**While you hold a lock.** A command, an edit, a selection or a change
of the view keeps the lock active. After the idle time without any of
these, MitCad does one of these:

- With no unsaved changes, MitCad sends the versions of the design and
  releases the lock.
- With unsaved changes and autosave on (Preferences › General), MitCad
  saves them as a version first ("Saved automatically before releasing
  the edit lock").
- With autosave off, the lock stays, marked idle. A request gets it at
  once, and your changes stay in your window.

The banner then says "Your edit lock was released after 10 minutes
without activity", and **Edit** takes the lock again. The indicator's
menu has **Release Edit Lock** and **Edit Lock Details…**. Edit Lock
Details shows the holder, since when, the requests, who else has the
design open, and the connection of the live updates.

**Requests.** When someone asks for your lock, your MitCad answers with
a receipt by itself. Then it asks you:

- **Release**: MitCad saves unsaved changes first. Or **Release Without
  Saving** keeps them in your read-only window. MitCad sends the
  versions, and the lock goes straight to the one who asked.
- **Keep 15 More Minutes**: MitCad asks you again after 15 minutes.
- **Decline…** with a message

Without an answer in the idle time, MitCad releases the lock as at the
idle time. Your MitCad does not answer at all when it ended or when the
computer is off. Then the one who asked takes the lock after two of its
polls. The other way round, the MitCad that asks renews its request
while it waits. If that MitCad ended, or if its window closed without a
withdrawal of the request, it no longer renews the request. After the
idle time and two polls, MitCad asks nobody about the request any more,
and the next MitCad that sees it removes it. A declined request goes the
same way unless you ask again.

**When you lose a lock.** If someone took your lock over, or took it
because your MitCad did not answer, the window becomes read-only and
says who took it. Unsaved changes stay. **Save as Copy…** or **Save as
New Version** keeps them. Sync then asks which version of the file to
keep.

**When you close a design.** When you close a design or quit, MitCad
sends the versions not sent yet and releases its lock and its requests.
When that sync needs your choice, select **Resolve Sync**, **Release
Without Sending** or **Keep Lock**. Offline, the lock stays until it is
stale. After a crash, the lock stays too. When you open the design
again, or restore its autosaved work, MitCad checks the lock as it does
when it opens a design.

**Sync.** Sync asks before it sends a version of a file whose lock
someone else holds: **Send Anyway** or **Cancel**.

**Locks set to off.** If you set edit locks to off, MitCad releases your
locks. The locks of other people stay. `mitcad-cli lock status` lists
them, and `mitcad-cli lock release --force` removes them.

## Libraries

Libraries give you standard components and the designs that other people
share. Component libraries and community indexes are git repositories.
MitCad fetches them, when you ask or, for the sources that you choose,
when MitCad starts. Libraries from `https://` addresses need no git
program. This
chapter describes the component libraries, the component picker that shows
their components, and the community library.

### Component libraries

Fasteners and other standard components come from component libraries. A
component library is a git repository of designs. Each design has a
table of its sizes.

**Tools › Libraries** has four groups:

- **Browse Components…** finds, gets and places a component: of the
  libraries you have, of the community indexes, or of your favorites.
- **Components in This Design…** shows the library components of the
  open design; **Publish to Library…** adds the design to a library of
  your own.
- **Material Library…**, **Printer Library…** and **Process Library…**
  open the Materials, Printers and Processes managers on My library,
  also with no design open (refer to [Where print items live](#where-print-items-live)).
- **Library Sources…** sets which component libraries and community
  indexes MitCad uses, and fetches them.

MitCad's component library has these components:

- ISO metric screws (ISO 4762, ISO 4017, ISO 10642)
- Nuts (ISO 4032)
- Washers (ISO 7089)
- The slot profiles, connectors and other components of the Build tab

The screws, nuts and washers have the sizes M1.6 to M24 (the
countersunk screws M3 to M20). Up to its version
1.0.0, this library was the fastener library. Designs made with the
fastener library keep their components.

MitCad fetches libraries from `https://` addresses itself: you do not
have to install git. A library in a folder or at an SSH address
(`ssh://…`, `git@host:…`) needs the installed git program, and so does a
library that asks you to sign in. For components that are already in a
design, nothing has to be fetched.

**Tools › Libraries › Library Sources…** lists the libraries and the
community indexes that MitCad uses. At the start, the list has MitCad's
component library and the community index. In **Library Sources…** you
can:

- Add a library by its URL or by a folder. Another form of the URL of a
  library in the list (for example with `www.` or without `.git`) is the
  same library: MitCad selects its row and does not add it twice.
- Remove a library. Its components leave the pickers and the search;
  designs that use them keep them. Add it again, in any form of its URL,
  and its components and your stars come back once.
- Disable a library
- Select **Fetch** to get a library or its newer versions
- Select **Auto-update** for a library that MitCad must fetch when it
  starts
- Select **Use the installed git program** (an advanced setting, off at
  first) to fetch every library with git: for SSH addresses, or for a
  library that needs a sign-in, which git does with its SSH key or its
  credential helper

**Auto-update** is off at first. When it is on, MitCad fetches the
library in the background when it starts. No dialog opens, and you can
work at once. The status bar shows the new versions, for example
`Libraries updated: mitcad-parts v2.7.0`. If MitCad cannot reach a
library, it writes a short note in the status bar and goes on. The
library browser and the component picker show new versions when they are
open. The components in your designs keep their version. You choose a newer
version yourself.

If MitCad cannot fetch a library, it says why in plain words: the server
cannot be reached (check the network and the address), there is no
repository at that address, the server asks for a sign-in (turn on **Use
the installed git program**), or an SSH address needs the installed git
program. Cloud projects still need git; libraries from `https://` addresses
do not.

**Auto-update** has an effect only while the library is on. When you
switch the library off, the check box is gray. MitCad keeps its state.
Without **Auto-update**, MitCad fetches a library only when you ask for
it.

**Tools › Libraries › Browse Components…** opens one window for all
library components. **Insert from Library** in the Assemble group opens
the same window. The window shows the components as pictures in the grid
of the component picker (refer to [Component picker](#component-picker)).
The tabs above the grid select what it lists:

- **Installed**: the components of the libraries that you have, and the
  libraries that the community indexes offer (in **Libraries to get**)
- **Community**: the libraries that the community indexes offer, with
  their components, then the components that you have. **Community
  Library…** in the command search opens the window on this tab.
- **Favorites**: the components that you starred

When you select another tab, the search, the filters and the selected
component stay.

To insert a component:

1. Select a group on the left, or search by name, standard or tag
   (`4762`, `washer`).
2. Select the version. The newest version is first.
3. Select the size: first **Size**, then **Length**.
4. Insert the component **linked** or as a **copy**. A linked component is
   read-only. A copy is editable and has its history.

MitCad shows the license and the attribution before you insert the component.

The main button follows the component that you select. It is **Insert**
when you have the component's library. It is **Get Library** when you do
not have it yet. **Get Library** shows the library's address and license
and fetches the library. After the fetch, the same component stays
selected, with the same search, and **Insert** places it.

**Library Sources…** in the window opens the same window as **Tools ›
Libraries › Library Sources…**.

#### Place Component

Some components know where they mount: they have **mount frames**. The
screws, nuts, washers and wood fasteners of the MitCad parts library have
one. A screw's frame is on its axis under the head (a countersunk screw's
at the top of its head). A nut's or a washer's frame is on the face that
bears on the part. When you insert such a component, **Place Component**
opens instead of the component going to the origin:

1. Select where it goes before **Insert from Library**, or click it in
   **Place Component**: for a screw, nut or washer the rim (the round
   edge) or the round face of a hole; for a component that lies on a
   face, a flat face or a plane.
2. The component shows there at once: a screw in the hole, its head on
   the face.
3. Press **Place** (Enter). MitCad makes the component and a joint to
   the hole or the face. **Place Component** stays open for the next
   one. **Shift + click** places at once.
4. Press **Close** (Esc) when you are done.

**Mount Frame** chooses the frame when the component has several,
**Rotate** turns the component about the frame, **Offset** leaves a gap
and **Flip** takes the other end of the hole.

Enter without a pick drops the component in at the origin. A component
without mount frames always goes to the origin: the message band says
`Inserted <name> at the origin: no mount frame; join it with Joint.`
Then join it to the model with **Joint**.

#### Standards and suppliers

One component can have the sizes of several standards. For example, the
hexagon socket head cap screw has the lengths of ISO 4762 and of DIN 912,
and some lengths that a supplier sells outside every standard. Each size
names its standards. When a library gives them, two filters show above
**Size**:

- **Standard** (Alt+D): **All**, or one standard. **Size** and
  **Length** then list only the sizes of that standard.
- **Supplier** (Alt+E): **All**, or one supplier. **Size** and
  **Length** then list only the sizes that the supplier sells.

The filters change no component and are not saved in the design. They
are in **Insert from Library**, **Components in This Design**, **Edit Options…**
and the Build tab's component picker.

The name of the component follows the standard: with **DIN 912**, the
M6 x 10 screw is `DIN 912 M6x10`. With **All**, it gets its first
standard (`ISO 4762 M6x10`). A size of no standard is
`M6x6 (non-standard)`. The component keeps the name, also in the parts
list.

Under the size, **Insert from Library** shows the designation of the
size. When the suppliers sell the size, **Insert from Library**,
**Components in This Design** and **Edit Options…** show a button
**Supplier Articles (n)…** (Alt+A), where n is the number of articles.
It opens a table with one line per article: the supplier (a link to its
site), the part number, the material, the finish, the date the article
was checked and the product page, as far as the library gives them.
Columns that no article fills are not shown.

- Click a column heading to sort by that column.
- **Copy as TSV** (Alt+T) copies the selected lines, or all lines, as
  tab-separated text that a spreadsheet takes.
- **Copy Part Number** (Alt+P) copies the part number of the current
  line. Ctrl+C copies the selected cells; one cell is copied alone.
- **Open Link** (Alt+L), a double click or Return on a link opens the
  supplier's site or the product page in the browser.

When the **Supplier** filter names a supplier, that supplier's articles
come first in the table. The bills of materials have the articles in the column
**Supplier part numbers** (also in CSV and PDF), for example
`Würth 0084 6 10 (8.8 zinc-plated)`.

The grid has its own **Supplier** filter (Alt+R) beside the search. It
hides the components that the supplier sells in no size. MitCad
remembers it.

#### Your region and shops

Choose where you buy in **Tools › Preferences › Suppliers** (on macOS
**MitCad › Settings… › Suppliers**):

- **Region** (Alt+R): the country whose shops the part numbers link to.
  **None** makes no links. Until you choose, MitCad takes your system's
  country when it knows a shop there.
- **Shops you buy from** (Alt+S): the shops of that region, each with a
  check box.

The **Supplier** filters of the grid and of the size choosers then start
with the shops you checked instead of **All**; several shops are one
entry, **Your shops (…)**. Choose another supplier, or **All**, whenever
you like; MitCad remembers that until you change your shops. In
**Edit Options…** the filter starts with your shops only when they sell
the component's size, so a default never changes a component.

In the **Supplier Articles** table, a part number in the link color
opens the product in the region's shop (a double click, Return or
**Open Link**). The bills of materials get a column **Product Pages** with
the links of each part's articles, also in CSV and PDF; a double click on
it opens one (a menu when there are several). A supplier with no shop in
your region gets no links. The libraries keep only the part numbers:
MitCad makes the links from its own list of shops.

When a community index offers a library that one of your libraries
already is (under its old name, for example), MitCad does not list it a
second time.

#### Components with options

Some components are configurable: they have **options** (for example
**Color** and **Length**) and **rules** that limit the combinations
(refer to [Configurations](#configurations)). For such a
component, **Insert from Library** shows a chooser per option instead of
the size, in the options' order:

- A list of values is a drop-down list. It shows only the values that
  the rules allow with the options before it.
- A length or another number is a field. You can type any value between
  its smallest and largest allowed value, in its steps.

When you change an option, the options after it keep their values if the
rules allow them. Otherwise MitCad adjusts them to the nearest allowed
value, and a note under the choosers names the rule. For example, with
the rule *Black is short*, the choice **Black** adjusts **Length** from
1200 mm to 1000 mm, and the field then allows at most 1000 mm. A
combination that a rule forbids is not taken, and the note says why.

The component is inserted with the values that you chose. If the values
are the same as a named row of the component (for example **Standard**),
the component gets that row's designation and part number.

A component that allows only its listed rows (**Only these rows** in its
Configurations window) has only those combinations. You select it by its
sizes as before.

**Edit Options…** in the context menu of an inserted component (in the
browser) changes its options later. The options belong to the
component, so all its occurrences change. For each option you can:

- Select a value in its chooser.
- Type an expression of the design's parameters in its expression field,
  for example `Shelf_W - 2 * Wall`. The **fx** button opens the Formula
  Editor. The option then follows the parameters: when you change
  **Shelf_W** in **Edit Parameters**, the component changes too.

Select **OK** to keep the options. This is one undo step.

The Build tab's component picker shows the options of a configurable component
under the grid. Click the tile, then change the options. The component that
you place has these options.

On a new installation, the list of components is empty until you press
**Update**. **Update** fetches all enabled libraries and searches again.
This is the same as **Fetch All** in **Library Sources…**. Press **Update**
later to get newer versions to select from. Components that are already in a
design keep their versions.

Each linked component keeps the library version that you selected for it.
When you open the design, MitCad reads that version, also when the
library has newer versions. Thus the design always rebuilds in the same
way.

**Tools › Libraries › Components in This Design…** shows the library
components of this design with their versions and sizes. To change a component:

1. Select another version or size.
2. Select **Show Changes** to list what changes
   (`M5x16: dk 8.5 mm -> 8.7 mm`).
3. Select **Update**. MitCad applies the change as one undo step.

**Check for Newer Versions** first fetches the libraries.

Library components carry their standard designation (`ISO 4762 M5x16`). The
**Bill of Materials** tab of **Components in This Design** lists every component with its
quantity, library, version and license. It copies the list as CSV. MitCad
records the license, the library, its URL and the version with each component
in the design, also with a copied component.

A design from another computer can use libraries that are not on this
computer. Then the design keeps the saved bodies of its components and says
so. **Get Missing Libraries** in **Components in This Design** shows the addresses of
the libraries, then fetches them.

MitCad and `mitcad-cli` share one default cache of fetched libraries:

| Platform | Default library cache |
|---|---|
| Windows | `%LOCALAPPDATA%\Mitcad\Mitcad\libraries` |
| macOS | `~/Library/Application Support/Mitcad/Mitcad/libraries` |
| Linux | `$XDG_DATA_HOME/Mitcad/Mitcad/libraries`, or `~/.local/share/Mitcad/Mitcad/libraries` |

Each URL has one clone. The environment variable `MITCAD_LIBRARIES_DIR`
sets a different folder for both, and the CLI's `--libraries` overrides it
for that command. Earlier CLI caches in the data folder's `mitcad/libraries`
remain readable with the default configuration; new fetches use the shared
cache. Explicit cache overrides read only the selected folder.
MitCad makes and
updates the clones (itself for `https://` addresses, with the git program
for folders, SSH addresses and when **Use the installed git program** is
on), and only when you ask with one of these commands:

- **Fetch** in **Library Sources…**
- **Update** in **Browse Components**
- **Check for Newer Versions** in **Components in This Design**
- **Get Library** in **Browse Components**

An update adds branches and tags. It removes nothing. Git's automatic
garbage collection is off in these clones. Thus the versions that
designs use stay readable.

### Component picker

The component picker shows the components of a library as a grid of pictures
(tiles). Each tile shows the name and the main sizes of a component.
**Browse Components** uses the component picker. The Build commands that
select a component also use it: **Profile**, **Panel**, **Linear Component** and
**Connector**.

The groups are on the left of the grid:

- **Favorites**
- **In this design**: the library components that the open design uses, each
  with the number of times (`×4`). MitCad shows this group while the
  design has library components. A component with several sizes shows each size's
  own count, only on the sizes that the design uses.
- **All components**
- The groups of the libraries, as a tree

MitCad's library has these groups, each with its subgroups:

- **Wood**: timber, boards, sheets, wood fasteners, timber connectors
- **Slot profile**: profiles, connectors, panels, caps, accessories,
  and the nuts and screws made for the slot
- **Motion**
- **General fasteners**: the ISO screws, nuts and washers
- **Hardware**
- **Workshop**

A click on a group shows the components of its subgroups too. A component can be
in several groups. It is the same component in each group, with one star and
one count. Fold a group with its arrow, or with Left and Right on the
keyboard. The picker remembers the fold. The libraries of others put
their groups beside these groups, or into the same groups by their
names.

Each group shows how many components it has, its subgroups included:
`Wood fasteners (12)`. The number counts only the components that the search
and the filters (**Supplier**, **System**, and the license in **Insert
from Library**) show. The numbers change as you type a search or change
a filter. While a search or a filter is on, the groups with no components
are not shown, and neither is a group whose subgroups are all empty.
The group that you selected stays, grayed with `(0)`, so that you can
change the search in it. It goes when you select another group.

Above the grid are these controls:

| Control | Function |
|---|---|
| **Search** | Searches by name, standard, size, supplier or part number |
| **System** | Filters by the slot system of Build components |
| **Supplier** | Shows only the components that one supplier sells |
| **Tiles** | Sets the size of the pictures |

Put the mouse pointer on a tile to see a larger picture. The larger
picture shows the supplier, the part number, the library and the
license.

Click the star in the corner of a tile, or use **Star** (Ctrl+D), to
mark a component. Components with a star come first in every group. All of them
are in **Favorites**. Click the star of a group (Ctrl+Shift+D) to put
the group and its subgroups first. In **Favorites**, the components of that
group and its subgroups come first. A click on a star changes only the
star. The tiles and the groups stay where they are while you look at
that list. Thus you can star several sizes one after the other. The
starred components move to the front the next time MitCad makes the list
again. That occurs when the picker opens, or when the group, the search
or the system changes. Stars belong to you on this computer, not to the
design. Library updates
do not remove stars. If a component is not in the library any more, it is not
in **Favorites** any more. **Search** and the **System** filter also
work in **Favorites**.

For a Build command, the picker is a floating window. It opens when you
click the **Component** or **Connector** tile of the command. You can move
the window, resize it or put it on another screen. It keeps its place
and size. At the same time, you can continue to use the view. The picker
shows only the components that the command can use. Components that nearly fit are
shown in gray, with the reason. The picker starts with the system of the
component that you selected last. A click on a tile selects the component, and the
command shows a preview at once. The window closes with the command
unless you **Pin** it. A pinned picker stays open for the next command
that selects components.

### Community library

The **Community** tab of **Tools › Libraries › Browse Components…**
searches the designs and components that other people share in their own
git repositories.
Community indexes list these repositories. An index is also a git
repository. Each item shows its license and the conditions of the
license. The licenses are CC0, CC-BY, CC-BY-SA, MIT, Apache, BSD and
CERN-OHL. MitCad hides items without a license unless you ask for them.

**Get Library** fetches a library that an index lists, also when you
select one of the components that the index lists for it. Then you can
insert its components linked or as copies. MitCad records the license and the
source with every component in the design.

Every item must have a license. A community index accepts these
licenses. The table is not legal advice.

| SPDX | What you must know |
|---|---|
| CC0-1.0 | No conditions |
| CC-BY-4.0 | Credit the authors when you share designs that contain it |
| CC-BY-SA-4.0 | Credit the authors. A design that you share with it can need the same license |
| MIT, BSD-2-Clause, BSD-3-Clause | Keep the copyright and license notice |
| Apache-2.0 | Keep the copyright, license and notice files |
| CERN-OHL-P-2.0, -W-2.0, -S-2.0 | Permissive, weakly and strongly reciprocal hardware licenses |

**Tools › Libraries › Publish to Library…** adds the open design to a
library of your own. This library is a folder that keeps versions like a
project. When the library is new, MitCad makes the folder and you select
its license. The command does these steps:

1. Records the design with an image of the view.
2. Pushes the library to a remote that you give.
3. Copies the entry, so that you can propose it to a community index.

The format of a library is in [libraries.md](libraries.md).

## Import and export

MitCad reads and writes many CAD formats. This chapter lists the
formats, tells how you import and export files, and describes each
design import. A design import reads a file's history and replays it as
MitCad features.

### File formats

The table that follows lists the formats that MitCad reads and writes.
The column **Design history** tells what each import keeps of the
file's history.

| Format | Import | Export | Design history |
|---|---|---|---|
| MitCad design (`.mitcad`) | Open, Insert Component | Save | Yes |
| FreeCAD (`.FCStd`) | Open, Import | None | Features, sketches and parameters |
| `.f3d`, `.f3z` | Open, Import. Mesh bodies come in as mesh bodies, in their components | None | Features, sketches and parameters |
| `.ipt` (parts) | Open, Import | None | Features, sketches and parameters. A feature that MitCad cannot replay is imported as the bodies that the file stored after it |
| `.iam` (assemblies) | Open, Import. Parts become components with their history or with their bodies. Sub-assemblies become components | None | The parts' features, or their bodies |
| IFC (`.ifc`, `.ifczip`) | Open, Import. Storeys become components, elements become features, bodies and meshes | None | Swept solids and their cuts as features |
| `.rvt` (projects) | Open. Levels become components, elements become bodies and meshes | None | Walls, floors, roofs, ceilings, columns, beams and stair runs as features |
| SolidCraft (`.sc`, `.solidcraft`) | Open, Import (every file format version) | None | Features, sketches, parameters, components and joints. Sheet metal |
| DWG (`.dwg`, R13 to R2018) | Open, Import. 3D solids become bodies, 3D faces and meshes become mesh bodies, blocks become components with their occurrences | A drawing (R2000) | None. A drawing keeps no history |
| STEP (`.step`, `.stp`) | Bodies. An assembly: its parts become components, each placed by an occurrence per instance | Bodies (AP214 or AP242). Components as an assembly | None |
| IGES (`.iges`, `.igs`) | Bodies | Bodies | None |
| BRep (`.brep`, `.brp`) | Bodies | Bodies | None |
| STL | Meshes | Meshes | None |
| OBJ | Meshes | Meshes | None |
| DXF | Into a sketch | From a sketch. A drawing | None |
| SVG | Into a sketch: paths, shapes and texts, filled regions by their fill rule, strokes as bands | None | None |
| 3MF | None | For slicers | None |
| USDZ | None | For AR and 3D viewers: meshes with appearances, components kept | None |
| Web page (`.html`) | None | The design in one HTML file with its viewer, for any browser | None |
| Drawing (PDF, SVG, DXF, DWG) | None | Views with hidden lines removed, sizes, hole diameters, a hole table and a title block | None |

If a file does not import correctly, please
[open an issue](https://github.com/mitcad/Mitcad/issues).

### Import

**File › Import** brings a file into the open design. The Insert
group has the same commands. The file dialogs list the CAD formats
under one *CAD files* filter. The table in [File formats](#file-formats)
tells which formats MitCad reads and writes, and which keep their design
history.

Each format is imported in its own way:

- STEP, IGES and BRep are imported as a base feature ([Base
  features](#base-features)). A STEP assembly can keep its structure
  instead ([STEP assemblies](#step-assemblies)). When the design
  already has content, MitCad asks **Where to Import** (for a STEP
  assembly, after the assembly question). **New Component** (the default)
  puts what the file brings into a new component named after the file,
  *bracket* for *bracket.step*, so that you can move or hide the import
  at once. **Into the Active Component** puts the bodies or components
  straight into the active component. File › Open of a STEP file makes
  a new design of the file, without such a component.
- STL and OBJ are imported as mesh bodies. Insert Mesh asks for the
  unit, and whether to convert the mesh to a solid ([Convert
  Mesh](#convert-mesh)).
- DXF is imported into a new sketch on a plane that you select (Insert
  DXF). In sketch mode, MitCad imports it into the sketch that you
  edit. Block references are expanded: an array insert gives its
  block once per cell, at most 100 000 cells (its first rows), and at
  most a million entities and block references are placed in all,
  counting the contents of every reference. The import's warnings say
  what was left out.
- SVG is imported into a sketch in the same way (Insert SVG). Refer to
  the paragraph that follows.
- Another MitCad design is imported as a component, linked or as a
  copy (Insert Component). When a design opens, MitCad reads its linked
  files again only from the design's project (without a project: from
  the design's folder and the folders in it). A link to a file elsewhere
  is not followed: its component keeps the bodies saved with the design,
  and a message tells you so. Keep linked designs in the project.

Insert SVG asks for these values:

- A scale, in sketch millimeters per millimeter of the drawing. The
  drawing's millimeter is its user unit, or the size that its width and
  height give.
- The point where the lower left corner of the drawing goes.
- If stroked paths become bands of their stroke width.

Curves are imported exactly. Texts are imported as sketch texts. The
regions that the drawing fills are selected after the import. Thus
Extrude makes a logo with its holes in one step.

Design files open as new designs:

- FreeCAD documents, `.f3d` designs, `.ipt` parts and SolidCraft
  designs open as new designs with their history ([FreeCAD
  import](#freecad-import), [.f3d import](#f3d-import), [.ipt
  import](#ipt-import), [SolidCraft import](#solidcraft-import)).
- `.iam` assemblies open as new designs of the components of their
  parts, with the parts' history or with their bodies ([.iam
  import](#iam-import)).
- IFC building models and `.rvt` projects open as new designs of
  their storeys and elements ([IFC import](#ifc-import), [.rvt
  import](#rvt-import)).
- `.dwg` drawings open as new designs of their 3D solids, meshes and
  blocks ([DWG import](#dwg-import)).

There are other ways to open or import a file:

- A file that you drop on the window opens or is imported.
- Open opens any of these file types as a new design.
- On macOS, a double-click in Finder, Open With, a drop on the Dock
  icon and `open -a Mitcad file` do the same as Open.
- Open Recent remembers the ten latest files, each with the preview its
  last save made. The [Start tab](#the-start-tab) shows them as cards.

On macOS, the title bar shows the file's proxy icon. The close button
shows a dot while the design has unsaved changes.

Each kind of file dialog remembers the folder in which you last used
it, also after a restart. These are the kinds:

- Open, with Open Read-Only and Save As
- Export: every export, Save as PDF and a saved rendered image
- Import, with the Insert commands
- Texture images
- Programs: the slicer and git

If a remembered folder no longer exists, the dialog starts in the
design's folder, the project or Documents. Save Copy As of a version
starts in the design's own folder.

#### STEP assemblies

A STEP file can be an assembly: its products (parts and
sub-assemblies) are placed by instances, and a part that is used four
times is stored once. When you import or open such a file, MitCad asks
how it comes in:

- **Components (keep the assembly)**, the default: each distinct part
  becomes a component that holds its bodies once, in the part's own
  coordinates, and each instance becomes an occurrence of that
  component, placed as the file places it. A sub-assembly becomes a
  component whose occurrences are placed in it. The components take the
  names of the products. The bodies of a repeated part are one body:
  the design keeps its data once, in memory and in the design file, and
  the view meshes it once. Thus a machine with many repeated parts
  opens, saves and draws faster.
- **One base feature**: every placed part becomes a body of its own,
  where the file places it, as for a file without an assembly.

The question has a check box, **Find repeated parts**, off by default.
Turn it on to find the bodies that are the same part, only moved or
turned, and make each such part one component with an occurrence per
copy ([Find Repeated Parts](#components)). With One base feature it
looks at all the bodies; with Components, at the bodies inside each
part. For a file without an assembly, use Find Repeated Parts in the
browser after the import.

A file without an assembly, or with only one placed part, comes in as
one base feature without the question. IGES and BRep files always come
in as one base feature. MitCad's own STEP export writes components as
an assembly ([Export](#export)), so a design that you export and import
again keeps its repeated parts shared. The export writes the placed
parts in one assembly, so sub-assemblies come back as their parts
placed in the design.

The whole import is one undo step. The components go into the active
component, or into the new component named after the file ([Import](#import)).

### Components from the web

**Insert › Component from Web…** opens a web browser in a window of its
own. Use it to get parts from the sites that you use: a supplier's
catalog, a model library or your own server. When you download a model
there, MitCad brings it into the open design as a component. The first
use downloads the browser ([The web browser
component](#the-web-browser-component)).

The window has these controls:

- **Back**, **Forward** and **Reload**.
- The address field. Type an address and press Enter. If you type no
  `http://` or `https://`, MitCad adds `https://`.
- **Sites**: the sites that you added. Click one to open it.

MitCad does not add sites for you. To add a site:

1. Type its address and press Enter.
2. Choose **Sites › Add This Page…**.
3. Change the name if necessary, then click OK.

**Sites › Edit Sites…** lists your sites. Double-click a name or an
address to change it. **Add** and **Remove** add and remove sites. When
no page is open, the window shows your sites as links.

When you download a file, MitCad does these steps:

1. It checks the type of the file. It accepts STEP, IGES, BRep, STL,
   OBJ and MitCad designs, and zip files that hold them. It refuses
   other files, and a note at the bottom of the window tells you the
   type. A PDF data sheet that the site shows in the window is not a
   download: you can read it in the window.
2. It downloads the file to a temporary folder of its own. You do not
   select a folder. A bar at the bottom of the window shows the
   progress.
3. It checks the contents of the file. If a site sends a web page (for
   example a sign-in page) in place of the model, MitCad refuses it.
4. It brings the model into the active component as a new component.
   The component's name is the file's name without its extension.
5. It sets the component's **Part number** property from the first part
   of the name, up to the first underscore or space, if that part has
   only letters and digits and at least one digit. For example,
   `6494K13_NO THREADS_Bearing.step` becomes the component
   `6494K13_NO THREADS_Bearing` with the part number `6494K13`. The
   Bill of Materials shows the part number, and lists the component as
   one part even when the file holds several bodies.
6. It starts **Move/Copy** with the new component selected. Drag the
   component to its place, then click OK.

The import is one undo step. You can rename the component and change its
part number later ([Properties](#properties)). A STEP assembly comes in
as one component with its parts as components in it. A design in
another format, for example an `.f3d` design, opens as a new design, as
**File › Open** opens it.

When you download a zip file, MitCad lists the files in it that it can
import, with their sizes. Select the files that you want, then click
**Import**. Each file becomes a component, as above. If the zip holds
only one such file, MitCad imports it immediately.

MitCad deletes the downloaded files when it has read them: the design
keeps its own copy of the component. If MitCad stops before it deletes
them, it deletes them the next time it starts.

The browser keeps its own cookies and sign-ins, apart from your other
web browsers. Thus a site where you signed in remembers you the next
time. **Sites › Clear Browsing Data…** removes the cookies, sign-ins and
cache. Your sites stay. Only one MitCad at a time can use the browser:
close it in the other MitCad first.

#### The web browser component

The browser is a separate download, the *web browser component* (the
Chromium Embedded Framework). The MitCad installer does not include it.
The first time that you use Component from Web…, MitCad tells you the
size of the download and asks if it can download the component. If you
click **Yes**, MitCad downloads it, makes sure that it is MitCad's own
(it checks the signature of the release), installs it for your user and
opens the browser. If you click **No**, nothing is downloaded; MitCad
asks again the next time.

MitCad updates the component by itself: when a newer one is available,
MitCad downloads it in the background and uses it from the next start.
Each version of MitCad uses only the components made for it. If no
component for your version is available, MitCad tells you to update
MitCad.

### Image references

An image reference is a picture in the design that you model from: a
photo of a part, a scanned drawing or a plan. It lies on a plane or a
planar face, and it changes no geometry.

1. Choose **Insert › Image Reference**.
2. Click **Choose Image…**, or type the image file's path in **Image**.
   MitCad reads PNG, JPEG, BMP, WebP and HEIF (`.heic`, `.heif`)
   files. A HEIF photo is shown the right way up: MitCad turns, mirrors
   and crops it as the file says, and keeps it in the design as a PNG
   image, so the design opens on any computer.

   MitCad reads HEIF photos with the decoder your system already has; it
   does not include one. macOS always has it. On Windows it needs **HEIF
   Image Extensions** and **HEVC Video Extensions** from the Microsoft
   Store (some computers come with them). On Linux it needs your
   distribution's libheif with its HEVC plugin (for example the packages
   libheif1 and libheif-plugin-libde265). If you never open a HEIF file,
   you do not need any of this: MitCad says what to install only when you
   choose a HEIF file it cannot read.
3. Pick an origin plane, a construction plane or a planar face.
4. Set the other values if you need to, then click **OK**.

The image is centered on the plane or face you picked. Its size comes
from the file's resolution; when the file does not have one, MitCad uses
96 pixels to the inch. The panel shows the image's outline while it is
open. These are the panel's values:

- **X** and **Y** move the image's center on its plane. They start at
  the middle of the plane or face you picked.
- **Angle** turns the image about the plane's normal.
- **Flip** mirrors the image from left to right.
- **Width** sets the image's width. Leave it empty to use the size from
  the file, or, when you change an image, to keep its size.
- **Opacity %** sets how much you can see through the image: 100 is
  solid and 0 is invisible. A new image is 50 % opaque.
- **Through Bodies** shows the image on top of the bodies. When it is
  off, bodies in front of the image hide it. Sketches are always drawn
  on top of the image, so you can trace it.

MitCad keeps a copy of the image in the design file, so the design does
not need the original file. The image stays on its face when the design
changes.

The image references are in the browser's **Images** folder. Each one
has a light bulb that shows or hides it. Double-click an image's row to
change its values in the same panel. You can also rename or delete an
image from its row's context menu.

If its face is gone, the image is not drawn and its row turns red (the
tooltip says why). This happens, for example, when a later feature
removes the face, or when you move the timeline marker back before the
feature that made it. Double-click the row and pick another plane.

You cannot select an image in the view, so it never gets in the way of
clicks on the geometry.

**Calibrate an image.** To set the image to its real size, measure
between two points on it:

1. Choose **Calibrate Image Reference** from the toolbar, or from the
   image row's context menu.
2. Click two points on the image whose distance you know, for example
   the two ends of a dimension line on a scanned drawing. After the first
   click, a line follows the pointer from the first point.
3. The dialog shows the distance between the points at the image's
   current size. Type the real distance over it. A number without a unit
   is in the design's length unit.

The image changes size about the first point, so that point stays where
it is. The image must be shown. Press Esc to stop before the second
click.

### Export

**File › Export** writes the bodies or the design to a file. These are
the exports:

- STEP (AP214 or AP242), IGES, STL, OBJ, BRep, 3MF and USDZ of all
  bodies or of the selected bodies. STL has a refinement and a binary
  or text format. USDZ: [USDZ for AR and 3D
  viewers](#usdz-for-ar-and-3d-viewers).
- A sketch as DXF. Export DXF in the sketch's context menu does the
  same. The file holds the sketch's curves that make its profiles:
  construction curves and center lines are left out, so that a cutting
  program cuts only the part. Select **Include construction geometry**
  to write them too, on the layer *construction*. Export DXF in the
  context menu always leaves them out.
- The design as a web page that opens in any browser ([Web
  viewer](#web-viewer)).

A 3MF file can carry the design's print plates, as Send to Slicer writes
them. When the design has plates, **Plates** offers all plates in one
file, a file per plate (named *file - plate.3mf*, beside the file you
give), one plate alone, or the bodies without plates. **Slicer** chooses
whose print settings go with the file; it starts with the design's
slicer when the design has plates. UltiMaker Cura has one build plate, so
it takes a file per plate. Without plates, **On the print bed** puts each
piece on its print bed face; **Center on Plate** and **Drop to Bed**
place the pieces on the plate.

The dialog starts with the type, the refinement, the STL format and
Include construction geometry of your last export, so repeated exports
for 3D printing keep 3MF and High.
When you edit a sketch, or a sketch is selected, it starts with DXF
instead; a sketch's DXF does not change the type remembered for the
bodies. The proposed file is in the folder of your last export. Its name
is the selected body's name for one body, the component's name for one
component, and the design's name otherwise, as in Save as Mesh.

MitCad writes the bodies where the design shows them, also the bodies
of moved components. A STEP file is then an assembly, with the parts
placed as the components are. In a design with components, the
Coordinates row writes each body in its own component's coordinates
instead.

The refinements Low, Medium and High apply to the mesh exports (STL,
OBJ, 3MF, USDZ). They keep each curved face in approximately 5 000,
20 000 and 50 000 triangles. A large doubly curved face, such as a
dome of meters, gets a little coarser mesh instead of millions of
triangles. Faces smaller than approximately 15 cm are not affected. A
custom deviation applies to every face.

**Send to Slicer** (Make › 3D Print) sends the selected bodies, or every
visible body, to a slicer ([3D printing](#3d-printing)).

#### The unit of a DXF file

Every DXF file that MitCad writes is in the design's length unit, unless
you choose another: a sketch's DXF, Export Flat Pattern, Export Face as
DXF, a cut sheet and a drawing as DXF. An inch design writes inches, a
millimeter design millimeters. The Export dialog and Export Drawing have
a **DXF unit** row; the save dialogs of Export Flat Pattern, Export Face
as DXF, Export DXF and the Drawing tab's DXF… offer the units as file
types, such as *DXF drawing in inches (\*.dxf)*. The design's unit is the
first one and is selected.

The file names its unit in its header, so programs that read the header
(and MitCad's own Insert DXF) read the right size. A cutting program that
does not read the header takes the numbers in the unit it is set to:
choose that unit when you export. The log line of each export names the
unit written, for example `…, in in`.

A drawing's DXF file is the sheet in that unit of paper. In inches, an A3
sheet is 16.54 by 11.69, and a view at 1:1 measures the model in inches.
The texts are scaled with the sheet. DWG files stay in millimeters of
paper.

#### Save as Mesh

**Save as Mesh…** in the context menu of a body or a component, in the
browser or in the view, saves its bodies as a mesh file. With several
bodies or components selected, it saves all of them. A component saves
the bodies in it and in its subcomponents. Surface and empty bodies are
left out. When there is nothing else, the entry is unavailable, and its
tooltip tells why.

The dialog has these choices:

- **Format:** 3MF, STL (binary), STL (ASCII) or OBJ.
- **Unit:** mm, cm, m, in or ft. The bodies keep their size; only the
  numbers in the file change.
- **Structure:** one file, or one file per body (with several bodies).
- **Refinement:** Low, Medium or High, as in Export, or Custom: the
  surface deviation, the normal deviation (the angle between neighboring
  triangles) and the maximum edge length. Longer edges are split, which
  adds triangles but keeps the shape. None is no limit.
- **On its print bed face:** each piece lies on its body's print bed face
  and is centered on the plate, as Send to Slicer does ([The print bed
  face](#the-print-bed-face)). One file is one piece; with one file per
  body each body lies on its own face, and a body without one on the
  face of [the automatic orientation](#the-automatic-orientation). The
  box is unavailable when no body has a usable print bed face; its
  tooltip tells why.

OK asks for the file, or for the folder of the files. The proposed name
is the body's name for one body, the component's name for one
component, and the design's name for several. With one file per body
each file has its body's name, made unique when two bodies share a name.
Characters that a file name cannot hold are replaced. The design does
not change.

**File › Export Drawing** writes the selected bodies, or every visible
body, as a drawing for the workshop or a supplier. The drawing has
front, top and right views with sizes, hole diameters and a hole table.
The formats are PDF, SVG, DXF and DWG ([Drawings](#drawings)).

### Base features

An imported file comes onto the timeline as a **base feature**. A base
feature holds bodies without a history. STEP and IGES bodies keep their
names and colors. STL and OBJ bring mesh bodies. Mesh bodies cannot
take part in boolean operations. Later features can still use an
imported body's faces and edges, for example a fillet.

A mesh body has triangles but no surfaces. Thus these features do not
offer it: Combine, the face tools, Fillet, Chamfer and Split Body. The
Join, Cut and Intersect of Extrude and the other features do not offer
it either. A feature that names a mesh body fails with a message that
names the body. An
extrusion that cuts or joins all bodies does not change mesh bodies.
You can still move, copy, pattern, mirror, measure, section and export
mesh bodies.

### .f3d import

An `.f3d` design file opens with its history. An `.f3z` package of
`.f3d` files opens in the same way. Use **File › Open** or Import, or
`mitcad-cli import-f3d`. The import replays the design's timeline as
MitCad features. You can undo the whole import as one step. Save the
result as a `.mitcad` design.

These items are replayed as MitCad features: parameters, sketches with
constraints and dimensions, and construction planes. Also extrudes,
revolves, sweeps, lofts, pipes, fillets, chamfers, holes, threads,
patterns, mirrors, splits and Combine. After every feature, MitCad
compares the bodies with the file's bodies. A feature that MitCad
cannot replay is imported as a base feature of the file's bodies at its
place. The timeline continues on those bodies. Thus the result always
has the file's geometry. Ribs and webs are always imported in this way.

Components and occurrences are imported at the file's placements.
Joints, as-built joints, joint origins, rigid groups, grounded
occurrences and captured positions are imported as MitCad's own. This
occurs where they hold at the positions in which the file puts the
parts. A joint that does not hold there is imported as an as-built
joint. Thus nothing moves. Components inserted from other designs, for
example fasteners, are imported as empty components. Such a component
is named after the item that inserted it. Its bodies are not in the
file, but its joints and positions are.

Mesh bodies of the design (scanned or inserted meshes) are imported as
MitCad mesh bodies, with the file's triangles at the file's placement.
They are in a base feature at the place of the item that made them, in
its component. A mesh that MitCad cannot read is left out, and the
report warns with the reason. A mesh body keeps the file's material and
appearance, as the other bodies do. `mitcad-cli import-f3d
--bodies-only` brings the mesh bodies in as well.

Bodies and faces keep the file's appearances. Each appearance that the
design uses becomes an appearance of the design. It keeps the file's
name, color, roughness, metal and transparency. MitCad gives it to the
bodies, and to the faces that the file painted on their own. An
occurrence's appearance goes to its bodies where its component is
placed once. A body's material becomes MitCad's material where MitCad
has the same one (steel, aluminum and others). The report lists every
material with its density. Texture images are not in the file. Thus an
appearance with a texture is imported with its stored color.

A progress dialog shows which item MitCad replays. In the application
the import has no time limit. It runs to the end, however long it
takes. A crash of the geometry kernel does not close the window. MitCad
stops a geometry kernel operation that does not return in 90 seconds.
That item is imported as the file's bodies, and the report warns. The
dialog can end the import early:

- **Stop and Keep What Is Imported**: the items replayed so far stay.
  The remaining features are imported as the file's bodies, that is,
  base features. Sketches and construction geometry are still imported
  as such. The design is complete, but less of it is parametric. The
  stop takes a moment.
- **Cancel Import** (or Esc): MitCad discards the import.

When a feature does not build on the imported bodies, the import tries
it again on the bodies stored in the file. This second try can replace
bodies that were already imported correctly. A body is correct when it
matches the file's body in the import tolerance. In that case, a
dialog asks once per import:

- **Keep Imported Bodies** (the default): those bodies stay parametric.
  If the feature still fails, it is imported as the file's bodies.
- **Replace with Stored Bodies**: the stored bodies replace them, so
  that the feature can build on the stored bodies. The features before
  it then no longer shape the replaced bodies.

The import report lists, for each feature tried again, the bodies kept
and the bodies replaced.

The *Import tolerance* setting (Preferences › General) is the distance
that replayed geometry can be from the file's geometry where it is not
exact. MitCad finds the edges and faces that the file names in this
tolerance. A feature's new faces must lie on the file's faces in this
tolerance.

A feature's own definition can build, but with a result that differs
from the file's result. An example is a fillet that removes 9 % more
volume than the file's fillet. Such a feature is also imported as the
file's bodies. After you close the import report, the dialog **Keep
Deviating Results?** lists such features, one row each. Each row tells
how far the feature deviates at that place:

- The volume it adds or removes, against the file's
- The difference in mm³ and per cent
- How far its new faces lie from the file's faces
- How much of their area is farther than the import tolerance
- The volume of the bodies it changed, against the stored bodies

A fillet that does not build with all its edges can be listed as built
without some of them. The row says how many edges. Its tooltip says
which edges. When you select a row, MitCad selects the faces where the
feature deviates. Thus the view shows where. For each row, select one
of these:

- **Use stored** (the default): MitCad uses the file's geometry in the
  feature's place, as imported.
- **Keep parametric** (check the row, or Space): the feature stays
  parametric with its own result. The features after it are built on
  that result.

**Keep All Parametric** and **Use Stored for All** decide every row at
once. **Apply** runs the import again and keeps the checked rows
parametric. The dialog comes again for the features that still deviate
after them. A feature kept parametric carries a note on the timeline
(*Deviation from the file accepted at import: …*). The import report
lists it with its numbers.

On the command line, `mitcad-cli import-f3d --accept-deviations all`
accepts every deviation. A list of timeline indices, for example
`88,93`, accepts only the deviations at those indices. By default the
command keeps the file's geometry, and the report lists the deviations.

After the import, a report shows how each item was imported:

- As a parametric feature
- Partly, for example a sketch without its unsupported constraints
- As the file's bodies
- Not at all, for example assembly relationships

A sketch whose constraints and dimensions do not solve to the file's
geometry keeps them. MitCad finds the constraints and dimensions that
the file does not meet and makes only those inactive (refer to
[Constraints](#constraints)). All the others stay active, and the
dimensions keep their parameters. The report names each inactive one,
for example "1 constraint inactive, not met in the file: coincident k7
(p3 on c5)". The sketch is then marked partial.

The report also tells how well the final bodies match the bodies stored
in the file. How well real designs import:
[core/import/README.md](../core/import/README.md). How the timeline
shows what was imported in which way: [Reading an imported
timeline](#reading-an-imported-timeline).

### Reading an imported timeline

The `.f3d`, `.ipt` and FreeCAD imports leave marks on the timeline for
what their report says. Thus the design tells it also after you save
it and open it again. These notes are information: they are not
warnings. Their cells have the information badge (a blue circle with an
"i"), not the yellow of a warning. The import itself reports what it
left out once, as a warning on the status line with the count, and in
its report. These are the marks:

- **Information badge, a base feature:** a fallback. Its tooltip names the items
  of the file that it replaces, and why each one did not replay
  ("Stands in for Extrude5 (its result is not in the file's history)").
  A base feature can bring the bodies that differ from the file's,
  after an approximate item or at the end. Such a base feature says
  so. Bodies that the import made in 0.01 mm of the file's are never
  replaced.
- **Information badge, another feature:** the feature was imported partly. The
  tooltip says what was not imported ("Partly imported: left out: 3
  constraints …").
- **Struck through with an information badge:** a placeholder. It is named
  after an item that the file had suppressed, or that failed in the
  file ("Suppressed: Suppressed in the file"). It builds nothing. It
  shows where the file had the item. Items with no geometry
  (occurrences, groups) get no placeholder.
- **Struck through with an information badge, a real feature:** an `.f3d`
  item suppressed in the file, imported with its definition
  ("Suppressed: Suppressed in the file; not checked against the file's
  history"). Unsuppress it to build it, as in the original design. The
  file keeps no result for it, so the import could not compare it. If
  it uses suppressed items before it, unsuppress those items too. An
  example is a fillet of the edges of a suppressed extrusion. Where its
  definition was not imported, it is a placeholder that says why.
- **Rolled back, a real feature:** an `.f3d` item after the file's
  timeline marker, imported with its definition ("Rolled back in the
  file (after its timeline marker); not checked against the file's
  history"). The timeline marker is before it, as in the file. Move the
  marker past it to build it. Where its definition was not imported, it
  is a placeholder that says why.
- **Red:** a feature that failed in MitCad, as anywhere else. Only
  these count in "n failed". The yellow "n warnings" beside it lists
  the features that built with warnings. A click on it goes to them.

If you edit such a feature so that it still builds, its note goes
away. You can edit its definition or one of its dimensions. Undo brings
the note back. On a long imported timeline, the [timeline
search](#timeline-filter-and-search) finds them:

- `is:note` (or `is:info`) finds the features with an import note
- `is:warning` finds the yellow ones
- `is:failed` finds the red ones
- `in:<component>` finds those of one component

### FreeCAD import

A FreeCAD document (`.FCStd`, FreeCAD 0.19 to 1.1) opens as a new
design with its history. Use File › Open or Import, or `mitcad-cli
import-fcstd`. The import does this:

- Each PartDesign Body and each final result of the Part workbench
  becomes a body named after its label. The bodies are in components
  for the document's parts (App::Part, Assembly) at their placements.
  Links (App::Link, also arrays and links to other documents next to
  it) are occurrences. Hidden objects stay hidden.
- Features are imported as MitCad features. Each one is compared with
  FreeCAD's stored result. These features are imported: pads and
  pockets, revolutions and grooves, fillets, chamfers, holes, mirrors,
  linear and polar patterns, and PartDesign booleans. Also drafts,
  thickness (shell), lofts, pipes (sweeps), helices, primitives, datum
  planes, lines and points. Also the Part workbench's primitives,
  extrusions of sketches (also into faces), of faces and of edges (for
  example a circle or a polygon: its edges are fixed curves in a sketch),
  faces of sketches, revolutions of sketches, cuts, unions, commons, fillets,
  chamfers and mirrorings. A feature that MitCad cannot rebuild, or
  whose result differs, is imported as FreeCAD's result. That is a base
  feature, `fallback` in the report. Later features continue on it. The
  report gives the reason and the kind of fallback: missing in MitCad,
  no parametric counterpart (a scripted object, for example a Draft
  shape), or other. If a Body's tip is before its last feature, the
  features after the tip are suppressed.
- Sketches are editable, with their constraints and dimensions, on the
  plane or face to which they were attached. A sketch can have a
  constraint that MitCad cannot express, for example a tangency to an
  ellipse, Snell's law, or a distance to a circle. Such a sketch is
  imported without that constraint and is marked partial. Constraints
  and dimensions that FreeCAD's solution does not meet are imported
  inactive, as in the .f3d import (refer to [.f3d import](#f3d-import)).
- A sketch's external geometry becomes linked projections where MitCad
  can follow it: edges, faces and vertices of the bodies, and curves and
  points of an earlier sketch, also through a shape binder of that sketch,
  also of another body that has a component of its own. These follow
  their sources when you change the design. Other external geometry is
  fixed in place, and the report says why.
- Shapes without a history (plain shapes, Draft objects, bodies of
  other documents) are base features.
- FreeCAD's colors come in as appearances of the design, on rebuilt
  bodies too: each body gets its color, and faces of another color get
  theirs. Each color is one appearance, named `FreeCAD colour` and its
  red, green and blue values, so you can change all the bodies and faces
  of one color in one place. Bodies in FreeCAD's default gray keep
  MitCad's look. Materials are not imported.
- A fillet or chamfer that rounds all edges in FreeCAD rounds all edges
  of the body in MitCad too, also edges that you add later by editing a
  sketch. Fillets on edges that FreeCAD merged follow the faces along the
  edge, so they stay on that edge when you change a sketch dimension.
- Spreadsheet cells, VarSets and named constraints become parameters.
  Expressions that drive imported values become MitCad expressions.
  Assembly joints are not imported.

The import report says:

- How many features were replayed, imported as FreeCAD's results, or
  skipped
- Which parameters and expressions were imported, and why some kept
  FreeCAD's values
- What each object became, or why it was not imported
- Which sketch binders were carried through linked projections or direct
  use of the bound sketch. These are information naming the binder and
  the way its sketch was carried; a binder whose geometry was lost still
  gives a skip warning.

On the timeline, FreeCAD's results are yellow base features. Partial
sketches and features are yellow. Features suppressed in FreeCAD are
placeholders ([Reading an imported
timeline](#reading-an-imported-timeline)).

### .ipt import

An `.ipt` part file opens as a new design with its history. Use File
› Open or Import, or `mitcad-cli import-ipt`. The import brings in:

- The parameters with their expressions. Edit Parameters lists them.
- The sketches with their constraints and dimensions
- Offset work planes
- The extrusions, revolutions, holes, fillets, chamfers, patterns and
  mirrors, as MitCad features

Each feature is compared with the bodies that the file stored after it.
Some features cannot be replayed: a face draft, a shell, sheet metal,
or a mirror that MitCad does not reproduce. Such a feature is imported
as the bodies that the file stored after it, a base feature. Thus the
final bodies are always the file's.

The design takes the part's length unit. The bodies take the part's
material when MitCad's material library has a material of the same
name. The part's properties (part number, material, title, author, the
user's own) become the design's properties ([Properties](#properties)).

The import report lists:

- Each feature and sketch, with how it was imported
- The parameters and expressions
- The bodies: solid or sheet, valid, faces
- The part number, material, units and the release that saved the file

The import runs in a process of its own, as the other design imports
do. Cancel Import ends it. `mitcad-cli import-ipt --bodies-only` imports
only the stored bodies, each as a base feature. Files of older releases
whose bodies are not stored in the ASM format are not read. The
timeline shows the fallbacks, partial items and suppressed features as
it does for an `.f3d` import ([Reading an imported
timeline](#reading-an-imported-timeline)).

### .iam import

An `.iam` assembly file opens as a new design. Use File › Open or
Import, or `mitcad-cli import-iam`. Each part file that the assembly
places becomes a component. Each placement of that part file becomes an
occurrence of the component, where the file places it. Sub-assemblies
become components of their own. MitCad finds the referenced files where
the assembly saved them, relative to the assembly. Or it finds them by
name in the assembly's folder tree. A missing file leaves an empty
component. Its occurrences are placed.

Before the import starts, the **Import Assembly** dialog asks how the
parts come in. **Components with their design history** is on by default.
With the option on, each part is imported as the [.ipt
import](#ipt-import) imports a part. MitCad replays its parameters,
sketches and features into its component. Thus the part's timeline and
parameters are there when you activate the component or open it for
edit. With the option off, each part is imported as the bodies that its
file stores, one base feature per part. This is much faster for large
assemblies. MitCad remembers the selection for the next assembly.
`--open` and the other ways that do not ask use the last selection.

With the history, the progress dialog shows the part that MitCad
imports (component 3 of 7) and the timeline item that it replays. The replay
of a part can fail. It can also hang in the geometry kernel each time
that the import tries it again. Such a part is imported as its stored
bodies. A part without a design history is imported as its stored
bodies too. If a single feature hangs, the replay of the part takes the
file's bodies for that feature.

The import report lists every file and how MitCad found it. For each
part, the report tells if the part was replayed or if it was imported
as its stored bodies, and why. For a replayed part, it lists the items,
how they came in, how many of them were checked against the file's
history, and the parameters. The report also checks each placement
against the placement that the file shows. It checks each part's bodies
against the range box that the file stores.

### IFC import

An IFC file (`.ifc` or `.ifczip`) opens as a new design. IFC is the
open exchange format of building models. MitCad reads IFC2X3, IFC4 and
IFC4X3. Use File › Open or Import, or `mitcad-cli import-ifc`. The
import runs in a process of its own. Cancel Import ends it.

The structure of the building becomes components:

- Each site, building and storey becomes a component, placed where the
  file places it.
- A storey's elevation becomes a construction plane in its component.
- A grid's axes become construction axes.

Each element (wall, slab, beam, column, door, window, furniture and
others) goes into its storey's component. Its bodies are named by its
name and GlobalId, and colored as the file colors them. The shape of
an element is imported by its kind:

- Walls, slabs, beams and other swept shapes become sketches and
  extrusions, or revolutions, sweeps and pipes, with their clippings
  and boolean cuts. You can edit these like any feature.
- Breps become the bodies that the file describes.
- Tessellated shapes become mesh bodies.

Openings are cut from their walls and slabs. When a type gives one
shape to many elements (doors, windows, columns, furniture), the shape
is one component, and each element is an occurrence of it. Spaces are
not imported. Each element's property sets and quantities, and those of
its type, become its properties ([Properties](#properties)).

The import report has a line for every element of the file. The line
says how the element was imported, or why it was not. The report
compares the bodies' volumes with the volumes that the file states for
its elements. What did not build stays on the timeline as a note
([Reading an imported timeline](#reading-an-imported-timeline)).

### .rvt import

An `.rvt` building project opens as a new design of reference
geometry. Use File › Open, or `mitcad-cli import-rvt`. The design has
one component per level, named as the level. In it is a body for each
building element: walls, floors, roofs, columns, beams, doors, windows,
furniture, fixtures, ducts, pipes and others. Each body is named
`Family:Type:Id`, as the project names the element.

Walls, floors, roofs, ceilings, columns, beams and stair runs are
rebuilt as features. Every other element (doors, windows, furniture,
curtain panels and mullions, railings and others) is a mesh of the
shape that the file keeps of it. A stair repeated up a multistory stair
is imported on every storey on which it is repeated. Its runs,
landings, supports, rails and their supports are copied. Each copy is
named after its level (`… (on L4)`).

An IFC export of the project next to it, of the same name
(`project.ifc`), is used to compare the elements. It also replaces an
element whose shape the file does not keep. Without an IFC export, each
element built as features is compared with the shape that the file
keeps of it. An element that differs is imported as that shape's mesh.
Linked models are found next to the project, in a folder of the
project's folder (such as `Links`), or where the project stores them.

Only an element of which nothing is kept is imported as its box. Such a
box is hidden unless the element is structural (a wall, floor, roof,
ceiling, stair run, column or beam). To see it, show its body in the
browser. Annotations, rooms, levels, grids, types and group definitions
that are placed nowhere are not imported. The timeline's yellow badge
notes how many bodies are meshes and boxes. An `.rfa` family file opens
with its metadata only. Its forms are not imported yet.

The import report:

- Counts the items that are not imported
- Lists the bodies per level and category
- Says for each box why
- Names the release that saved the file

Each element's bodies carry its identity (element id, GlobalId,
category, family, type, level), its instance parameters and its type's
parameters as properties ([Properties](#properties)).

### SolidCraft import

A SolidCraft design (`.sc` or `.solidcraft`) opens as a new design with
its history. The file is plain JSON or SolidCraft's container. MitCad
reads designs of every file format version. Older designs are upgraded
as SolidCraft upgrades them. Use File › Open or Import, drag the file
onto the window, `mitcad-cli import-solidcraft`, or the MCP server's
`document_open`. The file keeps the design's definitions, not its
bodies. Thus MitCad builds every feature itself.

Other programs save `.sc` files too. Thus MitCad opens a `.sc` file only
when its content says that it is a SolidCraft design (its format key).
It refuses any other `.sc` file with "Not a SolidCraft design" and the
reason, and imports nothing.

The import does this:

- Parameters keep their names and expressions. A sketch dimension and
  a feature's input keep the parameter name that the design gave them
  (`d3`). Thus Edit Parameters lists the same names, and a change
  rebuilds the design.
- Sketches are editable, with their constraints and dimensions, on the
  plane or face on which the design put them. Curves projected from
  bodies are imported fixed where they are. A few constraints have no
  MitCad counterpart: a line's angle to the sketch's axis, an arc's
  sweep, a slot's overall length. These are not imported, and the
  sketch is marked partial.
- These items are imported as MitCad features: extrusions,
  revolutions, fillets, chamfers, holes, threads, patterns, mirrors,
  shells, drafts, lofts, sweeps, pipes and coils. Also ribs, embosses,
  primitives, combines, boundary fills ([Boundary
  Fill](#boundary-fill)), moves, scales, splits and face features.
  Also patches, stitches, thickened surfaces, construction planes, axes
  and points, imported STEP bodies and meshes. Conversions of meshes are
  imported as [Convert Mesh](#convert-mesh). Faces and edges are found
  by the names that the design gave them and the points at which they
  were picked (clicked). After that, MitCad's own names identify them.
  Bodies keep the design's names.
- Components and their occurrences are imported at their placements.
  Joints are imported as MitCad joints where they hold at the positions
  in which the design puts the parts. Other joints are imported as
  as-built joints.
- Sheet metal rules, base flanges, contour flanges, edge flanges, hems,
  folds, unfolds, refolds, conversions to sheet metal and perforations
  are imported as MitCad's sheet metal features. A conversion's
  thickness is the one that the design takes from the body.
- Bounding solids are imported as MitCad's own ([Bounding
  Solid](#bounding-solid)), around the same bodies by the same margin,
  with the names that the design gives their boxes.
- Bosses, lips, grooves, snap fits and rests are imported as MitCad's
  [plastic part features](#plastic-part-features). The plastic rule
  that applies to their body is imported as a plastic rule, from which
  they take their draft and clearance. That rule is the one assigned to
  the body, else the design's active rule.
- Features with no MitCad counterpart (surface modeling) keep their
  place on the timeline as suppressed placeholders with a note. The
  timeline continues after them.
- Contact sets and motion studies are imported as MitCad's own ([Motion
  studies and contact sets](#motion-studies-and-contact-sets)). If
  contact is off in the design, its sets are imported suppressed. "All
  contact" becomes a set of every component. A study's keys of a joint
  that was imported as an as-built joint, or not at all, are not
  imported, with a note. Exploded views are not imported.
- Motion links (gears, racks and pinions, screws, cams) are imported as
  MitCad's own ([Motion links](#motion-links)). Sizes picked on round
  faces stay picked. The links keep the parts where the design puts
  them. A link whose joints were not imported is not imported, with a
  note.

The import report lists every feature, joint and motion link with how
it was imported. The ways are: parametric, partial with what was not
imported, without a MitCad counterpart, or failed with the reason. The
report also lists the kinds without a counterpart, the sketches'
constraint counts, and the bodies' volumes and boxes ([Reading an
imported timeline](#reading-an-imported-timeline)).

### DWG import

A `.dwg` drawing (R13 to R2018) opens as a new design with its 3D
content. Use File › Open or Import, or `mitcad-cli import-dwg`. A
drawing keeps no history. Thus everything is imported as bodies:

- 3D solids become bodies, built from the B-rep that the drawing
  stores. Bodies, regions and surfaces that keep a B-rep become bodies
  too.
- 3D faces become mesh bodies. The 3D faces of one block on one layer
  in one color become one mesh body. Polyface, polygon and subdivision
  meshes become a mesh body each. A subdivision mesh becomes its
  control mesh, unsmoothed.
- Each named block with 3D content becomes a component, named as the
  block. An occurrence places it for every insert of it. An array
  insert gives one occurrence per cell. Blocks inside blocks keep their
  nesting. Thus a truss segment inserted many times is one component
  with many occurrences. Occurrences only move and turn. Thus an insert
  that scales or mirrors its block, and an anonymous block, are copied
  into the component that inserts them instead.
- At most a million bodies and occurrences are placed, counting the
  contents of every occurrence. A drawing that places more, for example
  through very large or nested array inserts, imports the first million;
  the report says that the rest were left out.
- The bodies of a component on one layer are one base feature named
  after the layer (`Layer Tubes`, in a component `Segment layer
  Tubes`). The bodies take the entities' colors: their own, their
  layer's, or the block's that inserts them.
- The drawing's units (`$INSUNITS`) become the design's units. A
  drawing without units is read as millimeters.

Lines, arcs, text, dimensions, hatches and the other 2D entities are
not imported. Paper space is not imported. DXF files go into sketches
([Import](#import)). Drawings saved with a password cannot be read. The
import runs in a process of its own.

The import report:

- Counts the entities taken by type
- Names every type not imported, with the count and why
- Lists the components with their occurrences
- Makes sure that every body built and is valid
- Makes sure that the bodies lie in the drawing's extents (`$EXTMIN`,
  `$EXTMAX`)

## Outputs

This chapter tells how to get a design out of MitCad for other uses.
The outputs are files for a slicer, a USDZ package for AR and 3D
viewers, a drawing on a sheet, and web pages for people without MitCad.

### 3D printing

#### Tuning printed fits

New Thread, Gear, Pulley and Bearing panels suggest parameter names in
their clearance fields, and a new Hole suggests one in its press fit's
*Interference*. The ordinary user parameters are created when
you accept a feature that uses them. Cancel leaves the parameter list
unchanged, and one Undo removes both a new feature and its new parameters.

Open **Edit Parameters** to tune fits for your printer. Use
`thread_radial_clearance` (0.1 mm) for dimensional accuracy and
`thread_flank_clearance` (0.1 mm) for overhang quality;
`shaft_bore_clearance` (0.2 mm) for gears and pulleys seated on shafts;
and `gear_backlash` (0.2 mm) for the space between mating gear teeth.
Printable bearings use `bearing_radial_clearance` (0.4 mm),
`bearing_axial_clearance` (0.4 mm), `bearing_element_gap` (0.6 mm) and
`bearing_fit_clearance` (0.5 mm). Press-fit holes use
`press_fit_interference` (0.1 mm): how much smaller than the pin a hex's
or square's flats or the crush ribs' tips are. Flat-roofed holes use
`hole_roof_sag` (0.4 mm): how far above the circle the roof's bridge is
printed. Their comments explain which printer
quality affects each fit; bearing gaps are especially sensitive to circle
accuracy and ringing.

Changing one parameter updates the features that use it. A parameter
already in the design keeps its value, unit and comment. You can replace
any suggested name with a number or another expression in the panel.
Existing features and imported numeric values keep their stored values.

#### Sending bodies to a slicer

**Send to Slicer** (Make › 3D Print; once called 3D Print) sends bodies to a slicer. The slicer can be Bambu Studio,
OrcaSlicer, PrusaSlicer, UltiMaker Cura or any program that opens files
given on its command line.

The panel starts with the selected bodies in **Bodies**. A selected
face, edge or vertex stands for its body. A component's occurrence
selected in the browser stands for the bodies shown in it. If nothing
is selected, the panel starts with every visible solid body and mesh
body. On the Make tab, **Send to Slicer** in the selected body's card
([The selected body on the Make tab](#the-selected-body-on-the-make-tab))
starts it with that body alone. MitCad highlights the bodies that it will send. While the panel
is open, click a body in the view to add it or to remove it. A window
selection adds several bodies.

MitCad cannot print surface bodies or empty bodies, and you cannot
select them. If such bodies were selected, MitCad leaves them out.

The panel also has these options:

- **Bed Face:** the face that will lie on the print bed. MitCad fills it
  when you choose the bodies: the body's print bed face when it has one
  (*Plate: print bed face*), else a face that MitCad chooses itself
  (*Plate: automatic*, see [The automatic orientation](#the-automatic-orientation)).
  The face is highlighted in green while the panel is open; the bodies
  sent stay highlighted in blue. To use another face, click the
  **Bed Face** row and then a flat face of a body you send: it becomes
  the body's print bed face (one undo step), so the next send uses it
  too. When the bodies have no flat face to lie on, the row says
  *As designed, −Z down*. In a large design MitCad may take a moment to
  work the face out: the row says *Planning…* meanwhile, and the panel
  stays usable. If you press **Send** or Enter before it is done, MitCad
  sends as soon as the face is known.
- **Center on Plate** and **Drop to Bed:** both on until you change them,
  and MitCad remembers them for each design. Center on Plate puts the
  middle of the piece over the middle of the plate; Drop to Bed puts its
  lowest point on the bed. They apply to every piece, also to bodies
  without a print bed face and to bodies kept as they are assembled.
  Turn both off to send the bodies where the design has them.
- **Format:** *3MF* (the first time) or *STL*; MitCad remembers your
  choice.
  - With *STL*, MitCad writes one file for each
    body. A body can be shown more than once, in a component placed
    several times. Then each occurrence that you selected gets a file
    of its own, for example `Pin (Arm_2).stl`.
  - With *3MF*, MitCad writes one file. In the
    file, the bodies are the parts of one object, with their names and
    their appearance colors. Each material and appearance is a filament
    of the slicer (see
    [A filament's color is its appearance](#a-filaments-color-is-its-appearance)).

  Both formats turn the bodies onto the bed face and put them on the
  plate as Center on Plate and Drop to Bed say (see [The print bed
  face](#the-print-bed-face)). 3MF is the reliable way to get one object of several parts in the
  slicer. UltiMaker Cura loads the parts as a group.
- **Import from Slicer…**, **Printer**, **Process**, **Own settings**
  and **Materials** (3MF only, grayed out with STL): Import from
  slicer… first, above the design's printer and process, then the
  bodies' own process settings (whatever the process, Slicer default
  too: the slicer gets them all the same) and the bodies' materials (see
  [Print settings](#print-settings)).
- **Refinement:** Low, Medium, High (the default) or Custom. The
  refinement sets how far the triangles can lie from the surface (mm)
  and the angle between neighboring triangles.
- **Open in:** *Folder only* (MitCad opens the folder of the files), or
  the slicer of Preferences (3D Print, **Open in**). MitCad finds it
  automatically where possible. You can also select another program,
  for example an AppImage or, on macOS, an application in the
  Applications folder.
- **Use the open slicer window:** on until you turn it off; MitCad
  remembers your choice. When MitCad already started the slicer for
  this design and its window is still open, Send uses that window
  instead of opening another one. PrusaSlicer and UltiMaker Cura get the
  new file in their open window. Bambu Studio, OrcaSlicer and QIDI
  Studio get it too when their own preference to keep only one instance
  is on. Otherwise MitCad brings the slicer's window to the front and
  says that the file was updated: open it there again. MitCad only uses
  slicer windows that it started itself, never a window you opened. Turn
  the option off to get a new slicer window on every send.

**Send** (Enter) writes the files into `mitcad-print/<design>` in the
temporary folder. MitCad empties that folder before each send. Then
MitCad gives the files to the slicer. If there is no slicer, or if the
slicer does not start, MitCad opens the folder instead. MitCad
remembers your choices.

MitCad finds slicers in these places:

| Platform | Where MitCad finds slicers                                 |
|----------|-----------------------------------------------------------|
| Windows  | The usual install folders and the list of installed programs |
| macOS    | The Applications folders. An application gets the files through `open -a`. |
| Linux    | `PATH`, Flatpaks and desktop files                        |

#### The print bed face

A body can have a face that lies on the print bed. Right-click a planar
face of a body and choose **Set as Print Bed Face**. A body has one
print bed face: a new one replaces the old one. Right-click the print
bed face and choose **Clear Print Bed Face** to remove it. Each is one
undo step.

The print bed face belongs to the body. Thus it applies to every
occurrence of the body, it is saved in the design, and it stays the
same face when you edit the body. While the context menu of another
face of the body is open, the print bed face is highlighted in green.
The tooltip of the body in the browser names the face, for example
`Print bed face: end face of Extrude1`.

**Send to Slicer** uses the print bed face. The design itself does not move:

- A body sent alone, or bodies of which only one has a print bed face:
  MitCad turns them so that the face lies on the bed, puts the lowest
  point on the bed, and centers them on the plate. The plate is the bed
  of the design's printer (see [Print settings](#print-settings)), else
  a 256 × 256 mm plate. A component that is placed several times, only
  moved, turns the same way in every place.
- Several bodies that each have a print bed face, or a body placed
  several times, each turned differently (a nut in an assembly): with
  3MF, each body in each place is an object of its own, turned onto its
  print bed face and set beside the others on the plate. Bodies without
  a print bed face stay as they are assembled, turned as a whole by the
  automatic orientation.
- A component you selected before **Send to Slicer** is sent whole: its bodies
  stay as they are assembled, one object, turned onto the print bed face
  when one of them has one and every place turns it alike.
- When such a piece has two or more print bed faces (counted in each
  place: a nut placed twice, turned differently, has two), **Send** asks
  which one before it writes any file. The print bed faces of the piece
  are highlighted in green, in every place, and the panel and the status
  bar say what is asked, for example *Not sent yet: Pair:1 has 2 print
  bed faces. Click the green one to put on the bed.* **Send** is
  disabled until you choose. Point at a green face to see its body in the
  status bar (*Tall: print bed face*), and click it: MitCad turns the
  whole piece so that this face lies on the bed, the bodies staying
  where they are to each other, and sends it. A click on another face
  does nothing but say *Not one of the print bed faces: click a green
  face.* With the keyboard, Tab picks the green faces in turn (the one
  picked in blue, named in **Bed face**) and Enter sends on it. Click
  **Send as assembled** beside **Bed face** to send the piece as it is
  assembled. Esc ends the question without sending anything, and the
  panel stays open; a second Esc closes it, as everywhere. Changing
  **Format** or **Bodies** while it asks ends the question too. When two pieces each have several print bed
  faces, they are asked about one after the other, each by its name.
  The choice holds until the panel closes, so a second **Send** does not
  ask again. You can also choose before **Send**: pick one of the faces
  in the **Bed Face** row. With one print bed face, or none, nothing is
  asked.
- Only the places you picked are sent: select one occurrence of the nut,
  and Send to Slicer sends that nut alone, on its print bed face. With nothing
  picked of a body, every place where it is shown is sent.
- With STL, each file is turned onto the print bed face of its body.

The status line says how many bodies lie on their print bed faces, for
example `Sent 3 bodies to Bambu Studio. 3 bodies on their print bed
faces.`, and names a face you chose: `Sent 2 bodies to Bambu Studio.
Pair:1 on Tall's print bed face.`

If the print bed face is gone after an edit, or is no longer planar,
MitCad turns the body by the automatic orientation instead. The status
line tells which body, for example `Body1: the print bed face is gone`.
The tooltip in the browser says *gone* or
*not planar*, or another reason, for example when the face was renamed
by an edit and MitCad cannot find its plane. Then **Clear Print Bed
Face** is in the context menu of every face of the body, or choose a new
print bed face.

Surface bodies have no print bed face. MitCad does not print them:
**Send to Slicer** leaves them out. If a body with a print bed face becomes a
surface body, for example after **Unstitch**, the body keeps the face,
but MitCad does not use it. The tooltip in the browser says *not used, a
surface body is not printed*. The context menu of a face of the surface
body then offers only **Clear Print Bed Face**.

#### The automatic orientation

When the bodies you send have no usable print bed face, **Send to
Slicer** chooses the face to lie on itself, as slicers do. It looks at
the bodies of a piece together:

- Only flat faces that the whole piece can stand on count: the piece
  lies on the face without anything going below it.
- Of those, MitCad weighs three things: the area left hanging over
  (faces that face down more steeply than 45 degrees from the vertical,
  which need support) counts most, a large contact with the bed half as
  much, and a low piece a little. A tube with a flat flange stands on
  the flange; a block lies on its largest face.
- When faces are equally good, the face that already faces down in the
  design wins, so a body that lies well stays as it is.
- When no flat face can carry the piece (a sphere, a mesh body), MitCad
  sends it as the design has it, with −Z down.

MitCad does not store this choice: it chooses again on every send, so a
print bed face that you set later always wins. The **Bed Face** row and
the log say *automatic* for such a face.

#### Print plates

A design can carry its print plates, so that the slicer gets them laid
out and ready to print, every time you send. **Plates…** (Make › 3D
Print) opens a panel of the design's plates:

1. **Add Plate** adds a plate (*Plate 1*, *Plate 2*, …) and chooses it in
   **Plate**. The chosen plate is drawn beside the design: its bed's
   outline and its bodies where they will lie.
2. Pick bodies in the view (or a component in the Browser) to put them on
   the chosen plate. A component brings all its bodies as they are
   assembled. **Count** sets how many of the chosen item the plate
   prints ("4 of this on Plate 2"); **Take Off Plate** takes it off
   again. A body or component is on one plate at most: putting it on
   another moves it. Whatever no plate names prints on the first plate.
3. **Spiral Vase** prints the plate in vase mode. **Plate Settings…**
   opens the plate's own settings in the same form as the processes:
   layer height, walls, infill and every other process setting.

Each change is a step of its own in the undo history; **Close** keeps
them. MitCad lays each plate out by itself: every piece on its print bed
face (or the automatic orientation's), side by side, centered on the plate.
With **Center on Plate** off, the pieces stay where the design has them,
but the copies of an item with a count still lie side by side, starting
where the design has the first.

Neighbouring pieces lie 5 mm apart beyond their brims, so that the
slicer's brims do not run into each other. MitCad reads each piece's
brim from the settings it prints with (the plate's included): its
**Brim width** and **Brim gap**, unless **Brim type** is *None* or
*Inner only* (*Auto* counts, as the slicer adds a brim to tall, narrow
pieces), or its own skirt when **Skirt per object** is on. Two pieces
with an 8 mm brim lie 8 + 5 + 8 = 21 mm apart. When the pieces and their
brims no longer fit on the bed, MitCad warns, and you can move some to
another plate.

**Settings order.** A plate's values come after the design's printer and
process and before everything else: the design's process < the plate <
a component < an occurrence < a body. So a plate in vase mode prints every
body on it in vase mode, but a body that sets its own layer height keeps
it.

**Sending plates.** With plates in the design, **Send to Slicer** has
**Send the Design's Plates** on: it sends the plates instead of the bodies
chosen. Bambu Studio, OrcaSlicer, QIDI Studio and PrusaSlicer 2.9 or later
get one 3MF with every plate, each plate with its own settings where the
slicer keeps them per plate (the Bambu family's spiral vase and print
sequence); UltiMaker Cura and older PrusaSlicer versions get a 3MF per
plate. PrusaSlicer keeps spiral vase for the whole project, so a plate's
spiral vase reaches it only in a file of its own (MitCad warns).

A connected AI assistant ([Connect an AI assistant](#connect-an-ai-assistant))
can write the same files: it exports the design's plates for the slicer
you name into a folder for designs, one file of every plate or a file
per plate. It never writes over a file that is already there.

#### Print Analysis

**Print Analysis** (Make › 3D Print) shows where a body will print badly before
you send it to a slicer. It also estimates the weight, the filament and
the cost. It changes no body, no feature and no export.

The panel starts with the selected bodies, or with every visible solid
body and mesh body. These are the options:

- **Bed face:** optional. Select a planar face of a body. The face lies
  on the bed, so it sets the print direction. The body does not move.
  A new analysis starts with the print bed face of the bodies, if
  exactly one of them has one (see [The print bed face](#the-print-bed-face)). Without a bed
  face, down is the -Z direction of the design.
- **Overhangs** and **Angle** (45 degrees from the vertical): faces that
  face down more steeply than the angle need support. Faces on the bed
  are not overhangs.
- **Thin walls** and **Minimum wall** (0.8 mm): walls thinner than the
  printer can make.
- **Narrow gaps** and **Minimum gap** (0.3 mm): bodies printed in place,
  such as a hinge and its pin, fuse when they are closer than this. With
  two bodies or more, the analysis lists each pair that is closer than
  the gap somewhere they do not touch. Bodies may touch elsewhere: a
  wheel lying on a plate with its peg in the plate's hole is listed when
  the peg is too close to the hole's wall. Where bodies touch or overlap
  is not a gap: use Interference for it. If the design has the user parameter
  `print_in_place_gap`, the field shows its name and the analysis uses
  its value.
- **Estimate:** the material (the material of the bodies, else PLA), the
  line width, the walls, the layer height, the top and bottom layers,
  the infill, the filament diameter and the price per kg. If the price
  is empty, MitCad shows no cost.

The panel shows the overhang area for each body, and its share of the
area. It shows the thinnest wall and the number of thin regions. Click
the thinnest wall to zoom to it. The estimate looks like
`41.5 g · 13.9 m · 1.04 €`. It is only an estimate: the slicer decides.
The narrow gaps show the two bodies and their distance; click the
distance to zoom to the gap.

In the view, the overhangs are violet, the thin walls magenta and the
narrow gaps teal, lighter on a dark view background. A teal line joins
the closest points of each narrow gap, and a marker shows the smallest
gap. Only the thin part is colored: a thin rim at the
edge of a large flat face shows as a band along the rim, not across the
face. A marker shows the thinnest point. A legend in
the lower left corner shows the colors, the angle and the minimum wall.

**OK** keeps the analysis in the **Analysis** folder of the browser as
`Print1`. MitCad remembers the numbers for the next analysis. Use the
light bulb to show or hide the analysis. Double-click it to edit it. You
can rename and delete it like a section analysis. If a body or the bed
face is gone, the analysis is red and its tooltip tells why.

### USDZ for AR and 3D viewers

File › Export with the type *USDZ for AR and 3D viewers* writes a
Universal Scene Description package. A file name that ends in `.usdz`
selects the same type. The package is one file. AR viewers on phones
and tablets, web viewers, renderers and other 3D programs open it with
the design's look.

**What goes in.** MitCad writes the visible bodies, where the view
shows them. With *Selected bodies*, MitCad writes the selected bodies.
MitCad leaves out hidden bodies and hidden component occurrences.
Components stay a hierarchy. Each occurrence is a group with its
placement, and the group holds its bodies. Thus a viewer can move the
components apart. If Coordinates is set to the components'
coordinates, MitCad writes each body once, as its component has it.

**How it looks.** Each body's appearance becomes a material. The
material has the color, metal or not, roughness, clear coat, emission
and see-through for glass. A body without an appearance keeps its
color or the default gray. Faces with appearances of their own keep
them. A texture's image goes into the file with the projection that
the rendered view uses. If MitCad cannot find an image, it writes the
base color, and the log says so. Curved faces are smooth and edges
are sharp. Refinement sets how finely MitCad divides the faces into
triangles.

**Size and orientation.** The file is in millimeters and says so. The
file stands Y up, as viewers expect. Thus the design shows in AR at
its real size and upright.

MitCad writes the file with its own code. It is not necessary to
install anything else. The command line writes the same file:

```
mitcad-cli export design.mitcad design.usdz
```

### Drawings

A drawing shows the design on sheets of paper: views of it from the
front, the top, the side or a corner, sections and details. A
workshop, a supplier or a printed order needs such sheets. MitCad
keeps each drawing in the design itself, not in a file of its own.

#### Drawings in the design

**Where they are.** Drawings are in a **Drawings** folder in the
browser. The root's folder and a component's folder appear once they
have a drawing. A drawing in the root's folder shows the whole design.
A drawing in a component's folder shows that component. The
component's drawings go with the component: insert the component into
another design, or paste a new copy of it, and its drawings come too.
Delete the component, and its drawings go too.

**Creating one.** Use one of these:

- File › Create Drawing: a drawing of the whole design.
- The context menu of a component in the browser: Create Drawing.
- The context menu of a Drawings folder: Create Drawing.

A dialog asks for the first sheet's **Template** (refer to
[Templates and revisions](#templates-and-revisions)), its size, its
orientation and the projection method. A template brings its sheet size
and its projection method: ISO starts on A3 in first-angle projection,
ANSI on ANSI B in third-angle projection. MitCad names the drawing
Drawing1, Drawing2 and so on, opens it and starts **Base View**: click
where the first view goes.

The dialog opens with the template, size, orientation and projection
method that you chose last, so that you set them only once. At first,
or when the template you chose last is not offered (a template of
another project), it opens with ISO.

**Opening one.** Double-click the drawing in the browser. The drawing
takes the whole workspace: the sheet fills the area of the 3D view from
edge to edge, and the toolbar shows the **Drawing** tab with the
drawing's tools instead of Solid and Build. Above the sheet, one row
shows the drawing's name, its sheets as tabs, **+**, **Sheet…**,
**Add Revision…** and **Edit Parameters…**, which opens the design's
parameter table over the drawing; the drawing follows each change. The browser stays, so you can open another drawing
from it; in the Floating layout the other cards (the timeline, the
Projects card, the Command card) are hidden while the drawing is open.
**Close Drawing**, the first button of the Drawing tab, shows the 3D
view again, with Solid and Build back and the tab you opened the drawing
from shown; only Close Drawing closes the drawing. Esc
ends the active tool, then clears the selection, and leaves the
drawing open. A right-click on the sheet while a drawing tool is on does
what Esc does in that tool, without a menu, as in a sketch: the first
right-click drops what is in progress (a line's first end, the edges a
dimension picked so far) and keeps the tool, and a right-click with
nothing in progress ends the tool. A right-button drag is not a click.
With no tool on, a right-click opens the menu of what is under the
pointer, or the sheet's menu, as before. A command chosen on the toolbar also leads back to the
3D view, but the single-key shortcuts of its commands (E for
Extrude, H for Hole and the others) do nothing while the sheet or its
panel has the keyboard, so a stray key does not leave the drawing.
Undo, Redo and Save keep their shortcuts.

While a drawing is open, Delete deletes the selected views and
annotations of the drawing. It never deletes the bodies or features
selected in the design behind it.

The drawing stores only how its views are set up. MitCad draws the
lines from the design each time. When you change the design, the views
change with it. Undo and redo work for drawings too.

A view of bodies with only flat, cylindrical, conical, spherical and
toroidal faces is drawn from their exact edges. Exact edges are slow on
free-form faces, such as a modeled thread's, and on bodies with many
curved edges where such faces meet, such as a part read from a STEP
file with hundreds of cross holes: MitCad then draws the
view from a fine triangulation of the bodies, within a 2000th of their
size (0.06 mm on a 120 mm assembly), and draws its edges as straight
lines and arcs wherever they follow them, as polylines elsewhere.
Moving a view, or changing its scale or label, projects nothing again.

**Lines.** MitCad draws the lines of ISO 128's lighter set: visible
edges 0.35 mm wide; dimensions, tangent edges and thread roots
0.18 mm; hidden edges 0.18 mm dashed; hatching 0.13 mm; the frame
0.5 mm. A line or an arc drawn by hand can also be extra thick, 0.7 mm
(ISO 128's extra wide line: narrow, wide and extra wide at 1:2:4); a
DXF or DWG file has such lines on a layer of their own, `EXTRA_THICK`.
Its width and its pattern combine: a thick or extra thick line drawn by
hand can be dashed or a center line too, on a layer of its own in a DXF
or DWG file (`HIDDEN_THICK`, `HIDDEN_EXTRA_THICK`, `SECTION_LINE_THICK`,
`SECTION_LINE_EXTRA_THICK`).
The sheet on the screen, the PDF and the SVG file use the same widths. Lines closer together than twice the visible line's width
(0.7 mm on the sheet) would run into a blot, so a view leaves out what
is smaller than that at its scale. A fillet or chamfer smaller than
0.7 mm on the sheet is drawn as a sharp corner: the edges on either
side meet, and its arc and tangent edges are left out. A tangent edge
that runs that close to another edge is left out too, and so is an
edge shorter than the line is wide. At a larger scale, or in a detail
view, the fillet shows again.

**Threads.** A thread is drawn by ISO 6410, not as its geometry,
whether it is modeled or cosmetic. Seen from the side, the crests are
a visible line (a bolt's major diameter, a tapped hole's minor
diameter), the roots a thin line, and a visible line across marks
where the thread ends. Seen end on, the crest is a full circle and the
root a thin three-quarter circle. Where something hides the thread, a
nut or a wall, its lines are hidden like other edges: dashed when the
view shows hidden edges, otherwise left out. Other parts, such as the
nut itself, keep all their lines, and so does whatever else lies within
the thread on the same body: a cross hole through it, a socket in its
end, a flat or a keyway on it, a narrower bore below a tapped hole. Only
the thread's own lines are replaced. An isometric view draws
the thread the same way on its cylinder: the crests' outlines, the
roots' outlines as thin lines, and the visible half of the crest circle
where the thread ends. A tapered pipe thread's lines follow its taper.

#### Views

The toolbar's Drawing tab has the drawing's tools in groups, after
Close Drawing: **Create** (the views' tools below), **Geometry** (center
marks, centerlines, lines and arcs, center patterns and edge
extensions), **Dimensions**, **Text**, **Symbols** (datum labels,
feature control frames, surface finish symbols), **Tables** and
**Export**. Each group's name opens a menu of its tools, as on the
other tabs; the active tool's button shows pressed.

| Tool | What it makes |
|---|---|
| Base View | A view of the design: front, top, right, back, bottom, left, an isometric view from above or below, or *Current View*: the direction the 3D view looks at the design now, with its up. Click where it goes. |
| Projected View | A view seen from one side of another view. Drag it out of the view, or click where it goes. |
| Section View | A view of the design cut along a line. Click the line's two ends on a view, then where the section view goes. |
| Detail View | A circle of a view at a larger scale. Click the circle's middle on a view, then its edge, then where the detail view goes. |
| Break | A view made shorter: a band of it is taken out. Click the band's two sides on a view. |

The hint over the sheet says what the active tool waits for. While you
place a section, a detail or a view projected from another, a dashed
box about its size follows the pointer. The panel beside the sheet
shows the tool's options. A base view's *Shows* list names what it can
show: the whole design (in a component's drawing, the component) by
its name, each occurrence in it (`Arm:1`, `Arm:1/Pin:2`) and each of
its own bodies. A base view also has its orientation, its scale
(*Fit the Sheet* unless you choose one), and whether it shows hidden
edges, tangent edges and shaded faces. When you place a base view,
Projected View starts at once: each click beside the base view places
a view seen from that side. In third-angle projection, a click to the
right of the front view places the view from the right. In
first-angle projection, the view from the left goes there. A click at
a corner places an isometric view. Esc ends the tool.

A section view takes its letters (A, B and so on) and hatches the cut
faces. *Look the Other Way* turns the view round. The section line
with its arrows and letters shows on the view you drew it on. A detail
view is twice the scale of its view. Its circle and letter show on
that view.

**Changing a view.** Click a view to select it. Drag it to move it. A
view projected from another, and a section, stay in line with that
view: they move only along it. A view projected from a detail lines up
with the detail's middle. A view that moves takes the views made from
it along. A break shortens its own view only: the views in line with
it keep their full length. The panel shows the selected view's
properties:

- A base view: its orientation (*Current View* takes the 3D view's
  direction again), and what it shows (*Shows*, the same list as the
  tool's).
- A projected view: the side of its view it is on.
- A section: its letter, its hatching and *Look the Other Way*.
- A detail: its letter, its circle's radius and its circle's center
  (*Center X* and *Center Y*, in millimeters on the sheet). While the
  detail is selected, its circle shows in blue on its view: drag the
  circle to move it.
- Every view: its scale, its edges, its label and *In Line with Its
  Parent*. A base view's scale can *Fit the Sheet*. Another view takes
  its view's scale (a detail twice it) unless it has its own; the list
  shows the scale that gives. Besides the standard scales in the list,
  you can type any ratio of two positive numbers, such as `2:3` or
  `1:1.5`, and press Enter. A text that is not a ratio shows a red
  border and changes nothing. Drawings write a scale as 1:X or X:1
  (ISO 5455), so after Enter the field shows 2:3 as 1:1.5, and so does
  a detail's label. The tools' *Scale* option takes a typed ratio the
  same way.
- Every view but a detail: its **Rotation** in degrees,
  counter-clockwise. The view turns on the sheet about the middle of its
  box. The views made from it turn with it, and so do its dimensions,
  notes and symbols, the lines and texts drawn in it, and the circles of
  its details: they keep showing what they showed. A horizontal
  dimension turned a quarter becomes a vertical one; turned another
  angle, it keeps measuring along the same direction of the design. A
  view with breaks turns by quarter turns only. A detail turns with its
  view. The view's menu also has *Rotate 90° Left* and *Rotate 90°
  Right*.

You can turn a view to another direction while sections or details
are drawn on it or on the views made from it: a base view's
orientation, a projected view's side, *Look the Other Way* of a
section, or the projection method of the sheet. The views made from it
follow. A section line stays where you drew it on its view and cuts
what that view shows there now; a detail keeps its circle where it is
on its view. Dimensions and notes on them follow the edges they picked;
one whose edge the turned view no longer shows is marked broken, as when
the design changes. Delete removes the view and the views made from it.

**Several at once.** Ctrl+click a view or an annotation to add it to the
selection, or to take it out. Delete deletes everything selected, and a
drag of one of them moves them all, each as one undo step.

**The sheet's menu.** Right-click a view for its menu: Projected View,
Section View, Detail View and Break start from it; Hidden Edges, Tangent
Edges, Shaded, Center Marks (and a section's Hatching) switch as in its
panel; Delete View. Right-click an annotation for Edit Text… (a note or
a text), Tolerance… (a dimension or a frame) and Delete; the sheet
itself for Sheet…, Add Revision… and Add Sheet. Double-click a note or
a text to change its text.

**A line of its own.** Right-click an edge of a view and choose *Edit
Line…* to draw that edge with another *Width* (Thin, Thick, Extra Thick)
and *Pattern* (Continuous, Dashed, Center Line, Long Dash Double Dot) in
that view. *Applies To* chooses This Edge, Its Body (every line of the
body, for example an adjacent part drawn long dash double dotted as
ISO 128-3 shows it) or Its Component (every body of the component). An
edge's own choice wins over its body's, and a body's over its
component's; hidden lines stay dashed. The choice follows the edge when
the design changes, and the PDF, SVG and DXF exports and print draw it
too (DXF with line types and line weights). *Reset to View Style* draws
the lines as the view draws its others again.

A view shows the visible bodies of the design or of its component. A
view whose bodies are gone keeps a red box where it was, with the
reason, and the status bar says it too. Select the box to delete the
view, or undo to bring the bodies back.

#### Dimensions and notes

The Geometry, Dimensions, Text and Symbols groups have the annotations'
tools, as icons:

| Tool | What it makes |
|---|---|
| Dimension | A dimension; what you click chooses which (*Type: Auto*). Click one circle for its diameter, one arc for its radius, or one straight edge for its length, then where the dimension goes. Click two edges, circles or corners for the distance between them: where you place it chooses horizontal, vertical or aligned. Two edges that are not parallel give their angle. *Type* can force Linear (horizontal or vertical, after the picks), Aligned, Angle, Radius or Diameter. |
| Ordinate | A point's distance from an origin along the view's x or y. Click the origin, the point, then where the value goes. |
| Baseline | Several dimensions at once: from the first point to each of the others (Baseline) or from each to the next (Chain). Click the points, then a free place. |
| Center Mark | Center lines across a circle, or the centerline of a hole or a shaft seen from the side. Click the circle or the round face. |
| Centerline | A centerline through two circles, or between two edges. Click the two sides of a curved slot for its center arc, through the centers of its ends. |
| Line | A line drawn by hand, outside the design. Click its two ends. On a view a click snaps to the end or the middle of an edge, or to the center of a circle or an arc, near it (the point shows under the pointer; hold **Alt** to click freely). *Drawn: In the View Clicked* keeps it in the view under the first click, so that it moves, scales and turns with the view; *On the Sheet* keeps it on the paper. *Width* (Thin, Thick, Extra Thick: 0.7 mm) and *Pattern* (Continuous, Dashed, Center Line, Long Dash Double Dot) combine freely, a thick dashed line for example; the line's panel changes either later. |
| Arc | An arc drawn by hand: click its center, its start, then its end, counter-clockwise. Its clicks snap as a line's do, so an arc can run from one center to another. In a view or on the sheet, as a line. |
| Center Pattern | Center marks on every hole of a pattern in one step. Click one hole seen end on: MitCad finds the holes of the same size of that body the view shows. On a bolt circle it draws the circle through their centers (an arc when they stand on a part of it) and a line through each; in a row, the line through them; else a center mark on each. |
| Edge Extension | Thin lines that run straight edges on: two edges to where their lines meet (a corner rounded or chamfered away), or one edge beyond both its ends (*Edges: One, Both Ways*; how far, in millimeters of paper, is its *Length* in the panel: 10 unless set). |
| Text | Text on the sheet, or in the view clicked (*Drawn*). Click where it goes. |
| Note | Text on a leader. Click the edge or point it points at, then where the text goes. |
| Datum Label | A datum label (A, B, …) on a face or an axis. The letters of the design's datum labels and the drawings' are one set: a new label takes the first letter no other has. |
| Feature Control Frame | A geometric tolerance on a face or an axis. A dialog asks for it; its datums are the letters of the design's and the drawings' datum labels. |
| Surface Finish | A surface finish symbol on a face, with its requirement (`Ra 1.6`). |

A click picks an edge, a circle, a corner or a round face of the design
as the view shows it; before you click, what the click would pick shows
light blue under the pointer, and what it picked turns orange. Both are
a little wider than the line they mark at any zoom, and the line shows
through them. While
you place a dimension, its lines follow the pointer as it will be made;
a note, a symbol or a balloon shows its leader. A corner or a
circle's center near the click wins over an edge: a circle or an arc
shows its center when you point at it, and a click there picks the
center, also of an arc whose center lies in the open, such as a curved
slot's. So the distance from a slot's center to a hole is a dimension
between that center and the hole.

**Placing a dimension.** After one circle, arc or straight edge, the
dimension follows the pointer at once; a click on another edge, circle
or corner instead makes it a dimension between the two, and a click
elsewhere places it (hold **Alt** to place it anywhere). Between two
picks, place the dimension above or below them for the horizontal
distance, beside them for the vertical one, and between them, or
diagonally beyond them, for the distance straight from one to the
other (from an edge: square to it). Clicking the same edge or circle
twice counts once: it never makes a dimension of 0. The annotation keeps
what it picked, not a place on the paper: when you change the design, the
dimension changes with it. A tool stays on for the next annotation; Esc,
or a right-click on the sheet, drops the picks so far, then ends the tool.

**Dimension spacing:** a new dimension snaps to the spacing of the
dimension you placed before it in the drawing, and to two, three or
more times that spacing. So dimensions stacked beside a view lie evenly
apart, and you need no grid. Place the first dimension at the distance
you want from the view. A linear or ordinate dimension then snaps by
its distance beyond the view's box, an angle by its arc's radius, and a
radius or a diameter on a leader by how far its text is from the
circle. For three dimensions stacked on one side, the spacing is the
step between them, not the distance of the outermost one. While you
place a dimension, its preview jumps to the snapped place and shows the
spacing beside it, such as *12 mm × 2*. It snaps only within about 4 mm
of paper of such a place; elsewhere it goes where you click. A dimension
that you drag snaps in the same way. Hold **Alt** (Option on macOS)
while you click or drop to place it exactly at the pointer.

A view picks what it shows. A section view picks what lies behind its
cut, and the edges of the cut faces: such an edge stands for the face
it cuts. A detail view and a broken view pick an edge where they show
it, also when its middle lies outside the detail's circle or in a
break. Where two edges lie on one another, the one the view draws as a
visible line wins, then the one nearer to you. An arc is picked where
it is, not on the rest of its circle.

On an isometric view, lengths along the paper's x and y and angles
would come out shorter than they are: MitCad refuses horizontal,
vertical and ordinate dimensions and angles there. A dimension on such
a view is aligned and shows the true distance between its two points.

Values are written as the annotations in the 3D view write them: Ø and R
before diameters and radii, the decimals you choose, text before and
after, and a tolerance (±, deviations, limits, or a fit code such as H7).
Click an annotation to select it. Drag it to move its line or its text.
Its panel shows its value and has *Tolerance…* and *Decimals…* (the
same dialogs as in the 3D view), its text before and after, a note's
text, a datum's letter, a frame's tolerance or a finish's requirement,
its **Text Height**, a line's or an arc's pen, and *Delete*. Delete
deletes it too.

**The drawing's decimals and text height.** With nothing selected, the
panel shows the sheet with the drawing's **Decimals** and **Text
Height** (mm). They are for every dimension, note, symbol, table and
text of the drawing that has none of its own: change them once and the
whole drawing follows. A dimension's own decimals and an annotation's
own text height win over them; clear an annotation's text height to
take the drawing's again. *Automatic* decimals write as many as the
value needs, up to three; the text height is 3.5 mm unless you set one.
The dimensions a view retrieves from the sketches take the drawing's
decimals too.

An annotation whose edge or face is gone (the feature deleted or
suppressed, the body no longer in the view) is not drawn. A small red
box beside its view marks it; select it to see why, and delete it or
bring the geometry back.

A view also shows center marks on its holes (half holes too) and
centerlines on its round faces by itself (*Center Marks* in its panel):
on those it draws, so a hole hidden inside the part (a cross hole seen
from the side) gets a centerline only when the view shows hidden lines;
and on request the dimensions of the sketches it sees square on
(*Sketch Dimensions*): those of the sketches that shape the bodies it
shows, where it shows them. Exports draw all of them.

With *Model Annotations* in its panel, a view also shows the
annotations you made on the model (dimensions, notes, datum labels,
feature control frames, material notes and hole callouts) whose plane it
sees square on; a hole callout goes to the view that looks into its
hole. They are drawn as the drawing's own dimensions are and follow the
model: change the part and the values change. An annotation you hid in
the model is not brought in, and each one (like each sketch dimension) is
drawn only once on a sheet, in the first view that sees it.

**Model Items** (Dimensions group) does this for several views at once:
select the views (or none for all the sheet's views), choose *Sketch
Dimensions*, *Model Annotations* or both, and click *Apply*. One Undo
takes it back. To move or edit one of the model's annotations on the
drawing, select its view and click *Keep Model Annotations*: they become
the drawing's own annotations, and the model stays as it is.

Click an annotation on its lines or its text to select it; a click
between its lines picks the view under it.

#### Tables and balloons

The Tables group has the tables' tools:

| Tool | What it makes |
|---|---|
| Hole Table | A table of the holes a view sees end on. Click the view, then where the table goes. Each hole gets a tag in the view: A1, A2, … for the holes of one size, B1 for the next size (or 1, 2, 3 with *Tags: Numbers*). The table lists each tag with the hole's X and Y from the view's lower left corner (or from the component's origin), its size, depth and thread. |
| Bill of Materials | A numbered list of the components a view shows, with how many of each it shows, their names, descriptions and materials. Click the view, then where the list goes. *Rows: Components Only* lists the components with bodies inside subassemblies instead of the first level. With *Balloons* (on unless you switch it off) a balloon goes on each listed item in the view, as the Balloon tool would place it. |
| Balloon | A component's number from the bill of materials, in a circle on a leader. Click an edge of the component on a view, then where the balloon goes. |

A table follows the design: add a component, delete an occurrence or a
hole, and its rows change; the balloons keep their numbers in step. A
balloon whose component is gone, or that no bill of materials on its sheet
lists, is marked broken. Drag a table or a balloon to move it; a
balloon stays where it is relative to its component.

Select a table to change it in the panel: its **Columns** (a bill of materials
can also show any property of the components: type the property's name
under *More Columns*, such as `Vendor`), **Sort By** and
**Descending**, a hole table's **Origin** and **Tags**, a bill of materials's
**Rows** and **Group identical parts**, and its **Title**. The numbers of a
bill of materials follow its sort.

#### Sheets

The tabs over the sheet are the drawing's sheets: click a tab to show
its sheet. **+** adds a sheet. The context menu of a tab renames or
deletes it. **Sheet…** sets the sheet's template, size (A4 to A0, ANSI
A to E, or **Custom**: a width and a height of your own in millimeters,
10 to 10 000 each; such a sheet lies as you give it, so its orientation
is off), landscape or portrait, the projection method, the units of its
dimensions and tables (the design's, or millimeters, inches and others),
and the title block's title, drawing number, author and date. Without a drawing number, the title block names the bodies.
Without a date, it shows the date of the day.

The Drawing tab's **Export** menu writes the drawing as PDF (all sheets), SVG, DXF or DWG (a file
per sheet). The command line writes a drawing of the design too, all
its sheets or one (`--sheet 2`: the second):

```
mitcad-cli export-drawing design.mitcad design.pdf --drawing Drawing1
mitcad-cli export-drawing design.mitcad sheet2.svg --drawing Drawing1 --sheet 2
```

The drawing is written as it is: the options that lay out a new
drawing (`--views`, `--scale` and the others) are refused with
`--drawing`.

#### Templates and revisions

A sheet's **template** gives it its frame, its title block and its
revision table. Create Drawing and Sheet… offer:

- **ISO** (A sizes) and **ANSI** (ANSI sizes), built in;
- your own templates, kept in your library beside your materials and
  print profiles;
- the templates of the project the design is in, kept in the project;
- **Simple Title Block**: the block File › Export Drawing draws.

Choosing a template in Create Drawing or Sheet… also sets its sheet
size and its projection method (ISO first angle, ANSI third angle; a
template you save keeps its sheet's). Sheet… keeps the sheet's
projection method while sections or details are drawn on its projected
views, and says so. A sheet keeps a copy of its
template, so the drawing looks the same wherever the design goes.

The title block's fields come from the design and follow it: the title
(the component's property `Title`, which for the root is the Title of
File › Design Properties…, else the design's or the component's name),
the drawing number (else the part number), the part number, the
description (the property `Description`, else the summary's Comments),
the material (the property `Material`,
else the bodies' materials), the mass (the property `Mass`, else worked
out from the bodies and their materials), who drew it
(else the property `Author`, else the summary's Author), the date, the
scale, the sheet (`1/2`) and the latest revision. Sheet… sets the title, drawing number, author and
date of the sheet over the design's. A template's field can show any
property of the component, written in braces: `{Customer}`,
`{Identity/Customer}`.

**Save as Template…** in a sheet tab's menu saves the sheet's template
under a name to your library or the project's, with its size (a custom
size too) and its title block's images. A template is a small JSON file
in the folder `drawing-templates`, so it can be written by hand too.

**Edit Title Block** in a sheet tab's menu or the sheet's context menu
edits the sheet's title block on the sheet, for example to make your
company's block with its logo:

1. Choose Edit Title Block. The panel shows the block's tools; the
   drawing's own tools wait until you are done.
2. **Line**: click the two ends of a line. **Field**: click two
   opposite corners of its cell, then type its **Label** (written small
   in the corner) and its **Text**, where names in braces are filled in
   from the design: `{material}`, `{mass}`, `{Customer}`; **Text Height
   (mm)** sets its letters' height (3.5 when empty).
3. **Image…**: choose a PNG, JPEG or SVG file (a logo, 4 MB at most),
   then click two opposite corners of its box. The image is drawn whole
   in the box, keeping its proportions.
4. **Select**: click a line, field or image to select it, drag it to move
   it, Delete deletes it. Every click and move snaps to the **Grid**
   (1 mm at first) and stays inside the block; **Width** and **Height**
   size the block, which stays in the frame's lower right corner.
5. **Done** keeps the block in the sheet, one step to undo; **Cancel**
   leaves it as it was.

A sheet with the simple title block starts from the ISO template's
block. Then save the sheet with Save as Template…, and choose the
template in Create Drawing for your next drawings. The images are drawn
in PDF and SVG files. An SVG logo goes into PDF, DXF and DWG files as
its paths in their colors (in DXF and DWG as lines on a layer per
color), its text as the outlines of its letters in the font the logo
names. A font your computer does not have is replaced by the bundled
Droid Sans, and the export says so; text in Droid Sans, or in a generic
font such as sans-serif, looks the same on every computer. DXF and DWG
files hold no PNG or JPEG images, so they get such an image's outline,
and the export says so: use an SVG logo for DXF and DWG files.

**Add Revision…** adds a row to the drawing's revision table: its letter
(A, B, … in turn), a description, the date (today's when left empty)
and who made it. The revision table is drawn above the title block once
the drawing has a revision, the latest on top, and the title block
shows the latest letter. The sheet's panel lists the revisions: a click
on one edits or deletes it.

#### Cut sheets

A cut sheet lays flat components out at 1:1 on a sheet of material and writes
their outlines as a DXF or SVG file for a laser, water-jet or plasma
cutter: the ribs and formers of a model aircraft, brackets, panels.
It keeps no title block, frame, dimensions or hidden lines: only the
edges to cut. MitCad keeps it in the Drawings folder with the drawings,
and it follows the design: change a component, and its outline changes.

**Creating one.** Select the bodies to cut in the browser or the 3D view
(or none), then File › Create Drawing (or Create Drawing in a Drawings
folder's menu). In the dialog, set **Kind** to **Cut Sheet** and type
the material's **Width** and **Height** (600 × 300 mm unless changed).
MitCad names it CutSheet1, CutSheet2 and so on, adds the selected bodies
as components and opens it.

**Components.** While a cut sheet is open the toolbar shows its **Cut Sheet** tab: Close Drawing, **Add Components…** and Export. **Add Components…** lists the bodies: choose one or several
(Ctrl+click) and a **Count** (2 for two components of each body). Each component is
its body seen square on to its largest flat face: the outer boundary of
the whole body and the holes and cut-outs that go right through it. A
blind hole or a pocket is not cut. MitCad places new components in free room
from the top left of the sheet; it does not nest them.

- Drag a component to move it.
- Turn it from its menu (Rotate 90° Left, Rotate 90° Right) or type an
  angle in the panel's **Rotation**.
- **Face** in the panel chooses another flat face to lay it on.
- **Add Copy** adds one more of the same component; Delete removes one.

A component that overlaps another or reaches past the material's edge is
named in the panel and in the status bar. A component whose body is deleted
keeps a red box on the sheet until it is deleted or the body comes back.

**Writing the file.** The Cut Sheet tab's **Export** › **DXF…** or **SVG…** writes the
components at 1:1: an SVG file in millimeters, a DXF file in the
design's length unit or the unit you choose ([The unit of a DXF
file](#the-unit-of-a-dxf-file)). Outer boundaries are on the layer
`CUT_OUTER` and holes on `CUT_INNER`, as the sheet-metal Flat Pattern's
DXF names them; arcs and circles stay arcs and circles, and curved
edges (splines) stay splines with their own control points (in SVG,
Bézier curves). Check **Component
Names on the LABEL Layer** in the same menu to write each component's name
as text on a layer `LABEL` (for engraving, or to tell the components apart).

#### Export Drawing

File › Export Drawing writes the bodies as a drawing on a sheet. The
drawing has one of these formats:

| Format | Use                         |
|--------|-----------------------------|
| PDF    | To print or to send         |
| SVG    | For documents and web pages |
| DXF    | For other drawing programs, in the unit of the DXF unit row ([The unit of a DXF file](#the-unit-of-a-dxf-file)) |
| DWG    | For other drawing programs: version R2000, with what the DXF file holds |

**Bodies.** MitCad draws the selected bodies or every visible body,
where the design shows them. A selected face, edge or vertex stands
for its body.

**Views.** The drawing has a front view, a top view and a right view.
MitCad removes the edges behind faces. An isometric view and a section
view are available on request. Third-angle projection puts the top
view above the front view and the right view to its right. First-angle
projection puts them the other way round. *Hidden Edges* draws
the edges behind faces as dashed lines. *Tangent Edges* draws the
edges where faces meet smoothly, such as the edges of fillets, as thin
lines; without it they are left out, however the body was made. The
seam of a round face is never drawn. The lines, small
fillets and threads are drawn as in a drawing in the design (see
*Lines* and *Threads* under Drawings in the design).

A section view cuts where a Section Analysis of the design cuts.
Select the Section Analysis under *Section view*. MitCad hatches the
cut faces. The view that sees the cutting plane edge on shows the
plane with its arrows and letters (SECTION A-A).

**Dimensions.** MitCad gives the width and the height of each view.
MitCad gives the diameter of each size of round hole once, with the
count of such holes (`4x Ø6.6 THRU`). MitCad also gives the
counterbore or countersink, and a tapped hole's thread. *Sketch
dimensions* adds the dimensions of the sketches that a view sees
square on. Values are in the design's unit.

**Hole table.** Each hole seen end on gets a tag. The tags are A1, A2
and so on in the first view, and B1 and so on in the next view. The
table at the top right lists each hole's position, size, depth and
thread. The position is from the view's lower left corner. A small
cross marks that corner. The depth is THRU for a hole through the
body.

**Sheet.** The sheet is A4 to A0 or ANSI A to E, landscape or
portrait. The sheet has a frame and a title block. The title block
shows the design's name, the bodies, the scale, the units, the date,
the sheet and the projection method. *Fit the Sheet* takes the largest
standard scale at which the views fit. The standard scales are 1:1,
1:2, 1:5 and so on, or 2:1, 5:1 and so on. You can also type a scale,
for example 1:3.

*One sheet per body* draws each body on a sheet of its own. For a PDF,
the sheets are the pages of one file. For SVG and DXF, MitCad writes
numbered files.

The dialog remembers its choices for the next drawing. MitCad keeps
the drawing it lays out in the root's Drawings folder. Open it to
change it, and export it again with Export… after a change of the
model. *Drawing* says which drawing the dialog writes: the one it laid
out last (the first choice), another of the design's drawings, or *New
Drawing*. A drawing of the design is written as it is: it follows the
model already, and what you made on it (annotations, tables, moved
views, more sheets, its template) stays. The dialog's view, sheet and
scale settings are for a new drawing only and are off while a drawing
of the design is chosen; change that drawing in its own workspace. So
exporting again does not add a new drawing each time. When the design cannot change at the moment (it is read-only,
or a command's panel is open), MitCad writes the file without keeping
a drawing. When the file cannot be written, the drawing is not kept
either. The command line writes the same file without keeping a
drawing:

```
mitcad-cli export-drawing design.mitcad design.pdf --sheet A3
```

#### Printing

File › Print… (Ctrl+P) prints what is open:

- **A drawing** (in its workspace): each sheet on a page of its own size,
  at the sheet's scale, with what its PDF shows. The print dialog's page
  range chooses sheets. Choose a paper of the sheet's size: a printer
  with other paper gets each sheet fitted to its paper. Printing to a PDF
  file gives each page its sheet's size.
- **A cut sheet**: the components at 1:1, tiled over pages, or fitted to
  one page, in the **Print Cut Sheet** panel, as a sketch is printed
  ([Printing a template at 1:1](#printing-a-template-at-11)).
- **The open sketch** once something is drawn in it ([Printing a template
  at 1:1](#printing-a-template-at-11)).
- **The Cut List or Bill of Materials window** of the Build tab: its
  **Print…** button or Ctrl+P prints the tab shown.

With nothing of these open, Print is not available. The system's print
dialog chooses the printer, the paper and the copies.

### Web viewer

MitCad's web viewer shows a design in any web browser. It does not
need MitCad, a server or the internet. Each page is one HTML file. The
file holds the viewer, its libraries and the design. Open the page
with a double click (`file://`) or from any static web host. The page
works on a computer, a tablet or a phone.

#### Exporting a page

File › Export with the type *Web page* writes the design as one web
page. A file name that ends in `.html` selects the same type. The page
is a single file that holds the viewer and the design together. You
can send the file, put it on a USB stick or put it on any web server.
A double click opens it in the browser.

**What goes in.** MitCad writes every visible body, where the design
shows it, with its appearance. The appearance includes colors, metal,
roughness, see-through glass and textures. Faces with appearances of
their own keep them. The components go in as a tree. In the viewer,
you can show and hide the parts of the tree. The named views and a
Home view go in.

The design's units go in too. Thus distances that you measure in the
viewer are in the design's units. A person who walks through a
building model has their real size.

**Refinement** (Low, Medium, High) sets how finely MitCad divides
curved faces into triangles, as for STL and OBJ. Low makes the
smallest file. The smallest file loads fastest on a phone.

**Size.** When MitCad has written the page, the status bar shows the
size of the file. Textures go into the file too. MitCad scales large
images down to 2048 pixels a side. If MitCad cannot find a texture's
image, the texture keeps its color, and the log says so.

The page works offline. Everything it needs is inside it. The page
opens in current versions of the common browsers.

#### In the viewer

The toolbar at the top has **Home**, **Views** (the design's named
views), **Components**, **Person**, **Measure** and **?**. The **?**
button shows the keys and controls. The H key shows them too.

**Look around.** The mouse and the keys move the camera:

| Action                                | Mouse or key             |
|---------------------------------------|--------------------------|
| Orbit                                 | Left drag                |
| Pan                                   | Middle drag or right drag |
| Zoom at the cursor                    | Wheel                    |
| Fly forward, left, back and right     | W A S D                  |
| Fly down and up                       | Q and E                  |
| Fly four times as fast                | Shift                    |
| Return to the design's home view      | Home (the key or the button) |

Views goes to a named view. If the named view is orthographic, the
viewer shows it orthographic.

**Names and components.** Click a body. The viewer shows the body's
component and name at the bottom, and outlines the body. Components
opens the component tree. The boxes in the tree show and hide a
component with everything under it. Click a name to show its bodies.

**Walk as a person.** Click Person, then click a floor. The viewer
puts a person there, and the view is the person's eyes. Eye height is
in the toolbar. It is 1.65 m at first. Move the mouse to look around.
The page takes the mouse pointer. W A S D walk, and Shift runs.

The person stays on the floor below. The person climbs steps of up to
0.25 m (stairs). The person falls down from an edge. Walls stop the
person. **If you hold the right mouse button, the person walks through
walls.** Gravity is off while you hold the button, so the person can
pass through floors too. The person then moves where they look, and Q
and E move the person down and up. A hint on the screen says so.

Press Esc to give the mouse pointer back. A click takes the pointer
again. Press Esc again to return to the free camera where you were.
Lengths obey the design's units. Thus you walk through a building
drawn in millimeters at its real size. Hidden components are not
shown, and the person does not walk into them.

**Measure distances.** Click Measure, then click two points. The
viewer shows the distance between them in the design's units, with
its X, Y and Z parts. A point snaps to a corner or an edge near the
cursor. The color of the point shows the snap:

| Color | Snap               |
|--------|--------------------|
| Orange | A corner           |
| Blue   | An edge            |
| Gray   | A point on a face  |

Press Esc to clear the measurement. Press Esc a second time to stop
the measurement.

**Touch.** On a touch screen, the viewer has these controls:

| Gesture                 | Effect                      |
|-------------------------|-----------------------------|
| One finger              | Orbit                       |
| Two fingers             | Pan, and pinch to zoom      |
| Tap                     | Show the name of the item   |

As a person, the stick at the bottom left walks. A drag elsewhere
looks around. Hold Through walls to pass through walls and floors.

#### A project's viewer

People without MitCad can look at a project's designs in a web
browser. **File › Project › Web Viewer** enables the project's web
viewer. MitCad then writes a folder `web` into the project. The folder
holds:

- `web/index.html`: the project's name and a card for each design.
  The card shows the design's thumbnail, name and folder, and who
  saved it last and when. Open the file from the disk in any browser.
  It needs no server and no internet. Or let a static web host serve
  the project's repository.
- A page for each design, for example `web/parts--bracket.html` for
  `parts/bracket.mitcad`. The page shows the design's newest version
  in the viewer. The page is one file that also opens on its own.
- The design's thumbnail. This is the preview of the version that
  Version History shows. A design not yet saved, opened or restored in
  MitCad has no thumbnail until its next Save.
- `web/designs.json`: the same list for other programs.

The folder is part of the project's versions. Web Viewer records the
folder as a version, with the setting. Thus others get the folder with
the project, and a Cloud project sends it to the repository. From then
on, each Save that records a version also writes that design's page
and the index again, in the same version.

**Project Settings › Web viewer › Update the web viewer on each Save**
disables that, or enables it again, for everyone in the project. When
it is off, **File › Project › Update Web Viewer** writes every page and
the index when you want. Update Web Viewer also removes the pages of
designs that were deleted or renamed. Both commands work in projects
that keep versions.

When MitCad writes every page, it computes each design. A progress
dialog shows this. Its Cancel stops between designs. Then MitCad
records nothing.

If two people save different designs at the same time, both change
the index. Then Sync asks which index to keep. Either is fine. Update
Web Viewer writes the index again from all designs.

## Preferences and diagnostics

### Preferences

**Tools › Preferences** holds the settings of MitCad. It has one page
for each kind of setting: General, Navigation, Display, Cache, Cloud,
Updates, 3D Print, Diagnostics, Textures, AI Assistants, Fonts and Sketch.
Command search finds each page by its name or by what it sets, for example "autosave",
"slicer" or "git". The result shows as *Preferences: Cache* and so on.
The window opens no larger than the screen, and you can resize it.

On macOS, the same pages are in the **Settings** window
(MitCad › Settings, Cmd+,). Changes apply at once and are saved
automatically. There is no OK. After each change the note *Changes are
saved automatically* shows under the page for a few seconds. The
background can follow the system's appearance.

The table that follows lists the pages. Each page has a chapter in this
guide that explains its settings.

| Page | What it sets | Refer to |
|---|---|---|
| **General** | The **Language** of the interface. The *Import tolerance* of imported files. **Autosave**: on or off, 1 to 60 minutes, the folder with Open Folder. **Default units for new designs**: mm, cm, m, in or ft (mm by default). **Ask to activate the component of a picked face**. **Show the version in the view** (on by default). **Show used sketches under their features** (off by default; see [Browser](#browser)). | [Import](#import), [Saving and recovery](#saving-and-recovery), [Units](#units), [Components](#components), [The window](#the-window), [Layouts](#layouts) |
| **Navigation** | The mouse scheme for orbit, pan and zoom | [View and navigation](#view-and-navigation) |
| **Display** | **Camera**, **Orientation cube**, **Interface size**, **Text size**, **Icon size**, **Font**, **Anaglyph 3D** (**Stereo depth**, **Convergence**), **Layout**, **Show the menu bar** (Windows and Linux), **Browser background**, **Ghost inactive components**, **Pick the active component first**, **Small circles** | [The window](#the-window), [View and navigation](#view-and-navigation), [Layouts](#layouts), [Components](#components) |
| **Cache** | The cache of computed results: on or off, the size, the folder, **Clear**, the memory limit | [Cache of computed results](#cache-of-computed-results) |
| **Cloud** | The git program, the default author, the defaults of projects, live updates, the broker, **Known brokers** | [Sharing a project](#sharing-a-project), [Live updates](#live-updates) |
| **Updates** | The automatic check for updates: on or off. Pre-releases: offered or not | [Updates](#updates) |
| **3D Print** | The slicer | [3D printing](#3d-printing) |
| **Diagnostics** | **Write a debug log** | [Debug log](#debug-log) |
| **Textures** | The *Local Folders* of the texture sources. *Allow texture sources on the internet* | [Textures](#textures) |
| **AI Assistants** | Connect AI Assistant…, and the folders that an AI assistant connected through MCP can save to and open from | [mcp.md](mcp.md) |
| **Fonts** | The folders of font files that the font chooser of Edit Text lists | [Text, fonts and QR codes](#text-fonts-and-qr-codes) |
| **Sketch** | **Scale the sketch to its first dimension**: on or off. **Click adds to the selection in sketches**: on or off (off by default). **Esc leaves an unchanged sketch**: on or off (off by default; see Finish Sketch in [Shortcuts](#shortcuts)). **Show profiles when a sketch opens**: on or off (on by default; see [Sketch mode](#sketch-mode)) | [Dimensions](#dimensions), [Selection in sketch mode](#selection-in-sketch-mode) |


**Layout.** The **Layout** setting of the Display page selects how the
window is arranged. **Docked** puts the panels in docks around the 3D
view. **Floating** puts the panels as cards over a full-window 3D view.
Floating is the default. On Windows and Linux, only the menu bar stays
above the view. The title row floats over the view, and the toolbar's
groups float as cards. Refer to [Layouts](#layouts).

The layout takes effect when MitCad starts. After a change, MitCad asks
if it must restart now. **Restart Now** closes the window as a quit
does. Then it starts MitCad again with the design open. Before the
window closes, MitCad saves a design with unsaved changes, or asks
about it. Cancel keeps MitCad open. **Later** keeps the selection for
the next start. `--chrome=docked|floating` on the command line replaces
the setting for one run.

**Components on the Display page.** The Display page also has **Ghost
inactive components** and **Pick the active component first**. Each of
them can be set to off. Refer to [Components](#components).

**Navigation bar on the Display page.** **Show the navigation bar** is on
by default. Clear it to hide the bar at the bottom of the view. Refer to
[View and navigation](#view-and-navigation).

**Cloud.** The Cloud page of Tools › Preferences sets:

- The git program that sends and gets the versions of Cloud projects.
  MitCad finds it on `PATH`, on Windows also where Git for Windows
  installs itself. You can also select it. Git does its own sign-in.
  Local projects do not need git. The line under the field shows the
  version found, or why git was not found. A long message wraps, and
  its tooltip shows all of it.
- The default author, used when git has none
- The defaults of projects: send each version at once, and how often to
  check for newer versions
- If live updates are permitted. When they are off, no project connects
  to a broker.
- The broker that New Project offers
- **Known brokers**: the brokers you answered the trust question for,
  with the user you signed in as. **Sign Out** removes a broker's user
  name and password from the keychain. **Forget** also removes the
  answer, so that the next connection asks again.

Refer to [Sharing a project](#sharing-a-project) and
[Live updates](#live-updates).

**Textures.** The *Local Folders* of the texture sources are the
**Texture folders** that you add on the Textures page with **Add…**
(**Remove** takes the selected ones off the list). MitCad also searches
the subfolders for PNG and JPEG images named by their maps, such as
`oak_diff.png` and `oak_nor_gl.png`, and embeds a texture that a design
uses in the design. MitCad uses the
internet only when you permit it. The first time you select an online
source, MitCad asks. If you answer No, MitCad asks for nothing from the
internet. *Allow texture sources on the internet* on the Textures page
changes the answer later. Refer to [Textures](#textures).

**Fonts.** The font chooser of Edit Text lists the bundled Droid Sans
and the installed fonts. Then it lists the font files of the **Font
folders** of **Preferences › Fonts** and their subfolders (**Add…**
chooses a folder, **Remove** takes the selected ones off the list). Each
font has a preview. **File…** beside the chooser selects any font file
(`.ttf`, `.otf`, `.ttc`, `.woff`, `.woff2`). A text keeps its font
file's outlines in the design, so the design looks the same on a
computer without the file. Refer to [Text, fonts and QR codes](#text-fonts-and-qr-codes).

**3D Print.** The 3D Print page sets the slicer. MitCad finds it
automatically where possible. You can also select another program, for
example an AppImage or, on macOS, an application in the Applications
folder. Refer to [3D printing](#3d-printing).

**AI Assistants.** The AI Assistants page has **Connect AI Assistant…**
and lists the **Folders for designs**: a connected AI assistant may save
the open design to these folders and open designs from them. There are
none until you add one: **Add…** chooses a folder, **Remove** takes the
selected ones off the list (the files stay). Connect in Connect AI
Assistant adds its folder too. Refer to [mcp.md](mcp.md).

### Language

The **Language** setting of the General page selects the language of
MitCad's menus, toolbar, panels and dialogs: **Automatic**, **English**,
or a language MitCad has, listed by its own name (**Suomi** for Finnish).
Automatic uses the system's language when it is the first language of the
system and MitCad has it, else English. The language takes effect when
MitCad starts again. A text that the language does not have yet shows in
English.

Command search also finds a command by its English name. For example,
"preferences" finds Preferences in every language. Names that you give,
parameter names, formulas and file formats stay as they are. The messages
of the model, for example the errors of a feature, are in English.

`--language en` or `--language fi` on the command line replaces the
setting for one run.

### Cache of computed results

MitCad keeps the results of costly features on disk in your user cache
folder. Thus, when you open a design again, MitCad computes only what
changed. The same applies to a copy of the design and to an older
version of it. A new version of MitCad does not reuse the results of an
older one.

The Cache page of Tools › Preferences sets:

- The cache: on or off
- The size: 5 GB by default. When the cache is full, the least recently
  used results go first.
- The folder, with Open Folder
- **Clear**
- "In memory, at most": the memory that computed results can take. By
  default, it is a quarter of the machine's memory.

**Help › Diagnostics** shows what the disk cache and the memory cache
hold. The Cache page also opens it. It shows:

- The size against the limit
- Hits and misses
- Disk reads and writes
- Space by feature, by feature type and by design
- Where each result of the last recompute came from

It has **Clear Memory**, **Clear Disk** and **Export Report**. The
report is JSON, or text when you give it a `.txt` name.

### Updates

Once a day at start-up, MitCad asks its release page if a newer version
is out. **Help › Check for Updates** asks at once. MitCad sends only its
version, the platform and the channel.

When a newer version is out, a bar shows under the toolbar. It has
**Release Notes**, **Install**, **Skip This Version** and **Later**.
Install downloads the update and verifies its signature. Then it closes
MitCad, installs the update and starts MitCad again. Before MitCad
closes, it asks to save changes.

Install works for the Windows installation and for a Linux AppImage in
a folder of your own. Windows can ask to permit the installer. The
portable `.zip`, other Linux installations and builds of your own get
the release page instead. If a download's signature does not match,
MitCad deletes the download. MitCad never runs it. On Windows, the
download goes to a private folder, and the updater checks the installer
again right before it runs it.

On Windows the update replaces no file while a program from the
installation runs, for example another MitCad window or the MCP bridge
of an AI assistant session (`mitcad-cli.exe`). The updater then names
these programs and asks: **End Them and Install** or **Cancel**. End
Them and Install first asks MitCad to close as when you quit it, so it
asks to save your changes. If MitCad still runs after half a minute,
the updater asks again: **End Anyway** ends it and loses the changes it
has not saved, **Wait** waits again, **Cancel** stops. Then the updater
ends the other programs and installs. Cancel installs nothing. MitCad
then starts again and says the update was not installed; close the
programs it names and choose **Try Again**. The installer with its
pages asks the same with **End Them**, **Retry** and **Cancel**.

The update information of a release has a last day. When it has
expired, the check at start-up offers nothing. **Help › Check for
Updates** still shows the release, without **Install**, and says on
which day its update information expired. If that day has not passed
yet, the date of your computer is wrong: correct it, or get the release
from its release page.

A release can come out for one platform a few days before another.
Until it has a download for yours, the check at start-up says nothing
of it. **Help › Check for Updates** says that the release is out but
has no download for your platform yet, with **Release Notes** and
**Later** only. The notice comes at start-up once the download is out.

The Updates page of Tools › Preferences sets the automatic check on or
off, and if pre-releases are offered. With the setting off, MitCad
makes no request at start-up. *Check for Updates* still asks when you
select it.

An administrator sets update checks to off for every user, for offline
or managed installations. Then MitCad makes no request at all, and
*Check for Updates* makes none. Preferences shows the switch off and
why. The switches are:

- The environment variable `MITCAD_NO_UPDATE_CHECK=1` on every platform,
  for example in `/etc/environment` or in the system's environment
  variables. Any value but empty or `0` sets the checks to off.
- On Windows, the registry value `DisableUpdateCheck` (DWORD, not 0)
  under `HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Mitcad\Mitcad`:

  ```bat
  reg add HKLM\SOFTWARE\Policies\Mitcad\Mitcad /v DisableUpdateCheck /t REG_DWORD /d 1 /f
  ```

**What is sent.** A check is one HTTPS request for the manifest. On the
pre-release channel, it is two requests, GitHub's list of releases
first. An installation adds the download. The requests carry the
User-Agent `MitCad/<version> (<platform>; <channel>)`, for example
`MitCad/0.2.0 (windows-x64; stable)`. They carry nothing that
identifies the user or the machine:

- No cookies. MitCad keeps none and sends none.
- No cache
- `Accept-Language: *` instead of the user's languages
- No telemetry

MitCad uses HTTPS only. It obeys a redirect only from HTTPS to HTTPS,
at most five. GitHub's downloads redirect to its CDN. More:
[updates.md](updates.md).

### Feedback and error reports

**Help › Send Feedback** reports a problem or a wish. You do not leave
MitCad. The report has:

- The kind: bug report, wish or other
- A summary
- A description
- A contact address, if you want one. The issue tracker is public.
- The diagnostics: MitCad's version, the system, Qt and Open CASCADE,
  the graphics renderer and driver, and the window's layout. They are on
  by default.
- A screenshot of the window, if you tick it. To crop it, drag over the
  part to keep. **Whole Window** undoes the crop.

**Preview** shows everything that the report contains, section by
section. There you can edit any part, untick what must not be sent,
change the title, or Cancel. MitCad masks folders and file paths, your
user name and the computer's name before you see the preview
(`<path>/<file>.f3d`, `<user>`, `<host>`). The names of program and
source files stay, so that a stack stays readable. Your design and its
files are never attached.

**Send** opens MitCad's issue form in your web browser, with the title
and the report filled in. Nothing is sent until you submit the form
there. MitCad holds no account or token. If the report is too long for
the form's link, the report is on the clipboard. Paste it over the
shortened text. MitCad saves a screenshot in the `reports` folder of
its data folder, and the message says where. Drag it into the form.

**Error reports.** When MitCad crashes, it writes a crash report. The
report holds the version, the platform, the signal or exception, the
stack of the crashed thread and the last commands you used. It never
holds design content. MitCad offers the report the next time it starts.
A crash of the import process or of the render process is offered at
once. An internal error is also offered at once.

An internal error is one of these:

- The geometry kernel crashes inside an operation. The operation fails,
  and the design stays as it was.
- A command gives an unexpected error.

**Review Report** shows the same preview. Add what you did, edit or
leave out parts, then Send or Cancel. The report carries a duplicate
key, for example `mitcad-1a2b3c4d5e6f`, also in the title. The same
error gives the same key. Thus a search for the key finds the earlier
reports of the error.

**Do not offer error reports again** in the offer sets the offers to
off (setting `reports/offerErrors`). Help › Send Feedback stays. MitCad
keeps the newest 20 crash reports in the `crashes` folder of its data
folder.

Administrators and builds of their own point the reports at another
issue tracker with the setting `reports/issueUrl`. Its value is an
issue list's address, `https://host/owner/repository/issues`, or a
template with `{title}` and `{body}`.

### Debug log

When something odd occurs in normal use, a debug log shows afterwards
what MitCad did. Examples are a cursor that stays busy, or a command
that does nothing. **Preferences › Diagnostics › Write a debug log**
sets the log on at once, without a restart. It stays on, also at the
next start, until you set it off. Set it off there, or in the project
indicator's menu (**Turn Off Debug Log**). While the log runs, the
project indicator says **Debug log on**.

The project indicator is in the status bar. In the Mac layout, it is
beside the design's name. For one run only, start MitCad with
`--debug-log` or `--debug-log trace.log`, or with the variable
`MITCAD_DEBUG_LOG=1` (or a file's path).

**The log files.** MitCad writes the log to its log folder. **Help ›
Open Log Folder** opens it. The folder is `logs` in MitCad's local data
folder. MitCad writes one file per start, named
`mitcad-YYYYMMDD-HHMMSS-<process>.log`. At 20 MB, a file continues in a
new one. The folder keeps the newest 10 files, at most 200 MB.

Each file begins with:

- MitCad's version and build
- The system
- The graphics card and its OpenGL renderer
- The command line
- The open design's path
- The switches in use

When the log starts with MitCad, MitCad writes the file when the 3D
view knows its graphics card. This takes at most a few seconds. If
something goes wrong first, MitCad writes the file at once. When you
set the log on, the lines that MitCad kept in memory from before come
first (the newest 5000).

Each line has the time in milliseconds, the level (debug, info,
warning, critical), the thread when it is not the main one, and the
text. MitCad writes the log at least once a second and at every
warning. Thus a crash loses at most a second.

**What a log contains.** Commands, sketch operations, computations,
refusals and warnings. With them come file paths, design names and the
names of features, bodies and sketch entities. What you type into
fields and where clicks pick are left out. The variables
`MITCAD_LOG_TYPED_TEXT` and `MITCAD_LOG_PICKS` set them on separately,
for tests.

Logs stay on your computer. A report (Help › Send Feedback, an error
report) offers the latest log and the newest lines kept in memory. Both
are left out unless you tick them. A ticked log file is not sent, but
its place is shown. Drag it into the issue form.

## Command line

The application is `mitcad`, on Windows `mitcad.exe`. Its options:

- `--open file`: opens a design or a file to import at start-up. An
  invalid design gives exit code 2.
- `--set d3=35`: sets a parameter's value, as Edit Parameters does.
  The value is a number, in mm or radians. The option can be repeated.
- `--demo`: starts with a 60 × 40 × 20 mm block
- `--screenshot image.png`: saves the 3D view after start-up and quits
- `--no-recovery`: does not offer autosaved work at start-up. File ›
  Recover Designs still does.
- `file`: the same as `--open file`, as a file association passes it.
  One file, not together with `--open`.
- `--chrome=docked|floating`: puts the panels in docks, or as cards over
  the 3D view, for this run. The default is the Layout selected in
  Preferences › Display, else floating.
- `--debug-log [file.log]`, `--debug-log=file`: writes a debug log for
  this run ([Debug log](#debug-log)). It goes to the file given, or to a
  new file in the log folder.
- `--language en|fi`: the language of the interface for this run. The
  default is the Language selected in Preferences › General
  ([Language](#language)).

Environment variables:

| Variable | Effect |
|---|---|
| `MITCAD_NO_UPDATE_CHECK=1` | Sets update checks to off |
| `MITCAD_DEBUG_LOG` | `1`, or a file's path: writes a debug log as `--debug-log` does |
| `MITCAD_LANGUAGE` | `en`, `fi`: the language of the interface, over Preferences; `--language` comes first |
| `MITCAD_PROJECTS_DIR` | Where New Project puts projects, instead of Documents/Mitcad |
| `MITCAD_GIT` | The git program when it is not on `PATH`. Preferences › Cloud sets it for the application |
| `MITCAD_LIBRARIES_DIR` | Where fetched component libraries are kept |
| `MITCAD_TEXTURES_DIR` | Where the texture sources' thumbnails and fetched maps are kept |

Variables for tests: [development.md](development.md).

**mitcad-cli.** `mitcad-cli` works without a UI. `mitcad-cli --help`
lists all commands and options, `mitcad-cli --version` prints its
version. Each command below runs as `mitcad-cli <command>`. In the Linux
AppImage, `MitCad.AppImage cli <command>` runs it.

- `info part.mitcad [--json] [--properties]`: computes the design. Then
  it prints the timeline with its states, and the bodies with their
  volumes, areas, centers of mass and bounding boxes.
- `run script.json [--open part.mitcad] [--save result.mitcad]`: runs
  JSON commands and expectations, for example `{"expect": {"body":
  "Body1", "volume": 48000}}`. A failed expectation gives exit code 1.
  Examples: [tools/cli/tests](../tools/cli/tests). Commands:
  [commands.md](../core/model-api/src/commands.md).
- `import part.step [--unit-mm N] [--structure components|base] [--save result.mitcad]`:
  imports STEP, IGES, BRep, STL or OBJ as a base feature, a STEP
  assembly as components unless `--structure base` ([STEP
  assemblies](#step-assemblies)).
- `import-f3d part.f3d --save part.mitcad [--report report.json]`:
  imports an `.f3d` design with its history. Its options:
  - `--design` selects a document of an `.f3z`
  - `--time-limit S` imports the rest of the items as the file's bodies
    after S seconds
  - `--hang-limit S` interrupts a geometry kernel that hangs
  - `--no-verify` skips the comparison with the file
  - `--no-fallback` leaves out the items that do not replay
  - `--bodies-only` imports the bodies only, the mesh bodies with their
    materials and appearances among them
  - `--json` prints the reports as JSON
- `import-fcstd part.FCStd --save part.mitcad [--report report.json]
  [--bodies-only]`: imports a FreeCAD document. `--bodies-only` imports
  FreeCAD's results without history and sketches.
- `import-ipt part.ipt --save part.mitcad [--report report.json]
  [--reference part.stp [--max-relative X] [--deviation]]`: imports the
  bodies of an `.ipt` part. `--reference` compares its solids with a
  STEP file of the same part, by volume and area. The default limit is
  1e-6 relative. When they differ, the exit code is 1.
- `import-iam assembly.iam --save assembly.mitcad [--report report.json]
  [--history] [--search <folder>]…`: imports an `.iam` assembly as
  its parts' stored bodies. With `--history`, it replays each part's
  design. The report tells for each part if it was replayed or imported
  as its stored bodies, and why.
- `import-ifc building.ifc --save building.mitcad [--report report.json]
  [--no-openings] [--no-occurrences]`: imports an IFC building model.
  The report has a line for every product of the file.
- `import-rvt project.rvt --save project.mitcad [--report report.json]
  [--report-only] [--elements <id>,…]`: imports the building elements
  of an `.rvt` project, one component per level. For an `.rfa` family,
  it imports its metadata. `--report-only` reads and reports without an
  import. `--elements` imports only the elements listed by id.
- `import-solidcraft design.sc --save design.mitcad [--report
  report.json] [--set name=expression]… [--expected values.json]`:
  imports a SolidCraft design (`.sc` or `.solidcraft`) with its
  history. `--set` changes a parameter, by the design's name, before the
  report measures the bodies. `--expected` checks the bodies, timeline, parameters and
  sketches against given values. When they differ, the exit code is 1.
- `export part.mitcad result.step [--bodies …] [--schema ap214|ap242]
  [--unit mm|in|…] [--refinement low|medium|high] [--ascii]`: exports a
  design. A `.3mf` file holds the solid and mesh bodies as the parts of
  one object. A `.usdz` file holds the shown bodies with their
  appearances, with the textures relative to the design's folder.
- `export-sketch part.mitcad Sketch1 sketch.dxf [--unit in] [--r12]
  [--construction]`: exports a sketch, in the design's length unit unless
  `--unit` gives another. Construction curves and center lines are left
  out unless `--construction` writes them, on the layer `construction`.
- `render part.mitcad -o image.png [--view Front] [--size 1920x1080]
  [--samples 128] [--time-limit 60] [--format png|png16|jpeg|exr]
  [--transparent] [--no-denoise]`: renders the visible bodies as Render
  Image does, with the design's render settings. `--view` is a named
  view, or front, back, left, right, top, bottom or iso. The default is
  the design's Home view, else iso.
- `project init folder [--author "Name <email>"] [--no-history]`: makes
  a project folder with version history. The author is `--author`, else
  git's `user.name` and `user.email`. `--no-history` makes the folder
  only.
- `convert part.mitcad out.mitcad [--format v2|v3|auto]`: writes a
  design as a single-file design (`v2`) or as a design in a project
  (`v3`). In a `v3` design, the imported bodies are in the project's
  folder.
- `library fetch|list|show|search|diff|init|add|check|index-entry`:
  component libraries. `mitcad-cli --help` lists them. `parts
  part.mitcad` prints the bill of materials with the library components'
  designations, versions and licenses.
- Version history:
  - `version save part.mitcad [-m "message"]` records the saved file as
    a new version. If nothing changed, it records none.
  - `history part.mitcad` lists the versions of the design
  - `version show part.mitcad <version> [--save old.mitcad]` opens an
    older version. `<version>` is an id or its first digits, or
    `HEAD~2`.
  - `version changes part.mitcad <version> [<version>]` lists the files
    that differ
  - `version restore part.mitcad <version>` makes an older version the
    newest
- `diff part.mitcad <version> [<version>]` compares two versions of a
  design. Without the second version, it compares with the file as
  saved. `diff a.mitcad b.mitcad` compares two files. The result is in
  the model's terms: parameters (`d3 (Extrude1 distance): 10 mm -> 15
  mm`), features added, deleted, moved and changed, sketches,
  components, placements and bodies. `--geometry` also compares the
  bodies' volumes and areas. `--json` answers in JSON.
- Remote repositories. Refer to [Sharing a project](#sharing-a-project)
  for the requirements and for how to sign in.
  - `remote add project-folder <url>` connects the project and sends
    its versions. A repository that holds another project's history is
    refused.
  - `push project-folder` sends new versions. It never overwrites the
    server's versions.
  - `fetch project-folder` gets newer versions without a change to the
    files
  - `remote show project-folder` tells how many versions are to send
    and how many are newer
  - `clone <url> new-folder` opens a project from a server
  - `remote remove project-folder` disconnects the project
  - `sync project-folder` gets the server's newer versions, puts yours
    after them and sends the result. When you and someone else changed
    the same file, sync stops without a change and lists the file. Then
    select per file with `--resolve part.mitcad=mine`, `=theirs` or
    `=copy`, and run sync again. `=copy` takes theirs and keeps yours as
    `part (conflict copy <you> <date>).mitcad`. `--dry-run` tells what a
    sync would do. Record your changes as versions first: unrecorded
    changes in a file that the sync would change stop it. MitCad keeps
    your history from before the sync in a backup for 30 days, the
    latest five always.
- Local and Cloud projects:
  - `project inspect folder` tells what a folder is, with its designs
    and settings: a Local or Cloud project, a repository, designs, or
    empty.
  - `project create folder --author "Name <email>" [--url <url>]` makes
    a project with its first version. With `--url`, it makes a Cloud
    project. An empty server repository gets its versions. A repository
    with files, such as a README, is taken in beside the project.
  - `project settings folder [--set '{"shared": {"edit_locks":
    {"enabled": false}}}']` shows or changes the project's settings. A
    change for everyone is recorded as a version.
  - `remote check <url>` tells what a server repository holds
  - `remote share project-folder <url>` connects a Local project to a
    repository that holds files but no project. Your versions go after
    its own.
  - `clone <url> new-folder --adopt` makes a repository with files a
    project
  - `host-keys <server> [--trust]` shows the server's SSH key
    fingerprints. MitCad checks them against the fingerprints that
    GitHub, GitLab and Codeberg publish. With `--trust`, it adds them to
    `~/.ssh/known_hosts`.
- Edit locks ([Edit locks](#edit-locks)). When you set edit locks to
  off, MitCad releases your locks. The locks of others stay.
  - `lock status` lists them
  - `lock release --force` removes them

## Terms

Each of these words has one meaning in MitCad, its menus and this guide:

| Term | Meaning |
|---|---|
| **Design** | One `.mitcad` file: a single component or an assembly |
| **Project** | A folder that is one git repository and holds designs and folders. **Local** or **Cloud** ([Versions](#versions), [Sharing a project](#sharing-a-project)) |
| **Folder** | A folder inside a project |
| **Version** | A recorded state of a design (one commit of the project's repository) |
| **Owner** | A user or an organization on a git service: the first part of a Cloud project's address |
| **Projects panel** | The panel of projects, folders and designs. The **Browser** stays the design's feature tree |
| **Projects folder** | Where new projects go by default (Documents/Mitcad) |
