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
// jump and the orbit stops at the edge as it did before. Where the system
// reports the jump but then puts the cursor back (an absolute pointer: a
// virtual machine's mouse integration, a remote desktop, a pen tablet,
// XWayland), before or after a move at the jump's target, the jump is undone
// and the orbit stops at the edge too (GitHub mitcad/Mitcad#546,
// mitcad#2188). With two screens side by side the shared edge jumps as well:
// the cursor stays on the screen the orbit is on. The unit tests are in
// tests/orbit_wrap.cpp.

#include <cmath>
#include <optional>

#include <QPoint>
#include <QPointF>
#include <QRect>
#include <QtGlobal>

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
  // away from the edges or on a screen too small to jump on. A cursor past
  // the screen (on the next screen, beyond a shared edge) jumps as at the
  // edge it passed.
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
  // Halfway between the two, the screen's middle, and not near the target:
  // a fast move after the jump may land well beyond the edge's margin.
  static bool landed(const QPointF& global, const QPointF& from, const QPointF& to) {
    return (global - to).manhattanLength() < (global - from).manhattanLength();
  }

  // Whether a global position is back at the edge a jump from `from` to
  // `to` left: within the edge's margin of `from`, or beyond it (where the
  // system holds the cursor at the edge, or on the next screen past a
  // shared edge), along every axis the cursor jumped on. A drag's own moves
  // reach it from the far side only one by one, across the screen; one
  // move from the far side straight to it is the system putting the cursor
  // back. A move elsewhere on the old side (a test's cursor sent back to
  // where the drag was pressed) is not at the edge.
  static bool atOldEdge(const QPointF& global, const QPointF& from, const QPointF& to) {
    const Edge x = edge(global.x(), from.x(), to.x());
    const Edge y = edge(global.y(), from.y(), to.y());
    return x != Edge::Away && y != Edge::Away && (x == Edge::At || y == Edge::At);
  }

  // The position in the widget that the view gets for an event at `widget`,
  // `global` on the screen, with the event's own `timestamp`. While the jump
  // is to arrive: a move on the far side is the jump arrived; one stamped up
  // to the move that jumped is queued before the jump and counts without it;
  // a later one still on the old edge's side means the system put the
  // cursor back. After the jump arrived, a move from the far side straight
  // back to the old edge (atOldEdge) means the system put the cursor back
  // after it had delivered a move at the target (XWayland: the jump's own
  // move, then the next move of the real pointer). A jump put back is
  // undone: its distance is taken off again and the drag makes no other
  // jump, so the orbit stops at the edge (or goes on onto the next screen
  // past a shared edge) and no position the view gets moves by a screen's
  // width between two moves.
  //
  // The timestamp alone decides (mitcad#2205). mitcad#2188 also asked the
  // cursor (QCursor::pos) about a later-stamped move from the old edge, in
  // case X11 delivered a move the server had before it carried out the jump
  // after the move that jumped; no run showed one: the bursts of fast moves
  // over the shared edge in tools/ui-orbit-screens-test.sh, at device pixel
  // ratio 1 and 2, logged none in two bundle checks. Qt merges the moves
  // waiting in its queue into the last one (AA_CompressHighFrequencyEvents),
  // and when the move that jumped has been handled the jump's own move is
  // already behind such a move (OcctViewer asks the cursor after the jump,
  // and the server answers only after it has sent that move), so the move
  // never reaches the view on its own. The test now sends a move every
  // millisecond across the edge, several times per run, reports how far
  // the server moved the cursor between the move that jumped and the jump,
  // and fails when such a move undid the jump.
  QPointF position(const QPointF& widget, const QPointF& global, quint64 timestamp) {
    const QPointF last = m_last;
    const bool hadLast = m_hasLast;
    m_last = global;
    m_hasLast = true;
    if (m_pending) {
      if (landed(global, m_from, m_to)) {
        m_pending = false;
        m_arrived = true;
      } else if (timestamp <= m_stamp) {
        return widget + m_offset - m_shift; // queued before the jump
      } else {
        undo();
      }
    } else if (m_arrived && hadLast && landed(last, m_from, m_to) && atOldEdge(global, m_from, m_to)) {
      undo(); // put back after a move at the target
    }
    return widget + m_offset;
  }

  // The cursor jumped from `from` to `to` (global positions) on a move
  // stamped `timestamp`; `shift` is how far that moved the cursor in the
  // widget's own coordinates. These differ from `from - to` when the drag
  // passed onto a screen of its own scale and origin (Qt keeps each
  // screen's origin in device pixels, so with a device pixel ratio other
  // than 1 the global positions leave a gap between two screens that the
  // widget's do not): the view's positions are the widget's.
  void jumped(const QPointF& from, const QPointF& to, const QPointF& shift, quint64 timestamp) {
    m_offset += shift;
    m_shift = shift;
    m_from = from;
    m_to = to;
    m_stamp = timestamp;
    m_pending = true;
    m_arrived = false;
    m_jumpMove = true;
  }
  // The same on one screen whose global positions are the widget's moved.
  void jumped(const QPointF& from, const QPointF& to, quint64 timestamp) { jumped(from, to, from - to, timestamp); }

  // Whether the last position undid a jump the system put back, once: for
  // OcctViewer's log line.
  bool takeUndone() {
    const bool undone = m_undone;
    m_undone = false;
    return undone;
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
  // One axis of atOldEdge: not jumped on, at the old edge, or away from it.
  enum class Edge { None, At, Away };
  static Edge edge(double global, double from, double to) {
    if (std::abs(from - to) < 1.0) {
      return Edge::None;
    }
    const bool away = to < from ? global < from - kMargin : global > from + kMargin;
    return away ? Edge::Away : Edge::At;
  }

  // The system put the cursor back: the jump's distance taken off again,
  // and no other jump in this drag.
  void undo() {
    m_offset -= m_shift;
    m_pending = false;
    m_arrived = false;
    m_refused = true;
    m_undone = true;
  }

  QPointF m_offset;
  QPointF m_shift;
  QPointF m_from;
  QPointF m_to;
  QPointF m_last;
  quint64 m_stamp = 0;
  bool m_hasLast = false;
  bool m_pending = false;
  bool m_arrived = false;
  bool m_refused = false;
  bool m_jumpMove = false;
  bool m_undone = false;
};

} // namespace mitcad
