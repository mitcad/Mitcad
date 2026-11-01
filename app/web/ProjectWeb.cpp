// SPDX-License-Identifier: MIT
#include "ProjectWeb.hpp"

#include <algorithm>
#include <initializer_list>
#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSet>

namespace mitcad::web {
namespace {

// designs.json files larger than this are not read (someone else's).
constexpr qint64 kMaxDesignsJson = 16 * 1024 * 1024;

bool lessIgnoringCase(const QString& a, const QString& b) {
  const int order = a.compare(b, Qt::CaseInsensitive);
  return order != 0 ? order < 0 : a < b;
}

QString escaped(const QString& text) { return text.toHtmlEscaped(); }

QString savedText(const DesignCard& card) {
  const QString when = card.savedAt.isValid()
                           ? card.savedAt.toUTC().toString(QStringLiteral("yyyy-MM-dd HH:mm")) + QStringLiteral(" UTC")
                           : QString();
  if (!card.savedBy.isEmpty() && !when.isEmpty()) {
    return QStringLiteral("Saved by %1, %2").arg(card.savedBy, when);
  }
  if (!card.savedBy.isEmpty()) {
    return QStringLiteral("Saved by %1").arg(card.savedBy);
  }
  if (!when.isEmpty()) {
    return QStringLiteral("Saved %1").arg(when);
  }
  return QString();
}

// Writes `data` to `path` whole or not at all.
bool writeWhole(const QString& path, const QByteArray& data, QString& error) {
  QSaveFile out(path);
  if (!out.open(QIODevice::WriteOnly) || out.write(data) != data.size() || !out.commit()) {
    error = QStringLiteral("could not write %1: %2").arg(QDir::toNativeSeparators(path), out.errorString());
    return false;
  }
  return true;
}

const char* const kStyle = R"(:root {
  color-scheme: light dark;
  --bg: #f5f6f8;
  --card: #ffffff;
  --text: #1c222b;
  --muted: #5d6877;
  --border: #dce1e7;
  --accent: #2e6bd3;
  --thumb: #eceff3;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #14171c;
    --card: #1e232a;
    --text: #e5e8ed;
    --muted: #98a3b1;
    --border: #2d343e;
    --accent: #6c9fff;
    --thumb: #252b33;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  line-height: 1.4;
}
header, main, footer { max-width: 1200px; margin: 0 auto; padding-left: 24px; padding-right: 24px; }
header { padding-top: 32px; padding-bottom: 8px; }
h1 { margin: 0; font-size: 1.6rem; font-weight: 600; overflow-wrap: anywhere; }
header p { margin: 4px 0 0; color: var(--muted); }
main {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 16px;
  padding-top: 16px;
  padding-bottom: 32px;
}
a.card {
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  color: inherit;
  text-decoration: none;
  transition: border-color 0.15s, transform 0.15s;
}
a.card:hover, a.card:focus-visible { border-color: var(--accent); transform: translateY(-2px); outline: none; }
.thumb { aspect-ratio: 1 / 1; background: var(--thumb); display: flex; align-items: center; justify-content: center; }
.thumb img { width: 100%; height: 100%; object-fit: contain; display: block; }
.thumb span { color: var(--muted); font-size: 0.9rem; }
.info { padding: 10px 12px 12px; }
.name { font-weight: 600; overflow-wrap: anywhere; }
.file { margin-top: 2px; color: var(--muted); font-size: 0.8rem; overflow-wrap: anywhere; }
.saved { margin-top: 6px; color: var(--muted); font-size: 0.85rem; }
.empty { color: var(--muted); }
footer { padding-bottom: 24px; color: var(--muted); font-size: 0.8rem; }
@media (max-width: 480px) {
  header, main, footer { padding-left: 16px; padding-right: 16px; }
  main { grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; }
}
)";

} // namespace

QString designSlug(const QString& path) {
  QString base = path;
  if (base.endsWith(QLatin1String(".mitcad"), Qt::CaseInsensitive)) {
    base.chop(7);
  }
  QString slug;
  slug.reserve(base.size() + 8);
  for (qsizetype i = 0; i < base.size(); ++i) {
    const QChar c = base.at(i);
    if (c == QLatin1Char('/')) {
      slug += QStringLiteral("--");
      continue;
    }
    if (c.isHighSurrogate() && i + 1 < base.size() && base.at(i + 1).isLowSurrogate()) {
      ++i; // one character, one '-'
      slug += QLatin1Char('-');
      continue;
    }
    const char16_t u = c.unicode();
    const bool plain = (u >= u'a' && u <= u'z') || (u >= u'A' && u <= u'Z') || (u >= u'0' && u <= u'9') ||
                       u == u'.' || u == u'_' || u == u'-';
    slug += plain ? c : QLatin1Char('-');
  }
  if (slug.startsWith(QLatin1Char('.'))) {
    slug[0] = QLatin1Char('-');
  }
  return slug.isEmpty() ? QStringLiteral("design") : slug;
}

QHash<QString, QString> designSlugs(QStringList paths) {
  std::sort(paths.begin(), paths.end(), lessIgnoringCase);
  paths.removeDuplicates();
  QSet<QString> taken{QStringLiteral("index")};
  QHash<QString, QString> slugs;
  for (const QString& path : std::as_const(paths)) {
    const QString base = designSlug(path);
    QString slug = base;
    for (int n = 2; taken.contains(slug.toLower()); ++n) {
      slug = QStringLiteral("%1-%2").arg(base).arg(n);
    }
    taken.insert(slug.toLower());
    slugs.insert(path, slug);
  }
  return slugs;
}

bool isPageFileName(const QString& name, const QString& suffix) {
  static const QRegularExpression plain(QStringLiteral("^[A-Za-z0-9_-][A-Za-z0-9._-]*$"));
  return name.size() > suffix.size() && name.endsWith(suffix) && plain.match(name).hasMatch() &&
         name.compare(QLatin1String("index.html"), Qt::CaseInsensitive) != 0;
}

QByteArray designsJson(const std::vector<DesignCard>& cards) {
  QJsonArray list;
  for (const DesignCard& card : cards) {
    list.append(QJsonObject{
        {QStringLiteral("name"), card.name},
        {QStringLiteral("file"), card.file},
        {QStringLiteral("page"), card.page()},
        {QStringLiteral("thumbnail"), card.thumbnail ? QJsonValue(card.thumbnailFile()) : QJsonValue()},
        {QStringLiteral("saved_by"), card.savedBy.isEmpty() ? QJsonValue() : QJsonValue(card.savedBy)},
        {QStringLiteral("saved_at"), card.savedAt.isValid()
                                         ? QJsonValue(card.savedAt.toUTC().toString(Qt::ISODate))
                                         : QJsonValue()},
    });
  }
  return QJsonDocument(list).toJson(QJsonDocument::Indented);
}

std::vector<DesignCard> readDesignsJson(const QByteArray& json) {
  std::vector<DesignCard> cards;
  const QJsonDocument document = QJsonDocument::fromJson(json);
  if (!document.isArray()) {
    return cards;
  }
  const QString html = QStringLiteral(".html");
  for (const QJsonValue& value : document.array()) {
    const QJsonObject entry = value.toObject();
    const QString page = entry.value(QStringLiteral("page")).toString();
    if (!isPageFileName(page, html)) {
      continue;
    }
    DesignCard card;
    card.slug = page.chopped(html.size());
    card.name = entry.value(QStringLiteral("name")).toString();
    card.file = entry.value(QStringLiteral("file")).toString();
    card.thumbnail = entry.value(QStringLiteral("thumbnail")).toString() == card.thumbnailFile();
    card.savedBy = entry.value(QStringLiteral("saved_by")).toString();
    card.savedAt = QDateTime::fromString(entry.value(QStringLiteral("saved_at")).toString(), Qt::ISODate);
    cards.push_back(card);
  }
  return cards;
}

QByteArray indexHtml(const QString& projectName, const std::vector<DesignCard>& cards) {
  QString html;
  html += QStringLiteral("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n"
                         "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n"
                         "<meta name=\"generator\" content=\"MitCad\">\n");
  html += QStringLiteral("<title>%1</title>\n<style>\n").arg(escaped(projectName));
  html += QString::fromUtf8(kStyle);
  html += QStringLiteral("</style>\n</head>\n<body>\n<header>\n<h1>%1</h1>\n").arg(escaped(projectName));
  html += QStringLiteral("<p>%1</p>\n</header>\n<main>\n")
              .arg(cards.size() == 1 ? QStringLiteral("1 design")
                                     : QStringLiteral("%1 designs").arg(static_cast<qulonglong>(cards.size())));
  if (cards.empty()) {
    html += QStringLiteral("<p class=\"empty\">No designs yet.</p>\n");
  }
  for (const DesignCard& card : cards) {
    html += QStringLiteral("<a class=\"card\" href=\"%1\">\n").arg(escaped(card.page()));
    if (card.thumbnail) {
      html += QStringLiteral("<div class=\"thumb\"><img src=\"%1\" alt=\"\" loading=\"lazy\"></div>\n")
                  .arg(escaped(card.thumbnailFile()));
    } else {
      html += QStringLiteral("<div class=\"thumb\"><span>No preview</span></div>\n");
    }
    html += QStringLiteral("<div class=\"info\">\n<div class=\"name\">%1</div>\n<div class=\"file\">%2</div>\n")
                .arg(escaped(card.name), escaped(card.file));
    const QString saved = savedText(card);
    if (!saved.isEmpty()) {
      html += QStringLiteral("<div class=\"saved\">%1</div>\n").arg(escaped(saved));
    }
    html += QStringLiteral("</div>\n</a>\n");
  }
  html += QStringLiteral("</main>\n<footer>Each design opens in a viewer that runs in the browser, also offline. "
                         "Written by MitCad.</footer>\n</body>\n</html>\n");
  return html.toUtf8();
}

FolderUpdate writeIndex(const QString& folder, const QString& projectName, std::vector<DesignCard> cards) {
  FolderUpdate update;
  const QDir dir(folder);
  if (!QDir().mkpath(folder)) {
    update.error = QStringLiteral("could not make the folder %1").arg(QDir::toNativeSeparators(folder));
    return update;
  }
  std::sort(cards.begin(), cards.end(),
            [](const DesignCard& a, const DesignCard& b) { return lessIgnoringCase(a.file, b.file); });
  const QString jsonPath = dir.filePath(QStringLiteral("designs.json"));
  // The pages and thumbnails of designs no card has any more, as the
  // previous designs.json listed them (only plain names in the folder).
  QSet<QString> kept;
  for (const DesignCard& card : cards) {
    kept.insert(card.page().toLower());
    kept.insert(card.thumbnailFile().toLower());
  }
  QFile previous(jsonPath);
  if (previous.size() <= kMaxDesignsJson && previous.open(QIODevice::ReadOnly)) {
    const std::vector<DesignCard> listed = readDesignsJson(previous.readAll());
    previous.close();
    for (const DesignCard& card : listed) {
      for (const QString& name : {card.page(), card.thumbnailFile()}) {
        if (kept.contains(name.toLower())) {
          continue;
        }
        kept.insert(name.toLower()); // once
        const QString path = dir.filePath(name);
        if (QFileInfo::exists(path) && !QFile::remove(path)) {
          update.error = QStringLiteral("could not remove %1").arg(QDir::toNativeSeparators(path));
          continue;
        }
        update.removed << path;
      }
    }
  }
  QString error;
  const QString indexPath = dir.filePath(QStringLiteral("index.html"));
  if (!writeWhole(jsonPath, designsJson(cards), error) ||
      !writeWhole(indexPath, indexHtml(projectName, cards), error)) {
    update.error = error;
    return update;
  }
  update.written << indexPath << jsonPath;
  return update;
}

} // namespace mitcad::web
