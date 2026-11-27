// SPDX-License-Identifier: MIT
// A lathe setup's manual G-code in the setup panel (mitcad#2506,
// cam/CamManualCode.hpp): the places a block may follow, every block of the
// setup loaded as a row and written back in order with its name, its place
// and its lines, rows added, moved up and down and removed with their
// values (CommandState::moveRow), a row without code named in the panel's
// message, and where the Browser lists each block among the operations;
// the Browser's Delete and drag of a block (mitcad#2532).
#include "UnitTests.hpp"
#include "cam/CamManualCode.hpp"
#include "framework/Command.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_manual_code.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandState;
namespace cam = mitcad::cam;

QJsonObject op(int id, const QString& name) {
  return {{QStringLiteral("id"), id}, {QStringLiteral("name"), name}};
}

QJsonObject block(const QString& name, int after, const QString& text) {
  QJsonObject b{{QStringLiteral("text"), text}};
  if (!name.isEmpty()) {
    b.insert(QStringLiteral("name"), name);
  }
  if (after > 0) {
    b.insert(QStringLiteral("after"), after);
  }
  return b;
}

QString key(int row, const QString& input) { return CommandState::rowKey(cam::manualList(), row, input); }

} // namespace

int camManualCodeTests() {
  failures = 0;
  // Facing, roughing, parting off.
  const QJsonArray operations{op(2, QStringLiteral("Turn Facing 1")), op(3, QStringLiteral("Turn Roughing 1")),
                              op(4, QStringLiteral("Parting Off 1"))};

  // The places: the program's start, then the operations by id in order.
  const auto places = cam::manualPlaces(QJsonObject{{QStringLiteral("operations"), operations}});
  CHECK(places.size() == 4);
  CHECK(places.at(0).first == cam::programStart());
  CHECK(places.at(1).first == QStringLiteral("2") && places.at(1).second == QStringLiteral("Turn Facing 1"));
  CHECK(places.at(3).first == QStringLiteral("4") && places.at(3).second == QStringLiteral("Parting Off 1"));
  CHECK(cam::manualPlaces(QJsonObject()).size() == 1);

  // Every block loaded as a row and written back as it was.
  const QJsonArray blocks{block(QStringLiteral("Bar pull"), 4, QStringLiteral("M5\nM11\nG98 G1 Z5. F500.\nM10")),
                          block(QString(), 0, QStringLiteral("M0 (CHECK THE BAR)"))};
  CommandState state;
  cam::loadManualCode(state, blocks);
  CHECK(state.rows(cam::manualList()) == 2);
  CHECK(state.text(key(0, cam::manualName())) == QStringLiteral("Bar pull"));
  CHECK(state.choice(key(0, cam::manualAfter())) == QStringLiteral("4"));
  CHECK(state.choice(key(1, cam::manualAfter())) == cam::programStart());
  QString why;
  QString input;
  CHECK(cam::manualCodeOf(state, &why, &input) == blocks);
  CHECK(why.isEmpty() && input.isEmpty());

  // A row added (the panel's Add Manual G-code): its place and name by
  // default, at the program's start with the model's default name.
  state.setRows(cam::manualList(), 3);
  state.setText(key(2, cam::manualText()), QStringLiteral("M1"));
  state.setChoice(key(2, cam::manualAfter()), QStringLiteral("2"));
  QJsonArray written = cam::manualCodeOf(state, &why, &input);
  CHECK(written.size() == 3);
  CHECK(written.at(2).toObject() == block(QString(), 2, QStringLiteral("M1")));

  // Moved up: the row's values go with it, the others shift.
  state.moveRow(cam::manualList(), 2, 0);
  written = cam::manualCodeOf(state, &why, &input);
  CHECK(written.size() == 3);
  CHECK(written.at(0).toObject() == block(QString(), 2, QStringLiteral("M1")));
  CHECK(written.at(1).toObject() == blocks.at(0).toObject());
  CHECK(written.at(2).toObject() == blocks.at(1).toObject());
  // Moved down again, one place.
  state.moveRow(cam::manualList(), 0, 1);
  written = cam::manualCodeOf(state, &why, &input);
  CHECK(written.at(0).toObject() == blocks.at(0).toObject());
  CHECK(written.at(1).toObject() == block(QString(), 2, QStringLiteral("M1")));
  // Places past the ends change nothing.
  state.moveRow(cam::manualList(), 2, 3);
  state.moveRow(cam::manualList(), -1, 0);
  CHECK(cam::manualCodeOf(state, &why, &input) == written);

  // Removed: the rows after it come up.
  state.removeRow(cam::manualList(), 0);
  written = cam::manualCodeOf(state, &why, &input);
  CHECK(written.size() == 2);
  CHECK(written.at(0).toObject() == block(QString(), 2, QStringLiteral("M1")));
  CHECK(written.at(1).toObject() == blocks.at(1).toObject());

  // A row without code: the panel names it and its G-code field.
  state.setText(key(1, cam::manualText()), QStringLiteral("  \n "));
  cam::manualCodeOf(state, &why, &input);
  CHECK(why.contains(QStringLiteral("manual block 2")));
  CHECK(input == key(1, cam::manualText()));

  // None: an empty list.
  CommandState none;
  cam::loadManualCode(none, QJsonArray());
  CHECK(none.rows(cam::manualList()) == 0);
  QString noWhy;
  CHECK(cam::manualCodeOf(none, &noWhy, nullptr).isEmpty() && noWhy.isEmpty());

  // The Browser: each block after its operation, one at the start first,
  // one after an operation the setup does not have last.
  const QJsonObject setup{
      {QStringLiteral("operations"), operations},
      {QStringLiteral("lathe"),
       QJsonObject{{QStringLiteral("manual_code"),
                    QJsonArray{block(QStringLiteral("Bar pull"), 4, QStringLiteral("M5")),
                               block(QString(), 0, QStringLiteral("M0")),
                               block(QString(), 3, QStringLiteral("M1")),
                               block(QString(), 99, QStringLiteral("M1"))}}}}};
  const QVector<int> listed = cam::manualPlacesInOrder(setup);
  CHECK((listed == QVector<int>{3, 0, 2, 3}));
  CHECK(cam::manualPlacesInOrder(QJsonObject{{QStringLiteral("operations"), operations}}).isEmpty());

  // The Browser's Delete and drag (mitcad#2532). A, at the start; B and C
  // after Turn Facing 1 (2); D after Parting Off 1 (4). The Browser lists
  // A, Turn Facing 1, B, C, Turn Roughing 1, Parting Off 1, D.
  CHECK(cam::manualBlockOf(QStringLiteral("7:2")) == 2);
  CHECK(cam::manualBlockOf(QStringLiteral("7:0")) == 0);
  CHECK(cam::manualBlockOf(QStringLiteral("7")) == -1);
  const QJsonArray four{block(QStringLiteral("A"), 0, QStringLiteral("M0")),
                        block(QStringLiteral("B"), 2, QStringLiteral("M1")),
                        block(QStringLiteral("C"), 2, QStringLiteral("M1")),
                        block(QStringLiteral("D"), 4, QStringLiteral("M0"))};
  const QJsonObject lathe{{QStringLiteral("program_number"), 1000}, {QStringLiteral("manual_code"), four}};
  const QJsonObject dragged{{QStringLiteral("operations"), operations}, {QStringLiteral("lathe"), lathe}};
  // The blocks of a `lathe` as "name@after" (after 0: the program's start).
  const auto order = [](const QJsonObject& result) {
    QStringList names;
    for (const QJsonValue& value : result.value(QStringLiteral("manual_code")).toArray()) {
      const QJsonObject b = value.toObject();
      names << QStringLiteral("%1@%2")
                   .arg(b.value(QStringLiteral("name")).toString())
                   .arg(b.value(QStringLiteral("after")).toInt());
    }
    return names.join(QLatin1Char(' '));
  };

  // Deleted: the others in their order, the setup's other fields kept;
  // none left, no manual_code.
  QJsonObject without = cam::manualCodeWithout(lathe, {1, 3});
  CHECK(order(without) == QStringLiteral("A@0 C@2"));
  CHECK(without.value(QStringLiteral("program_number")).toInt() == 1000);
  without = cam::manualCodeWithout(lathe, {0, 1, 2, 3});
  CHECK(!without.contains(QStringLiteral("manual_code")));
  CHECK(without.value(QStringLiteral("program_number")).toInt() == 1000);
  CHECK(order(cam::manualCodeWithout(lathe, {})) == QStringLiteral("A@0 B@2 C@2 D@4"));

  // D let go before Turn Roughing 1: after Turn Facing 1, after B and C.
  QJsonObject moved = cam::manualCodeMoved(dragged, 3, cam::ManualDrop::BeforeOperation, 1);
  CHECK(order(moved) == QStringLiteral("A@0 B@2 C@2 D@2"));
  CHECK(moved.value(QStringLiteral("program_number")).toInt() == 1000);
  // A let go after Turn Facing 1: right after it, before B and C.
  moved = cam::manualCodeMoved(dragged, 0, cam::ManualDrop::AfterOperation, 0);
  CHECK(order(moved) == QStringLiteral("A@2 B@2 C@2 D@4"));
  // D let go after Turn Roughing 1 (3), where no block is: its own place.
  moved = cam::manualCodeMoved(dragged, 3, cam::ManualDrop::AfterOperation, 1);
  CHECK(order(moved) == QStringLiteral("A@0 B@2 C@2 D@3"));
  // C before B, B after C: their order swapped.
  CHECK(order(cam::manualCodeMoved(dragged, 2, cam::ManualDrop::BeforeBlock, 1)) == QStringLiteral("A@0 C@2 B@2 D@4"));
  CHECK(order(cam::manualCodeMoved(dragged, 1, cam::ManualDrop::AfterBlock, 2)) == QStringLiteral("A@0 C@2 B@2 D@4"));
  // A after D: after Parting Off 1, last.
  CHECK(order(cam::manualCodeMoved(dragged, 0, cam::ManualDrop::AfterBlock, 3)) == QStringLiteral("B@2 C@2 D@4 A@4"));
  // C before A: the program's start, first; B on the setup's row likewise.
  CHECK(order(cam::manualCodeMoved(dragged, 2, cam::ManualDrop::BeforeBlock, 0)) == QStringLiteral("C@0 A@0 B@2 D@4"));
  moved = cam::manualCodeMoved(dragged, 1, cam::ManualDrop::ProgramStart, -1);
  CHECK(order(moved) == QStringLiteral("B@0 A@0 C@2 D@4"));
  CHECK(!moved.value(QStringLiteral("manual_code")).toArray().at(0).toObject().contains(QStringLiteral("after")));
  // Where it is already: nothing to change.
  CHECK(cam::manualCodeMoved(dragged, 1, cam::ManualDrop::AfterOperation, 0).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 2, cam::ManualDrop::AfterBlock, 1).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 1, cam::ManualDrop::BeforeBlock, 2).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 0, cam::ManualDrop::ProgramStart, -1).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 3, cam::ManualDrop::BeforeBlock, 3).isEmpty()); // onto itself
  // No such block or place.
  CHECK(cam::manualCodeMoved(dragged, 4, cam::ManualDrop::ProgramStart, -1).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 0, cam::ManualDrop::AfterOperation, 3).isEmpty());
  CHECK(cam::manualCodeMoved(dragged, 0, cam::ManualDrop::BeforeBlock, 4).isEmpty());
  return failures;
}
MITCAD_UNIT_TEST(camManualCodeTests);
