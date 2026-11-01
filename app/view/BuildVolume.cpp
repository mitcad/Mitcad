// SPDX-License-Identifier: MIT
// Show Build Volume (mitcad#640): the design printer's bed, centred on the
// design's origin with its surface at Z = 0, its excluded areas and its
// printable height as lines in the view, and the boxes of the shown bodies
// that do not fit (the model's `build_volume` query) in the warning colour.
#include <algorithm>
#include <cmath>
#include <exception>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QtLogging>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "../OcctViewer.hpp"
#include "../browser/DocumentHost.hpp"
#include "../framework/Theme.hpp"
#include "ViewController.hpp"

namespace mitcad {
namespace {

gp_Pnt pointAt(const QJsonValue& value, double z) {
  const QJsonArray xy = value.toArray();
  return gp_Pnt(xy.at(0).toDouble(), xy.at(1).toDouble(), z);
}

// Adds the edge from a to b, unless they are the same point.
void addEdge(BRep_Builder& builder, TopoDS_Compound& lines, const gp_Pnt& a, const gp_Pnt& b) {
  if (a.Distance(b) > 1e-9) {
    builder.Add(lines, BRepBuilderAPI_MakeEdge(a, b).Edge());
  }
}

// A closed outline at height z.
void addOutline(BRep_Builder& builder, TopoDS_Compound& lines, const QJsonArray& points, double z) {
  for (int i = 0; i < points.size(); ++i) {
    addEdge(builder, lines, pointAt(points.at(i), z), pointAt(points.at((i + 1) % points.size()), z));
  }
}

// A box's twelve edges.
void addBox(BRep_Builder& builder, TopoDS_Compound& lines, const QJsonArray& min, const QJsonArray& max) {
  const double x[2] = {min.at(0).toDouble(), max.at(0).toDouble()};
  const double y[2] = {min.at(1).toDouble(), max.at(1).toDouble()};
  const double z[2] = {min.at(2).toDouble(), max.at(2).toDouble()};
  for (int i = 0; i < 2; ++i) {
    for (int j = 0; j < 2; ++j) {
      addEdge(builder, lines, gp_Pnt(x[0], y[i], z[j]), gp_Pnt(x[1], y[i], z[j]));
      addEdge(builder, lines, gp_Pnt(x[i], y[0], z[j]), gp_Pnt(x[i], y[1], z[j]));
      addEdge(builder, lines, gp_Pnt(x[i], y[j], z[0]), gp_Pnt(x[i], y[j], z[1]));
    }
  }
}

QString number(double value) { return QString::number(std::round(value * 1000.0) / 1000.0); }

} // namespace

void ViewController::setBuildVolumeShown(bool shown) {
  m_buildVolume = shown;
  syncChecks();
  qDebug().noquote() << QStringLiteral("Show build volume %1").arg(shown ? QStringLiteral("on") : QStringLiteral("off"));
  refreshBuildVolume();
}

void ViewController::refreshBuildVolume() {
  QString logged = QStringLiteral("Build volume: hidden");
  TopoDS_Shape frame;
  TopoDS_Shape outside;
  if (m_buildVolume) {
    QJsonObject answer;
    try {
      answer = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("build_volume")}});
    } catch (const std::exception& e) {
      logged = QStringLiteral("Build volume: %1").arg(QString::fromUtf8(e.what()));
    }
    const QJsonArray bed = answer.value(QStringLiteral("bed")).toArray();
    if (bed.size() >= 3) {
      BRep_Builder builder;
      TopoDS_Compound lines;
      builder.MakeCompound(lines);
      const double height = answer.value(QStringLiteral("height")).toDouble(0.0);
      addOutline(builder, lines, bed, 0.0);
      if (height > 0.0) {
        addOutline(builder, lines, bed, height);
        for (const QJsonValue& corner : bed) {
          addEdge(builder, lines, pointAt(corner, 0.0), pointAt(corner, height));
        }
      }
      for (const QJsonValue& area : answer.value(QStringLiteral("excluded")).toArray()) {
        addOutline(builder, lines, area.toArray(), 0.0);
      }
      frame = lines;
      QStringList names;
      BRep_Builder boxBuilder;
      TopoDS_Compound boxes;
      boxBuilder.MakeCompound(boxes);
      const QJsonArray bodies = answer.value(QStringLiteral("outside")).toArray();
      for (const QJsonValue& value : bodies) {
        const QJsonObject body = value.toObject();
        addBox(boxBuilder, boxes, body.value(QStringLiteral("min")).toArray(), body.value(QStringLiteral("max")).toArray());
        const QString path = body.value(QStringLiteral("path_name")).toString();
        names << (path.isEmpty() ? body.value(QStringLiteral("name")).toString()
                                 : QStringLiteral("%1 (%2)").arg(body.value(QStringLiteral("name")).toString(), path));
      }
      if (!bodies.isEmpty()) {
        outside = boxes;
      }
      double minX = 0.0;
      double maxX = 0.0;
      double minY = 0.0;
      double maxY = 0.0;
      for (int i = 0; i < bed.size(); ++i) {
        const gp_Pnt p = pointAt(bed.at(i), 0.0);
        minX = i == 0 ? p.X() : std::min(minX, p.X());
        maxX = i == 0 ? p.X() : std::max(maxX, p.X());
        minY = i == 0 ? p.Y() : std::min(minY, p.Y());
        maxY = i == 0 ? p.Y() : std::max(maxY, p.Y());
      }
      const QJsonObject printer = answer.value(QStringLiteral("printer")).toObject();
      logged = QStringLiteral("Build volume: %1 %2 x %3 x %4 mm, outside: %5")
                   .arg(printer.value(QStringLiteral("name")).toString(tr("the design's printer")),
                        number(maxX - minX), number(maxY - minY), number(height),
                        names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
    } else if (logged == QStringLiteral("Build volume: hidden")) {
      logged = QStringLiteral("Build volume: the design's printer has no bed");
    }
  }
  m_viewer.setBuildVolume(frame, outside);
  if (logged != m_loggedBuildVolume) {
    m_loggedBuildVolume = logged;
    qDebug().noquote() << logged;
  }
}

} // namespace mitcad
