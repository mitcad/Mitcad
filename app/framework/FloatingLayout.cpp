// SPDX-License-Identifier: MIT
#include "FloatingLayout.hpp"

#include <algorithm>

#include <QEvent>
#include <QShowEvent>
#include <QFontMetrics>
#include <QLabel>
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
  m_entries.push_back({card, anchor, sizing});
  card->setVisible(sizing.shown);
  if (!m_windowed) {
    card->raise();
  }
  requestLayout();
}

void FloatingLayout::watchContent(QWidget* content) { content->installEventFilter(this); }

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
    }
    break;
  case QEvent::Show:
  case QEvent::Hide:
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

  // The cards on the left, then the free area for the view's overlays: the
  // cube's corner, pushed down under a card that reaches it.
  const int topLimit = view.top() + kMargin;
  int insetTop = 0;
  const QRect cubeCorner(view.right() + 1 - OcctViewer::kCubeArea, view.top(), OcctViewer::kCubeArea,
                         OcctViewer::kCubeArea);
  for (const Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::TopLeft) {
      continue;
    }
    const int limit = std::min(static_cast<int>((view.height() - 2 * kMargin) * entry.sizing.maxHeightShare),
                               cardsBottom - topLimit);
    const QSize size = sized(entry, entry.sizing.width > 0 ? entry.sizing.width : 260, limit);
    entry.card->setSlot(QRect(view.left() + kMargin, topLimit, size.width(), size.height()));
    if (entry.card->slot().intersects(cubeCorner)) {
      insetTop = std::max(insetTop, entry.card->slot().bottom() + 1 + kGap - view.top());
    }
  }
  for (const Entry& entry : m_entries) {
    if (!visible(entry) || entry.anchor != Anchor::TopRight) {
      continue;
    }
    // Under the cube, and its arrows.
    const int top = view.top() + insetTop + OcctViewer::kCubeArea;
    const int limit = std::min(static_cast<int>((view.height() - 2 * kMargin) * entry.sizing.maxHeightShare),
                               cardsBottom - top);
    const QSize size = sized(entry, entry.sizing.width > 0 ? entry.sizing.width : 300, limit);
    entry.card->setSlot(QRect(view.right() + 1 - kMargin - size.width(), top, size.width(), size.height()));
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
      if (!visible(entry) || (entry.anchor != Anchor::TopLeft && entry.anchor != Anchor::TopRight)) {
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
  m_viewer->setOverlayInsets(QMargins(0, insetTop, 0, hasBottom ? view.bottom() + 1 - bottomEdge : 0));
  std::vector<QRect> covered;
  for (const Entry& entry : m_entries) {
    if (visible(entry)) {
      covered.push_back(entry.card->slot().translated(-view.topLeft()));
    }
  }
  m_viewer->setCoveredRects(covered);
  m_inLayout = false;
}

} // namespace mitcad
