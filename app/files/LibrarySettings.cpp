// SPDX-License-Identifier: MIT
// The library sources the user chose (settings libraries/sources): plain Qt
// Core, so that the unit tests (app/tests/library_settings.cpp) use it
// without the bridge.
#include "Libraries.hpp"
#include "framework/ModelMessages.hpp"

#include <mutex>

#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QSettings>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kSourcesKey = QStringLiteral("libraries/sources");
const QString kUseGitKey = QStringLiteral("libraries/useGit");

} // namespace

QList<LibrarySource> defaultLibrarySources() {
  // Where Mitcad's own repositories are published; fetched only when the
  // user asks (Auto-update is off).
  return {{QStringLiteral("https://github.com/mitcad/parts.git"), true, false},
          {QStringLiteral("https://github.com/mitcad/community-index.git"), true, false}};
}

QString normalLibraryUrl(const QString& url) {
  QString out = QDir::fromNativeSeparators(url.trimmed());
  while (out.endsWith(QLatin1Char('/'))) {
    out.chop(1);
  }
  return out;
}

LibrarySettings LibrarySettings::load() {
  LibrarySettings settings;
  QSettings store;
  settings.useGit = store.value(kUseGitKey, false).toBool();
  if (!store.contains(kSourcesKey)) {
    settings.sources = defaultLibrarySources();
    return settings;
  }
  const QJsonArray list = QJsonDocument::fromJson(store.value(kSourcesKey).toByteArray()).array();
  for (const QJsonValue& value : list) {
    const QJsonObject source = value.toObject();
    const QString url = source.value(QStringLiteral("url")).toString().trimmed();
    if (!url.isEmpty()) {
      // Settings written before Auto-update (mitcad#576) have no key: off.
      settings.sources.append({url, source.value(QStringLiteral("enabled")).toBool(true),
                               source.value(QStringLiteral("autoUpdate")).toBool(false)});
    }
  }
  return settings;
}

void LibrarySettings::save() const {
  QJsonArray list;
  for (const LibrarySource& source : sources) {
    list.append(QJsonObject{{QStringLiteral("url"), source.url},
                            {QStringLiteral("enabled"), source.enabled},
                            {QStringLiteral("autoUpdate"), source.autoUpdate}});
  }
  QSettings store;
  store.setValue(kSourcesKey, QJsonDocument(list).toJson(QJsonDocument::Compact));
  store.setValue(kUseGitKey, useGit);
}

QJsonObject libraryFetchCommand(const QString& url) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("library_fetch")}, {QStringLiteral("url"), url}};
  if (LibrarySettings::load().useGit) {
    command.insert(QStringLiteral("use_git"), true);
  }
  return command;
}

QStringList LibrarySettings::enabledUrls() const {
  QStringList urls;
  for (const LibrarySource& source : sources) {
    if (source.enabled) {
      urls << source.url;
    }
  }
  return urls;
}

QStringList LibrarySettings::autoUpdateUrls() const {
  QStringList urls;
  for (const LibrarySource& source : sources) {
    if (source.enabled && source.autoUpdate) {
      urls << source.url;
    }
  }
  return urls;
}

namespace {

// "host[:port]/path" without user information and the host's leading
// "www.".
QString hostAndPath(const QString& rest) {
  const int slash = static_cast<int>(rest.indexOf(QLatin1Char('/')));
  QString host = slash < 0 ? rest : rest.left(slash);
  const QString path = slash < 0 ? QString() : rest.mid(slash);
  const int at = static_cast<int>(host.lastIndexOf(QLatin1Char('@')));
  if (at >= 0) {
    host = host.mid(at + 1);
  }
  if (host.startsWith(QStringLiteral("www."))) {
    host = host.mid(4);
  }
  return host + path;
}

void chopSlashes(QString& text) {
  while (text.endsWith(QLatin1Char('/'))) {
    text.chop(1);
  }
}

} // namespace

QString repositoryKey(const QString& url) {
  QString text = url.trimmed();
  text.replace(QLatin1Char('\\'), QLatin1Char('/'));
  text = text.toLower();
  QString named = text;
  const int scheme = static_cast<int>(text.indexOf(QStringLiteral("://")));
  if (text.startsWith(QStringLiteral("file://"))) {
    // file:///C:/library is the folder C:/library.
    named = text.mid(7);
    if (named.size() >= 3 && named.at(0) == QLatin1Char('/') && named.at(1).isLetter() &&
        named.at(2) == QLatin1Char(':')) {
      named = named.mid(1);
    }
  } else if (scheme > 0) {
    static const QStringList network{QStringLiteral("https"), QStringLiteral("http"), QStringLiteral("ssh"),
                                     QStringLiteral("git+ssh"), QStringLiteral("ssh+git")};
    if (network.contains(text.left(scheme))) {
      named = hostAndPath(text.mid(scheme + 3));
    }
  } else {
    // git's short SSH form user@host:path; never a drive (c:/folder).
    const int colon = static_cast<int>(text.indexOf(QLatin1Char(':')));
    const QString before = colon < 0 ? QString() : text.left(colon);
    if (colon >= 2 && !before.contains(QLatin1Char('/'))) {
      QString path = text.mid(colon + 1);
      while (path.startsWith(QLatin1Char('/'))) {
        path.remove(0, 1);
      }
      const int at = static_cast<int>(before.lastIndexOf(QLatin1Char('@')));
      named = hostAndPath((at >= 0 ? before.mid(at + 1) : before) + QLatin1Char('/') + path);
    }
  }
  chopSlashes(named);
  if (named.endsWith(QStringLiteral(".git"))) {
    named.chop(4);
    chopSlashes(named);
  }
  return named;
}

QString LibrarySettings::sameRepository(const QString& url) const {
  const QString key = repositoryKey(url);
  for (const LibrarySource& source : sources) {
    if (repositoryKey(source.url) == key) {
      return source.url;
    }
  }
  return {};
}

QJsonArray browserLibraries(const QJsonObject& answer, const QStringList& sources, bool community,
                            QStringList* leftOut) {
  QStringList keys;
  for (const QString& source : sources) {
    keys << repositoryKey(source);
  }
  QStringList texts;
  for (const QJsonValue& value : answer.value(QStringLiteral("left_out")).toArray()) {
    const QString text = value.toObject().value(QStringLiteral("text")).toString();
    if (!text.isEmpty() && !texts.contains(text)) {
      texts << text;
    }
  }
  QJsonArray out;
  for (const QJsonValue& value : answer.value(QStringLiteral("libraries")).toArray()) {
    const QJsonObject entry = value.toObject();
    const bool fetched = entry.value(QStringLiteral("fetched")).toBool();
    const QString url = entry.value(QStringLiteral("url")).toString();
    // An enabled source's repository under an id not fetched from it.
    const int at = static_cast<int>(keys.indexOf(repositoryKey(url)));
    if (!fetched && !url.isEmpty() && at >= 0) {
      const QString text = QStringLiteral("%1 left out: same repository as %2")
                               .arg(entry.value(QStringLiteral("id")).toString(), sources.at(at));
      if (!texts.contains(text)) {
        texts << text;
      }
      continue;
    }
    // A fetched library's components are listed themselves; the community
    // view lists every library of the indexes.
    if (community || !fetched) {
      out.append(entry);
    }
  }
  if (leftOut != nullptr) {
    *leftOut = texts;
  }
  return out;
}

void logLeftOutLibraries(const QStringList& texts) {
  static QStringList logged;
  for (const QString& text : texts) {
    if (!logged.contains(text)) {
      logged << text;
      qInfo().noquote() << QStringLiteral("Library index: %1").arg(text);
    }
  }
}

bool LibrarySettings::add(const QString& url) {
  const QString trimmed = url.trimmed();
  // One repository is listed once, whatever form of its URL names it
  // (mitcad#1436: https://www.github.com/mitcad/parts.git beside
  // https://github.com/mitcad/parts.git listed its components twice).
  if (!sameRepository(trimmed).isEmpty()) {
    return false;
  }
  sources.append({trimmed, true, false});
  return true;
}

QString libraryClonesText(const QJsonObject& answer) {
  const QJsonObject clones = answer.value(QStringLiteral("clones")).toObject();
  const auto entry = [](const QJsonObject& clone) {
    const QString dir = QDir::fromNativeSeparators(clone.value(QStringLiteral("dir")).toString());
    return QStringLiteral("%1 (%2)").arg(clone.value(QStringLiteral("url")).toString(),
                                         dir.mid(dir.lastIndexOf(QLatin1Char('/')) + 1));
  };
  QStringList read;
  for (const QJsonValue& value : clones.value(QStringLiteral("read")).toArray()) {
    read << entry(value.toObject());
  }
  QStringList leftOut;
  for (const QJsonValue& value : clones.value(QStringLiteral("left_out")).toArray()) {
    const QJsonObject clone = value.toObject();
    leftOut << QStringLiteral("%1: %2").arg(entry(clone), mitcad::modelMessage(clone.value(QStringLiteral("reason")).toString()));
  }
  QString text = read.isEmpty() ? QStringLiteral("none") : read.join(QStringLiteral(", "));
  if (!leftOut.isEmpty()) {
    text += QStringLiteral("; left out: %1").arg(leftOut.join(QStringLiteral(", ")));
  }
  return text;
}

void logLibraryClones(const QJsonObject& answer) {
  if (!answer.contains(QStringLiteral("clones"))) {
    return;
  }
  // Once per change: every picker and search asks again.
  static std::mutex mutex;
  static QString logged;
  const QString text = libraryClonesText(answer);
  const std::lock_guard<std::mutex> lock(mutex);
  if (text != logged) {
    logged = text;
    qInfo().noquote() << QStringLiteral("Library clones the pickers read: %1").arg(text);
  }
}

} // namespace mitcad
