// SPDX-License-Identifier: MIT
#include "ParameterField.hpp"

#include <algorithm>
#include <cstdlib>
#include <exception>
#include <memory>
#include <utility>

#include <QAction>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFocusEvent>
#include <QFont>
#include <QFontMetrics>
#include <QFormLayout>
#include <QFrame>
#include <QHeaderView>
#include <QHelpEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QRectF>
#include <QRegularExpression>
#include <QStringList>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QVariant>
#include <QtLogging>

#include "Dialogs.hpp"
#include "FormulaEditor.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

constexpr const char* kSourceProperty = "mitcadParameterSource";

// What a row of the list is (kActionRole).
constexpr int kNameRole = Qt::UserRole;        // a parameter's name
constexpr int kProblemRole = Qt::UserRole + 1; // why the field cannot take it
constexpr int kActionRole = Qt::UserRole + 2;
constexpr int kHeader = -1; // a group's title or a note: not chosen
constexpr int kParameter = 0;
constexpr int kCreate = 1;
constexpr int kName = 2;
constexpr int kFormula = 3;
constexpr int kFunction = 4;
constexpr int kTemplate = 5;
constexpr int kTable = 6;  // a table's name (mitcad#667): puts `Screws.` in
constexpr int kColumn = 7; // a column after `Screws.`

constexpr int kMaxRows = 10;
constexpr int kPageRows = 8;

// The window's parameter source, kept as a child of the window.
class SourceHolder : public QObject {
public:
  SourceHolder(QObject* parent, ParameterSource source) : QObject(parent), source(std::move(source)) {}
  ParameterSource source;
};

// "fx" in the field's text colour, for the button in the field.
QIcon fxIcon(const QWidget& field) {
  const qreal ratio = std::max<qreal>(1.0, field.devicePixelRatioF());
  constexpr int kSize = 16;
  QPixmap pixmap(QSize(kSize, kSize) * ratio);
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setRenderHint(QPainter::TextAntialiasing);
  QFont font = field.font();
  font.setItalic(true);
  font.setBold(true);
  font.setPixelSize(11);
  painter.setFont(font);
  QColor color = field.palette().color(QPalette::Text);
  color.setAlphaF(0.7f);
  painter.setPen(color);
  painter.drawText(QRectF(0, 0, kSize, kSize), Qt::AlignCenter, QStringLiteral("fx"));
  painter.end();
  return QIcon(pixmap);
}

bool selectable(const QTreeWidgetItem* item) {
  return item != nullptr && item->flags().testFlag(Qt::ItemIsSelectable) &&
         item->data(0, kActionRole).toInt() != kHeader;
}

// Create User Parameter (mitcad#568): a name (the one typed, when it names
// nothing yet), an expression, a unit (the field's kind's) and a comment;
// stays open with the reason when the name is taken or invalid, the unit
// does not fit the field, or the model refuses the expression (a cycle, an
// unknown name, a unit mismatch). Nothing changes until the model takes
// it; Cancel changes nothing.
class CreateParameterDialog : public QDialog {
public:
  struct Setup {
    const ParameterSource* source = nullptr;
    std::vector<SuggestedParameter> parameters;
    QString name;
    QString label;
    ParameterKind kind = ParameterKind::Length;
    bool anyKind = false;
    bool naming = false;
    QString unit;
    // Whether the new name would be the field's whole expression.
    std::function<bool(const QString&)> whole;
  };

  CreateParameterDialog(Setup setup, QWidget* parent) : QDialog(parent), m_setup(std::move(setup)) {
    setWindowTitle(tr("Create User Parameter"));
    setObjectName(QStringLiteral("createUserParameter"));
    auto* form = new QFormLayout(this);
    QString about = tr("A new user parameter of its own. %1 (a %2) refers to it by name.")
                        .arg(m_setup.label, parameterKindName(m_setup.kind));
    if (m_setup.naming) {
      about += QLatin1Char(' ') + tr("To name %1's own parameter instead, type name = value in the field.")
                                      .arg(m_setup.label);
    }
    auto* note = new QLabel(about);
    note->setWordWrap(true);
    form->addRow(note);
    m_name = new QLineEdit(m_setup.name);
    m_name->setObjectName(QStringLiteral("createParameterName"));
    m_expression = new QLineEdit;
    m_expression->setObjectName(QStringLiteral("createParameterExpression"));
    m_expression->setPlaceholderText(tr("A value or an expression, such as 30 or d1 * 2"));
    m_unit = new QComboBox;
    m_unit->setEditable(true);
    QStringList units{QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"), QStringLiteral("in"),
                      QStringLiteral("ft"), QStringLiteral("deg"), QStringLiteral("rad")};
    if (!m_setup.unit.isEmpty() && !units.contains(m_setup.unit)) {
      units.prepend(m_setup.unit);
    }
    m_unit->addItems(units);
    m_unit->addItem(QString());
    m_unit->setToolTip(tr("Empty for a number without a unit"));
    m_unit->setCurrentIndex(m_setup.unit.isEmpty() ? m_unit->count() - 1
                                                   : static_cast<int>(units.indexOf(m_setup.unit)));
    m_comment = new QLineEdit;
    m_error = new QLabel;
    m_error->setWordWrap(true);
    setErrorStyleSheet(m_error);
    form->addRow(tr("Name"), m_name);
    form->addRow(tr("Expression"), m_expression);
    form->addRow(tr("Unit"), m_unit);
    form->addRow(tr("Comment"), m_comment);
    form->addRow(m_error);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
    m_create = buttons->addButton(tr("Create"), QDialogButtonBox::AcceptRole);
    m_create->setDefault(true);
    m_use = buttons->addButton(QString(), QDialogButtonBox::ActionRole);
    m_use->setAutoDefault(false);
    m_use->hide();
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { createParameter(); });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_use, &QPushButton::clicked, this, [this] {
      m_chosen = m_existing;
      m_created = false;
      qDebug().noquote() << QStringLiteral("Create User Parameter: using %1 for %2").arg(m_existing, m_setup.label);
      accept();
    });
    connect(m_name, &QLineEdit::textEdited, this, [this] {
      m_use->hide();
      m_error->clear();
    });
    if (m_setup.name.isEmpty()) {
      m_name->setFocus();
    } else {
      m_expression->setFocus();
    }
  }

  // The parameter to put into the field: the new one, or an existing one
  // chosen instead (created() false).
  QString chosen() const { return m_chosen; }
  bool created() const { return m_created; }
  QString unit() const { return m_unit->currentText(); }

private:
  void createParameter() {
    const QString name = m_name->text().trimmed();
    const QString unit = m_unit->currentText().trimmed();
    const QString expression = m_expression->text().trimmed();
    std::vector<SuggestedParameter> parameters = m_setup.parameters;
    try {
      if (m_setup.source->parameters) {
        parameters = suggestedParameters(m_setup.source->parameters());
      }
    } catch (const std::exception&) {
      // The ones read when the editor opened.
    }
    const SuggestedParameter* existing = nullptr;
    QString problem = newNameProblem(parameters, name, &existing);
    if (existing != nullptr) {
      // Never overwritten: the existing one can be used instead.
      m_existing = existing->name;
      m_use->setText(tr("Use %1").arg(existing->name));
      m_use->show();
    }
    if (problem.isEmpty() && expression.isEmpty()) {
      problem = tr("Give the parameter a value or an expression.");
    }
    if (problem.isEmpty() && !m_setup.anyKind && m_setup.whole && m_setup.whole(name)) {
      const QString kind = kindProblem(m_setup.kind, kindOfUnit(unit));
      if (!kind.isEmpty()) {
        problem = tr("%1 (the unit %2). Give it a unit of a %3, or use it inside an expression.")
                      .arg(kind, unit, parameterKindName(m_setup.kind));
      }
    }
    if (problem.isEmpty()) {
      const QString written = numberWithUnit(expression, unit);
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                                {QStringLiteral("name"), name},
                                {QStringLiteral("expression"), written},
                                {QStringLiteral("unit"), unit},
                                {QStringLiteral("comment"), m_comment->text().trimmed()}};
      QString error;
      bool added = false;
      try {
        added = m_setup.source->addParameter && m_setup.source->addParameter(command, &error);
      } catch (const std::exception& e) {
        error = QString::fromUtf8(e.what());
      }
      if (added) {
        m_chosen = name;
        m_created = true;
        qDebug().noquote() << QStringLiteral("Created user parameter %1 = %2%3 for %4")
                                  .arg(name, written, unit.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(unit),
                                       m_setup.label);
        accept();
        return;
      }
      problem = error.isEmpty() ? tr("The design did not take the parameter.") : error;
    }
    m_error->setText(problem);
    qDebug().noquote() << QStringLiteral("Create User Parameter %1 refused: %2").arg(name, problem);
  }

  Setup m_setup;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_expression = nullptr;
  QComboBox* m_unit = nullptr;
  QLineEdit* m_comment = nullptr;
  QLabel* m_error = nullptr;
  QPushButton* m_create = nullptr;
  QPushButton* m_use = nullptr;
  QString m_existing;
  QString m_chosen;
  bool m_created = false;
};

} // namespace

void setParameterSource(QWidget* window, ParameterSource source) {
  if (window == nullptr) {
    return;
  }
  auto* holder = new SourceHolder(window, std::move(source));
  window->setProperty(kSourceProperty, QVariant::fromValue(static_cast<QObject*>(holder)));
}

const ParameterSource* parameterSourceOf(const QWidget* widget) {
  for (const QWidget* window = widget != nullptr ? widget->window() : nullptr; window != nullptr;
       window = window->parentWidget() != nullptr ? window->parentWidget()->window() : nullptr) {
    if (const auto* holder = dynamic_cast<const SourceHolder*>(window->property(kSourceProperty).value<QObject*>())) {
      return &holder->source;
    }
  }
  return nullptr;
}

int leaveValueField(QLineEdit* edit, const QFocusEvent& event) {
  if (edit == nullptr || event.reason() == Qt::PopupFocusReason) {
    return -1;
  }
  const int caret = edit->cursorPosition();
  edit->home(false);
  return caret;
}

ParameterField* ParameterField::attach(QLineEdit* edit, ParameterFieldOptions options) {
  if (edit == nullptr) {
    return nullptr;
  }
  if (ParameterField* existing = of(edit)) {
    existing->m_options = std::move(options);
    return existing;
  }
  return new ParameterField(edit, std::move(options));
}

ParameterField* ParameterField::of(const QLineEdit* edit) {
  return edit != nullptr ? edit->findChild<ParameterField*>(QString(), Qt::FindDirectChildrenOnly) : nullptr;
}

ParameterField::ParameterField(QLineEdit* edit, ParameterFieldOptions options)
    : QObject(edit), m_edit(edit), m_options(std::move(options)) {
  setObjectName(QStringLiteral("parameterField"));
  m_fx = new QAction(edit);
  m_fx->setObjectName(QStringLiteral("parameterFx"));
  m_fx->setToolTip(m_options.naming
                       ? tr("Insert a parameter (fx), or type part of its name. name = value names this value.")
                       : tr("Insert a parameter (fx). Type part of a name to see the parameters."));
  updateIcon();
  edit->addAction(m_fx, QLineEdit::TrailingPosition);
  connect(m_fx, &QAction::triggered, this, [this] { openBrowser(); });
  connect(edit, &QLineEdit::textEdited, this, [this] { textEdited(); });
  connect(edit, &QLineEdit::cursorPositionChanged, this, [this] { caretMoved(); });
  edit->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(edit, &QWidget::customContextMenuRequested, this, &ParameterField::showMenu);
  edit->installEventFilter(this);
  edit->setMouseTracking(true);
  auto* assistance = new QTimer(this);
  assistance->setSingleShot(true);
  assistance->setInterval(150);
  connect(edit, &QLineEdit::textChanged, assistance, [assistance] { assistance->start(); });
  connect(edit, &QLineEdit::cursorPositionChanged, assistance, [assistance] { assistance->start(); });
  connect(edit, &QLineEdit::selectionChanged, assistance, [assistance] { assistance->start(); });
  connect(assistance, &QTimer::timeout, this, &ParameterField::updateAssistance);
  if (edit->hasFocus()) {
    QTimer::singleShot(0, this, [this] { logFx(); });
  }
}

ParameterField::~ParameterField() { delete m_popup.data(); }

void ParameterField::setOwnParameter(const QString& name) { m_options.own = name; }

void ParameterField::setLabel(const QString& label) { m_options.label = label; }

bool ParameterField::isOpen() const { return m_mode != Mode::Closed && m_popup && m_popup->isVisible(); }

SuggestContext ParameterField::context() const {
  SuggestContext result;
  result.kind = m_options.kind;
  result.anyKind = m_options.anyKind;
  result.own = m_options.own;
  result.sketch = m_options.sketch;
  return result;
}

ParameterField::Anchor ParameterField::anchor() const {
  if (m_mode == Mode::Browsing) {
    return m_anchor;
  }
  Anchor now;
  now.text = m_edit->text();
  now.caret = m_edit->cursorPosition();
  now.selection = m_edit->hasSelectedText() ? m_edit->selectionStart() : -1;
  now.selectionLength = m_edit->hasSelectedText() ? m_edit->selectionLength() : 0;
  return now;
}

QString ParameterField::typedName(const Anchor& where) const {
  const NameFragment fragment = nameFragmentAt(where.text, where.caret);
  QString name;
  if (fragment.active) {
    name = where.text.mid(fragment.start, fragment.length);
  } else if (m_mode == Mode::Browsing) {
    name = m_filter.trimmed();
  }
  if (!isValidParameterName(name) || tableNamed(m_tables, name) != nullptr) {
    return QString();
  }
  for (const SuggestedParameter& p : m_parameters) {
    if (p.name == name) {
      return QString();
    }
  }
  return name;
}

bool ParameterField::leadingName(const Anchor& where) const {
  const NameFragment fragment = nameFragmentAt(where.text, where.caret);
  return fragment.active && where.text.left(fragment.start).trimmed().isEmpty() &&
         expressionPart(where.text) == where.text.trimmed();
}

QWidget* ParameterField::topWindow() const {
  QWidget* window = m_edit->window();
  while (window->parentWidget() != nullptr) {
    window = window->parentWidget()->window();
  }
  return window;
}

void ParameterField::fetch() {
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (source == nullptr || !source->parameters) {
    m_parameters.clear();
    return;
  }
  try {
    m_parameters = suggestedParameters(source->parameters());
    if (source->query) {
      const QJsonObject help = source->query({{QStringLiteral("query"), QStringLiteral("formula_help")}});
      m_functions = help.value(QStringLiteral("functions")).toArray();
      m_tables = suggestedTables(help.value(QStringLiteral("tables")).toArray());
    }
  } catch (const std::exception& e) {
    // A job computes the model: the parameters read last.
    qDebug().noquote() << QStringLiteral("Parameter suggestions for %1: parameters not read now: %2")
                              .arg(m_options.label, QString::fromUtf8(e.what()));
  }
}

void ParameterField::textEdited() {
  if (m_inserting || !m_popupEnabled) {
    return;
  }
  // A column after `Table.` (mitcad#667).
  const ColumnFragment column = columnFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (column.active && !m_edit->hasSelectedText()) {
    if (m_mode != Mode::Typing) {
      fetch();
    }
    if (tableNamed(m_tables, column.table) != nullptr) {
      m_columnsOf = column.table;
      m_mode = Mode::Typing;
      m_filter = column.typed;
      open(Mode::Typing);
      return;
    }
  }
  m_columnsOf.clear();
  const NameFragment fragment = nameFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (!fragment.active || m_edit->hasSelectedText()) {
    close();
    return;
  }
  if (m_mode != Mode::Typing) {
    fetch(); // also a paste while browsing: the list follows the text now
  }
  m_mode = Mode::Typing;
  m_filter = fragment.typed;
  open(Mode::Typing);
}

void ParameterField::caretMoved() {
  if (m_inserting || m_mode == Mode::Closed) {
    return;
  }
  if (m_mode == Mode::Browsing) {
    // A click into the field moved the caret: where the browser opened is
    // no longer where a choice would go.
    if (m_edit->cursorPosition() != m_anchor.caret) {
      close();
    }
    return;
  }
  if (!m_columnsOf.isEmpty()) {
    const ColumnFragment column = columnFragmentAt(m_edit->text(), m_edit->cursorPosition());
    if (!column.active || column.table != m_columnsOf) {
      close();
    } else if (column.typed != m_filter) {
      m_filter = column.typed;
      open(Mode::Typing);
    }
    return;
  }
  const NameFragment fragment = nameFragmentAt(m_edit->text(), m_edit->cursorPosition());
  if (!fragment.active) {
    close();
  } else if (fragment.typed != m_filter) {
    m_filter = fragment.typed;
    open(Mode::Typing);
  }
}

void ParameterField::open(Mode mode) {
  if (!m_popupEnabled) return;
  m_mode = mode;
  if (!m_popup) {
    QWidget* window = m_edit->window();
    auto* popup = new QFrame(window);
    popup->setObjectName(QStringLiteral("parameterSuggestions"));
    popup->setFrameShape(QFrame::StyledPanel);
    popup->setAutoFillBackground(true);
    popup->setBackgroundRole(QPalette::Base);
    // No focus: the field keeps the keyboard (a table cell's editor stays
    // open, a panel's field is not committed).
    popup->setFocusPolicy(Qt::NoFocus);
    auto* box = new QVBoxLayout(popup);
    box->setContentsMargins(4, 4, 4, 4);
    box->setSpacing(2);
    m_search = new QLabel(popup);
    m_search->setObjectName(QStringLiteral("parameterSearch"));
    m_list = new QTreeWidget(popup);
    m_list->setObjectName(QStringLiteral("parameterList"));
    m_list->setFocusPolicy(Qt::NoFocus);
    m_list->setColumnCount(3);
    m_list->setHeaderHidden(true);
    m_list->setRootIsDecorated(false);
    m_list->setIndentation(0);
    m_list->setUniformRowHeights(true);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    m_list->setAllColumnsShowFocus(true);
    m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_list->setFrameShape(QFrame::NoFrame);
    m_list->header()->setStretchLastSection(true);
    m_footer = new QLabel(popup);
    m_footer->setObjectName(QStringLiteral("parameterHint"));
    m_footer->setWordWrap(true);
    QFont small = m_footer->font();
    if (small.pointSizeF() > 0.0) {
      small.setPointSizeF(small.pointSizeF() * 0.9);
    }
    m_footer->setFont(small);
    box->addWidget(m_search);
    box->addWidget(m_list);
    box->addWidget(m_footer);
    connect(m_list, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int) { activate(item); });
    connect(m_list, &QTreeWidget::currentItemChanged, this,
            [this](QTreeWidgetItem* item, QTreeWidgetItem*) { showProblem(item); });
    m_popup = popup;
  }
  if (!fill()) {
    close();
    return;
  }
  place();
  m_popup->show();
  m_popup->raise();
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

bool ParameterField::fillColumns() {
  const SuggestedTable* table = tableNamed(m_tables, m_columnsOf);
  if (table == nullptr) {
    return false;
  }
  QFont bold = m_list->font();
  bold.setBold(true);
  auto* header = new QTreeWidgetItem(m_list, {tr("Columns of %1").arg(table->name)});
  header->setFlags(Qt::ItemIsEnabled);
  header->setFont(0, bold);
  header->setData(0, kActionRole, kHeader);
  header->setFirstColumnSpanned(true);
  const std::vector<SuggestedTable::Column> columns = matchColumns(*table, m_filter);
  QStringList logged;
  QTreeWidgetItem* current = nullptr;
  for (const SuggestedTable::Column& column : columns) {
    // The unit box's words: a unit, "no unit" or "Text".
    const QString unit = column.unit.isEmpty() ? tr("no unit") : column.unit;
    auto* item = new QTreeWidgetItem(m_list, {column.name, unit, table->comment});
    item->setData(0, kNameRole, column.name);
    item->setData(0, kActionRole, kColumn);
    const QString tip = tr("%1.%2: a column (%3); look a value up with XLOOKUP or INDEX")
                            .arg(table->name, column.name, unit);
    for (int i = 0; i < 3; ++i) {
      item->setToolTip(i, tip);
    }
    if (current == nullptr) {
      current = item;
    }
    logged << column.name;
  }
  if (columns.empty()) {
    auto* item = new QTreeWidgetItem(m_list, {tr("No column of %1 matches “%2”").arg(table->name, m_filter)});
    item->setFlags(Qt::ItemIsEnabled);
    item->setData(0, kActionRole, kHeader);
    item->setFirstColumnSpanned(true);
  }
  m_list->setCurrentItem(current);
  showProblem(current);
  m_search->setVisible(false);
  const QString line = QStringLiteral("Parameter suggestions for %1 \"%2.%3\": %4")
                           .arg(m_options.label, table->name, m_filter,
                                logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", ")));
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
  // Exactly the one column typed already: nothing to choose.
  return !(columns.size() == 1 && columns.front().name == m_filter);
}

bool ParameterField::fill() {
  m_list->clear();
  if (!m_columnsOf.isEmpty() && m_mode == Mode::Typing) {
    return fillColumns();
  }
  const Anchor at = anchor();
  const auto inserted = [&at](const QString& name) {
    return insertParameter(at.text, at.caret, at.selection, at.selectionLength, name).text;
  };
  const bool browsing = m_mode == Mode::Browsing;
  const std::vector<ParameterChoice> choices = matchParameters(m_parameters, m_filter, browsing, context(), inserted);
  QFont bold = m_list->font();
  bold.setBold(true);
  QFont italic = m_list->font();
  italic.setItalic(true);
  const QColor muted = m_list->palette().color(QPalette::Disabled, QPalette::Text);
  const auto note = [this, &italic](const QString& text) {
    auto* item = new QTreeWidgetItem(m_list, {text});
    item->setFlags(Qt::ItemIsEnabled);
    item->setFont(0, italic);
    item->setData(0, kActionRole, kHeader);
    item->setFirstColumnSpanned(true);
  };
  QTreeWidgetItem* current = nullptr;
  QTreeWidgetItem* firstAny = nullptr;
  int group = -1;
  QStringList logged;
  for (const ParameterChoice& choice : choices) {
    const SuggestedParameter& p = *choice.parameter;
    if (static_cast<int>(p.group) != group) {
      group = static_cast<int>(p.group);
      auto* header = new QTreeWidgetItem(
          m_list, {p.group == SuggestedParameter::Group::User ? tr("User Parameters") : tr("Model Parameters")});
      header->setFlags(Qt::ItemIsEnabled);
      header->setFont(0, bold);
      header->setData(0, kActionRole, kHeader);
      header->setFirstColumnSpanned(true);
    }
    QString value = p.text.isEmpty() ? p.expression : p.text;
    if (p.kind == ParameterKind::Unitless) {
      value = tr("%1 (number)").arg(value);
    }
    QString comment = p.comment;
    if (p.reference) {
      comment = comment.isEmpty() ? tr("driven") : tr("driven: %1").arg(comment);
    }
    auto* item = new QTreeWidgetItem(m_list, {p.name, value, comment});
    item->setData(0, kNameRole, p.name);
    item->setData(0, kProblemRole, choice.problem);
    item->setData(0, kActionRole, kParameter);
    QString tip = QStringLiteral("%1 = %2").arg(p.name, p.expression);
    if (!p.text.isEmpty() && p.text != p.expression) {
      tip += QStringLiteral(" → %1").arg(p.text);
    }
    tip += QStringLiteral(" (%1)").arg(p.comment.isEmpty() ? parameterKindName(p.kind) : p.comment);
    if (!choice.problem.isEmpty()) {
      tip += QLatin1Char('\n') + choice.problem;
      for (int column = 0; column < 3; ++column) {
        item->setForeground(column, muted);
      }
    }
    for (int column = 0; column < 3; ++column) {
      item->setToolTip(column, tip);
    }
    if (current == nullptr && choice.problem.isEmpty()) {
      current = item;
    }
    if (firstAny == nullptr) {
      firstAny = item;
    }
    logged << (choice.problem.isEmpty() ? p.name : QStringLiteral("%1 (%2)").arg(p.name, choice.problem));
  }
  // The design's tables (mitcad#667): a choice puts `Screws.` in, and the
  // list goes on with its columns.
  const std::vector<const SuggestedTable*> tables = matchTables(m_tables, m_filter, browsing);
  QStringList loggedTables;
  if (!tables.empty()) {
    auto* header = new QTreeWidgetItem(m_list, {tr("Tables")});
    header->setFlags(Qt::ItemIsEnabled);
    header->setFont(0, bold);
    header->setData(0, kActionRole, kHeader);
    header->setFirstColumnSpanned(true);
  }
  for (const SuggestedTable* table : tables) {
    QStringList columns;
    for (const SuggestedTable::Column& column : table->columns) {
      columns << column.name;
    }
    auto* item = new QTreeWidgetItem(m_list, {table->name, columns.join(QStringLiteral(", ")), table->comment});
    item->setData(0, kNameRole, table->name + QLatin1Char('.'));
    item->setData(0, kActionRole, kTable);
    const QString tip = tr("Table %1: %2. Choose a column after the dot.").arg(table->name, columns.join(QStringLiteral(", ")));
    for (int column = 0; column < 3; ++column) {
      item->setToolTip(column, tip);
    }
    if (current == nullptr) {
      current = item;
    }
    loggedTables << table->name;
  }
  if (choices.empty() && tables.empty()) {
    if (m_parameters.empty()) {
      note(tr("The design has no parameters yet"));
    } else if (m_filter.trimmed().isEmpty()) {
      note(tr("No parameter fits this field"));
    } else {
      note(tr("No parameter matches “%1”").arg(m_filter.trimmed()));
    }
  }
  // What mitcad#568 adds: naming the field's own parameter, and a new user
  // parameter of its own.
  QStringList actions;
  QTreeWidgetItem* firstAction = nullptr;
  const QString name = typedName(at);
  const auto action = [this, &italic, &firstAction, &actions](int kind, const QString& text, const QString& about,
                                                              const QString& logName) {
    auto* item = new QTreeWidgetItem(m_list, {text, QString(), about});
    item->setData(0, kActionRole, kind);
    item->setFont(0, italic);
    item->setToolTip(0, about);
    item->setToolTip(2, about);
    if (firstAction == nullptr) {
      firstAction = item;
    }
    actions << logName;
  };
  if (m_options.naming && (browsing || (!name.isEmpty() && leadingName(at)))) {
    action(kName,
           name.isEmpty() || !leadingName(at) ? tr("Name this value: name = …")
                                              : tr("Name this value: %1 = …").arg(name),
           tr("names this field's own parameter"), QStringLiteral("name"));
  }
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (m_options.create && source != nullptr && source->addParameter && (browsing || !name.isEmpty())) {
    action(kCreate,
           name.isEmpty() ? tr("Create User Parameter…") : tr("Create User Parameter “%1”…").arg(name),
           tr("a new parameter of its own"), QStringLiteral("create"));
  }
  if (browsing && source != nullptr && source->query) {
    action(kFormula,tr("Edit Formula…"),tr("A larger formula editor with functions, templates and parameter limits"),
           QStringLiteral("formula"));
  }
  bool hasFunctions = false;
  for (const QJsonValue& value : m_functions) {
    const QJsonObject function=value.toObject();
    const QString functionName=function.value(QStringLiteral("name")).toString();
    const QString description=function.value(QStringLiteral("description")).toString();
    const QString example=function.value(QStringLiteral("example")).toString();
    const bool matches=browsing
        ? (functionName+QLatin1Char(' ')+description+QLatin1Char(' ')+example).contains(m_filter,Qt::CaseInsensitive)
        : functionName.startsWith(m_filter,Qt::CaseInsensitive);
    if (!matches) continue;
    auto* item=new QTreeWidgetItem(m_list,{functionName,description,example});
    item->setData(0,kNameRole,functionName+QStringLiteral("()"));
    item->setData(0,kActionRole,kFunction);
    item->setToolTip(0,description+QLatin1Char('\n')+example);
    if (!current && (!browsing || !firstAction)) current=item;
    hasFunctions=true;
  }
  if (browsing && source != nullptr && source->query) {
    const QVector<QPair<QString,QString>> templates{
        {tr("How many fit"),QStringLiteral("FLOOR(length / pitch)")},
        {tr("Spacing for N items"),QStringLiteral("length / (count - 1)")},
        {tr("Even count"),QStringLiteral("EVEN(count)")},{tr("Odd count"),QStringLiteral("ODD(count)")},
        {tr("Round to a step"),QStringLiteral("MROUND(value; step)")}};
    for (const auto& entry:templates) {
      if (!(entry.first+entry.second).contains(m_filter,Qt::CaseInsensitive)) continue;
      auto* item=new QTreeWidgetItem(m_list,{entry.first,tr("Formula template"),entry.second});
      item->setData(0,kNameRole,entry.second);
      item->setData(0,kActionRole,kTemplate);
      if (!current) current=item;
    }
  }
  if (current == nullptr) {
    current = firstAction != nullptr ? firstAction : firstAny;
  }
  m_list->setCurrentItem(current);
  showProblem(current);
  m_search->setVisible(browsing);
  m_search->setText(m_filter.isEmpty() ? tr("Type to search names and comments")
                                       : tr("Search: %1").arg(m_filter));
  const QString line =
      QStringLiteral("Parameter suggestions for %1 \"%2\": %3%4%5")
          .arg(m_options.label, m_filter, logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", ")),
               loggedTables.isEmpty() ? QString()
                                      : QStringLiteral("; tables: %1").arg(loggedTables.join(QStringLiteral(", "))),
               actions.isEmpty() ? QString() : QStringLiteral("; actions: %1").arg(actions.join(QStringLiteral(", "))));
  if (line != m_logged) {
    m_logged = line;
    qDebug().noquote() << line;
  }
  if (!browsing) {
    const NameFragment fragment = nameFragmentAt(at.text, at.caret);
    const QString word = at.text.mid(fragment.start, fragment.length);
    // Nothing to offer for a function's or a unit's name; nor for exactly
    // the one name typed already (Enter then confirms as without a list).
    if (choices.empty() && tables.empty() && actions.isEmpty() && !hasFunctions && isReservedName(word)) {
      return false;
    }
    if (choices.size() == 1 && choices.front().parameter->name == word && actions.isEmpty() && tables.empty()) {
      return false;
    }
  }
  return true;
}

void ParameterField::showProblem(QTreeWidgetItem* item) {
  if (!m_footer) {
    return;
  }
  const QString problem = item != nullptr ? item->data(0, kProblemRole).toString() : QString();
  if (!problem.isEmpty()) {
    setErrorStyleSheet(m_footer);
    m_footer->setText(problem);
    return;
  }
  m_footer->setStyleSheet(QString());
  m_footer->setForegroundRole(QPalette::PlaceholderText);
  QString hint = m_mode == Mode::Browsing ? tr("Type to search; Enter puts the name in; Esc closes the list.")
                                          : tr("Enter puts the name in; Esc closes the list.");
  if (m_options.naming) {
    hint += QLatin1Char(' ') + tr("name = value names this field's own parameter.");
  }
  m_footer->setText(hint);
}

void ParameterField::place() {
  if (!m_popup) {
    return;
  }
  QWidget* window = m_popup->parentWidget();
  const int rows = std::clamp(m_list->topLevelItemCount(), 1, kMaxRows);
  const int rowHeight = m_list->fontMetrics().height() + 6;
  m_list->setFixedHeight(rows * rowHeight + 2 * m_list->frameWidth() + 2);
  const int width = std::min(std::max(m_edit->width(), 420), std::max(200, window->width() - 8));
  m_popup->setFixedWidth(width);
  m_list->setColumnWidth(0, width * 34 / 100);
  m_list->setColumnWidth(1, width * 26 / 100);
  m_popup->adjustSize();
  const int height = m_popup->sizeHint().height();
  QPoint at = m_edit->mapTo(window, QPoint(0, m_edit->height() + 2));
  if (at.y() + height > window->height()) {
    // Above the field when there is no room below it.
    const int above = m_edit->mapTo(window, QPoint(0, 0)).y() - height - 2;
    at.setY(above >= 0 ? above : std::max(0, window->height() - height));
  }
  at.setX(std::clamp(at.x(), 0, std::max(0, window->width() - width)));
  m_popup->move(at);
}

void ParameterField::logPlaces() {
  if (!isOpen()) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = topWindow()->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* item = m_list->topLevelItem(i);
    if (!selectable(item)) {
      continue;
    }
    const QRect rect = m_list->visualItemRect(item);
    if (rect.isEmpty() || !m_list->viewport()->rect().contains(rect.center())) {
      continue; // scrolled away
    }
    const QPoint at = m_list->viewport()->mapToGlobal(rect.center()) - origin;
    const int kind = item->data(0, kActionRole).toInt();
    const QString what = kind == kCreate ? QStringLiteral("action create")
                         : kind == kName ? QStringLiteral("action name")
                                         : item->data(0, kNameRole).toString();
    lines << QStringLiteral("Parameter suggestion %1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
}

void ParameterField::logFx() {
  if (!m_edit->isVisible()) {
    return;
  }
  for (QToolButton* button : m_edit->findChildren<QToolButton*>()) {
    if (button->defaultAction() != m_fx || !button->isVisible()) {
      continue;
    }
    const QPoint at = button->mapToGlobal(button->rect().center()) - topWindow()->mapToGlobal(QPoint(0, 0));
    const QString line = QStringLiteral("Parameter fx %1 at %2,%3").arg(m_options.label).arg(at.x()).arg(at.y());
    if (line != m_loggedFx) {
      m_loggedFx = line;
      qDebug().noquote() << line;
    }
    return;
  }
}

void ParameterField::move(int step) {
  const int count = m_list->topLevelItemCount();
  if (count == 0) {
    return;
  }
  const int direction = step < 0 ? -1 : 1;
  int remaining = std::abs(step);
  int at = m_list->indexOfTopLevelItem(m_list->currentItem());
  int last = at;
  int i = at < 0 && direction < 0 ? count : at;
  while (remaining > 0) {
    i += direction;
    if (i < 0 || i >= count) {
      break;
    }
    if (selectable(m_list->topLevelItem(i))) {
      last = i;
      --remaining;
    }
  }
  if (last >= 0 && last != at) {
    QTreeWidgetItem* item = m_list->topLevelItem(last);
    m_list->setCurrentItem(item);
    m_list->scrollToItem(item);
    qDebug().noquote() << QStringLiteral("Parameter suggestion highlighted: %1")
                              .arg(item->data(0, kActionRole).toInt() == kParameter
                                       ? item->data(0, kNameRole).toString()
                                       : item->text(0));
    QTimer::singleShot(0, this, [this] { logPlaces(); });
  }
}

void ParameterField::activate(QTreeWidgetItem* item) {
  if (!selectable(item)) {
    return;
  }
  switch (item->data(0, kActionRole).toInt()) {
  case kCreate:
    // Out of the key's or the click's handling first: a dialog opens.
    QTimer::singleShot(0, this, [this] { openCreateEditor(); });
    return;
  case kName:
    startNaming();
    return;
  case kFormula:
    QTimer::singleShot(0,this,[this] { openFormulaEditor(); });
    return;
  case kTable: {
    // `Screws.` in place of the name typed; the list goes on with its
    // columns (mitcad#667).
    // A `.` after the name typed (`Scr|.dk`) is not put in twice: the
    // caret goes past it.
    const Anchor where = anchor();
    const NameFragment fragment = nameFragmentAt(where.text, where.caret);
    const int end = fragment.start + fragment.length;
    const bool dot = fragment.active && end < where.text.size() && where.text.at(end) == QLatin1Char('.');
    QString name = item->data(0, kNameRole).toString();
    if (dot) {
      name.chop(1);
    }
    insert(name, QStringLiteral("table"), where);
    if (dot) {
      m_edit->setCursorPosition(m_edit->cursorPosition() + 1);
    }
    QTimer::singleShot(0, this, [this] { textEdited(); });
    return;
  }
  case kColumn: {
    // In place of the column's name typed, also what follows the caret.
    Anchor where = anchor();
    const ColumnFragment column = columnFragmentAt(where.text, where.caret);
    if (column.active && column.length > 0) {
      where.selection = column.start;
      where.selectionLength = column.length;
    }
    insert(item->data(0, kNameRole).toString(), QStringLiteral("column"), where);
    return;
  }
  default:
    break;
  }
  const QString name = item->data(0, kNameRole).toString();
  const QString problem = item->data(0, kProblemRole).toString();
  if (!problem.isEmpty()) {
    // Said instead of putting in what the field cannot take.
    m_list->setCurrentItem(item);
    showProblem(item);
    qDebug().noquote() << QStringLiteral("Parameter suggestion %1 refused: %2").arg(name, problem);
    return;
  }
  const bool function = item->data(0,kActionRole).toInt()==kFunction;
  insert(name, function ? QStringLiteral("function") : m_mode == Mode::Browsing ? QStringLiteral("browser") : QStringLiteral("suggestion"), anchor());
  if (function) m_edit->setCursorPosition(m_edit->cursorPosition()-1);
}

void ParameterField::insert(const QString& name, const QString& how, const Anchor& where) {
  const Insertion insertion = insertParameter(where.text, where.caret, where.selection, where.selectionLength, name);
  m_inserting = true;
  if (m_edit->text() == where.text) {
    if (insertion.length > 0) {
      m_edit->setSelection(insertion.from, insertion.length);
    } else {
      m_edit->deselect();
      m_edit->setCursorPosition(insertion.from);
    }
  }
  // Otherwise the text changed meanwhile: at the caret. insert() keeps the
  // field's own undo.
  m_edit->insert(name);
  m_inserting = false;
  close();
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
  const QString now = m_edit->text();
  qDebug().noquote() << QStringLiteral("Parameter %1 inserted %2 into %3: %4").arg(how, name, m_options.label, now);
  // insert() told the field's own reactions (the panel's value and
  // preview, a sketch tool's value) through textEdited, as typing does.
}

void ParameterField::openBrowser() {
  if (!m_popupEnabled) return;
  if (m_mode == Mode::Browsing) {
    close(QStringLiteral("fx"));
    return;
  }
  fetch();
  m_anchor = anchor(); // as the field is now: not browsing yet
  const NameFragment fragment = nameFragmentAt(m_anchor.text, m_anchor.caret);
  m_filter = fragment.active && m_anchor.selection < 0 ? fragment.typed : QString();
  qDebug().noquote() << QStringLiteral("Parameter browser opened for %1").arg(m_options.label);
  open(Mode::Browsing);
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
}

void ParameterField::openCreateEditor() {
  const ParameterSource* source = parameterSourceOf(m_edit);
  if (source == nullptr || !source->addParameter) {
    return;
  }
  if (m_mode == Mode::Closed) {
    fetch();
  }
  const Anchor at = anchor();
  CreateParameterDialog::Setup setup;
  setup.source = source;
  setup.name = typedName(at);
  setup.label = m_options.label;
  setup.kind = m_options.kind;
  setup.anyKind = m_options.anyKind;
  setup.naming = m_options.naming;
  setup.whole = [at](const QString& name) {
    return expressionPart(insertParameter(at.text, at.caret, at.selection, at.selectionLength, name).text) == name;
  };
  switch (m_options.kind) {
  case ParameterKind::Angle:
    setup.unit = QStringLiteral("deg");
    break;
  case ParameterKind::Unitless:
    break;
  case ParameterKind::Text:
    setup.unit = QStringLiteral("Text");
    break;
  case ParameterKind::Length:
    setup.unit = QStringLiteral("mm");
    try {
      if (source->lengthUnit) {
        const QString unit = source->lengthUnit();
        if (!unit.isEmpty()) {
          setup.unit = unit;
        }
      }
    } catch (const std::exception&) {
      // Millimetres, the default.
    }
    break;
  }
  setup.parameters = m_parameters;
  close();
  const QPointer<QLineEdit> edit = m_edit;
  const QPointer<ParameterField> alive(this);
  CreateParameterDialog dialog(std::move(setup), m_edit->window());
  qDebug().noquote() << QStringLiteral("Create User Parameter opened for %1: name %2, unit %3")
                            .arg(m_options.label, typedName(at), dialog.unit());
  prepareModal(&dialog);
  const int result = execDialog(&dialog);
  if (!alive || !edit) {
    return; // the field went while the editor was open (its tool ended)
  }
  // Back to the field, its text as it was.
  edit->window()->activateWindow();
  edit->setFocus(Qt::OtherFocusReason);
  if (result != QDialog::Accepted || dialog.chosen().isEmpty()) {
    qDebug().noquote() << QStringLiteral("Create User Parameter cancelled for %1: %2").arg(m_options.label, edit->text());
    return;
  }
  fetch(); // the new one is offered from now on
  insert(dialog.chosen(), dialog.created() ? QStringLiteral("created") : QStringLiteral("existing"), at);
}

void ParameterField::setPopupEnabled(bool enabled) {
  m_popupEnabled = enabled;
  if (!enabled) close();
}

void ParameterField::refreshParameters() { fetch(); }

void ParameterField::startNaming() {
  const Anchor at = anchor();
  const QString typed = typedName(at);
  const bool leading = leadingName(at);
  close();
  static const QRegularExpression named(QStringLiteral(R"(^\s*(\w+)\s*=(?!=))"),
                                        QRegularExpression::UseUnicodePropertiesOption);
  m_inserting = true;
  QString shown;
  if (const QRegularExpressionMatch match = named.match(at.text); match.hasMatch()) {
    // Named already: its name to type over.
    m_edit->setSelection(static_cast<int>(match.capturedStart(1)), static_cast<int>(match.capturedLength(1)));
    shown = at.text;
  } else {
    const NameFragment fragment = nameFragmentAt(at.text, at.caret);
    QString text;
    if (!typed.isEmpty() && leading) {
      // The name typed first: `width` becomes `width = `, the value next.
      text = QStringLiteral("%1 = %2").arg(typed, at.text.mid(fragment.start + fragment.length).trimmed());
      m_edit->selectAll();
      m_edit->insert(text);
      m_edit->setCursorPosition(static_cast<int>(text.size()));
    } else {
      // The value as it is, after its parameter's name, which is selected
      // to type the new one over it.
      const QString name = m_options.own.isEmpty() ? QStringLiteral("name") : m_options.own;
      text = QStringLiteral("%1 = %2").arg(name, at.text.trimmed());
      m_edit->selectAll();
      m_edit->insert(text);
      m_edit->setSelection(0, static_cast<int>(name.size()));
    }
    shown = text;
  }
  m_inserting = false;
  if (!m_edit->hasFocus()) {
    m_edit->setFocus(Qt::OtherFocusReason);
  }
  qDebug().noquote() << QStringLiteral("Parameter naming for %1: %2").arg(m_options.label, shown);
}

void ParameterField::close(const QString& why) {
  if (m_mode == Mode::Closed && !(m_popup && m_popup->isVisible())) {
    return;
  }
  const bool shown = m_popup && m_popup->isVisible();
  m_mode = Mode::Closed;
  m_filter.clear();
  m_columnsOf.clear();
  if (m_popup) {
    m_popup->hide();
  }
  m_logged.clear();
  m_loggedPlaces.clear();
  if (shown && !why.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Parameter suggestions closed for %1 (%2)").arg(m_options.label, why);
  }
}

void ParameterField::showMenu(const QPoint& at) {
  const std::unique_ptr<QMenu> menu(m_edit->createStandardContextMenu());
  menu->addSeparator();
  QAction* browse = menu->addAction(tr("Insert Parameter…"));
  QAction* formula = menu->addAction(tr("Edit Formula…"));
  formula->setEnabled(parameterSourceOf(m_edit) && parameterSourceOf(m_edit)->query);
  QAction* naming = m_options.naming ? menu->addAction(tr("Name This Value (name = value)")) : nullptr;
  const ParameterSource* source = parameterSourceOf(m_edit);
  QAction* create = m_options.create && source != nullptr && source->addParameter
                        ? menu->addAction(tr("Create User Parameter…"))
                        : nullptr;
  const QPointer<ParameterField> alive(this);
  QAction* chosen = execMenu(menu.get(), m_edit->mapToGlobal(at));
  if (!alive || chosen == nullptr) {
    return;
  }
  if (chosen == browse) {
    QTimer::singleShot(0, this, [this] { openBrowser(); });
  } else if (chosen == formula) {
    QTimer::singleShot(0,this,[this] { openFormulaEditor(); });
  } else if (chosen == naming) {
    QTimer::singleShot(0, this, [this] { startNaming(); });
  } else if (chosen == create) {
    QTimer::singleShot(0, this, [this] { openCreateEditor(); });
  }
}

void ParameterField::updateIcon() {
  if (m_fx != nullptr) {
    m_fx->setIcon(fxIcon(*m_edit));
  }
}

void ParameterField::openFormulaEditor() {
  const ParameterSource* source=parameterSourceOf(m_edit);
  if (!source || !source->query) return;
  close();
  const QPointer<QLineEdit> edit(m_edit);
  const QPointer<ParameterField> alive(this);
  const QString original=edit->text();
  const QString value=expressionPart(original);
  // The `name =` before the expression, where the match found it: the
  // expression's text may also occur in the name ("t2 = 2", mitcad#755).
  const QString prefix=original.left(expressionStart(original));
  FormulaEditor dialog(*source,m_options,value,edit->window());
  while (execDialog(&dialog)==QDialog::Accepted) {
    if (!alive || !edit) return;
    if (dialog.limitsChanged() && source->addParameter) {
      QString error;
      if (!source->addParameter(dialog.parameterCommand(),&error)) {
        sheetWarning(&dialog,tr("Parameter Limits"),error);
        continue;
      }
    }
    if (!alive || !edit) return;
    const QString chosen=prefix+dialog.expression();
    edit->selectAll();
    edit->insert(chosen);
    edit->setFocus(Qt::OtherFocusReason);
    qDebug().noquote() << QStringLiteral("Formula editor applied: %1").arg(chosen);
    return;
  }
  if (edit) edit->setFocus(Qt::OtherFocusReason);
}

void ParameterField::updateAssistance() {
  const ParameterSource* source=parameterSourceOf(m_edit);
  if (!source || !source->query) return;
  try {
    const QString text=expressionPart(m_edit->text());
    const int offset=expressionStart(m_edit->text());
    const int start=m_edit->hasSelectedText() ? m_edit->selectionStart() : m_edit->cursorPosition();
    QJsonObject query{{QStringLiteral("query"),QStringLiteral("formula_help")},
                      {QStringLiteral("expression"),text}, {QStringLiteral("kind"),formulaKindName(m_options.kind)},
                      {QStringLiteral("cursor"),int(text.left(std::max(0,start-offset)).toUtf8().size())}};
    if (m_edit->hasSelectedText()) query.insert(QStringLiteral("selection_end"),
        int(text.left(std::max(0,start-offset)+int(m_edit->selectedText().size())).toUtf8().size()));
    const QJsonObject result=source->query(query);
    const QString signature=formulaSignatureHtml(result.value(QStringLiteral("signature")).toObject());
    const QJsonObject part=result.value(QStringLiteral("selection")).toObject();
    const QString shown=part.value(QStringLiteral("error")).toString(part.value(QStringLiteral("text")).toString());
    m_edit->setProperty("formulaSignature",signature);
    m_edit->setProperty("formulaPartValue",shown);
    const QJsonObject error=result.value(QStringLiteral("error")).toObject();
    m_edit->setProperty("formulaErrorStart",error.value(QStringLiteral("start")).toInt());
    m_edit->setProperty("formulaErrorEnd",error.value(QStringLiteral("end")).toInt());
    // A selected part's value, not the whole text's: a field selects all of
    // it on focus, and the tip below would only repeat it over the next
    // control (ui-wrap's Flip, mitcad#700).
    const bool partSelected=m_edit->hasSelectedText() && m_edit->selectedText().trimmed()!=m_edit->text().trimmed();
    if (m_edit->hasFocus() && !isOpen() && (!signature.isEmpty() || partSelected)) {
      const QString tip=signature+(partSelected && !shown.isEmpty() ? QStringLiteral("<br>%1").arg(shown.toHtmlEscaped()) : QString());
      if (!tip.isEmpty()) QToolTip::showText(m_edit->mapToGlobal(QPoint(0,m_edit->height())),tip,m_edit);
    }
    if (!signature.isEmpty()) qDebug().noquote() << QStringLiteral("Formula field signature: %1").arg(signature);
  } catch (const std::exception&) {
    m_edit->setProperty("formulaSignature",QString());
    m_edit->setProperty("formulaPartValue",QString());
    if (m_edit->hasFocus()) QToolTip::hideText();
  }
}

bool ParameterField::eventFilter(QObject* watched, QEvent* event) {
  if (watched != m_edit) {
    return QObject::eventFilter(watched, event);
  }
  switch (event->type()) {
  case QEvent::ToolTip: {
    const auto* help=static_cast<QHelpEvent*>(event);
    const ParameterSource* source=parameterSourceOf(m_edit);
    if (!source || !source->query) break;
    try {
      const int cursor=m_edit->cursorPositionAt(help->pos());
      const QJsonObject result=source->query({{QStringLiteral("query"),QStringLiteral("formula_help")},
          {QStringLiteral("expression"),m_edit->text()}, {QStringLiteral("kind"),formulaKindName(m_options.kind)},
          {QStringLiteral("cursor"),int(m_edit->text().left(cursor).toUtf8().size())}});
      const QJsonObject part=result.value(QStringLiteral("selection")).toObject();
      const QString shown=part.value(QStringLiteral("error")).toString(part.value(QStringLiteral("text")).toString());
      if (!shown.isEmpty()) {
        QToolTip::showText(help->globalPos(),shown,m_edit);
        return true;
      }
    } catch (const std::exception&) {}
    // Nothing to show at that place: the field's own tooltip (the
    // parameter's name, expression and comment, mitcad#250; mitcad#761).
    break;
  }
  case QEvent::ShortcutOverride: {
    // The list's keys are the list's while it is open: Enter does not
    // confirm the command, Esc does not cancel it (the window's shortcuts).
    if (!isOpen()) {
      break;
    }
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape || key == Qt::Key_Up ||
        key == Qt::Key_Down || key == Qt::Key_PageUp || key == Qt::Key_PageDown) {
      event->accept();
      return true;
    }
    break;
  }
  case QEvent::KeyPress: {
    if (!isOpen()) {
      break;
    }
    auto* key = static_cast<QKeyEvent*>(event);
    switch (key->key()) {
    case Qt::Key_Up:
      move(-1);
      return true;
    case Qt::Key_Down:
      move(1);
      return true;
    case Qt::Key_PageUp:
      move(-kPageRows);
      return true;
    case Qt::Key_PageDown:
      move(kPageRows);
      return true;
    case Qt::Key_Return:
    case Qt::Key_Enter:
      activate(m_list->currentItem());
      return true;
    case Qt::Key_Escape:
      close(QStringLiteral("Esc"));
      return true;
    case Qt::Key_Tab:
    case Qt::Key_Backtab:
      close();
      return false; // the field's own Tab (a sketch tool's next value)
    default:
      break;
    }
    if (m_mode != Mode::Browsing) {
      break; // typed into the field: textEdited follows
    }
    // Browsing: what is typed searches; the field keeps its text.
    if (key->key() == Qt::Key_Backspace) {
      if (!m_filter.isEmpty()) {
        m_filter.chop(1);
        open(Mode::Browsing);
      }
      return true;
    }
    const QString text = key->text();
    if (!text.isEmpty() && text.at(0).isPrint() &&
        !(key->modifiers() & (Qt::ControlModifier | Qt::MetaModifier | Qt::AltModifier))) {
      m_filter += text;
      open(Mode::Browsing);
      return true;
    }
    if (key->key() == Qt::Key_Left || key->key() == Qt::Key_Right || key->key() == Qt::Key_Home ||
        key->key() == Qt::Key_End || key->key() == Qt::Key_Delete) {
      close();
    }
    break;
  }
  case QEvent::FocusIn:
    QTimer::singleShot(0, this, [this] { logFx(); });
    break;
  case QEvent::FocusOut:
  case QEvent::Hide:
    close();
    break;
  case QEvent::Move:
  case QEvent::Resize:
    if (isOpen()) {
      place();
    }
    break;
  case QEvent::PaletteChange:
  case QEvent::StyleChange:
    updateIcon();
    break;
  default:
    break;
  }
  return QObject::eventFilter(watched, event);
}

} // namespace mitcad
