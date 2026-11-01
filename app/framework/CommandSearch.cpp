// SPDX-License-Identifier: MIT
#include "CommandSearch.hpp"

#include <algorithm>
#include <tuple>
#include <vector>

#include <QAction>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLineEdit>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "CommandRegistry.hpp"
#include "../platform/MacCommandSearch.hpp"

namespace mitcad {
namespace {

// Lower is a better match; -1 is none.
int score(const CommandDef& def, const QString& text) {
  if (text.isEmpty()) {
    return 6;
  }
  const QString name = def.name.toLower();
  // The whole name first, so "export" finds Export... before Export Drawing...
  QString bare = name;
  if (bare.endsWith(QStringLiteral("..."))) {
    bare.chop(3);
  } else if (bare.endsWith(QChar(0x2026))) {
    bare.chop(1);
  }
  if (bare == text) {
    return 0;
  }
  if (name.startsWith(text)) {
    return 1;
  }
  const QStringList words = name.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  if (std::any_of(words.begin(), words.end(), [&](const QString& w) { return w.startsWith(text); })) {
    return 2;
  }
  if (name.contains(text)) {
    return 3;
  }
  for (const QString& keyword : def.keywords) {
    if (keyword.toLower().startsWith(text)) {
      return 4;
    }
  }
  // The letters in order, as in "fll" for Fillet.
  int at = 0;
  for (const QChar c : text) {
    at = static_cast<int>(name.indexOf(c, at));
    if (at < 0) {
      return -1;
    }
    ++at;
  }
  return 5;
}

} // namespace

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
  m_list->setObjectName(QStringLiteral("commandSearchResults"));
  m_list->setAccessibleName(tr("Commands"));
  m_list->setColumnCount(2);
  m_list->setHeaderHidden(true);
  m_list->setRootIsDecorated(false);
  m_list->setFocusPolicy(Qt::NoFocus);
  m_list->header()->setStretchLastSection(false);
  m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  layout->addWidget(m_edit);
  layout->addWidget(m_list);
  resize(360, 300);
  mac::styleCommandSearch(this, m_edit, m_list, layout);
  connect(m_edit, &QLineEdit::textChanged, this, &CommandSearch::fill);
  connect(m_list, &QTreeWidget::itemActivated, this, &CommandSearch::choose);
  connect(m_list, &QTreeWidget::itemClicked, this, &CommandSearch::choose);
}

void CommandSearch::popup(const QPoint& globalCenter) {
  m_edit->clear();
  fill(QString());
  move(globalCenter - QPoint(width() / 2, height() / 3));
  show();
  raise();
  activateWindow();
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
  QString text = rawText.trimmed().toLower();
  if (text.endsWith(QStringLiteral("..."))) { text.chop(3); }
  else if (text.endsWith(QChar(0x2026))) { text.chop(1); }
  text = text.trimmed();
  std::vector<std::tuple<int, bool, QString, const CommandDef*>> found;
  for (const auto& def : m_registry.commands()) {
    QAction* action = m_registry.action(def->id);
    const int rank = score(*def, text);
    if (action == nullptr || rank < 0) {
      continue;
    }
    // Commands that can run now come first.
    found.emplace_back(rank, !action->isEnabled(), def->name, def.get());
  }
  std::sort(found.begin(), found.end(), [](const auto& a, const auto& b) {
    return std::tie(std::get<1>(a), std::get<0>(a), std::get<2>(a)) <
           std::tie(std::get<1>(b), std::get<0>(b), std::get<2>(b));
  });
  m_list->clear();
  for (const auto& entry : found) {
    const CommandDef* def = std::get<3>(entry);
    QAction* action = m_registry.action(def->id);
    auto* item = new QTreeWidgetItem(
        m_list, {def->name, action->shortcut().toString(QKeySequence::NativeText)});
    item->setIcon(0, action->icon());
    item->setData(0, Qt::UserRole, def->id);
    item->setToolTip(0, action->toolTip());
    if (!action->isEnabled()) {
      item->setFlags(item->flags() & ~Qt::ItemIsEnabled);
    }
  }
  if (m_list->topLevelItemCount() > 0 &&
      (m_list->topLevelItem(0)->flags() & Qt::ItemIsEnabled)) {
    m_list->setCurrentItem(m_list->topLevelItem(0));
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
