// SPDX-License-Identifier: MIT
#include "MinimumDistance.hpp"

#include <cmath>

#include <QJsonValue>
#include <QObject>
#include <QStringList>

#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

// A number with `decimals` decimals, always with a point, never "-0.000".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("6", "2.5").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

// A point of the answer: "(5, 5, 10)", rounded to a micrometre.
QString pointText(const QJsonValue& value) {
  const QJsonArray point = value.toArray();
  if (point.size() != 3) {
    return QStringLiteral("(?)");
  }
  const auto coordinate = [&point](int i) { return plain(std::round(point.at(i).toDouble() * 1e3) / 1e3); };
  return QStringLiteral("(%1, %2, %3)").arg(coordinate(0), coordinate(1), coordinate(2));
}

// The body a closest point lies on, by its name (its uid without one).
QString bodyOf(const QJsonObject& side) {
  const QString name = side.value(QStringLiteral("name")).toString();
  return name.isEmpty() ? side.value(QStringLiteral("body")).toString() : name;
}

bool hasClearance(const QJsonObject& answer) { return answer.value(QStringLiteral("clearance")).isDouble(); }

bool closer(const QJsonObject& answer) { return answer.value(QStringLiteral("within_clearance")).toBool(); }

} // namespace

QJsonArray distancePicks(const Selection& items) {
  QJsonArray picks;
  for (const SelectionItem& item : items) {
    QJsonObject pick;
    switch (item.kind) {
    case SelectKind::Body:
      pick.insert(QStringLiteral("body"), item.owner);
      break;
    case SelectKind::Face:
    case SelectKind::Edge:
    case SelectKind::Vertex: {
      const QString field = item.kind == SelectKind::Face   ? QStringLiteral("face")
                            : item.kind == SelectKind::Edge ? QStringLiteral("edge")
                                                            : QStringLiteral("vertex");
      pick.insert(QStringLiteral("body"), item.owner);
      pick.insert(field, item.name);
      break;
    }
    case SelectKind::Component:
      // The occurrence it was picked through, "" for the root itself.
      pick.insert(QStringLiteral("occurrence"), item.occurrence);
      break;
    default:
      continue;
    }
    if (item.kind != SelectKind::Component && !item.occurrence.isEmpty()) {
      pick.insert(QStringLiteral("occurrence"), item.occurrence);
    }
    if (!picks.contains(pick)) {
      picks.append(pick);
    }
  }
  return picks;
}

SelectionItem distancePickItem(const QJsonObject& pick) {
  const QString body = pick.value(QStringLiteral("body")).toString();
  if (body.isEmpty()) {
    return SelectionItem();
  }
  SelectionItem item{SelectKind::Body, body, QString(), QString()};
  const struct {
    const char* field;
    SelectKind kind;
  } named[] = {{"face", SelectKind::Face}, {"edge", SelectKind::Edge}, {"vertex", SelectKind::Vertex}};
  for (const auto& entry : named) {
    const QString name = pick.value(QLatin1String(entry.field)).toString();
    if (!name.isEmpty()) {
      item.kind = entry.kind;
      item.name = name;
    }
  }
  item.occurrence = pick.value(QStringLiteral("occurrence")).toString();
  return item;
}

QJsonObject minimumDistanceDef(const QJsonArray& first, const QJsonArray& second, std::optional<double> clearance) {
  QJsonObject def{{QStringLiteral("type"), QStringLiteral("minimum_distance")},
                  {QStringLiteral("first"), first},
                  {QStringLiteral("second"), second}};
  if (clearance && std::isfinite(*clearance) && *clearance > 0.0) {
    def.insert(QStringLiteral("clearance"), *clearance);
  }
  return def;
}

std::optional<double> clearanceOf(const QJsonObject& def) {
  const QJsonValue clearance = def.value(QStringLiteral("clearance"));
  const double value = clearance.toDouble(0.0);
  if (clearance.isDouble() && std::isfinite(value) && value > 0.0) {
    return value;
  }
  return std::nullopt;
}

QJsonObject minimumDistanceQuery(const QJsonObject& def) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("minimum_distance")},
                      {QStringLiteral("first"), def.value(QStringLiteral("first")).toArray()},
                      {QStringLiteral("second"), def.value(QStringLiteral("second")).toArray()}};
  if (const std::optional<double> clearance = clearanceOf(def)) {
    request.insert(QStringLiteral("clearance"), *clearance);
  }
  return request;
}

QString runMinimumDistance(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer) {
  answer = query(minimumDistanceQuery(def), true);
  // In plain words (mitcad#1612).
  const QJsonValue error = answer.value(QStringLiteral("error"));
  return error.isString() ? userText(error.toString()) : QString();
}

QString minimumDistanceText(const QJsonObject& answer, const LengthText& length, const std::array<QColor, 2>& colors) {
  const double distance = answer.value(QStringLiteral("distance")).toDouble();
  const QJsonArray points = answer.value(QStringLiteral("points")).toArray();
  const QJsonObject first = answer.value(QStringLiteral("first")).toObject();
  const QJsonObject second = answer.value(QStringLiteral("second")).toObject();
  QStringList lines{QObject::tr("<b>Minimum distance: %1</b>").arg(length(distance))};
  lines << QObject::tr("From %1 at %2").arg(bodyOf(first).toHtmlEscaped(), pointText(points.at(0)));
  lines << QObject::tr("To %1 at %2").arg(bodyOf(second).toHtmlEscaped(), pointText(points.at(1)));
  if (answer.value(QStringLiteral("inside")).toBool()) {
    lines << QObject::tr("One selection lies inside the other.");
  } else if (answer.value(QStringLiteral("touching")).toBool()) {
    lines << QObject::tr("The selections touch.");
  }
  const QColor& line = closer(answer) ? colors[1] : colors[0];
  QString status;
  if (!hasClearance(answer)) {
    status = QObject::tr("Minimum distance");
  } else {
    const QString clearance = length(answer.value(QStringLiteral("clearance")).toDouble());
    status = closer(answer) ? QObject::tr("Closer than the clearance of %1").arg(clearance)
                            : QObject::tr("Clear of the clearance of %1").arg(clearance);
  }
  lines << QStringLiteral("<span style='color:%1'>&#9632;</span> %2").arg(line.name(), status);
  return lines.join(QStringLiteral("<br>"));
}

QString minimumDistanceLog(const QJsonObject& answer) {
  const QJsonArray points = answer.value(QStringLiteral("points")).toArray();
  QString line = QStringLiteral("%1 mm").arg(fixed(answer.value(QStringLiteral("distance")).toDouble(), 3));
  if (answer.value(QStringLiteral("touching")).toBool()) {
    line += QStringLiteral(" (touching)");
  }
  line += QStringLiteral(" from %1 %2 to %3 %4")
              .arg(bodyOf(answer.value(QStringLiteral("first")).toObject()), pointText(points.at(0)),
                   bodyOf(answer.value(QStringLiteral("second")).toObject()), pointText(points.at(1)));
  if (hasClearance(answer)) {
    line += QStringLiteral("; clearance %1 mm: %2")
                .arg(plain(answer.value(QStringLiteral("clearance")).toDouble()),
                     closer(answer) ? QStringLiteral("closer") : QStringLiteral("clear"));
  }
  return line;
}

QJsonObject minimumDistanceOverlayJson(const QJsonObject& answer, const QString& title) {
  QJsonArray line;
  for (const QJsonValue& point : answer.value(QStringLiteral("points")).toArray()) {
    for (const QJsonValue& coordinate : point.toArray()) {
      line.append(coordinate.toDouble());
    }
  }
  QJsonObject distance{{QStringLiteral("points"), line},
                       {QStringLiteral("distance"), answer.value(QStringLiteral("distance")).toDouble()},
                       {QStringLiteral("within"), closer(answer)}};
  if (hasClearance(answer)) {
    distance.insert(QStringLiteral("clearance"), answer.value(QStringLiteral("clearance")).toDouble());
  }
  return {{QStringLiteral("minimum_distance"), distance}, {QStringLiteral("title"), title}};
}

} // namespace mitcad
