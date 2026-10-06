// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <optional>

#include <QByteArray>
#include <QByteArrayView>
#include <QObject>
#include <QString>
#include <QThreadPool>
#include <QTimer>

class QLockFile;

namespace mitcad {

struct GeneralSettings;

// Autosave (P8): a modified document is written now and then to the
// recovery folder (recoveryDirectory(): the user's local application data,
// never a project's folder, so nothing of it reaches a project's version
// control), from where it can be recovered after a crash (P8c).
//
// Every run of the application is a session with an id of its own (a
// UUID), and the folder holds per session:
//
// - `<id>.lock`: a QLockFile held while the session runs, from its first
//   autosave to its end. A lock whose process is gone (QLockFile checks the
//   process id and the application's name) marks a session that crashed.
// - `<id>.mitcad`: the project file (the same format as a saved one, so
//   load_document reads it), written first.
// - `<id>.json`: what it is, written last, so that it marks a complete
//   pair: {"format": "mitcad-autosave", "version": 1, "session", "pid",
//   "application", "application_version", "document" (the window's name for
//   it), "path" (the file it was opened from or saved to, or empty),
//   "base_digest" (that file's content as opened or saved, or empty),
//   "saved_at" (UTC, ISO 8601), "revision" (the model's), "project" (the
//   project file's name), "size" and "digest" (the project file's)}.
//   Digests are FNV-1a 64 of the bytes, 16 hex digits (fileDigest).
//
// A document is written only when the model says it is modified and its
// revision is not the one written last, so an unchanged document is not
// written again, and none that is as on disk (saved, or undone back to
// it): its files are removed then, as after Save and when the window takes
// another document. A normal end removes the session's files and its lock.
// The project file is made on the UI thread when the model is not busy (the
// window's `blocked` says why not; it is tried again a little later), and
// both files are written on a thread of the manager's own, each atomically
// (QSaveFile: a temporary file, flushed to disk, then renamed).
//
// A session that ended without removing its files left work to recover
// (Recovery.hpp). A document recovered from them is this session's then
// (recovered()): the earlier session's files stay, under the lock this
// session took of them, until this session has written the document, or it
// is saved or replaced.
class AutosaveManager : public QObject {
  Q_OBJECT

public:
  // The window's document as the model tells.
  struct State {
    bool modified = false;
    quint64 revision = 0;
  };
  // What is written.
  struct Snapshot {
    QByteArray project; // the project file
    QString name;       // the window's name of the document
    QString path;       // the file it was opened from or saved to, or empty
  };

  explicit AutosaveManager(QObject* parent = nullptr);
  // A normal end: waits for a write, removes the session's files and
  // unlocks it.
  ~AutosaveManager() override;
  AutosaveManager(const AutosaveManager&) = delete;
  AutosaveManager& operator=(const AutosaveManager&) = delete;

  // The window answers these on the UI thread when the timer asks. Why the
  // document cannot be written now ("busy": a job computes it), or empty.
  std::function<QString()> blocked;
  std::function<State()> state;
  std::function<Snapshot()> snapshot;

  // Starts, changes or stops the timer (Preferences, General);
  // MITCAD_TEST_AUTOSAVE_SECONDS replaces the minutes for tests.
  void apply(const GeneralSettings& settings);
  // The window has another document (New, Open, Close, an import): what was
  // written of the one before goes.
  void replaced();
  // The document is as on disk now (opened or saved; `digest` is
  // fileDigest of the file's content): what was written of it goes.
  void saved(const QString& digest);
  // Writes the document at the next chance, if it is modified (an imported
  // design that is saved nowhere yet).
  void saveSoon();
  // The window's new document was recovered from the files of an earlier
  // session (P8c), whose lock `lock` holds; `baseDigest` is that session's
  // digest of the document's file. Call after replaced().
  void recovered(const QString& baseDigest, const QString& session, std::unique_ptr<QLockFile> lock);

  QString directory() const { return m_directory; }
  QString session() const { return m_session; }

private:
  void tick();
  void retryLater();
  bool lock();
  void discard();
  // The recovered document's earlier files go: written here, saved or
  // replaced, there is nothing of it left to recover from them.
  void releaseRecovered();
  void written(quint64 generation, quint64 revision, bool ok);
  QString filePath(const char* suffix) const;

  QString m_directory;
  QString m_session;
  std::unique_ptr<QLockFile> m_lock;
  QTimer m_timer;
  QTimer m_retry;
  // One write at a time, off the UI thread.
  QThreadPool m_pool;
  bool m_applied = false; // settings were applied once
  bool m_enabled = false;
  QString m_baseDigest;
  // What is on disk of this document: the revision last written, and the
  // name it was written as (both empty when nothing is).
  std::optional<quint64> m_written;
  QString m_writtenName;
  bool m_writing = false;
  bool m_haveFiles = false;
  // Counts the documents' turns: a write that comes back for an earlier
  // document does not count for this one.
  quint64 m_generation = 0;
  // The earlier session the document was recovered from, and its lock.
  QString m_recoveredSession;
  std::unique_ptr<QLockFile> m_recoveredLock;
};

// FNV-1a 64 of the bytes as 16 hex digits (the model's digest of linked
// files is the same hash).
QString fileDigest(QByteArrayView data);

// Removes a session's files from the recovery folder: its metadata first
// (without it, what is left is no pair to recover), then its project file
// and what an interrupted write left of either; not its lock.
void removeSessionFiles(const QString& directory, const QString& session);

} // namespace mitcad
