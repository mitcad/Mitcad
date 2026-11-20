// SPDX-License-Identifier: MIT
#include "BoostButton.hpp"

#include <algorithm>
#include <utility>

#include <QEvent>
#include <QFontMetrics>
#include <QHelpEvent>
#include <QIcon>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QSettings>
#include <QToolTip>
#include <QtLogging>

#include "Icons.hpp"
#include "PlaceWatcher.hpp"
#include "Theme.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

// The pill's inner margins and the gaps between its parts, in pixels.
constexpr int kLeftPad = 10;
constexpr int kGap = 6;
constexpr int kRightPad = 4;
// The compact form (setCompact): left of the rocket, and between the
// rocket and the ×.
constexpr int kCompactPad = 8;
constexpr int kCompactGap = 4;

QString label() { return BoostButton::tr("Boost development"); }

// The rocket drawn in one colour: the icon's outline as a mask.
QPixmap tinted(const QIcon& icon, int side, qreal ratio, const QColor& color) {
  QPixmap pixmap = icon.pixmap(QSize(side, side), ratio);
  if (pixmap.isNull()) {
    return pixmap;
  }
  QPainter painter(&pixmap);
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(pixmap.rect(), color);
  return pixmap;
}

} // namespace

QString BoostSettings::visibleKey() { return QStringLiteral("appearance/boostButton"); }

bool BoostSettings::loadVisible() { return QSettings().value(visibleKey(), true).toBool(); }

bool BoostSettings::saveVisible(bool visible) {
  if (loadVisible() == visible) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(visibleKey(), visible);
  }
  emit BoostNotifier::instance().visibilityChanged(visible);
  return true;
}

QUrl BoostSettings::url() { return QUrl(QStringLiteral("https://ko-fi.com/mitcad")); }

BoostNotifier& BoostNotifier::instance() {
  static BoostNotifier notifier;
  return notifier;
}

BoostButton::BoostButton(int height, std::function<void(const QUrl&)> open, QWidget* parent)
    : QWidget(parent), m_open(std::move(open)) {
  setObjectName(QStringLiteral("boostButton"));
  setFixedHeight(height);
  setFocusPolicy(Qt::NoFocus);
  setAttribute(Qt::WA_Hover);
  setMouseTracking(true);
  setCursor(Qt::PointingHandCursor);
  setAccessibleName(label());
  setToolTip(tr("Opens MitCad's Ko-fi page"));
  // As wide as its label wants, narrower down to minimumSizeHint.
  setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
  connect(&BoostNotifier::instance(), &BoostNotifier::visibilityChanged, this, [this](bool) { applyVisible(); });
  new PlaceWatcher(this, [this] { logPlaces(); });
}

int BoostButton::wideWidth() const {
  const int icon = uiIconSize(16);
  const int cross = height() - 8;
  // The label is drawn semi-bold (paintEvent).
  QFont bold = font();
  bold.setWeight(QFont::DemiBold);
  const int text = QFontMetrics(bold).horizontalAdvance(label()) + 2;
  return kLeftPad + icon + kGap + text + kGap + cross + kRightPad;
}

int BoostButton::compactWidth() const {
  return kCompactPad + uiIconSize(16) + kCompactGap + (height() - 8) + kRightPad;
}

QSize BoostButton::sizeHint() const { return QSize(m_compact ? compactWidth() : wideWidth(), height()); }

QSize BoostButton::minimumSizeHint() const {
  if (m_compact) {
    return sizeHint();
  }
  const int cross = height() - 8;
  return QSize(kLeftPad + uiIconSize(16) + kGap + cross + kRightPad, height());
}

void BoostButton::setCompact(bool compact) {
  if (compact == m_compact) {
    return;
  }
  m_compact = compact;
  // The name, cut away, where the pointer finds it.
  setToolTip(compact ? QStringLiteral("%1\n%2").arg(label(), tr("Opens MitCad's Ko-fi page"))
                     : tr("Opens MitCad's Ko-fi page"));
  m_hovered = Part::None;
  updateGeometry();
  update();
  qDebug().noquote() << (compact ? QStringLiteral("Boost button compact") : QStringLiteral("Boost button wide"));
}

void BoostButton::applyVisible() {
  const bool wanted = BoostSettings::loadVisible();
  setVisible(wanted);
  if (m_logged != int(wanted)) {
    m_logged = int(wanted);
    qDebug().noquote() << (wanted ? QStringLiteral("Boost button shown") : QStringLiteral("Boost button hidden"));
  }
  if (!wanted) {
    // Its places are logged again when it comes back.
    m_loggedPlaces.clear();
  }
}

QRectF BoostButton::pillRect() const {
  // A little slimmer than the row's buttons, on their centre line.
  return QRectF(rect()).adjusted(0.5, 2.5, -0.5, -2.5);
}

QRect BoostButton::closeRect() const {
  const int side = height() - 8;
  return QRect(width() - kRightPad - side, (height() - side) / 2, side, side);
}

BoostButton::Part BoostButton::partAt(const QPoint& point) const {
  if (!rect().contains(point)) {
    return Part::None;
  }
  return closeRect().adjusted(-2, -2, 2, 2).contains(point) ? Part::Close : Part::Pill;
}

bool BoostButton::event(QEvent* event) {
  if (event->type() == QEvent::ToolTip) {
    // The × says what it does; the pill where it leads.
    auto* help = static_cast<QHelpEvent*>(event);
    const QString tip = partAt(help->pos()) == Part::Close
                            ? tr("Hides this button; Preferences › Display shows it again")
                            : toolTip();
    QToolTip::showText(help->globalPos(), tip, this);
    return true;
  }
  return QWidget::event(event);
}

void BoostButton::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  // Quiet until pointed at (owner, test build 10): an outline and a rocket
  // in the Support pink, the label in the window's text; a translucent
  // tint of the pink fills it only under the pointer, so the label keeps
  // its contrast on both themes (color_roles checks the pressed tint).
  const QColor support = themeColor(ColorRole::Support, pal);
  const QColor ink = themeColor(ColorRole::Text, pal);
  const QRectF pill = pillRect();
  // A top bar pill: every card's and bar's radius (mitcad#1990).
  const qreal radius = std::min<qreal>(kCardRadius, pill.height() / 2);
  if (m_hovered == Part::Pill) {
    QColor tint = support;
    tint.setAlpha(m_pressed == Part::Pill ? kBoostPressedTint : kBoostHoverTint);
    painter.setPen(Qt::NoPen);
    painter.setBrush(tint);
    painter.drawRoundedRect(pill, radius, radius);
  }
  QPen outline(support, 1.0);
  painter.setPen(outline);
  painter.setBrush(Qt::NoBrush);
  painter.drawRoundedRect(pill, radius, radius);

  // The rocket, then the label.
  const int icon = uiIconSize(16);
  const QPixmap rocket =
      tinted(themeIcon(QStringLiteral("boost"), IconBackdrop::Light), icon, devicePixelRatioF(), support);
  // The compact form: the rocket and the × close together, no label.
  painter.drawPixmap(QPointF(m_compact ? kCompactPad : kLeftPad, (height() - icon) / 2.0), rocket);
  const QRect closeBox = closeRect();
  const int textLeft = kLeftPad + icon + kGap;
  QFont bold = font();
  bold.setWeight(QFont::DemiBold);
  painter.setFont(bold);
  painter.setPen(ink);
  const int textWidth = closeBox.left() - kGap - textLeft;
  if (!m_compact && textWidth > 0) {
    painter.drawText(QRect(textLeft, 0, textWidth, height()), Qt::AlignVCenter | Qt::AlignLeft,
                     QFontMetrics(bold).elidedText(label(), Qt::ElideRight, textWidth));
  }

  // The ×: a light disc behind it while the pointer is over it.
  if (m_hovered == Part::Close) {
    QColor disc = ink;
    disc.setAlpha(m_pressed == Part::Close ? 60 : 36);
    painter.setPen(Qt::NoPen);
    painter.setBrush(disc);
    painter.drawEllipse(QRectF(closeBox));
  }
  QColor crossInk = themeColor(ColorRole::TextSecondary, pal);
  if (m_hovered == Part::Close) {
    crossInk = ink;
  }
  QPen cross(crossInk, 1.5);
  cross.setCapStyle(Qt::RoundCap);
  painter.setPen(cross);
  const qreal inset = closeBox.width() * 0.32;
  const QRectF mark = QRectF(closeBox).adjusted(inset, inset, -inset, -inset);
  painter.drawLine(mark.topLeft(), mark.bottomRight());
  painter.drawLine(mark.topRight(), mark.bottomLeft());
}

void BoostButton::mouseMoveEvent(QMouseEvent* event) {
  const Part part = partAt(event->position().toPoint());
  if (part != m_hovered) {
    m_hovered = part;
    update();
  }
  QWidget::mouseMoveEvent(event);
}

void BoostButton::mousePressEvent(QMouseEvent* event) {
  if (event->button() != Qt::LeftButton) {
    QWidget::mousePressEvent(event);
    return;
  }
  m_pressed = partAt(event->position().toPoint());
  m_hovered = m_pressed;
  update();
  event->accept();
}

void BoostButton::mouseReleaseEvent(QMouseEvent* event) {
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
    BoostSettings::saveVisible(false);
    return;
  }
  const QUrl url = BoostSettings::url();
  qDebug().noquote() << QStringLiteral("Boost link opened: %1").arg(url.toString());
  if (m_open) {
    m_open(url);
  }
}

void BoostButton::leaveEvent(QEvent* event) {
  m_hovered = Part::None;
  update();
  QWidget::leaveEvent(event);
}

void BoostButton::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::FontChange || event->type() == QEvent::StyleChange) {
    updateGeometry();
  }
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    update();
  }
}

void BoostButton::logPlaces() {
  if (!isVisible() || window() == nullptr) {
    return;
  }
  const QRect closeBox = closeRect();
  const int textLeft = kLeftPad + uiIconSize(16) + kGap;
  // The label's middle; in the compact form the rocket's.
  const QPoint text = m_compact ? mapTo(window(), QPoint(kCompactPad + uiIconSize(16) / 2, height() / 2))
                                : mapTo(window(), QPoint((textLeft + closeBox.left() - kGap) / 2, height() / 2));
  const QPoint cross = mapTo(window(), closeBox.center());
  const QString places = QStringLiteral("Boost button at %1,%2\nBoost button close at %3,%4")
                             .arg(text.x())
                             .arg(text.y())
                             .arg(cross.x())
                             .arg(cross.y());
  if (places == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = places;
  for (const QString& line : places.split(QLatin1Char('\n'))) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
