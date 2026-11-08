// SPDX-License-Identifier: MIT
#include "TablesPage.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <iterator>
#include <optional>
#include <utility>

#include <QAbstractItemView>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QList>
#include <QListWidget>
#include <QPushButton>
#include <QResizeEvent>
#include <QScrollBar>
#include <QShowEvent>
#include <QTableWidget>
#include <QRegularExpression>
#include <QTimer>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/ParameterField.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "ConfigurationsData.hpp"
#include "DocumentHost.hpp"

namespace mitcad {

// The grid's header (mitcad#667): each column's name and unit in boxes of
// their own over its section, kept in place as sections are resized and
// the grid scrolls.
class ColumnHeader : public QHeaderView {
public:
  explicit ColumnHeader(QWidget* parent) : QHeaderView(Qt::Horizontal, parent) {
    setSectionsClickable(true);
    setDefaultSectionSize(130);
    setMinimumSectionSize(100);
    setDefaultAlignment(Qt::AlignLeft | Qt::AlignTop);
    connect(this, &QHeaderView::sectionResized, this, [this] { place(); });
    connect(this, &QHeaderView::geometriesChanged, this, [this] { place(); });
  }

  // The column's boxes, one widget per column; the old ones are deleted.
  void setEditors(const QList<QWidget*>& editors) {
    for (QWidget* old : std::as_const(m_editors)) {
      old->hide();
      old->deleteLater();
    }
    m_editors = editors;
    QVariantList nativeEditors;
    int height = 0;
    for (QWidget* editor : editors) {
      editor->setParent(viewport());
      editor->show();
      nativeEditors << QVariant::fromValue(static_cast<QObject*>(editor));
      height = std::max(height, editor->sizeHint().height());
    }
    setMinimumHeight(editors.isEmpty() ? 0 : height + 4);
    setProperty(NativeColumnEditorsProperty, nativeEditors);
    place();
  }

  // As tall as the boxes.
  QSize sizeHint() const override {
    QSize size = QHeaderView::sizeHint();
    size.setHeight(std::max(size.height(), minimumHeight()));
    return size;
  }

  QWidget* editor(int column) const {
    return column >= 0 && column < m_editors.size() ? m_editors.at(column) : nullptr;
  }

  void place() {
    for (int i = 0; i < static_cast<int>(m_editors.size()); ++i) {
      QWidget* editor = m_editors.at(i);
      if (i >= count()) {
        editor->hide();
        continue;
      }
      editor->setGeometry(sectionViewportPosition(i) + 2, 2, std::max(0, sectionSize(i) - 4),
                          std::max(0, viewport()->height() - 4));
    }
  }

protected:
  void resizeEvent(QResizeEvent* event) override {
    QHeaderView::resizeEvent(event);
    place();
  }
  void scrollContentsBy(int dx, int dy) override {
    QHeaderView::scrollContentsBy(dx, dy);
    place();
  }

private:
  QList<QWidget*> m_editors;
};

namespace {

// A column's unit box: a unit changes only by a choice in its open list
// (a new unit converts the column's numbers, one undo step each), never
// by the mouse wheel passing over the header or by arrow keys and letters
// while the list is closed. Alt+Down, F4 and Space open the list.
class UnitBox : public QComboBox {
public:
  UnitBox() { setFocusPolicy(Qt::StrongFocus); }

protected:
  void wheelEvent(QWheelEvent* event) override { event->ignore(); }
  void keyPressEvent(QKeyEvent* event) override {
    const int key = event->key();
    const bool alt = event->modifiers().testFlag(Qt::AltModifier);
    const bool steps = !alt && (key == Qt::Key_Up || key == Qt::Key_Down || key == Qt::Key_PageUp ||
                                key == Qt::Key_PageDown || key == Qt::Key_Home || key == Qt::Key_End);
    const bool letter = !event->text().isEmpty() && event->text().at(0).isLetterOrNumber();
    if (steps || letter) {
      event->ignore();
      return;
    }
    QComboBox::keyPressEvent(event);
  }
};

QColor refusedText() { return themeColor(ColorRole::ErrorText); }
QColor refusedFill() { return themeColor(ColorRole::ErrorFill); }

bool isText(const QString& unit) { return unit == QStringLiteral("Text"); }

// A number as a cell shows it: the shortest that gives it back.
QString numberText(double value) { return QString::number(value, 'g', 15); }

QString cellText(const QJsonValue& cell) {
  if (cell.isDouble()) {
    return numberText(cell.toDouble());
  }
  return cell.isString() ? cell.toString() : QString();
}

// A unit's kind and its size in the kind's base unit (mm, deg); a power
// as `mm^2`. Unknown units have none.
struct UnitScale {
  QString kind;
  double factor = 1.0;
  bool known = false;
};

UnitScale scaleOf(const QString& unit) {
  QString base = unit.trimmed();
  int power = 1;
  const qsizetype caret = base.indexOf(QLatin1Char('^'));
  if (caret > 0) {
    bool ok = false;
    power = base.mid(caret + 1).toInt(&ok);
    if (!ok || power < 1) {
      return {};
    }
    base = base.left(caret);
  }
  static const QHash<QString, double> lengths{
      {QStringLiteral("um"), 0.001},   {QStringLiteral("µm"), 0.001},    {QStringLiteral("micron"), 0.001},
      {QStringLiteral("mm"), 1.0},     {QStringLiteral("cm"), 10.0},     {QStringLiteral("m"), 1000.0},
      {QStringLiteral("km"), 1e6},     {QStringLiteral("in"), 25.4},     {QStringLiteral("inch"), 25.4},
      {QStringLiteral("ft"), 304.8},   {QStringLiteral("foot"), 304.8},  {QStringLiteral("yd"), 914.4},
      {QStringLiteral("mi"), 1609344.0}, {QStringLiteral("mil"), 0.0254}};
  static const QHash<QString, double> angles{{QStringLiteral("deg"), 1.0},
                                             {QStringLiteral("°"), 1.0},
                                             {QStringLiteral("rad"), 180.0 / 3.14159265358979323846},
                                             {QStringLiteral("grad"), 0.9}};
  UnitScale scale;
  if (lengths.contains(base)) {
    scale.kind = QStringLiteral("length^%1").arg(power);
    scale.factor = std::pow(lengths.value(base), power);
    scale.known = true;
  } else if (angles.contains(base) && power == 1) {
    scale.kind = QStringLiteral("angle");
    scale.factor = angles.value(base);
    scale.known = true;
  }
  return scale;
}

// A converted number rounded to 12 significant digits: mm -> in -> mm
// gives 7 again, not 6.99999999999999.
double rounded(double value) { return QString::number(value, 'g', 12).toDouble(); }

// What a typed cell is (docs/table-lookups.md, Storage): empty is null; a
// Text column's cell its text; a number column's cell a number in the
// column's unit ("8,5" too). A number with a unit of the column's kind is
// converted to the column's unit ("0.25 in" in an mm column is 6.35), as
// the CSV import reads it. Anything else goes to the model as typed, which
// refuses it with its reason.
QJsonValue cellValue(const QString& typed, const QString& unit) {
  const QString text = typed.trimmed();
  if (text.isEmpty()) {
    return QJsonValue(QJsonValue::Null);
  }
  if (isText(unit)) {
    return text;
  }
  if (const std::optional<double> number = parsePlainNumber(text)) {
    return *number;
  }
  static const QRegularExpression withUnit(QStringLiteral(R"(^([-+]?[0-9.,]+(?:[eE][-+]?[0-9]+)?)\s*(\S+)$)"));
  const QRegularExpressionMatch match = withUnit.match(text);
  if (match.hasMatch()) {
    const std::optional<double> number = parsePlainNumber(match.captured(1));
    const UnitScale typedScale = scaleOf(match.captured(2));
    const UnitScale columnScale = scaleOf(unit);
    if (number && match.captured(2) == unit) {
      return *number;
    }
    if (number && typedScale.known && columnScale.known && typedScale.kind == columnScale.kind) {
      return rounded(*number * typedScale.factor / columnScale.factor);
    }
  }
  return text;
}

// A cell when its column's unit changes (docs/table-lookups.md, UI): the
// value stays (25.4 mm is 1 in); between unitless and a unit, or between
// kinds, the number stays; into Text a number becomes its text, out of it
// a text that is a number becomes the number (others go to the model,
// which refuses them).
QJsonValue convertCell(const QJsonValue& cell, const QString& from, const QString& to) {
  if (cell.isNull() || cell.isUndefined()) {
    return QJsonValue(QJsonValue::Null);
  }
  if (isText(to)) {
    return cell.isDouble() ? QJsonValue(numberText(cell.toDouble())) : cell;
  }
  if (cell.isString()) {
    const std::optional<double> number = parsePlainNumber(cell.toString());
    return number ? QJsonValue(*number) : cell;
  }
  const UnitScale a = scaleOf(from);
  const UnitScale b = scaleOf(to);
  if (cell.isDouble() && a.known && b.known && a.kind == b.kind) {
    return rounded(cell.toDouble() * a.factor / b.factor);
  }
  return cell;
}

// The units a column takes in its box: lengths, angles, none, Text.
QList<QPair<QString, QString>> unitChoices() {
  return {{QStringLiteral("mm"), QStringLiteral("mm")},     {QStringLiteral("cm"), QStringLiteral("cm")},
          {QStringLiteral("m"), QStringLiteral("m")},       {QStringLiteral("in"), QStringLiteral("in")},
          {QStringLiteral("ft"), QStringLiteral("ft")},     {QStringLiteral("deg"), QStringLiteral("deg")},
          {QStringLiteral("rad"), QStringLiteral("rad")},   {TablesPage::tr("no unit"), QString()},
          {QStringLiteral("Text"), QStringLiteral("Text")}};
}

QString columnLabel(const QJsonObject& column) {
  return QStringLiteral("%1 [%2]").arg(column.value(QStringLiteral("name")).toString(),
                                       column.value(QStringLiteral("unit")).toString());
}

QString compact(const QJsonArray& array) {
  return QString::fromUtf8(QJsonDocument(array).toJson(QJsonDocument::Compact));
}

// A name the model takes for a table from a file's name: letters, digits
// and _, not starting with a digit ("ISO 4762" is ISO_4762).
QString nameFromFile(const QString& base) {
  QString name;
  for (const QChar c : base) {
    name += c.isLetterOrNumber() || c == QLatin1Char('_') ? c : QLatin1Char('_');
  }
  if (name.isEmpty() || !(name.front().isLetter() || name.front() == QLatin1Char('_'))) {
    name.prepend(QLatin1Char('T'));
  }
  return name;
}

// The parameters a command's `changed` names (strings, or objects with a
// `name`).
QStringList changedNames(const QJsonObject& result) {
  QStringList names;
  for (const QJsonValue& value : result.value(QStringLiteral("changed")).toArray()) {
    names << (value.isString() ? value.toString() : value.toObject().value(QStringLiteral("name")).toString());
  }
  return names;
}

// A problem of the `tables` query or an import's failing parameter, as a
// line: a text, or an object with a message.
QString problemText(const QJsonValue& value) {
  if (value.isString()) {
    return value.toString();
  }
  const QJsonObject object = value.toObject();
  for (const char* key : {"message", "error", "reason"}) {
    const QString text = object.value(QLatin1String(key)).toString();
    if (!text.isEmpty()) {
      return text;
    }
  }
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

// New Table and Rename Table: a name; stays open with the model's reason
// when the model refuses it.
class TableNameDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::TableNameDialog)

public:
  TableNameDialog(const QString& title, const QString& initial, std::function<QString(const QString&)> apply,
                  QWidget* parent)
      : QDialog(parent), m_apply(std::move(apply)) {
    setObjectName(QStringLiteral("tableNameDialog"));
    setWindowTitle(title);
    auto* form = new QFormLayout(this);
    m_name = new QLineEdit(initial);
    m_name->setObjectName(QStringLiteral("tableName"));
    m_name->selectAll();
    m_error = new QLabel;
    m_error->setObjectName(QStringLiteral("tableNameError"));
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { tryAccept(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    setMinimumWidth(360);
    m_name->setFocus();
  }

private:
  void tryAccept() {
    const QString name = m_name->text().trimmed();
    const QString problem = name.isEmpty() ? tr("Give the table a name.") : m_apply(name);
    if (problem.isEmpty()) {
      accept();
      return;
    }
    m_error->setText(problem);
  }

  std::function<QString(const QString&)> m_apply;
  QLineEdit* m_name = nullptr;
  QLabel* m_error = nullptr;
};

// Import Table: what importing a table CSV would do, as the model's
// `table_csv_preview` says: the table's name (the file's by default; the
// preview follows a new one), the columns with their units (renamed ones
// and units read from the cells said), the rows, refused cells in red, the
// parameters whose values change and those that would fail (which refuse
// the import). Import runs `import_table_csv`, Cancel leaves the design.
class TableImportDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::TableImportDialog)

public:
  TableImportDialog(DocumentHost& host, QString csv, QString file, const QString& name, QHash<QString, int> existing,
                    QWidget* parent)
      : QDialog(parent), m_host(host), m_csv(std::move(csv)), m_file(std::move(file)), m_existing(std::move(existing)) {
    setObjectName(QStringLiteral("tableImportPreview"));
    setWindowTitle(tr("Import Table"));
    resize(760, 440);
    auto* layout = new QVBoxLayout(this);
    auto* form = new QFormLayout;
    m_name = new QLineEdit(name);
    m_name->setObjectName(QStringLiteral("tableImportName"));
    m_name->setToolTip(tr("A table of this name is created, or replaced whole"));
    form->addRow(tr("Table"), m_name);
    layout->addLayout(form);
    m_summary = new QLabel;
    m_summary->setObjectName(QStringLiteral("tableImportSummary"));
    m_summary->setWordWrap(true);
    layout->addWidget(m_summary);
    m_rows = new QTableWidget;
    m_rows->setObjectName(QStringLiteral("tableImportRows"));
    m_rows->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_rows->setSelectionMode(QAbstractItemView::NoSelection);
    m_rows->setAlternatingRowColors(true);
    layout->addWidget(m_rows, 1);
    m_changes = new QLabel;
    m_changes->setObjectName(QStringLiteral("tableImportChanges"));
    m_changes->setWordWrap(true);
    m_changes->setTextFormat(Qt::PlainText);
    layout->addWidget(m_changes);
    m_failing = new QLabel;
    m_failing->setObjectName(QStringLiteral("tableImportFailing"));
    m_failing->setWordWrap(true);
    setErrorStyleSheet(m_failing);
    layout->addWidget(m_failing);
    auto* buttons = new QDialogButtonBox;
    m_import = new QPushButton(tr("Import"));
    m_import->setObjectName(QStringLiteral("tableImportAccept"));
    auto* cancel = new QPushButton(tr("Cancel"));
    cancel->setObjectName(QStringLiteral("tableImportCancel"));
    buttons->addButton(m_import, QDialogButtonBox::AcceptRole);
    buttons->addButton(cancel, QDialogButtonBox::RejectRole);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_name, &QLineEdit::editingFinished, this, [this] {
      if (m_name->text().trimmed() != m_previewed) {
        preview();
      }
    });
    preview();
  }

  QString name() const { return m_name->text().trimmed(); }

private:
  void preview() {
    m_previewed = name();
    QJsonObject answer;
    QString problem;
    try {
      answer = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("table_csv_preview")},
                                           {QStringLiteral("name"), m_previewed},
                                           {QStringLiteral("csv"), m_csv}});
    } catch (const std::exception& e) {
      problem = QString::fromUtf8(e.what());
    }
    m_rows->clear();
    m_rows->setRowCount(0);
    m_rows->setColumnCount(0);
    m_changes->clear();
    m_failing->clear();
    if (!problem.isEmpty()) {
      m_summary->setText(tr("%1: %2").arg(m_file, problem));
      m_import->setEnabled(false);
      qDebug().noquote() << QStringLiteral("Table import preview of %1 as %2 refused: %3")
                                .arg(m_file, m_previewed, problem);
      return;
    }
    const QString summary = answer.value(QStringLiteral("summary")).toString();
    // Whether the import makes a table or replaces one, said plainly.
    const QString target = m_existing.contains(m_previewed)
                               ? tr("Replaces the table %1 (%n row(s)).", nullptr, m_existing.value(m_previewed))
                                     .arg(m_previewed)
                               : tr("Makes a new table %1.").arg(m_previewed);
    m_summary->setText(tr("%1: %2 %3").arg(m_file, target, summary));
    qDebug().noquote() << QStringLiteral("Table import preview of %1 as %2: %3").arg(m_file, m_previewed, summary);

    const QJsonArray columns = answer.value(QStringLiteral("columns")).toArray();
    QStringList labels;
    QStringList names; // for the refused cells' places
    for (const QJsonValue& value : columns) {
      const QJsonObject column = value.toObject();
      const QString fileName = column.value(QStringLiteral("name")).toString();
      const QJsonValue renamed = column.value(QStringLiteral("renamed"));
      const QString shownName =
          renamed.isString() && !renamed.toString().isEmpty() ? renamed.toString() : fileName;
      const QString unit = column.value(QStringLiteral("unit")).toString();
      labels << QStringLiteral("%1 [%2]").arg(shownName, unit);
      names << shownName;
      QStringList notes;
      if (shownName != fileName) {
        notes << tr("%1 in the file").arg(fileName);
      } else if (renamed.toBool()) {
        notes << tr("renamed");
      }
      if (column.value(QStringLiteral("inferred")).toBool()) {
        notes << tr("unit read from the cells");
      }
      qDebug().noquote() << QStringLiteral("Table import column %1 [%2]%3")
                                .arg(shownName, unit,
                                     notes.isEmpty() ? QString()
                                                     : QStringLiteral(" (%1)").arg(notes.join(QStringLiteral(", "))));
      if (!notes.isEmpty()) {
        labels.last() += QStringLiteral("\n(%1)").arg(notes.join(QStringLiteral(", ")));
      }
    }
    const QJsonArray rows = answer.value(QStringLiteral("rows")).toArray();
    m_rows->setColumnCount(static_cast<int>(labels.size()));
    m_rows->setHorizontalHeaderLabels(labels);
    m_rows->setRowCount(static_cast<int>(rows.size()));
    for (int r = 0; r < static_cast<int>(rows.size()); ++r) {
      const QJsonArray cells = rows.at(r).toArray();
      for (int c = 0; c < static_cast<int>(cells.size()) && c < static_cast<int>(labels.size()); ++c) {
        m_rows->setItem(r, c, new QTableWidgetItem(cellText(cells.at(c))));
      }
    }
    qDebug().noquote() << QStringLiteral("Table import rows: %1").arg(compact(rows));
    // Refused cells: red with the reason, where the place is known; all
    // of them listed.
    QStringList lines;
    for (const QJsonValue& value : answer.value(QStringLiteral("refused")).toArray()) {
      const QJsonObject cell = value.toObject();
      const int line = cell.value(QStringLiteral("line")).toInt();
      const QJsonValue column = cell.value(QStringLiteral("column"));
      const QString columnName = column.isString() ? column.toString() : QString::number(column.toInt());
      const QString reason = cell.value(QStringLiteral("reason")).toString();
      lines << tr("Line %1, %2: %3").arg(line).arg(columnName, reason);
      qDebug().noquote() << QStringLiteral("Table import refused line %1 %2: %3").arg(line).arg(columnName, reason);
      const int r = line - 2; // the header is line 1
      const int c = static_cast<int>(names.indexOf(columnName));
      if (r >= 0 && r < m_rows->rowCount() && c >= 0) {
        QTableWidgetItem* item = m_rows->item(r, c);
        if (item == nullptr) {
          item = new QTableWidgetItem;
          m_rows->setItem(r, c, item);
        }
        item->setForeground(refusedText());
        item->setBackground(refusedFill());
        item->setToolTip(reason);
      }
    }
    QStringList changes;
    for (const QJsonValue& value : answer.value(QStringLiteral("changed")).toArray()) {
      const QJsonObject parameter = value.toObject();
      const QString who = parameter.value(QStringLiteral("name")).toString();
      const QString from = parameter.value(QStringLiteral("from")).toVariant().toString();
      const QString to = parameter.value(QStringLiteral("to")).toVariant().toString();
      changes << tr("%1: %2 → %3").arg(who, from, to);
      qDebug().noquote() << QStringLiteral("Table import changes %1: %2 -> %3").arg(who, from, to);
    }
    QStringList failing;
    for (const QJsonValue& value : answer.value(QStringLiteral("failing")).toArray()) {
      const QJsonObject parameter = value.toObject();
      const QString who = parameter.value(QStringLiteral("name")).toString();
      const QString error = problemText(parameter);
      failing << tr("%1 would fail: %2").arg(who, error);
      qDebug().noquote() << QStringLiteral("Table import fails %1: %2").arg(who, error);
    }
    // Refused cells and failing parameters refuse the import (commands.md,
    // "Tables as CSV"): said in red, the import disabled.
    QStringList refusing = lines;
    refusing << failing;
    m_changes->setText(changes.isEmpty() ? tr("No parameter's value changes.")
                                         : tr("Parameters that change: %1").arg(changes.join(QStringLiteral("; "))));
    const bool unchanged = answer.value(QStringLiteral("unchanged")).toBool();
    m_failing->setText(refusing.isEmpty() ? QString()
                                          : refusing.join(QLatin1Char('\n')) + QLatin1Char('\n') +
                                                tr("Correct the file or the parameters to import it."));
    m_failing->setVisible(!refusing.isEmpty());
    const bool importable = refusing.isEmpty() && !unchanged;
    m_import->setEnabled(importable);
    m_import->setDefault(importable);
    m_import->setToolTip(unchanged ? tr("The table is already as the file has it") : QString());
  }

  DocumentHost& m_host;
  QString m_csv;
  QString m_file;
  QString m_previewed;
  QHash<QString, int> m_existing;
  QLineEdit* m_name = nullptr;
  QLabel* m_summary = nullptr;
  QTableWidget* m_rows = nullptr;
  QLabel* m_changes = nullptr;
  QLabel* m_failing = nullptr;
  QPushButton* m_import = nullptr;
};

QPushButton* button(const QString& text, const char* name, const QString& tip) {
  auto* b = new QPushButton(text);
  b->setObjectName(QLatin1String(name));
  b->setToolTip(tip);
  b->setAutoDefault(false);
  return b;
}

QString cellKey(const QString& table, int row, int column) {
  return QStringLiteral("%1|%2|%3").arg(table).arg(row).arg(column);
}

QString nameKey(const QString& table, int column) { return QStringLiteral("%1|name|%2").arg(table).arg(column); }

} // namespace

TablesPage::TablesPage(DocumentHost& host, QWidget* parent) : QWidget(parent), m_host(host) {
  setObjectName(QStringLiteral("tablesPage"));
  auto* layout = new QHBoxLayout(this);

  // The tables: New, Rename and Delete above the list and Import and
  // Export below it, as the Parameters tab has its buttons.
  auto* left = new QVBoxLayout;
  m_new = button(tr("&New"), "tablesNew", tr("Make a new table"));
  m_new->setIcon(themeIcon(QStringLiteral("add")));
  m_rename = button(tr("&Rename"), "tablesRename", tr("Rename the table; the formulas that use it follow"));
  m_delete = button(tr("&Delete"), "tablesDelete", tr("Delete the table (when no parameter uses it)"));
  m_delete->setIcon(themeIcon(QStringLiteral("delete")));
  m_import = button(tr("&Import CSV…"), "tablesImport", tr("Make a table from a CSV file, or replace one"));
  m_export = button(tr("E&xport CSV…"), "tablesExport", tr("Write the table to a CSV file"));
  auto* tableButtons = new QHBoxLayout;
  tableButtons->addWidget(m_new);
  tableButtons->addWidget(m_delete);
  tableButtons->addWidget(m_rename);
  tableButtons->addStretch(1);
  left->addLayout(tableButtons);
  m_list = new QListWidget;
  m_list->setObjectName(QStringLiteral("tablesList"));
  m_list->setAccessibleName(tr("Tables"));
  m_list->setMinimumWidth(170);
  m_list->setMaximumWidth(260);
  left->addWidget(m_list, 1);
  auto* fileButtons = new QHBoxLayout;
  fileButtons->addWidget(m_import);
  fileButtons->addWidget(m_export);
  fileButtons->addStretch(1);
  left->addLayout(fileButtons);
  layout->addLayout(left);
  mac::styleListButtons(m_new, m_delete);

  // The selected table.
  auto* right = new QVBoxLayout;
  m_empty = new QLabel(tr("The design has no tables. A formula looks a value up in one, such as "
                          "XLOOKUP(Size; Screws.Size; Screws.dk)."));
  m_empty->setObjectName(QStringLiteral("tablesEmpty"));
  m_empty->setWordWrap(true);
  m_empty->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  right->addWidget(m_empty, 1);
  m_editor = new QWidget;
  auto* editor = new QVBoxLayout(m_editor);
  editor->setContentsMargins(0, 0, 0, 0);
  auto* commentRow = new QHBoxLayout;
  auto* commentLabel = new QLabel(tr("C&omment"));
  m_comment = new QLineEdit;
  m_comment->setObjectName(QStringLiteral("tablesComment"));
  m_comment->installEventFilter(this); // Enter sets the comment and stays in the dialog
  m_comment->setPlaceholderText(tr("What the table holds, such as its standard"));
  commentLabel->setBuddy(m_comment);
  commentRow->addWidget(commentLabel);
  commentRow->addWidget(m_comment, 1);
  editor->addLayout(commentRow);
  auto* structure = new QHBoxLayout;
  m_addRow = button(tr("Add Ro&w"), "tablesAddRow", tr("A new row after the current one"));
  m_removeRow = button(tr("Remo&ve Row"), "tablesRemoveRow", tr("Remove the current row"));
  m_rowUp = button(QStringLiteral("▲"), "tablesRowUp", tr("Move the current row up"));
  m_rowDown = button(QStringLiteral("▼"), "tablesRowDown", tr("Move the current row down"));
  m_addColumn = button(tr("Add Co&lumn"), "tablesAddColumn", tr("A new column after the current one"));
  m_removeColumn = button(tr("Re&move Column"), "tablesRemoveColumn",
                          tr("Remove the current column (when no parameter uses it)"));
  m_columnLeft = button(QStringLiteral("◀"), "tablesColumnLeft", tr("Move the current column left"));
  m_columnRight = button(QStringLiteral("▶"), "tablesColumnRight", tr("Move the current column right"));
  m_rowUp->setAccessibleName(tr("Move row up"));
  m_rowDown->setAccessibleName(tr("Move row down"));
  m_columnLeft->setAccessibleName(tr("Move column left"));
  m_columnRight->setAccessibleName(tr("Move column right"));
  for (QPushButton* b : {m_addRow, m_removeRow, m_rowUp, m_rowDown}) {
    structure->addWidget(b);
  }
  structure->addSpacing(16);
  for (QPushButton* b : {m_addColumn, m_removeColumn, m_columnLeft, m_columnRight}) {
    structure->addWidget(b);
  }
  structure->addStretch(1);
  editor->addLayout(structure);
  m_grid = new QTableWidget;
  m_grid->setObjectName(QStringLiteral("tablesGrid"));
  m_comment->setProperty(NativeReturnTargetProperty, QVariant::fromValue(static_cast<QObject*>(m_grid)));
  m_header = new ColumnHeader(m_grid);
  m_grid->setHorizontalHeader(m_header);
  m_grid->setEditTriggers(QAbstractItemView::DoubleClicked | QAbstractItemView::EditKeyPressed |
                          QAbstractItemView::AnyKeyPressed);
  m_grid->setSelectionMode(QAbstractItemView::SingleSelection);
  m_grid->setAlternatingRowColors(true);
  m_grid->installEventFilter(this);
  editor->addWidget(m_grid, 1);
  m_usedBy = new QLabel;
  m_usedBy->setObjectName(QStringLiteral("tablesUsedBy"));
  m_usedBy->setWordWrap(true);
  editor->addWidget(m_usedBy);
  m_problems = new QLabel;
  m_problems->setObjectName(QStringLiteral("tablesProblems"));
  m_problems->setWordWrap(true);
  setErrorStyleSheet(m_problems);
  editor->addWidget(m_problems);
  right->addWidget(m_editor, 1);
  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("tablesMessage"));
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  right->addWidget(m_message);
  layout->addLayout(right, 1);

  connect(m_list, &QListWidget::currentTextChanged, this, [this](const QString& name) {
    if (m_building || name.isEmpty() || name == m_current) {
      return;
    }
    m_current = name;
    m_row = 0;
    m_column = 0;
    m_message->clear();
    showTable();
  });
  connect(m_new, &QPushButton::clicked, this, &TablesPage::newTable);
  connect(m_rename, &QPushButton::clicked, this, &TablesPage::renameTable);
  connect(m_delete, &QPushButton::clicked, this, &TablesPage::deleteTable);
  connect(m_import, &QPushButton::clicked, this, &TablesPage::importCsv);
  connect(m_export, &QPushButton::clicked, this, &TablesPage::exportCsv);
  connect(m_comment, &QLineEdit::editingFinished, this, &TablesPage::commentEdited);
  connect(m_addRow, &QPushButton::clicked, this, &TablesPage::addRow);
  connect(m_removeRow, &QPushButton::clicked, this, &TablesPage::removeRow);
  connect(m_rowUp, &QPushButton::clicked, this, [this] { moveRow(-1); });
  connect(m_rowDown, &QPushButton::clicked, this, [this] { moveRow(1); });
  connect(m_addColumn, &QPushButton::clicked, this, &TablesPage::addColumn);
  connect(m_removeColumn, &QPushButton::clicked, this, &TablesPage::removeColumn);
  connect(m_columnLeft, &QPushButton::clicked, this, [this] { moveColumn(-1); });
  connect(m_columnRight, &QPushButton::clicked, this, [this] { moveColumn(1); });
  connect(m_grid, &QTableWidget::itemChanged, this, &TablesPage::cellEdited);
  connect(m_grid, &QTableWidget::currentCellChanged, this, [this](int row, int column) {
    if (!m_building && row >= 0 && column >= 0) {
      m_row = row;
      m_column = column;
      updateButtons();
    }
  });
  connect(m_grid->horizontalScrollBar(), &QScrollBar::valueChanged, m_header, [this] { m_header->place(); });
  connect(m_grid->verticalScrollBar(), &QScrollBar::valueChanged, this,
          [this] { QTimer::singleShot(0, this, &TablesPage::logLayout); });
}

QJsonObject TablesPage::table() const {
  for (const QJsonValue& value : m_tables) {
    if (value.toObject().value(QStringLiteral("name")).toString() == m_current) {
      return value.toObject();
    }
  }
  return {};
}

QJsonArray TablesPage::columns() const { return table().value(QStringLiteral("columns")).toArray(); }

QJsonArray TablesPage::rows() const { return table().value(QStringLiteral("rows")).toArray(); }

void TablesPage::refresh() {
  QJsonValue answer;
  try {
    answer = m_host.model().query({{QStringLiteral("query"), QStringLiteral("tables")}});
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
  }
  // The answer's `tables` (a bare list taken as well).
  m_tables = answer.isArray() ? answer.toArray() : answer.toObject().value(QStringLiteral("tables")).toArray();
  QStringList names;
  QStringList logged;
  for (const QJsonValue& value : m_tables) {
    const QJsonObject t = value.toObject();
    names << t.value(QStringLiteral("name")).toString();
    logged << QStringLiteral("%1 (%2 columns, %3 rows)")
                  .arg(names.last())
                  .arg(t.value(QStringLiteral("columns")).toArray().size())
                  .arg(t.value(QStringLiteral("rows")).toArray().size());
  }
  if (!names.contains(m_current)) {
    m_current = names.isEmpty() ? QString() : names.first();
    m_row = 0;
    m_column = 0;
  }
  // Refusals of tables that are gone are forgotten.
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = names.contains(it.key().section(QLatin1Char('|'), 0, 0)) ? std::next(it) : m_refused.erase(it);
  }
  m_building = true;
  m_list->clear();
  for (const QJsonValue& value : m_tables) {
    const QJsonObject t = value.toObject();
    auto* item = new QListWidgetItem(t.value(QStringLiteral("name")).toString(), m_list);
    item->setToolTip(t.value(QStringLiteral("comment")).toString());
    if (item->text() == m_current) {
      m_list->setCurrentItem(item);
    }
  }
  m_building = false;
  const QString line = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", "));
  if (isVisible() && line != m_loggedTables) {
    m_loggedTables = line;
    qDebug().noquote() << QStringLiteral("Tables: %1").arg(line);
  }
  showTable();
}

void TablesPage::buildHeader(const QJsonArray& columns) {
  QList<QWidget*> editors;
  const QList<QPair<QString, QString>> units = unitChoices();
  for (int i = 0; i < static_cast<int>(columns.size()); ++i) {
    const QJsonObject column = columns.at(i).toObject();
    const QString name = column.value(QStringLiteral("name")).toString();
    const QString unit = column.value(QStringLiteral("unit")).toString();
    auto* box = new QWidget;
    box->setAutoFillBackground(true);
    auto* stack = new QVBoxLayout(box);
    stack->setContentsMargins(0, 0, 0, 0);
    stack->setSpacing(2);
    auto* nameEdit = new QLineEdit(name);
    nameEdit->setObjectName(QStringLiteral("tablesColumnName_%1").arg(i + 1));
    nameEdit->setProperty(NativeReturnTargetProperty, QVariant::fromValue(static_cast<QObject*>(m_grid)));
    nameEdit->setAccessibleName(tr("Column %1 name").arg(i + 1));
    nameEdit->setToolTip(tr("Formulas use the column as %1.%2").arg(m_current, name));
    const auto refused = m_refused.constFind(nameKey(m_current, i));
    if (refused != m_refused.cend()) {
      nameEdit->setText(refused.value().first);
      setErrorStyleSheet(nameEdit);
      nameEdit->setToolTip(refused.value().second);
    }
    auto* unitBox = new UnitBox;
    unitBox->setObjectName(QStringLiteral("tablesColumnUnit_%1").arg(i + 1));
    unitBox->setAccessibleName(tr("Column %1 unit").arg(i + 1));
    unitBox->setToolTip(tr("Lengths, angles, numbers without a unit, or Text; a new unit converts the numbers"));
    bool found = false;
    for (const auto& choice : units) {
      unitBox->addItem(choice.first, choice.second);
      found = found || choice.second == unit;
    }
    if (!found) {
      unitBox->insertItem(0, unit, unit); // a unit read from a file (mm^2)
    }
    unitBox->setCurrentIndex(unitBox->findData(unit));
    nameEdit->installEventFilter(this); // Enter renames and stays in the dialog
    stack->addWidget(nameEdit);
    stack->addWidget(unitBox);
    connect(nameEdit, &QLineEdit::editingFinished, this, [this, i, nameEdit] {
      const QString text = nameEdit->text().trimmed();
      QTimer::singleShot(0, this, [this, i, text] { renameColumn(i, text); });
    });
    connect(unitBox, &QComboBox::activated, this, [this, i, unitBox](int index) {
      const QString chosen = unitBox->itemData(index).toString();
      QTimer::singleShot(0, this, [this, i, chosen] { changeUnit(i, chosen); });
    });
    editors << box;
  }
  m_header->setEditors(editors);
  // Tab goes from the comment through the columns' names and units to the
  // grid (Shift+Tab from the grid back to the last column's unit).
  QWidget* previous = m_comment;
  for (QWidget* box : std::as_const(editors)) {
    for (QWidget* field : {static_cast<QWidget*>(box->findChild<QLineEdit*>()),
                           static_cast<QWidget*>(box->findChild<QComboBox*>())}) {
      if (field != nullptr) {
        QWidget::setTabOrder(previous, field);
        previous = field;
      }
    }
  }
  QWidget::setTabOrder(previous, m_grid);
}

void TablesPage::showTable() {
  const QJsonObject t = table();
  const bool any = !t.isEmpty();
  m_empty->setVisible(m_tables.isEmpty());
  m_editor->setVisible(any);
  const QJsonArray cols = t.value(QStringLiteral("columns")).toArray();
  const QJsonArray rs = t.value(QStringLiteral("rows")).toArray();
  const bool focused = m_grid->hasFocus();
  m_building = true;
  m_comment->setText(t.value(QStringLiteral("comment")).toString());
  m_grid->clear();
  m_grid->setColumnCount(static_cast<int>(cols.size()));
  m_grid->setRowCount(static_cast<int>(rs.size()));
  QStringList labels;
  for (const QJsonValue& value : cols) {
    labels << columnLabel(value.toObject());
  }
  m_grid->setHorizontalHeaderLabels(labels);
  QStringList refusedCells;
  for (int r = 0; r < static_cast<int>(rs.size()); ++r) {
    const QJsonArray cells = rs.at(r).toArray();
    for (int c = 0; c < static_cast<int>(cols.size()); ++c) {
      const QString unit = cols.at(c).toObject().value(QStringLiteral("unit")).toString();
      auto* item = new QTableWidgetItem(cellText(c < cells.size() ? cells.at(c) : QJsonValue()));
      item->setData(NativeControlIdentifierRole, QStringLiteral("table.%1.%2.%3").arg(m_current).arg(r).arg(c));
      item->setTextAlignment(isText(unit) ? Qt::AlignLeft | Qt::AlignVCenter : Qt::AlignRight | Qt::AlignVCenter);
      const auto refused = m_refused.constFind(cellKey(m_current, r, c));
      if (refused != m_refused.cend()) {
        // What the model refused stays in the cell with the reason.
        item->setText(refused.value().first);
        item->setForeground(refusedText());
        item->setBackground(refusedFill());
        item->setToolTip(refused.value().second);
        item->setData(NativeRefusedRole, true);
        refusedCells << QStringLiteral("row %1 %2: %3 (%4)")
                            .arg(r + 1)
                            .arg(cols.at(c).toObject().value(QStringLiteral("name")).toString(),
                                 refused.value().first, refused.value().second);
      }
      m_grid->setItem(r, c, item);
    }
  }
  buildHeader(cols);
  if (rs.size() > 0 && cols.size() > 0) {
    m_row = std::clamp(m_row, 0, static_cast<int>(rs.size()) - 1);
    m_column = std::clamp(m_column, 0, static_cast<int>(cols.size()) - 1);
    m_grid->setCurrentCell(m_row, m_column);
  }
  m_building = false;
  if ((focused || m_focusGrid) && isVisible()) {
    m_focusGrid = false;
    m_grid->setFocus();
  }

  QStringList users;
  for (const QJsonValue& value : t.value(QStringLiteral("used_by")).toArray()) {
    users << value.toString();
  }
  m_usedBy->setText(users.isEmpty() ? tr("Used by: no parameter yet")
                                    : tr("Used by: %1").arg(users.join(QStringLiteral(", "))));
  QStringList problems;
  for (const QJsonValue& value : t.value(QStringLiteral("problems")).toArray()) {
    problems << problemText(value);
  }
  m_problems->setText(problems.join(QLatin1Char('\n')));
  m_problems->setVisible(!problems.isEmpty());
  updateButtons();

  if (isVisible() && any) {
    const QString columnsLine = QStringLiteral("Table %1 columns: %2").arg(m_current, labels.join(QStringLiteral(", ")));
    if (columnsLine != m_loggedColumns) {
      m_loggedColumns = columnsLine;
      qDebug().noquote() << columnsLine;
    }
    const QString rowsLine = QStringLiteral("Table %1 rows: %2").arg(m_current, compact(rs));
    if (rowsLine != m_loggedRows) {
      m_loggedRows = rowsLine;
      qDebug().noquote() << rowsLine;
    }
    const QString usedLine = QStringLiteral("Table %1 used by: %2")
                                 .arg(m_current, users.isEmpty() ? QStringLiteral("none")
                                                                 : users.join(QStringLiteral(", ")));
    if (usedLine != m_loggedUsedBy) {
      m_loggedUsedBy = usedLine;
      qDebug().noquote() << usedLine;
    }
    const QString refusedLine = QStringLiteral("Table %1 refused cells: %2")
                                    .arg(m_current, refusedCells.isEmpty() ? QStringLiteral("none")
                                                                           : refusedCells.join(QStringLiteral("; ")));
    if (refusedLine != m_loggedRefused && (!refusedCells.isEmpty() || !m_loggedRefused.isEmpty())) {
      m_loggedRefused = refusedLine;
      qDebug().noquote() << refusedLine;
    }
  }
  QTimer::singleShot(0, this, &TablesPage::logLayout);
}

void TablesPage::updateButtons() {
  const QJsonObject t = table();
  const bool any = !t.isEmpty();
  const int rowCount = static_cast<int>(t.value(QStringLiteral("rows")).toArray().size());
  const int columnCount = static_cast<int>(t.value(QStringLiteral("columns")).toArray().size());
  m_rename->setEnabled(any);
  m_delete->setEnabled(any);
  m_export->setEnabled(any);
  m_addRow->setEnabled(any && columnCount > 0);
  m_removeRow->setEnabled(any && rowCount > 0);
  m_rowUp->setEnabled(any && m_row > 0 && m_row < rowCount);
  m_rowDown->setEnabled(any && m_row + 1 < rowCount);
  m_addColumn->setEnabled(any);
  m_removeColumn->setEnabled(any && columnCount > 1);
  m_columnLeft->setEnabled(any && m_column > 0 && m_column < columnCount);
  m_columnRight->setEnabled(any && m_column + 1 < columnCount);
}

bool TablesPage::run(const QJsonObject& command, QJsonObject* result, QString* problem) {
  // Outside sketches and panels with the model's answer (the parameters
  // that changed); in them as the parameters' own commands run.
  const bool done =
      m_host.parameterScope().isEmpty() ? m_host.runModelCommand(command, result) : m_host.runParameterCommand(command);
  if (!done) {
    *problem = m_host.lastCancelled() ? tr("the change was canceled") : m_host.lastError();
  }
  return done;
}

void TablesPage::submitTable(const QJsonArray& columns, const QJsonArray& rows, const QString& comment,
                             const QString& key, const QString& typed, const QString& what, const QString& shown,
                             const QString& done) {
  const QString name = m_current;
  const QJsonObject command{
      {QStringLiteral("cmd"), QStringLiteral("set_table")},
      {QStringLiteral("name"), name},
      {QStringLiteral("table"), QJsonObject{{QStringLiteral("comment"), comment},
                                            {QStringLiteral("columns"), columns},
                                            {QStringLiteral("rows"), rows}}}};
  // The command refreshes the window, the grid too: not while the grid is
  // still closing its editor.
  QTimer::singleShot(0, this, [this, name, command, key, typed, what, shown, done] {
    QJsonObject result;
    QString problem;
    if (run(command, &result, &problem)) {
      if (!key.isEmpty()) {
        m_refused.remove(key);
      }
      m_message->clear();
      qDebug().noquote() << QStringLiteral("Table %1 %2: %3").arg(name, what, typed);
      if (!done.isEmpty()) {
        m_host.showStatus(done, NoticeLevel::Information);
        qDebug().noquote() << QStringLiteral("Table %1: %2").arg(name, done);
      }
      if (result.contains(QStringLiteral("changed"))) {
        const QStringList parameters = changedNames(result);
        qDebug().noquote() << QStringLiteral("Table %1 changed parameters: %2")
                                  .arg(name, parameters.isEmpty() ? QStringLiteral("none")
                                                               : parameters.join(QStringLiteral(", ")));
      }
      // Configuration options the edit moved off removed values (mitcad#806).
      const QString moves =
          configurations::movesNotice(configurations::readMoves(result.value(QStringLiteral("moves"))));
      if (!moves.isEmpty()) {
        m_host.showStatus(moves, NoticeLevel::Information);
        qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(moves);
      }
    } else {
      if (!key.isEmpty() && !m_host.lastCancelled()) {
        m_refused.insert(key, {typed, problem});
      }
      m_message->setText(tr("%1, %2: %3").arg(name, shown.isEmpty() ? what : shown, problem));
      qDebug().noquote() << QStringLiteral("Table %1 %2: %3 refused: %4").arg(name, what, typed, problem);
    }
    emit changed(); // the dialog shows the tables and the parameters again
  });
}

void TablesPage::cellEdited(QTableWidgetItem* item) {
  if (m_building || item == nullptr) {
    return;
  }
  const int row = item->row();
  const int column = item->column();
  const QJsonArray cols = columns();
  QJsonArray rs = rows();
  if (column < 0 || column >= cols.size() || row < 0 || row >= rs.size()) {
    return;
  }
  const QJsonObject columnOf = cols.at(column).toObject();
  const QString typed = item->text().trimmed();
  QJsonArray cells = rs.at(row).toArray();
  while (cells.size() < cols.size()) {
    cells.append(QJsonValue(QJsonValue::Null));
  }
  const QJsonValue value = cellValue(typed, columnOf.value(QStringLiteral("unit")).toString());
  const QString key = cellKey(m_current, row, column);
  m_row = row;
  m_column = column;
  if (cells.at(column) == value) {
    // As it is: a refusal shown there is taken back.
    if (m_refused.remove(key)) {
      QTimer::singleShot(0, this, &TablesPage::showTable);
    }
    return;
  }
  cells[column] = value;
  rs[row] = cells;
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), key, typed,
              QStringLiteral("row %1 %2").arg(row + 1).arg(columnOf.value(QStringLiteral("name")).toString()),
              tr("row %1, column %2").arg(row + 1).arg(columnOf.value(QStringLiteral("name")).toString()));
}

void TablesPage::commentEdited() {
  const QJsonObject t = table();
  if (t.isEmpty() || m_comment->text() == t.value(QStringLiteral("comment")).toString()) {
    return;
  }
  submitTable(columns(), rows(), m_comment->text(), QString(), m_comment->text(), QStringLiteral("comment"));
}

void TablesPage::renameColumn(int column, const QString& name) {
  const QJsonArray cols = columns();
  if (column < 0 || column >= cols.size()) {
    return;
  }
  const QString table = m_current;
  const QString old = cols.at(column).toObject().value(QStringLiteral("name")).toString();
  const QString key = nameKey(table, column);
  if (name == old) {
    if (m_refused.remove(key)) {
      showTable();
    }
    return;
  }
  QJsonObject result;
  QString problem;
  if (run({{QStringLiteral("cmd"), QStringLiteral("rename_table_column")},
           {QStringLiteral("table"), table},
           {QStringLiteral("name"), old},
           {QStringLiteral("new_name"), name}},
          &result, &problem)) {
    m_refused.remove(key);
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Table %1 column %2 renamed: %3 -> %4").arg(table).arg(column + 1).arg(old, name);
  } else {
    if (!m_host.lastCancelled()) {
      m_refused.insert(key, {name, problem});
    }
    m_message->setText(tr("%1, column %2: %3").arg(table, old, problem));
    qDebug().noquote() << QStringLiteral("Table %1 column %2 name: %3 refused: %4")
                              .arg(table)
                              .arg(column + 1)
                              .arg(name, problem);
  }
  emit changed(); // the dialog shows the tables and the parameters again
  m_grid->setFocus();
}

void TablesPage::changeUnit(int column, const QString& unit) {
  QJsonArray cols = columns();
  if (column < 0 || column >= cols.size()) {
    return;
  }
  QJsonObject changedColumn = cols.at(column).toObject();
  const QString from = changedColumn.value(QStringLiteral("unit")).toString();
  if (from == unit) {
    return;
  }
  const QString name = changedColumn.value(QStringLiteral("name")).toString();
  changedColumn.insert(QStringLiteral("unit"), unit);
  cols[column] = changedColumn;
  QJsonArray rs = rows();
  QString example; // the first number converted, for the message
  for (int r = 0; r < static_cast<int>(rs.size()); ++r) {
    QJsonArray cells = rs.at(r).toArray();
    if (column < cells.size()) {
      const QJsonValue before = cells.at(column);
      const QJsonValue after = convertCell(before, from, unit);
      if (example.isEmpty() && before.isDouble() && after.isDouble() && before != after) {
        example = QStringLiteral(" (%1 → %2)").arg(numberText(before.toDouble()), numberText(after.toDouble()));
      }
      cells[column] = after;
      rs[r] = cells;
    }
  }
  const auto unitName = [](const QString& u) { return u.isEmpty() ? tr("no unit") : u; };
  // A conversion is said, as it changes the numbers shown.
  const QString done = example.isEmpty()
                           ? tr("%1: %2 → %3, the numbers kept.").arg(name, unitName(from), unitName(unit))
                           : tr("%1: %2 → %3, the numbers converted%4. Undo brings the old numbers back.")
                                 .arg(name, unitName(from), unitName(unit), example);
  m_focusGrid = true; // the unit box had the keys
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), QString(),
              QStringLiteral("%1 -> %2").arg(from, unit), QStringLiteral("column %1 unit").arg(name),
              tr("the unit of column %1").arg(name), done);
}

void TablesPage::addRow() {
  const QJsonArray cols = columns();
  QJsonArray rs = rows();
  if (table().isEmpty() || cols.isEmpty()) {
    return;
  }
  QJsonArray cells;
  for (int c = 0; c < static_cast<int>(cols.size()); ++c) {
    cells.append(QJsonValue(QJsonValue::Null));
  }
  const int at = rs.isEmpty() ? 0 : std::clamp(m_row + 1, 0, static_cast<int>(rs.size()));
  rs.insert(at, cells);
  m_row = at;
  // The rows below move: their refusals are forgotten.
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), QString(), QString::number(at + 1),
              QStringLiteral("row added"));
}

void TablesPage::removeRow() {
  QJsonArray rs = rows();
  if (m_row < 0 || m_row >= rs.size()) {
    return;
  }
  const int removed = m_row;
  rs.removeAt(removed);
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(columns(), rs, table().value(QStringLiteral("comment")).toString(), QString(),
              QString::number(removed + 1), QStringLiteral("row removed"));
}

void TablesPage::moveRow(int by) {
  QJsonArray rs = rows();
  const int to = m_row + by;
  if (m_row < 0 || m_row >= rs.size() || to < 0 || to >= rs.size()) {
    return;
  }
  const QJsonValue moved = rs.at(m_row);
  rs[m_row] = rs.at(to);
  rs[to] = moved;
  const int from = m_row;
  m_row = to;
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(columns(), rs, table().value(QStringLiteral("comment")).toString(), QString(),
              QStringLiteral("%1 -> %2").arg(from + 1).arg(to + 1), QStringLiteral("row moved"));
}

void TablesPage::addColumn() {
  if (table().isEmpty()) {
    return;
  }
  QJsonArray cols = columns();
  QStringList names;
  for (const QJsonValue& value : cols) {
    names << value.toObject().value(QStringLiteral("name")).toString();
  }
  QString name;
  for (int n = static_cast<int>(cols.size()) + 1;; ++n) {
    name = QStringLiteral("Column%1").arg(n);
    if (!names.contains(name)) {
      break;
    }
  }
  const ParameterSource* source = parameterSourceOf(this);
  const QString unit = source != nullptr && source->lengthUnit ? source->lengthUnit() : QStringLiteral("mm");
  const int at = cols.isEmpty() ? 0 : std::clamp(m_column + 1, 0, static_cast<int>(cols.size()));
  cols.insert(at, QJsonObject{{QStringLiteral("name"), name}, {QStringLiteral("unit"), unit}});
  QJsonArray rs = rows();
  for (int r = 0; r < static_cast<int>(rs.size()); ++r) {
    QJsonArray cells = rs.at(r).toArray();
    while (cells.size() < at) {
      cells.append(QJsonValue(QJsonValue::Null));
    }
    cells.insert(at, QJsonValue(QJsonValue::Null));
    rs[r] = cells;
  }
  m_column = at;
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), QString(),
              QStringLiteral("%1 [%2]").arg(name, unit), QStringLiteral("column added"));
}

void TablesPage::removeColumn() {
  QJsonArray cols = columns();
  if (m_column < 0 || m_column >= cols.size() || cols.size() <= 1) {
    return;
  }
  const int removed = m_column;
  const QString name = cols.at(removed).toObject().value(QStringLiteral("name")).toString();
  cols.removeAt(removed);
  QJsonArray rs = rows();
  for (int r = 0; r < static_cast<int>(rs.size()); ++r) {
    QJsonArray cells = rs.at(r).toArray();
    if (removed < cells.size()) {
      cells.removeAt(removed);
      rs[r] = cells;
    }
  }
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), QString(), name,
              QStringLiteral("column removed"));
}

void TablesPage::moveColumn(int by) {
  QJsonArray cols = columns();
  const int to = m_column + by;
  if (m_column < 0 || m_column >= cols.size() || to < 0 || to >= cols.size()) {
    return;
  }
  const auto swap = [](QJsonArray& array, int a, int b) {
    if (a < array.size() && b < array.size()) {
      const QJsonValue kept = array.at(a);
      array[a] = array.at(b);
      array[b] = kept;
    }
  };
  swap(cols, m_column, to);
  QJsonArray rs = rows();
  for (int r = 0; r < static_cast<int>(rs.size()); ++r) {
    QJsonArray cells = rs.at(r).toArray();
    swap(cells, m_column, to);
    rs[r] = cells;
  }
  const QString name = cols.at(to).toObject().value(QStringLiteral("name")).toString();
  m_column = to;
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = it.key().startsWith(m_current + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
  }
  m_focusGrid = true; // the button or the unit box had the keys
  submitTable(cols, rs, table().value(QStringLiteral("comment")).toString(), QString(),
              QStringLiteral("%1 to %2").arg(name).arg(to + 1), QStringLiteral("column moved"));
}

void TablesPage::newTable() {
  QStringList names;
  for (const QJsonValue& value : m_tables) {
    names << value.toObject().value(QStringLiteral("name")).toString();
  }
  QString initial;
  for (int n = 1;; ++n) {
    initial = QStringLiteral("Table%1").arg(n);
    if (!names.contains(initial)) {
      break;
    }
  }
  const ParameterSource* source = parameterSourceOf(this);
  const QString unit = source != nullptr && source->lengthUnit ? source->lengthUnit() : QStringLiteral("mm");
  QString created;
  TableNameDialog dialog(
      tr("New Table"), initial,
      [this, &names, &created, unit](const QString& name) {
        QString problem;
        if (names.contains(name)) {
          problem = tr("A table named %1 exists already: give another name.").arg(name);
        } else {
          // A key column and a value column, one empty row, to be renamed
          // and filled in.
          QJsonObject result;
          const QJsonObject command{
              {QStringLiteral("cmd"), QStringLiteral("set_table")},
              {QStringLiteral("name"), name},
              {QStringLiteral("table"),
               QJsonObject{{QStringLiteral("comment"), QString()},
                           {QStringLiteral("columns"),
                            QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("Key")},
                                                   {QStringLiteral("unit"), QStringLiteral("Text")}},
                                       QJsonObject{{QStringLiteral("name"), QStringLiteral("Value")},
                                                   {QStringLiteral("unit"), unit}}}},
                           {QStringLiteral("rows"),
                            QJsonArray{QJsonValue(QJsonArray{QJsonValue(QJsonValue::Null), QJsonValue(QJsonValue::Null)})}}}}};
          if (run(command, &result, &problem)) {
            created = name;
            qDebug().noquote() << QStringLiteral("Table %1 created").arg(name);
            return QString();
          }
        }
        qDebug().noquote() << QStringLiteral("New table %1 refused: %2").arg(name, problem);
        return problem;
      },
      this);
  qDebug().noquote() << QStringLiteral("New Table dialog opened: %1").arg(initial);
  prepareModal(&dialog);
  execDialog(&dialog);
  if (!created.isEmpty()) {
    m_current = created;
    m_focusGrid = true;
    m_row = 0;
    m_column = 0;
    m_message->clear();
  }
  emit changed(); // the dialog shows the tables and the parameters again
}

void TablesPage::renameTable() {
  const QString old = m_current;
  if (table().isEmpty()) {
    return;
  }
  QString renamed;
  TableNameDialog dialog(
      tr("Rename Table"), old,
      [this, old, &renamed](const QString& name) {
        if (name == old) {
          renamed = name;
          return QString();
        }
        QJsonObject result;
        QString problem;
        if (run({{QStringLiteral("cmd"), QStringLiteral("rename_table")},
                 {QStringLiteral("name"), old},
                 {QStringLiteral("new_name"), name}},
                &result, &problem)) {
          renamed = name;
          qDebug().noquote() << QStringLiteral("Table %1 renamed to %2").arg(old, name);
          return QString();
        }
        qDebug().noquote() << QStringLiteral("Rename table %1 to %2 refused: %3").arg(old, name, problem);
        return problem;
      },
      this);
  prepareModal(&dialog);
  execDialog(&dialog);
  if (!renamed.isEmpty() && renamed != old) {
    // The refusals of its cells go with it.
    QHash<QString, QPair<QString, QString>> kept;
    for (auto it = m_refused.cbegin(); it != m_refused.cend(); ++it) {
      const QString key = it.key().startsWith(old + QLatin1Char('|')) ? renamed + it.key().mid(old.size()) : it.key();
      kept.insert(key, it.value());
    }
    m_refused = kept;
    m_current = renamed;
  }
  emit changed(); // the dialog shows the tables and the parameters again
}

void TablesPage::deleteTable() {
  const QString name = m_current;
  if (table().isEmpty()) {
    return;
  }
  QJsonObject result;
  QString problem;
  if (run({{QStringLiteral("cmd"), QStringLiteral("delete_table")}, {QStringLiteral("name"), name}}, &result,
          &problem)) {
    m_message->clear();
    qDebug().noquote() << QStringLiteral("Deleted table %1").arg(name);
  } else {
    m_message->setText(QStringLiteral("%1: %2").arg(name, problem));
    qDebug().noquote() << QStringLiteral("Delete table %1 refused: %2").arg(name, problem);
  }
  emit changed(); // the dialog shows the tables and the parameters again
}

void TablesPage::importCsv() {
  const QString path = dialogOpenFileName(this, tr("Import Table from CSV"),
                                          dialogStartFolder(DialogGroup::Import, m_host.documentFolder()),
                                          tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Import of a table cancelled");
    return;
  }
  rememberDialogFolder(DialogGroup::Import, path);
  const QString file = QFileInfo(path).fileName();
  QFile input(path);
  if (!input.open(QIODevice::ReadOnly)) {
    const QString problem = tr("Could not read %1: %2").arg(QDir::toNativeSeparators(path), input.errorString());
    m_message->setText(problem);
    qDebug().noquote() << QStringLiteral("Import of a table from %1 refused: %2").arg(file, problem);
    return;
  }
  const QString csv = QString::fromUtf8(input.readAll());
  input.close();
  QHash<QString, int> existing; // the tables an import would replace, with their rows
  for (const QJsonValue& value : m_tables) {
    const QJsonObject t = value.toObject();
    existing.insert(t.value(QStringLiteral("name")).toString(),
                    static_cast<int>(t.value(QStringLiteral("rows")).toArray().size()));
  }
  TableImportDialog dialog(m_host, csv, file, nameFromFile(QFileInfo(path).completeBaseName()), existing, this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Import of a table from %1 cancelled").arg(file);
    return;
  }
  const QString name = dialog.name();
  QJsonObject result;
  QString problem;
  if (run({{QStringLiteral("cmd"), QStringLiteral("import_table_csv")},
           {QStringLiteral("name"), name},
           {QStringLiteral("csv"), csv}},
          &result, &problem)) {
    m_message->clear();
    m_current = name;
    m_focusGrid = true;
    for (auto it = m_refused.begin(); it != m_refused.end();) {
      it = it.key().startsWith(name + QLatin1Char('|')) ? m_refused.erase(it) : std::next(it);
    }
    const QString summary = result.value(QStringLiteral("summary")).toString();
    // Configuration options the import moved off removed values (mitcad#849).
    const QString moves =
        configurations::movesNotice(configurations::readMoves(result.value(QStringLiteral("moves"))));
    const QString imported = tr("Imported the table %1 from %2: %3.").arg(name, file, summary);
    m_host.showStatus(moves.isEmpty() ? imported : QStringLiteral("%1 %2").arg(imported, moves), NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Imported table %1 from %2: %3").arg(name, file, summary);
    if (!moves.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Configuration moves: %1").arg(moves);
    }
  } else {
    m_message->setText(QStringLiteral("%1: %2").arg(file, problem));
    qDebug().noquote() << QStringLiteral("Import of a table from %1 refused: %2").arg(file, problem);
  }
  emit changed(); // the dialog shows the tables and the parameters again
}

void TablesPage::exportCsv() {
  const QString name = m_current;
  if (table().isEmpty()) {
    return;
  }
  QJsonObject answer;
  try {
    answer = m_host.model().queryObject(
        {{QStringLiteral("query"), QStringLiteral("table_csv")}, {QStringLiteral("name"), name}});
  } catch (const std::exception& e) {
    m_message->setText(QString::fromUtf8(e.what()));
    return;
  }
  const QString folder = dialogStartFolder(DialogGroup::Export, m_host.documentFolder());
  const QString file = name + QStringLiteral(".csv");
  const QString path = dialogSaveFileName(this, tr("Export Table to CSV"),
                                          folder.isEmpty() ? file : QDir(folder).filePath(file),
                                          tr("CSV files (*.csv);;All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Export of the table %1 cancelled").arg(name);
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  const QByteArray bytes = answer.value(QStringLiteral("csv")).toString().toUtf8();
  QFile output(path);
  if (!output.open(QIODevice::WriteOnly | QIODevice::Truncate) || output.write(bytes) != bytes.size()) {
    const QString problem = tr("Could not write %1: %2").arg(QDir::toNativeSeparators(path), output.errorString());
    m_message->setText(problem);
    qDebug().noquote() << QStringLiteral("Export of the table %1 refused: %2").arg(name, problem);
    return;
  }
  output.close();
  m_message->clear();
  const int count = answer.value(QStringLiteral("rows")).toInt();
  m_host.showStatus(tr("Exported the table %1 to %2.").arg(name, QDir::toNativeSeparators(path)), NoticeLevel::Information);
  qDebug().noquote() << QStringLiteral("Exported table %1 to %2: %3 row(s)").arg(name, path).arg(count);
}

bool TablesPage::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() != QEvent::KeyPress) {
    return QWidget::eventFilter(watched, event);
  }
  const int key = static_cast<QKeyEvent*>(event)->key();
  const bool enter = key == Qt::Key_Return || key == Qt::Key_Enter;
  // Enter in a column's name or the comment applies it and goes on in the
  // grid: never the dialog's OK, which would close Change Parameters
  // before the change ran. The edit is applied as the field loses the
  // keyboard (editingFinished).
  auto* field = qobject_cast<QLineEdit*>(watched);
  if (enter && field != nullptr &&
      (field == m_comment || field->property(NativeReturnTargetProperty).isValid())) {
    m_grid->setFocus(Qt::OtherFocusReason);
    return true;
  }
  if (watched == m_grid) {
    // Delete empties the current cell (while a cell is edited, its editor
    // takes the keys).
    if (key == Qt::Key_Delete) {
      if (QTableWidgetItem* item = m_grid->currentItem()) {
        item->setText(QString());
        return true;
      }
    }
    // Enter on a cell goes to the cell below, as in a spreadsheet, and
    // never to the dialog's OK.
    if (enter) {
      if (m_grid->currentRow() + 1 < m_grid->rowCount()) {
        m_grid->setCurrentCell(m_grid->currentRow() + 1, std::max(0, m_grid->currentColumn()));
      }
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void TablesPage::takeFocus() {
  if (!table().isEmpty() && m_grid->isVisible()) {
    m_grid->setFocus();
  } else {
    m_list->setFocus();
  }
}

void TablesPage::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  // Told again when the tab is shown.
  m_loggedTables.clear();
  m_loggedColumns.clear();
  m_loggedRows.clear();
  m_loggedUsedBy.clear();
  m_loggedRefused.clear();
  m_loggedLayout.clear();
  refresh();
}

void TablesPage::logLayout() {
  if (!isVisible()) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  QWidget* top = window()->parentWidget() != nullptr ? window()->parentWidget()->window() : window();
  const QPoint origin = top->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  const QList<QPair<const char*, QPushButton*>> buttons{
      {"new", m_new},           {"rename", m_rename},           {"delete", m_delete},
      {"import", m_import},     {"export", m_export},           {"add row", m_addRow},
      {"remove row", m_removeRow}, {"row up", m_rowUp},         {"row down", m_rowDown},
      {"add column", m_addColumn}, {"remove column", m_removeColumn}, {"column left", m_columnLeft},
      {"column right", m_columnRight}};
  for (const auto& entry : buttons) {
    if (entry.second->isVisible()) {
      lines << QStringLiteral("Tables %1 at %2").arg(QLatin1String(entry.first), at(entry.second, entry.second->rect().center()));
    }
  }
  if (m_comment->isVisible()) {
    lines << QStringLiteral("Tables comment at %1").arg(at(m_comment, m_comment->rect().center()));
  }
  for (int i = 0; i < m_list->count(); ++i) {
    QListWidgetItem* item = m_list->item(i);
    const QRect rect = m_list->visualItemRect(item);
    if (!rect.isEmpty() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Tables list %1 at %2").arg(item->text(), at(m_list->viewport(), rect.center()));
    }
  }
  if (m_grid->isVisible()) {
    for (int c = 0; c < m_grid->columnCount(); ++c) {
      if (QWidget* editor = m_header->editor(c)) {
        auto* name = editor->findChild<QLineEdit*>();
        auto* unit = editor->findChild<QComboBox*>();
        if (name != nullptr && name->isVisible()) {
          lines << QStringLiteral("Tables column %1 name at %2")
                       .arg(c + 1)
                       .arg(at(name, name->rect().center()));
        }
        if (unit != nullptr && unit->isVisible()) {
          lines << QStringLiteral("Tables column %1 unit at %2")
                       .arg(c + 1)
                       .arg(at(unit, unit->rect().center()));
        }
      }
    }
    const QRect visible = m_grid->viewport()->rect();
    for (int r = 0; r < m_grid->rowCount(); ++r) {
      for (int c = 0; c < m_grid->columnCount(); ++c) {
        const QRect rect = m_grid->visualRect(m_grid->model()->index(r, c));
        if (!rect.isEmpty() && visible.contains(rect.center())) {
          lines << QStringLiteral("Tables cell %1 %2 at %3").arg(r + 1).arg(c + 1).arg(at(m_grid->viewport(), rect.center()));
        }
      }
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedLayout) {
    m_loggedLayout = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
