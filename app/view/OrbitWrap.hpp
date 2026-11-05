// SPDX-License-Identifier: MIT
#pragma once

// An orbit past the edge of the screen (GitHub mitcad/Mitcad#139). The
// orbit's angle follows how far the cursor has gone since the press, and at
// the edge of the screen the system stops the cursor, so the orbit would
// stop too. OcctViewer moves the cursor to the opposite edge instead and adds
// the distance it jumped to every later position, so the view sees one
// straight drag and does not notice the jump. A move queued before the jump
// arrives after it with the old position: it counts without the jump. Where
// the system refuses to move the cursor (Wayland), the drag makes no other
// jump and the orbit stops at the edge as it did before. The unit tests are
// in tests/orbit_wrap.cpp.

#include <optional>

#include <QPoint>
#include <QPointF>
#include <QRect>

namespace mitcad {

class OrbitWrap {
public:
  // How near the edge of the screen the cursor jumps, in logical pixels:
  // before it gets there, since the system cuts off the part of a move beyond
  // the edge and the orbit would lose that much. A fast mouse moves about
  // this far between two events.
  static constexpr int kMargin = 16;

  // Where the cursor at `cursor` jumps to on `screen`, inside the opposite
  // edge's margin so that it does not jump back at once; nothing while it is
  // away from the edges or on a screen too small to jump on.
  static std::optional<QPoint> target(const QPointF& cursor, const QRect& screen) {
    if (screen.width() <= 4 * kMargin || screen.height() <= 4 * kMargin) {
      return std::nullopt;
    }
    QPoint to = cursor.toPoint();
    bool jumps = false;
    if (cursor.x() <= screen.left() + kMargin) {
      to.setX(screen.right() - kMargin - 1);
      jumps = true;
    } else if (cursor.x() >= screen.right() - kMargin) {
      to.setX(screen.left() + kMargin + 1);
      jumps = true;
    }
    if (cursor.y() <= screen.top() + kMargin) {
      to.setY(screen.bottom() - kMargin - 1);
      jumps = true;
    } else if (cursor.y() >= screen.bottom() - kMargin) {
      to.setY(screen.top() + kMargin + 1);
      jumps = true;
    }
    if (!jumps) {
      return std::nullopt;
    }
    return to;
  }

  // Whether a global position is on the far side of a jump from `from` to
  // `to`: where a move after the jump is, and the cursor after a jump the
  // system carried out. (A system may refuse: Wayland, through XWayland.)
  static bool landed(const QPointF& global, const QPointF& from, const QPointF& to) {
    return (global - to).manhattanLength() < (global - from).manhattanLength();
  }

  // The position in the widget that the view gets for an event at `widget`,
  // `global` on the screen.
  QPointF position(const QPointF& widget, const QPointF& global) {
    QPointF at = widget + m_offset;
    if (m_pending) {
      if (landed(global, m_from, m_to)) {
        m_pending = false;
      } else {
        at -= m_from - m_to; // queued before the jump
      }
    }
    return at;
  }

  // The cursor jumped from `from` to `to`.
  void jumped(const QPointF& from, const QPointF& to) {
    m_offset += from - to;
    m_from = from;
    m_to = to;
    m_pending = true;
    m_jumpMove = true;
  }

  // The system did not move the cursor: no other jump in this drag, so the
  // orbit stops at the edge, without a call to the system on every move.
  void refused() { m_refused = true; }

  // Whether the last jump is still to arrive: no other until it has.
  bool pending() const { return m_pending; }

  // Whether the cursor may jump now: not while the last jump is still to
  // arrive, nor after the system refused one.
  bool canJump() const { return !m_pending && !m_refused; }

  // Whether a move without buttons at `global` is the one the jump itself
  // caused (macOS posts the cursor's move as a move without buttons), once
  // per jump and only at the jump's target. OcctViewer then keeps its
  // guard for a lost release from ending the orbit.
  bool takeJumpMove(const QPointF& global) {
    if (!m_jumpMove || (global - m_to).manhattanLength() > 1.0) {
      return false;
    }
    m_jumpMove = false;
    return true;
  }

  void reset() { *this = OrbitWrap(); }

private:
  QPointF m_offset;
  QPointF m_from;
  QPointF m_to;
  bool m_pending = false;
  bool m_refused = false;
  bool m_jumpMove = false;
};

} // namespace mitcad
