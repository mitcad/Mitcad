// SPDX-License-Identifier: MIT
#include "TitleBar.hpp"

#include <QAction>
#include <QEvent>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QMouseEvent>
#include <QPainter>
#include <QTabBar>
#include <QToolButton>
#include <QWindow>
#include <QtGlobal>

#include "Ribbon.hpp"
#include "Theme.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

constexpr int kRowHeight = 52;
constexpr int kControlHeight = 30;
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

// A capsule button of the title bar: the search field (an outline of the
// command search with its label) or the prominent button (accent fill, white
// text). The action gives the click, the state and the tooltip.
class TitleButton : public QToolButton {
public:
  enum class Kind { Field, Prominent };

  TitleButton(Kind kind, QAction* action, const QString& label, QWidget* parent = nullptr)
      : QToolButton(parent), m_kind(kind), m_label(label) {
    setDefaultAction(action);
    setToolButtonStyle(Qt::ToolButtonTextOnly);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_Hover);
    setCursor(Qt::ArrowCursor);
    setFixedHeight(kControlHeight);
    if (kind == Kind::Field) {
      setFixedWidth(220);
    } else {
      setFixedWidth(fontMetrics().horizontalAdvance(label) + 40);
    }
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
    if (m_kind == Kind::Field) {
      painter.setPen(QPen(cardBorderColor(pal), 1));
      painter.setBrush(tint(pal, isDarkPalette(pal) ? 24 : 15));
      painter.drawRoundedRect(box, radius, radius);
      const QColor quiet = tint(pal, 150);
      painter.setPen(quiet);
      const QPixmap glass = m_glass.pixmap(QSize(14, 14), devicePixelRatioF());
      painter.drawPixmap(QPointF(12, (height() - 14) / 2.0), glass);
      painter.drawText(QRect(34, 0, width() - 44, height()), Qt::AlignVCenter | Qt::AlignLeft, m_label);
      return;
    }
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
    painter.drawRoundedRect(box, radius, radius);
    QFont bold = font();
    bold.setWeight(QFont::DemiBold);
    painter.setFont(bold);
    painter.setPen(pal.color(QPalette::HighlightedText));
    painter.drawText(rect(), Qt::AlignCenter, m_label);
  }

private:
  Kind m_kind;
  QString m_label;
  QIcon m_glass = mac::symbolIcon(QStringLiteral("magnifyingglass"), QIcon());
};

} // namespace

int TitleBar::rowHeight() { return kRowHeight; }

TitleBar::TitleBar(Ribbon* ribbon, QAction* undo, QAction* redo, QAction* search, QAction* finish,
                   QWidget* parent)
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

  m_search = new TitleButton(TitleButton::Kind::Field, search, tr("Search commands"));
  m_search->setObjectName(QStringLiteral("titleSearch"));
  row->addWidget(m_search);
  m_finish = new TitleButton(TitleButton::Kind::Prominent, finish, finish->text());
  m_finish->setObjectName(QStringLiteral("titleFinishSketch"));
  m_finish->setVisible(false);
  row->addWidget(m_finish);

  connect(ribbon, &Ribbon::sketchModeChanged, this, &TitleBar::setSketchMode);
  updateInset();
}

void TitleBar::setSketchMode(bool sketching) {
  m_finish->setVisible(sketching);
  update();
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
  update();
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

void TitleBar::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  const QPalette pal = palette();
  // Between the tabs and the search field; the middle of the window when the
  // text fits there.
  const int freeLeft = m_tabs->geometry().right() + 16;
  const QWidget* right = m_indicator != nullptr && m_indicator->isVisible() ? m_indicator.data() : m_search;
  const int freeRight = right->geometry().left() - 16;
  if (freeRight - freeLeft < 60) {
    return;
  }
  const QString title = documentTitle();
  const QString subtitle = window()->isWindowModified() ? tr("Edited") : QString();
  QFont titleFont = font();
  titleFont.setWeight(QFont::DemiBold);
  QFont subFont = font();
  subFont.setPointSizeF(subFont.pointSizeF() * 0.85);
  const QFontMetrics titleMetrics(titleFont);
  const QFontMetrics subMetrics(subFont);
  const int available = freeRight - freeLeft;
  const QString shown = titleMetrics.elidedText(title, Qt::ElideMiddle, available);
  const int textWidth = qMax(titleMetrics.horizontalAdvance(shown), subMetrics.horizontalAdvance(subtitle));
  int left = width() / 2 - textWidth / 2;
  left = qBound(freeLeft, left, freeRight - textWidth);
  const int lineHeight = titleMetrics.height();
  const int block = subtitle.isEmpty() ? lineHeight : lineHeight + subMetrics.height() - 2;
  const int top = (height() - block) / 2;
  const QRect titleRect(left, top, textWidth, lineHeight);
  painter.setFont(titleFont);
  painter.setPen(pal.color(QPalette::WindowText));
  painter.drawText(titleRect, Qt::AlignHCenter | Qt::AlignVCenter, shown);
  if (!subtitle.isEmpty()) {
    painter.setFont(subFont);
    painter.setPen(tint(pal, 140));
    painter.drawText(QRect(left, top + lineHeight - 2, textWidth, subMetrics.height()),
                     Qt::AlignHCenter | Qt::AlignVCenter, subtitle);
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
