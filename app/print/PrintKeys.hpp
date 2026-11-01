// SPDX-License-Identifier: MIT
#pragma once

#include <limits>

#include <QHash>
#include <QJsonObject>
#include <QJsonValue>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// The numbers a key takes, in its unit; infinite where there is no bound.
struct PrintRange {
  double low = -std::numeric_limits<double>::infinity();
  double high = std::numeric_limits<double>::infinity();
};

// One key of the print key map (mitcad#640, mitcad#655: the `print_keys`
// query): its plain name, its kind of value and unit, its choices with
// plain names, and where the settings form shows it (its category, its
// sub-group, whether it is advanced). Plain Qt Core.
struct PrintKey {
  enum class Kind { Boolean, Integer, Number, Percent, NumberOrPercent, Text, Color, Choice, Points };

  QString key;
  QString set; // printer, process or filament
  QString name;
  Kind kind = Kind::Text;
  QString unit; // "mm", "°C", "layers"; empty for none
  QString of;   // a percentage's basis (number_or_percent)
  QVector<QPair<QString, QString>> choices; // value, plain name
  bool perDialect = false; // a value per slicer (a preset's name)
  QString category;        // as the query names it
  QString group;
  bool advanced = false;
  QStringList dialectKeys; // the slicers' own keys of it (`dialects`)
  QStringList dialects;    // the slicers that have it, by id (`dialects`' `dialect`), in the model's order
  // A number-or-percentage key: the slicers that take its percentage
  // (`dialects`' `percent`, mitcad#655); the editors offer [mm | %] for
  // the design's slicer only (takesPercent).
  QStringList percentDialects;
  // A number's range as its slicers take it (`min` and `max`, mitcad#746:
  // the widest of them; unbounded where the query gives no bound), and a
  // slicer's own where it takes less (its `dialects` entry's `min` and
  // `max`: Bambu Studio's -1 for "automatic" is not OrcaSlicer's).
  PrintRange range;
  QHash<QString, PrintRange> dialectRanges;
  bool takesPercent(const QString& dialect) const {
    return kind == Kind::NumberOrPercent && (dialect.isEmpty() || percentDialects.contains(dialect));
  }

  // A choice's plain name; the value itself when it has none.
  QString choiceName(const QString& value) const;
};

// A sub-group of a category, and a category with its sub-groups in order.
struct PrintGroup {
  QString id;
  QString title;
};
struct PrintCategory {
  QString id;
  QString title;
  QVector<PrintGroup> groups;
};

// The key map as the `print_keys` query gives it: the keys in display
// order and, per set, the categories and their groups (`layout`). The app
// keeps no table of its own: without a layout, the categories and groups
// are the keys' own in the keys' order.
class PrintKeyTable {
public:
  PrintKeyTable() = default;
  explicit PrintKeyTable(const QJsonObject& answer);

  bool isEmpty() const { return m_keys.isEmpty(); }
  const PrintKey* find(const QString& key) const;
  // The keys of a set in display order.
  QVector<const PrintKey*> keys(const QString& set) const;
  // The set's categories with their groups, in order.
  QVector<PrintCategory> layout(const QString& set) const;
  // The category and the group a key is shown in (ids of layout()).
  QString categoryOf(const PrintKey& key) const;
  QString groupOf(const PrintKey& key) const;
  // The plain name of a key, Mitcad's or a slicer's; empty when the table
  // has none (mitcad#655: no raw key in front of the user).
  QString plainName(const QString& raw) const;
  // A text with every key in it, Mitcad's or a slicer's, as its plain
  // name: a refusal or an import's problem as the user reads it.
  QString plainText(const QString& text) const;

private:
  QVector<PrintKey> m_keys;
  QHash<QString, int> m_index;
  QHash<QString, QString> m_names; // every key, Mitcad's and the slicers', to its plain name
  QHash<QString, QVector<PrintCategory>> m_layout;
};

// A title from an id when the model gives none: "slow_down_by_height" is
// "Slow down by height".
QString titleOfId(const QString& id);
// A slicer by its id as the UI names it: "bambu_studio" is "Bambu Studio".
QString slicerTitle(const QString& id);
// Whether a slicer is of the Bambu Studio family (Bambu Studio, OrcaSlicer,
// QIDI Studio), whose preset names stand in for each other.
bool bambuFamily(const QString& dialect);
// The name a value kept per slicer (`{"bambu_studio": "Generic PLA"}`, a
// filament's `filament_settings_id`, mitcad#1062) gives one slicer, as the
// model's export reads it (`print_profile::dialect_value`): the slicer's
// own entry, else for a Bambu-family slicer the first family member's
// (Bambu Studio, OrcaSlicer, QIDI Studio), whose id goes to `from`; a
// plain text for every slicer. Empty when it gives none.
QString slicerPresetOf(const QJsonValue& value, const QString& dialect, QString* from = nullptr);
// A number as a field shows it: no trailing zeros, at most 6 decimals.
QString printNumberText(double value);
// An outline's size: "256 × 256 mm"; empty when it has no points.
QString outlineText(const QJsonValue& points);
// A value as shown, in the key's own unit: "0.2 mm", "15 %", "Aligned",
// "yes", "256 × 256 mm", "Bambu Studio: Bench X; PrusaSlicer: Bench".
QString printValueText(const PrintKey& key, const QJsonValue& value);
// The values a key takes (mitcad#655, mitcad#746): the model's range of
// the key (`print_keys`' `min` and `max`, what its slicers take), within
// the range of `dialect` (the design's slicer, designDialectOf) where that
// slicer takes less; the key's own range for no slicer.
PrintRange printRange(const PrintKey& key, const QString& dialect = QString());

// Typed text as the model takes the key's value: a number, "110%" (a
// number or percent typed with %), a choice by its value or plain name, a
// boolean from yes/no; null for empty text. Sets `error` (and returns
// null) when the text is not of the key's kind or out of its range for
// `dialect` (printRange).
QJsonValue printValueOf(const PrintKey& key, const QString& text, QString* error = nullptr,
                        const QString& dialect = QString());

} // namespace mitcad
