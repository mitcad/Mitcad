// SPDX-License-Identifier: MIT
#include "Theme.hpp"

#include <optional>

#include <QEvent>
#include <QObject>
#include <QRegularExpression>
#include <QTimer>
#include <QVariant>
#include <QWidget>

namespace mitcad {
namespace {

// Re-applies a widget's themed style sheet when the palette changes. It is a
// child of the widget, so it goes with it. Applying a sheet changes the
// widget's palette too, but a sheet that is already in force is not set
// again, so that does not go round. Qt does not pass a parent's palette
// change on to a child with a style sheet of its own (mitcad#961: a badge
// stayed the light theme's dark red on a dark window), so the sheet is
// also checked when the widget shows and paints, and set again after the
// paint when it is stale.
class ThemeWatcher : public QObject {
public:
  explicit ThemeWatcher(QWidget* widget) : QObject(widget), m_widget(widget) {
    widget->installEventFilter(this);
  }

  void setFormat(const QString& format, bool warning) {
    m_format = format;
    m_warning = warning;
    m_applied.reset();
    apply();
  }

  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched != m_widget) {
      return false;
    }
    switch (event->type()) {
    case QEvent::ApplicationPaletteChange:
    case QEvent::PaletteChange:
    case QEvent::ParentChange:
    case QEvent::Show:
      apply();
      break;
    case QEvent::Paint:
      if (!m_pending && m_applied.has_value() && *m_applied != sheet()) {
        m_pending = true;
        QTimer::singleShot(0, this, [this] {
          m_pending = false;
          apply();
          m_widget->update();
        });
      }
      break;
    default:
      break;
    }
    return false;
  }

private:
  // The format with the colour for what the widget sits on (mitcad#961): a
  // dark card or window may be on an application palette that is light.
  // The first ancestor with an opaque background (the widget's own sheet
  // does not change theirs), else the application's.
  QString sheet() const {
    if (!m_format.contains(QLatin1String("%1"))) {
      return m_format;
    }
    QPalette palette = QGuiApplication::palette();
    for (const QWidget* up = m_widget->parentWidget(); up != nullptr; up = up->parentWidget()) {
      if (up->palette().color(QPalette::Window).alpha() == 255) {
        palette = up->palette();
        break;
      }
    }
    return m_format.arg((m_warning ? warningColor(palette) : errorColor(palette)).name());
  }

  void apply() {
    const QString wanted = sheet();
    if (!m_applied.has_value() || *m_applied != wanted) {
      m_applied = wanted;
      m_widget->setStyleSheet(wanted);
    }
  }

  QWidget* m_widget;
  QString m_format;
  bool m_warning = false;
  bool m_pending = false; // a stale sheet seen in a paint, set again after it
  std::optional<QString> m_applied;
};

void setThemedStyleSheet(QWidget* widget, const QString& format, bool warning) {
  if (widget == nullptr) {
    return;
  }
  ThemeWatcher* watcher = nullptr;
  for (QObject* child : widget->children()) {
    if (auto* found = dynamic_cast<ThemeWatcher*>(child)) {
      watcher = found;
      break;
    }
  }
  if (watcher == nullptr) {
    watcher = new ThemeWatcher(widget);
  }
  watcher->setFormat(format, warning);
  // The state, not the colour, for presenters without style sheets.
  static const QRegularExpression textColour(QStringLiteral("(^|[\\s;{])color\\s*:\\s*%1"));
  const QVariant tone = textColour.match(format).hasMatch()
                            ? QVariant(warning ? QStringLiteral("warning") : QStringLiteral("error"))
                            : QVariant();
  if (widget->property(kSemanticTextToneProperty) != tone) {
    widget->setProperty(kSemanticTextToneProperty, tone);
  }
}

} // namespace

bool isDarkPalette(const QPalette& palette) { return palette.color(QPalette::Window).lightness() < 128; }

QColor errorColor(const QPalette& palette) {
  // At least 4.5:1 on a dark window, panel or Floating card (#464748, mitcad#961).
  return isDarkPalette(palette) ? QColor(0xff, 0x9c, 0x9c) : QColor(0xb0, 0x00, 0x20);
}

QColor warningColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(0xe6, 0xb4, 0x50) : QColor(0x8a, 0x5a, 0x00);
}

QColor accentColor(const QPalette& palette) {
#ifdef Q_OS_MACOS
  return palette.color(QPalette::Accent);
#else
  return actionColor(palette);
#endif
}

QColor actionColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(0x5a, 0xa2, 0xff) : QColor(0x1a, 0x6e, 0xe0);
}

QColor cardFillColor(const QPalette& palette) {
  QColor color = palette.color(QPalette::Window);
  color.setAlphaF(isDarkPalette(palette) ? 0.80f : 0.85f);
  return color;
}

QColor cardBorderColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(255, 255, 255, 31) : QColor(0, 0, 0, 26);
}

QColor cardShadowColor(const QPalette& palette) {
  return QColor(0, 0, 0, isDarkPalette(palette) ? 115 : 46);
}

void setErrorStyleSheet(QWidget* widget, const QString& format) { setThemedStyleSheet(widget, format, false); }

void setWarningStyleSheet(QWidget* widget, const QString& format) { setThemedStyleSheet(widget, format, true); }

} // namespace mitcad
