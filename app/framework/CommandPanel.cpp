// SPDX-License-Identifier: MIT
#include "CommandPanel.hpp"

#include <algorithm>
#include <iterator>

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QEvent>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QStyle>
#include <QListWidget>
#include <QPushButton>
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "ChromeStyle.hpp"
#include "GlassCard.hpp"
#include "Icons.hpp"
#include "GlassCard.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

// Colours of the selection inputs in order, unless an input sets its own.
const QColor kInputColors[] = {QColor(0x1f, 0x7a, 0xff), QColor(0xff, 0x8c, 0x00),
                               QColor(0x2c, 0xa0, 0x2c), QColor(0x9b, 0x30, 0xff)};

QColor paletteColor(int index) {
  return kInputColors[static_cast<std::size_t>(index) % std::size(kInputColors)];
}

QString countText(const InputDef& input, int count) {
  if (count == 0) {
    return input.min > 0 ? QObject::tr("Select") : QObject::tr("None (all)");
  }
  return QObject::tr("%1 selected").arg(count);
}

bool isShown(const InputDef& input, const CommandState& state) {
  return !input.visible || input.visible(state);
}

// "sets.1.edges": the list "sets", row 1.
bool splitKey(const QString& key, QString& list, int& index) {
  const QStringList parts = key.split(QLatin1Char('.'));
  if (parts.size() < 3) {
    return false;
  }
  bool number = false;
  index = parts[1].toInt(&number);
  list = parts[0];
  return number;
}

QToolButton* smallButton(const QString& text, const QString& tip) {
  auto* button = new QToolButton;
  button->setText(text);
  button->setToolTip(tip);
  button->setFocusPolicy(Qt::NoFocus);
  button->setAutoRaise(true);
  if (chromeStyle() == ChromeStyle::Floating) {
    setFlatCardButton(button);
    button->setFixedSize(24, 24);
  }
  return button;
}

// The look of the controls in a floating card: flat and rounded like the
// ribbon's buttons. A checked toggle is outlined in the accent colour, which
// is the system's on macOS and so is not palette(highlight); the styles are
// set again when the palette changes (CommandPanel::applyControlStyles).
void styleToggle(QToolButton* button, const QColor& accent) {
  button->setAutoRaise(true);
  button->setFixedSize(30, 26);
  button->setStyleSheet(QStringLiteral(
      "QToolButton { border: 1px solid rgba(128, 128, 128, 70); background: rgba(128, 128, 128, 25);"
      " border-radius: 7px; padding: 1px; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 55); }"
      "QToolButton:pressed { background: rgba(128, 128, 128, 90); }"
      "QToolButton:checked { border: 1px solid %1; background: rgba(128, 128, 128, 55); }")
                            .arg(accent.name()));
}

// The selection button: a capsule that is filled with the accent colour
// while its input is the one that takes the picks.
void styleSelect(QPushButton* button, const QColor& accent, const QColor& accentText) {
  button->setFixedHeight(26);
  button->setStyleSheet(QStringLiteral(
      "QPushButton { border: 1px solid rgba(128, 128, 128, 70); background: rgba(128, 128, 128, 25);"
      " border-radius: 13px; padding: 0 12px; }"
      "QPushButton:hover { background: rgba(128, 128, 128, 55); }"
      "QPushButton:checked { border: 1px solid %1; background: %1; color: %2; }")
                            .arg(accent.name(), accentText.name()));
}

// The text fields: rounded with a subtle fill like the combo boxes next to
// them, and the accent colour as the focus ring (QMacStyle's own ring does not
// follow a style sheet). The selection colours stay the palette's.
void styleLineEdit(QLineEdit* edit, const QColor& accent, const QColor& error) {
  edit->setStyleSheet(QStringLiteral(
      "QLineEdit { border: 1px solid rgba(128, 128, 128, 70); background: rgba(128, 128, 128, 25);"
      " border-radius: 7px; padding: 3px 6px; min-height: 18px; }"
      "QLineEdit:focus { border: 1.5px solid %1; }"
      "QLineEdit[invalid=\"true\"] { border: 1px solid %2; }"
      "QLineEdit:disabled { background: rgba(128, 128, 128, 12); }")
                          .arg(accent.name(), error.name()));
}

} // namespace

CommandPanel::CommandPanel(const CommandDef& def, const CommandState& state, bool editing,
                           QWidget* parent)
    : QWidget(parent), m_def(def) {
  setObjectName(QStringLiteral("commandPanel"));
  auto* layout = new QVBoxLayout(this);
  if (chromeStyle() == ChromeStyle::Floating) {
    layout->setContentsMargins(4, 4, 4, 4); // the card has its own margins
    layout->setSpacing(4);                  // compact: the whole panel fits the card
  }

  auto* header = new QHBoxLayout;
  auto* icon = new QLabel;
  icon->setPixmap(themeIcon(def.icon).pixmap(QSize(24, 24), devicePixelRatioF()));
  auto* title = new QLabel(QStringLiteral("<b>%1</b>%2").arg(
      def.name.toHtmlEscaped(), editing ? tr(" <span style='color:gray'>(edit)</span>") : QString()));
  header->addWidget(icon);
  header->addWidget(title, 1);
  layout->addLayout(header);

  m_form = new QFormLayout;
  m_form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
  if (chromeStyle() == ChromeStyle::Floating) {
    m_form->setVerticalSpacing(3);
  }
  int selectionIndex = 0;
  for (const InputDef& input : def.inputs) {
    if (input.type == InputDef::Type::Selection) {
      m_colors.insert(input.id, input.color.isValid() ? input.color : paletteColor(selectionIndex));
      ++selectionIndex;
    } else if (input.type == InputDef::Type::List) {
      m_listColors.insert(input.id, selectionIndex);
      ++selectionIndex;
    }
  }
  for (const InputDef& input : def.inputs) {
    createRow(input, input.id, m_form, state, QString(), -1);
  }
  layout->addLayout(m_form);

  m_result = new QLabel;
  m_result->setObjectName(QStringLiteral("commandResult"));
  m_result->setWordWrap(true);
  m_result->setTextFormat(Qt::RichText);
  m_result->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_result->setFrameShape(QFrame::StyledPanel);
  m_result->setMargin(4);
  m_result->setVisible(static_cast<bool>(def.inspect));
  layout->addWidget(m_result);

  m_message = new QLabel;
  m_message->setWordWrap(true);
  m_message->setObjectName(QStringLiteral("commandMessage"));
  layout->addWidget(m_message);

  auto* buttons = new QHBoxLayout;
  m_ok = new QPushButton(tr("OK"));
  m_ok->setObjectName(QStringLiteral("commandOk"));
  auto* cancel = new QPushButton(tr("Cancel"));
  cancel->setObjectName(QStringLiteral("commandCancel"));
  for (QPushButton* button : {m_ok, cancel}) {
    // The keyboard stays in the value fields; Enter and Esc work anywhere.
    button->setFocusPolicy(Qt::NoFocus);
    button->setAutoDefault(false);
  }
  // In a floating card OK is the accent-coloured default button.
  m_ok->setDefault(chromeStyle() == ChromeStyle::Floating);
  m_ok->setToolTip(def.inspect ? tr("Close (Enter)") : tr("Create the feature (Enter)"));
  cancel->setToolTip(tr("Close without changes (Esc)"));
  connect(m_ok, &QPushButton::clicked, this, &CommandPanel::accepted);
  connect(cancel, &QPushButton::clicked, this, &CommandPanel::rejected);
  buttons->addStretch();
  buttons->addWidget(m_ok);
  buttons->addWidget(cancel);
  layout->addLayout(buttons);
  layout->addStretch();

  applyControlStyles();
  refresh(state);
}

void CommandPanel::applyControlStyles() {
  if (chromeStyle() != ChromeStyle::Floating) {
    return;
  }
  const QColor accent = accentColor(palette());
  // White on the accent colour, as the title bar's prominent button has it.
  const QColor accentText = palette().color(QPalette::HighlightedText);
  for (const auto& row : m_rows) {
    if (row->select != nullptr) {
      styleSelect(row->select, accent, accentText);
    }
    if (row->edit != nullptr) {
      styleLineEdit(row->edit, accent, errorColor(palette()));
    }
    if (auto* toggle = qobject_cast<QToolButton*>(row->check)) {
      styleToggle(toggle, accent);
    }
  }
}

void CommandPanel::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange) {
    applyControlStyles();
  }
}

CommandPanel::~CommandPanel() = default;

CommandPanel::Row* CommandPanel::createRow(const InputDef& input, const QString& key,
                                           QFormLayout* form, const CommandState& state,
                                           const QString& list, int index) {
  auto owned = std::make_unique<Row>();
  Row& row = *owned;
  row.input = &input;
  row.key = key;
  row.list = list;
  row.index = index;
  row.form = form;
  m_byKey.insert(key, owned.get());
  m_rows.push_back(std::move(owned));

  auto* container = new QWidget;
  auto* column = new QVBoxLayout(container);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(1);
  QWidget* field = nullptr;
  const CommandState scope = list.isEmpty() ? state : state.row(list, index);

  switch (input.type) {
  case InputDef::Type::Selection: {
    auto* line = new QWidget;
    auto* box = new QHBoxLayout(line);
    box->setContentsMargins(0, 0, 0, 0);
    row.select = new QPushButton(swatchIcon(colorOf(key)), QString());
    row.select->setCheckable(true);
    row.select->setFocusPolicy(Qt::NoFocus);
    row.select->setToolTip(input.tooltip.isEmpty() ? tr("Click to pick %1").arg(input.label)
                                                   : input.tooltip);
    row.clear = smallButton(QStringLiteral("✕"), tr("Clear the selection"));
    connect(row.select, &QPushButton::clicked, this, [this, key] { emit selectionActivated(key); });
    connect(row.clear, &QToolButton::clicked, this, [this, key] { emit selectionCleared(key); });
    box->addWidget(row.select, 1);
    box->addWidget(row.clear);
    field = line;
    if (input.ordered) {
      // The picks in order, with buttons that move them.
      auto* ordered = new QWidget;
      auto* orderedBox = new QHBoxLayout(ordered);
      orderedBox->setContentsMargins(0, 0, 0, 0);
      row.order = new QListWidget;
      row.order->setFocusPolicy(Qt::NoFocus);
      row.order->setMaximumHeight(90);
      row.order->setObjectName(QStringLiteral("order_") + key);
      auto* moves = new QVBoxLayout;
      moves->setSpacing(0);
      QListWidget* items = row.order;
      auto* up = smallButton(QStringLiteral("▲"), tr("Move up"));
      auto* down = smallButton(QStringLiteral("▼"), tr("Move down"));
      auto* remove = smallButton(QStringLiteral("✕"), tr("Remove"));
      up->setObjectName(QStringLiteral("up_") + key);
      down->setObjectName(QStringLiteral("down_") + key);
      connect(up, &QToolButton::clicked, this, [this, key, items] {
        const int at = items->currentRow();
        if (at > 0) {
          emit itemMoved(key, at, at - 1);
        }
      });
      connect(down, &QToolButton::clicked, this, [this, key, items] {
        const int at = items->currentRow();
        if (at >= 0 && at + 1 < items->count()) {
          emit itemMoved(key, at, at + 1);
        }
      });
      connect(remove, &QToolButton::clicked, this, [this, key, items] {
        if (items->currentRow() >= 0) {
          emit itemRemoved(key, items->currentRow());
        }
      });
      moves->addWidget(up);
      moves->addWidget(down);
      moves->addWidget(remove);
      moves->addStretch();
      orderedBox->addWidget(row.order, 1);
      orderedBox->addLayout(moves);
      column->addWidget(line);
      field = ordered;
    }
    break;
  }
  case InputDef::Type::Value:
  case InputDef::Type::Text: {
    row.edit = new QLineEdit(scope.text(input.id));
    if (input.type == InputDef::Type::Value) {
      row.edit->setToolTip(input.tooltip.isEmpty()
                               ? tr("A value or an expression, such as 20, 2 in or d1 * 2")
                               : input.tooltip);
    } else {
      row.edit->setToolTip(input.tooltip);
    }
    connect(row.edit, &QLineEdit::textEdited, this,
            [this, key](const QString& text) { emit textEdited(key, text); });
    if (input.type == InputDef::Type::Value) {
      connect(row.edit, &QLineEdit::editingFinished, this, [this, key] { emit valueCommitted(key); });
    }
    field = row.edit;
    break;
  }
  case InputDef::Type::Choice: {
    row.combo = new QComboBox;
    // No keyboard focus: choosing leaves it in the value field, where the
    // typed value goes.
    row.combo->setFocusPolicy(Qt::NoFocus);
    row.combo->setToolTip(input.tooltip);
    // Options that follow other inputs are filled by refreshRow.
    if (!input.choicesFor) {
      for (const auto& [value, label] : input.choices) {
        row.combo->addItem(label, value);
      }
    }
    connect(row.combo, &QComboBox::activated, this, [this, key, combo = row.combo](int at) {
      emit choiceChanged(key, combo->itemData(at).toString());
    });
    field = row.combo;
    break;
  }
  case InputDef::Type::Check: {
    auto* check = new QCheckBox;
    check->setFocusPolicy(Qt::NoFocus);
    check->setToolTip(input.tooltip);
    // Only as wide as its box, which is where a click toggles it.
    check->setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
    connect(check, &QCheckBox::clicked, this, [this, key](bool on) { emit checkChanged(key, on); });
    row.check = check;
    field = check;
    break;
  }
  case InputDef::Type::Flip: {
    auto* flip = new QToolButton;
    flip->setIcon(themeIcon(QStringLiteral("flip")));
    flip->setIconSize(QSize(20, 20));
    flip->setCheckable(true);
    flip->setFocusPolicy(Qt::NoFocus);
    flip->setToolTip(input.tooltip.isEmpty() ? tr("Flip the direction") : input.tooltip);
    connect(flip, &QToolButton::clicked, this, [this, key](bool on) { emit checkChanged(key, on); });
    row.check = flip;
    field = flip;
    break;
  }
  case InputDef::Type::List: {
    auto* box = new QWidget;
    auto* boxLayout = new QVBoxLayout(box);
    boxLayout->setContentsMargins(0, 0, 0, 0);
    boxLayout->setSpacing(2);
    row.rowsLayout = new QVBoxLayout;
    row.rowsLayout->setSpacing(2);
    boxLayout->addLayout(row.rowsLayout);
    if (!input.addLabel.isEmpty()) {
      row.add = new QPushButton(themeIcon(QStringLiteral("add")), input.addLabel);
      row.add->setFocusPolicy(Qt::NoFocus);
      row.add->setObjectName(QStringLiteral("add_") + key);
      connect(row.add, &QPushButton::clicked, this, [this, key] { emit rowAdded(key); });
      boxLayout->addWidget(row.add, 0, Qt::AlignLeft);
    }
    field = box;
    break;
  }
  }
  field->setObjectName(QStringLiteral("input_") + key);
  if (field != container) {
    column->addWidget(field);
  }
  row.note = new QLabel;
  row.note->setWordWrap(true);
  row.note->hide();
  column->addWidget(row.note);
  row.field = container;

  if (input.type == InputDef::Type::List) {
    // A list's rows take the whole width, under its name.
    auto* title = new QLabel(QStringLiteral("<b>%1</b>").arg(input.label.toHtmlEscaped()));
    title->setToolTip(input.tooltip);
    row.title = title;
    form->addRow(title);
    form->addRow(container);
  } else {
    row.label = new QLabel(input.label);
    row.label->setToolTip(input.tooltip);
    if (form != nullptr) {
      form->addRow(row.label, container);
    }
  }
  updateNote(row);
  return &row;
}

void CommandPanel::rebuildList(Row& list, const CommandState& state) {
  const int count = state.rows(list.key);
  if (count == list.built) {
    return;
  }
  const bool first = list.built < 0;
  // The rows' frames go; their widgets may be sending the signal that got
  // here (a row's remove button), so they are deleted later.
  for (QWidget* frame : list.frames) {
    frame->hide();
    frame->deleteLater();
  }
  list.frames.clear();
  list.removes.clear();
  for (auto it = m_rows.begin(); it != m_rows.end();) {
    if ((*it)->list == list.key) {
      m_byKey.remove((*it)->key);
      it = m_rows.erase(it);
    } else {
      ++it;
    }
  }
  const InputDef& input = *list.input;
  // Rows of values only (positions) are lines of a table; others are forms.
  const bool table = std::all_of(input.children.begin(), input.children.end(), [](const InputDef& child) {
    return child.type == InputDef::Type::Value || child.type == InputDef::Type::Text;
  });
  for (int i = 0; i < count; ++i) {
    auto* frame = new QFrame;
    frame->setFrameShape(table ? QFrame::NoFrame : QFrame::StyledPanel);
    auto* frameLayout = new QVBoxLayout(frame);
    frameLayout->setContentsMargins(table ? 0 : 4, table ? 0 : 2, table ? 0 : 4, table ? 0 : 2);
    frameLayout->setSpacing(1);
    auto* header = new QHBoxLayout;
    header->setSpacing(3);
    auto* name = new QLabel(QStringLiteral("<span style='color:gray'>%1</span>")
                                .arg(input.rowLabel.arg(i + 1).toHtmlEscaped()));
    header->addWidget(name, table ? 0 : 1);
    QToolButton* remove = smallButton(QStringLiteral("✕"), tr("Remove this row"));
    remove->setObjectName(QStringLiteral("remove_%1.%2").arg(list.key).arg(i));
    remove->setEnabled(count > input.minRows);
    const QString key = list.key;
    connect(remove, &QToolButton::clicked, this, [this, key, i] { emit rowRemoved(key, i); });
    if (table) {
      for (const InputDef& child : input.children) {
        Row* cell = createRow(child, CommandState::rowKey(list.key, i, child.id), nullptr, state, list.key, i);
        header->addWidget(cell->label);
        header->addWidget(cell->field, 1);
      }
      header->addWidget(remove);
      frameLayout->addLayout(header);
    } else {
      header->addWidget(remove);
      frameLayout->addLayout(header);
      auto* form = new QFormLayout;
      form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
      for (const InputDef& child : input.children) {
        createRow(child, CommandState::rowKey(list.key, i, child.id), form, state, list.key, i);
      }
      frameLayout->addLayout(form);
    }
    list.rowsLayout->addWidget(frame);
    list.frames.push_back(frame);
    list.removes.push_back(remove);
  }
  list.built = count;
  applyControlStyles();
  setActiveSelection(m_active);
  if (!first) {
    // Where the new rows are, for UI tests.
    scheduleLogLayout();
  }
}

void CommandPanel::refresh(const CommandState& state) {
  refreshRows(state);
  // Rows a choice showed or hid move the others: where they are now, for
  // UI tests.
  QStringList shown;
  for (const Row* row : orderedRows()) {
    if (!row->field->isHidden()) {
      shown << row->key;
    }
  }
  const QString layout = shown.join(QLatin1Char(','));
  if (!m_shownRows.isEmpty() && layout != m_shownRows) {
    scheduleLogLayout();
  }
  m_shownRows = layout;
}

void CommandPanel::scheduleLogLayout() {
  if (m_logTimer == nullptr) {
    m_logTimer = new QTimer(this);
    m_logTimer->setSingleShot(true);
    m_logTimer->setInterval(50);
    connect(m_logTimer, &QTimer::timeout, this, &CommandPanel::logLayout);
    TestSync::watch(m_logTimer);
  }
  m_logTimer->start();
}

void CommandPanel::refreshRows(const CommandState& state) {
  for (const InputDef& input : m_def.inputs) {
    Row* row = m_byKey.value(input.id);
    if (row == nullptr) {
      continue;
    }
    refreshRow(*row, state, state);
    if (input.type == InputDef::Type::List) {
      rebuildList(*row, state);
      for (int i = 0; i < row->built; ++i) {
        const CommandState scope = state.row(input.id, i);
        for (const InputDef& child : input.children) {
          if (Row* childRow = m_byKey.value(CommandState::rowKey(input.id, i, child.id))) {
            refreshRow(*childRow, scope, state);
          }
        }
      }
    }
  }
}

void CommandPanel::refreshRow(Row& row, const CommandState& scope, const CommandState& state) {
  const InputDef& input = *row.input;
  const bool shown = isShown(input, scope);
  if (input.type == InputDef::Type::List) {
    row.form->setRowVisible(row.title, shown);
    row.form->setRowVisible(row.field, shown);
    return;
  }
  if (row.form != nullptr) {
    row.form->setRowVisible(row.field, shown);
  } else {
    row.label->setVisible(shown);
    row.field->setVisible(shown);
  }
  const QString& id = input.id;
  switch (input.type) {
  case InputDef::Type::Selection: {
    const Selection& items = scope.items(id);
    row.select->setText(countText(input, static_cast<int>(items.size())));
    if (row.order != nullptr) {
      QStringList texts;
      for (int i = 0; i < items.size(); ++i) {
        texts << QStringLiteral("%1. %2").arg(i + 1).arg(items[i].describe());
      }
      QStringList shownTexts;
      for (int i = 0; i < row.order->count(); ++i) {
        shownTexts << row.order->item(i)->text();
      }
      if (texts != shownTexts) {
        const int current = row.order->currentRow();
        row.order->clear();
        row.order->addItems(texts);
        row.order->setCurrentRow(std::min(current, static_cast<int>(texts.size()) - 1));
      }
    }
    break;
  }
  case InputDef::Type::Value:
  case InputDef::Type::Text:
    if (row.edit->text() != scope.text(id)) {
      row.edit->setText(scope.text(id));
    }
    break;
  case InputDef::Type::Choice: {
    const QSignalBlocker block(row.combo);
    if (input.choicesFor) {
      QVector<QPair<QString, QString>> options = input.choicesFor(scope);
      // A value the options lack (an edited thread's designation the table
      // writes otherwise) stays shown as it is.
      const QString current = scope.choice(id);
      const bool listed = std::any_of(options.begin(), options.end(),
                                      [&current](const auto& option) { return option.first == current; });
      if (!listed && !current.isEmpty()) {
        options.append({current, current});
      }
      bool same = row.combo->count() == options.size();
      for (int i = 0; same && i < options.size(); ++i) {
        same = row.combo->itemData(i).toString() == options[i].first && row.combo->itemText(i) == options[i].second;
      }
      if (!same) {
        row.combo->clear();
        for (const auto& [value, label] : options) {
          row.combo->addItem(label, value);
        }
        QStringList labels;
        for (const auto& option : options) {
          labels << option.second;
        }
        qDebug().noquote() << QStringLiteral("Panel %1 options %2: %3")
                                  .arg(m_def.name, row.key, labels.join(QStringLiteral(" | ")));
      }
    }
    row.combo->setCurrentIndex(qMax(0, row.combo->findData(scope.choice(id))));
    break;
  }
  case InputDef::Type::Check:
  case InputDef::Type::Flip: {
    const QSignalBlocker block(row.check);
    row.check->setChecked(scope.checked(id));
    break;
  }
  case InputDef::Type::List:
    break;
  }
  Q_UNUSED(state);
}

void CommandPanel::setActiveSelection(const QString& key) {
  m_active = key;
  for (const auto& row : m_rows) {
    if (row->select != nullptr) {
      row->select->setChecked(row->key == key);
    }
  }
}

void CommandPanel::setInputError(const QString& key, const QString& message) {
  m_notes[key].first = message;
  if (Row* row = m_byKey.value(key)) {
    updateNote(*row);
  }
}

void CommandPanel::setValueHint(const QString& key, const QString& hint) {
  m_notes[key].second = hint;
  if (Row* row = m_byKey.value(key)) {
    updateNote(*row);
  }
}

void CommandPanel::updateNote(Row& row) {
  const auto [errorText, hint] = m_notes.value(row.key);
  const bool error = !errorText.isEmpty();
  const QString text = error ? errorText : hint;
  row.note->setText(text);
  setErrorStyleSheet(row.note, error ? QStringLiteral("color: %1;") : QStringLiteral("color: gray;"));
  if (row.note->isHidden() != text.isEmpty()) {
    // The rows below move.
    row.note->setVisible(!text.isEmpty());
    if (!m_shownRows.isEmpty()) {
      scheduleLogLayout();
    }
  }
  if (row.edit != nullptr && chromeStyle() == ChromeStyle::Floating) {
    // The field's style sheet is the card's (styleLineEdit), with the error
    // border for the property.
    row.edit->setProperty("invalid", error);
    row.edit->style()->unpolish(row.edit);
    row.edit->style()->polish(row.edit);
  } else if (row.edit != nullptr) {
    setErrorStyleSheet(row.edit, error ? QStringLiteral("border: 1px solid %1;") : QString());
  }
}

void CommandPanel::setMessage(const QString& message, bool error) {
  m_message->setText(message);
  setErrorStyleSheet(m_message, error ? QStringLiteral("color: %1;") : QStringLiteral("color: gray;"));
}

void CommandPanel::setResult(const QString& text) { m_result->setText(text); }

void CommandPanel::setOkEnabled(bool enabled) { m_ok->setEnabled(enabled); }

QColor CommandPanel::colorOf(const QString& key) const {
  const auto own = m_colors.constFind(key);
  if (own != m_colors.cend()) {
    return own.value();
  }
  QString list;
  int index = 0;
  if (splitKey(key, list, index) && m_listColors.contains(list)) {
    if (const Row* row = m_byKey.value(key); row != nullptr && row->input->color.isValid()) {
      return row->input->color;
    }
    return paletteColor(m_listColors.value(list) + index);
  }
  return paletteColor(0);
}

std::vector<const CommandPanel::Row*> CommandPanel::orderedRows() const {
  std::vector<const Row*> rows;
  for (const InputDef& input : m_def.inputs) {
    const Row* row = m_byKey.value(input.id);
    if (row == nullptr) {
      continue;
    }
    rows.push_back(row);
    if (input.type == InputDef::Type::List) {
      for (int i = 0; i < row->built; ++i) {
        for (const InputDef& child : input.children) {
          if (const Row* childRow = m_byKey.value(CommandState::rowKey(input.id, i, child.id))) {
            rows.push_back(childRow);
          }
        }
      }
    }
  }
  return rows;
}

bool CommandPanel::focusFirstValue() {
  for (const Row* row : orderedRows()) {
    if (row->edit != nullptr && row->input->type == InputDef::Type::Value &&
        row->field->isVisibleTo(this)) {
      row->edit->setFocus();
      row->edit->selectAll();
      return true;
    }
  }
  return false;
}

bool CommandPanel::focusValue(const QString& key) {
  const Row* row = m_byKey.value(key);
  if (row == nullptr || row->edit == nullptr || !row->field->isVisibleTo(this)) {
    return false;
  }
  row->edit->setFocus();
  row->edit->selectAll();
  return true;
}

void CommandPanel::logLayout() const {
  // Rows made or shown just now are placed first.
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const auto position = [](const QWidget* widget) {
    const QPoint center = GlassCard::mapToHost(widget, widget->rect().center());
    return QStringLiteral("%1,%2").arg(center.x()).arg(center.y());
  };
  for (const Row* row : orderedRows()) {
    const QWidget* widget = row->select != nullptr  ? static_cast<QWidget*>(row->select)
                            : row->edit != nullptr  ? static_cast<QWidget*>(row->edit)
                            : row->combo != nullptr ? static_cast<QWidget*>(row->combo)
                            : row->check != nullptr ? static_cast<QWidget*>(row->check)
                            : row->add != nullptr   ? static_cast<QWidget*>(row->add)
                                                    : row->field;
    const bool hidden = !row->field->isVisibleTo(this);
    qDebug().noquote() << QStringLiteral("Panel %1 input %2 at %3%4")
                              .arg(m_def.name, row->key, position(widget),
                                   hidden ? QStringLiteral(" (hidden)") : QString());
    if (row->order != nullptr && !hidden) {
      for (const char* button : {"up_", "down_"}) {
        if (const auto* move = findChild<QToolButton*>(QString::fromLatin1(button) + row->key)) {
          qDebug().noquote() << QStringLiteral("Panel %1 %2%3 at %4")
                                    .arg(m_def.name, QString::fromLatin1(button), row->key,
                                         position(move));
        }
      }
    }
    if (row->rowsLayout != nullptr && !hidden) {
      if (row->add != nullptr) {
        qDebug().noquote() << QStringLiteral("Panel %1 add %2 at %3")
                                  .arg(m_def.name, row->key, position(row->add));
      }
      for (std::size_t i = 0; i < row->removes.size(); ++i) {
        qDebug().noquote() << QStringLiteral("Panel %1 remove %2.%3 at %4")
                                  .arg(m_def.name, row->key)
                                  .arg(i)
                                  .arg(position(row->removes[i]));
      }
    }
  }
  qDebug().noquote() << QStringLiteral("Panel %1 OK at %2").arg(m_def.name, position(m_ok));
}

} // namespace mitcad
