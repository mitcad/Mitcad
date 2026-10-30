// SPDX-License-Identifier: MIT
#pragma once

#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

class QMessageBox;
class QTimer;
class QWidget;

namespace mitcad {

class ImportProgress;

// An .f3d design (.f3d, .f3z) imported with its history (U6, the model's
// import_f3d), a FreeCAD document (.FCStd) with its stored bodies
// (import_fcstd), an .ipt part's stored bodies (import_ipt, mitcad#60), or
// an IFC building model (import_ifc, mitcad#273),
// an .rvt project's elements by level (import_rvt, mitcad#274),
// a SolidCraft design with its history (import_solidcraft, mitcad#450), or
// a .dwg drawing's 3D content (import_dwg, mitcad#453),
// in a process of its own: the application runs itself with
// --import-worker, and a crash of the geometry kernel cannot take the
// window with it. The import runs to its end; a progress dialog shows the
// timeline item being replayed and offers two ways to end it early (T1e):
// Stop and Keep What Is Imported asks the worker to stop (the remaining
// items take the file's bodies, and the report says where it stopped; not
// for a FreeCAD document or a part file, which have no timeline to replay
// yet), and Cancel Import stops the process, leaving the open document as
// it is. The first time an item of an .f3d design is to be tried again on
// the file's stored bodies, which would replace bodies the import already
// has as the file's (mitcad#283), the user is asked whether to keep the
// imported (parametric) bodies or to replace them; the answer holds for
// the rest of the import.
//
// Nothing of the file is read in the application's process (mitcad#922):
// the documents an .f3d design refers to and a building project's linked
// models are listed by the worker too (--list-references, --list-links),
// behind the same progress dialog, and their file dialogs are asked when
// the listing returns. A fault of a reader on a broken or hostile file
// ends that process, and the import fails with "the import process
// stopped", the session's work untouched.
class F3dImport : public QObject {
  Q_OBJECT

public:
  F3dImport(const QString& path, QWidget* window);
  ~F3dImport() override;

  void start();

  // The import's options, set before start(): the import runs with these
  // (an .f3d design run again with items whose deviating results the user
  // keeps, mitcad#379) instead of asking for the documents it refers to.
  void setOptions(const QJsonObject& options);
  // Whether start() asks the import's choices (File > Open and Import):
  // an .iam assembly's Parts with their design history (mitcad#507). Not
  // asked (--open, MCP), the choice remembered last is taken.
  void setAskChoices(bool ask) { m_ask = ask; }
  // The options the import ran with (the documents chosen, ...).
  const QJsonObject& options() const { return m_options; }
  // The store the worker hands the import's evaluation over in (every
  // result a store takes, mitcad#413), while this object lives: the
  // application reads the imported design with it rather than evaluating
  // every feature again. Empty: none was written.
  QString handedOverResults() const {
    return m_handedOver > 0 ? m_dir.filePath(QStringLiteral("results")) : QString();
  }

signals:
  // ok: `design` is the imported document's design file (JSON) and
  // `result` the import_f3d command's result with its report. Not ok:
  // `error` says why (cancelled: empty).
  void finished(bool ok, const QByteArray& design, const QJsonObject& result, const QString& error);

private:
  // What the worker process does now.
  enum class Phase { None, ListingReferences, ListingLinks, Importing };

  // The documents the design refers to (its components of other
  // documents, mitcad#196) listed by the worker (mitcad#922); those whose
  // files are not found next to it are asked in referencesListed.
  void listReferences();
  // A file dialog for each document not found, naming the document,
  // whose Cancel skips it; after a chosen file the documents are listed
  // again (its folder is searched for the other missing ones, and a
  // document found refers to its own). When none is left to ask:
  // finishReferences, the import's options (the chosen files and the
  // skipped documents), and the import.
  void referencesListed(const QJsonObject& listed);
  void finishReferences();
  // A building project's linked models (mitcad#275) listed by the worker
  // (mitcad#922), once; linksListed asks for those not found next to it:
  // a file dialog for each, naming the linked file, whose Cancel skips it
  // (its component stays empty). The import's options: the chosen files by
  // the links' file names.
  void listLinks();
  void linksListed(const QJsonObject& listed);
  // The import itself in the worker (m_options).
  void startImport();
  // Runs the worker with these arguments, the progress dialog shown.
  void startWorker(const QStringList& arguments);
  // A listing's process ended: its result asked about, or the import
  // failed (the process stopped), or went on without it (it said why).
  void listingFinished(int exitCode, QProcess::ExitStatus status);
  // Why the worker failed: its own message, or that it stopped.
  QString failure(int exitCode, QProcess::ExitStatus status) const;
  void readProgress();
  // The worker's question (mitcad#283): keep the imported bodies that
  // match the file's or replace them with the stored ones; the answer goes
  // to the worker.
  void askRetryBodies(const QString& question);
  void processFinished(int exitCode, QProcess::ExitStatus status);
  void stop();
  void cancel();
  void tick();
  void done(bool ok, const QString& error);

  QString m_path;
  QJsonObject m_options; // more options of the import (finishReferences, linksListed)
  bool m_preset = false; // m_options given (setOptions)
  QPointer<QWidget> m_window;
  QTemporaryDir m_dir;
  int m_handedOver = 0; // results the worker handed over (handedOverResults)
  QProcess* m_process = nullptr; // the worker running now
  Phase m_phase = Phase::None;
  // The listing before the import (mitcad#922): the dialog's text while
  // it runs, the documents' files chosen and skipped so far and those
  // asked, the folder the next file dialog opens, and whether its time
  // limit ended it.
  QString m_listing;
  QJsonObject m_referenceFiles;
  QJsonArray m_skipped;
  QStringList m_asked;
  QString m_folder;
  QTimer* m_listWatch = nullptr;
  bool m_listTimedOut = false;
  ImportProgress* m_dialog = nullptr;
  QPointer<QMessageBox> m_question; // askRetryBodies, while it asks
  QTimer* m_ticker = nullptr;
  QElapsedTimer m_clock;
  QByteArray m_errors; // the worker's standard error, line by line
  int m_items = 0;     // timeline items, when the worker said
  int m_done = 0;      // items replayed
  int m_position = 0;  // the item being replayed (from 1)
  QString m_current;   // its name
  // An .iam assembly's part files (mitcad#507): how many the worker
  // counted, the one being imported (from 1), how many are done, its name.
  int m_parts = 0;
  int m_part = 0;
  int m_partsDone = 0;
  QString m_partName;
  bool m_ask = false;  // setAskChoices
  QString m_lastLine;  // the worker's last other line (a crash's)
  QString m_failure;   // the worker's own message why it failed (import-failed)
  bool m_stopping = false;
  bool m_finished = false;
};

// The worker process: imports the file given after --import-worker and
// writes the design (--output) and the command's result (--result). A
// line "stop" on its standard input stops the import early. The import's
// question to the user comes as a line "import-ask <question>" on its
// standard output, the answer as "answer keep" or "answer replace" on its
// standard input. With --list-references (an .f3d design) or --list-links
// (a building project) it imports nothing: it writes the listing (JSON) to
// --result and ends with a line "import-listed" (mitcad#922); the options
// file (--import-options) gives the documents' files chosen so far.
// Returns the process's exit code.
int runImportWorker(int argc, char* argv[]);

// Whether the command line asks for the worker.
bool isImportWorker(int argc, char* argv[]);

} // namespace mitcad
