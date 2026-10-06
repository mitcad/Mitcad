// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QPointer>
#include <QWidget>

class QLabel;
class QTimer;
class QVariantAnimation;

namespace mitcad {

class GlassCard;
class OcctViewer;

// The central widget of the Floating chrome: the header (title bar and
// ribbon, if any) on top, below it the 3D view filling the rest of the
// window, and GlassCards as children over the view, anchored to its corners
// with a margin that is concentric with the window's corner radius. A card
// has the size of its content up to a share of the view, and scrolls
// inside when it is longer (the content's own scroll area).
//
// Cards take mouse events only inside their own rounded rectangle (their
// shadows are mouse-transparent), so picking, orbiting and sketch clicks
// work everywhere else in the view. When a card moves, resizes, shows or
// hides, the overlays of the view (the orientation cube, its arrows, the
// home button) are given the free area (OcctViewer::setOverlayInsets), so
// that they are not under a card.
//
// All the cards' geometry is computed in one place (layoutCards, in this
// widget's coordinates). With native glass (glassActive) the cards are
// windows of their own, children of the main window; the layout maps their
// places to screen coordinates, keeps them there when the window moves,
// resizes or changes its state, and shows them only while the main window is.
//
// The status pill is a transient capsule bottom centre, above the
// timeline: showStatus() shows a message that fades out after a few
// seconds.
class FloatingLayout : public QWidget {
  Q_OBJECT

public:
  // Where a card sits in the view: the upper left corner, the upper right
  // corner under the orientation cube, or the bottom spanning the width.
  enum class Anchor { TopLeft, TopRight, Bottom };

  struct Sizing {
    int width = 0; // 0: the width of the view less the margins
    // The content's height; the card adds its header and margins. Null: the
    // content's size hint.
    std::function<int()> contentHeight;
    // At most this share of the view's height (its own scroll area takes
    // the rest).
    qreal maxHeightShare = 1.0;
    bool shown = true; // initially
  };

  static constexpr int kMargin = 12;
  static constexpr int kGap = 8;
  static constexpr int kPillPadding = 16; // the status pill's side margins

  // `header` is placed above the view (null: none).
  FloatingLayout(OcctViewer* viewer, QWidget* header, QWidget* parent = nullptr);

  // The card becomes a child of this widget, over the view; the layout keeps
  // it where it is anchored while it is shown.
  void addCard(GlassCard* card, Anchor anchor, const Sizing& sizing);
  void addCard(GlassCard* card, Anchor anchor) { addCard(card, anchor, Sizing()); }

  // Shows a message in the status pill; it fades out after `milliseconds`.
  void showStatus(const QString& text, bool error, int milliseconds);

  // The content's size hints decide the cards' sizes: its layout requests
  // place the cards again.
  void watchContent(QWidget* content);

  // Places the cards again (queued: many changes make one layout).
  void requestLayout();

  OcctViewer* viewer() const { return m_viewer; }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  struct Entry {
    QPointer<GlassCard> card;
    Anchor anchor;
    Sizing sizing;
  };

  void layoutCards();
  void adopt(GlassCard* card);
  void elidePill(int room = 0);
  GlassCard* ensurePill();

  OcctViewer* m_viewer;
  QWidget* m_header;
  std::vector<Entry> m_entries;
  bool m_layoutQueued = false;
  bool m_inLayout = false;
  bool m_windowed; // the cards are windows (native glass)
  QPointer<QWidget> m_hostWindow; // watched for moves and state changes

  GlassCard* m_pill = nullptr;
  QLabel* m_pillLabel = nullptr;
  QString m_pillText;
  QTimer* m_pillTimer = nullptr;
  QVariantAnimation* m_pillFade = nullptr;
};

} // namespace mitcad
