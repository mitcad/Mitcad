// SPDX-License-Identifier: MIT
#include "DraftAnalysis.hpp"

#include <cmath>
#include <cstddef>

#include <QObject>
#include <QStringList>

#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

// The classes in the answer's words, in the order of the colours.
const char* const kClasses[3] = {"positive", "under", "negative"};

// A number with `decimals` decimals, always with a point, never "-0.0".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("1", "2.5").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

double areaOf(const QJsonObject& entry, int c) {
  return entry.value(QStringLiteral("%1_area").arg(QLatin1String(kClasses[c]))).toDouble();
}

double percentOf(const QJsonObject& entry, int c) {
  const QJsonValue share = entry.value(QStringLiteral("%1_share").arg(QLatin1String(kClasses[c])));
  if (share.isDouble()) {
    return 100.0 * share.toDouble();
  }
  const double area = entry.value(QStringLiteral("area")).toDouble();
  return area > 0.0 ? 100.0 * areaOf(entry, c) / area : 0.0;
}

// The pull direction of an answer: "(0, 0, 1)".
QString pullText(const QJsonObject& answer) {
  const QJsonArray pull = answer.value(QStringLiteral("pull")).toArray();
  if (pull.size() != 3) {
    return QStringLiteral("(?)");
  }
  // Rounded so that a direction along an axis reads as one.
  const auto coordinate = [&pull](int i) { return plain(std::round(pull.at(i).toDouble() * 1e4) / 1e4); };
  return QStringLiteral("(%1, %2, %3)").arg(coordinate(0), coordinate(1), coordinate(2));
}

// The classes' lines of one body or of the total.
void describe(const QJsonObject& entry, double angle, const std::array<QColor, 3>& colors, QStringList& lines) {
  const QString words[3] = {
      QObject::tr("Draft %1° or more along the pull").arg(plain(angle)),
      QObject::tr("Needs more draft, under %1°").arg(plain(angle)),
      QObject::tr("Draft %1° or more against the pull").arg(plain(angle)),
  };
  for (int c = 0; c < 3; ++c) {
    lines << QStringLiteral("<span style='color:%1'>&#9632;</span> %2: %3 mm² (%4 %)")
                 .arg(colors[static_cast<std::size_t>(c)].name(), words[c], fixed(areaOf(entry, c), 1),
                      fixed(percentOf(entry, c), 1));
  }
}

// The classes of one body or of the total in the log's words.
QString logged(const QJsonObject& entry) {
  const char* const words[3] = {"along", "under", "against"};
  QStringList parts;
  for (int c = 0; c < 3; ++c) {
    parts << QStringLiteral("%1 %2 mm^2 (%3 %)")
                 .arg(QLatin1String(words[c]), fixed(areaOf(entry, c), 1), fixed(percentOf(entry, c), 1));
  }
  return parts.join(QStringLiteral(", "));
}

// A body's name: the answer's, else `name`'s.
QString nameOf(const QJsonObject& body, const BodyNames& name) {
  const QString given = body.value(QStringLiteral("name")).toString();
  return given.isEmpty() ? name(body.value(QStringLiteral("body")).toString()) : given;
}

} // namespace

QJsonObject draftAnalysisDef(const QJsonArray& bodies, const QJsonValue& pull, const QString& occurrence,
                             bool flip, double angle) {
  QJsonObject def{{QStringLiteral("type"), QStringLiteral("draft")},
                  {QStringLiteral("bodies"), bodies},
                  {QStringLiteral("pull"), pull.isObject() || pull.isString() ? pull : QJsonValue(QJsonValue::Null)},
                  {QStringLiteral("flip"), flip},
                  {QStringLiteral("angle"), angle}};
  if ((pull.isObject() || pull.isString()) && !occurrence.isEmpty()) {
    def.insert(QStringLiteral("occurrence"), occurrence);
  }
  return def;
}

double draftAngleOf(const QJsonObject& def) {
  const QJsonValue angle = def.value(QStringLiteral("angle"));
  const double value = angle.toDouble(-1.0);
  return angle.isDouble() && std::isfinite(value) && value >= 0.0 && value < 90.0 ? value : kDefaultDraftAngle;
}

QJsonObject draftAnalysisQuery(const QJsonObject& def) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("draft_analysis")},
                      {QStringLiteral("bodies"), def.value(QStringLiteral("bodies")).toArray()},
                      {QStringLiteral("flip"), def.value(QStringLiteral("flip")).toBool()},
                      {QStringLiteral("angle"), draftAngleOf(def)}};
  const QJsonValue pull = def.value(QStringLiteral("pull"));
  if (pull.isObject() || pull.isString()) {
    request.insert(QStringLiteral("pull"), pull);
    const QJsonValue occurrence = def.value(QStringLiteral("occurrence"));
    if (occurrence.isString()) {
      request.insert(QStringLiteral("occurrence"), occurrence);
    }
  }
  return request;
}

QString runDraftAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer) {
  answer = query(draftAnalysisQuery(def), true);
  // In plain words (mitcad#1612).
  const QJsonValue error = answer.value(QStringLiteral("error"));
  return error.isString() ? userText(error.toString()) : QString();
}

QString draftAnalysisText(const QJsonObject& answer, const BodyNames& name, const std::array<QColor, 3>& colors) {
  const double angle = answer.value(QStringLiteral("angle")).toDouble(kDefaultDraftAngle);
  QStringList parts{QObject::tr("Pulled along %1; a face needs %2° of draft or more.")
                        .arg(pullText(answer), plain(angle))};
  const QJsonArray bodies = answer.value(QStringLiteral("bodies")).toArray();
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    QStringList lines{QStringLiteral("<b>%1</b>").arg(nameOf(body, name).toHtmlEscaped())};
    describe(body, angle, colors, lines);
    parts << lines.join(QStringLiteral("<br>"));
  }
  if (bodies.size() > 1) {
    QStringList lines{QStringLiteral("<b>%1</b>").arg(QObject::tr("Total"))};
    describe(answer, angle, colors, lines);
    parts << lines.join(QStringLiteral("<br>"));
  }
  parts << QObject::tr("<i>Faces hidden behind other parts of a body (undercuts) are not looked for.</i>");
  return parts.join(QStringLiteral("<br><br>"));
}

QString draftAnalysisLog(const QJsonObject& answer, const BodyNames& name) {
  QStringList bodies;
  const QJsonArray all = answer.value(QStringLiteral("bodies")).toArray();
  for (const QJsonValue& value : all) {
    const QJsonObject body = value.toObject();
    bodies << QStringLiteral("%1 %2").arg(nameOf(body, name), logged(body));
  }
  if (all.size() > 1) {
    bodies << QStringLiteral("total %1").arg(logged(answer));
  }
  return QStringLiteral("pull %1, angle %2: %3")
      .arg(pullText(answer), plain(answer.value(QStringLiteral("angle")).toDouble(kDefaultDraftAngle)),
           bodies.isEmpty() ? QStringLiteral("no body") : bodies.join(QStringLiteral("; ")));
}

QJsonObject draftOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title) {
  QJsonObject draft{{QStringLiteral("angle"), draftAngleOf(def)}};
  for (const char* const name : kClasses) {
    // Each body's triangles of the class, one array per body.
    QJsonArray parts;
    for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
      parts.append(value.toObject().value(QStringLiteral("triangles")).toObject().value(QLatin1String(name)).toArray());
    }
    draft.insert(QLatin1String(name), parts);
  }
  return {{QStringLiteral("draft"), draft}, {QStringLiteral("title"), title}};
}

} // namespace mitcad
