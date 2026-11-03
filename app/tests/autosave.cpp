// SPDX-License-Identifier: MIT
// Publish real snapshots and read them through the normal recovery path.
#include "files/Autosave.hpp"
#include "files/Recovery.hpp"
#include "framework/AppSettings.hpp"

#include <QDir>
#include <QCoreApplication>
#include <QElapsedTimer>
#include <QThread>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLockFile>
#include <QSaveFile>
#include <QTemporaryDir>
#include <QUuid>

#include <cstdio>

int autosaveTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "autosave: %s\n", message);
      ++failures;
    }
  };
  const auto read = [](const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
  };
  const auto write = [](const QString& path, const QByteArray& data, QString&) {
    QSaveFile file(path);
    return file.open(QIODevice::WriteOnly) && file.write(data) == data.size() && file.commit();
  };
  QTemporaryDir folder;
  check(folder.isValid(), "temporary folder");
  const QDir dir(folder.path());
  const QString session = QUuid::createUuid().toString(QUuid::WithoutBraces);
  const QString other = QUuid::createUuid().toString(QUuid::WithoutBraces);
  const QJsonObject about{{QStringLiteral("format"), QStringLiteral("mitcad-autosave")},
                          {QStringLiteral("session"), session},
                          {QStringLiteral("document"), QStringLiteral("Untitled")}};
  QString error;
  const QByteArray first(
      R"({"format":"mitcad","version":2,"parameters":[{"name":"Width","expression":"10 mm","unit":"mm","value":10}],"features":[]})");
  const QByteArray second(
      R"({"format":"mitcad","version":2,"parameters":[{"name":"Width","expression":"20 mm","unit":"mm","value":20}],"features":[]})");
  check(publishAutosave(folder.path(), session, first, about, error), "first generation published");
  const QByteArray metadata = read(dir.filePath(session + QStringLiteral(".json")));
  QString oldSnapshot;
  {
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && !found[0].damaged(), "first snapshot recoverable");
    if (!found.empty()) {
      oldSnapshot = found[0].designFile();
      check(read(oldSnapshot) == first, "normal recovery names the first bytes");
    }
  }
  // Both data-write failure and interruption after data publication leave
  // the old metadata pointer and bytes untouched. The latter leaves an
  // orphan on disk exactly as a stopped process would.
  for (bool failMetadata : {false, true}) {
    const AutosaveWriter fail = [&](const QString& path, const QByteArray& bytes, QString& reason) {
      if (path.endsWith(QStringLiteral(".json")) == failMetadata) {
        reason = QStringLiteral("injected write failure");
        return false;
      }
      return write(path, bytes, reason);
    };
    check(!publishAutosave(folder.path(), session, second, about, error, fail), "injected update fails");
    check(read(dir.filePath(session + QStringLiteral(".json"))) == metadata, "old metadata stays");
    check(read(oldSnapshot) == first, "old immutable snapshot stays");
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && !found[0].damaged(), "failed update is recoverable, one session");
    if (!found.empty()) {
      check(read(found[0].designFile()) == first, "recovery reads old generation after failure");
    }
  }
  check(publishAutosave(folder.path(), session, second, about, error), "successful replacement");
  check(!QFile::exists(oldSnapshot), "obsolete snapshot removed after publication");
  check(dir.entryList({session + QStringLiteral("*.mitcad")}, QDir::Files).size() == 1,
        "orphan and obsolete generations cleaned");
  {
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && !found[0].damaged(), "updated generation recoverable");
    if (!found.empty()) {
      check(read(found[0].designFile()) == second, "recovery reads updated bytes");
      discardSession(found[0]);
    }
  }
  check(dir.entryList(QDir::Files | QDir::Hidden).isEmpty(), "discard removes session files and lock");

  // Existing version 1 autosaves still recover and upgrade atomically.
  QJsonObject legacy = about;
  legacy.insert(QStringLiteral("version"), 1);
  legacy.insert(QStringLiteral("project"), session + QStringLiteral(".mitcad"));
  legacy.insert(QStringLiteral("size"), static_cast<qint64>(first.size()));
  legacy.insert(QStringLiteral("digest"), fileDigest(first));
  check(write(dir.filePath(session + QStringLiteral(".mitcad")), first, error), "legacy snapshot");
  check(write(dir.filePath(session + QStringLiteral(".json")), QJsonDocument(legacy).toJson(), error), "legacy metadata");
  {
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && !found[0].damaged() && read(found[0].designFile()) == first, "v1 recovery");
  }
  const AutosaveWriter failUpgrade = [&](const QString& path, const QByteArray& bytes, QString& reason) {
    return !path.endsWith(QStringLiteral(".json")) && write(path, bytes, reason);
  };
  check(!publishAutosave(folder.path(), session, second, about, error, failUpgrade), "failed v1 upgrade");
  {
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && !found[0].damaged() && read(found[0].designFile()) == first,
          "v1 remains recoverable after interrupted upgrade");
  }
  check(publishAutosave(folder.path(), session, second, about, error), "v1 upgrade");
  check(!QFile::exists(dir.filePath(session + QStringLiteral(".mitcad"))), "legacy snapshot cleaned after upgrade");
  check(publishAutosave(folder.path(), other, first, about, error), "another session");
  // Traversal, other-session pointers and unknown generations are rejected
  // before opening any metadata-named file.
  for (const QString& bad : {QStringLiteral("../outside.mitcad"),
                             dir.filePath(session + QStringLiteral(".mitcad")),
                             other + QStringLiteral(".mitcad"),
                             session + QStringLiteral(".invalid.mitcad")}) {
    QJsonObject unsafe = QJsonDocument::fromJson(read(dir.filePath(session + QStringLiteral(".json")))).object();
    unsafe.insert(QStringLiteral("project"), bad);
    check(write(dir.filePath(session + QStringLiteral(".json")), QJsonDocument(unsafe).toJson(), error), "bad metadata fixture");
    auto found = findRecoverable(folder.path(), other);
    check(found.size() == 1 && found[0].damaged(), "unsafe pointer rejected");
  }
  removeSessionFiles(folder.path(), session);
  {
    auto found = findRecoverable(folder.path(), QString());
    check(found.size() == 1 && found[0].session == other && !found[0].damaged(), "cleanup preserves another session");
    if (!found.empty()) {
      discardSession(found[0]);
    }
  }
  check(dir.entryList(QDir::Files | QDir::Hidden).isEmpty(), "final cleanup");

  // Two documents in one process own different records and live locks.
  // Closing or replacing one must not discard the other's work.
  const bool hadDirectory = qEnvironmentVariableIsSet("MITCAD_AUTOSAVE_DIR");
  const QByteArray oldDirectory = qgetenv("MITCAD_AUTOSAVE_DIR");
  qputenv("MITCAD_AUTOSAVE_DIR", folder.path().toUtf8());
  {
    AutosaveManager left;
    AutosaveManager right;
    check(left.session() != right.session(), "document sessions have distinct UUIDs");
    left.state = right.state = [] { return AutosaveManager::State{true, 1}; };
    left.snapshot = [&] { return AutosaveManager::Snapshot{first, QStringLiteral("Left"), QString()}; };
    right.snapshot = [&] { return AutosaveManager::Snapshot{second, QStringLiteral("Right"), QString()}; };
    GeneralSettings settings;
    settings.autosave = true;
    left.apply(settings);
    right.apply(settings);
    left.saveSoon();
    right.saveSoon();
    const auto waitFor = [&](const auto& ready) {
      QElapsedTimer elapsed;
      elapsed.start();
      while (!ready() && elapsed.elapsed() < 10000) {
        QCoreApplication::processEvents();
        QThread::msleep(1);
      }
      return ready();
    };
    const QString leftRecord = dir.filePath(left.session() + QStringLiteral(".json"));
    const QString rightRecord = dir.filePath(right.session() + QStringLiteral(".json"));
    check(waitFor([&] { return QFile::exists(leftRecord) && QFile::exists(rightRecord); }),
          "both real managers publish independently");
    check(findRecoverable(folder.path(), QString()).empty(), "both live document locks exclude recovery");
    left.replaced();
    check(!QFile::exists(leftRecord) && QFile::exists(rightRecord), "replacing left preserves right record");
    check(findRecoverable(folder.path(), QString()).empty(), "right lock survives replacing left");
    const auto record = QJsonDocument::fromJson(read(rightRecord)).object();
    check(read(dir.filePath(record.value(QStringLiteral("project")).toString())) == second,
          "right snapshot retains its own document bytes");
  }
  check(dir.entryList(QDir::Files | QDir::Hidden).isEmpty(), "normal document close removes both owned sessions");
  if (hadDirectory) qputenv("MITCAD_AUTOSAVE_DIR", oldDirectory);
  else qunsetenv("MITCAD_AUTOSAVE_DIR");
  return failures;
}
