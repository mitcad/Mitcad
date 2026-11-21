// SPDX-License-Identifier: MIT
// The Import dialog's message when the import worker stopped without
// saying why (mitcad#2085, files/ImportWorkerFailure): in words, with the
// item it was importing; never the places tail of a model message the
// worker's trace printed last, JSON or a face name.
#include "UnitTests.hpp"
#include "files/ImportWorkerFailure.hpp"

#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "import_worker_failure.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// The tail line of a refusal, as the worker's trace prints it after the
// refusal's first line.
const QString kTail =
    QStringLiteral(R"tail([placed: {"message":"a face is listed more than once","places":[{"kind":"face",)tail"
                   R"tail("name":"F450:import(5)"}],"detail":"face F450:import(5) is listed more than once"}])tail");

bool clean(const QString& message) {
  return !message.contains(QLatin1String("[placed")) && !message.contains(QLatin1Char('{')) &&
         !message.contains(QLatin1String("F450")) && !message.contains(QLatin1String("import(5)"));
}

void crashTests() {
  // A crash whose last line is a tail: the process stopped, in words.
  const QString crashed = importWorkerStopped(true, 0, QStringLiteral("timeline item 503 (Move Face 5)"), kTail);
  CHECK(crashed.startsWith(QLatin1String("the import process stopped")));
  CHECK(crashed.contains(QLatin1String("unexpectedly")));
  CHECK(crashed.contains(QLatin1String("timeline item 503 (Move Face 5)")));
  CHECK(clean(crashed));
  // No item known yet.
  CHECK(importWorkerStopped(true, 0, QString(), kTail) == QLatin1String("the import process stopped unexpectedly"));
  // A crash never shows its last line, words or not.
  CHECK(!importWorkerStopped(true, 0, QString(), QStringLiteral("some words")).contains(QLatin1String("words")));
}

void exitTests() {
  // An exit with a code: its last line in words is shown, a tail never.
  const QString words = importWorkerStopped(false, 3, QString(), QStringLiteral("the installation is mixed"));
  CHECK(words.contains(QLatin1String("exit code 3")));
  CHECK(words.endsWith(QLatin1String(": the installation is mixed")));
  const QString tail = importWorkerStopped(false, 3, QString(), kTail);
  CHECK(tail == QLatin1String("the import process stopped (exit code 3)"));
  // A message with its tail on one line: the words before it.
  CHECK(importWorkerLineShown(QStringLiteral("a face is listed more than once\n") + kTail) ==
        QLatin1String("a face is listed more than once"));
  CHECK(importWorkerLineShown(QStringLiteral("{\"error\": 1}")).isEmpty());
  CHECK(clean(importWorkerStopped(false, 1, QStringLiteral("timeline item 2"), QStringLiteral("x ") + kTail)));
}

} // namespace

int importWorkerFailureTests() {
  failures = 0;
  crashTests();
  exitTests();
  return failures;
}
MITCAD_UNIT_TEST(importWorkerFailureTests);
