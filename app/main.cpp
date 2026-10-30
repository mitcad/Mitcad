// SPDX-License-Identifier: MIT
#include <QApplication>
#include <QCommandLineParser>
#include <QFileInfo>
#include <QIcon>
#include <QImage>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QStyle>
#include <QStyleHints>
#include <QSurfaceFormat>
#include <QTimer>
#include <QtGlobal>
#include <QtLogging>
#include <memory>
#include <optional>

#include "MainWindow.hpp"
#include "framework/CommandRegistry.hpp"
#include "OcctViewer.hpp"
#include "files/F3dImport.hpp"
#include "files/FileOpenFilter.hpp"
#include "framework/AppSettings.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/DebugLog.hpp"
#include "framework/DesktopEntry.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/DisplayMeshes.hpp"
#include "framework/Numbers.hpp"
#include "framework/ProcessEnd.hpp"
#include "framework/Restart.hpp"
#include "framework/TestDriver.hpp"
#include "framework/TestSync.hpp"
#include "framework/TypedTextLog.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mcp/AgentFolders.hpp"
#include "mcp/LiveServer.hpp"
#include "mcp/LogRing.hpp"
#include "platform/MacChrome.hpp"
#include "report/CrashHandler.hpp"
#include "report/ReportCenter.hpp"

namespace {

// The application, or its import worker; main ends the process with what
// this returns (framework/ProcessEnd.hpp).
int runApplication(int argc, char* argv[]) {
  // Crash reports (mitcad#62), in the application and its import worker.
  const bool importWorker = mitcad::isImportWorker(argc, argv);
  mitcad::crash::install(importWorker ? "import-worker" : "app", MITCAD_VERSION);
  // An .f3d, .FCStd, .ipt or .iam import in a process of its own, without
  // windows (U6).
  if (importWorker) {
    if (mitcad::crash::testCrashRequested("import-worker")) {
      // For tests: a crash where OCCT's handlers are installed, outside an
      // operation of theirs, as an import's could be.
      mitcad::geometry::catch_occt_crashes();
      mitcad::crash::crashNow();
    }
    mitcad::ReportCenter::notePanics();
    return mitcad::runImportWorker(argc, argv);
  }
  // OCCT's crash handlers on the UI thread too (on Windows they are per
  // thread; the model worker installs its own): Section Analysis clips the
  // bodies shown and measures sections here, and a crash inside OCCT in
  // such an operation is its error (mitcad#392). Any other crash goes to
  // the crash report installed above.
  mitcad::geometry::catch_occt_crashes();

#ifdef __linux__
  // OCCT's OpenGL driver uses GLX on Linux, so Qt must use the X11 (xcb)
  // platform plugin, also on Wayland desktops and WSLg.
  if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORM")) {
    qputenv("QT_QPA_PLATFORM", "xcb");
  }
  // The AppImage bundles no GTK, so Qt's gtk3 theme, which a GNOME desktop
  // would get its dark or light scheme from, is not there and Qt's own
  // falls back to light: the desktop portal's theme follows the scheme.
  if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORMTHEME") && !qEnvironmentVariableIsEmpty("APPIMAGE")) {
    qputenv("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
  }
#endif
#ifdef _WIN32
  // OCCT calls opengl32.dll directly, so Qt must use it too. Without a GPU
  // it can recognise (e.g. in a VM) Qt would switch to its own software
  // renderer, opengl32sw.dll, whose contexts OCCT cannot use.
  QCoreApplication::setAttribute(Qt::AA_UseDesktopOpenGL);
#endif
#ifdef Q_OS_MACOS
  // Every OpenGL context Qt creates must be a core profile: macOS cannot
  // share resources between contexts of different profiles, and Qt shares
  // the viewport's context with the one that composes the window.
  QSurfaceFormat::setDefaultFormat(mitcad::macSurfaceFormat());
#endif

#ifdef MITCAD_MACOS_NATIVE_UI
  bool nativeUiRequested = true;
  // Keep Qt's application delegate and activation handling while the shell
  // owns the menu bar. Plugin mode would suppress that delegate entirely.
  // The full parser below validates these options.
  for (int index = 1; index < argc; ++index) {
    const QString argument = QString::fromLocal8Bit(argv[index]);
    if (argument.startsWith(QStringLiteral("--ui="))) {
      nativeUiRequested = argument.mid(5) == QStringLiteral("native");
    } else if (argument == QStringLiteral("--ui") && index + 1 < argc) {
      nativeUiRequested = QString::fromLocal8Bit(argv[++index]) == QStringLiteral("native");
    } else if (argument.startsWith(QStringLiteral("--chrome"))) {
      nativeUiRequested = false;
    }
  }
  QCoreApplication::setAttribute(Qt::AA_DontUseNativeMenuBar, nativeUiRequested);
#else
  bool nativeUiRequested = false;
#endif
  QApplication app(argc, argv);
  // UI tests' sync key (MITCAD_TEST_SYNC), before the window's timers exist.
  mitcad::TestSync::installIfRequested();
  // What the UI tests type into text fields (MITCAD_LOG_TYPED_TEXT).
  mitcad::installTypedTextLogIfRequested();
  // The key presses the Windows UI tests post, with their modifiers
  // (MITCAD_LOG_KEYS, mitcad#271).
  mitcad::installKeyLogIfRequested();
  // The stored names (Mitcad/Mitcad), not the program's name MitCad: the
  // settings and the data folders are keyed on them (mitcad#822).
  QApplication::setOrganizationName(mitcad::storedApplicationName());
#ifdef Q_OS_MACOS
  // macOS names the settings file after the reversed domain
  // (fi.nocodo.Mitcad.plist, not com.Mitcad.Mitcad.plist); it should match
  // the bundle identifier fi.nocodo.mitcad (MITCAD_BUNDLE_ID in
  // app/packaging/macos). Windows and Linux key their settings on the
  // organization name, so the domain stays unset there.
  const QString kOrganizationDomain = QStringLiteral("nocodo.fi");
  QApplication::setOrganizationDomain(kOrganizationDomain);
#endif
  QApplication::setApplicationName(mitcad::storedApplicationName());
  QApplication::setApplicationVersion(QStringLiteral(MITCAD_VERSION));
  // Every rendered size, so that the title bar and the taskbar each get the
  // one drawn for them (the small ones are simplified).
  QIcon appIcon;
  for (const int size : {16, 20, 24, 32, 40, 48, 64, 96, 128, 256}) {
    appIcon.addFile(QStringLiteral(":/branding/png/mitcad-%1.png").arg(size), QSize(size, size));
  }
  QApplication::setWindowIcon(appIcon);
  // The desktop file whose icon the desktop shows (packaging/linux).
  QGuiApplication::setDesktopFileName(QStringLiteral("mitcad"));
  mitcad::integrateDesktop();
  // UI tests keep their settings (shortcuts, recent files, preferences)
  // apart from the user's: an INI file in a directory of their own. Linux
  // tests can use XDG_CONFIG_HOME; on Windows the settings are otherwise in
  // the registry.
  const QString settingsDir = qEnvironmentVariable("MITCAD_SETTINGS_DIR");
  if (!settingsDir.isEmpty()) {
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, settingsDir);
  }
  // Where crash reports go, also the workers' (mitcad#62).
  mitcad::ReportCenter::prepareCrashReports();

  // UI tests: MITCAD_TEST_INPUT=<script> runs the script in this process with
  // synthesised events (framework/TestDriver.hpp); inert without it. Before
  // the window, so that it records the log of the start-up too.
  mitcad::TestDriver testDriver;
  // The log ring (mcp/LogRing.hpp, mitcad#265, mitcad#269): one handler for
  // the whole run, after the test driver's, which it passes every message
  // on to; the debug log's file (framework/DebugLog.hpp) is its sink. The
  // file is closed and the handler removed before the test driver's goes.
  struct LogScope {
    LogScope() { mitcad::mcp::installLogRing(); }
    ~LogScope() {
      mitcad::DebugLog::instance().stop();
      mitcad::mcp::removeLogRing();
    }
    LogScope(const LogScope&) = delete;
    LogScope& operator=(const LogScope&) = delete;
  } logScope;

  QCommandLineParser parser;
  parser.addHelpOption();
  parser.addVersionOption();
  const QCommandLineOption uiOption(QStringLiteral("ui"),
                                    QStringLiteral("User interface: native (AppKit, when built) or qt."),
                                    QStringLiteral("native|qt"));
  parser.addOption(uiOption);
  const QCommandLineOption screenshotOption(
      QStringLiteral("screenshot"),
      QStringLiteral("Save an image of the 3D view to <file> after start-up and exit."),
      QStringLiteral("file"));
  const QCommandLineOption setOption(
      QStringLiteral("set"),
      QStringLiteral("Set parameter <name=value> after start-up, as if typed in the panel."),
      QStringLiteral("name=value"));
  const QCommandLineOption demoOption(
      QStringLiteral("demo"), QStringLiteral("Start with a dimensioned demo block."));
  const QCommandLineOption openOption(
      QStringLiteral("open"),
      QStringLiteral("Open <file> at start-up: a MitCad design (.mitcad), an .f3d or .f3z design, "
                     "a FreeCAD document (.FCStd), an .ipt part, an .iam assembly, an IFC file (.ifc, .ifczip), an "
                     ".rvt project or .rfa family, a SolidCraft design (.solidcraft), a .dwg drawing's 3D content, "
                     "STEP, IGES, BRep, STL, OBJ or DXF as a new document."),
      QStringLiteral("file"));
  const QCommandLineOption qtDialogsOption(
      QStringLiteral("no-native-dialogs"),
      QStringLiteral("Use Qt's own file dialogs instead of the platform's (for UI tests)."));
  const QCommandLineOption noRecoveryOption(
      QStringLiteral("no-recovery"),
      QStringLiteral("Do not offer unsaved work kept after a crash at start-up (File > Recover "
                     "Documents still does; for tests and scripts, as is --screenshot)."));
  const QCommandLineOption chromeOption(
      QStringLiteral("chrome"),
      QStringLiteral("Window layout: docked (panels in docks) or floating (panels over the 3D view; "
                     "the default on macOS)."),
      QStringLiteral("docked|floating"));
  const QCommandLineOption noGlassOption(
      QStringLiteral("no-glass"),
      QStringLiteral("Floating panels paint their own material instead of the system's Liquid Glass "
                     "(macOS 26 and later; elsewhere there is none)."));
  parser.addOption(noGlassOption);
  const QCommandLineOption noMcpOption(QStringLiteral("no-mcp"),
                                      QStringLiteral("Disable the local MCP connection to this application."));
  parser.addOption(noMcpOption);
  // Folders for designs for this run only (mitcad#402), besides those of
  // Preferences > AI Assistants; repeatable.
  const QCommandLineOption mcpFolderOption(
      QStringLiteral("mcp-folder"),
      QStringLiteral("Allow an MCP agent to save designs to and open them from <dir> for this run (besides the "
                     "folders of Preferences > AI Assistants); may be given more than once."),
      QStringLiteral("dir"));
  parser.addOption(mcpFolderOption);
  parser.addOption(screenshotOption);
  parser.addOption(setOption);
  parser.addOption(demoOption);
  parser.addOption(openOption);
  parser.addOption(qtDialogsOption);
  parser.addOption(noRecoveryOption);
  parser.addOption(chromeOption);
  // --debug-log [FILE] (mitcad#269): its optional file is taken out of the
  // arguments first (debugLogRequest), the parser sees a plain flag.
  const QCommandLineOption debugLogOption(
      QStringLiteral("debug-log"),
      QStringLiteral("Write a debug log for this run: to FILE when the next argument is a .log file or with "
                     "--debug-log=FILE, else to a new file in the log folder (Help > Open Log Folder). "
                     "MITCAD_DEBUG_LOG=1|<file> does the same."));
  parser.addOption(debugLogOption);
  parser.addPositionalArgument(
      QStringLiteral("file"),
      QStringLiteral("Open <file> at start-up, like --open (as a file association passes it)."),
      QStringLiteral("[file]"));
  QStringList arguments = QCoreApplication::arguments();
#ifdef Q_OS_MACOS
  // NSUserDefaults consumes this AppKit launch argument before our parser.
  // UI tests disable saved-state restoration without changing user defaults.
  const qsizetype persistence = arguments.indexOf(QStringLiteral("-ApplePersistenceIgnoreState"));
  if (persistence >= 0 && persistence + 1 < arguments.size()) {
    arguments.removeAt(persistence + 1);
    arguments.removeAt(persistence);
  }
#endif
  const mitcad::DebugLogRequest debugLog =
      mitcad::debugLogRequest(arguments, qEnvironmentVariable("MITCAD_DEBUG_LOG"));
  parser.process(arguments);

  if (parser.isSet(uiOption)) {
    const QString ui = parser.value(uiOption);
    if (ui != QStringLiteral("native") && ui != QStringLiteral("qt")) {
      qCritical() << "Invalid --ui argument (native or qt)";
      return 2;
    }
#ifndef MITCAD_MACOS_NATIVE_UI
    if (ui == QStringLiteral("native")) {
      qCritical() << "Native UI was not built (MITCAD_MACOS_NATIVE_UI=ON is required on macOS)";
      return 2;
    }
#endif
    nativeUiRequested = ui == QStringLiteral("native");
    if (nativeUiRequested && parser.isSet(chromeOption)) {
      qCritical() << "--chrome selects a Qt layout; use --ui=qt with it";
      return 2;
    }
  }

  // The layout (mitcad#419): --chrome for this run, else Preferences' Layout,
  // else the platform's. The native macOS frontend has a layout of its own.
  const std::optional<mitcad::ChromeStyle> layoutPreference = mitcad::layoutPreference();
  QString chromeSource = QStringLiteral("the platform's default");
  if (parser.isSet(chromeOption)) {
    const QString chrome = parser.value(chromeOption);
    if (chrome == QLatin1String("docked")) {
      mitcad::setChromeStyle(mitcad::ChromeStyle::Docked);
    } else if (chrome == QLatin1String("floating")) {
      mitcad::setChromeStyle(mitcad::ChromeStyle::Floating);
    } else {
      qCritical().noquote() << "Invalid --chrome argument (docked or floating):" << chrome;
      return 2;
    }
    chromeSource = layoutPreference ? QStringLiteral("--chrome; the preference %1")
                                          .arg(mitcad::chromeStyleName(*layoutPreference))
                                    : QStringLiteral("--chrome");
  } else if (layoutPreference && !nativeUiRequested) {
    mitcad::setChromeStyle(*layoutPreference);
    chromeSource = QStringLiteral("the preference");
  }
  mitcad::setLayoutChoosable(!nativeUiRequested);
  if (parser.isSet(noGlassOption)) {
    mitcad::setGlassRequested(false);
  }
  qInfo().noquote() << "Style:" << QApplication::style()->name() << "Chrome:"
                    << mitcad::chromeStyleName(mitcad::chromeStyle()) << QStringLiteral("(%1)").arg(chromeSource);
  // What a restart keeps of this run's arguments (framework/Restart.hpp): its
  // settings, not its start actions nor --chrome.
  {
    QStringList kept;
    if (parser.isSet(uiOption)) {
      kept << QStringLiteral("--ui=%1").arg(parser.value(uiOption));
    }
    for (const QCommandLineOption* flag : {&qtDialogsOption, &noRecoveryOption, &noGlassOption, &noMcpOption}) {
      if (parser.isSet(*flag)) {
        kept << QStringLiteral("--") + flag->names().constFirst();
      }
    }
    for (const QString& folder : parser.values(mcpFolderOption)) {
      kept << QStringLiteral("--mcp-folder=%1").arg(folder);
    }
    if (debugLog.on) {
      kept << QStringLiteral("--debug-log"); // a new file: a given one would be overwritten
    }
    mitcad::setRestartArguments(kept);
  }
  qInfo().noquote() << "Glass:" << (mitcad::glassActive() ? "on" : "off");

  if (parser.isSet(qtDialogsOption)) {
    QApplication::setAttribute(Qt::AA_DontUseNativeDialogs);
  }

  // Explicit QoS on OCCT's default pool before windows start model and
  // section workers. Starts the same pool first used by parallel meshing.
  mitcad::mac::initializeComputeThreads();

  // The file to open: --open or the positional argument, not both.
  const QStringList positional = parser.positionalArguments();
  if (positional.size() > 1 || (parser.isSet(openOption) && !positional.isEmpty())) {
    qCritical().noquote() << "Give one file only: either --open <file> or a file argument.";
    return 2;
  }
  const bool hasStartFile = parser.isSet(openOption) || !positional.isEmpty();
  const QString startFile = parser.isSet(openOption) ? parser.value(openOption)
                                                     : (positional.isEmpty() ? QString() : positional.first());

  // The debug log (mitcad#269): for this run (--debug-log, MITCAD_DEBUG_LOG) or
  // by Preferences' switch; before the window, so that the start-up is in it.
  mitcad::DebugLog& debugLogFile = mitcad::DebugLog::instance();
  if (hasStartFile) {
    debugLogFile.setFact(QStringLiteral("Design"), QFileInfo(startFile).absoluteFilePath());
  }
  if (debugLog.on || mitcad::DebugLog::settingOn()) {
    QString error;
    if (!debugLogFile.start(debugLog.on ? debugLog.file : QString(),
                            debugLog.on ? debugLog.source : QStringLiteral("Preferences"), &error)) {
      qWarning().noquote() << "Debug log not started:" << error;
    }
  }

  // Files the system asks to open (macOS: Finder, the Dock, `open -a`) can
  // arrive from the first event loop on, while the window and its start-up
  // questions are still there: they wait for the handler set further down.
  mitcad::FileOpenFilter fileOpenFilter;
  app.installEventFilter(&fileOpenFilter);

  // A screenshot run offers no crash reports either (mitcad#62).
  mitcad::ReportCenter::setOfferEarlierCrashes(!parser.isSet(screenshotOption));
  std::unique_ptr<mitcad::MainWindow> ownedWindow;
  try {
    ownedWindow = std::make_unique<mitcad::MainWindow>(parser.isSet(demoOption), nullptr, nativeUiRequested);
  } catch (const std::exception& error) {
    qCritical().noquote() << "Could not create the application window:" << error.what();
    return 2;
  }
  mitcad::MainWindow& window = *ownedWindow;
  // Check the actual registered defaults before showing a window or creating GL.
  if (qEnvironmentVariableIntValue("MITCAD_TEST_DEFAULT_SHORTCUTS") == 1) {
    const auto* registry = window.findChild<mitcad::CommandRegistry*>();
    if (registry == nullptr) return 1;
    const auto conflicts = registry->defaultShortcutConflicts();
    for (const auto& conflict : conflicts) qCritical().noquote() << conflict.description();
    if (!conflicts.isEmpty()) return 1;
    qInfo() << "Registered default shortcuts have no conflicts";
    return 0;
  }
  if (!nativeUiRequested) {
    window.resize(1280, 800);
  }
  if (hasStartFile) {
    const QString& path = startFile;
    QString error;
    // Cancelled (error empty): the window starts with an empty document.
    if (!window.openFile(path, error) && !error.isEmpty()) {
      qCritical().noquote() << "Could not open" << path + QLatin1Char(':') << error;
      return 2;
    }
  }
  window.present();
  // The window's layout once it shows, which tools/check-all.sh compares
  // to decide whether the UI tests must run (tools/ui-layout.sh, mitcad#412).
  if (!window.usesNativeUi()) {
    mitcad::TestSync::singleShot(0, &window, [&window] { window.logLayout(); });
  }

  mitcad::mcp::LiveServer mcpServer(&window);
  if (!parser.isSet(noMcpOption)) {
    mitcad::mcp::setRunAgentFolders(parser.values(mcpFolderOption));
    QString error;
    if (!mcpServer.start(&error)) {
      qWarning().noquote() << "Local MCP connection unavailable:" << error;
      // The agent indicator's menu says why, first.
      window.setMcpUnavailable(QCoreApplication::translate("main", "The listener for assistants did not start: %1").arg(error));
    }
  } else {
    window.setMcpUnavailable(QCoreApplication::translate("main", "Assistants are switched off (MitCad was started with --no-mcp)"));
  }

  // For tests (framework/Diagnostics.hpp): the theme switched while the
  // window shows.
  const QString scheme = mitcad::testColorScheme();
  if (!scheme.isEmpty()) {
    QTimer::singleShot(0, &window, [scheme] {
      qDebug().noquote() << QStringLiteral("Asking the platform for the %1 colour scheme").arg(scheme);
      QGuiApplication::styleHints()->setColorScheme(scheme == QStringLiteral("dark") ? Qt::ColorScheme::Dark
                                                                                    : Qt::ColorScheme::Light);
    });
  }

  for (const QString& assignment : parser.values(setOption)) {
    const QString name = assignment.section(QLatin1Char('='), 0, 0);
    const std::optional<double> value = mitcad::parsePlainNumber(assignment.section(QLatin1Char('='), 1));
    if (!value || !window.setParameterValue(name, *value)) {
      qCritical().noquote() << "Invalid --set argument:" << assignment;
      return 2;
    }
  }

  // Unsaved work of sessions that did not end normally (P8c), once the
  // window shows; a screenshot run asks nothing.
  if (!parser.isSet(noRecoveryOption) && !parser.isSet(screenshotOption)) {
    QTimer::singleShot(0, &window, [&window] { window.recoverAtStart(); });
  }

  // The library sources with Auto-update are fetched in the background a
  // moment after the window shows (mitcad#576): no dialog, nothing waits
  // for it. Not in a run of a UI test script (MITCAD_TEST_INPUT) nor a
  // screenshot run, so that no test touches the network; the CLI and the
  // MCP server have no such start.
  if (!testDriver.active() && !parser.isSet(screenshotOption)) {
    QTimer::singleShot(1000, &window, [&window] { window.updateLibrariesAtStart(); });
  }

  // After the recovery question (queued above): a file the system opens
  // waits for it, like for any other dialog.
  fileOpenFilter.setHandler([&window](const QString& path) { window.openExternalFile(path); });

  if (parser.isSet(screenshotOption)) {
    const QString path = parser.value(screenshotOption);
    QTimer::singleShot(2000, &window, [&window, path] {
      const bool saved = window.viewerWidget()->grabFramebuffer().save(path);
      QCoreApplication::exit(saved ? 0 : 1);
    });
  }

  // The application quits with its window (Qt's default), also on macOS,
  // where an application usually stays running without windows: Mitcad has
  // one window and one document, and a menu bar with no document would
  // only be something to close again.
  const int status = QApplication::exec();
  // Display meshes still to be stored after an open (mitcad#880): the body
  // being written is finished, the others are dropped (the next open
  // meshes and stores them).
  mitcad::finishDisplayMeshWrites();
  return status;
}

} // namespace

int main(int argc, char* argv[]) {
  // The window, the document session and the autosave are gone here, and
  // QApplication's post routines have run (a staged restart or update was
  // started); a face of a cancelled display meshing that never ended, or
  // an import call the watchdog gave up, must not see the static
  // destructors (mitcad#936).
  return mitcad::endProcess(runApplication(argc, argv));
}
