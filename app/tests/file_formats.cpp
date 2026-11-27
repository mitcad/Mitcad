// SPDX-License-Identifier: MIT
// The Open, Import and Insert Mesh filters against fileKind() (mitcad#2527):
// every extension fileKind() takes is listed in File > Open's and File >
// Import's "Supported files" filter and in its kind's own filter, in lower
// and upper case, as name filters match case-sensitively on Linux; every
// mesh extension in Insert Mesh's filter likewise.
#include "UnitTests.hpp"
#include "files/FileFormats.hpp"

#include <cstdio>

#include <QSet>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line, const QString& detail = QString()) {
  if (!condition) {
    std::fprintf(stderr, "file_formats.cpp:%d: check failed: %s %s\n", line, expression, qPrintable(detail));
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)
#define CHECK_FOR(expr, detail) check((expr), #expr, __LINE__, (detail))

// The patterns of each filter of a dialog filter string ("Name (*.a *.b);;…"),
// by the filter's position.
QList<QSet<QString>> patterns(const QString& dialogFilter) {
  QList<QSet<QString>> result;
  for (const QString& filter : dialogFilter.split(QStringLiteral(";;"))) {
    const qsizetype open = filter.lastIndexOf(QLatin1Char('('));
    const qsizetype close = filter.lastIndexOf(QLatin1Char(')'));
    QSet<QString> set;
    if (open >= 0 && close > open) {
      for (const QString& pattern : filter.mid(open + 1, close - open - 1).split(QLatin1Char(' '), Qt::SkipEmptyParts)) {
        set.insert(pattern);
      }
    }
    result << set;
  }
  return result;
}

bool bothSpellings(const QSet<QString>& set, const QString& suffix) {
  return set.contains(QStringLiteral("*.") + suffix.toLower()) && set.contains(QStringLiteral("*.") + suffix.toUpper());
}

// The position of a kind's own filter in openFilter() and importFilter().
int kindFilter(mitcad::FileKind kind) {
  using mitcad::FileKind;
  switch (kind) {
  case FileKind::Design:
    return 1;
  case FileKind::Mesh:
  case FileKind::Gltf:
    return 3;
  case FileKind::Drawing:
    return 4;
  default:
    return 2;
  }
}

void filterTests(const char* name, const QString& dialogFilter) {
  const QList<QSet<QString>> filters = patterns(dialogFilter);
  // Supported files, MitCad designs, CAD files, meshes, drawings, all files.
  CHECK_FOR(filters.size() == 6, QString::fromLatin1(name));
  if (filters.size() != 6) {
    return;
  }
  CHECK_FOR(filters[5] == QSet<QString>{QStringLiteral("*")}, QString::fromLatin1(name));
  for (const QString& suffix : mitcad::fileKindSuffixes()) {
    const QString detail = QString::fromLatin1(name) + QLatin1Char(' ') + suffix;
    const mitcad::FileKind kind = mitcad::fileKind(QStringLiteral("a.") + suffix);
    CHECK_FOR(kind != mitcad::FileKind::None, detail);
    // Upper case names are the same kind.
    CHECK_FOR(mitcad::fileKind(QStringLiteral("A.") + suffix.toUpper()) == kind, detail);
    CHECK_FOR(bothSpellings(filters[0], suffix), detail);
    CHECK_FOR(bothSpellings(filters[kindFilter(kind)], suffix), detail);
  }
  // No pattern names a suffix fileKind() does not take.
  const QStringList suffixes = mitcad::fileKindSuffixes();
  for (int i = 0; i < 5; ++i) {
    for (const QString& pattern : filters[i]) {
      CHECK_FOR(pattern.startsWith(QStringLiteral("*.")) && suffixes.contains(pattern.mid(2).toLower()),
                QString::fromLatin1(name) + QLatin1Char(' ') + pattern);
    }
  }
}

void meshFilterTests() {
  const QList<QSet<QString>> filters = patterns(mitcad::meshFilter());
  CHECK(filters.size() == 2);
  if (filters.size() != 2) {
    return;
  }
  CHECK(filters[1] == QSet<QString>{QStringLiteral("*")});
  for (const QString& suffix : mitcad::fileKindSuffixes()) {
    const mitcad::FileKind kind = mitcad::fileKind(QStringLiteral("a.") + suffix);
    const bool mesh = kind == mitcad::FileKind::Mesh || kind == mitcad::FileKind::Gltf;
    CHECK_FOR(mesh == bothSpellings(filters[0], suffix), suffix);
  }
}

void caseTests() {
  using mitcad::FileKind;
  using mitcad::fileKind;
  // The ticket's cases: an upper case STEP name and a .solvecraft design.
  CHECK(fileKind(QStringLiteral("/parts/PART.STP")) == FileKind::Cad);
  CHECK(fileKind(QStringLiteral("gear.solvecraft")) == FileKind::Solidcraft);
  CHECK(fileKind(QStringLiteral("model.FCStd")) == FileKind::FreeCad);
  CHECK(fileKind(QStringLiteral("notes.txt")) == FileKind::None);
  CHECK(fileKind(QStringLiteral("noextension")) == FileKind::None);
  const QList<QSet<QString>> open = patterns(mitcad::openFilter());
  CHECK(!open.isEmpty() && open[0].contains(QStringLiteral("*.STP")));
  CHECK(!open.isEmpty() && open[0].contains(QStringLiteral("*.solvecraft")));
  CHECK(!open.isEmpty() && open[0].contains(QStringLiteral("*.FCStd")));
}

} // namespace

int fileFormatsTests() {
  failures = 0;
  caseTests();
  filterTests("openFilter", mitcad::openFilter());
  filterTests("importFilter", mitcad::importFilter());
  meshFilterTests();
  return failures;
}
MITCAD_UNIT_TEST(fileFormatsTests);
