// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QString>

namespace mitcad {

class BrowserPanel;
class GlassCard;

// The Browser's background in the Floating layout (mitcad#419; Preferences
// › Display, the setting appearance/browserBackground): its card ("card"),
// or no card and a small box of the card's material behind each row and
// behind the header ("rows"), the 3D view showing between and beside them
// and taking the mouse there. The Docked layout has no such background; the
// choice is kept for the Floating layout (mitcad#429: Preferences shows it in
// both layouts).
enum class BrowserBackground { Card, Rows };

// Without a saved choice: the card (mitcad#1990; Rows on Windows and Linux
// before the toolbar panel, mitcad#429).
BrowserBackground defaultBrowserBackground();
// The saved choice, else the default.
BrowserBackground savedBrowserBackground();
// Saves the choice; every Browser card that follows it (followBrowserBackground)
// changes at once.
void saveBrowserBackground(BrowserBackground background);
// "card" or "rows", as saved and logged.
QString browserBackgroundId(BrowserBackground background);

// The Browser card's width without a remembered one, and the least its width
// grip (Card) leaves it (mitcad#1932). With Rows the card is at least
// kBrowserCardWidth wide, wider for its rows (browserCardWidthFor).
constexpr int kBrowserCardWidth = 260;
constexpr int kBrowserCardMinimumWidth = 200;

// With Rows, the widest the card grows for its rows, as a percentage of the
// view's width (mitcad#1932; Preferences › Display › Browser background
// › Rows at most,
// the setting appearance/browserRowsShare): by default a third; past it a
// name is elided.
constexpr int kDefaultBrowserRowsShare = 33;
constexpr int kMinimumBrowserRowsShare = 15;
constexpr int kMaximumBrowserRowsShare = 80;
int savedBrowserRowsShare();
// Saves the share; every Browser card that follows the background is laid
// out again at once.
void saveBrowserRowsShare(int percent);

// With Card, the card's width as the user dragged its grip
// (appearance/browserCardWidth; 0: none remembered), restored at the start.
int savedBrowserCardWidth();
void saveBrowserCardWidth(int width);

// The card's width for a view `viewWidth` wide (FloatingLayout::Sizing::
// widthFor): with Rows, as wide as its widest row in sight with its whole
// name (BrowserPanel::rowsWidth), at least kBrowserCardWidth, at most the
// share of the view (savedBrowserRowsShare); 0 with Card (the width the
// layout keeps, as dragged). Logs "Browser rows width <w> (widest row <n>,
// at most <p>% of <view>: <limit>)" when it changed.
int browserCardWidthFor(GlassCard* card, BrowserPanel* browser, int viewWidth);

// The card that holds the browser follows the saved choice now and whenever
// it is saved again: with Rows the card is bare (GlassCard::setBare), takes
// the mouse only where the rows are (BrowserPanel::rowsRegion) and has no
// width grip; with Card it has the width grip (GlassCard::setWidthAdjustable,
// mitcad#1932). A windowed card (native glass) is a widget over the view
// while bare. `relayout` places the cards again (the card's width follows
// its rows and the share). Logs "Browser background: card" (rows).
void followBrowserBackground(GlassCard* card, BrowserPanel* browser, std::function<void()> relayout = {});

} // namespace mitcad
