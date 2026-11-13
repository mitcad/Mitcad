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
// the application starts and by File > Recover Designs.

// A session whose process is gone, with what its files tell.
struct RecoverableSession {
  QString directory;
  QString session;    // its id: metadata, snapshots and lock share this prefix
  QString design;     // validated snapshot basename, from v1 or v2 metadata
  QString document;   // the window's name of the document ("Untitled", "block.mitcad")
  QString path;       // the file it was opened from or saved to, or empty
  QString baseDigest; // that file's content then (fileDigest), or empty
  QDateTime savedAt;  // when it was written (UTC)
  qint64 size = 0;    // of the design file
  QString digest;     // of the design file
  // Why it cannot be restored (no metadata, the design file missing or
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

  QString designFile() const;
  bool damaged() const { return !damage.isEmpty(); }
  // "Untitled, never saved, autosaved 2026-10-05T03:12:00Z, 2865 bytes":
  // what the log and the tests read.
  QString describe() const;
};

// The sessions in `directory` whose process is gone (their lock could be
// taken), newest first; `own` is this document window's session, never one of them.
// Locks of such sessions that left no files are removed.
std::vector<RecoverableSession> findRecoverable(const QString& directory, const QString& own);

// Removes a session's files and its lock.
void discardSession(RecoverableSession& session);

// The dialog "Recover Unsaved Work": a table of the sessions, with Restore
// (all healthy designs selected initially), Discard (the ones selected,
// after a question; they are removed and leave `sessions`) and Later.
// Returns the selected healthy sessions to restore, or an empty list.
// `atStart`: asked at start-up rather than by File > Recover Designs (live
// MCP's refusal then points to --no-recovery, mitcad#1725).
std::vector<std::size_t> askRecovery(QWidget* parent, std::vector<RecoverableSession>& sessions, bool atStart);

} // namespace mitcad
