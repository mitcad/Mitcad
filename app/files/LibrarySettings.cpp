// SPDX-License-Identifier: MIT
// The library sources the user chose (settings libraries/sources): plain Qt
// Core, so that the unit tests (app/tests/library_settings.cpp) use it
// without the bridge.
#include "Libraries.hpp"

#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QSettings>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kSourcesKey = QStringLiteral("libraries/sources");

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
  QSettings().setValue(kSourcesKey, QJsonDocument(list).toJson(QJsonDocument::Compact));
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

// A URL as a repository's key: without a trailing slash or ".git", in lower
// case.
QString repositoryKey(const QString& url) {
  QString key = normalLibraryUrl(url).toLower();
  if (key.endsWith(QStringLiteral(".git"))) {
    key.chop(4);
  }
  return key;
}

} // namespace

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
  for (const LibrarySource& source : sources) {
    if (source.url == trimmed) {
      return false;
    }
  }
  sources.append({trimmed, true, false});
  return true;
}

} // namespace mitcad
