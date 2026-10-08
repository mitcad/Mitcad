// SPDX-License-Identifier: MIT
#include "ReportCenter.hpp"

#ifdef _WIN32
#include <stdlib.h>
#endif

#include <algorithm>
#include <cstddef>
#include <utility>

#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QFileSystemWatcher>
#include <QLocale>
#include <QMainWindow>
#include <QMessageBox>
#include <QPushButton>
#include <QScreen>
#include <QSettings>
#include <QStandardPaths>
#include <QSysInfo>
#include <QTime>
#include <QTimer>
#include <QtLogging>

#include <Standard_Version.hxx>

#include "framework/ChromeStyle.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/TestSync.hpp"
#include "report/CrashHandler.hpp"
#include "report/ReportDialogs.hpp"
#ifdef MITCAD_RENDER
#include "render/RenderDevice.hpp"
#endif

// The Rust panic hook's sink (core/ffi/src/panic_note.rs).
extern "C" void mitcad_set_panic_sink(void (*sink)(const char* message, std::size_t length));

namespace mitcad {
namespace {

// Offered reports kept in the crash folder.
constexpr int kKeptReports = 20;
// Recent actions kept for reports.
constexpr int kActions = 32;
// Whether the start offers the crashes of earlier runs.
bool g_offerEarlier = true;

void notePanic(const char* message, std::size_t length) { crash::noteMessage(message, length); }

QString processLabel(const QString& process) {
  if (process == QStringLiteral("import-worker")) {
    return ReportCenter::tr("import process");
  }
  if (process == QStringLiteral("render-worker")) {
    return ReportCenter::tr("render process");
  }
  return ReportCenter::tr("application");
}

// "mitcad::MainWindow::save" of a frame's "mitcad::MainWindow::save() const".
QString shortSymbol(const QString& symbol) {
  QString name = symbol.section(QLatin1Char('('), 0, 0);
  return name.size() > 80 ? name.left(77) + QStringLiteral("...") : name;
}

QString oneLine(const QString& text) { return QString(text).replace(QLatin1Char('\n'), QStringLiteral(" | ")); }

} // namespace

void openExternalUrl(const QUrl& url) {
  if (qEnvironmentVariableIntValue("MITCAD_TEST_LOG_URLS") != 0) {
    qInfo().noquote() << QStringLiteral("Open URL (test, not opened): %1").arg(QString::fromLatin1(url.toEncoded()));
    return;
  }
  QDesktopServices::openUrl(url);
}

ReportCenter::ReportCenter(QMainWindow& window, Host host, QObject* parent)
    : QObject(parent), m_window(window), m_host(std::move(host)) {}

QString ReportCenter::crashDirectory() {
  const QString set = qEnvironmentVariable("MITCAD_CRASH_DIR");
  if (!set.isEmpty()) {
    return set;
  }
  return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) + QStringLiteral("/crashes");
}

void ReportCenter::prepareCrashReports() {
  const QString directory = crashDirectory();
  QDir().mkpath(directory);
#ifdef _WIN32
  crash::setDirectory(QDir::toNativeSeparators(directory).toUtf8().constData());
  // The workers inherit the folder and whose they are (wide, for folders
  // outside the code page).
  _wputenv_s(L"MITCAD_CRASH_DIR", reinterpret_cast<const wchar_t*>(QDir::toNativeSeparators(directory).utf16()));
#else
  crash::setDirectory(QFile::encodeName(directory).constData());
  qputenv("MITCAD_CRASH_DIR", QFile::encodeName(directory));
#endif
  qputenv("MITCAD_CRASH_PARENT", QByteArray::number(QCoreApplication::applicationPid()));
  notePanics();
}

void ReportCenter::notePanics() { mitcad_set_panic_sink(&notePanic); }

QString ReportCenter::issueTracker() {
  const QString set = QSettings().value(QStringLiteral("reports/issueUrl")).toString().trimmed();
  return set.isEmpty() ? QStringLiteral(MITCAD_ISSUE_URL) : set;
}

void ReportCenter::registerCommands(CommandRegistry& registry) {
  CommandDef feedback;
  feedback.id = QStringLiteral("help.send_feedback");
  feedback.name = tr("Send Feedback...");
  feedback.icon = QStringLiteral("feedback");
  feedback.kind = CommandDef::Kind::Action;
  feedback.mode = CommandDef::Mode::Any;
  feedback.tab.clear();
  feedback.tooltip = tr("Report a problem or suggest an improvement; you see and can edit the report before "
                        "it is sent");
  feedback.keywords = {QStringLiteral("bug"), QStringLiteral("report"), QStringLiteral("issue"),
                       QStringLiteral("wish"), QStringLiteral("problem"), QStringLiteral("feedback")};
  feedback.duringCommands = true;
  feedback.run = [this] { sendFeedback(); };
  registry.add(feedback);
}

void ReportCenter::setOfferEarlierCrashes(bool offer) { g_offerEarlier = offer; }

void ReportCenter::startUp() {
  const QString directory = crashDirectory();
  m_watcher = new QFileSystemWatcher(this);
  if (QDir(directory).exists()) {
    m_watcher->addPath(directory);
  }
  connect(m_watcher, &QFileSystemWatcher::directoryChanged, this, [this] { scanCrashes(false); });
  m_offerTimer = new QTimer(this);
  m_offerTimer->setInterval(250);
  connect(m_offerTimer, &QTimer::timeout, this, &ReportCenter::offerNext);
  QTimer::singleShot(0, this, [this] { scanCrashes(g_offerEarlier); });

  // Test crashes (MITCAD_TEST_CRASH), once the window shows.
  if (crash::testCrashRequested("app")) {
    QTimer::singleShot(500, this, [] {
      qInfo().noquote() << QStringLiteral("Crashing for a test (MITCAD_TEST_CRASH=app)");
      crash::crashNow();
    });
  } else if (crash::testCrashRequested("model-worker")) {
    QTimer::singleShot(500, this, [this] {
      qInfo().noquote() << QStringLiteral("Crashing the model's worker thread for a test (MITCAD_TEST_CRASH=model-worker)");
      m_host.onModelWorker([] { crash::crashNow(); });
    });
  }
}

void ReportCenter::noteAction(const QString& action) {
  m_actions << QTime::currentTime().toString(QStringLiteral("HH:mm:ss")) + QLatin1Char(' ') + action;
  while (m_actions.size() > kActions) {
    m_actions.removeFirst();
  }
  crash::noteAction(action.toUtf8().constData());
}

void ReportCenter::setGraphics(const QString& renderer, const QString& version) {
  m_renderer = renderer;
  m_glVersion = version;
}

QString ReportCenter::recentActions() const { return m_actions.join(QLatin1Char('\n')); }

QString ReportCenter::diagnostics() const {
  QStringList lines;
  lines << QStringLiteral("Mitcad %1").arg(QApplication::applicationVersion());
  lines << QStringLiteral("System: %1 (%2 %3, %4)")
               .arg(QSysInfo::prettyProductName(), QSysInfo::kernelType(), QSysInfo::kernelVersion(),
                    QSysInfo::currentCpuArchitecture());
  lines << QStringLiteral("Qt %1, Open CASCADE Technology %2").arg(QString::fromLatin1(qVersion()),
                                                                   QString::fromLatin1(OCC_VERSION_COMPLETE));
  lines << QStringLiteral("Graphics: %1%2")
               .arg(m_renderer.isEmpty() ? QStringLiteral("not known yet") : m_renderer,
                    m_glVersion.isEmpty() ? QString() : QStringLiteral(", OpenGL ") + m_glVersion);
#ifdef MITCAD_RENDER
  lines << QStringLiteral("Rendered view: render device setting %1").arg(render::renderDeviceChoice());
#else
  lines << QStringLiteral("Rendered view: not in this build");
#endif
  const QScreen* screen = m_window.screen();
  lines << QStringLiteral("Window: %1 layout, %2 x %3, device pixel ratio %4")
               .arg(chromeStyle() == ChromeStyle::Floating ? QStringLiteral("floating") : QStringLiteral("docked"))
               .arg(m_window.width())
               .arg(m_window.height())
               .arg(screen != nullptr ? screen->devicePixelRatio() : 1.0);
  lines << QStringLiteral("Locale: %1").arg(QLocale::system().name());
  return lines.join(QLatin1Char('\n'));
}

// ---------------------------------------------------------------------------
// Help > Send Feedback

void ReportCenter::sendFeedback() {
  // The window as it is, before the form covers it.
  const QImage screenshot = m_window.grab().toImage();
  report::FeedbackDialog* form = nullptr;
  const auto preview = [this, &form](const report::Feedback& feedback) {
    report::Report report;
    const QString summary = feedback.summary.isEmpty() ? tr("Feedback") : feedback.summary;
    report.title = feedback.kind == QStringLiteral("bug")    ? QStringLiteral("Bug: ") + summary
                   : feedback.kind == QStringLiteral("wish") ? QStringLiteral("Wish: ") + summary
                                                             : summary;
    report.sections << report::Section{QStringLiteral("description"), QStringLiteral("Description"),
                                       feedback.description, true, false, true};
    if (!feedback.contact.isEmpty()) {
      report.sections << report::Section{QStringLiteral("contact"), QStringLiteral("Contact"), feedback.contact,
                                         true, false, false};
    }
    if (feedback.kind == QStringLiteral("bug") && !m_actions.isEmpty()) {
      report.sections << report::Section{QStringLiteral("actions"), QStringLiteral("Recent actions"),
                                         recentActions(), true, true, true};
    }
    report.sections << report::Section{QStringLiteral("diagnostics"), QStringLiteral("Diagnostics"), diagnostics(),
                                       feedback.diagnostics, true, true};
    return previewAndSend(report, feedback.screenshot, form);
  };
  report::FeedbackDialog dialog(screenshot, preview, &m_window);
  form = &dialog;
  prepareModal(&dialog);
  qInfo().noquote() << QStringLiteral("Send Feedback: the form shows");
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Send Feedback cancelled");
  }
}

// ---------------------------------------------------------------------------
// The preview and the delivery

bool ReportCenter::previewAndSend(const report::Report& report, const QImage& screenshot, QWidget* parent) {
  // Masked before anyone sees it.
  const report::MaskContext context = report::currentMaskContext();
  report::Report masked = report;
  masked.title = report::mask(report.title, context);
  for (report::Section& section : masked.sections) {
    if (section.masked) {
      section.text = report::mask(section.text, context);
    }
  }
  const QString tracker = issueTracker();
  const QUrl trackerUrl(tracker);
  const QString destination = trackerUrl.host().isEmpty() ? tracker : trackerUrl.host();
  report::ReportPreviewDialog dialog(masked, screenshot, destination, parent);
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Report not sent: the preview was cancelled");
    return false;
  }
  deliver(dialog.report(), dialog.screenshot(), parent);
  return true;
}

void ReportCenter::deliver(const report::Report& report, const QImage& screenshot, QWidget* parent) {
  const QString body = report::reportBody(report, QApplication::applicationVersion());
  const report::IssueLink link = report::issueLink(issueTracker(), report.title, body);
  QStringList included;
  for (const report::Section& section : report.sections) {
    if (section.included && !section.text.trimmed().isEmpty()) {
      included << section.id;
    }
  }
  qInfo().noquote() << QStringLiteral("Report sent: '%1', sections %2; link %3 characters")
                           .arg(report.title, included.join(QStringLiteral(", ")))
                           .arg(link.url.toEncoded().size());
  QStringList notes;
  if (link.shortened) {
    QGuiApplication::clipboard()->setText(link.clipboard);
    qInfo().noquote() << QStringLiteral("Report on the clipboard: %1 characters (too long for the link)")
                             .arg(link.clipboard.size());
    notes << tr("The report was too long for the form's link: the whole report is on the clipboard. Paste it into "
                "the form's description in place of the shortened text.");
  }
  if (!screenshot.isNull()) {
    const QString folder =
        QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) + QStringLiteral("/reports");
    QDir().mkpath(folder);
    const QString path =
        folder + QStringLiteral("/screenshot-%1.png")
                     .arg(QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd-HHmmss")));
    if (screenshot.save(path)) {
      qInfo().noquote() << QStringLiteral("Report screenshot saved: %1 %2x%3")
                               .arg(path)
                               .arg(screenshot.width())
                               .arg(screenshot.height());
      notes << tr("The screenshot is saved as %1: drag it into the form to attach it.")
                   .arg(QDir::toNativeSeparators(path));
    }
  }
  openExternalUrl(link.url);
  if (notes.isEmpty()) {
    m_host.showHint(tr("The report's issue form is open in your browser: submit it there to send it."));
    return;
  }
  notes.prepend(tr("The report's issue form is open in your browser: submit it there to send it."));
  sheetInformation(parent != nullptr ? parent : &m_window, tr("Report"), notes.join(QStringLiteral("\n\n")));
}

// ---------------------------------------------------------------------------
// Crashes and internal errors

void ReportCenter::scanCrashes(bool atStart) {
  const QString directory = crashDirectory();
  if (m_watcher != nullptr && m_watcher->directories().isEmpty() && QDir(directory).exists()) {
    m_watcher->addPath(directory);
  }
  const QFileInfoList files =
      QDir(directory).entryInfoList({QStringLiteral("*.crash")}, QDir::Files, QDir::Time); // newest first
  const qint64 self = QCoreApplication::applicationPid();
  QList<report::CrashReport> earlier;
  for (const QFileInfo& info : files) {
    const QString path = info.absoluteFilePath();
    if (m_seenFiles.contains(path)) {
      continue;
    }
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
      continue;
    }
    report::CrashReport crash = report::parseCrashReport(file.readAll());
    crash.file = path;
    if (!crash.valid) {
      m_seenFiles.insert(path);
      qWarning().noquote() << QStringLiteral("Crash report %1 is unreadable").arg(info.fileName());
      markOffered({path});
      continue;
    }
    if (crash.parent == self && crash.process != QStringLiteral("app")) {
      // A worker of this application: now.
      m_seenFiles.insert(path);
      qInfo().noquote() << QStringLiteral("Crash report of this application's %1: %2 %3")
                               .arg(crash.process, crash.signal, info.fileName());
      Offer offer;
      offer.kind = QStringLiteral("worker-crash");
      offer.message = tr("The %1 quit unexpectedly (%2).").arg(processLabel(crash.process), crash.signal);
      offer.report = crashReport(crash, 0);
      offer.files = {path};
      queue(offer);
      continue;
    }
    if (atStart) {
      // A crash before this run (another run's worker too).
      m_seenFiles.insert(path);
      earlier << crash;
    }
    // Otherwise another Mitcad's, running now: its own, or the next start's.
  }
  if (earlier.isEmpty()) {
    return;
  }
  const report::CrashReport& newest = earlier.first();
  qInfo().noquote() << QStringLiteral("Crash reports of earlier runs: %1, the newest %2 of the %3")
                           .arg(earlier.size())
                           .arg(newest.signal, newest.process);
  Offer offer;
  offer.kind = QStringLiteral("crash");
  offer.message = newest.process == QStringLiteral("app")
                      ? tr("Mitcad quit unexpectedly the last time it ran (%1).").arg(newest.signal)
                      : tr("Mitcad's %1 quit unexpectedly when it last ran (%2).")
                            .arg(processLabel(newest.process), newest.signal);
  if (earlier.size() > 1) {
    offer.message += QLatin1Char(' ') + tr("%n more crash(es) before it are not offered.", nullptr,
                                           static_cast<int>(earlier.size() - 1));
  }
  offer.report = crashReport(newest, static_cast<int>(earlier.size() - 1));
  for (const report::CrashReport& crash : earlier) {
    offer.files << crash.file;
  }
  queue(offer);
}

report::Report ReportCenter::crashReport(const report::CrashReport& crash, int earlier) const {
  report::Report report;
  report.key = report::crashKey(crash.signal, crash.stack);
  QStringList frames;
  for (const QString& line : report::trimmedStack(crash.stack)) {
    frames << report::describeFrame(report::parseFrame(line));
  }
  // Where it crashed: the top frame's function, else its module.
  const QStringList top = report::normalizedFrames(crash.stack, 1);
  QString where;
  if (!top.isEmpty()) {
    const QString function = top.first().section(QLatin1Char('!'), 1);
    where = QStringLiteral(" in ") +
            (function.startsWith(QLatin1Char('+')) ? top.first().section(QLatin1Char('!'), 0, 0) : shortSymbol(function));
  }
  report.title = crash.process == QStringLiteral("app")
                     ? QStringLiteral("Crash: %1%2 [mitcad-%3]").arg(crash.signal, where, report.key)
                     : QStringLiteral("Crash of the %1: %2%3 [mitcad-%4]")
                           .arg(processLabel(crash.process), crash.signal, where, report.key);
  QStringList what;
  what << QStringLiteral("Mitcad %1 (%2, %3) crashed: %4 at address %5.")
              .arg(crash.version, processLabel(crash.process), crash.platform, crash.signal, crash.address);
  what << QStringLiteral("Time: %1").arg(QDateTime::fromSecsSinceEpoch(crash.time).toUTC().toString(Qt::ISODate));
  if (!crash.message.isEmpty()) {
    what << crash.message;
  }
  if (earlier > 0) {
    what << QStringLiteral("(%1 earlier crash(es) not reported)").arg(earlier);
  }
  report.sections << report::Section{QStringLiteral("description"), QStringLiteral("What I was doing"), QString(),
                                     true, false, true};
  report.sections << report::Section{QStringLiteral("error"), QStringLiteral("What happened"),
                                     what.join(QLatin1Char('\n')), true, false, true};
  report.sections << report::Section{QStringLiteral("stack"), QStringLiteral("Stack"), frames.join(QLatin1Char('\n')),
                                     true, true, true};
  QStringList actions = crash.actions;
  if (crash.process != QStringLiteral("app") && !m_actions.isEmpty()) {
    // A worker's application knows what led to it.
    actions << QStringLiteral("(the application:)") << m_actions;
  }
  report.sections << report::Section{QStringLiteral("actions"), QStringLiteral("Recent actions"),
                                     actions.join(QLatin1Char('\n')), true, true, true};
  report.sections << report::Section{QStringLiteral("diagnostics"), QStringLiteral("Diagnostics"), diagnostics(),
                                     true, true, true};
  return report;
}

void ReportCenter::errorShown(const QString& message) {
  if (report::isKernelCrashMessage(message)) {
    internalError(QStringLiteral("geometry kernel"), message);
  }
}

void ReportCenter::internalError(const QString& context, const QString& message) {
  // Once per run, also when another path reports the same message.
  const QString seen = report::errorKey(QString(), message);
  if (m_seenKeys.contains(seen)) {
    return;
  }
  m_seenKeys.insert(seen);
  const QString key = report::errorKey(context, message);
  const QString masked = report::mask(message, report::currentMaskContext());
  qInfo().noquote() << QStringLiteral("Internal error (%1): %2; key mitcad-%3").arg(context, oneLine(masked), key);
  Offer offer;
  offer.kind = QStringLiteral("error");
  offer.message = tr("An internal error occurred:\n%1").arg(masked);
  report::Report& report = offer.report;
  report.key = key;
  const QString first = message.section(QLatin1Char('\n'), 0, 0).simplified();
  report.title = QStringLiteral("Internal error: %1 [mitcad-%2]")
                     .arg(first.size() > 90 ? first.left(87) + QStringLiteral("...") : first, key);
  report.sections << report::Section{QStringLiteral("description"), QStringLiteral("What I was doing"), QString(),
                                     true, false, true};
  report.sections << report::Section{QStringLiteral("error"), QStringLiteral("What happened"),
                                     QStringLiteral("Internal error in the %1:\n%2").arg(context, message), true,
                                     false, true};
  report.sections << report::Section{QStringLiteral("actions"), QStringLiteral("Recent actions"), recentActions(),
                                     true, true, true};
  report.sections << report::Section{QStringLiteral("diagnostics"), QStringLiteral("Diagnostics"), diagnostics(),
                                     true, true, true};
  queue(offer);
}

bool ReportCenter::offersOn() const {
  return QSettings().value(QStringLiteral("reports/offerErrors"), true).toBool();
}

void ReportCenter::queue(const Offer& offer) {
  if (!offersOn()) {
    qInfo().noquote() << QStringLiteral("Error report not offered (turned off): %1").arg(oneLine(offer.message));
    markOffered(offer.files);
    return;
  }
  m_offers << offer;
  if (m_offerTimer != nullptr && !m_offerTimer->isActive()) {
    m_offerTimer->start();
  }
}

void ReportCenter::offerNext() {
  if (m_offering) {
    return;
  }
  if (m_offers.isEmpty()) {
    m_offerTimer->stop();
    return;
  }
  // Not over another dialog (a recovery question, an import's error), and
  // not while the model computes.
  if (QApplication::activeModalWidget() != nullptr || QApplication::activePopupWidget() != nullptr ||
      !m_window.isVisible() || (m_host.busy && m_host.busy())) {
    return;
  }
  m_offering = true;
  const Offer offer = m_offers.takeFirst();
  showOffer(offer);
  markOffered(offer.files);
  m_offering = false;
}

bool ReportCenter::showOffer(const Offer& offer) {
  QMessageBox box(&m_window);
  box.setObjectName(QStringLiteral("errorReportOffer"));
  box.setIcon(QMessageBox::Warning);
  box.setWindowTitle(tr("Error Report"));
  box.setText(offer.message);
  box.setInformativeText(tr("You can send a report to Mitcad's developers. You will see everything it contains, "
                            "and can change it, before anything is sent."));
  QPushButton* review = box.addButton(tr("&Review Report..."), QMessageBox::AcceptRole);
  box.addButton(tr("&Don't Send"), QMessageBox::RejectRole);
  box.setDefaultButton(review);
  auto* never = new QCheckBox(tr("Do &not offer error reports again"), &box);
  box.setCheckBox(never);
  prepareModal(&box);
  TestSync::singleShot(100, &box, [never] {
    report::logPlace(QStringLiteral("Error report no more offers"), never, never->rect());
  });
  qInfo().noquote() << QStringLiteral("Error report offered (%1): %2; key mitcad-%3")
                           .arg(offer.kind, oneLine(offer.message), offer.report.key);
  box.exec();
  if (never->isChecked()) {
    QSettings().setValue(QStringLiteral("reports/offerErrors"), false);
    qInfo().noquote() << QStringLiteral("Error reports: no more offers");
  }
  if (box.clickedButton() != review) {
    qInfo().noquote() << QStringLiteral("Error report declined");
    return false;
  }
  return previewAndSend(offer.report, QImage(), &m_window);
}

void ReportCenter::markOffered(const QStringList& files) {
  for (const QString& file : files) {
    const QString offered = file + QStringLiteral(".offered");
    QFile::remove(offered);
    QFile::rename(file, offered);
  }
  if (files.isEmpty()) {
    return;
  }
  // The newest offered reports stay, for a look at them later.
  const QFileInfoList old =
      QDir(crashDirectory()).entryInfoList({QStringLiteral("*.offered")}, QDir::Files, QDir::Time);
  for (qsizetype i = kKeptReports; i < old.size(); ++i) {
    QFile::remove(old[i].absoluteFilePath());
  }
}

} // namespace mitcad
