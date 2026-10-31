// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>
#include <vector>

#include <QList>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>
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
// with a margin that is concentric with the window's corner radius; no card
// starts above the header's bottom (mitcad#419), which is followed as it
// grows, shrinks or moves. On Windows and Linux there is no header
// (mitcad#425): the view starts below the menu bar, and the title row and the
// toolbar's groups float over its top as rows of cards (addTopRow), which
// the other cards start below. A card has the size of its content up to a
// share of the view and the card below it, and scrolls inside when it is
// longer (the content's own scroll area).
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
  // corner under the orientation cube, the bottom spanning the width, or
  // a panel on the right edge from the top rows down to the bottom card
  // (the chat, mitcad#947), with the upper right cards and the
  // orientation cube left of it.
  enum class Anchor { TopLeft, TopRight, Bottom, Right };

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

  // The cards a top row is made of: their names and rectangles in the row's
  // coordinates.
  using RowCards = std::function<std::vector<std::pair<QString, QRect>>()>;
  // A row floating at the top of the view (mitcad#425, floatingTopCards():
  // the title row, the toolbar's group cards, the notices): below the
  // header (if any) and the rows added before it, as wide as the view less
  // the margins, as high as its size hint within its minimum and maximum
  // height; the anchored cards start below the last row shown, and the
  // orientation cube under it. `cards`: the cards of a row that takes the
  // mouse only on them (a mask of its own: the title row's controls and the
  // box of its title, mitcad#429; the ribbon's group cards); null: the row is
  // one card, named by its object name. `parts`: the row is shown while any
  // of them is (the notices' bars, which come and go); none: while the row
  // is. Not for windowed cards.
  void addTopRow(QWidget* row, RowCards cards = {}, const QList<QWidget*>& parts = {});

  // The height that the content of the card holding `content` may take at
  // the card's place before it scrolls: to the card below it (the timeline)
  // or the view's bottom margin, less the card's header and margins, from
  // the last layout (mitcad#425: the Browser's card grows to it); -1 when
  // `content` is no card's content or the card was not placed yet.
  int maximumContentHeight(const QWidget* content) const;

  // Shows a message in the status pill; it fades out after `milliseconds`.
  void showStatus(const QString& text, bool error, int milliseconds);

  // The content's size hints decide the cards' sizes: its layout requests
  // place the cards again.
  void watchContent(QWidget* content);

  // Only the anchored cards of these object names are shown (mitcad#1008:
  // the browser while a drawing takes the view), the others hidden until
  // it is called with none, then shown or hidden as they were asked to be
  // meanwhile (GlassCard::setSuppressed). The rows at the top stay.
  void showOnlyCards(const QStringList& names);

  // Places the cards again (queued: many changes make one layout).
  void requestLayout();
  // Places the cards at once, for a change whose free area is read right
  // away (a sketch's view centred in it as sketch mode starts, mitcad#738).
  void layoutNow();

  // A card's width as the user dragged it (GlassCard::setWidthAdjustable,
  // mitcad#529), placed at once: at most two thirds of the view less the
  // margins. A card added with a width wider than the view gets the view's.
  void setCardWidth(GlassCard* card, int width);

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
    int maxContent = -1; // maximumContentHeight, from the last layout
  };
  struct Row {
    QPointer<QWidget> widget;
    RowCards cards;
    std::vector<QPointer<QWidget>> parts;
  };

  // A top row's cards in its coordinates (the row itself without `cards`).
  static std::vector<std::pair<QString, QRect>> cardsOf(const Row& row);

  void layoutCards();
  // "Floating cards: window 1280x800, top bar bottom 21, view top 21, floating
  // rows bottom 160; titleRow 12,33 1256x40; titleHistory 12,36 64x34; ...;
  // ribbon 12,81 1256x67; ribbonGroup_SOLID/CREATE 12,81 200x67; ...;
  // browserDock 12,168 260x300; ..." in the window's coordinates, the rows
  // first (a row of cards of its own by its name, then its cards), when it
  // changed (UI tests check that
  // no card is under the top bar, mitcad#419, and no other card under the
  // rows, mitcad#425); and "Floating view between the cards at x,y", the
  // middle of the gap between the first two cards of the lowest row that
  // has one (the group cards).
  void logCards();
  void adopt(GlassCard* card);
  void elidePill(int room = 0);
  GlassCard* ensurePill();

  OcctViewer* m_viewer;
  QWidget* m_header;
  std::vector<Entry> m_entries;
  std::vector<Row> m_rows; // addTopRow
  bool m_layoutQueued = false;
  bool m_inLayout = false;
  bool m_windowed; // the cards are windows (native glass)
  QString m_loggedCards;
  QString m_loggedGap; // "Floating view between the cards at x,y"
  QPointer<QWidget> m_hostWindow; // watched for moves and state changes

  GlassCard* m_pill = nullptr;
  QLabel* m_pillLabel = nullptr;
  QString m_pillText;
  QTimer* m_pillTimer = nullptr;
  QVariantAnimation* m_pillFade = nullptr;
};

} // namespace mitcad
