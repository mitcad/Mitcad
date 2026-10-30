// SPDX-License-Identifier: MIT
// The live MCP endpoint against a hostile local client (mitcad#935): a
// refusal repeats only the start of a client's text, the log ring keeps at
// most 8 MiB with every line under 4 KiB (200 refusals of a 7 MiB input
// leave the process's memory under 50 MB more), waiting connections give
// way to new ones without holding the clients' slots, and the token is
// compared in constant time.
#include <cstdio>
#include <vector>

#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QtLogging>

#include "mcp/EndpointLimits.hpp"
#include "mcp/LogRing.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "mcp_endpoint_limits.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// Lines logged during the test go only to the ring (not 1.4 GB to stderr).
void quiet(QtMsgType, const QMessageLogContext&, const QString&) {}

// The process's resident memory in kB (Linux), or -1.
qint64 residentKb() {
  QFile status(QStringLiteral("/proc/self/status"));
  if (!status.open(QIODevice::ReadOnly)) {
    return -1;
  }
  for (const QByteArray& line : status.readAll().split('\n')) {
    if (line.startsWith("VmRSS:")) {
      bool ok = false;
      const qint64 kb = line.mid(6).trimmed().split(' ').value(0).toLongLong(&ok);
      return ok ? kb : -1;
    }
  }
  return -1;
}

void echoTests() {
  using mitcad::mcp::echoed;
  using mitcad::mcp::kEchoChars;
  CHECK(echoed(QStringLiteral("F2.b0")) == QStringLiteral("F2.b0"));
  const QString exact(kEchoChars, QLatin1Char('a'));
  CHECK(echoed(exact) == exact);
  const QString longer = exact + QString(1000, QLatin1Char('b'));
  const QString cut = echoed(longer);
  CHECK(cut.startsWith(exact) && !cut.contains(QLatin1Char('b')));
  CHECK(cut.endsWith(QStringLiteral("… (1000 more characters)")));
  // A surrogate pair across the cut stays whole or goes whole.
  QString pairs(kEchoChars - 1, QLatin1Char('a'));
  pairs += QString::fromUtf8("\xF0\x9F\x94\xA7\xF0\x9F\x94\xA7");
  const QString pairCut = echoed(pairs);
  CHECK(pairCut.startsWith(QString(kEchoChars - 1, QLatin1Char('a')) + QStringLiteral(" (")) ||
        pairCut.startsWith(QString(kEchoChars - 1, QLatin1Char('a')) + QStringLiteral("…")));
  CHECK(!pairCut.at(kEchoChars - 1).isHighSurrogate());

  // ui_select's refused entry with a 7 MiB owner, as readItem words it.
  const QString owner(7 * 1024 * 1024, QLatin1Char('o'));
  const QJsonObject entry{{QStringLiteral("kind"), QStringLiteral("face")},
                          {QStringLiteral("owner"), owner},
                          {QStringLiteral("name"), QStringLiteral("x")}};
  const QString problem = QObject::tr("not an entity: %1")
                              .arg(echoed(QString::fromUtf8(QJsonDocument(entry).toJson(QJsonDocument::Compact))));
  CHECK(problem.size() < 300);
  CHECK(problem.startsWith(QStringLiteral("not an entity: {\"kind\":\"face\",\"name\":\"x\",\"owner\":\"ooo")));
  CHECK(echoed(problem + owner, mitcad::mcp::kRefusalLogChars).size() < mitcad::mcp::kRefusalLogChars + 40);
}

void tokenTests() {
  using mitcad::mcp::sameToken;
  const QString token(64, QLatin1Char('a'));
  CHECK(sameToken(token, token));
  CHECK(sameToken(QString(64, QLatin1Char('a')), token));
  // Unequal tokens of the same length: differing first, last, everywhere.
  CHECK(!sameToken(QLatin1Char('b') + token.mid(1), token));
  CHECK(!sameToken(token.left(63) + QLatin1Char('b'), token));
  CHECK(!sameToken(QString(64, QLatin1Char('f')), token));
  // Of different lengths: a prefix, a longer one, none.
  CHECK(!sameToken(token.left(32), token));
  CHECK(!sameToken(token + QLatin1Char('a'), token));
  CHECK(!sameToken(QString(), token));
  CHECK(!sameToken(QString(4096, QLatin1Char('a')), token));
  // No token matches nothing, not even no token.
  CHECK(!sameToken(QString(), QString()));
}

void slotTests() {
  using mitcad::mcp::clientSlotFree;
  using mitcad::mcp::kMaxClients;
  using mitcad::mcp::kMaxPending;
  using mitcad::mcp::pendingToClose;
  CHECK(kMaxPending < kMaxClients);
  CHECK(pendingToClose({}) == -1);
  CHECK(pendingToClose({3, 1, 2}) == -1);
  // Full: the oldest gives way, wherever it is in the list.
  CHECK(pendingToClose({7, 4, 9, 5}) == 1);
  CHECK(pendingToClose({2, 4, 9, 5}) == 0);
  CHECK(pendingToClose({12, 14, 19, 15, 11}) == 4);
  // Eight idle connections (mitcad#935's case), as they arrive: from the
  // fifth on, each new one closes the oldest waiting; the client that then
  // comes finds its place.
  std::vector<quint64> waiting;
  for (quint64 order = 1; order <= 9; ++order) {
    const int closed = pendingToClose(waiting);
    CHECK((order <= static_cast<quint64>(kMaxPending)) == (closed < 0));
    if (closed >= 0) {
      waiting.erase(waiting.begin() + closed);
    }
    waiting.push_back(order);
  }
  CHECK(static_cast<int>(waiting.size()) == kMaxPending);
  CHECK(waiting.back() == 9U);
  // Only authenticated connections count toward the clients' slots.
  CHECK(clientSlotFree(0));
  CHECK(clientSlotFree(kMaxClients - 1));
  CHECK(!clientSlotFree(kMaxClients));
}

void ringTests() {
  using mitcad::mcp::formatLogLine;
  using mitcad::mcp::kMaxLogLineBytes;
  using mitcad::mcp::logLines;
  using mitcad::mcp::LogLine;
  mitcad::mcp::removeLogRing();
  const QtMessageHandler previous = qInstallMessageHandler(quiet);
  mitcad::mcp::installLogRing();

  quint64 next = 0;
  logLines(0, 0, &next);
  const quint64 start = next - 1;
  // The refusal of a ui_select whose owner is 7 MiB, logged whole (the
  // ring is the last line of defence), 200 times: the memory hardly grows.
  const QString huge = QStringLiteral("MCP refused: Not in the design: ") + QString(7 * 1024 * 1024, QLatin1Char('o'));
  for (int i = 0; i < 5; ++i) {
    qInfo().noquote() << huge;
  }
  const qint64 before = residentKb();
  for (int i = 0; i < 200; ++i) {
    qInfo().noquote() << huge;
  }
  const qint64 after = residentKb();
  if (before >= 0 && after >= 0) {
    if (after - before >= 50 * 1024) {
      std::fprintf(stderr, "mcp_endpoint_limits.cpp: resident memory grew by %lld kB\n",
                   static_cast<long long>(after - before));
    }
    CHECK(after - before < 50 * 1024);
  }
  CHECK(mitcad::mcp::logRingBytes() <= 8U * 1024U * 1024U);
  const std::vector<LogLine> refused = logLines(start, 2000);
  CHECK(refused.size() == 205U);
  bool shortLines = true;
  for (const LogLine& line : refused) {
    shortLines = shortLines && formatLogLine(line).toUtf8().size() < kMaxLogLineBytes &&
                 line.text.toUtf8().size() < kMaxLogLineBytes &&
                 line.text.startsWith(QStringLiteral("MCP refused: Not in the design: ooo")) &&
                 line.text.endsWith(QStringLiteral(" more characters cut]"));
  }
  CHECK(shortLines);

  // Line breaks take the formatted line's indent, and characters beyond
  // ASCII their UTF-8 bytes: still under 4 KiB.
  qInfo().noquote() << QString(10000, QLatin1Char('\n'));
  qInfo().noquote() << QString(10000, QChar(u'\u00E4'));
  qInfo().noquote() << QString::fromUtf8("\xF0\x9F\x94\xA7").repeated(10000);
  logLines(0, 0, &next);
  for (const LogLine& line : logLines(next - 4, 3)) {
    CHECK(formatLogLine(line).toUtf8().size() < kMaxLogLineBytes);
    CHECK(line.text.endsWith(QStringLiteral(" more characters cut]")));
  }
  // A short line stays as it was.
  qInfo().noquote() << "a short line";
  const std::vector<LogLine> shortOne = logLines(next - 1, 1);
  CHECK(shortOne.size() == 1U && shortOne.front().text == QStringLiteral("a short line"));

  // Bounded by bytes before lines: 3000 long lines keep at most 8 MiB, the
  // older ones dropped.
  logLines(0, 0, &next);
  const quint64 floodStart = next - 1;
  for (int i = 0; i < 3000; ++i) {
    qInfo().noquote() << huge.left(100000);
  }
  bool dropped = false;
  const std::vector<LogLine> kept = logLines(floodStart, 5000, nullptr, &dropped);
  CHECK(mitcad::mcp::logRingBytes() <= 8U * 1024U * 1024U);
  CHECK(dropped);
  CHECK(!kept.empty() && kept.size() < 3000U);

  mitcad::mcp::removeLogRing();
  qInstallMessageHandler(previous);
}

} // namespace

int mcpEndpointLimitsTests() {
  echoTests();
  tokenTests();
  slotTests();
  ringTests();
  return failures;
}
