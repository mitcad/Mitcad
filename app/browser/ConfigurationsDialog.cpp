// SPDX-License-Identifier: MIT
#include "ConfigurationsDialog.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QAbstractItemView>
#include <QButtonGroup>
#include <QCheckBox>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QDropEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QPaintEvent>
#include <QPainter>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QSignalBlocker>
#include <QTabBar>
#include <QTabWidget>
#include <QTableWidget>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativeDialogs.hpp"
#include "ConfigurationBindingsPage.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

namespace cfg = configurations;

constexpr int kNameRole = Qt::UserRole; // an option's or a row's name

bool isTextUnit(const QString& unit) {
  return unit.trimmed().compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0;
}

// The window explains itself (mitcad#967): a lead line at the top of each
// tab, and what an empty view shows in its place. app/COMMANDS.md and the
// user guide quote them; the Rules tab's are in configurations/RulesPage.
const char* const kOptionsLead =
    "Options are the choices a user of this design makes, such as Color or Length. Each option is a user parameter.";
const char* const kRowsLead =
    "A row is a ready-made combination of the options, with its designation and part number.";
const char* const kNoOptions = "No options yet. Add Option… makes a parameter an option.";
const char* const kNoRows = "No rows yet. Add Row saves the options' values as a row.";

QLabel* leadLine(const char* text, const QString& tab) {
  auto* lead = new QLabel(QString::fromUtf8(text));
  lead->setObjectName(QStringLiteral("configurationsLead") + tab);
  lead->setWordWrap(true);
  qDebug().noquote() << QStringLiteral("Configurations lead %1: %2").arg(tab, lead->text());
  return lead;
}

// The placeholder of an empty view, in its viewport's top left corner: its
// NativeEmptyTextProperty, which the AppKit frontend shows as well.
void paintPlaceholder(QAbstractItemView* view) {
  QPainter painter(view->viewport());
  painter.setPen(view->palette().color(QPalette::PlaceholderText));
  painter.drawText(view->viewport()->rect().adjusted(12, 12, -12, -12),
                   Qt::AlignLeft | Qt::AlignTop | Qt::TextWordWrap,
                   view->property(NativeEmptyTextProperty).toString());
}

// The options as a tree of one level whose rows are dragged into their
// order: `dropped` is called once the drop is done. Empty, it says how to
// add one.
class OptionTree : public QTreeWidget {
public:
  explicit OptionTree(std::function<void()> dropped) : m_dropped(std::move(dropped)) {
    setProperty(NativeEmptyTextProperty, QString::fromUtf8(kNoOptions));
  }

protected:
  void dropEvent(QDropEvent* event) override {
    QTreeWidget::dropEvent(event);
    QTimer::singleShot(0, this, [this] { m_dropped(); });
  }

  void paintEvent(QPaintEvent* event) override {
    QTreeWidget::paintEvent(event);
    if (topLevelItemCount() == 0) {
      paintPlaceholder(this);
    }
  }

private:
  std::function<void()> m_dropped;
};

// Delete (or Backspace) without a modifier: the selected cells emptied.
bool isClearKey(const QKeyEvent* event) {
  return (event->key() == Qt::Key_Delete || event->key() == Qt::Key_Backspace) &&
         !(event->modifiers() & (Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier));
}

// The Rows sheet; empty, it says how to add a row. Outside a cell's editor,
// Copy (Ctrl+C) and Paste (Ctrl+V) work on blocks of cells as spreadsheets
// copy them, and Delete empties the selected cells (mitcad#1044).
class RowsSheet : public QTableWidget {
public:
  RowsSheet(std::function<void()> copy, std::function<void()> paste, std::function<void()> clear)
      : m_copy(std::move(copy)), m_paste(std::move(paste)), m_clear(std::move(clear)) {
    setProperty(NativeEmptyTextProperty, QString::fromUtf8(kNoRows));
    // The AppKit frontend (mitcad#1355): its table selects ranges of cells
    // and sends these keys, and the Edit menu's Copy and Paste, here.
    setProperty(NativeCellSelectionProperty, true);
    setProperty(NativeModelKeysProperty,
                QVariantList{QKeySequence(QKeySequence::Copy)[0].toCombined(),
                             QKeySequence(QKeySequence::Paste)[0].toCombined(), int(Qt::Key_Delete),
                             int(Qt::Key_Backspace)});
  }

protected:
  bool event(QEvent* event) override {
    // The sheet's own keys before any shortcut of the application.
    if (event->type() == QEvent::ShortcutOverride && state() != QAbstractItemView::EditingState) {
      const auto* key = static_cast<QKeyEvent*>(event);
      if (key->matches(QKeySequence::Copy) || key->matches(QKeySequence::Paste) || isClearKey(key)) {
        event->accept();
        return true;
      }
    }
    return QTableWidget::event(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    if (state() != QAbstractItemView::EditingState) {
      if (event->matches(QKeySequence::Copy)) {
        m_copy();
        event->accept();
        return;
      }
      if (event->matches(QKeySequence::Paste)) {
        m_paste();
        event->accept();
        return;
      }
      if (isClearKey(event)) {
        m_clear();
        event->accept();
        return;
      }
    }
    QTableWidget::keyPressEvent(event);
  }

  void paintEvent(QPaintEvent* event) override {
    QTableWidget::paintEvent(event);
    if (rowCount() == 0) {
      paintPlaceholder(this);
    }
  }

private:
  std::function<void()> m_copy;
  std::function<void()> m_paste;
  std::function<void()> m_clear;
};

// Add Option: an existing user parameter that is no option yet, or a new
// one (name, unit, value); the parameter and the option are one undo step.
// Every row takes the parameter's current value (mitcad#1044), so that a
// closed table with rows takes the option; a closed table's parameter
// column moves to the options with its rows' values.
class AddOptionDialog : public QDialog {
public:
  AddOptionDialog(DocumentHost& host, cfg::Table table, const QStringList& candidates,
                  QHash<QString, QJsonObject> parameters, QWidget* parent)
      : QDialog(parent), m_host(host), m_table(std::move(table)), m_parameters(std::move(parameters)) {
    setObjectName(QStringLiteral("addOptionDialog"));
    setWindowTitle(tr("Add Option"));
    auto* form = new QFormLayout(this);
    m_parameter = new QComboBox;
    m_parameter->setObjectName(QStringLiteral("addOptionParameter"));
    for (const QString& name : candidates) {
      // A closed table's parameter column moves to the options.
      m_parameter->addItem(m_table.parameters.contains(name) ? tr("%1 (a parameter column)").arg(name) : name, name);
    }
    m_parameter->addItem(tr("New Parameter"), QString());
    m_parameter->setToolTip(tr("A user parameter becomes the option; or a new parameter"));
    m_name = new QLineEdit;
    m_name->setObjectName(QStringLiteral("addOptionName"));
    m_unit = new QComboBox;
    m_unit->setObjectName(QStringLiteral("addOptionUnit"));
    m_unit->setEditable(true);
    m_unit->addItems({QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"), QStringLiteral("in"),
                      QStringLiteral("deg"), QStringLiteral("Text"), QString()});
    m_value = new QLineEdit(QStringLiteral("0"));
    m_value->setObjectName(QStringLiteral("addOptionValue"));
    m_value->setToolTip(tr("The option's value: a number (with its unit) or a text"));
    form->addRow(tr("Parameter:"), m_parameter);
    form->addRow(tr("Name:"), m_name);
    form->addRow(tr("Unit:"), m_unit);
    form->addRow(tr("Value:"), m_value);
    if (!m_table.rows.isEmpty()) {
      auto* rows = new QLabel(tr("Every row takes the parameter's current value; change a row's value in the Rows tab."));
      rows->setObjectName(QStringLiteral("addOptionRows"));
      rows->setWordWrap(true);
      form->addRow(rows);
    }
    m_message = new QLabel;
    m_message->setWordWrap(true);
    setErrorStyleSheet(m_message);
    form->addRow(m_message);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
    ok->setObjectName(QStringLiteral("addOptionOK"));
    ok->setDefault(true);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { tryAccept(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_parameter, &QComboBox::currentIndexChanged, this, [this] { updateFields(); });
    updateFields();
    m_parameter->setFocus();
  }

  QString added() const { return m_added; }

private:
  void updateFields() {
    const bool fresh = m_parameter->currentData().toString().isEmpty();
    for (QWidget* field : {static_cast<QWidget*>(m_name), static_cast<QWidget*>(m_unit),
                           static_cast<QWidget*>(m_value)}) {
      field->setEnabled(fresh);
    }
  }

  void tryAccept() {
    QString name = m_parameter->currentData().toString();
    QString expression; // each row's value of the new option
    QJsonArray commands;
    if (name.isEmpty()) {
      name = m_name->text().trimmed();
      const QString unit = m_unit->currentText().trimmed();
      const bool text = isTextUnit(unit);
      expression = cfg::rowExpression(text, text ? QString() : unit, m_value->text());
      if (expression.isEmpty()) {
        expression = text ? QStringLiteral("''") : QStringLiteral("0");
      }
      QJsonObject add{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                      {QStringLiteral("name"), name},
                      {QStringLiteral("expression"), expression}};
      if (!unit.isEmpty()) {
        add.insert(QStringLiteral("unit"), unit);
      }
      commands.append(add);
    } else {
      expression = cfg::currentConstant(m_parameters.value(name));
    }
    const bool column = m_table.parameters.contains(name);
    commands.append(cfg::setTableCommand(cfg::withOption(m_table, name, expression)));
    if (m_host.runModelCommands(commands, tr("Add Option %1").arg(name))) {
      if (!m_table.rows.isEmpty()) {
        qDebug().noquote() << (column ? QStringLiteral("Configurations option %1 moved from the parameter columns")
                                            .arg(name)
                                      : QStringLiteral("Configurations rows take %1 = %2").arg(name, expression));
      }
      m_added = name;
      qDebug().noquote() << QStringLiteral("Configurations added option %1").arg(name);
      accept();
    } else if (!m_host.lastCancelled()) {
      m_message->setText(m_host.lastError());
      qDebug().noquote() << QStringLiteral("Configurations add option %1 refused: %2").arg(name, m_host.lastError());
    }
  }

  DocumentHost& m_host;
  cfg::Table m_table;
  QHash<QString, QJsonObject> m_parameters;
  QComboBox* m_parameter = nullptr;
  QLineEdit* m_name = nullptr;
  QComboBox* m_unit = nullptr;
  QLineEdit* m_value = nullptr;
  QLabel* m_message = nullptr;
  QString m_added;
};

// Add Column (mitcad#999): a user parameter that is neither an option nor
// a column yet, or a new one (name, unit, value), becomes a column of the
// closed table: another parameter every row sets, each row's value its
// current expression. An open table becomes closed with it (an open
// table's rows set options only), which the dialog says. The parameter
// and the table are one undo step.
class AddColumnDialog : public QDialog {
public:
  AddColumnDialog(DocumentHost& host, cfg::Table table, const QStringList& candidates,
                  QHash<QString, QJsonObject> parameters, QWidget* parent)
      : QDialog(parent), m_host(host), m_table(std::move(table)), m_parameters(std::move(parameters)) {
    setObjectName(QStringLiteral("addColumnDialog"));
    setWindowTitle(tr("Add Column"));
    auto* form = new QFormLayout(this);
    auto* about = new QLabel(tr("A parameter column: a parameter that every row sets, as d, P, s and k of a screw "
                                "size. It does not become an option."));
    about->setWordWrap(true);
    form->addRow(about);
    m_parameter = new QComboBox;
    m_parameter->setObjectName(QStringLiteral("addColumnParameter"));
    for (const QString& name : candidates) {
      m_parameter->addItem(name, name);
    }
    m_parameter->addItem(tr("New Parameter"), QString());
    m_parameter->setToolTip(tr("A user parameter that is no option; or a new parameter"));
    m_name = new QLineEdit;
    m_name->setObjectName(QStringLiteral("addColumnName"));
    m_unit = new QComboBox;
    m_unit->setObjectName(QStringLiteral("addColumnUnit"));
    m_unit->setEditable(true);
    m_unit->addItems({QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"), QStringLiteral("in"),
                      QStringLiteral("deg"), QStringLiteral("Text"), QString()});
    m_value = new QLineEdit(QStringLiteral("0"));
    m_value->setObjectName(QStringLiteral("addColumnValue"));
    m_value->setToolTip(tr("The parameter's value, which every row then has: a number (with its unit) or a text"));
    form->addRow(tr("Parameter:"), m_parameter);
    form->addRow(tr("Name:"), m_name);
    form->addRow(tr("Unit:"), m_unit);
    form->addRow(tr("Value:"), m_value);
    if (!m_table.closed) {
      auto* closes = new QLabel(tr("Only these rows turns on: only the combinations listed in the Rows tab are "
                                   "allowed."));
      closes->setObjectName(QStringLiteral("addColumnCloses"));
      closes->setWordWrap(true);
      form->addRow(closes);
    }
    m_message = new QLabel;
    m_message->setWordWrap(true);
    setErrorStyleSheet(m_message);
    form->addRow(m_message);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
    ok->setObjectName(QStringLiteral("addColumnOK"));
    ok->setDefault(true);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { tryAccept(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_parameter, &QComboBox::currentIndexChanged, this, [this] { updateFields(); });
    updateFields();
    m_parameter->setFocus();
  }

  QString added() const { return m_added; }

private:
  void updateFields() {
    const bool fresh = m_parameter->currentData().toString().isEmpty();
    for (QWidget* field : {static_cast<QWidget*>(m_name), static_cast<QWidget*>(m_unit),
                           static_cast<QWidget*>(m_value)}) {
      field->setEnabled(fresh);
    }
  }

  void tryAccept() {
    QString name = m_parameter->currentData().toString();
    QString expression;
    QJsonArray commands;
    if (name.isEmpty()) {
      name = m_name->text().trimmed();
      const QString unit = m_unit->currentText().trimmed();
      const bool text = isTextUnit(unit);
      expression = cfg::rowExpression(text, text ? QString() : unit, m_value->text());
      if (expression.isEmpty()) {
        expression = text ? QStringLiteral("''") : QStringLiteral("0");
      }
      QJsonObject add{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                      {QStringLiteral("name"), name},
                      {QStringLiteral("expression"), expression}};
      if (!unit.isEmpty()) {
        add.insert(QStringLiteral("unit"), unit);
      }
      commands.append(add);
    } else {
      // Each row's value: the parameter's expression as it is now.
      expression = m_parameters.value(name).value(QStringLiteral("expression")).toString();
    }
    if (const QString readOnly = m_host.readOnlyReason(); !readOnly.isEmpty()) {
      m_message->setText(readOnly);
      return;
    }
    if (!m_host.canChangeModel()) {
      m_message->setText(m_host.changeBlockedReason());
      return;
    }
    commands.append(cfg::setTableCommand(cfg::withColumn(m_table, name, expression)));
    if (m_host.runModelCommands(commands, tr("Add Column %1").arg(name))) {
      m_added = name;
      qDebug().noquote() << QStringLiteral("Configurations: column %1 added").arg(name);
      accept();
    } else if (!m_host.lastCancelled()) {
      m_message->setText(m_host.lastError());
      qDebug().noquote() << QStringLiteral("Configurations add column %1 refused: %2").arg(name, m_host.lastError());
    }
  }

  DocumentHost& m_host;
  cfg::Table m_table;
  QHash<QString, QJsonObject> m_parameters;
  QComboBox* m_parameter = nullptr;
  QLineEdit* m_name = nullptr;
  QComboBox* m_unit = nullptr;
  QLineEdit* m_value = nullptr;
  QLabel* m_message = nullptr;
  QString m_added;
};

// Edit Possible Values (the option's domain): what an option can be before
// the rules, as its parameter's limits: a list (the `values` limit, one
// value per line), a range (min, max, step), a column of one of the design's tables (the `values` limit
// as `Table.Column`, mitcad#849: the column's distinct cells) or free. A
// text option is a list, a text column or free; a number option takes a
// number column.
class DomainDialog : public QDialog {
public:
  DomainDialog(DocumentHost& host, QString name, const QJsonObject& parameter, bool text,
               const std::vector<SuggestedTable>& tables, QWidget* parent)
      : QDialog(parent), m_host(host), m_name(std::move(name)), m_text(text) {
    setObjectName(QStringLiteral("optionDomainDialog"));
    setWindowTitle(tr("Possible Values"));
    m_unit = parameter.value(QStringLiteral("unit")).toString();
    const QJsonObject limits = parameter.value(QStringLiteral("limits")).toObject();
    m_columnWas = cfg::valuesColumn(parameter);
    if (m_columnWas.isEmpty()) {
      for (const QJsonValue& value : limits.value(QStringLiteral("values")).toArray()) {
        m_values << value.toString();
      }
    }
    m_min = limits.value(QStringLiteral("min")).toString();
    m_max = limits.value(QStringLiteral("max")).toString();
    m_step = limits.value(QStringLiteral("step")).toString();

    auto* layout = new QVBoxLayout(this);
    layout->addWidget(new QLabel(tr("The values %1 can have before rules and rows restrict them:").arg(m_name)));
    auto* kinds = new QHBoxLayout;
    m_list = new QRadioButton(tr("List"));
    m_list->setObjectName(QStringLiteral("domainList"));
    m_range = new QRadioButton(tr("Range"));
    m_range->setObjectName(QStringLiteral("domainRange"));
    m_range->setEnabled(!m_text);
    m_column = new QRadioButton(tr("&Table Column"));
    m_column->setObjectName(QStringLiteral("domainColumn"));
    m_free = new QRadioButton(tr("Free"));
    m_free->setObjectName(QStringLiteral("domainFree"));
    auto* group = new QButtonGroup(this);
    for (QRadioButton* button : {m_list, m_range, m_column, m_free}) {
      group->addButton(button);
      kinds->addWidget(button);
    }
    kinds->addStretch(1);
    layout->addLayout(kinds);
    // The columns of the option's kind: texts for a text option, numbers
    // for a number option (the model checks the dimension).
    m_columnChoice = new QComboBox;
    m_columnChoice->setObjectName(QStringLiteral("domainColumnChoice"));
    for (const SuggestedTable& table : tables) {
      for (const SuggestedTable::Column& column : table.columns) {
        if (isTextUnit(column.unit) != m_text) {
          continue;
        }
        m_columnChoice->addItem(QStringLiteral("%1.%2").arg(table.name, columnLabel(column)),
                                cfg::columnReference(table.name, column.name));
      }
    }
    if (!m_columnWas.isEmpty() && m_columnChoice->findData(m_columnWas) < 0) {
      m_columnChoice->addItem(m_columnWas, m_columnWas);
    }
    m_columnChoice->setCurrentIndex(std::max(0, m_columnChoice->findData(m_columnWas)));
    m_columnChoice->setToolTip(tr("The option can be each value of the column; a table edit updates it"));
    if (m_columnChoice->count() == 0) {
      m_column->setEnabled(false);
      m_column->setToolTip(m_text ? tr("The design has no text column in its tables")
                                  : tr("The design has no number column in its tables"));
    }
    auto* columnForm = new QFormLayout;
    columnForm->addRow(tr("Column:"), m_columnChoice);
    layout->addLayout(columnForm);
    m_lines = new QPlainTextEdit;
    m_lines->setObjectName(QStringLiteral("domainValues"));
    m_lines->setToolTip(m_text ? tr("One text per line") : tr("One value per line; a bare number gets the unit"));
    QStringList shown;
    for (const QString& value : std::as_const(m_values)) {
      shown << cfg::rowText(m_text, value);
    }
    m_lines->setPlainText(shown.join(QLatin1Char('\n')));
    layout->addWidget(m_lines, 1);
    auto* form = new QFormLayout;
    m_minField = new QLineEdit(m_min);
    m_minField->setObjectName(QStringLiteral("domainMin"));
    m_maxField = new QLineEdit(m_max);
    m_maxField->setObjectName(QStringLiteral("domainMax"));
    m_stepField = new QLineEdit(m_step);
    m_stepField->setObjectName(QStringLiteral("domainStep"));
    form->addRow(tr("Minimum:"), m_minField);
    form->addRow(tr("Maximum:"), m_maxField);
    form->addRow(tr("Step:"), m_stepField);
    layout->addLayout(form);
    m_message = new QLabel;
    m_message->setWordWrap(true);
    setErrorStyleSheet(m_message);
    layout->addWidget(m_message);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    buttons->button(QDialogButtonBox::Ok)->setObjectName(QStringLiteral("domainOK"));
    buttons->button(QDialogButtonBox::Ok)->setDefault(true);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { tryAccept(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(group, &QButtonGroup::buttonToggled, this, [this] { updateFields(); });
    if (!m_columnWas.isEmpty()) {
      m_column->setChecked(true);
    } else if (!m_values.isEmpty()) {
      m_list->setChecked(true);
    } else if (!m_text && (!m_min.isEmpty() || !m_max.isEmpty() || !m_step.isEmpty())) {
      m_range->setChecked(true);
    } else {
      m_free->setChecked(true);
    }
    updateFields();
  }

private:
  void updateFields() {
    m_lines->setEnabled(m_list->isChecked());
    m_columnChoice->setEnabled(m_column->isChecked());
    for (QLineEdit* field : {m_minField, m_maxField, m_stepField}) {
      field->setEnabled(m_range->isChecked());
    }
  }

  void tryAccept() {
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_parameter")}, {QStringLiteral("name"), m_name}};
    QString described;
    // A limit is sent only when it changes: a text parameter takes no
    // range at all.
    const auto setLimit = [&command](const char* key, const QString& was, const QString& now) {
      if (now != was) {
        command.insert(QLatin1String(key), now);
      }
    };
    if (m_column->isChecked()) {
      const QString reference = m_columnChoice->currentData().toString();
      if (reference.isEmpty()) {
        m_message->setText(tr("Choose a column of the design's tables."));
        return;
      }
      if (reference != m_columnWas) {
        command.insert(QStringLiteral("values"), reference);
      }
      if (!m_text) {
        setLimit("min", m_min, QString());
        setLimit("max", m_max, QString());
        setLimit("step", m_step, QString());
      }
      described = QStringLiteral("column %1").arg(reference);
    } else if (m_list->isChecked()) {
      QJsonArray values;
      QStringList texts;
      for (const QString& line : m_lines->toPlainText().split(QLatin1Char('\n'))) {
        const QString expression = cfg::rowExpression(m_text, m_unit, line);
        if (!expression.isEmpty()) {
          values.append(expression);
          texts << expression;
        }
      }
      if (values.isEmpty()) {
        m_message->setText(tr("A list needs at least one value."));
        return;
      }
      command.insert(QStringLiteral("values"), values);
      if (!m_text) {
        setLimit("min", m_min, QString());
        setLimit("max", m_max, QString());
        setLimit("step", m_step, QString());
      }
      described = QStringLiteral("list %1").arg(texts.join(QStringLiteral(", ")));
    } else {
      if (!m_values.isEmpty() || !m_columnWas.isEmpty()) {
        command.insert(QStringLiteral("values"), QJsonValue());
      }
      const bool range = m_range->isChecked();
      const auto bound = [this, range](const QLineEdit* field) {
        return range ? cfg::rowExpression(false, m_unit, field->text()) : QString();
      };
      if (!m_text) {
        setLimit("min", m_min, bound(m_minField));
        setLimit("max", m_max, bound(m_maxField));
        setLimit("step", m_step, bound(m_stepField));
      }
      described = range ? QStringLiteral("range %1 … %2, step %3")
                              .arg(bound(m_minField), bound(m_maxField), bound(m_stepField))
                        : QStringLiteral("free");
    }
    if (command.size() == 2) {
      accept(); // nothing changed
      return;
    }
    if (!m_host.canChangeModel()) {
      m_message->setText(m_host.changeBlockedReason());
      return;
    }
    if (m_host.runModelCommand(command)) {
      qDebug().noquote() << QStringLiteral("Option %1 domain: %2").arg(m_name, described);
      accept();
    } else if (!m_host.lastCancelled()) {
      m_message->setText(m_host.lastError());
      qDebug().noquote() << QStringLiteral("Option %1 domain refused: %2").arg(m_name, m_host.lastError());
    }
  }

  DocumentHost& m_host;
  QString m_name;
  bool m_text = false;
  QString m_unit;
  QStringList m_values;
  QString m_columnWas; // `Table.Column` when the values are a column
  QString m_min;
  QString m_max;
  QString m_step;
  QRadioButton* m_list = nullptr;
  QRadioButton* m_range = nullptr;
  QRadioButton* m_column = nullptr;
  QRadioButton* m_free = nullptr;
  QComboBox* m_columnChoice = nullptr;
  QPlainTextEdit* m_lines = nullptr;
  QLineEdit* m_minField = nullptr;
  QLineEdit* m_maxField = nullptr;
  QLineEdit* m_stepField = nullptr;
  QLabel* m_message = nullptr;
};

// Generate Rows from Table (mitcad#806, #849): one row per distinct key of
// a table's column, named by the key, that sets an option to it
// (`generate_configuration_rows`; rows of the same name are updated, not
// duplicated; one undo step). The table, its key column and the option are
// chosen; the dialog offers first the column an option's values come from
// and that option.
class GenerateRowsDialog : public QDialog {
public:
  GenerateRowsDialog(DocumentHost& host, const std::vector<SuggestedTable>& tables, const cfg::Table& table,
                     const QStringList& userParameters, const QHash<QString, QJsonObject>& parameters,
                     QWidget* parent)
      : QDialog(parent), m_host(host), m_tables(tables), m_options(table.options),
        m_userParameters(userParameters), m_parameters(parameters) {
    setObjectName(QStringLiteral("generateRowsDialog"));
    setWindowTitle(tr("Generate Rows from Table"));
    auto* form = new QFormLayout(this);
    auto* about = new QLabel(tr("One row per value of the key column, named by it, that sets the option to it; "
                                "each checked column sets the parameter of its name in every row. "
                                "Rows of the same name are updated."));
    about->setWordWrap(true);
    form->addRow(about);
    m_tableChoice = new QComboBox;
    m_tableChoice->setObjectName(QStringLiteral("generateRowsTable"));
    m_columnChoice = new QComboBox;
    m_columnChoice->setObjectName(QStringLiteral("generateRowsColumn"));
    m_optionChoice = new QComboBox;
    m_optionChoice->setObjectName(QStringLiteral("generateRowsOption"));
    m_optionChoice->setToolTip(tr("An option, or a user parameter that becomes one"));
    // The options first, then the user parameters that can become one (a
    // closed table's other parameters cannot).
    for (const QString& option : std::as_const(m_options)) {
      m_optionChoice->addItem(option, option);
    }
    for (const QString& name : std::as_const(m_userParameters)) {
      if (!m_options.contains(name) && !table.parameters.contains(name)) {
        m_optionChoice->addItem(name, name);
      }
    }
    // The table an option's values come from, else the first.
    int first = -1;
    for (const SuggestedTable& candidate : m_tables) {
      if (first < 0 && !keyColumn(candidate).isEmpty()) {
        first = m_tableChoice->count();
      }
      m_tableChoice->addItem(candidate.name, candidate.name);
    }
    form->addRow(tr("Table:"), m_tableChoice);
    form->addRow(tr("Key column:"), m_columnChoice);
    form->addRow(tr("Option:"), m_optionChoice);
    // The table's other columns (mitcad#1044): each checked one becomes a
    // parameter column of the closed table.
    m_columnsLabel = new QLabel(tr("&Columns:"));
    m_columnsBox = new QWidget;
    m_columnsBox->setObjectName(QStringLiteral("generateRowsColumns"));
    // Focusable for the label's Alt+C, its focus its first check's
    // (fillChecks); a widget with a focus proxy is no stop of Tab.
    m_columnsBox->setFocusPolicy(Qt::StrongFocus);
    m_columnsLayout = new QVBoxLayout(m_columnsBox);
    m_columnsLayout->setContentsMargins(0, 0, 0, 0);
    form->addRow(m_columnsLabel, m_columnsBox);
    m_message = new QLabel;
    m_message->setWordWrap(true);
    setErrorStyleSheet(m_message);
    form->addRow(m_message);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
    ok->setObjectName(QStringLiteral("generateRowsOK"));
    ok->setText(tr("Generate"));
    ok->setDefault(true);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { tryAccept(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    m_tableChoice->setCurrentIndex(std::max(0, first));
    fillColumns();
    connect(m_tableChoice, &QComboBox::currentIndexChanged, this, [this] { fillColumns(); });
    connect(m_columnChoice, &QComboBox::currentIndexChanged, this, [this] {
      chooseOption();
      fillChecks();
    });
    qDebug().noquote() << QStringLiteral("Generate Rows dialog opened: %1 for %2")
                              .arg(cfg::columnReference(m_tableChoice->currentData().toString(),
                                                        m_columnChoice->currentData().toString()),
                                   m_optionChoice->currentData().toString());
    logChecks();
    m_tableChoice->setFocus();
  }

  bool generated() const { return m_done; }
  const cfg::GeneratedRows& rows() const { return m_rows; }
  QString generatedTable() const { return m_tableName; }
  QString generatedReference() const { return m_reference; }
  QString generatedOption() const { return m_option; }

private:
  // The column of the table an option's values come from; empty for none.
  QString keyColumn(const SuggestedTable& table) const {
    for (const QString& option : m_options) {
      const QString reference = cfg::valuesColumn(m_parameters.value(option));
      for (const SuggestedTable::Column& column : table.columns) {
        if (reference == cfg::columnReference(table.name, column.name)) {
          return column.name;
        }
      }
    }
    return {};
  }

  void fillColumns() {
    const QSignalBlocker blocker(m_columnChoice);
    m_columnChoice->clear();
    const SuggestedTable* table = tableNamed(m_tables, m_tableChoice->currentData().toString());
    if (table != nullptr) {
      for (const SuggestedTable::Column& column : table->columns) {
        m_columnChoice->addItem(columnLabel(column), column.name);
      }
      // The column an option's values come from, else the first (a table's
      // key is its first column, as lookups read it).
      m_columnChoice->setCurrentIndex(std::max(0, m_columnChoice->findData(keyColumn(*table))));
    }
    chooseOption();
    fillChecks();
  }

  // A check per column of the table but the key, all checked.
  void fillChecks() {
    for (QCheckBox* check : std::as_const(m_checks)) {
      m_columnsLayout->removeWidget(check);
      check->hide();
      check->deleteLater();
    }
    m_checks.clear();
    const QString key = m_columnChoice->currentData().toString();
    if (const SuggestedTable* table = tableNamed(m_tables, m_tableChoice->currentData().toString())) {
      for (const SuggestedTable::Column& column : table->columns) {
        if (column.name == key) {
          continue;
        }
        auto* check = new QCheckBox(columnLabel(column));
        check->setObjectName(QStringLiteral("generateRowsColumn_") + column.name);
        check->setProperty("column", column.name);
        check->setChecked(true);
        check->setToolTip(tr("Every row sets the parameter %1 from this column").arg(column.name));
        connect(check, &QCheckBox::toggled, this, [this] { logChecks(); });
        m_columnsLayout->addWidget(check);
        m_checks << check;
      }
    }
    // Alt+C goes to the first check without toggling it: a label whose
    // buddy is a button clicks it, so the buddy is the box, whose focus
    // goes to its first check.
    m_columnsBox->setFocusProxy(m_checks.isEmpty() ? nullptr : m_checks.first());
    m_columnsLabel->setBuddy(m_checks.isEmpty() ? nullptr : m_columnsBox);
    m_columnsLabel->setEnabled(!m_checks.isEmpty());
    m_columnsBox->setVisible(!m_checks.isEmpty());
  }

  QStringList checkedColumns() const {
    QStringList out;
    for (const QCheckBox* check : m_checks) {
      if (check->isChecked()) {
        out << check->property("column").toString();
      }
    }
    return out;
  }

  void logChecks() const {
    QStringList all;
    for (const QCheckBox* check : m_checks) {
      all << check->property("column").toString();
    }
    const QStringList checked = checkedColumns();
    qDebug().noquote() << QStringLiteral("Generate Rows columns: %1; checked: %2")
                              .arg(all.isEmpty() ? QStringLiteral("none") : all.join(QStringLiteral(", ")),
                                   checked.isEmpty() ? QStringLiteral("none") : checked.join(QStringLiteral(", ")));
  }

  void chooseOption() {
    const QString option =
        cfg::keyOption(m_tableChoice->currentData().toString(), m_columnChoice->currentData().toString(), m_options,
                       m_userParameters, m_parameters);
    const int index = m_optionChoice->findData(option);
    if (index >= 0) {
      m_optionChoice->setCurrentIndex(index);
    }
  }

  void tryAccept() {
    const QString table = m_tableChoice->currentData().toString();
    const QString column = m_columnChoice->currentData().toString();
    const QString option = m_optionChoice->currentData().toString();
    if (table.isEmpty() || column.isEmpty() || option.isEmpty()) {
      m_message->setText(tr("Choose a table, its key column and an option."));
      return;
    }
    if (const QString readOnly = m_host.readOnlyReason(); !readOnly.isEmpty()) {
      m_message->setText(readOnly);
      return;
    }
    if (!m_host.canChangeModel()) {
      m_message->setText(m_host.changeBlockedReason());
      return;
    }
    QJsonObject result;
    if (m_host.runModelCommand(cfg::generateRowsCommand(table, column, option, checkedColumns()), &result)) {
      m_rows = cfg::readGeneratedRows(result);
      m_tableName = table;
      m_reference = cfg::columnReference(table, column);
      m_option = option;
      m_done = true;
      accept();
    } else if (!m_host.lastCancelled()) {
      m_message->setText(m_host.lastError());
      qDebug().noquote() << QStringLiteral("Configurations generate rows from %1 for %2 refused: %3")
                                .arg(cfg::columnReference(table, column), option, m_host.lastError());
    }
  }

  DocumentHost& m_host;
  std::vector<SuggestedTable> m_tables;
  QStringList m_options;
  QStringList m_userParameters;
  QHash<QString, QJsonObject> m_parameters;
  QComboBox* m_tableChoice = nullptr;
  QComboBox* m_columnChoice = nullptr;
  QComboBox* m_optionChoice = nullptr;
  QLabel* m_columnsLabel = nullptr;
  QWidget* m_columnsBox = nullptr;
  QVBoxLayout* m_columnsLayout = nullptr;
  QVector<QCheckBox*> m_checks;
  QLabel* m_message = nullptr;
  bool m_done = false;
  cfg::GeneratedRows m_rows;
  QString m_tableName;
  QString m_reference;
  QString m_option;
};

// A cell marked at a level (mitcad#1051, docs/ui-conventions.md "Messages
// and severity"): the level's colour, its highlight box's fill and its
// badge, and why in the tooltip. A change the design refused is an error
// (its word "Refused"), a row that breaks a rule a warning.
void markLevel(QTableWidgetItem* item, NoticeLevel level, const QString& tooltip, const QPalette& palette) {
  item->setForeground(noticeColor(level, palette));
  item->setBackground(noticeFill(level, palette));
  item->setIcon(noticeIcon(level));
  item->setToolTip(tooltip);
}

void markLevel(QTreeWidgetItem* item, int column, NoticeLevel level, const QString& tooltip,
               const QPalette& palette) {
  item->setForeground(column, noticeColor(level, palette));
  item->setBackground(column, noticeFill(level, palette));
  item->setIcon(column, noticeIcon(level));
  item->setToolTip(column, tooltip);
}

QString refusedText(const QString& reason) { return QObject::tr("Refused: %1").arg(reason); }

void markRefused(QTableWidgetItem* item, const QString& typed, const QString& reason, const QPalette& palette) {
  item->setText(typed);
  markLevel(item, NoticeLevel::Error, refusedText(reason), palette);
}


// Import Rows (mitcad#1044): what importing a rows CSV would do, as
// `configuration_rows_csv_preview` says: what each column of the file is
// (the key option, an option, a parameter column, new or not, or a field
// of the rows), the rows, refused cells in red, the rows added, updated
// and kept, the parameters made, and what the table would have wrong,
// which refuses the import. Import runs `import_configuration_rows_csv`;
// Cancel changes nothing.
class RowsImportDialog : public QDialog {
public:
  RowsImportDialog(DocumentHost& host, const QString& csv, QString file, QWidget* parent)
      : QDialog(parent), m_file(std::move(file)) {
    setObjectName(QStringLiteral("rowsImportPreview"));
    setWindowTitle(tr("Import Rows"));
    resize(760, 440);
    auto* layout = new QVBoxLayout(this);
    m_summary = new QLabel;
    m_summary->setObjectName(QStringLiteral("rowsImportSummary"));
    m_summary->setWordWrap(true);
    m_summary->setTextFormat(Qt::PlainText);
    layout->addWidget(m_summary);
    m_cells = new QTableWidget;
    m_cells->setObjectName(QStringLiteral("rowsImportRows"));
    m_cells->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_cells->setSelectionMode(QAbstractItemView::NoSelection);
    m_cells->setAlternatingRowColors(true);
    layout->addWidget(m_cells, 1);
    m_problem = new QLabel;
    m_problem->setObjectName(QStringLiteral("rowsImportProblem"));
    m_problem->setWordWrap(true);
    m_problem->setTextFormat(Qt::PlainText);
    setErrorStyleSheet(m_problem);
    layout->addWidget(m_problem);
    auto* buttons = new QDialogButtonBox;
    m_import = new QPushButton(tr("Import"));
    m_import->setObjectName(QStringLiteral("rowsImportAccept"));
    auto* cancel = new QPushButton(tr("Cancel"));
    cancel->setObjectName(QStringLiteral("rowsImportCancel"));
    buttons->addButton(m_import, QDialogButtonBox::AcceptRole);
    buttons->addButton(cancel, QDialogButtonBox::RejectRole);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    preview(host, csv);
  }

private:
  static QString roleText(const QString& role, bool made) {
    if (role == QStringLiteral("key")) {
      return made ? tr("key option, a new parameter") : tr("key option");
    }
    if (role == QStringLiteral("option")) {
      return tr("option");
    }
    if (role == QStringLiteral("parameter")) {
      return made ? tr("parameter column, a new parameter") : tr("parameter column");
    }
    if (role == QStringLiteral("name")) {
      return tr("row name");
    }
    if (role == QStringLiteral("part_number")) {
      return tr("part number");
    }
    if (role == QStringLiteral("supplier_url")) {
      return tr("supplier URL");
    }
    return role; // designation
  }

  void preview(DocumentHost& host, const QString& csv) {
    QJsonObject answer;
    QString problem;
    try {
      answer = host.model().queryObject(cfg::rowsCsvPreviewQuery(csv));
    } catch (const std::exception& e) {
      problem = QString::fromUtf8(e.what());
    }
    if (!problem.isEmpty()) {
      m_summary->setText(tr("%1: %2").arg(m_file, problem));
      m_problem->hide();
      m_import->setEnabled(false);
      qDebug().noquote() << QStringLiteral("Rows import preview of %1 refused: %2").arg(m_file, problem);
      return;
    }
    const QString summary = answer.value(QStringLiteral("summary")).toString();
    QString closes;
    if (answer.value(QStringLiteral("closed")).toBool()) {
      closes = tr(" Only these rows is on after the import: only the combinations listed in the Rows tab are "
                  "allowed.");
    }
    m_summary->setText(tr("%1: %2.%3").arg(m_file, summary, closes));
    qDebug().noquote() << QStringLiteral("Rows import preview of %1: %2").arg(m_file, summary);

    QStringList created;
    for (const QJsonValue& name : answer.value(QStringLiteral("created")).toArray()) {
      created << name.toString();
    }
    QStringList labels;
    QStringList names; // the refused cells' columns
    for (const QJsonValue& value : answer.value(QStringLiteral("columns")).toArray()) {
      const QJsonObject column = value.toObject();
      const QString renamed = column.value(QStringLiteral("renamed")).toString();
      const QString name = renamed.isEmpty() ? column.value(QStringLiteral("name")).toString() : renamed;
      const QString unit = column.value(QStringLiteral("unit")).toString();
      const QString role = roleText(column.value(QStringLiteral("role")).toString(), created.contains(name));
      labels << QStringLiteral("%1 [%2]\n(%3)").arg(name, unit, role);
      names << name;
      qDebug().noquote() << QStringLiteral("Rows import column %1 [%2]: %3").arg(name, unit, role);
    }
    const QJsonArray rows = answer.value(QStringLiteral("rows")).toArray();
    m_cells->setColumnCount(static_cast<int>(labels.size()));
    m_cells->setHorizontalHeaderLabels(labels);
    m_cells->setRowCount(static_cast<int>(rows.size()));
    for (int r = 0; r < static_cast<int>(rows.size()); ++r) {
      const QJsonArray cells = rows.at(r).toArray();
      for (int c = 0; c < static_cast<int>(cells.size()) && c < static_cast<int>(labels.size()); ++c) {
        const QJsonValue cell = cells.at(c);
        m_cells->setItem(r, c, new QTableWidgetItem(cell.isDouble() ? QString::number(cell.toDouble(), 'g', 12)
                                                                    : cell.toString()));
      }
    }
    qDebug().noquote() << QStringLiteral("Rows import rows: %1")
                              .arg(QString::fromUtf8(QJsonDocument(rows).toJson(QJsonDocument::Compact)));
    QStringList refusing;
    for (const QJsonValue& value : answer.value(QStringLiteral("refused")).toArray()) {
      const QJsonObject cell = value.toObject();
      const int line = cell.value(QStringLiteral("line")).toInt();
      const QString columnName = cell.value(QStringLiteral("column")).toString();
      const QString reason = cell.value(QStringLiteral("reason")).toString();
      refusing << tr("Line %1, %2: %3").arg(line).arg(columnName, reason);
      qDebug().noquote() << QStringLiteral("Rows import refused line %1 %2: %3").arg(line).arg(columnName, reason);
      const int r = line - 2; // the header is line 1
      const int c = static_cast<int>(names.indexOf(columnName));
      if (r >= 0 && r < m_cells->rowCount() && c >= 0) {
        QTableWidgetItem* item = m_cells->item(r, c);
        if (item == nullptr) {
          item = new QTableWidgetItem;
          m_cells->setItem(r, c, item);
        }
        item->setText(cell.value(QStringLiteral("text")).toString());
        markLevel(item, NoticeLevel::Error, refusedText(reason), m_cells->palette());
      }
    }
    const QString tableProblem = answer.value(QStringLiteral("problem")).toString();
    if (!tableProblem.isEmpty()) {
      refusing << tableProblem;
      qDebug().noquote() << QStringLiteral("Rows import problem: %1").arg(tableProblem);
    }
    m_problem->setText(refusing.isEmpty() ? QString()
                                          : refusing.join(QLatin1Char('\n')) + QLatin1Char('\n') +
                                                tr("Correct the file or the table to import it."));
    m_problem->setVisible(!refusing.isEmpty());
    const bool unchanged = answer.value(QStringLiteral("unchanged")).toBool();
    const bool importable = refusing.isEmpty() && !unchanged;
    m_import->setEnabled(importable);
    m_import->setDefault(importable);
    m_import->setToolTip(unchanged ? tr("The rows are already as the file has them") : QString());
  }

  QString m_file;
  QLabel* m_summary = nullptr;
  QTableWidget* m_cells = nullptr;
  QLabel* m_problem = nullptr;
  QPushButton* m_import = nullptr;
};
} // namespace

ConfigurationsDialog::ConfigurationsDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
  setObjectName(QStringLiteral("configurationsDialog"));
  setWindowTitle(tr("Configurations"));
  setWindowIcon(themeIcon(QStringLiteral("configurations")));
  resize(900, 460);
  auto* layout = new QVBoxLayout(this);
  m_tabs = new QTabWidget;
  m_tabs->setObjectName(QStringLiteral("configurationsTabs"));
  m_tabs->addTab(makeOptionsPage(), tr("Options"));
  m_tabs->addTab(makeRowsPage(), tr("Rows"));
  m_rules = makeConfigurationRulesPage(m_host, m_tabs);
  m_tabs->addTab(m_rules, tr("Rules"));
  // Every binding of the design (mitcad#862).
  m_bindings = new ConfigurationBindingsPage(m_host, m_tabs);
  m_tabs->addTab(m_bindings, tr("Bindings"));
  layout->addWidget(m_tabs, 1);

  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("configurationsMessage"));
  m_message->setWordWrap(true);
  layout->addWidget(m_message);
  auto* buttons = new QDialogButtonBox;
  QPushButton* closeButton = buttons->addButton(QDialogButtonBox::Close);
  closeButton->setObjectName(QStringLiteral("configurationsClose"));
  closeButton->setAutoDefault(false);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);

  connect(m_tabs, &QTabWidget::currentChanged, this, [this](int page) {
    qDebug().noquote() << QStringLiteral("Configurations page %1").arg(m_tabs->tabText(page));
    QTimer::singleShot(0, this, &ConfigurationsDialog::logLayout);
  });
  // The cells' places are logged again when the window moves or resizes.
  new PlaceWatcher(this, [this] { logLayout(); });
  refresh();
}

QWidget* ConfigurationsDialog::makeOptionsPage() {
  auto* page = new QWidget;
  auto* layout = new QVBoxLayout(page);
  layout->addWidget(leadLine(kOptionsLead, QStringLiteral("Options")));
  auto* tree = new OptionTree([this] { optionsDropped(); });
  m_options = tree;
  m_options->setObjectName(QStringLiteral("configurationsOptions"));
  panel::setCompactIndentation(m_options, panel::Indent::Flat);
  m_options->setColumnCount(7);
  // The approved words (mitcad#968, docs/ui-conventions.md
  // "Configurations"): Possible Values before restrictions, the current
  // Value (not a stored default), By Formula.
  m_options->setHeaderLabels({tr("Option"), tr("Unit"), tr("Possible Values"), tr("Value"), tr("Comment"),
                              tr("By Formula"), tr("Used By")});
  m_options->headerItem()->setToolTip(kOptionUsedBy, tr("The bindings that follow the option; double-click to "
                                                        "list them in the Bindings tab"));
  m_options->headerItem()->setToolTip(kOptionDomain,
                                      tr("The values before rules and rows restrict them: a list, a range, a "
                                         "table column or free"));
  m_options->headerItem()->setToolTip(
      kOptionValue, tr("The current value; for a By Formula option, the value its expression gives"));
  m_options->headerItem()->setToolTip(kOptionFormula,
                                      tr("The value is calculated from its expression: neither a chooser nor "
                                         "automatic adjustment sets it"));
  QStringList headers;
  for (int column = 0; column < m_options->columnCount(); ++column) {
    headers << m_options->headerItem()->text(column);
  }
  qDebug().noquote() << QStringLiteral("Configurations options header: %1").arg(headers.join(QStringLiteral(" | ")));
  m_options->setRootIsDecorated(false);
  m_options->setUniformRowHeights(true);
  m_options->setAlternatingRowColors(true);
  m_options->setSelectionMode(QAbstractItemView::SingleSelection);
  m_options->setEditTriggers(QAbstractItemView::NoEditTriggers);
  // The choice order: rows dragged into place.
  m_options->setDragDropMode(QAbstractItemView::InternalMove);
  m_options->setDefaultDropAction(Qt::MoveAction);
  m_options->setDropIndicatorShown(true);
  m_options->header()->resizeSection(kOptionName, 140);
  m_options->header()->resizeSection(kOptionUnit, 60);
  m_options->header()->resizeSection(kOptionDomain, 230);
  m_options->header()->resizeSection(kOptionValue, 120);
  m_options->header()->setSectionResizeMode(kOptionComment, QHeaderView::Stretch);
  m_options->header()->setStretchLastSection(false);
  // As wide as its header, By Formula.
  m_options->header()->setSectionResizeMode(kOptionFormula, QHeaderView::ResizeToContents);
  // As wide as its widest `used by N bindings` (mitcad#862 review: 130 px
  // cut it with larger fonts).
  m_options->header()->setSectionResizeMode(kOptionUsedBy, QHeaderView::ResizeToContents);
  layout->addWidget(m_options, 1);
  auto* buttons = new QHBoxLayout;
  m_addOption = new QPushButton(themeIcon(QStringLiteral("add")), tr("Add Option…"));
  m_addOption->setObjectName(QStringLiteral("configurationsAddOption"));
  m_addOption->setToolTip(tr("Make a user parameter an option, or a new parameter"));
  m_removeOption = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Remove Option"));
  m_removeOption->setObjectName(QStringLiteral("configurationsRemoveOption"));
  m_removeOption->setToolTip(tr("The parameter stays; the rows no longer set it"));
  m_editDomain = new QPushButton(tr("Edit Possible Values…"));
  m_editDomain->setObjectName(QStringLiteral("configurationsEditDomain"));
  m_editDomain->setToolTip(tr("The selected option's values before rules and rows restrict them"));
  m_up = new QPushButton(tr("Move Up"));
  m_up->setObjectName(QStringLiteral("configurationsMoveUp"));
  m_down = new QPushButton(tr("Move Down"));
  m_down->setObjectName(QStringLiteral("configurationsMoveDown"));
  for (QPushButton* button : {m_addOption, m_removeOption, m_editDomain, m_up, m_down}) {
    button->setAutoDefault(false);
    buttons->addWidget(button);
  }
  buttons->addStretch(1);
  layout->addLayout(buttons);

  connect(m_addOption, &QPushButton::clicked, this, [this] { later([this] { addOption(); }); });
  connect(m_removeOption, &QPushButton::clicked, this, [this] { later([this] { removeOption(); }); });
  connect(m_editDomain, &QPushButton::clicked, this, [this] { later([this] { editDomain(); }); });
  connect(m_up, &QPushButton::clicked, this, [this] { later([this] { moveOption(-1); }); });
  connect(m_down, &QPushButton::clicked, this, [this] { later([this] { moveOption(1); }); });
  connect(m_options, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item, int column) {
    if (item == nullptr) {
      return;
    }
    if (column == kOptionDomain) {
      later([this] { editDomain(); });
    } else if (column == kOptionUsedBy) {
      // The bindings the option drives (mitcad#862).
      showBindings(item->data(kOptionName, kNameRole).toString());
    } else if ((column == kOptionValue && item->text(kOptionFormula).isEmpty()) || column == kOptionComment) {
      // A By Formula option's Value is its evaluated value: not edited here.
      m_options->editItem(item, column);
      qDebug().noquote() << QStringLiteral("Editing option %1 %2")
                                .arg(item->data(kOptionName, kNameRole).toString(),
                                     m_options->headerItem()->text(column).toLower());
    }
  });
  connect(m_options, &QTreeWidget::itemChanged, this, &ConfigurationsDialog::optionEdited);
  connect(m_options, &QTreeWidget::itemSelectionChanged, this, [this] {
    const bool selected = !selectedOption().isEmpty();
    if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
      const auto* current = m_options->currentItem();
      qDebug().noquote() << QStringLiteral("Configurations option selection: current=%1 selected=%2 domain=%3")
          .arg(current != nullptr ? current->data(kOptionName, kNameRole).toString() : QStringLiteral("none"))
          .arg(current != nullptr && current->isSelected()).arg(selected);
    }
    for (QPushButton* button : {m_removeOption, m_editDomain, m_up, m_down}) {
      button->setEnabled(selected);
    }
  });
  return page;
}

QWidget* ConfigurationsDialog::makeRowsPage() {
  auto* page = new QWidget;
  auto* layout = new QVBoxLayout(page);
  layout->addWidget(leadLine(kRowsLead, QStringLiteral("Rows")));
  auto* top = new QHBoxLayout;
  // The table's `closed` (mitcad#968: named by what it does). Unchecked,
  // a combination must still keep to the options' possible values and the
  // rules.
  m_closed = new QCheckBox(tr("Only these rows"));
  m_closed->setObjectName(QStringLiteral("configurationsClosed"));
  m_closed->setToolTip(tr("Allow only the combinations listed in the Rows tab"));
  top->addWidget(m_closed);
  m_current = new QLabel;
  m_current->setObjectName(QStringLiteral("configurationsCurrentRow"));
  top->addWidget(m_current, 1);
  layout->addLayout(top);
  m_rows = new RowsSheet([this] { copyCells(); }, [this] { later([this] { pasteCells(); }); },
                         [this] { later([this] { clearCells(); }); });
  m_rows->setObjectName(QStringLiteral("configurationsRows"));
  // Ranges of cells, as a spreadsheet's (mitcad#1044): Copy, Paste, Delete.
  m_rows->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_rows->setSelectionBehavior(QAbstractItemView::SelectItems);
  m_rows->setEditTriggers(QAbstractItemView::DoubleClicked | QAbstractItemView::EditKeyPressed);
  m_rows->setAlternatingRowColors(true);
  m_rows->verticalHeader()->hide();
  m_rows->horizontalHeader()->setStretchLastSection(true);
  layout->addWidget(m_rows, 1);
  auto* buttons = new QHBoxLayout;
  m_addRow = new QPushButton(themeIcon(QStringLiteral("add")), tr("Add Row"));
  m_addRow->setObjectName(QStringLiteral("configurationsAddRow"));
  m_addRow->setToolTip(tr("A new row with the options' values as they are now"));
  m_generateRows = new QPushButton(tr("Generate Rows from Table…"));
  m_generateRows->setObjectName(QStringLiteral("configurationsGenerateRows"));
  m_generateRows->setToolTip(tr("A row per value of a table's key column, setting an option to it"));
  m_deleteRow = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Delete Row"));
  m_deleteRow->setObjectName(QStringLiteral("configurationsDeleteRow"));
  m_defaultRow = new QPushButton(tr("Set as Default"));
  m_defaultRow->setObjectName(QStringLiteral("configurationsDefaultRow"));
  // The default row: the user guide says what else it is for.
  m_defaultRow->setToolTip(tr("The row a library component is inserted with when no other row is chosen"));
  m_apply = new QPushButton(tr("Apply"));
  m_apply->setObjectName(QStringLiteral("configurationsApply"));
  m_apply->setToolTip(tr("Set the options to the selected row's values"));
  // The parameter columns of a table with Only these rows (mitcad#999).
  m_addColumn = new QPushButton(themeIcon(QStringLiteral("add")), tr("Add Column…"));
  m_addColumn->setObjectName(QStringLiteral("configurationsAddColumn"));
  m_removeColumn = new QPushButton(themeIcon(QStringLiteral("delete")), tr("Remove Column"));
  m_removeColumn->setObjectName(QStringLiteral("configurationsRemoveColumn"));
  for (QPushButton* button : {m_addRow, m_generateRows, m_deleteRow, m_defaultRow, m_addColumn, m_removeColumn}) {
    button->setAutoDefault(false);
    buttons->addWidget(button);
  }
  buttons->addStretch(1);
  m_apply->setAutoDefault(false);
  buttons->addWidget(m_apply);
  layout->addLayout(buttons);
  // The rows as CSV (mitcad#1044): a row per size, a column per parameter.
  auto* files = new QHBoxLayout;
  m_importRows = new QPushButton(tr("&Import CSV…"));
  m_importRows->setObjectName(QStringLiteral("configurationsImportRows"));
  m_importRows->setToolTip(tr("Fill the rows from a CSV file, a row per line and a column per parameter"));
  m_exportRows = new QPushButton(tr("E&xport CSV…"));
  m_exportRows->setObjectName(QStringLiteral("configurationsExportRows"));
  m_exportRows->setToolTip(tr("Write the rows to a CSV file"));
  for (QPushButton* button : {m_importRows, m_exportRows}) {
    button->setAutoDefault(false);
    files->addWidget(button);
  }
  files->addStretch(1);
  layout->addLayout(files);

  connect(m_closed, &QCheckBox::toggled, this, [this](bool on) {
    if (m_building) {
      return;
    }
    later([this, on] {
      cfg::Table table = m_table;
      table.closed = on;
      submitTable(table, QStringLiteral("closed"), on ? QStringLiteral("on") : QStringLiteral("off"),
                  QStringLiteral("closed"));
    });
  });
  connect(m_addRow, &QPushButton::clicked, this, [this] { later([this] { addRow(); }); });
  connect(m_generateRows, &QPushButton::clicked, this, [this] { later([this] { generateRows(); }); });
  connect(m_deleteRow, &QPushButton::clicked, this, [this] { later([this] { deleteRow(); }); });
  connect(m_defaultRow, &QPushButton::clicked, this, [this] { later([this] { setDefaultRow(); }); });
  connect(m_apply, &QPushButton::clicked, this, [this] { later([this] { applyRow(); }); });
  connect(m_addColumn, &QPushButton::clicked, this, [this] { later([this] { addColumn(); }); });
  connect(m_removeColumn, &QPushButton::clicked, this, [this] { later([this] { removeColumn(); }); });
  connect(m_importRows, &QPushButton::clicked, this, [this] { later([this] { importRows(); }); });
  connect(m_exportRows, &QPushButton::clicked, this, [this] { later([this] { exportRows(); }); });
  connect(m_rows, &QTableWidget::itemChanged, this, &ConfigurationsDialog::rowEdited);
  connect(m_rows, &QTableWidget::itemSelectionChanged, this, [this] {
    const bool selected = !selectedRow().isEmpty();
    for (QPushButton* button : {m_deleteRow, m_defaultRow, m_apply}) {
      button->setEnabled(selected);
    }
  });
  // Remove Column takes the current cell's column: enabled on a parameter
  // column's cell.
  connect(m_rows, &QTableWidget::currentCellChanged, this,
          [this] { m_removeColumn->setEnabled(!selectedColumn().isEmpty()); });
  return page;
}

void ConfigurationsDialog::later(std::function<void()> call) {
  QTimer::singleShot(0, this, [this, call = std::move(call)] {
    if (m_host.modelBusy()) {
      m_host.whenIdle(this, call);
    } else {
      call();
    }
  });
}

bool ConfigurationsDialog::isText(const QString& parameter) const {
  if (const cfg::OptionReport* report = m_report.option(parameter)) {
    return report->text;
  }
  return isTextUnit(unitOf(parameter));
}

QString ConfigurationsDialog::unitOf(const QString& parameter) const {
  return m_parameters.value(parameter).value(QStringLiteral("unit")).toString();
}

void ConfigurationsDialog::showMessage(const QString& text, bool error) {
  m_message->setText(text);
  if (error) {
    setErrorStyleSheet(m_message);
  } else {
    m_message->setStyleSheet(QString());
  }
}

QString ConfigurationsDialog::selectedOption() const {
  const QTreeWidgetItem* item = m_options->currentItem();
  return item != nullptr && item->isSelected() ? item->data(kOptionName, kNameRole).toString() : QString();
}

QString ConfigurationsDialog::selectedRow() const {
  // The current cell's row; with ranges of cells selected, the first.
  if (const QTableWidgetItem* current = m_rows->currentItem(); current != nullptr && current->isSelected()) {
    return current->data(kNameRole).toString();
  }
  const QList<QTableWidgetItem*> items = m_rows->selectedItems();
  return items.isEmpty() ? QString() : items.first()->data(kNameRole).toString();
}

QString ConfigurationsDialog::selectedColumn() const {
  const QVector<RowColumn> columns = rowColumns();
  const int c = m_rows->currentColumn();
  if (m_rows->currentRow() < 0 || c < 0 || c >= columns.size() || columns.at(c).kind != RowColumn::Parameter) {
    return {};
  }
  return columns.at(c).parameter;
}

// ---------------------------------------------------------------------------
// Reading the model

void ConfigurationsDialog::refresh() {
  const CommandContext& model = m_host.model();
  QStringList problems;
  try {
    const QJsonObject answer = model.queryObject(cfg::configurationsQuery());
    m_table = cfg::readTable(answer);
    for (const QJsonValue& problem : answer.value(QStringLiteral("problems")).toArray()) {
      problems << problem.toString();
    }
    m_parameters.clear();
    m_userParameters.clear();
    for (const QJsonValue& value : model.queryArray(QStringLiteral("parameters"))) {
      const QJsonObject parameter = value.toObject();
      const QString name = parameter.value(QStringLiteral("name")).toString();
      if (parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("user")) {
        m_parameters.insert(name, parameter);
        m_userParameters << name;
      }
    }
    // The design's tables, for a column as a domain and generated rows.
    m_tables = suggestedTables(model.queryObject({{QStringLiteral("query"), QStringLiteral("formula_help")}})
                                   .value(QStringLiteral("tables"))
                                   .toArray());
  } catch (const std::exception& e) {
    problems << QString::fromUtf8(e.what());
  }
  // The options' reports; none from a model without options (an empty
  // table) or one that cannot answer.
  m_report = cfg::Report();
  if (!m_table.options.isEmpty()) {
    try {
      m_report = cfg::readReport(model.queryObject(cfg::configureQuery()));
    } catch (const std::exception& e) {
      problems << QString::fromUtf8(e.what());
    }
  }
  checkRows();
  // The bindings first: the Options tab counts their users (mitcad#862).
  if (m_bindings != nullptr) {
    m_bindings->refresh();
  }
  m_building = true;
  fillOptions();
  fillRows();
  m_building = false;
  if (m_rules != nullptr) {
    m_rules->refresh();
  }
  const bool option = !selectedOption().isEmpty();
  for (QPushButton* button : {m_removeOption, m_editDomain, m_up, m_down}) {
    button->setEnabled(option);
  }
  const bool row = !selectedRow().isEmpty();
  for (QPushButton* button : {m_deleteRow, m_defaultRow, m_apply}) {
    button->setEnabled(row);
  }
  m_addRow->setEnabled(!m_table.options.isEmpty() || m_table.closed);
  // mitcad#999: an open table becomes closed with its first column.
  m_addColumn->setToolTip(m_table.closed
                              ? tr("A parameter column: another parameter that every row sets, as d, P, s and k "
                                   "of a screw size")
                              : tr("A parameter column: another parameter that every row sets; Only these rows "
                                   "turns on"));
  m_removeColumn->setEnabled(!selectedColumn().isEmpty());
  m_removeColumn->setToolTip(m_table.parameters.isEmpty()
                                 ? tr("The rows have no parameter columns")
                                 : tr("Take the current cell's parameter column out of the table; the parameter "
                                      "stays"));
  m_exportRows->setEnabled(!m_table.rows.isEmpty());
  m_generateRows->setEnabled(!m_tables.empty());
  m_generateRows->setToolTip(m_tables.empty()
                                 ? tr("The design has no tables (Edit Parameters, Tables)")
                                 : tr("A row per value of a table's key column, setting an option to it"));
  if (!problems.isEmpty() && m_message->text().isEmpty()) {
    showMessage(problems.first(), true);
  }
  QTimer::singleShot(0, this, &ConfigurationsDialog::logLayout);
}

void ConfigurationsDialog::checkRows() {
  m_rowIssues.clear();
  if (m_table.rules().isEmpty() || m_table.options.isEmpty()) {
    return;
  }
  // Each row's combination tried through the cascade, nothing changed: a
  // refusal names the rule the row breaks.
  for (const cfg::Row& row : std::as_const(m_table.rows)) {
    try {
      const QJsonObject answer =
          m_host.model().queryObject(cfg::configureQuery(cfg::rowChoice(m_table, row, m_report)));
      if (const std::optional<cfg::Refusal> refused = cfg::readRefusal(answer.value(QStringLiteral("refused")))) {
        m_rowIssues.insert(row.name, *refused);
      }
    } catch (const std::exception& e) {
      m_rowIssues.insert(row.name, cfg::Refusal{QString(), QString(), QString::fromUtf8(e.what())});
    }
  }
  QStringList issues;
  for (const cfg::Row& row : std::as_const(m_table.rows)) {
    if (m_rowIssues.contains(row.name)) {
      issues << QStringLiteral("%1: %2").arg(row.name, m_rowIssues.value(row.name).describe());
    }
  }
  const QString logged = issues.join(QStringLiteral("; "));
  if (logged != m_loggedIssues) {
    m_loggedIssues = logged;
    qDebug().noquote() << QStringLiteral("Configurations row issues: %1")
                              .arg(logged.isEmpty() ? QStringLiteral("none") : logged);
  }
}

void ConfigurationsDialog::fillOptions() {
  const QString current = selectedOption();
  m_options->clear();
  QStringList logged;
  QStringList usage;
  QStringList marks; // the cells marked at a level (mitcad#1051)
  for (const QString& name : std::as_const(m_table.options)) {
    const QJsonObject parameter = m_parameters.value(name);
    const cfg::OptionReport* report = m_report.option(name);
    auto* item = new QTreeWidgetItem(m_options);
    // Dragged into its order, never onto another option.
    item->setFlags(Qt::ItemIsSelectable | Qt::ItemIsEnabled | Qt::ItemIsDragEnabled | Qt::ItemIsEditable);
    item->setData(kOptionName, kNameRole, name);
    item->setData(kOptionName, NativeControlIdentifierRole, QStringLiteral("configurationOption.") + name);
    item->setText(kOptionName, name);
    item->setText(kOptionUnit, parameter.value(QStringLiteral("unit")).toString());
    const bool driven = report != nullptr && report->driven;
    QString domain = report != nullptr ? report->domain.summary() : QString();
    // Values from a table column: the column, then its values (mitcad#849).
    const QString valuesFrom = cfg::valuesColumn(parameter);
    if (!valuesFrom.isEmpty()) {
      domain = domain.isEmpty() ? valuesFrom : QStringLiteral("%1: %2").arg(valuesFrom, domain);
    }
    item->setText(kOptionDomain, domain);
    // Possible values, and what is left of them now: the allowed values.
    if (report != nullptr && !report->restrictedBy.isEmpty()) {
      item->setToolTip(kOptionDomain,
                       tr("Allowed values now: %1 (restricted by %2)")
                           .arg(report->allowed.summary(), report->restrictedBy.join(QStringLiteral(", "))));
    } else {
      item->setToolTip(kOptionDomain, tr("Double-click to edit the possible values"));
    }
    // The current value (mitcad#968): a By Formula option's evaluated value,
    // its expression in the tooltip; never the unevaluated formula.
    const QString expression = parameter.value(QStringLiteral("expression")).toString();
    const cfg::ValueCell value = cfg::valueCell(report, isText(name), expression);
    item->setText(kOptionValue, value.text);
    item->setText(kOptionComment, parameter.value(QStringLiteral("comment")).toString());
    if (driven) {
      item->setText(kOptionFormula, tr("By formula"));
      item->setToolTip(kOptionFormula, tr("Calculated from its expression %1: neither a chooser nor automatic "
                                          "adjustment sets it")
                                           .arg(value.expression));
      item->setToolTip(kOptionValue, tr("Expression: %1").arg(value.expression));
    }
    if (parameter.isEmpty()) {
      // An option without its parameter: an error (mitcad#1051).
      markLevel(item, kOptionName, NoticeLevel::Error, tr("%1 is no user parameter of the design").arg(name),
                m_options->palette());
      marks << QStringLiteral("%1/name %2").arg(name, noticeLevelName(NoticeLevel::Error));
    }
    for (const int column : {kOptionValue, kOptionComment}) {
      const QString header = m_options->headerItem()->text(column).toLower();
      const auto refused = m_refused.constFind(QStringLiteral("%1|%2").arg(name, header));
      if (refused != m_refused.cend()) {
        item->setText(column, refused.value().first);
        markLevel(item, column, NoticeLevel::Error, refusedText(refused.value().second), m_options->palette());
        marks << QStringLiteral("%1/%2 %3").arg(name, header, noticeLevelName(NoticeLevel::Error));
      }
    }
    // The bindings that follow it (mitcad#862).
    if (const int users = m_bindings != nullptr ? m_bindings->usage().value(name) : 0; users > 0) {
      item->setText(kOptionUsedBy, users == 1 ? tr("used by 1 binding") : tr("used by %1 bindings").arg(users));
      item->setToolTip(kOptionUsedBy, m_bindings->usersOf(name).join(QLatin1Char('\n')));
      usage << QStringLiteral("Configurations option %1 used by %2 binding%3")
                   .arg(name)
                   .arg(users)
                   .arg(users == 1 ? QString() : QStringLiteral("s"));
    }
    if (name == current) {
      m_options->setCurrentItem(item);
    }
    // A By Formula option with the expression its Value's tooltip shows.
    logged << QStringLiteral("%1 (%2) = %3%4")
                  .arg(name, domain.isEmpty() ? QStringLiteral("?") : domain, item->text(kOptionValue),
                       driven ? QStringLiteral(", by formula %1").arg(value.expression) : QString());
  }
  const QString text = QStringLiteral("Configurations options: %1")
                           .arg(logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral("; ")));
  if (text != m_loggedOptions) {
    m_loggedOptions = text;
    qDebug().noquote() << text;
    if (logged.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configurations empty Options: %1").arg(QString::fromUtf8(kNoOptions));
    }
  }
  m_options->viewport()->update(); // its placeholder
  const QString marked = marks.join(QStringLiteral("; "));
  if (marked != m_loggedOptionMarks) {
    m_loggedOptionMarks = marked;
    qDebug().noquote() << QStringLiteral("Configurations option marks: %1")
                              .arg(marked.isEmpty() ? QStringLiteral("none") : marked);
  }
  const QString used = usage.join(QLatin1Char('\n'));
  if (used != m_loggedUsage) {
    m_loggedUsage = used;
    for (const QString& line : std::as_const(usage)) {
      qDebug().noquote() << line;
    }
  }
}

void ConfigurationsDialog::setBindingActions(const BindingActions& actions) { m_bindings->setActions(actions); }

void ConfigurationsDialog::showBindings(const QString& option) {
  m_tabs->setCurrentWidget(m_bindings);
  m_bindings->showOption(option);
}

QVector<ConfigurationsDialog::RowColumn> ConfigurationsDialog::rowColumns() const {
  QVector<RowColumn> columns{{RowColumn::Name, QString()}};
  for (const QString& option : m_table.options) {
    columns.append({RowColumn::Option, option});
  }
  for (const QString& parameter : m_table.parameters) {
    columns.append({RowColumn::Parameter, parameter});
  }
  columns.append({RowColumn::Designation, QString()});
  columns.append({RowColumn::PartNumber, QString()});
  columns.append({RowColumn::SupplierUrl, QString()});
  return columns;
}

void ConfigurationsDialog::fillRows() {
  const QString current = m_selectRow.isEmpty() ? selectedRow() : m_selectRow;
  m_selectRow.clear();
  const QVector<RowColumn> columns = rowColumns();
  m_rows->clear();
  m_rows->setColumnCount(static_cast<int>(columns.size()));
  m_rows->setRowCount(static_cast<int>(m_table.rows.size()));
  QStringList headers;
  for (const RowColumn& column : columns) {
    switch (column.kind) {
    case RowColumn::Name:
      headers << tr("Name");
      break;
    case RowColumn::Option:
    case RowColumn::Parameter:
      headers << column.parameter;
      break;
    case RowColumn::Designation:
      headers << tr("Designation");
      break;
    case RowColumn::PartNumber:
      headers << tr("Part Number");
      break;
    case RowColumn::SupplierUrl:
      headers << tr("Supplier URL");
      break;
    }
  }
  m_rows->setHorizontalHeaderLabels(headers);
  QStringList logged;
  QStringList marks; // the cells marked at a level (mitcad#1051)
  for (int r = 0; r < m_table.rows.size(); ++r) {
    const cfg::Row& row = m_table.rows.at(r);
    const auto issue = m_rowIssues.constFind(row.name);
    const QStringList named =
        issue != m_rowIssues.cend() ? cfg::optionsNamed(m_table.options, issue.value()) : QStringList();
    QStringList values;
    for (int c = 0; c < columns.size(); ++c) {
      const RowColumn& column = columns.at(c);
      QString shown;
      switch (column.kind) {
      case RowColumn::Name:
        shown = row.name;
        break;
      case RowColumn::Option:
      case RowColumn::Parameter:
        shown = cfg::rowText(isText(column.parameter), row.value(column.parameter));
        values << QStringLiteral("%1 = %2").arg(column.parameter, shown);
        break;
      case RowColumn::Designation:
        shown = row.designation;
        break;
      case RowColumn::PartNumber:
        shown = row.partNumber;
        break;
      case RowColumn::SupplierUrl:
        shown = row.supplierUrl;
        break;
      }
      auto* item = new QTableWidgetItem(shown);
      item->setData(kNameRole, row.name);
      if (column.kind == RowColumn::Name) {
        item->setData(NativeControlIdentifierRole, QStringLiteral("configurationRow.") + row.name);
      }
      item->setFlags(Qt::ItemIsSelectable | Qt::ItemIsEnabled | Qt::ItemIsEditable);
      if (column.kind == RowColumn::Name && row.name == m_table.defaultRow) {
        QFont font = item->font();
        font.setBold(true);
        item->setFont(font);
        item->setToolTip(tr("The default row"));
      }
      // A row that breaks a rule: the options the rule's text names, else
      // its name.
      // A warning (mitcad#1051): the row is kept, the user should check it.
      if (issue != m_rowIssues.cend() &&
          ((column.kind == RowColumn::Option && named.contains(column.parameter)) ||
           (column.kind == RowColumn::Name && named.isEmpty()))) {
        markLevel(item, NoticeLevel::Warning,
                  noticeLevelText(NoticeLevel::Warning, tr("breaks %1").arg(issue.value().describe())),
                  m_rows->palette());
        marks << QStringLiteral("%1/%2 %3").arg(row.name, headers.at(c), noticeLevelName(NoticeLevel::Warning));
      }
      const auto refused = m_refused.constFind(QStringLiteral("%1|%2").arg(row.name, headers.at(c)));
      if (refused != m_refused.cend()) {
        markRefused(item, refused.value().first, refused.value().second, m_rows->palette());
        marks << QStringLiteral("%1/%2 %3").arg(row.name, headers.at(c), noticeLevelName(NoticeLevel::Error));
      }
      m_rows->setItem(r, c, item);
    }
    if (row.name == current) {
      m_rows->selectRow(r);
    }
    logged << QStringLiteral("%1: %2").arg(row.name, values.join(QStringLiteral(", ")));
  }
  {
    const QSignalBlocker blocker(m_closed);
    m_closed->setChecked(m_table.closed);
  }
  m_current->setText(m_report.row.isEmpty() ? tr("The options match no row.")
                                            : tr("The options match the row %1.").arg(m_report.row));
  const QString text = QStringLiteral("%1, default %2, current %3: %4")
                           .arg(m_table.closed ? QStringLiteral("closed") : QStringLiteral("open"),
                                m_table.defaultRow.isEmpty() ? QStringLiteral("none") : m_table.defaultRow,
                                m_report.row.isEmpty() ? QStringLiteral("none") : m_report.row,
                                logged.isEmpty() ? QStringLiteral("no rows") : logged.join(QStringLiteral("; ")));
  if (text != m_loggedRows) {
    m_loggedRows = text;
    qDebug().noquote() << QStringLiteral("Configurations rows: %1").arg(text);
    if (m_table.rows.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configurations empty Rows: %1").arg(QString::fromUtf8(kNoRows));
    }
  }
  const QString marked = marks.join(QStringLiteral("; "));
  if (marked != m_loggedRowMarks) {
    m_loggedRowMarks = marked;
    qDebug().noquote() << QStringLiteral("Configurations row marks: %1")
                              .arg(marked.isEmpty() ? QStringLiteral("none") : marked);
  }
  m_rows->viewport()->update(); // its placeholder
}

// ---------------------------------------------------------------------------
// Changing the model

bool ConfigurationsDialog::run(const QJsonObject& command, QJsonObject* result) {
  m_reason.clear();
  if (const QString readOnly = m_host.readOnlyReason(); !readOnly.isEmpty()) {
    m_reason = readOnly;
  } else if (!m_host.canChangeModel()) {
    m_reason = m_host.changeBlockedReason();
  } else if (m_host.runModelCommand(command, result)) {
    return true;
  } else if (m_host.lastCancelled()) {
    showMessage(tr("The change was canceled."), false);
    return false;
  } else {
    m_reason = m_host.lastError();
  }
  showMessage(m_reason, true);
  return false;
}

void ConfigurationsDialog::refuse(const QString& key, const QString& what, const QString& typed,
                                  const QString& reason) {
  if (!key.isEmpty()) {
    m_refused.insert(key, {typed, reason});
  }
  showMessage(reason, true);
  qDebug().noquote() << QStringLiteral("Configurations %1: %2 refused: %3").arg(what, typed, reason);
}

bool ConfigurationsDialog::submitTable(const cfg::Table& table, const QString& key, const QString& typed,
                                       const QString& what) {
  const bool done = run(cfg::setTableCommand(table));
  if (done) {
    m_refused.remove(key);
    showMessage(QString(), false);
    qDebug().noquote() << QStringLiteral("Configurations %1: %2").arg(what, typed);
  } else if (!m_reason.isEmpty()) {
    refuse(key, what, typed, m_reason);
  }
  refresh();
  return done;
}

void ConfigurationsDialog::addOption() {
  // A closed table's parameter columns too: one moves to the options with
  // its rows' values (mitcad#1044).
  const QStringList candidates = cfg::optionCandidates(m_table, m_userParameters);
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  AddOptionDialog dialog(m_host, m_table, candidates, m_parameters, this);
  qDebug().noquote() << QStringLiteral("Add Option dialog opened: %1")
                            .arg((candidates + QStringList{QStringLiteral("New Parameter")}).join(QStringLiteral(", ")));
  prepareModal(&dialog);
  execDialog(&dialog);
  if (!dialog.added().isEmpty()) {
    showMessage(QString(), false);
  }
  refresh();
}

void ConfigurationsDialog::removeOption() {
  const QString name = selectedOption();
  if (name.isEmpty()) {
    return;
  }
  cfg::Table table = m_table;
  table.options.removeAll(name);
  for (cfg::Row& row : table.rows) {
    row.setValue(name, QString());
  }
  submitTable(table, QString(), name, QStringLiteral("removed option"));
}

void ConfigurationsDialog::editDomain() {
  const QString name = selectedOption();
  if (name.isEmpty()) {
    return;
  }
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  DomainDialog dialog(m_host, name, m_parameters.value(name), isText(name), m_tables, this);
  qDebug().noquote() << QStringLiteral("Possible Values dialog opened for %1").arg(name);
  prepareModal(&dialog);
  execDialog(&dialog);
  refresh();
}

void ConfigurationsDialog::moveOption(int by) {
  const QString name = selectedOption();
  const qsizetype from = m_table.options.indexOf(name);
  const qsizetype to = from + by;
  if (from < 0 || to < 0 || to >= m_table.options.size()) {
    return;
  }
  cfg::Table table = m_table;
  table.options.move(from, to);
  submitTable(table, QString(), table.options.join(QStringLiteral(", ")), QStringLiteral("options reordered"));
}

void ConfigurationsDialog::optionsDropped() {
  QStringList order;
  for (int i = 0; i < m_options->topLevelItemCount(); ++i) {
    order << m_options->topLevelItem(i)->data(kOptionName, kNameRole).toString();
  }
  if (order == m_table.options || order.size() != m_table.options.size()) {
    refresh(); // nothing moved, or a drop the tree should not have taken
    return;
  }
  later([this, order] {
    cfg::Table table = m_table;
    table.options = order;
    submitTable(table, QString(), order.join(QStringLiteral(", ")), QStringLiteral("options reordered"));
  });
}

void ConfigurationsDialog::optionEdited(QTreeWidgetItem* item, int column) {
  if (m_building || item == nullptr || (column != kOptionValue && column != kOptionComment)) {
    return;
  }
  const QString name = item->data(kOptionName, kNameRole).toString();
  const QString typed = item->text(column);
  const QString header = m_options->headerItem()->text(column).toLower();
  later([this, name, typed, header, column] {
    const QString key = QStringLiteral("%1|%2").arg(name, header);
    const QString what = QStringLiteral("option %1 %2").arg(name, header);
    QJsonObject command;
    if (column == kOptionComment) {
      if (typed == m_parameters.value(name).value(QStringLiteral("comment")).toString()) {
        return;
      }
      command = {{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
                 {QStringLiteral("name"), name},
                 {QStringLiteral("comment"), typed}};
    } else {
      // The option's own value goes through the cascade, as a chooser's.
      cfg::OptionReport option;
      if (const cfg::OptionReport* report = m_report.option(name)) {
        option = *report;
      } else {
        option.name = name;
        option.text = isText(name);
        option.unit = unitOf(name);
      }
      command = cfg::configureCommand({{name, cfg::chooserValue(option, typed)}});
    }
    QJsonObject result;
    if (run(command, &result)) {
      if (const std::optional<cfg::Refusal> refused = cfg::readRefusal(result.value(QStringLiteral("refused")))) {
        refuse(key, what, typed, refused->describe());
      } else {
        m_refused.remove(key);
        const QString notice = cfg::movesNotice(cfg::readMoves(result.value(QStringLiteral("moves"))));
        showMessage(notice, false);
        qDebug().noquote() << QStringLiteral("Configurations %1: %2").arg(what, typed);
        if (!notice.isEmpty()) {
          qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(notice);
        }
      }
    } else if (!m_reason.isEmpty()) {
      refuse(key, what, typed, m_reason);
    }
    refresh();
  });
}

void ConfigurationsDialog::rowEdited(QTableWidgetItem* item) {
  if (m_building || item == nullptr) {
    return;
  }
  const QVector<RowColumn> columns = rowColumns();
  const int r = item->row();
  const int c = item->column();
  if (r < 0 || r >= m_table.rows.size() || c < 0 || c >= columns.size()) {
    return;
  }
  const QString rowName = m_table.rows.at(r).name;
  const RowColumn column = columns.at(c);
  const QString header = m_rows->horizontalHeaderItem(c) != nullptr ? m_rows->horizontalHeaderItem(c)->text() : QString();
  const QString typed = item->text().trimmed();
  later([this, rowName, column, header, typed] {
    const QString key = QStringLiteral("%1|%2").arg(rowName, header);
    const QString what = QStringLiteral("row %1 %2").arg(rowName, header);
    cfg::Table table = m_table;
    cfg::Row* row = table.row(rowName);
    if (row == nullptr) {
      refresh();
      return;
    }
    switch (column.kind) {
    case RowColumn::Name:
      if (typed == rowName) {
        refresh();
        return;
      }
      if (table.defaultRow == rowName) {
        table.defaultRow = typed;
      }
      row->name = typed;
      break;
    case RowColumn::Option:
    case RowColumn::Parameter: {
      const QString expression = cfg::rowExpression(isText(column.parameter), unitOf(column.parameter), typed);
      if (expression == row->value(column.parameter)) {
        m_refused.remove(key);
        refresh();
        return;
      }
      row->setValue(column.parameter, expression);
      // A value a rule refuses is not written (decision 4: rules hold
      // everywhere): the row's combination tried through the cascade.
      if (column.kind == RowColumn::Option && !table.rules().isEmpty()) {
        try {
          const QJsonObject answer =
              m_host.model().queryObject(cfg::configureQuery(cfg::rowChoice(table, *row, m_report)));
          if (const std::optional<cfg::Refusal> refused =
                  cfg::readRefusal(answer.value(QStringLiteral("refused")))) {
            refuse(key, what, typed, refused->describe());
            refresh();
            return;
          }
        } catch (const std::exception& e) {
          refuse(key, what, typed, QString::fromUtf8(e.what()));
          refresh();
          return;
        }
      }
      break;
    }
    case RowColumn::Designation:
      row->designation = typed;
      break;
    case RowColumn::PartNumber:
      row->partNumber = typed;
      break;
    case RowColumn::SupplierUrl:
      row->supplierUrl = typed;
      break;
    }
    if (column.kind == RowColumn::Name) {
      m_selectRow = typed;
    }
    submitTable(table, key, typed, what);
  });
}

cfg::Row ConfigurationsDialog::freshRow() const {
  cfg::Row row;
  // The options' values as they are now, as constants; a closed table's
  // other parameters' expressions too.
  for (const QString& option : m_table.options) {
    const cfg::OptionReport* report = m_report.option(option);
    const QString expression = report != nullptr ? report->expression
                                                 : m_parameters.value(option).value(QStringLiteral("expression")).toString();
    row.setValue(option, expression);
  }
  for (const QString& parameter : m_table.parameters) {
    row.setValue(parameter, m_parameters.value(parameter).value(QStringLiteral("expression")).toString());
  }
  return row;
}

void ConfigurationsDialog::addRow() {
  cfg::Table table = m_table;
  cfg::Row row = freshRow();
  row.name = table.nextRowName();
  table.rows.append(row);
  m_selectRow = row.name;
  submitTable(table, QString(), row.name, QStringLiteral("added row"));
}

void ConfigurationsDialog::generateRows() {
  if (m_tables.empty()) {
    showMessage(tr("The design has no tables: add one in Edit Parameters, Tables."), true);
    return;
  }
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  GenerateRowsDialog dialog(m_host, m_tables, m_table, m_userParameters, m_parameters, this);
  prepareModal(&dialog);
  execDialog(&dialog);
  if (dialog.generated()) {
    const cfg::GeneratedRows& rows = dialog.rows();
    const QString notice = cfg::movesNotice(rows.moves);
    const QString described = rows.describe(dialog.generatedTable());
    const QString done = tr("Generated %1.").arg(described);
    const QString text = notice.isEmpty() ? done : QStringLiteral("%1 %2").arg(done, notice);
    showMessage(text, false);
    m_host.showStatus(text, NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Configurations generated rows from %1 for %2: %3")
                              .arg(dialog.generatedReference(), dialog.generatedOption(), described);
    if (!notice.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(notice);
    }
    if (!rows.rows.isEmpty()) {
      m_selectRow = rows.rows.first();
    }
  }
  refresh();
}

void ConfigurationsDialog::deleteRow() {
  const QString name = selectedRow();
  if (name.isEmpty()) {
    return;
  }
  cfg::Table table = m_table;
  for (qsizetype i = 0; i < table.rows.size(); ++i) {
    if (table.rows.at(i).name == name) {
      table.rows.removeAt(i);
      break;
    }
  }
  if (table.defaultRow == name) {
    table.defaultRow.clear();
  }
  submitTable(table, QString(), name, QStringLiteral("deleted row"));
}

void ConfigurationsDialog::setDefaultRow() {
  const QString name = selectedRow();
  if (name.isEmpty()) {
    return;
  }
  cfg::Table table = m_table;
  table.defaultRow = name;
  m_selectRow = name;
  submitTable(table, QString(), name, QStringLiteral("default row"));
}

void ConfigurationsDialog::applyRow() {
  const QString name = selectedRow();
  if (name.isEmpty()) {
    showMessage(tr("Select the row to apply."), true);
    return;
  }
  QJsonObject result;
  if (run(cfg::applyCommand(name), &result)) {
    const QString notice = cfg::movesNotice(cfg::readMoves(result.value(QStringLiteral("moves"))));
    const QString text = notice.isEmpty() ? tr("Applied %1.").arg(name) : tr("Applied %1. %2").arg(name, notice);
    showMessage(text, false);
    m_host.showStatus(text, NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Configurations applied %1%2")
                              .arg(name, notice.isEmpty() ? QString() : QStringLiteral("; moves: ") + notice);
    if (!notice.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(notice);
    }
  } else if (!m_reason.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Configurations apply %1 refused: %2").arg(name, m_reason);
  }
  m_selectRow = name;
  refresh();
}

void ConfigurationsDialog::addColumn() {
  if (const QString readOnly = m_host.readOnlyReason(); !readOnly.isEmpty()) {
    showMessage(readOnly, true);
    return;
  }
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  const QStringList candidates = cfg::columnCandidates(m_table, m_userParameters);
  AddColumnDialog dialog(m_host, m_table, candidates, m_parameters, this);
  qDebug().noquote() << QStringLiteral("Add Column dialog opened: %1")
                            .arg((candidates + QStringList{QStringLiteral("New Parameter")}).join(QStringLiteral(", ")));
  prepareModal(&dialog);
  execDialog(&dialog);
  if (!dialog.added().isEmpty()) {
    showMessage(QString(), false);
  }
  refresh();
}

void ConfigurationsDialog::removeColumn() {
  const QString name = selectedColumn();
  if (name.isEmpty()) {
    showMessage(tr("Select a cell of the parameter column to remove."), true);
    return;
  }
  // The rows no longer set it; the parameter keeps its value.
  if (submitTable(cfg::withoutColumn(m_table, name), QString(), name, QStringLiteral("removed column"))) {
    qDebug().noquote() << QStringLiteral("Configurations: column %1 removed").arg(name);
  }
}

// ---------------------------------------------------------------------------
// Filling a closed table (mitcad#1044)

QVector<cfg::SheetColumn> ConfigurationsDialog::sheetColumns() const {
  QVector<cfg::SheetColumn> out;
  for (const RowColumn& column : rowColumns()) {
    cfg::SheetColumn sheet;
    switch (column.kind) {
    case RowColumn::Name:
      sheet.kind = cfg::SheetColumn::Kind::Name;
      break;
    case RowColumn::Option:
      sheet.kind = cfg::SheetColumn::Kind::Option;
      break;
    case RowColumn::Parameter:
      sheet.kind = cfg::SheetColumn::Kind::Parameter;
      break;
    case RowColumn::Designation:
      sheet.kind = cfg::SheetColumn::Kind::Designation;
      break;
    case RowColumn::PartNumber:
      sheet.kind = cfg::SheetColumn::Kind::PartNumber;
      break;
    case RowColumn::SupplierUrl:
      sheet.kind = cfg::SheetColumn::Kind::SupplierUrl;
      break;
    }
    sheet.parameter = column.parameter;
    if (!column.parameter.isEmpty()) {
      sheet.text = isText(column.parameter);
      sheet.unit = unitOf(column.parameter);
    }
    out.append(sheet);
  }
  return out;
}

void ConfigurationsDialog::copyCells() {
  const QList<QTableWidgetSelectionRange> ranges = m_rows->selectedRanges();
  if (ranges.isEmpty()) {
    return;
  }
  // The block from the top left selected cell to the bottom right; a cell
  // between them that is not selected is empty.
  int top = ranges.first().topRow();
  int bottom = ranges.first().bottomRow();
  int left = ranges.first().leftColumn();
  int right = ranges.first().rightColumn();
  for (const QTableWidgetSelectionRange& range : ranges) {
    top = std::min(top, range.topRow());
    bottom = std::max(bottom, range.bottomRow());
    left = std::min(left, range.leftColumn());
    right = std::max(right, range.rightColumn());
  }
  QVector<QStringList> block;
  for (int r = top; r <= bottom; ++r) {
    QStringList cells;
    for (int c = left; c <= right; ++c) {
      const QTableWidgetItem* item = m_rows->item(r, c);
      cells << (item != nullptr && item->isSelected() ? item->text() : QString());
    }
    block.append(cells);
  }
  QGuiApplication::clipboard()->setText(cfg::writeBlock(block));
  qDebug().noquote() << QStringLiteral("Configurations copied %1 x %2 cells").arg(bottom - top + 1).arg(right - left + 1);
}

void ConfigurationsDialog::pasteCells() {
  const QVector<QStringList> block = cfg::readBlock(QGuiApplication::clipboard()->text());
  if (block.isEmpty()) {
    showMessage(tr("The clipboard has no cells to paste."), true);
    return;
  }
  const QVector<cfg::SheetColumn> columns = sheetColumns();
  // At the current cell; below the last row without one.
  const int row = m_rows->currentRow() < 0 ? static_cast<int>(m_table.rows.size()) : m_rows->currentRow();
  const int column = std::max(0, m_rows->currentColumn());
  const auto header = [this](int c) {
    const QTableWidgetItem* item = m_rows->horizontalHeaderItem(c);
    return item != nullptr ? item->text() : QString();
  };
  const QString at = QStringLiteral("%1 %2").arg(row < m_table.rows.size() ? m_table.rows.at(row).name
                                                                             : QStringLiteral("a new row"),
                                                 header(column));
  const cfg::Paste paste = cfg::pasteBlock(m_table, columns, row, column, block, freshRow());
  // What was typed into each pasted cell, in the paste's order.
  QStringList typed;
  qsizetype widest = 0;
  for (const QStringList& cells : block) {
    widest = std::max(widest, cells.size());
    for (qsizetype j = 0; j < cells.size(); ++j) {
      if (column + j < columns.size()) {
        typed << cells.at(j).trimmed();
      }
    }
  }
  const QString extent = QStringLiteral("%1 x %2").arg(block.size()).arg(widest);
  if (run(cfg::setTableCommand(paste.table))) {
    for (const auto& cell : paste.cells) {
      m_refused.remove(QStringLiteral("%1|%2").arg(cell.first, header(cell.second)));
    }
    QString notice = tr("Pasted %1 cells.").arg(extent);
    if (!paste.added.isEmpty()) {
      notice += QStringLiteral(" %1").arg(tr("Added %1.").arg(paste.added.join(QStringLiteral(", "))));
    }
    if (paste.beyond > 0) {
      notice += QStringLiteral(" %1").arg(tr("%n cell(s) right of the last column left out.", nullptr, paste.beyond));
    }
    showMessage(notice, false);
    qDebug().noquote() << QStringLiteral("Configurations pasted %1 cells at %2: added %3")
                              .arg(extent, at,
                                   paste.added.isEmpty() ? QStringLiteral("none")
                                                         : paste.added.join(QStringLiteral(", ")));
  } else if (!m_reason.isEmpty()) {
    // Each pasted cell keeps what was pasted, in red with the reason.
    for (qsizetype i = 0; i < paste.cells.size(); ++i) {
      const auto& cell = paste.cells.at(i);
      m_refused.insert(QStringLiteral("%1|%2").arg(cell.first, header(cell.second)), {typed.value(i), m_reason});
    }
    qDebug().noquote() << QStringLiteral("Configurations paste of %1 cells at %2 refused: %3").arg(extent, at, m_reason);
  }
  refresh();
}

void ConfigurationsDialog::clearCells() {
  QVector<QPair<int, int>> cells;
  const QList<QTableWidgetItem*> selected = m_rows->selectedItems();
  for (const QTableWidgetItem* item : selected) {
    cells.append(qMakePair(item->row(), item->column()));
  }
  const cfg::Table table = cfg::clearCells(m_table, sheetColumns(), cells);
  if (cells.isEmpty() || table.toJson() == m_table.toJson()) {
    return; // nothing selected, or names only
  }
  if (run(cfg::setTableCommand(table))) {
    showMessage(QString(), false);
    qDebug().noquote() << QStringLiteral("Configurations cleared %1 cells").arg(cells.size());
  }
  refresh();
}

void ConfigurationsDialog::importRows() {
  if (const QString readOnly = m_host.readOnlyReason(); !readOnly.isEmpty()) {
    showMessage(readOnly, true);
    return;
  }
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  const QString path = dialogOpenFileName(this, tr("Import Rows from CSV"),
                                          dialogStartFolder(DialogGroup::Import, m_host.documentFolder()),
                                          tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Import of configuration rows cancelled");
    return;
  }
  rememberDialogFolder(DialogGroup::Import, path);
  const QString file = QFileInfo(path).fileName();
  QFile input(path);
  if (!input.open(QIODevice::ReadOnly)) {
    const QString problem = tr("Could not read %1: %2").arg(QDir::toNativeSeparators(path), input.errorString());
    showMessage(problem, true);
    qDebug().noquote() << QStringLiteral("Import of configuration rows from %1 refused: %2").arg(file, problem);
    return;
  }
  const QString csv = QString::fromUtf8(input.readAll());
  input.close();
  RowsImportDialog dialog(m_host, csv, file, this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Import of configuration rows from %1 cancelled").arg(file);
    refresh();
    return;
  }
  QJsonObject result;
  if (run(cfg::importRowsCsvCommand(csv), &result)) {
    const QString summary = result.value(QStringLiteral("summary")).toString();
    const QString moves = cfg::movesNotice(cfg::readMoves(result.value(QStringLiteral("moves"))));
    const QString imported = tr("Imported the rows from %1: %2.").arg(file, summary);
    const QString text = moves.isEmpty() ? imported : QStringLiteral("%1 %2").arg(imported, moves);
    showMessage(text, false);
    m_host.showStatus(text, NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Configurations imported rows from %1: %2").arg(file, summary);
    if (!moves.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(moves);
    }
  } else if (!m_reason.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Import of configuration rows from %1 refused: %2").arg(file, m_reason);
  }
  refresh();
}

void ConfigurationsDialog::exportRows() {
  QJsonObject answer;
  try {
    answer = m_host.model().queryObject(cfg::rowsCsvQuery());
  } catch (const std::exception& e) {
    showMessage(QString::fromUtf8(e.what()), true);
    return;
  }
  const QString folder = dialogStartFolder(DialogGroup::Export, m_host.documentFolder());
  const QString file = QStringLiteral("Rows.csv");
  const QString path = dialogSaveFileName(this, tr("Export Rows to CSV"),
                                          folder.isEmpty() ? file : QDir(folder).filePath(file),
                                          tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Export of configuration rows cancelled");
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QByteArray bytes = answer.value(QStringLiteral("csv")).toString().toUtf8();
  QFile output(path);
  if (!output.open(QIODevice::WriteOnly | QIODevice::Truncate) || output.write(bytes) != bytes.size()) {
    const QString problem = tr("Could not write %1: %2").arg(QDir::toNativeSeparators(path), output.errorString());
    showMessage(problem, true);
    qDebug().noquote() << QStringLiteral("Export of configuration rows refused: %1").arg(problem);
    return;
  }
  output.close();
  const int count = answer.value(QStringLiteral("rows")).toInt();
  const QString text = tr("Exported %n row(s) to %1.", nullptr, count).arg(QDir::toNativeSeparators(path));
  showMessage(text, false);
  m_host.showStatus(text, NoticeLevel::Information);
  qDebug().noquote() << QStringLiteral("Configurations exported rows to %1: %2 row(s)").arg(path).arg(count);
}

// ---------------------------------------------------------------------------
// The log, for UI tests

void ConfigurationsDialog::logLayout() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  const auto place = [&lines, &origin](const QString& what, const QWidget* widget, const QPoint& point) {
    const QPoint at = widget->mapToGlobal(point) - origin;
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  QTabBar* bar = m_tabs->tabBar();
  for (int i = 0; i < bar->count(); ++i) {
    place(QStringLiteral("Configurations tab %1").arg(bar->tabText(i)), bar, bar->tabRect(i).center());
  }
  const std::pair<const char*, QWidget*> buttons[] = {
      {"add option", m_addOption},   {"remove option", m_removeOption}, {"edit domain", m_editDomain},
      {"move up", m_up},             {"move down", m_down},            {"closed", m_closed},
      {"add row", m_addRow},         {"delete row", m_deleteRow},      {"set as default", m_defaultRow},
      {"apply", m_apply},            {"generate rows", m_generateRows}, {"add column", m_addColumn},
      {"remove column", m_removeColumn}, {"import csv", m_importRows},  {"export csv", m_exportRows}};
  for (const auto& [name, widget] : buttons) {
    if (widget->isVisible()) {
      place(QStringLiteral("Configurations %1").arg(QLatin1String(name)), widget, widget->rect().center());
    }
  }
  if (m_options->isVisible()) {
    const QRect viewport = m_options->viewport()->rect();
    for (int i = 0; i < m_options->topLevelItemCount(); ++i) {
      QTreeWidgetItem* item = m_options->topLevelItem(i);
      const QRect row = m_options->visualItemRect(item);
      if (!viewport.contains(row.center())) {
        continue;
      }
      for (int column = kOptionName; column <= kOptionUsedBy; ++column) {
        const QRect cell(m_options->header()->sectionViewportPosition(column), row.top(),
                         m_options->header()->sectionSize(column), row.height());
        place(QStringLiteral("Configurations option %1 %2")
                  .arg(item->data(kOptionName, kNameRole).toString(), m_options->headerItem()->text(column).toLower()),
              m_options->viewport(), cell.center());
      }
    }
  }
  if (m_rows->isVisible()) {
    const QRect viewport = m_rows->viewport()->rect();
    for (int r = 0; r < m_rows->rowCount(); ++r) {
      for (int c = 0; c < m_rows->columnCount(); ++c) {
        const QTableWidgetItem* item = m_rows->item(r, c);
        const QTableWidgetItem* header = m_rows->horizontalHeaderItem(c);
        const QRect cell = item != nullptr ? m_rows->visualItemRect(item) : QRect();
        if (item == nullptr || header == nullptr || !viewport.contains(cell.center())) {
          continue;
        }
        place(QStringLiteral("Configurations row %1 %2").arg(item->data(kNameRole).toString(), header->text()),
              m_rows->viewport(), cell.center());
      }
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
