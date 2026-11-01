// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QFrame>
#include <QPair>
#include <QPixmap>
#include <QRect>
#include <QString>
#include <QTimer>
#include <QVector>

#include "../framework/Command.hpp"
#include "BindingList.hpp"
#include "../platform/NativeBindingsCard.hpp"

class QHideEvent;
class QKeyEvent;
class QPushButton;
class QVBoxLayout;

namespace mitcad {

// A colour's swatch for bindings (the card's cases, the Bindings tab's Now
// column): a filled disc with a darker border.
QPixmap bindingSwatch(const QColor& color, int side);

// The fx card (mitcad#862): what a body, a feature or an occurrence
// follows, in words. The browser's fx badge (`fx 3`) and the timeline's
// show it while the pointer is on the badge (until it leaves the badge and
// the card); a click on the badge pins it, which Esc or a click outside
// closes. The card reads:
//
// - `Body2 follows`, with the current values of the options it uses
//   (`now: Color = Black, Holes = No`);
// - a line per property (Appearance, Material, Shown, Suppressed, each face
//   with an appearance of its own): a switch() over one option as a case
//   per value with the result's swatch, the current one marked ●; a
//   condition as `when Holes = Yes · hidden now`; another expression as its
//   text and what it gives now; a body's appearance and material that
//   follow nothing as plain lines (`Steel, set by hand`); results that name
//   nothing and the model's warnings under their line;
// - Edit… per line (Actions::edit: the Appearance or Materials window for
//   that target, Show When… or Suppress When…);
// - the link All bindings of the design (Actions::showAll: the
//   Configurations window's Bindings tab).
//
// Logged for UI tests: `Bindings card shown for Body2 (hover|pinned)`,
// `Bindings card Body2 follows; now: Color = Black`, `Bindings card line
// Appearance: Color: Black → Paint - Black ●, …`, `Bindings card
// warning Appearance: no appearance “x”`, `Bindings card Edit <property>
// at x,y`, `Bindings card all bindings at x,y` (in the main window's
// coordinates), `Bindings card Edit Appearance of Body2`, `Bindings card
// all bindings`, `Bindings card closed`.
class BindingsCard : public QFrame {
  Q_OBJECT

public:
  struct Actions {
    std::function<void(const BindingEntry&)> edit;
    std::function<void()> showAll;
  };

  BindingsCard(const CommandContext& model, Actions actions, QWidget* window);
  ~BindingsCard() override;

  // Shows the card of an item beside `anchor` (the badge, in global
  // coordinates): following the pointer, or pinned.
  void showFor(const BindingItem& item, const QRect& anchor, bool pinned);
  bool isPinned() const { return m_pinned; }
  BindingItem item() const { return m_item; }
  // The model changed: the card reads its item again (and closes when the
  // item has nothing left to show).
  void refresh();

protected:
  void keyPressEvent(QKeyEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  // Reads the item's bindings and lays out the card; false when it has
  // none.
  bool rebuild();
  void place();
  void followPointer();
  void logPlaces();

  const CommandContext& m_model;
  Actions m_actions;
  BindingItem m_item;
  QString m_name; // the item's, as the design names it
  QRect m_anchor;
  bool m_pinned = false;
  QVBoxLayout* m_layout = nullptr;
  QWidget* m_content = nullptr;
  // The Edit… buttons with their properties, and the link.
  QVector<QPair<QString, QPushButton*>> m_edits;
  QPushButton* m_all = nullptr;
  QTimer m_follow;
  int m_away = 0; // the pointer's looks outside the badge and the card in a row
  std::unique_ptr<NativeBindingsCardView> m_native;
};

} // namespace mitcad
