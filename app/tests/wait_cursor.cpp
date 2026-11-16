// SPDX-License-Identifier: MIT
// The wait cursor of the jobs the window waits for (mitcad#268): waits as
// MainWindow::waitForJob makes them (a guard, released when the progress
// dialog shows, else at the end), nested, left early; no override cursor
// remains and the log lines pair up; the idle recovery of a cursor left on.
#include "UnitTests.hpp"
#include "framework/WaitCursor.hpp"

#include <cstdio>
#include <functional>
#include <map>
#include <stdexcept>

#include <QCursor>
#include <QGuiApplication>
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QtLogging>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "wait_cursor.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QStringList g_lines;
QtMessageHandler g_previous = nullptr;

void collect(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  g_lines << message;
  if (g_previous != nullptr) {
    g_previous(type, context, message);
  }
}

// A wait as waitForJob makes it: the cursor while it waits; at `dialogAt`
// (a step, -1 never) the progress dialog shows and the cursor goes; at
// `nestedAt` another wait runs inside this one's loop.
void jobWait(const QString& label, int steps, int dialogAt, int nestedAt = -1,
             const std::function<void()>& nested = {}) {
  mitcad::WaitCursor cursor(label);
  CHECK(QGuiApplication::overrideCursor() != nullptr || !cursor.holds());
  for (int step = 0; step < steps; ++step) {
    if (step == dialogAt) {
      cursor.release();
      cursor.release(); // once only
    }
    if (step == nestedAt && nested) {
      nested();
    }
  }
}

// Every "Wait cursor on: <job> (depth N)" has its "Wait cursor off: <job>
// (depth N)" after it, and none is off before it is on.
bool balanced(const QStringList& lines) {
  static const QRegularExpression on(QStringLiteral("^Wait cursor on: (.*)$"));
  static const QRegularExpression off(QStringLiteral("^Wait cursor off: (.*)$"));
  std::map<QString, int> open;
  for (const QString& line : lines) {
    if (const auto match = on.match(line); match.hasMatch()) {
      ++open[match.captured(1)];
    } else if (const auto ended = off.match(line); ended.hasMatch()) {
      if (--open[ended.captured(1)] < 0) {
        std::fprintf(stderr, "wait_cursor.cpp: off before on: %s\n", qPrintable(line));
        return false;
      }
    }
  }
  for (const auto& [key, count] : open) {
    if (count != 0) {
      std::fprintf(stderr, "wait_cursor.cpp: %d unbalanced: %s\n", count, qPrintable(key));
      return false;
    }
  }
  return true;
}

int count(const QStringList& lines, const QString& text) {
  int n = 0;
  for (const QString& line : lines) {
    n += line == text ? 1 : 0;
  }
  return n;
}

} // namespace

int waitCursorTests() {
  using mitcad::WaitCursor;
  g_lines.clear();
  g_previous = qInstallMessageHandler(collect);
  // Nothing left over from before.
  while (QGuiApplication::overrideCursor() != nullptr) {
    QGuiApplication::restoreOverrideCursor();
  }

  // A quick wait: the cursor until the end.
  jobWait(QStringLiteral("quick"), 3, -1);
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(WaitCursor::depth() == 0);
  CHECK(g_lines.contains(QStringLiteral("Wait cursor on: quick (depth 1)")));
  CHECK(g_lines.contains(QStringLiteral("Wait cursor off: quick (depth 1)")));

  // The progress dialog: the cursor goes when it shows, not again at the end.
  jobWait(QStringLiteral("slow"), 5, 2);
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(count(g_lines, QStringLiteral("Wait cursor off: slow (depth 1)")) == 1);

  // Nested: a wait inside a wait that still holds the cursor, its dialog
  // showing; then the outer one's dialog, and a wait inside it.
  jobWait(QStringLiteral("outer"), 6, 4, 1, [] {
    CHECK(WaitCursor::depth() == 1);
    jobWait(QStringLiteral("inner"), 4, 2, 1, [] { CHECK(WaitCursor::depth() == 2); });
    CHECK(WaitCursor::depth() == 1);
    CHECK(QGuiApplication::overrideCursor() != nullptr); // the outer one's
  });
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(g_lines.contains(QStringLiteral("Wait cursor on: inner (depth 2)")));
  CHECK(g_lines.contains(QStringLiteral("Wait cursor off: inner (depth 2)")));
  jobWait(QStringLiteral("dialog first"), 6, 1, 3, [] {
    CHECK(WaitCursor::depth() == 0);
    jobWait(QStringLiteral("after the dialog"), 3, -1);
  });
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(g_lines.contains(QStringLiteral("Wait cursor on: after the dialog (depth 1)")));

  // A wait left early.
  try {
    jobWait(QStringLiteral("thrown"), 4, -1, 1, [] { throw std::runtime_error("a failed wait"); });
  } catch (const std::runtime_error&) {
  }
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(WaitCursor::depth() == 0);
  CHECK(balanced(g_lines));

  // The idle check after the waits: none set, said once.
  CHECK(WaitCursor::recoverIfIdle() == 0);
  CHECK(count(g_lines, QStringLiteral("Wait cursor at idle: none set")) == 1);
  CHECK(WaitCursor::recoverIfIdle() == 0);
  CHECK(count(g_lines, QStringLiteral("Wait cursor at idle: none set")) == 1);

  // A cursor left on: cleared at idle, with a warning naming the wait.
  WaitCursor::leaveOnForTest(QStringLiteral("test hook"));
  CHECK(QGuiApplication::overrideCursor() != nullptr);
  CHECK(WaitCursor::recoverIfIdle() == 1);
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(g_lines.contains(QStringLiteral("Wait cursor left on after test hook: cleared")));
  CHECK(count(g_lines, QStringLiteral("Wait cursor at idle: none set")) == 2);
  // Two left by a wait whose name is the latest.
  jobWait(QStringLiteral("before two"), 1, -1);
  QGuiApplication::setOverrideCursor(Qt::WaitCursor);
  QGuiApplication::setOverrideCursor(Qt::WaitCursor);
  CHECK(WaitCursor::recoverIfIdle() == 2);
  CHECK(g_lines.contains(QStringLiteral("Wait cursor left on after before two: cleared (2 override cursors)")));
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  // Not while a wait holds its cursor: that one is not left on.
  {
    WaitCursor holding(QStringLiteral("holding"));
    CHECK(WaitCursor::recoverIfIdle() == 0);
    CHECK(QGuiApplication::overrideCursor() != nullptr);
  }
  CHECK(QGuiApplication::overrideCursor() == nullptr);
  CHECK(balanced(g_lines));

  qInstallMessageHandler(g_previous);
  g_previous = nullptr;
  return failures;
}
MITCAD_UNIT_TEST(waitCursorTests);
