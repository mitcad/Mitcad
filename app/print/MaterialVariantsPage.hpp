// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QPoint>
#include <QString>
#include <QStringList>
#include <QWidget>

class QLabel;
class QPushButton;
class QTreeWidget;

namespace mitcad {

class PrintKeyTable;
class SettingsForm;

// A material's Variants (mitcad#1054), the last tab of Materials' editor
// beside its print settings (SettingsForm::setEndPage): the material's
// values for printers and nozzles that differ from its own. One filament
// is one material; the printer a body prints with picks the variant.
//
// On top the variants, one row each: its printer and nozzle ("Bench X,
// 0.4 mm nozzle", "Bench Y, any nozzle") and the values that differ, by
// their plain names ("Nozzle temperature 220 °C, Bed temperature 55 °C";
// "Same as the material" with none). Under the list Add Variant ▾, a menu
// of the printer profiles there are (the design's and the libraries'), each
// adding the variant of its printer and nozzle (one the material has
// already is disabled), and Remove Variant. Under them the selected
// variant's values in a settings form of the filament's keys: what it
// leaves unset shows the material's own value as its placeholder; a value
// typed is the variant's, ↺ clears it back to the material's. Read only
// (a built-in material) disables the buttons and the form. With no
// variant a quiet line says what variants are.
//
// The page changes nothing itself: changeRequested gives an
// `edit_material` `variant` change to the window, which runs it and shows
// the material again (setMaterial). The selected variant stays selected.
//
// Logs "<log> variants: <n>" and "<log> variant row: <label>: <values>"
// when they change, "<log> variant selected: <label>", "<log> variant add
// menu: <entry> | <entry>", and the places "<log> variant item <label> at
// x,y", "<log> variant add at", "<log> variant remove at" and the form's
// ("<log> variant settings field <key> at", ...).
class MaterialVariantsPage : public QWidget {
  Q_OBJECT

public:
  explicit MaterialVariantsPage(const QString& logName, QWidget* parent = nullptr);
  ~MaterialVariantsPage() override;

  // The material's fields (`filament` its own values, `variants`), the
  // key table, the design's slicer and whether it can change.
  void setMaterial(const QJsonObject& fields, const PrintKeyTable& keys, const QString& dialect, bool readOnly);
  // The printer profiles Add Variant offers, asked for when it opens: the
  // `library_items` rows of printers (`name`, `item`, `variant_printer`,
  // `nozzle`).
  std::function<QJsonArray()> printers;
  // How many variants the material has.
  int count() const;
  // The places of what is in sight (see the class comment).
  QStringList places(const std::function<QString(QWidget*, const QPoint&)>& at) const;
  // A variant's name for people: "Bench X, 0.4 mm nozzle", "Bench X, any
  // nozzle" (as the model's `label`).
  static QString variantLabel(const QString& printer, const QJsonValue& nozzle);

signals:
  // An `edit_material` `variant` change ({printer, nozzle, filament} or
  // {printer, nozzle, remove}); `what` says it for the log.
  void changeRequested(const QJsonObject& variant, const QString& what);
  // The rows in sight changed.
  void layoutChanged();

private:
  void rebuild();
  void showSelected();
  void addVariant();
  void removeVariant();
  // The selected variant's object; empty when none is selected.
  QJsonObject selected() const;
  // The values of a variant that differ, by their plain names.
  QString differences(const QJsonObject& variant) const;

  QString m_log;
  const PrintKeyTable* m_keys = nullptr;
  QString m_dialect;
  bool m_readOnly = true;
  QJsonObject m_base;     // the material's own filament values
  QJsonArray m_variants;  // the material's variants
  QString m_current;      // the selected variant's label
  QString m_loggedRows;
  QTreeWidget* m_list = nullptr;
  QLabel* m_empty = nullptr;
  QPushButton* m_add = nullptr;
  QPushButton* m_remove = nullptr;
  SettingsForm* m_form = nullptr;
  bool m_building = false;
};

} // namespace mitcad
