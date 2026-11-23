// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

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
  // Limits' columns come after the first six (whose numbers test scripts
  // use); a user parameter's delete button last (mitcad#2202).
  enum Column { kName, kUnit, kExpression, kValue, kComment, kFavorite, kMin, kMax, kStep, kStrict, kSlider, kDelete };

  QTreeWidgetItem* addRow(QTreeWidgetItem* parent, const QJsonObject& parameter);
  void edit(QTreeWidgetItem* item, int column);
  void cellEdited(QTreeWidgetItem* item, int column);
  void editFormula();
  // Edit Formula for a parameter with a text (mitcad#2179): from the
  // button with the saved expression, from a cell's fx with what is typed
  // in the cell (whose editing ends first, without applying it). OK sets
  // the formula and its limits as one undo step.
  void formulaFromCell(QLineEdit* editor, const QString& name, const QString& unit, const QString& text);
  void runFormulaEditor(const QString& name, const QString& unit, const QString& text);
  void updateButtons();
  void applyFilter();
  void logLayout();
  // Narrows the wide columns towards their minimums when the window is
  // narrower than the columns want (a small screen clamps it), so that the
  // rows' delete buttons at their ends stay in view; widths the user set
  // by dragging a header's edge are kept.
  void fitColumns();
  bool m_fittingColumns = false;
  bool m_columnsByUser = false;

  DocumentHost& m_host;
  QTreeWidget* m_table = nullptr;
  QLineEdit* m_filter = nullptr;
  QLabel* m_message = nullptr;
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

  // Groups of user parameters (mitcad#2180): under User Parameters each
  // group is a collapsible row with its parameters and subgroups (the
  // sort orders them within it); ungrouped parameters stay directly under
  // User Parameters, and Favorites stays a view across the groups. New
  // Group… makes one (in the selected group); a group row is renamed in
  // place (double-click or F2) and deleted with Delete (asked only when
  // it holds parameters, which move up a level). Parameters move by drag
  // onto a group row or onto User Parameters, and by the context menu's
  // Move to Group ▸. The filter matches group names too; the collapsed
  // groups are kept per design for the session.
  void newGroup();
  void renameGroup(const QString& path, const QString& name);
  void deleteGroup(const QString& path);
  void moveToGroup(const QStringList& names, const QString& group);
  void showContextMenu(const QPoint& at);
  void groupExpansionChanged(QTreeWidgetItem* item, bool expanded);
  // The user parameters selected (each once, Favorites' rows too).
  QStringList selectedUserParameters() const;
  // The group a new parameter or group goes in: the selected group row's,
  // or the group of the selected user parameter; empty for none.
  QString selectedGroup(bool ofParameter) const;
  // A name for a new group in `parent` from the user; empty when cancelled.
  QString askGroupName(const QString& title, const QString& parent,
                       const std::function<QString(const QString&)>& make);
  QPushButton* m_newGroup = nullptr;
  QStringList m_groupPaths; // every group, a parent before its subgroups

  // Adding and deleting in the table (mitcad#2202, GitHub
  // mitcad/Mitcad#512). User Parameters ends in one empty row ("New
  // parameter"), last whatever the sort and hidden while the search or
  // Only limited filters the rows. Its Name, Unit (the design's length
  // unit first), Expression and Comment are typed in place; Tab goes Name,
  // Expression, Comment, Unit and round (Shift+Tab back). Enter, or leaving
  // the row with a name and an expression, makes it a user parameter (one
  // undo step) and a new empty row takes the keyboard; what the model
  // refuses stays typed in the row, with the reason in the message line.
  // Each user parameter's row ends in a delete button, disabled with what
  // uses the parameter in its tooltip; deleting is undone as any change.
  struct NewRow {
    QString name;
    QString unit; // the model's unit: empty for Number
    bool unitChosen = false; // else the design's length unit
    QString expression;
    QString comment;
    QString refusal; // the model's reason, in words
  };
  QTreeWidgetItem* newRow() const;
  void addNewRow(QTreeWidgetItem* users, const QString& after);
  void newRowEdited(QTreeWidgetItem* item, int column);
  void newRowEditorClosed(int column, int hint);
  void createFromNewRow(bool asked);
  // Edit Formula for the empty row (mitcad#2290), from its Expression
  // cell's fx (whose editing ends first, without applying it) or the
  // Edit Formula… button, with `text`: OK makes the row a parameter with
  // the window's formula as Enter would (one undo step; without a name the
  // row keeps the formula and the message line asks for one), Cancel makes
  // nothing. While the window is open, the cell's editor closing makes no
  // parameter of the row (m_newRowFormula).
  void formulaForNewRow(QLineEdit* editor, const QString& text);
  void runNewRowFormula(const QString& text);
  bool m_newRowFormula = false;
  void addDeleteButton(QTreeWidgetItem* row, const QJsonObject& parameter);
  void deleteParameter(const QString& name);
  // The message line under the table, which takes no room while empty.
  void setMessage(const QString& text);
  NewRow m_new;
  bool m_editNewRow = false; // the new empty row's Name is edited at the next refresh
  int m_loggedNewHidden = -1;
  QString m_loggedNewRow;
  QString m_loggedInUse;
  QString m_loggedHeld; // the slider value last told during a drag
};

} // namespace mitcad
