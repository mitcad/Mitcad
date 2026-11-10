// SPDX-License-Identifier: MIT
#include "ParametersDialog.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <iterator>
#include <optional>
#include <utility>

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPushButton>
#include <QSlider>
#include <QTabBar>
#include <QTabWidget>
#include <QStyle>
#include <QStyleOptionSlider>
#include <QTimer>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/ParameterField.hpp"
#include "../framework/FormulaEditor.hpp"
#include "../framework/IdleCall.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacParameters.hpp"
#include "../platform/NativeDialogs.hpp"
#include "../platform/MacDialogs.hpp"
#include "DecimalAlignDelegate.hpp"
#include "DocumentHost.hpp"
#include "TablesPage.hpp"

namespace mitcad {
namespace {

QColor refusedText() { return themeColor(ColorRole::ErrorText); }
QColor refusedFill() { return themeColor(ColorRole::ErrorFill); }
constexpr int kNameRole = Qt::UserRole;     // the parameter's name (rows of parameters)
constexpr int kKindRole = Qt::UserRole + 1; // user, model or reference
// The open panel's own parameter (mitcad#564): its name and expression are
// changed in the panel.
constexpr int kHeldRole = Qt::UserRole + 64;
// Limits (mitcad#665): which limit changed a row's value ("min", "max",
// "step", empty when none did), whether it has limits of its own, and in
// the Min, Max and Step cells its own limit and a feature's fixed one
// shown in its place.
constexpr int kLimitedRole = Qt::UserRole + 65;
constexpr int kHasLimitsRole = Qt::UserRole + 66;
constexpr int kOwnLimitRole = Qt::UserRole + 67;
constexpr int kFixedLimitRole = Qt::UserRole + 68;

// The mark of a limited value in its cell.
QString limitMark(const QString& limited) {
  if (limited == QStringLiteral("max")) {
    return QStringLiteral("▲max");
  }
  if (limited == QStringLiteral("min")) {
    return QStringLiteral("▼min");
  }
  return QStringLiteral("◆step");
}

// The limits' JSON keys of the Min, Max and Step columns.
QString limitKey(int column, int minColumn, int maxColumn) {
  return column == minColumn ? QStringLiteral("min")
                             : column == maxColumn ? QStringLiteral("max") : QStringLiteral("step");
}

// A column as log lines name it, in English whatever the interface
// language (mitcad#1471): in the order of the dialog's Column enum.
QString columnLogName(int column) {
  static const char* const names[] = {"name", "unit",  "expression", "value",  "comment", "favorite",
                                      "min",  "max",   "step",       "strict", "slider"};
  return column >= 0 && column < int(std::size(names)) ? QString::fromLatin1(names[column])
                                                         : QString::number(column);
}

QString refusedKey(const QString& name, int column) {
  return QStringLiteral("%1|%2").arg(name).arg(column);
}

// The unit of a text parameter (mitcad#664).
bool isTextUnit(const QString& unit) {
  return unit.trimmed().compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0;
}

// A unit box starts at `unit`, the design's length unit (mitcad#1452):
// its item when it lists one, else typed in.
void selectUnit(QComboBox* box, const QString& unit) {
  if (unit.isEmpty()) {
    return;
  }
  const int item = box->findText(unit);
  if (item >= 0) {
    box->setCurrentIndex(item);
  } else {
    box->setCurrentText(unit);
  }
}

// A bare number gets the parameter's unit ("30" in a
// millimetre parameter is "30 mm"), and a decimal point ("2,5" is
// "2.5 mm"; the model writes other expressions' decimal commas as points).
QString withUnit(const QString& expression, const QString& unit) {
  QString number;
  // A text parameter's expression is a text (mitcad#664), never a number
  // with a unit.
  if (isTextUnit(unit) || !parsePlainNumber(expression, &number)) {
    return expression;
  }
  return unit.isEmpty() ? number : number + QLatin1Char(' ') + unit;
}

// Add User Parameter: name, unit, expression and comment; stays open with
// the model's reason when the model refuses them.
class AddParameterDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::AddParameterDialog)

public:
  AddParameterDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
    setObjectName(QStringLiteral("addParameterDialog"));
    setWindowTitle(tr("Add User Parameter"));
    auto* form = new QFormLayout(this);
    m_name = new QLineEdit;
    m_unit = new QComboBox;
    m_unit->setEditable(true);
    m_unit->addItems({QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"),
                      QStringLiteral("in"), QStringLiteral("ft"), QStringLiteral("deg"),
                      QStringLiteral("rad"), QStringLiteral("Text"), QString()});
    m_unit->setItemText(m_unit->count() - 1, QString());
    // The design's length unit (mitcad#1452): 0.875 typed in a design in
    // inches is 0.875 in, as in every value field, unless chosen otherwise.
    selectUnit(m_unit, m_host.model().lengthUnit());
    m_unit->setToolTip(tr("Empty for a number without a unit; Text for a text, such as 'Box ' & Width"));
    m_expression = new QLineEdit;
    m_comment = new QLineEdit;
    m_error = new QLabel;
    m_name->setObjectName(QStringLiteral("parameterName"));
    m_unit->setObjectName(QStringLiteral("parameterUnit"));
    m_expression->setObjectName(QStringLiteral("parameterExpression"));
    m_comment->setObjectName(QStringLiteral("parameterComment"));
    m_error->setObjectName(QStringLiteral("parameterError"));
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(tr("Unit"), m_unit);
    form->addRow(tr("Expression"), m_expression);
    form->addRow(tr("Comment"), m_comment);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &AddParameterDialog::add);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    mac::styleParameterForm(this, form);
    mac::styleDialogButtons(buttons, nullptr, true);
    m_name->setFocus();
  }

  QString unit() const { return m_unit->currentText(); }

private:
  void add() {
    const QString name = m_name->text().trimmed();
    const QString unit = m_unit->currentText().trimmed();
    const QString expression = withUnit(m_expression->text().trimmed(), unit);
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                        {QStringLiteral("name"), name},
                        {QStringLiteral("expression"), expression},
                        {QStringLiteral("unit"), unit},
                        {QStringLiteral("comment"), m_comment->text()}};
    QString problem;
    if (name.isEmpty() || expression.isEmpty()) {
      problem = tr("Give a name and an expression.");
    } else if (m_host.runParameterCommand(command)) {
      qDebug().noquote() << QStringLiteral("Added parameter %1 = %2%3")
                                .arg(name, expression,
                                     unit.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(unit));
      accept();
      return;
    } else {
      problem = mitcad::modelMessage(m_host.lastError());
    }
    m_error->setText(problem);
    qDebug().noquote() << QStringLiteral("Add parameter %1 refused: %2").arg(name, problem);
  }

  DocumentHost& m_host;
  QLineEdit* m_name;
  QComboBox* m_unit;
  QLineEdit* m_expression;
  QLineEdit* m_comment;
  QLabel* m_error;
};

// F2 edits the current cell, as double-click does.
class ParameterTable : public QTreeWidget {
public:
  explicit ParameterTable(std::function<void(QTreeWidgetItem*, int)> edit) : m_edit(std::move(edit)) {}

protected:
  void keyPressEvent(QKeyEvent* event) override {
    if (event->key() == Qt::Key_F2 && currentItem() != nullptr &&
        state() != QAbstractItemView::EditingState) {
      m_edit(currentItem(), currentColumn() >= 0 ? currentColumn() : 2);
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

private:
  std::function<void(QTreeWidgetItem*, int)> m_edit;
};

// Import Parameters (mitcad#666): what importing a parameter CSV would do,
// as the model's `parameters_csv_preview` says, row by row; Import runs the
// import, Cancel leaves the design as it is.
class ImportPreviewDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::ImportPreviewDialog)

public:
  ImportPreviewDialog(const QJsonObject& preview, const QString& file, QWidget* parent) : QDialog(parent) {
    setObjectName(QStringLiteral("parametersImportPreview"));
    setWindowTitle(tr("Import Parameters"));
    resize(780, 420);
    auto* layout = new QVBoxLayout(this);
    const bool table = preview.value(QStringLiteral("format")).toString() == QStringLiteral("parameter_table");
    const QString summary = preview.value(QStringLiteral("summary")).toString();
    auto* label = new QLabel(tr("%1 (%2): %3.")
                                 .arg(file,
                                      table ? tr("the parameter table of the .f3d application")
                                            : tr("a MitCad parameter table"),
                                      summary));
    label->setObjectName(QStringLiteral("parametersImportSummary"));
    label->setWordWrap(true);
    layout->addWidget(label);

    auto* rows = new QTreeWidget;
    rows->setObjectName(QStringLiteral("parametersImportRows"));
    panel::setCompactIndentation(rows, panel::Indent::Flat);
    rows->setRootIsDecorated(false);
    rows->setAlternatingRowColors(true);
    rows->setUniformRowHeights(true);
    rows->setSelectionMode(QAbstractItemView::NoSelection);
    rows->setColumnCount(6);
    rows->setHeaderLabels({tr("Name"), tr("Action"), tr("Expression"), tr("Value"), tr("Limits"), tr("Note")});
    rows->header()->resizeSection(0, 150);
    rows->header()->resizeSection(1, 150);
    rows->header()->resizeSection(2, 170);
    rows->header()->resizeSection(3, 100);
    rows->header()->resizeSection(4, 150);
    rows->header()->setStretchLastSection(true);
    qDebug().noquote() << QStringLiteral("Parameters import preview of %1: %2").arg(file, summary);
    for (const QJsonValue& value : preview.value(QStringLiteral("rows")).toArray()) {
      const QJsonObject row = value.toObject();
      const QString name = row.value(QStringLiteral("name")).toString();
      const QString renamed = row.value(QStringLiteral("renamed")).toString();
      const QString action = row.value(QStringLiteral("action")).toString();
      const bool refused = action == QStringLiteral("refuse") || action == QStringLiteral("skip");
      const bool fromValue = row.value(QStringLiteral("from_value")).toBool();
      const QString expression = row.value(QStringLiteral("expression")).toString();
      const QString text = row.value(QStringLiteral("text")).toString();
      const QString reason = mitcad::modelMessage(row.value(QStringLiteral("reason")).toString());
      QStringList changes;
      for (const QJsonValue& change : row.value(QStringLiteral("changes")).toArray()) {
        changes << change.toString();
      }
      QStringList notes;
      for (const QJsonValue& note : row.value(QStringLiteral("notes")).toArray()) {
        notes << mitcad::modelMessage(note.toString());
      }
      QString shownAction;
      if (action == QStringLiteral("add")) {
        shownAction = tr("Add");
      } else if (action == QStringLiteral("change")) {
        shownAction = tr("Change: %1").arg(changes.join(QStringLiteral(", ")));
      } else if (action == QStringLiteral("unchanged")) {
        shownAction = tr("Unchanged");
      } else if (action == QStringLiteral("skip")) {
        shownAction = tr("Skipped");
      } else {
        shownAction = tr("Refused");
      }
      QStringList note;
      if (fromValue) {
        note << tr("From its value: %1").arg(reason);
      } else if (!reason.isEmpty()) {
        note << reason;
      }
      note << notes;
      // The limits after the import (mitcad#665): "min 10, max 80 mm,
      // step 0.5 mm, strict", and which one changed the value.
      const QJsonObject limitsOf = row.value(QStringLiteral("limits")).toObject();
      QStringList limitParts;
      for (const char* which : {"min", "max", "step"}) {
        const QString limit = limitsOf.value(QLatin1String(which)).toString();
        if (!limit.isEmpty()) {
          limitParts << QStringLiteral("%1 %2").arg(QLatin1String(which), limit);
        }
      }
      if (limitsOf.value(QStringLiteral("strict")).toBool()) {
        limitParts << QStringLiteral("strict");
      }
      const QString limits = limitParts.join(QStringLiteral(", "));
      const QString limited = row.value(QStringLiteral("limited")).toString();
      const QString shownText = limited.isEmpty() ? text : tr("%1 (limited by its %2)").arg(text, limited);
      const QString shownName = renamed.isEmpty() ? name : QStringLiteral("%1 → %2").arg(name, renamed);
      auto* item = new QTreeWidgetItem(rows, {shownName, shownAction, expression, shownText, limits,
                                              note.join(QStringLiteral("; "))});
      item->setToolTip(5, note.join(QLatin1Char('\n')));
      if (refused) {
        for (int column = 0; column < rows->columnCount(); ++column) {
          item->setForeground(column, refusedText());
        }
      }
      QString logged = QStringLiteral("Parameters import row %1").arg(name);
      if (!renamed.isEmpty()) {
        logged += QStringLiteral(" as %1").arg(renamed);
      }
      logged += QStringLiteral(": %1").arg(action);
      if (!changes.isEmpty()) {
        logged += QStringLiteral(" (%1)").arg(changes.join(QStringLiteral(", ")));
      }
      logged += refused ? QStringLiteral(": %1").arg(reason) : QStringLiteral(" %1 = %2").arg(expression, text);
      if (fromValue) {
        logged += QStringLiteral(", from its value: %1").arg(reason);
      }
      if (!limits.isEmpty()) {
        logged += QStringLiteral(", limits: %1").arg(limits);
      }
      if (!limited.isEmpty()) {
        logged += QStringLiteral(", limited by %1").arg(limited);
      }
      qDebug().noquote() << logged;
    }
    layout->addWidget(rows, 1);

    auto* buttons = new QDialogButtonBox;
    auto* importButton = new QPushButton(tr("Import"));
    importButton->setObjectName(QStringLiteral("parametersImportAccept"));
    auto* cancelButton = new QPushButton(tr("Cancel"));
    cancelButton->setObjectName(QStringLiteral("parametersImportCancel"));
    buttons->addButton(importButton, QDialogButtonBox::AcceptRole);
    buttons->addButton(cancelButton, QDialogButtonBox::RejectRole);
    // Nothing to import: only Cancel.
    const bool importable =
        preview.value(QStringLiteral("added")).toInt() + preview.value(QStringLiteral("changed")).toInt() > 0;
    importButton->setEnabled(importable);
    importButton->setDefault(importable);
    cancelButton->setDefault(!importable);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    QPushButton* primary = importable ? importButton : cancelButton;
    // Return imports (or cancels when there is nothing to import) on every
    // platform: a confirmation, not a dialog of fields (mitcad#653).
    mac::styleDialogButtons(buttons, primary, true);
    primary->setFocus();
  }
};

} // namespace

ParametersDialog::ParametersDialog(DocumentHost& host, QWidget* parent)
    : QDialog(parent), m_host(host) {
  setObjectName(QStringLiteral("parametersDialog"));
  setWindowTitle(tr("Parameters"));
  setWindowIcon(themeIcon(QStringLiteral("parameters")));
  // Room for the limits' columns (mitcad#665), and for the tabs and the
  // OK row of mitcad#667 above and below the rows.
  resize(1200, 530);
  auto* dialogLayout = new QVBoxLayout(this);
  // The Parameters tab; the Tables tab beside it (mitcad#667).
  m_tabs = new QTabWidget;
  m_tabs->setObjectName(QStringLiteral("parametersTabs"));
  m_tabs->setDocumentMode(true);
  auto* parametersPage = new QWidget;
  auto* layout = new QVBoxLayout(parametersPage);
  layout->setContentsMargins(0, 6, 0, 0);

  auto* top = new QHBoxLayout;
  m_add = new QPushButton(themeIcon(QStringLiteral("add")), tr("User Parameter"));
  m_add->setToolTip(tr("Add a user parameter"));
  m_delete = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Delete"));
  m_delete->setToolTip(tr("Delete the selected user parameter (when nothing uses it)"));
  m_filter = new QLineEdit;
  m_add->setObjectName(QStringLiteral("parametersAdd"));
  m_delete->setObjectName(QStringLiteral("parametersDelete"));
  m_filter->setObjectName(QStringLiteral("parametersFilter"));
  // The parameters as CSV (mitcad#666).
  m_import = new QPushButton(tr("Import…"));
  m_import->setObjectName(QStringLiteral("parametersImport"));
  m_import->setToolTip(tr("Import parameters from a CSV file"));
  m_import->setAutoDefault(false);
  m_export = new QPushButton(tr("Export…"));
  m_export->setObjectName(QStringLiteral("parametersExport"));
  m_export->setToolTip(tr("Export the parameters to a CSV file"));
  m_export->setAutoDefault(false);
  m_filter->setAccessibleName(tr("Search parameters"));
  m_filter->setPlaceholderText(tr("Search"));
  m_filter->setClearButtonEnabled(true);
#ifndef MITCAD_MAC_UI
  top->addWidget(m_add);
  top->addWidget(m_delete);
#endif
  top->addStretch(1);
  // Only the parameters a limit changed (mitcad#665).
  m_limitedOnly = new QCheckBox(tr("Only limited"));
  m_limitedOnly->setObjectName(QStringLiteral("parametersLimitedOnly"));
  m_limitedOnly->setToolTip(tr("Show only the parameters whose value a minimum, maximum or step changed"));
  top->addWidget(m_limitedOnly);
  top->addWidget(m_filter);
  layout->addLayout(top);

  m_table = new ParameterTable([this](QTreeWidgetItem* item, int column) { edit(item, column); });
  m_table->setObjectName(QStringLiteral("parametersTable"));
  panel::setCompactIndentation(m_table, panel::Indent::Tree);
  m_table->setColumnCount(11);
  m_table->setHeaderLabels({tr("Name"), tr("Unit"), tr("Expression"), tr("Value"), tr("Comment"), QStringLiteral("★"),
                            tr("Min"), tr("Max"), tr("Step"), tr("Strict"), tr("Slider")});
  m_table->headerItem()->setToolTip(kFavorite, tr("Favorites: listed first"));
  m_table->headerItem()->setToolTip(kMin, tr("The smallest value: an expression, may use other parameters"));
  m_table->headerItem()->setToolTip(kMax, tr("The largest value: an expression, may use other parameters"));
  m_table->headerItem()->setToolTip(kStep, tr("Values are rounded to steps counted from the minimum"));
  m_table->headerItem()->setToolTip(kStrict, tr("Refuse a value outside the limits instead of limiting it"));
  m_table->headerItem()->setToolTip(kSlider, tr("Between the minimum and the maximum"));
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_table->setSelectionMode(QAbstractItemView::SingleSelection);
  m_table->setAlternatingRowColors(true);
  m_table->setUniformRowHeights(true);
  m_table->header()->setStretchLastSection(false);
  m_table->header()->resizeSection(kName, 170);
  m_table->header()->resizeSection(kUnit, 60);
  m_table->header()->resizeSection(kExpression, 190);
  m_table->header()->resizeSection(kValue, 130); // room for an aligned number with its unit (mitcad#566)
  m_table->header()->setSectionResizeMode(kComment, QHeaderView::Stretch);
  m_table->header()->resizeSection(kFavorite, 32);
  m_table->header()->resizeSection(kMin, 90);
  m_table->header()->resizeSection(kMax, 90);
  m_table->header()->resizeSection(kStep, 70);
  m_table->header()->resizeSection(kStrict, 50);
  m_table->header()->resizeSection(kSlider, 120);
  // The numbers of Expression and Value on one decimal axis each (mitcad#566).
  m_align = new DecimalAlignDelegate(m_table, {kExpression, kValue});
  m_table->setItemDelegateForColumn(kExpression, m_align);
  m_table->setItemDelegateForColumn(kValue, m_align);
  // The native cells line them up too (mitcad#1080).
  m_table->setProperty(NativeDecimalColumnsProperty,
                       QVariantList{static_cast<int>(kExpression), static_cast<int>(kValue)});
  layout->addWidget(m_table, 1);

  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("parametersMessage"));
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  layout->addWidget(m_message);

  auto* bottom = new QHBoxLayout;
#ifdef MITCAD_MAC_UI
  auto* controls = new QHBoxLayout;
  controls->setSpacing(0);
  controls->addWidget(m_add);
  controls->addWidget(m_delete);
  bottom->addLayout(controls);
#endif
  bottom->addWidget(m_import);
  bottom->addWidget(m_export);
  m_formula = new QPushButton(tr("Edit &Formula…"));
  m_formula->setObjectName(QStringLiteral("parametersFormula"));
  m_formula->setAutoDefault(false);
  m_formula->setToolTip(tr("Edit the selected parameter's formula and limits"));
  bottom->addWidget(m_formula);
  bottom->addStretch(1);
  layout->addLayout(bottom);
  m_tabs->addTab(parametersPage, tr("&Parameters"));
  m_tablesPage = new TablesPage(host);
  m_tabs->addTab(m_tablesPage, tr("&Tables"));
  dialogLayout->addWidget(m_tabs, 1);
  auto* closing = new QHBoxLayout;
  closing->addStretch(1);
  auto* buttons = new QDialogButtonBox;
  m_ok = new QPushButton(tr("OK"));
  m_ok->setObjectName(QStringLiteral("parametersOK"));
  m_ok->setAutoDefault(false);
  m_add->setAutoDefault(false);
  m_delete->setAutoDefault(false);
  buttons->addButton(m_ok, QDialogButtonBox::AcceptRole);
  closing->addWidget(buttons);
  dialogLayout->addLayout(closing);
  // A table's change changes the values of the parameters that use it.
  connect(m_tablesPage, &TablesPage::changed, this, &ParametersDialog::refresh);
  connect(m_tabs, &QTabWidget::currentChanged, this, [this](int index) {
    qDebug().noquote() << QStringLiteral("Parameters tab shown: %1").arg(m_tabs->tabText(index).remove(QLatin1Char('&')));
    // The keys go to the tab's table (Alt+P, Alt+T).
    if (m_tabs->widget(index) == m_tablesPage) {
      m_tablesPage->takeFocus();
    } else {
      m_table->setFocus();
    }
    m_logged.clear();
    QTimer::singleShot(0, this, &ParametersDialog::logLayout);
  });
  m_table->setFocus();
  mac::styleParametersDialog(this, m_table, m_filter, m_add, m_delete, m_ok);
  mac::styleListButtons(m_add, m_delete);
  mac::styleDialogButtons(buttons, m_ok);

  connect(m_ok, &QPushButton::clicked, this, &QDialog::accept);
  connect(m_add, &QPushButton::clicked, this, &ParametersDialog::addParameter);
  connect(m_delete, &QPushButton::clicked, this, &ParametersDialog::deleteParameter);
  connect(m_formula, &QPushButton::clicked, this, &ParametersDialog::editFormula);
  connect(m_import, &QPushButton::clicked, this, &ParametersDialog::importParameters);
  connect(m_export, &QPushButton::clicked, this, &ParametersDialog::exportParameters);
  connect(m_filter, &QLineEdit::textChanged, this, &ParametersDialog::applyFilter);
  connect(m_limitedOnly, &QCheckBox::toggled, this, [this](bool on) {
    applyFilter();
    QStringList shown;
    for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
      const QString name = (*it)->data(kName, kNameRole).toString();
      if (!name.isEmpty() && !(*it)->isHidden() && !shown.contains(name)) {
        shown << name;
      }
    }
    qDebug().noquote() << QStringLiteral("Parameters only limited: %1: %2")
                              .arg(on ? QStringLiteral("on") : QStringLiteral("off"),
                                   shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", ")));
  });
  connect(m_table, &QTreeWidget::itemDoubleClicked, this, &ParametersDialog::edit);
  connect(m_table, &QTreeWidget::itemChanged, this, &ParametersDialog::cellEdited);
  connect(m_table, &QTreeWidget::itemSelectionChanged, this, &ParametersDialog::updateButtons);
  // A user parameter's row: Add Comment… for an AI agent (mitcad#1119).
  m_table->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(m_table, &QTreeWidget::customContextMenuRequested, this, [this](const QPoint& at) {
    const QTreeWidgetItem* item = m_table->itemAt(at);
    if (item == nullptr || item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
      return;
    }
    const QString name = item->data(kName, kNameRole).toString();
    if (name.isEmpty()) {
      return;
    }
    QMenu menu(this);
    QAction* comment = menu.addAction(themeIcon(QStringLiteral("comment")), tr("Add Comment…"));
    connect(comment, &QAction::triggered, this, [this, name] {
      m_host.addCommentOn({{QStringLiteral("parameter"), name}}, name);
    });
    // As the other context menus log their entries (ui_menu_choose).
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(tr("Add Comment…"));
    execMenu(&menu, m_table->viewport()->mapToGlobal(at));
  });
  refresh();
}

QTreeWidgetItem* ParametersDialog::addRow(QTreeWidgetItem* parent, const QJsonObject& parameter) {
  const QString name = parameter.value(QStringLiteral("name")).toString();
  const bool user = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("user");
  auto* row = new QTreeWidgetItem(parent);
  row->setText(kName, name);
  row->setText(kUnit, parameter.value(QStringLiteral("unit")).toString());
  row->setText(kExpression, parameter.value(QStringLiteral("expression")).toString());
  row->setText(kValue, parameter.value(QStringLiteral("text")).toString());
  row->setText(kComment, parameter.value(QStringLiteral("comment")).toString());
  row->setData(kName, kNameRole, name);
  row->setFlags(row->flags() | Qt::ItemIsEditable | Qt::ItemIsUserCheckable);
  // What a driven dimension measures: renamed, never set (mitcad#231).
  const bool reference = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("reference");
  // A text parameter (mitcad#664): its value is its text; no limits, no
  // slider.
  const bool text = isTextUnit(parameter.value(QStringLiteral("unit")).toString());
  row->setData(kName, kKindRole,
               user ? QStringLiteral("user") : reference ? QStringLiteral("reference") : QStringLiteral("model"));
  for (const int column : {kName, kUnit, kExpression, kValue, kComment, kFavorite, kMin, kMax, kStep, kStrict, kSlider}) {
    const bool limit = column == kMin || column == kMax || column == kStep;
    const bool editable = column == kName || (column == kExpression && !reference) || column == kComment ||
                          (column == kUnit && user) || (limit && !reference && !text);
    row->setData(column, NativeEditableRole, editable);
    row->setData(column, NativeControlIdentifierRole,
                 QStringLiteral("parameter.%1.%2").arg(name).arg(column));
  }
  if (reference) {
    row->setToolTip(kExpression, tr("Measured by a driven dimension; make it driving to set it"));
  }
  row->setCheckState(kFavorite, parameter.value(QStringLiteral("favorite")).toBool() ? Qt::Checked : Qt::Unchecked);
  row->setToolTip(kFavorite, tr("Favorite"));
  const QStringList uses = parameter.value(QStringLiteral("dependencies")).toVariant().toStringList();
  if (!uses.isEmpty()) {
    row->setToolTip(kExpression, tr("Uses %1").arg(uses.join(QStringLiteral(", "))));
  }
  // Limits (mitcad#665): its own in Min, Max and Step, else a feature's
  // fixed ones in grey; the mark of a limited value and why.
  const QJsonObject limits = parameter.value(QStringLiteral("limits")).toObject();
  const QJsonArray intrinsic = parameter.value(QStringLiteral("intrinsic")).toArray();
  const QJsonObject fixed = intrinsic.isEmpty() ? QJsonObject() : intrinsic.first().toObject();
  row->setData(kName, kHasLimitsRole, !limits.isEmpty());
  for (const int column : {kMin, kMax, kStep}) {
    const QString key = limitKey(column, kMin, kMax);
    const QString own = limits.value(key).toString();
    row->setData(column, kOwnLimitRole, own);
    const QString fixedText = fixed.value(key + QStringLiteral("_text")).toString();
    if (!own.isEmpty()) {
      row->setText(column, own);
      const QString value = limits.value(key + QStringLiteral("_text")).toString();
      if (value != own) {
        row->setToolTip(column, QStringLiteral("%1 = %2").arg(own, value));
      }
    } else if (!fixedText.isEmpty()) {
      row->setText(column, fixedText);
      row->setData(column, kFixedLimitRole, fixedText);
      row->setForeground(column, m_table->palette().color(QPalette::Disabled, QPalette::Text));
      row->setToolTip(column, tr("Fixed by %1 (%2): %3")
                                  .arg(fixed.value(QStringLiteral("feature_name")).toString(),
                                       fixed.value(QStringLiteral("slot")).toString(),
                                       fixed.value(QStringLiteral("describe")).toString()));
    }
  }
  if (!reference && !text) {
    row->setCheckState(kStrict, limits.value(QStringLiteral("strict")).toBool() ? Qt::Checked : Qt::Unchecked);
    row->setToolTip(kStrict, tr("Strict: refuse a value outside the limits instead of limiting it"));
  }
  if (text) {
    row->setToolTip(kValue, tr("A text: %1").arg(parameter.value(QStringLiteral("text")).toString()));
  }
  const QString limited = parameter.value(QStringLiteral("limited")).toString();
  row->setData(kName, kLimitedRole, limited);
  if (!limited.isEmpty()) {
    row->setText(kValue, QStringLiteral("%1 → %2 %3")
                             .arg(parameter.value(QStringLiteral("unclamped_text")).toString(),
                                  parameter.value(QStringLiteral("text")).toString(), limitMark(limited)));
    QStringList why{parameter.value(QStringLiteral("limit_message")).toString()};
    const QString bound = limits.value(limited).toString();
    if (!bound.isEmpty()) {
      why << tr("%1 = %2").arg(limited == QStringLiteral("min")   ? tr("Minimum")
                               : limited == QStringLiteral("max") ? tr("Maximum")
                                                                  : tr("Step"),
                               bound);
    }
    const QStringList from = parameter.value(QStringLiteral("limit_references")).toVariant().toStringList();
    if (!from.isEmpty()) {
      why << tr("The limits use %1").arg(from.join(QStringLiteral(", ")));
    }
    row->setToolTip(kValue, why.join(QLatin1Char('\n')));
  }
  if (!reference && !text && !m_scope.holds(parameter)) {
    addSlider(row, parameter);
  }
  markOption(row, parameter);
  for (const int column : {kName, kUnit, kExpression, kComment, kMin, kMax, kStep}) {
    const auto refused = m_refused.constFind(refusedKey(name, column));
    if (refused != m_refused.cend()) {
      row->setText(column, refused.value().first);
      if (!mac::styleRefusedParameter(row, column)) {
        row->setForeground(column, refusedText());
        row->setBackground(column, refusedFill());
      }
      row->setToolTip(column, refused.value().second);
    }
  }
  if (m_scope.holds(parameter)) {
    // The open panel's own value (mitcad#564).
    row->setData(kName, kHeldRole, true);
    for (const int column : {kName, kExpression}) {
      row->setData(column, NativeEditableRole, false);
      row->setToolTip(column, m_scope.holdReason(name));
    }
  }
  return row;
}

void ParametersDialog::markOption(QTreeWidgetItem* row, const QJsonObject& parameter) {
  const QString name = parameter.value(QStringLiteral("name")).toString();
  const configurations::OptionReport* option = m_options.option(name);
  if (option == nullptr && !parameter.value(QStringLiteral("option")).toBool()) {
    return;
  }
  row->setIcon(kName, themeIcon(QStringLiteral("configurations")));
  // The configuration's words (mitcad#968, docs/ui-conventions.md
  // "Configurations").
  QStringList tip{tr("An option of the configuration (Configurations…)")};
  QString logged = name;
  if (option != nullptr && option->driven) {
    tip << tr("By formula: calculated from its expression; neither a chooser nor automatic adjustment sets it");
  }
  if (option != nullptr && !option->restrictedBy.isEmpty()) {
    const configurations::Domain& allowed = option->allowed;
    const QString rules = option->restrictedBy.join(QStringLiteral(", "));
    const QString narrowed = tr("Allowed values now: %2 (restricted by %1)").arg(rules, allowed.summary());
    tip << narrowed;
    logged += QStringLiteral(" (narrowed by %1: %2)").arg(rules, allowed.summary());
    if (allowed.kind == configurations::Domain::Kind::Range) {
      // The rules' bounds as a feature's fixed limits are shown: in grey
      // where the parameter has none of its own, else in the tooltip.
      const std::pair<int, QString> bounds[] = {{kMin, allowed.min ? allowed.minText : QString()},
                                                {kMax, allowed.max ? allowed.maxText : QString()},
                                                {kStep, allowed.step ? allowed.stepText : QString()}};
      const configurations::Domain& domain = option->domain;
      const QString own[] = {domain.min ? domain.minText : QString(), domain.max ? domain.maxText : QString(),
                             domain.step ? domain.stepText : QString()};
      for (int i = 0; i < 3; ++i) {
        const int column = bounds[i].first;
        const QString& bound = bounds[i].second;
        if (bound.isEmpty() || bound == own[i]) {
          continue;
        }
        const QString why = tr("Restricted to %1 by %2").arg(bound, rules);
        if (row->data(column, kOwnLimitRole).toString().isEmpty()) {
          row->setText(column, bound);
          row->setData(column, kFixedLimitRole, bound);
          row->setForeground(column, m_table->palette().color(QPalette::Disabled, QPalette::Text));
          row->setToolTip(column, why);
        } else {
          row->setToolTip(column, row->toolTip(column).isEmpty() ? why : row->toolTip(column) + QLatin1Char('\n') + why);
        }
      }
    } else {
      const QString value = row->toolTip(kValue);
      row->setToolTip(kValue, value.isEmpty() ? narrowed : value + QLatin1Char('\n') + narrowed);
    }
  }
  row->setToolTip(kName, tip.join(QLatin1Char('\n')));
  if (!m_optionRows.contains(logged)) {
    m_optionRows << logged;
  }
}

void ParametersDialog::refresh() {
  updateScope(); // what is edited now (mitcad#564)
  // The configuration's options (mitcad#807); none when the design has no
  // table or the model cannot report it.
  m_options = configurations::Report();
  m_optionRows.clear();
  try {
    if (!configurations::readTable(m_host.model().queryObject(configurations::configurationsQuery()))
             .options.isEmpty()) {
      m_options = configurations::readReport(m_host.model().queryObject(configurations::configureQuery()));
    }
  } catch (const std::exception&) {
    m_options = configurations::Report();
  }
  QString current;
  if (QTreeWidgetItem* item = m_table->currentItem()) {
    current = item->data(kName, kNameRole).toString();
  }
  m_building = true;
  m_table->clear();
  QJsonArray parameters;
  QHash<QString, QString> featureNames;
  try {
    parameters = m_host.model().queryArray(QStringLiteral("parameters"));
    for (const QJsonValue& value : m_host.model()
                                       .queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}})
                                       .value(QStringLiteral("features"))
                                       .toArray()) {
      const QJsonObject feature = value.toObject();
      featureNames.insert(feature.value(QStringLiteral("uid")).toString(),
                          feature.value(QStringLiteral("name")).toString());
    }
  } catch (const std::exception& e) {
    m_message->setText(mitcad::modelMessage(e.what()));
  }
  // Favourites first (P9); they stay in their own
  // groups too.
  auto* favorites = new QTreeWidgetItem(m_table, {tr("Favorites")});
  auto* users = new QTreeWidgetItem(m_table, {tr("User Parameters")});
  auto* models = new QTreeWidgetItem(m_table, {tr("Model Parameters")});
  // Section rows' stable identifiers (native tests, mitcad#814).
  favorites->setData(kName, NativeControlIdentifierRole, QStringLiteral("parameters.group.favorites"));
  users->setData(kName, NativeControlIdentifierRole, QStringLiteral("parameters.group.users"));
  models->setData(kName, NativeControlIdentifierRole, QStringLiteral("parameters.group.models"));
  QHash<QString, QTreeWidgetItem*> owners;
  QStringList names;
  QStringList rows; // for the log
  QStringList limitedRows;
  QStringList favored;
  for (const QJsonValue& value : parameters) {
    const QJsonObject parameter = value.toObject();
    if (parameter.value(QStringLiteral("favorite")).toBool()) {
      favored << parameter.value(QStringLiteral("name")).toString();
      addRow(favorites, parameter);
    }
  }
  if (favored.isEmpty()) {
    delete favorites;
    favorites = nullptr;
  }
  for (const QJsonValue& value : parameters) {
    const QJsonObject parameter = value.toObject();
    names << parameter.value(QStringLiteral("name")).toString();
    // A limited value with its mark (mitcad#665).
    const QString limited = parameter.value(QStringLiteral("limited")).toString();
    rows << QStringLiteral("%1 = %2 (%3%4)")
                .arg(names.last(), parameter.value(QStringLiteral("expression")).toString(),
                     parameter.value(QStringLiteral("text")).toString(),
                     limited.isEmpty() ? QString() : QString(QLatin1Char(' ') + limitMark(limited)));
    QTreeWidgetItem* parent = users;
    if (parameter.value(QStringLiteral("kind")).toString() != QStringLiteral("user")) {
      const QString owner = parameter.value(QStringLiteral("owner")).toString();
      parent = owners.value(owner);
      if (parent == nullptr) {
        parent = new QTreeWidgetItem(models, {featureNames.value(owner, owner)});
        parent->setData(kName, NativeControlIdentifierRole,
                        QStringLiteral("parameters.group.%1").arg(featureNames.value(owner, owner)));
        parent->setExpanded(true);
        owners.insert(owner, parent);
      }
    }
    QTreeWidgetItem* row = addRow(parent, parameter);
    if (row->data(kName, kNameRole).toString() == current) {
      m_table->setCurrentItem(row);
    }
    // The marked cell and its tooltip, for the log (mitcad#665).
    if (!limited.isEmpty()) {
      limitedRows << QStringLiteral("%1 = %2 (%3)")
                         .arg(names.last(), row->text(kValue),
                              row->toolTip(kValue).replace(QLatin1Char('\n'), QStringLiteral("; ")));
    }
  }
  // Refusals of parameters that are gone are forgotten.
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = names.contains(it.key().section(QLatin1Char('|'), 0, 0)) ? std::next(it) : m_refused.erase(it);
  }
  users->setExpanded(true);
  models->setExpanded(true);
  for (QTreeWidgetItem* group : {favorites, users, models}) {
    if (group == nullptr) {
      continue;
    }
    group->setExpanded(true);
    QFont font = group->font(kName);
    font.setBold(true);
    group->setFont(kName, font);
    group->setFirstColumnSpanned(true);
  }
  // A feature's section row is bold too: the row delegate paints every row
  // without a parameter so; the font role says it to other presenters
  // (the AppKit table, mitcad#814).
  for (QTreeWidgetItem* group : std::as_const(owners)) {
    QFont font = group->font(kName);
    font.setBold(true);
    group->setFont(kName, font);
  }
  m_building = false;
  m_align->invalidate();
  applyFilter();
  updateButtons();
  const QString logged = rows.join(QStringLiteral("; "));
  if (logged != m_loggedRows && isVisible()) {
    m_loggedRows = logged;
    qDebug().noquote() << QStringLiteral("Parameters: %1").arg(logged);
  }
  // The parameters a limit changed, with the model's reason (mitcad#665).
  const QString limitedNames = limitedRows.join(QStringLiteral("; "));
  if (limitedNames != m_loggedLimited && isVisible()) {
    m_loggedLimited = limitedNames;
    qDebug().noquote() << QStringLiteral("Parameters limited: %1")
                              .arg(limitedNames.isEmpty() ? QStringLiteral("none") : limitedNames);
  }
  const QString favoriteNames = favored.join(QStringLiteral(", "));
  if (favoriteNames != m_loggedFavorites && isVisible()) {
    m_loggedFavorites = favoriteNames;
    qDebug().noquote() << QStringLiteral("Parameters favorites: %1")
                              .arg(favoriteNames.isEmpty() ? QStringLiteral("none") : favoriteNames);
  }
  // The options and the rules' restrictions (mitcad#807).
  const QString optionNames = m_optionRows.join(QStringLiteral("; "));
  if (optionNames != m_loggedOptions && isVisible()) {
    m_loggedOptions = optionNames;
    qDebug().noquote() << QStringLiteral("Parameters options: %1")
                              .arg(optionNames.isEmpty() ? QStringLiteral("none") : optionNames);
  }
  m_tablesPage->refresh(); // the tables, with the parameters that use them (mitcad#667)
  QTimer::singleShot(0, this, &ParametersDialog::logLayout);
}

void ParametersDialog::edit(QTreeWidgetItem* item, int column) {
  if (item == nullptr || item->data(kName, kNameRole).toString().isEmpty() || column == kValue ||
      column == kFavorite || column == kStrict || column == kSlider) {
    return;
  }
  // A driven dimension's reference has no limits (mitcad#665), nor has a
  // text parameter (mitcad#664).
  if ((column == kMin || column == kMax || column == kStep) &&
      (item->data(kName, kKindRole).toString() == QStringLiteral("reference") || isTextUnit(item->text(kUnit)))) {
    return;
  }
  if ((column == kName || column == kExpression) && item->data(kName, kHeldRole).toBool()) {
    // The open panel's own value (mitcad#564).
    const QString name = item->data(kName, kNameRole).toString();
    m_message->setText(m_scope.holdReason(name));
    qDebug().noquote() << QStringLiteral("Parameter %1 is held by the open panel of %2")
                              .arg(name, m_scope.editedName.isEmpty() ? m_scope.editedFeature : m_scope.editedName);
    return;
  }
  // A model parameter's unit is its feature's.
  if (column == kUnit && item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
    return;
  }
  // A reference's expression is what its dimension measures (mitcad#231).
  if (column == kExpression && item->data(kName, kKindRole).toString() == QStringLiteral("reference")) {
    return;
  }
  m_table->editItem(item, column);
  if (column == kExpression) {
    // Parameter suggestions and the fx browser in the expression's editor
    // (mitcad#567); no dialog from it, which would end the cell's editing.
    if (auto* editor = qobject_cast<QLineEdit*>(m_table->itemWidget(item, column))) {
      ParameterFieldOptions suggest;
      suggest.kind = kindOfUnit(item->text(kUnit));
      suggest.own = item->data(kName, kNameRole).toString();
      suggest.label = QStringLiteral("%1 expression").arg(suggest.own);
      suggest.create = false;
      ParameterField::attach(editor, suggest);
      // What the limits would make of it, while it is typed (mitcad#665).
      if (item->data(kName, kHasLimitsRole).toBool()) {
        const QString name = item->data(kName, kNameRole).toString();
        const QString unit = item->text(kUnit);
        connect(editor, &QLineEdit::textEdited, this, [this, name, unit](const QString& text) {
          previewExpression(name, withUnit(text.trimmed(), unit));
        });
      }
    }
  }
  qDebug().noquote() << QStringLiteral("Editing parameter %1 %2")
                            .arg(item->data(kName, kNameRole).toString(), columnLogName(column));
}

void ParametersDialog::cellEdited(QTreeWidgetItem* item, int column) {
  const QString name = item->data(kName, kNameRole).toString();
  if (m_building || name.isEmpty() || column == kValue) {
    return;
  }
  QString text = item->text(column).trimmed();
  QJsonObject command{{QStringLiteral("name"), name}};
  switch (column) {
  case kFavorite: {
    // The star (P9); the model makes no undo step when nothing changes.
    const bool on = item->checkState(kFavorite) == Qt::Checked;
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("favorite"), on);
    text = on ? QStringLiteral("on") : QStringLiteral("off");
    break;
  }
  case kName:
    if (text == name) {
      return;
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("rename_parameter"));
    command.insert(QStringLiteral("new_name"), text);
    break;
  case kUnit:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("unit"), text);
    break;
  case kExpression:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("expression"), withUnit(text, item->text(kUnit)));
    break;
  case kComment:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("comment"), item->text(column));
    break;
  case kMin:
  case kMax:
  case kStep: {
    // A limit (mitcad#665); empty removes it. A feature's fixed limit
    // left as shown is no limit of the parameter's own.
    const QString own = item->data(column, kOwnLimitRole).toString();
    const QString fixed = item->data(column, kFixedLimitRole).toString();
    if (text == own || (own.isEmpty() && !fixed.isEmpty() && text == fixed)) {
      afterEventWhenIdle(m_host, this, [this] { refresh(); });
      return;
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(limitKey(column, kMin, kMax), text);
    break;
  }
  case kStrict: {
    // A row is built with its box set; a change is the user's.
    const bool on = item->checkState(kStrict) == Qt::Checked;
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("strict"), on);
    text = on ? QStringLiteral("on") : QStringLiteral("off");
    break;
  }
  default:
    return;
  }
  submit(name, column, text, columnLogName(column), command);
}

void ParametersDialog::submit(const QString& name, int column, const QString& text, const QString& what,
                              const QJsonObject& command) {
  // The model command refreshes the window, this table too: not while the
  // table is still handling its edit.
  afterEventWhenIdle(m_host, this, [this, name, column, text, what, command] {
    const CommandContext& model = m_host.model();
    // The window logs what the change did to the bodies.
    if (m_host.runParameterCommand(command)) {
      m_refused.remove(refusedKey(name, column));
      m_message->clear();
      QString value;
      QString limitMessage;
      const QString shown = column == kName ? text : name;
      for (const QJsonValue& parameter : model.queryArray(QStringLiteral("parameters"))) {
        if (parameter.toObject().value(QStringLiteral("name")).toString() == shown) {
          value = parameter.toObject().value(QStringLiteral("text")).toString();
          limitMessage = parameter.toObject().value(QStringLiteral("limit_message")).toString();
        }
      }
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 = %4").arg(name, what, text, value);
      // A limit took effect (mitcad#665): said at once.
      if (!limitMessage.isEmpty()) {
        m_message->setText(limitMessage);
        qDebug().noquote() << QStringLiteral("Parameter %1 limited: %2").arg(shown, limitMessage);
      }
    } else if (m_host.lastCancelled()) {
      // Cancelled while it computed (P7): no refusal, and the cell shows
      // the parameter as it still is.
      m_refused.remove(refusedKey(name, column));
      m_message->setText(tr("%1: the change was canceled.").arg(name));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 cancelled").arg(name, what, text);
    } else {
      // The model's reason stays with the cell; the document is unchanged.
      const QString problem = mitcad::modelMessage(m_host.lastError());
      m_refused.insert(refusedKey(name, column), {text, problem});
      m_message->setText(QStringLiteral("%1: %2").arg(name, problem));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 refused: %4").arg(name, what, text, problem);
    }
    refresh();
  });
}

void ParametersDialog::addParameter() {
  AddParameterDialog dialog(m_host, this);
  qDebug().noquote() << QStringLiteral("Add User Parameter dialog opened: unit %1").arg(dialog.unit());
  prepareModal(&dialog);
  execDialog(&dialog);
  refresh();
}

void ParametersDialog::deleteParameter() {
  QTreeWidgetItem* item = m_table->currentItem();
  if (item == nullptr || item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
    return;
  }
  const QString name = item->data(kName, kNameRole).toString();
  if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("delete_parameter")},
                                  {QStringLiteral("name"), name}})) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Deleted parameter %1").arg(name);
  } else {
    m_message->setText(QStringLiteral("%1: %2").arg(name, mitcad::modelMessage(m_host.lastError())));
    qDebug().noquote() << QStringLiteral("Delete parameter %1 refused: %2").arg(name, m_host.lastError());
  }
  refresh();
}

void ParametersDialog::exportParameters() {
  QJsonObject table;
  try {
    table = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("parameters_csv")}});
  } catch (const std::exception& e) {
    m_message->setText(mitcad::modelMessage(e.what()));
    return;
  }
  const QString folder = dialogStartFolder(DialogGroup::Export, m_host.documentFolder());
  const QString suggested =
      folder.isEmpty() ? QStringLiteral("parameters.csv") : QDir(folder).filePath(QStringLiteral("parameters.csv"));
  const QString path =
      dialogSaveFileName(this, tr("Export Parameters to CSV"), suggested, tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Export of the parameters cancelled");
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QByteArray bytes = table.value(QStringLiteral("csv")).toString().toUtf8();
  QFile file(path);
  if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate) || file.write(bytes) != bytes.size()) {
    const QString problem = tr("Could not write %1: %2").arg(QDir::toNativeSeparators(path), file.errorString());
    m_message->setText(problem);
    qDebug().noquote() << QStringLiteral("Export of the parameters refused: %1").arg(problem);
    return;
  }
  file.close();
  m_message->clear();
  const int count = table.value(QStringLiteral("parameters")).toInt();
  m_host.showStatus(tr("Exported %n parameter(s) to %1.", nullptr, count).arg(QDir::toNativeSeparators(path)),
                    NoticeLevel::Information);
  qDebug().noquote() << QStringLiteral("Exported parameters to %1: %2 parameter(s)").arg(path).arg(count);
}

void ParametersDialog::importParameters() {
  if (!m_scope.isEmpty()) {
    return;
  }
  const QString path = dialogOpenFileName(this, tr("Import Parameters from CSV"),
                                          dialogStartFolder(DialogGroup::Import, m_host.documentFolder()),
                                          tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Import of parameters cancelled");
    return;
  }
  rememberDialogFolder(DialogGroup::Import, path);
  const QString file = QFileInfo(path).fileName();
  QFile input(path);
  if (!input.open(QIODevice::ReadOnly)) {
    const QString problem = tr("Could not read %1: %2").arg(QDir::toNativeSeparators(path), input.errorString());
    m_message->setText(problem);
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 refused: %2").arg(file, problem);
    return;
  }
  // The model reads the table (Mitcad's or the .f3d application's).
  const QString csv = QString::fromUtf8(input.readAll());
  input.close();
  QJsonObject preview;
  try {
    preview = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("parameters_csv_preview")},
                                          {QStringLiteral("csv"), csv}});
  } catch (const std::exception& e) {
    const QString problem = mitcad::modelMessage(e.what());
    m_message->setText(QStringLiteral("%1: %2").arg(file, problem));
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 refused: %2").arg(file, problem);
    return;
  }
  ImportPreviewDialog dialog(preview, file, this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 cancelled").arg(file);
    return;
  }
  QJsonObject result;
  if (m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("import_parameters_csv")},
                              {QStringLiteral("csv"), csv}},
                             &result)) {
    m_message->clear();
    const QString summary = result.value(QStringLiteral("summary")).toString();
    m_host.showStatus(tr("Imported parameters from %1: %2.").arg(file, summary), NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Imported parameters from %1: %2").arg(file, summary);
  } else if (m_host.lastCancelled()) {
    m_message->setText(tr("%1: the import was canceled.").arg(file));
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 cancelled while computing").arg(file);
  } else {
    m_message->setText(QStringLiteral("%1: %2").arg(file, mitcad::modelMessage(m_host.lastError())));
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 refused: %2").arg(file, m_host.lastError());
  }
  refresh();
}

void ParametersDialog::editFormula() {
  QTreeWidgetItem* item=m_table->currentItem();
  if (!item) return;
  const QString name=item->data(kName,kNameRole).toString();
  if (name.isEmpty() || item->data(kName,kKindRole).toString()==QStringLiteral("reference")) return;
  if (item->data(kName,kHeldRole).toBool()) {
    m_message->setText(m_scope.holdReason(name));
    return;
  }
  const ParameterSource* source=parameterSourceOf(this);
  if (!source || !source->query) return;
  ParameterFieldOptions options;
  options.own=name;
  options.label=name;
  options.kind=kindOfUnit(item->text(kUnit));
  options.anyKind=true;
  FormulaEditor dialog(*source,options,item->text(kExpression),this);
  if (execDialog(&dialog)!=QDialog::Accepted) return;
  submit(name,kExpression,dialog.expression(),QStringLiteral("Edited formula"),dialog.parameterCommand());
}

void ParametersDialog::updateButtons() {
  QTreeWidgetItem* item = m_table->currentItem();
  m_delete->setEnabled(item != nullptr &&
                       item->data(kName, kKindRole).toString() == QStringLiteral("user"));
  m_formula->setEnabled(item != nullptr && !item->data(kName,kNameRole).toString().isEmpty() &&
                        item->data(kName,kKindRole).toString()!=QStringLiteral("reference"));
  // An import changes many parameters at once: not while a sketch or a
  // panel holds some (mitcad#666).
  m_import->setEnabled(m_scope.isEmpty());
  m_import->setToolTip(m_scope.isEmpty()
                           ? tr("Import parameters from a CSV file")
                           : tr("Finish the sketch or the command to import parameters"));
}

void ParametersDialog::applyFilter() {
  const QString filter = m_filter->text().trimmed();
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (item->data(kName, kNameRole).toString().isEmpty()) {
      continue; // a group
    }
    const bool limitedOnly = m_limitedOnly != nullptr && m_limitedOnly->isChecked();
    const bool shown = (filter.isEmpty() || item->text(kName).contains(filter, Qt::CaseInsensitive) ||
                        item->text(kComment).contains(filter, Qt::CaseInsensitive)) &&
                       (!limitedOnly || !item->data(kName, kLimitedRole).toString().isEmpty());
    item->setHidden(!shown);
  }
}

void ParametersDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QString name = item->data(kName, kNameRole).toString();
    if (name.isEmpty() || item->isHidden()) {
      continue;
    }
    for (const int column : {kName, kUnit, kExpression, kComment, kFavorite, kMin, kMax, kStep, kStrict}) {
      const QRect rect = m_table->visualItemRect(item);
      const QRect cell(m_table->header()->sectionViewportPosition(column), rect.top(),
                       m_table->header()->sectionSize(column), rect.height());
      // The favourite's and Strict's check boxes sit at the cell's left.
      const QPoint point =
          column == kFavorite || column == kStrict ? QPoint(cell.left() + 10, cell.center().y()) : cell.center();
      lines << QStringLiteral("Parameters %1 %2 at %3")
                   .arg(name, columnLogName(column), at(m_table->viewport(), point));
    }
  }
  // Where each aligned number's decimal separator and unit are (mitcad#566);
  // a text that is no number, or a column too narrow for its numbers, has none.
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QString name = item->data(kName, kNameRole).toString();
    if (name.isEmpty() || item->isHidden()) {
      continue;
    }
    const QRect rect = m_table->visualItemRect(item);
    for (const int column : {kExpression, kValue}) {
      const int left = m_table->header()->sectionViewportPosition(column);
      const std::optional<DecimalAlignDelegate::Placement> place = m_align->placement(
          item->text(column), column, m_table->font(), m_table->header()->sectionSize(column));
      if (!place) {
        continue;
      }
      const QString what = columnLogName(column);
      lines << QStringLiteral("Parameters %1 %2 decimal at %3")
                   .arg(name, what, at(m_table->viewport(), QPoint(left + place->axisX, rect.center().y())));
      if (place->unitX >= 0) {
        lines << QStringLiteral("Parameters %1 %2 unit at %3")
                     .arg(name, what, at(m_table->viewport(), QPoint(left + place->unitX, rect.center().y())));
      }
    }
  }
  // The sliders and their handles (mitcad#665).
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QString name = item->data(kName, kNameRole).toString();
    auto* slider = qobject_cast<QSlider*>(m_table->itemWidget(item, kSlider));
    if (name.isEmpty() || item->isHidden() || slider == nullptr) {
      continue;
    }
    // From the cell the slider fills, as it may not be laid out yet.
    const QRect rect = m_table->visualItemRect(item);
    const QRect cell(m_table->header()->sectionViewportPosition(kSlider), rect.top(),
                     m_table->header()->sectionSize(kSlider), rect.height());
    QStyleOptionSlider option;
    option.initFrom(slider);
    option.rect = QRect(QPoint(0, 0), cell.size());
    option.state |= QStyle::State_Horizontal;
    option.orientation = slider->orientation();
    option.minimum = slider->minimum();
    option.maximum = slider->maximum();
    option.sliderPosition = slider->sliderPosition();
    option.sliderValue = slider->value();
    option.singleStep = slider->singleStep();
    option.pageStep = slider->pageStep();
    option.upsideDown = false;
    option.subControls = QStyle::SC_SliderGroove | QStyle::SC_SliderHandle;
    const QRect handle = slider->style()->subControlRect(QStyle::CC_Slider, &option, QStyle::SC_SliderHandle, slider);
    lines << QStringLiteral("Parameters %1 slider at %2").arg(name, at(m_table->viewport(), cell.center()));
    lines << QStringLiteral("Parameters %1 slider handle at %2")
                 .arg(name, at(m_table->viewport(), cell.topLeft() + handle.center()));
  }
  lines << QStringLiteral("Parameters only limited at %1")
               .arg(at(m_limitedOnly, QPoint(10, m_limitedOnly->rect().center().y())));
  lines << QStringLiteral("Parameters add at %1").arg(at(m_add, m_add->rect().center()));
  lines << QStringLiteral("Parameters delete at %1").arg(at(m_delete, m_delete->rect().center()));
  lines << QStringLiteral("Parameters OK at %1").arg(at(m_ok, m_ok->rect().center()));
  lines << QStringLiteral("Parameters import at %1").arg(at(m_import, m_import->rect().center()));
  lines << QStringLiteral("Parameters export at %1").arg(at(m_export, m_export->rect().center()));
  lines << QStringLiteral("Parameters formula at %1").arg(at(m_formula, m_formula->rect().center()));
  // The tabs (mitcad#667).
  for (int i = 0; i < m_tabs->count(); ++i) {
    lines << QStringLiteral("Parameters tab %1 at %2")
                 .arg(m_tabs->tabText(i).remove(QLatin1Char('&')), at(m_tabs->tabBar(), m_tabs->tabBar()->tabRect(i).center()));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

// While a sketch or a feature is edited (mitcad#564).
void ParametersDialog::updateScope() {
  m_scope = m_host.parameterScope();
  if (m_scopeNote == nullptr) {
    // Above the tabs: it holds for the tables too (mitcad#667).
    m_scopeNote = new QLabel(this);
    m_scopeNote->setObjectName(QStringLiteral("parametersScope"));
    m_scopeNote->setWordWrap(true);
    if (auto* box = qobject_cast<QVBoxLayout*>(layout())) {
      box->insertWidget(0, m_scopeNote);
    }
  }
  const QString note = m_scope.note();
  m_scopeNote->setText(note);
  m_scopeNote->setVisible(!note.isEmpty());
  const QString logged = QStringLiteral("Parameters during %1").arg(m_scope.describe());
  if (isVisible() && logged != m_loggedScope) {
    m_loggedScope = logged;
    qDebug().noquote() << logged;
  }
}

bool ParametersDialog::event(QEvent* event) {
  // Activated in another edit than the one it shows (a panel opened or
  // closed meanwhile): the open panel's own parameters change (a cell
  // editor left open goes with the rows).
  if (event->type() == QEvent::WindowActivate && m_host.parameterScope() != m_scope) {
    refresh();
  }
  if (event->type() == QEvent::Hide) {
    m_loggedScope.clear(); // told again when it opens again
  }
  return QDialog::event(event);
}

// A parameter with a minimum and a maximum of its own (mitcad#665): a
// slider between them, in its steps when it has a step (a thousand
// positions otherwise, and when it has more than ten thousand steps). A
// released handle or a key sets the expression to the number, in the
// parameter's unit.
void ParametersDialog::addSlider(QTreeWidgetItem* row, const QJsonObject& parameter) {
  const QJsonObject limits = parameter.value(QStringLiteral("limits")).toObject();
  const QJsonValue minValue = limits.value(QStringLiteral("min_value"));
  const QJsonValue maxValue = limits.value(QStringLiteral("max_value"));
  if (!minValue.isDouble() || !maxValue.isDouble() || !(maxValue.toDouble() > minValue.toDouble())) {
    return;
  }
  const double minNumber = limits.value(QStringLiteral("min_number")).toDouble();
  const double maxNumber = limits.value(QStringLiteral("max_number")).toDouble();
  const double step = limits.value(QStringLiteral("step_number")).toDouble(0.0);
  int positions = 1000;
  double increment = (maxNumber - minNumber) / positions;
  if (step > 0.0) {
    const double steps = std::floor((maxNumber - minNumber) / step + 1e-9);
    if (steps >= 1.0 && steps <= 10000.0) {
      positions = static_cast<int>(steps);
      increment = step;
    }
  }
  // Where the value is, from the canonical values (the same proportion).
  const double fraction = (parameter.value(QStringLiteral("value")).toDouble() - minValue.toDouble()) /
                          (maxValue.toDouble() - minValue.toDouble());
  const double number = minNumber + fraction * (maxNumber - minNumber);
  const int at = std::clamp(static_cast<int>(std::lround((number - minNumber) / increment)), 0, positions);

  auto* slider = new QSlider(Qt::Horizontal);
  slider->setObjectName(QStringLiteral("parameterSlider"));
  slider->setRange(0, positions);
  slider->setSingleStep(1);
  slider->setPageStep(std::max(1, positions / 10));
  slider->setValue(at);
  slider->setFocusPolicy(Qt::StrongFocus);
  slider->setToolTip(tr("%1 to %2").arg(limits.value(QStringLiteral("min_text")).toString(),
                                         limits.value(QStringLiteral("max_text")).toString()));
  slider->setAccessibleName(tr("%1 slider").arg(parameter.value(QStringLiteral("name")).toString()));
  m_table->setItemWidget(row, kSlider, slider);

  const QString name = parameter.value(QStringLiteral("name")).toString();
  const QString unit = parameter.value(QStringLiteral("unit")).toString();
  const auto commit = [this, name, unit, minNumber, maxNumber, increment, positions, at](int position) {
    if (position == at) {
      return; // the handle clicked, not moved
    }
    const double value = position == positions && increment * positions > maxNumber - minNumber
                             ? maxNumber
                             : minNumber + increment * position;
    const QString text = withUnit(QString::number(value, 'g', 12), unit);
    submit(name, kSlider, text, QStringLiteral("slider"),
           {{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
            {QStringLiteral("name"), name},
            {QStringLiteral("expression"), text}});
  };
  connect(slider, &QSlider::sliderReleased, this, [slider, commit] { commit(slider->value()); });
  connect(slider, &QSlider::valueChanged, this, [this, slider, commit](int position) {
    if (!m_building && !slider->isSliderDown()) {
      commit(position);
    }
  });
}

// What the limits would make of an expression typed into a limited
// parameter's cell, said at once (mitcad#665): limited to a bound, or
// refused by a strict parameter.
void ParametersDialog::previewExpression(const QString& name, const QString& text) {
  QString message;
  if (!text.isEmpty()) {
    try {
      const QJsonObject answer =
          m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                      {QStringLiteral("expression"), text},
                                      {QStringLiteral("parameter"), name}});
      const QString limited = answer.value(QStringLiteral("limited")).toString();
      if (!limited.isEmpty()) {
        message = tr("%1: %2 will be limited to %3 (%4)")
                      .arg(name, answer.value(QStringLiteral("text")).toString(),
                           answer.value(QStringLiteral("limited_text")).toString(), limitMark(limited));
      }
    } catch (const std::exception& e) {
      // A strict parameter refuses it; other problems (a half-typed
      // expression) are told when the edit ends.
      const QString problem = mitcad::modelMessage(e.what());
      if (problem.contains(QStringLiteral("minimum")) || problem.contains(QStringLiteral("maximum")) ||
          problem.contains(QStringLiteral("not on a step"))) {
        message = tr("%1: will be refused: %2").arg(name, problem);
      }
    }
  }
  m_message->setText(message);
  if (message != m_loggedPreview) {
    m_loggedPreview = message;
    qDebug().noquote() << QStringLiteral("Parameter %1 typed %2: %3")
                              .arg(name, text, message.isEmpty() ? QStringLiteral("within its limits") : message);
  }
}

} // namespace mitcad
