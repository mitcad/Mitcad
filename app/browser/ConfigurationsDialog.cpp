// SPDX-License-Identifier: MIT
#include "ConfigurationsDialog.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QAbstractItemView>
#include <QButtonGroup>
#include <QCheckBox>
#include <QColor>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDropEvent>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
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

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativeDialogs.hpp"
#include "ConfigurationBindingsPage.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

namespace cfg = configurations;

const QColor kRefusedText(0xb0, 0x00, 0x20);
const QColor kRefusedFill(0xfb, 0xe3, 0xe3);
const QColor kIssueFill(0xff, 0xf1, 0xc8);
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

// The Rows sheet; empty, it says how to add a row.
class RowsSheet : public QTableWidget {
public:
  RowsSheet() { setProperty(NativeEmptyTextProperty, QString::fromUtf8(kNoRows)); }

protected:
  void paintEvent(QPaintEvent* event) override {
    QTableWidget::paintEvent(event);
    if (rowCount() == 0) {
      paintPlaceholder(this);
    }
  }
};

// Add Option: an existing user parameter that is no option yet, or a new
// one (name, unit, value); the parameter and the option are one undo step.
class AddOptionDialog : public QDialog {
public:
  AddOptionDialog(DocumentHost& host, cfg::Table table, const QStringList& candidates, QWidget* parent)
      : QDialog(parent), m_host(host), m_table(std::move(table)) {
    setObjectName(QStringLiteral("addOptionDialog"));
    setWindowTitle(tr("Add Option"));
    auto* form = new QFormLayout(this);
    m_parameter = new QComboBox;
    m_parameter->setObjectName(QStringLiteral("addOptionParameter"));
    for (const QString& name : candidates) {
      m_parameter->addItem(name, name);
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
    QJsonArray commands;
    if (name.isEmpty()) {
      name = m_name->text().trimmed();
      const QString unit = m_unit->currentText().trimmed();
      const bool text = isTextUnit(unit);
      QString expression = cfg::rowExpression(text, text ? QString() : unit, m_value->text());
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
    }
    cfg::Table table = m_table;
    table.options.append(name);
    commands.append(cfg::setTableCommand(table));
    if (m_host.runModelCommands(commands, tr("Add Option %1").arg(name))) {
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
    auto* about = new QLabel(tr("A parameter that every row sets, as d, P, s and k of a screw size."));
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
      auto* closes = new QLabel(tr("The table becomes closed: only its rows' combinations are allowed."));
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

// Edit Domain: what an option can be before the rules, as its parameter's
// limits: a list (the `values` limit, one value per line), a range (min,
// max, step), a column of one of the design's tables (the `values` limit
// as `Table.Column`, mitcad#849: the column's distinct cells) or free. A
// text option is a list, a text column or free; a number option takes a
// number column.
class DomainDialog : public QDialog {
public:
  DomainDialog(DocumentHost& host, QString name, const QJsonObject& parameter, bool text,
               const std::vector<SuggestedTable>& tables, QWidget* parent)
      : QDialog(parent), m_host(host), m_name(std::move(name)), m_text(text) {
    setObjectName(QStringLiteral("optionDomainDialog"));
    setWindowTitle(tr("Option Domain"));
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
    layout->addWidget(new QLabel(tr("What %1 can be before the rules:").arg(m_name)));
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
    auto* about = new QLabel(tr("One row per value of the key column, named by it, that sets the option to it. "
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
    connect(m_columnChoice, &QComboBox::currentIndexChanged, this, [this] { chooseOption(); });
    qDebug().noquote() << QStringLiteral("Generate Rows dialog opened: %1 for %2")
                              .arg(cfg::columnReference(m_tableChoice->currentData().toString(),
                                                        m_columnChoice->currentData().toString()),
                                   m_optionChoice->currentData().toString());
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
    if (m_host.runModelCommand(cfg::generateRowsCommand(table, column, option), &result)) {
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
  QLabel* m_message = nullptr;
  bool m_done = false;
  cfg::GeneratedRows m_rows;
  QString m_tableName;
  QString m_reference;
  QString m_option;
};

void markRefused(QTableWidgetItem* item, const QString& typed, const QString& reason) {
  item->setText(typed);
  item->setForeground(kRefusedText);
  item->setBackground(kRefusedFill);
  item->setToolTip(reason);
}

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
  m_options->setColumnCount(7);
  m_options->setHeaderLabels(
      {tr("Option"), tr("Unit"), tr("Domain"), tr("Default"), tr("Comment"), tr("Driven"), tr("Used By")});
  m_options->headerItem()->setToolTip(kOptionUsedBy, tr("The bindings that follow the option; double-click to "
                                                        "list them in the Bindings tab"));
  m_options->headerItem()->setToolTip(kOptionDomain, tr("What the option can be before the rules: a list or a range"));
  m_options->headerItem()->setToolTip(
      kOptionDefault, tr("The option's own value: what the design has before a row or a chooser changes it"));
  m_options->headerItem()->setToolTip(kOptionDriven,
                                      tr("A driven option follows its formula: no chooser sets it, no rule moves it"));
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
  m_options->header()->resizeSection(kOptionDefault, 120);
  m_options->header()->setSectionResizeMode(kOptionComment, QHeaderView::Stretch);
  m_options->header()->setStretchLastSection(false);
  m_options->header()->resizeSection(kOptionDriven, 70);
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
  m_editDomain = new QPushButton(tr("Edit Domain…"));
  m_editDomain->setObjectName(QStringLiteral("configurationsEditDomain"));
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
    } else if ((column == kOptionDefault && item->text(kOptionDriven).isEmpty()) || column == kOptionComment) {
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
  m_closed = new QCheckBox(tr("Closed"));
  m_closed->setObjectName(QStringLiteral("configurationsClosed"));
  m_closed->setToolTip(tr("A closed table allows only its rows' combinations; an open one any the rules allow"));
  top->addWidget(m_closed);
  m_current = new QLabel;
  m_current->setObjectName(QStringLiteral("configurationsCurrentRow"));
  top->addWidget(m_current, 1);
  layout->addLayout(top);
  m_rows = new RowsSheet;
  m_rows->setObjectName(QStringLiteral("configurationsRows"));
  m_rows->setSelectionMode(QAbstractItemView::SingleSelection);
  m_rows->setSelectionBehavior(QAbstractItemView::SelectRows);
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
  m_defaultRow->setToolTip(tr("The row a library component is inserted with, and the cascade's fallback"));
  m_apply = new QPushButton(tr("Apply"));
  m_apply->setObjectName(QStringLiteral("configurationsApply"));
  m_apply->setToolTip(tr("Set the options to the selected row's values"));
  // A closed table's parameter columns (mitcad#999).
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
                              ? tr("Another parameter that every row sets, as d, P, s and k of a screw size")
                              : tr("Another parameter that every row sets; the table becomes closed"));
  m_removeColumn->setEnabled(!selectedColumn().isEmpty());
  m_removeColumn->setToolTip(m_table.parameters.isEmpty()
                                 ? tr("The table has no parameter columns")
                                 : tr("Take the current cell's parameter column out of the table; the parameter "
                                      "stays"));
  m_generateRows->setEnabled(!m_tables.empty());
  m_generateRows->setToolTip(m_tables.empty()
                                 ? tr("The design has no tables (Change Parameters, Tables)")
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
    if (report != nullptr && !report->restrictedBy.isEmpty()) {
      item->setToolTip(kOptionDomain, tr("Allowed now: %1 (%2)")
                                          .arg(report->allowed.summary(), report->restrictedBy.join(QStringLiteral(", "))));
    } else {
      item->setToolTip(kOptionDomain, tr("Double-click to edit the domain"));
    }
    const QString expression = parameter.value(QStringLiteral("expression")).toString();
    item->setText(kOptionDefault, cfg::rowText(isText(name), expression));
    item->setText(kOptionComment, parameter.value(QStringLiteral("comment")).toString());
    if (driven) {
      item->setText(kOptionDriven, tr("Driven"));
      item->setToolTip(kOptionDriven, tr("Driven by its formula %1: no chooser sets it, no rule moves it").arg(expression));
      item->setToolTip(kOptionDefault, tr("Driven by its formula"));
    }
    if (parameter.isEmpty()) {
      item->setForeground(kOptionName, kRefusedText);
      item->setToolTip(kOptionName, tr("%1 is no user parameter of the design").arg(name));
    }
    for (const int column : {kOptionDefault, kOptionComment}) {
      const auto refused = m_refused.constFind(
          QStringLiteral("%1|%2").arg(name, m_options->headerItem()->text(column).toLower()));
      if (refused != m_refused.cend()) {
        item->setText(column, refused.value().first);
        item->setForeground(column, kRefusedText);
        item->setBackground(column, kRefusedFill);
        item->setToolTip(column, refused.value().second);
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
    logged << QStringLiteral("%1 (%2) = %3%4")
                  .arg(name, domain.isEmpty() ? QStringLiteral("?") : domain, item->text(kOptionDefault),
                       driven ? QStringLiteral(", driven") : QString());
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
      if (issue != m_rowIssues.cend() &&
          ((column.kind == RowColumn::Option && named.contains(column.parameter)) ||
           (column.kind == RowColumn::Name && named.isEmpty()))) {
        item->setBackground(kIssueFill);
        item->setToolTip(tr("Breaks %1").arg(issue.value().describe()));
      }
      const auto refused = m_refused.constFind(QStringLiteral("%1|%2").arg(row.name, headers.at(c)));
      if (refused != m_refused.cend()) {
        markRefused(item, refused.value().first, refused.value().second);
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
    showMessage(tr("The change was cancelled."), false);
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
  QStringList candidates;
  for (const QString& name : std::as_const(m_userParameters)) {
    if (!m_table.options.contains(name)) {
      candidates << name;
    }
  }
  if (!m_host.canChangeModel()) {
    showMessage(m_host.changeBlockedReason(), true);
    return;
  }
  AddOptionDialog dialog(m_host, m_table, candidates, this);
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
  qDebug().noquote() << QStringLiteral("Option Domain dialog opened for %1").arg(name);
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
  if (m_building || item == nullptr || (column != kOptionDefault && column != kOptionComment)) {
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

void ConfigurationsDialog::addRow() {
  cfg::Table table = m_table;
  cfg::Row row;
  row.name = table.nextRowName();
  // The options' values as they are now, as constants; a closed table's
  // other parameters' expressions too.
  for (const QString& option : std::as_const(table.options)) {
    const cfg::OptionReport* report = m_report.option(option);
    const QString expression = report != nullptr ? report->expression
                                                 : m_parameters.value(option).value(QStringLiteral("expression")).toString();
    row.setValue(option, expression);
  }
  for (const QString& parameter : std::as_const(table.parameters)) {
    row.setValue(parameter, m_parameters.value(parameter).value(QStringLiteral("expression")).toString());
  }
  table.rows.append(row);
  m_selectRow = row.name;
  submitTable(table, QString(), row.name, QStringLiteral("added row"));
}

void ConfigurationsDialog::generateRows() {
  if (m_tables.empty()) {
    showMessage(tr("The design has no tables: add one in Change Parameters, Tables."), true);
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
    m_host.showStatus(text, false);
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
    m_host.showStatus(text, false);
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
      {"remove column", m_removeColumn}};
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
