// SPDX-License-Identifier: MIT
// The Properties window's report (mitcad#1816): bodies named by their
// component, "No material" without a mass, equal bodies merged into one
// row (the same size and volume, the same placed part, or the same body
// placed twice), the summary of several bodies, even units and the text
// Copy puts on the clipboard.
#include "commands/PropertiesReport.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "properties_report.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonArray xyz(double x, double y, double z) {
  QJsonArray a;
  a.append(x);
  a.append(y);
  a.append(z);
  return a;
}

// A box body from `min` to `max` with a material (empty: none set) of
// `density` g/cm³.
QJsonObject box(const QString& uid, const QString& name, const QString& material, double density,
                std::array<double, 3> min, std::array<double, 3> max) {
  const double volume = (max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2]);
  QJsonObject body{{QStringLiteral("uid"), uid},
                   {QStringLiteral("name"), name},
                   {QStringLiteral("material"), material.isEmpty() ? QStringLiteral("steel") : material},
                   {QStringLiteral("density"), material.isEmpty() ? 7.85 : density},
                   {QStringLiteral("volume"), volume},
                   {QStringLiteral("area"), 2.0 * ((max[0] - min[0]) * (max[1] - min[1]) +
                                                   (max[1] - min[1]) * (max[2] - min[2]) +
                                                   (max[0] - min[0]) * (max[2] - min[2]))},
                   {QStringLiteral("mass"), volume * (material.isEmpty() ? 7.85 : density) * 1e-6},
                   {QStringLiteral("center_of_mass"),
                    xyz((min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2)},
                   {QStringLiteral("principal_moments"), xyz(1, 2, 3)},
                   {QStringLiteral("bbox"), QJsonObject{{QStringLiteral("min"), xyz(min[0], min[1], min[2])},
                                                        {QStringLiteral("max"), xyz(max[0], max[1], max[2])}}},
                   {QStringLiteral("faces"), 6},
                   {QStringLiteral("edges"), 12},
                   {QStringLiteral("vertices"), 8}};
  if (material.isEmpty()) {
    body.insert(QStringLiteral("no_material"), true);
  }
  return body;
}

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9; }

const QHash<QString, QString> kNames{{QStringLiteral("steel"), QStringLiteral("Steel")},
                                     {QStringLiteral("aluminum"), QStringLiteral("Aluminum")}};

void unitTests() {
  CHECK(propertiesMassText(3.6719) == QStringLiteral("3.672 kg"));
  CHECK(propertiesMassText(17.8038) == QStringLiteral("17.804 kg"));
  CHECK(propertiesMassText(0.0) == QStringLiteral("0.000 kg"));
  CHECK(propertiesVolumeText(8000) == QStringLiteral("8 cm³"));
  CHECK(propertiesVolumeText(5400000) == QStringLiteral("5.4 dm³"));
  CHECK(propertiesVolumeText(2.5e9) == QStringLiteral("2.5 m³"));
  CHECK(propertiesAreaText(2400) == QStringLiteral("24 cm²"));
  CHECK(propertiesAreaText(2720400) == QStringLiteral("2.72 m²"));
  CHECK(propertiesSizeText({1800, 28, 45}, QStringLiteral("mm")) == QStringLiteral("1800 × 28 × 45 mm"));
  CHECK(propertiesSizeText({25.4, 50.8, 12.7}, QStringLiteral("in")) == QStringLiteral("1 × 2 × 0.5 in"));
  CHECK(propertiesPointText({10, 20, 5}, QString()) == QStringLiteral("(10, 20, 5) mm"));
  CHECK(propertiesLogText(QStringLiteral("8 cm³, 24 cm², 1 × 2")) == QStringLiteral("8 cm3, 24 cm2, 1 x 2"));
}

// A plate of aluminium, two equal steel cubes in other places and a box
// without a material.
void tableTests() {
  QJsonArray bodies;
  bodies.append(box(QStringLiteral("F1.b0"), QStringLiteral("Body1"), QStringLiteral("aluminum"), 2.7, {0, 0, 0},
                    {100, 50, 5}));
  bodies.append(box(QStringLiteral("F2.b0"), QStringLiteral("Body2"), QStringLiteral("steel"), 7.85, {0, 0, 5},
                    {20, 20, 25}));
  bodies.append(box(QStringLiteral("F3.b0"), QStringLiteral("Body3"), QStringLiteral("steel"), 7.85, {40, 0, 5},
                    {60, 20, 25}));
  bodies.append(
      box(QStringLiteral("F4.b0"), QStringLiteral("Body4"), QString(), 0.0, {70, 0, 5}, {80, 20, 35}));
  const PropertiesReport report = propertiesReport(QJsonObject{{QStringLiteral("bodies"), bodies}}, kNames);
  CHECK(report.bodies.size() == 4);
  CHECK(report.rows.size() == 3);
  CHECK(report.rows.size() == 3 && report.rows[1].size() == 2);
  // No material: in words, no mass, out of the total.
  CHECK(!report.bodies[3].hasMaterial);
  CHECK(report.bodies[3].material == QStringLiteral("No material"));
  CHECK(report.bodies[3].mass == 0.0);
  CHECK(report.bodies[0].material == QStringLiteral("Aluminum"));
  const PropertiesSummary& s = report.summary;
  CHECK(s.count == 4);
  CHECK(s.withoutMaterial == 1);
  CHECK(nearly(s.mass, 25000 * 2.7e-6 + 2 * 8000 * 7.85e-6));
  CHECK(nearly(s.volume, 25000 + 16000 + 6000));
  // Weighted by the masses of the bodies with a material.
  const double plate = 25000 * 2.7e-6;
  const double cube = 8000 * 7.85e-6;
  CHECK(s.center.has_value() && nearly((*s.center)[0], (plate * 50 + cube * 10 + cube * 50) / s.mass));
  CHECK(s.hasBox && nearly(s.size[0], 100) && nearly(s.size[1], 50) && nearly(s.size[2], 35));

  const QStringList cube2 = propertiesRowCells(report, 1, QStringLiteral("mm"));
  CHECK(cube2 == (QStringList{QStringLiteral("2"), QStringLiteral("Body2, Body3"), QStringLiteral("Steel"),
                              QStringLiteral("0.063 kg"), QStringLiteral("8 cm³"),
                              QStringLiteral("20 × 20 × 20 mm")}));
  const QStringList none = propertiesRowCells(report, 2, QStringLiteral("mm"));
  CHECK(none.size() == 6 && none[2] == QStringLiteral("No material") && none[3].isEmpty());

  // One body's groups: no density, mass or moments without a material.
  const PropertiesGroups groups = propertiesGroups(report.bodies[3], QStringLiteral("mm"));
  CHECK(groups.physical.size() == 3 && groups.physical[0].second == QStringLiteral("No material"));
  CHECK(groups.place.size() == 2);
  CHECK(groups.more.size() == 1 && groups.more[0].second == QStringLiteral("6 / 12 / 8"));
  const PropertiesGroups steel = propertiesGroups(report.bodies[1], QStringLiteral("mm"));
  CHECK(steel.more.size() == 2);

  // Copy: the summary, an empty line, the headings and the rows.
  const QStringList lines = propertiesCopyText(report, QStringLiteral("mm")).split(QLatin1Char('\n'));
  CHECK(lines.size() == 6 + 1 + 1 + 3);
  CHECK(lines.value(0) == QStringLiteral("Bodies\t4"));
  CHECK(lines.value(1) == QStringLiteral("Mass\t0.193 kg"));
  CHECK(lines.value(2) == QStringLiteral("Bodies without material\t1"));
  CHECK(lines.value(6).isEmpty());
  CHECK(lines.value(7) == QStringLiteral("Quantity\tName\tMaterial\tMass\tVolume\tSize"));
  CHECK(lines.value(9) == QStringLiteral("2\tBody2, Body3\tSteel\t0.063 kg\t8 cm³\t20 × 20 × 20 mm"));
}

// Names: the component's, with the body's only when it has more than one;
// a body placed twice, and equal placed parts turned another way, are
// one row; another part of the same size is not.
void mergeTests() {
  QJsonObject a = box(QStringLiteral("F2.b0"), QStringLiteral("Body1"), QStringLiteral("steel"), 7.85, {0, 0, 0},
                      {1800, 28, 45});
  a.insert(QStringLiteral("component_name"), QStringLiteral("Planed timber"));
  a.insert(QStringLiteral("component_bodies"), 1);
  a.insert(QStringLiteral("occurrence"), QStringLiteral("Frame:1/Planed timber:1"));
  a.insert(QStringLiteral("path"), QStringLiteral("O1/O2"));
  a.insert(QStringLiteral("part"), QStringLiteral("{\"source\":\"S1\"}#0"));
  QJsonObject turned = box(QStringLiteral("F5.b0"), QStringLiteral("Body1"), QStringLiteral("steel"), 7.85,
                           {0, 0, 0}, {28, 1800, 45});
  turned.insert(QStringLiteral("component_name"), QStringLiteral("Planed timber 2"));
  turned.insert(QStringLiteral("component_bodies"), 1);
  turned.insert(QStringLiteral("part"), QStringLiteral("{\"source\":\"S1\"}#0"));
  QJsonObject again = a;
  again.insert(QStringLiteral("path"), QStringLiteral("O1/O3"));
  again.insert(QStringLiteral("bbox"), QJsonObject{{QStringLiteral("min"), xyz(0, 0, 0)},
                                                   {QStringLiteral("max"), xyz(45, 28, 1800)}});
  QJsonObject two = box(QStringLiteral("F7.b1"), QStringLiteral("Body2"), QStringLiteral("steel"), 7.85, {0, 0, 0},
                        {28, 1800, 45});
  two.insert(QStringLiteral("component_name"), QStringLiteral("Bracket"));
  two.insert(QStringLiteral("component_bodies"), 2);
  QJsonArray bodies;
  bodies.append(a);
  bodies.append(turned);
  bodies.append(again);
  bodies.append(two);
  const PropertiesReport report = propertiesReport(QJsonObject{{QStringLiteral("bodies"), bodies}}, kNames);
  CHECK(report.bodies[0].name == QStringLiteral("Planed timber"));
  CHECK(report.bodies[0].logName == QStringLiteral("Body1 in Frame:1/Planed timber:1"));
  CHECK(report.bodies[3].name == QStringLiteral("Bracket (Body2)"));
  CHECK(report.bodies[0].path == QStringLiteral("O1/O2"));
  // a, turned (the same part) and again (the same body): one row; the
  // bracket's body, the size of the turned one but no part, compares its
  // size with the row's first and stays apart.
  CHECK(report.rows.size() == 2 && report.rows[0].size() == 3);
  // Another volume never merges: one beyond a millionth of the body's
  // 2 268 cm³ (1 mm³ is within it).
  PropertiesBody other = report.bodies[0];
  other.volume += 10.0;
  CHECK(!propertiesEqual(report.bodies[0], other));
  PropertiesBody aluminium = report.bodies[0];
  aluminium.materialId = QStringLiteral("aluminum");
  CHECK(!propertiesEqual(report.bodies[0], aluminium));
  // Without a part or the same body, the size along the axes decides.
  PropertiesBody plain = report.bodies[2];
  plain.uid = QStringLiteral("F9.b0");
  plain.part.clear();
  CHECK(!propertiesEqual(report.bodies[0], plain));
  plain.size = report.bodies[0].size;
  plain.size[2] += 0.5e-6;
  CHECK(propertiesEqual(report.bodies[0], plain));
}

// One body: Copy gives its name and its groups' rows.
void oneBodyTests() {
  QJsonArray bodies;
  bodies.append(box(QStringLiteral("F1.b0"), QStringLiteral("Body1"), QStringLiteral("steel"), 7.85, {0, 0, 0},
                    {20, 20, 20}));
  const PropertiesReport report = propertiesReport(QJsonObject{{QStringLiteral("bodies"), bodies}}, kNames);
  const QStringList lines = propertiesCopyText(report, QStringLiteral("mm")).split(QLatin1Char('\n'));
  CHECK(lines.value(0) == QStringLiteral("Body1"));
  CHECK(lines.value(1) == QStringLiteral("Material\tSteel"));
  CHECK(lines.contains(QStringLiteral("Mass\t0.063 kg")));
  CHECK(lines.contains(QStringLiteral("Bounding Box\t20 × 20 × 20 mm")));
}

} // namespace

int propertiesReportTests() {
  unitTests();
  tableTests();
  mergeTests();
  oneBodyTests();
  return failures;
}
