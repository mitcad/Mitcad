// SPDX-License-Identifier: MIT
#pragma once

// The chat's history files (mitcad#947): one JSON Lines file per design in
// a folder of the user's data (chatDataDirectory()), named by a digest of
// the design's path, so that no path shows in the file names. The lines'
// format is the chat logic's (core/mcp/src/chat/history.rs); this keeps
// the files: appends, reads the end of one, moves it with its design and
// removes them. Only the user can read them. Plain Qt Core.

#include <QByteArray>
#include <QString>

namespace mitcad::agent {

class ChatHistoryStore {
public:
  // The most of a file's end read: the newest entries.
  static constexpr qint64 kReadLimit = 4LL * 1024 * 1024;

  explicit ChatHistoryStore(QString directory);

  // A design's key: the SHA-256 of its absolute, cleaned path (letter case
  // ignored on Windows), in hex.
  static QString keyFor(const QString& designPath);
  QString fileFor(const QString& designPath) const;
  const QString& directory() const { return m_directory; }

  // The end of the design's file, whole lines, at most `limit` bytes;
  // empty without one.
  QByteArray read(const QString& designPath, qint64 limit = kReadLimit) const;
  bool append(const QString& designPath, const QByteArray& lines) const;
  // The design was saved under another path (Save As, renamed): its
  // history goes with it, after what the new path already had.
  bool move(const QString& fromPath, const QString& toPath) const;
  bool clear(const QString& designPath) const;
  // Every design's history.
  bool clearAll() const;

private:
  QString m_directory;
};

} // namespace mitcad::agent
