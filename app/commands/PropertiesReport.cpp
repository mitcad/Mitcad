// SPDX-License-Identifier: MIT
#include "PropertiesReport.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>

#include <QJsonArray>
#include <QJsonValue>
#include <QObject>

#include "../framework/LengthUnits.hpp"

namespace mitcad {

namespace {

// Lengths compare within 1e-6 mm; volumes within a millionth of their size.
constexpr double kLengthTolerance = 1e-6;
constexpr double kVolumeTolerance = 1e-6;

PropertiesPoint pointOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return {a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
}

bool hasPoint(const QJsonValue& value) { return value.toArray().size() == 3; }

QString unitOf(const QString& unit) { return unit.isEmpty() ? QStringLiteral("mm") : unit; }

bool sameSize(const PropertiesPoint& a, const PropertiesPoint& b) {
  for (std::size_t i = 0; i < 3; ++i) {
    if (std::abs(a[i] - b[i]) > kLengthTolerance) {
      return false;
    }
  }
  return true;
}

using Values = QVector<QPair<QString, QString>>;

void add(Values& values, const QString& label, const QString& value) { values.append(qMakePair(label, value)); }

} // namespace

QString propertiesMassText(double kilograms) {
  const QString text = QString::number(kilograms, 'f', 3);
  return (text == QStringLiteral("-0.000") ? QStringLiteral("0.000") : text) + QStringLiteral(" kg");
}

QString propertiesVolumeText(double cubicMillimetres) {
  const double size = std::abs(cubicMillimetres);
  if (size >= 1e9) {
    return plainDecimal(cubicMillimetres / 1e9, 3) + QStringLiteral(" m³");
  }
  if (size >= 1e6) {
    return plainDecimal(cubicMillimetres / 1e6, 3) + QStringLiteral(" dm³");
  }
  return plainDecimal(cubicMillimetres / 1e3, 3) + QStringLiteral(" cm³");
}

QString propertiesAreaText(double squareMillimetres) {
  if (std::abs(squareMillimetres) >= 1e6) {
    return plainDecimal(squareMillimetres / 1e6, 3) + QStringLiteral(" m²");
  }
  return plainDecimal(squareMillimetres / 1e2, 3) + QStringLiteral(" cm²");
}

QString propertiesSizeText(const PropertiesPoint& size, const QString& unit) {
  return QStringLiteral("%1 × %2 × %3 %4")
      .arg(coordinateInUnit(size[0], unit), coordinateInUnit(size[1], unit), coordinateInUnit(size[2], unit),
           unitOf(unit));
}

QString propertiesPointText(const PropertiesPoint& point, const QString& unit) {
  return QStringLiteral("(%1, %2, %3) %4")
      .arg(coordinateInUnit(point[0], unit), coordinateInUnit(point[1], unit), coordinateInUnit(point[2], unit),
           unitOf(unit));
}

PropertiesReport propertiesReport(const QJsonObject& answer, const QHash<QString, QString>& materialNames) {
  PropertiesReport report;
  for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject entry = value.toObject();
    PropertiesBody body;
    body.uid = entry.value(QStringLiteral("uid")).toString();
    body.path = entry.value(QStringLiteral("path")).toString();
    const QString bodyName = entry.value(QStringLiteral("name")).toString();
    const QString occurrence = entry.value(QStringLiteral("occurrence")).toString();
    body.logName = occurrence.isEmpty() ? bodyName : QStringLiteral("%1 in %2").arg(bodyName, occurrence);
    // Its component's own name, never the occurrence path; the body's name
    // too where the component has others.
    const QString component = entry.value(QStringLiteral("component_name")).toString();
    if (component.isEmpty()) {
      body.name = bodyName;
    } else if (entry.value(QStringLiteral("component_bodies")).toInt(1) > 1) {
      body.name = QObject::tr("%1 (%2)").arg(component, bodyName);
    } else {
      body.name = component;
    }
    // No material set anywhere: said in words, without a mass (the model's
    // built-in default only stands in for agents).
    body.materialId = entry.value(QStringLiteral("material")).toString();
    body.hasMaterial = !entry.value(QStringLiteral("no_material")).toBool() && !body.materialId.isEmpty();
    if (body.hasMaterial) {
      body.material = materialNames.value(body.materialId, body.materialId);
    } else {
      body.materialId.clear();
      body.material = QObject::tr("No material");
    }
    body.density = entry.value(QStringLiteral("density")).toDouble();
    body.mass = body.hasMaterial ? entry.value(QStringLiteral("mass")).toDouble() : 0.0;
    body.volume = entry.value(QStringLiteral("volume")).toDouble();
    body.area = entry.value(QStringLiteral("area")).toDouble();
    body.center = pointOf(entry.value(QStringLiteral("center_of_mass")));
    const QJsonObject box = entry.value(QStringLiteral("bbox")).toObject();
    if (hasPoint(box.value(QStringLiteral("min"))) && hasPoint(box.value(QStringLiteral("max")))) {
      body.hasBox = true;
      body.min = pointOf(box.value(QStringLiteral("min")));
      body.max = pointOf(box.value(QStringLiteral("max")));
      for (std::size_t i = 0; i < 3; ++i) {
        body.size[i] = body.max[i] - body.min[i];
      }
    }
    if (hasPoint(entry.value(QStringLiteral("principal_moments")))) {
      body.moments = pointOf(entry.value(QStringLiteral("principal_moments")));
    }
    if (entry.contains(QStringLiteral("faces"))) {
      body.hasCounts = true;
      body.faces = entry.value(QStringLiteral("faces")).toInt();
      body.edges = entry.value(QStringLiteral("edges")).toInt();
      body.vertices = entry.value(QStringLiteral("vertices")).toInt();
    }
    if (body.hasMaterial) {
      for (const QJsonValue& row : entry.value(QStringLiteral("material_properties")).toArray()) {
        const QJsonObject property = row.toObject();
        add(body.materialValues, property.value(QStringLiteral("name")).toString(),
            property.value(QStringLiteral("text")).toString());
      }
    }
    body.part = entry.value(QStringLiteral("part")).toString();
    report.bodies.push_back(std::move(body));
  }

  // The rows: each body joins the first row whose first body it equals.
  for (int i = 0; i < static_cast<int>(report.bodies.size()); ++i) {
    bool joined = false;
    for (std::vector<int>& row : report.rows) {
      if (propertiesEqual(report.bodies[static_cast<std::size_t>(row.front())],
                          report.bodies[static_cast<std::size_t>(i)])) {
        row.push_back(i);
        joined = true;
        break;
      }
    }
    if (!joined) {
      report.rows.push_back({i});
    }
  }

  PropertiesSummary& summary = report.summary;
  summary.count = static_cast<int>(report.bodies.size());
  PropertiesPoint weighted{};
  for (const PropertiesBody& body : report.bodies) {
    summary.volume += body.volume;
    if (body.hasMaterial) {
      summary.mass += body.mass;
      for (std::size_t i = 0; i < 3; ++i) {
        weighted[i] += body.mass * body.center[i];
      }
    } else {
      ++summary.withoutMaterial;
    }
    if (body.hasBox) {
      for (std::size_t i = 0; i < 3; ++i) {
        summary.min[i] = summary.hasBox ? std::min(summary.min[i], body.min[i]) : body.min[i];
        summary.max[i] = summary.hasBox ? std::max(summary.max[i], body.max[i]) : body.max[i];
      }
      summary.hasBox = true;
    }
  }
  if (summary.mass > 0.0) {
    summary.center = PropertiesPoint{weighted[0] / summary.mass, weighted[1] / summary.mass,
                                     weighted[2] / summary.mass};
  }
  for (std::size_t i = 0; i < 3; ++i) {
    summary.size[i] = summary.max[i] - summary.min[i];
  }
  return report;
}

bool propertiesEqual(const PropertiesBody& a, const PropertiesBody& b) {
  if (a.hasMaterial != b.hasMaterial || a.materialId != b.materialId) {
    return false;
  }
  if (std::abs(a.volume - b.volume) > kVolumeTolerance * std::max(1.0, std::abs(a.volume))) {
    return false;
  }
  if (a.uid == b.uid || (!a.part.isEmpty() && a.part == b.part)) {
    return true;
  }
  return a.hasBox && b.hasBox && sameSize(a.size, b.size);
}

QStringList propertiesHeadings() {
  return {QObject::tr("Quantity"), QObject::tr("Name"), QObject::tr("Material"), QObject::tr("Mass"),
          QObject::tr("Volume"), QObject::tr("Size")};
}

QStringList propertiesRowCells(const PropertiesReport& report, int row, const QString& unit) {
  const std::vector<int>& members = report.rows.at(static_cast<std::size_t>(row));
  const PropertiesBody& first = report.bodies.at(static_cast<std::size_t>(members.front()));
  // Bodies merged by their size and volume may have other names: each once.
  QStringList names;
  for (const int i : members) {
    const QString& name = report.bodies.at(static_cast<std::size_t>(i)).name;
    if (!names.contains(name)) {
      names << name;
    }
  }
  return {QString::number(static_cast<int>(members.size())), names.join(QStringLiteral(", ")), first.material,
          first.hasMaterial ? propertiesMassText(first.mass) : QString(), propertiesVolumeText(first.volume),
          first.hasBox ? propertiesSizeText(first.size, unit) : QString()};
}

QVector<QPair<QString, QString>> propertiesSummaryValues(const PropertiesSummary& summary, const QString& unit) {
  Values values;
  add(values, QObject::tr("Bodies"), QString::number(summary.count));
  add(values, QObject::tr("Mass"), propertiesMassText(summary.mass));
  if (summary.withoutMaterial > 0) {
    add(values, QObject::tr("Bodies without material"), QString::number(summary.withoutMaterial));
  }
  add(values, QObject::tr("Volume"), propertiesVolumeText(summary.volume));
  if (summary.center) {
    add(values, QObject::tr("Center of Mass"), propertiesPointText(*summary.center, unit));
  }
  if (summary.hasBox) {
    add(values, QObject::tr("Bounding Box"), propertiesSizeText(summary.size, unit));
  }
  return values;
}

PropertiesGroups propertiesGroups(const PropertiesBody& body, const QString& unit) {
  PropertiesGroups groups;
  add(groups.physical, QObject::tr("Material"), body.material);
  if (body.hasMaterial) {
    add(groups.physical, QObject::tr("Density"), QStringLiteral("%1 g/cm³").arg(plainDecimal(body.density, 4)));
    groups.physical += body.materialValues;
    add(groups.physical, QObject::tr("Mass"), propertiesMassText(body.mass));
  }
  add(groups.physical, QObject::tr("Volume"), propertiesVolumeText(body.volume));
  add(groups.physical, QObject::tr("Area"), propertiesAreaText(body.area));
  // The center of the body's volume where no material gives it a mass:
  // the same point for any one density.
  add(groups.place, QObject::tr("Center of Mass"), propertiesPointText(body.center, unit));
  if (body.hasBox) {
    add(groups.place, QObject::tr("Bounding Box"), propertiesSizeText(body.size, unit));
  }
  if (body.moments && body.hasMaterial) {
    const PropertiesPoint& m = *body.moments;
    add(groups.more, QObject::tr("Principal Moments"),
        QStringLiteral("%1, %2, %3 kg mm²").arg(plainDecimal(m[0], 3), plainDecimal(m[1], 3), plainDecimal(m[2], 3)));
  }
  if (body.hasCounts) {
    add(groups.more, QObject::tr("Faces / Edges / Vertices"),
        QStringLiteral("%1 / %2 / %3").arg(body.faces).arg(body.edges).arg(body.vertices));
  }
  return groups;
}

QString propertiesCopyText(const PropertiesReport& report, const QString& unit) {
  QStringList lines;
  const auto pairs = [&lines](const Values& values) {
    for (const auto& [label, value] : values) {
      lines << label + QLatin1Char('\t') + value;
    }
  };
  if (report.bodies.size() == 1) {
    const PropertiesGroups groups = propertiesGroups(report.bodies.front(), unit);
    lines << report.bodies.front().name;
    pairs(groups.physical);
    pairs(groups.place);
    pairs(groups.more);
    return lines.join(QLatin1Char('\n'));
  }
  pairs(propertiesSummaryValues(report.summary, unit));
  lines << QString();
  lines << propertiesHeadings().join(QLatin1Char('\t'));
  for (int row = 0; row < static_cast<int>(report.rows.size()); ++row) {
    lines << propertiesRowCells(report, row, unit).join(QLatin1Char('\t'));
  }
  return lines.join(QLatin1Char('\n'));
}

QString propertiesLogText(QString text) {
  return text.replace(QChar(0x00D7), QLatin1Char('x')).replace(QChar(0x00B3), QLatin1Char('3'))
      .replace(QChar(0x00B2), QLatin1Char('2'));
}

} // namespace mitcad
