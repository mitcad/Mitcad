// SPDX-License-Identifier: MIT
#include "CommandPanel.hpp"

#include <algorithm>
#include <functional>
#include <iterator>
#include <utility>
#include <vector>

#include <QAbstractScrollArea>
#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFocusEvent>
#include <QFont>
#include <QFormLayout>
#include <QFrame>
#include <QHeaderView>
#include <QHBoxLayout>
#include <QIcon>
#include <QImage>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QStyle>
#include <QStyleOptionComboBox>
#include <QStylePainter>
#include <QListWidget>
#include <QPixmap>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QStandardItem>
#include <QStandardItemModel>
#include <QTimer>
#include <QToolButton>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "ChromeStyle.hpp"
#include "CommandMessage.hpp"
#include "CutTargetsPanel.hpp"
#include "GlassCard.hpp"
#include "Icons.hpp"
#include "NoteRow.hpp"
#include "ParameterField.hpp"
#include "ResultRows.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"

#include "../platform/MacDialogs.hpp"

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
    // An optional pick of one item is just none (mitcad#656: Align to
    // Edge); none of several takes them all (a cut's objects).
    return input.min > 0 ? QObject::tr("Select") : input.max == 1 ? QObject::tr("None") : QObject::tr("None (all)");
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

// Gives a value field the keyboard. With native glass the panel's card is a
// window of its own, so the field's window is activated first when another
// window of the same main window has the keys (the timeline card); a popup,
// a dialog or another application keeps them.
void focusField(QLineEdit* edit) {
  QWidget* window = edit->window();
  const QWidget* active = QApplication::activeWindow();
  if (active != nullptr && active != window && QApplication::activePopupWidget() == nullptr &&
      GlassCard::hostWindow(active) == GlassCard::hostWindow(window)) {
    window->activateWindow();
  }
  edit->setFocus();
  edit->selectAll();
}

QToolButton* smallButton(const QString& text, const QString& tip) {
  auto* button = new QToolButton;
  button->setText(text);
  button->setToolTip(tip);
  button->setFocusPolicy(Qt::NoFocus);
  button->setAutoRaise(true);
  if (text == QStringLiteral("▲")) {
    mac::styleIconToggle(button, QStringLiteral("arrow.up"));
  } else if (text == QStringLiteral("▼")) {
    mac::styleIconToggle(button, QStringLiteral("arrow.down"));
  } else if (text == QStringLiteral("✕")) {
    mac::styleIconToggle(button, QStringLiteral("xmark"));
  }
  button->setAccessibleName(tip);
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
#ifdef Q_OS_MACOS
  Q_UNUSED(accent);
  setFlatCardButton(button);
#else
  button->setStyleSheet(QStringLiteral(
      "QToolButton { border: 1px solid rgba(128, 128, 128, 70); background: rgba(128, 128, 128, 25);"
      " border-radius: 7px; padding: 1px; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 55); }"
      "QToolButton:pressed { background: rgba(128, 128, 128, 90); }"
      "QToolButton:checked { border: 1px solid %1; background: rgba(128, 128, 128, 55); }")
                            .arg(accent.name()));
#endif
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

// A drop-down whose chosen text is cut in the middle, and only when it
// must, so that a long name keeps its start and its end (mitcad#655: 3D
// Print's printer and process); it may be narrower than its longest
// option.
class MiddleElideComboBox : public QComboBox {
public:
  MiddleElideComboBox() {
    setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
    setMinimumContentsLength(8);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QStylePainter painter(this);
    QStyleOptionComboBox option;
    initStyleOption(&option);
    painter.drawComplexControl(QStyle::CC_ComboBox, option);
    const QRect field = style()->subControlRect(QStyle::CC_ComboBox, &option, QStyle::SC_ComboBoxEditField, this);
    const int icon = option.currentIcon.isNull() ? 0 : option.iconSize.width() + 4;
    option.currentText = fontMetrics().elidedText(option.currentText, Qt::ElideMiddle, std::max(0, field.width() - icon));
    painter.drawControl(QStyle::CE_ComboBoxLabel, option);
  }
};

// A choice shown as a tile (InputDef::choiceTile, mitcad#427): the chosen
// option's picture, its name and a smaller line under it. A click, or Enter
// or Space while it has the keyboard, calls `activated`. It takes the
// keyboard by Tab only: after a click Enter still confirms the command.
class TileButton : public QToolButton {
public:
  explicit TileButton(std::function<void()> activated) : m_activated(std::move(activated)) {
    // A choice tile's persistent outline identifies its selectable content;
    // it is not a tool button bezel (also distinguished by UI style audits).
    setProperty("commandChoiceTile", true);
    setFocusPolicy(Qt::TabFocus);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Minimum);
    auto* box = new QHBoxLayout(this);
    box->setContentsMargins(6, 4, 6, 4);
    box->setSpacing(8);
    m_image = new QLabel;
    m_image->setFixedSize(kImageSize, kImageSize);
    m_image->setAlignment(Qt::AlignCenter);
    m_title = new QLabel;
    m_title->setWordWrap(true);
    m_detail = new QLabel;
    m_detail->setWordWrap(true);
    m_detail->setForegroundRole(QPalette::PlaceholderText);
    QFont small = m_detail->font();
    if (small.pointSizeF() > 0.0) {
      small.setPointSizeF(small.pointSizeF() * 0.85);
    } else if (small.pixelSize() > 0) {
      small.setPixelSize(std::max(1, small.pixelSize() * 85 / 100));
    }
    m_detail->setFont(small);
    for (QLabel* label : {m_image, m_title, m_detail}) {
      label->setAttribute(Qt::WA_TransparentForMouseEvents);
    }
    auto* texts = new QVBoxLayout;
    texts->setSpacing(0);
    texts->addStretch(1);
    texts->addWidget(m_title);
    texts->addWidget(m_detail);
    texts->addStretch(1);
    box->addWidget(m_image);
    box->addLayout(texts, 1);
    setMinimumHeight(kImageSize + 8);
    connect(this, &QToolButton::clicked, this, [this] {
      if (m_activated) {
        m_activated();
      }
    });
  }

  // Shows a tile; false when it shows that already.
  bool setTile(const ChoiceTile& tile) {
    const qint64 key = tile.image.isNull() ? 0 : tile.image.cacheKey();
    if (m_shown && tile.title == m_title->text() && tile.detail == m_detail->text() && key == m_imageKey) {
      return false;
    }
    m_shown = true;
    m_imageKey = key;
    if (tile.image.isNull()) {
      m_image->clear();
    } else {
      const qreal ratio = devicePixelRatioF();
      QPixmap pixmap = QPixmap::fromImage(tile.image.scaled(QSize(kImageSize, kImageSize) * ratio,
                                                            Qt::KeepAspectRatio, Qt::SmoothTransformation));
      pixmap.setDevicePixelRatio(ratio);
      m_image->setPixmap(pixmap);
    }
    m_image->setVisible(!tile.image.isNull());
    m_title->setText(tile.title);
    m_detail->setText(tile.detail);
    m_detail->setVisible(!tile.detail.isEmpty());
    return true;
  }

protected:
  bool event(QEvent* event) override {
    // Enter and Space open it rather than confirming the command (the main
    // window's shortcut) while it has the keyboard.
    if (event->type() == QEvent::ShortcutOverride) {
      const auto* key = static_cast<QKeyEvent*>(event);
      if (key->modifiers() == Qt::NoModifier &&
          (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter || key->key() == Qt::Key_Space)) {
        event->accept();
        return true;
      }
    }
    return QToolButton::event(event);
  }
  void keyPressEvent(QKeyEvent* event) override {
    if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
      event->accept();
      click();
      return;
    }
    QToolButton::keyPressEvent(event); // Space: pressed and released, a click
  }

private:
  static constexpr int kImageSize = 40;
  std::function<void()> m_activated;
  QLabel* m_image = nullptr;
  QLabel* m_title = nullptr;
  QLabel* m_detail = nullptr;
  qint64 m_imageKey = 0;
  bool m_shown = false;
};

} // namespace

CommandPanel::CommandPanel(const CommandDef& def, const CommandState& state, bool editing,
                           QWidget* parent)
    : QWidget(parent), m_def(def) {
  setObjectName(QStringLiteral("commandPanel"));
  // Keep the widget signals for existing test and UI callers, and forward
  // them through the same presenter actions the native inspector uses.
  connect(this, &CommandPanel::textEdited, this, [this](const QString& key, const QString& value) {
    auto call = CommandPresenter::actions().textEdited;
    if (call) {
      call(key, value);
    }
  });
  connect(this, &CommandPanel::valueCommitted, this, [this](const QString& key) {
    auto call = CommandPresenter::actions().valueCommitted;
    if (call) {
      call(key);
    }
  });
  connect(this, &CommandPanel::choiceChanged, this, [this](const QString& key, const QString& value) {
    auto call = CommandPresenter::actions().choiceChanged;
    if (call) {
      call(key, value);
    }
  });
  connect(this, &CommandPanel::checkChanged, this, [this](const QString& key, bool value) {
    auto call = CommandPresenter::actions().checkChanged;
    if (call) {
      call(key, value);
    }
  });
  connect(this, &CommandPanel::selectionActivated, this, [this](const QString& key) {
    auto call = CommandPresenter::actions().selectionActivated;
    if (call) {
      call(key);
    }
  });
  connect(this, &CommandPanel::selectionCleared, this, [this](const QString& key) {
    auto call = CommandPresenter::actions().selectionCleared;
    if (call) {
      call(key);
    }
  });
  connect(this, &CommandPanel::itemMoved, this, [this](const QString& key, int from, int to) {
    auto call = CommandPresenter::actions().itemMoved;
    if (call) {
      call(key, from, to);
    }
  });
  connect(this, &CommandPanel::itemRemoved, this, [this](const QString& key, int index) {
    auto call = CommandPresenter::actions().itemRemoved;
    if (call) {
      call(key, index);
    }
  });
  connect(this, &CommandPanel::rowAdded, this, [this](const QString& key) {
    auto call = CommandPresenter::actions().rowAdded;
    if (call) {
      call(key);
    }
  });
  connect(this, &CommandPanel::rowRemoved, this, [this](const QString& key, int index) {
    auto call = CommandPresenter::actions().rowRemoved;
    if (call) {
      call(key, index);
    }
  });
  connect(this, &CommandPanel::buttonClicked, this, [this](const QString& key) {
    auto call = CommandPresenter::actions().buttonClicked;
    if (call) {
      call(key);
    }
  });
  connect(this, &CommandPanel::noteActionClicked, this, [this](const QString& key, int index) {
    auto call = CommandPresenter::actions().noteAction;
    if (call) {
      call(key, index);
    }
  });
  connect(this, &CommandPanel::accepted, this, [this] {
    auto call = CommandPresenter::actions().accepted;
    if (call) {
      call();
    }
  });
  connect(this, &CommandPanel::rejected, this, [this] {
    auto call = CommandPresenter::actions().rejected;
    if (call) {
      call();
    }
  });

  // The panel is as wide as the page shows it (the scroll area's viewport,
  // which never scrolls sideways), whatever its rows ask for: a row that
  // would want more is squeezed rather than pushing the fields and OK and
  // Cancel out of sight (mitcad#198).
  setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  auto* layout = new QVBoxLayout(this);
  if (chromeStyle() == ChromeStyle::Floating) {
    layout->setContentsMargins(4, 4, 4, 4); // the card has its own margins
    layout->setSpacing(10);
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
    m_form->setVerticalSpacing(8);
    m_form->setHorizontalSpacing(10);
    m_form->setRowWrapPolicy(QFormLayout::WrapLongRows);
    m_form->setLabelAlignment(Qt::AlignLeft | Qt::AlignVCenter);
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
  Row* previous = nullptr;
  for (const InputDef& input : def.inputs) {
    const bool flat = input.type != InputDef::Type::List && input.type != InputDef::Type::CutTargets;
    if (input.besidePrevious && flat && previous != nullptr && previous->form != nullptr &&
        previous->field != nullptr && previous->field->layout() != nullptr) {
      // On the previous row's line, after its field (InputDef::besidePrevious,
      // mitcad#656): the line gets a box of its own the first time.
      Row* cell = createRow(input, input.id, nullptr, state, QString(), -1);
      delete cell->label;
      cell->label = nullptr;
      auto* column = qobject_cast<QVBoxLayout*>(previous->field->layout());
      if (column != nullptr && column->count() > 0) {
        QWidget* first = column->itemAt(0)->widget();
        QWidget* line = first != nullptr && first->objectName() == QStringLiteral("besideLine") ? first : nullptr;
        if (line == nullptr) {
          line = new QWidget;
          line->setObjectName(QStringLiteral("besideLine"));
          auto* box = new QHBoxLayout(line);
          box->setContentsMargins(0, 0, 0, 0);
          box->setSpacing(4);
          if (first != nullptr) {
            column->removeWidget(first);
            box->addWidget(first, 1);
          }
          column->insertWidget(0, line);
        }
        // Wide enough to type in beside its fx button (mitcad#656).
        cell->field->setMinimumWidth(100);
        static_cast<QHBoxLayout*>(line->layout())->addWidget(cell->field, 1);
      }
      continue;
    }
    previous = createRow(input, input.id, m_form, state, QString(), -1);
  }
  layout->addLayout(m_form);

  // In the panel before it is shown: shown without a parent, the label
  // would be a window of its own for a moment, which takes the activation
  // from the main window, and when it goes the system may hand it to
  // another application's window, so that Enter no longer reaches the
  // command's OK (mitcad#255).
  m_result = new QLabel(this);
  m_result->setObjectName(QStringLiteral("commandResult"));
  m_result->setWordWrap(true);
  m_result->setTextFormat(Qt::RichText);
  m_result->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::LinksAccessibleByMouse);
  m_result->setFrameShape(QFrame::StyledPanel);
  m_result->setMargin(4);
  // A link in the result (Print Analysis' thinnest wall, mitcad#620) shows
  // what it points at.
  connect(m_result, &QLabel::linkActivated, this, [this](const QString& link) {
    const auto call = CommandPresenter::actions().resultLinkActivated;
    if (call) {
      call(link);
    }
  });
  layout->addWidget(m_result);
  // (A panel with only Close shows it once there is a text, mitcad#1010.)
  m_result->setVisible(static_cast<bool>(def.inspect) && !def.closeOnly);
  // The answer as rows of names and values in its place, when the
  // inspection gives them (Measure, mitcad#1010).
  m_resultRows = new ResultRows(def.name, this);
  m_resultRows->setFloatingStyle(chromeStyle() == ChromeStyle::Floating);
  layout->addWidget(m_resultRows);
  // A group folded or unfolded moves its rows and what is under them: the
  // places again (mitcad#528).
  m_resultRows->installEventFilter(this);

  // Errors and hints (mitcad#198): within the panel's width however long
  // their words, scrolling after a few lines, hidden when there is none.
  m_message = new CommandMessage;
  m_message->setObjectName(QStringLiteral("commandMessage"));
  layout->addWidget(m_message);

  auto* buttons = new QDialogButtonBox;
  m_buttonBox = buttons;
  if (def.closeOnly) {
    // A panel that only shows something (Measure, mitcad#1010): Close,
    // which OK and Cancel both were; Enter and Esc close it too.
    m_ok = new QPushButton(tr("Close"));
    m_ok->setObjectName(QStringLiteral("commandClose"));
    m_ok->setFocusPolicy(Qt::NoFocus);
    m_ok->setAutoDefault(false);
    m_ok->setToolTip(tr("Close (Enter, Esc)"));
    connect(m_ok, &QPushButton::clicked, this, [this] {
      qDebug().noquote() << QStringLiteral("Panel %1 Close clicked").arg(m_def.name);
      emit accepted();
    });
    buttons->addButton(m_ok, QDialogButtonBox::AcceptRole);
  } else {
    m_ok = new QPushButton(def.okLabel.isEmpty() ? tr("OK") : def.okLabel);
    m_ok->setObjectName(QStringLiteral("commandOk"));
    // A command that repeats (BUILD's Place, mitcad#375) keeps what it
    // applied: its Cancel only closes it.
    auto* cancel = new QPushButton(def.repeats ? tr("Close") : tr("Cancel"));
    cancel->setObjectName(QStringLiteral("commandCancel"));
    for (QPushButton* button : {m_ok, cancel}) {
      // The keyboard stays in the value fields; Enter and Esc work anywhere.
      button->setFocusPolicy(Qt::NoFocus);
      button->setAutoDefault(false);
    }
    // In a floating card OK is the accent-coloured default button.
    m_ok->setDefault(chromeStyle() == ChromeStyle::Floating);
    m_ok->setToolTip(def.apply               ? tr("%1 (Enter)").arg(def.okLabel.isEmpty() ? tr("OK") : def.okLabel)
                     : def.inspect           ? tr("Close (Enter)")
                     : def.okLabel.isEmpty() ? tr("Create the feature (Enter)")
                                             : tr("%1 (Enter)").arg(def.okLabel));
    cancel->setToolTip(def.repeats ? tr("Close; what was placed stays (Esc)") : tr("Close without changes (Esc)"));
    connect(m_ok, &QPushButton::clicked, this, &CommandPanel::accepted);
    connect(cancel, &QPushButton::clicked, this, &CommandPanel::rejected);
    buttons->addButton(m_ok, QDialogButtonBox::AcceptRole);
    buttons->addButton(cancel, QDialogButtonBox::RejectRole);
  }
  layout->addWidget(buttons);
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
  m_result->setFrameShape(QFrame::NoFrame);
  m_result->setMargin(10);
  m_result->setStyleSheet(QStringLiteral(
      "QLabel { background: rgba(128, 128, 128, 18); border: none; border-radius: 10px; }"));
  for (const auto& row : m_rows) {
    if (row->select != nullptr) {
      styleSelect(row->select, accent, accentText);
    }
    if (row->edit != nullptr) {
      styleLineEdit(row->edit, accent, errorColor(palette()));
    }
    if (auto* toggle = qobject_cast<QToolButton*>(row->check); toggle != nullptr && !row->input->disclosure) {
      styleToggle(toggle, accent);
    }
    for (QToolButton* button : row->choiceButtons) {
      styleToggle(button, accent);
    }
    if (row->tile != nullptr) {
      // A tile: rounded like the fields, the accent colour as its focus ring.
      row->tile->setStyleSheet(QStringLiteral(
          "QToolButton { border: 1px solid rgba(128, 128, 128, 70); background: rgba(128, 128, 128, 25);"
          " border-radius: 7px; }"
          "QToolButton:hover { background: rgba(128, 128, 128, 55); }"
          "QToolButton:pressed { background: rgba(128, 128, 128, 90); }"
          "QToolButton:focus { border: 1.5px solid %1; }")
                                   .arg(accent.name()));
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
    row.select->setObjectName(QStringLiteral("select_") + key);
    row.select->setCheckable(true);
    row.select->setFocusPolicy(Qt::NoFocus);
    row.select->setToolTip(input.tooltip.isEmpty() ? tr("Click to pick %1").arg(input.label)
                                                   : input.tooltip);
    row.clear = smallButton(QStringLiteral("✕"), tr("Clear the selection"));
    row.clear->setObjectName(QStringLiteral("clear_") + key);
    connect(row.select, &QPushButton::clicked, this, [this, key] { emit selectionActivated(key); });
    connect(row.clear, &QToolButton::clicked, this, [this, key] { emit selectionCleared(key); });
    box->addWidget(row.select, 1);
    box->addWidget(row.clear);
    field = line;
    if (input.ordered) {
      // The picks in order, with buttons that move them.
      auto* ordered = new QWidget;
#ifdef Q_OS_MACOS
      auto* orderedBox = new QVBoxLayout(ordered);
#else
      auto* orderedBox = new QHBoxLayout(ordered);
#endif
      orderedBox->setContentsMargins(0, 0, 0, 0);
      row.order = new QListWidget;
      row.order->setFocusPolicy(Qt::NoFocus);
      row.order->setMaximumHeight(90);
      row.order->setObjectName(QStringLiteral("order_") + key);
#ifdef Q_OS_MACOS
      auto* moves = new QHBoxLayout;
#else
      auto* moves = new QVBoxLayout;
#endif
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
    row.edit->setCursorPosition(0); // a long expression from its start
    if (input.type == InputDef::Type::Value) {
      row.edit->setToolTip(input.tooltip.isEmpty()
                               ? tr("A value or an expression, such as 20, 2 in or d1 * 2")
                               : input.tooltip);
    } else {
      row.edit->setToolTip(input.tooltip);
    }
    connect(row.edit, &QLineEdit::textEdited, this,
            [this, key](const QString& text) { emit textEdited(key, text); });
    if (input.type == InputDef::Type::Value && input.autoText) {
      // Empty: the model works it out (mitcad#656), shown once a preview has it.
      row.edit->setPlaceholderText(tr("Auto"));
    }
    if (input.type == InputDef::Type::Value && input.markFor) {
      // A mark in the field (mitcad#980: a Hole field that no longer follows
      // the screw size), shown by refreshRow; a click does its action.
      row.mark = new QAction(row.edit);
      row.mark->setObjectName(QStringLiteral("mark_") + key);
      row.mark->setVisible(false);
      row.edit->addAction(row.mark, QLineEdit::TrailingPosition);
      connect(row.mark, &QAction::triggered, this, [this, key] {
        auto call = CommandPresenter::actions().markClicked;
        if (call) {
          call(key);
        }
      });
    }
    if (input.type == InputDef::Type::Value) {
      connect(row.edit, &QLineEdit::editingFinished, this, [this, key] { emit valueCommitted(key); });
      row.edit->installEventFilter(this); // its keyboard focus (mitcad#250)
      // Parameter suggestions while typing, the fx browser, name = value and
      // Create User Parameter (mitcad#567, mitcad#568).
      ParameterFieldOptions suggest;
      suggest.kind = input.valueKind == ValueKind::Angle      ? ParameterKind::Angle
                     : input.valueKind == ValueKind::Unitless ? ParameterKind::Unitless
                                                              : ParameterKind::Length;
      suggest.own = scope.parameterName(input.id);
      suggest.label = QStringLiteral("%1 %2").arg(m_def.name, input.label);
      suggest.naming = true;
      ParameterField::attach(row.edit, suggest);
    }
    field = row.edit;
    break;
  }
  case InputDef::Type::Choice: {
    if (input.choiceTile && input.onButton) {
      // A tile: the chosen option's picture and name; a click on it does
      // what the button would (mitcad#427: BUILD's part picker).
      auto* tile = new TileButton([this, key] { emit buttonClicked(key); });
      tile->setToolTip(input.tooltip);
      row.tile = tile;
      field = tile;
      break;
    }
    if (!input.choiceColumns.isEmpty()) {
      // Rows under column headings, the chosen one selected (mitcad#656:
      // Gear Pair's candidates); refreshRow fills them.
      auto* table = new QTreeWidget;
      table->setColumnCount(static_cast<int>(input.choiceColumns.size()));
      table->setHeaderLabels(input.choiceColumns);
      table->setRootIsDecorated(false);
      table->setUniformRowHeights(true);
      table->setVerticalScrollMode(QAbstractItemView::ScrollPerItem); // a row per step: its logged place
      table->setSelectionMode(QAbstractItemView::SingleSelection);
      // The keyboard: a click gives it the list, the arrow keys step the
      // rows, each row chosen as it becomes the current one.
      table->setFocusPolicy(Qt::StrongFocus);
      table->setToolTip(input.tooltip);
      // Six rows and the headings in sight, floating or docked.
      const int rowHeight = table->fontMetrics().height() + 8;
      table->setMinimumHeight(table->header()->sizeHint().height() + 6 * rowHeight + 2 * table->frameWidth() + 4);
      // The columns share the panel's width: no scrolling sideways.
      table->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
      table->setTextElideMode(Qt::ElideRight);
      // Each column as wide as its cells (short ones, mitcad#656), the list
      // at least as wide as its columns: nothing clipped.
      table->setIndentation(0);
      table->header()->setStretchLastSection(false);
      table->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
      // The chosen row stands out also while the list has no keyboard.
      table->setStyleSheet(QStringLiteral("QTreeWidget::item:selected { background: palette(highlight);"
                                          " color: palette(highlighted-text); }"));
      connect(table, &QTreeWidget::currentItemChanged, this, [this, key](QTreeWidgetItem* item, QTreeWidgetItem*) {
        if (item != nullptr) {
          emit choiceChanged(key, item->data(0, Qt::UserRole).toString());
        }
      });
      row.table = table;
      field = table;
      break;
    }
    if (!input.choiceIcons.isEmpty() && !input.choicesFor) {
      // A row of icon buttons, the chosen one pressed (mitcad#394: BUILD's
      // X / Y / Z).
      auto* line = new QWidget;
      auto* box = new QHBoxLayout(line);
      box->setContentsMargins(0, 0, 0, 0);
      box->setSpacing(4);
      for (const auto& option : input.choices) {
        const QString value = option.first;
        const QString label = option.second;
        auto* button = new QToolButton;
        button->setIcon(themeIcon(input.choiceIcons.value(value)));
        button->setIconSize(QSize(20, 20));
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setFocusPolicy(Qt::NoFocus);
        button->setToolTip(input.tooltip.isEmpty() ? label : QStringLiteral("%1: %2").arg(label, input.tooltip));
        button->setObjectName(QStringLiteral("input_%1_%2").arg(key, value));
        connect(button, &QToolButton::clicked, this, [this, key, value] {
          emit choiceChanged(key, value);
        });
        row.choiceButtons.insert(value, button);
        box->addWidget(button);
      }
      box->addStretch();
      field = line;
      break;
    }
    row.combo = input.choiceIcon ? new MiddleElideComboBox : new QComboBox;
    // No keyboard focus: choosing leaves it in the value field, where the
    // typed value goes.
    row.combo->setFocusPolicy(Qt::NoFocus);
    row.combo->setToolTip(input.tooltip);
    if (input.choiceImage) {
      row.combo->setIconSize(QSize(48, 48)); // the options' pictures
    }
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
    if (input.onButton) {
      // A button beside the drop-down (mitcad#418: BUILD's part picker).
      auto* line = new QWidget;
      auto* box = new QHBoxLayout(line);
      box->setContentsMargins(0, 0, 0, 0);
      box->setSpacing(4);
      row.button = new QToolButton;
      if (input.buttonIcon.isEmpty()) {
        row.button->setText(input.buttonText);
      } else {
        // A small icon (mitcad#655): the text is its accessible name.
        row.button->setIcon(themeIcon(input.buttonIcon));
        row.button->setIconSize(QSize(16, 16));
        row.button->setAutoRaise(true);
        row.button->setAccessibleName(input.buttonText);
      }
      row.button->setToolTip(input.buttonTooltip);
      row.button->setFocusPolicy(Qt::NoFocus);
      row.button->setObjectName(QStringLiteral("button_") + key);
      connect(row.button, &QToolButton::clicked, this, [this, key] { emit buttonClicked(key); });
      box->addWidget(row.combo, 1);
      box->addWidget(row.button);
      field = line;
    }
    break;
  }
  case InputDef::Type::Check: {
    if (input.disclosure) {
      // A small button that opens the inputs that follow it ("More",
      // mitcad#394).
      auto* more = new QToolButton;
      more->setCheckable(true);
      more->setAutoRaise(true);
      more->setFocusPolicy(Qt::NoFocus);
      more->setToolButtonStyle(Qt::ToolButtonTextOnly);
      more->setText(QStringLiteral("▸ %1").arg(input.label));
      more->setToolTip(input.tooltip);
      connect(more, &QToolButton::clicked, this, [this, key](bool on) { emit checkChanged(key, on); });
      row.check = more;
      field = more;
      break;
    }
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
  case InputDef::Type::CutTargets: {
    row.targets = new CutTargetsPanel;
    row.targets->setScope(scope.object(input.id));
    connect(row.targets, &CutTargetsPanel::scopeChanged, this, [this, key](const QJsonObject& value) {
      auto call = CommandPresenter::actions().objectChanged;
      if (call) { call(key, value); }
    });
    connect(row.targets, &CutTargetsPanel::highlightChanged, this, [this](const Selection& items) {
      auto call = CommandPresenter::actions().highlightRequested;
      if (call) { call(items); }
    });
    field = row.targets;
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
      mac::auxiliaryButton(row.add);
      mac::styleIconToggle(row.add, QStringLiteral("plus"));
      row.add->setObjectName(QStringLiteral("add_") + key);
      connect(row.add, &QPushButton::clicked, this, [this, key] { emit rowAdded(key); });
      boxLayout->addWidget(row.add, 0, Qt::AlignLeft);
    }
    field = box;
    break;
  }
  case InputDef::Type::Note: {
    // A line of text and its buttons (mitcad#655): 3D Print's update bar
    // and import line.
    row.noteRow = new NoteRow;
    connect(row.noteRow, &NoteRow::actionClicked, this, [this, key](int index) { emit noteActionClicked(key, index); });
    field = row.noteRow;
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

  if (input.type == InputDef::Type::List || input.type == InputDef::Type::CutTargets ||
      (input.type == InputDef::Type::Choice && !input.choiceColumns.isEmpty() && form != nullptr)) {
    // A list's rows take the whole width, under its name (a choice's
    // columns too, mitcad#656: nothing clipped beside a label).
    auto* title = new QLabel(QStringLiteral("<b>%1</b>").arg(input.label.toHtmlEscaped()));
    title->setToolTip(input.tooltip);
    row.title = title;
    form->addRow(title);
    form->addRow(container);
  } else {
    if (!input.heading.isEmpty() && form != nullptr) {
      // The heading of a group of inputs, a row of its own above.
      auto* heading = new QLabel(QStringLiteral("<b>%1</b>").arg(input.heading.toHtmlEscaped()));
      row.title = heading;
      form->addRow(heading);
    }
    if (input.type == InputDef::Type::Note && input.label.isEmpty() && !input.fieldColumn && form != nullptr) {
      // A note row without a label takes the panel's whole width
      // (mitcad#655): its text is not squeezed beside the labels.
      form->addRow(container);
      updateNote(row);
      return &row;
    }
    // A disclosure button carries its label itself.
    row.label = new QLabel(input.disclosure ? QString() : input.label);
    row.label->setToolTip(input.tooltip);
    if (form != nullptr && input.fieldBelow && !input.disclosure) {
      // The field under its label, across the panel (mitcad#1010): shown
      // and hidden with it as a title row.
      row.title = row.label;
      form->addRow(row.label);
      form->addRow(container);
    } else if (form != nullptr) {
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
  // A row that moves (one above it grew: a tile's picture, a note) logs
  // the places again (mitcad#528; eventFilter). Once per field.
  for (const auto& row : m_rows) {
    if (row->field != nullptr && !row->field->property("mitcadPlaceWatched").toBool()) {
      row->field->setProperty("mitcadPlaceWatched", true);
      row->field->installEventFilter(this);
    }
  }
  // Rows a choice showed or hid move the others: where they are now, for
  // UI tests.
  QStringList shown;
  for (const Row* row : orderedRows()) {
    if (!row->field->isHidden()) {
      // A mark shown or gone (mitcad#980) is a place more or less.
      shown << (row->mark != nullptr && row->mark->isVisible() ? row->key + QLatin1Char('*') : row->key);
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
  // A note row with nothing to say is hidden (mitcad#655).
  const InputNote note =
      input.type == InputDef::Type::Note && input.noteFor ? input.noteFor(scope) : InputNote();
  const bool shown = isShown(input, scope) && (input.type != InputDef::Type::Note || !note.isEmpty());
  if (input.type == InputDef::Type::List || input.type == InputDef::Type::CutTargets) {
    row.form->setRowVisible(row.title, shown);
    row.form->setRowVisible(row.field, shown);
    if (row.targets) { row.targets->setScope(scope.object(input.id)); }
    return;
  }
  if (row.form != nullptr) {
    row.form->setRowVisible(row.field, shown);
    if (row.title != nullptr) {
      row.form->setRowVisible(row.title, shown);
    }
  } else {
    if (row.label != nullptr) { // none on a line beside another (besidePrevious)
      row.label->setVisible(shown);
    }
    row.field->setVisible(shown);
  }
  // The button beside a field enabled while InputDef::buttonEnabled says so
  // (mitcad#655).
  if (input.buttonEnabled && row.button != nullptr) {
    row.button->setEnabled(input.buttonEnabled(scope));
  }
  // Disabled rather than hidden while InputDef::enabled says no (mitcad#655).
  if (input.enabled && row.field != nullptr) {
    const bool on = input.enabled(scope);
    if (row.field->isEnabled() != on) {
      qDebug().noquote() << QStringLiteral("Panel %1 input %2 %3")
                                .arg(m_def.name, row.key, on ? QStringLiteral("enabled") : QStringLiteral("disabled"));
    }
    row.field->setEnabled(on);
    if (row.label != nullptr) {
      row.label->setEnabled(on);
    }
    if (row.title != nullptr) {
      row.title->setEnabled(on);
    }
  }
  const QString& id = input.id;
  // A label that follows other inputs (InputDef::labelFor, mitcad#545): a
  // pattern's length row says Spacing or Extent, with the parameter it
  // edits after it as below.
  if (input.labelFor && row.label != nullptr && !input.disclosure) {
    const bool valued = input.type == InputDef::Type::Value || input.type == InputDef::Type::Text;
    const QString name = valued ? scope.parameterName(id) : QString();
    const QString label = input.labelIn(scope);
    const QString text = name.isEmpty() ? label : QStringLiteral("%1 (%2)").arg(label, name);
    if (row.label->text() != text) {
      row.label->setText(text);
      qDebug().noquote() << QStringLiteral("Panel %1 label %2: %3").arg(m_def.name, row.key, text);
    }
  }
  switch (input.type) {
  case InputDef::Type::Selection: {
    const Selection& items = scope.items(id);
    // The picks by name where the input lists them (Combine's tools in
    // occurrences, mitcad#544), else their count.
    const QString listed = items.isEmpty() ? QString() : scope.itemsText(id);
    const QString tip = input.tooltip.isEmpty() ? tr("Click to pick %1").arg(input.label) : input.tooltip;
    // An empty row may stand for something (InputDef::emptyShows): its
    // text and swatch then.
    const auto empty = items.isEmpty() && input.emptyShows ? input.emptyShows(scope) : std::pair<QString, QColor>();
    const QColor swatch = empty.first.isEmpty() || !empty.second.isValid() ? colorOf(row.key) : empty.second;
    if (row.select->property("swatch").toString() != swatch.name()) {
      row.select->setProperty("swatch", swatch.name());
      row.select->setIcon(swatchIcon(swatch));
    }
    if (!empty.first.isEmpty()) {
      row.select->setText(empty.first);
      row.select->setToolTip(tip);
    } else if (listed.isEmpty()) {
      row.select->setText(countText(input, static_cast<int>(items.size())));
      row.select->setToolTip(tip);
    } else {
      row.select->setText(row.select->fontMetrics().elidedText(listed, Qt::ElideRight, 220));
      row.select->setToolTip(QStringLiteral("%1\n%2").arg(listed, tip));
    }
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
      if (!row.edit->hasFocus()) {
        // The start of a long expression in sight, not its end (mitcad#656:
        // "shaft_bore_clearance", not "aft_bore_clearance").
        row.edit->setCursorPosition(0);
      }
    }
    // A field that no longer follows what fills it (InputDef::markFor,
    // mitcad#980): the mark's icon in it, its tooltip and action.
    if (row.mark != nullptr) {
      const FieldMark mark = input.markFor(scope);
      if (mark != row.shownMark) {
        row.shownMark = mark;
        row.mark->setVisible(!mark.isEmpty());
        if (!mark.isEmpty()) {
          row.mark->setIcon(themeIcon(mark.icon));
          row.mark->setText(mark.action.isEmpty() ? mark.tooltip : mark.action);
          row.mark->setToolTip(mark.action.isEmpty() ? mark.tooltip
                                                     : tr("%1\nClick: %2").arg(mark.tooltip, mark.action));
        }
      }
    }
    // Its own parameter is not offered to it (mitcad#567).
    if (ParameterField* suggest = ParameterField::of(row.edit)) {
      suggest->setOwnParameter(scope.parameterName(id));
    }
    // The parameter the field edits, after its label: "Distance (d5)"
    // (mitcad#231).
    if (const QString name = scope.parameterName(id); row.label != nullptr && !name.isEmpty()) {
      const QString text = QStringLiteral("%1 (%2)").arg(input.labelIn(scope), name);
      if (row.label->text() != text) {
        row.label->setText(text);
        qDebug().noquote() << QStringLiteral("Panel %1 label: %2").arg(m_def.name, text);
      }
      // What the parameter is, as a sketch dimension's tooltip says
      // (mitcad#250): its name, expression, value and comment.
      const QString about = scope.parameterTooltip(id);
      const QString tip = (about.isEmpty() ? QStringLiteral("%1 = %2").arg(name, scope.text(id)) : about) +
                          QLatin1Char('\n') +
                          tr("Type name = value to rename it; a click on a dimension or a value in the view "
                             "inserts its name");
      if (row.edit->toolTip() != tip) {
        row.edit->setToolTip(tip);
        if (!about.isEmpty()) {
          qDebug().noquote() << QStringLiteral("Panel %1 tooltip %2: %3").arg(m_def.name, row.key, about);
        }
      }
    }
    break;
  case InputDef::Type::Choice: {
    if (row.tile != nullptr) {
      // A tile: the chosen option as the input says it is.
      const ChoiceTile tile = input.choiceTile(scope);
      if (static_cast<TileButton*>(row.tile)->setTile(tile)) {
        qDebug().noquote() << QStringLiteral("Panel %1 tile %2: %3%4")
                                  .arg(m_def.name, row.key, tile.title,
                                       tile.detail.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(tile.detail));
      }
      break;
    }
    if (row.table != nullptr) {
      // Rows of columns (InputDef::choiceColumns, mitcad#656): filled
      // again when the options change, the chosen one selected.
      const QSignalBlocker blockTable(row.table);
      if (input.choiceColumnsFor) {
        // Headings with a value in them (InputDef::choiceColumnsFor).
        const QStringList headings = input.choiceColumnsFor(scope);
        if (headings.size() == row.table->columnCount()) {
          for (int column = 0; column < headings.size(); ++column) {
            if (row.table->headerItem()->text(column) != headings[column]) {
              row.table->headerItem()->setText(column, headings[column]);
            }
          }
        }
      }
      const QVector<QPair<QString, QString>> options = input.choicesIn(scope);
      bool same = row.table->topLevelItemCount() == options.size();
      for (int i = 0; same && i < options.size(); ++i) {
        const QTreeWidgetItem* item = row.table->topLevelItem(i);
        same = item->data(0, Qt::UserRole).toString() == options[i].first &&
               item->data(0, Qt::UserRole + 1).toString() == options[i].second;
      }
      if (!same) {
        row.table->clear();
        QStringList labels;
        const QChar warningSign(0x26A0);
        std::vector<bool> used(static_cast<std::size_t>(row.table->columnCount()), false);
        for (const auto& [value, label] : options) {
          QStringList cells = label.split(QLatin1Char('\t'));
          auto* item = new QTreeWidgetItem;
          for (int column = 0; column < cells.size() && column < row.table->columnCount(); ++column) {
            QString cell = cells[column];
            if (cell.startsWith(warningSign)) {
              // A warning sign leading a cell is shown as the warning icon
              // (mitcad#656: an undercut gear's teeth).
              item->setIcon(column, style()->standardIcon(QStyle::SP_MessageBoxWarning));
              cell = cell.mid(1).trimmed();
            }
            item->setText(column, cell);
            if (!cells[column].isEmpty()) {
              used[static_cast<std::size_t>(column)] = true;
            }
          }
          item->setData(0, Qt::UserRole, value);
          item->setData(0, Qt::UserRole + 1, label);
          row.table->addTopLevelItem(item);
          labels << QString(label).replace(QLatin1Char('\t'), QStringLiteral(", "));
        }
        for (int column = 0; column < row.table->columnCount(); ++column) {
          // A column empty in every row is left out (a ratio met exactly).
          row.table->setColumnHidden(column, !options.isEmpty() && !used[static_cast<std::size_t>(column)]);
          row.table->resizeColumnToContents(column);
        }
        int width = 2 * row.table->frameWidth() + row.table->verticalScrollBar()->sizeHint().width();
        for (int column = 0; column < row.table->columnCount(); ++column) {
          if (!row.table->isColumnHidden(column)) {
            width += row.table->columnWidth(column);
          }
        }
        row.table->setMinimumWidth(width);
        qDebug().noquote() << QStringLiteral("Panel %1 options %2: %3")
                                  .arg(m_def.name, row.key, labels.join(QStringLiteral(" | ")));
        scheduleLogLayout(); // where its rows are now, for UI tests
      }
      for (int i = 0; i < row.table->topLevelItemCount(); ++i) {
        QTreeWidgetItem* item = row.table->topLevelItem(i);
        const bool chosen = item->data(0, Qt::UserRole).toString() == scope.choice(id);
        item->setSelected(chosen);
        if (chosen) {
          row.table->setCurrentItem(item);
        }
      }
      break;
    }
    if (row.combo == nullptr) {
      // Icon buttons: the chosen one pressed; those not offered hidden
      // (InputDef::choiceOffered, mitcad#524).
      for (auto it = row.choiceButtons.cbegin(); it != row.choiceButtons.cend(); ++it) {
        const QSignalBlocker blockButton(it.value());
        it.value()->setChecked(it.key() == scope.choice(id));
        it.value()->setVisible(!input.choiceOffered || input.choiceOffered(scope, it.key()));
      }
      break;
    }
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
        int pictures = 0;
        for (const auto& [value, label] : options) {
          if (isChoiceSeparator(value)) {
            // A line between the options (mitcad#828): disabled, so not
            // choosable and skipped by the keyboard; its value kept so the
            // next refresh finds the same options.
            row.combo->insertSeparator(row.combo->count());
            row.combo->setItemData(row.combo->count() - 1, value);
            continue;
          }
          // A picture beside each option that has one (InputDef::choiceImage).
          const QImage image = input.choiceImage ? input.choiceImage(value) : QImage();
          if (input.choiceIcon) {
            // A small icon and its tooltip (InputDef::choiceIcon, mitcad#655).
            const ChoiceIcon icon = input.choiceIcon(value);
            row.combo->addItem(icon.icon.isEmpty() ? QIcon() : themeIcon(icon.icon), label, value);
            row.combo->setItemData(row.combo->count() - 1, icon.tooltip, Qt::ToolTipRole);
          } else if (image.isNull()) {
            row.combo->addItem(label, value);
          } else {
            row.combo->addItem(QIcon(QPixmap::fromImage(image)), label, value);
            ++pictures;
          }
          if (value.startsWith(choiceHeadingPrefix())) {
            // A heading of the options after it (mitcad#655): small, bold,
            // not a choice.
            if (auto* model = qobject_cast<QStandardItemModel*>(row.combo->model())) {
              QStandardItem* heading = model->item(row.combo->count() - 1);
              heading->setFlags(heading->flags() & ~(Qt::ItemIsEnabled | Qt::ItemIsSelectable));
              QFont font = row.combo->font();
              font.setBold(true);
              if (font.pointSizeF() > 0.0) {
                font.setPointSizeF(font.pointSizeF() * 0.9);
              }
              heading->setFont(font);
            }
          }
        }
        QStringList labels;
        for (const auto& option : options) {
          labels << (isChoiceSeparator(option.first) ? QStringLiteral("---") : option.second);
        }
        qDebug().noquote() << QStringLiteral("Panel %1 options %2: %3")
                                  .arg(m_def.name, row.key, labels.join(QStringLiteral(" | ")));
        if (input.choiceImage) {
          qDebug().noquote() << QStringLiteral("Panel %1 option pictures %2: %3 of %4")
                                    .arg(m_def.name, row.key)
                                    .arg(pictures)
                                    .arg(options.size());
        }
      }
    }
    row.combo->setCurrentIndex(qMax(0, row.combo->findData(scope.choice(id))));
    break;
  }
  case InputDef::Type::Check:
  case InputDef::Type::Flip: {
    const QSignalBlocker block(row.check);
    row.check->setChecked(scope.checked(id));
    if (input.disclosure) {
      row.check->setText(QStringLiteral("%1 %2").arg(scope.checked(id) ? QStringLiteral("▾") : QStringLiteral("▸"),
                                                     input.label));
    }
    break;
  }
  case InputDef::Type::Note:
    if (row.noteRow->setNote(note.text, note.actions, note.icons, note.oneLine) && !note.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Panel %1 note %2: %3%4")
                                .arg(m_def.name, row.key, note.text,
                                     note.actions.isEmpty()
                                         ? QString()
                                         : QStringLiteral(" [%1]").arg(note.actions.join(QStringLiteral("] ["))));
      if (!m_shownRows.isEmpty()) {
        scheduleLogLayout(); // its buttons moved or came
      }
    }
    break;
  case InputDef::Type::List:
  case InputDef::Type::CutTargets:
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

void CommandPanel::setPlaceholder(const QString& key, const QString& text) {
  Row* row = m_byKey.value(key);
  if (row == nullptr || row->edit == nullptr || row->edit->placeholderText() == text) {
    return;
  }
  row->edit->setPlaceholderText(text);
  qDebug().noquote() << QStringLiteral("Panel %1 placeholder %2: %3").arg(m_def.name, key, text);
}

void CommandPanel::updateNote(Row& row) {
  const auto [errorText, hint] = m_notes.value(row.key);
  const bool error = !errorText.isEmpty();
  const QString text = error ? errorText : hint;
  // A long name in an error (an unknown parameter) breaks rather than
  // widening the panel (mitcad#198).
  const QString shown = breakableText(text);
  if (row.note->text() != shown) {
    row.note->setText(shown);
    if (!m_shownRows.isEmpty()) {
      scheduleLogFit();
    }
  }
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
  setErrorStyleSheet(m_message, error ? QStringLiteral("QTextEdit { color: %1; background: transparent; }")
                                      : QStringLiteral("QTextEdit { color: gray; background: transparent; }"));
  if (message == m_message->message()) {
    return;
  }
  m_message->setMessage(message);
  // What the panel says in red, for UI tests (mitcad#531: the model's
  // refusal of a preview shown on the panel).
  if (error && !message.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Panel %1 error: %2").arg(m_def.name, message);
  }
  scheduleLogFit();
}

void CommandPanel::scheduleLogFit() {
  if (m_fitTimer == nullptr) {
    m_fitTimer = new QTimer(this);
    m_fitTimer->setSingleShot(true);
    m_fitTimer->setInterval(50);
    connect(m_fitTimer, &QTimer::timeout, this, &CommandPanel::logFit);
    TestSync::watch(m_fitTimer);
  }
  m_fitTimer->start();
}

void CommandPanel::logFit() const {
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  // What the page shows of the panel: the scroll area's viewport.
  const QWidget* page = parentWidget() != nullptr ? parentWidget() : this;
  QString shown = QStringLiteral("hidden");
  if (!m_message->isHidden()) {
    shown = QStringLiteral("%1x%2").arg(m_message->width()).arg(m_message->height());
    const QScrollBar* bar = m_message->verticalScrollBar();
    if (bar != nullptr && bar->maximum() > 0) {
      shown += QStringLiteral(" scrolling");
    }
  }
  qDebug().noquote() << QStringLiteral("Panel %1 message %2, panel width %3 of %4")
                            .arg(m_def.name, shown)
                            .arg(width())
                            .arg(page->width());
  // A message that changed its height moved the rows: their new places.
  const int messageHeight = m_message->isHidden() ? 0 : m_message->height();
  if (messageHeight != m_loggedMessageHeight) {
    logLayout();
  }
}

void CommandPanel::setResult(const QString& text) {
  Inspection result;
  result.text = text;
  setInspection(result);
}

void CommandPanel::setInspection(const Inspection& result) {
  // The text all the same: the panel's state for live MCP (shownState).
  m_result->setText(result.text);
  const bool rows = !result.values.empty() || !result.groups.empty();
  // The text's box while there are no rows; a panel with only Close
  // (Measure, mitcad#1010) shows no empty box between its picks and its
  // actions.
  const bool box = static_cast<bool>(m_def.inspect) && !rows && (!m_def.closeOnly || !result.text.isEmpty());
  if (m_result->isVisibleTo(this) != box) {
    m_result->setVisible(box);
    scheduleLogLayout();
  }
  if (!rows && m_resultRows->isEmpty()) {
    return;
  }
  m_resultRows->set(result);
  // The rows' places, and the section's under them, again.
  scheduleLogLayout();
}

void CommandPanel::setOkEnabled(bool enabled) { m_ok->setEnabled(enabled || m_def.closeOnly); }

QWidget* CommandPanel::takeButtons() {
  QWidget* buttons = m_buttonBox;
  m_buttonBox = nullptr;
  return buttons;
}

void CommandPanel::setPreviewReport(const QJsonObject& report) {
  for (const auto& row : m_rows) {
    if (row->targets != nullptr) { row->targets->setPreview(report.value(QStringLiteral("cut_targets")).toObject()); }
  }
}

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
      focusField(row->edit);
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
  focusField(row->edit);
  return true;
}

void CommandPanel::logLayout() const {
  const QStringList lines = layoutLines();
  m_loggedMessageHeight = m_message->isHidden() ? 0 : m_message->height();
  m_loggedLines = lines;
  for (const QString& line : lines) {
    qDebug().noquote() << line;
  }
}

void CommandPanel::scheduleMovedCheck() {
  if (m_movedTimer == nullptr) {
    m_movedTimer = new QTimer(this);
    m_movedTimer->setSingleShot(true);
    m_movedTimer->setInterval(50);
    // Where the inputs are now, when it is not what was logged last
    // (mitcad#528): a place logged before the layout settled (a tile's
    // picture read, the dock or the window laid out again) would else stay
    // the last one, and a test would click where the input was.
    connect(m_movedTimer, &QTimer::timeout, this, [this] {
      if (!m_loggedLines.isEmpty() && isVisible() && layoutLines() != m_loggedLines) {
        logLayout();
      }
    });
    TestSync::watch(m_movedTimer);
  }
  m_movedTimer->start();
}

void CommandPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  // The panel and every widget it is in, up to the window: a move or a
  // resize of any of them moves the inputs (mitcad#528).
  for (QWidget* widget = this; widget != nullptr; widget = widget->parentWidget()) {
    widget->installEventFilter(this);
  }
  scheduleMovedCheck();
}

QStringList CommandPanel::layoutLines() const {
  QStringList lines;
  // Rows made or shown just now are placed first.
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const auto position = [](const QWidget* widget) {
    QRect area = widget->rect();
    if (const auto* edit = qobject_cast<const QLineEdit*>(widget)) {
      // The middle of where the text is typed: not on the field's trailing
      // buttons (the fx of a value, mitcad#656: a narrow distance field
      // beside its reference had its middle on it).
      // (A field's mark only while shown, mitcad#980.)
      const QList<QAction*> buttons = edit->actions();
      const int trailing =
          22 * static_cast<int>(std::count_if(buttons.begin(), buttons.end(), [](const QAction* button) {
            return button->isVisible();
          }));
      area.setWidth(std::max(8, area.width() - trailing));
    }
    const QPoint center = GlassCard::mapToHost(widget, area.center());
    return QStringLiteral("%1,%2").arg(center.x()).arg(center.y());
  };
  for (const Row* row : orderedRows()) {
    const QWidget* widget = row->select != nullptr  ? static_cast<QWidget*>(row->select)
                            : row->edit != nullptr  ? static_cast<QWidget*>(row->edit)
                            : row->combo != nullptr ? static_cast<QWidget*>(row->combo)
                            : row->table != nullptr ? static_cast<QWidget*>(row->table)
                            : row->check != nullptr ? static_cast<QWidget*>(row->check)
                            : row->add != nullptr   ? static_cast<QWidget*>(row->add)
                                                    : row->field;
    const bool hidden = !row->field->isVisibleTo(this);
    lines << QStringLiteral("Panel %1 input %2 at %3%4")
                              .arg(m_def.name, row->key, position(widget),
                                   hidden ? QStringLiteral(" (hidden)") : QString());
    if (!hidden) {
      // Each icon button of a choice (InputDef::choiceIcons).
      for (auto it = row->choiceButtons.cbegin(); it != row->choiceButtons.cend(); ++it) {
        if (!it.value()->isVisibleTo(this)) {
          continue; // not offered now (InputDef::choiceOffered)
        }
        lines << QStringLiteral("Panel %1 input %2=%3 at %4")
                                  .arg(m_def.name, row->key, it.key(), position(it.value()));
      }
      if (row->table != nullptr) {
        // Each row of a choice shown as rows of columns (mitcad#656), by
        // its value; the rows scrolled out of the list are not logged.
        // The list lays its rows out later by itself (a timer of the item
        // view): now, so that the rows have their places.
        // Before the list is shown its columns can still be 0 wide (an
        // empty item rect): a row's place is then the middle of the list's
        // width at the row's height (rows of one height, one per scroll
        // step).
        QTreeWidget* table = row->table;
        table->doItemsLayout();
        const QWidget* viewport = table->viewport();
        const int width = viewport->width() > 0 ? viewport->width() : table->width();
        const int height = viewport->height() > 0 ? viewport->height() : table->height() - table->header()->height();
        const int first = table->verticalScrollBar()->value();
        for (int i = 0; i < table->topLevelItemCount(); ++i) {
          const QTreeWidgetItem* item = table->topLevelItem(i);
          const QRect rect = table->visualItemRect(item);
          int y = rect.center().y();
          if (rect.height() <= 0) {
            const int step = std::max(table->sizeHintForRow(i), table->fontMetrics().height() + 4);
            y = (i - first) * step + step / 2;
          }
          if (y < 0 || y >= height) {
            continue;
          }
          const QPoint center = GlassCard::mapToHost(viewport, QPoint(width / 2, y));
          lines << QStringLiteral("Panel %1 input %2=%3 at %4,%5")
                       .arg(m_def.name, row->key, item->data(0, Qt::UserRole).toString())
                       .arg(center.x())
                       .arg(center.y());
        }
      }
      if (row->mark != nullptr && row->mark->isVisible()) {
        // A value field's mark (InputDef::markFor, mitcad#980): its button
        // in the field.
        for (const QToolButton* button : row->edit->findChildren<QToolButton*>()) {
          if (button->defaultAction() == row->mark && button->isVisibleTo(this)) {
            lines << QStringLiteral("Panel %1 mark %2 at %3").arg(m_def.name, row->key, position(button));
            break;
          }
        }
      }
      if (row->button != nullptr) {
        // The button beside a choice (InputDef::onButton).
        lines << QStringLiteral("Panel %1 button_%2 at %3")
                                  .arg(m_def.name, row->key, position(row->button));
      }
      if (row->noteRow != nullptr) {
        // A note row's buttons by their index (InputDef::onNoteAction).
        for (std::size_t i = 0; i < row->noteRow->buttons().size(); ++i) {
          lines << QStringLiteral("Panel %1 action %2.%3 at %4")
                       .arg(m_def.name, row->key)
                       .arg(i)
                       .arg(position(row->noteRow->buttons()[i]));
        }
      }
    }
    if (row->order != nullptr && !hidden) {
      for (const char* button : {"up_", "down_"}) {
        if (const auto* move = findChild<QToolButton*>(QString::fromLatin1(button) + row->key)) {
          lines << QStringLiteral("Panel %1 %2%3 at %4")
                                    .arg(m_def.name, QString::fromLatin1(button), row->key,
                                         position(move));
        }
      }
    }
    if (row->rowsLayout != nullptr && !hidden) {
      if (row->add != nullptr) {
        lines << QStringLiteral("Panel %1 add %2 at %3")
                                  .arg(m_def.name, row->key, position(row->add));
      }
      for (std::size_t i = 0; i < row->removes.size(); ++i) {
        lines << QStringLiteral("Panel %1 remove %2.%3 at %4")
                                  .arg(m_def.name, row->key)
                                  .arg(i)
                                  .arg(position(row->removes[i]));
      }
    }
  }
  // The answer's rows (mitcad#1010), then the sections' own places
  // (addSection, mitcad#648).
  lines << m_resultRows->places();
  for (const auto& places : m_sectionPlaces) {
    lines << places();
  }
  lines << QStringLiteral("Panel %1 %2 at %3")
               .arg(m_def.name, m_def.closeOnly ? QStringLiteral("Close") : QStringLiteral("OK"), position(m_ok));
  // Wider than the page would cut its right side off (mitcad#198).
  const QWidget* page = parentWidget() != nullptr ? parentWidget() : this;
  lines << QStringLiteral("Panel %1 width %2 of %3").arg(m_def.name).arg(width()).arg(page->width());
  // Whether an input's label or a choice's text is cut at the panel's
  // width (mitcad#1010): "Panel Measure inputs: none clipped", or "Panel
  // Measure inputs clipped: filter label (95 of 70)".
  QStringList clipped;
  for (const Row* row : orderedRows()) {
    if (!row->field->isVisibleTo(this)) {
      continue;
    }
    for (const QWidget* widget : {static_cast<const QWidget*>(row->label), static_cast<const QWidget*>(row->title)}) {
      const auto* label = qobject_cast<const QLabel*>(widget);
      if (label != nullptr && label->isVisibleTo(this) && !label->wordWrap() && !label->text().isEmpty() &&
          label->width() < label->sizeHint().width()) {
        clipped << QStringLiteral("%1 label (%2 of %3)").arg(row->key).arg(label->sizeHint().width()).arg(label->width());
        break;
      }
    }
    if (row->combo != nullptr && row->combo->isVisibleTo(this)) {
      // The current text in the combo's text area, its icon before it.
      QStyleOptionComboBox option;
      option.initFrom(row->combo);
      option.editable = row->combo->isEditable();
      option.frame = row->combo->hasFrame();
      option.currentText = row->combo->currentText();
      option.currentIcon = row->combo->itemIcon(row->combo->currentIndex());
      option.iconSize = row->combo->iconSize();
      const int room =
          row->combo->style()->subControlRect(QStyle::CC_ComboBox, &option, QStyle::SC_ComboBoxEditField, row->combo).width();
      int needed = row->combo->fontMetrics().horizontalAdvance(option.currentText);
      if (!option.currentIcon.isNull()) {
        needed += option.iconSize.width() + 4;
      }
      if (room < needed) {
        clipped << QStringLiteral("%1 choice (%2 of %3)").arg(row->key).arg(needed).arg(room);
      }
    }
  }
  lines << (clipped.isEmpty() ? QStringLiteral("Panel %1 inputs: none clipped").arg(m_def.name)
                              : QStringLiteral("Panel %1 inputs clipped: %2")
                                    .arg(m_def.name, clipped.join(QStringLiteral(", "))));
  // What a long panel's page shows of it: a place outside the view is
  // scrolled away, and a wheel over the bar scrolls it (UI tests in a low
  // window; MainWindow logs the places again when it scrolls).
  const auto* area = parentWidget() != nullptr
                         ? qobject_cast<const QAbstractScrollArea*>(parentWidget()->parentWidget())
                         : nullptr;
  if (area != nullptr && area->viewport() == parentWidget()) {
    const QPoint top = GlassCard::mapToHost(page, QPoint(0, 0));
    const QScrollBar* bar = area->verticalScrollBar();
    QString line = QStringLiteral("Panel %1 view y %2 to %3, scrolled %4 of %5")
                       .arg(m_def.name)
                       .arg(top.y())
                       .arg(top.y() + page->height())
                       .arg(bar->value())
                       .arg(bar->maximum());
    if (bar->isVisible()) {
      line += QStringLiteral(", bar at %1").arg(position(bar));
    }
    lines << line;
  }
  return lines;
}

bool CommandPanel::eventFilter(QObject* watched, QEvent* event) {
  switch (event->type()) {
  case QEvent::Move:
  case QEvent::Resize:
  case QEvent::Show:
  case QEvent::Hide:
    // The panel, a row's field or a widget the panel is in moved: its
    // inputs' places again once it settles, when they changed (mitcad#528).
    if (watched->isWidgetType()) {
      scheduleMovedCheck();
    }
    break;
  default:
    break;
  }
  // A click in a field counts as the user giving it the keyboard, also when
  // it has it already (the panel put it there as it opened, mitcad#587).
  const bool pressed = event->type() == QEvent::MouseButtonPress;
  if (event->type() == QEvent::FocusIn || event->type() == QEvent::FocusOut || pressed) {
    for (const auto& row : m_rows) {
      if (row->edit == watched && event->type() == QEvent::FocusOut) {
        // Left, a long expression shows from its start again (mitcad#656);
        // a name picked from the view still goes where the cursor was. Not
        // for the field's own context menu (mitcad#756).
        const int caret = leaveValueField(row->edit, *static_cast<QFocusEvent*>(event));
        if (caret >= 0) {
          row->leftAt = caret;
        }
      }
      if (row->edit == watched && row->input->type == InputDef::Type::Value) {
        auto call = CommandPresenter::actions().valueFocused;
        if (call && (!pressed || row->edit->hasFocus())) {
          call(row->key, event->type() != QEvent::FocusOut);
        }
        break;
      }
    }
  }
  return QWidget::eventFilter(watched, event);
}

bool CommandPanel::insertIntoValue(const QString& key, const QString& text) {
  Row* row = m_byKey.value(key);
  if (row == nullptr || row->edit == nullptr || !row->field->isVisibleTo(this)) {
    return false;
  }
  // Back where it was: the cursor stays where the view's click left it.
  const int at = row->edit->hasFocus() ? -1 : row->leftAt;
  row->edit->setFocus(Qt::OtherFocusReason);
  if (at >= 0) {
    row->edit->setCursorPosition(std::min(at, static_cast<int>(row->edit->text().size())));
  }
  row->edit->insert(text);
  const QString now = row->edit->text();
  qDebug().noquote() << QStringLiteral("Panel %1 input %2: inserted %3, now %4").arg(m_def.name, key, text, now);
  emit textEdited(key, now);
  return true;
}

} // namespace mitcad
