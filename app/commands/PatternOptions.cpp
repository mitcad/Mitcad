// SPDX-License-Identifier: MIT
#include "PatternOptions.hpp"

#include "../framework/Command.hpp"

#include <QJsonValue>
#include <QObject>
#include <QStringList>

namespace mitcad::cmd {
namespace {

// A count, distance or angle of a definition: a number, or the expression
// or parameter name the panel wrote.
QString valueText(const QJsonValue& value) {
  return value.isDouble() ? QString::number(value.toDouble(), 'g', 10) : value.toString();
}

QString directionWords(const QJsonObject& holder) {
  return directionTypeOf(holder) == symmetricDirection() ? QStringLiteral("symmetric")
                                                         : QStringLiteral("one direction");
}

QString distanceTypeOf(const QJsonObject& def) {
  const QString type = def.value(QStringLiteral("distance_type")).toString();
  return type == QStringLiteral("extent") ? type : QStringLiteral("spacing");
}

} // namespace

QString oneDirection() { return QStringLiteral("one"); }

QString symmetricDirection() { return QStringLiteral("symmetric"); }

QString directionTypeOf(const QJsonObject& holder) {
  return holder.value(QStringLiteral("symmetric")).toBool() ? symmetricDirection() : oneDirection();
}

void setDirectionType(QJsonObject& holder, const QString& type) {
  if (type == symmetricDirection()) {
    holder.insert(QStringLiteral("symmetric"), true);
  } else {
    holder.remove(QStringLiteral("symmetric"));
  }
}

bool flipApplies(const QString& directionType, bool flip) { return flip && directionType != symmetricDirection(); }

QString lengthLabel(const QString& distanceType, const QString& suffix) {
  const QString name = distanceType == QStringLiteral("extent") ? QObject::tr("Extent") : QObject::tr("Spacing");
  return suffix.isEmpty() ? name : QStringLiteral("%1 %2").arg(name, suffix);
}

QString patternOptionsText(const QJsonObject& def) {
  const QString type = def.value(QStringLiteral("type")).toString();
  if (type == QStringLiteral("rectangular_pattern")) {
    QStringList directions;
    for (const char* n : {"1", "2", "3"}) {
      const QString suffix = QString::fromLatin1(n);
      const QJsonObject direction = def.value(QStringLiteral("direction") + suffix).toObject();
      if (direction.isEmpty()) {
        continue;
      }
      directions << QStringLiteral("direction %1 %2, quantity %3, %4 %5")
                        .arg(suffix, directionWords(direction),
                             valueText(direction.value(QStringLiteral("quantity"))), distanceTypeOf(def),
                             valueText(direction.value(QStringLiteral("distance"))));
    }
    return directions.join(QStringLiteral("; "));
  }
  if (type == QStringLiteral("circular_pattern")) {
    return QStringLiteral("%1, quantity %2, angle %3")
        .arg(directionWords(def), valueText(def.value(QStringLiteral("quantity"))),
             valueText(def.value(QStringLiteral("angle"))));
  }
  if (type == QStringLiteral("path_pattern")) {
    return QStringLiteral("%1, quantity %2, %3 %4%5")
        .arg(directionWords(def), valueText(def.value(QStringLiteral("quantity"))), distanceTypeOf(def),
             valueText(def.value(QStringLiteral("distance"))),
             def.value(QStringLiteral("flip")).toBool() ? QStringLiteral(", flipped") : QString());
  }
  return QString();
}

void choosePreselectedPatternType(CommandState& state) {
  if (state.items(QStringLiteral("bodies")).isEmpty() && !state.items(QStringLiteral("features")).isEmpty()) {
    state.setChoice(QStringLiteral("object_type"), QStringLiteral("features"));
  }
}

} // namespace mitcad::cmd
