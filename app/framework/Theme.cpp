// SPDX-License-Identifier: MIT
#include "Theme.hpp"

#include <optional>

#include <QEvent>
#include <QObject>
#include <QWidget>

namespace mitcad {
namespace {

// Re-applies a widget's themed style sheet when the palette changes. It is a
// child of the widget, so it goes with it. Applying a sheet changes the
// widget's palette too, but a sheet that is already in force is not set
// again, so that does not go round.
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
    if (watched == m_widget &&
        (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange)) {
      apply();
    }
    return false;
  }

private:
  void apply() {
    QString sheet = m_format;
    if (m_format.contains(QLatin1String("%1"))) {
      const QPalette palette = QGuiApplication::palette();
      sheet = m_format.arg((m_warning ? warningColor(palette) : errorColor(palette)).name());
    }
    if (!m_applied.has_value() || *m_applied != sheet) {
      m_applied = sheet;
      m_widget->setStyleSheet(sheet);
    }
  }

  QWidget* m_widget;
  QString m_format;
  bool m_warning = false;
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
}

} // namespace

bool isDarkPalette(const QPalette& palette) { return palette.color(QPalette::Window).lightness() < 128; }

QColor errorColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(0xff, 0x6b, 0x6b) : QColor(0xb0, 0x00, 0x20);
}

QColor warningColor(const QPalette& palette) {
  return isDarkPalette(palette) ? QColor(0xe6, 0xb4, 0x50) : QColor(0x8a, 0x5a, 0x00);
}

QColor accentColor(const QPalette& palette) {
#ifdef Q_OS_MACOS
  return palette.color(QPalette::Accent);
#else
  return isDarkPalette(palette) ? QColor(0x6f, 0xa8, 0xff) : QColor(0x1f, 0x5f, 0xc0);
#endif
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
