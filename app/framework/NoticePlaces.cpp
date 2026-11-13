// SPDX-License-Identifier: MIT
#include "NoticePlaces.hpp"

#include <QJsonValue>

namespace mitcad {

QJsonArray noticePlaces(const QJsonArray& notices) {
  QJsonArray places;
  for (const QJsonValue& notice : notices) {
    for (const QJsonValue& place : notice.toObject().value(QStringLiteral("places")).toArray()) {
      if (!places.contains(place)) {
        places.append(place);
      }
    }
  }
  return places;
}

QStringList noticeMessages(const QJsonObject& result, const QString& level) {
  QStringList messages;
  for (const QJsonValue& value : result.value(QStringLiteral("notices")).toArray()) {
    const QJsonObject notice = value.toObject();
    if (notice.value(QStringLiteral("level")).toString() == level) {
      messages << notice.value(QStringLiteral("message")).toString();
    }
  }
  return messages;
}

QString placesText(const QJsonArray& places) {
  QStringList parts;
  for (const QJsonValue& value : places) {
    const QJsonObject place = value.toObject();
    const QString kind = place.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("point") || kind == QStringLiteral("box")) {
      const QJsonArray at = place.value(kind == QStringLiteral("point") ? QStringLiteral("at") : QStringLiteral("min"))
                                .toArray();
      parts << QStringLiteral("%1 %2,%3,%4")
                   .arg(kind)
                   .arg(at.at(0).toDouble(), 0, 'f', 3)
                   .arg(at.at(1).toDouble(), 0, 'f', 3)
                   .arg(at.at(2).toDouble(), 0, 'f', 3);
      continue;
    }
    if (kind == QStringLiteral("occurrence")) {
      parts << QStringLiteral("occurrence %1").arg(place.value(QStringLiteral("path")).toString());
      continue;
    }
    // A face's, an edge's or a vertex's body; a region's sketch (mitcad#1334).
    const QString body = place.value(QStringLiteral("body")).toString();
    const QString owner = body.isEmpty() ? place.value(QStringLiteral("sketch")).toString() : body;
    parts << QStringLiteral("%1 %2%3")
                 .arg(kind, place.value(QStringLiteral("name")).toString(),
                      owner.isEmpty() ? QString() : QStringLiteral(" of ") + owner);
  }
  return parts.isEmpty() ? QStringLiteral("none") : parts.join(QStringLiteral(", "));
}

QString noticeHoverText(const QString& source, const QJsonArray& places, int drawn) {
  if (places.isEmpty()) {
    return QStringLiteral("none");
  }
  const QString count = drawn < 0 ? QStringLiteral("drawn when the job ends") : QStringLiteral("%1 drawn").arg(drawn);
  return QStringLiteral("%1: %2 (%3)").arg(source, placesText(places), count);
}

} // namespace mitcad
