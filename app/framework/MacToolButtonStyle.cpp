// SPDX-License-Identifier: MIT
#include "MacToolButtonStyle.hpp"

#include <QApplication>
#include <QEvent>
#include <QImage>
#include <QMetaObject>
#include <QPainter>
#include <QPointer>
#include <QProxyStyle>
#include <QStyleFactory>
#include <QStyleOptionToolButton>
#include <QToolButton>

namespace mitcad {
#ifdef MITCAD_MAC_UI
namespace {

class MacToolButtonStyle final : public QProxyStyle {
public:
  MacToolButtonStyle() : QProxyStyle(QStyleFactory::create(QApplication::style()->name())) {
    setObjectName(QStringLiteral("macToolButtonStyle"));
  }

  void drawComplexControl(ComplexControl control, const QStyleOptionComplex* option,
                          QPainter* painter, const QWidget* widget) const override {
    const auto* button = qstyleoption_cast<const QStyleOptionToolButton*>(option);
    if (control != CC_ToolButton || button == nullptr) {
      QProxyStyle::drawComplexControl(control, option, painter, widget);
      return;
    }

    const bool enabled = button->state.testFlag(State_Enabled);
    const bool pressed = button->state.testFlag(State_Sunken);
    const bool checked = button->state.testFlag(State_On);
    const bool hovered = button->state.testFlag(State_MouseOver);
    const QRectF box = QRectF(button->rect).adjusted(0.5, 0.5, -0.5, -0.5);
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(Qt::NoPen);
    if (enabled && (pressed || checked || hovered)) {
      QColor fill = button->palette.color(checked ? QPalette::Highlight : QPalette::ButtonText);
      fill.setAlpha(pressed ? 58 : (checked ? 40 : 24));
      painter->setBrush(fill);
      painter->drawRoundedRect(box, 6, 6);
    }
    if (enabled && button->state.testFlag(State_HasFocus)) {
      QColor focus = button->palette.color(QPalette::Highlight);
      focus.setAlpha(190);
      painter->setPen(QPen(focus, 2));
      painter->setBrush(Qt::NoBrush);
      painter->drawRoundedRect(box.adjusted(1, 1, -1, -1), 5, 5);
    }
    painter->restore();

    // Leave hit testing, size metrics and the split menu's geometry to the
    // platform style. Only the bezel is replaced; icons and text stay native.
    QStyleOptionToolButton label = *button;
    label.state &= ~(State_Sunken | State_MouseOver | State_HasFocus);
    label.rect = subControlRect(CC_ToolButton, button, SC_ToolButton, widget);
    const bool split = button->features.testFlag(QStyleOptionToolButton::MenuButtonPopup);
    const bool menu = button->features.testFlag(QStyleOptionToolButton::HasMenu);
    label.features &= ~(QStyleOptionToolButton::Menu | QStyleOptionToolButton::HasMenu);
    if (menu && !split) {
      const int width = pixelMetric(PM_MenuButtonIndicator, button, widget);
      if (button->direction == Qt::RightToLeft) {
        label.rect.adjust(width, 0, 0, 0);
      } else {
        label.rect.adjust(0, 0, -width, 0);
      }
    }
    drawControl(CE_ToolButtonLabel, &label, painter, widget);
    if (menu) {
      QRect arrow = subControlRect(CC_ToolButton, button, SC_ToolButtonMenu, widget);
      if (!split || arrow.isEmpty()) {
        const int width = pixelMetric(PM_MenuButtonIndicator, button, widget);
        const QRect logical(button->rect.right() - width + 1, button->rect.top(), width,
                            button->rect.height());
        arrow = visualRect(button->direction, button->rect, logical);
      }
      QStyleOption arrowOption = *button;
      arrowOption.rect = QRect(arrow.center() - QPoint(3, 2), QSize(7, 5));
      drawPrimitive(PE_IndicatorArrowDown, &arrowOption, painter, widget);
    }
  }
};

QStyle* sharedStyle() {
  static QPointer<QStyle> style;
  if (style.isNull()) {
    style = new MacToolButtonStyle;
    style->setParent(qApp);
  }
  return style;
}

// QWidget::setStyle can synchronously send StyleChange while the native
// style or a stylesheet wrapper is still being replaced. The enrollment
// marker describes the stable result; it is not a reentrancy lock. Keep a
// separate per-button lock so every filter (including nested roots) sees it.
class ButtonStyleInstallation final {
public:
  explicit ButtonStyleInstallation(QToolButton* button) : m_button(button) {
    button->setProperty("macToolButtonInstalling", true);
  }
  ~ButtonStyleInstallation() {
    if (m_button != nullptr) {
      m_button->setProperty("macToolButtonInstalling", false);
    }
  }

private:
  QPointer<QToolButton> m_button;
};

void queueStyleRecovery(QToolButton* button) {
  if (button->property("macToolButtonRecoveryQueued").toBool()) {
    return;
  }
  button->setProperty("macToolButtonRecoveryQueued", true);
  const QPointer<QToolButton> guarded(button);
  QMetaObject::invokeMethod(button, [guarded] {
    if (guarded == nullptr) {
      return;
    }
    guarded->setProperty("macToolButtonRecoveryQueued", false);
    // StyleChange is sent from inside Qt's outer setStyle transaction. A
    // synchronous correction may be overwritten when that transaction ends.
    // Inspect only its stable result, once, on the next event-loop turn.
    if (guarded->property("macToolButtonInstalling").toBool() || guarded->style() == sharedStyle()) {
      return;
    }
    // WA_StyleSheet can remain set after a stylesheet was cleared, even
    // when the effective style is an unwrapped native style. setStyle keeps
    // any active stylesheet wrapper while replacing its underlying style,
    // so repair both bare styles and wrappers without relying on that flag.
    // The button might have moved or a nested root might have been destroyed
    // since scheduling. Any still-owning installed root can maintain it.
    for (QObject* owner = guarded->parent(); owner != nullptr; owner = owner->parent()) {
      if (owner->findChild<QObject*>(QStringLiteral("macToolButtonFilter"),
                                    Qt::FindDirectChildrenOnly) != nullptr) {
        guarded->setProperty("macFlatToolButton", false);
        styleMacToolButton(guarded);
        break;
      }
    }
  }, Qt::QueuedConnection);
}

class MacToolButtonFilter final : public QObject {
public:
  explicit MacToolButtonFilter(QWidget* root) : QObject(root), m_root(root) {
    setObjectName(QStringLiteral("macToolButtonFilter"));
    qApp->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::Polish || event->type() == QEvent::Show ||
        event->type() == QEvent::ParentChange || event->type() == QEvent::StyleChange) {
      auto* widget = qobject_cast<QWidget*>(watched);
      auto* button = qobject_cast<QToolButton*>(widget);
      if (button != nullptr && button->property("macToolButtonInstalling").toBool()) {
        return false;
      }
      for (QObject* owner = widget; owner != nullptr; owner = owner->parent()) {
        if (owner == m_root) {
          if (event->type() == QEvent::StyleChange && button != nullptr) {
            // Coalesced across all owning filters. Qt keeps active CSS while
            // replacing its base style; no paint event schedules work.
            queueStyleRecovery(button);
          } else if (event->type() != QEvent::StyleChange) {
            if (button != nullptr) {
              styleMacToolButton(button);
            } else {
              // A detached container can already hold polished buttons when
              // a layout adopts it. Those descendants get no new ParentChange
              // of their own (for example a search field's hidden clear icon).
              for (QToolButton* child : widget->findChildren<QToolButton*>()) {
                styleMacToolButton(child);
              }
            }
          }
          break;
        }
      }
    }
    return false;
  }

private:
  QPointer<QWidget> m_root;
};

} // namespace
#endif

void styleMacToolButton(QToolButton* button) {
#ifdef MITCAD_MAC_UI
  if (button != nullptr && !button->property("macToolButtonInstalling").toBool() &&
      !button->property("macFlatToolButton").toBool()) {
    const ButtonStyleInstallation installing(button);
    button->setProperty("macFlatToolButton", true);
    button->setAttribute(Qt::WA_Hover);
    button->setStyle(sharedStyle());
  }
#else
  Q_UNUSED(button);
#endif
}

void installMacToolButtonStyle(QWidget* root) {
#ifdef MITCAD_MAC_UI
  if (root == nullptr || root->findChild<QObject*>(QStringLiteral("macToolButtonFilter"),
                                                 Qt::FindDirectChildrenOnly) != nullptr) {
    return;
  }
  new MacToolButtonFilter(root);
  for (QToolButton* button : root->findChildren<QToolButton*>()) {
    styleMacToolButton(button);
  }
#else
  Q_UNUSED(root);
#endif
}

bool macToolButtonPaintsIdleSurface(const QToolButton* button) {
  QStyleOptionToolButton option;
  option.initFrom(button);
  option.rect = QRect(0, 0, 40, 28);
  option.state = QStyle::State_Enabled;
  option.subControls = QStyle::SC_ToolButton;
  option.toolButtonStyle = Qt::ToolButtonIconOnly;
  option.features = QStyleOptionToolButton::None;
  option.icon = QIcon();
  option.text.clear();
  const qreal scale = button->devicePixelRatioF();
  QImage image(QSize(qRound(40 * scale), qRound(28 * scale)), QImage::Format_ARGB32_Premultiplied);
  image.setDevicePixelRatio(scale);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    button->style()->drawComplexControl(QStyle::CC_ToolButton, &option, &painter, button);
  }
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      if (image.pixelColor(x, y).alpha() != 0) {
        return true;
      }
    }
  }
  return false;
}

} // namespace mitcad
