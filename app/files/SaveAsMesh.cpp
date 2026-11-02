// SPDX-License-Identifier: MIT
#include "SaveAsMesh.hpp"

#include <algorithm>
#include <utility>

#include <QJsonValue>
#include <QObject>
#include <QtMath>

#include "Print3d.hpp"

namespace mitcad {

MeshPlan planSaveAsMesh(const QVector<MeshItem>& items, const QJsonArray& instances,
                        const QHash<QString, MeshBody>& bodies, const QHash<QString, QString>& components,
                        const QString& design) {
  MeshPlan plan;
  // The items once each (two faces of a body are the body), and every body
  // they stand for with the occurrences it came from.
  QVector<MeshItem> distinct;
  QStringList found;
  QHash<QString, QStringList> from;
  const auto add = [&found, &from](const QString& body, const QString& occurrence) {
    if (!found.contains(body)) {
      found << body;
    }
    if (!from[body].contains(occurrence)) {
      from[body] << occurrence;
    }
  };
  for (const MeshItem& item : items) {
    const bool seen = std::any_of(distinct.begin(), distinct.end(), [&item](const MeshItem& other) {
      return other.kind == item.kind && other.uid == item.uid && other.occurrence == item.occurrence;
    });
    if (seen) {
      continue;
    }
    distinct << item;
    if (item.kind == MeshItem::Kind::Body) {
      add(item.uid, item.occurrence);
      continue;
    }
    // A component's bodies: those placed in its occurrence and in the
    // occurrences in it; the root's, every body.
    for (const QJsonValue& value : instances) {
      const QJsonObject instance = value.toObject();
      QStringList path;
      for (const QJsonValue& uid : instance.value(QStringLiteral("path")).toArray()) {
        path << uid.toString();
      }
      const QString placed = path.join(QLatin1Char('/'));
      if (item.occurrence.isEmpty() || placed == item.occurrence ||
          placed.startsWith(item.occurrence + QLatin1Char('/'))) {
        add(instance.value(QStringLiteral("body")).toString(), item.occurrence);
      }
    }
  }
  QStringList paths;
  for (const MeshItem& item : std::as_const(distinct)) {
    if (!paths.contains(item.occurrence)) {
      paths << item.occurrence;
    }
  }
  if (paths.size() == 1) {
    plan.occurrence = paths.first();
  }
  for (const QString& uid : std::as_const(found)) {
    const auto body = bodies.constFind(uid);
    if (body == bodies.constEnd()) {
      continue;
    }
    if (!body->printable) {
      plan.skipped << body->name;
      continue;
    }
    plan.bodies << uid;
    plan.names << body->name;
    const QStringList& mine = from[uid];
    plan.occurrences << (mine.size() == 1 ? mine.first() : QString());
  }

  // One body: its name; one component: its name; else the design's.
  QHash<QString, QString> bodyNames;
  for (auto body = bodies.constBegin(); body != bodies.constEnd(); ++body) {
    bodyNames.insert(body.key(), body.value().name);
  }
  plan.name = meshItemsName(distinct, bodyNames, components, design);

  if (plan.bodies.isEmpty()) {
    plan.blocked = found.isEmpty() ? QObject::tr("There are no bodies to save")
                                   : QObject::tr("Surface and empty bodies are not saved as meshes");
  }
  QString bedError;
  bool bed = false;
  for (const QString& uid : std::as_const(plan.bodies)) {
    const MeshBody body = bodies.value(uid);
    bed = bed || body.bedFace;
    if (bedError.isEmpty() && !body.bedError.isEmpty()) {
      bedError = QStringLiteral("%1: %2").arg(body.name, body.bedError);
    }
  }
  if (!bed) {
    plan.bedBlocked = !bedError.isEmpty() ? bedError : QObject::tr("No body has a print bed face");
  }
  return plan;
}

QString meshItemsName(const QVector<MeshItem>& items, const QHash<QString, QString>& bodyNames,
                      const QHash<QString, QString>& components, const QString& design) {
  QVector<MeshItem> distinct;
  for (const MeshItem& item : items) {
    const bool seen = std::any_of(distinct.begin(), distinct.end(), [&item](const MeshItem& other) {
      return other.kind == item.kind && other.uid == item.uid && other.occurrence == item.occurrence;
    });
    if (!seen) {
      distinct << item;
    }
  }
  QString name = design;
  if (distinct.size() == 1) {
    const MeshItem& item = distinct.first();
    if (item.kind == MeshItem::Kind::Body) {
      name = bodyNames.value(item.uid);
    } else if (!item.occurrence.isEmpty()) {
      name = components.value(item.uid, design);
    }
  }
  return name.trimmed().isEmpty() ? design : name;
}

QStringList meshFileNames(const MeshPlan& plan, bool perBody, const QString& extension) {
  if (perBody) {
    return printFileNames(plan.names, extension);
  }
  return {safeFileName(plan.name) + QLatin1Char('.') + extension};
}

QString SaveMeshOptions::describe() const {
  QString refined = refinement;
  if (refinement == QStringLiteral("custom")) {
    refined += QStringLiteral(" %1 mm %2 deg").arg(deviation).arg(angle);
    refined += maxEdge > 0.0 ? QStringLiteral(" %1 mm").arg(maxEdge) : QStringLiteral(" no edge limit");
  }
  return QStringLiteral("%1, %2, %3, %4%5")
      .arg(format + (format == QStringLiteral("stl") ? (ascii ? QStringLiteral(" ascii") : QStringLiteral(" binary"))
                                                      : QString()),
           unit, perBody ? QStringLiteral("a file per body") : QStringLiteral("one file"), refined,
           onBed ? QStringLiteral(", on bed") : QString());
}

QJsonObject meshExportCommand(const QString& path, const QStringList& bodies, const QString& occurrence,
                              const SaveMeshOptions& options, const QJsonObject& colors) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("export")},
                      {QStringLiteral("path"), path},
                      {QStringLiteral("format"), options.format},
                      {QStringLiteral("bodies"), QJsonArray::fromStringList(bodies)},
                      {QStringLiteral("unit"), options.unit}};
  if (options.refinement == QStringLiteral("custom")) {
    QJsonObject refinement{{QStringLiteral("deviation"), options.deviation},
                           {QStringLiteral("angle"), qDegreesToRadians(options.angle)}};
    if (options.maxEdge > 0.0) {
      refinement.insert(QStringLiteral("max_edge"), options.maxEdge);
    }
    command.insert(QStringLiteral("refinement"), refinement);
  } else {
    command.insert(QStringLiteral("refinement"), options.refinement);
  }
  if (options.format == QStringLiteral("stl")) {
    command.insert(QStringLiteral("ascii"), options.ascii);
  } else if (!colors.isEmpty()) {
    // The colours the view shows (3MF, OBJ).
    command.insert(QStringLiteral("colors"), colors);
  }
  if (!occurrence.isEmpty()) {
    command.insert(QStringLiteral("occurrence"), occurrence);
  }
  if (options.onBed) {
    command.insert(QStringLiteral("on_bed"), true);
  }
  return command;
}

} // namespace mitcad
