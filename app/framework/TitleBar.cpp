// SPDX-License-Identifier: MIT
#include "TitleBar.hpp"

#include <algorithm>
#include <initializer_list>

#include <QAction>
#include <QEvent>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QMouseEvent>
#include <QPainter>
#include <QRegion>
#include <QTabBar>
#include <QToolButton>
#include <QWindow>
#include <QtGlobal>

#include "GlassCard.hpp"
#include "Ribbon.hpp"
#include "Theme.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

constexpr int kRowHeight = 52;
constexpr int kControlHeight = 30;
// The Floating title row on Windows and Linux (setFloatingRow): the
// controls and the title's box, without the macOS toolbar's room.
constexpr int kFloatingRowHeight = 40;
// The room around the document's name in its box.
constexpr int kTitlePadX = 12;
constexpr int kTitlePadY = 4;
// The traffic lights' width, from the window's edge to past the green one.
constexpr int kTrafficLights = 78;

QColor tint(const QPalette& palette, int alpha) {
  QColor color = palette.color(QPalette::WindowText);
  color.setAlpha(alpha);
  return color;
}

// A hairline between the two segments of the Undo and Redo pair.
class Divider : public QWidget {
public:
  explicit Divider(QWidget* parent = nullptr) : QWidget(parent) {
    setFixedSize(1, 16);
    setAttribute(Qt::WA_TransparentForMouseEvents);
  }

protected:
  void paintEvent(QPaintEvent*) override { QPainter(this).fillRect(rect(), cardBorderColor(palette())); }
};

// The title bar's search field: an outline of command search with its label.
// The action gives the click, the state and the tooltip.
class TitleButton : public QToolButton {
public:
  TitleButton(QAction* action, const QString& label, QWidget* parent = nullptr)
      : QToolButton(parent), m_label(label) {
    setDefaultAction(action);
    setToolButtonStyle(Qt::ToolButtonTextOnly);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::ArrowCursor);
    setFixedHeight(kControlHeight);
    setFixedWidth(220);
  }

protected:
  void changeEvent(QEvent* event) override {
    QToolButton::changeEvent(event);
    if (event->type() == QEvent::ApplicationPaletteChange || event->type() == QEvent::PaletteChange) {
      update();
    }
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QPalette pal = palette();
    const QRectF box = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    const qreal radius = box.height() / 2;
    painter.setPen(QPen(cardBorderColor(pal), 1));
    painter.setBrush(tint(pal, isDarkPalette(pal) ? 24 : 15));
    painter.drawRoundedRect(box, radius, radius);
    painter.setPen(tint(pal, 150));
    const QPixmap glass = m_glass.pixmap(QSize(14, 14), devicePixelRatioF());
    painter.drawPixmap(QPointF(12, (height() - 14) / 2.0), glass);
    painter.drawText(QRect(34, 0, width() - 44, height()), Qt::AlignVCenter | Qt::AlignLeft, m_label);
  }

private:
  QString m_label;
  QIcon m_glass = mac::symbolIcon(QStringLiteral("magnifyingglass"), QIcon());
};

} // namespace

int TitleBar::rowHeight() { return kRowHeight; }

TitleBar::TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QWidget* parent)
    : QWidget(parent) {
  setObjectName(QStringLiteral("titleBar"));
  setFixedHeight(kRowHeight);
  auto* row = new QHBoxLayout(this);
  row->setContentsMargins(12, 0, 12, 0);
  row->setSpacing(12);

  m_inset = new QWidget;
  m_inset->setAttribute(Qt::WA_TransparentForMouseEvents);
  row->addWidget(m_inset);

  auto* history = new CapsuleFrame(-1);
  m_history = history;
  history->setObjectName(QStringLiteral("titleHistory"));
  auto* pair = new QHBoxLayout(history);
  pair->setContentsMargins(4, 2, 4, 3);
  pair->setSpacing(2);
  pair->addWidget(new RibbonButton(undo, 18));
  pair->addWidget(new Divider);
  pair->addWidget(new RibbonButton(redo, 18));
  history->setFixedHeight(34);
  row->addWidget(history);

  QTabBar* tabs = ribbon->tabBar();
  m_tabs = tabs;
  row->addWidget(tabs);
  row->addStretch();

  m_search = new TitleButton(search, tr("Search commands"));
  m_search->setObjectName(QStringLiteral("titleSearch"));
  row->addWidget(m_search);
  updateInset();
}

void TitleBar::setIndicator(QWidget* indicator) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || indicator == nullptr) {
    return;
  }
  m_indicator = indicator;
  indicator->setParent(this);
  indicator->setFixedHeight(kControlHeight);
  row->insertWidget(row->indexOf(m_search), indicator);
  indicator->show();
  if (m_floatingRow) {
    indicator->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setAgentIndicator(QWidget* indicator) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || indicator == nullptr) {
    return;
  }
  m_agentIndicator = indicator;
  indicator->setParent(this);
  indicator->setFixedHeight(kControlHeight);
  row->insertWidget(row->indexOf(m_search), indicator);
  indicator->show();
  if (m_floatingRow) {
    indicator->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setChatButton(QWidget* button) {
  auto* row = qobject_cast<QHBoxLayout*>(layout());
  if (row == nullptr || button == nullptr) {
    return;
  }
  m_chatButton = button;
  button->setParent(this);
  button->setFixedHeight(kControlHeight);
  row->insertWidget(row->indexOf(m_search), button);
  button->show();
  if (m_floatingRow) {
    button->installEventFilter(this);
    updateRowMask();
  }
  update();
}

void TitleBar::setFloatingRow(bool floating) {
  if (floating == m_floatingRow) {
    return;
  }
  m_floatingRow = floating;
  setObjectName(floating ? QStringLiteral("titleRow") : QStringLiteral("titleBar"));
  setFixedHeight(floating ? kFloatingRowHeight : kRowHeight);
  // The row starts and ends at the layout's margin to the view's edges, as
  // the ribbon's group cards below it; no traffic lights to leave room for.
  layout()->setContentsMargins(floating ? 0 : 12, 0, floating ? 0 : 12, 0);
  m_inset->setVisible(!floating);
  m_history->setCardMaterial(floating);
  for (QWidget* part :
       std::initializer_list<QWidget*>{m_history, m_tabs, m_search, m_indicator.data(), m_agentIndicator.data(),
                                       m_chatButton.data()}) {
    if (part == nullptr) {
      continue;
    }
    if (floating) {
      part->installEventFilter(this);
    } else {
      part->removeEventFilter(this);
    }
  }
  if (floating) {
    updateRowMask();
  } else {
    clearMask();
  }
  update();
}

std::vector<std::pair<QString, QRect>> TitleBar::rowParts() const {
  std::vector<std::pair<QString, QRect>> parts;
  const auto add = [this, &parts](const char* name, const QWidget* widget) {
    if (widget != nullptr && !widget->isHidden()) {
      const QRect rect = widget->geometry() & this->rect();
      if (!rect.isEmpty()) {
        parts.emplace_back(QString::fromLatin1(name), rect);
      }
    }
  };
  add("titleHistory", m_history);
  add("titleTabs", m_tabs);
  add("titleIndicator", m_indicator.data());
  add("titleAgent", m_agentIndicator.data());
  add("titleChat", m_chatButton.data());
  add("titleSearch", m_search);
  const QRect box = titleBox();
  if (!box.isEmpty()) {
    parts.emplace_back(QStringLiteral("titleBox"), box);
  }
  std::sort(parts.begin(), parts.end(), [](const auto& a, const auto& b) { return a.second.left() < b.second.left(); });
  return parts;
}

QRect TitleBar::titleBox() const {
  const TitleText text = titleText();
  if (!text.valid) {
    return {};
  }
  return text.block.adjusted(-kTitlePadX, -kTitlePadY, kTitlePadX, kTitlePadY) & rect();
}

void TitleBar::updateRowMask() {
  if (!m_floatingRow) {
    return;
  }
  QRegion region;
  for (const auto& part : rowParts()) {
    region += part.second;
  }
  if (region.isEmpty()) {
    // An empty mask is none at all: the whole row would take the mouse.
    region = QRegion(0, 0, 1, 1);
  }
  if (region != mask()) {
    setMask(region);
    emit rowPartsChanged();
  }
}

void TitleBar::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  updateRowMask();
}

bool TitleBar::underNativeTitleBar() const {
#if defined(Q_OS_MACOS) && QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
  const QWidget* top = window();
  return top != nullptr && top->windowFlags().testFlag(Qt::ExpandedClientAreaHint);
#else
  return false;
#endif
}

void TitleBar::updateInset() {
  int inset = 0;
  if (underNativeTitleBar()) {
    inset = window()->isFullScreen() ? 0 : kTrafficLights;
#if QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
    if (const QWindow* handle = window()->windowHandle()) {
      // The platform's own figure, where it gives one.
      if (!window()->isFullScreen() && handle->safeAreaMargins().left() > 0) {
        inset = handle->safeAreaMargins().left();
      }
    }
#endif
  }
  m_inset->setFixedWidth(inset);
}

void TitleBar::watchWindow() {
  QWidget* top = window();
  if (top == m_watched.data()) {
    return;
  }
  if (m_watched != nullptr) {
    m_watched->removeEventFilter(this);
  }
  m_watched = top;
  if (top != this) {
    top->installEventFilter(this);
  }
#if QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
  if (QWindow* handle = top->windowHandle()) {
    connect(handle, &QWindow::safeAreaMarginsChanged, this, &TitleBar::updateInset, Qt::UniqueConnection);
  }
#endif
}

void TitleBar::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  watchWindow();
  updateInset();
  if (underNativeTitleBar()) {
    // The title is drawn here; the native one would be drawn over it.
    mac::hideNativeTitle(window());
  }
}

bool TitleBar::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_watched.data()) {
    switch (event->type()) {
    case QEvent::WindowTitleChange:
    case QEvent::ModifiedChange:
    case QEvent::WindowStateChange:
      updateInset();
      updateRowMask(); // the title's box follows the name
      update();
      break;
    case QEvent::WinIdChange:
    case QEvent::Show:
      watchWindow();
      updateInset();
      break;
    default:
      break;
    }
  } else if (m_floatingRow) {
    // A control of the row moved, resized, came or went: the mask follows.
    switch (event->type()) {
    case QEvent::Move:
    case QEvent::Resize:
    case QEvent::Show:
    case QEvent::Hide:
      updateRowMask();
      break;
    default:
      break;
    }
  }
  return QWidget::eventFilter(watched, event);
}

QString TitleBar::documentTitle() const {
  QString title = window()->windowTitle();
  title.remove(QStringLiteral("[*]"));
  const int dash = title.lastIndexOf(QStringLiteral(" - "));
  if (dash > 0) {
    title.truncate(dash);
  }
  return title;
}

TitleBar::TitleText TitleBar::titleText() const {
  TitleText text;
  // Between the tabs and the search field; the middle of the window when the
  // text fits there.
  const int freeLeft = m_tabs->geometry().right() + 16;
  // The leftmost of the indicators and the search field.
  const QWidget* right = m_search;
  for (const QWidget* part : {m_chatButton.data(), m_agentIndicator.data(), m_indicator.data()}) {
    if (part != nullptr && !part->isHidden()) {
      right = part;
    }
  }
  const int freeRight = right->geometry().left() - 16;
  if (freeRight - freeLeft < 60) {
    return text;
  }
  const QString title = documentTitle();
  text.subtitle = window()->isWindowModified() ? tr("Edited") : QString();
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const QFontMetrics titleMetrics(titleFont);
  const QFontMetrics subMetrics(subFont);
  // The box's padding stays within the free part too.
  const int padding = m_floatingRow ? kTitlePadX : 0;
  const int available = freeRight - freeLeft - 2 * padding;
  text.title = titleMetrics.elidedText(title, Qt::ElideMiddle, available);
  const int textWidth =
      qMax(titleMetrics.horizontalAdvance(text.title), subMetrics.horizontalAdvance(text.subtitle));
  int left = width() / 2 - textWidth / 2;
  left = qBound(freeLeft + padding, left, freeRight - padding - textWidth);
  const int lineHeight = titleMetrics.height();
  const int block = text.subtitle.isEmpty() ? lineHeight : lineHeight + subMetrics.height() - 2;
  text.block = QRect(left, (height() - block) / 2, textWidth, block);
  text.valid = true;
  return text;
}

void TitleBar::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  const QPalette pal = palette();
  if (m_floatingRow) {
    // Over the view (mitcad#429): the cards' fill and hairline behind the
    // tabs, the project indicator and the search field (capsules; the Undo
    // and Redo pair has its own), and behind the
    // document's name a box as a bare card's header.
    painter.setRenderHint(QPainter::Antialiasing);
    const qreal hairline = 1.0 / devicePixelRatioF();
    painter.setPen(QPen(cardBorderColor(pal), hairline));
    painter.setBrush(cardFillColor(pal));
    for (const auto& [name, rect] : rowParts()) {
      if (name == QLatin1String("titleHistory")) {
        continue;
      }
      const QRectF box = QRectF(rect).adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
      const qreal radius = name == QLatin1String("titleBox") ? qreal(GlassCard::kBoxRadius) : box.height() / 2;
      painter.drawRoundedRect(box, radius, radius);
    }
  }
  const TitleText text = titleText();
  if (!text.valid) {
    return;
  }
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const int lineHeight = QFontMetrics(titleFont).height();
  const QRect titleRect(text.block.left(), text.block.top(), text.block.width(), lineHeight);
  painter.setFont(titleFont);
  painter.setPen(pal.color(QPalette::WindowText));
  painter.drawText(titleRect, Qt::AlignHCenter | Qt::AlignVCenter, text.title);
  if (!text.subtitle.isEmpty()) {
    painter.setFont(subFont);
    painter.setPen(tint(pal, 140));
    painter.drawText(QRect(text.block.left(), text.block.top() + lineHeight - 2, text.block.width(),
                           QFontMetrics(subFont).height()),
                     Qt::AlignHCenter | Qt::AlignVCenter, text.subtitle);
  }
}

void TitleBar::mousePressEvent(QMouseEvent* event) {
  if (event->button() == Qt::LeftButton && underNativeTitleBar()) {
    if (QWindow* handle = window()->windowHandle()) {
      handle->startSystemMove();
      event->accept();
      return;
    }
  }
  QWidget::mousePressEvent(event);
}

void TitleBar::mouseDoubleClickEvent(QMouseEvent* event) {
  if (event->button() == Qt::LeftButton && underNativeTitleBar()) {
    QWidget* top = window();
    if (top->isMaximized()) {
      top->showNormal();
    } else {
      top->showMaximized();
    }
    event->accept();
    return;
  }
  QWidget::mouseDoubleClickEvent(event);
}

} // namespace mitcad
