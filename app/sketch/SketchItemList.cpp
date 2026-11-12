// SPDX-License-Identifier: MIT
#include "SketchItemList.hpp"

#include <QDebug>
#include <QEvent>
#include <QKeyEvent>
#include <QListWidgetItem>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QScrollBar>
#include <QSet>
#include <QStringList>
#include <QtLogging>

#include <cstddef>
#include <utility>
#include <vector>

#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/Selection.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/UiSizes.hpp"
#include "SketchController.hpp"
#include "SketchOverlay.hpp"
#include "SketchTool.hpp"

namespace mitcad::sketch {

namespace {

constexpr int kIdRole = Qt::UserRole;

// A curve's kind in words and its icon: the drawing tool's.
QString curveKind(const CurveData& curve) {
  if (curve.isLine()) {
    return SketchItemList::tr("Line");
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
QString flagsText(bool construction, bool centerline, bool reference) {
  QStringList flags;
  if (reference) {
    flags << SketchItemList::tr("projected");
  }
  if (construction) {
    flags << SketchItemList::tr("construction");
  }
  if (centerline) {
    flags << SketchItemList::tr("centerline");
  }
  return flags.join(QStringLiteral(", "));
}

} // namespace

SketchItemList::SketchItemList(SketchController& controller, QWidget* parent)
    : QListWidget(parent), m_c(controller) {
  setObjectName(QStringLiteral("sketchItemList"));
  setAccessibleName(tr("Sketch items"));
  // Cells left to right in creation order, onto the next line when the
  // panel is full, scrolling down for more.
  setViewMode(QListView::ListMode);
  setFlow(QListView::LeftToRight);
  setWrapping(true);
  setResizeMode(QListView::Adjust);
  setSpacing(2);
  setIconSize(uiIconSize(16, 16));
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  // The view's selection is the one selection: the rows only show it, so a
  // click or a key never changes them by itself.
  setSelectionMode(QAbstractItemView::ExtendedSelection);
  setEditTriggers(QAbstractItemView::NoEditTriggers);
  setDragEnabled(false);
  setFocusPolicy(Qt::ClickFocus);
  setFrameShape(QFrame::NoFrame);
  // The panel's own background shows through, as under the timeline.
  viewport()->setAutoFillBackground(false);
  connect(&m_c, &SketchController::changed, this, &SketchItemList::rebuild);
  connect(&m_c, &SketchController::viewRefreshed, this, &SketchItemList::showSelection);
  connect(verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { schedulePlaces(); });
  // Its places are logged again when it moves in the window.
  new PlaceWatcher(this, [this] { logPlaces(); });
}

QSize SketchItemList::sizeHint() const {
  return {uiIconSize(16, 16).width() * 16, fontMetrics().height() + 12};
}

QSize SketchItemList::minimumSizeHint() const { return {uiIconSize(16, 16).width() * 4, fontMetrics().height() + 8}; }

QStringList SketchItemList::orderedIds() const {
  const SketchModel& model = m_c.model();
  if (!model.order.isEmpty()) {
    return model.order;
  }
  QStringList ids;
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
  return ids;
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

QIcon SketchItemList::rowIcon(const QString& name, NoticeLevel level) const {
  const QIcon icon = themeIcon(name);
  if (!isProblem(level) && level != NoticeLevel::Information) {
    return icon;
  }
  const QSize size = iconSize();
  const qreal ratio = devicePixelRatioF();
  QPixmap pixmap(QSize(qRound(size.width() * ratio), qRound(size.height() * ratio)));
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  icon.paint(&painter, QRect(QPoint(0, 0), size));
  const qreal side = size.width() * 0.6;
  paintNoticeBadge(painter, QRectF(size.width() - side, 0.0, side, side), level, palette().color(QPalette::Base));
  painter.end();
  return QIcon(pixmap);
}

SketchItemList::Row SketchItemList::pointRow(const PointData& point) const {
  Row row;
  row.id = point.id;
  row.icon = QStringLiteral("point");
  const bool origin = point.id == m_c.model().originPoint();
  row.text = origin ? tr("Origin") : tr("Point");
  const QString flags = flagsText(point.construction, false, point.reference);
  if (!flags.isEmpty()) {
    row.text = tr("%1 (%2)").arg(row.text, flags);
  }
  row.tooltip = row.text;
  row.logged = QStringLiteral("%1 %2").arg(point.id, origin ? QStringLiteral("origin") : QStringLiteral("point"));
  if (point.construction) {
    row.logged += QStringLiteral(" construction");
  }
  return row;
}

SketchItemList::Row SketchItemList::curveRow(const CurveData& curve) const {
  Row row;
  row.id = curve.id;
  row.icon = curveIcon(curve);
  if (curve.axis == QStringLiteral("x")) {
    row.text = tr("X axis");
  } else if (curve.axis == QStringLiteral("y")) {
    row.text = tr("Y axis");
  } else {
    row.text = curveKind(curve);
    const QString flags = flagsText(curve.construction, curve.centerline, curve.reference);
    if (!flags.isEmpty()) {
      row.text = tr("%1 (%2)").arg(row.text, flags);
    }
  }
  row.tooltip = row.text;
  row.logged = QStringLiteral("%1 %2").arg(curve.id, curve.type);
  if (curve.construction) {
    row.logged += QStringLiteral(" construction");
  }
  if (curve.centerline) {
    row.logged += QStringLiteral(" centerline");
  }
  return row;
}

SketchItemList::Row SketchItemList::constraintRow(const ConstraintData& constraint) const {
  Row row;
  row.id = constraint.id;
  row.icon = constraintIcon(constraint.type);
  row.text = constraintName(constraint.type);
  row.tooltip = row.text;
  const QString inactive = SketchOverlay::inactiveTooltip(constraint.active, constraint.inactiveReason);
  if (!inactive.isEmpty()) {
    row.tooltip += QLatin1Char('\n') + inactive;
  }
  row.dimmed = !constraint.active;
  row.logged = QStringLiteral("%1 %2").arg(constraint.id, constraint.type);
  if (!constraint.active) {
    row.logged += QStringLiteral(" inactive");
  }
  return row;
}

SketchItemList::Row SketchItemList::dimensionRow(const DimensionData& dimension) const {
  Row row;
  row.id = dimension.id;
  row.icon = QStringLiteral("dimension");
  // The value as the view shows it: R, Ø, fx:, the name, parentheses when
  // driven.
  row.text = m_c.overlay().dimensionText(dimension);
  row.tooltip = m_c.overlay().dimensionTooltip(dimension);
  const QString inactive = SketchOverlay::inactiveTooltip(dimension.active, dimension.inactiveReason);
  if (!inactive.isEmpty()) {
    row.tooltip += QLatin1Char('\n') + inactive;
  }
  row.dimmed = !dimension.active;
  row.logged = QStringLiteral("%1 %2 %3").arg(dimension.id, dimension.type, row.text);
  if (!dimension.active) {
    row.logged += QStringLiteral(" inactive");
  }
  return row;
}

void SketchItemList::rebuild() {
  if (!m_c.isActive()) {
    clear();
    m_built = false;
    m_shown.clear();
    m_shownConflicts.clear();
    m_loggedItems.clear();
    m_loggedPlaces.clear();
    m_loggedSelection.clear();
    return;
  }
  const SketchModel& model = m_c.model();
  m_shownConflicts = m_c.conflicts();
  std::vector<Row> rows;
  for (const QString& id : orderedIds()) {
    if (const PointData* point = model.point(id)) {
      rows.push_back(pointRow(*point));
    } else if (const CurveData* curve = model.curve(id)) {
      rows.push_back(curveRow(*curve));
    } else if (const ConstraintData* constraint = model.constraint(id)) {
      rows.push_back(constraintRow(*constraint));
    } else if (const DimensionData* dimension = model.dimension(id)) {
      rows.push_back(dimensionRow(*dimension));
    }
  }
  // The marks, and what each cell shows.
  std::vector<NoticeLevel> levels;
  QStringList shown;
  for (Row& row : rows) {
    QString why;
    levels.push_back(markOf(row.id, why));
    if (levels.back() != NoticeLevel::Hint) {
      row.tooltip += QLatin1Char('\n') + noticeLevelText(levels.back(), why);
      row.logged += QStringLiteral(" [%1]").arg(noticeLevelName(levels.back()));
    }
    shown << QStringList{row.logged, row.icon, row.text, row.tooltip,
                         row.dimmed ? QStringLiteral("dimmed") : QString()}
                 .join(QLatin1Char('\t'));
  }
  // A reload that changed none of them (a drag's step, a selection) keeps
  // the cells as they are.
  if (m_built && shown == m_shown) {
    showSelection();
    return;
  }
  m_built = true;
  m_shown = shown;
  const int scrolled = verticalScrollBar()->value();
  m_marking = true;
  clear();
  QStringList logged;
  const QColor dimmed = palette().color(QPalette::Disabled, QPalette::Text);
  for (std::size_t i = 0; i < rows.size(); ++i) {
    const Row& row = rows[i];
    auto* cell = new QListWidgetItem(rowIcon(row.icon, levels[i]), row.text, this);
    cell->setData(kIdRole, row.id);
    cell->setToolTip(row.tooltip);
    if (row.dimmed) {
      cell->setForeground(dimmed);
    }
    logged << row.logged;
  }
  m_marking = false;
  verticalScrollBar()->setValue(scrolled);
  const QString line = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", "));
  if (line != m_loggedItems) {
    m_loggedItems = line;
    qDebug().noquote() << QStringLiteral("Sketch items: %1").arg(line);
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
    QListWidgetItem* row = item(i);
    const QString id = row->data(kIdRole).toString();
    const bool on = selected.contains(id);
    row->setSelected(on);
    if (on) {
      marked << id;
      if (first == nullptr) {
        first = row;
      }
    }
  }
  m_marking = false;
  const QString line = marked.isEmpty() ? QStringLiteral("none") : marked.join(QStringLiteral(", "));
  if (line != m_loggedSelection) {
    m_loggedSelection = line;
    qDebug().noquote() << QStringLiteral("Sketch item rows selected: %1").arg(line);
    // What the view selected comes into sight here too.
    if (first != nullptr) {
      scrollToItem(first, QAbstractItemView::EnsureVisible);
    }
  }
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
  const QRect shown = viewport()->rect();
  for (int i = 0; i < count(); ++i) {
    const QRect cell = visualItemRect(item(i));
    if (!shown.contains(cell.center())) {
      continue;
    }
    const QPoint at = GlassCard::mapToHost(viewport(), cell.center());
    lines << QStringLiteral("Sketch item %1 at %2,%3").arg(item(i)->data(kIdRole).toString()).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

void SketchItemList::mousePressEvent(QMouseEvent* event) {
  QListWidgetItem* row = itemAt(event->position().toPoint());
  if (event->button() != Qt::LeftButton || row == nullptr) {
    event->accept();
    return;
  }
  const QString id = row->data(kIdRole).toString();
  if (m_c.selectItem(id, event->modifiers())) {
    qDebug().noquote() << QStringLiteral("Sketch item %1 clicked").arg(id);
  }
  event->accept();
}

void SketchItemList::mouseReleaseEvent(QMouseEvent* event) { event->accept(); }

void SketchItemList::mouseMoveEvent(QMouseEvent* event) {
  // No rubber band or drag: the press selected what it was on.
  event->accept();
}

void SketchItemList::mouseDoubleClickEvent(QMouseEvent* event) {
  QListWidgetItem* row = itemAt(event->position().toPoint());
  if (event->button() != Qt::LeftButton || row == nullptr) {
    event->accept();
    return;
  }
  const QString id = row->data(kIdRole).toString();
  if (m_c.model().dimension(id) != nullptr && m_c.tool() == nullptr) {
    // Its value editor where the view shows its value.
    m_c.editDimension(id);
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
    // a row selected.
    m_c.deleteSelection();
    event->accept();
    return;
  }
  // The arrows and the other keys do not move a selection of the rows'
  // own: they go on to the window (its shortcuts, Esc).
  event->ignore();
}

void SketchItemList::changeEvent(QEvent* event) {
  QListWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange && !m_marking && m_c.isActive()) {
    m_built = false;
    rebuild(); // the icons and the dimmed rows in the new colours
  }
}

} // namespace mitcad::sketch
