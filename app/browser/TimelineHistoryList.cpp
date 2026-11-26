// SPDX-License-Identifier: MIT
// The timeline's History list (mitcad#2427, TimelineHistoryList.hpp): the
// strip's items as rows painted as the Browser's, the marker a bar between
// them; every gesture goes to TimelineWidget and its strip.
#include "TimelineHistoryList.hpp"

#include <algorithm>
#include <climits>
#include <cstdlib>
#include <utility>

#include <QBrush>
#include <QColor>
#include <QContextMenuEvent>
#include <QEvent>
#include <QFont>
#include <QHash>
#include <QHelpEvent>
#include <QIcon>
#include <QItemSelectionModel>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QPen>
#include <QScrollBar>
#include <QSet>
#include <QSignalBlocker>
#include <QStyledItemDelegate>
#include <QToolTip>
#include <QTreeWidgetItemIterator>
#include <QVariant>
#include <QtLogging>

#include "BrowserPreferences.hpp"
#include "BrowserRows.hpp"
#include "ComponentColors.hpp"
#include "DocumentSnapshot.hpp"
#include "FeatureStatus.hpp"
#include "TimelineWidget.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "../framework/WindowStyle.hpp"

namespace mitcad {
namespace {

// The rows' own data, above the Browser's roles (BrowserRows.hpp): what
// the row is, the items it stands for (first and last, by index) and its
// feature's uid (a group's first feature's).
constexpr int kKindRole = Qt::UserRole + 40;
constexpr int kFirstRole = Qt::UserRole + 41;
constexpr int kLastRole = Qt::UserRole + 42;
constexpr int kUidRole = Qt::UserRole + 43;

// A press this many pixels above or below the marker's bar takes it.
constexpr int kMarkerReach = 4;
constexpr int kDragPixels = 6;
// The most rows the "History list:" line names.
constexpr int kLoggedRows = 80;

// Delete deletes; on macOS the delete key of the keyboard (Backspace) too.
bool isDeleteKey(int key) {
#ifdef Q_OS_MACOS
  return key == Qt::Key_Delete || key == Qt::Key_Backspace;
#else
  return key == Qt::Key_Delete;
#endif
}

// A level's name colour, as the Browser's rows have it.
ColorRole levelTextRole(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return ColorRole::ErrorText;
  case NoticeLevel::Warning:
    return ColorRole::WarningText;
  case NoticeLevel::Success:
    return ColorRole::SuccessText;
  default:
    return ColorRole::InfoText;
  }
}

} // namespace

TimelineHistoryList::TimelineHistoryList(TimelineWidget& owner) : QTreeWidget(&owner), m_owner(owner) {
  setObjectName(QStringLiteral("timelineHistoryList"));
  setAccessibleName(tr("History list"));
  setColumnCount(1);
  setHeaderHidden(true);
  panel::setCompactIndentation(this, panel::Indent::Tree);
  setRootIsDecorated(true);
  setUniformRowHeights(true);
  setExpandsOnDoubleClick(false);
  setSelectionMode(QAbstractItemView::ExtendedSelection);
  setEditTriggers(QAbstractItemView::NoEditTriggers);
  setDragEnabled(false);
  setIconSize(uiIconSize(18, 18)); // the Browser's
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  setTextElideMode(Qt::ElideRight);
  setMouseTracking(true);
  viewport()->setMouseTracking(true);
  // Only a click gives it the keyboard, as the Browser's tree.
  setFocusPolicy(Qt::ClickFocus);
  if (chromeStyle() == ChromeStyle::Floating) {
    // On a card: its material shows through, the rows as the Browser's.
    setFrameShape(QFrame::NoFrame);
    setAttribute(Qt::WA_MacShowFocusRect, false);
    QPalette colours = palette();
    for (const QPalette::ColorRole role : {QPalette::Base, QPalette::AlternateBase, QPalette::Highlight}) {
      colours.setColor(role, Qt::transparent);
    }
    setPalette(colours);
    viewport()->setAutoFillBackground(false);
  } else {
    stylePanelTree(this);
  }
  m_delegate = browserRows::installDelegate(this);
  // The light bulb where the Browser's is (mitcad#2223).
  browserRows::setEyeRight(m_delegate, browserEyePlace() == BrowserControlPlace::Right);
  connect(&BrowserPreferencesNotifier::instance(), &BrowserPreferencesNotifier::controlPlacesChanged, this, [this] {
    browserRows::setEyeRight(m_delegate, browserEyePlace() == BrowserControlPlace::Right);
    scheduleDelayedItemsLayout();
    viewport()->update();
    TestSync::singleShot(0, &m_owner, [owner = &m_owner] { owner->logLayout(); });
  });
  // A name typed into a row's editor: the feature renamed as the strip's
  // editor does (after the edit has ended: the rows are made again).
  connect(this, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* row, int column) {
    if (m_building || column != 0 || kindOf(row) != Kind::Feature) {
      return;
    }
    const QString uid = uidOf(row);
    const int i = firstOf(row);
    m_renaming.clear();
    if (i < 0 || i >= m_owner.itemCount() || m_owner.itemFeature(i).uid != uid) {
      return;
    }
    const QString before = m_owner.itemFeature(i).name;
    const QString name = row->text(0).trimmed();
    if (name.isEmpty() || name == before) {
      const QSignalBlocker blocker(this);
      row->setText(0, before);
      return;
    }
    TestSync::singleShot(0, &m_owner, [owner = &m_owner, uid, name] { emit owner->renamed(uid, name); });
  });
  // The places logged again when the rows scroll.
  const auto logLater = [this] { TestSync::singleShot(0, &m_owner, [owner = &m_owner] { owner->logLayout(); }); };
  connect(verticalScrollBar(), &QScrollBar::valueChanged, this, logLater);
  connect(verticalScrollBar(), &QScrollBar::rangeChanged, this, logLater);
}

// ---------------------------------------------------------------------------
// The rows

TimelineHistoryList::Kind TimelineHistoryList::kindOf(const QTreeWidgetItem* row) const {
  return row != nullptr ? static_cast<Kind>(row->data(0, kKindRole).toInt()) : Kind::Run;
}

int TimelineHistoryList::firstOf(const QTreeWidgetItem* row) const {
  return row != nullptr ? row->data(0, kFirstRole).toInt() : -1;
}

int TimelineHistoryList::lastOf(const QTreeWidgetItem* row) const {
  return row != nullptr ? row->data(0, kLastRole).toInt() : -1;
}

QString TimelineHistoryList::uidOf(const QTreeWidgetItem* row) const {
  return row != nullptr ? row->data(0, kUidRole).toString() : QString();
}

void TimelineHistoryList::rebuild() {
  if (state() == QAbstractItemView::EditingState) {
    cancelRename();
  }
  const QSignalBlocker blocker(this);
  m_building = true;
  const int scrolled = verticalScrollBar()->value();
  m_hover = nullptr;
  clear();
  const int count = m_owner.itemCount();
  const int marker = m_owner.markerPosition();
  const DocumentSnapshot& snapshot = m_owner.snapshot();
  const QHash<QString, QStringList> bodies = snapshot.bodiesByFeature();
  const QVector<TimelineWidget::GroupRange> groups = m_owner.groupRanges();
  QHash<int, int> starts; // a group's first item: the group
  for (int g = 0; g < groups.size(); ++g) {
    starts.insert(groups[g].first, g);
  }
  m_topRow.assign(static_cast<std::size_t>(count) + 1, nullptr);
  m_featureRows.assign(static_cast<std::size_t>(count), nullptr);
  const auto topRow = [this](int i, QTreeWidgetItem* row) {
    if (m_topRow[static_cast<std::size_t>(i)] == nullptr) {
      m_topRow[static_cast<std::size_t>(i)] = row;
    }
  };
  // A feature's row: its icon and name, its state's colour and badge as
  // the Browser's rows have them, its fx badge, its component's bar and
  // its light bulb.
  const auto featureRow = [&](QTreeWidgetItem* parent, int i) {
    const DocumentSnapshot::Feature& feature = m_owner.itemFeature(i);
    auto* row = parent != nullptr ? new QTreeWidgetItem(parent) : new QTreeWidgetItem(this);
    row->setText(0, feature.name);
    row->setData(0, kKindRole, static_cast<int>(Kind::Feature));
    row->setData(0, kFirstRole, i);
    row->setData(0, kLastRole, i);
    row->setData(0, kUidRole, feature.uid);
    row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
    QIcon icon = themeIcon(featureIcon(feature.type));
    // An acknowledged warning is shown as ok (mitcad#1935).
    const FeatureState state = featureState(feature.shownStatus());
    const bool off = feature.suppressed || i >= marker;
    if (state != FeatureState::Ok) {
      const NoticeLevel level = featureStateLevel(state);
      icon = browserRows::levelIcon(icon, level);
      row->setData(0, browserRows::kTextRole, static_cast<int>(levelTextRole(level)));
    } else if (off) {
      // Rolled back or suppressed: the hidden rows' grey.
      row->setData(0, browserRows::kTextRole, static_cast<int>(ColorRole::TextHidden));
    }
    row->setIcon(0, icon);
    if (feature.suppressed) {
      QFont font = row->font(0);
      font.setStrikeOut(true); // as the strip's line across its cell
      row->setFont(0, font);
    }
    // Suppressed when a condition holds (mitcad#809): the fx badge.
    if (!feature.suppressedWhen.isEmpty()) {
      row->setData(0, browserRows::kBindingsRole, 1);
    }
    if (const int colour = m_owner.itemColour(i); colour >= 0) {
      row->setData(0, browserRows::kComponentColorRole, QVariant::fromValue(componentColor(colour)));
    }
    // The light bulb of what it shows at the marker: a sketch's or a
    // construction feature's own, or the bodies it made or last changed.
    if (!off && feature.isActive()) {
      if (feature.isAnySketch() || feature.isConstruction()) {
        row->setData(0, browserRows::kEyeRole, snapshot.isShown(feature.uid));
      } else if (const auto made = bodies.constFind(feature.uid); made != bodies.cend()) {
        bool shown = false;
        for (const QString& uid : *made) {
          const auto body = snapshot.bodies.constFind(uid);
          shown = shown || (body != snapshot.bodies.cend() && body->visible);
        }
        const int n = static_cast<int>(made->size());
        row->setData(0, browserRows::kEyeRole, shown);
        row->setData(0, browserRows::kEyeTipRole,
                     shown ? tr("Hide its bodies (%n)", nullptr, n) : tr("Show its bodies (%n)", nullptr, n));
      }
    }
    m_featureRows[static_cast<std::size_t>(i)] = row;
    topRow(i, row);
    return row;
  };
  std::vector<std::pair<QTreeWidgetItem*, bool>> opened; // the group rows: open or folded
  QTreeWidgetItem* group = nullptr;
  int groupLast = -1;
  QTreeWidgetItem* run = nullptr; // the row of the filter's run being filled
  int runCount = 0;
  for (int i = 0; i < count; ++i) {
    if (group != nullptr && i > groupLast) {
      group = nullptr;
      run = nullptr;
    }
    if (const auto start = starts.constFind(i); start != starts.cend()) {
      // A group's folder row, its features under it.
      const TimelineWidget::GroupRange& range = groups[*start];
      group = new QTreeWidgetItem(this);
      group->setText(0, range.name);
      group->setIcon(0, themeIcon(QStringLiteral("folder")));
      group->setData(0, kKindRole, static_cast<int>(Kind::Group));
      group->setData(0, kFirstRole, range.first);
      group->setData(0, kLastRole, range.last);
      group->setData(0, kUidRole, m_owner.itemFeature(range.first).uid);
      group->setFlags(Qt::ItemIsEnabled);
      // Folded: the worst state of its features, grey when none is built,
      // as the strip's folded cell.
      if (range.collapsed) {
        FeatureState worst = FeatureState::Ok;
        bool off = true;
        for (int k = range.first; k <= range.last; ++k) {
          const DocumentSnapshot::Feature& member = m_owner.itemFeature(k);
          worst = std::max(worst, featureState(member.shownStatus()));
          off = off && (member.suppressed || k >= marker);
        }
        if (worst != FeatureState::Ok) {
          const NoticeLevel level = featureStateLevel(worst);
          group->setIcon(0, browserRows::levelIcon(group->icon(0), level));
          group->setData(0, browserRows::kTextRole, static_cast<int>(levelTextRole(level)));
        } else if (off) {
          group->setData(0, browserRows::kTextRole, static_cast<int>(ColorRole::TextHidden));
        }
      }
      opened.emplace_back(group, !range.collapsed);
      groupLast = range.last;
      topRow(i, group);
      run = nullptr;
    }
    if (m_owner.itemFiltered(i)) {
      // A run of features the filter hides (mitcad#98): one grey row.
      if (run == nullptr) {
        run = group != nullptr ? new QTreeWidgetItem(group) : new QTreeWidgetItem(this);
        run->setData(0, kKindRole, static_cast<int>(Kind::Run));
        run->setData(0, kFirstRole, i);
        run->setIcon(0, themeIcon(QStringLiteral("filter")));
        run->setData(0, browserRows::kTextRole, static_cast<int>(ColorRole::TextHidden));
        run->setFlags(Qt::ItemIsEnabled);
        run->setToolTip(0, tr("Features of components the timeline's filter does not show"));
        runCount = 0;
      }
      ++runCount;
      run->setData(0, kLastRole, i);
      run->setText(0, tr("%n feature(s) hidden by the filter", nullptr, runCount));
      topRow(i, run);
      continue;
    }
    run = nullptr;
    featureRow(group, i);
  }
  for (const auto& [row, open] : opened) {
    row->setExpanded(open);
  }
  doItemsLayout();
  verticalScrollBar()->setValue(scrolled);
  m_building = false;
  updateMarks();
  logRows();
  emit m_owner.listHeightChanged();
}

void TimelineHistoryList::updateMarks() {
  if (m_building) {
    return;
  }
  const QStringList selected = m_owner.selectedFeatures();
  const QSet<QString> chosen(selected.cbegin(), selected.cend());
  QColor found = themeColor(ColorRole::SearchMatch, palette());
  found.setAlphaF(0.22f);
  QStringList names;
  {
    const QSignalBlocker blocker(this);
    const auto mark = [&found](QTreeWidgetItem* row, bool marked, bool matched) {
      row->setData(0, browserRows::kOwnerMarkRole, marked ? QVariant(true) : QVariant());
      row->setData(0, Qt::BackgroundRole, matched ? QVariant(QBrush(found)) : QVariant());
    };
    for (QTreeWidgetItem* row : m_featureRows) {
      if (row == nullptr) {
        continue;
      }
      const QString uid = uidOf(row);
      const bool on = chosen.contains(uid);
      if (row->isSelected() != on) {
        row->setSelected(on);
      }
      if (on) {
        names << row->text(0);
      }
      mark(row, m_owner.itemMarked(uid), m_owner.itemFound(uid));
    }
    // A folded group's row: its features' marks and matches.
    for (int t = 0; t < topLevelItemCount(); ++t) {
      QTreeWidgetItem* row = topLevelItem(t);
      if (kindOf(row) != Kind::Group) {
        continue;
      }
      bool marked = false;
      bool matched = false;
      for (int c = 0; c < row->childCount() && !row->isExpanded(); ++c) {
        const QString uid = uidOf(row->child(c));
        marked = marked || (!uid.isEmpty() && m_owner.itemMarked(uid));
        matched = matched || (!uid.isEmpty() && m_owner.itemFound(uid));
      }
      mark(row, marked, matched);
    }
  }
  const QString logged = QStringLiteral("History selected: %1")
                             .arg(names.isEmpty() ? QStringLiteral("nothing") : names.join(QStringLiteral(", ")));
  if (logged != m_loggedSelection) {
    m_loggedSelection = logged;
    qDebug().noquote() << logged;
  }
  viewport()->update();
}

void TimelineHistoryList::scrollToFeature(const QString& uid) {
  for (QTreeWidgetItem* row : m_featureRows) {
    if (row != nullptr && uidOf(row) == uid) {
      QTreeWidgetItem* shown = row->parent() != nullptr && !row->parent()->isExpanded() ? row->parent() : row;
      scrollToItem(shown, QAbstractItemView::EnsureVisible);
      return;
    }
  }
}

void TimelineHistoryList::showMarked() {
  QTreeWidgetItem* last = nullptr;
  const QRect shown = viewport()->rect();
  for (QTreeWidgetItemIterator it(this); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    if (!row->data(0, browserRows::kOwnerMarkRole).toBool() ||
        (row->parent() != nullptr && !row->parent()->isExpanded())) {
      continue;
    }
    if (shown.contains(visualItemRect(row).center())) {
      return; // one is in sight
    }
    last = row;
  }
  if (last != nullptr) {
    scrollToItem(last, QAbstractItemView::EnsureVisible);
  }
}

void TimelineHistoryList::startRename(const QString& uid) {
  for (QTreeWidgetItem* row : m_featureRows) {
    if (row == nullptr || uidOf(row) != uid) {
      continue;
    }
    if (row->parent() != nullptr && !row->parent()->isExpanded()) {
      return;
    }
    m_renaming = uid;
    setFocus(Qt::OtherFocusReason);
    setCurrentItem(row, 0, QItemSelectionModel::NoUpdate);
    scrollToItem(row);
    editItem(row, 0);
    qDebug().noquote() << QStringLiteral("Renaming %1 in the History list").arg(row->text(0));
    return;
  }
}

bool TimelineHistoryList::cancelRename() {
  if (state() != QAbstractItemView::EditingState) {
    return false;
  }
  if (QWidget* editor = indexWidget(currentIndex())) {
    closeEditor(editor, QAbstractItemDelegate::RevertModelCache);
  }
  m_renaming.clear();
  return true;
}

int TimelineHistoryList::contentHeight() const {
  int rows = 0;
  for (int t = 0; t < topLevelItemCount(); ++t) {
    const QTreeWidgetItem* top = topLevelItem(t);
    rows += 1 + (top->isExpanded() ? top->childCount() : 0);
  }
  const int row = std::max(sizeHintForRow(0), fontMetrics().height() + 8);
  return std::max(1, rows) * row + 2 * frameWidth() + 4;
}

QTreeWidgetItem* TimelineHistoryList::lastShownRow() const {
  if (topLevelItemCount() == 0) {
    return nullptr;
  }
  QTreeWidgetItem* last = topLevelItem(topLevelItemCount() - 1);
  if (last->isExpanded() && last->childCount() > 0) {
    last = last->child(last->childCount() - 1);
  }
  return last;
}

int TimelineHistoryList::boundaryY(int g) const {
  QTreeWidgetItem* row = g >= 0 && static_cast<std::size_t>(g) < m_topRow.size()
                             ? m_topRow[static_cast<std::size_t>(g)]
                             : nullptr;
  if (row != nullptr) {
    if (row->parent() != nullptr && !row->parent()->isExpanded()) {
      row = row->parent(); // inside a folded group (no boundary the strip allows)
    }
    return visualItemRect(row).top();
  }
  const QTreeWidgetItem* last = lastShownRow();
  return last != nullptr ? visualItemRect(last).bottom() + 1 : 0;
}

int TimelineHistoryList::nearestBoundary(int y) const {
  const int count = m_owner.itemCount();
  // The boundaries at the edges of the row at y: the nearer one first.
  int nearer = count;
  int other = count;
  if (QTreeWidgetItem* row = itemAt(QPoint(std::max(0, viewport()->width() / 2), y))) {
    const int first = firstOf(row);
    const int last = lastOf(row);
    nearer = first;
    other = kindOf(row) == Kind::Group && row->isExpanded() ? first : last + 1;
    if (y >= visualItemRect(row).center().y()) {
      std::swap(nearer, other);
    }
  } else if (topLevelItemCount() > 0 && y < visualItemRect(topLevelItem(0)).top()) {
    nearer = 0;
    other = 0;
  }
  if (m_owner.boundaryAllowed(nearer)) {
    return nearer;
  }
  if (m_owner.boundaryAllowed(other)) {
    return other;
  }
  // The nearest the strip allows by its place.
  int best = -1;
  int distance = INT_MAX;
  for (int g = 0; g <= count; ++g) {
    if (m_owner.boundaryAllowed(g)) {
      const int d = std::abs(boundaryY(g) - y);
      if (d < distance) {
        distance = d;
        best = g;
      }
    }
  }
  return best;
}

QRect TimelineHistoryList::markerHandle() const {
  const int y = boundaryY(m_dragMarker ? m_dragPosition : m_owner.markerPosition());
  return QRect(2, y - 4, 9, 8);
}

bool TimelineHistoryList::onMarker(const QPoint& at) const {
  const int y = boundaryY(m_dragMarker ? m_dragPosition : m_owner.markerPosition());
  return std::abs(at.y() - y) <= kMarkerReach || markerHandle().adjusted(-3, -3, 3, 3).contains(at);
}

QRect TimelineHistoryList::eyeRect(QTreeWidgetItem* row) const {
  if (row == nullptr || !row->data(0, browserRows::kEyeRole).isValid() ||
      (row->parent() != nullptr && !row->parent()->isExpanded())) {
    return QRect();
  }
  const QModelIndex index = indexFromItem(row, 0);
  QStyleOptionViewItem option;
  initViewItemOption(&option);
  option.rect = visualRect(index);
  return browserRows::rowParts(m_delegate, option, index).eye;
}

QRect TimelineHistoryList::fxRect(QTreeWidgetItem* row) const {
  if (row == nullptr || row->data(0, browserRows::kBindingsRole).toInt() <= 0 ||
      (row->parent() != nullptr && !row->parent()->isExpanded())) {
    return QRect();
  }
  const QModelIndex index = indexFromItem(row, 0);
  QStyleOptionViewItem option;
  initViewItemOption(&option);
  option.rect = visualRect(index);
  return browserRows::rowParts(m_delegate, option, index).fx;
}

void TimelineHistoryList::toggleGroup(QTreeWidgetItem* row) {
  const QString name = row->text(0);
  const bool fold = row->isExpanded();
  qDebug().noquote() << QStringLiteral("History group %1 %2").arg(name, fold ? QStringLiteral("folded")
                                                                              : QStringLiteral("opened"));
  // The strip's one folded state (TimelineWidget::setGroupCollapsed), after
  // this event: it makes the rows again.
  TestSync::singleShot(0, &m_owner, [owner = &m_owner, name, fold] { owner->setGroupCollapsed(name, fold); });
}

void TimelineHistoryList::hoverRow(QTreeWidgetItem* row) {
  m_hover = row;
  QString uid;
  if (row != nullptr && kindOf(row) == Kind::Feature) {
    uid = uidOf(row);
  } else if (row != nullptr && kindOf(row) == Kind::Group && !row->isExpanded()) {
    uid = uidOf(row); // the folded group's first: its features, as the strip's cell
  }
  m_owner.hoverFeature(uid);
}

QStringList TimelineHistoryList::layoutLines() const {
  QStringList lines;
  const QRect shown = viewport()->rect();
  const auto place = [this](const QPoint& at) { return GlassCard::mapToHost(viewport(), at); };
  const auto line = [&lines, &place](const QString& what, const QPoint& at) {
    const QPoint host = place(at);
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(host.x()).arg(host.y());
  };
  for (QTreeWidgetItemIterator it(const_cast<TimelineHistoryList*>(this)); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    if (row->parent() != nullptr && !row->parent()->isExpanded()) {
      continue;
    }
    const QRect rect = visualItemRect(row);
    if (!rect.isValid() || !shown.contains(rect.center())) {
      continue;
    }
    const QModelIndex index = indexFromItem(row, 0);
    QStyleOptionViewItem option;
    initViewItemOption(&option);
    option.rect = rect;
    const browserRows::RowParts parts = browserRows::rowParts(m_delegate, option, index);
    const QPoint name = parts.name.isValid() ? parts.name.center() : rect.center();
    switch (kindOf(row)) {
    case Kind::Feature:
      line(QStringLiteral("History row %1").arg(row->text(0)), name);
      if (!parts.eye.isEmpty()) {
        line(QStringLiteral("History eye %1").arg(row->text(0)), parts.eye.center());
      }
      if (!parts.fx.isEmpty()) {
        line(QStringLiteral("History fx %1").arg(row->text(0)), parts.fx.center());
      }
      break;
    case Kind::Group:
      line(QStringLiteral("History group %1").arg(row->text(0)), name);
      line(QStringLiteral("History group %1 arrow").arg(row->text(0)),
           QPoint(rect.left() - indentation() / 2, rect.center().y()));
      break;
    case Kind::Run:
      break;
    }
  }
  const QRect handle = markerHandle();
  if (shown.contains(QPoint(shown.center().x(), handle.center().y()))) {
    line(QStringLiteral("History marker"), handle.center());
  }
  return lines;
}

void TimelineHistoryList::logRows() {
  const auto text = [this](QTreeWidgetItem* row) {
    return kindOf(row) == Kind::Run ? QStringLiteral("(%1 hidden)").arg(lastOf(row) - firstOf(row) + 1)
                                    : row->text(0);
  };
  QStringList parts;
  for (int t = 0; t < topLevelItemCount() && parts.size() < kLoggedRows; ++t) {
    QTreeWidgetItem* row = topLevelItem(t);
    if (kindOf(row) != Kind::Group) {
      parts << text(row);
      continue;
    }
    if (!row->isExpanded()) {
      parts << QStringLiteral("[%1 folded]").arg(row->text(0));
      continue;
    }
    QStringList members;
    for (int c = 0; c < row->childCount(); ++c) {
      members << text(row->child(c));
    }
    parts << QStringLiteral("[%1: %2]").arg(row->text(0), members.join(QStringLiteral(", ")));
  }
  if (parts.size() >= kLoggedRows) {
    parts << QStringLiteral("...");
  }
  const int marker = m_owner.markerPosition();
  const QString where = marker <= 0 || marker > m_owner.itemCount()
                            ? QStringLiteral("marker at the start")
                            : QStringLiteral("marker after %1").arg(m_owner.itemFeature(marker - 1).name);
  const QString logged = QStringLiteral("History list: %1; %2")
                             .arg(parts.isEmpty() ? QStringLiteral("no rows") : parts.join(QStringLiteral(", ")),
                                  where);
  if (logged != m_loggedRows) {
    m_loggedRows = logged;
    qDebug().noquote() << logged;
  }
  // The light bulbs' states, each when it changed.
  QStringList eyes;
  for (QTreeWidgetItem* row : m_featureRows) {
    if (row != nullptr && row->data(0, browserRows::kEyeRole).isValid()) {
      eyes << QStringLiteral("History eye %1: %2")
                  .arg(row->text(0), row->data(0, browserRows::kEyeRole).toBool() ? QStringLiteral("shown")
                                                                                   : QStringLiteral("hidden"));
    }
  }
  for (const QString& eye : std::as_const(eyes)) {
    if (!m_loggedEyes.contains(eye)) {
      qDebug().noquote() << eye;
    }
  }
  m_loggedEyes = eyes;
}

// ---------------------------------------------------------------------------
// Painting and the gestures

void TimelineHistoryList::drawRow(QPainter* painter, const QStyleOptionViewItem& option,
                                  const QModelIndex& index) const {
  QTreeWidget::drawRow(painter, option, index);
  // The component's colour bar, as the Browser's rows have it (mitcad#1876).
  const QVariant colour = index.data(browserRows::kComponentColorRole);
  if (colour.isValid()) {
    browserRows::paintComponentBar(painter, visualRect(index), indentation(), colour.value<QColor>());
  }
}

void TimelineHistoryList::paintEvent(QPaintEvent* event) {
  QTreeWidget::paintEvent(event);
  QPainter painter(viewport());
  painter.setRenderHint(QPainter::Antialiasing);
  const int width = viewport()->width();
  // Where a dragged feature would go: a line between the rows.
  if (m_dragItem && m_dropIndex >= 0 && m_dropIndex != m_press) {
    const int gap = m_dropIndex >= m_press ? m_dropIndex + 1 : m_dropIndex;
    const double y = std::max(1, boundaryY(gap));
    painter.setPen(QPen(themeColor(m_dropAllowed ? ColorRole::DropAllowed : ColorRole::DropRefused, palette()), 3.0));
    painter.drawLine(QPointF(1, y), QPointF(width - 1, y));
  }
  // The history marker: a bar between the rows, its handle at the left.
  const double y = std::max(1, boundaryY(m_dragMarker ? m_dragPosition : m_owner.markerPosition()));
  const QColor bar = m_owner.listMarkerColor();
  painter.setPen(QPen(bar, 2.0));
  painter.drawLine(QPointF(4, y), QPointF(width - 2, y));
  painter.setPen(QPen(bar, 1.0));
  painter.setBrush(m_owner.listMarkerHandleColor());
  painter.drawRoundedRect(QRectF(markerHandle()).adjusted(0.5, 0.5, -0.5, -0.5), 1.5, 1.5);
}

void TimelineHistoryList::mousePressEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    event->accept(); // no selection of its own: the context menu follows
    return;
  }
  setFocus(Qt::MouseFocusReason);
  QToolTip::hideText();
  const QPoint at = event->position().toPoint();
  m_press = -1;
  m_dragItem = false;
  if (onMarker(at)) {
    m_dragMarker = true;
    m_dragPosition = m_owner.markerPosition();
    setCursor(Qt::SizeVerCursor);
    emit m_owner.markerDragStarted();
    return;
  }
  QTreeWidgetItem* row = itemAt(at);
  if (row == nullptr) {
    return;
  }
  const Kind kind = kindOf(row);
  // A group's fold arrow, left of its row.
  if (kind == Kind::Group && at.x() < visualItemRect(row).left()) {
    toggleGroup(row);
    return;
  }
  if (kind != Kind::Feature) {
    return;
  }
  if (eyeRect(row).contains(at)) {
    qDebug().noquote() << QStringLiteral("History eye click %1").arg(row->text(0));
    emit m_owner.eyeClicked(uidOf(row));
    return;
  }
  // A click on the fx badge pins its bindings card (mitcad#862).
  if (const QRect fx = fxRect(row); fx.contains(at)) {
    qDebug().noquote() << QStringLiteral("History fx click %1").arg(row->text(0));
    emit m_owner.bindingsRequested(uidOf(row), QRect(viewport()->mapToGlobal(fx.topLeft()), fx.size()), true);
    return;
  }
  m_press = firstOf(row);
  m_pressAt = at;
  m_shift = event->modifiers().testFlag(Qt::ShiftModifier);
  m_toggle = event->modifiers().testFlag(Qt::ControlModifier);
}

void TimelineHistoryList::mouseMoveEvent(QMouseEvent* event) {
  const QPoint at = event->position().toPoint();
  // Near the top or the bottom while dragging: the rows scroll.
  if (m_dragMarker || m_dragItem) {
    QScrollBar* bar = verticalScrollBar();
    const int edge = std::max(8, sizeHintForRow(0) / 2);
    if (at.y() < edge) {
      bar->setValue(bar->value() - bar->singleStep());
    } else if (at.y() > viewport()->height() - edge) {
      bar->setValue(bar->value() + bar->singleStep());
    }
  }
  if (m_dragMarker) {
    const int position = nearestBoundary(at.y());
    if (position >= 0 && position != m_dragPosition) {
      m_dragPosition = position;
      viewport()->update();
      emit m_owner.markerDragged(position);
    }
    return;
  }
  if (m_press >= 0 && (event->buttons() & Qt::LeftButton)) {
    if (!m_dragItem && (at - m_pressAt).manhattanLength() > kDragPixels) {
      m_dragItem = true;
      setCursor(Qt::ClosedHandCursor);
    }
    if (m_dragItem) {
      const int gap = nearestBoundary(at.y());
      m_dropIndex = gap < 0 ? -1 : (gap <= m_press ? gap : gap - 1);
      QString reason;
      m_dropAllowed = m_dropIndex < 0 || m_dropIndex == m_press || m_owner.canMoveItem(m_press, m_dropIndex, &reason);
      if (!m_dropAllowed) {
        QToolTip::showText(event->globalPosition().toPoint(), reason, this);
      } else {
        QToolTip::hideText();
      }
      // Not by the line's colour alone (mitcad#1041): a refused drop has
      // the "no" cursor and the reason.
      setCursor(m_dropAllowed ? Qt::ClosedHandCursor : Qt::ForbiddenCursor);
      viewport()->update();
    }
    return;
  }
  QTreeWidgetItem* row = itemAt(at);
  if (row != m_hover) {
    hoverRow(row);
  }
  if (onMarker(at)) {
    setCursor(Qt::SizeVerCursor);
  } else {
    unsetCursor();
  }
}

void TimelineHistoryList::mouseReleaseEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    return;
  }
  unsetCursor();
  if (m_dragMarker) {
    m_dragMarker = false;
    viewport()->update();
    emit m_owner.markerDragFinished(m_dragPosition);
    return;
  }
  const int pressed = m_press;
  const bool dragged = m_dragItem;
  const int index = m_dropIndex;
  m_press = -1;
  m_dragItem = false;
  m_dropIndex = -1;
  QToolTip::hideText();
  viewport()->update();
  if (pressed < 0 || pressed >= m_owner.itemCount()) {
    return;
  }
  if (dragged) {
    if (index >= 0 && index != pressed) {
      emit m_owner.reorderRequested(m_owner.itemFeature(pressed).uid, index);
    }
    return;
  }
  QTreeWidgetItem* row = itemAt(event->position().toPoint());
  if (row != nullptr && kindOf(row) == Kind::Feature && firstOf(row) == pressed) {
    m_owner.clickItem(pressed, m_shift, m_toggle);
  }
}

void TimelineHistoryList::mouseDoubleClickEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    return;
  }
  m_press = -1;
  const QPoint at = event->position().toPoint();
  if (onMarker(at)) {
    return;
  }
  QTreeWidgetItem* row = itemAt(at);
  if (row == nullptr || eyeRect(row).contains(at) || fxRect(row).contains(at)) {
    return; // a quick second click on the light bulb or the badge: nothing more
  }
  if (kindOf(row) == Kind::Group) {
    // The arrow's own press folded or opened it already.
    if (at.x() >= visualItemRect(row).left()) {
      toggleGroup(row);
    }
    return;
  }
  if (kindOf(row) == Kind::Feature) {
    emit m_owner.editRequested(uidOf(row));
  }
}

void TimelineHistoryList::contextMenuEvent(QContextMenuEvent* event) {
  QTreeWidgetItem* row = itemAt(event->pos());
  if (row == nullptr) {
    return;
  }
  switch (kindOf(row)) {
  case Kind::Group:
    emit m_owner.groupMenuRequested(row->text(0), event->globalPos());
    break;
  case Kind::Feature:
    m_owner.contextItem(firstOf(row), event->globalPos());
    break;
  case Kind::Run:
    break;
  }
}

void TimelineHistoryList::keyPressEvent(QKeyEvent* event) {
  if (event->matches(QKeySequence::Find)) {
    m_owner.openSearch(); // Ctrl+F, Cmd+F on macOS (mitcad#179)
    return;
  }
  const QStringList selected = m_owner.selectedFeatures();
  if (!selected.isEmpty() && isDeleteKey(event->key())) {
    emit m_owner.deleteRequested(selected);
    return;
  }
  if (event->key() == Qt::Key_F2 && !m_owner.selectedFeature().isEmpty()) {
    m_owner.startRename(m_owner.selectedFeature());
    return;
  }
  // No selection or current row of its own: the strip's is the one.
  event->ignore();
}

void TimelineHistoryList::leaveEvent(QEvent* event) {
  QTreeWidget::leaveEvent(event);
  if (!m_dragMarker && !m_dragItem) {
    hoverRow(nullptr);
    unsetCursor();
  }
}

bool TimelineHistoryList::viewportEvent(QEvent* event) {
  if (event->type() == QEvent::ToolTip) {
    const auto* help = static_cast<QHelpEvent*>(event);
    QTreeWidgetItem* row = itemAt(help->pos());
    if (row == nullptr || m_dragItem || m_dragMarker) {
      QToolTip::hideText();
      return true;
    }
    // The light bulb says what a click does (the Browser's delegate).
    if (eyeRect(row).contains(help->pos())) {
      return QTreeWidget::viewportEvent(event);
    }
    // The pointer on the fx badge: its bindings card (mitcad#862).
    if (const QRect fx = fxRect(row); fx.contains(help->pos())) {
      QToolTip::hideText();
      emit m_owner.bindingsRequested(uidOf(row), QRect(viewport()->mapToGlobal(fx.topLeft()), fx.size()), false);
      return true;
    }
    QString tip;
    const int first = firstOf(row);
    const bool current = first >= 0 && first < m_owner.itemCount() && m_owner.itemFeature(first).uid == uidOf(row);
    switch (kindOf(row)) {
    case Kind::Feature:
      tip = current ? m_owner.itemToolTip(first) : QString();
      break;
    case Kind::Group:
      // Folded: the strip's folded cell's tooltip; open: its name and size.
      tip = !row->isExpanded() && current
                ? m_owner.itemToolTip(first)
                : tr("<b>%1</b><br>%n feature(s)", nullptr, lastOf(row) - first + 1).arg(row->text(0).toHtmlEscaped());
      break;
    case Kind::Run:
      tip = row->toolTip(0);
      break;
    }
    if (tip.isEmpty()) {
      QToolTip::hideText();
    } else {
      QToolTip::showText(help->globalPos(), tip, viewport(), visualItemRect(row));
    }
    return true;
  }
  return QTreeWidget::viewportEvent(event);
}

void TimelineHistoryList::endGestures() {
  unsetCursor();
  m_press = -1;
  m_dragItem = false;
  m_dropIndex = -1;
  QToolTip::hideText();
  viewport()->update();
  if (m_dragMarker) {
    m_dragMarker = false;
    emit m_owner.markerDragFinished(m_dragPosition);
  }
}

bool TimelineHistoryList::event(QEvent* event) {
  if ((event->type() == QEvent::Hide || event->type() == QEvent::WindowDeactivate) && m_hover != nullptr) {
    m_hover = nullptr;
    m_owner.hoverFeature(QString());
    QToolTip::hideText();
  }
  // A modal dialog took the input (a long computation's progress) and with
  // it the release: the marker stays where it was dragged to, as the
  // strip's.
  if (event->type() == QEvent::WindowBlocked) {
    endGestures();
  }
  return QTreeWidget::event(event);
}

} // namespace mitcad
