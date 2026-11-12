// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QDialog>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QPoint>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

class QLabel;
class QLineEdit;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class CommandContext;

// A unit of a material's values (mitcad#1213, the `material_units` query):
// its symbol and how a value in the model's unit (g/cm³, MPa, °C,
// µm/(m·°C)) becomes one in it: value * scale + offset.
struct MaterialUnit {
  QString symbol;
  double scale = 1.0;
  double offset = 0.0;
  double shown(double model) const { return model * scale + offset; }
  double model(double shown) const { return (shown - offset) / scale; }
};

// The units a design shows its materials' values in (mitcad#1213): its
// system's (a metric design MPa, g/cm³ and µm/(m·°C), an inch design psi,
// lb/in³ and µin/(in·°F)), temperatures in the user's preferred unit
// (Preferences › General); and the properties and printing values a
// material shows, in their order.
struct MaterialUnits {
  QString system = QStringLiteral("metric");
  QString temperature = QStringLiteral("C");
  // By quantity: density, stress, temperature, expansion, ratio.
  QHash<QString, MaterialUnit> quantities;
  // {key, name, group, group_title, quantity}, Mechanical then Thermal.
  QJsonArray properties;
  // The print settings' {key, name}: the extruder and the bed temperature.
  QJsonArray printing;

  MaterialUnit unit(const QString& quantity) const;
  // The `material_units` query's answer.
  static MaterialUnits fromAnswer(const QJsonObject& answer);
  // Asked of the model: the design's system, temperatures in the
  // preferred unit; the model's units when the query fails.
  static MaterialUnits ask(const CommandContext& model);
};

// The temperature unit the user prefers (Preferences › General, the
// setting units/temperature, mitcad#1213): "C" or "F".
QString preferredTemperatureUnit();
// Gives the model the preferred temperature unit (mitcad#1213): every
// temperature of materials it writes follows it, parts lists and bills of
// materials too. At start and when the preference changes.
void applyTemperaturePreference();

// A material's value as its fields show it: at most six significant
// digits, without trailing zeros ("29007500", "0.3", "6.66667").
QString materialValueText(double value);

// What a material's reference says (mitcad#1213): "material-properties
// dataset, 6061, T6"; empty without one.
QString materialReferenceText(const QJsonObject& fields);

// A material's mechanical, thermal and printing values on the General tab
// of the Materials window (mitcad#1213): Mechanical (Young's modulus,
// Poisson's ratio, yield strength), Thermal (maximum service temperature,
// coefficient of thermal expansion) and, for an FDM plastic, Printing (the
// print settings' extruder and bed temperatures: the same values as the
// Filament tab's, no second set), each a field with its unit in the
// design's units; a value not given is empty with the placeholder "Not
// given", never 0. Then where the values came from (the reference) and
// one line that they are typical, to be checked against the supplier's
// data. A field is saved at Enter or when it is left: changeRequested
// with `edit_material`'s `properties` or `filament` in the model's units,
// an emptied field as null. A value that is not a number is refused under
// the fields.
class MaterialPropertiesBox : public QWidget {
  Q_OBJECT

public:
  explicit MaterialPropertiesBox(QWidget* parent = nullptr);

  // The units the fields show (a design's).
  void setUnits(const MaterialUnits& units);
  // Shows a material's values (its fields as the `materials` query or a
  // library item gives them); `printing` shows the Printing group.
  void setMaterial(const QJsonObject& fields, bool editable, bool printing);
  // A refusal of the model, under the fields.
  void setError(const QString& text);
  // While the window's Search filters the General tab (mitcad#1228): only
  // the fields whose names contain `search`, their groups' headings, and
  // the source and caution lines when a field is shown. Empty: every one.
  void setSearch(const QString& search);
  // What the fields show, in one line: "Young's modulus 29007500 psi |
  // Poisson's ratio not given | ..." (the log, UI tests).
  QString shownText() const;
  // The fields' places, "<prefix> property <key> at x,y".
  QStringList places(const QString& prefix, const std::function<QString(QWidget*, const QPoint&)>& at) const;

signals:
  // A value changed: `edit_material`'s fields, a description for the log.
  void changeRequested(const QJsonObject& change, const QString& what);

private:
  struct Field {
    QString key;
    QString name;
    QString quantity;
    bool printing = false;
    QLineEdit* edit = nullptr;
    QLabel* unit = nullptr;
    QLabel* label = nullptr;
    QString shown; // the text shown for the material's value
    bool given = false;
  };

  void rebuild();
  void finish(int index);
  // The rows shown: Printing for an FDM plastic, those Search finds.
  void applyVisibility();

  MaterialUnits m_units;
  QVector<Field> m_fields;
  // The groups' headings and their fields' indices (Mechanical, Thermal,
  // Printing), a heading shown with any of its fields.
  QVector<std::pair<QLabel*, QVector<int>>> m_groups;
  QString m_search; // setSearch's
  QWidget* m_rows = nullptr;
  QLabel* m_reference = nullptr;
  QLabel* m_caution = nullptr;
  QLabel* m_error = nullptr;
  QJsonObject m_fieldsShown;
  bool m_editable = false;
  bool m_printing = false;
};

// Browse Material Dataset… (mitcad#1213, New ▾ of the Materials window):
// the bundled material dataset searched offline (the `material_dataset`
// query), materials and grades picked, the current one's values shown in
// the design's units; Create makes the user's own material of each one
// picked (mitcad#1706: Ctrl and Shift pick several, Select All every one
// the search shows). Says where the values come from and that they are
// typical. Logs "Material dataset opened: <n> materials", "Material
// dataset search: <text>, <n> shown", "Material dataset selected: <name>:
// <values>", "Material dataset picked: <n>: <name> | <name>" and the
// places "Material dataset search at x,y", "Material dataset item <name>
// at x,y", "Material dataset create at x,y", "Material dataset select all
// at x,y" in the main window's coordinates.
class MaterialDatasetDialog : public QDialog {
  Q_OBJECT

public:
  MaterialDatasetDialog(const CommandContext& model, const MaterialUnits& units, QWidget* parent = nullptr);

  // The picked entries' ids (`aluminum/6061/t6`) and names, in the list's
  // order; empty when none.
  QStringList chosenIds() const;
  QStringList chosenNames() const;

protected:
  void showEvent(QShowEvent* event) override;

private:
  void search(const QString& text);
  void showSelected();
  void picked();
  void logPlaces();
  QList<QTreeWidgetItem*> pickedRows() const;

  const CommandContext& m_model;
  MaterialUnits m_units;
  QJsonObject m_about;   // the `material_dataset` query's answer: name, licence, caution
  QJsonArray m_entries; // the search's
  QLineEdit* m_search = nullptr;
  QTreeWidget* m_list = nullptr;
  QLabel* m_details = nullptr;
  QPushButton* m_create = nullptr;
  QPushButton* m_selectAll = nullptr;
  QString m_loggedSelected;
  QString m_loggedPicked;
  QString m_loggedPlaces;
};

} // namespace mitcad
