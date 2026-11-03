// SPDX-License-Identifier: MIT
#include "VersionWatermark.hpp"

#include <algorithm>
#include <initializer_list>
#include <vector>

#include <QCoreApplication>
#include <QEvent>
#include <QFont>
#include <QFontMetrics>
#include <QPainter>
#include <QSettings>
#include <QTimer>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {

namespace {

const char* const kSetting = "view/showVersion";
// From the free area's right and bottom edges; the Floating layout's
// insets already keep a gap above the timeline and the message band.
constexpr int kEdge = 12;
// Around what floats over the view.
constexpr int kGap = 6;
// Of the theme's secondary text colour: readable, not prominent.
constexpr qreal kOpacity = 0.6;
// Of the view's font size.
constexpr qreal kFontScale = 0.8;

} // namespace

bool versionWatermarkShown() { return QSettings().value(QLatin1String(kSetting), true).toBool(); }

void setVersionWatermarkShown(bool shown) {
  if (shown == versionWatermarkShown()) {
    return;
  }
  QSettings().setValue(QLatin1String(kSetting), shown);
  emit VersionWatermarkNotifier::instance().shownChanged(shown);
}

VersionWatermarkNotifier& VersionWatermarkNotifier::instance() {
  static VersionWatermarkNotifier notifier;
  return notifier;
}

VersionWatermark::VersionWatermark(OcctViewer* viewer) : QWidget(viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("versionWatermark"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setFocusPolicy(Qt::NoFocus);
  m_dark = darkBackground();
  m_viewer->installEventFilter(this);
  connect(m_viewer, &OcctViewer::coveredChanged, this, &VersionWatermark::requestPlace);
  connect(m_viewer, &OcctViewer::overlayInsetsChanged, this, &VersionWatermark::requestPlace);
  connect(m_viewer, &OcctViewer::badgeWidthChanged, this, &VersionWatermark::requestPlace);
  // A frame was drawn: the background may have changed between light and
  // dark (Preferences' view background).
  connect(m_viewer, &OcctViewer::viewChanged, this, [this] {
    const bool dark = darkBackground();
    if (dark != m_dark) {
      m_dark = dark;
      update();
    }
  });
  connect(&VersionWatermarkNotifier::instance(), &VersionWatermarkNotifier::shownChanged, this,
          &VersionWatermark::requestPlace);
  requestPlace();
}

QString VersionWatermark::text() {
  return QStringLiteral("%1 %2").arg(programName(), QCoreApplication::applicationVersion());
}

void VersionWatermark::setArea(QWidget* area) {
  if (area == m_area) {
    return;
  }
  for (QWidget* watched : {m_area.data(), m_areaHolder.data()}) {
    if (watched != nullptr) {
      watched->removeEventFilter(this);
    }
  }
  m_area = area;
  // The child of the view that holds the area (a drawing's workspace): it
  // raises itself over the view's other children, the watermark then goes
  // above it again.
  QWidget* holder = area;
  while (holder != nullptr && holder->parentWidget() != m_viewer) {
    holder = holder->parentWidget();
  }
  m_areaHolder = holder;
  for (QWidget* watched : {m_area.data(), m_areaHolder.data()}) {
    if (watched != nullptr) {
      watched->installEventFilter(this);
    }
  }
  m_dark = darkBackground();
  update();
  requestPlace();
}

void VersionWatermark::requestPlace() {
  if (m_placeQueued) {
    return;
  }
  m_placeQueued = true;
  QTimer::singleShot(0, this, [this] { place(); });
}

bool VersionWatermark::eventFilter(QObject* watched, QEvent* event) {
  switch (event->type()) {
  case QEvent::Resize:
  case QEvent::Move:
  case QEvent::Show:
  case QEvent::Hide:
  case QEvent::FontChange:
  case QEvent::PaletteChange:
  // A child of the view that covers it (a drawing's workspace) came: the
  // watermark goes above it again.
  case QEvent::ChildAdded:
    if (watched == m_viewer || watched == m_area) {
      requestPlace();
    }
    break;
  case QEvent::ZOrderChange:
    if (watched == m_areaHolder) {
      requestPlace();
    }
    break;
  default:
    break;
  }
  return QWidget::eventFilter(watched, event);
}

void VersionWatermark::changeEvent(QEvent* event) {
  if (event->type() == QEvent::PaletteChange) {
    update();
  }
  QWidget::changeEvent(event);
}

bool VersionWatermark::darkBackground() const {
  return m_area != nullptr ? isDarkPalette(m_area->palette()) : m_viewer->backgroundDark();
}

void VersionWatermark::place() {
  m_placeQueued = false;
  if (!versionWatermarkShown()) {
    hide();
    logLine(QStringLiteral("Version watermark hidden: preference"));
    return;
  }
  // The view's font, smaller.
  QFont small = m_viewer->font();
  if (small.pointSizeF() > 0) {
    small.setPointSizeF(small.pointSizeF() * kFontScale);
  } else if (small.pixelSize() > 0) {
    small.setPixelSize(std::max(9, static_cast<int>(small.pixelSize() * kFontScale)));
  }
  if (small != font()) {
    setFont(small);
  }
  const QFontMetrics metrics(small);
  const QSize size(metrics.horizontalAdvance(text()) + 4, metrics.height());

  // The free area: the drawing's sheet, or the view less the rows over its
  // top and the cards along its bottom (the timeline, the message band).
  QRect free;
  if (m_area != nullptr && m_area->isVisible()) {
    free = QRect(m_viewer->mapFromGlobal(m_area->mapToGlobal(QPoint(0, 0))), m_area->size());
  } else {
    const QMargins insets = m_viewer->overlayInsets();
    free = m_viewer->rect().adjusted(0, insets.top(), 0, -insets.bottom());
  }
  // What it keeps clear of: the cards, the navigation bar and the message
  // band, the rows' cards at the top, the orientation cube, and the
  // sketch's badge at the bottom left.
  std::vector<QRect> obstacles = m_viewer->coveredRects();
  obstacles.push_back(m_viewer->overlayCubeArea());
  if (m_viewer->badgeWidth() > 0) {
    obstacles.emplace_back(0, 0, m_viewer->badgeWidth(), m_viewer->height());
  }
  QRect box(QPoint(free.right() + 1 - kEdge - size.width(), free.bottom() + 1 - kEdge - size.height()), size);
  bool fits = false;
  for (int step = 0; step < 16 && free.contains(box); ++step) {
    const QRect around = box.adjusted(-kGap, -kGap, kGap, kGap);
    const auto hit = std::find_if(obstacles.begin(), obstacles.end(),
                                  [&around](const QRect& obstacle) { return obstacle.intersects(around); });
    if (hit == obstacles.end()) {
      fits = true;
      break;
    }
    // A panel along the view's right edge, from above the middle down: beside
    // it, to its left; anything lower (the navigation bar, a card at the
    // bottom): above it.
    if (hit->top() <= free.top() + free.height() / 2) {
      box.moveRight(hit->left() - kGap - 1);
    } else {
      box.moveBottom(hit->top() - kGap - 1);
    }
  }
  if (!fits) {
    hide();
    logLine(QStringLiteral("Version watermark hidden: no room"));
    return;
  }
  if (geometry() != box) {
    setGeometry(box);
  }
  show();
  raise();
  const QPoint at = GlassCard::mapToHost(this, rect().center());
  const QPoint corner = GlassCard::mapToHost(this, QPoint(0, 0));
  const QPoint view = GlassCard::mapToHost(m_viewer, QPoint(0, 0));
  logLine(QStringLiteral("Version watermark '%1' at %2,%3: box %4,%5 %6x%7, view %8,%9 %10x%11")
          .arg(text())
          .arg(at.x())
          .arg(at.y())
          .arg(corner.x())
          .arg(corner.y())
          .arg(box.width())
          .arg(box.height())
          .arg(view.x())
          .arg(view.y())
          .arg(m_viewer->width())
          .arg(m_viewer->height()));
}

void VersionWatermark::logLine(const QString& line) {
  // Once the window shows: placed before, it logs again when the view does.
  if (line == m_logged || window() == nullptr || !window()->isVisible()) {
    return;
  }
  m_logged = line;
  qDebug().noquote() << line;
}

void VersionWatermark::paintEvent(QPaintEvent* /*event*/) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::TextAntialiasing);
  QColor color = themeColor(ColorRole::TextSecondary, m_dark);
  color.setAlphaF(static_cast<float>(color.alphaF() * kOpacity));
  painter.setPen(color);
  painter.setFont(font());
  painter.drawText(rect(), Qt::AlignRight | Qt::AlignVCenter, text());
}

} // namespace mitcad
