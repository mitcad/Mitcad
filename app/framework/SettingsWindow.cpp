// SPDX-License-Identifier: MIT
#include "SettingsWindow.hpp"

#include <algorithm>

#include <QCloseEvent>
#include <QFrame>
#include <QHBoxLayout>
#include <QPainter>
#include <QPainterPath>
#include <QShortcut>
#include <QShowEvent>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>

#include "Theme.hpp"

namespace mitcad {

// A button of the row: the icon above the name, a rounded tint behind it when
// the pane is the chosen one (the accent colour) or the pointer is over it.
// It paints with the palette of the moment, so it follows dark mode.
class PaneButton : public QToolButton {
public:
  explicit PaneButton(const QString& title, const QIcon& icon, QWidget* parent = nullptr) : QToolButton(parent) {
    setText(title);
    setIcon(icon);
    setCheckable(true);
    setFocusPolicy(Qt::NoFocus);
    setCursor(Qt::ArrowCursor);
  }

  QSize sizeHint() const override {
    const QFontMetrics metrics(labelFont());
    return QSize(std::max(72, metrics.horizontalAdvance(text()) + 24), 58);
  }

protected:
  void enterEvent(QEnterEvent* event) override {
    QToolButton::enterEvent(event);
    update();
  }
  void leaveEvent(QEvent* event) override {
    QToolButton::leaveEvent(event);
    update();
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QColor accent = accentColor(palette());
    QColor tint = palette().color(QPalette::ButtonText);
    if (isChecked()) {
      tint = accent;
      tint.setAlphaF(0.16f);
    } else if (underMouse()) {
      tint.setAlphaF(0.07f);
    } else {
      tint = Qt::transparent;
    }
    painter.setPen(Qt::NoPen);
    painter.setBrush(tint);
    painter.drawRoundedRect(QRectF(rect()).adjusted(2, 2, -2, -2), 9, 9);

    const QSize iconSize(24, 24);
    const qreal ratio = devicePixelRatioF();
    QPixmap pixmap = icon().pixmap(iconSize, ratio, isEnabled() ? QIcon::Normal : QIcon::Disabled);
    if (isChecked() && !pixmap.isNull()) {
      QPainter tinter(&pixmap);
      tinter.setCompositionMode(QPainter::CompositionMode_SourceIn);
      tinter.fillRect(QRectF(QPointF(0, 0), QSizeF(pixmap.size()) / pixmap.devicePixelRatio()), accent);
    }
    painter.drawPixmap(QRect(QPoint((width() - iconSize.width()) / 2, 7), iconSize), pixmap);

    painter.setFont(labelFont());
    painter.setPen(isChecked() ? accent : palette().color(QPalette::ButtonText));
    painter.drawText(QRect(0, 34, width(), 18), Qt::AlignHCenter | Qt::AlignVCenter, text());
  }

private:
  QFont labelFont() const {
    QFont f = font();
    f.setPointSizeF(std::max(9.0, f.pointSizeF() - 1.0));
    return f;
  }
};

SettingsWindow::SettingsWindow(QWidget* parent) : QWidget(parent, Qt::Window) {
  setAttribute(Qt::WA_DeleteOnClose);
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);
  // The window takes the size of the chosen pane, and cannot be resized.
  layout->setSizeConstraint(QLayout::SetFixedSize);

  m_header = new QWidget;
  auto* row = new QHBoxLayout(m_header);
  row->setContentsMargins(12, 8, 12, 8);
  row->setSpacing(4);
  row->addStretch(1);
  row->addStretch(1);
  layout->addWidget(m_header);
  auto* line = new QFrame;
  line->setFrameShape(QFrame::HLine);
  line->setFrameShadow(QFrame::Sunken);
  layout->addWidget(line);

  m_pages = new QStackedWidget;
  layout->addWidget(m_pages);

  // Cmd+W (Ctrl+W elsewhere) closes the window, not the document.
  auto* close = new QShortcut(QKeySequence::Close, this);
  close->setContext(Qt::WindowShortcut);
  connect(close, &QShortcut::activated, this, &QWidget::close);
}

void SettingsWindow::addPane(const QString& title, const QIcon& icon, QWidget* page) {
  const int index = int(m_buttons.size());
  auto* button = new PaneButton(title, icon);
  m_buttons.push_back(button);
  // Before the stretch at the end.
  auto* row = static_cast<QHBoxLayout*>(m_header->layout());
  row->insertWidget(row->count() - 1, button);
  connect(button, &QToolButton::clicked, this, [this, index] { showPane(index); });

  // Margins of the pane; the stack is as large as its current page only.
  auto* frame = new QWidget;
  auto* inner = new QVBoxLayout(frame);
  inner->setContentsMargins(24, 20, 24, 20);
  inner->addWidget(page);
  m_pages->addWidget(frame);
}

void SettingsWindow::showPane(int index) {
  for (int i = 0; i < m_pages->count(); ++i) {
    m_buttons[std::size_t(i)]->setChecked(i == index);
    // Ignored pages do not count for the stack's size.
    m_pages->widget(i)->setSizePolicy(i == index ? QSizePolicy::Preferred : QSizePolicy::Ignored,
                                      i == index ? QSizePolicy::Preferred : QSizePolicy::Ignored);
  }
  m_pages->setCurrentIndex(index);
  layout()->invalidate();
  layout()->activate();
}

void SettingsWindow::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  // The sizes the layout took before the widgets were polished (the
  // platform's font) were not the final ones.
  layout()->invalidate();
  layout()->activate();
}

void SettingsWindow::closeEvent(QCloseEvent* event) {
  if (m_closed) {
    m_closed();
  }
  QWidget::closeEvent(event);
}

} // namespace mitcad
