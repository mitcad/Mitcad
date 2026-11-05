// SPDX-License-Identifier: MIT
#include <cstdio>
#include <cstring>

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
#include "WindowManager.hpp"
#include "DocumentFrame.hpp"
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
#include "print/MaterialProperties.hpp"
#include "report/CrashHandler.hpp"
#include "report/ReportCenter.hpp"
#include "update/OpenSslConfiguration.hpp"

namespace {

// The names Qt keys the settings on, and the UI tests' own settings. Before
// the application object exists: main() reads the frontend preference
// before it makes one (mitcad#1056).
void useStoredSettings() {
  // The stored names (Mitcad/Mitcad), not the program's name MitCad: the
  // settings and the data folders are keyed on them (mitcad#822).
  QCoreApplication::setOrganizationName(mitcad::storedApplicationName());
#ifdef Q_OS_MACOS
  // macOS names the settings file after the reversed domain
  // (fi.nocodo.Mitcad.plist, not com.Mitcad.Mitcad.plist); it should match
  // the bundle identifier fi.nocodo.mitcad (MITCAD_BUNDLE_ID in
  // app/packaging/macos). Windows and Linux key their settings on the
  // organization name, so the domain stays unset there.
  const QString kOrganizationDomain = QStringLiteral("nocodo.fi");
  QCoreApplication::setOrganizationDomain(kOrganizationDomain);
#endif
  QCoreApplication::setApplicationName(mitcad::storedApplicationName());
  // UI tests keep their settings (shortcuts, recent files, preferences)
  // apart from the user's: an INI file in a directory of their own. Linux
  // tests can use XDG_CONFIG_HOME; on Windows the settings are otherwise in
  // the registry.
  const QString settingsDir = qEnvironmentVariable("MITCAD_SETTINGS_DIR");
  if (!settingsDir.isEmpty()) {
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, settingsDir);
  }
}

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
  // A document viewport can move between top-level frames. Qt otherwise
  // destroys its QOpenGLWidget context during that move, invalidating the
  // OCCT shader/mesh resources retained by the same document viewer.
  // Global sharing keeps that exact context alive across reparenting.
  QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
  // The global share context and every viewport/compositor must use the
  // same existing platform format, selected before QApplication creates it.
  QSurfaceFormat::setDefaultFormat(mitcad::viewportSurfaceFormat());

  useStoredSettings();
  // The frontend (mitcad#1056): Qt by default, also in a build with the
  // AppKit frontend; Settings › General's native macOS interface chooses
  // AppKit for the next starts, and --ui=native|qt for one run. --chrome
  // chooses a Qt layout, so Qt too. The start-up's Frontend: line says
  // which decided.
  QString frontendSource = QStringLiteral("default");
#ifdef MITCAD_MACOS_NATIVE_UI
  bool nativeUiRequested = mitcad::nativeUiPreference();
  if (nativeUiRequested) {
    frontendSource = QStringLiteral("preference");
  }
  // Keep Qt's application delegate and activation handling while the shell
  // owns the menu bar. Plugin mode would suppress that delegate entirely.
  // The full parser below validates these options.
  bool uiGiven = false;
  bool chromeGiven = false;
  for (int index = 1; index < argc; ++index) {
    const QString argument = QString::fromLocal8Bit(argv[index]);
    if (argument.startsWith(QStringLiteral("--ui="))) {
      uiGiven = true;
      nativeUiRequested = argument.mid(5) == QStringLiteral("native");
    } else if (argument == QStringLiteral("--ui") && index + 1 < argc) {
      uiGiven = true;
      nativeUiRequested = QString::fromLocal8Bit(argv[++index]) == QStringLiteral("native");
    } else if (argument.startsWith(QStringLiteral("--chrome"))) {
      chromeGiven = true;
    }
  }
  if (!uiGiven && chromeGiven && nativeUiRequested) {
    nativeUiRequested = false;
    frontendSource = QStringLiteral("--chrome");
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
  // What decides whether a shortcut takes a key (MITCAD_LOG_SHORTCUTS,
  // mitcad#1016).
  mitcad::installShortcutLogIfRequested();
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
  // Where crash reports go, also the workers' (mitcad#62).
  mitcad::ReportCenter::prepareCrashReports();
  // The preferred temperature unit, which every temperature of materials
  // follows (mitcad#1213).
  mitcad::applyTemperaturePreference();

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
  const QCommandLineOption uiOption(
      QStringLiteral("ui"),
      QStringLiteral("User interface for this run: qt (the default) or native (AppKit, when built; Settings > "
                     "General can choose it for every start)."),
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
  // --version and the parser's errors print the application name: the
  // program's name MitCad there ("MitCad 1.2.3", mitcad#1043), the stored
  // name again for everything after (the settings and folders).
  QApplication::setApplicationName(mitcad::programName());
  parser.process(arguments);
  QApplication::setApplicationName(mitcad::storedApplicationName());
  // The AppImage's TLS (mitcad#1208): the system's OpenSSL configuration,
  // or the AppImage's own where the bundled OpenSSL cannot load it; before
  // anything uses TLS, and after --version and --help, which print only
  // themselves.
  mitcad::checkOpenSslConfiguration();

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
    frontendSource = QStringLiteral("--ui");
    if (nativeUiRequested && parser.isSet(chromeOption)) {
      qCritical() << "--chrome selects a Qt layout; use --ui=qt with it";
      return 2;
    }
  }
  qInfo().noquote() << QStringLiteral("Frontend: %1 (%2)")
                           .arg(nativeUiRequested ? QStringLiteral("native") : QStringLiteral("qt"), frontendSource);

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
  mitcad::WindowManager manager(nativeUiRequested);
  mitcad::DocumentFrame::install(manager);
  mitcad::MainWindow* ownedWindow = nullptr;
  try {
    ownedWindow = manager.createWindow(parser.isSet(demoOption));
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
    // The sketch constraints' keys (mitcad#1229), in sketches only; Add
    // Comment keeps Shift+N.
    static const char* const expected[][2] = {
        {"sketch.constraint.horizontal_vertical", "Shift+V"}, {"sketch.constraint.coincident", "Shift+C"},
        {"sketch.constraint.tangent", "Shift+T"},             {"sketch.constraint.equal", "Shift+E"},
        {"sketch.constraint.parallel", "Shift+P"},            {"sketch.constraint.perpendicular", "Shift+R"},
        {"sketch.constraint.fix", "Shift+F"},                 {"sketch.constraint.midpoint", "Shift+M"},
        {"sketch.constraint.concentric", "Shift+O"},          {"sketch.constraint.collinear", "Shift+L"},
        {"sketch.constraint.symmetric", "Shift+S"},           {"sketch.constraint.smooth", "Shift+G"},
        {"comment.add", "Shift+N"}};
    bool expectedKeys = true;
    for (const auto& entry : expected) {
      const QString id = QString::fromLatin1(entry[0]);
      const QString key = registry->defaultShortcut(id).toString(QKeySequence::PortableText);
      const QString wanted = QString::fromLatin1(entry[1]);
      if (key != wanted) {
        qCritical().noquote() << QStringLiteral("%1 has the default key '%2', expected %3").arg(id, key, wanted);
        expectedKeys = false;
      }
      if (id.startsWith(QLatin1String("sketch.constraint.")) &&
          registry->shortcutContexts(id) != unsigned(mitcad::SketchShortcutContext)) {
        qCritical().noquote() << QStringLiteral("%1 has a key outside sketches").arg(id);
        expectedKeys = false;
      }
    }
    if (!expectedKeys) return 1;
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
  // The Start tab at launch (mitcad#1258): by Preferences' choice (the
  // page's Show start page at launch, on by default), unless the start
  // opens a file, the demo block or a screenshot, sets parameters, or is a
  // test's: MITCAD_START_PAGE=0 (the UI test libraries set it), or a UI
  // test script's run unless MITCAD_START_PAGE=1.
  {
    const QByteArray startPage = qgetenv("MITCAD_START_PAGE");
    QString skip;
    if (hasStartFile) {
      skip = QStringLiteral("a file opens");
    } else if (parser.isSet(demoOption)) {
      skip = QStringLiteral("--demo");
    } else if (parser.isSet(screenshotOption)) {
      skip = QStringLiteral("--screenshot");
    } else if (!parser.values(setOption).isEmpty()) {
      skip = QStringLiteral("--set");
    } else if (startPage == "0") {
      skip = QStringLiteral("MITCAD_START_PAGE=0");
    } else if (startPage != "1" && testDriver.active()) {
      skip = QStringLiteral("a UI test script");
    }
    window.showStartPageAtLaunch(skip);
  }
  manager.present(&window);
  // The window's layout once it shows, which tools/check-all.sh compares
  // to decide whether the UI tests must run (tools/ui-layout.sh, mitcad#412).
  if (!window.usesNativeUi()) {
    mitcad::TestSync::singleShot(0, &window, [&window] { window.logLayout(); });
  }

  mitcad::mcp::LiveServer mcpServer(&manager);
  if (!parser.isSet(noMcpOption)) {
    mitcad::mcp::setRunAgentFolders(parser.values(mcpFolderOption));
    QString error;
    if (!mcpServer.start(&error)) {
      qWarning().noquote() << "Local MCP connection unavailable:" << error;
      // The agent indicator's menu says why, first.
      window.setMcpUnavailable(QCoreApplication::translate("main", "The listener for assistants did not start: %1").arg(error));
    }
  } else {
    const QString reason = QCoreApplication::translate("main", "Assistants are switched off (MitCad was started with --no-mcp)");
    window.setMcpUnavailable(reason);
    QObject::connect(&manager, &mitcad::WindowManager::windowAdded, &manager,
                     [reason](mitcad::MainWindow* document) { document->setMcpUnavailable(reason); });
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
    QTimer::singleShot(1000, &manager, [&manager] {
      if (auto* current = manager.activeWindow()) current->updateLibrariesAtStart();
    });
  }

  // After the recovery question (queued above): a file the system opens
  // waits for it, like for any other dialog.
  fileOpenFilter.setHandler([&manager](const QString& path) {
    if (auto* current = manager.activeWindow()) current->openExternalFile(path);
  });

  if (parser.isSet(screenshotOption)) {
    const QString path = parser.value(screenshotOption);
    QTimer::singleShot(2000, &window, [&window, path] {
      const bool saved = window.viewerWidget()->grabFramebuffer().save(path);
      QCoreApplication::exit(saved ? 0 : 1);
    });
  }

  // Each window owns one document. Closing the last document ends this
  // process, also on macOS; Quit applies every window's save/lock guard.
  const int status = QApplication::exec();
  // Display meshes still to be stored after an open (mitcad#880): the body
  // being written is finished, the others are dropped (the next open
  // meshes and stores them).
  mitcad::finishDisplayMeshWrites();
  return status;
}

} // namespace

int main(int argc, char* argv[]) {
  // --version answers before any window, display or log is set up (a run
  // without a display, as in the release's build root, and its output
  // reaches a pipe even if a later teardown would crash).
  for (int index = 1; index < argc; ++index) {
    if (std::strcmp(argv[index], "--version") == 0 || std::strcmp(argv[index], "-v") == 0) {
      std::printf("MitCad %s\n", MITCAD_VERSION);
      std::fflush(stdout);
      return 0;
    }
  }
  // The window, the document session and the autosave are gone here, and
  // QApplication's post routines have run (a staged restart or update was
  // started); a face of a cancelled display meshing that never ended, or
  // an import call the watchdog gave up, must not see the static
  // destructors (mitcad#936).
  return mitcad::endProcess(runApplication(argc, argv));
}
