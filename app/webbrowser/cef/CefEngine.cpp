// SPDX-License-Identifier: MIT
// The Chromium Embedded Framework in the web browser component's module
// (mitcad#1579): see CefEngine.hpp.
#include "CefEngine.hpp"

#include <algorithm>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QList>
#include <QMetaObject>
#include <QObject>
#include <QPointer>
#include <QThread>
#include <QTimer>
#include <QtGlobal>
#include <QtLogging>

#include "CefView.hpp"
#include "webbrowser/BrowserDiagnostics.hpp"
#include "include/cef_app.h"
#include "include/cef_command_line.h"
#include "include/cef_version.h"
#if defined(__APPLE__)
#include "include/wrapper/cef_library_loader.h"
#endif

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace mitcad::cef {
namespace {

// What the helper is called in the component's folder.
QString helperPath(const QString& folder) {
#if defined(_WIN32)
  return QDir(folder).filePath(QStringLiteral("mitcad-webbrowser-helper.exe"));
#elif defined(__APPLE__)
  return QDir(folder).filePath(QStringLiteral("MitCad Browser Helper.app/Contents/MacOS/MitCad Browser Helper"));
#else
  return QDir(folder).filePath(QStringLiteral("mitcad-webbrowser-helper"));
#endif
}

std::string utf8(const QString& text) { return text.toStdString(); }

// The loop's driver on Qt's main thread: CEF's work whenever CEF asks for
// it (OnScheduleMessagePumpWork, from any thread), and at least every 30 ms.
class Pump : public QObject {
public:
  Pump() {
    m_regular.setInterval(30);
    connect(&m_regular, &QTimer::timeout, this, [this] { work(); });
    m_soon.setSingleShot(true);
    connect(&m_soon, &QTimer::timeout, this, [this] { work(); });
  }
  void start() { m_regular.start(); }
  void stop() {
    m_regular.stop();
    m_soon.stop();
  }
  void schedule(std::int64_t delayMs) {
    const int delay = static_cast<int>(std::clamp<std::int64_t>(delayMs, 0, 30));
    if (!m_soon.isActive() || m_soon.remainingTime() > delay) {
      m_soon.start(delay);
    }
  }
  void work() {
    if (m_working || m_stopped) {
      return;
    }
    m_working = true;
    CefDoMessageLoopWork();
    m_working = false;
  }
  void halt() {
    stop();
    m_stopped = true;
  }

private:
  QTimer m_regular;
  QTimer m_soon;
  bool m_working = false;
  bool m_stopped = false;
};

Pump* g_pump = nullptr;

// How WebGL is drawn this run (mitcad#1906): chosen before CEF starts, read
// as CEF makes its command line.
browserdiag::GpuChoice g_gpu;

class App : public CefApp, public CefBrowserProcessHandler {
public:
  CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override { return this; }

  void OnBeforeCommandLineProcessing(const CefString& processType, CefRefPtr<CefCommandLine> commandLine) override {
    if (!processType.empty()) {
      return;
    }
    // The page is composited in software into the off-screen buffer
    // (OnPaint), which the view paints into its widget: no GPU texture to
    // share with Qt, the same on a machine without a GPU.
    commandLine->AppendSwitch("disable-gpu-compositing");
    // WebGL (mitcad#1861): model sites draw their 3D previews with it, and
    // fail without it. The GPU process renders it on the machine's GPU
    // where there is one that Chromium accepts, and in software
    // (SwiftShader) where there is none or Chromium refuses its driver (VMs,
    // test sessions); the canvas then reaches the software compositor as
    // any other picture. Chromium no longer falls back to SwiftShader for
    // WebGL by itself, and calls the switch that allows it unsafe for
    // untrusted content: its shader compiler and rasteriser run on the CPU
    // in the GPU process. Allowed here, as a site without WebGL does not
    // work at all, the GPU is used wherever it can be, and the pages are
    // the supplier sites the user chose (app/COMMANDS.md, "Component from
    // Web", The browser). Not "disable-gpu" by default, which leaves only
    // SwiftShader (and without this switch no WebGL at all).
    commandLine->AppendSwitch("enable-unsafe-swiftshader");
    // A WebGL context lost on the GPU (the GPU process ended or reset) does
    // not block WebGL for the page's site for the rest of the run (mitcad#1906,
    // GitHub mitcad/Mitcad#364): Chromium does that by default, and the
    // site's preview then failed sooner after each Reload.
    commandLine->AppendSwitch("disable-domain-blocking-for-3d-apis");
    // The GPU path failed in an earlier session of this CEF (its marker in
    // the profile), or MITCAD_WEB_GPU=software: no GPU, WebGL in software
    // (SwiftShader, allowed above) as on a machine without a usable GPU.
    if (g_gpu.software) {
      commandLine->AppendSwitch("disable-gpu");
    }
    commandLine->AppendSwitch("no-first-run");
    commandLine->AppendSwitch("no-default-browser-check");
#if defined(__APPLE__)
    // Chrome's updater clones its application and starts a cleanup helper
    // at shutdown without CEF's external-framework arguments. MitCad and
    // this separately installed component use their own update mechanism.
    std::string disabledFeatures = commandLine->GetSwitchValue("disable-features").ToString();
    if (!disabledFeatures.empty()) {
      disabledFeatures += ",";
    }
    disabledFeatures += "MacAppCodeSignClone";
    commandLine->AppendSwitchWithValue("disable-features", disabledFeatures);
    // The browsing profile's passwords not in the user's keychain.
    commandLine->AppendSwitch("use-mock-keychain");
#elif defined(__linux__)
    commandLine->AppendSwitchWithValue("password-store", "basic");
    // The same windowing system as Qt's (mitcad#1716): Qt runs on xcb or,
    // in a Wayland session, on Wayland (app/main.cpp, mitcad#2031), and a
    // Chromium left to itself may pick the other one beside it. Not when
    // the command line names one already.
    if (!commandLine->HasSwitch("ozone-platform")) {
      const QString platform = QGuiApplication::platformName();
      if (platform == QLatin1String("xcb")) {
        commandLine->AppendSwitchWithValue("ozone-platform", "x11");
        qInfo().noquote() << QStringLiteral("Component browser: CEF on X11 as Qt (ozone-platform x11)");
      } else if (platform.startsWith(QLatin1String("wayland"))) {
        commandLine->AppendSwitchWithValue("ozone-platform", "wayland");
        qInfo().noquote() << QStringLiteral("Component browser: CEF on Wayland as Qt (ozone-platform wayland)");
      }
    }
#endif
  }

  void OnScheduleMessagePumpWork(std::int64_t delayMs) override {
    // From any thread: onto Qt's main thread.
    if (g_pump == nullptr) {
      return;
    }
    QMetaObject::invokeMethod(
        g_pump, [delayMs] {
          if (g_pump != nullptr) {
            g_pump->schedule(delayMs);
          }
        },
        Qt::QueuedConnection);
  }

private:
  IMPLEMENT_REFCOUNTING(App);
};

struct Engine {
  bool tried = false;
  bool running = false;
  QString error;
  CefRefPtr<CefRequestContext> context;
  QList<QPointer<CefView>> views;
  std::vector<CefRefPtr<CefBrowser>> browsers;
  // The diagnostics (mitcad#1906, webbrowser/BrowserDiagnostics.hpp): the
  // state the application object carries, the profile (the GPU's marker),
  // CEF's log read from where it ended before this start, and whether the
  // GPU path failed with WebGL drawn on the GPU.
  browserdiag::State diag;
  QString profile;
  QString logPath;
  qint64 logOffset = 0;
  QByteArray logPartial;
  QTimer* logTimer = nullptr;
  bool seenGpuWebgl = false;
  bool failurePending = false;
  bool markerFailed = false; // the marker could not be written: not tried again
  QString failure;
  QString testFailure; // MITCAD_TEST_WEB_GPU_FAILURE
};

Engine& engine() {
  static Engine state;
  return state;
}

// The state into the application object's property, where Help › Send
// Feedback's diagnostics read it.
void publish() {
  if (QCoreApplication* application = QCoreApplication::instance()) {
    application->setProperty(browserdiag::kStateProperty, browserdiag::toVariant(engine().diag));
  }
}

QString webglText(const QString& webgl, const QString& detail) {
  if (webgl == QLatin1String("gpu")) {
    return QStringLiteral("on the GPU, ") + detail;
  }
  if (webgl == QLatin1String("software")) {
    return QStringLiteral("in software, ") + detail;
  }
  return QStringLiteral("off, ") + detail;
}

// After a failure of the GPU process where WebGL had been drawn on the GPU
// this run (or is off), or when a page found WebGL off with the GPU in use
// (Chromium refused a context there without falling back to SwiftShader):
// the next start of CEF draws it in software (CEF starts once per process,
// so not this run), and the windows say so
// (browserdiag::softwareNextWanted). Not where WebGL was in software all
// along (a machine whose GPU Chromium refused at once): nothing to gain
// there.
void fallBackIfFailed() {
  Engine& state = engine();
  if (state.markerFailed || !browserdiag::softwareNextWanted(state.diag, state.seenGpuWebgl, state.failurePending)) {
    return;
  }
  if (!state.failurePending) {
    state.failure = QStringLiteral("WebGL unavailable on the GPU: %1").arg(state.diag.webglDetail);
  }
  const QString marker = QDir(state.profile).filePath(QLatin1String(browserdiag::kSoftwareMarker));
  if (!browserdiag::writeSoftwareMarker(marker, QStringLiteral(CEF_VERSION), state.failure)) {
    qWarning().noquote() << QStringLiteral("Component browser: the GPU's marker could not be written: %1")
                                .arg(QDir::toNativeSeparators(marker));
    state.failurePending = false;
    state.markerFailed = true;
    return;
  }
  state.diag.softwareNext = true;
  qInfo().noquote() << QStringLiteral("Component browser: the GPU failed (%1); WebGL in software from the next start")
                           .arg(state.failure);
  publish();
  for (const QPointer<CefView>& view : std::as_const(state.views)) {
    if (!view.isNull()) {
      view->gpuFailedNotice();
    }
  }
}

// A line of CEF's log: the GPU's kept, a failure of its process counted.
void noteLogLine(const QString& line) {
  const browserdiag::GpuLogLine kind = browserdiag::classifyGpuLogLine(line);
  if (kind == browserdiag::GpuLogLine::Other) {
    return;
  }
  Engine& state = engine();
  const QString text = browserdiag::gpuLogText(line);
  browserdiag::addGpuLine(state.diag, text);
  if (kind == browserdiag::GpuLogLine::Failure) {
    ++state.diag.gpuFailures;
    state.failurePending = true;
    state.failure = text;
    qInfo().noquote() << QStringLiteral("Component browser GPU failure: %1").arg(text);
    fallBackIfFailed();
  }
  publish();
}

// CEF's log (every CEF process writes it, from warnings up) since the last
// read: its new whole lines.
void readLog() {
  Engine& state = engine();
  if (!state.testFailure.isEmpty()) {
    // For the test of the fallback: a failure of the GPU process as after
    // WebGL had been drawn on the GPU (the test machines have no GPU).
    const QString line = state.testFailure;
    state.testFailure.clear();
    state.seenGpuWebgl = true;
    qInfo().noquote() << QStringLiteral("Component browser: a GPU failure for a test (MITCAD_TEST_WEB_GPU_FAILURE)");
    noteLogLine(line);
  }
  QFile file(state.logPath);
  if (!file.open(QIODevice::ReadOnly)) {
    return;
  }
  const qint64 size = file.size();
  if (size < state.logOffset) {
    // Started again from nothing.
    state.logOffset = 0;
    state.logPartial.clear();
  }
  if (size == state.logOffset || !file.seek(state.logOffset)) {
    return;
  }
  QByteArray data = file.read(std::min<qint64>(size - state.logOffset, qint64(4) * 1024 * 1024));
  state.logOffset += data.size();
  data.prepend(state.logPartial);
  const qsizetype end = data.lastIndexOf('\n');
  if (end < 0) {
    state.logPartial = data.size() > 65536 ? QByteArray() : data;
    return;
  }
  state.logPartial = data.mid(end + 1);
  const QList<QByteArray> lines = data.left(end).split('\n');
  for (const QByteArray& line : lines) {
    const QString text = QString::fromUtf8(line).trimmed();
    if (!text.isEmpty()) {
      noteLogLine(text);
    }
  }
}

void shutDown() {
  Engine& state = engine();
  if (!state.running) {
    return;
  }
  if (state.logTimer != nullptr) {
    state.logTimer->stop();
  }
  // Every view drops its browser, the browsers close, then CEF ends; within
  // a few seconds, whatever a page does.
  for (const QPointer<CefView>& view : std::as_const(state.views)) {
    if (!view.isNull()) {
      view->closeBrowser();
    }
  }
  QElapsedTimer waited;
  waited.start();
  while (!state.browsers.empty() && waited.elapsed() < 5000) {
    CefDoMessageLoopWork();
    QThread::msleep(5);
  }
  if (!state.browsers.empty()) {
    qWarning().noquote() << QStringLiteral("Component browser: %1 browser(s) did not close before CEF ended")
                                .arg(state.browsers.size());
    state.browsers.clear();
  }
  state.context = nullptr;
  g_pump->halt();
  CefShutdown();
  state.running = false;
#if defined(__APPLE__)
  cef_unload_library();
#endif
  qInfo().noquote() << QStringLiteral("Component browser: CEF ended");
}

} // namespace

bool startEngine(const QString& componentFolder, const QString& profileFolder, QString* error) {
  Engine& state = engine();
  if (state.tried) {
    *error = state.error;
    return state.running;
  }
  state.tried = true;
  const QString folder = QDir::cleanPath(componentFolder);
#if defined(__APPLE__)
  const QString framework = QDir(folder).filePath(QStringLiteral("Chromium Embedded Framework.framework"));
  if (cef_load_library(utf8(QDir(framework).filePath(QStringLiteral("Chromium Embedded Framework"))).c_str()) != 1) {
    state.error = QStringLiteral("CEF's framework did not load from %1").arg(framework);
    *error = state.error;
    return false;
  }
#endif
#if defined(_WIN32)
  CefMainArgs arguments(GetModuleHandleW(nullptr));
#else
  // The browser process's own arguments are not CEF's.
  static std::string program = utf8(QCoreApplication::applicationFilePath());
  static char* argv[] = {program.data(), nullptr};
  CefMainArgs arguments(1, argv);
#endif
  QDir().mkpath(profileFolder);
  CefSettings settings;
  settings.no_sandbox = true;
  settings.multi_threaded_message_loop = false;
  settings.external_message_pump = true;
  settings.windowless_rendering_enabled = true;
  settings.command_line_args_disabled = true;
  settings.log_severity = LOGSEVERITY_WARNING;
  CefString(&settings.root_cache_path) = utf8(QDir::toNativeSeparators(profileFolder));
  CefString(&settings.log_file) = utf8(QDir::toNativeSeparators(QDir(profileFolder).filePath(QStringLiteral("cef.log"))));
  CefString(&settings.browser_subprocess_path) = utf8(QDir::toNativeSeparators(helperPath(folder)));
#if defined(__APPLE__)
  CefString(&settings.framework_dir_path) = utf8(framework);
  // The component's helpers live outside the application's bundle. Give
  // every CEF process the same outer bundle and Mach rendezvous namespace.
  CefString(&settings.main_bundle_path) =
      utf8(QDir(QCoreApplication::applicationDirPath()).absoluteFilePath(QStringLiteral("../..")));
#else
  CefString(&settings.resources_dir_path) = utf8(QDir::toNativeSeparators(folder));
  CefString(&settings.locales_dir_path) = utf8(QDir::toNativeSeparators(QDir(folder).filePath(QStringLiteral("locales"))));
#endif
  if (!QFileInfo::exists(helperPath(folder))) {
    state.error = QStringLiteral("the component has no subprocess helper %1").arg(helperPath(folder));
    *error = state.error;
    return false;
  }
  // How WebGL is drawn this run (mitcad#1906), and CEF's log from where it
  // ends now: what this run adds is read for the GPU's failures.
  state.profile = profileFolder;
  g_gpu = browserdiag::chooseGpu(qEnvironmentVariable("MITCAD_WEB_GPU"),
                                 QDir(profileFolder).filePath(QLatin1String(browserdiag::kSoftwareMarker)),
                                 QStringLiteral(CEF_VERSION));
  state.logPath = QDir(profileFolder).filePath(QStringLiteral("cef.log"));
  state.logOffset = QFileInfo(state.logPath).size();
  state.testFailure = qEnvironmentVariable("MITCAD_TEST_WEB_GPU_FAILURE");
  g_pump = new Pump();
  g_pump->setParent(QCoreApplication::instance());
  CefRefPtr<App> app(new App());
  if (!CefInitialize(arguments, settings, app.get(), nullptr)) {
    // Another running MitCad uses the same profile, or the runtime does not
    // start here.
    state.error = QStringLiteral("CEF did not start; is another MitCad using the profile %1?")
                      .arg(QDir::toNativeSeparators(profileFolder));
    *error = state.error;
    g_pump->halt();
    return false;
  }
  state.running = true;
  g_pump->start();
  CefRequestContextSettings contextSettings;
  CefString(&contextSettings.cache_path) =
      utf8(QDir::toNativeSeparators(QDir(profileFolder).filePath(QStringLiteral("MitCad"))));
  contextSettings.persist_session_cookies = true;
  state.context = CefRequestContext::CreateContext(contextSettings, nullptr);
  QObject::connect(QCoreApplication::instance(), &QCoreApplication::aboutToQuit, QCoreApplication::instance(),
                   [] { shutDown(); });
  qInfo().noquote() << QStringLiteral("Component browser: CEF %1 started from %2, profile %3")
                           .arg(QStringLiteral(CEF_VERSION), QDir::toNativeSeparators(folder),
                                QDir::toNativeSeparators(profileFolder));
  state.diag.cef = QStringLiteral(CEF_VERSION);
  state.diag.softwareOnly = g_gpu.software;
  state.diag.softwareReason = g_gpu.reason;
  publish();
  if (g_gpu.software) {
    qInfo().noquote() << QStringLiteral("Component browser: WebGL in software only (%1)").arg(g_gpu.reason);
  } else {
    qInfo().noquote() << QStringLiteral("Component browser: WebGL on the GPU where Chromium accepts it, else in software");
  }
  state.logTimer = new QTimer(QCoreApplication::instance());
  state.logTimer->setInterval(2000);
  QObject::connect(state.logTimer, &QTimer::timeout, state.logTimer, [] { readLog(); });
  state.logTimer->start();
  return true;
}

void notePageMessage(const QString& pageUrl, bool error, const QString& message, const QString& source, int line) {
  if (!browserdiag::keepPageMessage(error, message, source)) {
    return;
  }
  Engine& state = engine();
  const QString text = browserdiag::pageErrorLine(pageUrl, message, source, line);
  if (state.diag.pageErrors.isEmpty() || state.diag.pageErrors.constLast() != text) {
    qDebug().noquote() << QStringLiteral("Component browser page console: %1").arg(text);
  }
  browserdiag::addPageError(state.diag, text);
  publish();
}

bool noteScriptMessage(const QString& message) {
  Engine& state = engine();
  if (message.startsWith(QLatin1String("mitcad:webgl-lost"))) {
    ++state.diag.contextsLost;
    qDebug().noquote() << QStringLiteral("Component browser: a WebGL context of the page was lost");
    publish();
    return true;
  }
  QString webgl;
  QString detail;
  if (!browserdiag::parseWebglProbe(message, &webgl, &detail)) {
    return false;
  }
  if (webgl != state.diag.webgl || detail != state.diag.webglDetail) {
    state.diag.webgl = webgl;
    state.diag.webglDetail = detail;
    qInfo().noquote() << QStringLiteral("Component browser WebGL: %1").arg(webglText(webgl, detail));
  }
  if (webgl == QLatin1String("gpu")) {
    state.seenGpuWebgl = true;
  }
  fallBackIfFailed();
  publish();
  return true;
}

bool gpuFailed() { return engine().diag.softwareNext; }

bool engineRunning() { return engine().running; }

CefRefPtr<CefRequestContext> requestContext() { return engine().context; }

void pumpSoon() {
  if (g_pump != nullptr) {
    g_pump->schedule(0);
  }
}

void addView(CefView* view) { engine().views.append(QPointer<CefView>(view)); }

void removeView(CefView* view) {
  engine().views.removeIf([view](const QPointer<CefView>& known) { return known.isNull() || known.data() == view; });
}

void browserCreated(CefRefPtr<CefBrowser> browser) { engine().browsers.push_back(browser); }

void browserClosed(CefRefPtr<CefBrowser> browser) {
  std::vector<CefRefPtr<CefBrowser>>& browsers = engine().browsers;
  browsers.erase(std::remove_if(browsers.begin(), browsers.end(),
                                [&browser](const CefRefPtr<CefBrowser>& known) { return known->IsSame(browser); }),
                 browsers.end());
}

} // namespace mitcad::cef
