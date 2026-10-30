// SPDX-License-Identifier: MIT
#include "RulesPage.hpp"

#include <algorithm>
#include <optional>
#include <utility>

#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QDebug>
#include <QDialog>
#include <QFocusEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QMenu>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QSplitter>
#include <QStyle>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextEdit>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>

#include "../framework/FormulaEditor.hpp"
#include "../framework/ParameterSuggest.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativeDialogs.hpp"
#include "RuleCanvas.hpp"

namespace {

// Whether a list's item editor is open: the focus is on a widget inside the
// list's viewport (QAbstractItemView::state() is protected).
bool editingIn(const QAbstractItemView* view) {
  const QWidget* focus = QApplication::focusWidget();
  return focus != nullptr && view->viewport()->isAncestorOf(focus);
}

} // namespace

namespace mitcad::rules {

namespace {

QStringList strings(const QJsonValue& value) {
  QStringList out;
  const QJsonArray array = value.toArray();
  for (const QJsonValue& item : array) {
    if (item.isString()) {
      out << item.toString();
    }
  }
  return out;
}

QWidget* labelled(QLabel* label, QWidget* buddy) {
  label->setBuddy(buddy);
  return label;
}

// The page's own words (mitcad#966, #967; app/COMMANDS.md and the user
// guide quote them).
const char* const kLead = "Rules take away values and combinations that are not allowed. Most designs need none.";
const char* const kClosed = "This table is closed: only its rows are allowed, so rules are not needed.";
const char* const kNoRules = "No rules yet. A rule is, for example: when Color is Black, Length is at most 1000 mm.";
const char* const kChoose = "Add a rule, or choose one in the list.";
const char* const kTextHint = "Enter saves the rule, Shift+Enter starts a new line, Esc restores it.";

// The status line of rules not saved yet: "Rule 1 and Rule 2 are not
// saved yet: fill their dashed places." (mitcad#967).
QString waitingText(const QStringList& names) {
  if (names.size() == 1) {
    return QStringLiteral("%1 is not saved yet: fill its dashed places.").arg(names.front());
  }
  QStringList first = names;
  const QString last = first.takeLast();
  return QStringLiteral("%1 and %2 are not saved yet: fill their dashed places.")
      .arg(first.join(QStringLiteral(", ")), last);
}

} // namespace

RulesPage::RulesPage(RulesSource source, QWidget* parent) : QWidget(parent), m_source(std::move(source)) {
  setObjectName(QStringLiteral("rulesPage"));
  m_lead = new QLabel(QString::fromUtf8(kLead));
  m_lead->setObjectName(QStringLiteral("rulesLead"));
  m_lead->setWordWrap(true);
  m_closedNote = new QLabel(QString::fromUtf8(kClosed));
  m_closedNote->setObjectName(QStringLiteral("rulesClosedNote"));
  m_closedNote->setWordWrap(true);
  m_closedNote->hide();
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("rulesList"));
  m_list->setColumnCount(2);
  m_list->setHeaderLabels({QStringLiteral("Rule"), QString()});
  m_list->setRootIsDecorated(false);
  m_list->setUniformRowHeights(true);
  m_list->header()->setStretchLastSection(false);
  m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_list->setEditTriggers(QAbstractItemView::DoubleClicked | QAbstractItemView::EditKeyPressed);
  m_list->installEventFilter(this);
  m_add = new QPushButton(QStringLiteral("&Add"));
  m_add->setObjectName(QStringLiteral("rulesAdd"));
  m_add->setToolTip(QStringLiteral("A new rule from a template, complete and saved at once: replace its values"));
  // The templates (mitcad#966), filled in refresh().
  m_addMenu = new QMenu(m_add);
  m_addMenu->setObjectName(QStringLiteral("rulesAddMenu"));
  m_addMenu->setToolTipsVisible(true);
  m_add->setMenu(m_addMenu);
  m_duplicate = new QPushButton(QStringLiteral("D&uplicate"));
  m_duplicate->setObjectName(QStringLiteral("rulesDuplicate"));
  m_delete = new QPushButton(QStringLiteral("&Delete"));
  m_delete->setObjectName(QStringLiteral("rulesDelete"));
  for (QPushButton* button : {m_add, m_duplicate, m_delete}) {
    button->setAutoDefault(false);
  }
  auto* left = new QWidget;
  auto* leftLayout = new QVBoxLayout(left);
  leftLayout->setContentsMargins(0, 0, 0, 0);
  leftLayout->addWidget(labelled(new QLabel(QStringLiteral("&Rules")), m_list));
  leftLayout->addWidget(m_list, 1);
  auto* buttons = new QHBoxLayout;
  buttons->addWidget(m_add);
  buttons->addWidget(m_duplicate);
  buttons->addWidget(m_delete);
  buttons->addStretch(1);
  leftLayout->addLayout(buttons);

  m_palette = new RulePalette;
  m_canvas = new RuleCanvas;
  m_palette->setCanvas(m_canvas);
  m_canvas->setDeleteArea(m_palette);
  m_palette->installEventFilter(this);
  m_canvas->installEventFilter(this);
  m_check = new QPushButton(QStringLiteral("Chec&k Rules"));
  m_check->setObjectName(QStringLiteral("rulesCheck"));
  m_check->setAutoDefault(false);
  m_status = new QLabel;
  m_status->setObjectName(QStringLiteral("rulesStatus"));
  m_status->setWordWrap(true);
  m_status->hide();
  // The rule as text (mitcad#965): always in sight under the blocks, and
  // edited as they are. A line per part, so a long one scrolls rather than
  // wraps (a problem's line and column are the lines as typed).
  m_text = new QPlainTextEdit;
  m_text->setObjectName(QStringLiteral("ruleText"));
  m_text->setLineWrapMode(QPlainTextEdit::NoWrap);
  m_text->setTabChangesFocus(true);
  m_text->setToolTip(QStringLiteral("The rule as text, a line per part: when <condition> (or always), then "
                                    "<action>, else <action>"));
  const int lineHeight = m_text->fontMetrics().lineSpacing();
  m_text->setMinimumHeight(lineHeight * 4 + 12);
  m_text->setMaximumHeight(lineHeight * 7 + 12);
  m_text->installEventFilter(this);
  auto* textHint = new QLabel(QString::fromUtf8(kTextHint));
  textHint->setObjectName(QStringLiteral("ruleTextHint"));
  textHint->setForegroundRole(QPalette::PlaceholderText);
  m_report = new QPlainTextEdit;
  m_report->setObjectName(QStringLiteral("rulesReport"));
  m_report->setReadOnly(true);
  m_report->setMaximumHeight(140);
  m_report->hide();

  auto* paletteColumn = new QWidget;
  auto* paletteLayout = new QVBoxLayout(paletteColumn);
  paletteLayout->setContentsMargins(0, 0, 0, 0);
  paletteLayout->addWidget(labelled(new QLabel(QStringLiteral("&Palette")), m_palette));
  paletteLayout->addWidget(m_palette, 1);
  auto* canvasColumn = new QWidget;
  auto* canvasLayout = new QVBoxLayout(canvasColumn);
  canvasLayout->setContentsMargins(0, 0, 0, 0);
  auto* tools = new QHBoxLayout;
  tools->addWidget(labelled(new QLabel(QStringLiteral("&Blocks")), m_canvas));
  tools->addStretch(1);
  tools->addWidget(m_check);
  canvasLayout->addLayout(tools);
  canvasLayout->addWidget(m_canvas, 1);
  auto* editor = new QSplitter(Qt::Horizontal);
  editor->addWidget(paletteColumn);
  editor->addWidget(canvasColumn);
  editor->setStretchFactor(0, 0);
  editor->setStretchFactor(1, 1);
  editor->setSizes({170, 480});
  auto* right = new QWidget;
  auto* rightLayout = new QVBoxLayout(right);
  rightLayout->setContentsMargins(0, 0, 0, 0);
  rightLayout->addWidget(editor, 1);
  rightLayout->addWidget(m_status);
  auto* textRow = new QHBoxLayout;
  textRow->addWidget(labelled(new QLabel(QStringLiteral("&Text")), m_text));
  textRow->addStretch(1);
  textRow->addWidget(textHint);
  rightLayout->addLayout(textRow);
  rightLayout->addWidget(m_text);
  rightLayout->addWidget(m_report);

  auto* split = new QSplitter(Qt::Horizontal);
  split->addWidget(left);
  split->addWidget(right);
  split->setStretchFactor(0, 0);
  split->setStretchFactor(1, 1);
  split->setSizes({200, 660});
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->addWidget(m_lead);
  layout->addWidget(m_closedNote);
  layout->addWidget(split, 1);
  // Logged as the window's other tabs log theirs (ConfigurationsDialog).
  qDebug().noquote() << QStringLiteral("Configurations lead Rules: %1").arg(m_lead->text());

  if (m_source.parameters.parameters) {
    m_canvas->setFormulaEditor([this](const QString& text, QWidget* owner) -> std::optional<QString> {
      ParameterFieldOptions options;
      options.anyKind = true;
      options.kind = ParameterKind::Unitless;
      options.create = false;
      options.label = QStringLiteral("rule condition");
      FormulaEditor dialog(m_source.parameters, options, text, owner);
      if (dialog.exec() != QDialog::Accepted) {
        return std::nullopt;
      }
      return dialog.expression();
    });
  }

  connect(m_addMenu, &QMenu::aboutToShow, this, [this] {
    QStringList entries;
    const QList<QAction*> actions = m_addMenu->actions();
    for (const QAction* action : actions) {
      if (action->isSeparator()) {
        continue;
      }
      QString label = action->text();
      label.remove(QLatin1Char('&'));
      entries << (action->isEnabled() ? label : QStringLiteral("%1 (disabled: %2)").arg(label, action->toolTip()));
    }
    qDebug().noquote() << QStringLiteral("Rules add menu: %1").arg(entries.join(QStringLiteral(" | ")));
  });
  connect(m_duplicate, &QPushButton::clicked, this, [this] { duplicateRule(); });
  connect(m_delete, &QPushButton::clicked, this, [this] { deleteRule(); });
  connect(m_check, &QPushButton::clicked, this, [this] { checkRules(); });
  connect(m_text, &QPlainTextEdit::textChanged, this, [this] {
    if (m_settingText) {
      return;
    }
    m_textDirty = true;
    if (!m_text->extraSelections().isEmpty()) {
      m_text->setExtraSelections({});
    }
  });
  connect(m_list, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building) {
      return;
    }
    m_current = item != nullptr ? m_list->indexOfTopLevelItem(item) : -1;
    showCurrent();
  });
  connect(m_list, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building || item == nullptr) {
      return;
    }
    const int index = m_list->indexOfTopLevelItem(item);
    // After the list's own handling of the edit: the list is filled anew.
    QTimer::singleShot(0, this, [this, index] { listEdited(index); });
  });
  connect(m_canvas, &RuleCanvas::edited, this, [this](const Rule& rule, const QString& what) {
    if (m_current < 0 || m_current >= static_cast<int>(m_rules.size())) {
      return;
    }
    std::vector<Rule> changed = m_rules;
    changed[static_cast<std::size_t>(m_current)] = rule;
    commit(std::move(changed), what);
  });
  connect(m_canvas, &RuleCanvas::paletteRequested, this, [this] {
    if (m_palette->currentItem() == nullptr || m_palette->currentItem()->childCount() > 0) {
      m_palette->setCurrentItem(m_palette->entry(QStringLiteral("compare")));
    }
    m_palette->setFocus(Qt::OtherFocusReason);
    qDebug().noquote() << QStringLiteral("Rule palette chooses for %1").arg(m_canvas->focusName());
  });
  refresh();
}

void RulesPage::refresh() {
  const QString current =
      m_current >= 0 && m_current < static_cast<int>(m_rules.size()) ? m_rules[static_cast<std::size_t>(m_current)].name
                                                                       : QString();
  QJsonObject answer;
  if (m_source.configurations) {
    answer = m_source.configurations();
  }
  m_table = answer.value(QStringLiteral("configurations")).toObject();
  m_problems = strings(answer.value(QStringLiteral("problems")));
  const QStringList options = strings(m_table.value(QStringLiteral("options")));
  m_infos = m_source.configure ? optionInfos(m_source.configure()) : std::vector<OptionInfo>();
  QStringList numbers;
  for (const OptionInfo& info : m_infos) {
    if (!info.text) {
      numbers << info.name;
    }
  }
  m_canvas->setOptions(options, numbers);
  m_canvas->setOptionInfos(m_infos);
  m_text->setPlaceholderText(exampleRuleText(m_infos).join(QLatin1Char('\n')));
  // A closed table allows only its rows: rules are not needed (mitcad#966).
  const bool closed = m_table.value(QStringLiteral("closed")).toBool();
  if (m_closedNote->isHidden() == closed) {
    m_closedNote->setVisible(closed);
    qDebug().noquote() << QStringLiteral("Rules closed note: %1")
                              .arg(closed ? m_closedNote->text() : QStringLiteral("none"));
  }
  QStringList parameters;
  if (m_source.parameters.parameters) {
    try {
      const QJsonArray all = m_source.parameters.parameters();
      for (const QJsonValue& parameter : all) {
        const QString name = parameter.toObject().value(QStringLiteral("name")).toString();
        if (!name.isEmpty()) {
          parameters << name;
        }
      }
    } catch (...) {
      // The model computes now: the names come with the next refresh.
    }
  }
  m_canvas->setParameters(parameters);
  m_tableName.clear();
  m_tableColumns.clear();
  if (m_source.tables) {
    std::vector<SuggestedTable> tables = suggestedTables(m_source.tables());
    if (!tables.empty()) {
      m_tableName = tables.front().name;
      for (const SuggestedTable::Column& column : tables.front().columns) {
        m_tableColumns << column.name;
      }
    }
    m_canvas->setTables(std::move(tables));
  }
  updateAddMenu();
  const QJsonArray rules = m_table.value(QStringLiteral("rules")).toArray();
  // Nothing to show anew (the window refreshes the page after every change
  // of the design): an open editor of a name or a block stays open.
  if (!m_drafts.empty() || (rules == m_shownRules && m_problems == m_shownProblems)) {
    m_shownProblems = m_problems;
    if (!editingIn(m_list) && m_canvas->editor() == nullptr) {
      fillList();
    }
    return;
  }
  m_shownRules = rules;
  m_shownProblems = m_problems;
  m_rules.clear();
  for (const QJsonValue& rule : rules) {
    m_rules.push_back(ruleFromJson(rule.toObject()));
  }
  m_refusal.clear();
  m_current = -1;
  for (std::size_t i = 0; i < m_rules.size(); ++i) {
    if (m_rules[i].name == current) {
      m_current = static_cast<int>(i);
    }
  }
  if (m_current < 0 && !m_rules.empty()) {
    m_current = 0;
  }
  fillList();
  showCurrent();
}

QString RulesPage::problemOf(int index) const {
  if (index < 0 || index >= static_cast<int>(m_rules.size())) {
    return {};
  }
  const Rule& rule = m_rules[static_cast<std::size_t>(index)];
  if (!isComplete(rule)) {
    if (rule.name.trimmed().isEmpty()) {
      return QStringLiteral("Not saved: the rule has no name");
    }
    if (rule.hasElse && rule.otherwise.empty()) {
      return QStringLiteral("Not saved: its else has no action");
    }
    return QStringLiteral("Not saved yet: fill its dashed places");
  }
  if (!m_drafts.empty() && !m_refusal.isEmpty() && (m_refusal.contains(rule.name) || index == m_current)) {
    return QStringLiteral("Not saved: %1").arg(m_refusal);
  }
  QStringList problems;
  for (const QString& problem : m_problems) {
    if (problem.contains(rule.name)) {
      problems << problem;
    }
  }
  return problems.join(QStringLiteral("; "));
}

void RulesPage::fillList() {
  m_building = true;
  m_list->clear();
  QStringList rows;
  for (std::size_t i = 0; i < m_rules.size(); ++i) {
    const Rule& rule = m_rules[i];
    auto* item = new QTreeWidgetItem(m_list, QStringList{rule.name});
    item->setData(0, NativeControlIdentifierRole, QStringLiteral("rules.") + rule.name);
    item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable | Qt::ItemIsUserCheckable);
    item->setCheckState(0, rule.enabled ? Qt::Checked : Qt::Unchecked);
    const QString comment = rule.extra.value(QStringLiteral("comment")).toString();
    const QString problem = problemOf(static_cast<int>(i));
    if (!problem.isEmpty()) {
      item->setIcon(1, style()->standardIcon(QStyle::SP_MessageBoxWarning));
      item->setToolTip(0, problem);
      item->setToolTip(1, problem);
    } else if (!comment.isEmpty()) {
      item->setToolTip(0, comment);
    }
    rows << QStringLiteral("Rules row %1: %2, %3")
                .arg(rule.name, rule.enabled ? QStringLiteral("enabled") : QStringLiteral("disabled"),
                     problem.isEmpty() ? QStringLiteral("ok") : problem);
  }
  if (m_current >= 0 && m_current < m_list->topLevelItemCount()) {
    m_list->setCurrentItem(m_list->topLevelItem(m_current));
  }
  m_building = false;
  const QString joined = rows.join(QLatin1Char('\n'));
  if (joined != m_loggedRows) {
    m_loggedRows = joined;
    qDebug().noquote() << QStringLiteral("Rules: %1").arg(m_rules.size());
    for (const QString& row : rows) {
      qDebug().noquote() << row;
    }
  }
  m_duplicate->setEnabled(m_current >= 0);
  m_delete->setEnabled(m_current >= 0);
  // The empty state (mitcad#967): the canvas says what a rule is.
  const bool none = m_rules.empty();
  m_canvas->setHint(QString::fromUtf8(none ? kNoRules : kChoose));
  const QString empty = none ? QString::fromUtf8(kNoRules) : QString();
  if (empty != m_loggedEmpty) {
    m_loggedEmpty = empty;
    if (none) {
      qDebug().noquote() << QStringLiteral("Configurations empty Rules: %1").arg(empty);
    }
  }
  updateStatus();
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void RulesPage::showCurrent() {
  if (m_current >= 0 && m_current < static_cast<int>(m_rules.size())) {
    m_canvas->setRule(m_rules[static_cast<std::size_t>(m_current)]);
  } else {
    m_canvas->clearRule();
  }
  m_duplicate->setEnabled(m_current >= 0);
  m_delete->setEnabled(m_current >= 0);
  updateText();
}

void RulesPage::updateStatus() {
  QStringList waiting;
  for (std::size_t i = 0; i < m_rules.size(); ++i) {
    if (!isComplete(m_rules[i])) {
      waiting << m_rules[i].name;
    }
  }
  QString text;
  if (!m_textProblem.isEmpty()) {
    text = QStringLiteral("Not saved: %1").arg(m_textProblem);
  } else if (!waiting.isEmpty()) {
    text = waitingText(waiting);
  } else if (!m_drafts.empty() && !m_refusal.isEmpty()) {
    text = QStringLiteral("Not saved: %1").arg(m_refusal);
  }
  m_status->setText(text);
  m_status->setVisible(!text.isEmpty());
  if (!m_textProblem.isEmpty() || (!m_drafts.empty() && !m_refusal.isEmpty())) {
    setErrorStyleSheet(m_status);
  } else {
    setWarningStyleSheet(m_status);
  }
  if (text != m_loggedStatus) {
    m_loggedStatus = text;
    qDebug().noquote() << QStringLiteral("Rules status: %1").arg(text.isEmpty() ? QStringLiteral("none") : text);
  }
}

void RulesPage::updateText() {
  QStringList lines;
  QString name;
  const bool shown = m_current >= 0 && m_current < static_cast<int>(m_rules.size());
  bool empty = false;
  if (shown) {
    const Rule& rule = m_rules[static_cast<std::size_t>(m_current)];
    lines = ruleText(rule);
    name = rule.name;
    empty = !rule.hasWhen && rule.then.empty() && !rule.hasElse;
  }
  // What is being typed stays; anything else shows the rule as it is (an
  // empty rule as nothing, so that the example lines show).
  if (!(m_textDirty && m_text->hasFocus())) {
    const QString text = shown && !empty ? lines.join(QLatin1Char('\n')) : QString();
    if (m_text->toPlainText() != text) {
      m_settingText = true;
      m_text->setPlainText(text);
      m_settingText = false;
    }
    m_textDirty = false;
    if (!m_textProblem.isEmpty() || !m_text->extraSelections().isEmpty()) {
      m_textProblem.clear();
      m_text->setExtraSelections({});
      updateStatus();
    }
  }
  m_text->setReadOnly(!shown);
  const QString logged = QStringLiteral("Rule as text %1: %2").arg(name, lines.join(QStringLiteral(" / ")));
  if (logged != m_loggedText) {
    m_loggedText = logged;
    qDebug().noquote() << logged;
  }
}

bool RulesPage::applyText() {
  if (m_current < 0 || m_current >= static_cast<int>(m_rules.size())) {
    return false;
  }
  const Rule current = m_rules[static_cast<std::size_t>(m_current)];
  Rule read;
  TextProblem problem;
  if (!ruleFromText(m_text->toPlainText(), current, m_infos, &read, &problem)) {
    m_textProblem = problem.text();
    markText(problem);
    qDebug().noquote() << QStringLiteral("Rule text not read: %1").arg(m_textProblem);
    updateStatus();
    return false;
  }
  m_textProblem.clear();
  m_textDirty = false;
  m_text->setExtraSelections({});
  if (ruleToJson(read) == ruleToJson(current)) {
    // The same rule (spaces, a line as it was): its text as it prints.
    updateText();
    updateStatus();
    return true;
  }
  std::vector<Rule> changed = m_rules;
  changed[static_cast<std::size_t>(m_current)] = std::move(read);
  commit(std::move(changed), QStringLiteral("Edit %1 as text").arg(current.name));
  return true;
}

void RulesPage::restoreText() {
  m_textDirty = false;
  m_textProblem.clear();
  m_text->setExtraSelections({});
  updateText();
  updateStatus();
  qDebug().noquote() << QStringLiteral("Rule text restored");
}

void RulesPage::markText(const TextProblem& problem) {
  QList<QTextEdit::ExtraSelection> marks;
  const QTextBlock block = m_text->document()->findBlockByNumber(problem.line - 1);
  if (block.isValid()) {
    const QColor error = errorColor(palette());
    QColor fill = error;
    fill.setAlpha(40);
    QTextEdit::ExtraSelection line;
    line.format.setBackground(fill);
    line.format.setProperty(QTextFormat::FullWidthSelection, true);
    line.cursor = QTextCursor(block);
    marks.append(line);
    // The place itself, underlined; the caret goes there.
    const int end = block.length() - 1;
    const int start = std::min(std::max(problem.column - 1, 0), end);
    QTextCursor place(block);
    place.setPosition(block.position() + start);
    place.setPosition(block.position() + std::min(start + problem.length, end), QTextCursor::KeepAnchor);
    QTextEdit::ExtraSelection underline;
    underline.format.setUnderlineStyle(QTextCharFormat::WaveUnderline);
    underline.format.setUnderlineColor(error);
    underline.cursor = place;
    marks.append(underline);
    QTextCursor caret(block);
    caret.setPosition(block.position() + start);
    m_text->setTextCursor(caret);
  }
  m_text->setExtraSelections(marks);
}

QString RulesPage::uniqueName(const QString& base, int first) const {
  const auto taken = [this](const QString& name) {
    for (const Rule& rule : m_rules) {
      if (rule.name == name) {
        return true;
      }
    }
    return false;
  };
  for (int n = first;; ++n) {
    const QString name = QStringLiteral("%1 %2").arg(base).arg(n);
    if (!taken(name)) {
      return name;
    }
  }
}

QString RulesPage::freeName(const QString& base) const {
  for (const Rule& rule : m_rules) {
    if (rule.name == base) {
      return uniqueName(base, 2);
    }
  }
  return base;
}

void RulesPage::updateAddMenu() {
  const std::vector<RuleTemplate> templates = ruleTemplates(m_infos, m_tableName, m_tableColumns);
  QStringList state;
  for (const RuleTemplate& entry : templates) {
    state << QStringLiteral("%1:%2:%3").arg(entry.id, entry.enabled ? entry.lines.join(QLatin1Char('/')) : entry.why);
  }
  const QString joined = state.join(QLatin1Char('|'));
  if (joined == m_menuState) {
    return;
  }
  m_menuState = joined;
  m_addMenu->clear();
  for (const RuleTemplate& entry : templates) {
    if (entry.id == QLatin1String("empty")) {
      m_addMenu->addSeparator(); // the empty rule apart, last
    }
    QAction* action = m_addMenu->addAction(entry.label);
    action->setObjectName(QStringLiteral("rulesAdd_") + entry.id);
    action->setData(entry.id);
    action->setEnabled(entry.enabled);
    // What it makes, as text; or why it cannot.
    action->setToolTip(!entry.enabled          ? entry.why
                       : entry.lines.isEmpty() ? QStringLiteral("A rule with no condition and no action yet")
                                               : entry.lines.join(QLatin1Char('\n')));
    const QString id = entry.id;
    // After the menu has closed: the new rule's name editor keeps the focus.
    connect(action, &QAction::triggered, this,
            [this, id] { QTimer::singleShot(0, this, [this, id] { addRule(id); }); });
  }
}

bool RulesPage::addRule(const QString& templateId) {
  Rule rule;
  if (!ruleFromTemplate(templateId, m_infos, m_tableName, m_tableColumns, &rule)) {
    qDebug().noquote() << QStringLiteral("Rules add %1: not for this design").arg(templateId);
    return false;
  }
  // A template's name says what the rule does; the empty rule is numbered.
  rule.name = templateId == QLatin1String("empty") ? uniqueName(rule.name) : freeName(rule.name);
  std::vector<Rule> changed = m_rules;
  changed.push_back(rule);
  m_current = static_cast<int>(changed.size()) - 1;
  const QString name = rule.name;
  commit(std::move(changed), QStringLiteral("Add rule %1").arg(name));
  selectRule(name);
  // Its name, to be typed over at once.
  if (QTreeWidgetItem* item = m_list->currentItem()) {
    m_list->setFocus(Qt::OtherFocusReason);
    m_list->editItem(item, 0);
    qDebug().noquote() << QStringLiteral("Rules naming %1").arg(name);
  }
  return true;
}

void RulesPage::duplicateRule() {
  if (m_current < 0 || m_current >= static_cast<int>(m_rules.size())) {
    return;
  }
  std::vector<Rule> changed = m_rules;
  Rule copy = changed[static_cast<std::size_t>(m_current)];
  copy.name = uniqueName(copy.name, 2);
  const QString name = copy.name;
  changed.insert(changed.begin() + m_current + 1, copy);
  m_current += 1;
  commit(std::move(changed), QStringLiteral("Duplicate rule as %1").arg(name));
  selectRule(name);
}

void RulesPage::deleteRule() {
  if (m_current < 0 || m_current >= static_cast<int>(m_rules.size())) {
    return;
  }
  std::vector<Rule> changed = m_rules;
  const QString name = changed[static_cast<std::size_t>(m_current)].name;
  changed.erase(changed.begin() + m_current);
  if (m_current >= static_cast<int>(changed.size())) {
    m_current = static_cast<int>(changed.size()) - 1;
  }
  commit(std::move(changed), QStringLiteral("Delete rule %1").arg(name));
}

bool RulesPage::selectRule(const QString& name) {
  for (std::size_t i = 0; i < m_rules.size(); ++i) {
    if (m_rules[i].name == name) {
      m_current = static_cast<int>(i);
      m_building = true;
      m_list->setCurrentItem(m_list->topLevelItem(m_current));
      m_building = false;
      showCurrent();
      return true;
    }
  }
  return false;
}

void RulesPage::listEdited(int index) {
  if (index < 0 || index >= static_cast<int>(m_rules.size()) || index >= m_list->topLevelItemCount()) {
    return;
  }
  const QTreeWidgetItem* item = m_list->topLevelItem(index);
  const QString name = item->text(0).trimmed();
  const bool enabled = item->checkState(0) == Qt::Checked;
  const Rule& rule = m_rules[static_cast<std::size_t>(index)];
  if (name == rule.name && enabled == rule.enabled) {
    return;
  }
  if (name != rule.name) {
    QString refused;
    if (name.isEmpty()) {
      refused = QStringLiteral("A rule needs a name.");
    }
    for (std::size_t i = 0; i < m_rules.size() && refused.isEmpty(); ++i) {
      if (static_cast<int>(i) != index && m_rules[i].name == name) {
        refused = QStringLiteral("Another rule is named %1.").arg(name);
      }
    }
    if (!refused.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Rules rename refused: %1").arg(refused);
      fillList();
      m_status->setText(refused);
      setErrorStyleSheet(m_status);
      m_status->show();
      return;
    }
  }
  std::vector<Rule> changed = m_rules;
  Rule& edited = changed[static_cast<std::size_t>(index)];
  const QString what = name != edited.name ? QStringLiteral("Rename rule %1 to %2").arg(edited.name, name)
                       : enabled          ? QStringLiteral("Enable rule %1").arg(name)
                                          : QStringLiteral("Disable rule %1").arg(name);
  edited.name = name;
  if (enabled != edited.enabled) {
    edited.enabled = enabled;
    edited.enabledWritten = edited.enabledWritten || !enabled;
  }
  m_current = index;
  commit(std::move(changed), what);
}

void RulesPage::commit(std::vector<Rule> rules, const QString& what) {
  std::vector<Rule> previous = std::move(m_rules);
  m_rules = std::move(rules);
  bool complete = true;
  QJsonArray array;
  for (const Rule& rule : m_rules) {
    bool done = false;
    array.append(ruleToJson(rule, &done));
    complete = complete && done;
  }
  if (complete && m_source.setRules) {
    QString error;
    if (m_source.setRules(array, &error)) {
      m_drafts.clear();
      m_refusal.clear();
      qDebug().noquote() << QStringLiteral("Rules saved: %1").arg(what);
      refresh();
      emit saved();
      return;
    }
    m_refusal = error.isEmpty() ? QStringLiteral("the design refused the rules") : error;
    qDebug().noquote() << QStringLiteral("Rules refused: %1: %2").arg(what, m_refusal);
  } else {
    m_refusal.clear();
    qDebug().noquote() << QStringLiteral("Rules not saved yet: %1").arg(what);
  }
  m_drafts.push_back(std::move(previous));
  fillList();
  showCurrent();
}

bool RulesPage::undo() {
  if (!m_drafts.empty()) {
    m_rules = std::move(m_drafts.back());
    m_drafts.pop_back();
    m_refusal.clear();
    qDebug().noquote() << QStringLiteral("Rules undo: the last change not saved");
    if (m_current >= static_cast<int>(m_rules.size())) {
      m_current = static_cast<int>(m_rules.size()) - 1;
    }
    fillList();
    showCurrent();
    return true;
  }
  if (m_source.undo) {
    qDebug().noquote() << QStringLiteral("Rules undo: the design's last step");
    m_source.undo();
    refresh();
    return true;
  }
  return false;
}

void RulesPage::checkRules() {
  if (!m_source.checkRules) {
    return;
  }
  const QJsonObject report = m_source.checkRules();
  QStringList lines;
  if (!m_drafts.empty()) {
    lines << QStringLiteral("The saved rules were checked; the changes not saved yet were not.");
  }
  if (report.contains(QStringLiteral("error"))) {
    lines << QStringLiteral("Check Rules failed: %1").arg(report.value(QStringLiteral("error")).toString());
  } else {
    QStringList found = strings(report.value(QStringLiteral("lines")));
    if (found.isEmpty()) {
      const QJsonArray never = report.value(QStringLiteral("never_allowed")).toArray();
      for (const QJsonValue& value : never) {
        const QJsonObject entry = value.toObject();
        found << QStringLiteral("%1 %2 is never allowed")
                     .arg(entry.value(QStringLiteral("option")).toString(), entry.value(QStringLiteral("text")).toString());
      }
      for (const QString& rule : strings(report.value(QStringLiteral("never_applies")))) {
        found << QStringLiteral("The rule %1 never applies").arg(rule);
      }
      const QJsonArray rows = report.value(QStringLiteral("rows")).toArray();
      for (const QJsonValue& value : rows) {
        const QJsonObject entry = value.toObject();
        found << QStringLiteral("The row %1 breaks the rule %2: %3")
                     .arg(entry.value(QStringLiteral("row")).toString(), entry.value(QStringLiteral("rule")).toString(),
                          entry.value(QStringLiteral("message")).toString());
      }
      if (found.isEmpty()) {
        found << QStringLiteral("No problems found.");
      }
    }
    lines << found;
  }
  m_report->setPlainText(lines.join(QLatin1Char('\n')));
  m_report->show();
  for (const QString& line : lines) {
    qDebug().noquote() << QStringLiteral("Check Rules: %1").arg(line);
  }
  const QJsonArray never = report.value(QStringLiteral("never_allowed")).toArray();
  for (const QJsonValue& value : never) {
    const QJsonObject entry = value.toObject();
    qDebug().noquote() << QStringLiteral("Check Rules never allowed: %1 %2")
                              .arg(entry.value(QStringLiteral("option")).toString(),
                                   entry.value(QStringLiteral("text")).toString());
  }
}

bool RulesPage::eventFilter(QObject* watched, QEvent* event) {
  // The text (mitcad#965): Enter reads and saves it, Shift+Enter starts a
  // new line, Esc restores it while it has changes, and leaving it reads it.
  if (watched == m_text) {
    if (event->type() == QEvent::ShortcutOverride || event->type() == QEvent::KeyPress) {
      auto* key = static_cast<QKeyEvent*>(event);
      const bool enter = key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter;
      const bool escape = key->key() == Qt::Key_Escape && key->modifiers() == Qt::NoModifier &&
                          (m_textDirty || !m_textProblem.isEmpty());
      if ((enter && !m_text->isReadOnly()) || escape) {
        event->accept();
        if (event->type() == QEvent::KeyPress) {
          if (escape) {
            restoreText();
          } else if (key->modifiers().testFlag(Qt::ShiftModifier)) {
            QTextCursor cursor = m_text->textCursor();
            cursor.insertText(QStringLiteral("\n"));
            m_text->setTextCursor(cursor);
          } else {
            applyText();
          }
        }
        return true;
      }
    } else if (event->type() == QEvent::FocusOut && m_textDirty &&
               static_cast<QFocusEvent*>(event)->reason() != Qt::PopupFocusReason) {
      applyText();
    }
    return QWidget::eventFilter(watched, event);
  }
  if (event->type() == QEvent::ShortcutOverride || event->type() == QEvent::KeyPress) {
    auto* key = static_cast<QKeyEvent*>(event);
    const bool undoKey = key->matches(QKeySequence::Undo);
    const bool redoKey = key->matches(QKeySequence::Redo);
    if ((watched == m_canvas || watched == m_palette || watched == m_list) && (undoKey || redoKey) &&
        m_canvas->editor() == nullptr) {
      const bool takes = undoKey ? (!m_drafts.empty() || static_cast<bool>(m_source.undo))
                                 : (m_drafts.empty() && static_cast<bool>(m_source.redo));
      if (takes) {
        event->accept();
        if (event->type() == QEvent::KeyPress) {
          if (undoKey) {
            undo();
          } else {
            m_source.redo();
            refresh();
          }
        }
        return event->type() == QEvent::KeyPress;
      }
    }
    if (watched == m_list && event->type() == QEvent::KeyPress && !editingIn(m_list) &&
        key->modifiers() == Qt::NoModifier) {
      if (key->key() == Qt::Key_Insert) {
        m_add->showMenu(); // the templates, as Add
        return true;
      }
      if (key->key() == Qt::Key_Delete) {
        deleteRule();
        return true;
      }
    }
  }
  return QWidget::eventFilter(watched, event);
}

void RulesPage::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  QTimer::singleShot(0, this, [this] {
    logPlaces();
    m_palette->logPlaces();
    m_canvas->logPlaces();
  });
}

void RulesPage::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  logPlaces();
}

void RulesPage::logPlaces() {
  if (!isVisible()) {
    return;
  }
  QWidget* top = window();
  if (top->parentWidget() != nullptr) {
    top = top->parentWidget()->window();
  }
  const QPoint origin = top->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  const std::pair<const char*, QWidget*> widgets[] = {
      {"add", m_add}, {"duplicate", m_duplicate}, {"delete", m_delete}, {"text", m_text}, {"check", m_check}};
  for (const auto& [name, widget] : widgets) {
    const QPoint at = widget->mapToGlobal(widget->rect().center()) - origin;
    lines << QStringLiteral("Rules %1 at %2,%3").arg(QLatin1String(name)).arg(at.x()).arg(at.y());
  }
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* item = m_list->topLevelItem(i);
    const QRect rect = m_list->visualItemRect(item);
    if (!m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Rules row %1 out of sight").arg(item->text(0));
      continue;
    }
    const QPoint at = m_list->viewport()->mapToGlobal(QPoint(rect.left() + 40, rect.center().y())) - origin;
    lines << QStringLiteral("Rules row %1 at %2,%3").arg(item->text(0)).arg(at.x()).arg(at.y());
  }
  const QString joined = lines.join(QLatin1Char('\n'));
  if (joined == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = joined;
  for (const QString& line : lines) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad::rules
