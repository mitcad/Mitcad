// SPDX-License-Identifier: MIT
#include "TimelineWidget.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <utility>

#include <QContextMenuEvent>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QIcon>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QScrollArea>
#include <QScrollBar>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacChrome.hpp"

namespace mitcad {
namespace {

// Delete deletes; on macOS the delete key of the keyboard (Backspace) too.
bool isDeleteKey(int key) {
#ifdef Q_OS_MACOS
  return key == Qt::Key_Delete || key == Qt::Key_Backspace;
#else
  return key == Qt::Key_Delete;
#endif
}

constexpr int kPad = 6;
constexpr int kItem = 28;     // an icon's cell
constexpr int kIcon = 22;
constexpr int kGap = 3;
constexpr int kMarkerSlot = 14; // the marker's place between two cells
constexpr int kTop = 5; // room for an open group's band (P9) and the marker's handle
constexpr int kBand = 3; // the component colour, in the cell's room under its icon
constexpr int kBandTop = kTop + kItem - kBand;
constexpr int kHeight = kTop + kItem + 1;
constexpr int kDragPixels = 6;

const QColor kErrorFill(0xfb, 0xd5, 0xd5);
const QColor kErrorBorder(0xd0, 0x20, 0x20);
const QColor kWarningFill(0xff, 0xf1, 0xc2);
const QColor kWarningBorder(0xd0, 0xa0, 0x00);
const QColor kSelectedFill(0xd6, 0xe6, 0xfb);
const QColor kSelectedBorder(0x1f, 0x7a, 0xff);
const QColor kHoverFill(0xe9, 0xf1, 0xfc);
const QColor kMarkerColor(0x2b, 0x3a, 0x4a);
// The same on a dark palette (fills that the light ones would glare among).
const QColor kErrorFillDark(0x5c, 0x2b, 0x30);
const QColor kWarningFillDark(0x57, 0x4a, 0x20);
const QColor kSelectedFillDark(0x26, 0x44, 0x70);
const QColor kHoverFillDark(0x34, 0x3c, 0x48);
const QColor kMarkerColorDark(0xc8, 0xd0, 0xda); // on a dark theme's background (mitcad#14)
const QColor kMarkerHandle(0xe0, 0x7b, 0x1a);
const QColor kAllowed(0x20, 0xa0, 0x40);
const QColor kRefused(0xd0, 0x20, 0x20);
const QColor kGroupBand(0x7d, 0x8c, 0x9e); // over an open group's features (P9)

// The colours of the strip's cells and marker. Docked: the fixed ones above
// (light or dark). Floating: derived from the palette and the Theme's colours,
// so that they sit on the card's material and follow the accent colour.
struct StripColors {
  QColor errorFill, errorBorder, warningFill, warningBorder, selectedFill, selectedBorder, hoverFill, marker;
};

StripColors stripColors(const QPalette& palette) {
  const bool dark = isDarkPalette(palette);
  if (chromeStyle() != ChromeStyle::Floating) {
    return {dark ? kErrorFillDark : kErrorFill,     kErrorBorder,
            dark ? kWarningFillDark : kWarningFill, kWarningBorder,
            dark ? kSelectedFillDark : kSelectedFill, kSelectedBorder,
            dark ? kHoverFillDark : kHoverFill,     dark ? kMarkerColorDark : kMarkerColor};
  }
  const auto tint = [](QColor color, qreal alpha) {
    color.setAlphaF(static_cast<float>(alpha));
    return color;
  };
  const QColor text = palette.color(QPalette::WindowText);
  const QColor accent = accentColor(palette);
  return {tint(errorColor(palette), 0.20),  errorColor(palette),
          tint(warningColor(palette), 0.22), warningColor(palette),
          tint(accent, 0.24),                accent,
          tint(text, 0.09),                  text};
}

// Component colours; the root has none.
const QColor kComponentColors[] = {
    QColor(0xe0, 0x89, 0x2a), QColor(0x3a, 0xa3, 0x5b), QColor(0x8e, 0x5b, 0xd0),
    QColor(0x2a, 0xa7, 0xa7), QColor(0xc9, 0x48, 0x3f), QColor(0x9a, 0x9a, 0x2a),
    QColor(0xc4, 0x5a, 0x9a), QColor(0x3d, 0x8b, 0xd9)};

// The colours that follow the theme (mitcad#14): a dark theme's palette
// has light text on a dark window, where the light theme's dark marker and
// glyphs would disappear.
bool darkPalette(const QPalette& palette) {
  return palette.color(QPalette::Window).lightness() < palette.color(QPalette::WindowText).lightness();
}

// The history marker's bar and the outline of its handle.
QColor markerColor(const QPalette& palette) { return darkPalette(palette) ? kMarkerColorDark : kMarkerColor; }

// The line across a suppressed feature: between the text and the window
// (#606060 on a light theme's window, as before).
QColor strikeColor(const QPalette& palette) {
  const QColor text = palette.color(QPalette::WindowText);
  const QColor window = palette.color(QPalette::Window);
  const auto mix = [](int a, int b) { return (a * 3 + b * 2 + 2) / 5; };
  return QColor(mix(text.red(), window.red()), mix(text.green(), window.green()), mix(text.blue(), window.blue()));
}

// A playback button's icon: the style's own on a light theme; on a dark
// one its glyph in the palette's button text colour, since the styles draw
// these glyphs dark whatever the palette.
QIcon playbackIcon(const QStyle& style, QStyle::StandardPixmap which, const QPalette& palette) {
  const QIcon icon = style.standardIcon(which);
  if (!darkPalette(palette)) {
    return icon;
  }
  QIcon tinted;
  for (const int size : {16, 20, 24, 32}) {
    QPixmap pixmap = icon.pixmap(QSize(size, size));
    if (pixmap.isNull()) {
      continue;
    }
    QPainter painter(&pixmap);
    painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
    painter.fillRect(pixmap.rect(), palette.color(QPalette::ButtonText));
    painter.end();
    tinted.addPixmap(pixmap);
  }
  return tinted;
}

} // namespace

// The painted row of features with the marker, inside a scroll area.
class TimelineStrip : public QWidget {
public:
  struct Item {
    DocumentSnapshot::Feature feature;
    int colour = -1; // index into kComponentColors, -1 for the root
    int group = -1;  // index into the groups
  };
  // A timeline group (P9): its first and last item, folded or open.
  struct Group {
    QString name;
    int first = 0;
    int last = 0;
    bool collapsed = false;
  };

  explicit TimelineStrip(TimelineWidget& owner) : QWidget(&owner), m_owner(owner) {
    setMouseTracking(true);
    setFocusPolicy(Qt::ClickFocus);
    setFixedHeight(kHeight);
  }

  void setItems(QVector<Item> items, int marker, QVector<Group> groups) {
    m_items = std::move(items);
    m_groups = std::move(groups);
    m_marker = std::clamp(marker, 0, static_cast<int>(m_items.size()));
    // A group the marker is inside of shows open.
    for (Group& group : m_groups) {
      group.collapsed = group.collapsed && !(m_marker > group.first && m_marker <= group.last);
    }
    m_reorder.clear();
    if (m_press >= static_cast<int>(m_items.size())) {
      m_press = -1;
      m_dragItem = false;
    }
    // Where each cell starts; the cells of a folded group after its first
    // take no room.
    m_left.assign(m_items.size(), 0);
    int x = kPad;
    for (int i = 0; i <= count(); ++i) {
      if (i == m_marker) {
        m_markerX = x;
        x += kMarkerSlot + kGap;
      }
      if (i < count()) {
        m_left[static_cast<std::size_t>(i)] = x;
        if (!hidden(i)) {
          x += kItem + kGap;
        }
      }
    }
    setFixedWidth(x + kPad);
    update();
  }

  int count() const { return static_cast<int>(m_items.size()); }
  int marker() const { return m_marker; }
  const Item* item(int i) const { return i >= 0 && i < count() ? &m_items[i] : nullptr; }
  const QVector<Group>& groups() const { return m_groups; }
  int indexOf(const QString& uid) const {
    for (int i = 0; i < count(); ++i) {
      if (m_items[i].feature.uid == uid) {
        return i;
      }
    }
    return -1;
  }

  QString selected() const { return m_selected; }
  void select(const QString& uid) {
    m_selected = uid;
    m_selectedTo = uid;
    update();
  }
  // The selected run: from the anchor to the last Shift+click.
  QStringList selectedRun() const {
    const int a = indexOf(m_selected);
    const int b = indexOf(m_selectedTo);
    QStringList run;
    if (a < 0 || b < 0) {
      return run;
    }
    for (int i = std::min(a, b); i <= std::max(a, b); ++i) {
      run << m_items[i].feature.uid;
    }
    return run;
  }

  // A cell of a folded group after its first is not shown; the first
  // stands for the group.
  bool hidden(int i) const {
    const int g = m_items[i].group;
    return g >= 0 && m_groups[g].collapsed && i != m_groups[g].first;
  }
  bool isHead(int i) const {
    const int g = m_items[i].group;
    return g >= 0 && m_groups[g].collapsed && i == m_groups[g].first;
  }

  QRect cell(int i) const {
    return QRect(m_left[static_cast<std::size_t>(i)], kTop, hidden(i) ? 0 : kItem, kItem);
  }
  QRect markerRect() const { return QRect(m_markerX, 0, kMarkerSlot, height()); }
  // The band over an open group's cells.
  QRect bandRect(const Group& group) const {
    const int left = cell(group.first).left();
    return QRect(left, 1, cell(group.last).right() + 1 - left, kTop - 2);
  }
  // Where a group is shown: its folded cell or its band.
  QRect groupRect(const Group& group) const { return group.collapsed ? cell(group.first) : bandRect(group); }

private:
  // Where the boundary before feature g is (g = count(): after the last).
  double boundary(int g) const {
    if (g == m_marker) {
      return markerRect().center().x() + 0.5;
    }
    if (g < count()) {
      return cell(g).left() - kGap / 2.0;
    }
    return cell(count() - 1).right() + 1 + kGap / 2.0;
  }

  // Not inside a folded group.
  bool open(int g) const {
    return std::none_of(m_groups.begin(), m_groups.end(),
                        [g](const Group& group) { return group.collapsed && g > group.first && g <= group.last; });
  }

  int nearestBoundary(double x) const {
    int best = 0;
    for (int g = 1; g <= count(); ++g) {
      if (open(g) && std::abs(boundary(g) - x) < std::abs(boundary(best) - x)) {
        best = g;
      }
    }
    return best;
  }

  // The group whose band or folded cell is at a point, or -1.
  int groupAt(const QPoint& at) const {
    for (int g = 0; g < m_groups.size(); ++g) {
      if (groupRect(m_groups[g]).adjusted(0, -1, 0, 2).contains(at)) {
        return g;
      }
    }
    return -1;
  }

  int itemAt(const QPoint& at) const {
    for (int i = 0; i < count(); ++i) {
      if (cell(i).adjusted(0, 0, 0, 1).contains(at)) {
        return i;
      }
    }
    return -1;
  }

  // The timeline index a dragged feature would go to at x.
  int dropIndex(double x) const {
    const int gap = nearestBoundary(x);
    return gap <= m_press ? gap : gap - 1;
  }

  // Whether the dragged feature can go to an index; the model's reason when
  // not (asked once per index while dragging).
  bool canMove(int index, QString* reason) {
    auto cached = m_reorder.constFind(index);
    if (cached == m_reorder.cend()) {
      QString error;
      try {
        const QJsonObject answer = m_owner.m_model.queryObject(
            {{QStringLiteral("query"), QStringLiteral("can_reorder")},
             {QStringLiteral("uid"), m_items[m_press].feature.uid},
             {QStringLiteral("index"), index}});
        if (!answer.value(QStringLiteral("ok")).toBool()) {
          error = answer.value(QStringLiteral("error")).toString(tr("It cannot go there."));
        }
      } catch (const std::exception& e) {
        error = QString::fromUtf8(e.what());
      }
      cached = m_reorder.insert(index, error);
    }
    if (reason != nullptr) {
      *reason = cached.value();
    }
    return cached.value().isEmpty();
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QStringList run = selectedRun();
    // Open groups: a band over their cells (P9).
    for (const Group& group : std::as_const(m_groups)) {
      if (!group.collapsed) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(kGroupBand);
        painter.drawRoundedRect(QRectF(bandRect(group)).adjusted(1, 0.5, -1, 0), 1.5, 1.5);
      }
    }
    const StripColors colors = stripColors(palette());
    for (int i = 0; i < count(); ++i) {
      if (hidden(i)) {
        continue;
      }
      const Item& item = m_items[i];
      const DocumentSnapshot::Feature& feature = item.feature;
      const QRectF rect = QRectF(cell(i)).adjusted(0.5, 0.5, -0.5, -0.5);
      const bool head = isHead(i);
      // A folded group shows the worst of its features.
      bool failed = false;
      bool warned = false;
      bool off = true;
      const int last = head ? m_groups[item.group].last : i;
      for (int k = i; k <= last; ++k) {
        const DocumentSnapshot::Feature& member = m_items[k].feature;
        failed = failed || member.status == QStringLiteral("error");
        warned = warned || member.status == QStringLiteral("warning");
        off = off && (member.suppressed || k >= m_marker);
      }
      const bool selected = run.contains(feature.uid);
      QColor fill = Qt::transparent;
      QColor border = Qt::transparent;
      if (failed) {
        fill = colors.errorFill;
        border = colors.errorBorder;
      } else if (warned) {
        fill = colors.warningFill;
        border = colors.warningBorder;
      }
      if (selected) {
        fill = failed || warned ? fill : colors.selectedFill;
        border = colors.selectedBorder;
      } else if (i == m_hover && !failed && !warned) {
        fill = colors.hoverFill;
      }
      painter.setPen(QPen(border, selected ? 2.0 : 1.2));
      painter.setBrush(fill);
      painter.drawRoundedRect(rect, 4, 4);
      if (item.colour >= 0) {
        const QColor colour = kComponentColors[item.colour % 8];
        painter.setPen(Qt::NoPen);
        painter.setBrush(colour);
        painter.drawRoundedRect(QRectF(rect.left() + 4, kBandTop, kItem - 8, kBand - 0.5), 1.5, 1.5);
      }
      painter.setOpacity(off ? 0.3 : 1.0);
      const QPixmap pixmap = themeIcon(head ? QStringLiteral("folder") : featureIcon(feature.type))
                                 .pixmap(QSize(kIcon, kIcon), devicePixelRatioF());
      painter.drawPixmap(cell(i).left() + (kItem - kIcon) / 2, kTop + (kItem - kIcon) / 2, pixmap);
      painter.setOpacity(1.0);
      if (head) {
        continue;
      }
      if (feature.suppressed) {
        painter.setPen(QPen(strikeColor(palette()), 1.6));
        painter.drawLine(rect.bottomLeft() + QPointF(4, -4), rect.topRight() + QPointF(-4, 4));
      }
    }
    // The history marker: a bar with a handle to drag, from the groups'
    // bands down to the cells' bottom.
    const double x = markerRect().center().x() + 1.0; // the 2 px bar on whole pixels
    painter.setPen(QPen(colors.marker, 2.0));
    painter.drawLine(QPointF(x, 4), QPointF(x, kTop + kItem - 1)); // square caps
    painter.setPen(QPen(colors.marker, 1.0));
    painter.setBrush(kMarkerHandle);
    painter.drawRoundedRect(QRectF(x - 3.5, 1.5, 7, 8), 1.5, 1.5);
    // Where a dragged feature would go.
    if (m_dragItem && m_dropIndex >= 0 && m_dropIndex != m_press) {
      const int gap = m_dropIndex >= m_press ? m_dropIndex + 1 : m_dropIndex;
      const double at = boundary(gap);
      painter.setPen(QPen(m_dropAllowed ? kAllowed : kRefused, 3.0));
      painter.drawLine(QPointF(at, 1), QPointF(at, height() - 1));
    }
  }

  void mousePressEvent(QMouseEvent* event) override {
    if (event->button() != Qt::LeftButton) {
      QWidget::mousePressEvent(event);
      return;
    }
    const QPoint at = event->position().toPoint();
    if (markerRect().adjusted(-3, 0, 3, 0).contains(at)) {
      m_dragMarker = true;
      m_dragPosition = m_marker;
      setCursor(Qt::SizeHorCursor);
      emit m_owner.markerDragStarted();
      return;
    }
    m_press = itemAt(at);
    // A folded group's cell is no feature to drag; an open group's band
    // folds it on release.
    m_pressGroup = m_press >= 0 && isHead(m_press) ? m_items[m_press].group : -1;
    if (m_press < 0) {
      m_pressGroup = groupAt(at);
    }
    if (m_pressGroup >= 0) {
      m_press = -1;
    }
    m_pressAt = at;
    m_dragItem = false;
    m_shift = event->modifiers().testFlag(Qt::ShiftModifier);
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    const QPoint at = event->position().toPoint();
    if (m_dragMarker) {
      const int position = nearestBoundary(at.x());
      if (position != m_dragPosition) {
        m_dragPosition = position;
        emit m_owner.markerDragged(position);
      }
      return;
    }
    if (m_press >= 0 && (event->buttons() & Qt::LeftButton)) {
      if (!m_dragItem && (at - m_pressAt).manhattanLength() > kDragPixels) {
        m_dragItem = true;
        setCursor(Qt::ClosedHandCursor);
      }
      if (m_dragItem) {
        m_dropIndex = dropIndex(at.x());
        QString reason;
        m_dropAllowed = m_dropIndex == m_press || canMove(m_dropIndex, &reason);
        if (!m_dropAllowed) {
          QToolTip::showText(event->globalPosition().toPoint(), reason, this);
        } else {
          QToolTip::hideText();
        }
        update();
      }
      return;
    }
    const int hover = itemAt(at);
    if (hover != m_hover) {
      m_hover = hover;
      update();
    }
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (event->button() != Qt::LeftButton) {
      QWidget::mouseReleaseEvent(event);
      return;
    }
    unsetCursor();
    if (m_dragMarker) {
      m_dragMarker = false;
      emit m_owner.markerDragFinished(m_dragPosition);
      return;
    }
    const int pressed = m_press;
    const bool dragged = m_dragItem;
    const int index = dragged ? dropIndex(event->position().x()) : -1;
    const int pressedGroup = m_pressGroup;
    m_press = -1;
    m_pressGroup = -1;
    m_dragItem = false;
    QToolTip::hideText();
    update();
    if (pressedGroup >= 0) {
      // A click on an open group's band folds it (P9); a folded group's cell
      // opens with a double-click.
      if (groupAt(event->position().toPoint()) == pressedGroup && !m_groups[pressedGroup].collapsed) {
        m_owner.setGroupCollapsed(m_groups[pressedGroup].name, true);
      }
      return;
    }
    if (pressed < 0 || pressed >= count()) {
      return;
    }
    const QString uid = m_items[pressed].feature.uid;
    if (dragged) {
      if (index != pressed) {
        emit m_owner.reorderRequested(uid, index);
      }
      return;
    }
    if (itemAt(event->position().toPoint()) == pressed) {
      if (m_shift && !m_selected.isEmpty()) {
        // Shift+click: a run of features from the one selected.
        m_selectedTo = uid;
        update();
        const QStringList run = selectedRun();
        qDebug().noquote() << QStringLiteral("Timeline selected: %1").arg(run.join(QStringLiteral(", ")));
        emit m_owner.featuresClicked(run);
        return;
      }
      select(uid);
      emit m_owner.featuresClicked({uid});
    }
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    const QPoint at = event->position().toPoint();
    const int i = itemAt(at);
    if (event->button() != Qt::LeftButton) {
      return;
    }
    if (i >= 0 && isHead(i)) {
      m_press = -1;
      m_owner.setGroupCollapsed(m_groups[m_items[i].group].name, false);
      return;
    }
    if (i >= 0) {
      m_press = -1;
      emit m_owner.editRequested(m_items[i].feature.uid);
    }
  }

  void contextMenuEvent(QContextMenuEvent* event) override {
    const int i = itemAt(event->pos());
    const int group = i >= 0 && isHead(i) ? m_items[i].group : (i < 0 ? groupAt(event->pos()) : -1);
    if (group >= 0) {
      emit m_owner.groupMenuRequested(m_groups[group].name, event->globalPos());
      return;
    }
    if (i < 0) {
      return;
    }
    // In the selected run the run stays; elsewhere the feature is selected.
    if (!selectedRun().contains(m_items[i].feature.uid)) {
      select(m_items[i].feature.uid);
    }
    emit m_owner.contextMenuRequested(m_items[i].feature.uid, event->globalPos());
  }

  void keyPressEvent(QKeyEvent* event) override {
    if (!m_selected.isEmpty() && isDeleteKey(event->key())) {
      emit m_owner.deleteRequested(m_selected);
      return;
    }
    if (!m_selected.isEmpty() && event->key() == Qt::Key_F2) {
      m_owner.startRename(m_selected);
      return;
    }
    QWidget::keyPressEvent(event);
  }

  void leaveEvent(QEvent*) override {
    m_hover = -1;
    update();
  }

  bool event(QEvent* event) override {
    if (event->type() == QEvent::WindowBlocked) {
      // A modal dialog took the input (a long computation's progress, P7),
      // and with it the release: the marker stays where it was dragged to,
      // a feature being dragged where it was.
      unsetCursor();
      m_press = -1;
      m_pressGroup = -1;
      m_dragItem = false;
      QToolTip::hideText();
      update();
      if (m_dragMarker) {
        m_dragMarker = false;
        emit m_owner.markerDragFinished(m_dragPosition);
      }
    }
    if (event->type() == QEvent::ToolTip) {
      const auto* help = static_cast<QHelpEvent*>(event);
      const int i = itemAt(help->pos());
      if (i < 0) {
        QToolTip::hideText();
        return true;
      }
      const DocumentSnapshot::Feature& feature = m_items[i].feature;
      QString tip = QStringLiteral("<b>%1</b>").arg(feature.name.toHtmlEscaped());
      if (isHead(i)) {
        const Group& group = m_groups[m_items[i].group];
        tip = tr("<b>%1</b><br>%n feature(s); double-click to open", nullptr, group.last - group.first + 1)
                  .arg(group.name.toHtmlEscaped());
      } else if (feature.status == QStringLiteral("error") || feature.status == QStringLiteral("warning")) {
        tip += QStringLiteral("<br>%1").arg(feature.error.toHtmlEscaped());
      } else if (feature.suppressed) {
        tip += tr("<br>Suppressed");
      } else if (i >= m_marker) {
        tip += tr("<br>Rolled back");
      }
      QToolTip::showText(help->globalPos(), tip, this, cell(i));
      if (tip != m_loggedTip) {
        m_loggedTip = tip;
        QString plain = tip;
        plain.replace(QStringLiteral("<br>"), QStringLiteral(": "));
        plain.remove(QStringLiteral("<b>")).remove(QStringLiteral("</b>"));
        qDebug().noquote() << QStringLiteral("Timeline tooltip: %1").arg(plain);
      }
      return true;
    }
    return QWidget::event(event);
  }

private:
  TimelineWidget& m_owner;
  QVector<Item> m_items;
  QVector<Group> m_groups;
  std::vector<int> m_left; // where each cell starts
  int m_markerX = kPad;
  int m_marker = 0;
  QString m_selected;   // the anchor of the selection
  QString m_selectedTo; // the other end of a run (Shift+click)
  int m_hover = -1;
  int m_pressGroup = -1; // a press on a group's band or folded cell
  bool m_shift = false;
  // Dragging the marker.
  bool m_dragMarker = false;
  int m_dragPosition = 0;
  // Pressing and dragging a feature.
  int m_press = -1;
  QPoint m_pressAt;
  bool m_dragItem = false;
  int m_dropIndex = -1;
  bool m_dropAllowed = true;
  QHash<int, QString> m_reorder; // index: why not (empty: allowed)
  QString m_loggedTip;
};

TimelineWidget::TimelineWidget(const CommandContext& model, QWidget* parent)
    : QWidget(parent), m_model(model) {
  auto* layout = new QHBoxLayout(this);
  layout->setContentsMargins(2, 0, 2, 0);
  layout->setSpacing(1);
  const bool floating = chromeStyle() == ChromeStyle::Floating;
  const auto button = [this, layout, floating](const char* id, QStyle::StandardPixmap icon, const char* symbol,
                                               const QString& tip, std::function<int()> position) {
    auto* step = new QToolButton(this);
    step->setToolTip(tip);
    step->setAutoRaise(true);
    if (floating) {
      setFlatCardButton(step);
    }
    step->setFocusPolicy(Qt::NoFocus);
    connect(step, &QToolButton::clicked, this, [this, position] { emit markerRequested(position()); });
    layout->addWidget(step);
    m_buttons.append({QString::fromLatin1(id), step});
    m_buttonIcons.append(static_cast<int>(icon));
    m_buttonSymbols.append(floating ? QString::fromLatin1(symbol) : QString());
  };
  button("start", QStyle::SP_MediaSkipBackward, "backward.end.fill", tr("Move to Beginning"), [] { return 0; });
  button("back", QStyle::SP_MediaSeekBackward, "backward.fill", tr("Step Back"),
         [this] { return std::max(0, m_strip->marker() - 1); });
  button("forward", QStyle::SP_MediaSeekForward, "forward.fill", tr("Step Forward"),
         [this] { return std::min(m_strip->count(), m_strip->marker() + 1); });
  button("end", QStyle::SP_MediaSkipForward, "forward.end.fill", tr("Move to End"), [this] { return m_strip->count(); });

  m_strip = new TimelineStrip(*this);
  m_scroll = new QScrollArea(this);
  m_scroll->setWidget(m_strip);
  if (floating) {
    // The card's material shows through (setWidget makes both opaque).
    m_scroll->viewport()->setAutoFillBackground(false);
    m_strip->setAutoFillBackground(false);
  }
  m_scroll->setWidgetResizable(false);
  m_scroll->setFrameShape(QFrame::NoFrame);
  m_scroll->setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
  // Room for the scroll bar only while the features do not fit, and no
  // more height than that when the dock is made taller.
  m_scroll->setFixedHeight(kHeight);
  setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
  layout->addWidget(m_scroll, 1);
  QScrollBar* scrollBar = m_scroll->horizontalScrollBar();
  connect(scrollBar, &QScrollBar::rangeChanged, this, [this, scrollBar](int, int maximum) {
    m_scroll->setFixedHeight(kHeight + (maximum > 0 ? scrollBar->sizeHint().height() : 0));
  });
  connect(scrollBar, &QScrollBar::valueChanged, this,
          [this] { QTimer::singleShot(0, this, &TimelineWidget::logLayout); });

  m_editor = new QLineEdit(m_strip);
  m_editor->hide();
  connect(m_editor, &QLineEdit::returnPressed, this, [this] {
    const QString uid = m_editing;
    const QString name = m_editor->text().trimmed();
    m_editing.clear();
    m_editor->hide();
    if (!uid.isEmpty() && !name.isEmpty()) {
      emit renamed(uid, name);
    }
  });
  connect(m_editor, &QLineEdit::editingFinished, this, [this] {
    if (!m_editor->hasFocus()) {
      m_editing.clear();
      m_editor->hide();
    }
  });
  applyPalette();
}

void TimelineWidget::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  // The theme changed while Mitcad runs (mitcad#14): the buttons' glyphs
  // and the marker follow at once.
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::StyleChange) {
    applyPalette();
  }
}

void TimelineWidget::applyPalette() {
  if (m_strip == nullptr) {
    return; // still being built
  }
  for (int i = 0; i < m_buttons.size(); ++i) {
    // The system's symbols in a floating card (macOS), else the style's.
    const QIcon standard =
        playbackIcon(*style(), static_cast<QStyle::StandardPixmap>(m_buttonIcons[i]), palette());
    m_buttons[i].second->setIcon(m_buttonSymbols[i].isEmpty() ? standard
                                                              : mac::symbolIcon(m_buttonSymbols[i], standard));
  }
  m_strip->update();
  const QString colours = QStringLiteral("Timeline colours: %1, marker %2, buttons %3, window %4")
                              .arg(darkPalette(palette()) ? QStringLiteral("dark") : QStringLiteral("light"),
                                   markerColor(palette()).name(),
                                   darkPalette(palette()) ? palette().color(QPalette::ButtonText).name()
                                                          : QStringLiteral("the style's"),
                                   palette().color(QPalette::Window).name());
  if (colours != m_loggedColours) {
    m_loggedColours = colours;
    qDebug().noquote() << colours;
  }
}

void TimelineWidget::rebuild(const DocumentSnapshot& snapshot) {
  m_snapshot = snapshot;
  QHash<QString, int> colours;
  for (int i = 1; i < snapshot.components.size(); ++i) {
    colours.insert(snapshot.components[i].uid, i - 1);
  }
  QVector<TimelineStrip::Item> items;
  QHash<QString, int> index;
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    index.insert(feature.uid, static_cast<int>(items.size()));
    items.append({feature, colours.value(feature.component, -1), -1});
  }
  // Timeline groups (P9): runs of features, folded or open.
  QVector<TimelineStrip::Group> groups;
  QSet<QString> names;
  for (const DocumentSnapshot::Group& group : snapshot.groups) {
    int first = static_cast<int>(items.size());
    int last = -1;
    for (const QString& uid : group.features) {
      if (index.contains(uid)) {
        first = std::min(first, index.value(uid));
        last = std::max(last, index.value(uid));
      }
    }
    if (last < 0) {
      continue;
    }
    for (int i = first; i <= last; ++i) {
      items[i].group = static_cast<int>(groups.size());
    }
    groups.append({group.name, first, last, m_collapsed.contains(group.name)});
    names.insert(group.name);
  }
  m_collapsed.intersect(names); // groups that are gone
  m_strip->setItems(std::move(items), snapshot.marker, std::move(groups));
  if (m_strip->indexOf(m_strip->selected()) < 0) {
    m_strip->select(QString());
  }
  QTimer::singleShot(0, this, &TimelineWidget::logLayout);
}

void TimelineWidget::setGroupCollapsed(const QString& name, bool collapsed) {
  if (m_collapsed.contains(name) == collapsed) {
    return;
  }
  if (collapsed) {
    m_collapsed.insert(name);
  } else {
    m_collapsed.remove(name);
  }
  qDebug().noquote() << QStringLiteral("Timeline group %1 %2").arg(name, collapsed ? QStringLiteral("collapsed")
                                                                                   : QStringLiteral("expanded"));
  rebuild(DocumentSnapshot(m_snapshot));
}

void TimelineWidget::groupRenamed(const QString& name, const QString& newName) {
  if (m_collapsed.remove(name)) {
    m_collapsed.insert(newName);
  }
}

QStringList TimelineWidget::selectedFeatures() const { return m_strip->selectedRun(); }

void TimelineWidget::selectFeature(const QString& uid) {
  // A feature in a folded group opens it.
  int i = m_strip->indexOf(uid);
  if (i >= 0 && m_strip->hidden(i)) {
    setGroupCollapsed(m_strip->groups()[m_strip->item(i)->group].name, false);
    i = m_strip->indexOf(uid);
  }
  m_strip->select(i >= 0 ? uid : QString());
  if (i >= 0) {
    const QRect cell = m_strip->cell(i);
    m_scroll->ensureVisible(cell.center().x(), cell.center().y(), kItem * 2, 0);
  }
}

QString TimelineWidget::selectedFeature() const { return m_strip->selected(); }

void TimelineWidget::startRename(const QString& uid) {
  const int i = m_strip->indexOf(uid);
  if (i < 0) {
    return;
  }
  selectFeature(uid);
  m_editing = uid;
  const QRect cell = m_strip->cell(i);
  m_editor->setText(m_strip->item(i)->feature.name);
  m_editor->setGeometry(cell.left(), cell.top() + 2, std::max(140, cell.width()), cell.height() - 4);
  m_editor->show();
  m_editor->raise();
  m_editor->setFocus();
  m_editor->selectAll();
  qDebug().noquote() << QStringLiteral("Renaming %1 in the timeline").arg(m_strip->item(i)->feature.name);
}

bool TimelineWidget::cancelEditing() {
  if (m_editing.isEmpty()) {
    return false;
  }
  m_editing.clear();
  m_editor->hide();
  return true;
}

void TimelineWidget::logLayout() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  for (int i = 0; i < m_strip->count(); ++i) {
    if (m_strip->hidden(i) || m_strip->isHead(i)) {
      continue; // in a folded group
    }
    const QPoint at = GlassCard::mapToHost(m_strip, m_strip->cell(i).center());
    lines << QStringLiteral("Timeline %1 at %2,%3").arg(m_strip->item(i)->feature.name).arg(at.x()).arg(at.y());
  }
  for (const TimelineStrip::Group& group : m_strip->groups()) {
    const QPoint at = GlassCard::mapToHost(m_strip, m_strip->groupRect(group).center());
    lines << QStringLiteral("Timeline group %1 at %2,%3").arg(group.name).arg(at.x()).arg(at.y());
  }
  const QPoint marker = GlassCard::mapToHost(m_strip, m_strip->markerRect().center());
  lines << QStringLiteral("Timeline marker at %1,%2").arg(marker.x()).arg(marker.y());
  for (const auto& [id, button] : std::as_const(m_buttons)) {
    const QPoint at = GlassCard::mapToHost(button, button->rect().center());
    lines << QStringLiteral("Timeline button %1 at %2,%3").arg(id).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
