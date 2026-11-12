// SPDX-License-Identifier: MIT
// The list of what lies under a press held still on the 3D view
// (mitcad#1703): the widget itself; what it lists and what a row does are
// the view's (view/PickListView.cpp).
#include "PickList.hpp"

#include <algorithm>
#include <optional>
#include <utility>

#include <QApplication>
#include <QCoreApplication>
#include <QFontMetrics>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QWheelEvent>

#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

constexpr int kMargin = 4;    // the frame's inner margin
constexpr int kPadding = 10;  // the text's room at a row's sides
constexpr int kGap = 12;      // between the pointer and the list
constexpr int kMaxWidth = 420;

} // namespace

PickList::PickList(QWidget* view) : QWidget(view) {
  setObjectName(QStringLiteral("pickList"));
  setMouseTracking(true);
  setFocusPolicy(Qt::NoFocus);
  setAccessibleName(QCoreApplication::translate("Selection", "Items under the pointer"));
  hide();
}

PickList::~PickList() {
  if (m_filtering) {
    qApp->removeEventFilter(this);
  }
}

int PickList::rowHeight() const { return QFontMetrics(font()).height() + 8; }

void PickList::open(const QStringList& rows, const QPoint& pointer, const QRect& area,
                    const std::vector<QRect>& covered) {
  m_rows = rows;
  m_first = 0;
  m_hovered = -1;
  m_pressed = -1;
  const int step = rowHeight();
  const int fitting = std::max(1, (area.height() - 2 * kMargin) / step);
  m_shown = std::min(rowCount(), fitting);
  const QFontMetrics metrics(font());
  int textWidth = 0;
  for (const QString& row : std::as_const(m_rows)) {
    textWidth = std::max(textWidth, metrics.horizontalAdvance(row));
  }
  const QSize extent(std::min(textWidth + 2 * (kPadding + kMargin), std::min(kMaxWidth, std::max(120, area.width()))),
                   m_shown * step + 2 * kMargin);
  // Below and to the right of the pointer, else on another side: the first
  // inside the area and clear of the cards, else the first inside the area.
  const QPoint corners[] = {QPoint(pointer.x() + kGap, pointer.y() + kGap),
                            QPoint(pointer.x() - kGap - extent.width(), pointer.y() + kGap),
                            QPoint(pointer.x() + kGap, pointer.y() - kGap - extent.height()),
                            QPoint(pointer.x() - kGap - extent.width(), pointer.y() - kGap - extent.height())};
  const auto clear = [&covered](const QRect& place) {
    return std::none_of(covered.begin(), covered.end(), [&place](const QRect& card) { return card.intersects(place); });
  };
  std::optional<QRect> chosen;
  for (const QPoint& corner : corners) {
    const QRect place(corner, extent);
    if (area.contains(place) && clear(place)) {
      chosen = place;
      break;
    }
  }
  for (const QPoint& corner : corners) {
    const QRect place(corner, extent);
    if (!chosen && area.contains(place)) {
      chosen = place;
    }
  }
  if (!chosen) {
    // No side has room: the first, moved into the area.
    const int left = std::clamp(corners[0].x(), area.left(), std::max(area.left(), area.right() + 1 - extent.width()));
    const int top = std::clamp(corners[0].y(), area.top(), std::max(area.top(), area.bottom() + 1 - extent.height()));
    chosen = QRect(QPoint(left, top), extent);
  }
  setGeometry(*chosen);
  raise();
  show();
  update();
  if (!m_filtering) {
    qApp->installEventFilter(this);
    m_filtering = true;
  }
}

void PickList::dismiss() {
  if (m_filtering) {
    qApp->removeEventFilter(this);
    m_filtering = false;
  }
  m_hovered = -1;
  m_pressed = -1;
  hide();
}

void PickList::closeFor(const QString& why) {
  if (!isVisible()) {
    return;
  }
  dismiss();
  emit dismissed(why);
}

QRect PickList::rowRect(int row) const {
  if (row < m_first || row >= m_first + m_shown || row >= rowCount()) {
    return {};
  }
  const int step = rowHeight();
  return QRect(kMargin, kMargin + (row - m_first) * step, width() - 2 * kMargin, step);
}

int PickList::rowAt(const QPoint& position) const {
  for (int row = m_first; row < std::min(rowCount(), m_first + m_shown); ++row) {
    if (rowRect(row).contains(position)) {
      return row;
    }
  }
  return -1;
}

void PickList::setHovered(int row) {
  if (row == m_hovered) {
    return;
  }
  m_hovered = row;
  update();
  emit rowHovered(row);
}

bool PickList::eventFilter(QObject* watched, QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick: {
    // A press elsewhere closes the list and goes on (a press on the view
    // there is the view's again).
    const auto* mouse = static_cast<QMouseEvent*>(event);
    const QRect inScreen(mapToGlobal(QPoint(0, 0)), size());
    if (isVisible() && !inScreen.contains(mouse->globalPosition().toPoint())) {
      closeFor(QStringLiteral("click elsewhere"));
    }
    break;
  }
  case QEvent::ShortcutOverride: {
    // Esc is the list's while it is open, before any shortcut (a command's
    // Cancel, Clear Selection).
    auto* key = static_cast<QKeyEvent*>(event);
    if (key->key() == Qt::Key_Escape && isVisible()) {
      key->accept();
      return true;
    }
    break;
  }
  case QEvent::KeyPress:
    if (static_cast<QKeyEvent*>(event)->key() == Qt::Key_Escape && isVisible()) {
      closeFor(QStringLiteral("Esc"));
      return true;
    }
    break;
  default:
    break;
  }
  return QWidget::eventFilter(watched, event);
}

void PickList::paintEvent(QPaintEvent* /*event*/) {
  QPainter painter(this);
  const QPalette& colors = palette();
  painter.setPen(themeColor(ColorRole::Line, colors));
  painter.setBrush(themeColor(ColorRole::Raised, colors));
  painter.drawRect(rect().adjusted(0, 0, -1, -1));
  painter.setRenderHint(QPainter::TextAntialiasing);
  painter.setFont(font());
  const QFontMetrics metrics(font());
  for (int row = m_first; row < std::min(rowCount(), m_first + m_shown); ++row) {
    const QRect place = rowRect(row);
    if (row == m_hovered) {
      painter.fillRect(place, themeColor(ColorRole::HoverFill, colors));
    }
    painter.setPen(themeColor(ColorRole::Text, colors));
    const QRect text = place.adjusted(kPadding, 0, -kPadding, 0);
    painter.drawText(text, Qt::AlignLeft | Qt::AlignVCenter,
                     metrics.elidedText(m_rows.at(row), Qt::ElideMiddle, text.width()));
  }
  // More rows than fit: a line at the side the wheel brings them from.
  painter.setPen(themeColor(ColorRole::TextSecondary, colors));
  if (m_first > 0) {
    painter.drawLine(kPadding, 1, width() - kPadding, 1);
  }
  if (m_first + m_shown < rowCount()) {
    painter.drawLine(kPadding, height() - 2, width() - kPadding, height() - 2);
  }
}

void PickList::mouseMoveEvent(QMouseEvent* event) {
  setHovered(rowAt(event->position().toPoint()));
  event->accept();
}

void PickList::mousePressEvent(QMouseEvent* event) {
  m_pressed = event->button() == Qt::LeftButton ? rowAt(event->position().toPoint()) : -1;
  event->accept(); // not the view's under it
}

void PickList::mouseReleaseEvent(QMouseEvent* event) {
  const int row = rowAt(event->position().toPoint());
  const bool chosen = event->button() == Qt::LeftButton && row >= 0 && row == m_pressed;
  m_pressed = -1;
  event->accept();
  if (chosen) {
    emit rowChosen(row, event->modifiers());
  }
}

void PickList::leaveEvent(QEvent* event) {
  setHovered(-1);
  QWidget::leaveEvent(event);
}

void PickList::wheelEvent(QWheelEvent* event) {
  const int notches = event->angleDelta().y() / 120;
  const int last = std::max(0, rowCount() - m_shown);
  const int first = std::clamp(m_first - notches, 0, last);
  if (first != m_first) {
    m_first = first;
    update();
    setHovered(rowAt(event->position().toPoint()));
  }
  event->accept(); // not a zoom of the view under it
}

} // namespace mitcad
