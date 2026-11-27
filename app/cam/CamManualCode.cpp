// SPDX-License-Identifier: MIT
#include "CamManualCode.hpp"

#include "../framework/Command.hpp"

#include <QCoreApplication>
#include <QJsonValue>

namespace mitcad::cam {

QVector<QPair<QString, QString>> manualPlaces(const QJsonObject& setup) {
  QVector<QPair<QString, QString>> places{
      {programStart(), QCoreApplication::translate("MainWindow", "Program start")}};
  for (const QJsonValue& value : setup.value(QStringLiteral("operations")).toArray()) {
    const QJsonObject op = value.toObject();
    places.append(qMakePair(QString::number(op.value(QStringLiteral("id")).toInt()),
                            op.value(QStringLiteral("name")).toString()));
  }
  return places;
}

void loadManualCode(CommandState& state, const QJsonArray& blocks) {
  const QString list = manualList();
  state.setRows(list, static_cast<int>(blocks.size()));
  for (int i = 0; i < blocks.size(); ++i) {
    const QJsonObject block = blocks.at(i).toObject();
    state.setText(CommandState::rowKey(list, i, manualName()), block.value(QStringLiteral("name")).toString());
    const QJsonValue after = block.value(QStringLiteral("after"));
    state.setChoice(CommandState::rowKey(list, i, manualAfter()),
                    after.isDouble() ? QString::number(after.toInt()) : programStart());
    state.setText(CommandState::rowKey(list, i, manualText()), block.value(QStringLiteral("text")).toString());
  }
}

QJsonArray manualCodeOf(const CommandState& state, QString* why, QString* input) {
  const QString list = manualList();
  QJsonArray blocks;
  for (int i = 0; i < state.rows(list); ++i) {
    const CommandState row = state.row(list, i);
    const QString code = row.text(manualText());
    if (code.trimmed().isEmpty() && why != nullptr && why->isEmpty()) {
      *why = QCoreApplication::translate("MainWindow", "Write the G-code of manual block %1, or remove the block.")
                 .arg(i + 1);
      if (input != nullptr) {
        *input = CommandState::rowKey(list, i, manualText());
      }
    }
    QJsonObject block;
    const QString name = row.text(manualName()).trimmed();
    if (!name.isEmpty()) {
      block.insert(QStringLiteral("name"), name);
    }
    const QString after = row.choice(manualAfter());
    bool number = false;
    const int id = after.toInt(&number);
    if (number && after != programStart()) {
      block.insert(QStringLiteral("after"), id);
    }
    block.insert(QStringLiteral("text"), code);
    blocks.append(block);
  }
  return blocks;
}

QVector<int> manualPlacesInOrder(const QJsonObject& setup) {
  const QJsonArray operations = setup.value(QStringLiteral("operations")).toArray();
  const QJsonArray blocks =
      setup.value(QStringLiteral("lathe")).toObject().value(QStringLiteral("manual_code")).toArray();
  QVector<int> places;
  for (const QJsonValue& value : blocks) {
    const QJsonValue after = value.toObject().value(QStringLiteral("after"));
    if (!after.isDouble()) {
      places.append(0);
      continue;
    }
    int place = static_cast<int>(operations.size());
    for (int i = 0; i < operations.size(); ++i) {
      if (operations.at(i).toObject().value(QStringLiteral("id")).toInt() == after.toInt()) {
        place = i + 1;
        break;
      }
    }
    places.append(place);
  }
  return places;
}

int manualBlockOf(const QString& uid) {
  bool number = false;
  const int block = uid.section(QLatin1Char(':'), 1, 1).toInt(&number);
  return number && block >= 0 ? block : -1;
}

QJsonObject manualCodeWithout(const QJsonObject& lathe, const QVector<int>& blocks) {
  const QJsonArray all = lathe.value(QStringLiteral("manual_code")).toArray();
  QJsonArray kept;
  for (int i = 0; i < all.size(); ++i) {
    if (!blocks.contains(i)) {
      kept.append(all.at(i));
    }
  }
  QJsonObject result = lathe;
  if (kept.isEmpty()) {
    result.remove(QStringLiteral("manual_code"));
  } else {
    result.insert(QStringLiteral("manual_code"), kept);
  }
  return result;
}

QJsonObject manualCodeMoved(const QJsonObject& setup, int block, ManualDrop drop, int index) {
  const QJsonArray operations = setup.value(QStringLiteral("operations")).toArray();
  QJsonObject lathe = setup.value(QStringLiteral("lathe")).toObject();
  const QJsonArray blocks = lathe.value(QStringLiteral("manual_code")).toArray();
  const int count = static_cast<int>(blocks.size());
  const int operationCount = static_cast<int>(operations.size());
  if (block < 0 || block >= count) {
    return {};
  }
  const QVector<int> places = manualPlacesInOrder(setup);
  // The other blocks at a place (after as many operations), in their order.
  const auto othersAt = [&](int place) {
    QVector<int> others;
    for (int b = 0; b < count; ++b) {
      if (b != block && places.at(b) == place) {
        others.append(b);
      }
    }
    return others;
  };
  // Its place among the operations, and its rank among the other blocks
  // there.
  int place = 0;
  int rank = 0;
  switch (drop) {
  case ManualDrop::ProgramStart:
    break;
  case ManualDrop::BeforeOperation:
  case ManualDrop::AfterOperation:
    if (index < 0 || index >= operationCount) {
      return {};
    }
    // Right after the operation, or after every block that follows the
    // one before it.
    place = drop == ManualDrop::AfterOperation ? index + 1 : index;
    rank = drop == ManualDrop::AfterOperation ? 0 : static_cast<int>(othersAt(place).size());
    break;
  case ManualDrop::BeforeBlock:
  case ManualDrop::AfterBlock:
    if (index < 0 || index >= count || index == block) {
      return {};
    }
    place = places.at(index);
    rank = static_cast<int>(othersAt(place).indexOf(index)) + (drop == ManualDrop::AfterBlock ? 1 : 0);
    break;
  }
  // Its place in `manual_code` as it is now: before the block that comes
  // after it there, after the last block there, or its own when no other
  // block is there.
  const QVector<int> same = othersAt(place);
  int at = block;
  if (rank < same.size()) {
    at = same.at(rank);
  } else if (!same.isEmpty()) {
    at = same.last() + 1;
  }
  const int into = at > block ? at - 1 : at;
  QJsonObject moved = blocks.at(block).toObject();
  const QJsonValue before = moved.value(QStringLiteral("after"));
  int follows = -1;
  if (place > 0) {
    follows = operations.at(place - 1).toObject().value(QStringLiteral("id")).toInt();
    moved.insert(QStringLiteral("after"), follows);
  } else {
    moved.remove(QStringLiteral("after"));
  }
  const bool sameAfter = before.isDouble() ? place > 0 && before.toInt() == follows : place == 0;
  if (into == block && sameAfter) {
    return {};
  }
  QJsonArray rest = blocks;
  rest.removeAt(block);
  rest.insert(into, moved);
  lathe.insert(QStringLiteral("manual_code"), rest);
  return lathe;
}

} // namespace mitcad::cam
