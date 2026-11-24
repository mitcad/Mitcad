// SPDX-License-Identifier: MIT
// Split Body's tools of other components (mitcad#2299): a face or body
// picked as the Splitting Tool in an occurrence of another component than
// the bodies to split goes into `tool_links` keyed by its body (source: the
// picked occurrence, target: the bodies' occurrence, an empty path left
// out); one of the split's own component, in any of its occurrences, a
// plane and sketch curves write none; a loaded split's picks come back
// where its links saw them.
#include "UnitTests.hpp"
#include "commands/SplitToolLinks.hpp"
#include "framework/Selection.hpp"

#include <cstdio>

#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "split_tool_links.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::SelectionItem;
using mitcad::SelectKind;
using mitcad::Selection;
using namespace mitcad::cmd;

// The root (C0) places A:1 (O1, C1) and B:1 (O2, C2) and B:2 (O3, C2);
// B:1 places Sub:1 (O2/O4, C3).
QString componentOf(const QString& path) {
  if (path.isEmpty()) {
    return QStringLiteral("C0");
  }
  if (path == QStringLiteral("O1")) {
    return QStringLiteral("C1");
  }
  if (path == QStringLiteral("O2") || path == QStringLiteral("O3")) {
    return QStringLiteral("C2");
  }
  if (path == QStringLiteral("O2/O4")) {
    return QStringLiteral("C3");
  }
  return QString();
}

SelectionItem body(const QString& uid, const QString& occurrence) {
  SelectionItem item{SelectKind::Body, uid, QString(), QString()};
  item.occurrence = occurrence;
  return item;
}

SelectionItem face(const QString& uid, const QString& name, const QString& occurrence) {
  SelectionItem item{SelectKind::Face, uid, name, QStringLiteral("plane")};
  item.occurrence = occurrence;
  return item;
}

QJsonObject link(const QString& source, const QString& target) {
  QJsonObject value;
  if (!source.isEmpty()) {
    value.insert(QStringLiteral("source"), source);
  }
  if (!target.isEmpty()) {
    value.insert(QStringLiteral("target"), target);
  }
  return value;
}

void buildTests() {
  const Selection bodies{body(QStringLiteral("F4.b0"), QStringLiteral("O2"))};
  // A face of A's body, picked where A:1 shows it: linked.
  QJsonObject links =
      splitToolLinks(bodies, {face(QStringLiteral("F2.b0"), QStringLiteral("F2:end(r1)"), QStringLiteral("O1"))},
                     componentOf);
  CHECK(links == QJsonObject({{QStringLiteral("F2.b0"), link(QStringLiteral("O1"), QStringLiteral("O2"))}}));
  // A's body itself as the tool: the same link.
  links = splitToolLinks(bodies, {body(QStringLiteral("F2.b0"), QStringLiteral("O1"))}, componentOf);
  CHECK(links == QJsonObject({{QStringLiteral("F2.b0"), link(QStringLiteral("O1"), QStringLiteral("O2"))}}));
  // A body of the root as the tool: its empty path left out.
  links = splitToolLinks(bodies, {body(QStringLiteral("F9.b0"), QString())}, componentOf);
  CHECK(links == QJsonObject({{QStringLiteral("F9.b0"), link(QString(), QStringLiteral("O2"))}}));
  // A nested component's face: its whole path.
  links = splitToolLinks(bodies, {face(QStringLiteral("F7.b0"), QStringLiteral("F7:side"), QStringLiteral("O2/O4"))},
                         componentOf);
  CHECK(links == QJsonObject({{QStringLiteral("F7.b0"), link(QStringLiteral("O2/O4"), QStringLiteral("O2"))}}));
  // Bodies of the root split by A's face: no target.
  links = splitToolLinks({body(QStringLiteral("F9.b0"), QString())},
                         {face(QStringLiteral("F2.b0"), QStringLiteral("F2:end(r1)"), QStringLiteral("O1"))},
                         componentOf);
  CHECK(links == QJsonObject({{QStringLiteral("F2.b0"), link(QStringLiteral("O1"), QString())}}));

  // The split's own component writes no link: picked where the bodies are,
  // or in another occurrence of the same component (B:2).
  CHECK(splitToolLinks(bodies, {face(QStringLiteral("F4.b0"), QStringLiteral("F4:end(r1)"), QStringLiteral("O2"))},
                       componentOf)
            .isEmpty());
  CHECK(splitToolLinks(bodies, {body(QStringLiteral("F6.b0"), QStringLiteral("O3"))}, componentOf).isEmpty());
  // Planes and sketch curves are unchanged: no link.
  SelectionItem plane{SelectKind::Plane, QStringLiteral("F8"), QString(), QStringLiteral("plane")};
  plane.occurrence = QStringLiteral("O1");
  CHECK(splitToolLinks(bodies, {plane}, componentOf).isEmpty());
  SelectionItem curve{SelectKind::SketchCurve, QStringLiteral("F1"), QStringLiteral("c1"), QStringLiteral("line")};
  curve.occurrence = QStringLiteral("O1");
  CHECK(splitToolLinks(bodies, {curve}, componentOf).isEmpty());
  // No tool yet: nothing.
  CHECK(splitToolLinks(bodies, {}, componentOf).isEmpty());
}

void loadTests() {
  // A loaded split: the tool and the bodies without occurrences, as the
  // definition names them.
  Selection bodies{body(QStringLiteral("F4.b0"), QString())};
  Selection tools{face(QStringLiteral("F2.b0"), QStringLiteral("F2:end(r1)"), QString())};
  linkSplitPicks({{QStringLiteral("F2.b0"), link(QStringLiteral("O1"), QStringLiteral("O3"))}}, bodies, tools);
  CHECK(tools.first().occurrence == QStringLiteral("O1"));
  CHECK(bodies.first().occurrence == QStringLiteral("O3"));
  // Built again from the loaded picks: the same link.
  CHECK(splitToolLinks(bodies, tools, componentOf) ==
        QJsonObject({{QStringLiteral("F2.b0"), link(QStringLiteral("O1"), QStringLiteral("O3"))}}));

  // A root tool (no source) for bodies of B:1.
  bodies = {body(QStringLiteral("F4.b0"), QString())};
  tools = {body(QStringLiteral("F9.b0"), QString())};
  linkSplitPicks({{QStringLiteral("F9.b0"), link(QString(), QStringLiteral("O2"))}}, bodies, tools);
  CHECK(tools.first().occurrence.isEmpty());
  CHECK(bodies.first().occurrence == QStringLiteral("O2"));

  // Without links nothing changes (the session puts the picks where their
  // components are seen first).
  bodies = {body(QStringLiteral("F4.b0"), QString())};
  tools = {face(QStringLiteral("F4.b0"), QStringLiteral("F4:end(r1)"), QString())};
  linkSplitPicks({}, bodies, tools);
  CHECK(bodies.first().occurrence.isEmpty());
  CHECK(tools.first().occurrence.isEmpty());
}

} // namespace

int splitToolLinksTests() {
  failures = 0;
  buildTests();
  loadTests();
  return failures;
}
MITCAD_UNIT_TEST(splitToolLinksTests);
