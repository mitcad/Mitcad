// SPDX-License-Identifier: MIT
#pragma once

// Insert › Component from Web's browser in the diagnostics of Help › Send
// Feedback (mitcad#1906, app/COMMANDS.md "Component from Web", WebGL): what
// the browser's module (mitcad-webbrowser) keeps of the pages' consoles and
// of the GPU while the window was used, and how WebGL is drawn. Plain Qt
// Core, compiled into the module, the application and test_app
// (app/tests/browser_diagnostics.cpp).
//
// The module hands its state to the application as a property of the
// application object (kStateProperty, the QVariantMap of toVariant): the
// module links nothing of the application, and the module's interface
// (ComponentBrowserHost.hpp) stays as it is, so that a component of the same
// interface brings its fixes to the MitCads already installed.
//
// WebGL in software only: where the GPU path failed in a session (CEF's log
// says the GPU process ended, after WebGL had been drawn on the GPU, or
// while WebGL was off; or a page found WebGL off with the GPU in use,
// softwareNextWanted), the module writes a marker into the browsing
// profile (kSoftwareMarker) and says so in the window; the next start of
// CEF of that version then draws WebGL in software (SwiftShader).
// MITCAD_WEB_GPU=software or gpu chooses for a run, over the marker.

#include <QString>
#include <QStringList>
#include <QVariant>

class QObject;

namespace mitcad::browserdiag {

// The application object's property the module writes and the reports read.
inline constexpr char kStateProperty[] = "mitcadComponentBrowserState";
// The marker of a failed GPU path in the browsing profile's folder.
inline constexpr char kSoftwareMarker[] = "gpu-software.json";
// The source of the scripts MitCad runs in a page (CefView::runJavaScript):
// their console lines are not the page's own.
inline constexpr char kScriptSource[] = "mitcad://script";
// The page errors and the GPU's log lines the diagnostics keep (the
// newest), and the length a kept line is cut to.
inline constexpr int kPageErrors = 5;
inline constexpr int kGpuLines = 3;
inline constexpr int kLineLength = 200;

// The script the module runs in each page it loaded (http, https, file):
// makes a WebGL context of its own and logs its renderer through the
// page's console (parseWebglProbe), then lets it go; once per page it also
// logs every WebGL context of the page that is lost ("mitcad:webgl-lost").
extern const char* const kWebglProbeScript;

struct State {
  QString cef;               // CEF's version; empty while the browser has not started
  bool softwareOnly = false; // WebGL in software only for this run
  QString softwareReason;    // why
  QString webgl;             // "gpu", "software" or "off"; empty before a page was checked
  QString webglDetail;       // "WebGL 2, <renderer>", or why it is off
  int gpuFailures = 0;       // the GPU process's failures in CEF's log this run
  QStringList gpuLines;      // the GPU's lines of CEF's log, newest last
  bool softwareNext = false; // the GPU path failed: software from the next start
  int contextsLost = 0;      // WebGL contexts of the pages lost
  QStringList pageErrors;    // the pages' errors, newest last
};

QVariant toVariant(const State& state);
State fromVariant(const QVariant& value);

// Whether a message of a page's console goes into the diagnostics: the
// page's own (not of MitCad's scripts, kScriptSource or "mitcad:"), an
// error, or any about WebGL or a lost context.
bool keepPageMessage(bool error, const QString& message, const QString& source);
// "<host>: <message> (<file>:<line>)" on one line, cut to kLineLength.
QString pageErrorLine(const QString& pageUrl, const QString& message, const QString& source, int line);
// Keeps the newest kPageErrors; the same line again is not added twice in a
// row.
void addPageError(State& state, const QString& line);

// A line of CEF's log: of the GPU (its process, GL, EGL, ANGLE, Vulkan,
// SwiftShader, WebGL), and among those a failure of the GPU process (it
// ended, exited, failed to start or is not usable, or a context's GPU state
// was lost), or neither.
enum class GpuLogLine { Other, Gpu, Failure };
GpuLogLine classifyGpuLogLine(const QString& line);
// The line's message without Chromium's prefix ("[pid:tid:time:LEVEL:file]"),
// on one line, cut to kLineLength.
QString gpuLogText(const QString& line);
// Keeps the newest kGpuLines.
void addGpuLine(State& state, const QString& text);

// The probe's answer, "mitcad:webgl on <1|2> <renderer>" or "mitcad:webgl
// off <why>": `webgl` "gpu", "software" (SwiftShader or a system's software
// rasterizer) or "off", `detail` "WebGL 2, <renderer>" or why; false for
// any other message.
bool parseWebglProbe(const QString& message, QString* webgl, QString* detail);

// The diagnostics' lines of the browser (empty while it has not started).
QStringList diagnosticsLines(const State& state);
// Those of the state an application object carries (kStateProperty);
// empty when the window was not used.
QStringList componentBrowserDiagnostics(const QObject* application);

// How CEF draws WebGL this run: `variable` is MITCAD_WEB_GPU ("software",
// "gpu" or empty), `markerPath` the profile's marker, `cef` CEF's version (a
// marker of another version is not followed).
struct GpuChoice {
  bool software = false;
  QString reason; // why software only, for the log and the diagnostics
};
GpuChoice chooseGpu(const QString& variable, const QString& markerPath, const QString& cef);
// Writes the marker: CEF's version, what failed and when.
bool writeSoftwareMarker(const QString& path, const QString& cef, const QString& reason);

// Whether the GPU path failed this run, so that the next start draws WebGL
// in software (the marker): the GPU process failed (`gpuFailed`, CEF's
// log) after WebGL had been drawn on the GPU (`gpuWebglSeen`) or while it
// is off, or WebGL is off with the GPU in use (Chromium refused a context
// on the GPU, or its driver, without falling back to SwiftShader). Never
// where WebGL is in software only already, or the marker was written.
bool softwareNextWanted(const State& state, bool gpuWebglSeen, bool gpuFailed);

} // namespace mitcad::browserdiag
