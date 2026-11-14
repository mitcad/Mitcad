// SPDX-License-Identifier: MIT
#include "SketchItemList.hpp"

#include <QApplication>
#include <QDebug>
#include <QEvent>
#include <QFont>
#include <QFontMetrics>
#include <QFrame>
#include <QKeyEvent>
#include <QLabel>
#include <QListWidgetItem>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QResizeEvent>
#include <QScrollBar>
#include <QSet>
#include <QStringList>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QStyleOptionViewItem>
#include <QVBoxLayout>
#include <QWindow>
#include <QtLogging>

#include <algorithm>
#include <cstddef>
#include <functional>
#include <utility>
#include <vector>

#include "../browser/FeatureStatus.hpp"
#include "../browser/TimelinePresentation.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Selection.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "../OcctViewer.hpp"
#include "SketchController.hpp"
#include "SketchOverlay.hpp"
#include "SketchTool.hpp"

namespace mitcad::sketch {

namespace {

constexpr int kKeyRole = Qt::UserRole;
constexpr int kItemsRole = Qt::UserRole + 1;
// A cell whose icon is drawn in its disabled colours (a step of inactive
// items, mitcad#1763).
constexpr int kDimmedRole = Qt::UserRole + 2;
// The hover list's rows: no more in sight at once, the rest scroll.
constexpr int kMostRows = 12;
// How long the hover list waits for the pointer to come over it after
// leaving its cell.
constexpr int kHideMs = 400;

// Where a cell's icon is: in the middle of its cell, both ways
// (mitcad#1760). The delegate paints it there and logPlaces logs it.
QRect iconRect(const QRect& cell, const QSize& icon) {
  return {cell.left() + (cell.width() - icon.width()) / 2, cell.top() + (cell.height() - icon.height()) / 2,
          icon.width(), icon.height()};
}

// A selected cell as the timeline draws one (TimelineWidget's stripColors):
// a tint of the accent colour and its ring, which the icon in its own
// colours stays readable on (mitcad#1763; the style's full accent fill and
// its selected icon mode left the icon nearly invisible).
timeline::CellLook selectedLook(const QPalette& palette) {
  const QColor accent = accentColor(palette);
  QColor fill = accent;
  if (chromeStyle() == ChromeStyle::Floating) {
    fill.setAlphaF(0.24f);
  } else {
    fill.setAlphaF(isDarkPalette(palette) ? 0.30f : 0.18f);
  }
  timeline::CellLook look;
  look.fill = fill;
  look.outerRing = accent;
  return look;
}

// How CellDelegate draws a cell, in the words logged for it: "selected",
// "dimmed", both or "plain", then its icon's colours ("icon normal", or
// "icon disabled" for a dimmed cell or a disabled one).
QString cellLook(bool selected, bool dimmed, bool enabled) {
  QStringList look;
  if (selected) {
    look << QStringLiteral("selected");
  }
  if (dimmed) {
    look << QStringLiteral("dimmed");
  }
  if (look.isEmpty()) {
    look << QStringLiteral("plain");
  }
  look << ((dimmed || !enabled) ? QStringLiteral("icon disabled") : QStringLiteral("icon normal"));
  return look.join(QStringLiteral(", "));
}

// Paints a cell: the style's background (hover, focus) over the whole
// cell, the selection as the timeline's, and the icon in its middle in its
// own colours (a dimmed cell's icon was made in its disabled colours,
// badgedIcon). The list's default left-aligned decoration put it off the
// cell's middle. `painted` hears how each cell was drawn.
class CellDelegate : public QStyledItemDelegate {
public:
  using Painted = std::function<void(const QString& key, const QString& look)>;

  CellDelegate(QObject* parent, Painted painted) : QStyledItemDelegate(parent), m_painted(std::move(painted)) {}

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem cell(option);
    initStyleOption(&cell, index);
    const QIcon icon = cell.icon;
    const QSize size = cell.decorationSize;
    const bool selected = cell.state.testFlag(QStyle::State_Selected);
    const bool dimmed = index.data(kDimmedRole).toBool();
    cell.icon = QIcon();
    cell.text.clear();
    cell.features &= ~QStyleOptionViewItem::HasDecoration;
    cell.state &= ~QStyle::State_Selected;
    QStyle* style = cell.widget != nullptr ? cell.widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &cell, painter, cell.widget);
    if (selected) {
      timeline::paintCellFrame(*painter, QRectF(option.rect), selectedLook(cell.palette));
    }
    // Never the selected mode: the icon keeps its colours on the tint.
    const QIcon::Mode mode = cell.state.testFlag(QStyle::State_Enabled) ? QIcon::Normal : QIcon::Disabled;
    icon.paint(painter, iconRect(option.rect, size), Qt::AlignCenter, mode, QIcon::Off);
    if (m_painted) {
      m_painted(index.data(kKeyRole).toString(), cellLook(selected, dimmed, mode == QIcon::Normal));
    }
  }

private:
  Painted m_painted;
};

// The hover list's rows: a selected row's icon in its own colours, not the
// style's selected mode, which tints it into the selection's fill
// (mitcad#1763).
class RowDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    if (option->state.testFlag(QStyle::State_Selected) && !option->icon.isNull()) {
      const qreal ratio = option->widget != nullptr ? option->widget->devicePixelRatioF() : qApp->devicePixelRatio();
      QIcon normal;
      normal.addPixmap(option->icon.pixmap(option->decorationSize, ratio, QIcon::Normal), QIcon::Selected);
      option->icon = normal;
    }
  }
};

// The worse of two levels: an error over a warning over information.
int rank(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return 3;
  case NoticeLevel::Warning:
    return 2;
  case NoticeLevel::Information:
    return 1;
  default:
    return 0;
  }
}

// A curve's kind in words and its icon: the drawing tool's.
QString curveKind(const CurveData& curve) {
  if (curve.isLine()) {
    return curve.centerline ? SketchItemList::tr("Centerline") : SketchItemList::tr("Line");
  }
  if (curve.isCircle()) {
    return SketchItemList::tr("Circle");
  }
  if (curve.isArc()) {
    return SketchItemList::tr("Arc");
  }
  if (curve.type == QStringLiteral("ellipse")) {
    return SketchItemList::tr("Ellipse");
  }
  if (curve.type == QStringLiteral("elliptical_arc")) {
    return SketchItemList::tr("Elliptical Arc");
  }
  if (curve.type == QStringLiteral("fitted_spline")) {
    return SketchItemList::tr("Fit Point Spline");
  }
  if (curve.type == QStringLiteral("spline")) {
    return SketchItemList::tr("Control Point Spline");
  }
  return SketchItemList::tr("Curve");
}

QString curveIcon(const CurveData& curve) {
  if (curve.isLine()) {
    return curve.centerline ? QStringLiteral("centerline") : QStringLiteral("line");
  }
  if (curve.isCircle()) {
    return QStringLiteral("circle");
  }
  if (curve.isArc()) {
    return QStringLiteral("arc-3pt");
  }
  if (curve.isEllipse()) {
    return QStringLiteral("ellipse");
  }
  if (curve.type == QStringLiteral("spline")) {
    return QStringLiteral("spline-control");
  }
  return QStringLiteral("spline-fit");
}

// A constraint's icon: its constraint command's.
QString constraintIcon(const QString& type) {
  if (type == QStringLiteral("horizontal") || type == QStringLiteral("vertical") ||
      type == QStringLiteral("horizontal_points") || type == QStringLiteral("vertical_points")) {
    return QStringLiteral("horizontal-vertical");
  }
  if (type == QStringLiteral("symmetric")) {
    return QStringLiteral("symmetry");
  }
  if (type == QStringLiteral("smooth")) {
    return QStringLiteral("curvature");
  }
  if (type == QStringLiteral("equal_spacing")) {
    return QStringLiteral("equal-spacing");
  }
  static const QSet<QString> own = {QStringLiteral("coincident"),    QStringLiteral("parallel"),
                                    QStringLiteral("perpendicular"), QStringLiteral("tangent"),
                                    QStringLiteral("equal"),         QStringLiteral("concentric"),
                                    QStringLiteral("collinear"),     QStringLiteral("midpoint")};
  return own.contains(type) ? type : QStringLiteral("constraint");
}

// The words after a curve's or a point's kind.
QString flagsText(bool construction, bool reference) {
  QStringList flags;
  if (reference) {
    flags << SketchItemList::tr("projected");
  }
  if (construction) {
    flags << SketchItemList::tr("construction");
  }
  return flags.join(QStringLiteral(", "));
}

} // namespace

SketchItemList::SketchItemList(SketchController& controller, QWidget* parent)
    : QListWidget(parent), m_c(controller) {
  setObjectName(QStringLiteral("sketchItemList"));
  setAccessibleName(tr("Sketch steps"));
  // Cells left to right in the order they were made, onto the next line
  // when the panel is full, scrolling down for more.
  setViewMode(QListView::ListMode);
  setFlow(QListView::LeftToRight);
  setWrapping(true);
  setResizeMode(QListView::Adjust);
  setUniformItemSizes(true);
  setIconSize(uiIconSize(timeline::icon, timeline::icon));
  setGridSize(uiIconSize(timeline::cell, timeline::cell));
  setItemDelegate(new CellDelegate(this, [this](const QString& key, const QString& look) { logLook(key, look); }));
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  // The view's selection is the one selection: the cells only show it, so
  // a click or a key never changes them by itself.
  setSelectionMode(QAbstractItemView::ExtendedSelection);
  setEditTriggers(QAbstractItemView::NoEditTriggers);
  setDragEnabled(false);
  setMouseTracking(true);
  viewport()->setMouseTracking(true);
  setFocusPolicy(Qt::ClickFocus);
  setFrameShape(QFrame::NoFrame);
  // The panel's own background shows through, as under the timeline.
  viewport()->setAutoFillBackground(false);
  m_hideTimer.setSingleShot(true);
  m_hideTimer.setInterval(kHideMs);
  connect(&m_hideTimer, &QTimer::timeout, this, [this] {
    if (!m_inCell && !m_inHoverList) {
      hideHoverList();
    }
  });
  TestSync::watch(&m_hideTimer);
  connect(&m_c, &SketchController::changed, this, &SketchItemList::rebuild);
  connect(&m_c, &SketchController::viewRefreshed, this, &SketchItemList::showSelection);
  connect(verticalScrollBar(), &QScrollBar::valueChanged, this, [this] {
    hideHoverList();
    schedulePlaces();
  });
  // Its places are logged again when it moves in the window.
  new PlaceWatcher(this, [this] { logPlaces(); });
}

SketchItemList::~SketchItemList() {
  // The hover list lives in the window, which outlives the list.
  if (m_hover) {
    m_hover->deleteLater();
  }
}

// One row of cells high (mitcad#1760): the list has no marker, so none of
// the timeline strip's band above its cells. The list shares the
// timeline's place in a stack; MainWindow::updateTimelinePlace lets only the
// page in sight size it, so the place is this high in sketch mode and the
// timeline's height outside it.
QSize SketchItemList::sizeHint() const {
  const int cell = gridSize().height();
  return {gridSize().width() * 16, cell};
}

QSize SketchItemList::minimumSizeHint() const {
  const int cell = gridSize().height();
  return {gridSize().width() * 4, cell};
}

void SketchItemList::resizeEvent(QResizeEvent* event) {
  // The rows that fit in the middle of the list's height, the rest of it
  // even above and below them: a place taller than a row (a docked panel
  // made taller) keeps the icons in the middle of the bar, as a cell keeps
  // its icon in its middle.
  const int cell = std::max(1, gridSize().height());
  const int rows = std::max(1, height() / cell);
  const int spare = std::max(0, height() - rows * cell);
  setViewportMargins(0, spare / 2, 0, spare - spare / 2);
  QListWidget::resizeEvent(event);
  schedulePlaces();
}

NoticeLevel SketchItemList::markOf(const QString& id, QString& why) const {
  const SketchModel& model = m_c.model();
  if (m_shownConflicts.contains(id)) {
    why = tr("Conflicts with other constraints or dimensions");
    return NoticeLevel::Error;
  }
  if (model.redundant.contains(id)) {
    why = tr("Follows from the other constraints and dimensions");
    return NoticeLevel::Warning;
  }
  QStringList warnings;
  for (std::size_t i = 0; i < model.warningEntities.size(); ++i) {
    if (model.warningEntities[i].contains(id) && static_cast<qsizetype>(i) < model.warnings.size()) {
      warnings << model.warnings.at(static_cast<qsizetype>(i));
    }
  }
  if (!warnings.isEmpty()) {
    why = warnings.join(QLatin1Char('\n'));
    return NoticeLevel::Warning;
  }
  if (model.unmetInactive.contains(id)) {
    why = tr("Not met by the geometry: activating it moves it");
    return NoticeLevel::Information;
  }
  return NoticeLevel::Hint;
}

QIcon SketchItemList::badgedIcon(const QString& name, NoticeLevel level, int count, const QSize& size,
                                 bool dimmed) const {
  const QIcon icon = themeIcon(name);
  const bool badge = isProblem(level) || level == NoticeLevel::Information;
  if (!badge && count < 2 && !dimmed) {
    return icon;
  }
  const qreal ratio = devicePixelRatioF();
  QPixmap pixmap(QSize(qRound(size.width() * ratio), qRound(size.height() * ratio)));
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  // Dimmed: the disabled colours at their opacity (themeIcon), under a
  // badge and a count in full colour. A colour of their own (setForeground)
  // does nothing for a cell that shows its icon only (mitcad#1763).
  icon.paint(&painter, QRect(QPoint(0, 0), size), Qt::AlignCenter, dimmed ? QIcon::Disabled : QIcon::Normal,
             QIcon::Off);
  if (badge) {
    const qreal side = size.width() * 0.5;
    paintNoticeBadge(painter, QRectF(size.width() - side, 0.0, side, side), level, palette().color(QPalette::Base));
  }
  if (count > 1) {
    // The count over the bottom right corner, in a pill of the selection's
    // colour.
    const QString text = count > 99 ? QStringLiteral("99+") : QString::number(count);
    QFont small = font();
    small.setBold(true);
    small.setPixelSize(std::max(7, qRound(size.height() * 0.42)));
    painter.setFont(small);
    const QFontMetrics metrics(small);
    const qreal height = metrics.height();
    const qreal width = std::max(height, static_cast<qreal>(metrics.horizontalAdvance(text)) + 4.0);
    const QRectF pill(size.width() - width, size.height() - height, width, height);
    painter.setPen(Qt::NoPen);
    painter.setBrush(palette().color(QPalette::Highlight));
    painter.drawRoundedRect(pill, height / 2.0, height / 2.0);
    painter.setPen(palette().color(QPalette::HighlightedText));
    painter.drawText(pill, Qt::AlignCenter, text);
  }
  painter.end();
  return QIcon(pixmap);
}

SketchItemList::ItemRow SketchItemList::pointRow(const PointData& point) const {
  ItemRow row;
  row.id = point.id;
  row.icon = QStringLiteral("point");
  const bool origin = point.id == m_c.model().originPoint();
  row.text = origin ? tr("Origin") : tr("Point");
  const QString flags = flagsText(point.construction, point.reference);
  if (!flags.isEmpty()) {
    row.text = tr("%1 (%2)").arg(row.text, flags);
  }
  row.logged = QStringLiteral("%1 %2").arg(point.id, origin ? QStringLiteral("origin") : QStringLiteral("point"));
  if (point.construction) {
    row.logged += QStringLiteral(" construction");
  }
  return row;
}

SketchItemList::ItemRow SketchItemList::curveRow(const CurveData& curve) const {
  ItemRow row;
  row.id = curve.id;
  row.icon = curveIcon(curve);
  if (curve.axis == QStringLiteral("x")) {
    row.text = tr("X axis");
  } else if (curve.axis == QStringLiteral("y")) {
    row.text = tr("Y axis");
  } else {
    row.text = curveKind(curve);
    const QString flags = flagsText(curve.construction, curve.reference);
    if (!flags.isEmpty()) {
      row.text = tr("%1 (%2)").arg(row.text, flags);
    }
  }
  row.logged = QStringLiteral("%1 %2").arg(curve.id, curve.type);
  if (curve.construction) {
    row.logged += QStringLiteral(" construction");
  }
  if (curve.centerline) {
    row.logged += QStringLiteral(" centerline");
  }
  return row;
}

SketchItemList::ItemRow SketchItemList::constraintRow(const ConstraintData& constraint) const {
  ItemRow row;
  row.id = constraint.id;
  row.icon = constraintIcon(constraint.type);
  row.text = constraintName(constraint.type);
  const QString inactive = SketchOverlay::inactiveTooltip(constraint.active, constraint.inactiveReason);
  if (!inactive.isEmpty()) {
    row.why = inactive;
  }
  row.dimmed = !constraint.active;
  row.logged = QStringLiteral("%1 %2").arg(constraint.id, constraint.type);
  if (!constraint.active) {
    row.logged += QStringLiteral(" inactive");
  }
  return row;
}

SketchItemList::ItemRow SketchItemList::dimensionRow(const DimensionData& dimension) const {
  ItemRow row;
  row.id = dimension.id;
  row.icon = QStringLiteral("dimension");
  // The value as the view shows it: R, Ø, fx:, the name, parentheses when
  // driven.
  const QString value = m_c.overlay().dimensionText(dimension);
  row.text = tr("Dimension %1").arg(value);
  const QString inactive = SketchOverlay::inactiveTooltip(dimension.active, dimension.inactiveReason);
  if (!inactive.isEmpty()) {
    row.why = inactive;
  }
  row.dimmed = !dimension.active;
  row.logged = QStringLiteral("%1 %2 %3").arg(dimension.id, dimension.type, value);
  if (!dimension.active) {
    row.logged += QStringLiteral(" inactive");
  }
  return row;
}

bool SketchItemList::itemRow(const QString& id, ItemRow& row) const {
  const SketchModel& model = m_c.model();
  if (const PointData* point = model.point(id)) {
    row = pointRow(*point);
  } else if (const CurveData* curve = model.curve(id)) {
    row = curveRow(*curve);
  } else if (const ConstraintData* constraint = model.constraint(id)) {
    row = constraintRow(*constraint);
  } else if (const DimensionData* dimension = model.dimension(id)) {
    row = dimensionRow(*dimension);
  } else if (model.text(id) != nullptr) {
    // A text (mitcad#1750): what it is, not what it says.
    row = ItemRow();
    row.id = id;
    row.icon = QStringLiteral("text");
    row.text = tr("Text");
    row.logged = QStringLiteral("%1 text").arg(id);
  } else {
    return false;
  }
  // The inactive one's words stay unless a notice says more.
  QString why;
  row.level = markOf(id, why);
  if (row.level != NoticeLevel::Hint) {
    row.why = row.why.isEmpty() ? noticeLevelText(row.level, why)
                                : row.why + QLatin1Char('\n') + noticeLevelText(row.level, why);
    row.logged += QStringLiteral(" [%1]").arg(noticeLevelName(row.level));
  }
  return true;
}

void SketchItemList::describeStep(const QString& kind, Cell& cell) const {
  const SketchModel& model = m_c.model();
  // The first curve and constraint the step made, for the kinds whose
  // name or icon depends on them.
  const CurveData* firstCurve = nullptr;
  const ConstraintData* firstConstraint = nullptr;
  for (const ItemRow& row : cell.items) {
    if (firstCurve == nullptr) {
      firstCurve = model.curve(row.id);
    }
    if (firstConstraint == nullptr) {
      firstConstraint = model.constraint(row.id);
    }
  }
  struct Tool {
    const char* command;
    const char* iconName;
    QString title;
  };
  const std::vector<Tool> tools = {
      {"add_line", "line", tr("Line")},
      {"rectangle", "rectangle", tr("Rectangle")},
      {"add_rectangle", "rectangle", tr("Rectangle")},
      {"circle", "circle", tr("Circle")},
      {"add_circle", "circle", tr("Circle")},
      {"arc", "arc-3pt", tr("Arc")},
      {"polygon", "polygon", tr("Polygon")},
      {"slot", "slot", tr("Slot")},
      {"ellipse", "ellipse", tr("Ellipse")},
      {"spline", "spline-fit", tr("Spline")},
      {"add_point", "point", tr("Point")},
      {"add_text", "text", tr("Text")},
      {"add_code", "text", tr("QR Code")},
      {"fillet", "sketch-fillet", tr("Sketch Fillet")},
      {"chamfer", "sketch-chamfer", tr("Sketch Chamfer")},
      {"trim", "trim", tr("Trim")},
      {"extend", "extend", tr("Extend")},
      {"offset", "offset", tr("Offset")},
      {"edit_offset", "offset", tr("Offset")},
      {"mirror", "mirror", tr("Mirror")},
      {"circular_pattern", "circular-pattern", tr("Circular Pattern")},
      {"rectangular_pattern", "rectangular-pattern", tr("Rectangular Pattern")},
      {"edit_pattern", "circular-pattern", tr("Pattern")},
      {"project", "project", tr("Project")},
      {"project_again", "project", tr("Project Again")},
      {"intersect", "sketch-intersect", tr("Intersect")},
      {"convert_projected", "convert-projected", tr("Convert to Sketch Geometry")},
      {"add_dimension", "dimension", tr("Sketch Dimension")},
      {"add_equal_spacing", "equal-spacing", tr("Equal Spacing")},
      {"edit_equal_spacing", "equal-spacing", tr("Equal Spacing")},
      {"paste", "paste", tr("Paste")},
      {"import_svg", "import", tr("Insert SVG")},
      {"import_dxf", "insert-dxf", tr("Insert DXF")},
      {"move", "move", tr("Move/Copy")},
      {"scale", "scale", tr("Scale")},
      {"axis", "line", tr("Sketch Axis")},
      {"insert_spline_point", "spline-fit", tr("Spline Point")},
  };
  cell.name = tr("Sketch Step");
  cell.icon = QStringLiteral("sketch");
  for (const Tool& tool : tools) {
    if (kind == QLatin1String(tool.command)) {
      cell.name = tool.title;
      cell.icon = QString::fromLatin1(tool.iconName);
      break;
    }
  }
  if (kind == QStringLiteral("add_constraint") && firstConstraint != nullptr) {
    // A constraint: its own name and icon.
    cell.name = constraintName(firstConstraint->type);
    cell.icon = constraintIcon(firstConstraint->type);
  } else if (kind == QStringLiteral("add_line") && firstCurve != nullptr && firstCurve->centerline) {
    cell.name = tr("Centerline");
    cell.icon = QStringLiteral("centerline");
  } else if (kind == QStringLiteral("spline") && firstCurve != nullptr) {
    cell.name = curveKind(*firstCurve);
    cell.icon = curveIcon(*firstCurve);
  }
}

std::vector<SketchItemList::Cell> SketchItemList::cells() const {
  const SketchModel& model = m_c.model();
  std::vector<SketchModel::StepData> steps = model.steps;
  if (steps.empty()) {
    // A model without steps: each item alone, in the order they were made.
    QStringList ids = model.order;
    if (ids.isEmpty()) {
      for (const PointData& point : model.points) {
        ids << point.id;
      }
      for (const CurveData& curve : model.curves) {
        ids << curve.id;
      }
      for (const ConstraintData& constraint : model.constraints) {
        ids << constraint.id;
      }
      for (const DimensionData& dimension : model.dimensions) {
        ids << dimension.id;
      }
    }
    for (const QString& id : std::as_const(ids)) {
      SketchModel::StepData alone;
      alone.items << id;
      steps.push_back(alone);
    }
  }
  std::vector<Cell> out;
  for (const SketchModel::StepData& step : steps) {
    Cell cell;
    for (const QString& id : step.items) {
      ItemRow row;
      if (itemRow(id, row)) {
        // A curve a trim or an extension changed, which stays with the
        // step that made it.
        if (step.changed.contains(id)) {
          row.text = tr("Changed: %1").arg(row.text);
          row.logged += QStringLiteral(" changed");
        }
        cell.items.push_back(row);
      }
    }
    if (cell.items.empty()) {
      continue;
    }
    cell.dimmed = true;
    for (const ItemRow& row : cell.items) {
      if (rank(row.level) > rank(cell.level)) {
        cell.level = row.level;
        cell.why = row.why;
      }
      cell.dimmed = cell.dimmed && row.dimmed;
    }
    if (step.id.isEmpty()) {
      // An item alone: its own name and icon.
      const ItemRow& row = cell.items.front();
      cell.key = row.id;
      cell.name = row.text;
      cell.icon = row.icon;
      cell.logged = row.logged;
    } else {
      cell.key = step.id;
      cell.kind = step.kind;
      describeStep(step.kind, cell);
      cell.logged = QStringLiteral("%1 %2 x%3").arg(step.id, step.kind).arg(cell.items.size());
      if (cell.level != NoticeLevel::Hint) {
        cell.logged += QStringLiteral(" [%1]").arg(noticeLevelName(cell.level));
      }
    }
    out.push_back(std::move(cell));
  }
  return out;
}

const SketchItemList::Cell* SketchItemList::cellOf(const QString& key) const {
  for (const Cell& cell : m_cells) {
    if (cell.key == key) {
      return &cell;
    }
  }
  return nullptr;
}

void SketchItemList::rebuild() {
  if (!m_c.isActive()) {
    hideHoverList();
    clear();
    m_built = false;
    m_cells.clear();
    m_shown.clear();
    m_shownConflicts.clear();
    m_loggedSteps.clear();
    m_loggedPlaces.clear();
    m_loggedSelection.clear();
    m_loggedLooks.clear();
    return;
  }
  m_shownConflicts = m_c.conflicts();
  std::vector<Cell> fresh = cells();
  // What each cell shows, its hover list with it.
  QStringList shown;
  for (const Cell& cell : fresh) {
    QStringList line{cell.logged, cell.icon, cell.name, cell.why, cell.dimmed ? QStringLiteral("dimmed") : QString()};
    for (const ItemRow& row : cell.items) {
      line << row.logged << row.text << row.why;
    }
    shown << line.join(QLatin1Char('\t'));
  }
  // A reload that changed none of them (a drag's step, a selection) keeps
  // the cells as they are.
  if (m_built && shown == m_shown) {
    showSelection();
    return;
  }
  m_built = true;
  m_shown = shown;
  m_cells = std::move(fresh);
  const int scrolled = verticalScrollBar()->value();
  m_marking = true;
  clear();
  QStringList logged;
  // The new cells' looks are logged as they are painted again.
  m_loggedLooks.clear();
  for (const Cell& cell : m_cells) {
    const int made = static_cast<int>(cell.items.size());
    auto* entry =
        new QListWidgetItem(badgedIcon(cell.icon, cell.level, made, iconSize(), cell.dimmed), QString(), this);
    entry->setData(kKeyRole, cell.key);
    entry->setData(kDimmedRole, cell.dimmed);
    QStringList ids;
    for (const ItemRow& row : cell.items) {
      ids << row.id;
    }
    entry->setData(kItemsRole, ids);
    // The name in the hover list's title (its tooltip) and for screen
    // readers; never as text in the cell.
    entry->setData(Qt::AccessibleTextRole, cell.name);
    entry->setSizeHint(gridSize());
    logged << cell.logged;
  }
  m_marking = false;
  verticalScrollBar()->setValue(scrolled);
  const QString line = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", "));
  if (line != m_loggedSteps) {
    m_loggedSteps = line;
    qDebug().noquote() << QStringLiteral("Sketch steps: %1").arg(line);
  }
  // The hover list follows its step, or goes with it.
  if (m_hover && m_hover->isVisible()) {
    if (const Cell* cell = cellOf(m_hoverKey)) {
      fillHoverList(*cell);
      placeHoverList(m_hoverCell);
    } else {
      hideHoverList();
    }
  }
  m_loggedSelection.clear();
  showSelection();
  schedulePlaces();
}

void SketchItemList::showSelection() {
  if (!m_c.isActive()) {
    return;
  }
  // A refused edit's blocking items (setConflict) come without a reload:
  // their badges with them.
  if (m_c.conflicts() != m_shownConflicts) {
    rebuild();
    return;
  }
  QSet<QString> selected;
  for (const SelectionItem& picked : m_c.host().selection()) {
    if (picked.owner == m_c.uid()) {
      selected.insert(picked.name);
    }
  }
  m_marking = true;
  QStringList marked;
  QListWidgetItem* first = nullptr;
  for (int i = 0; i < count(); ++i) {
    QListWidgetItem* cell = item(i);
    const QStringList ids = cell->data(kItemsRole).toStringList();
    const bool on = std::any_of(ids.cbegin(), ids.cend(), [&selected](const QString& id) { return selected.contains(id); });
    cell->setSelected(on);
    if (on) {
      marked << cell->data(kKeyRole).toString();
      if (first == nullptr) {
        first = cell;
      }
    }
  }
  if (m_hover && m_hoverRows != nullptr) {
    for (int i = 0; i < m_hoverRows->count(); ++i) {
      QListWidgetItem* row = m_hoverRows->item(i);
      row->setSelected(selected.contains(row->data(kKeyRole).toString()));
    }
  }
  m_marking = false;
  const QString line = marked.isEmpty() ? QStringLiteral("none") : marked.join(QStringLiteral(", "));
  if (line != m_loggedSelection) {
    m_loggedSelection = line;
    qDebug().noquote() << QStringLiteral("Sketch steps selected: %1").arg(line);
    // What the view selected comes into sight here too.
    if (first != nullptr) {
      scrollToItem(first, QAbstractItemView::EnsureVisible);
    }
  }
  logUnpaintedLooks();
}

void SketchItemList::schedulePlaces() {
  // Once the list has laid out its cells (it does so on the next turn).
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void SketchItemList::logPlaces() {
  if (!isVisible() || !m_c.isActive()) {
    return;
  }
  QStringList lines;
  // The bar it is in (mitcad#1760): the timeline's card (Floating) or
  // panel (Docked), whose place it takes.
  const QWidget* bar = this;
  while (bar->parentWidget() != nullptr && bar->objectName() != QLatin1String("timelineDock")) {
    bar = bar->parentWidget();
  }
  const auto rectText = [](const QWidget* part, const QRect& area) {
    const QPoint at = GlassCard::mapToHost(part, area.topLeft());
    return QStringLiteral("%1,%2 %3x%4").arg(at.x()).arg(at.y()).arg(area.width()).arg(area.height());
  };
  lines << QStringLiteral("Sketch steps bar at %1").arg(rectText(bar, bar->rect()));
  const QRect inSight = viewport()->rect();
  for (int i = 0; i < count(); ++i) {
    const QRect cell = visualItemRect(item(i));
    if (!inSight.contains(cell.center())) {
      continue;
    }
    const QString key = item(i)->data(kKeyRole).toString();
    const QPoint at = GlassCard::mapToHost(viewport(), cell.center());
    lines << QStringLiteral("Sketch step %1 at %2,%3").arg(key).arg(at.x()).arg(at.y());
    lines << QStringLiteral("Sketch step %1 icon at %2").arg(key, rectText(viewport(), iconRect(cell, iconSize())));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
  // Shown now: the cells' looks too where it is never painted.
  logUnpaintedLooks();
}

void SketchItemList::logUnpaintedLooks() {
  // An exposed window paints its cells, and the delegate logs each as it
  // draws it.
  const QWindow* handle = window()->windowHandle();
  if (!isVisible() || !m_c.isActive() || (handle != nullptr && handle->isExposed())) {
    return;
  }
  for (int i = 0; i < count(); ++i) {
    const QListWidgetItem* cell = item(i);
    const bool enabled = isEnabled() && cell->flags().testFlag(Qt::ItemIsEnabled);
    logLook(cell->data(kKeyRole).toString(), cellLook(cell->isSelected(), cell->data(kDimmedRole).toBool(), enabled));
  }
}

void SketchItemList::logLook(const QString& key, const QString& look) {
  // A plain cell only once it was drawn otherwise (selected, dimmed).
  const auto logged = m_loggedLooks.constFind(key);
  if (logged == m_loggedLooks.cend() ? look.startsWith(QLatin1String("plain")) : logged.value() == look) {
    return;
  }
  m_loggedLooks.insert(key, look);
  qDebug().noquote() << QStringLiteral("Sketch step %1 drawn: %2").arg(key, look);
}

// ---------------------------------------------------------------------------
// The hover list

void SketchItemList::showHoverList(const QString& key, const QRect& cellRect) {
  const Cell* cell = cellOf(key);
  if (cell == nullptr) {
    hideHoverList();
    return;
  }
  if (m_hover && m_hover->isVisible() && key == m_hoverKey) {
    return;
  }
  if (!m_hover) {
    // A child of the window, raised over its other children, as the message
    // history is: above every floating card (mitcad#1763: as a child of
    // the view the message band's card lay over its lower rows, which the
    // pointer then never reached), and no window of its own, so that the
    // pointer goes onto its rows. With native glass the cards are windows
    // of their own (GlassCard::setWindowed): it is then a tool window
    // over them, as they are over the main window.
    QWidget* host = m_c.viewer().window();
    auto* frame = glassActive() ? new QFrame(host, Qt::Tool | Qt::FramelessWindowHint | Qt::NoDropShadowWindowHint)
                                : new QFrame(host);
    if (frame->isWindow()) {
      frame->setAttribute(Qt::WA_ShowWithoutActivating); // the keys stay with the main window
    }
    frame->setObjectName(QStringLiteral("sketchStepItems"));
    frame->setFrameShape(QFrame::StyledPanel);
    frame->setAutoFillBackground(true);
    frame->setBackgroundRole(QPalette::ToolTipBase);
    frame->setForegroundRole(QPalette::ToolTipText);
    frame->setMouseTracking(true);
    auto* layout = new QVBoxLayout(frame);
    layout->setContentsMargins(6, 4, 6, 6);
    layout->setSpacing(4);
    m_hoverTitle = new QLabel(frame);
    m_hoverTitle->setObjectName(QStringLiteral("sketchStepName"));
    m_hoverTitle->setTextFormat(Qt::PlainText);
    QFont bold = m_hoverTitle->font();
    bold.setBold(true);
    m_hoverTitle->setFont(bold);
    m_hoverTitle->setMouseTracking(true);
    layout->addWidget(m_hoverTitle);
    m_hoverRows = new QListWidget(frame);
    m_hoverRows->setObjectName(QStringLiteral("sketchStepItemRows"));
    m_hoverRows->setAccessibleName(tr("Items of the step"));
    m_hoverRows->setFrameShape(QFrame::NoFrame);
    m_hoverRows->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_hoverRows->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_hoverRows->setFocusPolicy(Qt::NoFocus);
    m_hoverRows->setMouseTracking(true);
    m_hoverRows->viewport()->setMouseTracking(true);
    m_hoverRows->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_hoverRows->setIconSize(uiIconSize(16, 16));
    m_hoverRows->setItemDelegate(new RowDelegate(m_hoverRows));
    m_hoverRows->viewport()->setAutoFillBackground(false);
    layout->addWidget(m_hoverRows);
    frame->installEventFilter(this);
    m_hoverTitle->installEventFilter(this);
    m_hoverRows->viewport()->installEventFilter(this);
    m_hover = frame;
  }
  m_hoverKey = key;
  m_hoverCell = cellRect;
  fillHoverList(*cell);
  placeHoverList(cellRect);
  m_hover->show();
  m_hover->raise();
  QStringList rows;
  for (const ItemRow& row : cell->items) {
    rows << row.logged;
  }
  qDebug().noquote() << QStringLiteral("Sketch step %1 hover list: %2: %3")
                            .arg(key, cell->name, rows.join(QStringLiteral(", ")));
  m_loggedRowPlaces.clear();
  TestSync::singleShot(0, this, [this] { logRowPlaces(); });
}

void SketchItemList::fillHoverList(const Cell& cell) {
  if (!m_hover) {
    return;
  }
  // The step's name, and what is wrong with it in words.
  QString title = cell.name;
  if (cell.level != NoticeLevel::Hint && !cell.why.isEmpty()) {
    title += QLatin1Char('\n') + cell.why;
  }
  m_hoverTitle->setText(title);
  m_hoverRows->clear();
  // An inactive item's row: its words in the disabled text colour, its icon
  // in its disabled colours.
  const QColor dimmed = palette().color(QPalette::Disabled, QPalette::Text);
  for (const ItemRow& row : cell.items) {
    auto* entry = new QListWidgetItem(badgedIcon(row.icon, row.level, 1, m_hoverRows->iconSize(), row.dimmed),
                                      row.text, m_hoverRows);
    entry->setData(kKeyRole, row.id);
    if (!row.why.isEmpty()) {
      entry->setToolTip(row.why);
    }
    if (row.dimmed) {
      entry->setForeground(dimmed);
    }
  }
  // As many rows as fit, the rest scroll.
  const int rows = std::min(static_cast<int>(cell.items.size()), kMostRows);
  const int rowHeight = std::max(m_hoverRows->sizeHintForRow(0), m_hoverRows->iconSize().height() + 4);
  m_hoverRows->setFixedHeight(rows * rowHeight + 2 * m_hoverRows->frameWidth() + 2);
  int width = m_hoverTitle->sizeHint().width();
  width = std::max(width, m_hoverRows->sizeHintForColumn(0) + m_hoverRows->verticalScrollBar()->sizeHint().width());
  m_hoverRows->setMinimumWidth(width);
  m_hover->adjustSize();
  showSelection();
}

void SketchItemList::placeHoverList(const QRect& cellRect) {
  if (!m_hover) {
    return;
  }
  // In the window's coordinates, over the view's area.
  QWidget& view = m_c.viewer();
  QWidget* host = view.window();
  const QRect area(host->mapFromGlobal(view.mapToGlobal(QPoint(0, 0))), view.size());
  const QPoint top = host->mapFromGlobal(viewport()->mapToGlobal(cellRect.topLeft()));
  const QPoint bottom = host->mapFromGlobal(viewport()->mapToGlobal(cellRect.bottomLeft()));
  const QSize size = m_hover->sizeHint().expandedTo(m_hover->size());
  // Above its cell where the view has room for it, else below; inside the
  // view either way (the Docked chrome has the list below the view). Over
  // the cards there (the Floating message band right above the timeline's
  // card, mitcad#1763).
  int y = top.y() - size.height() - 4;
  if (y < area.top()) {
    y = bottom.y() + 4;
  }
  y = std::clamp(y, area.top(), std::max(area.top(), area.bottom() + 1 - size.height()));
  const int x = std::clamp(top.x(), area.left(), std::max(area.left(), area.right() + 1 - size.width()));
  // A tool window (native glass) is placed on the screen.
  m_hover->move(m_hover->isWindow() ? host->mapToGlobal(QPoint(x, y)) : QPoint(x, y));
}

void SketchItemList::hideHoverList() {
  m_hideTimer.stop();
  hoverRow(QString());
  if (m_hover && m_hover->isVisible()) {
    m_hover->hide();
    qDebug().noquote() << QStringLiteral("Sketch step hover list closed");
  }
  m_hoverKey.clear();
  m_inHoverList = false;
}

void SketchItemList::hideHoverListLater() {
  if (m_hover && m_hover->isVisible()) {
    m_hideTimer.start();
  }
}

void SketchItemList::hoverRow(const QString& id) {
  if (id == m_hoveredRow) {
    return;
  }
  // Only the highlight this list set is taken back.
  if (!m_hoveredRow.isEmpty()) {
    if (m_c.hover() == m_hoveredRow) {
      m_c.setHover(QString());
    }
    if (m_c.hoverAnnotation() == m_hoveredRow) {
      m_c.setHoverAnnotation(QString());
    }
  }
  m_hoveredRow = id;
  if (!id.isEmpty() && m_c.isActive()) {
    const SketchModel& model = m_c.model();
    if (model.point(id) != nullptr || model.curve(id) != nullptr || model.text(id) != nullptr) {
      m_c.setHover(id);
    } else {
      m_c.setHoverAnnotation(id);
    }
  }
  qDebug().noquote() << QStringLiteral("Sketch step item %1 hovered").arg(id.isEmpty() ? QStringLiteral("none") : id);
}

void SketchItemList::logRowPlaces() {
  if (!m_hover || !m_hover->isVisible() || m_hoverRows == nullptr) {
    return;
  }
  QStringList lines;
  const QRect inSight = m_hoverRows->viewport()->rect();
  for (int i = 0; i < m_hoverRows->count(); ++i) {
    QListWidgetItem* row = m_hoverRows->item(i);
    const QRect place = m_hoverRows->visualItemRect(row);
    if (!inSight.contains(place.center())) {
      continue;
    }
    // In the main window's coordinates, also from a tool window (native glass).
    QWidget* host = m_c.viewer().window();
    const QPoint at = host->mapFromGlobal(m_hoverRows->viewport()->mapToGlobal(place.center()));
    lines << QStringLiteral("Sketch step item %1 at %2,%3").arg(row->data(kKeyRole).toString()).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedRowPlaces) {
    m_loggedRowPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

bool SketchItemList::eventFilter(QObject* watched, QEvent* event) {
  const bool ours = m_hover && (watched == m_hover.data() || watched == m_hoverTitle ||
                                (m_hoverRows != nullptr && watched == m_hoverRows->viewport()));
  if (!ours) {
    return QListWidget::eventFilter(watched, event);
  }
  const bool rows = m_hoverRows != nullptr && watched == m_hoverRows->viewport();
  switch (event->type()) {
  case QEvent::Enter:
  case QEvent::MouseMove:
  case QEvent::HoverMove:
    m_inHoverList = true;
    m_hideTimer.stop();
    if (rows && event->type() == QEvent::MouseMove) {
      const auto* move = static_cast<QMouseEvent*>(event);
      QListWidgetItem* row = m_hoverRows->itemAt(move->position().toPoint());
      hoverRow(row != nullptr ? row->data(kKeyRole).toString() : QString());
    }
    break;
  case QEvent::Leave:
    if (rows) {
      hoverRow(QString());
    } else if (watched == m_hover.data()) {
      m_inHoverList = false;
      hideHoverListLater();
    }
    break;
  case QEvent::MouseButtonPress:
    if (rows) {
      const auto* press = static_cast<QMouseEvent*>(event);
      QListWidgetItem* row = m_hoverRows->itemAt(press->position().toPoint());
      if (press->button() == Qt::LeftButton && row != nullptr) {
        const QString id = row->data(kKeyRole).toString();
        if (m_c.selectItem(id, press->modifiers())) {
          qDebug().noquote() << QStringLiteral("Sketch step item %1 clicked").arg(id);
        }
      }
      // The view's selection is the one selection: the rows only show it.
      return true;
    }
    break;
  case QEvent::MouseButtonRelease:
  case QEvent::MouseButtonDblClick:
    if (rows) {
      return true;
    }
    break;
  default:
    break;
  }
  return QListWidget::eventFilter(watched, event);
}

// ---------------------------------------------------------------------------
// The cells' events

bool SketchItemList::viewportEvent(QEvent* event) {
  if (event->type() == QEvent::Leave) {
    m_inCell = false;
    hideHoverListLater();
  }
  return QListWidget::viewportEvent(event);
}

void SketchItemList::mousePressEvent(QMouseEvent* event) {
  QListWidgetItem* cell = itemAt(event->position().toPoint());
  if (event->button() != Qt::LeftButton || cell == nullptr) {
    event->accept();
    return;
  }
  const QString key = cell->data(kKeyRole).toString();
  if (m_c.selectItems(cell->data(kItemsRole).toStringList(), event->modifiers())) {
    qDebug().noquote() << QStringLiteral("Sketch step %1 clicked").arg(key);
  }
  event->accept();
}

void SketchItemList::mouseReleaseEvent(QMouseEvent* event) { event->accept(); }

void SketchItemList::mouseMoveEvent(QMouseEvent* event) {
  // No rubber band or drag: the pointer over a cell opens its hover list.
  QListWidgetItem* cell = itemAt(event->position().toPoint());
  if (cell != nullptr && event->buttons() == Qt::NoButton) {
    m_inCell = true;
    m_hideTimer.stop();
    showHoverList(cell->data(kKeyRole).toString(), visualItemRect(cell));
  } else if (cell == nullptr) {
    m_inCell = false;
    hideHoverListLater();
  }
  event->accept();
}

void SketchItemList::mouseDoubleClickEvent(QMouseEvent* event) {
  QListWidgetItem* cell = itemAt(event->position().toPoint());
  if (event->button() != Qt::LeftButton || cell == nullptr) {
    event->accept();
    return;
  }
  if (m_c.tool() == nullptr) {
    // A dimension's step: its value editor where the view shows its value.
    for (const QString& id : cell->data(kItemsRole).toStringList()) {
      if (m_c.model().dimension(id) != nullptr) {
        hideHoverList();
        m_c.editDimension(id);
        break;
      }
    }
  }
  event->accept();
}

void SketchItemList::keyPressEvent(QKeyEvent* event) {
  bool remove = event->key() == Qt::Key_Delete;
#ifdef Q_OS_MACOS
  remove = remove || event->key() == Qt::Key_Backspace;
#endif
  if (remove && event->modifiers() == Qt::NoModifier) {
    // As the sketch's Delete: what the view has selected, which a click on
    // a cell selected.
    hideHoverList();
    m_c.deleteSelection();
    event->accept();
    return;
  }
  if (event->key() == Qt::Key_Escape) {
    hideHoverList();
  }
  // The arrows and the other keys do not move a selection of the cells'
  // own: they go on to the window (its shortcuts, Esc).
  event->ignore();
}

void SketchItemList::changeEvent(QEvent* event) {
  QListWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange && !m_marking && m_c.isActive()) {
    m_built = false;
    rebuild(); // the icons and the dimmed cells in the new colours
  }
}

} // namespace mitcad::sketch
