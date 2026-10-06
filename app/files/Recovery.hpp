// SPDX-License-Identifier: MIT
#pragma once

#include <cstddef>
#include <memory>
#include <optional>
#include <vector>

#include <QDateTime>
#include <QString>

class QLockFile;
class QWidget;

namespace mitcad {

// Recovery (P8c): the work that sessions which did not end normally (a
// crash, a kill) left in the recovery folder (Autosave.hpp), offered when
// the application starts and by File > Recover Documents.

// A session whose process is gone, with what its files tell.
struct RecoverableSession {
  QString directory;
  QString session;    // its id: the files are <id>.json, <id>.mitcad and <id>.lock
  QString document;   // the window's name of the document ("Untitled", "block.mitcad")
  QString path;       // the file it was opened from or saved to, or empty
  QString baseDigest; // that file's content then (fileDigest), or empty
  QDateTime savedAt;  // when it was written (UTC)
  qint64 size = 0;    // of the project file
  QString digest;     // of the project file
  // Why it cannot be restored (no metadata, the project file missing or
  // not the one the metadata describes), or empty.
  QString damage;
  // The file at `path` has changed since (its digest is not baseDigest),
  // or is gone.
  bool fileChanged = false;
  bool fileMissing = false;
  // Taken when it was found: no other instance offers the session while
  // this one does. Unlocked, the session stays for later.
  std::unique_ptr<QLockFile> lock;

  RecoverableSession();
  ~RecoverableSession();
  RecoverableSession(RecoverableSession&&) noexcept;
  RecoverableSession& operator=(RecoverableSession&&) noexcept;

  QString projectFile() const;
  bool damaged() const { return !damage.isEmpty(); }
  // "Untitled, never saved, autosaved 2026-10-05T03:12:00Z, 2865 bytes":
  // what the log and the tests read.
  QString describe() const;
};

// The sessions in `directory` whose process is gone (their lock could be
// taken), newest first; `own` is this run's session, never one of them.
// Locks of such sessions that left no files are removed.
std::vector<RecoverableSession> findRecoverable(const QString& directory, const QString& own);

// Removes a session's files and its lock.
void discardSession(RecoverableSession& session);

// The dialog "Recover Unsaved Work": a table of the sessions, with Restore
// (the one selected), Discard (the ones selected, after a question; they
// are removed and leave `sessions`) and Later. Returns the index of the
// session to restore, or nothing.
std::optional<std::size_t> askRecovery(QWidget* parent, std::vector<RecoverableSession>& sessions);

} // namespace mitcad
