// SPDX-License-Identifier: MIT
#include "DiscordButton.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QAction>
#include <QEvent>
#include <QHelpEvent>
#include <QImage>
#include <QMouseEvent>
#include <QPainter>
#include <QSettings>
#include <QSvgRenderer>
#include <QToolTip>
#include <QtLogging>

#include "PlaceWatcher.hpp"
#include "Theme.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

// The pill's inner margins and the gap between the mark and the ×, in
// pixels: the Boost pill's (BoostButton.cpp).
constexpr int kLeftPad = 10;
constexpr int kGap = 6;
constexpr int kRightPad = 4;
// Discord's logo mark's proportions (its viewBox, 64 x 48): never
// distorted.
constexpr double kMarkAspect = 48.0 / 64.0;

QString tip() { return DiscordButton::tr("Opens MitCad's Discord server"); }

// The mark's width: the top bar's icon size, as wide as Boost's rocket
// looks beside it.
int markWidth() { return uiIconSize(18); }

// Discord's mark drawn in one colour (it is used unchanged, in one of the
// colours its owner allows: white on the blurple).
QImage markImage(const QSizeF& size, qreal ratio, const QColor& color) {
  const QSize pixels(std::max(1, int(std::lround(size.width() * ratio))),
                     std::max(1, int(std::lround(size.height() * ratio))));
  QImage image(pixels, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QSvgRenderer renderer(QStringLiteral(":/brand/discord-mark.svg"));
  if (!renderer.isValid()) {
    return {};
  }
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  renderer.render(&painter, QRectF(QPointF(0, 0), QSizeF(pixels)));
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(image.rect(), color);
  painter.end();
  image.setDevicePixelRatio(ratio);
  return image;
}

} // namespace

QUrl discordUrl() { return QUrl(QStringLiteral("https://mitcad.org/discord")); }

QString DiscordSettings::visibleKey() { return QStringLiteral("appearance/discordButton"); }

bool DiscordSettings::loadVisible() { return QSettings().value(visibleKey(), true).toBool(); }

bool DiscordSettings::saveVisible(bool visible) {
  if (loadVisible() == visible) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(visibleKey(), visible);
  }
  emit DiscordNotifier::instance().visibilityChanged(visible);
  return true;
}

DiscordNotifier& DiscordNotifier::instance() {
  static DiscordNotifier notifier;
  return notifier;
}

DiscordButton::DiscordButton(QAction* action, int height, QWidget* parent) : QWidget(parent), m_action(action) {
  setObjectName(QStringLiteral("discordButton"));
  setFixedHeight(height);
  setFocusPolicy(Qt::NoFocus);
  setAttribute(Qt::WA_Hover);
  setMouseTracking(true);
  setCursor(Qt::PointingHandCursor);
  setAccessibleName(tr("MitCad on Discord"));
  setToolTip(tip());
  setFixedWidth(sizeHint().width());
  connect(&DiscordNotifier::instance(), &DiscordNotifier::visibilityChanged, this, [this](bool) { applyVisible(); });
  new PlaceWatcher(this, [this] { logPlaces(); });
}

QSize DiscordButton::sizeHint() const {
  // The mark and the × in the Boost pill's margins, as high as it.
  const int cross = height() - 8;
  return QSize(kLeftPad + markWidth() + kGap + cross + kRightPad, height());
}

void DiscordButton::applyVisible() {
  const bool wanted = DiscordSettings::loadVisible();
  setVisible(wanted);
  if (m_logged != int(wanted)) {
    m_logged = int(wanted);
    qDebug().noquote() << (wanted ? QStringLiteral("Discord button shown") : QStringLiteral("Discord button hidden"));
  }
  if (!wanted) {
    // Its places are logged again when it comes back.
    m_loggedPlaces.clear();
  }
}

QRectF DiscordButton::pillRect() const {
  // As the Boost pill: a little slimmer than the row's buttons, on their
  // centre line.
  return QRectF(rect()).adjusted(0.5, 2.5, -0.5, -2.5);
}

QRectF DiscordButton::markRect() const {
  const double width = markWidth();
  const double high = width * kMarkAspect;
  return QRectF(kLeftPad, (height() - high) / 2.0, width, high);
}

QRect DiscordButton::closeRect() const {
  const int side = height() - 8;
  return QRect(width() - kRightPad - side, (height() - side) / 2, side, side);
}

DiscordButton::Part DiscordButton::partAt(const QPoint& point) const {
  if (!rect().contains(point)) {
    return Part::None;
  }
  return closeRect().adjusted(-2, -2, 2, 2).contains(point) ? Part::Close : Part::Pill;
}

bool DiscordButton::event(QEvent* event) {
  if (event->type() == QEvent::ToolTip) {
    // The × says what it does; the pill where it leads.
    auto* help = static_cast<QHelpEvent*>(event);
    const QString text =
        partAt(help->pos()) == Part::Close ? tr("Hides this button; Preferences › Display shows it again") : toolTip();
    QToolTip::showText(help->globalPos(), text, this);
    return true;
  }
  if (event->type() == QEvent::Hide) {
    // Its places are logged again when it shows.
    m_loggedPlaces.clear();
  }
  return QWidget::event(event);
}

void DiscordButton::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  // Discord's blurple on both themes (the owner, mitcad#2159), a shade
  // darker under the pointer and pressed; the mark and the × in white,
  // which reads at least 4.5:1 on it (color_roles).
  const QColor mark = themeColor(ColorRole::DiscordMark, palette());
  QColor fill = themeColor(ColorRole::DiscordFill, palette());
  if (m_hovered == Part::Pill) {
    fill = fill.darker(m_pressed == Part::Pill ? kDiscordPressedDarker : kDiscordHoverDarker);
  }
  const QRectF pill = pillRect();
  // The Boost pill's radius: every card's and bar's (mitcad#1990).
  const qreal radius = std::min<qreal>(kCardRadius, pill.height() / 2);
  painter.setPen(Qt::NoPen);
  painter.setBrush(fill);
  painter.drawRoundedRect(pill, radius, radius);

  // The mark.
  const QRectF markBox = markRect();
  const QImage image = markImage(markBox.size(), devicePixelRatioF(), mark);
  if (!image.isNull()) {
    painter.drawImage(markBox.topLeft(), image);
  }

  // The ×: a light disc behind it while the pointer is over it.
  const QRect closeBox = closeRect();
  if (m_hovered == Part::Close) {
    QColor disc = mark;
    disc.setAlpha(m_pressed == Part::Close ? 82 : 51);
    painter.setPen(Qt::NoPen);
    painter.setBrush(disc);
    painter.drawEllipse(QRectF(closeBox));
  }
  QColor crossInk = mark;
  if (m_hovered != Part::Close) {
    // Quieter than the mark until pointed at.
    crossInk.setAlpha(210);
  }
  QPen cross(crossInk, 1.5);
  cross.setCapStyle(Qt::RoundCap);
  painter.setPen(cross);
  const qreal inset = closeBox.width() * 0.32;
  const QRectF markX = QRectF(closeBox).adjusted(inset, inset, -inset, -inset);
  painter.drawLine(markX.topLeft(), markX.bottomRight());
  painter.drawLine(markX.topRight(), markX.bottomLeft());
}

void DiscordButton::mouseMoveEvent(QMouseEvent* event) {
  const Part part = partAt(event->position().toPoint());
  if (part != m_hovered) {
    m_hovered = part;
    update();
  }
  QWidget::mouseMoveEvent(event);
}

void DiscordButton::mousePressEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    QWidget::mousePressEvent(event);
    return;
  }
  m_pressed = partAt(event->position().toPoint());
  m_hovered = m_pressed;
  update();
  event->accept();
}

void DiscordButton::mouseReleaseEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    QWidget::mouseReleaseEvent(event);
    return;
  }
  const Part pressed = std::exchange(m_pressed, Part::None);
  const Part released = partAt(event->position().toPoint());
  update();
  event->accept();
  if (pressed == Part::None || pressed != released) {
    return;
  }
  if (released == Part::Close) {
    // Hidden in every window, and saved; Preferences shows it again.
    DiscordSettings::saveVisible(false);
    return;
  }
  // help.discord opens the server and logs "Discord link opened: <url>".
  if (m_action != nullptr && m_action->isEnabled()) {
    m_action->trigger();
  }
}

void DiscordButton::leaveEvent(QEvent* event) {
  m_hovered = Part::None;
  update();
  QWidget::leaveEvent(event);
}

void DiscordButton::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    update();
  }
}

void DiscordButton::logPlaces() {
  if (!isVisible() || window() == nullptr) {
    return;
  }
  const QPoint markAt = mapTo(window(), markRect().center().toPoint());
  const QPoint crossAt = mapTo(window(), closeRect().center());
  const QString places = QStringLiteral("Discord button at %1,%2\nDiscord button close at %3,%4")
                             .arg(markAt.x())
                             .arg(markAt.y())
                             .arg(crossAt.x())
                             .arg(crossAt.y());
  if (places == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = places;
  for (const QString& line : places.split(QLatin1Char('\n'))) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
