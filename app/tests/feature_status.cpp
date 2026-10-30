// SPDX-License-Identifier: MIT
// A feature's build state on the timeline and the selection card
// (mitcad#961): a failed and a warned cell, selected and not, hovered,
// found or rolled back, keep their status ring (inside the selection's)
// and their status badge, as the painted pixels show; the badges differ by
// shape as well as colour, also as the failures' and warnings' buttons'
// icons; the card's line of a failed feature; a red text's colour for the
// card it sits on.
#include "browser/FeatureStatus.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <initializer_list>

#include <QColor>
#include <QCoreApplication>
#include <QFont>
#include <QIcon>
#include <QImage>
#include <QLabel>
#include <QPainter>
#include <QPalette>
#include <QRectF>
#include <QSize>
#include <QString>
#include <QToolButton>
#include <QWidget>

#include "framework/Theme.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "feature_status.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;
using timeline::CellLook;
using timeline::CellState;

const QColor kBackground(0x2d, 0x2d, 0x2d);

// Distinct colours, so that every pixel says which part it is of.
timeline::CellColors colors() {
  return {QColor(0x86, 0x2c, 0x34), QColor(0xd0, 0x20, 0x20), QColor(0x6e, 0x5a, 0x1a), QColor(0xd0, 0xa0, 0x00),
          QColor(0x20, 0x40, 0x80), QColor(0x1a, 0x6e, 0xe0), QColor(0x34, 0x3c, 0x48), QColor(0xff, 0x7a, 0x00),
          QColor(0x90, 0x90, 0x90), kBackground};
}

bool similar(const QColor& a, const QColor& b) {
  return std::abs(a.red() - b.red()) <= 24 && std::abs(a.green() - b.green()) <= 24 &&
         std::abs(a.blue() - b.blue()) <= 24;
}
bool red(const QColor& c) { return c.red() > 180 && c.green() < 90 && c.blue() < 90; }
bool amber(const QColor& c) { return c.red() > 200 && c.green() > 140 && c.blue() < 60; }
bool white(const QColor& c) { return c.red() > 230 && c.green() > 230 && c.blue() > 230; }
bool dark(const QColor& c) { return c.red() < 70 && c.green() < 70 && c.blue() < 70; }

CellState cellState(FeatureState state, bool selected) {
  CellState cell;
  cell.state = state;
  cell.selected = selected;
  return cell;
}

const QRectF kCell(8, 6, 28, 28);

// A cell painted as the timeline paints it (its frame, then the badge over
// where the icon would be).
QImage paint(const CellState& state) {
  QImage image(48, 40, QImage::Format_ARGB32_Premultiplied);
  image.fill(kBackground);
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  const CellLook look = timeline::cellLook(state, colors());
  timeline::paintCellFrame(painter, kCell, look);
  if (look.badge) {
    timeline::paintStatusBadge(painter, kCell, state.state, colors().halo);
  }
  painter.end();
  return image;
}

QColor pixel(const QImage& image, double x, double y) {
  return image.pixelColor(static_cast<int>(x), static_cast<int>(y));
}

void stateTests() {
  CHECK(featureState(QStringLiteral("error")) == FeatureState::Failed);
  CHECK(featureState(QStringLiteral("warning")) == FeatureState::Warning);
  CHECK(featureState(QStringLiteral("ok")) == FeatureState::Ok);
  CHECK(featureState(QStringLiteral("suppressed")) == FeatureState::Ok);
  CHECK(featureState(QString()) == FeatureState::Ok);
}

void lookTests() {
  const timeline::CellColors c = colors();
  // Failed: the error fill, the red ring and the badge, selected or not;
  // the selection's ring outside the red one.
  CellLook look = timeline::cellLook(cellState(FeatureState::Failed, false), c);
  CHECK(look.fill == c.errorFill && look.statusRing == c.errorBorder && !look.outerRing.isValid() && look.badge);
  look = timeline::cellLook(cellState(FeatureState::Failed, true), c);
  CHECK(look.fill == c.errorFill && look.statusRing == c.errorBorder && look.outerRing == c.selectedBorder &&
        look.badge);
  CHECK(timeline::describeCell(cellState(FeatureState::Failed, true), look) ==
        QStringLiteral("failed, selected, status ring, outer ring, badge"));
  // Hovered, a search's match or rolled back: still the fill, ring and badge.
  CellState state = cellState(FeatureState::Failed, false);
  state.hovered = true;
  look = timeline::cellLook(state, c);
  CHECK(look.fill == c.errorFill && look.statusRing == c.errorBorder && look.badge);
  // The pointer shows as a faint ring outside the red one.
  CHECK(look.outerRing == c.hoverRing);
  CHECK(timeline::describeCell(state, look) == QStringLiteral("failed, hovered, status ring, outer ring, badge"));
  state.found = true;
  look = timeline::cellLook(state, c);
  CHECK(look.outerRing == c.found && look.statusRing == c.errorBorder && look.badge);
  state = cellState(FeatureState::Failed, false);
  state.rolledBack = true;
  look = timeline::cellLook(state, c);
  CHECK(look.badge && look.statusRing == c.errorBorder);
  CHECK(timeline::describeCell(state, look) == QStringLiteral("failed, rolled back, status ring, badge"));
  // A warning: yellow, with its badge.
  look = timeline::cellLook(cellState(FeatureState::Warning, false), c);
  CHECK(look.fill == c.warningFill && look.statusRing == c.warningBorder && look.badge);
  CHECK(timeline::describeCell(cellState(FeatureState::Warning, false), look) ==
        QStringLiteral("warning, status ring, badge"));
  look = timeline::cellLook(cellState(FeatureState::Warning, true), c);
  CHECK(look.fill == c.warningFill && look.statusRing == c.warningBorder && look.outerRing == c.selectedBorder &&
        look.badge);
  // An ok feature: no status ring nor badge; selected, the selection's fill.
  look = timeline::cellLook(cellState(FeatureState::Ok, true), c);
  CHECK(look.fill == c.selectedFill && !look.statusRing.isValid() && look.outerRing == c.selectedBorder &&
        !look.badge);
  state = cellState(FeatureState::Ok, false);
  state.hovered = true;
  CHECK(timeline::cellLook(state, c).fill == c.hoverFill);
  // An import placeholder: a yellow outline, no badge.
  state = cellState(FeatureState::Ok, false);
  state.noted = true;
  look = timeline::cellLook(state, c);
  CHECK(look.statusRing == c.warningBorder && !look.badge && !look.fill.isValid());
}

void paintTests() {
  const timeline::CellColors c = colors();
  const QRectF badge = timeline::statusBadgeRect(kCell);
  CHECK(kCell.contains(badge.center()));
  const double y = kCell.center().y() + 4; // the left side's middle, below the badge
  for (const FeatureState state : {FeatureState::Failed, FeatureState::Warning}) {
    const QColor ring = state == FeatureState::Failed ? c.errorBorder : c.warningBorder;
    const QColor fill = state == FeatureState::Failed ? c.errorFill : c.warningFill;
    for (const bool selected : {false, true}) {
      const QImage image = paint(cellState(state, selected));
      if (selected) {
        // The selection's ring on the edge, the status ring inside it.
        CHECK(similar(pixel(image, kCell.left(), y), c.selectedBorder));
        CHECK(similar(pixel(image, kCell.left() + 1, y), c.selectedBorder));
        CHECK(similar(pixel(image, kCell.left() + 2, y), ring));
        CHECK(similar(pixel(image, kCell.left() + 3, y), ring)); // two pixels wide
      } else {
        CHECK(similar(pixel(image, kCell.left(), y), ring));
        CHECK(similar(pixel(image, kCell.left() + 1, y), ring));
      }
      CHECK(similar(pixel(image, kCell.left() + 6, y), fill));
      // The badge with its "!".
      if (state == FeatureState::Failed) {
        CHECK(red(pixel(image, badge.center().x() - 4, badge.center().y())));
        CHECK(white(pixel(image, badge.center().x(), badge.top() + 4)));
        // A circle: its bounding box's bottom left corner is not red.
        CHECK(!red(pixel(image, badge.left() + 0.5, badge.bottom() - 1)));
      } else {
        CHECK(amber(pixel(image, badge.center().x() - 4, badge.top() + 10)));
        CHECK(dark(pixel(image, badge.center().x(), badge.top() + 5)));
        // A triangle: wide at its base.
        CHECK(amber(pixel(image, badge.left() + 0.5, badge.bottom() - 1)));
      }
    }
  }
  // An ok cell: no badge, its pixels the background's.
  const QImage ok = paint(cellState(FeatureState::Ok, false));
  CHECK(similar(pixel(ok, badge.center().x() - 4, badge.center().y()), kBackground));
  CHECK(similar(pixel(ok, kCell.left(), y), kBackground));
}

// WCAG's relative luminance and contrast ratio.
double luminance(const QColor& c) {
  const auto channel = [](int value) {
    const double v = value / 255.0;
    return v <= 0.03928 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(c.red()) + 0.7152 * channel(c.green()) + 0.0722 * channel(c.blue());
}
double contrast(const QColor& a, const QColor& b) {
  const double la = luminance(a);
  const double lb = luminance(b);
  return (std::max(la, lb) + 0.05) / (std::min(la, lb) + 0.05);
}

QPalette windowPalette(const QColor& window, const QColor& text) {
  QPalette palette;
  palette.setColor(QPalette::Window, window);
  palette.setColor(QPalette::WindowText, text);
  return palette;
}

// A themed red text follows what it sits on: a dark card gets the dark
// theme's red whatever the application's palette, also when the label is
// put on the card after its sheet was set, and when the window's palette
// changes after it (Qt passes that on to no child with a sheet of its own:
// the sheet is checked when it paints).
void themedSheetTests() {
  const QPalette night = windowPalette(QColor(0x14, 0x15, 0x18), QColor(0xe6, 0xe8, 0xec));
  const QPalette day = windowPalette(QColor(0xe4, 0xe7, 0xec), QColor(0x1d, 0x20, 0x26));
  CHECK(errorColor(night) != errorColor(day));
  QWidget card;
  card.setPalette(night);
  auto* label = new QLabel(QStringLiteral("1 failed"));
  setErrorStyleSheet(label, QStringLiteral("color: %1;"));
  label->setParent(&card);
  CHECK(label->styleSheet() == QStringLiteral("color: %1;").arg(errorColor(night).name()));
  QWidget window;
  window.setPalette(day);
  label->setParent(&window);
  CHECK(label->styleSheet() == QStringLiteral("color: %1;").arg(errorColor(day).name()));
  // The theme turns dark: the window's palette, as WindowStyle sets it.
  window.setPalette(night);
  label->resize(label->sizeHint());
  (void)label->grab(); // a paint
  QCoreApplication::processEvents();
  CHECK(label->styleSheet() == QStringLiteral("color: %1;").arg(errorColor(night).name()));
}

// The error text (the selection card's `Failed:` line, the status message)
// at least 4.5:1 on what it is shown on: the dark window, panels and
// Floating card (#464748), the light window and panels.
void errorTextTests() {
  for (const QColor& background : {QColor(0x14, 0x15, 0x18), QColor(0x1f, 0x20, 0x24), QColor(0x46, 0x47, 0x48)}) {
    CHECK(contrast(errorColor(windowPalette(background, QColor(0xe6, 0xe8, 0xec))), background) >= 4.5);
  }
  for (const QColor& background : {QColor(0xe4, 0xe7, 0xec), QColor(0xf8, 0xf9, 0xfb), QColor(0xff, 0xff, 0xff)}) {
    CHECK(contrast(errorColor(windowPalette(background, QColor(0x1d, 0x20, 0x26))), background) >= 4.5);
  }
}

// The failures' and warnings' buttons: badges of their own colours, their
// text at least 4.5:1 on their fill as the real widgets paint it, on a
// dark and a light window alike.
void issueBadgeTests() {
  const QPalette night = windowPalette(QColor(0x14, 0x15, 0x18), QColor(0xe6, 0xe8, 0xec));
  const QPalette grey = windowPalette(QColor(0x49, 0x49, 0x49), QColor(0xf0, 0xf0, 0xf0));
  const QPalette day = windowPalette(QColor(0xe4, 0xe7, 0xec), QColor(0x1d, 0x20, 0x26));
  for (const FeatureState state : {FeatureState::Failed, FeatureState::Warning}) {
    CHECK(contrast(issueBadgeTextColor(state), issueBadgeColor(state)) >= 4.5);
    for (const QPalette& palette : {night, grey, day}) {
      QWidget window;
      window.setPalette(palette);
      auto* button = new QToolButton(&window);
      QFont font = button->font();
      font.setPixelSize(24); // whole pixels in the strokes, to read the text's colour
      button->setFont(font);
      button->setText(state == FeatureState::Failed ? QStringLiteral("1 failed") : QStringLiteral("1"));
      button->setStyleSheet(issueBadgeStyle(state));
      button->ensurePolished();
      button->resize(button->sizeHint());
      const QImage image = button->grab().toImage();
      const QColor fill = image.pixelColor(image.width() - 4, image.height() / 2);
      CHECK(similar(fill, issueBadgeColor(state)));
      double best = 1;
      QColor text;
      // Within the capsule (its corners' radius is 9), opaque pixels only.
      for (int y = 2; y < image.height() - 2; ++y) {
        for (int x = 9; x < image.width() - 9; ++x) {
          if (image.pixelColor(x, y).alpha() < 255) {
            continue;
          }
          const double ratio = contrast(image.pixelColor(x, y), fill);
          if (ratio > best) {
            best = ratio;
            text = image.pixelColor(x, y);
          }
        }
      }
      CHECK(best >= 4.5);
      CHECK(similar(text, issueBadgeTextColor(state)));
    }
  }
}

// The failures' and warnings' buttons show the timeline's badges, in a halo
// of the button's text colour.
void iconTests() {
  CHECK(featureStateIcon(FeatureState::Ok).isNull());
  const QImage failed = featureStateIcon(FeatureState::Failed).pixmap(QSize(24, 24)).toImage();
  const QImage warned = featureStateIcon(FeatureState::Warning).pixmap(QSize(24, 24)).toImage();
  CHECK(failed.width() == 24 && warned.width() == 24);
  CHECK(red(failed.pixelColor(7, 12)));
  CHECK(white(failed.pixelColor(12, 9)));
  CHECK(amber(warned.pixelColor(7, 18)));
  CHECK(dark(warned.pixelColor(12, 10)));
}

void cardTests() {
  const QColor error(0xb0, 0x00, 0x20);
  const QColor warning(0x8a, 0x5a, 0x00);
  CHECK(featureStatusLine(QStringLiteral("error"), QStringLiteral("the radius is too large"), error, warning) ==
        QStringLiteral("<span style=\"color:#b00020\">Failed: the radius is too large</span>"));
  CHECK(featureStatusLine(QStringLiteral("error"), QStringLiteral("a < b"), error, warning) ==
        QStringLiteral("<span style=\"color:#b00020\">Failed: a &lt; b</span>"));
  CHECK(featureStatusLine(QStringLiteral("error"), QString(), error, warning) ==
        QStringLiteral("<span style=\"color:#b00020\">Failed</span>"));
  CHECK(featureStatusLine(QStringLiteral("warning"), QStringLiteral("built 0.1 % smaller"), error, warning) ==
        QStringLiteral("<span style=\"color:#8a5a00\">Warning: built 0.1 % smaller</span>"));
  CHECK(featureStatusLine(QStringLiteral("ok"), QStringLiteral("ignored"), error, warning).isEmpty());
  CHECK(featureStatusLine(QStringLiteral("suppressed"), QStringLiteral("skipped"), error, warning).isEmpty());
}

} // namespace

int featureStatusTests() {
  failures = 0;
  stateTests();
  lookTests();
  paintTests();
  iconTests();
  themedSheetTests();
  issueBadgeTests();
  errorTextTests();
  cardTests();
  return failures;
}
