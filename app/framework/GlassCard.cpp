// SPDX-License-Identifier: MIT
#include "GlassCard.hpp"

#include <algorithm>
#include <cmath>

#include <QCoreApplication>
#include <QEvent>
#include <QKeyEvent>
#include <QGraphicsOpacityEffect>
#include <QHBoxLayout>
#include <QImage>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QToolButton>
#include <QVBoxLayout>
#include <QWheelEvent>

#include "../platform/MacChrome.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

constexpr int kHeaderHeight = 30;
constexpr int kChevronSize = 20;

// A chevron drawn with the palette's text colour, for where the system has no
// SF Symbols: pointing down (open) or right (collapsed).
QIcon drawnChevron(bool collapsed, const QPalette& palette, qreal ratio) {
  const int side = 20;
  QPixmap pixmap(QSize(side, side) * ratio);
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  QColor color = palette.color(QPalette::ButtonText);
  color.setAlphaF(0.7f);
  QPen pen(color, 1.8);
  pen.setCapStyle(Qt::RoundCap);
  pen.setJoinStyle(Qt::RoundJoin);
  painter.setPen(pen);
  QPainterPath path;
  if (collapsed) {
    path.moveTo(8, 5.5);
    path.lineTo(13, 10);
    path.lineTo(8, 14.5);
  } else {
    path.moveTo(5.5, 8);
    path.lineTo(10, 13);
    path.lineTo(14.5, 8);
  }
  painter.drawPath(path);
  painter.end();
  return QIcon(pixmap);
}

} // namespace

void setFlatCardButton(QToolButton* button) {
  button->setAutoRaise(true);
  button->setStyleSheet(QStringLiteral(
      "QToolButton { border: none; background: transparent; border-radius: 6px; padding: 2px; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 40); }"
      "QToolButton:pressed { background: rgba(128, 128, 128, 80); }"
      "QToolButton:disabled { background: transparent; }"));
}

CapsuleButton::CapsuleButton(const QString& text, bool prominent, QWidget* parent)
    : QToolButton(parent), m_prominent(prominent) {
  setText(text);
  setToolButtonStyle(Qt::ToolButtonTextOnly);
  setFocusPolicy(Qt::NoFocus);
  setAttribute(Qt::WA_Hover);
  setCursor(Qt::ArrowCursor);
}

QSize CapsuleButton::sizeHint() const {
  return QSize(fontMetrics().horizontalAdvance(text()) + 32, 26);
}

void CapsuleButton::changeEvent(QEvent* event) {
  QToolButton::changeEvent(event);
  if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange ||
      event->type() == QEvent::EnabledChange) {
    update();
  }
}

void CapsuleButton::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QPalette pal = palette();
  const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
  const qreal radius = box.height() / 2;
  QColor text = pal.color(QPalette::ButtonText);
  if (m_prominent) {
    QColor fill = accentColor(pal);
    if (!isEnabled()) {
      fill.setAlpha(90);
    } else if (isDown()) {
      fill = fill.darker(125);
    } else if (underMouse()) {
      fill = fill.lighter(110);
    }
    painter.setPen(Qt::NoPen);
    painter.setBrush(fill);
    text = pal.color(QPalette::HighlightedText);
  } else {
    // The same grey tints as the card's other controls (the toggles).
    const int alpha = isDown() ? 90 : (underMouse() && isEnabled() ? 55 : 25);
    painter.setPen(QPen(QColor(128, 128, 128, 70), 1));
    painter.setBrush(QColor(128, 128, 128, alpha));
    if (!isEnabled()) {
      text.setAlphaF(0.4f);
    }
  }
  painter.drawRoundedRect(box, radius, radius);
  QFont font = this->font();
  if (m_prominent) {
    font.setWeight(QFont::DemiBold);
  }
  painter.setFont(font);
  painter.setPen(text);
  painter.drawText(rect(), Qt::AlignCenter, this->text());
}

// The shadow of a card: a mouse-transparent widget under it, a little larger
// than the card, that paints through the card's paintShadow().
class CardShadow : public QWidget {
public:
  explicit CardShadow(GlassCard* card) : QWidget(card->parentWidget()), m_card(card) {
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
  }

  static constexpr int margin() { return GlassCard::kShadowBlur; }

  void follow() {
    setGeometry(m_card->geometry().adjusted(-margin(), -margin(), margin(), margin()));
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setOpacity(m_card->opacityLevel());
    const QRectF cardRect(margin(), margin(), m_card->width(), m_card->height());
    m_card->paintShadow(painter, cardRect, m_card->cornerRadius());
  }

private:
  GlassCard* m_card;
};

GlassCard::GlassCard(QWidget* parent) : QWidget(parent) {
  // Painted entirely by the card; what is under it (the 3D view) shows
  // through the translucent fill.
  setAttribute(Qt::WA_NoSystemBackground);
  m_layout = new QVBoxLayout(this);
  m_layout->setContentsMargins(0, 0, 0, 0);
  m_layout->setSpacing(0);

  m_header = new QWidget(this);
  m_header->setFixedHeight(kHeaderHeight);
  m_headerLayout = new QHBoxLayout(m_header);
  m_headerLayout->setContentsMargins(8, 4, 8, 0);
  m_headerLayout->setSpacing(4);
  m_chevron = new QToolButton(m_header);
  m_chevron->setObjectName(QStringLiteral("cardChevron"));
  setFlatCardButton(m_chevron);
  m_chevron->setFocusPolicy(Qt::NoFocus);
  m_chevron->setFixedSize(kChevronSize, kChevronSize);
  m_chevron->setIconSize(QSize(12, 12));
  m_chevron->setToolTip(tr("Collapse or expand"));
  connect(m_chevron, &QToolButton::clicked, this, [this] { setCollapsed(!m_collapsed); });
  m_titleLabel = new QLabel(m_header);
  QFont font = m_titleLabel->font();
  font.setWeight(QFont::DemiBold);
  font.setPointSizeF(font.pointSizeF() * 0.95);
  m_titleLabel->setFont(font);
  m_titleLabel->setForegroundRole(QPalette::WindowText);
  m_headerLayout->addWidget(m_chevron);
  m_headerLayout->addWidget(m_titleLabel, 1);
  m_layout->addWidget(m_header);
  m_header->hide();

  m_holder = new QWidget(this);
  m_holderLayout = new QVBoxLayout(m_holder);
  m_holderLayout->setContentsMargins(m_contentMargins);
  m_layout->addWidget(m_holder, 1);
  updateChevron();
}

GlassCard::~GlassCard() { delete m_shadow; }

void GlassCard::setTitle(const QString& title) {
  m_titleText = title;
  m_titleLabel->setText(title);
  updateHeader();
}

void GlassCard::setCollapsible(bool collapsible) {
  m_collapsible = collapsible;
  if (!collapsible && m_collapsed) {
    setCollapsed(false);
  }
  updateHeader();
}

void GlassCard::setCollapsed(bool collapsed) {
  if (collapsed == m_collapsed || (collapsed && !m_collapsible)) {
    return;
  }
  m_collapsed = collapsed;
  m_holder->setVisible(!collapsed);
  updateChevron();
  emit collapsedChanged(collapsed);
}

void GlassCard::addHeaderWidget(QWidget* widget) {
  widget->setParent(m_header);
  m_headerLayout->addWidget(widget);
  m_hasHeaderWidgets = true;
  updateHeader();
}

void GlassCard::setContent(QWidget* content, const QMargins& margins) {
  if (m_content != nullptr) {
    m_holderLayout->removeWidget(m_content);
    m_content->deleteLater();
  }
  m_content = content;
  m_contentMargins = margins;
  m_holderLayout->setContentsMargins(margins);
  if (content != nullptr) {
    m_holderLayout->addWidget(content);
  }
}

void GlassCard::setCapsule(bool capsule) {
  m_capsule = capsule;
  update();
  if (m_shadow) {
    m_shadow->update();
  }
}

qreal GlassCard::cornerRadius() const {
  const qreal radius = m_capsule ? height() / 2.0 : static_cast<qreal>(kRadius);
  return std::min(radius, std::min(width(), height()) / 2.0);
}

void GlassCard::setBackdrop(Backdrop backdrop) {
  m_backdrop = backdrop;
  update();
  if (m_shadow) {
    m_shadow->update();
  }
}

void GlassCard::setWindowed(bool windowed) {
  if (windowed == m_windowed) {
    return;
  }
  m_windowed = windowed;
  m_wanted = !isHidden();
  if (windowed) {
    // A tool window, not a plain one: Qt then takes the shortcuts of the
    // parent window as active while a card has the focus (the main window's
    // commands keep working). It is not shown on its own at start, see
    // setVisible. The native glass is attached when it is shown.
    setParent(parentWidget(), Qt::Tool | Qt::FramelessWindowHint | Qt::NoDropShadowWindowHint |
                                  (testAttribute(Qt::WA_TransparentForMouseEvents) ? Qt::WindowTransparentForInput
                                                                                   : Qt::WindowFlags()));
    setAttribute(Qt::WA_TranslucentBackground);
    setAttribute(Qt::WA_ShowWithoutActivating); // a card that appears does not take the keys
    setBackdrop(Backdrop::External);
    delete m_shadow;
  } else {
    setParent(parentWidget());
    setAttribute(Qt::WA_TranslucentBackground, false);
    setBackdrop(Backdrop::Painted);
    attachShadow();
  }
}

void GlassCard::setSlot(const QRect& slot) {
  m_slot = slot;
  if (m_windowed) {
    setGeometry(QRect(parentWidget()->mapToGlobal(slot.topLeft()), slot.size()));
  } else {
    setGeometry(slot);
  }
}

QWidget* GlassCard::hostWindow(const QWidget* widget) {
  QWidget* window = widget->window();
  const auto* card = qobject_cast<const GlassCard*>(window);
  if (card != nullptr && card->m_windowed && card->parentWidget() != nullptr) {
    return card->parentWidget()->window();
  }
  return window;
}

QPoint GlassCard::mapToHost(const QWidget* widget, const QPoint& point) {
  return hostWindow(widget)->mapFromGlobal(widget->mapToGlobal(point));
}

void GlassCard::hostVisibilityChanged(bool hostVisible) {
  if (m_windowed && m_wanted) {
    QWidget::setVisible(hostVisible);
  }
}

void GlassCard::setVisible(bool visible) {
  if (!m_windowed) {
    QWidget::setVisible(visible);
    return;
  }
  m_wanted = visible;
  // Qt shows a window at once, whether its parent is shown or not; it
  // would then float on its own until the host appears (and could not be
  // attached to it).
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  QWidget::setVisible(visible && host != nullptr && host->isVisible());
}

void GlassCard::syncNativeWindow() {
  mac::attachGlassWindow(this, parentWidget()->window(), cornerRadius());
}

void GlassCard::setOpacityLevel(qreal opacity) {
  m_opacity = std::clamp(opacity, 0.0, 1.0);
  if (m_windowed) {
    setWindowOpacity(m_opacity); // the window's alpha: the glass fades too
    return;
  }
  if (m_opacity >= 1.0) {
    // No effect at full opacity: it would render the card to a pixmap.
    if (m_effect != nullptr) {
      setGraphicsEffect(nullptr); // deletes it
      m_effect = nullptr;
    }
  } else {
    if (m_effect == nullptr) {
      m_effect = new QGraphicsOpacityEffect(this);
      setGraphicsEffect(m_effect);
    }
    m_effect->setOpacity(m_opacity);
  }
  if (m_shadow) {
    m_shadow->update();
  }
}

int GlassCard::headerHeight() const { return m_header->isHidden() && !headerWanted() ? 0 : kHeaderHeight; }

int GlassCard::heightForContent(int contentHeight) const {
  if (m_collapsed) {
    return headerHeight();
  }
  return headerHeight() + m_contentMargins.top() + m_contentMargins.bottom() + contentHeight;
}

bool GlassCard::headerWanted() const { return !m_titleText.isEmpty() || m_collapsible || m_hasHeaderWidgets; }

void GlassCard::updateHeader() {
  m_header->setVisible(headerWanted());
  m_chevron->setVisible(m_collapsible);
  m_titleLabel->setVisible(!m_titleText.isEmpty());
  emit collapsedChanged(m_collapsed); // the header's height may have changed
}

void GlassCard::updateChevron() {
  const QIcon fallback = drawnChevron(m_collapsed, palette(), devicePixelRatioF());
  m_chevron->setIcon(
      mac::symbolIcon(m_collapsed ? QStringLiteral("chevron.right") : QStringLiteral("chevron.down"), fallback));
}

void GlassCard::attachShadow() {
  if (m_shadow || m_windowed || parentWidget() == nullptr) {
    return;
  }
  m_shadow = new CardShadow(this);
  m_shadow->follow();
  m_shadow->setVisible(isVisible());
  m_shadow->stackUnder(this);
}

void GlassCard::paintMaterial(QPainter& painter, const QRectF& rect, qreal radius) {
  const qreal hairline = 1.0 / devicePixelRatioF();
  const QRectF inner = rect.adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
  painter.setPen(QPen(cardBorderColor(palette()), hairline));
  painter.setBrush(cardFillColor(palette()));
  painter.drawRoundedRect(inner, radius - hairline / 2, radius - hairline / 2);
}

// The shadow: a rounded rectangle shifted down by kShadowOffset, blurred over
// kShadowBlur by stacking layers of growing size whose alphas add up to a
// smooth fall-off (about half the strength at the card's edge). Rendered
// once per size and colour; the card's own area is cut out so that the
// translucent fill does not darken over it.
void GlassCard::paintShadow(QPainter& painter, const QRectF& cardRect, qreal radius) {
  if (m_backdrop == Backdrop::External) {
    return;
  }
  const QColor color = cardShadowColor(palette());
  const qreal ratio = painter.device()->devicePixelRatioF();
  const QSize size(static_cast<int>(cardRect.width()) + 2 * kShadowBlur,
                   static_cast<int>(cardRect.height()) + 2 * kShadowBlur);
  const quint64 key = (static_cast<quint64>(size.width()) << 40) ^ (static_cast<quint64>(size.height()) << 24) ^
                      (static_cast<quint64>(color.rgba()) << 1) ^ static_cast<quint64>(radius * 4) ^
                      static_cast<quint64>(ratio * 100) << 8;
  if (m_shadowKey != key || m_shadowPixmap.isNull()) {
    constexpr int kLayers = 24;
    // 16 bits per channel: the layers' alphas are a few per cent each and
    // would band at 8 bits.
    QImage image(size * ratio, QImage::Format_RGBA64_Premultiplied);
    image.setDevicePixelRatio(ratio);
    image.fill(Qt::transparent);
    QPainter layers(&image);
    layers.setRenderHint(QPainter::Antialiasing);
    layers.setCompositionMode(QPainter::CompositionMode_Plus);
    layers.setPen(Qt::NoPen);
    const auto profile = [](qreal x) { // 1 inside, 0 outside, over [-1, 1]
      const qreal t = std::clamp((x + 1.0) / 2.0, 0.0, 1.0);
      return 1.0 - t * t * (3.0 - 2.0 * t);
    };
    const QRectF base(kShadowBlur, kShadowBlur + kShadowOffset, cardRect.width(), cardRect.height());
    const qreal half = kShadowBlur / 2.0;
    qreal previous = 1.0;
    for (int k = 0; k < kLayers; ++k) {
      const qreal x = -1.0 + 2.0 * (k + 1) / kLayers;
      const qreal weight = previous - profile(x);
      previous = profile(x);
      const qreal grow = x * half;
      QColor layer = color;
      layer.setAlphaF(static_cast<float>(std::min(1.0, weight * color.alphaF())));
      layers.setBrush(layer);
      layers.drawRoundedRect(base.adjusted(-grow, -grow, grow, grow), std::max(0.0, radius + grow),
                             std::max(0.0, radius + grow));
    }
    layers.end();
    m_shadowPixmap = QPixmap::fromImage(image.convertToFormat(QImage::Format_ARGB32_Premultiplied));
    m_shadowPixmap.setDevicePixelRatio(ratio);
    m_shadowKey = key;
  }
  QPainterPath outside;
  outside.addRect(QRectF(QPointF(0, 0), QSizeF(size)));
  QPainterPath card;
  card.addRoundedRect(cardRect, radius, radius);
  painter.setClipPath(outside.subtracted(card));
  painter.drawPixmap(QPoint(0, 0), m_shadowPixmap);
}

void GlassCard::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  if (m_backdrop == Backdrop::Painted) {
    paintMaterial(painter, QRectF(rect()), cornerRadius());
  }
}

void GlassCard::mousePressEvent(QMouseEvent* event) { event->accept(); }

void GlassCard::mouseReleaseEvent(QMouseEvent* event) {
  // A click on the header's empty part folds like the chevron.
  if (m_collapsible && event->button() == Qt::LeftButton && !m_header->isHidden() &&
      m_header->geometry().contains(event->position().toPoint())) {
    setCollapsed(!m_collapsed);
  }
  event->accept();
}

void GlassCard::wheelEvent(QWheelEvent* event) { event->accept(); }

// A click on a card's label or background makes its window the key window
// although nothing in it takes keys: they go on to where the user was (the
// main window's focus widget, usually the 3D view), as they did when the
// card was a part of that window.
bool GlassCard::forwardKey(QKeyEvent* event) {
  if (!m_windowed || focusWidget() != nullptr || parentWidget() == nullptr) {
    return false;
  }
  QWidget* target = parentWidget()->window()->focusWidget();
  if (target == nullptr) {
    return false;
  }
  QKeyEvent copy(event->type(), event->key(), event->modifiers(), event->text(), event->isAutoRepeat(),
                 static_cast<ushort>(event->count()));
  QCoreApplication::sendEvent(target, &copy);
  event->setAccepted(copy.isAccepted());
  return true;
}

void GlassCard::keyPressEvent(QKeyEvent* event) {
  if (!forwardKey(event)) {
    QWidget::keyPressEvent(event);
  }
}

void GlassCard::keyReleaseEvent(QKeyEvent* event) {
  if (!forwardKey(event)) {
    QWidget::keyReleaseEvent(event);
  }
}

void GlassCard::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) {
    updateChevron();
    m_shadowKey = 0; // the colour changed
    update();
    if (m_shadow) {
      m_shadow->update();
    }
  }
}

bool GlassCard::event(QEvent* event) {
  switch (event->type()) {
  case QEvent::ParentChange:
    delete m_shadow;
    attachShadow();
    break;
  case QEvent::Polish:
    attachShadow();
    break;
  case QEvent::Move:
  case QEvent::Resize:
    if (m_shadow) {
      m_shadow->follow();
    }
    if (m_windowed && event->type() == QEvent::Resize && isVisible()) {
      mac::setGlassCornerRadius(this, cornerRadius()); // a capsule's radius follows its height
    }
    break;
  case QEvent::Show:
    if (m_windowed) {
      syncNativeWindow();
      break;
    }
    attachShadow();
    if (m_shadow) {
      m_shadow->follow();
      m_shadow->stackUnder(this);
      m_shadow->show();
    }
    break;
  case QEvent::Hide:
    if (m_shadow) {
      m_shadow->hide();
    }
    break;
  case QEvent::ZOrderChange:
    if (m_shadow) {
      m_shadow->stackUnder(this);
    }
    break;
  default:
    break;
  }
  return QWidget::event(event);
}

} // namespace mitcad
