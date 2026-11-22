// SPDX-License-Identifier: MIT
#include "BrowserBackground.hpp"

#include <algorithm>
#include <optional>
#include <utility>
#include <vector>

#include <QObject>
#include <QPointer>
#include <QRect>
#include <QSettings>
#include <QVariant>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "BrowserPanel.hpp"

namespace mitcad {
namespace {

const QString kKey = QStringLiteral("appearance/browserBackground");
const QString kRowsShareKey = QStringLiteral("appearance/browserRowsShare");
const QString kCardWidthKey = QStringLiteral("appearance/browserCardWidth");

struct Follower {
  QPointer<GlassCard> card;
  QPointer<BrowserPanel> browser;
  std::function<void()> relayout;
};

std::vector<Follower>& followers() {
  static std::vector<Follower> list;
  return list;
}

// The followers still there.
std::vector<Follower>& liveFollowers() {
  std::vector<Follower>& list = followers();
  const auto gone = [](const Follower& follower) { return follower.card.isNull() || follower.browser.isNull(); };
  list.erase(std::remove_if(list.begin(), list.end(), gone), list.end());
  return list;
}

void apply(const Follower& follower, BrowserBackground background) {
  GlassCard* card = follower.card;
  BrowserPanel* browser = follower.browser;
  const bool rows = background == BrowserBackground::Rows;
  // Native glass is a window's, behind the whole card: a bare card is a
  // widget over the view, which draws its boxes itself.
  const auto glass = [card](bool windowed) {
    if (!glassActive() || card->isWindowed() == windowed) {
      return;
    }
    const bool shown = card->isShown();
    const QRect slot = card->slot();
    card->setWindowed(windowed);
    card->setSlot(slot);
    card->setVisible(shown);
  };
  if (rows) {
    glass(false);
    browser->setRowsBackground(true);
    browser->setScrollBarOnLeft(true); // beside the rows (mitcad#425)
    card->setBare(true);
    card->setContentRegion(browser->rowsRegion());
    // Its width follows the rows (mitcad#1932): no grip.
    card->clearWidthAdjustable();
  } else {
    card->setBare(false);
    browser->setRowsBackground(false);
    browser->setScrollBarOnLeft(false);
    glass(true);
    // Its width dragged by its right edge and remembered (mitcad#1932, as
    // the Projects card's, mitcad#529).
    card->setWidthAdjustable(kBrowserCardMinimumWidth);
  }
  qDebug().noquote() << QStringLiteral("Browser background: %1").arg(browserBackgroundId(background));
  if (follower.relayout) {
    follower.relayout();
  }
}

// Every Browser card that follows the background shows `background`.
void applyToFollowers(BrowserBackground background) {
  for (const Follower& follower : liveFollowers()) {
    if (follower.card->isBare() != (background == BrowserBackground::Rows)) {
      apply(follower, background);
    }
  }
}

} // namespace

BrowserBackground defaultBrowserBackground() {
  // Rows on every platform (mitcad#2144, as before the toolbar panel on
  // Windows and Linux, mitcad#429; macOS has the shared Qt UI now): the
  // Browser's rows under the toolbar panel, the view between and beside
  // them. Card stays a choice.
  return BrowserBackground::Rows;
}

std::optional<BrowserBackground> chosenBrowserBackground() {
  const QString saved = QSettings().value(kKey).toString();
  if (saved == QLatin1String("rows")) {
    return BrowserBackground::Rows;
  }
  if (saved == QLatin1String("card")) {
    return BrowserBackground::Card;
  }
  return std::nullopt;
}

BrowserBackground savedBrowserBackground() { return chosenBrowserBackground().value_or(defaultBrowserBackground()); }

void saveBrowserBackground(BrowserBackground background) {
  QSettings().setValue(kKey, browserBackgroundId(background));
  applyToFollowers(background);
}

void clearBrowserBackground() {
  QSettings().remove(kKey);
  applyToFollowers(defaultBrowserBackground());
}

void restoreBrowserBackground(const std::optional<BrowserBackground>& choice) {
  if (choice) {
    saveBrowserBackground(*choice);
  } else {
    clearBrowserBackground();
  }
}

QString browserBackgroundId(BrowserBackground background) {
  return background == BrowserBackground::Rows ? QStringLiteral("rows") : QStringLiteral("card");
}

int savedBrowserRowsShare() {
  bool ok = false;
  const int saved = QSettings().value(kRowsShareKey).toInt(&ok);
  if (!ok || saved <= 0) {
    return kDefaultBrowserRowsShare;
  }
  return std::clamp(saved, kMinimumBrowserRowsShare, kMaximumBrowserRowsShare);
}

void saveBrowserRowsShare(int percent) {
  QSettings().setValue(kRowsShareKey, std::clamp(percent, kMinimumBrowserRowsShare, kMaximumBrowserRowsShare));
  for (const Follower& follower : liveFollowers()) {
    if (follower.relayout) {
      follower.relayout();
    }
  }
}

int savedBrowserCardWidth() {
  bool ok = false;
  const int saved = QSettings().value(kCardWidthKey).toInt(&ok);
  return ok && saved > 0 ? std::max(saved, kBrowserCardMinimumWidth) : 0;
}

void saveBrowserCardWidth(int width) { QSettings().setValue(kCardWidthKey, std::max(width, kBrowserCardMinimumWidth)); }

int browserCardWidthFor(GlassCard* card, BrowserPanel* browser, int viewWidth) {
  if (card == nullptr || browser == nullptr || !card->isBare()) {
    return 0;
  }
  const int share = savedBrowserRowsShare();
  const int limit = std::max(1, viewWidth * share / 100);
  // What the card adds around the panel (its content's margins).
  const int around = card->width() > browser->width() ? card->width() - browser->width() : 12;
  const int rows = browser->rowsWidth();
  const int widest = rows > 0 ? rows + around : 0;
  const int width = std::min(std::max(widest, kBrowserCardWidth), limit);
  const QString line = QStringLiteral("Browser rows width %1 (widest row %2, at most %3% of %4: %5)")
                           .arg(width)
                           .arg(widest)
                           .arg(share)
                           .arg(viewWidth)
                           .arg(limit);
  if (card->property("browserRowsWidthLogged").toString() != line) {
    card->setProperty("browserRowsWidthLogged", line);
    qDebug().noquote() << line;
  }
  return width;
}

void followBrowserBackground(GlassCard* card, BrowserPanel* browser, std::function<void()> relayout) {
  followers().push_back({card, browser, relayout});
  // The rows scrolled, opened, folded, changed or were selected: where the
  // card takes the mouse, and its width (mitcad#1932), follow.
  QObject::connect(browser, &BrowserPanel::rowsRegionChanged, card, [card, browser, relayout] {
    if (card->isBare()) {
      card->setContentRegion(browser->rowsRegion());
      if (relayout) {
        relayout();
      }
    }
  });
  apply(followers().back(), savedBrowserBackground());
}

} // namespace mitcad
