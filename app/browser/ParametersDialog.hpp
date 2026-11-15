// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QHash>
#include <QJsonObject>
#include <QPair>
#include <QString>

#include "../framework/ParameterEdits.hpp"
#include "ConfigurationsData.hpp"

class QCheckBox;
class QEvent;
class QLabel;
class QLineEdit;
class QPushButton;
class QTabWidget;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class DecimalAlignDelegate;
class DocumentHost;
class TablesPage;

// Change Parameters: the user parameters and the model's (the
// dimensions of each feature) with their name, unit, expression, value and
// comment. Each change is a model command and an undo step at once; names,
// expressions, units and comments are edited in place (double-click or
// F2), and what the model refuses (a cycle, a unit that does not fit, an
// unknown name) stays in the cell in red with the reason. The star column
// marks favourites (P9), which are also listed in a Favorites group at the
// top. Limits (mitcad#665): Min, Max and Step are expressions edited in
// place, Strict refuses a result outside them instead of clamping it, a
// limited value is marked in its cell (`372 mm → 300 mm ▲max`, the tooltip
// says why), a parameter with a minimum and a maximum gets a slider, and
// "Only limited" shows only the parameters a limit changed. The Tables tab
// (mitcad#667) edits the design's tables that formulas look up. A column's
// header sorts the rows within their groups (mitcad#1866).
class ParametersDialog : public QDialog {
  Q_OBJECT

public:
  ParametersDialog(DocumentHost& host, QWidget* parent = nullptr);

  // Shows the parameters as the model has them now.
  void refresh();

private:
  // Limits' columns come after the first six (whose numbers test scripts use).
  enum Column { kName, kUnit, kExpression, kValue, kComment, kFavorite, kMin, kMax, kStep, kStrict, kSlider };

  QTreeWidgetItem* addRow(QTreeWidgetItem* parent, const QJsonObject& parameter);
  void edit(QTreeWidgetItem* item, int column);
  void cellEdited(QTreeWidgetItem* item, int column);
  void addParameter();
  void deleteParameter();
  void editFormula();
  void updateButtons();
  void applyFilter();
  void logLayout();

  DocumentHost& m_host;
  QTreeWidget* m_table = nullptr;
  QLineEdit* m_filter = nullptr;
  QLabel* m_message = nullptr;
  QPushButton* m_add = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_formula = nullptr;
  QPushButton* m_ok = nullptr;
  bool m_building = false;
  // Edits the model refused, by "name|column": the typed text and why.
  QHash<QString, QPair<QString, QString>> m_refused;
  QString m_logged;     // the cells' places
  QString m_loggedRows; // the parameters as shown
  QString m_loggedFavorites;

  // While a sketch or a feature is edited (mitcad#564): what stays open
  // (a line above the rows), the open panel's own parameters left to it,
  // and the table read again when it is activated in another edit.
  void updateScope();
  bool event(QEvent* event) override;
  ParameterEditScope m_scope;
  QLabel* m_scopeNote = nullptr;
  QString m_loggedScope;
  // Expression's and Value's numbers on one decimal axis each (mitcad#566).
  DecimalAlignDelegate* m_align = nullptr;

  // Limits (mitcad#665): a model command for a cell, the slider or the
  // strict box, run after the table finished its own handling; the
  // slider of a row; what typing an expression would do; the filter.
  void submit(const QString& name, int column, const QString& text, const QString& what,
              const QJsonObject& command);
  void addSlider(QTreeWidgetItem* row, const QJsonObject& parameter);
  void previewExpression(const QString& name, const QString& text);
  QCheckBox* m_limitedOnly = nullptr;
  QString m_loggedPreview;
  QString m_loggedLimited;

  // The parameters as CSV (mitcad#666): Export writes Mitcad's table to a
  // file; Import reads Mitcad's table or the .f3d application's parameter
  // table, shows what it would add, change, rename, refuse and skip, and
  // imports it as one undo step. Not while a sketch or a panel is open.
  void exportParameters();
  void importParameters();
  QPushButton* m_import = nullptr;
  QPushButton* m_export = nullptr;

  // Configurations (mitcad#807): the options' reports (the `configure`
  // query), for an option's mark on its name and the rules' restrictions
  // shown as the limits are (in grey in Min, Max and Step, the rules in the
  // tooltip).
  void markOption(QTreeWidgetItem* row, const QJsonObject& parameter);
  configurations::Report m_options;
  QStringList m_optionRows; // for the log
  QString m_loggedOptions;
  // Two tabs (mitcad#667): Parameters (the table above) and Tables, the
  // design's tables of data that formulas look up (TablesPage).
  QTabWidget* m_tabs = nullptr;
  TablesPage* m_tablesPage = nullptr;

  // Sorting (mitcad#1866, GitHub mitcad/Mitcad#338): a click on a column's
  // header sorts the rows by it, again in the reverse order, a third time
  // back to the creation order. Rows sort within their group; the groups
  // and Model Parameters' features keep their order. The column and order
  // are kept in the settings (parameters/sort) for the next opening. A row
  // renamed or added is selected where the sort puts it.
  void sortBy(int column, Qt::SortOrder order);
  void logOrder();
  int m_sortColumn = -1; // creation order
  Qt::SortOrder m_sortOrder = Qt::AscendingOrder;
  QString m_select;      // the row to select at the next refresh
  QString m_loggedOrder; // the rows' order as shown, told on each opening
};

} // namespace mitcad
