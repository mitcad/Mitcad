// SPDX-License-Identifier: MIT
#include "TimelineWidget.hpp"
#include "Bindings.hpp"
#include "TimelinePresentation.hpp"

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <functional>
#include <utility>

#include <QContextMenuEvent>
#include <QAction>
#include <QCursor>
#include <QApplication>
#include <QDockWidget>
#include <QFocusEvent>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QHelpEvent>
#include <QIcon>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QResizeEvent>
#include <QScrollArea>
#include <QScrollBar>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QWheelEvent>
#include <QtLogging>

#include "ComponentColors.hpp"
#include "FeatureStatus.hpp"
#include "TimelineSearch.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacChrome.hpp"
#include "../platform/MacDialogs.hpp"

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

constexpr int kPad = timeline::padding;
constexpr int kItem = timeline::cell;
constexpr int kIcon = timeline::icon;
constexpr int kGap = timeline::gap;
constexpr int kMarkerSlot = timeline::marker;
constexpr int kTop = timeline::top;
constexpr int kBand = 3; // the component colour, in the cell's room under its icon
constexpr int kBandTop = kTop + kItem - kBand;
constexpr int kHeight = timeline::height;
constexpr int kDragPixels = 6;
constexpr int kRunMark = 8; // stands for a run of features the filter hides (mitcad#98)

const QColor kErrorFill(0xf8, 0xc6, 0xc6);
const QColor kErrorBorder(0xd0, 0x20, 0x20);
const QColor kWarningFill(0xff, 0xec, 0xb0);
const QColor kWarningBorder(0xa8, 0x7d, 0x00); // at least 3:1 on the light strip (mitcad#961)
const QColor kWarningTint(0xf2, 0xb2, 0x00);   // the Floating light theme's warning fill, tinted
const QColor kHoverFill(0xe9, 0xf1, 0xfc);
const QColor kMarkerColor(0x2b, 0x3a, 0x4a);
// The same on a dark palette (fills that the light ones would glare among);
// the error fill plainly red beside the strip's background (mitcad#961).
const QColor kErrorFillDark(0x86, 0x2c, 0x34);
const QColor kWarningFillDark(0x6e, 0x5a, 0x1a);
// The status rings brighter on a dark palette, plain beside the fills and the
// selection's ring (mitcad#961).
const QColor kErrorBorderDark(0xff, 0x5a, 0x5a);
const QColor kWarningBorderDark(0xf2, 0xc2, 0x30);
const QColor kHoverFillDark(0x34, 0x3c, 0x48);
const QColor kMarkerColorDark(0xc8, 0xd0, 0xda); // on a dark theme's background (mitcad#14)
const QColor kMarkerHandle(0xe0, 0x7b, 0x1a);
const QColor kAllowed(0x20, 0xa0, 0x40);
const QColor kRefused(0xd0, 0x20, 0x20);
const QColor kGroupBand(0x7d, 0x8c, 0x9e); // over an open group's features (P9)
const QColor kFoundBorder(0xff, 0x7a, 0x00); // a match of the search (mitcad#179), light or dark

// The colours of the strip's cells and marker. Docked: the fixed ones above
// (light or dark), with the selection and the marker's handle in the window's
// one accent colour (mitcad#251). Floating: derived from the palette and the
// Theme's colours, so that they sit on the card's material and follow the
// accent colour; the handle stays orange.
struct StripColors {
  QColor errorFill, errorBorder, warningFill, warningBorder, selectedFill, selectedBorder, hoverFill, marker;
  QColor found = kFoundBorder;
  QColor handle = kMarkerHandle;
};

StripColors stripColors(const QPalette& palette) {
  const bool dark = isDarkPalette(palette);
  if (chromeStyle() != ChromeStyle::Floating) {
    const QColor accent = actionColor(palette);
    QColor selected = accent;
    selected.setAlphaF(dark ? 0.30f : 0.18f);
    return {dark ? kErrorFillDark : kErrorFill,     dark ? kErrorBorderDark : kErrorBorder,
            dark ? kWarningFillDark : kWarningFill, dark ? kWarningBorderDark : kWarningBorder,
            selected,                               accent,
            dark ? kHoverFillDark : kHoverFill,     dark ? kMarkerColorDark : kMarkerColor,
            kFoundBorder,                           accent};
  }
  const auto tint = [](QColor color, qreal alpha) {
    color.setAlphaF(static_cast<float>(alpha));
    return color;
  };
  const QColor text = palette.color(QPalette::WindowText);
  const QColor accent = accentColor(palette);
  // The status fills stronger on a dark card, where a faint tint vanishes;
  // on a light one the warning amber as Docked, not the text's brown
  // (mitcad#961).
  return {tint(errorColor(palette), dark ? 0.40 : 0.24),  errorColor(palette),
          dark ? tint(warningColor(palette), 0.36) : tint(kWarningTint, 0.30),
          dark ? warningColor(palette) : kWarningBorder,
          tint(accent, 0.24),                              accent,
          tint(text, 0.09),                                text,
          kFoundBorder};
}

// The cells' colours (FeatureStatus.hpp): the pointer's ring on a failed or
// warned cell the text colour at 80 %, the strip's background around the status
// badge.
timeline::CellColors cellColors(const StripColors& strip, const QPalette& palette) {
  QColor hoverRing = palette.color(QPalette::WindowText);
  hoverRing.setAlphaF(0.8f);
  return {strip.errorFill,    strip.errorBorder,    strip.warningFill, strip.warningBorder,
          strip.selectedFill, strip.selectedBorder, strip.hoverFill,   strip.found,
          hoverRing,          palette.color(QPalette::Window)};
}

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
    int colour = -1; // index into the component colours (ComponentColors.hpp), -1 for the root
    int group = -1;  // index into the groups
    bool filtered = false; // a feature the filter hides (mitcad#98)
  };
  // A timeline group (P9): its first and last item, folded or open.
  struct Group {
    QString name;
    int first = 0;
    int last = 0;
    bool collapsed = false;
  };

  explicit TimelineStrip(TimelineWidget& owner) : QWidget(&owner), m_owner(owner) {
    setObjectName(QStringLiteral("timelineStrip"));
    setMouseTracking(true);
    setFocusPolicy(Qt::ClickFocus);
    setFixedHeight(kHeight);
    m_hoverTimer.setSingleShot(true);
    m_hoverTimer.setInterval(350);
    connect(&m_hoverTimer, &QTimer::timeout, this, [this] {
      if (m_hover < 0 || m_dragItem || m_dragMarker) return;
      // On a feature's fx badge: there, which shows its card (mitcad#862).
      const QPoint pointer = mapFromGlobal(QCursor::pos());
      const QPoint at = badgeAt(pointer) == m_hover ? pointer : cell(m_hover).center();
      QHelpEvent help(QEvent::ToolTip, at, mapToGlobal(at));
      QApplication::sendEvent(this, &help);
    });
  }

  void setItems(QVector<Item> items, int marker, QVector<Group> groups) {
    setHover(-1);
    m_badgeHover = -1;
    QToolTip::hideText();
    m_items = std::move(items);
    m_groups = std::move(groups);
    m_marker = std::clamp(marker, 0, static_cast<int>(m_items.size()));
    if (m_hasExternalSelection) {
      pruneExternalSelection();
    }
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
    // take no room, nor do those the filter hides: a mark stands for each
    // run of them, with the cells at its left, so that a drop or the
    // marker left of the mark goes before the run and right of it after.
    m_left.assign(m_items.size(), 0);
    m_runs.clear();
    int x = kPad;
    int run = -1; // where the current run's cells are
    for (int i = 0; i <= count(); ++i) {
      if (i == m_marker) {
        m_markerX = x;
        x += kMarkerSlot + kGap;
      }
      if (i < count()) {
        if (m_items[i].filtered) {
          if (run < 0) {
            run = x;
            m_runs.push_back(x);
            x += kRunMark + kGap;
          }
          m_left[static_cast<std::size_t>(i)] = run;
          continue;
        }
        run = -1;
        m_left[static_cast<std::size_t>(i)] = x;
        if (!hidden(i)) {
          x += kItem + kGap;
        }
      }
    }
    setFixedWidth(x + kPad);
    update();
  }

  // The features the filter hides (mitcad#98), and those after the marker.
  int filteredCount() const {
    return static_cast<int>(std::count_if(m_items.begin(), m_items.end(), [](const Item& i) { return i.filtered; }));
  }
  int filteredAfter(int position) const {
    int n = 0;
    for (int i = std::max(0, position); i < count(); ++i) {
      n += m_items[i].filtered ? 1 : 0;
    }
    return n;
  }

  // Where the playback buttons step (mitcad#98): the boundaries of the
  // features shown, so that a run the filter hides is passed in one step.
  int stepBefore(int g) const {
    for (int k = std::min(g, count() + 1) - 1; k > 0; --k) {
      if (shownBoundary(k)) {
        return k;
      }
    }
    return 0;
  }
  int stepAfter(int g) const {
    for (int k = std::max(g, -1) + 1; k < count(); ++k) {
      if (shownBoundary(k)) {
        return k;
      }
    }
    return count();
  }

  // A boundary of the filtered view (mitcad#98): not between two features
  // it hides.
  bool shownBoundary(int g) const {
    return g <= 0 || g >= count() || !m_items[g].filtered || !m_items[g - 1].filtered;
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

  // The matches of the search (mitcad#179), outlined.
  void setFound(QSet<QString> found) {
    if (found != m_found) {
      m_found = std::move(found);
      update();
    }
  }

  QString selected() const { return m_selected; }
  void select(const QString& uid) {
    m_hasExternalSelection = false;
    m_externalSelected.clear();
    m_selected = uid;
    m_selectedTo = uid;
    update();
  }
  void selectFeatures(const QStringList& uids) {
    m_externalSelected = QSet<QString>(uids.begin(), uids.end());
    m_hasExternalSelection = true;
    pruneExternalSelection();
    update();
  }
  // An external selection is exact; Qt Shift+click selects an anchor-to-end
  // run without the features the filter hides.
  QStringList selectedRun() const {
    if (m_hasExternalSelection) {
      QStringList selected;
      for (const Item& item : m_items) {
        if (m_externalSelected.contains(item.feature.uid)) {
          selected.append(item.feature.uid);
        }
      }
      return selected;
    }
    const int a = indexOf(m_selected);
    const int b = indexOf(m_selectedTo);
    QStringList run;
    if (a < 0 || b < 0) {
      return run;
    }
    for (int i = std::min(a, b); i <= std::max(a, b); ++i) {
      if (!m_items[i].filtered) {
        run << m_items[i].feature.uid;
      }
    }
    return run;
  }

  // A cell of a folded group after its first is not shown; the first
  // stands for the group. Nor is one the filter hides.
  bool hidden(int i) const {
    const int g = m_items[i].group;
    return m_items[i].filtered || (g >= 0 && m_groups[g].collapsed && i != m_groups[g].first);
  }
  bool filtered(int i) const { return m_items[i].filtered; }
  bool isHead(int i) const {
    const int g = m_items[i].group;
    return g >= 0 && m_groups[g].collapsed && i == m_groups[g].first;
  }

  QRect cell(int i) const {
    return QRect(m_left[static_cast<std::size_t>(i)], kTop, hidden(i) ? 0 : kItem, kItem);
  }
  QRect markerRect() const { return QRect(m_markerX, 0, kMarkerSlot, height()); }
  // The fx badge on the icon of a feature suppressed by a condition
  // (mitcad#809; its card, mitcad#862), as boundIcon draws it; empty for
  // another feature.
  QRect badgeRect(int i) const {
    if (hidden(i) || isHead(i) || m_items[i].feature.suppressedWhen.isEmpty()) {
      return QRect();
    }
    const QRect icon(cell(i).left() + (kItem - kIcon) / 2, kTop + (kItem - kIcon) / 2, kIcon, kIcon);
    const int side = std::max(7, kIcon * 3 / 5);
    return QRect(icon.right() + 1 - side, icon.bottom() + 1 - side, side, side).adjusted(-1, -1, 1, 1);
  }
  // The feature whose fx badge is at a point, or -1.
  int badgeAt(const QPoint& at) const {
    for (int i = 0; i < count(); ++i) {
      if (badgeRect(i).contains(at)) {
        return i;
      }
    }
    return -1;
  }
  // The band over an open group's cells.
  QRect bandRect(const Group& group) const {
    const int left = cell(group.first).left();
    return QRect(left, 1, cell(group.last).right() + 1 - left, kTop - 2);
  }
  // Where a group is shown: its folded cell or its band.
  QRect groupRect(const Group& group) const { return group.collapsed ? cell(group.first) : bandRect(group); }

private:
  void pruneExternalSelection() {
    QSet<QString> retained;
    QString first;
    for (const Item& item : m_items) {
      if (m_externalSelected.contains(item.feature.uid)) {
        retained.insert(item.feature.uid);
        if (first.isEmpty()) {
          first = item.feature.uid;
        }
      }
    }
    m_externalSelected = std::move(retained);
    m_selected = first;
    m_selectedTo = first;
  }

  // Where the boundary before feature g is (g = count(): after the last).
  double boundary(int g) const {
    if (g == m_marker) {
      return markerRect().center().x() + 0.5;
    }
    if (g < count()) {
      return cell(g).left() - kGap / 2.0;
    }
    if (count() > 0 && m_items[count() - 1].filtered && !m_runs.empty()) {
      return m_runs.back() + kRunMark + kGap / 2.0; // after the last run's mark
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
      if (open(g) && shownBoundary(g) && std::abs(boundary(g) - x) < std::abs(boundary(best) - x)) {
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
    const timeline::CellColors cells = cellColors(colors, palette());
    QHash<QString, QString> cellLines; // the painted state of cells with a status (mitcad#961)
    for (int i = 0; i < count(); ++i) {
      if (hidden(i)) {
        continue;
      }
      const Item& item = m_items[i];
      const DocumentSnapshot::Feature& feature = item.feature;
      const QRectF rect = QRectF(cell(i)).adjusted(0.5, 0.5, -0.5, -0.5);
      const bool head = isHead(i);
      // A folded group shows the worst of its features.
      timeline::CellState state;
      bool off = true;
      const int last = head ? m_groups[item.group].last : i;
      for (int k = i; k <= last; ++k) {
        const DocumentSnapshot::Feature& member = m_items[k].feature;
        state.state = std::max(state.state, featureState(member.status));
        // An import note on a feature that is not built (mitcad#170).
        state.noted = state.noted || !member.importNote.isEmpty();
        off = off && (member.suppressed || k >= m_marker);
        // A match of the search (mitcad#179).
        state.found = state.found || m_found.contains(member.uid);
      }
      state.selected = run.contains(feature.uid);
      state.hovered = i == m_hover;
      state.rolledBack = off;
      // The status ring stays inside the selection's (mitcad#961).
      const timeline::CellLook look = timeline::cellLook(state, cells);
      timeline::paintCellFrame(painter, QRectF(cell(i)), look);
      if (state.state != FeatureState::Ok || state.noted) {
        cellLines.insert(head ? m_groups[item.group].name : feature.name, timeline::describeCell(state, look));
      }
      if (item.colour >= 0) {
        const QColor colour = componentColor(item.colour);
        painter.setPen(Qt::NoPen);
        painter.setBrush(colour);
        painter.drawRoundedRect(QRectF(rect.left() + 4, kBandTop, kItem - 8, kBand - 0.5), 1.5, 1.5);
      }
      painter.setOpacity(off ? 0.3 : 1.0);
      QIcon icon = themeIcon(head ? QStringLiteral("folder") : featureIcon(feature.type));
      // Suppressed when a condition holds (mitcad#809): an fx badge.
      if (!head && !feature.suppressedWhen.isEmpty()) {
        icon = boundIcon(icon);
      }
      const QPixmap pixmap = icon.pixmap(QSize(kIcon, kIcon), devicePixelRatioF());
      painter.drawPixmap(cell(i).left() + (kItem - kIcon) / 2, kTop + (kItem - kIcon) / 2, pixmap);
      painter.setOpacity(1.0);
      if (!head && feature.suppressed) {
        painter.setPen(QPen(strikeColor(palette()), 1.6));
        painter.drawLine(rect.bottomLeft() + QPointF(4, -4), rect.topRight() + QPointF(-4, 4));
      }
      // The status badge over everything, whatever the cell's state
      // (mitcad#961): a folded group's when one of its features failed.
      if (look.badge) {
        timeline::paintStatusBadge(painter, QRectF(cell(i)), state.state, cells.halo);
      }
    }
    // The painted state of the cells with a status, for UI tests: logged
    // when it changes (mitcad#961).
    for (auto line = cellLines.cbegin(); line != cellLines.cend(); ++line) {
      if (m_loggedCells.value(line.key()) != line.value()) {
        qDebug().noquote() << QStringLiteral("Timeline cell %1: %2").arg(line.key(), line.value());
      }
    }
    m_loggedCells = std::move(cellLines);
    // A run of features the filter hides (mitcad#98): a dotted bar.
    for (const int at : m_runs) {
      painter.setPen(Qt::NoPen);
      painter.setBrush(strikeColor(palette()));
      const double middle = at + kRunMark / 2.0;
      for (int dot = 0; dot < 4; ++dot) {
        painter.drawEllipse(QPointF(middle, kTop + kItem * (dot + 1) / 5.0), 1.3, 1.3);
      }
    }
    // The history marker: a bar with a handle to drag, from the groups'
    // bands down to the cells' bottom.
    const double x = markerRect().center().x() + 1.0; // the 2 px bar on whole pixels
    painter.setPen(QPen(colors.marker, 2.0));
    painter.drawLine(QPointF(x, 4), QPointF(x, kTop + kItem - 1)); // square caps
    painter.setPen(QPen(colors.marker, 1.0));
    painter.setBrush(colors.handle);
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
    setHover(-1);
    QToolTip::hideText();
    const QPoint at = event->position().toPoint();
    // A click on a feature's fx badge pins its bindings card (mitcad#862).
    if (const int bound = badgeAt(at); bound >= 0) {
      m_press = -1;
      m_pressGroup = -1;
      m_dragItem = false;
      const QRect badge = badgeRect(bound);
      qDebug().noquote() << QStringLiteral("Timeline fx click %1").arg(m_items[bound].feature.name);
      emit m_owner.bindingsRequested(m_items[bound].feature.uid,
                                     QRect(mapToGlobal(badge.topLeft()), badge.size()), true);
      return;
    }
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
      setHover(hover);
      update();
    }
    // Onto a feature's fx badge within its cell: the card after the same
    // pause (mitcad#862).
    const int badge = badgeAt(at);
    if (badge != m_badgeHover) {
      m_badgeHover = badge;
      if (badge >= 0 && badge == m_hover) {
        m_hoverTimer.start();
      }
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
        m_hasExternalSelection = false;
        m_externalSelected.clear();
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
    // A quick second click on an fx badge pins its card again, no edit.
    if (badgeAt(at) >= 0) {
      m_press = -1;
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
    if (event->matches(QKeySequence::Find)) {
      m_owner.openSearch(); // Ctrl+F, Cmd+F on macOS (mitcad#179)
      return;
    }
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
    setHover(-1);
    QToolTip::hideText();
    update();
  }

  bool event(QEvent* event) override {
    if (event->type() == QEvent::Hide || event->type() == QEvent::WindowDeactivate) {
      setHover(-1);
      QToolTip::hideText();
    }
    if (event->type() == QEvent::WindowBlocked) {
      // A modal dialog took the input (a long computation's progress, P7),
      // and with it the release: the marker stays where it was dragged to,
      // a feature being dragged where it was.
      unsetCursor();
      setHover(-1);
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
      // The pointer on a feature's fx badge: its bindings card (mitcad#862).
      if (const int bound = badgeAt(help->pos()); bound >= 0 && !m_dragItem && !m_dragMarker) {
        QToolTip::hideText();
        const QRect badge = badgeRect(bound);
        emit m_owner.bindingsRequested(m_items[bound].feature.uid,
                                       QRect(mapToGlobal(badge.topLeft()), badge.size()), false);
        return true;
      }
      const int i = itemAt(help->pos());
      // The marker rolls back the features the filter hides too
      // (mitcad#98): how many of them are after it.
      const int hiddenAfter = filteredAfter(m_marker);
      if (i < 0 && filteredCount() > 0 && markerRect().adjusted(-3, 0, 3, 0).contains(help->pos())) {
        const QString tip = tr("History marker: %n feature(s) of components not shown are rolled back", nullptr,
                               hiddenAfter);
        QToolTip::showText(help->globalPos(), tip, this, markerRect());
        if (tip != m_loggedTip) {
          m_loggedTip = tip;
          qDebug().noquote() << QStringLiteral("Timeline tooltip: %1").arg(tip);
        }
        return true;
      }
      if (i < 0) {
        QToolTip::hideText();
        return true;
      }
      const DocumentSnapshot::Feature& feature = m_items[i].feature;
      const QString title = QStringLiteral("<b>%1</b>").arg(feature.name.toHtmlEscaped());
      // The feature's details (its kind, component and sizes), then its
      // state; the log names the feature and its state only.
      const QString details = m_owner.m_featureDetails ? m_owner.m_featureDetails(feature.uid) : title;
      QString state;
      if (isHead(i)) {
        const Group& group = m_groups[m_items[i].group];
        state = tr("<b>%1</b><br>%n feature(s); double-click to open", nullptr, group.last - group.first + 1)
                    .arg(group.name.toHtmlEscaped());
      } else if (feature.status == QStringLiteral("error") || feature.status == QStringLiteral("warning")) {
        state = QStringLiteral("<br>%1").arg(feature.error.toHtmlEscaped());
      } else if (feature.suppressed) {
        // A placeholder says why the import skipped its item (mitcad#170).
        state = feature.error.isEmpty() ? tr("<br>Suppressed")
                                        : tr("<br>Suppressed: %1").arg(feature.error.toHtmlEscaped());
      } else if (i >= m_marker) {
        state = tr("<br>Rolled back");
        if (!feature.error.isEmpty()) {
          state += QStringLiteral("<br>%1").arg(feature.error.toHtmlEscaped());
        }
      }
      if (!isHead(i) && (!feature.succeeded() || feature.suppressed || i >= m_marker)) {
        state += tr("<br>No geometry to show");
      }
      // The condition it is suppressed under (mitcad#809).
      if (!isHead(i) && !feature.suppressedWhen.isEmpty()) {
        state += tr("<br>Suppressed when %1").arg(feature.suppressedWhen.toHtmlEscaped());
      }
      const QString tip = isHead(i) ? state : details + state;
      QToolTip::showText(help->globalPos(), tip, this, cell(i));
      if (tip != m_loggedTip) {
        m_loggedTip = tip;
        QString plain = isHead(i) ? state : title + state;
        plain.replace(QStringLiteral("<br>"), QStringLiteral(": "));
        plain.remove(QStringLiteral("<b>")).remove(QStringLiteral("</b>"));
        qDebug().noquote() << QStringLiteral("Timeline tooltip: %1").arg(plain);
      }
      return true;
    }
    return QWidget::event(event);
  }

private:
  void setHover(int index) {
    m_hover = index;
    m_hoverTimer.stop();
    QStringList features;
    if (index >= 0) {
      const int last = isHead(index) ? m_groups[m_items[index].group].last : index;
      for (int i = index; i <= last; ++i) {
        const auto& feature = m_items[i].feature;
        if (i < m_marker && feature.succeeded() && !feature.suppressed) features << feature.uid;
      }
      m_hoverTimer.start();
    }
    if (features != m_owner.m_hoveredFeatures) {
      m_owner.m_hoveredFeatures = features;
      emit m_owner.featuresHovered(features);
    }
  }
  QTimer m_hoverTimer;
  int m_badgeHover = -1; // the feature whose fx badge has the pointer (mitcad#862)
  TimelineWidget& m_owner;
  QVector<Item> m_items;
  QVector<Group> m_groups;
  std::vector<int> m_left; // where each cell starts
  std::vector<int> m_runs; // where the marks of hidden runs are (mitcad#98)
  int m_markerX = kPad;
  int m_marker = 0;
  QString m_selected;   // the anchor of the selection
  QString m_selectedTo; // the other end of a run (Shift+click)
  QSet<QString> m_externalSelected; // exact selection mirrored by another presentation
  bool m_hasExternalSelection = false;
  QSet<QString> m_found; // the search's matches (mitcad#179)
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
  QHash<QString, QString> m_loggedCells; // name: the painted state last logged (mitcad#961)
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
  // A step passes over the features the filter hides (mitcad#98).
  button("back", QStyle::SP_MediaSeekBackward, "backward.fill", tr("Step Back"),
         [this] { return m_strip->stepBefore(m_strip->marker()); });
  button("forward", QStyle::SP_MediaSeekForward, "forward.fill", tr("Step Forward"),
         [this] { return m_strip->stepAfter(m_strip->marker()); });
  button("end", QStyle::SP_MediaSkipForward, "forward.end.fill", tr("Move to End"), [this] { return m_strip->count(); });

  // The filter (mitcad#98): which components' features are shown.
  m_filterButton = new QToolButton(this);
  m_filterButton->setAutoRaise(true);
  m_filterButton->setCheckable(true);
  if (floating) {
    setFlatCardButton(m_filterButton);
  }
  m_filterButton->setFocusPolicy(Qt::NoFocus);
  connect(m_filterButton, &QToolButton::clicked, this, [this] {
    updateFilterButton(); // the click toggled it; the filter decides
    showFilterMenu();
  });
  layout->addWidget(m_filterButton);
  updateFilterButton();

  // The search (mitcad#179): its button, and the field with the count of
  // matches and the step buttons, shown while it is open.
  const QString findKey = QKeySequence(QKeySequence::Find).toString(QKeySequence::NativeText);
  m_searchButton = new QToolButton(this);
  m_searchButton->setAutoRaise(true);
  m_searchButton->setCheckable(true);
  if (floating) {
    setFlatCardButton(m_searchButton);
  }
  m_searchButton->setFocusPolicy(Qt::NoFocus);
  m_searchButton->setToolTip(findKey.isEmpty() ? tr("Search the Timeline")
                                               : tr("Search the Timeline (%1)").arg(findKey));
  connect(m_searchButton, &QToolButton::clicked, this, [this] {
    if (isSearchOpen()) {
      closeSearch();
    } else {
      openSearch();
    }
  });
  layout->addWidget(m_searchButton);
  m_searchBar = new QWidget(this);
  auto* bar = new QHBoxLayout(m_searchBar);
  bar->setContentsMargins(2, 0, 2, 0);
  bar->setSpacing(1);
  m_searchField = new QLineEdit(m_searchBar);
  m_searchField->setObjectName(QStringLiteral("timelineSearch"));
  m_searchField->setPlaceholderText(tr("Name or type; is:failed, in:component"));
  m_searchField->setToolTip(tr("Finds features by name or type; filters such as is:failed and in:<component>"));
  m_searchField->setClearButtonEnabled(true);
  mac::styleSearchField(m_searchField);
  m_searchField->setFixedWidth(240);
  m_searchField->installEventFilter(this);
  QAction* filters = m_searchField->addAction(themeIcon(QStringLiteral("filter")), QLineEdit::TrailingPosition);
  filters->setToolTip(tr("Filters: component and status"));
  connect(filters, &QAction::triggered, this, [this] { showSearchMenu(); });
  connect(m_searchField, &QLineEdit::textChanged, this, &TimelineWidget::searchTextChanged);
  bar->addWidget(m_searchField);
  m_searchCount = new QLabel(m_searchBar);
  m_searchCount->setMinimumWidth(m_searchCount->fontMetrics().horizontalAdvance(QStringLiteral("888 of 888")) + 8);
  m_searchCount->setAlignment(Qt::AlignCenter);
  bar->addWidget(m_searchCount);
  const auto stepButton = [this, bar, floating](QStyle::StandardPixmap icon, const QString& tip, int delta) {
    auto* step = new QToolButton(m_searchBar);
    step->setAutoRaise(true);
    if (floating) {
      setFlatCardButton(step);
    }
    step->setFocusPolicy(Qt::NoFocus);
    step->setIcon(style()->standardIcon(icon));
    step->setToolTip(tip);
    connect(step, &QToolButton::clicked, this, [this, delta] { stepSearch(delta); });
    bar->addWidget(step);
    return step;
  };
  m_searchPrevious = stepButton(QStyle::SP_ArrowLeft, tr("Previous Match (Shift+Enter)"), -1);
  m_searchNext = stepButton(QStyle::SP_ArrowRight, tr("Next Match (Enter)"), 1);
  m_searchBar->hide();
  layout->addWidget(m_searchBar);

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
  // The wheel over the strip and its scroll bar scrolls the strip sideways
  // (mitcad#826): ahead of the scroll area, which gives a vertical delta to
  // its vertical bar, and of a floating card, which swallows the rest.
  m_scroll->viewport()->installEventFilter(this);
  scrollBar->installEventFilter(this);
  // Its places are logged again when it moves in the window (a banner or
  // another panel makes room above or beside it).
  new PlaceWatcher(this, [this] { logLayout(); });

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
  updateFilterButton(); // its icon in the theme's colours
  m_searchButton->setIcon(themeIcon(QStringLiteral("search")));
  m_searchPrevious->setIcon(playbackIcon(*style(), QStyle::SP_ArrowLeft, palette()));
  m_searchNext->setIcon(playbackIcon(*style(), QStyle::SP_ArrowRight, palette()));
  m_strip->update();
  const QString colours = QStringLiteral("Timeline colours: %1, marker %2, buttons %3, window %4, handle %5")
                              .arg(darkPalette(palette()) ? QStringLiteral("dark") : QStringLiteral("light"),
                                   markerColor(palette()).name(),
                                   darkPalette(palette()) ? palette().color(QPalette::ButtonText).name()
                                                          : QStringLiteral("the style's"),
                                   palette().color(QPalette::Window).name(), stripColors(palette()).handle.name());
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
  // A filter of a component that is gone shows everything again.
  if (!m_filter.isEmpty() && m_filter != kActiveFilter && snapshot.component(m_filter) == nullptr) {
    m_filter.clear();
    updateFilterButton();
  }
  const QSet<QString> shown = filteredComponents();
  QHash<QString, QString> occurrenceComponents;
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&)> collect =
      [&](const QVector<DocumentSnapshot::Occurrence>& occurrences) {
        for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
          occurrenceComponents.insert(occurrence.uid, occurrence.component);
          collect(occurrence.children);
        }
      };
  collect(snapshot.occurrences);
  QVector<TimelineStrip::Item> items;
  QHash<QString, int> index;
  int hidden = 0;
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    index.insert(feature.uid, static_cast<int>(items.size()));
    const bool filtered = !shown.isEmpty() && !passesFilter(feature, shown, occurrenceComponents);
    hidden += filtered ? 1 : 0;
    items.append({feature, colours.value(feature.component, -1), -1, filtered});
  }
  // Timeline groups (P9): runs of features, folded or open; with a filter,
  // of the features it shows (a group with none is not shown).
  QVector<TimelineStrip::Group> groups;
  QSet<QString> names;
  for (const DocumentSnapshot::Group& group : snapshot.groups) {
    names.insert(group.name);
    int first = static_cast<int>(items.size());
    int last = -1;
    for (const QString& uid : group.features) {
      if (index.contains(uid) && !items[index.value(uid)].filtered) {
        first = std::min(first, index.value(uid));
        last = std::max(last, index.value(uid));
      }
    }
    if (last < 0) {
      continue;
    }
    for (int i = first; i <= last; ++i) {
      if (!items[i].filtered) {
        items[i].group = static_cast<int>(groups.size());
      }
    }
    groups.append({group.name, first, last, m_collapsed.contains(group.name)});
  }
  m_collapsed.intersect(names); // groups that are gone
  m_strip->setItems(std::move(items), snapshot.marker, std::move(groups));
  if (m_strip->indexOf(m_strip->selected()) < 0 ||
      (!m_strip->selected().isEmpty() && m_strip->filtered(m_strip->indexOf(m_strip->selected())))) {
    m_strip->select(QString());
  }
  if (m_searching) {
    updateMatches(); // the features, their names and states may have changed
  }
  const QString logged = shown.isEmpty() ? QStringLiteral("all components")
                                         : QStringLiteral("%1: %2 of %3 features")
                                               .arg(filterLabel())
                                               .arg(snapshot.features.size() - hidden)
                                               .arg(snapshot.features.size());
  if (logged != m_loggedFilter) {
    m_loggedFilter = logged;
    qDebug().noquote() << QStringLiteral("Timeline filter: %1").arg(logged);
  }
  QTimer::singleShot(0, this, &TimelineWidget::logLayout);
}

const QString TimelineWidget::kActiveFilter = QStringLiteral("active");

void TimelineWidget::setFilter(const QString& filter, bool subcomponents) {
  if (filter == m_filter && subcomponents == m_subcomponents) {
    return;
  }
  m_filter = filter;
  m_subcomponents = subcomponents;
  updateFilterButton();
  rebuild(DocumentSnapshot(m_snapshot));
  emit filterChanged();
}

QSet<QString> TimelineWidget::filteredFeatures() const {
  QSet<QString> hidden;
  for (int index = 0; index < m_strip->count(); ++index) {
    if (m_strip->filtered(index)) {
      hidden.insert(m_strip->item(index)->feature.uid);
    }
  }
  return hidden;
}

QSet<QString> TimelineWidget::filteredComponents() const {
  if (m_filter.isEmpty()) {
    return {};
  }
  const QString component = m_filter == kActiveFilter ? m_snapshot.activeComponent : m_filter;
  QSet<QString> components{component};
  if (!m_subcomponents) {
    return components;
  }
  // The components placed in it, at any depth.
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&, bool)> walk =
      [&](const QVector<DocumentSnapshot::Occurrence>& occurrences, bool inside) {
        for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
          const bool in = inside || occurrence.component == component;
          if (inside) {
            components.insert(occurrence.component);
          }
          walk(occurrence.children, in);
        }
      };
  walk(m_snapshot.occurrences, component == QStringLiteral("C0"));
  return components;
}

bool TimelineWidget::passesFilter(const DocumentSnapshot::Feature& feature, const QSet<QString>& components,
                                  const QHash<QString, QString>& occurrenceComponents) const {
  if (components.contains(feature.component)) {
    return true;
  }
  if (!feature.isJoint()) {
    return false;
  }
  // A joint or rigid group of another component that places occurrences of
  // a shown one: its sides' paths or its members.
  const auto reaches = [&](const QJsonArray& occurrences) {
    return std::any_of(occurrences.begin(), occurrences.end(), [&](const QJsonValue& uid) {
      return components.contains(occurrenceComponents.value(uid.toString()));
    });
  };
  const QJsonObject joint = m_snapshot.joint(feature.uid);
  if (!joint.isEmpty()) {
    return reaches(joint.value(QStringLiteral("a")).toObject().value(QStringLiteral("path")).toArray()) ||
           reaches(joint.value(QStringLiteral("b")).toObject().value(QStringLiteral("path")).toArray());
  }
  for (const QJsonValue& value : m_snapshot.joints.value(QStringLiteral("rigid_groups")).toArray()) {
    const QJsonObject group = value.toObject();
    if (group.value(QStringLiteral("uid")).toString() == feature.uid) {
      return reaches(group.value(QStringLiteral("occurrences")).toArray());
    }
  }
  return false;
}

QString TimelineWidget::filterLabel() const {
  if (m_filter.isEmpty()) {
    return tr("All Components");
  }
  const QString component = m_filter == kActiveFilter ? m_snapshot.activeComponent : m_filter;
  QString label = m_filter == kActiveFilter ? tr("Active Component (%1)").arg(m_snapshot.componentName(component))
                                            : m_snapshot.componentName(component);
  if (m_subcomponents) {
    label = tr("%1 with Subcomponents").arg(label);
  }
  return label;
}

void TimelineWidget::updateFilterButton() {
  if (m_filterButton == nullptr) {
    return;
  }
  m_filterButton->setIcon(themeIcon(QStringLiteral("filter")));
  m_filterButton->setChecked(!m_filter.isEmpty());
  m_filterButton->setToolTip(m_filter.isEmpty() ? tr("Timeline Filter: all components")
                                                : tr("Timeline Filter: %1").arg(filterLabel()));
}

void TimelineWidget::showFilterMenu() {
  showFilterMenu(m_filterButton->mapToGlobal(QPoint(0, m_filterButton->height())));
}

void TimelineWidget::showFilterMenu(const QPoint& globalPosition) {
  QMenu menu(this);
  QStringList entries;
  const auto add = [&](const QString& text, bool checked, std::function<void()> call) {
    QAction* action = menu.addAction(text);
    action->setCheckable(true);
    action->setChecked(checked);
    connect(action, &QAction::triggered, this, std::move(call));
    entries << text;
  };
  add(tr("All Components"), m_filter.isEmpty(), [this] { setFilter(QString(), m_subcomponents); });
  add(tr("Active Component"), m_filter == kActiveFilter, [this] { setFilter(kActiveFilter, m_subcomponents); });
  menu.addSeparator();
  for (const DocumentSnapshot::Component& component : std::as_const(m_snapshot.components)) {
    const QString uid = component.uid;
    add(component.name, m_filter == uid, [this, uid] { setFilter(uid, m_subcomponents); });
  }
  menu.addSeparator();
  add(tr("Include Subcomponents"), m_subcomponents, [this] { setFilter(m_filter, !m_subcomponents); });
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, globalPosition);
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

void TimelineWidget::setSelectedFeatures(const QStringList& uids) {
  const QStringList before = m_strip->selectedRun();
  m_strip->selectFeatures(uids);
  // Logged when it changed (the browser's rows select here, mitcad#177).
  const QStringList after = m_strip->selectedRun();
  if (after != before) {
    qDebug().noquote() << QStringLiteral("Timeline selected: %1")
                              .arg(after.isEmpty() ? QStringLiteral("nothing") : after.join(QStringLiteral(", ")));
    // The first in sight, unless a folded group or the filter hides it.
    const int first = after.isEmpty() ? -1 : m_strip->indexOf(after.first());
    if (first >= 0 && !m_strip->hidden(first)) {
      const QRect cell = m_strip->cell(first);
      m_scroll->ensureVisible(cell.center().x(), cell.center().y(), kItem * 2, 0);
    }
  }
}

void TimelineWidget::selectFeature(const QString& uid) {
  // A feature the filter hides shows everything again (mitcad#98).
  if (const int at = m_strip->indexOf(uid); at >= 0 && m_strip->filtered(at)) {
    qDebug().noquote() << QStringLiteral("Timeline filter cleared to show %1").arg(m_strip->item(at)->feature.name);
    setFilter(QString(), m_subcomponents);
  }
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
    // A feature's fx badge (mitcad#862).
    if (const QRect badge = m_strip->badgeRect(i); !badge.isEmpty()) {
      const QPoint fx = GlassCard::mapToHost(m_strip, badge.center());
      lines << QStringLiteral("Timeline fx %1 at %2,%3").arg(m_strip->item(i)->feature.name).arg(fx.x()).arg(fx.y());
    }
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
  const QPoint filterAt = GlassCard::mapToHost(m_filterButton, m_filterButton->rect().center());
  lines << QStringLiteral("Timeline button filter at %1,%2").arg(filterAt.x()).arg(filterAt.y());
  const QPoint searchAt = GlassCard::mapToHost(m_searchButton, m_searchButton->rect().center());
  lines << QStringLiteral("Timeline button search at %1,%2").arg(searchAt.x()).arg(searchAt.y());
  if (m_searchBar->isVisible()) {
    for (const auto& [id, widget] : {std::pair<const char*, QWidget*>{"search field", m_searchField},
                                     std::pair<const char*, QWidget*>{"button search-previous", m_searchPrevious},
                                     std::pair<const char*, QWidget*>{"button search-next", m_searchNext}}) {
      const QPoint at = GlassCard::mapToHost(widget, widget->rect().center());
      lines << QStringLiteral("Timeline %1 at %2,%3").arg(QLatin1String(id)).arg(at.x()).arg(at.y());
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

// ---------------------------------------------------------------------------
// The search (mitcad#179)

bool TimelineWidget::isSearchOpen() const { return m_externalSearch || !m_searchBar->isHidden(); }

void TimelineWidget::beginExternalSearch() {
  m_externalSearch = true;
  updateMatches(); // Publish an empty initial presentation too.
}
void TimelineWidget::setSearchText(const QString& text) { m_searchField->setText(text); }
QString TimelineWidget::searchText() const { return m_searchField->text(); }
QString TimelineWidget::searchCountText() const { return m_searchCount->text(); }

void TimelineWidget::openSearch() {
  // A closed timeline dock opens again (the docked layout).
  for (QWidget* parent = parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
    if (auto* dock = qobject_cast<QDockWidget*>(parent)) {
      if (dock->isHidden()) {
        dock->show();
      }
      break;
    }
  }
  const bool wasOpen = isSearchOpen();
  m_searchBar->show();
  m_searchButton->setChecked(true);
  m_searchField->setFocus(Qt::ShortcutFocusReason);
  m_searchField->selectAll();
  if (!wasOpen) {
    qDebug().noquote() << QStringLiteral("Timeline search opened");
  }
  if (m_searching) {
    showSearchList();
  }
  QTimer::singleShot(0, this, &TimelineWidget::logLayout);
}

void TimelineWidget::closeSearch() {
  if (!isSearchOpen()) {
    return;
  }
  const bool focused = m_searchField->hasFocus();
  m_searchField->clear(); // ends the search (searchTextChanged)
  m_externalSearch = false;
  hideSearchList();
  m_searchBar->hide();
  m_searchButton->setChecked(false);
  if (focused) {
    m_strip->setFocus(Qt::OtherFocusReason);
  }
  qDebug().noquote() << QStringLiteral("Timeline search closed");
  QTimer::singleShot(0, this, &TimelineWidget::logLayout);
}

void TimelineWidget::searchTextChanged() {
  const bool searching = !m_searchField->text().trimmed().isEmpty();
  if (searching && !m_searching) {
    // Where the timeline was and what was selected, for when it is cleared.
    m_searching = true;
    m_savedScroll = m_scroll->horizontalScrollBar()->value();
    m_savedSelection = m_strip->selectedRun();
    m_searchSelected.clear();
    m_match = -1;
    emit searchStarted();
  } else if (!searching && m_searching) {
    m_searching = false;
    endSearch();
  }
  updateMatches();
  if (m_searching && m_searchField->hasFocus()) {
    showSearchList();
  } else {
    hideSearchList();
  }
}

void TimelineWidget::endSearch() {
  // Back to the selection and the scroll position from before the search,
  // unless something other than its match was selected since.
  const bool restore = !m_searchSelected.isEmpty() && m_strip->selectedRun() == QStringList{m_searchSelected};
  m_searchSelected.clear();
  m_match = -1;
  if (!restore) {
    qDebug().noquote() << QStringLiteral("Timeline search cleared");
    return;
  }
  if (m_savedSelection.size() > 1) {
    m_strip->selectFeatures(m_savedSelection);
  } else {
    const QString uid = m_savedSelection.value(0);
    m_strip->select(m_strip->indexOf(uid) >= 0 ? uid : QString());
  }
  m_scroll->horizontalScrollBar()->setValue(m_savedScroll);
  QStringList names;
  for (const QString& uid : m_strip->selectedRun()) {
    names << m_strip->item(m_strip->indexOf(uid))->feature.name;
  }
  qDebug().noquote() << QStringLiteral("Timeline search cleared: selected %1 again, scrolled to %2")
                            .arg(names.isEmpty() ? QStringLiteral("nothing") : names.join(QStringLiteral(", ")))
                            .arg(m_scroll->horizontalScrollBar()->value());
  emit searchRestored();
}

void TimelineWidget::updateMatches() {
  if (!m_searching) {
    m_matches.clear();
    m_match = -1;
    m_strip->setFound({});
    m_loggedSearch.clear();
    updateSearchCount();
    fillSearchList();
    return;
  }
  const QString text = m_searchField->text().trimmed();
  const TimelineQuery query = TimelineQuery::parse(text);
  const QString current = m_match >= 0 && m_match < m_matches.size() ? m_matches[m_match] : QString();
  // Among the features the filter shows (mitcad#98), in timeline order.
  QStringList matches;
  QStringList listed;
  for (int i = 0; i < m_strip->count(); ++i) {
    if (m_strip->filtered(i)) {
      continue;
    }
    const DocumentSnapshot::Feature& feature = m_strip->item(i)->feature;
    TimelineSearchItem item;
    item.name = feature.name;
    item.type = feature.type;
    item.component = m_snapshot.componentName(feature.component);
    item.failed = feature.status == QStringLiteral("error");
    // As the warnings badge counts them (mitcad#170).
    item.warned = !item.failed && (feature.status == QStringLiteral("warning") || !feature.importNote.isEmpty());
    item.suppressed = feature.suppressed;
    item.rolledBack = i >= m_strip->marker();
    if (query.matches(item)) {
      matches << feature.uid;
      listed << QStringLiteral("%1 (%2)").arg(item.name, item.component);
    }
  }
  m_matches = matches;
  m_match = current.isEmpty() ? -1 : static_cast<int>(matches.indexOf(current));
  m_strip->setFound(QSet<QString>(matches.begin(), matches.end()));
  updateSearchCount();
  fillSearchList();
  constexpr int kLoggedMatches = 30;
  if (listed.size() > kLoggedMatches) {
    listed = listed.mid(0, kLoggedMatches) << QStringLiteral("...");
  }
  const QString found =
      !query.unknown.isEmpty()
          ? QStringLiteral("unknown filter %1").arg(query.unknown.join(QStringLiteral(", ")))
          : matches.isEmpty() ? QStringLiteral("no matches")
                              : QStringLiteral("%1 %2: %3")
                                    .arg(matches.size())
                                    .arg(matches.size() == 1 ? QStringLiteral("match") : QStringLiteral("matches"))
                                    .arg(listed.join(QStringLiteral(", ")));
  const QString logged = QStringLiteral("Timeline search \"%1\": %2").arg(text, found);
  if (logged != m_loggedSearch) {
    m_loggedSearch = logged;
    qDebug().noquote() << logged;
  }
}

void TimelineWidget::updateSearchCount() {
  QString count;
  QString tip;
  if (m_searching) {
    const TimelineQuery query = TimelineQuery::parse(m_searchField->text());
    if (!query.unknown.isEmpty()) {
      count = tr("Unknown: %1").arg(query.unknown.join(QStringLiteral(", ")));
      tip = tr("Statuses: is:failed, is:warning, is:suppressed, is:rolled-back");
    } else if (m_matches.isEmpty()) {
      count = tr("No matches");
    } else if (m_match >= 0) {
      count = tr("%1 of %2").arg(m_match + 1).arg(m_matches.size());
    } else {
      count = m_matches.size() == 1 ? tr("1 match") : tr("%1 matches").arg(m_matches.size());
    }
  }
  m_searchCount->setText(count);
  m_searchCount->setToolTip(tip);
  m_searchPrevious->setEnabled(!m_matches.isEmpty());
  m_searchNext->setEnabled(!m_matches.isEmpty());
  emit searchChanged();
}

void TimelineWidget::stepSearch(int delta) {
  if (m_matches.isEmpty()) {
    return;
  }
  const int count = static_cast<int>(m_matches.size());
  goToMatch(m_match < 0 ? (delta > 0 ? 0 : count - 1) : ((m_match + delta) % count + count) % count);
}

void TimelineWidget::goToMatch(int index) {
  if (index < 0 || index >= m_matches.size()) {
    return;
  }
  m_match = index;
  const QString uid = m_matches[index];
  // Scrolled to and selected, a folded group opened (which finds the
  // matches again, the same one stepped to).
  selectFeature(uid);
  m_searchSelected = uid;
  if (const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid)) {
    qDebug().noquote() << QStringLiteral("Timeline search match %1 of %2: %3 (%4)")
                              .arg(m_match + 1)
                              .arg(m_matches.size())
                              .arg(feature->name, m_snapshot.componentName(feature->component));
  }
  updateSearchCount();
  if (m_searchList != nullptr && m_match < m_searchList->topLevelItemCount()) {
    m_searchList->setCurrentItem(m_searchList->topLevelItem(m_match));
    m_searchList->scrollToItem(m_searchList->currentItem());
  }
  emit featuresClicked({uid}); // selected as a click selects it
}

void TimelineWidget::fillSearchList() {
  if (m_searchList == nullptr) {
    return; // made when first shown
  }
  m_searchList->clear();
  // The component colours of the strip's bands.
  QHash<QString, int> colours;
  for (int i = 1; i < m_snapshot.components.size(); ++i) {
    colours.insert(m_snapshot.components[i].uid, i - 1);
  }
  for (const QString& uid : std::as_const(m_matches)) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    if (feature == nullptr) {
      continue;
    }
    auto* row = new QTreeWidgetItem(m_searchList, {feature->name, m_snapshot.componentName(feature->component)});
    row->setIcon(0, themeIcon(featureIcon(feature->type)));
    const int colour = colours.value(feature->component, -1);
    row->setIcon(1, swatchIcon(colour >= 0 ? componentColor(colour) : QColor(Qt::transparent)));
    row->setData(0, Qt::UserRole, uid);
    QString state;
    if (feature->status == QStringLiteral("error") || feature->status == QStringLiteral("warning")) {
      state = feature->error;
    } else if (feature->suppressed) {
      state = feature->error.isEmpty() ? tr("Suppressed") : tr("Suppressed: %1").arg(feature->error);
    } else if (feature->index >= m_snapshot.marker) {
      state = tr("Rolled back");
    }
    row->setToolTip(0, state.isEmpty() ? feature->name : QStringLiteral("%1: %2").arg(feature->name, state));
    if (feature->status == QStringLiteral("error")) {
      row->setForeground(0, errorColor(palette()));
    }
  }
  if (m_match >= 0 && m_match < m_searchList->topLevelItemCount()) {
    m_searchList->setCurrentItem(m_searchList->topLevelItem(m_match));
  }
  if (m_matches.isEmpty()) {
    hideSearchList();
  } else if (m_searchList->isVisible()) {
    placeSearchList();
    QTimer::singleShot(0, this, &TimelineWidget::logSearchList);
  }
}

void TimelineWidget::showSearchList() {
  if (m_externalSearch) return;
  // Over the window (the main window, also from a floating card's own
  // window), so that it is not clipped to the timeline's dock or card.
  QWidget* host = GlassCard::hostWindow(this);
  if (m_searchList == nullptr) {
    auto* list = new QTreeWidget(host);
    list->setObjectName(QStringLiteral("timelineSearchResults"));
    list->setColumnCount(2);
    list->setHeaderHidden(true);
    list->setRootIsDecorated(false);
    list->setUniformRowHeights(true);
    list->setFocusPolicy(Qt::NoFocus); // the field keeps the keys
    list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    list->header()->setStretchLastSection(false);
    list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
    list->hide();
    connect(list, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* row) {
      goToMatch(m_searchList->indexOfTopLevelItem(row));
      // The keys back to the field (from a floating card's own window, the
      // click activated the main window).
      if (!m_searchField->hasFocus()) {
        m_searchField->activateWindow();
        m_searchField->setFocus(Qt::PopupFocusReason);
      }
    });
    connect(list->verticalScrollBar(), &QScrollBar::valueChanged, this,
            [this] { QTimer::singleShot(0, this, &TimelineWidget::logSearchList); });
    m_searchList = list;
    host->installEventFilter(this);
  } else if (m_searchList->parentWidget() != host) {
    m_searchList->setParent(host); // a floated dock went back, or the other way
    host->installEventFilter(this);
  }
  fillSearchList();
  if (m_matches.isEmpty()) {
    hideSearchList();
    return;
  }
  placeSearchList();
  m_searchList->show();
  m_searchList->raise();
  QTimer::singleShot(0, this, &TimelineWidget::logSearchList);
}

void TimelineWidget::hideSearchList() {
  if (m_searchList != nullptr && !m_searchList->isHidden()) {
    m_searchList->hide();
    m_loggedResults.clear();
    qDebug().noquote() << QStringLiteral("Timeline search results hidden");
  }
}

void TimelineWidget::placeSearchList() {
  if (m_searchList == nullptr || m_searchList->topLevelItemCount() == 0) {
    return;
  }
  // Up to eight rows above the field, below it when there is no room.
  QWidget* host = m_searchList->parentWidget();
  const int rows = std::min(m_searchList->topLevelItemCount(), 8);
  const int rowHeight = std::max(m_searchList->sizeHintForRow(0), fontMetrics().height() + 4);
  const int height = rows * rowHeight + 2 * m_searchList->frameWidth() + 2;
  const int width = std::max(m_searchField->width() + 100, 340);
  const QPoint field = GlassCard::mapToHost(m_searchField, QPoint(0, 0));
  QPoint at(field.x(), field.y() - height - 2);
  if (at.y() < 0) {
    at.setY(field.y() + m_searchField->height() + 2);
  }
  at.setX(std::clamp(at.x(), 0, std::max(0, host->width() - width)));
  m_searchList->setGeometry(QRect(at, QSize(width, height)));
}

void TimelineWidget::logSearchList() {
  if (m_searchList == nullptr || !m_searchList->isVisible()) {
    return;
  }
  QStringList lines;
  const QRect shown = m_searchList->viewport()->rect();
  for (int i = 0; i < m_searchList->topLevelItemCount(); ++i) {
    QTreeWidgetItem* row = m_searchList->topLevelItem(i);
    const QRect rect = m_searchList->visualItemRect(row);
    if (!rect.isValid() || !shown.contains(rect.center())) {
      continue;
    }
    const QPoint at = GlassCard::mapToHost(m_searchList->viewport(), rect.center());
    lines << QStringLiteral("Timeline search result %1 (%2) at %3,%4")
                 .arg(row->text(0), row->text(1))
                 .arg(at.x())
                 .arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedResults) {
    m_loggedResults = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

void TimelineWidget::showSearchMenu() {
  showSearchMenu(m_searchField->mapToGlobal(QPoint(0, m_searchField->height())));
}

void TimelineWidget::showSearchMenu(const QPoint& globalPosition) {
  QMenu menu(this);
  QStringList entries;
  const QString text = m_searchField->text();
  const auto add = [&](const QString& label, const QString& token) {
    QAction* action = menu.addAction(label);
    action->setCheckable(true);
    action->setChecked(hasTimelineToken(text, token));
    connect(action, &QAction::triggered, this, [this, token] {
      m_searchField->setText(toggleTimelineToken(m_searchField->text(), token));
      if (!m_externalSearch) m_searchField->setFocus(Qt::PopupFocusReason);
    });
    entries << label;
  };
  add(tr("Failed"), timelineStatusToken(TimelineQuery::Failed));
  add(tr("Warnings and Import Notes"), timelineStatusToken(TimelineQuery::Warning));
  add(tr("Suppressed"), timelineStatusToken(TimelineQuery::Suppressed));
  add(tr("Rolled Back"), timelineStatusToken(TimelineQuery::RolledBack));
  menu.addSeparator();
  for (const DocumentSnapshot::Component& component : std::as_const(m_snapshot.components)) {
    add(component.name, timelineComponentToken(component.name));
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, globalPosition);
}

bool TimelineWidget::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::Wheel && m_scroll != nullptr &&
      (watched == m_scroll->viewport() || watched == m_scroll->horizontalScrollBar())) {
    wheelStrip(static_cast<QWheelEvent*>(event));
    event->accept();
    return true;
  }
  if (watched == m_searchField) {
    switch (event->type()) {
    case QEvent::ShortcutOverride: {
      // The field's own keys, not the window's OK and Cancel (Enter, Esc).
      const int key = static_cast<QKeyEvent*>(event)->key();
      if (key == Qt::Key_Return || key == Qt::Key_Enter || key == Qt::Key_Escape || key == Qt::Key_Up ||
          key == Qt::Key_Down) {
        event->accept();
        return true;
      }
      break;
    }
    case QEvent::KeyPress: {
      const auto* key = static_cast<QKeyEvent*>(event);
      switch (key->key()) {
      case Qt::Key_Return:
      case Qt::Key_Enter:
        stepSearch(key->modifiers().testFlag(Qt::ShiftModifier) ? -1 : 1);
        return true;
      case Qt::Key_Down:
        stepSearch(1);
        return true;
      case Qt::Key_Up:
        stepSearch(-1);
        return true;
      case Qt::Key_Escape:
        closeSearch();
        return true;
      default:
        break;
      }
      break;
    }
    case QEvent::FocusIn:
      if (m_searching) {
        showSearchList();
      }
      break;
    case QEvent::FocusOut: {
      // Not for the field's menu of filters, nor for a click on the list
      // that activated its window (a floating card's field is in a window
      // of its own).
      const Qt::FocusReason reason = static_cast<QFocusEvent*>(event)->reason();
      const bool onList = reason == Qt::ActiveWindowFocusReason && m_searchList != nullptr &&
                          m_searchList->isVisible() && m_searchList->underMouse();
      if (reason != Qt::PopupFocusReason && !onList) {
        hideSearchList();
      }
      break;
    }
    default:
      break;
    }
  } else if (m_searchList != nullptr && watched == m_searchList->parentWidget() &&
             event->type() == QEvent::Resize && m_searchList->isVisible()) {
    placeSearchList();
  }
  return QWidget::eventFilter(watched, event);
}

void TimelineWidget::wheelStrip(QWheelEvent* event) {
  QScrollBar* bar = m_scroll->horizontalScrollBar();
  // The larger of the two deltas, so that a diagonal swipe does not jitter.
  // Down (negative) and a tilt or swipe to the right (negative) both move
  // towards the newer features, to the right.
  const auto larger = [](const QPoint& delta) {
    return std::abs(delta.y()) > std::abs(delta.x()) ? delta.y() : delta.x();
  };
  double pixels = 0;
  if (event->phase() != Qt::NoScrollPhase && !event->pixelDelta().isNull()) {
    // A trackpad's stream of scrolling: its own pixels. (A wheel's pixel
    // delta, where one comes, depends on the driver.)
    pixels = larger(event->pixelDelta());
  } else {
    // A wheel: the bar's single step for each of the wheel's lines a notch
    // (120 eighths of a degree).
    pixels = larger(event->angleDelta()) / 120.0 * QApplication::wheelScrollLines() * bar->singleStep();
  }
  if (pixels * m_wheelRemainder < 0) {
    m_wheelRemainder = 0; // turned the other way
  }
  m_wheelRemainder += pixels;
  // Whole pixels only, the rest kept for the next event; no more than a page.
  const int step = std::clamp(static_cast<int>(m_wheelRemainder), -bar->pageStep(), bar->pageStep());
  m_wheelRemainder -= static_cast<int>(m_wheelRemainder);
  if (step == 0) {
    return;
  }
  const int before = bar->value();
  bar->setValue(before - step); // the bar stops at both ends
  if (bar->value() == before) {
    m_wheelRemainder = 0;
  }
  qDebug().noquote()
      << QStringLiteral("Timeline wheel: scrolled to %1 of %2").arg(bar->value()).arg(bar->maximum());
}

void TimelineWidget::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  if (m_searchList != nullptr && m_searchList->isVisible()) {
    placeSearchList();
  }
}

} // namespace mitcad
