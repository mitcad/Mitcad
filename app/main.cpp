// SPDX-License-Identifier: MIT
#include <QApplication>
#include <QCommandLineParser>
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

#include "MainWindow.hpp"
#include "OcctViewer.hpp"
#include "files/F3dImport.hpp"
#include "files/FileOpenFilter.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/DesktopEntry.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/Numbers.hpp"
#include "framework/TestDriver.hpp"
#include "framework/TestSync.hpp"
#include "mitcad/geometry/guard.hpp"
#include "report/CrashHandler.hpp"
#include "report/ReportCenter.hpp"

int main(int argc, char* argv[]) {
  // Crash reports (mitcad#62), in the application and its import worker.
  const bool importWorker = mitcad::isImportWorker(argc, argv);
  mitcad::crash::install(importWorker ? "import-worker" : "app", MITCAD_VERSION);
  // An .f3d, .FCStd or .ipt import in a process of its own, without
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

  QApplication app(argc, argv);
  // UI tests' sync key (MITCAD_TEST_SYNC), before the window's timers exist.
  mitcad::TestSync::installIfRequested();
  QApplication::setOrganizationName(QStringLiteral("Mitcad"));
#ifdef Q_OS_MACOS
  // macOS names the settings file after the reversed domain
  // (fi.nocodo.Mitcad.plist, not com.Mitcad.Mitcad.plist); it should match
  // the bundle identifier fi.nocodo.mitcad (MITCAD_BUNDLE_ID in
  // app/packaging/macos). Windows and Linux key their settings on the
  // organization name, so the domain stays unset there.
  const QString kOrganizationDomain = QStringLiteral("nocodo.fi");
  QApplication::setOrganizationDomain(kOrganizationDomain);
#endif
  QApplication::setApplicationName(QStringLiteral("Mitcad"));
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

  QCommandLineParser parser;
  parser.addHelpOption();
  parser.addVersionOption();
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
      QStringLiteral("Open <file> at start-up: a project (.mitcad), an .f3d or .f3z design, "
                     "a FreeCAD document (.FCStd), an .ipt part, STEP, IGES, BRep, STL, OBJ or DXF as a "
                     "new document."),
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
  parser.addOption(screenshotOption);
  parser.addOption(setOption);
  parser.addOption(demoOption);
  parser.addOption(openOption);
  parser.addOption(qtDialogsOption);
  parser.addOption(noRecoveryOption);
  parser.addOption(chromeOption);
  parser.addPositionalArgument(
      QStringLiteral("file"),
      QStringLiteral("Open <file> at start-up, like --open (as a file association passes it)."),
      QStringLiteral("[file]"));
  parser.process(app);

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
  }
  if (parser.isSet(noGlassOption)) {
    mitcad::setGlassRequested(false);
  }
  qInfo().noquote() << "Style:" << QApplication::style()->name() << "Chrome:"
                    << (mitcad::chromeStyle() == mitcad::ChromeStyle::Floating ? "floating" : "docked");
  qInfo().noquote() << "Glass:" << (mitcad::glassActive() ? "on" : "off");

  if (parser.isSet(qtDialogsOption)) {
    QApplication::setAttribute(Qt::AA_DontUseNativeDialogs);
  }

  // The file to open: --open or the positional argument, not both.
  const QStringList positional = parser.positionalArguments();
  if (positional.size() > 1 || (parser.isSet(openOption) && !positional.isEmpty())) {
    qCritical().noquote() << "Give one file only: either --open <file> or a file argument.";
    return 2;
  }
  const bool hasStartFile = parser.isSet(openOption) || !positional.isEmpty();
  const QString startFile = parser.isSet(openOption) ? parser.value(openOption)
                                                     : (positional.isEmpty() ? QString() : positional.first());

  // Files the system asks to open (macOS: Finder, the Dock, `open -a`) can
  // arrive from the first event loop on, while the window and its start-up
  // questions are still there: they wait for the handler set further down.
  mitcad::FileOpenFilter fileOpenFilter;
  app.installEventFilter(&fileOpenFilter);

  // A screenshot run offers no crash reports either (mitcad#62).
  mitcad::ReportCenter::setOfferEarlierCrashes(!parser.isSet(screenshotOption));
  mitcad::MainWindow window(parser.isSet(demoOption));
  window.resize(1280, 800);
  if (hasStartFile) {
    const QString& path = startFile;
    QString error;
    // Cancelled (error empty): the window starts with an empty document.
    if (!window.openFile(path, error) && !error.isEmpty()) {
      qCritical().noquote() << "Could not open" << path + QLatin1Char(':') << error;
      return 2;
    }
  }
  window.show();

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
  return QApplication::exec();
}
