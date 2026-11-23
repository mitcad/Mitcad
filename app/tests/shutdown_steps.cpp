// SPDX-License-Identifier: MIT
// The shutdown's steps (mitcad#2275): no line before the teardown begins,
// each step's line and a step with an end's "done" line, the step timer
// told of each step only once the event loop has ended (the enclosing step
// timed again when an inner one ends, "after <step>" when none is left), a
// mismatched end ignored, the web browser module's steps by name, nothing
// timed after end().
#include "UnitTests.hpp"
#include "framework/ShutdownModuleStep.hpp"
#include "framework/ShutdownSteps.hpp"

#include <cstdio>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QtLogging>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "shutdown_steps.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QStringList g_lines;
QtMessageHandler g_previous = nullptr;

void capture(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  if (message.startsWith(QStringLiteral("Shutdown"))) {
    g_lines << message;
    return;
  }
  if (g_previous != nullptr) {
    g_previous(type, context, message);
  }
}

bool hasLine(const QString& pattern) {
  const QRegularExpression re(pattern);
  for (const QString& line : std::as_const(g_lines)) {
    if (re.match(line).hasMatch()) {
      return true;
    }
  }
  return false;
}

using Timed = std::vector<std::pair<QString, int>>;

void stepTests() {
  using namespace mitcad;
  shutdown::resetForTests();
  g_lines.clear();
  Timed timed;
  shutdown::setStepTimer([&timed](const QString& step, int limitMs) { timed.emplace_back(step, limitMs); });

  // Before the teardown: nothing.
  shutdown::step(QStringLiteral("too early"));
  { const shutdown::Step step(QStringLiteral("too early with an end")); }
  CHECK(!shutdown::running());
  CHECK(g_lines.isEmpty());
  CHECK(timed.empty());

  // The last window closed: lines, not timed yet (the event loop turns).
  shutdown::begin(QStringLiteral("the last document window closed"));
  shutdown::begin(QStringLiteral("again"));
  CHECK(shutdown::running());
  shutdown::step(QStringLiteral("window-1: section worker"));
  CHECK(hasLine(QStringLiteral("^Shutdown begins: the last document window closed$")));
  CHECK(!hasLine(QStringLiteral("again")));
  CHECK(hasLine(QStringLiteral("^Shutdown: window-1: section worker$")));
  CHECK(timed.empty());

  // The event loop ended: timed from now on, once.
  shutdown::eventLoopEnded();
  shutdown::eventLoopEnded();
  CHECK(timed.size() == 1);
  CHECK(!timed.empty() && timed.back().first == QStringLiteral("the event loop ended"));
  CHECK(g_lines.count(QStringLiteral("Shutdown: the event loop ended")) == 1);

  // Nested steps: the enclosing step is timed again with its own limit.
  timed.clear();
  {
    const shutdown::Step outer(QStringLiteral("outer"), 700);
    shutdown::step(QStringLiteral("a mark"));
    { const shutdown::Step inner(QStringLiteral("inner")); }
  }
  const Timed expected{{QStringLiteral("outer"), 700},
                       {QStringLiteral("a mark"), 0},
                       {QStringLiteral("inner"), 0},
                       {QStringLiteral("outer"), 700},
                       {QStringLiteral("after outer"), 0}};
  CHECK(timed == expected);
  CHECK(hasLine(QStringLiteral("^Shutdown: inner done \\([0-9]+ ms\\)$")));
  CHECK(hasLine(QStringLiteral("^Shutdown: outer done \\([0-9]+ ms\\)$")));
  CHECK(!hasLine(QStringLiteral("a mark done")));

  // An end without its beginning: nothing.
  timed.clear();
  const qsizetype before = g_lines.size();
  shutdown::endStep(QStringLiteral("never begun"));
  CHECK(timed.empty());
  CHECK(g_lines.size() == before);

  // The module's steps by name, through the application's object.
  if (QCoreApplication* app = QCoreApplication::instance()) {
    auto* object = new shutdown::StepsObject(app);
    { const shutdown::ModuleStep step(QStringLiteral("component browser: CEF shuts down")); }
    delete object;
    CHECK(hasLine(QStringLiteral("^Shutdown: component browser: CEF shuts down$")));
    CHECK(hasLine(QStringLiteral("^Shutdown: component browser: CEF shuts down done \\([0-9]+ ms\\)$")));
    CHECK(!timed.empty() && timed.back().first == QStringLiteral("after component browser: CEF shuts down"));
  }
  // Without it: nothing.
  g_lines.clear();
  { const shutdown::ModuleStep step(QStringLiteral("no object")); }
  CHECK(g_lines.isEmpty());

  // After the end: lines, nothing timed.
  timed.clear();
  shutdown::end();
  shutdown::step(QStringLiteral("the session log ends"));
  CHECK(hasLine(QStringLiteral("^Shutdown: the session log ends$")));
  CHECK(timed.empty());

  shutdown::resetForTests();
}

} // namespace

int shutdownStepsTests() {
  failures = 0;
  g_previous = qInstallMessageHandler(capture);
  stepTests();
  qInstallMessageHandler(g_previous);
  return failures;
}

MITCAD_UNIT_TEST(shutdownStepsTests);
