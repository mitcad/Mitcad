// SPDX-License-Identifier: MIT
// Test steps for sketches: clicks and drags at sketch millimetres (and at a drawing sheet's),
// the sketch pointer, highlights and point markers, dimensions and texts,
// the profiles.
// Split from TestDriver.cpp (mitcad#2173), so that a UI test depends on the
// step areas it uses; the steps are described in TestDriver.hpp.
#include "TestDriver.hpp"
#include "TestDriverSupport.hpp"
#include "Diagnostics.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdlib>
#include <limits>
#include <memory>
#include <unordered_set>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Polygon3D.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_TShape.hxx>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QAbstractScrollArea>
#include <QApplication>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QContextMenuEvent>
#include <QDir>
#include <QDockWidget>
#include <QEnterEvent>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFormLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPixmap>
#include <QPointingDevice>
#include <QRegion>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QSettings>
#include <QShortcut>
#include <QStyleHints>
#include <QThread>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QTextDocument>
#include <QWheelEvent>
#include <QWidget>
#include <QWindow>
#include <QtGlobal>
#include <QtLogging>

#include "../platform/MacChrome.hpp"
#include "../platform/MacTestApplication.hpp"
#include "../platform/NativeTestInput.hpp"
#include "../OcctViewer.hpp"
#include "../DocumentFrame.hpp"
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"
#include "../update/UpdateController.hpp"
#include "../view/SpaceMouseController.hpp"
#include "../view/ViewController.hpp"
#include "../view/ViewEdges.hpp"
#include "../sketch/SketchController.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "../sketch/SketchTool.hpp"
#include "../browser/TimelineWidget.hpp"
#include "GlassCard.hpp"
#include "Ribbon.hpp"
#include "TestSync.hpp"
#include "HangWatchdog.hpp"
#include "SessionLog.hpp"
#include "Dialogs.hpp"
#include "DisplayMeshes.hpp"
#include "Icons.hpp"
#include "ComputeProgress.hpp"
#include "Theme.hpp"
#include "TreeItemDelegate.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {

using namespace testdriver;

namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"sketch-click", 2, 4, TestDriver::kInputOfMain},
    {"sketch-dblclick", 2, 4, TestDriver::kInputOfMain},
    {"sketch-move", 2, 2, TestDriver::kInputOfMain},
    {"drawing-sheet-click", 2, 4, TestDriver::kInputOfMain}, // mitcad#1841
    {"drawing-sheet-pointer", 3, 4, TestDriver::kInputOfMain},
    {"expect-sketch-field-dimension", 3, 3, TestDriver::kIdle},
    {"expect-sketch-dimension-clear", 1, 1, TestDriver::kIdle},
    {"expect-sketch-dimensions-apart", 1, 1, TestDriver::kIdle},
    {"expect-sketch-text", 2, 2, TestDriver::kIdle},
    {"sketch-click-text", 1, 2, TestDriver::kInputOfMain},
    // A click on the middle of a curve of the edited sketch (mitcad#2163).
    {"sketch-click-curve", 1, 2, TestDriver::kInputOfMain},
    {"sketch-drag", 4, 4, TestDriver::kInputOfMain},
    {"sketch-pointer", 3, 4, TestDriver::kInputOfMain},
    {"expect-profiles", 1, 1, TestDriver::kIdle},
    {"expect-sketch-highlights-on-curves", 1, 1, TestDriver::kIdle},
    {"expect-sketch-point-markers", 2, 2, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kSketchSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runSketchStep};

void TestDriver::runSketchStep(const QString& name, const QStringList& a, int line) {
  QWidget* mainWindow = m_main.data();
  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("expect-sketch-highlights-on-curves")) {
    // expect-sketch-highlights-on-curves <curves>: at least that many
    // highlighted sketch curves, each highlight's polyline on the shown
    // curve's within a pixel of the view as it is now and twice the view's
    // line deviation (mitcad#1633).
    double curves = 0;
    if (!toNumber(a.at(0), curves)) {
      fail(line, QStringLiteral("expect-sketch-highlights-on-curves <curves>"));
      return;
    }
    whenReady([this, curves] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const OcctViewer::SketchHighlightFit fit = window->viewer().sketchHighlightFit();
      const QString counts = QStringLiteral("%1 sketch curve highlights, at most %2 mm and %3 px off their curves")
                                 .arg(fit.curves)
                                 .arg(fit.millimetres, 0, 'g', 3)
                                 .arg(fit.pixels, 0, 'f', 2);
      m_waitDetail = counts;
      if (fit.curves < qRound(curves) || fit.pixels > 1.0 || fit.millimetres > 2.0 * kViewEdgeDeviation + 1e-6) {
        return false;
      }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(counts);
      return true;
    }, 6, QStringLiteral("a sketch curve's highlight lies off the curve"));
  } else if (name == QLatin1String("expect-sketch-point-markers")) {
    // expect-sketch-point-markers <sketch uid> drawn|hidden: the view shows
    // points of that sketch, every one with its marker (drawn) or every one
    // without (hidden, Show points off; mitcad#1764), also outside sketch
    // mode. Logged as "TestDriver: sketch F1 point markers: <n> drawn, <m>
    // hidden".
    const QString sketch = a.at(0);
    const bool drawn = a.at(1) == QLatin1String("drawn");
    if (!drawn && a.at(1) != QLatin1String("hidden")) {
      fail(line, QStringLiteral("expect-sketch-point-markers <sketch uid> drawn|hidden"));
      return;
    }
    whenReady([this, sketch, drawn] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const OcctViewer::SketchPointMarkers markers = window->viewer().sketchPointMarkers(sketch);
      const QString counts = QStringLiteral("sketch %1 point markers: %2 drawn, %3 hidden")
                                 .arg(sketch)
                                 .arg(markers.drawn)
                                 .arg(markers.hidden);
      m_waitDetail = counts;
      if ((drawn ? markers.drawn : markers.hidden) == 0 || (drawn ? markers.hidden : markers.drawn) != 0) {
        return false;
      }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(counts);
      return true;
    }, 6, QStringLiteral("the sketch's points are not all %1").arg(a.at(1)));
  } else if (name == QLatin1String("expect-sketch-dimension-clear")) {
    auto* controller = mainWindow->findChild<sketch::SketchController*>();
    if (!controller || !controller->isActive() || !controller->showDimensions() || !controller->showConstraints()) {
      fail(line, QStringLiteral("dimension readability requires an active sketch with dimensions and constraints shown")); return;
    }
    QRegularExpression expression;
    if (!regex(a.at(0), expression)) return;
    const auto& overlay = controller->overlay();
    const auto glyphs = overlay.constraintGlyphBounds();
    if (glyphs.empty()) { fail(line, QStringLiteral("no constraint glyphs are drawn")); return; }
    int matched = 0;
    for (const auto& dimension : controller->model().dimensions) {
      const QString text = overlay.dimensionText(dimension);
      if (!expression.match(text).hasMatch()) continue;
      ++matched;
      const QRectF bounds = overlay.dimensionTextRect(dimension.id);
      if (bounds.isEmpty() || !QRectF(viewer()->rect()).contains(bounds)) {
        fail(line, QStringLiteral("dimension %1 (%2) is not visible inside the viewport").arg(dimension.id, text)); return;
      }
      for (const QRectF& glyph : glyphs) {
        if (glyph.intersects(bounds.adjusted(-2, -2, 2, 2))) {
          fail(line, QStringLiteral("dimension %1 (%2) is covered by a constraint glyph").arg(dimension.id, text)); return;
        }
      }
      const auto hit = overlay.hitTest(bounds.center());
      if (hit.kind != sketch::SketchOverlay::Hit::Kind::Dimension || hit.id != dimension.id) {
        fail(line, QStringLiteral("dimension %1 lost its ordinary pick target").arg(dimension.id)); return;
      }
    }
    if (matched == 0) { fail(line, QStringLiteral("no dimension text matches '%1'").arg(a.at(0))); return; }
    for (const QRectF& glyph : glyphs) {
      const auto kind = overlay.hitTest(glyph.center()).kind;
      if (kind != sketch::SketchOverlay::Hit::Kind::Constraint && kind != sketch::SketchOverlay::Hit::Kind::Fixed) {
        fail(line, QStringLiteral("a drawn constraint glyph lost its ordinary pick target")); return;
      }
    }
    qInfo() << "Sketch dimension readability:" << matched << "matching labels clear of" << glyphs.size()
            << "constraint glyphs; ordinary pick targets retained";
  } else if (name == QLatin1String("expect-sketch-text")) {
    // expect-sketch-text <text regex> <lines> (mitcad#2029): the edited
    // sketch's text whose text (its lines joined by \n) matches was laid
    // out in that many lines (or fewest-most), and a text wrapped in a
    // frame lies within the frame's width (its outlines along the frame's
    // x side, 0.05 mm allowed for the letters' ink). Logged as
    // "TestDriver: text t3: 2 lines, from 0.31 to 38.20 in a frame 40.00
    // wide".
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    // The lines: a count, or a range "2-3" where a font's widths decide.
    const QStringList range = a.at(1).split(QLatin1Char('-'));
    bool counted = false;
    bool high = true;
    const int fewest = range.value(0).toInt(&counted);
    const int most = range.size() == 2 ? range.at(1).toInt(&high) : fewest;
    if (range.size() > 2 || !counted || !high || most < fewest) {
      fail(line, QStringLiteral("expect-sketch-text <text regex> <lines, or fewest-most>"));
      return;
    }
    whenReady([this, re, fewest, most] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      auto* controller = window->findChild<sketch::SketchController*>();
      if (!controller || !controller->isActive()) return false;
      const sketch::SketchModel& model = controller->model();
      for (const sketch::TextData& text : model.texts) {
        if (!re.match(text.text).hasMatch()) continue;
        QString found = QStringLiteral("text %1: %2 lines").arg(text.id).arg(text.lines);
        bool inside = true;
        if (text.wrap && text.frame.size() >= 2) {
          const sketch::PointData* o = model.point(text.frame.at(0));
          const sketch::PointData* x = model.point(text.frame.at(1));
          if (!o || !x) return false;
          const double width = sketch::dist(o->at, x->at);
          if (!(width > 0.0)) return false;
          const sketch::V2 u{(x->at.x - o->at.x) / width, (x->at.y - o->at.y) / width};
          double lo = std::numeric_limits<double>::infinity();
          double hi = -std::numeric_limits<double>::infinity();
          for (const sketch::Curve2& curve : text.outline) {
            for (const sketch::V2& p : curve.sample(0.05)) {
              const double along = (p.x - o->at.x) * u.x + (p.y - o->at.y) * u.y;
              lo = std::min(lo, along);
              hi = std::max(hi, along);
            }
          }
          found += QStringLiteral(", from %1 to %2 in a frame %3 wide")
                       .arg(lo, 0, 'f', 2).arg(hi, 0, 'f', 2).arg(width, 0, 'f', 2);
          inside = lo <= hi && lo >= -0.05 && hi <= width + 0.05;
        }
        m_waitDetail = found;
        if (text.lines < fewest || text.lines > most || !inside) return false;
        qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(found);
        return true;
      }
      m_waitDetail = QStringLiteral("no text of the sketch matches");
      return false;
    }, 6, QStringLiteral("no text matching %1 in %2 lines within its frame").arg(a.at(0), a.at(1)));
  } else if (name == QLatin1String("sketch-click-curve")) {
    // sketch-click-curve <curve id> [right|mods] (mitcad#2163): a click on
    // the middle of the edited sketch's curve of that id.
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!parseMouseOptions(a, 1, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    const QString id = a.at(0);
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, id, at] {
          auto* window = qobject_cast<MainWindow*>(m_main.data());
          if (!window || window->modelBusy()) return false;
          auto* controller = window->findChild<sketch::SketchController*>();
          if (!controller || !controller->isActive()) return false;
          const sketch::CurveData* curve = controller->model().curve(id);
          if (curve == nullptr) return false;
          const sketch::V2 p = curve->curve.middle();
          return sketchPoint(p.x, p.y, *at);
        },
        6, QStringLiteral("no curve %1 in the edited sketch").arg(id),
        [this, at, button, modifiers] { queueClick(*at, button, modifiers, 1); }, mainWindow);
  } else if (name == QLatin1String("sketch-click-text")) {
    // sketch-click-text <text regex> [right|mods] (mitcad#2029): a click on
    // the outline of the edited sketch's text whose text matches (the middle
    // of its first letter's first piece), which picks the text wherever its
    // lines were laid out; `right` opens its context menu (mitcad#2163).
    QRegularExpression re;
    if (!regex(a.at(0), re)) return;
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!parseMouseOptions(a, 1, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    const auto at = std::make_shared<QPoint>();
    whenReady(
        [this, re, at] {
          auto* window = qobject_cast<MainWindow*>(m_main.data());
          if (!window || window->modelBusy()) return false;
          auto* controller = window->findChild<sketch::SketchController*>();
          if (!controller || !controller->isActive()) return false;
          for (const sketch::TextData& text : controller->model().texts) {
            if (re.match(text.text).hasMatch() && !text.outline.empty()) {
              const sketch::V2 p = text.outline.front().middle();
              return sketchPoint(p.x, p.y, *at);
            }
          }
          return false;
        },
        6, QStringLiteral("no outline of a text matching %1").arg(a.at(0)),
        [this, at, button, modifiers] { queueClick(*at, button, modifiers, 1); }, mainWindow);
  } else if (name == QLatin1String("expect-sketch-dimensions-apart")) {
    // The values of the dimensions whose text matches, as drawn, overlap
    // none of the others (mitcad#1673).
    auto* controller = mainWindow->findChild<sketch::SketchController*>();
    if (!controller || !controller->isActive() || !controller->showDimensions()) {
      fail(line, QStringLiteral("dimension values apart requires an active sketch with dimensions shown")); return;
    }
    QRegularExpression expression;
    if (!regex(a.at(0), expression)) return;
    const auto& overlay = controller->overlay();
    std::vector<std::pair<QString, QRectF>> drawn;
    for (const auto& dimension : controller->model().dimensions) {
      const QString text = overlay.dimensionText(dimension);
      if (!expression.match(text).hasMatch()) continue;
      const QRectF bounds = overlay.dimensionTextRect(dimension.id);
      if (bounds.isEmpty()) {
        fail(line, QStringLiteral("dimension %1 (%2) has no value drawn").arg(dimension.id, text)); return;
      }
      for (const auto& earlier : drawn) {
        if (earlier.second.intersects(bounds)) {
          fail(line, QStringLiteral("dimension %1's value (%2) overlaps %3's").arg(dimension.id, text, earlier.first));
          return;
        }
      }
      drawn.emplace_back(dimension.id, bounds);
    }
    if (drawn.size() < 2) {
      fail(line, QStringLiteral("fewer than two dimension texts match '%1'").arg(a.at(0))); return;
    }
    qInfo().noquote() << QStringLiteral("Sketch dimension values apart: %1 values at a pixel size of %2 mm")
                             .arg(static_cast<int>(drawn.size()))
                             .arg(controller->pixel(), 0, 'g', 4);
  } else if (name == QLatin1String("expect-sketch-field-dimension")) {
    auto* controller = mainWindow->findChild<sketch::SketchController*>();
    double expected = 0;
    if (!controller || !controller->tool() || !toNumber(a.at(2), expected)) {
      fail(line, QStringLiteral("expected an active sketch drawing tool and a numeric dimension")); return;
    }
    sketch::ToolPreview preview;
    controller->tool()->paint(preview);
    const auto found = std::find_if(preview.fieldDimensions.begin(), preview.fieldDimensions.end(),
                                    [&a](const auto& value) { return value.field == a.at(0); });
    if (found == preview.fieldDimensions.end() || found->dimension.type != a.at(1)) {
      fail(line, QStringLiteral("field %1 has no %2 dimension preview").arg(a.at(0), a.at(1))); return;
    }
    const auto& value = *found;
    auto measured = sketch::measure(value.dimension, value.model);
    if (value.dimension.type == QStringLiteral("arc_length")) {
      const auto* arc = value.model.curve(QStringLiteral("preview"));
      measured = arc ? std::optional<double>(value.dimension.previewSweep.value_or(
                           sketch::sweep(arc->curve.start, arc->curve.end))) : std::nullopt;
    }
    if (!measured || std::abs(*measured - expected) > 1e-6) {
      fail(line, QStringLiteral("field %1 dimension measures %2, expected %3")
                     .arg(a.at(0)).arg(measured.value_or(-1)).arg(expected)); return;
    }
    const QRectF field = controller->fieldRect(value.field);
    const auto at = controller->sketchAtF(field.center());
    if (field.isEmpty() || !at || !QRectF(viewer()->rect()).contains(field)) {
      fail(line, QStringLiteral("field %1 lies outside the view").arg(value.field)); return;
    }
    auto dimension = value.dimension;
    dimension.text = *at;
    const sketch::Projection projection{[controller](const sketch::V2& p) { return controller->toScreen(p); },
                                         controller->pixel(), controller->faceBend()};
    const auto shape = sketch::dimensionShape(dimension, value.model, projection, QString());
    if (!shape.valid || shape.arrows.empty() || (shape.lines.empty() && shape.arcs.empty()) ||
        QLineF(shape.textAt, field.center()).length() > 1.0) {
      fail(line, QStringLiteral("field %1 is missing its dimension graphics: valid=%2 arrows=%3 lines=%4 arcs=%5 text-field delta=%6 px")
                     .arg(value.field).arg(shape.valid).arg(shape.arrows.size()).arg(shape.lines.size())
                     .arg(shape.arcs.size()).arg(QLineF(shape.textAt, field.center()).length(), 0, 'g', 12)); return;
    }
    for (auto* other : viewer()->findChildren<QLineEdit*>()) {
      if (other->isVisible() && other->objectName().startsWith(QStringLiteral("sketchField_")) &&
          other->objectName() != QStringLiteral("sketchField_") + value.field &&
          field.intersects(QRectF(other->geometry()))) {
        fail(line, QStringLiteral("field %1 overlaps %2").arg(value.field, other->objectName())); return;
      }
    }
    for (auto* label : viewer()->findChildren<QLabel*>()) {
      if (!label->isVisible() || !label->objectName().startsWith(QStringLiteral("sketchFieldLabel_"))) {
        continue;
      }
      if (!QRectF(viewer()->rect()).contains(QRectF(label->geometry()))) {
        fail(line, QStringLiteral("field label %1 lies outside the view").arg(label->objectName())); return;
      }
      for (auto* edit : viewer()->findChildren<QLineEdit*>()) {
        if (edit->isVisible() && edit->objectName().startsWith(QStringLiteral("sketchField_")) &&
            QRectF(label->geometry()).intersects(QRectF(edit->geometry()))) {
          fail(line, QStringLiteral("field label %1 covers %2").arg(label->objectName(), edit->objectName())); return;
        }
      }
    }
  } else if (name == QLatin1String("expect-profiles")) {
    double count = 0;
    if (!toNumber(a.at(0), count) || count < 0 || count != std::floor(count)) {
      fail(line, QStringLiteral("expect-profiles <nonnegative-count>")); return;
    }
    whenReady([this, count] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      return view && view->profileCount() == static_cast<std::size_t>(count);
    }, 6, QStringLiteral("expected %1 displayed profiles").arg(count));
  } else if (name == QLatin1String("sketch-pointer") || name == QLatin1String("drawing-sheet-pointer")) {
    const bool sheet = name == QLatin1String("drawing-sheet-pointer");
    const QString phase = a.at(0);
    double x = 0;
    double y = 0;
    // Modifiers held on this event only (mitcad#1538: Shift during a
    // point's drag turns its snapping off).
    Qt::KeyboardModifiers held = Qt::NoModifier;
    if ((phase != QLatin1String("press") && phase != QLatin1String("move") && phase != QLatin1String("release")) ||
        !toNumber(a.at(1), x) || !toNumber(a.at(2), y) || (a.size() > 3 && !parseModifiers(a.at(3), held))) {
      fail(line, QStringLiteral("%1 press|move|release <x> <y> [mods]").arg(name)); return;
    }
    const auto at = std::make_shared<QPoint>();
    whenReady([this, sheet, x, y, at] { return sheet ? drawingSheetPoint(x, y, *at) : sketchPoint(x, y, *at); }, 6,
              sheet ? QStringLiteral("the app logged no drawing sheet view") : QStringLiteral("the app logged no sketch view"),
              [this, phase, at, held] {
      if (phase == QLatin1String("press")) {
        sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::NoButton, held);
        sendMouse(QEvent::MouseButtonPress, *at, Qt::LeftButton, Qt::LeftButton, held);
      } else if (phase == QLatin1String("move")) {
        sendMouse(QEvent::MouseMove, *at, Qt::NoButton, Qt::LeftButton, held);
      } else {
        sendMouse(QEvent::MouseButtonRelease, *at, Qt::LeftButton, Qt::NoButton, held);
      }
    }, mainWindow);
  } else if (name == QLatin1String("sketch-click") || name == QLatin1String("sketch-dblclick") ||
             name == QLatin1String("sketch-drag") ||
             name == QLatin1String("sketch-move") || name == QLatin1String("drawing-sheet-click")) {
    // drawing-sheet-click: a click at a drawing sheet's point (mm), as
    // sketch-click at a sketch's (mitcad#1841).
    const bool sheet = name == QLatin1String("drawing-sheet-click");
    const bool drag = name == QLatin1String("sketch-drag");
    const bool move = name == QLatin1String("sketch-move");
    const int clicks = name == QLatin1String("sketch-dblclick") ? 2 : 1;
    double c[4] = {0, 0, 0, 0};
    const int count = drag ? 4 : 2;
    for (int i = 0; i < count; ++i) {
      if (!toNumber(a.at(i), c[i])) {
        fail(line, QStringLiteral("'%1' is not a sketch coordinate").arg(a.at(i)));
        return;
      }
    }
    Qt::MouseButton button = Qt::LeftButton;
    Qt::KeyboardModifiers modifiers;
    QString error;
    if (!drag && !parseMouseOptions(a, 2, button, modifiers, error)) {
      fail(line, error);
      return;
    }
    const auto from = std::make_shared<QPoint>();
    const auto to = std::make_shared<QPoint>();
    const double x1 = c[0];
    const double y1 = c[1];
    const double x2 = c[2];
    const double y2 = c[3];
    whenReady(
        [this, sheet, drag, x1, y1, x2, y2, from, to] {
          if (sheet) {
            return drawingSheetPoint(x1, y1, *from);
          }
          return sketchPoint(x1, y1, *from) && (!drag || sketchPoint(x2, y2, *to));
        },
        6, sheet ? QStringLiteral("the app logged no drawing sheet view") : QStringLiteral("the app logged no sketch view"),
        [this, drag, move, clicks, from, to, button, modifiers] {
          if (drag) {
            queueDrag(*from, *to, Qt::LeftButton, Qt::NoModifier);
          } else if (move) {
            sendMouse(QEvent::MouseMove, *from, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
          } else {
            queueClick(*from, button, modifiers, clicks);
          }
        },
        mainWindow);

    // --- Commands, menus and buttons
  }
}

} // namespace mitcad
