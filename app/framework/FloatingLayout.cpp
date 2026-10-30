// SPDX-License-Identifier: MIT
#include "FloatingLayout.hpp"

#include <algorithm>
#include <utility>

#include <QEvent>
#include <QShowEvent>
#include <QStringList>
#include <QFontMetrics>
#include <QLabel>
#include <QMainWindow>
#include <QMargins>
#include <QTimer>
#include <QVBoxLayout>
#include <QVariantAnimation>

#include "../OcctViewer.hpp"
#include "ChromeStyle.hpp"
#include "GlassCard.hpp"
#include "Theme.hpp"

namespace mitcad {

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

void FloatingLayout::requestLayout() {
  if (m_layoutQueued || m_inLayout) {
    return;
  }
  m_layoutQueued = true;
  QTimer::singleShot(0, this, [this] {
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
  if (m_pill != nullptr) {
    m_pill->hostVisibilityChanged(true);
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
  if (m_pill != nullptr) {
    m_pill->hostVisibilityChanged(false);
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

GlassCard* FloatingLayout::ensurePill() {
  if (m_pill != nullptr) {
    return m_pill;
  }
  m_pill = new GlassCard(this);
  m_pill->setObjectName(QStringLiteral("statusPill"));
  m_pill->setCapsule(true);
  m_pill->setAttribute(Qt::WA_TransparentForMouseEvents);
  m_pill->setWindowed(m_windowed);
  m_pillLabel = new QLabel;
  m_pillLabel->setObjectName(QStringLiteral("statusPillText"));
  m_pill->setContent(m_pillLabel, QMargins(kPillPadding, 6, kPillPadding, 6));
  m_pill->hide();
  m_pill->installEventFilter(this);
  m_pillTimer = new QTimer(this);
  m_pillTimer->setSingleShot(true);
  m_pillFade = new QVariantAnimation(this);
  m_pillFade->setDuration(400);
  m_pillFade->setStartValue(1.0);
  m_pillFade->setEndValue(0.0);
  connect(m_pillFade, &QVariantAnimation::valueChanged, this,
          [this](const QVariant& value) { m_pill->setOpacityLevel(value.toDouble()); });
  connect(m_pillFade, &QVariantAnimation::finished, this, [this] {
    m_pill->hide();
    m_pill->setOpacityLevel(1.0);
  });
  connect(m_pillTimer, &QTimer::timeout, m_pillFade, [this] { m_pillFade->start(); });
  return m_pill;
}

void FloatingLayout::showStatus(const QString& text, bool error, int milliseconds) {
  if (text.isEmpty()) {
    if (m_pill != nullptr) {
      m_pillTimer->stop();
      m_pillFade->stop();
      m_pill->hide();
    }
    return;
  }
  GlassCard* pill = ensurePill();
  m_pillTimer->stop();
  m_pillFade->stop();
  pill->setOpacityLevel(1.0);
  if (error) {
    setErrorStyleSheet(m_pillLabel);
  } else {
    setErrorStyleSheet(m_pillLabel, QString());
  }
  m_pillText = text;
  m_pillLabel->setToolTip(text);
  elidePill();
  pill->show();
  if (!m_windowed) {
    pill->raise();
  }
  m_pillTimer->start(milliseconds);
  layoutCards();
}

void FloatingLayout::elidePill(int room) {
  // One line: a long message is cut in the middle, the full text is in the
  // tooltip and the log.
  if (room <= 0) {
    room = std::max(120, m_viewer->width() - 2 * kMargin - 2 * kPillPadding);
  }
  m_pillLabel->setText(m_pillLabel->fontMetrics().elidedText(m_pillText, Qt::ElideMiddle, room));
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
  // lowest free y (exclusive) for the cards and the status pill.
  int floor = view.bottom() + 1 - kMargin;
  bool hasBottom = false;
  int bottomEdge = 0; // the top of the lowest bottom card
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
  // Several upper left cards (the Projects card and the Browser) stack in
  // the order they were added.
  int leftTop = topLimit;
  for (Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::TopLeft) {
      continue;
    }
    const int limit = std::min(static_cast<int>((view.height() - 2 * kMargin) * entry.sizing.maxHeightShare),
                               cardsBottom - leftTop);
    // A width the user chose (mitcad#529) within the view.
    const int width = std::min(entry.sizing.width > 0 ? entry.sizing.width : 260,
                               std::max(1, view.width() - 2 * kMargin));
    const QSize size = sized(entry, width, limit);
    entry.card->setSlot(QRect(view.left() + kMargin, leftTop, size.width(), size.height()));
    entry.maxContent = std::max(0, limit - entry.card->heightForContent(0));
    leftTop = entry.card->slot().bottom() + 1 + kGap;
    if (entry.card->slot().intersects(cubeCorner)) {
      insetTop = std::max(insetTop, entry.card->slot().bottom() + 1 + kGap - view.top());
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
  if (m_pill != nullptr && m_pill->isShown()) {
    // Bottom centre of the free area between the side cards, above the
    // timeline; shrunk (and its text elided) where that is narrow, never over
    // a card.
    const int pillHeight = m_pill->sizeHint().height();
    const int pillTop = floor - pillHeight;
    int freeLeft = view.left() + kMargin;
    int freeRight = view.right() + 1 - kMargin;
    if (m_viewer->badgeWidth() > 0) { // the sketch's badge, at the bottom left
      freeLeft = std::max(freeLeft, view.left() + m_viewer->badgeWidth() + kGap);
    }
    for (const Entry& entry : m_entries) {
      if (!visible(entry) || entry.anchor == Anchor::Bottom) {
        continue;
      }
      const QRect slot = entry.card->slot();
      if (slot.bottom() < pillTop - kGap || slot.top() > floor) {
        continue; // not in the pill's row
      }
      if (entry.anchor == Anchor::TopLeft) {
        freeLeft = std::max(freeLeft, slot.right() + 1 + kGap);
      } else {
        freeRight = std::min(freeRight, slot.left() - kGap);
      }
    }
    const int freeWidth = std::max(60, freeRight - freeLeft);
    elidePill(freeWidth - 2 * kPillPadding);
    const int width = std::min(m_pill->sizeHint().width(), freeWidth);
    m_pill->setSlot(QRect(freeLeft + (freeWidth - width) / 2, pillTop, width, pillHeight));
  }
  if (!m_windowed) { // the child windows keep their order
    for (const Entry& entry : m_entries) {
      if (visible(entry)) {
        entry.card->raise();
      }
    }
    if (m_pill != nullptr && !m_pill->isHidden()) {
      m_pill->raise();
    }
  }
  m_viewer->setOverlayInsets(QMargins(0, insetTop, cubeRight, hasBottom ? view.bottom() + 1 - bottomEdge : 0));
  std::vector<QRect> covered;
  for (const Entry& entry : m_entries) {
    if (visible(entry)) {
      covered.push_back(entry.card->slot().translated(-view.topLeft()));
    }
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
}

} // namespace mitcad
