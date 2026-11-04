// SPDX-License-Identifier: MIT
#pragma once

#include <deque>
#include <optional>

#include <QDateTime>
#include <QString>

#include "NoticeLevel.hpp"

namespace mitcad {

// One message of the session's history (mitcad#1042): when it came, its
// level, its text and the feature it is about (its uid; empty when it is
// about none). A message repeated at once is one entry, counted.
struct MessageEntry {
  QDateTime time;
  NoticeLevel level = NoticeLevel::Hint;
  QString text;
  QString feature;
  int count = 1;
};

// What the message band shows and the session's message history
// (mitcad#1042, docs/ui-conventions.md "The message band"), without
// widgets or timers: the band asks it what to show and tells it when a
// message's time ran out, the user closed it or the next command started.
//
// - A message is shown in full at once, unless a problem of a higher level
//   is shown: a hint does not push an error or a warning away (it goes to
//   the history only).
// - Hints, progress, information and successes fade after 6 s, a warning
//   after 10 s (expire); an error stays until the user closes it (close)
//   or starts the next command (commandStarted).
// - Faded or closed, the band shows the latest message as a one-line stub,
//   which opens the history.
// - Another design in the window (New, Open, recovery; mitcad#1174) clears
//   the band: neither the message shown in full nor the stub stays, until
//   the next message (clear). The history keeps them.
// - The history keeps this session's messages, newest first, at most
//   kMaxEntries of them; nothing is saved.
class MessageLog {
public:
  static constexpr int kMaxEntries = 500;

  // What the band shows: nothing (no message yet), a message in full, or
  // the stub of the latest message.
  enum class Shown { None, Full, Stub };

  // How long a message of the level is shown in full, in milliseconds; 0
  // for an error, which stays until it is closed or the next command
  // starts.
  static int lifetime(NoticeLevel level);

  // A message: recorded in the history and shown in full unless a problem
  // of a higher level is shown (see above). Returns whether it is shown in
  // full now. An empty text records nothing and turns a message shown in
  // full that is not a problem into the stub (a cleared status line).
  bool add(const QString& text, NoticeLevel level, const QString& feature, const QDateTime& time);

  // The message shown in full timed out: the stub. Not for an error.
  // Returns whether the band changed.
  bool expire();
  // The user closed the message shown in full: the stub. Returns whether
  // the band changed.
  bool close();
  // The next command started: an error shown in full becomes the stub.
  // Returns whether the band changed.
  bool commandStarted();
  // Another design in the window (mitcad#1174): the band shows nothing,
  // neither a message in full (an error too) nor the stub, until the next
  // message; the history keeps them. Returns whether the band changed.
  bool clear();
  // What a message was about is over (mitcad#1230: a section that cuts
  // again): when the message shown in full has this text, it becomes the
  // stub, a problem too; the history keeps it. Returns whether the band
  // changed.
  bool withdraw(const QString& text);

  Shown shown() const;
  // The message shown in full; null unless shown() is Full.
  const MessageEntry* full() const { return m_full ? &*m_full : nullptr; }
  // The latest message; null without a message.
  const MessageEntry* latest() const { return m_entries.empty() ? nullptr : &m_entries.front(); }
  // The message the stub shows (the latest); null unless shown() is Stub.
  const MessageEntry* stub() const { return shown() == Shown::Stub ? &m_entries.front() : nullptr; }

  // The session's messages, newest first.
  const std::deque<MessageEntry>& entries() const { return m_entries; }
  int size() const { return static_cast<int>(m_entries.size()); }

private:
  std::deque<MessageEntry> m_entries;
  std::optional<MessageEntry> m_full; // shown in full
  bool m_cleared = false;             // no stub until the next message (clear)
};

} // namespace mitcad
