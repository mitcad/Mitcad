// SPDX-License-Identifier: MIT
#include "FloatingLayout.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>

#include <QEvent>
#include <QShowEvent>
#include <QStringList>
#include <QMainWindow>
#include <QMargins>
#include <QMouseEvent>
#include <QSettings>
#include <QTimer>
#include <QSize>
#include <QVBoxLayout>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "ChromeStyle.hpp"
#include "ColumnShare.hpp"
#include "GlassCard.hpp"
#include "MessageBand.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"

namespace mitcad {

// ColumnShare.hpp (mitcad#1020): shared with the AppKit frontend's column.
std::vector<int> shareColumn(const std::vector<ColumnClaim>& claims, int available) {
  std::vector<int> heights(claims.size(), 0);
  std::vector<bool> placed(claims.size(), false);
  long long remaining = available;
  for (std::size_t i = 0; i < claims.size(); ++i) {
    if (claims[i].fixed) {
      heights[i] = claims[i].want;
      placed[i] = true;
      remaining -= claims[i].want;
    }
  }
  remaining = std::max(0LL, remaining);
  for (bool again = true; again;) {
    again = false;
    double weights = 0.0;
    for (std::size_t i = 0; i < claims.size(); ++i) {
      if (!placed[i]) {
        weights += claims[i].weight;
      }
    }
    if (weights <= 0.0) {
      break;
    }
    long long kept = 0;
    for (std::size_t i = 0; i < claims.size(); ++i) {
      if (!placed[i] && claims[i].want <= static_cast<double>(remaining) * claims[i].weight / weights) {
        heights[i] = claims[i].want;
        placed[i] = true;
        kept += claims[i].want;
        again = true;
      }
    }
    remaining = std::max(0LL, remaining - kept);
  }
  double weights = 0.0;
  std::size_t last = claims.size();
  for (std::size_t i = 0; i < claims.size(); ++i) {
    if (!placed[i]) {
      weights += claims[i].weight;
      last = i;
    }
  }
  long long given = 0;
  for (std::size_t i = 0; i < claims.size(); ++i) {
    if (placed[i]) {
      continue;
    }
    const long long height =
        i == last ? remaining - given
                  : static_cast<long long>(std::floor(static_cast<double>(remaining) * claims[i].weight / weights));
    heights[i] = static_cast<int>(std::max(0LL, height));
    given += heights[i];
  }
  for (std::size_t i = 0; i < claims.size(); ++i) {
    heights[i] = std::max(heights[i], claims[i].floor);
  }
  return heights;
}

namespace {

// The boundary between two left cards (mitcad#1020): the gap between them,
// which takes the mouse with a resize cursor and paints nothing (the view
// shows through it).
class SplitHandle : public QWidget {
public:
  std::function<void(int)> pressed;
  std::function<void(int)> moved;
  std::function<void()> released;
  std::function<void()> reset;

  explicit SplitHandle(QWidget* parent) : QWidget(parent) {
    setObjectName(QStringLiteral("floatingSplit"));
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
    setCursor(Qt::SizeVerCursor);
    setToolTip(QObject::tr("Drag to share the height; double-click for the automatic split"));
  }

protected:
  void mousePressEvent(QMouseEvent* event) override {
    if (event->button() != Qt::LeftButton) {
      event->ignore();
      return;
    }
    m_dragging = true;
    pressed(static_cast<int>(std::lround(event->globalPosition().y())));
    event->accept();
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    if (!m_dragging) {
      event->ignore();
      return;
    }
    moved(static_cast<int>(std::lround(event->globalPosition().y())));
    event->accept();
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (!m_dragging) {
      event->ignore();
      return;
    }
    m_dragging = false;
    released();
    event->accept();
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    if (event->button() != Qt::LeftButton) {
      event->ignore();
      return;
    }
    m_dragging = false;
    reset();
    event->accept();
  }

private:
  bool m_dragging = false;
};

} // namespace

FloatingLayout::FloatingLayout(OcctViewer* viewer, QWidget* header, QWidget* parent)
    : QWidget(parent), m_viewer(viewer), m_header(header), m_windowed(glassActive()) {
  setObjectName(QStringLiteral("floatingLayout"));
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(0);
  if (header != nullptr) {
    column->addWidget(header);
    // The cards start below it (mitcad#419): its size and place are followed
    // too, as the ribbon's groups collapse and notices come and go.
    header->installEventFilter(this);
  }
  column->addWidget(viewer, 1);
  // The view's size and place are what the cards follow.
  viewer->installEventFilter(this);
  connect(viewer, &OcctViewer::badgeWidthChanged, this, &FloatingLayout::requestLayout);
}

void FloatingLayout::adopt(GlassCard* card) {
  card->setParent(this);
  card->setWindowed(m_windowed);
  card->installEventFilter(this);
}

void FloatingLayout::addCard(GlassCard* card, Anchor anchor, const Sizing& sizing) {
  adopt(card);
  connect(card, &GlassCard::collapsedChanged, this, &FloatingLayout::requestLayout);
  // A width the user drags (mitcad#529).
  connect(card, &GlassCard::widthDragged, this, [this, card](int width) { setCardWidth(card, width); });
  m_entries.push_back({card, anchor, sizing, -1});
  card->setVisible(sizing.shown);
  if (!m_windowed) {
    card->raise();
  }
  requestLayout();
}

void FloatingLayout::watchContent(QWidget* content) { content->installEventFilter(this); }

void FloatingLayout::addTopRow(QWidget* row, RowCards cards, const QList<QWidget*>& parts) {
  row->setParent(this);
  row->installEventFilter(this);
  Row entry{row, std::move(cards), {}};
  for (QWidget* part : parts) {
    entry.parts.emplace_back(part);
    part->installEventFilter(this); // shown and hidden: the row follows
  }
  if (entry.parts.empty()) {
    row->show();
  }
  m_rows.push_back(std::move(entry));
  requestLayout();
}

std::vector<std::pair<QString, QRect>> FloatingLayout::cardsOf(const Row& row) {
  if (row.cards) {
    return row.cards();
  }
  return {{row.widget->objectName(), row.widget->rect()}};
}

int FloatingLayout::maximumContentHeight(const QWidget* content) const {
  for (const Entry& entry : m_entries) {
    if (entry.card != nullptr && entry.card->content() == content) {
      return entry.maxContent;
    }
  }
  return -1;
}

void FloatingLayout::setCardWidth(GlassCard* card, int width) {
  const int most = std::max(1, (m_viewer->width() - 2 * kMargin) * 2 / 3);
  for (Entry& entry : m_entries) {
    if (entry.card == card) {
      entry.sizing.width = std::clamp(width, 1, std::max(1, most));
      layoutCards();
      return;
    }
  }
}

QString FloatingLayout::splitText(const std::vector<std::pair<QString, int>>& split) {
  QStringList parts;
  for (const auto& [name, height] : split) {
    parts << QStringLiteral("%1 %2").arg(name).arg(height);
  }
  return parts.join(QStringLiteral(", "));
}

void FloatingLayout::setSplitKey(const QString& key) {
  m_splitKey = key;
  m_split.clear();
  const QSettings settings;
  const QStringList kept = settings.value(key).toStringList();
  for (const QString& entry : kept) {
    const qsizetype at = entry.lastIndexOf(QLatin1Char('='));
    bool ok = false;
    const int height = at > 0 ? entry.mid(at + 1).toInt(&ok) : 0;
    if (ok && height > 0) {
      m_split.emplace_back(entry.left(at), height);
    }
  }
  if (!m_split.empty()) {
    qInfo().noquote() << QStringLiteral("Floating split restored: %1").arg(splitText(m_split));
  }
  requestLayout();
}

QWidget* FloatingLayout::splitHandle(int index) {
  const auto at = static_cast<std::size_t>(index);
  if (at >= m_splitHandles.size()) {
    m_splitHandles.resize(at + 1);
  }
  if (m_splitHandles[at] == nullptr) {
    auto* handle = new SplitHandle(this);
    handle->pressed = [this, index](int y) { startSplit(index, y); };
    handle->moved = [this](int y) { moveSplit(y); };
    handle->released = [this] { endSplit(); };
    handle->reset = [this] { resetSplit(); };
    m_splitHandles[at] = handle;
  }
  return m_splitHandles[at];
}

void FloatingLayout::startSplit(int index, int globalY) {
  if (index < 0 || static_cast<std::size_t>(index) >= m_splitPairs.size()) {
    return;
  }
  SplitDrag drag;
  drag.above = m_splitPairs[static_cast<std::size_t>(index)].first;
  drag.below = m_splitPairs[static_cast<std::size_t>(index)].second;
  drag.startY = globalY;
  // The open left cards' heights as they are: the drag moves height
  // between two of them, the others keep theirs.
  for (const Entry& entry : m_entries) {
    if (entry.card != nullptr && entry.card->isShown() && entry.anchor == Anchor::TopLeft &&
        !entry.card->isCollapsed()) {
      drag.heights.emplace_back(entry.card->objectName(), entry.card->slot().height());
    }
  }
  m_drag = drag;
}

void FloatingLayout::moveSplit(int globalY) {
  if (!m_drag) {
    return;
  }
  const auto find = [this](const QString& name) {
    return std::find_if(m_drag->heights.begin(), m_drag->heights.end(),
                        [&name](const std::pair<QString, int>& kept) { return kept.first == name; });
  };
  const auto above = find(m_drag->above);
  const auto below = find(m_drag->below);
  if (above == m_drag->heights.end() || below == m_drag->heights.end()) {
    return;
  }
  const auto least = [this](const QString& name, int height) {
    for (const Entry& entry : m_entries) {
      if (entry.card != nullptr && entry.card->objectName() == name) {
        // Its header and a row, or less when it is that low already.
        return std::min(height, entry.card->heightForContent(kSplitMinimumContent));
      }
    }
    return height;
  };
  // A card's height for its content in the last layout (mitcad#1152): the
  // shares give a card whose weight reaches it exactly that height and the
  // rest to all the other cards, so a drag past it would move the other
  // boundaries instead of this one.
  const auto want = [this](const QString& name) {
    for (const Entry& entry : m_entries) {
      if (entry.card != nullptr && entry.card->objectName() == name && entry.want > 0) {
        return entry.want;
      }
    }
    return std::numeric_limits<int>::max() / 4;
  };
  const int total = above->second + below->second;
  const int lowest = std::max(least(above->first, above->second), total - want(below->first));
  const int highest = std::min(total - least(below->first, below->second), want(above->first));
  const int upper = std::clamp(above->second + globalY - m_drag->startY, std::min(lowest, above->second),
                               std::max(highest, above->second));
  m_drag->moved = m_drag->moved || globalY != m_drag->startY;
  m_split = m_drag->heights;
  for (auto& [name, height] : m_split) {
    if (name == m_drag->above) {
      height = upper;
    } else if (name == m_drag->below) {
      height = total - upper;
    }
  }
  layoutCards();
}

void FloatingLayout::endSplit() {
  if (!m_drag) {
    return;
  }
  const bool moved = m_drag->moved;
  m_drag.reset();
  if (!moved) {
    return;
  }
  if (!m_splitKey.isEmpty()) {
    QStringList kept;
    for (const auto& [name, height] : m_split) {
      kept << QStringLiteral("%1=%2").arg(name).arg(height);
    }
    QSettings settings;
    settings.setValue(m_splitKey, kept);
  }
  qInfo().noquote() << QStringLiteral("Floating split remembered: %1").arg(splitText(m_split));
}

void FloatingLayout::resetSplit() {
  m_drag.reset();
  m_split.clear();
  if (!m_splitKey.isEmpty()) {
    QSettings settings;
    settings.remove(m_splitKey);
  }
  qInfo().noquote() << QStringLiteral("Floating split: automatic");
  layoutCards();
}

void FloatingLayout::showOnlyCards(const QStringList& names) {
  for (const Entry& entry : m_entries) {
    if (entry.card != nullptr) {
      entry.card->setSuppressed(!names.isEmpty() && !names.contains(entry.card->objectName()));
    }
  }
  requestLayout();
}

void FloatingLayout::requestLayout() {
  if (m_layoutQueued || m_inLayout) {
    return;
  }
  m_layoutQueued = true;
  // Watched (the UI tests' sync waits for it): what the layout moves, the
  // cards and the pick places logged clear of them (mitcad#1086), is in the
  // log before a test reads it.
  TestSync::singleShot(0, this, [this] {
    m_layoutQueued = false;
    layoutCards();
  });
}

void FloatingLayout::layoutNow() { layoutCards(); }

void FloatingLayout::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  if (!m_windowed) {
    return;
  }
  // The cards' windows follow the window that they are children of.
  if (m_hostWindow.isNull()) {
    m_hostWindow = window();
    m_hostWindow->installEventFilter(this);
  }
  for (const Entry& entry : m_entries) {
    if (entry.card != nullptr) {
      entry.card->hostVisibilityChanged(true);
    }
  }
  if (m_band != nullptr) {
    m_band->hostVisibilityChanged(true);
  }
  requestLayout();
}

void FloatingLayout::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  if (!m_windowed || window()->isVisible()) {
    return; // hidden by a page change, not with the window
  }
  for (const Entry& entry : m_entries) {
    if (entry.card != nullptr) {
      entry.card->hostVisibilityChanged(false);
    }
  }
  if (m_band != nullptr) {
    m_band->hostVisibilityChanged(false);
  }
}

bool FloatingLayout::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_hostWindow.data()) {
    // The window moved, resized or changed (full screen): the cards' windows
    // are placed again, in screen coordinates.
    if (event->type() == QEvent::Move || event->type() == QEvent::Resize ||
        event->type() == QEvent::WindowStateChange) {
      requestLayout();
    }
    return false;
  }
  switch (event->type()) {
  case QEvent::Resize:
  case QEvent::Move:
    if (watched == m_viewer) {
      layoutCards();
    } else if (watched == m_header || qobject_cast<GlassCard*>(watched) != nullptr) {
      // A native card can grow to its layout's minimum after a content or
      // font change. Stack its neighbours against the actual new bounds.
      requestLayout();
    }
    break;
  case QEvent::Show:
  case QEvent::Hide:
  case QEvent::ShowToParent: // a top row's part, also while the row is hidden
  case QEvent::HideToParent:
  case QEvent::LayoutRequest:
    if (watched != m_viewer) {
      requestLayout();
    }
    break;
  default:
    break;
  }
  return false;
}

void FloatingLayout::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  // The view is resized by the layout, which comes after this event.
  requestLayout();
}

void FloatingLayout::setMessageBand(MessageBand* band) {
  m_band = new GlassCard(this);
  m_band->setObjectName(QStringLiteral("messageBandCard"));
  m_band->setWindowed(m_windowed);
  m_band->setContent(band, kBandMargins);
  m_bandContent = band;
  m_band->hide();
  m_band->installEventFilter(this);
  connect(band, &MessageBand::layoutChanged, this, &FloatingLayout::updateBand);
  updateBand();
}

void FloatingLayout::updateBand() {
  if (m_band == nullptr) {
    return;
  }
  const bool wanted = m_bandContent->hasMessage();
  if (wanted != m_band->isShown()) {
    m_band->setVisible(wanted);
  }
  // Dimmed while it is the stub of the latest message.
  m_band->setOpacityLevel(m_bandContent->isStub() ? kStubOpacity : 1.0);
  if (wanted && !m_windowed) {
    m_band->raise();
  }
  requestLayout();
}

void FloatingLayout::layoutCards() {
  if (m_inLayout) {
    return;
  }
  m_inLayout = true;
  const QRect view = m_viewer->geometry();
  const auto visible = [](const Entry& entry) { return entry.card != nullptr && entry.card->isShown(); };
  const auto sized = [&](const Entry& entry, int width, int limit) {
    GlassCard* card = entry.card;
    const int hint = entry.sizing.contentHeight ? entry.sizing.contentHeight()
                                                : (card->content() != nullptr ? card->content()->sizeHint().height() : 0);
    const int height = std::min(card->heightForContent(hint), std::max(limit, card->heightForContent(0)));
    return QSize(width, height);
  };

  // The bottom card first: the others stay above it. `floor` is the
  // lowest free y (exclusive) for the cards and the message band.
  int floor = view.bottom() + 1 - kMargin;
  bool hasBottom = false;
  int bottomEdge = 0; // the top of the lowest bottom card
  QRect topBottom;    // the highest bottom card (the timeline)
  for (const Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::Bottom) {
      continue;
    }
    const int width = entry.sizing.width > 0 ? entry.sizing.width : view.width() - 2 * kMargin;
    const QSize size = sized(entry, width, view.height() / 2);
    entry.card->setSlot(
        QRect(view.left() + (view.width() - size.width()) / 2, floor - size.height(), size.width(), size.height()));
    floor = entry.card->slot().y() - kGap;
    hasBottom = true;
    bottomEdge = entry.card->slot().y() - kGap;
    topBottom = entry.card->slot();
  }
  // The message band (mitcad#1042) right above the timeline, as wide as it
  // and at least as high, room for two lines of text; the other cards stay
  // above it. Its stub keeps the band's room (the cards above do not move
  // as messages come and fade), one line high and as wide as its text,
  // centred on the timeline.
  if (m_band != nullptr && m_band->isShown()) {
    const int left = topBottom.isValid() ? topBottom.left() : view.left() + kMargin;
    const int width = topBottom.isValid() ? topBottom.width() : view.width() - 2 * kMargin;
    m_bandContent->setAvailableWidth(width - kBandMargins.left() - kBandMargins.right());
    const int row = std::max(topBottom.isValid() ? topBottom.height() : 0,
                             m_band->heightForContent(m_bandContent->fullHeight()));
    if (m_bandContent->isStub()) {
      const int high = m_band->heightForContent(m_bandContent->sizeHint().height());
      const int wide = std::min(width, m_band->sizeHint().width());
      m_band->setSlot(QRect(left + (width - wide) / 2, floor - high, wide, high));
    } else {
      m_band->setSlot(QRect(left, floor - row, width, row));
    }
    m_bandRow = QRect(left, floor - row, width, row);
    floor -= row + kGap;
    hasBottom = true;
    bottomEdge = floor;
    const QString placed = QStringLiteral("Message band placed: %1 %2,%3 %4x%5, row %6, timeline %7,%8 %9x%10")
                               .arg(m_bandContent->isStub() ? QStringLiteral("stub") : QStringLiteral("full"))
                               .arg(m_band->slot().x())
                               .arg(m_band->slot().y())
                               .arg(m_band->slot().width())
                               .arg(m_band->slot().height())
                               .arg(row)
                               .arg(topBottom.x())
                               .arg(topBottom.y())
                               .arg(topBottom.width())
                               .arg(topBottom.height());
    if (placed != m_loggedBand && window() != nullptr && window()->isVisible()) {
      m_loggedBand = placed;
      qDebug().noquote() << placed;
    }
  }
  const int cardsBottom = floor;

  // The top bar above the view (the title bar row, the workspace switch, the
  // ribbon's groups, the notices; mitcad#419): no card reaches under it,
  // whatever the view's top says, at any window size and however the
  // ribbon's groups collapse.
  int barBottom = view.top();
  if (m_header != nullptr && !m_header->isHidden()) {
    barBottom = std::max(barBottom, m_header->geometry().bottom() + 1);
  }
  // The rows floating at the top of the view (mitcad#425): one under the
  // other, as wide as the view less the margins; the other cards and the
  // orientation cube below the last one.
  int rowTop = barBottom + kMargin;
  int rowsBottom = -1; // none shown
  std::vector<QRect> rowCards; // in the view's coordinates
  for (const Row& row : m_rows) {
    QWidget* widget = row.widget;
    if (widget == nullptr) {
      continue;
    }
    if (!row.parts.empty()) {
      const bool any = std::any_of(row.parts.begin(), row.parts.end(),
                                   [](const QPointer<QWidget>& part) { return part != nullptr && !part->isHidden(); });
      if (widget->isHidden() == any) {
        widget->setVisible(any);
      }
    }
    if (widget->isHidden()) {
      continue;
    }
    // The height it asks for, within its minimum and maximum: a fixed
    // height is not in a layout's size hint (mitcad#429: the title row's
    // card had its layout's hint, lower than the row's fixed height, and cut
    // the row's controls at the bottom).
    const auto hinted = [](const QWidget* hinting) {
      return std::clamp(hinting->sizeHint().height(), hinting->minimumHeight(), hinting->maximumHeight());
    };
    auto* card = qobject_cast<GlassCard*>(widget);
    const int height = card != nullptr && card->content() != nullptr ? card->heightForContent(hinted(card->content()))
                                                                     : hinted(widget);
    const QRect slot(view.left() + kMargin, rowTop, std::max(0, view.width() - 2 * kMargin), height);
    if (card != nullptr) {
      card->setSlot(slot);
    } else {
      widget->setGeometry(slot);
    }
    if (!m_windowed) {
      widget->raise();
    }
    rowTop = slot.bottom() + 1 + kGap;
    rowsBottom = slot.bottom() + 1;
    for (const auto& named : cardsOf(row)) {
      rowCards.push_back(named.second.translated(slot.topLeft() - view.topLeft()));
    }
  }
  // The cards on the left, then the free area for the view's overlays: the
  // cube's corner, below the rows and pushed down under a card that reaches
  // it.
  const int topLimit = rowsBottom >= 0 ? rowsBottom + kGap : barBottom + kMargin;
  int insetTop = rowsBottom >= 0 ? rowsBottom + kGap - view.top() : 0;
  // The panels on the right edge (the chat, mitcad#947): as high as the
  // room from the rows to the bottom card; the upper right cards and the
  // orientation cube left of them.
  int rightEdge = view.right() + 1 - kMargin;
  for (Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::Right) {
      continue;
    }
    const int width = std::min(entry.sizing.width > 0 ? entry.sizing.width : 340,
                               std::max(1, (view.width() - 2 * kMargin) * 2 / 3));
    const int height = std::max(entry.card->heightForContent(0), cardsBottom - topLimit);
    entry.card->setSlot(QRect(rightEdge - width, topLimit, width, height));
    entry.maxContent = std::max(0, height - entry.card->heightForContent(0));
    rightEdge -= width + kGap;
  }
  // How far the cube and the upper right cards move left (0: none).
  const int rightPanels = view.right() + 1 - kMargin - rightEdge;
  const int cubeShift = rightPanels > 0 ? kMargin + rightPanels : 0;
  const QRect cubeCorner(view.right() + 1 - cubeShift - OcctViewer::kCubeArea, view.top() + insetTop,
                         OcctViewer::kCubeArea, OcctViewer::kCubeArea);
  // The upper left cards (Projects, Configuration, Browser) stack in the
  // order they were added and share the column from the rows down to the
  // bottom card (mitcad#1020): each at most its content's height, the
  // rest shared by the user's split or evenly.
  std::vector<Entry*> left;
  for (Entry& entry : m_entries) {
    if (visible(entry) && entry.anchor == Anchor::TopLeft) {
      left.push_back(&entry);
    }
  }
  const int column = std::max(0, cardsBottom - topLimit - kGap * std::max(0, static_cast<int>(left.size()) - 1));
  std::vector<ColumnClaim> claims;
  int open = 0;
  for (const Entry* entry : left) {
    GlassCard* card = entry->card;
    ColumnClaim claim;
    // The column decides a left card's height (mitcad#1020): a card in a
    // native tool window (glassActive) would otherwise keep its layout's
    // minimum whatever its slot, and push the cards below it out of a short
    // window (a 2x macOS one). Its share still keeps that minimum while
    // the column has room for it (its toolbar and a row of its content);
    // when it has not, the lower cards give way (below).
    QLayout* layout = card->layout();
    if (layout != nullptr && layout->sizeConstraint() != QLayout::SetNoConstraint) {
      layout->setSizeConstraint(QLayout::SetNoConstraint);
      card->setMinimumSize(0, 0);
    }
    if (layout != nullptr && !card->isCollapsed()) {
      layout->activate();
    }
    claim.floor = std::max(card->heightForContent(0),
                           layout != nullptr && !card->isCollapsed() ? layout->minimumSize().height() : 0);
    claim.fixed = card->isCollapsed();
    const int share = static_cast<int>((view.height() - 2 * kMargin) * entry->sizing.maxHeightShare);
    const int hint = entry->sizing.contentHeight
                         ? entry->sizing.contentHeight()
                         : (card->content() != nullptr ? card->content()->sizeHint().height() : 0);
    claim.want = std::max(claim.floor, std::min(card->heightForContent(hint), std::max(share, claim.floor)));
    open += claim.fixed ? 0 : 1;
    claims.push_back(claim);
  }
  if (!m_split.empty()) {
    // The user's heights are the shares' weights; a card shown since gets
    // an even share of the column.
    const double even = static_cast<double>(std::max(1, column)) / std::max(1, open);
    for (std::size_t i = 0; i < left.size(); ++i) {
      const QString name = left[i]->card->objectName();
      const auto found = std::find_if(m_split.begin(), m_split.end(),
                                      [&name](const std::pair<QString, int>& kept) { return kept.first == name; });
      claims[i].weight = found != m_split.end() ? std::max(1, found->second) : even;
    }
  }
  const std::vector<int> heights = shareColumn(claims, column);
  int leftTop = topLimit;
  for (std::size_t i = 0; i < left.size(); ++i) {
    Entry& entry = *left[i];
    // A width the user chose (mitcad#529) within the view.
    const int width = std::min(entry.sizing.width > 0 ? entry.sizing.width : 260,
                               std::max(1, view.width() - 2 * kMargin));
    // Within the column, above the card below it (the timeline): when the
    // cards' least heights do not fit, the lower ones give way, as before
    // the column was shared; a card never ends below the column.
    const int height = std::max(1, std::min(heights[i], cardsBottom - leftTop));
    entry.card->setSlot(QRect(view.left() + kMargin, leftTop, width, height));
    entry.want = claims[i].want; // where a split drag stops (mitcad#1152)
    // Its content at its largest: the height it would get wanting all of
    // the column (mitcad#426: the Browser folds a design opened whose rows
    // would not fit it).
    std::vector<ColumnClaim> greedy = claims;
    greedy[i].fixed = false;
    greedy[i].want = std::numeric_limits<int>::max() / 4;
    entry.maxContent = std::max(0, shareColumn(greedy, column)[i] - entry.card->heightForContent(0));
    leftTop = entry.card->slot().bottom() + 1 + kGap;
    if (entry.card->slot().intersects(cubeCorner)) {
      insetTop = std::max(insetTop, entry.card->slot().bottom() + 1 + kGap - view.top());
    }
  }
  // The boundaries between open neighbours: a handle in each gap.
  m_splitPairs.clear();
  for (std::size_t i = 0; i + 1 < left.size(); ++i) {
    GlassCard* above = left[i]->card;
    GlassCard* below = left[i + 1]->card;
    if (above->isCollapsed() || below->isCollapsed()) {
      continue;
    }
    const QRect top = above->slot();
    const QRect bottom = below->slot();
    QWidget* handle = splitHandle(static_cast<int>(m_splitPairs.size()));
    handle->setGeometry(top.left(), top.bottom() + 1, std::max(top.width(), bottom.width()),
                        std::max(1, bottom.top() - top.bottom() - 1));
    handle->show();
    if (!m_windowed) {
      handle->raise();
    }
    m_splitPairs.emplace_back(above->objectName(), below->objectName());
  }
  for (std::size_t i = m_splitPairs.size(); i < m_splitHandles.size(); ++i) {
    if (m_splitHandles[i] != nullptr) {
      m_splitHandles[i]->hide();
    }
  }
  int cubeRight = cubeShift; // how far the cube moves left of a tall card on the right
  for (Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::TopRight) {
      continue;
    }
    // Under the cube, and its arrows; below the top bar in any case. A card
    // taller than the room under the cube (mitcad#655: 3D Print's panel)
    // grows up to the top first, the cube moving to its left, before it
    // scrolls.
    const int width = entry.sizing.width > 0 ? entry.sizing.width : 300;
    const int share = static_cast<int>((view.height() - 2 * kMargin) * entry.sizing.maxHeightShare);
    int top = std::max(view.top() + insetTop + OcctViewer::kCubeArea, topLimit);
    int limit = std::min(share, cardsBottom - top);
    QSize size = sized(entry, width, limit);
    const int tallLimit = std::min(share, cardsBottom - topLimit);
    const QSize tall = sized(entry, width, tallLimit);
    if (top > topLimit && tall.height() > size.height()) {
      top = topLimit;
      limit = tallLimit;
      size = tall;
      cubeRight = std::max(cubeRight, kMargin + rightPanels + size.width() + kGap);
    }
    entry.card->setSlot(QRect(rightEdge - size.width(), top, size.width(), size.height()));
    entry.maxContent = std::max(0, limit - entry.card->heightForContent(0));
  }
  // The bottom centre cards (the navigation bar, mitcad#1086): just above
  // the message band's row (mitcad#1042) or, without it, the bottom card,
  // in the middle of the free width between the side cards and the
  // sketch's badge that reach into their row.
  for (Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::BottomCenter) {
      continue;
    }
    GlassCard* card = entry.card;
    const int hint = entry.sizing.contentHeight
                         ? entry.sizing.contentHeight()
                         : (card->content() != nullptr ? card->content()->sizeHint().height() : 0);
    const int height = card->heightForContent(hint);
    const int width = std::min(entry.sizing.width > 0 ? entry.sizing.width : card->sizeHint().width(),
                               std::max(1, view.width() - 2 * kMargin));
    const int top = floor - height;
    int freeLeft = view.left() + kMargin;
    int freeRight = view.right() + 1 - kMargin;
    if (m_viewer->badgeWidth() > 0) { // the sketch's badge, at the bottom left
      freeLeft = std::max(freeLeft, view.left() + m_viewer->badgeWidth() + kGap);
    }
    for (const Entry& other : m_entries) {
      if (!visible(other) || other.anchor == Anchor::Bottom || other.anchor == Anchor::BottomCenter) {
        continue;
      }
      const QRect slot = other.card->slot();
      if (slot.bottom() < top - kGap || slot.top() > floor) {
        continue; // not in its row
      }
      if (other.anchor == Anchor::TopLeft) {
        freeLeft = std::max(freeLeft, slot.right() + 1 + kGap);
      } else {
        freeRight = std::min(freeRight, slot.left() - kGap);
      }
    }
    // Centred in the free width; where that is narrower than the card, at
    // its left end.
    const int x = freeLeft + std::max(0, (freeRight - freeLeft - width) / 2);
    card->setSlot(QRect(x, top, width, height));
    floor = top - kGap;
  }
  if (!m_windowed) { // the child windows keep their order
    for (const Entry& entry : m_entries) {
      if (visible(entry)) {
        entry.card->raise();
      }
    }
    if (m_band != nullptr && !m_band->isHidden()) {
      m_band->raise();
    }
  }
  m_viewer->setOverlayInsets(QMargins(0, insetTop, cubeRight, hasBottom ? view.bottom() + 1 - bottomEdge : 0));
  std::vector<QRect> covered;
  for (const Entry& entry : m_entries) {
    if (visible(entry)) {
      covered.push_back(entry.card->slot().translated(-view.topLeft()));
    }
  }
  if (m_band != nullptr && m_band->isShown()) {
    covered.push_back(m_band->slot().translated(-view.topLeft()));
  }
  m_viewer->setCoveredRects(covered);
  // The rows' cards: the free area starts below them, and the view picks
  // between them.
  m_viewer->setTopCards(rowCards, rowsBottom >= 0 ? rowsBottom - view.top() : 0);
  m_inLayout = false;
  logCards();
}

void FloatingLayout::logCards() {
  QWidget* host = window();
  if (host == nullptr || !host->isVisible()) {
    return;
  }
  // The bottom of the bar above the cards in the window: the menu bar (none
  // on macOS) and the header, as they are placed.
  int barBottom = 0;
  if (const auto* mainWindow = qobject_cast<const QMainWindow*>(host)) {
    if (mainWindow->menuWidget() != nullptr && mainWindow->menuWidget()->isVisible()) {
      barBottom = mainWindow->menuWidget()->geometry().bottom() + 1;
    }
  }
  if (m_header != nullptr && m_header->isVisible()) {
    barBottom = std::max(barBottom, m_header->mapTo(host, QPoint(0, m_header->height())).y());
  }
  const auto entryText = [](const QString& name, const QPoint& at, const QSize& size) {
    return QStringLiteral("%1 %2,%3 %4x%5").arg(name).arg(at.x()).arg(at.y()).arg(size.width()).arg(size.height());
  };
  // The rows' cards first (mitcad#425), and the bottom of the lowest.
  QStringList cards;
  int rowsBottom = barBottom;
  QString gap;
  for (const Row& row : m_rows) {
    if (row.widget == nullptr || !row.widget->isVisible()) {
      continue;
    }
    const auto named = cardsOf(row);
    if (row.cards) {
      // A row of cards of its own (the title row, the ribbon), then its
      // cards.
      const QPoint at = GlassCard::mapToHost(row.widget.data(), QPoint(0, 0));
      cards << entryText(row.widget->objectName(), at, row.widget->size());
      rowsBottom = std::max(rowsBottom, at.y() + row.widget->height());
    }
    for (const auto& [name, rect] : named) {
      const QPoint at = GlassCard::mapToHost(row.widget.data(), rect.topLeft());
      cards << entryText(name, at, rect.size());
      rowsBottom = std::max(rowsBottom, at.y() + rect.height());
    }
    if (named.size() >= 2 && named[1].second.left() - named[0].second.right() > 2) {
      // A click here reaches the view, between two cards: of the lowest row
      // that has a gap (the group cards, below the title row).
      const QRect& first = named[0].second;
      const QPoint at = GlassCard::mapToHost(
          row.widget.data(), QPoint((first.right() + 1 + named[1].second.left()) / 2, first.center().y()));
      gap = QStringLiteral("Floating view between the cards at %1,%2").arg(at.x()).arg(at.y());
    }
  }
  for (const Entry& entry : m_entries) {
    if (entry.card == nullptr || !entry.card->isShown()) {
      continue;
    }
    cards << entryText(entry.card->objectName(), GlassCard::mapToHost(entry.card.data(), QPoint(0, 0)),
                       entry.card->size());
  }
  if (m_band != nullptr && m_band->isShown()) {
    cards << entryText(m_band->objectName(), GlassCard::mapToHost(m_band, QPoint(0, 0)), m_band->size());
    // The row it keeps (its stub is smaller): the other cards end above it.
    cards << entryText(QStringLiteral("messageBandRow"), mapTo(host, m_bandRow.topLeft()), m_bandRow.size());
  }
  const QString line = QStringLiteral("Floating cards: window %1x%2, top bar bottom %3, view top %4, floating rows "
                                      "bottom %5; %6")
                           .arg(host->width())
                           .arg(host->height())
                           .arg(barBottom)
                           .arg(m_viewer->mapTo(host, QPoint(0, 0)).y())
                           .arg(rowsBottom)
                           .arg(cards.join(QStringLiteral("; ")));
  if (line != m_loggedCards) {
    m_loggedCards = line;
    qDebug().noquote() << line;
  }
  if (!gap.isEmpty() && gap != m_loggedGap) {
    m_loggedGap = gap;
    qDebug().noquote() << gap;
  }
  // The left column's boundaries (mitcad#1020), where a drag starts:
  // "Floating split configurationDock/browserDock at x,y".
  QStringList splits;
  for (std::size_t i = 0; i < m_splitPairs.size() && i < m_splitHandles.size(); ++i) {
    const QWidget* handle = m_splitHandles[i];
    if (handle == nullptr || handle->isHidden()) {
      continue;
    }
    const QPoint at = GlassCard::mapToHost(handle, handle->rect().center());
    splits << QStringLiteral("Floating split %1/%2 at %3,%4")
                  .arg(m_splitPairs[i].first, m_splitPairs[i].second)
                  .arg(at.x())
                  .arg(at.y());
  }
  const QString splitLines = splits.join(QLatin1Char('\n'));
  if (splitLines != m_loggedSplits) {
    m_loggedSplits = splitLines;
    for (const QString& split : std::as_const(splits)) {
      qDebug().noquote() << split;
    }
  }
}

} // namespace mitcad
