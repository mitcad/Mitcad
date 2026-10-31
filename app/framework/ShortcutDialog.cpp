// SPDX-License-Identifier: MIT
#include "ShortcutDialog.hpp"

#include <algorithm>

#include <QAction>
#include <QApplication>
#include <QComboBox>
#include <QDebug>
#include <QDialogButtonBox>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QPainter>
#include <QPixmap>
#include <QPushButton>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "CommandRegistry.hpp"

#include "Dialogs.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

// A row's contexts (ShortcutContext bits), on its first column: a
// command's row all its contexts, a context's row its own.
constexpr int kContextsRole = Qt::UserRole + 1;
constexpr int kCommandColumn = 0;
constexpr int kContextColumn = 1;
constexpr int kShortcutColumn = 2;
// A reserved key's read-only row (mitcad#730): true, its keys in portable
// text for the log, and what a try to change it shows ("Ctrl+F is
// reserved for Search Timeline.").
constexpr int kReservedRole = Qt::UserRole + 2;
constexpr int kReservedKeysRole = Qt::UserRole + 3;
constexpr int kReservedMessageRole = Qt::UserRole + 4;

bool sameEverywhere(const ContextShortcuts& keys) {
  const QList<QKeySequence> values = keys.values();
  return std::all_of(values.cbegin(), values.cend(),
                     [&values](const QKeySequence& key) { return key == values.front(); });
}

QString keyText(const QKeySequence& key) { return key.toString(QKeySequence::NativeText); }

// "All" for every context, else the names: "Model, Sketch" for a command
// of two (Extrude), "Sketch" for one.
QString contextText(unsigned contexts) {
  return contexts == AllShortcutContexts ? ShortcutDialog::tr("All") : shortcutContextName(contexts);
}

// The keys of the contexts in `contexts`.
ContextShortcuts keysIn(const ContextShortcuts& keys, unsigned contexts) {
  ContextShortcuts own;
  for (auto it = keys.cbegin(); it != keys.cend(); ++it) {
    if ((it.key() & contexts) != 0) own.insert(it.key(), it.value());
  }
  return own;
}

// A selected row's command icon in the selected text's colour, as the
// Projects panel's status marks: the monochrome drawings would nearly
// vanish on the selection's colour.
class ShortcutIconDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem cell = option;
    initStyleOption(&cell, index);
    if ((cell.state & QStyle::State_Selected) && !cell.icon.isNull()) {
      QPixmap mark = cell.icon.pixmap(cell.decorationSize, painter->device()->devicePixelRatioF());
      QPainter tint(&mark);
      tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
      tint.fillRect(mark.rect(), cell.palette.color(QPalette::HighlightedText));
      tint.end();
      cell.icon = QIcon();
      cell.icon.addPixmap(mark, QIcon::Normal);
      cell.icon.addPixmap(mark, QIcon::Selected);
    }
    const QWidget* widget = cell.widget;
    const QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, widget);
  }
};

} // namespace

ShortcutDialog::ShortcutDialog(CommandRegistry& registry, QWidget* parent)
    : QDialog(parent), m_registry(registry) {
  setWindowTitle(tr("Keyboard Shortcuts"));
  auto* layout = new QVBoxLayout(this);
  if (!registry.shortcutWarnings().isEmpty()) {
    auto* warning = new QLabel(tr("Some saved shortcuts conflict and are disabled. The commands remain in menus. "
                                  "Change these bindings or restore defaults:\n%1")
                                  .arg(registry.shortcutWarnings().join(QLatin1Char('\n'))));
    warning->setWordWrap(true);
    layout->addWidget(warning);
  }
  auto* filters = new QHBoxLayout;
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("shortcutSearch"));
  m_search->setPlaceholderText(tr("Search commands or shortcuts"));
  m_search->setClearButtonEnabled(true);
  mac::styleSearchField(m_search);
  // Down goes from the search to the first row found.
  m_search->installEventFilter(this);
  filters->addWidget(m_search, 1);
  // The rows of one context: the keys that work there (mitcad#720).
  m_contextFilter = new QComboBox;
  m_contextFilter->setObjectName(QStringLiteral("shortcutContextFilter"));
  m_contextFilter->addItem(tr("All Contexts"), unsigned(AllShortcutContexts));
  for (const unsigned context : shortcutContextList(AllShortcutContexts)) {
    m_contextFilter->addItem(shortcutContextName(context), context);
  }
  filters->addWidget(m_contextFilter);
  layout->addLayout(filters);

  m_tree = new QTreeWidget;
  m_tree->setObjectName(QStringLiteral("shortcutTree"));
  m_tree->setColumnCount(3);
  m_tree->setHeaderLabels({tr("Command"), tr("Context"), tr("Shortcut")});
  m_tree->setUniformRowHeights(true);
  m_tree->setAllColumnsShowFocus(true);
  m_tree->setAlternatingRowColors(true);
  m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
  m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_tree->setItemDelegateForColumn(kCommandColumn, new ShortcutIconDelegate(m_tree));
  // The Context column as wide as the longest text it can show.
  const QFontMetrics metrics(m_tree->font());
  int contextWidth = metrics.horizontalAdvance(tr("Context"));
  for (unsigned contexts = 1; contexts <= AllShortcutContexts; ++contexts) {
    contextWidth = std::max(contextWidth, metrics.horizontalAdvance(contextText(contexts)));
  }
  const int padding = 4 * metrics.averageCharWidth();
  QHeaderView* header = m_tree->header();
  header->setStretchLastSection(false);
  header->setSectionResizeMode(kCommandColumn, QHeaderView::Stretch);
  header->setSectionResizeMode(kContextColumn, QHeaderView::Fixed);
  header->setSectionResizeMode(kShortcutColumn, QHeaderView::Fixed);
  header->resizeSection(kContextColumn, contextWidth + padding);
  header->resizeSection(kShortcutColumn,
                        metrics.horizontalAdvance(QKeySequence(Qt::CTRL | Qt::ALT | Qt::SHIFT | Qt::Key_F12)
                                                      .toString(QKeySequence::NativeText)) +
                            2 * padding);
  // Return or F2 records a row's key; Delete clears it.
  m_tree->installEventFilter(this);
  m_tree->setContextMenuPolicy(Qt::ActionsContextMenu);
  auto* clearKey = new QAction(tr("Clear Shortcut"), m_tree);
  clearKey->setShortcuts({QKeySequence(Qt::Key_Delete), QKeySequence(Qt::Key_Backspace)});
  clearKey->setShortcutContext(Qt::WidgetShortcut);
  connect(clearKey, &QAction::triggered, this, [this] {
    if (QTreeWidgetItem* item = m_tree->currentItem()) setKey(item, QKeySequence());
  });
  m_tree->addAction(clearKey);
  auto* defaultKey = new QAction(tr("Reset to Default"), m_tree);
  connect(defaultKey, &QAction::triggered, this, [this] {
    if (QTreeWidgetItem* item = m_tree->currentItem()) setKey(item, m_registry.defaultShortcut(idOf(item)));
  });
  m_tree->addAction(defaultKey);
  layout->addWidget(m_tree);
  fill(false);

  connect(m_search, &QLineEdit::textChanged, this, &ShortcutDialog::filter);
  connect(m_contextFilter, &QComboBox::currentIndexChanged, this, &ShortcutDialog::contextFilterChanged);
  connect(m_tree, &QTreeWidget::currentItemChanged, this, [this, clearKey, defaultKey](QTreeWidgetItem* current) {
    // A reserved key's row is read-only.
    clearKey->setEnabled(!isReserved(current));
    defaultKey->setEnabled(!isReserved(current));
    rememberSelection(current);
    logRow(current);
  });
  connect(m_tree, &QTreeWidget::itemExpanded, this, [](QTreeWidgetItem* item) {
    qDebug().noquote() << QStringLiteral("Keyboard Shortcuts expanded: %1").arg(item->text(kCommandColumn));
  });
  connect(m_tree, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item) { record(item); });
  connect(m_tree, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int column) {
    if (column == kShortcutColumn) record(item);
  });
  // A click elsewhere ends a recording without a key.
  connect(qApp, &QApplication::focusChanged, this, [this](QWidget* old, QWidget* now) {
    if (!m_recorder.isNull() && inRecorder(old) && now != nullptr && !inRecorder(now)) {
      finishRecording(QKeySequence(), false, false);
    }
  });

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel |
                                       QDialogButtonBox::RestoreDefaults);
  connect(buttons, &QDialogButtonBox::accepted, this, &ShortcutDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &ShortcutDialog::reject);
  connect(buttons->button(QDialogButtonBox::RestoreDefaults), &QPushButton::clicked, this,
          [this] { fill(true); });
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons, nullptr, true);
  mac::styleDialog(this);
  m_search->setFocus();
  resize(600, 520);
  qDebug() << "Keyboard Shortcuts opened";
}

void ShortcutDialog::showCommand(const QString& id) {
  const CommandDef* def = m_registry.find(id);
  if (def == nullptr) {
    qDebug().noquote() << QStringLiteral("Keyboard Shortcuts at: no command %1").arg(id);
    return;
  }
  m_search->clear();
  QTreeWidgetItem* item = m_items.value(def->id);
  if (item == nullptr && m_contextFilter->currentIndex() != 0) {
    // Not a command of the context shown: all of them.
    m_contextFilter->setCurrentIndex(0);
    item = m_items.value(def->id);
  }
  if (item == nullptr) {
    qDebug().noquote() << QStringLiteral("Keyboard Shortcuts at: %1 is not listed").arg(def->name);
    return;
  }
  m_tree->setCurrentItem(item);
  m_tree->scrollToItem(item);
  m_tree->setFocus();
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts at %1").arg(def->name);
}

void ShortcutDialog::fill(bool defaults) {
  m_keys.clear();
  for (const auto& def : m_registry.commands()) {
    ContextShortcuts keys = m_registry.shortcuts(def->id);
    if (defaults) {
      for (QKeySequence& key : keys) key = def->shortcut;
    }
    if (!keys.isEmpty()) m_keys.insert(def->id, keys);
  }
  build();
}

void ShortcutDialog::build() {
  finishRecording(QKeySequence(), false, false);
  // One context: a row per command of it, that context's key alone (a key
  // given there changes no other context). All of them: a command of
  // several contexts expands into a row per context, collapsed unless its
  // keys differ.
  const unsigned wanted = m_contextFilter->currentData().toUInt();
  const bool single = wanted != AllShortcutContexts;
  m_tree->clear();
  m_items.clear();
  // With one context, every row would repeat its name (mitcad#730).
  m_tree->setColumnHidden(kContextColumn, single);
  for (const auto& def : m_registry.commands()) {
    const auto found = m_keys.constFind(def->id);
    if (found == m_keys.cend()) continue;
    const ContextShortcuts& keys = found.value();
    unsigned contexts = 0;
    for (auto it = keys.cbegin(); it != keys.cend(); ++it) contexts |= it.key();
    if ((contexts & wanted) == 0) continue;
    const unsigned shown = single ? wanted : contexts;
    QAction* action = m_registry.action(def->id);
    auto* top = new QTreeWidgetItem(m_tree);
    top->setIcon(kCommandColumn, action != nullptr ? action->icon() : QIcon());
    top->setText(kCommandColumn, def->name);
    top->setData(kCommandColumn, Qt::UserRole, def->id);
    top->setData(kCommandColumn, kContextsRole, shown);
    top->setText(kContextColumn, contextText(shown));
    if (!single && keys.size() > 1) {
      for (auto it = keys.cbegin(); it != keys.cend(); ++it) {
        auto* child = new QTreeWidgetItem(top);
        child->setData(kCommandColumn, Qt::UserRole, def->id);
        child->setData(kCommandColumn, kContextsRole, it.key());
        child->setText(kContextColumn, contextText(it.key()));
      }
      top->setExpanded(!sameEverywhere(keys));
    }
    m_items.insert(def->id, top);
    showKeys(def->id);
  }
  addReservedRows(wanted);
  filter();
  restoreSelection();
}

void ShortcutDialog::addReservedRows(unsigned wanted) {
  // The keys no command here can take (Ctrl+F for Search Timeline, Enter
  // for OK, …), muted, marked and read-only: under the command of the same
  // name when it is listed (Search Timeline), else after the commands.
  const bool single = wanted != AllShortcutContexts;
  const QBrush muted = palette().brush(QPalette::Disabled, QPalette::Text);
  for (const ReservedShortcut& reserved : reservedShortcuts()) {
    if ((reserved.contexts & wanted) == 0) continue;
    const unsigned shown = single ? wanted : reserved.contexts;
    QStringList shownKeys;
    QStringList portable;
    for (const QKeySequence& key : reserved.keys) {
      shownKeys.append(keyText(key));
      portable.append(key.toString(QKeySequence::PortableText));
    }
    const QString keys = shownKeys.join(QStringLiteral(", "));
    QTreeWidgetItem* command = m_items.value(reserved.id);
    QTreeWidgetItem* row = nullptr;
    if (command != nullptr) {
      row = new QTreeWidgetItem(command);
      command->setExpanded(true);
    } else {
      QAction* action = m_registry.action(reserved.id);
      row = new QTreeWidgetItem(m_tree);
      row->setIcon(kCommandColumn, action != nullptr ? action->icon() : QIcon());
      row->setText(kCommandColumn, reserved.name);
    }
    row->setData(kCommandColumn, Qt::UserRole, reserved.id);
    row->setData(kCommandColumn, kContextsRole, shown);
    row->setData(kCommandColumn, kReservedRole, true);
    row->setData(kCommandColumn, kReservedKeysRole, portable.join(QStringLiteral(", ")));
    row->setData(kCommandColumn, kReservedMessageRole, tr("%1 is reserved for %2.").arg(keys, reserved.name));
    row->setText(kContextColumn, contextText(shown));
    // The mark stays readable on a selected row, unlike the muted colour.
    row->setText(kShortcutColumn, tr("%1 (reserved)").arg(keys));
    const QString tip = reserved.browserFocus ? tr("Works while the Browser or the timeline has the keyboard focus")
                                              : tr("Reserved; this key cannot be changed");
    for (int column = 0; column < m_tree->columnCount(); ++column) {
      row->setForeground(column, muted);
      row->setToolTip(column, tip);
    }
  }
}

void ShortcutDialog::rememberSelection(const QTreeWidgetItem* item) {
  // The rows made anew (filter changes, Restore Defaults) select nothing:
  // the row the user selected last is remembered over them, and not
  // replaced by the row restoreSelection() picks for another filter (All,
  // Model, All again comes back to the Sketch row chosen first).
  if (item == nullptr || m_restoring) return;
  m_selectedId = idOf(item);
  m_selectedContexts = contextOf(item);
  m_selectedReserved = isReserved(item);
}

void ShortcutDialog::restoreSelection() {
  if (m_selectedId.isEmpty()) return;
  QTreeWidgetItem* kept = nullptr;
  if (m_selectedReserved) {
    for (QTreeWidgetItemIterator it(m_tree); *it != nullptr && kept == nullptr; ++it) {
      if (isReserved(*it) && idOf(*it) == m_selectedId) kept = *it;
    }
  } else if (QTreeWidgetItem* top = m_items.value(m_selectedId)) {
    kept = top;
    // A context's row (or one context's flat row) chosen, all contexts
    // shown: that context's own row, the command's rows opened.
    for (int i = 0; i < top->childCount(); ++i) {
      QTreeWidgetItem* child = top->child(i);
      if (!isReserved(child) && contextOf(child) == m_selectedContexts && m_selectedContexts != contextOf(top)) {
        top->setExpanded(true);
        kept = child;
      }
    }
  }
  if (kept == nullptr || kept->isHidden() || (kept->parent() != nullptr && kept->parent()->isHidden())) return;
  m_restoring = true;
  m_tree->setCurrentItem(kept);
  m_restoring = false;
  m_tree->scrollToItem(kept);
  const QTreeWidgetItem* command = kept->parent() != nullptr ? kept->parent() : kept;
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts selection kept: %1 | %2")
                            .arg(command->text(kCommandColumn), kept->text(kContextColumn));
}

bool ShortcutDialog::isReserved(const QTreeWidgetItem* item) const {
  return item != nullptr && item->data(kCommandColumn, kReservedRole).toBool();
}

ContextShortcuts ShortcutDialog::keysOf(const QTreeWidgetItem* item) const {
  return keysIn(m_keys.value(idOf(item)), contextOf(item));
}

void ShortcutDialog::showKeys(const QString& id) {
  QTreeWidgetItem* top = m_items.value(id);
  if (top == nullptr) return;
  const auto show = [this](QTreeWidgetItem* row) {
    const ContextShortcuts keys = keysOf(row);
    const bool same = sameEverywhere(keys);
    QFont font = row->font(kShortcutColumn);
    font.setItalic(!same);
    row->setFont(kShortcutColumn, font);
    row->setText(kShortcutColumn, keys.isEmpty() ? QString()
                                  : same         ? keyText(keys.first())
                                                 : tr("Varies"));
  };
  show(top);
  for (int i = 0; i < top->childCount(); ++i) {
    if (!isReserved(top->child(i))) show(top->child(i));
  }
}

void ShortcutDialog::contextFilterChanged() {
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts context filter: %1").arg(m_contextFilter->currentText());
  build();
  QStringList columns;
  for (int column = 0; column < m_tree->columnCount(); ++column) {
    if (!m_tree->isColumnHidden(column)) columns.append(m_tree->headerItem()->text(column));
  }
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts columns: %1").arg(columns.join(QStringLiteral(", ")));
}

void ShortcutDialog::filter() {
  const QString needle = m_search->text().trimmed();
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    QTreeWidgetItem* top = m_tree->topLevelItem(i);
    bool match = needle.isEmpty() || top->text(kCommandColumn).contains(needle, Qt::CaseInsensitive);
    if (isReserved(top)) {
      // Its keys are its own, not those of a command of the same id
      // (Search Timeline).
      top->setHidden(!match && !top->text(kShortcutColumn).contains(needle, Qt::CaseInsensitive));
      continue;
    }
    const ContextShortcuts keys = keysOf(top);
    for (auto it = keys.cbegin(); !match && it != keys.cend(); ++it) {
      match = keyText(it.value()).contains(needle, Qt::CaseInsensitive);
    }
    // A reserved key under the command: Ctrl+F finds Search Timeline.
    for (int c = 0; !match && c < top->childCount(); ++c) {
      const QTreeWidgetItem* child = top->child(c);
      match = isReserved(child) && child->text(kShortcutColumn).contains(needle, Qt::CaseInsensitive);
    }
    top->setHidden(!match);
  }
}

void ShortcutDialog::setKey(QTreeWidgetItem* item, const QKeySequence& key) {
  if (item == nullptr || isReserved(item)) return;
  const QString id = idOf(item);
  if (!m_keys.contains(id)) return;
  // The row's contexts only: a command's row all of them, a context's row
  // (or the one row of a single context shown) its own; the command's keys
  // may then differ, or match again.
  ContextShortcuts& keys = m_keys[id];
  const unsigned contexts = contextOf(item);
  for (auto it = keys.begin(); it != keys.end(); ++it) {
    if ((it.key() & contexts) != 0) it.value() = key;
  }
  m_edited.removeAll(id);
  m_edited.append(id);
  showKeys(id);
  QTreeWidgetItem* top = m_items.value(id);
  if (top != nullptr && top->childCount() > 0 && !sameEverywhere(keys)) top->setExpanded(true);
  const QString name = top != nullptr ? top->text(kCommandColumn) : id;
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts key: %1 | %2 = %3")
                            .arg(name, item->text(kContextColumn), loggedKey(item));
  if (item != top && top != nullptr) logRow(top);
  QStringList all;
  for (auto it = keys.cbegin(); it != keys.cend(); ++it) {
    all.append(QStringLiteral("%1 %2").arg(shortcutContextName(it.key()),
                                           it.value().isEmpty() ? QStringLiteral("(none)")
                                                                : it.value().toString(QKeySequence::PortableText)));
  }
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts keys of %1: %2").arg(name, all.join(QStringLiteral(", ")));
  filter();
}

void ShortcutDialog::record(QTreeWidgetItem* item) {
  if (item == nullptr || item == m_recording) return;
  if (isReserved(item)) {
    // Enter, F2, a double-click or a click on the key: say why nothing
    // happens, at the row.
    const QString message = item->data(kCommandColumn, kReservedMessageRole).toString();
    const QRect cell = m_tree->visualItemRect(item);
    QToolTip::showText(m_tree->viewport()->mapToGlobal(cell.bottomLeft()), message, m_tree->viewport(), cell);
    const QTreeWidgetItem* command = item->parent() != nullptr ? item->parent() : item;
    qDebug().noquote() << QStringLiteral("Keyboard Shortcuts reserved: %1 | %2")
                              .arg(command->text(kCommandColumn), message);
    return;
  }
  finishRecording(QKeySequence(), false, false);
  m_tree->setCurrentItem(item);
  m_recording = item;
  auto* recorder = new QKeySequenceEdit;
  recorder->setObjectName(QStringLiteral("shortcutRecorder"));
  recorder->setMaximumSequenceLength(1);
  m_recorder = recorder;
  m_tree->setItemWidget(item, kShortcutColumn, recorder);
  // Esc and Return end the recording without a key: they are the window's.
  recorder->installEventFilter(this);
  for (QWidget* child : recorder->findChildren<QWidget*>()) child->installEventFilter(this);
  connect(recorder, &QKeySequenceEdit::editingFinished, this, [this] {
    if (!m_recorder.isNull()) finishRecording(m_recorder->keySequence(), true, true);
  });
  recorder->setFocus(Qt::OtherFocusReason);
  const QTreeWidgetItem* top = m_items.value(idOf(item));
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts recording: %1 | %2")
                            .arg(top != nullptr ? top->text(kCommandColumn) : idOf(item), item->text(kContextColumn));
}

void ShortcutDialog::finishRecording(const QKeySequence& key, bool keep, bool refocus) {
  if (m_recorder.isNull()) {
    m_recording = nullptr;
    return;
  }
  QTreeWidgetItem* item = m_recording;
  m_recorder = nullptr;
  m_recording = nullptr;
  m_tree->removeItemWidget(item, kShortcutColumn); // deleted later
  const bool windowKey = key == QKeySequence(Qt::Key_Escape) || key == QKeySequence(Qt::Key_Return) ||
                         key == QKeySequence(Qt::Key_Enter);
  if (keep && !key.isEmpty() && !windowKey) {
    setKey(item, key);
  } else {
    qDebug() << "Keyboard Shortcuts recording cancelled";
  }
  if (refocus) {
    m_tree->setCurrentItem(item);
    m_tree->setFocus(Qt::OtherFocusReason);
  }
}

bool ShortcutDialog::inRecorder(QObject* object) const {
  const auto* widget = qobject_cast<QWidget*>(object);
  return widget != nullptr && !m_recorder.isNull() && (widget == m_recorder || m_recorder->isAncestorOf(widget));
}

bool ShortcutDialog::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<QKeyEvent*>(event);
    const bool plain = (key->modifiers() & ~Qt::KeypadModifier) == Qt::NoModifier;
    const bool enter = key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter;
    if (watched == m_search && key->key() == Qt::Key_Down && plain) {
      for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
        if (!m_tree->topLevelItem(i)->isHidden()) {
          m_tree->setCurrentItem(m_tree->topLevelItem(i));
          m_tree->setFocus(Qt::OtherFocusReason);
          return true;
        }
      }
    } else if (watched == m_tree && m_recorder.isNull() && plain && (enter || key->key() == Qt::Key_F2)) {
      record(m_tree->currentItem());
      return true;
    } else if (inRecorder(watched) && (enter || key->key() == Qt::Key_Escape)) {
      finishRecording(QKeySequence(), false, true);
      return true;
    }
  }
  return QDialog::eventFilter(watched, event);
}

QTreeWidgetItem* ShortcutDialog::itemFor(const QString& id, unsigned context) const {
  QTreeWidgetItem* top = m_items.value(id);
  if (top == nullptr) return nullptr;
  for (int i = 0; i < top->childCount(); ++i) {
    if (!isReserved(top->child(i)) && (contextOf(top->child(i)) & context) != 0) return top->child(i);
  }
  return (contextOf(top) & context) != 0 ? top : nullptr;
}

QString ShortcutDialog::idOf(const QTreeWidgetItem* item) const {
  return item != nullptr ? item->data(kCommandColumn, Qt::UserRole).toString() : QString();
}

unsigned ShortcutDialog::contextOf(const QTreeWidgetItem* item) const {
  return item != nullptr ? item->data(kCommandColumn, kContextsRole).toUInt() : 0u;
}

QString ShortcutDialog::loggedKey(const QTreeWidgetItem* item) const {
  if (isReserved(item)) {
    return QStringLiteral("%1 (reserved)").arg(item->data(kCommandColumn, kReservedKeysRole).toString());
  }
  const ContextShortcuts keys = keysOf(item);
  if (item == nullptr || keys.isEmpty()) return QString();
  if (!sameEverywhere(keys)) return QStringLiteral("Varies");
  const QKeySequence key = keys.first();
  return key.isEmpty() ? QStringLiteral("(none)") : key.toString(QKeySequence::PortableText);
}

void ShortcutDialog::logRow(const QTreeWidgetItem* item) const {
  if (item == nullptr) return;
  const QTreeWidgetItem* top = item->parent() != nullptr ? item->parent() : item;
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts row: %1 | %2 | %3")
                            .arg(top->text(kCommandColumn), item->text(kContextColumn), loggedKey(item));
}

void ShortcutDialog::showConflict(const ShortcutConflict& conflict) {
  // The row to change: the command changed here last, else the second.
  QString id;
  for (const ShortcutBinding* binding : {&conflict.first, &conflict.second}) {
    if (binding->fixed || !m_keys.contains(binding->id)) continue;
    if (id.isEmpty() || m_edited.lastIndexOf(binding->id) >= m_edited.lastIndexOf(id)) id = binding->id;
  }
  if (id.isEmpty()) return;
  unsigned contexts = 0;
  const ContextShortcuts keys = m_keys.value(id);
  for (auto it = keys.cbegin(); it != keys.cend(); ++it) contexts |= it.key();
  const QList<unsigned> meet = shortcutContextList(conflict.contexts & contexts);
  const unsigned context = meet.isEmpty() ? contexts : meet.front();
  QTreeWidgetItem* item = itemFor(id, context);
  if (item == nullptr || item->isHidden() || (item->parent() != nullptr && item->parent()->isHidden())) {
    // Not shown under the filters: all of them shown again (the rows are
    // made again for all contexts).
    m_search->clear();
    m_contextFilter->setCurrentIndex(0);
    item = itemFor(id, context);
  }
  if (item == nullptr) return;
  if (item->parent() != nullptr) item->parent()->setExpanded(true);
  m_tree->setCurrentItem(item);
  m_tree->scrollToItem(item);
  m_tree->setFocus(Qt::OtherFocusReason);
  qDebug().noquote() << QStringLiteral("Keyboard Shortcuts conflict at: %1 | %2")
                            .arg(m_items.value(id)->text(kCommandColumn), item->text(kContextColumn));
}

void ShortcutDialog::accept() {
  finishRecording(QKeySequence(), false, false);
  const auto conflicts = m_registry.shortcutConflicts(m_keys);
  if (!conflicts.isEmpty()) {
    const QString message = conflicts.front().description();
    showConflict(conflicts.front());
    qDebug().noquote() << QStringLiteral("Keyboard Shortcuts conflict: %1").arg(message);
    sheetWarning(this, windowTitle(), message);
    return;
  }
  m_registry.setShortcuts(m_keys);
  qDebug() << "Keyboard Shortcuts saved";
  QDialog::accept();
}

} // namespace mitcad
