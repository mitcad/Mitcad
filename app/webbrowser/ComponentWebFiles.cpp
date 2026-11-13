// SPDX-License-Identifier: MIT
// Insert › Component from Web's decisions without the browser (mitcad#1579):
// plain Qt Core, see ComponentWebFiles.hpp.
#include "ComponentWebFiles.hpp"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLockFile>
#include <QSettings>
#include <QStandardPaths>
#include <QtGlobal>

#include "files/FileFormats.hpp"

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(ComponentWebFiles)
};

const QString kSitesKey = QStringLiteral("componentBrowser/sites");
const QString kRunPrefix = QStringLiteral("run-");
const QString kLockName = QStringLiteral(".lock");

WebFileCheck refused(const QString& note) { return {WebFileKind::Refused, note}; }

// The bytes after a UTF-8 byte order mark and leading white space, lower
// case, for the text signatures.
QByteArray textStart(const QByteArray& head) {
  QByteArray start = head.left(1024);
  if (start.startsWith("\xEF\xBB\xBF")) {
    start.remove(0, 3);
  }
  return start.trimmed().toLower();
}

bool isWebPage(const QByteArray& start) {
  return start.startsWith("<!doctype html") || start.startsWith("<html") ||
         (start.startsWith('<') && start.contains("<html"));
}

} // namespace

WebFileCheck checkWebFileName(const QString& fileName) {
  const QString suffix = QFileInfo(fileName).suffix().toLower();
  if (suffix == QStringLiteral("zip")) {
    return {WebFileKind::Zip, QString()};
  }
  switch (fileKind(fileName)) {
  case FileKind::Design:
  case FileKind::Cad:
  case FileKind::Mesh:
    return {WebFileKind::Component, QString()};
  case FileKind::F3d:
  case FileKind::FreeCad:
  case FileKind::Ipt:
  case FileKind::Ifc:
  case FileKind::Rvt:
  case FileKind::Solidcraft:
  case FileKind::Dwg:
  case FileKind::ThreeDm:
    return {WebFileKind::Document, QString()};
  case FileKind::Drawing:
    return refused(Text::tr("MitCad brings DXF and SVG drawings into sketches, not in as components (.%1).")
                       .arg(suffix));
  case FileKind::None:
    break;
  }
  if (suffix.isEmpty()) {
    return refused(Text::tr("MitCad cannot import files without an extension."));
  }
  if (suffix == QStringLiteral("pdf")) {
    return refused(Text::tr("MitCad cannot import PDF documents (.pdf)."));
  }
  if (suffix == QStringLiteral("3mf")) {
    return refused(Text::tr("MitCad cannot import 3MF files as components (.3mf)."));
  }
  return refused(Text::tr("MitCad cannot import .%1 files.").arg(suffix));
}

WebFileCheck checkWebFileContent(const QString& fileName, const QByteArray& head) {
  WebFileCheck check = checkWebFileName(fileName);
  if (check.kind == WebFileKind::Refused) {
    return check;
  }
  const QString name = QFileInfo(fileName).fileName();
  if (head.isEmpty()) {
    return refused(Text::tr("%1 is empty.").arg(name));
  }
  const QByteArray start = textStart(head);
  if (isWebPage(start)) {
    return refused(Text::tr("%1 is a web page, not a model file: the site may want you to sign in first.").arg(name));
  }
  if (head.startsWith("%PDF-")) {
    return refused(Text::tr("%1 is a PDF document, not a model file.").arg(name));
  }
  const QString suffix = QFileInfo(fileName).suffix().toLower();
  if (check.kind == WebFileKind::Zip && !head.startsWith("PK\x03\x04") && !head.startsWith("PK\x05\x06")) {
    return refused(Text::tr("%1 is not a zip file.").arg(name));
  }
  if ((suffix == QStringLiteral("step") || suffix == QStringLiteral("stp")) && !start.startsWith("iso-10303-21")) {
    return refused(Text::tr("%1 is not a STEP file: it has no ISO-10303-21 header.").arg(name));
  }
  return check;
}

QString webComponentName(const QString& fileName) {
  const QString base = QFileInfo(fileName).fileName();
  const qsizetype dot = base.lastIndexOf(QLatin1Char('.'));
  return (dot > 0 ? base.left(dot) : base).trimmed();
}

QString webPartNumber(const QString& componentName) {
  const QString name = componentName.trimmed();
  qsizetype end = name.size();
  for (qsizetype i = 0; i < name.size(); ++i) {
    if (name.at(i) == QLatin1Char('_') || name.at(i).isSpace()) {
      end = i;
      break;
    }
  }
  const QString first = name.left(end);
  bool digit = false;
  for (const QChar c : first) {
    if (!c.isLetterOrNumber()) {
      return QString();
    }
    digit = digit || c.isDigit();
  }
  return digit ? first : QString();
}

QString webDownloadFailure(int interruptReason) {
  // Chromium's download interrupt reasons (CEF's
  // cef_download_interrupt_reason_t), grouped by what the user can do
  // about them: the file, the connection, the server, the user.
  switch (interruptReason) {
  case 2: // FILE_ACCESS_DENIED
    return Text::tr("MitCad may not write into its download folder");
  case 3: // FILE_NO_SPACE
  case 6: // FILE_TOO_LARGE
    return Text::tr("the disk is full");
  case 5: // FILE_NAME_TOO_LONG
    return Text::tr("the file's name is too long");
  case 7:  // FILE_VIRUS_INFECTED
  case 11: // FILE_BLOCKED
  case 12: // FILE_SECURITY_CHECK_FAILED
    return Text::tr("the system's security check blocked the file");
  case 1:  // FILE_FAILED
  case 10: // FILE_TRANSIENT_ERROR
  case 13: // FILE_TOO_SHORT
  case 14: // FILE_HASH_MISMATCH
  case 15: // FILE_SAME_AS_SOURCE
    return Text::tr("the file could not be written");
  case 20: // NETWORK_FAILED
  case 22: // NETWORK_DISCONNECTED
    return Text::tr("the connection was lost");
  case 21: // NETWORK_TIMEOUT
    return Text::tr("the connection timed out");
  case 23: // NETWORK_SERVER_DOWN
  case 37: // SERVER_UNREACHABLE
    return Text::tr("the server could not be reached");
  case 24: // NETWORK_INVALID_REQUEST
    return Text::tr("the network refused the request");
  case 30: // SERVER_FAILED
    return Text::tr("the server reported an error");
  case 31: // SERVER_NO_RANGE
  case 38: // SERVER_CONTENT_LENGTH_MISMATCH
    return Text::tr("the server sent the file incomplete");
  case 33: // SERVER_BAD_CONTENT
    return Text::tr("the server does not have the file");
  case 34: // SERVER_UNAUTHORIZED
    return Text::tr("the server refused it: the site may want you to sign in first");
  case 35: // SERVER_CERT_PROBLEM
    return Text::tr("the server's certificate is not trusted");
  case 36: // SERVER_FORBIDDEN
    return Text::tr("the server refused it");
  case 39: // SERVER_CROSS_ORIGIN_REDIRECT
    return Text::tr("the server sent it on to another site");
  case 40: // USER_CANCELED
    return Text::tr("it was canceled");
  case 41: // USER_SHUTDOWN
    return Text::tr("the browser was closing");
  case 50: // CRASH
    return Text::tr("the browser stopped unexpectedly");
  default: // NONE and any reason a later browser adds
    return Text::tr("it stopped before the end");
  }
}

QList<WebSite> loadWebSites() {
  QList<WebSite> sites;
  const QJsonArray list = QJsonDocument::fromJson(QSettings().value(kSitesKey).toByteArray()).array();
  for (const QJsonValue& value : list) {
    const QJsonObject site = value.toObject();
    const QString url = site.value(QStringLiteral("url")).toString().trimmed();
    if (!url.isEmpty()) {
      const QString name = site.value(QStringLiteral("name")).toString().trimmed();
      sites.append({name.isEmpty() ? url : name, url});
    }
  }
  return sites;
}

void saveWebSites(const QList<WebSite>& sites) {
  QJsonArray list;
  for (const WebSite& site : sites) {
    list.append(QJsonObject{{QStringLiteral("name"), site.name}, {QStringLiteral("url"), site.url}});
  }
  QSettings().setValue(kSitesKey, QJsonDocument(list).toJson(QJsonDocument::Compact));
}

QString normalWebAddress(const QString& typed) {
  const QString text = typed.trimmed();
  if (text.isEmpty()) {
    return QString();
  }
  if (text.contains(QStringLiteral("://")) || text.startsWith(QStringLiteral("about:"), Qt::CaseInsensitive)) {
    return text;
  }
  return QStringLiteral("https://") + text;
}

// ---------------------------------------------------------------------------
// The downloads' folders

WebDownloadFolders::WebDownloadFolders() = default;

WebDownloadFolders::~WebDownloadFolders() { removeAll(); }

QString WebDownloadFolders::root() {
  const QString given = qEnvironmentVariable("MITCAD_WEB_DOWNLOADS_DIR");
  if (!given.isEmpty()) {
    return QDir::cleanPath(given);
  }
  return QDir(QStandardPaths::writableLocation(QStandardPaths::TempLocation))
      .filePath(QStringLiteral("mitcad-web-downloads"));
}

int WebDownloadFolders::removeLeftovers() {
  int removed = 0;
  const QDir root(WebDownloadFolders::root());
  if (!root.exists()) {
    return 0;
  }
  const QStringList runs =
      root.entryList({kRunPrefix + QLatin1Char('*')}, QDir::Dirs | QDir::NoDotAndDotDot | QDir::Hidden);
  for (const QString& run : runs) {
    const QString folder = root.filePath(run);
    // A run's lock is taken only when the MitCad that made it no longer
    // runs (its process is gone), never by its age: a run lasts for hours.
    QLockFile lock(QDir(folder).filePath(kLockName));
    lock.setStaleLockTime(0);
    if (!lock.tryLock(0)) {
      continue;
    }
    lock.unlock();
    if (QDir(folder).removeRecursively()) {
      ++removed;
    }
  }
  return removed;
}

QString WebDownloadFolders::runFolder() {
  if (!m_folder.isEmpty()) {
    return m_folder;
  }
  const QDir root(WebDownloadFolders::root());
  if (!root.mkpath(QStringLiteral("."))) {
    return QString();
  }
  const QString name = QStringLiteral("%1%2-%3")
                           .arg(kRunPrefix)
                           .arg(QCoreApplication::applicationPid())
                           .arg(QDateTime::currentMSecsSinceEpoch());
  const QString folder = root.filePath(name);
  if (!QDir().mkpath(folder)) {
    return QString();
  }
  auto lock = std::make_unique<QLockFile>(QDir(folder).filePath(kLockName));
  lock->setStaleLockTime(0);
  if (!lock->tryLock(0)) {
    return QString();
  }
  m_lock = std::move(lock);
  m_folder = folder;
  return m_folder;
}

QString WebDownloadFolders::newDownloadFolder() {
  const QString run = runFolder();
  if (run.isEmpty()) {
    return QString();
  }
  QString folder;
  do {
    folder = QDir(run).filePath(QStringLiteral("d%1").arg(++m_next));
  } while (QFileInfo::exists(folder));
  return QDir().mkpath(folder) ? folder : QString();
}

void WebDownloadFolders::remove(const QString& folder) {
  if (m_folder.isEmpty() || folder.isEmpty()) {
    return;
  }
  const QString clean = QDir::cleanPath(folder);
  if (clean.startsWith(m_folder + QLatin1Char('/')) && clean != m_folder) {
    QDir(clean).removeRecursively();
  }
}

void WebDownloadFolders::removeAll() {
  if (m_folder.isEmpty()) {
    return;
  }
  if (m_lock) {
    m_lock->unlock();
    m_lock.reset();
  }
  QDir(m_folder).removeRecursively();
  m_folder.clear();
}

} // namespace mitcad
