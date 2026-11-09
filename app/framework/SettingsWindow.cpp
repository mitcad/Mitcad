// SPDX-License-Identifier: MIT
#include "SettingsWindow.hpp"

#include <algorithm>

#include <QCloseEvent>
#include <QDebug>
#include <QFrame>
#include <QGraphicsOpacityEffect>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QPainter>
#include <QPainterPath>
#include <QPropertyAnimation>
#include <QShortcut>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QStackedWidget>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QVariant>

#include "Theme.hpp"
#include "UiSizes.hpp"

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
    f.setPointSizeF(std::max(9.0 * uiTextScale(), f.pointSizeF() - 1.0));
    return f;
  }
};

SettingsWindow::SettingsWindow(QWidget* parent, Selector selector) : QWidget(parent, Qt::Window) {
  setAttribute(Qt::WA_DeleteOnClose);
  m_pages = new QStackedWidget;
  auto* layout = new QVBoxLayout(this);
  // The window takes the size of the chosen pane, and cannot be resized.
  layout->setSizeConstraint(QLayout::SetFixedSize);
  if (selector == Selector::PageList) {
    layout->setContentsMargins(12, 12, 12, 12);
    auto* row = new QHBoxLayout;
    m_list = new QListWidget;
    m_list->setObjectName(QStringLiteral("settingsPageList"));
    m_list->setSpacing(2);
    m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    row->addWidget(m_list, 0, Qt::AlignTop);
    row->addWidget(m_pages, 1);
    layout->addLayout(row);
    connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
      if (row >= 0) showPane(row);
    });
  } else {
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

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
    layout->addWidget(m_pages);
  }

  // The footer under the pane (mitcad#1479): a quiet note that a change was
  // saved, at the right, where it covers nothing. Its row keeps its height
  // while the note is empty, so the window does not change size.
  m_savedNote = new QLabel;
  m_savedNote->setObjectName(QStringLiteral("settingsSavedNote"));
  m_savedNote->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
  m_savedNote->setEnabled(false); // the quiet colour of a hint
  m_savedNote->setProperty("noteShown", false);
  m_savedOpacity = new QGraphicsOpacityEffect(m_savedNote);
  m_savedOpacity->setOpacity(0.0);
  m_savedNote->setGraphicsEffect(m_savedOpacity);
  auto* footer = new QHBoxLayout;
  // The pane's own margins at the sides (addPane), so that the note ends
  // where the pane's controls do; below it the window's margin.
  footer->setContentsMargins(24, 0, 24, selector == Selector::IconRow ? 12 : 0);
  footer->addWidget(m_savedNote, 1);
  layout->addLayout(footer);
  m_savedFade = new QPropertyAnimation(m_savedOpacity, "opacity", this);
  connect(m_savedFade, &QPropertyAnimation::finished, this, [this] {
    if (m_savedOpacity->opacity() <= 0.0) {
      m_savedNote->clear();
      m_savedNote->setProperty("noteShown", false);
      qDebug().noquote() << QStringLiteral("Settings: saved note faded");
    }
  });
  m_savedHold = new QTimer(this);
  m_savedHold->setSingleShot(true);
  m_savedHold->setInterval(3000);
  connect(m_savedHold, &QTimer::timeout, this, &SettingsWindow::fadeSavedNote);

  // Cmd+W (Ctrl+W elsewhere) closes the window, not the document.
  auto* close = new QShortcut(QKeySequence::Close, this);
  close->setContext(Qt::WindowShortcut);
  connect(close, &QShortcut::activated, this, &QWidget::close);
}

void SettingsWindow::addPane(const QString& title, const QIcon& icon, QWidget* page) {
  page->setProperty("nativePaneTitle", title);
  if (m_list != nullptr) {
    m_list->addItem(title);
    // As wide as its entries, with room for a scroll bar.
    ensurePolished();
    m_list->setFixedWidth(m_list->sizeHintForColumn(0) + 2 * m_list->spacing() + 2 * m_list->frameWidth() +
                          m_list->style()->pixelMetric(QStyle::PM_ScrollBarExtent, nullptr, m_list) +
                          m_list->fontMetrics().averageCharWidth());
    m_list->setFixedHeight(m_list->count() * (m_list->sizeHintForRow(0) + 2 * m_list->spacing()) +
                           2 * m_list->frameWidth() + 2 * m_list->spacing());
  } else {
    const int index = int(m_buttons.size());
    auto* button = new PaneButton(title, icon);
    m_buttons.push_back(button);
    // Before the stretch at the end.
    auto* row = static_cast<QHBoxLayout*>(m_header->layout());
    row->insertWidget(row->count() - 1, button);
    connect(button, &QToolButton::clicked, this, [this, index] { showPane(index); });
  }

  // Margins of the pane; the stack is as large as its current page only.
  auto* frame = new QWidget;
  frame->setProperty("nativePaneTitle", title);
  // The native selector shows the same icon above the name (mitcad#971).
  frame->setProperty("nativePaneIcon", QVariant::fromValue(icon));
  auto* inner = new QVBoxLayout(frame);
  inner->setContentsMargins(24, 20, 24, 20);
  inner->addWidget(page);
  m_pages->addWidget(frame);
}

void SettingsWindow::showPane(int index) {
  if (m_list != nullptr && m_list->currentRow() != index) {
    const QSignalBlocker quiet(m_list);
    m_list->setCurrentRow(index);
  }
  for (int i = 0; i < m_pages->count(); ++i) {
    if (std::size_t(i) < m_buttons.size()) m_buttons[std::size_t(i)]->setChecked(i == index);
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
  // The note's row is as tall as a line of its text, empty or not.
  m_savedNote->setMinimumHeight(m_savedNote->fontMetrics().height());
  // The sizes the layout took before the widgets were polished (the
  // platform's font) were not the final ones.
  layout()->invalidate();
  layout()->activate();
}

QString SettingsWindow::savedNoteText() {
  return tr("Changes are saved automatically");
}

void SettingsWindow::noteSaved() {
  m_savedNote->setText(savedNoteText());
  // A property change also tells the native frontend's mirror to refresh.
  m_savedNote->setProperty("noteShown", true);
  m_savedFade->stop();
  m_savedFade->setDuration(150);
  m_savedFade->setStartValue(m_savedOpacity->opacity());
  m_savedFade->setEndValue(1.0);
  m_savedFade->start();
  m_savedHold->start();
  qDebug().noquote() << QStringLiteral("Settings: saved note shown");
}

void SettingsWindow::fadeSavedNote() {
  m_savedFade->stop();
  m_savedFade->setDuration(600);
  m_savedFade->setStartValue(m_savedOpacity->opacity());
  m_savedFade->setEndValue(0.0);
  m_savedFade->start();
}

void SettingsWindow::closeEvent(QCloseEvent* event) {
  if (m_closed) {
    m_closed();
  }
  QWidget::closeEvent(event);
}

} // namespace mitcad
