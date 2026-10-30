// SPDX-License-Identifier: MIT
#pragma once

// Edit Options… of an inserted component (mitcad#810; app/COMMANDS.md,
// "Options of library parts"): the options of a library part, a linked
// design or a BUILD part, which belong to the component and so to all of
// its occurrences (docs/configurations.md, "Inserted components").
//
// Above, a chooser per option filtered by the source's rules
// (OptionChooser); below, each option's expression: a constant the
// chooser writes, or a formula of the design's own parameters (the field's
// suggestions, and the Formula Editor from its fx button or menu), which
// binds the option: the component then follows the parameter, and its
// chooser shows the value it gets. OK keeps the expressions in one undo
// step (`set_component_parameters`, run by the caller).
//
// Above the choosers, the Standard and Supplier filters of a library part
// whose rows name standards or suppliers' articles (mitcad#949); under them
// the row's articles behind Supplier Articles (n)… (Alt+A), which opens
// their table (mitcad#979, SupplierPartsDialog.hpp; "Edit Options supplier
// part numbers: ..." in the log). Logged for the UI tests: "Edit Options <option> expression at x,y" and
// "Edit Options OK at x,y" (in the main window, as the panels' places;
// Return in an expression field may take a suggestion instead of OK),
// "Edit Options bound <option> =
// <expression>" and the chooser's "Edit Options options: ..." lines.

#include <QDialog>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QTimer>

#include "PartOptions.hpp"
#include "SupplierPartsDialog.hpp"

class QFormLayout;
class QLabel;
class QLineEdit;
class QPushButton;

namespace mitcad {

class OptionChooser;

class EditOptionsDialog : public QDialog {
  Q_OBJECT

public:
  // `query` runs the model's queries (a failure as {"error": {"message"}});
  // `component` the inserted component's uid, `name` as the browser shows it;
  // `libraryTable` its library component's table as library_show gives it
  // (with the component's `standards`), when it has one: its rows' standards
  // and suppliers' articles make the Standard and Supplier filters above the
  // choosers and the row's articles under them (mitcad#949); its
  // `suppliers` ({id, name, url}) and `supplier_url` (the component's
  // product page) go to the articles' table (mitcad#979).
  EditOptionsDialog(JsonRunner query, const QString& component, const QString& name,
                    const QJsonObject& libraryTable = QJsonObject(), QWidget* parent = nullptr);

  // Whether the component has options (else the dialog says why and has
  // nothing to keep).
  bool hasOptions() const { return m_has; }
  // Every option's expression ({"Color": "'Black'", "Length": "Shelf_L"}),
  // as `set_component_parameters` keeps them.
  QJsonObject parameters() const;
  // "Color = 'Black', Length = Shelf_L".
  QString summary() const;

  void accept() override;

protected:
  void showEvent(QShowEvent* event) override;

private:
  // The expressions' fields again from the chooser's answer.
  void fillExpressions();
  // The combination's row's suppliers' articles: the button's count and
  // what its table shows (mitcad#949, mitcad#979).
  void showSupplierParts();
  void expressionEdited(const QString& option);
  void logPlaces() const;

  QString m_component;
  OptionChooser* m_chooser = nullptr;
  QFormLayout* m_expressionsForm = nullptr;
  QLabel* m_intro = nullptr;
  QPushButton* m_supplierButton = nullptr;
  SupplierPartsRequest m_supplierRequest;
  QString m_name;
  QJsonArray m_suppliers;  // the library's suppliers
  QString m_productPage;   // the component's supplier_url
  QHash<QString, QLineEdit*> m_expressions;
  QStringList m_order;
  // The options bound to the design's parameters: option -> formula.
  QJsonObject m_bound;
  bool m_read = false; // the component's own expressions were read
  bool m_has = false;
  QPushButton* m_ok = nullptr;
  QTimer m_placesTimer;
};

} // namespace mitcad
