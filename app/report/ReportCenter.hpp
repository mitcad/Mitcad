// SPDX-License-Identifier: MIT
#pragma once

// Feedback and error reports (mitcad#61, mitcad#62): Help > Send Feedback,
// and reports offered after a crash (on the next start; a worker's at once)
// or an internal error (at once). Every report goes through the same
// preview (ReportDialogs.hpp) with personal data masked
// (report::mask), and the same delivery: the issue tracker's form, filled
// in through its address and opened in the browser, so that Mitcad holds
// no token and sends nothing itself. The user submits the form there.
//
// - The tracker: Mitcad's issue list, MITCAD_ISSUE_URL at build time (CMake
//   cache variable; default the project's issue page from README.md), the
//   setting reports/issueUrl overriding it (a list's address or a template
//   with {title} and {body}, report::issueLink).
// - Crashes: CrashHandler.hpp writes `*.crash` files into crashDirectory();
//   the application offers them on its next start, and a worker's of its
//   own (MITCAD_CRASH_PARENT) as soon as it appears (the folder is
//   watched). An offered report becomes `*.crash.offered`; the newest 20
//   are kept.
// - Internal errors: a crash inside the geometry kernel that OCCT turned
//   into an error (report::isKernelCrashMessage) and an unexpected
//   exception out of a model command; offered once per duplicate key and
//   run. The setting reports/offerErrors (Don't Offer Again) turns the
//   offers off; Help > Send Feedback stays.
// - Offers wait until no dialog is open and the model computes nothing.
//
// For tests: MITCAD_TEST_LOG_URLS=1 logs the address instead of opening a
// browser (tools/ui-test-lib.sh sets it); MITCAD_TEST_CRASH=app or
// model-worker crashes the application once it shows (on the UI thread or
// on the model's worker thread), import-worker or render-worker that
// worker when it starts (main.cpp, RenderWorker.cpp).

#include <functional>

#include <QImage>
#include <QList>
#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QUrl>

#include "report/ReportText.hpp"

class QFileSystemWatcher;
class QMainWindow;
class QTimer;

namespace mitcad {

class CommandRegistry;

class ReportCenter : public QObject {
  Q_OBJECT

public:
  struct Host {
    // True while an offer must wait (a job or an import runs).
    std::function<bool()> busy;
    // Runs `call` as a job on the model's worker thread (the test crash).
    std::function<void(std::function<void()> call)> onModelWorker;
    // A short message in the status bar.
    std::function<void(const QString&)> showHint;
  };

  ReportCenter(QMainWindow& window, Host host, QObject* parent = nullptr);

  // Help > Send Feedback (help.send_feedback).
  void registerCommands(CommandRegistry& registry);
  // Once the window shows: watches the crash folder and offers the reports
  // of crashes before this run.
  void startUp();
  // Whether startUp offers earlier crashes (not for --screenshot, which
  // asks nothing); before the window is made.
  static void setOfferEarlierCrashes(bool offer);

  // A recent action for reports (a command's id, a model command's name;
  // no design content): kept in memory and by the crash handler.
  void noteAction(const QString& action);
  // The 3D view's OpenGL renderer and version, for the diagnostics.
  void setGraphics(const QString& renderer, const QString& version);

  // An error the window shows: an internal error when it looks like one
  // (report::isKernelCrashMessage), offered then.
  void errorShown(const QString& message);
  // An internal error: offered once per duplicate key and run.
  void internalError(const QString& context, const QString& message);

  // Help > Send Feedback.
  void sendFeedback();

  // Where crash reports go: MITCAD_CRASH_DIR, else "crashes" in the
  // user's local application data.
  static QString crashDirectory();
  // Before the window: the crash handler's folder, the environment that
  // tells the workers where it is and whose they are, and the Rust panic
  // hook's sink.
  static void prepareCrashReports();
  // The Rust panic hook's sink: a panic's message goes into the crash
  // report of the abort that follows it (also in the import worker).
  static void notePanics();
  // The issue tracker the reports go to (see above).
  static QString issueTracker();

private:
  struct Offer {
    QString kind;    // "crash", "worker-crash", "error"
    QString message; // what the offer says happened
    report::Report report;
    QStringList files; // crash files the offer answers
  };

  void scanCrashes(bool atStart);
  void queue(const Offer& offer);
  void offerNext();
  bool showOffer(const Offer& offer);
  report::Report crashReport(const report::CrashReport& crash, int earlier) const;
  QString diagnostics() const;
  QString recentActions() const;
  // The preview, then the delivery; true when sent.
  bool previewAndSend(const report::Report& report, const QImage& screenshot, QWidget* parent);
  void deliver(const report::Report& report, const QImage& screenshot, QWidget* parent);
  void markOffered(const QStringList& files);
  bool offersOn() const;

  QMainWindow& m_window;
  Host m_host;
  QStringList m_actions;
  QString m_renderer;
  QString m_glVersion;
  QFileSystemWatcher* m_watcher = nullptr;
  QSet<QString> m_seenFiles; // crash files already queued or ignored
  QSet<QString> m_seenKeys;  // internal errors offered this run
  QList<Offer> m_offers;
  QTimer* m_offerTimer = nullptr;
  bool m_offering = false;
};

// Opens `url` in the browser; with MITCAD_TEST_LOG_URLS=1 (UI tests) only
// logs "Open URL (test, not opened): <url>".
void openExternalUrl(const QUrl& url);

} // namespace mitcad
