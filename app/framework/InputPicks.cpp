// SPDX-License-Identifier: MIT
#include "InputPicks.hpp"

#include <QCoreApplication>
#include <QEvent>
#include <QHash>
#include <QListWidget>
#include <QMouseEvent>
#include <QSettings>

#include <optional>
#include <utility>

namespace mitcad {
namespace {

const QString& shownKey() {
  static const QString key = QStringLiteral("commandPanel/picksShown");
  return key;
}

// Read from the settings once: every panel's refresh asks for it.
std::optional<bool>& cachedShown() {
  static std::optional<bool> shown;
  return shown;
}

} // namespace

bool inputPicksShown() {
  std::optional<bool>& shown = cachedShown();
  if (!shown) {
    const QSettings settings;
    shown = settings.value(shownKey(), false).toBool();
  }
  return *shown;
}

void setInputPicksShown(bool shown) {
  if (shown == inputPicksShown()) {
    return;
  }
  cachedShown() = shown;
  {
    QSettings settings;
    settings.setValue(shownKey(), shown);
  }
  emit InputPicksState::instance().shownChanged(shown);
}

InputPicksState& InputPicksState::instance() {
  static InputPicksState state;
  return state;
}

QStringList pickWords(const Selection& items, const std::function<QString(const SelectionItem&)>& text) {
  QStringList words;
  words.reserve(items.size());
  QHash<QString, QString> byOwner;
  for (const SelectionItem& item : items) {
    const bool named =
        item.kind == SelectKind::Plane || item.kind == SelectKind::Axis || item.kind == SelectKind::Point;
    if (named) {
      words << text(item);
      continue;
    }
    const QString key = QStringLiteral("%1|%2").arg(static_cast<unsigned>(item.kind)).arg(item.owner);
    auto found = byOwner.constFind(key);
    if (found == byOwner.cend()) {
      found = byOwner.insert(key, text(item));
    }
    words << *found;
  }
  return words;
}

QString inputPicksTooltip(const QStringList& words, const QString& hint) {
  if (words.isEmpty()) {
    return hint;
  }
  QStringList lines = words.mid(0, kInputPicksTipItems);
  const int more = static_cast<int>(words.size()) - kInputPicksTipItems;
  if (more > 0) {
    lines << QCoreApplication::translate("mitcad::CommandPanel", "and %n more", nullptr, more);
  }
  if (!hint.isEmpty()) {
    lines << hint;
  }
  return lines.join(QLatin1Char('\n'));
}

PickHoverWatch::PickHoverWatch(QListWidget* list, std::function<void(int)> hovered)
    : QObject(list), m_list(list), m_hovered(std::move(hovered)) {
  // Moves without a button reach the viewport only with tracking on.
  list->setMouseTracking(true);
  list->viewport()->setMouseTracking(true);
  list->viewport()->installEventFilter(this);
}

bool PickHoverWatch::eventFilter(QObject* watched, QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseMove: {
    const QListWidgetItem* item = m_list->itemAt(static_cast<QMouseEvent*>(event)->position().toPoint());
    update(item != nullptr ? m_list->row(item) : -1);
    break;
  }
  case QEvent::Leave:
  case QEvent::Hide:
    update(-1);
    break;
  default:
    break;
  }
  return QObject::eventFilter(watched, event);
}

void PickHoverWatch::update(int row) {
  if (row != m_row) {
    m_row = row;
    m_hovered(row);
  }
}

} // namespace mitcad
