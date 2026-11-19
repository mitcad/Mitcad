// SPDX-License-Identifier: MIT
#include "CommandSearch.hpp"

#include <utility>
#include <vector>

#include <QAction>
#include <QFont>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMenu>
#include <QPalette>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "CommandLayout.hpp"
#include "CommandRegistry.hpp"
#include "CommandSearchRank.hpp"
#include "Language.hpp"
#include "PanelTree.hpp"
#include "QuickAccess.hpp"
#include "Ribbon.hpp"
#include "../platform/MacCommandSearch.hpp"

namespace mitcad {

CommandSearch::CommandSearch(CommandRegistry& registry, QWidget* parent)
    : QFrame(parent, Qt::Popup), m_registry(registry) {
  setObjectName(QStringLiteral("commandSearch"));
  setAccessibleName(tr("Command Search"));
  setFrameShape(QFrame::StyledPanel);
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(6, 6, 6, 6);
  m_edit = new QLineEdit;
  m_edit->setObjectName(QStringLiteral("commandSearchInput"));
  m_edit->setAccessibleName(tr("Command Search"));
  m_edit->setPlaceholderText(tr("Search commands"));
  m_edit->setClearButtonEnabled(true);
  m_edit->installEventFilter(this);
  m_list = new QTreeWidget;
  panel::setCompactIndentation(m_list, panel::Indent::Flat);
  m_list->setObjectName(QStringLiteral("commandSearchResults"));
  m_list->setAccessibleName(tr("Commands"));
  // The command, its place (mitcad#1983) and its shortcut.
  m_list->setColumnCount(3);
  m_list->setHeaderHidden(true);
  m_list->setRootIsDecorated(false);
  m_list->setFocusPolicy(Qt::NoFocus);
  m_list->header()->setStretchLastSection(false);
  m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_list->header()->setSectionResizeMode(2, QHeaderView::ResizeToContents);
  layout->addWidget(m_edit);
  layout->addWidget(m_list);
  resize(520, 300);
  mac::styleCommandSearch(this, m_edit, m_list, layout);
  connect(m_edit, &QLineEdit::textChanged, this, &CommandSearch::fill);
  connect(m_list, &QTreeWidget::itemActivated, this, &CommandSearch::choose);
  connect(m_list, &QTreeWidget::itemClicked, this, &CommandSearch::choose);
  m_list->setContextMenuPolicy(Qt::CustomContextMenu);
  connect(m_list, &QWidget::customContextMenuRequested, this, &CommandSearch::openRowMenu);
}

void CommandSearch::openRowMenu(const QPoint& at) {
  const QTreeWidgetItem* item = m_list->itemAt(at);
  const QString id = item != nullptr ? item->data(0, Qt::UserRole).toString() : QString();
  if (!m_quickAccessEntries || id.isEmpty()) {
    return; // the mode hint's row has no command
  }
  // A command that cannot run now can still be added: the group shows it
  // disabled, with the reason in its tooltip.
  const bool chosen = QuickAccessSettings::isChosen(id);
  QMenu menu(this);
  menu.setObjectName(QStringLiteral("commandSearchRowMenu"));
  QAction* quick = menu.addAction(chosen ? tr("Remove from Quick Access") : tr("Add to Quick Access"));
  qDebug().noquote() << QStringLiteral("Command search menu %1: %2").arg(id, quick->text());
  if (menu.exec(m_list->viewport()->mapToGlobal(at)) == quick) {
    QuickAccessSettings::choose(id, !chosen);
  }
}

void CommandSearch::popup(const QPoint& globalCenter) {
  m_edit->clear();
  fill(QString());
  move(globalCenter - QPoint(width() / 2, height() / 3));
  // Shown as Qt shows its own popups (menus, drop-down lists): without
  // raising or activating it. An open popup is above its window and takes
  // the keys while that window stays active. Asking for the activation made
  // the search close as it opened where the system refuses it (Windows
  // session 0, mitcad#1477): Qt then sees the application lose the focus
  // and closes every popup.
  show();
  m_edit->setFocus();
  qDebug() << "Command search opened";
}

void CommandSearch::paintEvent(QPaintEvent* event) {
  if (!mac::paintCommandSearch(this)) {
    QFrame::paintEvent(event);
  }
}

// The window it opened over takes the keys again: its shortcuts work only
// while it is active, and the system may activate another window when the
// popup goes. Logged for the UI tests: a key sent right after Esc would race
// the popup still closing.
void CommandSearch::hideEvent(QHideEvent* event) {
  QFrame::hideEvent(event);
  if (QWidget* owner = parentWidget()) {
    owner->window()->activateWindow();
  }
  qDebug() << "Command search closed";
}

void CommandSearch::fill(const QString& rawText) {
  const QString text = search::normalizedText(rawText);
  std::vector<const CommandDef*> defs;
  std::vector<search::Candidate> candidates;
  for (const auto& def : m_registry.commands()) {
    QAction* action = m_registry.action(def->id);
    if (action == nullptr) {
      continue;
    }
    search::Candidate candidate;
    candidate.id = def->id;
    candidate.shown = def->name;
    candidate.english = englishText(def->name);
    candidate.keywords = def->keywords;
    if (search::score(candidate, text) < 0) {
      continue;
    }
    candidate.enabled = action->isEnabled();
    // Asked only for the matches of a text typed (an empty search ranks
    // every command alike, so no hint): it may look at the model.
    if (!candidate.enabled && !text.isEmpty() && m_modeHint) {
      candidate.modeHint = m_modeHint(*def);
    }
    defs.push_back(def.get());
    candidates.push_back(std::move(candidate));
  }
  // Among equal matches the commands run last first (mitcad#1690).
  const search::Listing listing = search::list(candidates, text, search::recentCommands());
  m_list->clear();
  if (!listing.hint.isEmpty()) {
    // A row of its own above the commands that wait for the mode
    // (mitcad#1681): nothing to run, so Enter starts no weaker match; no id.
    auto* hint = new QTreeWidgetItem(m_list, {listing.hint, QString(), QString()});
    hint->setFlags(Qt::NoItemFlags);
    hint->setToolTip(0, listing.hint);
    QFont font = m_list->font();
    font.setItalic(true);
    hint->setFont(0, font);
    // Readable, not greyed as the commands that cannot run are.
    hint->setForeground(0, m_list->palette().brush(QPalette::Active, QPalette::Text));
    hint->setFirstColumnSpanned(true);
    qDebug().noquote() << QStringLiteral("Command search hint: %1").arg(listing.hint);
  }
  // Where each command sits (mitcad#1983): from the layout held in memory
  // (the default with the user's layer), never read from its file here.
  search::PlaceCaptions captions;
  captions.ribbon = [](const QString& key) { return displayName(key); };
  captions.menu = [this](const QString& key) {
    const QString title = m_menuCaption ? m_menuCaption(key) : QString();
    return title.isEmpty() ? displayName(key) : title;
  };
  captions.menuBar = tr("Menu bar");
  captions.nowhere = tr("No toolbar place");
  const CommandLayout& layout = commandLayout();
  for (const int index : listing.rows) {
    const CommandDef* def = defs[static_cast<size_t>(index)];
    QAction* action = m_registry.action(def->id);
    const QString place = search::placeLine(layout, def->id, captions);
    auto* item = new QTreeWidgetItem(
        m_list, {def->name, place, action->shortcut().toString(QKeySequence::NativeText)});
    item->setIcon(0, action->icon());
    item->setData(0, Qt::UserRole, def->id);
    item->setToolTip(0, action->toolTip());
    item->setToolTip(1, place);
    // Quieter than the command's name, as the shortcut is on macOS.
    item->setForeground(1, m_list->palette().brush(QPalette::Active, QPalette::PlaceholderText));
    if (!action->isEnabled()) {
      item->setFlags(item->flags() & ~Qt::ItemIsEnabled);
    }
  }
  if (listing.current >= 0) {
    const int row = listing.current + (listing.hint.isEmpty() ? 0 : 1);
    m_list->setCurrentItem(m_list->topLevelItem(row));
  }
}

void CommandSearch::choose() {
  QTreeWidgetItem* item = m_list->currentItem();
  if (item == nullptr || !(item->flags() & Qt::ItemIsEnabled)) {
    return;
  }
  const QString id = item->data(0, Qt::UserRole).toString();
  hide();
  qDebug().noquote() << "Command search chose" << id;
  emit chosen(id);
}

bool CommandSearch::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_edit && event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<QKeyEvent*>(event);
    switch (key->key()) {
    case Qt::Key_Up:
    case Qt::Key_Down:
    case Qt::Key_PageUp:
    case Qt::Key_PageDown: {
      // Move in the list and skip what cannot run.
      const int step = key->key() == Qt::Key_Up || key->key() == Qt::Key_PageUp ? -1 : 1;
      int row = m_list->indexOfTopLevelItem(m_list->currentItem());
      for (int next = row + step; next >= 0 && next < m_list->topLevelItemCount(); next += step) {
        if (m_list->topLevelItem(next)->flags() & Qt::ItemIsEnabled) {
          row = next;
          break;
        }
      }
      if (row >= 0) {
        m_list->setCurrentItem(m_list->topLevelItem(row));
      }
      return true;
    }
    case Qt::Key_Return:
    case Qt::Key_Enter:
      choose();
      return true;
    case Qt::Key_Escape:
      hide();
      return true;
    default:
      break;
    }
  }
  return QFrame::eventFilter(watched, event);
}

} // namespace mitcad
