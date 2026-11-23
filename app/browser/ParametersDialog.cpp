// SPDX-License-Identifier: MIT
#include "ParametersDialog.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <iterator>
#include <numeric>
#include <optional>
#include <utility>

#include <QAbstractItemDelegate>
#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QDir>
#include <QDragMoveEvent>
#include <QDropEvent>
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
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QRegularExpression>
#include <QScreen>
#include <QSet>
#include <QSettings>
#include <QSignalBlocker>
#include <QSlider>
#include <QTabBar>
#include <QTabWidget>
#include <QStyle>
#include <QStyleOptionSlider>
#include <QStyledItemDelegate>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/ExpressionCheck.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/ParameterField.hpp"
#include "../framework/ParameterUnits.hpp"
#include "../framework/FormulaEditor.hpp"
#include "../framework/IdleCall.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"
#include "../platform/MacParameters.hpp"
#include "../platform/NativeDialogs.hpp"
#include "../platform/MacDialogs.hpp"
#include "DecimalAlignDelegate.hpp"
#include "DocumentHost.hpp"
#include "ParameterSort.hpp"
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
// The model's unit of a row, in its Name cell (mitcad#1920): the Unit cell
// shows Number for a parameter without a unit.
constexpr int kUnitRole = Qt::UserRole + 69;
// Groups of user parameters (mitcad#2180): a group row's path, the group a
// parameter's row is in, and which section a section row is ("favorites",
// "users", "models").
constexpr int kGroupRole = Qt::UserRole + 70;
constexpr int kInGroupRole = Qt::UserRole + 71;
constexpr int kSectionRole = Qt::UserRole + 72;

// A group path as the model writes it (mitcad#2180): each name trimmed,
// joined with /.
QString plainGroupPath(const QString& text) {
  QStringList names;
  for (const QString& name : text.split(QLatin1Char('/'))) {
    names << name.trimmed();
  }
  return names.join(QLatin1Char('/'));
}

// A group's parent group, empty at the top; and its own name.
QString parentGroup(const QString& path) {
  const qsizetype at = path.lastIndexOf(QLatin1Char('/'));
  return at < 0 ? QString() : path.left(at);
}

QString groupLeaf(const QString& path) {
  return path.mid(path.lastIndexOf(QLatin1Char('/')) + 1);
}

// Whether `path` is `group` or one of its subgroups.
bool inGroup(const QString& path, const QString& group) {
  return path == group || path.startsWith(group + QLatin1Char('/'));
}

// A row's key for keeping it selected: a parameter's name, or "group "
// and a group's path (no parameter name has a space).
QString rowKey(const QTreeWidgetItem* item) {
  if (item == nullptr) {
    return {};
  }
  const QString group = item->data(0, kGroupRole).toString();
  return group.isEmpty() ? item->data(0, kNameRole).toString() : QStringLiteral("group ") + group;
}

// The collapsed groups of each design (DocumentHost::designId), kept for
// the session (mitcad#2180).
QHash<QString, QSet<QString>>& collapsedGroups() {
  static QHash<QString, QSet<QString>> groups;
  return groups;
}

// The section (Favorites, User Parameters, Model Parameters) a row is in.
QString sectionOf(const QTreeWidgetItem* item) {
  while (item != nullptr && item->parent() != nullptr) {
    item = item->parent();
  }
  return item != nullptr ? item->data(0, kSectionRole).toString() : QString();
}

// The empty row at the end of User Parameters (mitcad#2202), in its Name
// cell: no parameter's row (its kNameRole is empty) and no group's.
constexpr int kNewRowRole = mac::NewParameterRowRole;

// What the empty row's Name cell shows until a name is typed.
QString newRowPlaceholder() { return QCoreApplication::translate("mitcad::ParametersDialog", "New parameter"); }

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
  static const char* const names[] = {"name", "unit", "expression", "value",  "comment", "favorite",
                                      "min",  "max",  "step",       "strict", "slider",  "delete"};
  return column >= 0 && column < int(std::size(names)) ? QString::fromLatin1(names[column])
                                                         : QString::number(column);
}

// The table's sort (mitcad#1866) in the settings: "<column> ascending" or
// "<column> descending" with the column as the log names it, or "none"
// for the creation order.
const char* const kSortSetting = "parameters/sort";

QString refusedKey(const QString& name, int column) {
  return QStringLiteral("%1|%2").arg(name).arg(column);
}

// The whole expression in its cell's editor (mitcad#2179): the editor
// grows with its text from the cell's width over the next columns up to
// the table's width, moving left where the table ends, so that neither the
// start nor the end (under the fx button) is out of sight; a text longer
// than the table's width is shown whole, wrapped, under the editor
// (parameterExpressionWhole). `cell` is the cell's place in the viewport.
const char* const kCellRectProperty = "mitcadCellRect";
const char* const kWholeTextProperty = "mitcadWholeText";

// What the editor needs besides its text: the frame, the margins, the side
// buttons (fx) and room for the caret.
int editorChrome(const QLineEdit* edit) {
  const int frame = edit->hasFrame() ? 2 * edit->style()->pixelMetric(QStyle::PM_DefaultFrameWidth, nullptr, edit) : 0;
  const QMargins margins = edit->textMargins();
  int side = 0;
  for (const QToolButton* button : edit->findChildren<QToolButton*>()) {
    if (button->isVisibleTo(edit)) {
      side += std::max(button->width(), button->sizeHint().width()) + 4;
    }
  }
  return frame + margins.left() + margins.right() + side + 2 * 2 /* the editor's own margin */ + 8;
}

void fitWholeText(QLineEdit* edit, const QRect& cell) {
  QWidget* viewport = edit->parentWidget();
  if (viewport == nullptr || !cell.isValid()) {
    return;
  }
  edit->setProperty(kCellRectProperty, cell);
  const int available = std::max(cell.width(), viewport->width());
  const int needed = edit->fontMetrics().horizontalAdvance(edit->text()) + editorChrome(edit);
  const int width = std::clamp(needed, cell.width(), available);
  const int x = cell.left() + width > available ? std::max(0, available - width) : cell.left();
  edit->setMinimumWidth(0);
  edit->setMaximumWidth(QWIDGETSIZE_MAX);
  const QRect place(x, cell.top(), width, cell.height());
  if (edit->geometry() != place) {
    edit->setGeometry(place);
  }
  // Longer than the table is wide: the whole text, wrapped, under it.
  auto* whole = qobject_cast<QLabel*>(edit->property(kWholeTextProperty).value<QObject*>());
  if (needed <= available) {
    if (whole != nullptr) {
      whole->hide();
    }
    return;
  }
  if (whole == nullptr) {
    whole = new QLabel(viewport);
    whole->setObjectName(QStringLiteral("parameterExpressionWhole"));
    whole->setWordWrap(true);
    whole->setTextFormat(Qt::PlainText);
    whole->setFrameShape(QFrame::Box);
    whole->setAutoFillBackground(true);
    whole->setBackgroundRole(QPalette::Base);
    whole->setAttribute(Qt::WA_TransparentForMouseEvents);
    QObject::connect(edit, &QObject::destroyed, whole, &QObject::deleteLater);
    edit->setProperty(kWholeTextProperty, QVariant::fromValue<QObject*>(whole));
  }
  whole->setFont(edit->font());
  whole->setText(edit->text());
  whole->setContentsMargins(4, 2, 4, 2);
  const int height = whole->heightForWidth(width);
  const int below = place.bottom() + 1;
  const int y = below + height <= viewport->height() ? below : std::max(0, place.top() - height);
  whole->setGeometry(x, y, width, height);
  whole->show();
  whole->raise();
}

// The Expression column's delegate: its numbers on their decimal axis
// (mitcad#566) and its editor showing the whole text (mitcad#2179).
class ExpressionDelegate : public DecimalAlignDelegate {
public:
  ExpressionDelegate(QTreeWidget* tree, std::vector<int> columns, int expressionColumn)
      : DecimalAlignDelegate(tree, std::move(columns)), m_expressionColumn(expressionColumn) {}

  void updateEditorGeometry(QWidget* editor, const QStyleOptionViewItem& option,
                            const QModelIndex& index) const override {
    DecimalAlignDelegate::updateEditorGeometry(editor, option, index);
    auto* edit = qobject_cast<QLineEdit*>(editor);
    if (edit != nullptr && index.column() == m_expressionColumn) {
      fitWholeText(edit, option.rect);
    }
  }

private:
  int m_expressionColumn;
};

// The unit of a text parameter (mitcad#664).
bool isTextUnit(const QString& unit) {
  return unit.trimmed().compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0;
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

// A refusal of a parameter's change in the dialog's message line: the
// model's words when they name the parameter already ("The expression of
// width uses …", mitcad#2181: no name said twice), else the name before
// them ("width: …").
QString refusalLine(const QString& name, const QString& problem) {
  const QRegularExpression word(QStringLiteral("(^|[^\\w])%1($|[^\\w])").arg(QRegularExpression::escape(name)),
                                QRegularExpression::UseUnicodePropertiesOption);
  return word.match(problem).hasMatch() ? problem : QStringLiteral("%1: %2").arg(name, problem);
}

// The problem an Expression cell's editor shows, kept on it (mitcad#2181).
const char* const kCellProblemProperty = "mitcadExpressionProblem";

// An Expression cell's text checked while it is typed (mitcad#2181): what
// is wrong in words in the dialog's message line (in place of the limits'
// preview), the part it is about marked in the editor; the message goes
// when the text is right again. The empty row's (mitcad#2202, `isNew`) is
// no parameter yet: its text is checked as a new parameter's value in the
// row's unit, `name` (what is typed in its Name, maybe nothing) only told.
void checkCellExpression(DocumentHost& host, QLabel* message, QLineEdit* editor, const QString& name,
                         const QString& unit, const QString& text, bool isNew = false) {
  const ExpressionProblem problem =
      checkExpression([&host](const QJsonObject& query) { return host.model().queryObject(query); }, text,
                      withUnit(text.trimmed(), unit), unit, isNew ? QString() : name);
  markExpressionRange(editor, problem.start, problem.end);
  const QString shown = editor->property(kCellProblemProperty).toString();
  if (!problem.isEmpty()) {
    message->setText(problem.message);
  } else if (!shown.isEmpty() && message->text() == shown) {
    message->clear();
  }
  message->setVisible(!message->text().isEmpty()); // no room while empty (mitcad#2202)
  if (problem.message != shown) {
    editor->setProperty(kCellProblemProperty, problem.message);
    qDebug().noquote() << QStringLiteral("Parameter %1 expression problem while typed: %2")
                              .arg(name, problem.isEmpty() ? QStringLiteral("none")
                                                           : QStringLiteral("%1 (marked %2..%3)")
                                                                 .arg(problem.message)
                                                                 .arg(problem.start)
                                                                 .arg(problem.end));
  }
}

// New Group… and Move to Group ▸ New Group… (mitcad#2180): the new group's
// name, which may nest with / (Doors/Hinges), in `parent` when one is
// given. `make` runs the model command for the whole path and returns the
// model's reason when it refuses; the dialog stays open with it.
class GroupNameDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::GroupNameDialog)

public:
  GroupNameDialog(const QString& title, const QString& parent, std::function<QString(const QString&)> make,
                  QWidget* parentWidget)
      : QDialog(parentWidget), m_parent(parent), m_make(std::move(make)) {
    setObjectName(QStringLiteral("parameterGroupDialog"));
    setWindowTitle(title);
    auto* form = new QFormLayout(this);
    if (!parent.isEmpty()) {
      form->addRow(tr("In"), new QLabel(parent));
    }
    m_name = new QLineEdit;
    m_name->setObjectName(QStringLiteral("parameterGroupName"));
    m_name->setToolTip(tr("The group's name; nested groups with / (Cabinet/Doors)"));
    m_error = new QLabel;
    m_error->setObjectName(QStringLiteral("parameterGroupError"));
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &GroupNameDialog::make);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    mac::styleParameterForm(this, form);
    mac::styleDialogButtons(buttons, nullptr, true);
    m_name->setFocus();
  }

  // The new group's whole path.
  QString path() const { return m_path; }

private:
  void make() {
    const QString name = plainGroupPath(m_name->text());
    QString problem;
    if (name.isEmpty()) {
      problem = tr("Give the group a name.");
    } else {
      m_path = m_parent.isEmpty() ? name : m_parent + QLatin1Char('/') + name;
      problem = m_make(m_path);
      if (problem.isEmpty()) {
        accept();
        return;
      }
    }
    m_error->setText(problem);
    qDebug().noquote() << QStringLiteral("Parameter group %1 refused: %2").arg(name, problem);
  }

  QString m_parent;
  std::function<QString(const QString&)> m_make;
  QLineEdit* m_name;
  QLabel* m_error;
  QString m_path;
};

// F2 edits the current cell, as double-click does. The Menu key and
// Shift+F10 open the current row's context menu. User parameters dragged
// onto a group row, onto User Parameters or onto a row in a group go in
// that group (mitcad#2180): the table moves no row itself, the model
// moves the parameters and the table is built again. An editor of the
// empty row for a new parameter (mitcad#2202) closed by Tab, Enter, Escape
// or leaving it goes to `closed` with the column it edited and how it
// closed: the dialog moves along that row or creates the parameter (a tree
// view's own Tab goes to the row below).
class ParameterTable : public QTreeWidget {
public:
  using Closed = std::function<void(int column, QAbstractItemDelegate::EndEditHint hint)>;
  ParameterTable(std::function<void(QTreeWidgetItem*, int)> edit, Closed closed)
      : m_edit(std::move(edit)), m_closed(std::move(closed)) {}

  // Called after a drop with the group's path, empty for User Parameters.
  void setDropped(std::function<void(const QString&)> dropped) { m_dropped = std::move(dropped); }

  // The group a drop at `pos` (in the viewport) puts parameters in; none
  // outside User Parameters.
  std::optional<QString> dropGroup(const QPoint& pos) const {
    const QTreeWidgetItem* item = itemAt(pos);
    if (item == nullptr || sectionOf(item) != QStringLiteral("users")) {
      return std::nullopt;
    }
    const QString group = item->data(0, kGroupRole).toString();
    if (!group.isEmpty()) {
      return group;
    }
    // User Parameters itself, or a parameter's row: its group.
    return item->parent() == nullptr ? QString() : item->data(0, kInGroupRole).toString();
  }

protected:
  bool event(QEvent* event) override {
    // The context menu's keys are the table's, not a shortcut's.
    if (event->type() == QEvent::ShortcutOverride && menuKey(static_cast<QKeyEvent*>(event))) {
      event->accept();
      return true;
    }
    return QTreeWidget::event(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    if (event->key() == Qt::Key_F2 && currentItem() != nullptr &&
        state() != QAbstractItemView::EditingState) {
      m_edit(currentItem(), currentColumn() >= 0 ? currentColumn() : 2);
      return;
    }
    if (menuKey(event) && currentItem() != nullptr && state() != QAbstractItemView::EditingState) {
      emit customContextMenuRequested(visualItemRect(currentItem()).center());
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

  void dragMoveEvent(QDragMoveEvent* event) override {
    QTreeWidget::dragMoveEvent(event); // scrolls and opens groups while dragging
    if (event->source() == this && dropGroup(event->position().toPoint())) {
      event->setDropAction(Qt::MoveAction);
      event->accept();
    } else {
      event->ignore();
    }
  }

  void dropEvent(QDropEvent* event) override {
    const std::optional<QString> group =
        event->source() == this ? dropGroup(event->position().toPoint()) : std::nullopt;
    // A copy for the drag, so that the view removes no row.
    event->setDropAction(Qt::CopyAction);
    event->accept();
    setState(QAbstractItemView::NoState);
    stopAutoScroll();
    viewport()->update();
    if (group && m_dropped) {
      m_dropped(*group);
    }
  }

  void closeEditor(QWidget* editor, QAbstractItemDelegate::EndEditHint hint) override {
    QTreeWidgetItem* item = currentItem();
    if (item == nullptr || !item->data(0, kNewRowRole).toBool()) {
      QTreeWidget::closeEditor(editor, hint);
      return;
    }
    const int column = currentColumn();
    QTreeWidget::closeEditor(editor, QAbstractItemDelegate::NoHint);
    m_closed(column, hint);
  }

private:
  static bool menuKey(const QKeyEvent* event) {
    return event->key() == Qt::Key_Menu || (event->key() == Qt::Key_F10 && event->modifiers() == Qt::ShiftModifier);
  }

  std::function<void(QTreeWidgetItem*, int)> m_edit;
  std::function<void(const QString&)> m_dropped;
  Closed m_closed;
};

// The Name cells (mitcad#2202): the empty row's shows "New parameter" in
// the placeholder colour until a name is typed, and so does its editor.
// The other rows are painted as the table's own delegate paints them.
class NameDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  QWidget* createEditor(QWidget* parent, const QStyleOptionViewItem& option,
                        const QModelIndex& index) const override {
    QWidget* editor = QStyledItemDelegate::createEditor(parent, option, index);
    if (auto* line = qobject_cast<QLineEdit*>(editor); line != nullptr && index.data(kNewRowRole).toBool()) {
      line->setPlaceholderText(newRowPlaceholder());
    }
    return editor;
  }

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    mac::styleParameterCell(option, index);
    if (index.data(kNewRowRole).toBool() && option->text.isEmpty()) {
      option->text = newRowPlaceholder();
      option->palette.setColor(QPalette::Text, option->palette.color(QPalette::PlaceholderText));
    }
  }
};

// Where a slider's handle is in a slider of `size` (its own size, or the
// cell it fills before it is laid out).
QRect sliderHandleRect(const QSlider* slider, const QSize& size) {
  QStyleOptionSlider option;
  option.initFrom(slider);
  option.rect = QRect(QPoint(0, 0), size);
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
  return slider->style()->subControlRect(QStyle::CC_Slider, &option, QStyle::SC_SliderHandle, slider);
}

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

  // A user parameter is added in the empty row at the end of User
  // Parameters and deleted by its row's button (mitcad#2202): no buttons
  // of their own above the table; New Group… stays there (mitcad#2180).
  auto* top = new QHBoxLayout;
  // Groups of user parameters (mitcad#2180).
  m_newGroup = new QPushButton(themeIcon(QStringLiteral("folder")), tr("New Group…"));
  m_newGroup->setToolTip(tr("Make a group of user parameters (in the selected group)"));
  m_newGroup->setObjectName(QStringLiteral("parametersNewGroup"));
  m_newGroup->setAutoDefault(false);
  m_filter = new QLineEdit;
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
  top->addWidget(m_newGroup);
  top->addStretch(1);
  // Only the parameters a limit changed (mitcad#665).
  m_limitedOnly = new QCheckBox(tr("Only limited"));
  m_limitedOnly->setObjectName(QStringLiteral("parametersLimitedOnly"));
  m_limitedOnly->setToolTip(tr("Show only the parameters whose value a minimum, maximum or step changed"));
  top->addWidget(m_limitedOnly);
  top->addWidget(m_filter);
  layout->addLayout(top);

  m_table = new ParameterTable(
      [this](QTreeWidgetItem* item, int column) { edit(item, column); },
      [this](int column, QAbstractItemDelegate::EndEditHint hint) {
        newRowEditorClosed(column, static_cast<int>(hint));
      });
  m_table->setObjectName(QStringLiteral("parametersTable"));
  panel::setCompactIndentation(m_table, panel::Indent::Tree);
  m_table->setColumnCount(12);
  m_table->setHeaderLabels({tr("Name"), tr("Unit"), tr("Expression"), tr("Value"), tr("Comment"), QStringLiteral("★"),
                            tr("Min"), tr("Max"), tr("Step"), tr("Strict"), tr("Slider"), QString()});
  m_table->headerItem()->setToolTip(kDelete, tr("Delete a user parameter (when nothing uses it)"));
  m_table->headerItem()->setToolTip(kUnit, tr("Number for a number without a unit; Text for a text"));
  m_table->headerItem()->setToolTip(kFavorite, tr("Favorites: listed first"));
  m_table->headerItem()->setToolTip(kMin, tr("The smallest value: an expression, may use other parameters"));
  m_table->headerItem()->setToolTip(kMax, tr("The largest value: an expression, may use other parameters"));
  m_table->headerItem()->setToolTip(kStep, tr("Values are rounded to steps counted from the minimum"));
  m_table->headerItem()->setToolTip(kStrict, tr("Refuse a value outside the limits instead of limiting it"));
  m_table->headerItem()->setToolTip(kSlider, tr("Between the minimum and the maximum"));
  m_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  // Several user parameters at once for Move to Group and a drag
  // (mitcad#2180); the other commands work on the current row.
  m_table->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_table->setDragEnabled(true);
  m_table->setAcceptDrops(true);
  m_table->viewport()->setAcceptDrops(true);
  m_table->setDragDropMode(QAbstractItemView::DragDrop);
  m_table->setDefaultDropAction(Qt::MoveAction);
  m_table->setDropIndicatorShown(false);
  // A double-click renames a group row; it opens and closes the sections.
  m_table->setExpandsOnDoubleClick(false);
  m_table->setAlternatingRowColors(true);
  m_table->setUniformRowHeights(true);
  m_table->header()->setStretchLastSection(false);
  m_table->header()->setSectionResizeMode(kComment, QHeaderView::Stretch);
  // A user parameter's delete button (mitcad#2202): an icon's width.
  m_table->header()->setSectionResizeMode(kDelete, QHeaderView::Fixed);
  fitColumns(); // the wanted widths, narrowed only in a narrow window
  // A width the user drags is kept (Comment follows the others).
  connect(m_table->header(), &QHeaderView::sectionResized, this, [this](int column, int, int) {
    if (!m_fittingColumns && column != kComment && isVisible()) {
      m_columnsByUser = true;
    }
  });
  // Sorting (mitcad#1866): a header's click sorts by its column, the next
  // one in the reverse order, the third back to the creation order (Qt's
  // clearable sort indicator); the last choice comes from the settings.
  static_assert(kSlider == static_cast<int>(ParameterColumn::Slider), "the sort's columns are the table's");
  QHeaderView* header = m_table->header();
  header->setSectionsClickable(true);
  header->setSortIndicatorShown(true);
  header->setSortIndicatorClearable(true);
  const QStringList savedSort =
      QSettings().value(QLatin1String(kSortSetting)).toString().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  if (savedSort.size() == 2) {
    for (int column = kName; column <= kSlider; ++column) {
      if (columnLogName(column) == savedSort.at(0)) {
        m_sortColumn = column;
        m_sortOrder = savedSort.at(1) == QStringLiteral("descending") ? Qt::DescendingOrder : Qt::AscendingOrder;
      }
    }
  }
  header->setSortIndicator(m_sortColumn, m_sortOrder);
  connect(header, &QHeaderView::sortIndicatorChanged, this, &ParametersDialog::sortBy);
  // The numbers of Expression and Value on one decimal axis each (mitcad#566).
  m_align = new ExpressionDelegate(m_table, {kExpression, kValue}, kExpression);
  m_table->setItemDelegateForColumn(kExpression, m_align);
  m_table->setItemDelegateForColumn(kValue, m_align);
  // Unit: Number for a parameter without a unit, edited in a Unit list
  // (mitcad#1920).
  m_table->setItemDelegateForColumn(kUnit, new ParameterUnitDelegate(m_table));
  // Name: the empty row's placeholder (mitcad#2202).
  m_table->setItemDelegateForColumn(kName, new NameDelegate(m_table));
  // The native cells line them up too (mitcad#1080).
  m_table->setProperty(NativeDecimalColumnsProperty,
                       QVariantList{static_cast<int>(kExpression), static_cast<int>(kValue)});
  layout->addWidget(m_table, 1);

  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("parametersMessage"));
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  // No room while it is empty (mitcad#2202): setMessage shows it.
  m_message->hide();
  layout->addWidget(m_message);

  m_tabs->addTab(parametersPage, tr("&Parameters"));
  m_tablesPage = new TablesPage(host);
  m_tabs->addTab(m_tablesPage, tr("&Tables"));
  dialogLayout->addWidget(m_tabs, 1);
  // One row at the bottom of the window (mitcad#2202): the Parameters
  // tab's Import…, Export… and Edit Formula… on the left, OK on the right,
  // on the window's margins. The Tables tab has its own Import CSV… and
  // Export CSV…: the parameters' buttons are hidden there.
  auto* bottom = new QHBoxLayout;
  bottom->addWidget(m_import);
  bottom->addWidget(m_export);
  m_formula = new QPushButton(tr("Edit &Formula…"));
  m_formula->setObjectName(QStringLiteral("parametersFormula"));
  m_formula->setAutoDefault(false);
  // A click leaves the keyboard where it is: an Expression cell being
  // edited stays open, and the button opens Edit Formula with what is
  // typed in it (mitcad#2290); Tab still reaches the button.
  m_formula->setFocusPolicy(Qt::TabFocus);
  m_formula->setToolTip(tr("Edit the selected parameter's formula and limits"));
  bottom->addWidget(m_formula);
  bottom->addStretch(1);
  auto* buttons = new QDialogButtonBox;
  m_ok = new QPushButton(tr("OK"));
  m_ok->setObjectName(QStringLiteral("parametersOK"));
  m_ok->setAutoDefault(false);
  buttons->addButton(m_ok, QDialogButtonBox::AcceptRole);
  bottom->addWidget(buttons);
  dialogLayout->addLayout(bottom);
  // A table's change changes the values of the parameters that use it.
  connect(m_tablesPage, &TablesPage::changed, this, &ParametersDialog::refresh);
  connect(m_tabs, &QTabWidget::currentChanged, this, [this](int index) {
    qDebug().noquote() << QStringLiteral("Parameters tab shown: %1").arg(m_tabs->tabText(index).remove(QLatin1Char('&')));
    // The keys go to the tab's table (Alt+P, Alt+T).
    const bool tables = m_tabs->widget(index) == m_tablesPage;
    if (tables) {
      m_tablesPage->takeFocus();
    } else {
      m_table->setFocus();
    }
    // The bottom row offers only the shown tab's actions (mitcad#2202).
    for (QPushButton* button : {m_import, m_export, m_formula}) {
      button->setVisible(!tables);
    }
    qDebug().noquote() << QStringLiteral("Parameters bottom row: %1")
                              .arg(tables ? QStringLiteral("OK") : QStringLiteral("Import, Export, Edit Formula, OK"));
    m_logged.clear();
    QTimer::singleShot(0, this, &ParametersDialog::logLayout);
  });
  m_table->setFocus();
  mac::styleParametersDialog(this, m_table, m_filter, m_ok);
  mac::styleDialogButtons(buttons, m_ok);

  connect(m_ok, &QPushButton::clicked, this, &QDialog::accept);
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
  connect(m_table, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item, int column) {
    // A section's row opens and closes (the tree does not on a
    // double-click, which renames a group row, mitcad#2180).
    if (item != nullptr && item->data(kName, kNameRole).toString().isEmpty() &&
        item->data(kName, kGroupRole).toString().isEmpty() && !item->data(kName, kNewRowRole).toBool()) {
      item->setExpanded(!item->isExpanded());
      return;
    }
    edit(item, column);
  });
  connect(m_table, &QTreeWidget::itemChanged, this, &ParametersDialog::cellEdited);
  connect(m_table, &QTreeWidget::itemSelectionChanged, this, &ParametersDialog::updateButtons);
  connect(m_table, &QTreeWidget::itemExpanded, this,
          [this](QTreeWidgetItem* item) { groupExpansionChanged(item, true); });
  connect(m_table, &QTreeWidget::itemCollapsed, this,
          [this](QTreeWidgetItem* item) { groupExpansionChanged(item, false); });
  // The empty row (mitcad#2202): a click edits its cell; leaving the row
  // for another one makes it a parameter when it has a name and an
  // expression.
  connect(m_table, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int column) {
    // (A click on the cell being edited goes to its editor.)
    if (item != nullptr && item->data(kName, kNewRowRole).toBool()) {
      edit(item, column);
    }
  });
  connect(m_table, &QTreeWidget::currentItemChanged, this,
          [this](QTreeWidgetItem* current, QTreeWidgetItem* previous) {
            if (!m_building && previous != nullptr && previous != current &&
                previous->data(kName, kNewRowRole).toBool()) {
              createFromNewRow(false);
            }
            // The empty row becomes current without a selection
            // (mitcad#2202): Edit Formula… follows it too (mitcad#2290).
            updateButtons();
          });
  // User parameters dropped on a group (mitcad#2180), moved once the drag
  // has ended.
  static_cast<ParameterTable*>(m_table)->setDropped([this](const QString& group) {
    const QStringList names = selectedUserParameters();
    if (names.isEmpty()) {
      return;
    }
    qDebug().noquote() << QStringLiteral("Parameters dropped on %1: %2")
                              .arg(group.isEmpty() ? QStringLiteral("User Parameters") : group,
                                   names.join(QStringLiteral(", ")));
    afterEventWhenIdle(m_host, this, [this, names, group] { moveToGroup(names, group); });
  });
  // A user parameter's row: Add Comment… for an AI agent (mitcad#1119) and
  // Move to Group ▸; a group's row: New Group…, Rename, Delete
  // (mitcad#2180).
  m_table->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(m_table, &QTreeWidget::customContextMenuRequested, this, &ParametersDialog::showContextMenu);
  connect(m_newGroup, &QPushButton::clicked, this, &ParametersDialog::newGroup);
  refresh();
}

QTreeWidgetItem* ParametersDialog::addRow(QTreeWidgetItem* parent, const QJsonObject& parameter) {
  const QString name = parameter.value(QStringLiteral("name")).toString();
  const bool user = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("user");
  auto* row = new QTreeWidgetItem(parent);
  row->setText(kName, name);
  // Number for a parameter without a unit (mitcad#1920): display only; the
  // model's unit is in kUnitRole, and the sort and the filter read the model.
  const QString unit = parameter.value(QStringLiteral("unit")).toString();
  row->setText(kUnit, unitDisplayText(unit));
  row->setData(kName, kUnitRole, unit);
  row->setText(kExpression, parameter.value(QStringLiteral("expression")).toString());
  row->setText(kValue, parameter.value(QStringLiteral("text")).toString());
  row->setText(kComment, parameter.value(QStringLiteral("comment")).toString());
  row->setData(kName, kNameRole, name);
  row->setFlags(row->flags() | Qt::ItemIsEditable | Qt::ItemIsUserCheckable);
  // Its group (mitcad#2180): a user parameter's row under User Parameters
  // is dragged to another group and takes a drop as its group does.
  row->setData(kName, kInGroupRole, parameter.value(QStringLiteral("group")).toString());
  if (!user || sectionOf(parent) != QStringLiteral("users")) {
    row->setFlags(row->flags() & ~(Qt::ItemIsDragEnabled | Qt::ItemIsDropEnabled));
  }
  // What a driven dimension measures: renamed, never set (mitcad#231).
  const bool reference = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("reference");
  // A text parameter (mitcad#664): its value is its text; no limits, no
  // slider.
  const bool text = isTextUnit(unit);
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
      // A refused unit as the Unit list shows it: Number for none (mitcad#1920).
      row->setText(column, column == kUnit ? unitDisplayText(refused.value().first) : refused.value().first);
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
  if (user) {
    addDeleteButton(row, parameter);
  }
  return row;
}

// A user parameter's delete button at the end of its row (mitcad#2202):
// a flat icon button, disabled while something uses the parameter (the
// `parameters` query's `used_by`) or the open panel holds it, with the
// reason in its tooltip; a click deletes the parameter, one undo step.
void ParametersDialog::addDeleteButton(QTreeWidgetItem* row, const QJsonObject& parameter) {
  const QString name = parameter.value(QStringLiteral("name")).toString();
  const QStringList users = parameter.value(QStringLiteral("used_by")).toVariant().toStringList();
  auto* button = new QToolButton;
  button->setObjectName(QStringLiteral("parameterDelete_%1").arg(name));
  button->setIcon(themeIcon(QStringLiteral("delete")));
  button->setAutoRaise(true);
  button->setFocusPolicy(Qt::NoFocus);
  button->setAccessibleName(tr("Delete %1").arg(name));
  QString tip = tr("Delete %1").arg(name);
  if (!users.isEmpty()) {
    // In the words of the model's refusal of `delete_parameter`.
    tip = mitcad::modelMessage(
        QStringLiteral("parameter %1 is used by %2").arg(name, users.join(QStringLiteral(", "))));
  } else if (m_scope.holds(parameter)) {
    tip = m_scope.holdReason(name);
  }
  button->setEnabled(users.isEmpty() && !m_scope.holds(parameter));
  button->setToolTip(tip);
  row->setToolTip(kDelete, tip);
  row->setData(kDelete, NativeControlIdentifierRole, QStringLiteral("parameter.%1.delete").arg(name));
  m_table->setItemWidget(row, kDelete, button);
  connect(button, &QToolButton::clicked, this, [this, name] { deleteParameter(name); });
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
  // The row selected stays selected; a row just renamed or added is
  // (mitcad#1866), wherever the sort puts it.
  // The selected row, not the current one: the empty row (mitcad#2202) can
  // be edited while the row just added stays selected.
  QString current = std::exchange(m_select, QString());
  // With several rows selected (mitcad#2180), the current one when it is
  // among them.
  QTreeWidgetItem* item = m_table->currentItem();
  if (item == nullptr || !item->isSelected()) {
    const QList<QTreeWidgetItem*> selectedBefore = m_table->selectedItems();
    if (!selectedBefore.isEmpty()) {
      item = selectedBefore.first();
    }
  }
  if (item != nullptr && current.isEmpty()) {
    current = rowKey(item); // a group's row too (mitcad#2180)
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
    setMessage(mitcad::modelMessage(e.what()));
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
  favorites->setData(kName, kSectionRole, QStringLiteral("favorites"));
  users->setData(kName, kSectionRole, QStringLiteral("users"));
  models->setData(kName, kSectionRole, QStringLiteral("models"));
  // Only User Parameters takes dropped parameters (mitcad#2180); no
  // section is dragged.
  for (QTreeWidgetItem* section : {favorites, users, models}) {
    section->setFlags(section->flags() & ~Qt::ItemIsDragEnabled);
    if (section != users) {
      section->setFlags(section->flags() & ~Qt::ItemIsDropEnabled);
    }
  }
  // The groups of the user parameters (mitcad#2180) under User Parameters,
  // before the parameters: a parent before its subgroups, in the order
  // they were made, or by name when the rows sort by Name.
  QHash<QString, QTreeWidgetItem*> groupRows;
  m_groupPaths.clear();
  try {
    for (const QJsonValue& value : m_host.model()
                                       .queryObject({{QStringLiteral("query"), QStringLiteral("parameter_groups")}})
                                       .value(QStringLiteral("groups"))
                                       .toArray()) {
      m_groupPaths << value.toObject().value(QStringLiteral("path")).toString();
    }
  } catch (const std::exception&) {
    m_groupPaths.clear(); // a model without groups
  }
  QStringList groupOrder = m_groupPaths;
  if (m_sortColumn == kName) {
    // Name by name, a parent before its subgroups in either order.
    const Qt::SortOrder order = m_sortOrder;
    std::stable_sort(groupOrder.begin(), groupOrder.end(), [order](const QString& a, const QString& b) {
      const QStringList left = a.split(QLatin1Char('/'));
      const QStringList right = b.split(QLatin1Char('/'));
      for (qsizetype i = 0; i < std::min(left.size(), right.size()); ++i) {
        const int compared = naturalCompare(left.at(i), right.at(i));
        if (compared != 0) {
          return order == Qt::AscendingOrder ? compared < 0 : compared > 0;
        }
      }
      return left.size() < right.size();
    });
  }
  const QSet<QString> collapsed = collapsedGroups().value(m_host.designId());
  for (const QString& path : std::as_const(groupOrder)) {
    QTreeWidgetItem* parent = groupRows.value(parentGroup(path), users);
    auto* row = new QTreeWidgetItem(parent, {groupLeaf(path)});
    row->setData(kName, kGroupRole, path);
    row->setData(kName, NativeControlIdentifierRole, QStringLiteral("parameters.group.user.%1").arg(path));
    row->setData(kName, NativeEditableRole, true);
    row->setIcon(kName, themeIcon(QStringLiteral("folder")));
    row->setToolTip(kName, tr("Group %1: double-click or F2 renames it").arg(path));
    row->setFlags((row->flags() | Qt::ItemIsEditable | Qt::ItemIsDropEnabled) &
                  ~(Qt::ItemIsDragEnabled | Qt::ItemIsUserCheckable));
    row->setFirstColumnSpanned(true);
    groupRows.insert(path, row);
  }
  QHash<QString, QTreeWidgetItem*> owners;
  QStringList names;
  QStringList rows; // for the log
  QStringList limitedRows;
  QStringList favored;
  // The rows in the sorted order (mitcad#1866), each within its group;
  // the groups, the features' among them, in the creation order.
  std::vector<int> order(static_cast<size_t>(parameters.size()));
  std::iota(order.begin(), order.end(), 0);
  if (m_sortColumn >= 0) {
    order = sortedParameterOrder(parameters, static_cast<ParameterColumn>(m_sortColumn), m_sortOrder);
  }
  for (const int index : order) {
    const QJsonObject parameter = parameters.at(index).toObject();
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
    if (parameter.value(QStringLiteral("kind")).toString() != QStringLiteral("user")) {
      const QString owner = parameter.value(QStringLiteral("owner")).toString();
      if (!owners.contains(owner)) {
        auto* group = new QTreeWidgetItem(models, {featureNames.value(owner, owner)});
        group->setData(kName, NativeControlIdentifierRole,
                       QStringLiteral("parameters.group.%1").arg(featureNames.value(owner, owner)));
        group->setExpanded(true);
        owners.insert(owner, group);
      }
    }
  }
  QHash<QString, QString> limitedOf;
  QTreeWidgetItem* selected = nullptr;
  QString lastUser;  // the user parameter the empty row follows (mitcad#2202)
  QStringList inUse; // the user parameters whose delete button is disabled, for the log
  for (const int index : order) {
    const QJsonObject parameter = parameters.at(index).toObject();
    const QString name = parameter.value(QStringLiteral("name")).toString();
    // A user parameter in its group's row (mitcad#2180).
    QTreeWidgetItem* parent = groupRows.value(parameter.value(QStringLiteral("group")).toString(), users);
    if (parameter.value(QStringLiteral("kind")).toString() != QStringLiteral("user")) {
      parent = owners.value(parameter.value(QStringLiteral("owner")).toString(), models);
    } else {
      lastUser = name;
      const QStringList usedBy = parameter.value(QStringLiteral("used_by")).toVariant().toStringList();
      if (!usedBy.isEmpty()) {
        inUse << QStringLiteral("%1 (%2)").arg(name, usedBy.join(QStringLiteral(", ")));
      }
    }
    QTreeWidgetItem* row = addRow(parent, parameter);
    if (name == current) {
      m_table->setCurrentItem(row);
      selected = row;
    }
    // The marked cell and its tooltip, for the log (mitcad#665).
    if (!parameter.value(QStringLiteral("limited")).toString().isEmpty()) {
      limitedOf.insert(name, QStringLiteral("%1 = %2 (%3)")
                                 .arg(name, row->text(kValue),
                                      row->toolTip(kValue).replace(QLatin1Char('\n'), QStringLiteral("; "))));
    }
  }
  // The empty row for a new parameter, last whatever the sort (mitcad#2202).
  addNewRow(users, lastUser);
  for (const QString& name : std::as_const(names)) {
    const auto limited = limitedOf.constFind(name);
    if (limited != limitedOf.cend()) {
      limitedRows << limited.value();
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
  // The groups open but those collapsed in this design (mitcad#2180); the
  // selected group's row selected again.
  for (auto it = groupRows.cbegin(); it != groupRows.cend(); ++it) {
    it.value()->setExpanded(!collapsed.contains(it.key()));
    if (current == rowKey(it.value())) {
      m_table->setCurrentItem(it.value());
      selected = it.value();
    }
  }
  m_building = false;
  m_align->invalidate();
  applyFilter();
  updateButtons();
  if (selected != nullptr && !selected->isHidden()) {
    m_table->scrollToItem(selected);
  }
  logOrder();
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
  // The user parameters whose delete button is disabled, and what uses
  // them (mitcad#2202).
  const QString inUseNames = inUse.join(QStringLiteral("; "));
  if (inUseNames != m_loggedInUse && isVisible()) {
    m_loggedInUse = inUseNames;
    qDebug().noquote() << QStringLiteral("Parameters in use: %1")
                              .arg(inUseNames.isEmpty() ? QStringLiteral("none") : inUseNames);
  }
  // A parameter just made in the empty row: the new empty row takes the
  // keyboard (mitcad#2202), the parameter made stays selected.
  if (std::exchange(m_editNewRow, false)) {
    if (QTreeWidgetItem* row = newRow(); row != nullptr && !row->isHidden()) {
      m_table->scrollToItem(row);
      m_table->setFocus();
      edit(row, kName);
    }
  }
  m_tablesPage->refresh(); // the tables, with the parameters that use them (mitcad#667)
  QTimer::singleShot(0, this, &ParametersDialog::logLayout);
}

void ParametersDialog::edit(QTreeWidgetItem* item, int column) {
  // A group's row: its name renamed in place (mitcad#2180).
  if (item != nullptr && !item->data(kName, kGroupRole).toString().isEmpty()) {
    m_table->editItem(item, kName);
    qDebug().noquote() << QStringLiteral("Editing parameter group %1").arg(item->data(kName, kGroupRole).toString());
    return;
  }
  if (item != nullptr && item->data(kName, kNewRowRole).toBool()) {
    // The empty row (mitcad#2202): its Name, Unit, Expression and Comment.
    if (column != kName && column != kUnit && column != kExpression && column != kComment) {
      return;
    }
    // The row is current (Tab and Enter go by it); the parameter just made
    // stays selected.
    m_table->setCurrentItem(item, column, QItemSelectionModel::NoUpdate);
    m_table->editItem(item, column);
    if (column == kExpression) {
      if (auto* editor = qobject_cast<QLineEdit*>(m_table->itemWidget(item, column))) {
        ParameterFieldOptions suggest;
        suggest.kind = kindOfUnit(m_new.unit);
        suggest.label = QStringLiteral("new parameter expression");
        suggest.create = false;
        // Its fx opens Edit Formula for the parameter not yet made, never
        // the list (mitcad#2290).
        const QPointer<QLineEdit> cellEditor(editor);
        suggest.formula = [this, cellEditor](const QString& text) { formulaForNewRow(cellEditor.data(), text); };
        ParameterField::attach(editor, suggest);
        // The whole text in sight and what is wrong with it in words while
        // it is typed, as in a parameter's cell (mitcad#2179, mitcad#2181).
        connect(editor, &QLineEdit::textChanged, editor,
                [editor] { fitWholeText(editor, editor->property(kCellRectProperty).toRect()); });
        fitWholeText(editor, editor->property(kCellRectProperty).toRect());
        connect(editor, &QLineEdit::textEdited, this, [this, editor](const QString& text) {
          checkCellExpression(m_host, m_message, editor, m_new.name, m_new.unit, text, true);
        });
      }
    }
    qDebug().noquote() << QStringLiteral("Editing new parameter %1").arg(columnLogName(column));
    return;
  }
  if (item == nullptr || item->data(kName, kNameRole).toString().isEmpty() || column == kValue ||
      column == kFavorite || column == kStrict || column == kSlider) {
    return;
  }
  // A driven dimension's reference has no limits (mitcad#665), nor has a
  // text parameter (mitcad#664).
  if ((column == kMin || column == kMax || column == kStep) &&
      (item->data(kName, kKindRole).toString() == QStringLiteral("reference") ||
       isTextUnit(item->data(kName, kUnitRole).toString()))) {
    return;
  }
  if ((column == kName || column == kExpression) && item->data(kName, kHeldRole).toBool()) {
    // The open panel's own value (mitcad#564).
    const QString name = item->data(kName, kNameRole).toString();
    setMessage(m_scope.holdReason(name));
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
    // Parameter suggestions in the expression's editor (mitcad#567); its
    // fx opens Edit Formula with the text typed so far, ending the cell's
    // editing without applying it (mitcad#2179).
    if (auto* editor = qobject_cast<QLineEdit*>(m_table->itemWidget(item, column))) {
      ParameterFieldOptions suggest;
      suggest.kind = kindOfUnit(item->data(kName, kUnitRole).toString());
      suggest.own = item->data(kName, kNameRole).toString();
      suggest.label = QStringLiteral("%1 expression").arg(suggest.own);
      suggest.create = false;
      const QString unitOfRow = item->data(kName, kUnitRole).toString();
      const QPointer<QLineEdit> cellEditor(editor);
      suggest.formula = [this, name = suggest.own, unitOfRow, cellEditor](const QString& text) {
        formulaFromCell(cellEditor.data(), name, unitOfRow, text);
      };
      ParameterField::attach(editor, suggest);
      // The whole text in sight while it is typed (mitcad#2179).
      connect(editor, &QLineEdit::textChanged, editor,
              [editor] { fitWholeText(editor, editor->property(kCellRectProperty).toRect()); });
      fitWholeText(editor, editor->property(kCellRectProperty).toRect());
      // What the limits would make of it, while it is typed (mitcad#665).
      if (item->data(kName, kHasLimitsRole).toBool()) {
        const QString name = item->data(kName, kNameRole).toString();
        const QString unit = item->data(kName, kUnitRole).toString();
        connect(editor, &QLineEdit::textEdited, this, [this, name, unit](const QString& text) {
          previewExpression(name, withUnit(text.trimmed(), unit));
        });
      }
      // What is wrong with it in words, the part it is about marked in the
      // editor (mitcad#2181); after the limits' preview, which a problem
      // takes the place of.
      {
        const QString name = item->data(kName, kNameRole).toString();
        const QString unit = item->data(kName, kUnitRole).toString();
        connect(editor, &QLineEdit::textEdited, this, [this, editor, name, unit](const QString& text) {
          checkCellExpression(m_host, m_message, editor, name, unit, text);
        });
      }
    }
  }
  qDebug().noquote() << QStringLiteral("Editing parameter %1 %2")
                            .arg(item->data(kName, kNameRole).toString(), columnLogName(column));
}

void ParametersDialog::cellEdited(QTreeWidgetItem* item, int column) {
  // A group's row renamed in place (mitcad#2180).
  const QString group = item->data(kName, kGroupRole).toString();
  if (!m_building && !group.isEmpty() && column == kName) {
    renameGroup(group, item->text(kName));
    return;
  }
  if (item->data(kName, kNewRowRole).toBool()) {
    if (!m_building) {
      newRowEdited(item, column);
    }
    return;
  }
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
    // The Unit list writes the model's unit, empty for Number (mitcad#1920).
    text = unitOfText(text);
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("unit"), text);
    break;
  case kExpression:
    command.insert(QStringLiteral("cmd"), QStringLiteral("set_parameter"));
    command.insert(QStringLiteral("expression"), withUnit(text, item->data(kName, kUnitRole).toString()));
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
      setMessage(QString());
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
      // A renamed row stays selected under its new name (mitcad#1866).
      if (column == kName) {
        m_select = text;
      }
      // A limit took effect (mitcad#665): said at once.
      if (!limitMessage.isEmpty()) {
        setMessage(limitMessage);
        qDebug().noquote() << QStringLiteral("Parameter %1 limited: %2").arg(shown, limitMessage);
      }
    } else if (m_host.lastCancelled()) {
      // Cancelled while it computed (P7): no refusal, and the cell shows
      // the parameter as it still is.
      m_refused.remove(refusedKey(name, column));
      setMessage(tr("%1: the change was canceled.").arg(name));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 cancelled").arg(name, what, text);
    } else {
      // The model's reason stays with the cell; the document is unchanged.
      const QString problem = mitcad::modelMessage(m_host.lastError());
      m_refused.insert(refusedKey(name, column), {text, problem});
      setMessage(refusalLine(name, problem));
      qDebug().noquote() << QStringLiteral("Parameter %1 %2: %3 refused: %4").arg(name, what, text, problem);
    }
    refresh();
  });
}

// The empty row (mitcad#2202).
QTreeWidgetItem* ParametersDialog::newRow() const {
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    if ((*it)->data(kName, kNewRowRole).toBool()) {
      return *it;
    }
  }
  return nullptr;
}

// The empty row at the end of User Parameters, after `after` (the last
// user parameter as sorted, empty when there is none), with what was typed
// in it and not yet made a parameter, and the model's refusal of it.
void ParametersDialog::addNewRow(QTreeWidgetItem* users, const QString& after) {
  if (!m_new.unitChosen) {
    // The design's length unit first (mitcad#1452), as Add User Parameter.
    m_new.unit = m_host.model().lengthUnit();
  }
  auto* row = new QTreeWidgetItem(users);
  row->setData(kName, kNewRowRole, true);
  row->setText(kName, m_new.name);
  row->setText(kUnit, unitDisplayText(m_new.unit));
  row->setText(kExpression, m_new.expression);
  row->setText(kComment, m_new.comment);
  row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
  row->setToolTip(kName, tr("A new user parameter: its name, Tab, its expression, Enter"));
  for (int column = kName; column <= kDelete; ++column) {
    const bool editable = column == kName || column == kUnit || column == kExpression || column == kComment;
    row->setData(column, NativeEditableRole, editable);
    if (editable) {
      row->setData(column, NativeControlIdentifierRole, QStringLiteral("parameters.new.%1").arg(column));
    }
  }
  if (!m_new.refusal.isEmpty()) {
    // Refused: kept as typed, with the reason (mitcad#2202).
    for (const int column : {kName, kExpression}) {
      if (!mac::styleRefusedParameter(row, column)) {
        row->setForeground(column, refusedText());
        row->setBackground(column, refusedFill());
      }
      row->setToolTip(column, m_new.refusal);
    }
  }
  // What it holds: its unit, where it is, and what was typed and kept.
  QString logged = QStringLiteral("Parameters new row: unit %1, after %2")
                       .arg(unitDisplayText(m_new.unit), after.isEmpty() ? QStringLiteral("none") : after);
  if (!m_new.name.isEmpty() || !m_new.expression.isEmpty()) {
    logged += QStringLiteral(", typed %1 = %2").arg(m_new.name, m_new.expression);
  }
  if (!m_new.refusal.isEmpty()) {
    logged += QStringLiteral(", refused: %1").arg(m_new.refusal);
  }
  if (logged != m_loggedNewRow && isVisible()) {
    m_loggedNewRow = logged;
    qDebug().noquote() << logged;
  }
}

// A cell of the empty row typed in: kept until the row becomes a
// parameter. A unit is shown as the Unit list shows it (Number for none).
void ParametersDialog::newRowEdited(QTreeWidgetItem* item, int column) {
  const QString text = item->text(column);
  switch (column) {
  case kName:
    m_new.name = text.trimmed();
    break;
  case kUnit:
    m_new.unit = unitOfText(text);
    m_new.unitChosen = true;
    m_building = true;
    item->setText(kUnit, unitDisplayText(m_new.unit));
    m_building = false;
    break;
  case kExpression:
    m_new.expression = text.trimmed();
    break;
  case kComment:
    m_new.comment = text;
    break;
  default:
    return;
  }
  if (!m_new.refusal.isEmpty()) {
    // Typed again: no longer the refused row.
    m_new.refusal.clear();
    m_building = true;
    for (const int refused : {kName, kExpression}) {
      item->setData(refused, Qt::ForegroundRole, QVariant());
      item->setData(refused, Qt::BackgroundRole, QVariant());
      item->setData(refused, NativeRefusedRole, QVariant());
      item->setToolTip(refused, refused == kName ? tr("A new user parameter: its name, Tab, its expression, Enter")
                                                 : QString());
    }
    m_building = false;
  }
  qDebug().noquote() << QStringLiteral("New parameter %1: %2")
                            .arg(columnLogName(column), column == kUnit ? unitDisplayText(m_new.unit) : text);
}

// An editor of the empty row closed (ParameterTable::closeEditor): Tab and
// Shift+Tab go along the row, Name, Expression, Comment, Unit and round
// (the unit starts as the design's, so a value follows the name); Enter
// makes the row a parameter; leaving the table does when the row has a
// name and an expression; Escape leaves the cell as it was.
void ParametersDialog::newRowEditorClosed(int column, int hint) {
  // Closed by its fx for Edit Formula (mitcad#2290): the window decides.
  if (m_building || m_newRowFormula) {
    return;
  }
  switch (static_cast<QAbstractItemDelegate::EndEditHint>(hint)) {
  case QAbstractItemDelegate::EditNextItem:
  case QAbstractItemDelegate::EditPreviousItem: {
    static constexpr int kOrder[] = {kName, kExpression, kComment, kUnit};
    constexpr int count = static_cast<int>(std::size(kOrder));
    const int* at = std::find(std::begin(kOrder), std::end(kOrder), column);
    int next = at == std::end(kOrder) ? 0 : static_cast<int>(at - std::begin(kOrder));
    next = (next + (hint == QAbstractItemDelegate::EditNextItem ? 1 : count - 1)) % count;
    if (QTreeWidgetItem* row = newRow()) {
      edit(row, kOrder[next]);
    }
    return;
  }
  case QAbstractItemDelegate::SubmitModelCache:
    createFromNewRow(true);
    return;
  case QAbstractItemDelegate::NoHint:
    // Left for another row: currentItemChanged; for another control of
    // the window: here, once the focus has moved.
    QTimer::singleShot(0, this, [this] {
      QWidget* focus = QApplication::focusWidget();
      if (focus != m_table && !m_table->isAncestorOf(focus)) {
        createFromNewRow(false);
      }
    });
    return;
  case QAbstractItemDelegate::RevertModelCache:
    return;
  }
}

// The empty row made a user parameter (mitcad#2202): `add_parameter` with
// what was typed, one undo step. `asked` (Enter) says what is missing;
// leaving a row with only a name or only an expression keeps it as typed.
void ParametersDialog::createFromNewRow(bool asked) {
  // The model command refreshes this table: not while the table is still
  // handling the edit that ended.
  afterEventWhenIdle(m_host, this, [this, asked] {
    const QString name = m_new.name.trimmed();
    const QString unit = m_new.unit;
    const QString expression = withUnit(m_new.expression.trimmed(), unit);
    if (name.isEmpty() && expression.isEmpty()) {
      return;
    }
    QString problem;
    if (name.isEmpty() || expression.isEmpty()) {
      if (!asked) {
        return;
      }
      problem = name.isEmpty() ? tr("Give the new parameter a name.") : tr("Give %1 an expression.").arg(name);
    } else {
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                                {QStringLiteral("name"), name},
                                {QStringLiteral("expression"), expression},
                                {QStringLiteral("unit"), unit},
                                {QStringLiteral("comment"), m_new.comment}};
      if (m_host.runParameterCommand(command)) {
        m_new = NewRow();
        setMessage(QString());
        qDebug().noquote() << QStringLiteral("Created user parameter %1 = %2 (%3) in the new row")
                                  .arg(name, expression, unitDisplayText(unit));
        m_select = name;      // selected where the sort puts it (mitcad#1866)
        m_editNewRow = true;  // and the next one typed in the new empty row
        refresh();
        return;
      }
      if (m_host.lastCancelled()) {
        setMessage(tr("%1: the change was canceled.").arg(name));
        qDebug().noquote() << QStringLiteral("New user parameter %1 cancelled").arg(name);
        refresh();
        return;
      }
      problem = mitcad::modelMessage(m_host.lastError());
    }
    m_new.refusal = problem;
    setMessage(name.isEmpty() ? tr("New parameter: %1").arg(problem) : tr("New parameter %1: %2").arg(name, problem));
    qDebug().noquote() << QStringLiteral("New user parameter %1 refused: %2")
                              .arg(name.isEmpty() ? QStringLiteral("(no name)") : name, problem);
    refresh();
  });
}

// A row's delete button (mitcad#2202): `delete_parameter`, one undo step,
// no question. The model refuses a parameter something uses (the button
// is disabled then, but the design may have changed meanwhile).
void ParametersDialog::deleteParameter(const QString& name) {
  // The button goes with the rows the command refreshes: after its click.
  afterEventWhenIdle(m_host, this, [this, name] {
    if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("delete_parameter")},
                                    {QStringLiteral("name"), name}})) {
      setMessage(QString());
      qDebug().noquote() << QStringLiteral("Deleted user parameter %1 by its row button").arg(name);
    } else if (m_host.lastCancelled()) {
      setMessage(tr("%1: the change was canceled.").arg(name));
      qDebug().noquote() << QStringLiteral("Delete user parameter %1 cancelled").arg(name);
    } else {
      const QString problem = mitcad::modelMessage(m_host.lastError());
      setMessage(refusalLine(name, problem));
      qDebug().noquote() << QStringLiteral("Delete user parameter %1 refused: %2").arg(name, problem);
    }
    refresh();
  });
}

void ParametersDialog::setMessage(const QString& text) {
  m_message->setText(text);
  m_message->setVisible(!text.isEmpty());
}

void ParametersDialog::exportParameters() {
  QJsonObject table;
  try {
    table = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("parameters_csv")}});
  } catch (const std::exception& e) {
    setMessage(mitcad::modelMessage(e.what()));
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
    setMessage(problem);
    qDebug().noquote() << QStringLiteral("Export of the parameters refused: %1").arg(problem);
    return;
  }
  file.close();
  setMessage(QString());
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
    setMessage(problem);
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
    setMessage(QStringLiteral("%1: %2").arg(file, problem));
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
    setMessage(QString());
    const QString summary = result.value(QStringLiteral("summary")).toString();
    m_host.showStatus(tr("Imported parameters from %1: %2.").arg(file, summary), NoticeLevel::Information);
    qDebug().noquote() << QStringLiteral("Imported parameters from %1: %2").arg(file, summary);
  } else if (m_host.lastCancelled()) {
    setMessage(tr("%1: the import was canceled.").arg(file));
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 cancelled while computing").arg(file);
  } else {
    setMessage(QStringLiteral("%1: %2").arg(file, mitcad::modelMessage(m_host.lastError())));
    qDebug().noquote() << QStringLiteral("Import of parameters from %1 refused: %2").arg(file, m_host.lastError());
  }
  refresh();
}

void ParametersDialog::editFormula() {
  QTreeWidgetItem* item=m_table->currentItem();
  if (!item) return;
  // An Expression cell being edited (the button takes no keyboard focus on
  // a click, so the cell's editor stays open): as its fx, with what is
  // typed in it so far (mitcad#2290).
  if (auto* editor = qobject_cast<QLineEdit*>(m_table->itemWidget(item, kExpression))) {
    if (ParameterField* field = ParameterField::of(editor); field != nullptr && field->options().formula) {
      field->activateFx();
      return;
    }
  }
  // The empty row (mitcad#2290): Edit Formula for the parameter not yet
  // made, with what its Expression holds.
  if (item->data(kName, kNewRowRole).toBool()) {
    formulaForNewRow(nullptr, m_new.expression);
    return;
  }
  const QString name=item->data(kName,kNameRole).toString();
  if (name.isEmpty() || item->data(kName,kKindRole).toString()==QStringLiteral("reference")) return;
  if (item->data(kName,kHeldRole).toBool()) {
    setMessage(m_scope.holdReason(name));
    return;
  }
  runFormulaEditor(name, item->data(kName, kUnitRole).toString(), item->text(kExpression));
}

void ParametersDialog::formulaFromCell(QLineEdit* editor, const QString& name, const QString& unit,
                                       const QString& text) {
  // The cell's editing ends without applying what it holds (no commit:
  // the window opens with it, and Cancel leaves the parameter as it was).
  if (editor != nullptr) {
    emit m_align->closeEditor(editor, QAbstractItemDelegate::NoHint);
  }
  // Out of the fx button's handling first: a window opens.
  QTimer::singleShot(0, this, [this, name, unit, text] { runFormulaEditor(name, unit, text); });
}

void ParametersDialog::runFormulaEditor(const QString& name, const QString& unit, const QString& text) {
  const ParameterSource* source=parameterSourceOf(this);
  if (!source || !source->query) return;
  ParameterFieldOptions options;
  options.own=name;
  options.label=name;
  options.kind=kindOfUnit(unit);
  options.anyKind=true;
  FormulaEditor dialog(*source,options,text,this);
  qDebug().noquote() << QStringLiteral("Formula editor opened for parameter %1 with: %2").arg(name, text);
  if (execDialog(&dialog)!=QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Formula editor for parameter %1 canceled").arg(name);
    return;
  }
  // The formula and its limits as one parameter command: one undo step.
  submit(name,kExpression,dialog.expression(),QStringLiteral("Edited formula"),dialog.parameterCommand());
}

void ParametersDialog::formulaForNewRow(QLineEdit* editor, const QString& text) {
  // The cell's editing ends without applying what it holds; its closing
  // makes no parameter of the row while the window is open.
  m_newRowFormula = true;
  if (editor != nullptr) {
    emit m_align->closeEditor(editor, QAbstractItemDelegate::NoHint);
  }
  // Out of the fx button's handling first: a window opens.
  QTimer::singleShot(0, this, [this, text] { runNewRowFormula(text); });
}

void ParametersDialog::runNewRowFormula(const QString& text) {
  const ParameterSource* source = parameterSourceOf(this);
  if (source == nullptr || !source->query) {
    m_newRowFormula = false;
    return;
  }
  // No parameter yet: no own name (none to leave out, no limits to show:
  // the window says that limits belong to a saved parameter), the row's
  // unit as the kind.
  const QString name = m_new.name.trimmed();
  const QString shownName = name.isEmpty() ? QStringLiteral("(no name)") : name;
  ParameterFieldOptions options;
  options.label = name;
  options.kind = kindOfUnit(m_new.unit);
  options.anyKind = true;
  FormulaEditor dialog(*source, options, text, this);
  qDebug().noquote() << QStringLiteral("Formula editor opened for new parameter %1 with: %2").arg(shownName, text);
  const bool accepted = execDialog(&dialog) == QDialog::Accepted;
  m_newRowFormula = false;
  if (!accepted) {
    qDebug().noquote() << QStringLiteral("Formula editor for new parameter %1 canceled").arg(shownName);
    return;
  }
  // As if typed in the row and Enter pressed: `add_parameter`, one undo
  // step; without a name the row keeps the formula and asks for one.
  m_new.expression = dialog.expression();
  m_new.refusal.clear();
  qDebug().noquote() << QStringLiteral("New parameter %1 expression from Edit Formula: %2")
                            .arg(shownName, m_new.expression);
  createFromNewRow(true);
}

void ParametersDialog::updateButtons() {
  QTreeWidgetItem* item = m_table->currentItem();
  // Also the empty row's: Edit Formula for the parameter not yet made
  // (mitcad#2290).
  m_formula->setEnabled(item != nullptr &&
                        (item->data(kName, kNewRowRole).toBool() ||
                         (!item->data(kName, kNameRole).toString().isEmpty() &&
                          item->data(kName, kKindRole).toString() != QStringLiteral("reference"))));
  // An import changes many parameters at once: not while a sketch or a
  // panel holds some (mitcad#666).
  m_import->setEnabled(m_scope.isEmpty());
  m_import->setToolTip(m_scope.isEmpty()
                           ? tr("Import parameters from a CSV file")
                           : tr("Finish the sketch or the command to import parameters"));
}

void ParametersDialog::applyFilter() {
  const QString filter = m_filter->text().trimmed();
  const bool limitedOnly = m_limitedOnly != nullptr && m_limitedOnly->isChecked();
  // A parameter group whose name (or a parent group's) matches shows all
  // its parameters (mitcad#2180).
  const auto groupMatches = [&filter](const QTreeWidgetItem* item) {
    for (; item != nullptr; item = item->parent()) {
      if (!item->data(kName, kGroupRole).toString().isEmpty() &&
          item->text(kName).contains(filter, Qt::CaseInsensitive)) {
        return true;
      }
    }
    return false;
  };
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    if (item->data(kName, kNewRowRole).toBool()) {
      // The empty row (mitcad#2202) is no parameter a search or Only
      // limited finds: hidden while either filters the rows.
      const bool hidden = !filter.isEmpty() || limitedOnly;
      item->setHidden(hidden);
      if (static_cast<int>(hidden) != m_loggedNewHidden && isVisible()) {
        m_loggedNewHidden = static_cast<int>(hidden);
        qDebug().noquote() << (hidden ? QStringLiteral("Parameters new row hidden by the filter")
                                      : QStringLiteral("Parameters new row shown"));
      }
      continue;
    }
    if (item->data(kName, kNameRole).toString().isEmpty()) {
      continue; // a section or a group
    }
    const bool shown = (filter.isEmpty() || item->text(kName).contains(filter, Qt::CaseInsensitive) ||
                        item->text(kComment).contains(filter, Qt::CaseInsensitive) ||
                        (sectionOf(item) == QStringLiteral("users") && groupMatches(item->parent()))) &&
                       (!limitedOnly || !item->data(kName, kLimitedRole).toString().isEmpty());
    item->setHidden(!shown);
  }
  // A group's row shows while nothing is filtered, and else when it holds
  // a shown parameter or its name matches.
  const std::function<bool(QTreeWidgetItem*)> showsRows = [&](QTreeWidgetItem* item) {
    bool any = false;
    for (int i = 0; i < item->childCount(); ++i) {
      QTreeWidgetItem* child = item->child(i);
      if (!child->data(kName, kGroupRole).toString().isEmpty()) {
        const bool holds = showsRows(child); // its subgroups first
        const bool shown = (filter.isEmpty() && !limitedOnly) || holds ||
                           (!filter.isEmpty() && !limitedOnly && groupMatches(child));
        child->setHidden(!shown);
        any = any || shown;
      } else if (!child->data(kName, kNameRole).toString().isEmpty()) {
        any = any || !child->isHidden();
      }
    }
    return any;
  };
  for (int i = 0; i < m_table->topLevelItemCount(); ++i) {
    showsRows(m_table->topLevelItem(i));
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
    // A user parameter's delete button (mitcad#2202).
    if (m_table->itemWidget(item, kDelete) != nullptr) {
      const QRect rect = m_table->visualItemRect(item);
      const QPoint center(m_table->header()->sectionViewportPosition(kDelete) +
                              m_table->header()->sectionSize(kDelete) / 2,
                          rect.center().y());
      lines << QStringLiteral("Parameters %1 delete at %2").arg(name, at(m_table->viewport(), center));
    }
  }
  // The empty row's cells (mitcad#2202).
  if (QTreeWidgetItem* row = newRow(); row != nullptr && !row->isHidden()) {
    const QRect rect = m_table->visualItemRect(row);
    for (const int column : {kName, kUnit, kExpression, kComment}) {
      const QPoint center(m_table->header()->sectionViewportPosition(column) +
                              m_table->header()->sectionSize(column) / 2,
                          rect.center().y());
      lines << QStringLiteral("Parameters new %1 at %2").arg(columnLogName(column), at(m_table->viewport(), center));
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
    const QRect handle = sliderHandleRect(slider, cell.size());
    lines << QStringLiteral("Parameters %1 slider at %2").arg(name, at(m_table->viewport(), cell.center()));
    lines << QStringLiteral("Parameters %1 slider handle at %2")
                 .arg(name, at(m_table->viewport(), cell.topLeft() + handle.center()));
  }
  // The groups' rows (mitcad#2180): in the Name column, where a click
  // selects them; none inside a collapsed group.
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    const QString group = (*it)->data(kName, kGroupRole).toString();
    const QRect rect = m_table->visualItemRect(*it);
    if (group.isEmpty() || (*it)->isHidden() || rect.isEmpty()) {
      continue;
    }
    const QPoint point(m_table->header()->sectionViewportPosition(kName) + m_table->header()->sectionSize(kName) / 2,
                       rect.center().y());
    lines << QStringLiteral("Parameters group %1 at %2").arg(group, at(m_table->viewport(), point));
  }
  // The columns' headers, which sort the rows (mitcad#1866).
  QHeaderView* header = m_table->header();
  for (int column = kName; column <= kSlider; ++column) {
    if (header->isSectionHidden(column)) {
      continue;
    }
    const QPoint center(header->sectionViewportPosition(column) + header->sectionSize(column) / 2,
                        header->viewport()->height() / 2);
    lines << QStringLiteral("Parameters header %1 at %2").arg(columnLogName(column), at(header->viewport(), center));
  }
  lines << QStringLiteral("Parameters only limited at %1")
               .arg(at(m_limitedOnly, QPoint(10, m_limitedOnly->rect().center().y())));
  lines << QStringLiteral("Parameters new group at %1").arg(at(m_newGroup, m_newGroup->rect().center()));
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
    m_loggedOrder.clear();
    m_logged.clear(); // the places too (mitcad#2180: a group's row)
    m_loggedNewRow.clear(); // the empty row and the buttons too (mitcad#2202)
    m_loggedInUse.clear();
    m_loggedNewHidden = -1;
  }
  const bool handled = QDialog::event(event);
  // A window narrower than the columns want (a small screen clamps it):
  // the columns narrowed so that the delete buttons stay in view.
  if (event->type() == QEvent::Resize || event->type() == QEvent::Show) {
    fitColumns();
    QTimer::singleShot(0, this, &ParametersDialog::logLayout);
  }
  return handled;
}

void ParametersDialog::fitColumns() {
  if (m_columnsByUser) {
    return;
  }
  struct Width {
    int column;
    int wanted;
    int least;
  };
  // Unit, the check boxes and the delete button keep theirs; Comment
  // stretches over what is left, at least kCommentLeast.
  static constexpr Width kWidths[] = {
      {kName, 170, 110},
      {kUnit, 60, 60},
      {kExpression, 190, 120},
      {kValue, 130, 100}, // room for an aligned number with its unit (mitcad#566)
      {kFavorite, 32, 32},
      {kMin, 90, 60},
      {kMax, 90, 60},
      {kStep, 70, 50},
      {kStrict, 50, 50},
      {kSlider, 120, 70},
      {kDelete, 30, 30},
  };
  constexpr int kCommentLeast = 80;
  int wanted = kCommentLeast;
  int slack = 0;
  for (const Width& width : kWidths) {
    wanted += width.wanted;
    slack += width.wanted - width.least;
  }
  // Before the first show the viewport has no width of its own yet.
  const int room = m_table->isVisible() ? m_table->viewport()->width() : wanted;
  const int missing = std::clamp(wanted - room, 0, slack);
  m_fittingColumns = true;
  for (const Width& width : kWidths) {
    const int give = slack > 0 ? (width.wanted - width.least) * missing / slack : 0;
    m_table->header()->resizeSection(width.column, width.wanted - give);
  }
  m_fittingColumns = false;
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
  // The expression of a handle's position, in the parameter's unit.
  const auto textAt = [unit, minNumber, maxNumber, increment, positions](int position) {
    const double value = position == positions && increment * positions > maxNumber - minNumber
                             ? maxNumber
                             : minNumber + increment * position;
    return withUnit(QString::number(value, 'g', 12), unit);
  };
  const auto commit = [this, name, textAt, at](int position) {
    if (position == at) {
      return; // the handle clicked, not moved
    }
    const QString text = textAt(position);
    submit(name, kSlider, text, QStringLiteral("slider"),
           {{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
            {QStringLiteral("name"), name},
            {QStringLiteral("expression"), text}});
  };
  // While the handle is dragged (mitcad#2202): the row's Expression and
  // Value and a tooltip at the handle show the value at the handle; the
  // model gets it when the handle is released, as before.
  connect(slider, &QSlider::sliderMoved, this, [this, row, slider, name, textAt](int position) {
    const QString text = textAt(position);
    m_building = true; // shown, not edited: no command
    row->setText(kExpression, text);
    row->setText(kValue, text);
    m_building = false;
    m_align->invalidate();
    const QRect handle = sliderHandleRect(slider, slider->size());
    QToolTip::showText(slider->mapToGlobal(QPoint(handle.center().x(), handle.top())), text, slider);
    const QString logged = QStringLiteral("Parameter %1 slider held at %2: expression %3, value %4, tooltip %5")
                               .arg(name, text, row->text(kExpression), row->text(kValue),
                                    QToolTip::isVisible() ? QToolTip::text() : QStringLiteral("none"));
    if (logged != m_loggedHeld) {
      m_loggedHeld = logged;
      qDebug().noquote() << logged;
    }
  });
  connect(slider, &QSlider::sliderReleased, this, [this, slider, commit] {
    QToolTip::hideText();
    m_loggedHeld.clear();
    commit(slider->value());
  });
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
      const QString problem = mitcad::modelMessage(userText(QString::fromUtf8(e.what())));
      if (problem.contains(QStringLiteral("minimum")) || problem.contains(QStringLiteral("maximum")) ||
          problem.contains(QStringLiteral("not on a step"))) {
        message = tr("%1: will be refused: %2").arg(name, problem);
      }
    }
  }
  setMessage(message);
  if (message != m_loggedPreview) {
    m_loggedPreview = message;
    qDebug().noquote() << QStringLiteral("Parameter %1 typed %2: %3")
                              .arg(name, text, message.isEmpty() ? QStringLiteral("within its limits") : message);
  }
}

// A header's click (mitcad#1866): the rows sorted by its column, or in the
// creation order again; kept for the next opening.
void ParametersDialog::sortBy(int column, Qt::SortOrder order) {
  if (column == kDelete) {
    // The delete buttons' column sorts nothing (mitcad#2202).
    const QSignalBlocker blocker(m_table->header());
    m_table->header()->setSortIndicator(m_sortColumn, m_sortOrder);
    return;
  }
  if (column < 0 || column >= m_table->columnCount()) {
    column = -1;
  }
  if (column == m_sortColumn && (column < 0 || order == m_sortOrder)) {
    return;
  }
  m_sortColumn = column;
  m_sortOrder = order;
  const QString sort =
      column < 0 ? QStringLiteral("none")
                 : QStringLiteral("%1 %2").arg(columnLogName(column), order == Qt::AscendingOrder
                                                                          ? QStringLiteral("ascending")
                                                                          : QStringLiteral("descending"));
  QSettings().setValue(QLatin1String(kSortSetting), sort);
  qDebug().noquote() << (column < 0 ? QStringLiteral("Parameters sorted in creation order")
                                    : QStringLiteral("Parameters sorted by %1").arg(sort));
  refresh();
}

// The rows as shown, group by group (mitcad#1866): "[User Parameters] a,
// b [Model Parameters] [Extrude1] d1", and the selected row.
void ParametersDialog::logOrder() {
  if (!isVisible()) {
    return;
  }
  QString order;
  bool afterRow = false;
  // A section's or a group's label; a group's is its path (mitcad#2180).
  const auto label = [](const QTreeWidgetItem* item) {
    const QString group = item->data(kName, kGroupRole).toString();
    return group.isEmpty() ? item->text(kName) : group;
  };
  const QTreeWidgetItem* heading = nullptr; // the last label written
  QStringList collapsed;
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    if ((*it)->data(kName, kNewRowRole).toBool()) {
      continue; // the empty row: "Parameters new row" (mitcad#2202)
    }
    const QString name = (*it)->data(kName, kNameRole).toString();
    if (name.isEmpty()) {
      order += QStringLiteral("%1[%2]").arg(order.isEmpty() ? QString() : QStringLiteral(" "), label(*it));
      afterRow = false;
      heading = *it;
      if (!(*it)->data(kName, kGroupRole).toString().isEmpty() && !(*it)->isExpanded()) {
        collapsed << label(*it);
      }
    } else {
      // A row after its group's subgroups: its group's label again.
      if ((*it)->parent() != nullptr && heading != nullptr && (*it)->parent() != heading) {
        heading = (*it)->parent();
        order += QStringLiteral(" [%1]").arg(label(heading));
        afterRow = false;
      }
      order += (afterRow ? QStringLiteral(", ") : QStringLiteral(" ")) + name;
      afterRow = true;
    }
  }
  if (!collapsed.isEmpty()) {
    order += QStringLiteral("; collapsed %1").arg(collapsed.join(QStringLiteral(", ")));
  }
  // The selected row: the empty row can be edited while the row just made
  // in it stays selected (mitcad#2202); of several selected, the current
  // one (mitcad#2180).
  const QTreeWidgetItem* current = m_table->currentItem();
  if (current == nullptr || !current->isSelected()) {
    const QList<QTreeWidgetItem*> chosen = m_table->selectedItems();
    current = chosen.isEmpty() ? nullptr : chosen.first();
  }
  const QString selected = rowKey(current);
  if (!selected.isEmpty()) {
    order += QStringLiteral("; selected %1").arg(selected);
  }
  if (order != m_loggedOrder) {
    m_loggedOrder = order;
    qDebug().noquote() << QStringLiteral("Parameters order: %1").arg(order);
  }
}

// Groups of user parameters (mitcad#2180).

QStringList ParametersDialog::selectedUserParameters() const {
  QStringList names;
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    const QString name = (*it)->data(kName, kNameRole).toString();
    if ((*it)->isSelected() && (*it)->data(kName, kKindRole).toString() == QStringLiteral("user") &&
        !names.contains(name)) {
      names << name;
    }
  }
  return names;
}

QString ParametersDialog::selectedGroup(bool ofParameter) const {
  const QTreeWidgetItem* item = m_table->currentItem();
  if (item == nullptr) {
    return {};
  }
  const QString group = item->data(kName, kGroupRole).toString();
  if (!group.isEmpty() || !ofParameter || item->data(kName, kKindRole).toString() != QStringLiteral("user")) {
    return group;
  }
  return item->data(kName, kInGroupRole).toString();
}

QString ParametersDialog::askGroupName(const QString& title, const QString& parent,
                                       const std::function<QString(const QString&)>& make) {
  GroupNameDialog dialog(title, parent, make, this);
  qDebug().noquote() << QStringLiteral("%1 dialog opened%2")
                            .arg(title, parent.isEmpty() ? QString() : QStringLiteral(" in %1").arg(parent));
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("%1 cancelled").arg(title);
    return {};
  }
  return dialog.path();
}

// New Group…: a group in the selected group, or at the top.
void ParametersDialog::newGroup() {
  const QString path = askGroupName(tr("New Group"), selectedGroup(false), [this](const QString& made) {
    if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("add_parameter_group")},
                                    {QStringLiteral("group"), made}})) {
      return QString();
    }
    return mitcad::modelMessage(m_host.lastError());
  });
  if (!path.isEmpty()) {
    setMessage(QString());
    m_select = QStringLiteral("group ") + path;
    qDebug().noquote() << QStringLiteral("Parameter group added: %1").arg(path);
  }
  refresh();
}

// A group's row renamed in place to `name`: the group keeps its parent.
void ParametersDialog::renameGroup(const QString& path, const QString& name) {
  const QString leaf = plainGroupPath(name);
  const QString parent = parentGroup(path);
  const QString renamed = leaf.isEmpty() ? QString() : parent.isEmpty() ? leaf : parent + QLatin1Char('/') + leaf;
  // The model command refreshes the table: not while it handles its edit.
  afterEventWhenIdle(m_host, this, [this, path, renamed] {
    if (renamed == path) {
      refresh();
      return;
    }
    if (!renamed.isEmpty() && m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("rename_parameter_group")},
                                                          {QStringLiteral("group"), path},
                                                          {QStringLiteral("new_group"), renamed}})) {
      // The collapsed groups follow the new path.
      QSet<QString>& collapsed = collapsedGroups()[m_host.designId()];
      QSet<QString> moved;
      for (const QString& group : std::as_const(collapsed)) {
        moved.insert(inGroup(group, path) ? renamed + group.mid(path.size()) : group);
      }
      collapsed = moved;
      setMessage(QString());
      m_select = QStringLiteral("group ") + renamed;
      qDebug().noquote() << QStringLiteral("Parameter group %1 renamed to %2").arg(path, renamed);
    } else {
      const QString problem = renamed.isEmpty() ? tr("Give the group a name.") : mitcad::modelMessage(m_host.lastError());
      setMessage(QStringLiteral("%1: %2").arg(path, problem));
      qDebug().noquote() << QStringLiteral("Parameter group %1 rename to %2 refused: %3").arg(path, renamed, problem);
    }
    refresh();
  });
}

// Delete on a group's row: asked only when it holds parameters, which
// move up a level (none is deleted).
void ParametersDialog::deleteGroup(const QString& path) {
  QStringList own; // the parameters in the group itself
  int held = 0;    // and those in its subgroups too
  for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
    const QString name = (*it)->data(kName, kNameRole).toString();
    const QString in = (*it)->data(kName, kInGroupRole).toString();
    if (name.isEmpty() || sectionOf(*it) != QStringLiteral("users") || in.isEmpty() || !inGroup(in, path)) {
      continue;
    }
    ++held;
    if (in == path) {
      own << name;
    }
  }
  const QString parent = parentGroup(path);
  const QString destination = parent.isEmpty() ? tr("User Parameters") : parent;
  if (held > 0) {
    QMessageBox box(QMessageBox::Question, tr("Delete Group"),
                    tr("Delete the group %1? Its %n parameter(s) move up a level, to %2; no parameter is deleted.",
                       nullptr, held)
                        .arg(path, destination),
                    QMessageBox::NoButton, this);
    QPushButton* remove = box.addButton(tr("Delete Group"), QMessageBox::AcceptRole);
    box.addButton(QMessageBox::Cancel);
    box.setDefaultButton(remove);
    qDebug().noquote() << QStringLiteral("Delete parameter group %1 asked: %2 parameter(s)").arg(path).arg(held);
    prepareModal(&box);
    execDialog(&box);
    if (box.clickedButton() != remove) {
      qDebug().noquote() << QStringLiteral("Delete parameter group %1 cancelled").arg(path);
      return;
    }
  }
  if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("delete_parameter_group")},
                                  {QStringLiteral("group"), path}})) {
    QSet<QString>& collapsed = collapsedGroups()[m_host.designId()];
    for (auto it = collapsed.begin(); it != collapsed.end();) {
      it = inGroup(*it, path) ? collapsed.erase(it) : std::next(it);
    }
    setMessage(QString());
    m_select = !own.isEmpty() ? own.first() : parent.isEmpty() ? QString() : QStringLiteral("group ") + parent;
    qDebug().noquote() << QStringLiteral("Parameter group %1 deleted: %2 moved to %3")
                              .arg(path,
                                   own.isEmpty() ? QStringLiteral("no parameter") : own.join(QStringLiteral(", ")),
                                   parent.isEmpty() ? QStringLiteral("User Parameters") : parent);
  } else {
    const QString problem = mitcad::modelMessage(m_host.lastError());
    setMessage(QStringLiteral("%1: %2").arg(path, problem));
    qDebug().noquote() << QStringLiteral("Delete parameter group %1 refused: %2").arg(path, problem);
  }
  refresh();
}

// Parameters into a group (empty: out of their groups), one undo step.
void ParametersDialog::moveToGroup(const QStringList& names, const QString& group) {
  if (names.isEmpty()) {
    return;
  }
  const QString where = group.isEmpty() ? QStringLiteral("User Parameters") : QStringLiteral("group %1").arg(group);
  if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("set_parameter_group")},
                                  {QStringLiteral("names"), QJsonArray::fromStringList(names)},
                                  {QStringLiteral("group"), group}})) {
    setMessage(QString());
    m_select = names.first();
    qDebug().noquote() << QStringLiteral("Parameters moved to %1: %2").arg(where, names.join(QStringLiteral(", ")));
  } else {
    const QString problem = mitcad::modelMessage(m_host.lastError());
    setMessage(problem);
    qDebug().noquote() << QStringLiteral("Parameters move to %1 refused: %2").arg(where, problem);
  }
  refresh();
}

void ParametersDialog::showContextMenu(const QPoint& at) {
  QTreeWidgetItem* item = m_table->itemAt(at);
  if (item == nullptr) {
    return;
  }
  std::function<void()> chosen; // run once the menu is closed
  QMenu menu(this);
  QStringList entries; // logged as the other context menus log theirs
  const auto add =[&](QMenu* into, const QString& text, bool enabled, std::function<void()> run) {
    QAction* action = into->addAction(text);
    action->setEnabled(enabled);
    connect(action, &QAction::triggered, this, [&chosen, run = std::move(run)] { chosen = run; });
    return action;
  };
  const QString group = item->data(kName, kGroupRole).toString();
  if (!group.isEmpty()) {
    // A group's row: a subgroup, a new name, or no group.
    m_table->setCurrentItem(item);
    add(&menu, tr("New Group…"), true, [this] { newGroup(); })->setIcon(themeIcon(QStringLiteral("folder")));
    add(&menu, tr("Rename"), true, [this, group] {
      for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
        if ((*it)->data(kName, kGroupRole).toString() == group) {
          edit(*it, kName);
          return;
        }
      }
    });
    add(&menu, tr("Delete"), true, [this, group] { deleteGroup(group); })->setIcon(themeIcon(QStringLiteral("delete")));
    entries << tr("New Group…") << tr("Rename") << tr("Delete");
  } else if (item->data(kName, kKindRole).toString() == QStringLiteral("user")) {
    // A user parameter's row: the selected user parameters, this one when
    // it is not among them.
    if (!item->isSelected()) {
      m_table->setCurrentItem(item);
    }
    const QStringList names = selectedUserParameters();
    const QString name = item->data(kName, kNameRole).toString();
    if (names.size() <= 1) {
      // Add Comment… for an AI agent (mitcad#1119).
      add(&menu, tr("Add Comment…"), true, [this, name] {
        m_host.addCommentOn({{QStringLiteral("parameter"), name}}, name);
      })->setIcon(themeIcon(QStringLiteral("comment")));
      entries << tr("Add Comment…");
    }
    // Move to Group ▸: User Parameters (no group), each group, New Group….
    QSet<QString> now;
    for (QTreeWidgetItemIterator it(m_table); *it != nullptr; ++it) {
      if ((*it)->isSelected() && (*it)->data(kName, kKindRole).toString() == QStringLiteral("user")) {
        now.insert((*it)->data(kName, kInGroupRole).toString());
      }
    }
    const auto there = [&now](const QString& path) { return now.size() == 1 && now.contains(path); };
    QMenu* move = menu.addMenu(themeIcon(QStringLiteral("folder")), tr("Move to Group"));
    add(move, tr("User Parameters"), !there(QString()), [this, names] { moveToGroup(names, QString()); });
    if (!m_groupPaths.isEmpty()) {
      move->addSeparator();
    }
    for (const QString& path : std::as_const(m_groupPaths)) {
      add(move, path, !there(path), [this, names, path] { moveToGroup(names, path); });
    }
    move->addSeparator();
    add(move, tr("New Group…"), true, [this, names] {
      const QString path = askGroupName(tr("New Group"), QString(), [this, names](const QString& made) {
        if (m_host.runParameterCommand({{QStringLiteral("cmd"), QStringLiteral("set_parameter_group")},
                                        {QStringLiteral("names"), QJsonArray::fromStringList(names)},
                                        {QStringLiteral("group"), made}})) {
          return QString();
        }
        return mitcad::modelMessage(m_host.lastError());
      });
      if (!path.isEmpty()) {
        setMessage(QString());
        m_select = names.first();
        qDebug().noquote() << QStringLiteral("Parameters moved to group %1: %2")
                                  .arg(path, names.join(QStringLiteral(", ")));
      }
      refresh();
    });
    entries << tr("Move to Group");
  } else {
    return;
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(", ")));
  execMenu(&menu, m_table->viewport()->mapToGlobal(at));
  // What was chosen runs once the menu is closed: it may open a dialog.
  if (chosen) {
    chosen();
  }
}

// A group's row opened or closed: kept for the design for the session.
void ParametersDialog::groupExpansionChanged(QTreeWidgetItem* item, bool expanded) {
  const QString group = item != nullptr ? item->data(kName, kGroupRole).toString() : QString();
  if (m_building || group.isEmpty()) {
    return;
  }
  QSet<QString>& collapsed = collapsedGroups()[m_host.designId()];
  if (expanded) {
    collapsed.remove(group);
  } else {
    collapsed.insert(group);
  }
  qDebug().noquote() << QStringLiteral("Parameters group %1 %2")
                            .arg(group, expanded ? QStringLiteral("expanded") : QStringLiteral("collapsed"));
  logOrder();
  QTimer::singleShot(0, this, &ParametersDialog::logLayout);
}

} // namespace mitcad
