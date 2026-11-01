// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QList>
#include <QMargins>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QWidget>

namespace mitcad {

class GlassCard;
class MessageBand;
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
// The upper left cards (Projects, Configuration, Browser; mitcad#1020) are
// one column: stacked in the order they were added, they share the height
// from below the rows to above the card below them (the timeline). A card
// whose content is shorter than its share keeps its content's height and
// gives the rest to the others; only when the contents of all of them are
// longer than the column do they scroll. The boundary between two open
// neighbours is dragged (a resize cursor over the gap between them): the
// height moves from one to the other, each keeping its header and a row,
// and the heights then are the shares the cards get (remembered under
// setSplitKey's settings key); a double-click on the boundary returns to
// the even split. A collapsed card gives its height to the others.
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
// The message band (mitcad#1042, setMessageBand) sits right above the
// timeline, as wide as it: the window's messages, in full or as the stub of
// the latest one (MessageBand); the other cards stay above its row.
class FloatingLayout : public QWidget {
  Q_OBJECT

public:
  // Where a card sits in the view: the upper left corner, the upper right
  // corner under the orientation cube, the bottom spanning the width, or
  // a panel on the right edge from the top rows down to the bottom card
  // (the chat, mitcad#947), with the upper right cards and the
  // orientation cube left of it; or bottom centre (the navigation bar,
  // mitcad#1086): just above the bottom card, at its content's size, in the
  // middle of the free width between the side cards (and the sketch's
  // badge) of its row, the status pill above it.
  enum class Anchor { TopLeft, TopRight, Bottom, Right, BottomCenter };

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
  // The message band's margins around its content, and the stub's opacity.
  static constexpr QMargins kBandMargins = QMargins(16, 6, 8, 6);
  static constexpr qreal kStubOpacity = 0.8;
  // What a dragged boundary leaves of a left card's content at least (a
  // row; mitcad#1020).
  static constexpr int kSplitMinimumContent = 24;

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

  // The message band (mitcad#1042): `band` becomes the content of a card
  // right above the timeline, shown while it has a message (in full or as
  // its stub). Logged `Message band placed: full|stub x,y wxh, row <h>,
  // timeline x,y wxh` when its place changes; in "Floating cards: …" its
  // card (`messageBandCard`) and the row it keeps (`messageBandRow`), which
  // the other cards end above.
  void setMessageBand(MessageBand* band);

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

  // The settings key under which the user's split of the left column is
  // kept (mitcad#1020: "floating/leftSplit", a list of "<card>=<height>");
  // read at once ("Floating split restored: configurationDock 90,
  // browserDock 520"), written when a drag of a boundary ends ("Floating
  // split remembered: …") and removed by a double-click on one ("Floating
  // split: automatic").
  void setSplitKey(const QString& key);

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
  // The left column's boundaries (mitcad#1020): a drag from the boundary
  // `index` (between m_splitPairs[index]) at the global y, moved, ended;
  // a double-click on one.
  void startSplit(int index, int globalY);
  void moveSplit(int globalY);
  void endSplit();
  void resetSplit();
  QWidget* splitHandle(int index);
  static QString splitText(const std::vector<std::pair<QString, int>>& split);
  void adopt(GlassCard* card);
  // The band's card shown, hidden or dimmed as its content asks.
  void updateBand();

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

  // The left column's split (mitcad#1020): the user's heights of the cards
  // by object name, in their order (none: the even split), the settings
  // key that keeps them, the boundaries' handles and the cards each one
  // is between, and a drag in progress (the heights it started from).
  struct SplitDrag {
    QString above;
    QString below;
    int startY = 0;
    std::vector<std::pair<QString, int>> heights;
    bool moved = false;
  };
  std::vector<std::pair<QString, int>> m_split;
  QString m_splitKey;
  std::vector<QPointer<QWidget>> m_splitHandles;
  std::vector<std::pair<QString, QString>> m_splitPairs;
  std::optional<SplitDrag> m_drag;
  QString m_loggedSplits;

  GlassCard* m_band = nullptr; // the message band's card (mitcad#1042)
  MessageBand* m_bandContent = nullptr;
  QRect m_bandRow; // the row it keeps, from the last layout
  QString m_loggedBand;
};

} // namespace mitcad
