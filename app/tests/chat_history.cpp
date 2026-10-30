// SPDX-License-Identifier: MIT
// The chat's history files and settings (mitcad#947): a design's file is
// named by its path's digest, appends keep what was there, the end of a
// large file is read from a line's start, the history moves with its design
// (also onto one that has a history), clearing one design leaves the others;
// the approval modes' names, Always allow per agent and its reset, the
// privacy notice per agent.
#include "agent/ChatHistoryStore.hpp"
#include "agent/ChatSettings.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "chat_history.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int chatHistoryTests() {
  using mitcad::agent::ChatHistoryStore;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString folder = QDir(directory.path()).filePath(QStringLiteral("chat"));
  const ChatHistoryStore store(folder);
  const QString design = QDir(directory.path()).filePath(QStringLiteral("designs/box.mitcad"));
  const QString other = QDir(directory.path()).filePath(QStringLiteral("designs/plate.mitcad"));

  // Keys: the digest of the cleaned absolute path; no path in the name.
  const QString key = ChatHistoryStore::keyFor(design);
  CHECK(key.size() == 64);
  CHECK(key == ChatHistoryStore::keyFor(QDir(directory.path()).filePath(QStringLiteral("designs/../designs/box.mitcad"))));
  CHECK(key != ChatHistoryStore::keyFor(other));
  CHECK(!store.fileFor(design).contains(QStringLiteral("box")));
  CHECK(store.fileFor(design).endsWith(QStringLiteral(".jsonl")));
  CHECK(store.fileFor(QString()).isEmpty());
  CHECK(ChatHistoryStore(QString()).fileFor(design).isEmpty());

  // Nothing yet; appends keep the lines in order; the folder is made.
  CHECK(store.read(design).isEmpty());
  CHECK(store.append(design, "{\"v\":1,\"role\":\"user\",\"text\":\"one\"}\n"));
  CHECK(store.append(design, "{\"v\":1,\"role\":\"assistant\",\"text\":\"two\"}\n"));
  CHECK(QFileInfo(folder).isDir());
  CHECK(store.read(design) ==
        QByteArray("{\"v\":1,\"role\":\"user\",\"text\":\"one\"}\n{\"v\":1,\"role\":\"assistant\",\"text\":\"two\"}\n"));
#ifndef Q_OS_WIN
  CHECK(QFile::permissions(store.fileFor(design)) == (QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ReadUser |
                                                       QFileDevice::WriteUser));
#endif

  // The end of a file past the limit: from the first whole line.
  CHECK(store.append(other, QByteArray("0123456789\n").repeated(10)));
  const QByteArray tail = store.read(other, 25);
  CHECK(tail == QByteArray("0123456789\n0123456789\n"));

  // The history moves with its design (Save As), after what is there.
  const QString moved = QDir(directory.path()).filePath(QStringLiteral("designs/box copy.mitcad"));
  CHECK(store.move(design, moved));
  CHECK(store.read(design).isEmpty());
  CHECK(store.read(moved).startsWith("{\"v\":1,\"role\":\"user\""));
  CHECK(store.append(design, "{\"v\":1,\"role\":\"user\",\"text\":\"three\"}\n"));
  CHECK(store.move(design, moved));
  CHECK(store.read(moved).endsWith("\"three\"}\n"));
  CHECK(!QFileInfo::exists(store.fileFor(design)));
  // Nothing to move: no error.
  CHECK(store.move(design, other));

  // Clearing one design leaves the others; all clears every history only.
  CHECK(store.clear(moved));
  CHECK(store.read(moved).isEmpty());
  CHECK(!store.read(other).isEmpty());
  QFile unrelated(QDir(folder).filePath(QStringLiteral("notes.txt")));
  CHECK(unrelated.open(QIODevice::WriteOnly));
  unrelated.close();
  CHECK(store.clearAll());
  CHECK(store.read(other).isEmpty());
  CHECK(QFileInfo::exists(unrelated.fileName()));

  // Settings in a file of the test's own, the application's restored after.
  using namespace mitcad::agent;
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadChatTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadChatTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(!privacyNoticeShown(QStringLiteral("claude")));
  setPrivacyNoticeShown(QStringLiteral("claude"));
  CHECK(privacyNoticeShown(QStringLiteral("claude")));
  CHECK(!privacyNoticeShown(QStringLiteral("codex")));
  CHECK(approvalModeKey(ApprovalMode::Ask) == QStringLiteral("ask"));
  CHECK(approvalModeKey(ApprovalMode::ModelEdits) == QStringLiteral("model"));
  CHECK(approvalModeKey(ApprovalMode::Everything) == QStringLiteral("all"));
  CHECK(approvalModeFromKey(QStringLiteral("all")) == ApprovalMode::Everything);
  CHECK(approvalModeFromKey(QStringLiteral("unknown")) == ApprovalMode::ModelEdits);
  resetAlwaysAllowed();
  CHECK(!anyAlwaysAllowed());
  allowAlways(QStringLiteral("claude"), QStringLiteral("document_save"));
  allowAlways(QStringLiteral("claude"), QStringLiteral("document_save"));
  CHECK(alwaysAllowed(QStringLiteral("claude")) == QStringList{QStringLiteral("document_save")});
  CHECK(alwaysAllowed(QStringLiteral("codex")).isEmpty());
  CHECK(anyAlwaysAllowed());
  resetAlwaysAllowed();
  CHECK(alwaysAllowed(QStringLiteral("claude")).isEmpty());
  CHECK(!anyAlwaysAllowed());
  ChatSettings chosen = ChatSettings::load();
  chosen.approvals = ApprovalMode::Ask;
  chosen.history = false;
  chosen.save();
  CHECK(ChatSettings::load() == chosen);
  ChatSettings defaults;
  defaults.save();
  CHECK(ChatSettings::load().approvals == ApprovalMode::ModelEdits);
  {
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
