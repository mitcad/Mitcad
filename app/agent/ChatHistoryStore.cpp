// SPDX-License-Identifier: MIT
#include "ChatHistoryStore.hpp"

#include <utility>

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QStringList>

namespace mitcad::agent {
namespace {

const QString kSuffix = QStringLiteral(".jsonl");

bool privateFolder(const QString& directory) {
  if (directory.isEmpty() || !QDir().mkpath(directory)) {
    return false;
  }
  QFile::setPermissions(directory, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
  return true;
}

} // namespace

ChatHistoryStore::ChatHistoryStore(QString directory) : m_directory(std::move(directory)) {}

QString ChatHistoryStore::keyFor(const QString& designPath) {
  QString path = QDir::cleanPath(QFileInfo(designPath).absoluteFilePath());
#ifdef Q_OS_WIN
  path = path.toLower();
#endif
  return QString::fromLatin1(QCryptographicHash::hash(path.toUtf8(), QCryptographicHash::Sha256).toHex());
}

QString ChatHistoryStore::fileFor(const QString& designPath) const {
  if (m_directory.isEmpty() || designPath.isEmpty()) {
    return QString();
  }
  return QDir(m_directory).filePath(keyFor(designPath) + kSuffix);
}

QByteArray ChatHistoryStore::read(const QString& designPath, qint64 limit) const {
  QFile file(fileFor(designPath));
  if (file.fileName().isEmpty() || !file.open(QIODevice::ReadOnly)) {
    return QByteArray();
  }
  const qint64 size = file.size();
  if (size <= limit) {
    return file.readAll();
  }
  // The newest lines: from the first line that starts within the limit.
  file.seek(size - limit);
  QByteArray tail = file.read(limit);
  const qsizetype newline = tail.indexOf('\n');
  return newline < 0 ? QByteArray() : tail.mid(newline + 1);
}

bool ChatHistoryStore::append(const QString& designPath, const QByteArray& lines) const {
  if (lines.isEmpty()) {
    return true;
  }
  if (!privateFolder(m_directory)) {
    return false;
  }
  QFile file(fileFor(designPath));
  if (file.fileName().isEmpty() || !file.open(QIODevice::WriteOnly | QIODevice::Append)) {
    return false;
  }
  file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
  return file.write(lines) == lines.size();
}

bool ChatHistoryStore::move(const QString& fromPath, const QString& toPath) const {
  const QString from = fileFor(fromPath);
  const QString to = fileFor(toPath);
  if (from.isEmpty() || to.isEmpty() || from == to || !QFileInfo::exists(from)) {
    return true;
  }
  if (!QFileInfo::exists(to)) {
    return QFile::rename(from, to);
  }
  QFile source(from);
  if (!source.open(QIODevice::ReadOnly)) {
    return false;
  }
  const QByteArray content = source.readAll();
  source.close();
  return append(toPath, content) && QFile::remove(from);
}

bool ChatHistoryStore::clear(const QString& designPath) const {
  const QString path = fileFor(designPath);
  return path.isEmpty() || !QFileInfo::exists(path) || QFile::remove(path);
}

bool ChatHistoryStore::clearAll() const {
  if (m_directory.isEmpty()) {
    return true;
  }
  QDir folder(m_directory);
  bool cleared = true;
  const QStringList files = folder.entryList({QStringLiteral("*") + kSuffix}, QDir::Files);
  for (const QString& name : files) {
    cleared = folder.remove(name) && cleared;
  }
  return cleared;
}

} // namespace mitcad::agent
