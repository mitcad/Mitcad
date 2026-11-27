// SPDX-License-Identifier: MIT
#include "ContinuityAnalysis.hpp"

#include <algorithm>
#include <cstddef>
#include <iterator>

#include <QHash>
#include <QJsonValue>
#include <QObject>
#include <QStringList>

#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRep_Builder.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

namespace mitcad {

const std::array<const char*, 4> kContinuityClasses = {{"g2", "g1", "g0", "none"}};

namespace {

// A number with `decimals` decimals, always with a point, never "-0.0".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("0.01", "2").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

// How bad a class is: "none" the worst (0), "g2" the best (3); an edge not
// measured after all of them.
int classRank(const QString& name) {
  for (std::size_t c = 0; c < kContinuityClasses.size(); ++c) {
    if (name == QLatin1String(kContinuityClasses[c])) {
      return static_cast<int>(kContinuityClasses.size() - 1 - c);
    }
  }
  return static_cast<int>(kContinuityClasses.size());
}

// A class in a listed edge's words.
QString classShort(const QString& name) {
  if (name == QStringLiteral("g2")) {
    return QStringLiteral("G2");
  }
  if (name == QStringLiteral("g1")) {
    return QStringLiteral("G1");
  }
  if (name == QStringLiteral("g0")) {
    return QStringLiteral("G0");
  }
  return QObject::tr("not touching");
}

double settingOf(const QJsonObject& answer, const char* key, double fallback) {
  const QJsonValue value = answer.value(QLatin1String(key));
  return value.isDouble() ? value.toDouble() : fallback;
}

// The edge's polyline through its sampled points; null when it has fewer
// than two apart.
TopoDS_Shape polylineOf(const QJsonArray& points) {
  std::vector<gp_Pnt> at;
  for (const QJsonValue& value : points) {
    const QJsonArray p = value.toArray();
    if (p.size() == 3) {
      at.emplace_back(p.at(0).toDouble(), p.at(1).toDouble(), p.at(2).toDouble());
    }
  }
  // A closed edge (a circle) ends where it starts: closed, not run back.
  const bool closed = at.size() > 2 && at.front().Distance(at.back()) < 1e-7;
  if (closed) {
    at.pop_back();
  }
  if (at.size() < 2) {
    return TopoDS_Shape();
  }
  try {
    BRepBuilderAPI_MakePolygon polygon;
    for (const gp_Pnt& p : at) {
      polygon.Add(p);
    }
    if (closed) {
      polygon.Close();
    }
    return polygon.IsDone() ? TopoDS_Shape(polygon.Wire()) : TopoDS_Shape();
  } catch (const Standard_Failure&) {
    return TopoDS_Shape();
  }
}

} // namespace

ColorRole continuityRole(const QString& className) {
  if (className == QStringLiteral("g2")) {
    return ColorRole::ContinuityG2;
  }
  if (className == QStringLiteral("g1")) {
    return ColorRole::ContinuityG1;
  }
  if (className == QStringLiteral("g0")) {
    return ColorRole::ContinuityG0;
  }
  return ColorRole::ContinuityNone;
}

QJsonArray continuityBodies(const Selection& picks) {
  struct Entry {
    QString body;
    QString occurrence;
    bool whole = false;
    QStringList edges;
    QStringList faces;
  };
  std::vector<Entry> entries;
  for (const SelectionItem& item : picks) {
    const bool body = item.kind == SelectKind::Body;
    const bool edge = item.kind == SelectKind::Edge;
    const bool face = item.kind == SelectKind::Face;
    if (!(body || edge || face) || item.owner.isEmpty() || (!body && item.name.isEmpty())) {
      continue;
    }
    auto entry = std::find_if(entries.begin(), entries.end(), [&item](const Entry& e) {
      return e.body == item.owner && e.occurrence == item.occurrence;
    });
    if (entry == entries.end()) {
      Entry added;
      added.body = item.owner;
      added.occurrence = item.occurrence;
      entries.push_back(added);
      entry = std::prev(entries.end());
    }
    if (body) {
      entry->whole = true;
    } else if (edge && !entry->edges.contains(item.name)) {
      entry->edges << item.name;
    } else if (face && !entry->faces.contains(item.name)) {
      entry->faces << item.name;
    }
  }
  QJsonArray out;
  for (const Entry& entry : entries) {
    QJsonObject body{{QStringLiteral("body"), entry.body}};
    if (!entry.occurrence.isEmpty()) {
      body.insert(QStringLiteral("occurrence"), entry.occurrence);
    }
    if (!entry.whole && !entry.edges.isEmpty()) {
      body.insert(QStringLiteral("edges"), QJsonArray::fromStringList(entry.edges));
    }
    if (!entry.whole && !entry.faces.isEmpty()) {
      body.insert(QStringLiteral("faces"), QJsonArray::fromStringList(entry.faces));
    }
    out.append(body);
  }
  return out;
}

QJsonObject continuityQuery(const QJsonArray& bodies, int samples, double gap, double angle, double curvature) {
  return {{QStringLiteral("query"), QStringLiteral("edge_continuity")},
          {QStringLiteral("bodies"), bodies},
          {QStringLiteral("samples"), samples},
          {QStringLiteral("gap"), gap},
          {QStringLiteral("angle"), angle},
          {QStringLiteral("curvature"), curvature}};
}

QString continuityText(const QJsonObject& answer, const std::array<QColor, 4>& colors) {
  const double gap = settingOf(answer, "gap", kDefaultContinuityGap);
  const double angle = settingOf(answer, "angle", kDefaultContinuityAngle);
  const double curvature = settingOf(answer, "curvature", kDefaultContinuityCurvature);
  const QJsonArray edges = answer.value(QStringLiteral("edges")).toArray();
  const QJsonObject counts = answer.value(QStringLiteral("counts")).toObject();
  QStringList parts{QObject::tr("Within a gap of %1 mm, an angle of %2° and a curvature difference of %3 %.")
                        .arg(plain(gap), plain(angle), plain(curvature))};
  const QString words[4] = {
      QObject::tr("G2, curvature continuous"),
      QObject::tr("G1, tangent"),
      QObject::tr("G0, touching"),
      QObject::tr("Not touching"),
  };
  QStringList lines{QStringLiteral("<b>%1</b>").arg(QObject::tr("%n edge(s)", nullptr, static_cast<int>(edges.size())))};
  for (std::size_t c = 0; c < kContinuityClasses.size(); ++c) {
    lines << QStringLiteral("<span style='color:%1'>&#9632;</span> %2: %3")
                 .arg(colors[c].name(), words[c])
                 .arg(counts.value(QLatin1String(kContinuityClasses[c])).toInt());
  }
  const int unmeasured = counts.value(QStringLiteral("unmeasured")).toInt();
  if (unmeasured > 0) {
    lines << QObject::tr("%n edge(s) not measured: free edges, or no normal on a face", nullptr, unmeasured);
  }
  parts << lines.join(QStringLiteral("<br>"));
  // The edges measured, the worst first, each named by its body and its
  // place among that body's edges in the answer.
  struct Listed {
    int rank;
    double angle;
    double curvature;
    double gap;
    QString text;
  };
  std::vector<Listed> listed;
  QHash<QString, int> numbers;
  for (const QJsonValue& value : edges) {
    const QJsonObject edge = value.toObject();
    const QString key =
        edge.value(QStringLiteral("body")).toString() + QLatin1Char('|') + edge.value(QStringLiteral("occurrence")).toString();
    const int number = ++numbers[key];
    const QString name = edge.value(QStringLiteral("class")).toString();
    if (!edge.value(QStringLiteral("angle")).isDouble()) {
      continue; // not measured
    }
    Listed row;
    row.rank = classRank(name);
    row.angle = edge.value(QStringLiteral("angle")).toDouble();
    row.curvature = edge.value(QStringLiteral("curvature_percent")).toDouble();
    row.gap = edge.value(QStringLiteral("gap")).toDouble();
    row.text = QObject::tr("%1, edge %2: %3, gap %4 mm, angle %5°, curvature %6 %")
                   .arg(edge.value(QStringLiteral("name")).toString().toHtmlEscaped())
                   .arg(number)
                   .arg(classShort(name), fixed(row.gap, 4), fixed(row.angle, 3), fixed(row.curvature, 1));
    listed.push_back(row);
  }
  if (!listed.empty()) {
    const QJsonObject worst = answer.value(QStringLiteral("worst")).toObject();
    parts << QObject::tr("Worst: gap %1 mm, angle %2°, curvature %3 %")
                 .arg(fixed(worst.value(QStringLiteral("gap")).toDouble(), 4),
                      fixed(worst.value(QStringLiteral("angle")).toDouble(), 3),
                      fixed(worst.value(QStringLiteral("curvature_percent")).toDouble(), 1));
    std::stable_sort(listed.begin(), listed.end(), [](const Listed& a, const Listed& b) {
      if (a.rank != b.rank) {
        return a.rank < b.rank;
      }
      if (a.angle != b.angle) {
        return a.angle > b.angle;
      }
      if (a.curvature != b.curvature) {
        return a.curvature > b.curvature;
      }
      return a.gap > b.gap;
    });
    QStringList rows{QStringLiteral("<b>%1</b>").arg(QObject::tr("Worst edges"))};
    for (std::size_t i = 0; i < listed.size() && i < static_cast<std::size_t>(kContinuityListed); ++i) {
      rows << listed[i].text;
    }
    parts << rows.join(QStringLiteral("<br>"));
  }
  return parts.join(QStringLiteral("<br><br>"));
}

QString continuityLog(const QJsonObject& answer) {
  const QJsonObject counts = answer.value(QStringLiteral("counts")).toObject();
  QStringList classes;
  for (const char* const name : kContinuityClasses) {
    classes << QStringLiteral("%1 %2").arg(QLatin1String(name)).arg(counts.value(QLatin1String(name)).toInt());
  }
  classes << QStringLiteral("unmeasured %1").arg(counts.value(QStringLiteral("unmeasured")).toInt());
  const QJsonObject worst = answer.value(QStringLiteral("worst")).toObject();
  return QStringLiteral("%1 edges: %2; worst gap %3 mm, angle %4 deg, curvature %5 %")
      .arg(answer.value(QStringLiteral("edges")).toArray().size())
      .arg(classes.join(QStringLiteral(", ")), fixed(worst.value(QStringLiteral("gap")).toDouble(), 4),
           fixed(worst.value(QStringLiteral("angle")).toDouble(), 3),
           fixed(worst.value(QStringLiteral("curvature_percent")).toDouble(), 1));
}

std::vector<std::pair<TopoDS_Shape, ColorRole>> continuityEdges(const QJsonObject& answer) {
  const QJsonArray edges = answer.value(QStringLiteral("edges")).toArray();
  std::vector<std::pair<TopoDS_Shape, ColorRole>> drawn;
  for (const char* const name : kContinuityClasses) {
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    bool any = false;
    for (const QJsonValue& value : edges) {
      const QJsonObject edge = value.toObject();
      if (edge.value(QStringLiteral("class")).toString() != QLatin1String(name)) {
        continue;
      }
      const TopoDS_Shape line = polylineOf(edge.value(QStringLiteral("points")).toArray());
      if (!line.IsNull()) {
        builder.Add(compound, line);
        any = true;
      }
    }
    if (any) {
      drawn.emplace_back(compound, continuityRole(QLatin1String(name)));
    }
  }
  return drawn;
}

} // namespace mitcad
