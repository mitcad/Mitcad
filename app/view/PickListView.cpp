// SPDX-License-Identifier: MIT
// The 3D view's pick list (mitcad#1703, app/COMMANDS.md "Selection"): a
// left press held still for about half a second lists everything under the
// pointer that the filter takes, nearest first, beside it (view/PickList);
// the pointer over a row highlights that item in the view, a click on a row
// reports it as a click on it would, so the selection, a command's active
// input or the sketch takes it as such a click.
#include <algorithm>
#include <cstddef>
#include <map>
#include <optional>
#include <utility>

#include <QTimer>
#include <QtLogging>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <Prs3d_Drawer.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <SelectMgr_SelectableObject.hxx>
#include <SelectMgr_SortCriterion.hxx>
#include <StdSelect_ViewerSelector3d.hxx>

#include "../OcctViewer.hpp"
#include "PickList.hpp"

namespace mitcad {
namespace {

// How long a press is held still before the list opens.
constexpr int kPickListHoldMs = 500;

} // namespace

void OcctViewer::setPickListNames(std::function<QString(const SelectionItem&)> names) {
  m_pickListNames = std::move(names);
}

bool OcctViewer::pickListOpen() const { return m_pickList != nullptr && m_pickList->isOpen(); }

QString OcctViewer::pickListWords(const SelectionItem& item) const {
  if (m_pickListNames) {
    const QString words = m_pickListNames(item);
    if (!words.isEmpty()) {
      return words;
    }
  }
  QString kind = SelectionItem::kindText(item.kind);
  if (!kind.isEmpty()) {
    kind[0] = kind[0].toUpper();
  }
  return kind;
}

void OcctViewer::armPickList(Qt::MouseButton button, Qt::MouseButtons buttons) {
  disarmPickList();
  // The left button alone; a drawing tool takes its presses itself.
  if (button != Qt::LeftButton || buttons != Qt::LeftButton || m_sketchInput) {
    return;
  }
  if (m_pickListTimer == nullptr) {
    m_pickListTimer = new QTimer(this);
    m_pickListTimer->setSingleShot(true);
    m_pickListTimer->setInterval(kPickListHoldMs);
    connect(m_pickListTimer, &QTimer::timeout, this, &OcctViewer::openPickList);
  }
  m_pickListPosition = m_pressPosition;
  m_pickListArmed = true;
  m_pickListTimer->start();
}

void OcctViewer::disarmPickList() {
  m_pickListArmed = false;
  if (m_pickListTimer != nullptr) {
    m_pickListTimer->stop();
  }
}

std::vector<OcctViewer::PickListEntry> OcctViewer::pickListAt(const QPointF& position) {
  std::vector<PickListEntry> entries;
  if (m_view->Window().IsNull() || !QRectF(rect()).contains(position) || isCovered(position)) {
    return entries;
  }
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
  // How far each owner lies along the pointer's ray, as the pick found it:
  // the order the list shows, whatever the click's own priorities put
  // first (an edge before its face, a region before a nearer face).
  const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
  std::map<const SelectMgr_EntityOwner*, double> depths;
  for (int rank = 1; rank <= selector->NbPicked(); ++rank) {
    depths.emplace(selector->Picked(rank).get(), selector->PickedData(rank).Depth);
  }
  for (m_context->InitDetected(); m_context->MoreDetected(); m_context->NextDetected()) {
    const occ::handle<SelectMgr_EntityOwner> owner = m_context->DetectedCurrentOwner();
    if (owner.IsNull() || owner->Selectable() == m_orientationCube || !accepts(owner)) {
      continue;
    }
    std::optional<SelectionItem> item = itemFor(owner);
    if (!item) {
      continue;
    }
    item->at = pickedPoint(owner);
    const auto found = depths.find(owner.get());
    const double depth = found != depths.end() ? found->second : 0.0;
    // Each item once, where it is nearest (a selected item's highlight
    // takes the picks of its body's item).
    const auto same = std::find_if(entries.begin(), entries.end(),
                                   [&item](const PickListEntry& entry) { return entry.item == *item; });
    if (same == entries.end()) {
      entries.push_back({*item, owner, depth});
    } else if (depth < same->depth) {
      *same = {*item, owner, depth};
    }
  }
  m_context->ClearDetected(false);
  ResetPreviousMoveTo(); // the hover picks again as before
  std::stable_sort(entries.begin(), entries.end(),
                   [](const PickListEntry& a, const PickListEntry& b) { return a.depth < b.depth; });
  return entries;
}

void OcctViewer::openPickList() {
  if (!m_pickListArmed) {
    return;
  }
  m_pickListArmed = false;
  if (m_pickingSuspended || m_dragging) {
    return;
  }
  std::vector<PickListEntry> entries = pickListAt(m_pickListPosition);
  if (entries.empty()) {
    return; // nothing under it: the release clicks on nothing, as before
  }
  // The press is the list's now: its moves orbit nothing, and its release
  // neither ends a window selection nor clicks.
  cancelPointerInteraction();
  m_pickListHeld = true;
  m_pickListEntries = std::move(entries);
  if (m_pickList == nullptr) {
    m_pickList = new PickList(this);
    connect(m_pickList, &PickList::rowHovered, this, &OcctViewer::hoverPickListRow);
    connect(m_pickList, &PickList::rowChosen, this, &OcctViewer::choosePickListRow);
    connect(m_pickList, &PickList::dismissed, this, [this](const QString& why) { closePickList(why); });
  }
  QStringList rows;
  QStringList logged;
  for (const PickListEntry& entry : m_pickListEntries) {
    const QString words = pickListWords(entry.item);
    rows << words;
    logged << QStringLiteral("%1 [%2]").arg(entry.item.describe(), words);
  }
  m_pickList->open(rows, m_pickListPosition.toPoint(), rect().marginsRemoved(m_overlayInsets), coveredRects());
  m_context->ClearDetected(false); // no hover beside the row's highlight
  requestFrame();
  const QPoint at = mapTo(window(), m_pickListPosition.toPoint());
  qDebug().noquote() << QStringLiteral("Pick list at %1,%2: %3 items: %4")
                            .arg(QString::number(at.x()), QString::number(at.y()), QString::number(rows.size()),
                                 logged.join(QStringLiteral("; ")));
  // Where a click takes each row, for UI tests.
  for (int row = 0; row < m_pickList->rowCount(); ++row) {
    const QRect place = m_pickList->rowRect(row);
    if (place.isEmpty()) {
      continue; // scrolled out of sight
    }
    const QPoint middle = m_pickList->mapTo(window(), place.center());
    qDebug().noquote() << QStringLiteral("Pick list row %1 %2 at %3,%4")
                              .arg(QString::number(row + 1), logged.at(row), QString::number(middle.x()),
                                   QString::number(middle.y()));
  }
}

void OcctViewer::closePickList(const QString& why) {
  disarmPickList();
  if (m_pickListEntries.empty()) {
    return;
  }
  if (m_pickList != nullptr) {
    m_pickList->dismiss();
  }
  m_context->MainPrsMgr()->ClearImmediateDraw();
  m_pickListEntries.clear();
  requestFrame();
  qDebug().noquote() << QStringLiteral("Pick list closed: %1").arg(why);
}

void OcctViewer::hoverPickListRow(int row) {
  // The row's item in the hover's look, as the view's own hover draws it.
  const occ::handle<PrsMgr_PresentationManager>& manager = m_context->MainPrsMgr();
  manager->ClearImmediateDraw();
  if (row >= 0 && row < static_cast<int>(m_pickListEntries.size())) {
    const PickListEntry& entry = m_pickListEntries[static_cast<std::size_t>(row)];
    const occ::handle<AIS_InteractiveObject> object =
        occ::handle<AIS_InteractiveObject>::DownCast(entry.owner->Selectable());
    if (!object.IsNull() && m_context->IsDisplayed(object)) {
      const occ::handle<Prs3d_Drawer>& style = m_context->HighlightStyle(
          entry.owner->ComesFromDecomposition() ? Prs3d_TypeOfHighlight_LocalDynamic : Prs3d_TypeOfHighlight_Dynamic);
      const int mode = style->DisplayMode() >= 0 ? style->DisplayMode()
                       : object->HasHilightMode() ? object->HilightMode()
                       : object->HasDisplayMode() ? object->DisplayMode()
                                                  : m_context->DisplayMode();
      manager->BeginImmediateDraw();
      if (entry.owner->IsAutoHilight()) {
        entry.owner->HilightWithColor(manager, style, mode);
      } else {
        object->HilightOwnerWithColor(manager, style, entry.owner);
      }
      manager->EndImmediateDraw(m_viewer);
    }
    qDebug().noquote() << QStringLiteral("Pick list hover %1").arg(entry.item.describe());
  }
  requestFrame();
}

void OcctViewer::choosePickListRow(int row, Qt::KeyboardModifiers modifiers) {
  if (row < 0 || row >= static_cast<int>(m_pickListEntries.size())) {
    return;
  }
  const SelectionItem item = m_pickListEntries[static_cast<std::size_t>(row)].item;
  const QPointF position = m_pickListPosition;
  qDebug().noquote() << QStringLiteral("Pick list chose %1 [%2]").arg(item.describe(), pickListWords(item));
  closePickList(QStringLiteral("chosen"));
  // As a click on it would (OnSelectionChanged): where it was, with the
  // keys held as the row was clicked (Ctrl or Shift adds), then the item.
  m_clickPosition = position;
  m_pressModifiers = modifiers;
  m_windowPick = false;
  m_clickPickedNothing = false;
  if (!isCovered(position)) {
    emit clickedAt(position, modifiers);
  }
  emit picked(Selection{item}, modifiers, false);
}

} // namespace mitcad
