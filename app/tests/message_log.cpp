// SPDX-License-Identifier: MIT
// The message band's rules and the session's message history (mitcad#1042,
// docs/ui-conventions.md "The message band"): hints and information fade
// after 6 s, a warning after 10 s, an error stays until it is closed or the
// next command starts; a hint does not push a problem away; faded, the
// band is the stub of the latest message; the history is newest first,
// a repeated message one entry, at most kMaxEntries.
#include "framework/MessageLog.hpp"

#include <cstdio>

#include <QDate>
#include <QDateTime>
#include <QString>
#include <QTime>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "message_log.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QDateTime at(int second) { return QDateTime(QDate(2026, 11, 1), QTime(12, 0, 0).addSecs(second)); }

void lifetimeTests() {
  CHECK(MessageLog::lifetime(NoticeLevel::Hint) == 6000);
  CHECK(MessageLog::lifetime(NoticeLevel::Progress) == 6000);
  CHECK(MessageLog::lifetime(NoticeLevel::Information) == 6000);
  CHECK(MessageLog::lifetime(NoticeLevel::Success) == 6000);
  CHECK(MessageLog::lifetime(NoticeLevel::Warning) == 10000);
  CHECK(MessageLog::lifetime(NoticeLevel::Error) == 0); // until closed or the next command
}

void bandTests() {
  MessageLog log;
  CHECK(log.shown() == MessageLog::Shown::None);
  CHECK(log.full() == nullptr && log.latest() == nullptr);

  // A hint in full, then faded: the stub of it.
  CHECK(log.add(QStringLiteral("Done."), NoticeLevel::Hint, QString(), at(0)));
  CHECK(log.shown() == MessageLog::Shown::Full);
  CHECK(log.full()->text == QStringLiteral("Done."));
  CHECK(log.expire());
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(log.latest()->text == QStringLiteral("Done."));
  CHECK(!log.expire()); // nothing in full any more

  // An error stays: it does not time out, a hint after it goes to the
  // history only, a command started closes it.
  CHECK(log.add(QStringLiteral("Fillet2: too large"), NoticeLevel::Error, QStringLiteral("F5"), at(1)));
  CHECK(!log.expire());
  CHECK(log.shown() == MessageLog::Shown::Full);
  CHECK(!log.add(QStringLiteral("Pick a face"), NoticeLevel::Hint, QString(), at(2)));
  CHECK(log.full()->text == QStringLiteral("Fillet2: too large"));
  CHECK(log.latest()->text == QStringLiteral("Pick a face"));
  CHECK(!log.add(QStringLiteral("A warning"), NoticeLevel::Warning, QString(), at(3)));
  CHECK(log.full()->level == NoticeLevel::Error);
  // A cleared status line does not take an error away either.
  CHECK(!log.add(QString(), NoticeLevel::Hint, QString(), at(3)));
  CHECK(log.shown() == MessageLog::Shown::Full);
  CHECK(log.commandStarted());
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(log.latest()->text == QStringLiteral("A warning")); // the stub is the latest

  // A newer error replaces the one shown; closed by the user.
  CHECK(log.add(QStringLiteral("First"), NoticeLevel::Error, QString(), at(4)));
  CHECK(log.add(QStringLiteral("Second"), NoticeLevel::Error, QString(), at(5)));
  CHECK(log.full()->text == QStringLiteral("Second"));
  CHECK(log.close());
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(!log.close());

  // A warning: times out, a hint does not push it away, the next command
  // leaves it; an error after it does.
  CHECK(log.add(QStringLiteral("Overhangs"), NoticeLevel::Warning, QString(), at(6)));
  CHECK(!log.add(QStringLiteral("Done."), NoticeLevel::Hint, QString(), at(7)));
  CHECK(!log.commandStarted());
  CHECK(log.full()->text == QStringLiteral("Overhangs"));
  CHECK(log.add(QStringLiteral("Not written"), NoticeLevel::Error, QString(), at(8)));
  CHECK(log.close());
  CHECK(log.add(QStringLiteral("Overhangs"), NoticeLevel::Warning, QString(), at(9)));
  CHECK(log.expire());
  CHECK(log.shown() == MessageLog::Shown::Stub);

  // A hint replaces a hint; a cleared status line makes it the stub.
  CHECK(log.add(QStringLiteral("One"), NoticeLevel::Hint, QString(), at(10)));
  CHECK(log.add(QStringLiteral("Two"), NoticeLevel::Information, QString(), at(11)));
  CHECK(log.full()->text == QStringLiteral("Two"));
  CHECK(!log.add(QString(), NoticeLevel::Hint, QString(), at(12)));
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(log.latest()->text == QStringLiteral("Two"));
}

// Another design in the window (mitcad#1174): the band shows nothing, an
// error neither, nor the stub, until the next message; the history keeps
// them.
void clearTests() {
  MessageLog log;
  CHECK(!log.clear()); // nothing shown: nothing changes
  CHECK(log.shown() == MessageLog::Shown::None);

  // An error in full: gone, without its stub.
  CHECK(log.add(QStringLiteral("Nothing to pick there"), NoticeLevel::Error, QString(), at(0)));
  CHECK(log.clear());
  CHECK(log.shown() == MessageLog::Shown::None);
  CHECK(log.full() == nullptr && log.stub() == nullptr);
  CHECK(log.latest()->text == QStringLiteral("Nothing to pick there")); // in the history
  CHECK(log.size() == 1);
  CHECK(!log.expire() && !log.close() && !log.commandStarted());
  // A cleared status line brings no stub back.
  CHECK(!log.add(QString(), NoticeLevel::Hint, QString(), at(1)));
  CHECK(log.shown() == MessageLog::Shown::None);

  // The next message is shown in full, then its stub as before.
  CHECK(log.add(QStringLiteral("Create a sketch"), NoticeLevel::Hint, QString(), at(2)));
  CHECK(log.shown() == MessageLog::Shown::Full);
  CHECK(log.expire());
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(log.stub()->text == QStringLiteral("Create a sketch"));

  // A stub: gone too.
  CHECK(log.clear());
  CHECK(log.shown() == MessageLog::Shown::None);
  CHECK(!log.clear());
  CHECK(log.size() == 2);
}

// A withdrawn message (mitcad#1230): the warning shown in full with that
// text becomes the stub, a warning too; another text, or a message no
// longer shown in full, changes nothing; the history keeps it.
void withdrawTests() {
  MessageLog log;
  const QString warning = QStringLiteral("Section1 could not cut Body1; it is shown whole.");
  CHECK(!log.withdraw(warning)); // nothing shown
  const QString details = QStringLiteral("Section1: Body1: the boolean failed");
  CHECK(log.add(details, NoticeLevel::Information, QString(), at(0)));
  CHECK(log.add(warning, NoticeLevel::Warning, QString(), at(0)));
  CHECK(!log.withdraw(QStringLiteral("Another warning.")));
  CHECK(!log.withdraw(QString()));
  CHECK(log.shown() == MessageLog::Shown::Full);
  CHECK(log.withdraw(warning));
  CHECK(log.shown() == MessageLog::Shown::Stub);
  CHECK(log.stub()->text == warning);
  CHECK(!log.withdraw(warning)); // already the stub
  CHECK(log.size() == 2);
  CHECK(log.entries().at(1).text == details);
  // A newer message shown in full is not the warning: it stays.
  CHECK(log.add(warning, NoticeLevel::Warning, QString(), at(1)));
  CHECK(log.add(QStringLiteral("Nothing to pick there"), NoticeLevel::Error, QString(), at(2)));
  CHECK(!log.withdraw(warning));
  CHECK(log.full()->text == QStringLiteral("Nothing to pick there"));
}

void historyTests() {
  MessageLog log;
  log.add(QStringLiteral("Fillet2: too large"), NoticeLevel::Error, QStringLiteral("F5"), at(0));
  log.add(QStringLiteral("Done."), NoticeLevel::Hint, QString(), at(1));
  log.add(QStringLiteral("Done."), NoticeLevel::Hint, QString(), at(2)); // repeated: counted
  log.add(QString(), NoticeLevel::Hint, QString(), at(3));               // nothing recorded
  CHECK(log.size() == 2);
  // Newest first, with its time, level, text and feature.
  CHECK(log.entries().at(0).text == QStringLiteral("Done."));
  CHECK(log.entries().at(0).count == 2);
  CHECK(log.entries().at(0).time == at(2));
  CHECK(log.entries().at(1).level == NoticeLevel::Error);
  CHECK(log.entries().at(1).feature == QStringLiteral("F5"));
  CHECK(log.entries().at(1).count == 1);
  // The same text at another level is another entry.
  log.add(QStringLiteral("Done."), NoticeLevel::Information, QString(), at(4));
  CHECK(log.size() == 3);
  // At most kMaxEntries, the oldest dropped.
  for (int i = 0; i < MessageLog::kMaxEntries + 10; ++i) {
    log.add(QStringLiteral("Message %1").arg(i), NoticeLevel::Information, QString(), at(10 + i));
  }
  CHECK(log.size() == MessageLog::kMaxEntries);
  CHECK(log.entries().front().text == QStringLiteral("Message %1").arg(MessageLog::kMaxEntries + 9));
  CHECK(log.entries().back().text == QStringLiteral("Message 10"));
}

} // namespace

int messageLogTests() {
  failures = 0;
  lifetimeTests();
  bandTests();
  clearTests();
  withdrawTests();
  historyTests();
  return failures;
}
