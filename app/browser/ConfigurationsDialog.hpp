// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QDialog>
#include <QHash>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "../framework/ParameterSuggest.hpp"
#include "ConfigurationsData.hpp"

class QCheckBox;
class QLabel;
class QPushButton;
class QTabWidget;
class QTableWidget;
class QTableWidgetItem;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class DocumentHost;

// A page of the Configurations window that reads the model itself: the
// Rules page (mitcad#808) is one. The window refreshes it after every
// change of the design while it is shown.
class ConfigurationPage : public QWidget {
public:
  using QWidget::QWidget;
  ~ConfigurationPage() override = default;
  virtual void refresh() = 0;
};

// The Rules page of the Configurations window: the rules' list and the
// block editor of mitcad#808, which defines it
// (ConfigurationRulesPage.cpp). The window takes it as its third tab.
ConfigurationPage* makeConfigurationRulesPage(DocumentHost& host, QWidget* parent);

// The Bindings page (mitcad#862, ConfigurationBindingsPage.hpp): the
// window's fourth tab, and what its Edit… and Select in Browser call.
class ConfigurationBindingsPage;
struct BindingActions;

// Configurations (mitcad#807; docs/configurations.md, decision 7): the
// design's configuration table in a window of its own, with the tabs
// Options, Rows, Rules and Bindings. Every change is a model command and an undo
// step at once, as in Change Parameters; what the model refuses stays in
// its cell in red with the reason.
//
// - Options: the options in choice order (dragged, or Move Up and Move
//   Down), Add Option… (an existing user parameter or a new one), Remove
//   Option, and per option its domain (Edit Domain…: a list, a range, or
//   a column of one of the design's tables, mitcad#849),
//   its default (the option's own value), its comment and whether it is
//   driven.
// - Rows: a sheet of the rows with a column per option (and per other
//   parameter of a closed table), Designation, Part Number and Supplier
//   URL; Closed, Add Row (the current values), Generate Rows from Table…
//   (a row per key of a table's column, mitcad#849), Delete Row, Set as
//   Default, Add Column… and Remove Column (a closed table's parameter
//   columns, mitcad#999) and Apply. A value a rule refuses is not written: its cell keeps it in
//   red with the rule's name; a row that breaks a rule is marked in its
//   cells with the rule in the tooltip.
// - Rules: makeConfigurationRulesPage.
// - Bindings (mitcad#862): ConfigurationBindingsPage, every binding of the
//   design; the Options tab says per option how many use it (Used By).
//
// The JSON is configurations::'s (ConfigurationsData.hpp).
class ConfigurationsDialog : public QDialog {
  Q_OBJECT

public:
  ConfigurationsDialog(DocumentHost& host, QWidget* parent = nullptr);

  // Shows the table as the model has it now.
  void refresh();

  // The Bindings tab (mitcad#862): what its Edit… and Select in Browser
  // do, and the tab shown with the bindings an option drives (empty: all).
  void setBindingActions(const BindingActions& actions);
  void showBindings(const QString& option = QString());

private:
  enum OptionColumn {
    kOptionName,
    kOptionUnit,
    kOptionDomain,
    kOptionDefault,
    kOptionComment,
    kOptionDriven,
    kOptionUsedBy // mitcad#862
  };

  QWidget* makeOptionsPage();
  QWidget* makeRowsPage();
  void fillOptions();
  void fillRows();
  void checkRows();

  // Options.
  void addOption();
  void removeOption();
  void editDomain();
  void moveOption(int by);
  void optionsDropped();
  void optionEdited(QTreeWidgetItem* item, int column);
  QString selectedOption() const;

  // Rows: the columns of the sheet.
  struct RowColumn {
    enum Kind { Name, Option, Parameter, Designation, PartNumber, SupplierUrl } kind = Name;
    QString parameter; // an option's or a parameter's name
  };
  QVector<RowColumn> rowColumns() const;
  void rowEdited(QTableWidgetItem* item);
  void addRow();
  void generateRows();
  void deleteRow();
  void setDefaultRow();
  void applyRow();
  QString selectedRow() const;
  // A closed table's parameter columns (mitcad#999).
  void addColumn();
  void removeColumn();
  // The parameter of the current cell's column when it is a parameter
  // column (not an option's); empty otherwise.
  QString selectedColumn() const;

  // Calls `call` once the table's own handling of an edit is over and no
  // job computes the model.
  void later(std::function<void()> call);
  // Runs a command of the window; false, with the reason in m_reason and
  // the message line, when the design cannot change now or the model
  // refused it (nothing when it was cancelled).
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  // An edit the window refuses itself or the model refused: the typed
  // text stays in its cell with the reason.
  void refuse(const QString& key, const QString& what, const QString& typed, const QString& reason);
  // A new table: set_configurations, then the window shows the model's.
  // `key` names the cell the edit came from ("row|column"), whose typed
  // text stays in red with the reason when the model refuses it. True when
  // the model took the table.
  bool submitTable(const configurations::Table& table, const QString& key, const QString& typed,
                   const QString& what);
  bool isText(const QString& parameter) const;
  QString unitOf(const QString& parameter) const;
  void showMessage(const QString& text, bool error);
  void logLayout();

  DocumentHost& m_host;
  QTabWidget* m_tabs = nullptr;
  QLabel* m_message = nullptr;

  QTreeWidget* m_options = nullptr;
  QPushButton* m_addOption = nullptr;
  QPushButton* m_removeOption = nullptr;
  QPushButton* m_editDomain = nullptr;
  QPushButton* m_up = nullptr;
  QPushButton* m_down = nullptr;

  QTableWidget* m_rows = nullptr;
  QCheckBox* m_closed = nullptr;
  QLabel* m_current = nullptr;
  QPushButton* m_addRow = nullptr;
  QPushButton* m_generateRows = nullptr;
  QPushButton* m_deleteRow = nullptr;
  QPushButton* m_defaultRow = nullptr;
  QPushButton* m_apply = nullptr;
  QPushButton* m_addColumn = nullptr;    // mitcad#999
  QPushButton* m_removeColumn = nullptr;

  ConfigurationPage* m_rules = nullptr;
  ConfigurationBindingsPage* m_bindings = nullptr; // mitcad#862
  QString m_loggedUsage;

  // The model's table, its options' reports and the user parameters
  // (by name, as the parameters query gives them).
  configurations::Table m_table;
  configurations::Report m_report;
  QHash<QString, QJsonObject> m_parameters;
  QStringList m_userParameters; // in the model's order
  // The design's tables and their columns (`formula_help`'s `tables`):
  // what Edit Domain… and Generate Rows from Table… choose from.
  std::vector<SuggestedTable> m_tables;
  // Edits refused, by "row|column" (the Rows sheet) or "option|column":
  // the typed text and why.
  QHash<QString, QPair<QString, QString>> m_refused;
  // Rows that break a rule now: the refusal of their combination.
  QHash<QString, configurations::Refusal> m_rowIssues;
  bool m_building = false;
  QString m_reason;    // why the last run() failed
  QString m_selectRow; // a row to select once the sheet shows it (Add Row)

  QString m_logged;
  QString m_loggedOptions;
  QString m_loggedRows;
  QString m_loggedIssues;
};

} // namespace mitcad
