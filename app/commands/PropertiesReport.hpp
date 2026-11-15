// SPDX-License-Identifier: MIT
#pragma once

// What the Properties window shows (mitcad#1816, app/COMMANDS.md "Physical
// Properties"), read from the model's `properties` answer: each placed
// body by its component's name, its material ("No material" when none is
// set anywhere, without a mass), the summary of several bodies (their
// count, the mass of those with a material, the volume, the center of
// mass weighted by those masses, the box around all), the table's rows
// with equal bodies merged, the even units (kg with three decimals; cm³,
// dm³ or m³; cm² or m²; lengths in the design's unit) and the text Copy
// puts on the clipboard. Plain Qt Core, without the model or widgets.

#include <array>
#include <optional>
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

using PropertiesPoint = std::array<double, 3>;

// A mass in kg with three decimals: "3.672 kg".
QString propertiesMassText(double kilograms);
// A volume in cm³ below 1 dm³, else in dm³ below 1 m³, else in m³, with
// at most three decimals: "8 cm³", "5.4 dm³".
QString propertiesVolumeText(double cubicMillimetres);
// An area in cm² below 1 m², else in m²: "24 cm²", "2.72 m²".
QString propertiesAreaText(double squareMillimetres);
// A box's size in the design's unit: "1800 × 28 × 45 mm".
QString propertiesSizeText(const PropertiesPoint& size, const QString& unit);
// A point in the design's unit: "(10, 20, 5) mm".
QString propertiesPointText(const PropertiesPoint& point, const QString& unit);

// One placed body of the answer.
struct PropertiesBody {
  QString uid;
  // The occurrence path by uids ("O1/O4"); empty for the root's bodies.
  QString path;
  // What the window calls it: its component's name, with the body's name
  // when the component has more than one body; the body's name in the
  // root.
  QString name;
  // The log's name, as before (mitcad#1110): "Body1 in Arm:1".
  QString logName;
  // The material's id and the name the window shows; "No material" (and no
  // mass) when none is set anywhere.
  QString materialId;
  QString material;
  bool hasMaterial = false;
  double density = 0.0;
  double mass = 0.0;
  double volume = 0.0;
  double area = 0.0;
  PropertiesPoint center{};
  bool hasBox = false;
  PropertiesPoint min{};
  PropertiesPoint max{};
  PropertiesPoint size{};
  std::optional<PropertiesPoint> moments;
  bool hasCounts = false;
  int faces = 0;
  int edges = 0;
  int vertices = 0;
  // The material's other values (mitcad#1213): its name and text.
  QVector<QPair<QString, QString>> materialValues;
  // A placed part's definition with its values (equal for equal parts).
  QString part;
};

// Several bodies together.
struct PropertiesSummary {
  int count = 0;
  // The mass of the bodies with a material, and how many have none.
  double mass = 0.0;
  int withoutMaterial = 0;
  double volume = 0.0;
  // Weighted by the masses of the bodies with a material; none when no
  // body has one.
  std::optional<PropertiesPoint> center;
  bool hasBox = false;
  PropertiesPoint min{};
  PropertiesPoint max{};
  PropertiesPoint size{};
};

struct PropertiesReport {
  std::vector<PropertiesBody> bodies;
  // The table's rows: the bodies of each (indexes into `bodies`), equal
  // ones in one row, in the order the first of each comes.
  std::vector<std::vector<int>> rows;
  PropertiesSummary summary;
};

// The report of the model's `properties` answer; `materialNames` gives a
// built-in material's name as the language shows it by its id (another
// keeps its id).
PropertiesReport propertiesReport(const QJsonObject& answer, const QHash<QString, QString>& materialNames);

// Whether two bodies count as one part in the table: the same material
// (or both none) and the same volume, and either the same body in another
// occurrence, the same placed part (`part`) or the same size along the
// design's axes, within 1e-6 mm.
bool propertiesEqual(const PropertiesBody& a, const PropertiesBody& b);

// The table's column headings and a row's cells: quantity, name,
// material, mass (each; empty without a material), volume and size.
QStringList propertiesHeadings();
QStringList propertiesRowCells(const PropertiesReport& report, int row, const QString& unit);

// The summary's labels and values, in order: Bodies, Mass (and Without
// material when some have none), Volume, Center of Mass (when known) and
// Bounding Box.
QVector<QPair<QString, QString>> propertiesSummaryValues(const PropertiesSummary& summary, const QString& unit);

// A body's values in the window's three groups.
struct PropertiesGroups {
  QVector<QPair<QString, QString>> physical;
  QVector<QPair<QString, QString>> place;
  QVector<QPair<QString, QString>> more;
};
PropertiesGroups propertiesGroups(const PropertiesBody& body, const QString& unit);

// What Copy puts on the clipboard, tab-separated lines joined by "\n":
// several bodies' summary (label, value), an empty line and the table with
// its headings; one body's groups' rows.
QString propertiesCopyText(const PropertiesReport& report, const QString& unit);

// A text for the log: "×" as "x", "³" as "3", "²" as "2".
QString propertiesLogText(QString text);

} // namespace mitcad
