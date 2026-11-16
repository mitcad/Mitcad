// SPDX-License-Identifier: MIT
// A project's web viewer folder (mitcad#423): the designs' slugs (the
// path's '/' as "--", other characters as '-', clashes ignoring case and
// "index" numbered), designs.json written and read back (an unsafe page
// name left out), the index page (escaped, no scripts, each card linking
// its page, a thumbnail or a placeholder, light and dark), and writeIndex
// removing a removed design's page and thumbnail but nothing else.
#include "UnitTests.hpp"
#include "web/ProjectWeb.hpp"

#include <cstdio>
#include <vector>

#include <QByteArray>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "project_web.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool touch(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write("x") == 1;
}

QByteArray readAll(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

mitcad::web::DesignCard makeCard(const QString& name, const QString& file, const QString& slug, bool thumbnail) {
  mitcad::web::DesignCard made;
  made.name = name;
  made.file = file;
  made.slug = slug;
  made.thumbnail = thumbnail;
  made.savedBy = QStringLiteral("Ada Tester");
  made.savedAt = QDateTime::fromString(QStringLiteral("2026-10-18T12:34:56Z"), Qt::ISODate);
  return made;
}

} // namespace

int projectWebTests() {
  using namespace mitcad::web;
  // Slugs.
  CHECK(designSlug(QStringLiteral("bracket.mitcad")) == QStringLiteral("bracket"));
  CHECK(designSlug(QStringLiteral("parts/My bracket.mitcad")) == QStringLiteral("parts--My-bracket"));
  CHECK(designSlug(QStringLiteral("a/b/c.v2.MITCAD")) == QStringLiteral("a--b--c.v2"));
  CHECK(designSlug(QStringLiteral("Hall (old)/r\u00e9ck_1.mitcad")) == QStringLiteral("Hall--old---r-ck_1"));
  CHECK(designSlug(QString::fromUtf8("\xF0\x9F\x94\xA7.mitcad")) == QStringLiteral("-")); // one character, one '-'
  CHECK(designSlug(QStringLiteral(".hidden.mitcad")) == QStringLiteral("-hidden"));
  CHECK(designSlug(QStringLiteral(".mitcad")) == QStringLiteral("design"));
  const QHash<QString, QString> slugs =
      designSlugs({QStringLiteral("b c.mitcad"), QStringLiteral("index.mitcad"), QStringLiteral("B-c.mitcad"),
                   QStringLiteral("parts/x.mitcad")});
  CHECK(slugs.size() == 4);
  CHECK(slugs.value(QStringLiteral("b c.mitcad")) == QStringLiteral("b-c"));
  CHECK(slugs.value(QStringLiteral("B-c.mitcad")) == QStringLiteral("B-c-2")); // after "b c" in the sorted order
  CHECK(slugs.value(QStringLiteral("index.mitcad")) == QStringLiteral("index-2"));
  CHECK(slugs.value(QStringLiteral("parts/x.mitcad")) == QStringLiteral("parts--x"));
  // The same list in another order gives the same slugs.
  CHECK(designSlugs({QStringLiteral("parts/x.mitcad"), QStringLiteral("b c.mitcad"), QStringLiteral("B-c.mitcad"),
                     QStringLiteral("index.mitcad")}) == slugs);
  CHECK(isPageFileName(QStringLiteral("parts--x.html"), QStringLiteral(".html")));
  CHECK(!isPageFileName(QStringLiteral("index.html"), QStringLiteral(".html")));
  CHECK(!isPageFileName(QStringLiteral("../x.html"), QStringLiteral(".html")));
  CHECK(!isPageFileName(QStringLiteral(".x.html"), QStringLiteral(".html")));
  CHECK(!isPageFileName(QStringLiteral("x.png"), QStringLiteral(".html")));
  CHECK(!isPageFileName(QStringLiteral(".html"), QStringLiteral(".html")));

  // designs.json and back.
  std::vector<DesignCard> cards{
      makeCard(QStringLiteral("plate"), QStringLiteral("plate.mitcad"), QStringLiteral("plate"), true),
      makeCard(QStringLiteral("x"), QStringLiteral("parts/x.mitcad"), QStringLiteral("parts--x"), false)};
  cards[1].savedBy.clear();
  const QByteArray json = designsJson(cards);
  const QJsonArray list = QJsonDocument::fromJson(json).array();
  CHECK(list.size() == 2);
  const QJsonObject first = list.at(0).toObject();
  CHECK(first.value(QStringLiteral("name")).toString() == QStringLiteral("plate"));
  CHECK(first.value(QStringLiteral("file")).toString() == QStringLiteral("plate.mitcad"));
  CHECK(first.value(QStringLiteral("page")).toString() == QStringLiteral("plate.html"));
  CHECK(first.value(QStringLiteral("thumbnail")).toString() == QStringLiteral("plate.png"));
  CHECK(first.value(QStringLiteral("saved_by")).toString() == QStringLiteral("Ada Tester"));
  CHECK(first.value(QStringLiteral("saved_at")).toString() == QStringLiteral("2026-10-18T12:34:56Z"));
  const QJsonObject second = list.at(1).toObject();
  CHECK(second.value(QStringLiteral("thumbnail")).isNull());
  CHECK(second.value(QStringLiteral("saved_by")).isNull());
  const std::vector<DesignCard> back = readDesignsJson(json);
  CHECK(back.size() == 2);
  if (back.size() == 2) {
    CHECK(back[0].slug == QStringLiteral("plate") && back[0].thumbnail && back[0].savedAt == cards[0].savedAt);
    CHECK(back[1].slug == QStringLiteral("parts--x") && !back[1].thumbnail && back[1].file == cards[1].file);
  }
  // Someone else's designs.json: an unsafe page is not taken.
  const QByteArray unsafe("[{\"page\": \"../../x.html\"}, {\"page\": \"index.html\"}, {\"page\": \"ok.html\"}]");
  CHECK(readDesignsJson(unsafe).size() == 1);
  CHECK(readDesignsJson(QByteArray("not json")).empty());

  // The index.
  std::vector<DesignCard> named = cards;
  named.push_back(makeCard(QStringLiteral("<b>&\"x\""), QStringLiteral("q.mitcad"), QStringLiteral("q"), false));
  const QString index = QString::fromUtf8(indexHtml(QStringLiteral("Shop & <Hall>"), named));
  CHECK(index.startsWith(QStringLiteral("<!doctype html>")));
  CHECK(index.contains(QStringLiteral("<title>Shop &amp; &lt;Hall&gt;</title>")));
  CHECK(index.contains(QStringLiteral("<h1>Shop &amp; &lt;Hall&gt;</h1>")));
  CHECK(index.contains(QStringLiteral("3 designs")));
  CHECK(index.contains(QStringLiteral("href=\"plate.html\"")));
  CHECK(index.contains(QStringLiteral("href=\"parts--x.html\"")));
  CHECK(index.contains(QStringLiteral("<img src=\"plate.png\"")));
  CHECK(!index.contains(QStringLiteral("parts--x.png")));
  CHECK(index.contains(QStringLiteral("No preview")));
  CHECK(index.contains(QStringLiteral("&lt;b&gt;&amp;&quot;x&quot;")));
  CHECK(!index.contains(QStringLiteral("<b>&")));
  CHECK(index.contains(QStringLiteral("Saved by Ada Tester, 2026-10-18 12:34 UTC")));
  CHECK(index.contains(QStringLiteral("prefers-color-scheme: dark")));
  CHECK(!index.contains(QStringLiteral("<script"), Qt::CaseInsensitive));

  // writeIndex: the files written, a removed design's page and thumbnail
  // removed, other files kept.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString folder = directory.filePath(QStringLiteral("web"));
  FolderUpdate update = writeIndex(folder, QStringLiteral("Shop"), cards);
  CHECK(update.error.isEmpty());
  CHECK(update.written.size() == 2);
  CHECK(update.removed.isEmpty());
  CHECK(QFileInfo::exists(folder + QStringLiteral("/index.html")));
  CHECK(readDesignsJson(readAll(folder + QStringLiteral("/designs.json"))).size() == 2);
  for (const char* name : {"plate.html", "plate.png", "parts--x.html", "own.html"}) {
    CHECK(touch(folder + QLatin1Char('/') + QLatin1String(name)));
  }
  // parts/x.mitcad renamed to parts/y.mitcad.
  std::vector<DesignCard> renamed{cards[0], makeCard(QStringLiteral("y"), QStringLiteral("parts/y.mitcad"),
                                                 QStringLiteral("parts--y"), false)};
  update = writeIndex(folder, QStringLiteral("Shop"), renamed);
  CHECK(update.error.isEmpty());
  CHECK(!QFileInfo::exists(folder + QStringLiteral("/parts--x.html")));
  CHECK(update.removed.contains(folder + QStringLiteral("/parts--x.html")));
  CHECK(update.removed.contains(folder + QStringLiteral("/parts--x.png"))); // listed, gone already
  CHECK(QFileInfo::exists(folder + QStringLiteral("/plate.html")));
  CHECK(QFileInfo::exists(folder + QStringLiteral("/plate.png")));
  CHECK(QFileInfo::exists(folder + QStringLiteral("/own.html"))); // not the viewer's
  const std::vector<DesignCard> now = readDesignsJson(readAll(folder + QStringLiteral("/designs.json")));
  // Sorted by their files: parts/y.mitcad before plate.mitcad.
  CHECK(now.size() == 2 && now[0].slug == QStringLiteral("parts--y") && now[1].slug == QStringLiteral("plate"));
  return failures;
}
MITCAD_UNIT_TEST(projectWebTests);
