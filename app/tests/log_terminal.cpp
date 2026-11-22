// SPDX-License-Identifier: MIT
// The log ring's stderr in a terminal (mitcad#2213): with stderr on a
// pseudo-terminal only warnings and worse go on to the handler before the
// ring, while the ring keeps every line; MITCAD_LOG_STDERR=1 and the UI
// test driver's runs (MITCAD_TEST_INPUT) pass every line on; a stderr on a
// file gets every line. Windows has no console: every line passes on.
#include "UnitTests.hpp"

#include <cstdio>
#include <vector>

#include <QByteArray>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QtLogging>

#include "mcp/LogRing.hpp"

#if !defined(_WIN32)
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#endif

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "log_terminal.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// What the handler before the ring got.
QStringList g_passed;

void collect(QtMsgType /*type*/, const QMessageLogContext& /*context*/, const QString& message) {
  g_passed << message;
}

const QStringList kAll = {QStringLiteral("terminal test debug"), QStringLiteral("terminal test info"),
                          QStringLiteral("terminal test warning"), QStringLiteral("terminal test critical")};
const QStringList kWarnings = {QStringLiteral("terminal test warning"), QStringLiteral("terminal test critical")};

struct Run {
  QStringList passed; // to the handler before the ring
  QStringList kept;   // in the ring
};

// Installs the ring after collect(), logs one message of each level and
// removes it again.
Run logEachLevel() {
  g_passed.clear();
  const QtMessageHandler before = qInstallMessageHandler(collect);
  mitcad::mcp::installLogRing();
  quint64 next = 0;
  mitcad::mcp::logLines(0, 0, &next);
  const quint64 start = next - 1;
  qDebug().noquote() << kAll.at(0);
  qInfo().noquote() << kAll.at(1);
  qWarning().noquote() << kAll.at(2);
  qCritical().noquote() << kAll.at(3);
  Run run;
  for (const mitcad::mcp::LogLine& line : mitcad::mcp::logLines(start, 100)) {
    if (line.text.startsWith(QStringLiteral("terminal test "))) {
      run.kept << line.text;
    }
  }
  mitcad::mcp::removeLogRing();
  qInstallMessageHandler(before);
  for (const QString& message : g_passed) {
    if (message.startsWith(QStringLiteral("terminal test "))) {
      run.passed << message;
    }
  }
  return run;
}

// An environment variable put back as it was at the end of the scope.
class EnvironmentGuard {
public:
  explicit EnvironmentGuard(const char* name)
      : m_name(name), m_had(qEnvironmentVariableIsSet(name)), m_value(qgetenv(name)) {
    qunsetenv(name);
  }
  ~EnvironmentGuard() {
    if (m_had) {
      qputenv(m_name, m_value);
    } else {
      qunsetenv(m_name);
    }
  }
  EnvironmentGuard(const EnvironmentGuard&) = delete;
  EnvironmentGuard& operator=(const EnvironmentGuard&) = delete;

private:
  const char* m_name;
  bool m_had;
  QByteArray m_value;
};

#if !defined(_WIN32)

// Runs logEachLevel() with stderr on `fd`; nothing is written to stderr
// meanwhile (the checks come after, with stderr back).
Run withStderrOn(int fd, bool* redirected) {
  std::fflush(stderr);
  const int saved = dup(STDERR_FILENO);
  *redirected = saved >= 0 && dup2(fd, STDERR_FILENO) >= 0;
  Run run;
  if (*redirected) {
    run = logEachLevel();
  }
  std::fflush(stderr);
  if (saved >= 0) {
    dup2(saved, STDERR_FILENO);
    close(saved);
  }
  return run;
}

void terminalTests() {
  const EnvironmentGuard logStderr("MITCAD_LOG_STDERR");
  const EnvironmentGuard testInput("MITCAD_TEST_INPUT");
  mitcad::mcp::removeLogRing();

  // A pseudo-terminal: its other end stays open, so that writes to it
  // succeed (Qt's handlers write nothing here: collect() is before the
  // ring).
  const int master = posix_openpt(O_RDWR | O_NOCTTY);
  CHECK(master >= 0);
  if (master < 0) {
    return;
  }
  CHECK(grantpt(master) == 0);
  CHECK(unlockpt(master) == 0);
  const char* name = ptsname(master);
  CHECK(name != nullptr);
  const int terminal = name != nullptr ? open(name, O_RDWR | O_NOCTTY) : -1;
  CHECK(terminal >= 0);
  if (terminal >= 0) {
    CHECK(isatty(terminal) != 0);
    bool redirected = false;

    // Only warnings and worse go on; the ring keeps every line.
    Run run = withStderrOn(terminal, &redirected);
    CHECK(redirected);
    CHECK(run.passed == kWarnings);
    CHECK(run.kept == kAll);

    // MITCAD_LOG_STDERR=1: every line.
    qputenv("MITCAD_LOG_STDERR", "1");
    run = withStderrOn(terminal, &redirected);
    CHECK(redirected);
    CHECK(run.passed == kAll);
    CHECK(run.kept == kAll);
    qunsetenv("MITCAD_LOG_STDERR");

    // MITCAD_LOG_STDERR=0 is the default: warnings only.
    qputenv("MITCAD_LOG_STDERR", "0");
    run = withStderrOn(terminal, &redirected);
    CHECK(run.passed == kWarnings);
    qunsetenv("MITCAD_LOG_STDERR");

    // The UI test driver records every line through the handler before
    // the ring.
    qputenv("MITCAD_TEST_INPUT", "script.mitcad-ui");
    run = withStderrOn(terminal, &redirected);
    CHECK(run.passed == kAll);
    qunsetenv("MITCAD_TEST_INPUT");
    close(terminal);
  }
  close(master);

  // A stderr redirected to a file: every line, as before.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QByteArray path = directory.filePath(QStringLiteral("stderr.log")).toLocal8Bit();
  const int file = open(path.constData(), O_WRONLY | O_CREAT | O_TRUNC, 0600);
  CHECK(file >= 0);
  if (file >= 0) {
    CHECK(isatty(file) == 0);
    bool redirected = false;
    const Run run = withStderrOn(file, &redirected);
    CHECK(redirected);
    CHECK(run.passed == kAll);
    CHECK(run.kept == kAll);
    close(file);
  }
}

#else

// Windows: the app has no console; every line passes on wherever stderr
// goes.
void terminalTests() {
  const EnvironmentGuard logStderr("MITCAD_LOG_STDERR");
  const EnvironmentGuard testInput("MITCAD_TEST_INPUT");
  mitcad::mcp::removeLogRing();
  const Run run = logEachLevel();
  CHECK(run.passed == kAll);
  CHECK(run.kept == kAll);
}

#endif

} // namespace

int logTerminalTests() {
  failures = 0;
  terminalTests();
  return failures;
}

MITCAD_UNIT_TEST(logTerminalTests);
