// SPDX-License-Identifier: MIT
#include "MessageLog.hpp"

namespace mitcad {
namespace {

// How much a shown message holds its place against a newer one.
int rank(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return 2;
  case NoticeLevel::Warning:
    return 1;
  default:
    return 0;
  }
}

} // namespace

int MessageLog::lifetime(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return 0;
  case NoticeLevel::Warning:
    return 10000;
  default:
    return 6000;
  }
}

bool MessageLog::add(const QString& text, NoticeLevel level, const QString& feature, const QDateTime& time) {
  if (text.isEmpty()) {
    if (m_full && !isProblem(m_full->level)) {
      m_full.reset();
    }
    return false;
  }
  m_cleared = false; // the stub is this message's again
  if (!m_entries.empty() && m_entries.front().text == text && m_entries.front().level == level &&
      m_entries.front().feature == feature) {
    MessageEntry& same = m_entries.front();
    same.time = time;
    ++same.count;
  } else {
    m_entries.push_front({time, level, text, feature, 1});
    while (static_cast<int>(m_entries.size()) > kMaxEntries) {
      m_entries.pop_back();
    }
  }
  if (m_full && rank(m_full->level) > rank(level)) {
    return false; // the problem shown stays; this one is in the history
  }
  m_full = m_entries.front();
  return true;
}

bool MessageLog::expire() {
  if (!m_full || m_full->level == NoticeLevel::Error) {
    return false;
  }
  m_full.reset();
  return true;
}

bool MessageLog::close() {
  if (!m_full) {
    return false;
  }
  m_full.reset();
  return true;
}

bool MessageLog::commandStarted() {
  if (!m_full || m_full->level != NoticeLevel::Error) {
    return false;
  }
  m_full.reset();
  return true;
}

bool MessageLog::clear() {
  const bool changed = shown() != Shown::None;
  m_full.reset();
  m_cleared = true;
  return changed;
}

bool MessageLog::withdraw(const QString& text) {
  if (!m_full || text.isEmpty() || m_full->text != text) {
    return false;
  }
  m_full.reset();
  return true;
}

MessageLog::Shown MessageLog::shown() const {
  if (m_full) {
    return Shown::Full;
  }
  return m_entries.empty() || m_cleared ? Shown::None : Shown::Stub;
}

} // namespace mitcad
