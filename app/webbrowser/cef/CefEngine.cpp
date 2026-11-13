// SPDX-License-Identifier: MIT
// The Chromium Embedded Framework in the web browser component's module
// (mitcad#1579): see CefEngine.hpp.
#include "CefEngine.hpp"

#include <algorithm>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
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

class App : public CefApp, public CefBrowserProcessHandler {
public:
  CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override { return this; }

  void OnBeforeCommandLineProcessing(const CefString& processType, CefRefPtr<CefCommandLine> commandLine) override {
    if (!processType.empty()) {
      return;
    }
    // Painted in software from the off-screen buffer (OnPaint): no GPU
    // process, which VMs and test sessions do not have.
    commandLine->AppendSwitch("disable-gpu");
    commandLine->AppendSwitch("disable-gpu-compositing");
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
    // The same windowing system as Qt's (mitcad#1716): Qt runs on xcb
    // (app/main.cpp; XWayland on a Wayland desktop), and a Chromium left to
    // itself may pick Wayland beside it. Not when the command line names
    // one already.
    if (QGuiApplication::platformName() == QLatin1String("xcb") && !commandLine->HasSwitch("ozone-platform")) {
      commandLine->AppendSwitchWithValue("ozone-platform", "x11");
      qInfo().noquote() << QStringLiteral("Component browser: CEF on X11 as Qt (ozone-platform x11)");
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
};

Engine& engine() {
  static Engine state;
  return state;
}

void shutDown() {
  Engine& state = engine();
  if (!state.running) {
    return;
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
  return true;
}

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
