// SPDX-License-Identifier: MIT
#include "MinRadiusAnalysis.hpp"

#include <cmath>

#include <QJsonValue>
#include <QObject>
#include <QStringList>

#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

// A number with `decimals` decimals, always with a point, never "-0.0".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("4", "2.5").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

// A body's name: the answer's, else `name`'s.
QString nameOf(const QJsonObject& entry, const QString& nameKey, const BodyNames& name) {
  const QString given = entry.value(nameKey).toString();
  return given.isEmpty() ? name(entry.value(QStringLiteral("body")).toString()) : given;
}

// The smallest radius of an entry (the answer or a body) as a link that
// shows where it is.
QString smallestText(const QJsonObject& entry) {
  const QString value = QStringLiteral("%1 mm").arg(fixed(entry.value(QStringLiteral("smallest")).toDouble(), 3));
  const QJsonArray at = entry.value(QStringLiteral("at")).toArray();
  if (at.size() != 3) {
    return value;
  }
  return QStringLiteral("<a href='point:%1,%2,%3'>%4</a>")
      .arg(QString::number(at.at(0).toDouble(), 'g', 12), QString::number(at.at(1).toDouble(), 'g', 12),
           QString::number(at.at(2).toDouble(), 'g', 12), value);
}

// The faces under the limit of an entry: "1 face, 62.8 mm²".
QString underText(const QJsonObject& entry) {
  const int count = entry.value(QStringLiteral("under_count")).toInt();
  return QObject::tr("%n face(s), %1 mm²", nullptr, count)
      .arg(fixed(entry.value(QStringLiteral("under_area")).toDouble(), 1));
}

} // namespace

QJsonObject minRadiusAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, double limit) {
  QJsonObject def{{QStringLiteral("type"), QStringLiteral("min_radius")}, {QStringLiteral("bodies"), bodies}};
  if (!faces.isEmpty()) {
    def.insert(QStringLiteral("faces"), faces);
  }
  if (std::isfinite(limit) && limit > 0.0) {
    def.insert(QStringLiteral("limit"), limit);
  }
  return def;
}

double minRadiusLimitOf(const QJsonObject& def) {
  const double limit = def.value(QStringLiteral("limit")).toDouble(0.0);
  return std::isfinite(limit) && limit > 0.0 ? limit : 0.0;
}

QJsonArray minRadiusBodiesOf(const QJsonObject& def) {
  QJsonArray bodies;
  for (const QJsonValue& body : def.value(QStringLiteral("bodies")).toArray()) {
    if (!bodies.contains(body)) {
      bodies.append(body);
    }
  }
  for (const QJsonValue& face : def.value(QStringLiteral("faces")).toArray()) {
    const QJsonValue body = face.toObject().value(QStringLiteral("body"));
    if (body.isString() && !bodies.contains(body)) {
      bodies.append(body);
    }
  }
  return bodies;
}

QJsonObject minRadiusAnalysisQuery(const QJsonObject& def) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("min_radius_analysis")},
                      {QStringLiteral("bodies"), def.value(QStringLiteral("bodies")).toArray()},
                      {QStringLiteral("faces"), def.value(QStringLiteral("faces")).toArray()}};
  const double limit = minRadiusLimitOf(def);
  if (limit > 0.0) {
    request.insert(QStringLiteral("limit"), limit);
  }
  return request;
}

QString runMinRadiusAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer) {
  answer = query(minRadiusAnalysisQuery(def), true);
  // In plain words (mitcad#1612).
  const QJsonValue error = answer.value(QStringLiteral("error"));
  return error.isString() ? userText(error.toString()) : QString();
}

QString minRadiusAnalysisText(const QJsonObject& answer, const BodyNames& name, const QColor& color) {
  const QJsonValue limit = answer.value(QStringLiteral("limit"));
  const bool hasLimit = limit.isDouble() && limit.toDouble() > 0.0;
  QStringList parts;
  if (answer.value(QStringLiteral("smallest")).isDouble()) {
    parts << QObject::tr("Smallest radius: <b>%1</b> on %2")
                 .arg(smallestText(answer), nameOf(answer, QStringLiteral("body_name"), name).toHtmlEscaped());
  } else {
    parts << QObject::tr("No face bends: every face analysed is flat.");
  }
  if (hasLimit) {
    parts << QStringLiteral("<span style='color:%1'>&#9632;</span> %2")
                 .arg(color.name(),
                      QObject::tr("Radius under %1 mm: %2").arg(plain(limit.toDouble()), underText(answer)));
  }
  const QJsonArray bodies = answer.value(QStringLiteral("bodies")).toArray();
  if (bodies.size() > 1) {
    QStringList lines;
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      QString line = QStringLiteral("<b>%1</b>: ").arg(nameOf(body, QStringLiteral("name"), name).toHtmlEscaped());
      line += body.value(QStringLiteral("smallest")).isDouble()
                  ? QObject::tr("smallest %1").arg(smallestText(body))
                  : QObject::tr("flat");
      if (hasLimit) {
        line += QStringLiteral(", ") + QObject::tr("under the limit %1").arg(underText(body));
      }
      lines << line;
    }
    parts << lines.join(QStringLiteral("<br>"));
  }
  parts << QObject::tr("<i>The smallest radius of curvature of the faces, where they bend most, convex or "
                       "concave.</i>");
  return parts.join(QStringLiteral("<br><br>"));
}

QString minRadiusAnalysisLog(const QJsonObject& answer, const BodyNames& name) {
  QString line = answer.value(QStringLiteral("smallest")).isDouble()
                     ? QStringLiteral("smallest %1 mm on %2")
                           .arg(fixed(answer.value(QStringLiteral("smallest")).toDouble(), 3),
                                nameOf(answer, QStringLiteral("body_name"), name))
                     : QStringLiteral("no radius, every face is flat");
  const QJsonValue limit = answer.value(QStringLiteral("limit"));
  if (limit.isDouble() && limit.toDouble() > 0.0) {
    line += QStringLiteral(", under %1 mm: %2 face(s), %3 mm^2")
                .arg(plain(limit.toDouble()))
                .arg(answer.value(QStringLiteral("under_count")).toInt())
                .arg(fixed(answer.value(QStringLiteral("under_area")).toDouble(), 1));
  }
  return line;
}

QJsonObject minRadiusOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title) {
  // Each body's triangles of the faces under the limit, one array per body.
  QJsonArray parts;
  for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
    parts.append(value.toObject().value(QStringLiteral("triangles")).toArray());
  }
  QJsonObject radius{{QStringLiteral("limit"), minRadiusLimitOf(def)},
                     {QStringLiteral("parts"), parts},
                     {QStringLiteral("smallest"), answer.value(QStringLiteral("smallest"))},
                     {QStringLiteral("at"), answer.value(QStringLiteral("at"))}};
  return {{QStringLiteral("min_radius"), radius}, {QStringLiteral("title"), title}};
}

} // namespace mitcad
