// SPDX-License-Identifier: MIT
// The cutters a milling operation's panel offers and starts with
// (mitcad#2590, GitHub mitcad/Mitcad#748; cam/CamToolFit.hpp): a ball end
// mill before a flat one leaves Pocket, Contour and Facing on the flat
// one, Adaptive Clearing, Rest Machining and 3D Roughing offer only flat
// and bull nose end mills, the finishing starts with a ball and offers the
// end mills too, Contour on the line takes engraving cutters and chamfer
// mills, a saved choice that does not fit stays offered, a machine without
// a fitting cutter gives no default, and Drill keeps every tool.
#include "UnitTests.hpp"
#include "cam/CamToolFit.hpp"

#include <cstdio>
#include <initializer_list>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_tool_fit.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject tool(int number, const char* kind) {
  return {{QStringLiteral("number"), number},
          {QStringLiteral("name"), QString::fromLatin1(kind)},
          {QStringLiteral("shape"), QJsonObject{{QStringLiteral("kind"), QString::fromLatin1(kind)}}}};
}

// The numbers of the tools offered, as text.
QStringList numbers(const QJsonArray& tools) {
  QStringList out;
  for (const QJsonValue& value : tools) {
    out << QString::number(value.toObject().value(QStringLiteral("number")).toInt());
  }
  return out;
}

QStringList texts(std::initializer_list<const char*> items) {
  QStringList out;
  for (const char* item : items) {
    out << QString::fromLatin1(item);
  }
  return out;
}

} // namespace

int camToolFitTests() {
  failures = 0;
  using mitcad::cam::Cutters;
  using mitcad::cam::cuttersOf;
  using mitcad::cam::defaultTool;
  using mitcad::cam::offeredTools;
  using mitcad::cam::toolFits;
  const QString outside = QStringLiteral("outside");
  const QString on = QStringLiteral("on");

  // The reproduction: a ball end mill as tool 1, a flat end mill as tool 2.
  const QJsonArray ballFirst{tool(1, "ball"), tool(2, "flat")};
  for (const char* type : {"pocket", "contour", "facing", "adaptive", "rest", "roughing_3d"}) {
    CHECK(defaultTool(ballFirst, QString::fromLatin1(type), outside) == QStringLiteral("2"));
    CHECK(numbers(offeredTools(ballFirst, cuttersOf(QString::fromLatin1(type), outside), -1)) == texts({"2"}));
  }

  // A machine with every kind of tool.
  const QJsonArray every{tool(1, "ball"), tool(2, "drill"), tool(3, "chamfer"), tool(4, "engraver"),
                         tool(5, "bull"),  tool(6, "flat"),  tool(7, "turning")};
  // The roughing operations: flat and bull nose end mills only.
  for (const char* type : {"facing", "pocket", "adaptive", "rest", "roughing_3d"}) {
    CHECK(cuttersOf(QString::fromLatin1(type), on) == Cutters::EndMill);
    CHECK(numbers(offeredTools(every, Cutters::EndMill, -1)) == texts({"5", "6"}));
    CHECK(defaultTool(every, QString::fromLatin1(type), outside) == QStringLiteral("5"));
  }
  // Contour beside the outlines likewise; on the line also the V-bits.
  CHECK(cuttersOf(QStringLiteral("contour"), outside) == Cutters::EndMill);
  CHECK(cuttersOf(QStringLiteral("contour"), QStringLiteral("inside")) == Cutters::EndMill);
  CHECK(cuttersOf(QStringLiteral("contour"), on) == Cutters::EndMillOrVBit);
  CHECK(numbers(offeredTools(every, Cutters::EndMillOrVBit, -1)) == texts({"3", "4", "5", "6"}));
  // The finishing: a ball first, the end mills offered too.
  for (const char* type : {"raster_finish", "waterline_finish"}) {
    CHECK(cuttersOf(QString::fromLatin1(type), outside) == Cutters::Finishing);
    CHECK(defaultTool(every, QString::fromLatin1(type), outside) == QStringLiteral("1"));
  }
  CHECK(numbers(offeredTools(every, Cutters::Finishing, -1)) == texts({"1", "5", "6"}));
  const QJsonArray endMills{tool(1, "flat"), tool(2, "bull")};
  CHECK(defaultTool(endMills, QStringLiteral("raster_finish"), outside) == QStringLiteral("1"));
  // Drill: every tool, a drill first.
  CHECK(cuttersOf(QStringLiteral("drill"), outside) == Cutters::Any);
  CHECK(numbers(offeredTools(every, Cutters::Any, -1)).size() == 7);
  CHECK(defaultTool(every, QStringLiteral("drill"), outside) == QStringLiteral("2"));
  CHECK(defaultTool(ballFirst, QStringLiteral("drill"), outside) == QStringLiteral("1"));

  // A saved operation's ball end mill stays offered, in its place.
  CHECK(numbers(offeredTools(ballFirst, Cutters::EndMill, 1)) == texts({"1", "2"}));
  CHECK(!toolFits(tool(1, "ball"), Cutters::EndMill));
  CHECK(toolFits(tool(1, "ball"), Cutters::Finishing));
  CHECK(!toolFits(tool(1, "engraver"), Cutters::EndMill));
  CHECK(toolFits(tool(1, "engraver"), Cutters::EndMillOrVBit));
  CHECK(!toolFits(tool(1, "drill"), Cutters::Finishing));
  CHECK(!toolFits(QJsonObject(), Cutters::EndMill));

  // Only cutters the operation cannot use: no default, nothing offered.
  const QJsonArray unfit{tool(1, "ball"), tool(2, "drill"), tool(3, "engraver")};
  CHECK(defaultTool(unfit, QStringLiteral("pocket"), outside).isEmpty());
  CHECK(offeredTools(unfit, Cutters::EndMill, -1).isEmpty());
  // On the line the engraving cutter is the default when no end mill is.
  CHECK(defaultTool(unfit, QStringLiteral("contour"), on) == QStringLiteral("3"));
  CHECK(defaultTool(QJsonArray{tool(1, "drill")}, QStringLiteral("raster_finish"), outside).isEmpty());
  CHECK(defaultTool(QJsonArray(), QStringLiteral("drill"), outside).isEmpty());

  CHECK(mitcad::cam::toolOf(every, 4).value(QStringLiteral("name")).toString() == QStringLiteral("engraver"));
  CHECK(mitcad::cam::toolOf(every, 9).isEmpty());
  return failures;
}
MITCAD_UNIT_TEST(camToolFitTests);
