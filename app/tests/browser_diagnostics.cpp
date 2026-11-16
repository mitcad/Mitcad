// SPDX-License-Identifier: MIT
// Insert › Component from Web's browser in Help › Send Feedback's
// diagnostics (mitcad#1906): the pages' own errors kept and cut, the GPU's
// lines of CEF's log and the failures of its process, the WebGL probe's
// answer, the diagnostics' lines of the state as the application object
// carries it, and WebGL in software only after the GPU path failed (the
// profile's marker, MITCAD_WEB_GPU).
#include "UnitTests.hpp"
#include "webbrowser/BrowserDiagnostics.hpp"

#include <cstdio>

#include <QChar>
#include <QDir>
#include <QFile>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_diagnostics.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

namespace diag = mitcad::browserdiag;

const QString kNvidia = QStringLiteral("ANGLE (NVIDIA Corporation, NVIDIA GeForce RTX 4070 SUPER/PCIe/SSE2, "
                                       "OpenGL 4.5.0 NVIDIA 620.70)");
const QString kExited = QStringLiteral("[4242:4242:1115/101010.123456:ERROR:content/browser/gpu/"
                                       "gpu_process_host.cc:1000] GPU process exited unexpectedly: exit_code=139");

void pageMessageTests() {
  const QString page = QStringLiteral("https://www.example.com/models/42?session=1");
  const QString chunk = QStringLiteral("https://www.example.com/_next/static/chunks/main-abc.js");
  // The page's own errors, and its messages about WebGL whatever their level.
  CHECK(diag::keepPageMessage(true, QStringLiteral("Uncaught TypeError: a is null"), chunk));
  CHECK(diag::keepPageMessage(false, QStringLiteral("WebGL: CONTEXT_LOST_WEBGL: loseContext: context lost"), chunk));
  CHECK(diag::keepPageMessage(false, QStringLiteral("THREE.WebGLRenderer: Context Lost."), chunk));
  CHECK(!diag::keepPageMessage(false, QStringLiteral("Download the React DevTools"), chunk));
  // Never MitCad's own scripts' lines, errors included.
  CHECK(!diag::keepPageMessage(true, QStringLiteral("mitcad:links:3"), chunk));
  CHECK(!diag::keepPageMessage(true, QStringLiteral("Uncaught ReferenceError: x"),
                               QString::fromLatin1(diag::kScriptSource)));

  // One line: the host, the message, where it came from.
  CHECK(diag::pageErrorLine(page, QStringLiteral("Uncaught TypeError: a is null\nat f"), chunk, 12) ==
        QStringLiteral("www.example.com: Uncaught TypeError: a is null | at f (main-abc.js:12)"));
  CHECK(diag::pageErrorLine(page, QStringLiteral("Error"), QString(), 0) == QStringLiteral("www.example.com: Error"));
  CHECK(diag::pageErrorLine(QStringLiteral("data:text/html,x"), QStringLiteral("Error"), QString(), 0) ==
        QStringLiteral("data: Error"));
  // A long message is cut, its place kept.
  const QString cut = diag::pageErrorLine(page, QString(500, QLatin1Char('x')), chunk, 1);
  CHECK(cut.endsWith(QStringLiteral("x") + QChar(0x2026) + QStringLiteral(" (main-abc.js:1)")));
  CHECK(cut.size() == diag::kLineLength + int(QStringLiteral(" (main-abc.js:1)").size()));

  // The newest few, the same one again not twice in a row.
  diag::State state;
  for (int i = 1; i <= 7; ++i) {
    diag::addPageError(state, QStringLiteral("error %1").arg(i));
  }
  diag::addPageError(state, QStringLiteral("error 7"));
  CHECK(state.pageErrors.size() == diag::kPageErrors);
  CHECK(state.pageErrors.first() == QStringLiteral("error 3"));
  CHECK(state.pageErrors.last() == QStringLiteral("error 7"));
}

void gpuLogTests() {
  using diag::GpuLogLine;
  // The GPU process ended, exited, or a context's GPU state was lost.
  CHECK(diag::classifyGpuLogLine(kExited) == GpuLogLine::Failure);
  CHECK(diag::classifyGpuLogLine(QStringLiteral(
            "[1:2:1115/101011.000001:ERROR:gpu/ipc/service/gpu_channel_manager.cc:400] Exiting GPU process "
            "because some drivers can't recover from errors. GPU process will restart shortly.")) ==
        GpuLogLine::Failure);
  CHECK(diag::classifyGpuLogLine(QStringLiteral(
            "[1:3:1115/101012.000001:ERROR:gpu/command_buffer/client/command_buffer_proxy_impl.cc:327] GPU state "
            "invalid after WaitForGetOffsetInRange.")) == GpuLogLine::Failure);
  CHECK(diag::classifyGpuLogLine(QStringLiteral("GPU process launch failed: error_code=1002")) == GpuLogLine::Failure);
  CHECK(diag::classifyGpuLogLine(QStringLiteral("The GPU process exited with code 34.")) == GpuLogLine::Failure);
  CHECK(diag::classifyGpuLogLine(QStringLiteral("GPU process isn't usable. Goodbye.")) == GpuLogLine::Failure);
  // The GPU's other lines: kept, no failure.
  CHECK(diag::classifyGpuLogLine(QStringLiteral(
            "[1:2:1115/101010.000001:WARNING:ui/gl/gl_display.cc:900] eglInitialize OpenGL failed with error "
            "EGL_NOT_INITIALIZED, trying next display type")) == GpuLogLine::Gpu);
  CHECK(diag::classifyGpuLogLine(QStringLiteral("Automatic fallback to software WebGL has been deprecated.")) ==
        GpuLogLine::Gpu);
  CHECK(diag::classifyGpuLogLine(QStringLiteral("[1:2:1115/1.0:ERROR:third_party/angle/src/x.cpp:1] ANGLE error")) ==
        GpuLogLine::Gpu);
  // Anything else.
  CHECK(diag::classifyGpuLogLine(QStringLiteral(
            "[1:5:1115/101010.000001:ERROR:net/socket/ssl_client_socket_impl.cc:900] handshake failed")) ==
        GpuLogLine::Other);

  // The message without Chromium's prefix.
  CHECK(diag::gpuLogText(kExited) == QStringLiteral("GPU process exited unexpectedly: exit_code=139"));
  CHECK(diag::gpuLogText(QStringLiteral("  no prefix ")) == QStringLiteral("no prefix"));

  diag::State state;
  for (int i = 1; i <= 4; ++i) {
    diag::addGpuLine(state, QStringLiteral("line %1").arg(i));
  }
  CHECK(state.gpuLines == (QStringList{QStringLiteral("line 2"), QStringLiteral("line 3"), QStringLiteral("line 4")}));
}

void probeTests() {
  QString webgl;
  QString detail;
  CHECK(diag::parseWebglProbe(QStringLiteral("mitcad:webgl on 2 ") + kNvidia, &webgl, &detail));
  CHECK(webgl == QStringLiteral("gpu"));
  CHECK(detail == QStringLiteral("WebGL 2, ") + kNvidia);
  CHECK(diag::parseWebglProbe(QStringLiteral("mitcad:webgl on 2 ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device "
                                             "(Subzero) (0x0000C0DE)), SwiftShader driver)"),
                              &webgl, &detail));
  CHECK(webgl == QStringLiteral("software"));
  CHECK(diag::parseWebglProbe(QStringLiteral("mitcad:webgl on 1 llvmpipe (LLVM 15.0.7, 256 bits)"), &webgl, &detail));
  CHECK(webgl == QStringLiteral("software"));
  CHECK(detail == QStringLiteral("WebGL 1, llvmpipe (LLVM 15.0.7, 256 bits)"));
  CHECK(diag::parseWebglProbe(QStringLiteral("mitcad:webgl off Web page caused context loss and was blocked"), &webgl,
                              &detail));
  CHECK(webgl == QStringLiteral("off"));
  CHECK(detail == QStringLiteral("Web page caused context loss and was blocked"));
  CHECK(diag::parseWebglProbe(QStringLiteral("mitcad:webgl off"), &webgl, &detail));
  CHECK(detail == QStringLiteral("no WebGL context"));
  // Not the probe's.
  CHECK(!diag::parseWebglProbe(QStringLiteral("mitcad:webgl-lost "), &webgl, &detail));
  CHECK(!diag::parseWebglProbe(QStringLiteral("mitcad:links:3"), &webgl, &detail));
  // The script logs what the parser reads.
  const QString script = QString::fromLatin1(diag::kWebglProbeScript);
  CHECK(script.contains(QStringLiteral("'mitcad:webgl on '")));
  CHECK(script.contains(QStringLiteral("'mitcad:webgl off '")));
  CHECK(script.contains(QStringLiteral("'mitcad:webgl-lost '")));
}

void diagnosticsTests() {
  // Not started: nothing.
  CHECK(diag::diagnosticsLines(diag::State()).isEmpty());
  QObject application;
  CHECK(diag::componentBrowserDiagnostics(&application).isEmpty());
  CHECK(diag::componentBrowserDiagnostics(nullptr).isEmpty());

  // The state as the module leaves it on the application object.
  diag::State state;
  state.cef = QStringLiteral("156.0.28");
  state.webgl = QStringLiteral("gpu");
  state.webglDetail = QStringLiteral("WebGL 2, ") + kNvidia;
  state.gpuFailures = 2;
  diag::addGpuLine(state, diag::gpuLogText(kExited));
  state.softwareNext = true;
  state.contextsLost = 3;
  diag::addPageError(state, QStringLiteral("www.example.com: Uncaught TypeError: a is null (main-abc.js:12)"));
  application.setProperty(diag::kStateProperty, diag::toVariant(state));
  const QStringList lines = diag::componentBrowserDiagnostics(&application);
  CHECK(lines.contains(QStringLiteral("Component browser: CEF 156.0.28, WebGL on the GPU where Chromium accepts it, "
                                      "else in software")));
  CHECK(lines.contains(QStringLiteral("Component browser WebGL: on the GPU, WebGL 2, ") + kNvidia));
  CHECK(lines.contains(QStringLiteral(
      "Component browser GPU process: failures in CEF's log: 2; WebGL in software from the next start")));
  CHECK(lines.contains(QStringLiteral("Component browser GPU log: GPU process exited unexpectedly: exit_code=139")));
  CHECK(lines.contains(QStringLiteral("Component browser WebGL contexts lost: 3")));
  CHECK(lines.contains(QStringLiteral(
      "Component browser page error: www.example.com: Uncaught TypeError: a is null (main-abc.js:12)")));

  // In software only, before a page: said so, no errors.
  diag::State software;
  software.cef = QStringLiteral("156.0.28");
  software.softwareOnly = true;
  software.softwareReason = QStringLiteral("MITCAD_WEB_GPU=software");
  const QStringList softwareLines = diag::diagnosticsLines(diag::fromVariant(diag::toVariant(software)));
  CHECK(softwareLines.contains(QStringLiteral(
      "Component browser: CEF 156.0.28, WebGL in software only (MITCAD_WEB_GPU=software)")));
  CHECK(softwareLines.contains(QStringLiteral("Component browser WebGL: not checked yet (no page loaded)")));
  CHECK(softwareLines.contains(QStringLiteral("Component browser GPU process: no failure in CEF's log")));
  CHECK(softwareLines.contains(QStringLiteral("Component browser page errors: none")));
}

void softwareOnlyTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const QString marker = QDir(folder.path()).filePath(QStringLiteral("profile/") +
                                                      QString::fromLatin1(diag::kSoftwareMarker));
  const QString cef = QStringLiteral("156.0.28");
  // No marker: the GPU where Chromium accepts it.
  CHECK(!diag::chooseGpu(QString(), marker, cef).software);
  // MITCAD_WEB_GPU=software: software only.
  const diag::GpuChoice chosen = diag::chooseGpu(QStringLiteral("software"), marker, cef);
  CHECK(chosen.software);
  CHECK(chosen.reason == QStringLiteral("MITCAD_WEB_GPU=software"));
  // After a failure of the GPU path: software only for that CEF.
  CHECK(diag::writeSoftwareMarker(marker, cef, QStringLiteral("GPU process exited unexpectedly: exit_code=139")));
  const diag::GpuChoice after = diag::chooseGpu(QString(), marker, cef);
  CHECK(after.software);
  CHECK(after.reason.startsWith(QStringLiteral("the GPU failed in an earlier session (")));
  CHECK(after.reason.endsWith(QStringLiteral("): GPU process exited unexpectedly: exit_code=139")));
  // MITCAD_WEB_GPU=gpu over the marker; another CEF does not follow it.
  CHECK(!diag::chooseGpu(QStringLiteral("gpu"), marker, cef).software);
  CHECK(!diag::chooseGpu(QString(), marker, QStringLiteral("157.0.1")).software);
  // A marker that is not one: the GPU.
  QFile broken(marker);
  CHECK(broken.open(QIODevice::WriteOnly | QIODevice::Truncate));
  broken.write("not json");
  broken.close();
  CHECK(!diag::chooseGpu(QString(), marker, cef).software);
}

// When the GPU path counts as failed, for software from the next start.
void softwareNextTests() {
  diag::State state;
  state.cef = QStringLiteral("156.0.28");
  // Nothing yet: no page checked, no failure.
  CHECK(!diag::softwareNextWanted(state, false, false));
  // WebGL on the GPU, no failure: the GPU stays.
  state.webgl = QStringLiteral("gpu");
  CHECK(!diag::softwareNextWanted(state, true, false));
  // The GPU process failed after WebGL on the GPU: software next.
  CHECK(diag::softwareNextWanted(state, true, true));
  // A failure before any page drew WebGL on the GPU: not yet.
  state.webgl.clear();
  CHECK(!diag::softwareNextWanted(state, false, true));
  // WebGL in software all along (Chromium refused the GPU at once): nothing
  // to gain, failure or not.
  state.webgl = QStringLiteral("software");
  CHECK(!diag::softwareNextWanted(state, false, true));
  CHECK(!diag::softwareNextWanted(state, false, false));
  // WebGL off with the GPU in use (a context refused there, no SwiftShader
  // fallback), with or without a failure of the GPU process: software next.
  state.webgl = QStringLiteral("off");
  CHECK(diag::softwareNextWanted(state, false, false));
  CHECK(diag::softwareNextWanted(state, false, true));
  // Not again once the marker is written, and never in software only.
  state.softwareNext = true;
  CHECK(!diag::softwareNextWanted(state, false, false));
  state.softwareNext = false;
  state.softwareOnly = true;
  CHECK(!diag::softwareNextWanted(state, true, true));
}

} // namespace

int browserDiagnosticsTests() {
  pageMessageTests();
  gpuLogTests();
  probeTests();
  diagnosticsTests();
  softwareOnlyTests();
  softwareNextTests();
  return failures;
}
MITCAD_UNIT_TEST(browserDiagnosticsTests);
