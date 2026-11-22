// SPDX-License-Identifier: MIT
// Test steps for what the 3D view shows of the design: the bodies shown and their meshes,
// edges and seams, the origin, the view's own shapes, body highlights, a
// preview's back faces and the view's buttons.
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
    {"preview-back-faces", 1, 1, TestDriver::kIdle},
    {"expect-view-buttons", 1, 1, TestDriver::kIdle},
    {"test-body-highlights", 1, 1, TestDriver::kInputOfMain},
    {"expect-shown-body", 3, 9, TestDriver::kIdle},
    {"capture-shown-body", 3, 3, TestDriver::kIdle},
    {"expect-shown-body-meshes", 3, 3, TestDriver::kIdle},
    {"expect-body-edges-unmeshed", 1, 1, TestDriver::kIdle},
    {"expect-view-own-shapes", 0, 0, TestDriver::kIdle},
    {"expect-view-origin", 1, 1, TestDriver::kIdle},
    {"expect-view-edges", 2, 3, TestDriver::kIdle},
    {"expect-view-seams", 3, 3, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kViewShapeSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runViewShapeStep};

void TestDriver::runViewShapeStep(const QString& name, const QStringList& a, int line) {
  if (name == QLatin1String("capture-shown-body")) {
    const QString prefix = a.at(0);
    const QString uid = a.at(1);
    const QString occurrence = a.at(2) == QLatin1String("-") ? QString() : a.at(2);
    whenReady([this, prefix, uid, occurrence] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || QString::fromStdString(body.occurrence) != occurrence || !body.shape)
          continue;
        const double volume = geometry::volume(*body.shape);
        const geometry::Shape world(body.shape->occt().Moved(body.placement));
        const auto box = geometry::bounding_box(world);
        if (!std::isfinite(volume) || volume < 0 || box.empty) break;
        const std::array<double, 7> values{volume, box.min.X(), box.min.Y(), box.min.Z(),
                                         box.max.X(), box.max.Y(), box.max.Z()};
        const char* keys[] = {"volume", "min_x", "min_y", "min_z", "max_x", "max_y", "max_z"};
        for (std::size_t i = 0; i < values.size(); ++i) {
          if (!std::isfinite(values[i])) return false;
        }
        for (std::size_t i = 0; i < values.size(); ++i)
          m_vars.insert(prefix + QLatin1Char('_') + QLatin1String(keys[i]), QString::number(values[i], 'g', 17));
        qInfo().noquote() << QStringLiteral("TestDriver: shown body %1: %2 at %3, volume %4, bounds %5 %6 %7 %8 %9 %10")
            .arg(prefix, uid, occurrence).arg(volume, 0, 'g', 17)
            .arg(values[1], 0, 'g', 17).arg(values[2], 0, 'g', 17).arg(values[3], 0, 'g', 17)
            .arg(values[4], 0, 'g', 17).arg(values[5], 0, 'g', 17).arg(values[6], 0, 'g', 17);
        return true;
      }
      m_waitDetail = QStringLiteral("no finite displayed shape and world bounds for %1 at %2").arg(uid, occurrence);
      return false;
    }, 6, QStringLiteral("cannot capture displayed geometry for %1 at %2").arg(uid, occurrence));
  } else if (name == QLatin1String("expect-shown-body")) {
    std::array<double, 7> expected{};
    bool valid = a.size() == 3 || a.size() == 9;
    for (int i = 2; valid && i < a.size(); ++i) valid = toNumber(a.at(i), expected[static_cast<std::size_t>(i - 2)]);
    if (!valid || expected[0] < 0) {
      fail(line, QStringLiteral("expect-shown-body <uid> <occurrence|-> <volume> [min-x min-y min-z max-x max-y max-z]"));
      return;
    }
    const QString uid = a.at(0);
    const QString occurrence = a.at(1) == QLatin1String("-") ? QString() : a.at(1);
    const bool bounds = a.size() == 9;
    whenReady([this, uid, occurrence, expected, bounds] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const auto close = [](double actual, double wanted) {
        return std::isfinite(actual) && std::abs(actual - wanted) <= std::max(1e-5, std::abs(wanted) * 1e-6);
      };
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || QString::fromStdString(body.occurrence) != occurrence) continue;
        if (!body.shape) break;
        const double volume = geometry::volume(*body.shape);
        m_waitDetail = QStringLiteral("shown %1 at %2: volume %3 mm3").arg(uid, occurrence).arg(volume, 0, 'g', 12);
        if (!close(volume, expected[0])) return false;
        if (!bounds) return true;
        const geometry::Shape world(body.shape->occt().Moved(body.placement));
        const auto box = geometry::bounding_box(world);
        if (box.empty) { m_waitDetail += QStringLiteral(", empty bounds"); return false; }
        const std::array<double, 6> actual{box.min.X(), box.min.Y(), box.min.Z(), box.max.X(), box.max.Y(), box.max.Z()};
        for (std::size_t i = 0; i < actual.size(); ++i) {
          if (!close(actual[i], expected[i + 1])) {
            m_waitDetail += QStringLiteral(", bound %1: %2 instead of %3").arg(i).arg(actual[i]).arg(expected[i + 1]);
            return false;
          }
        }
        return true;
      }
      m_waitDetail = QStringLiteral("no displayed shape for %1 at %2").arg(uid, occurrence);
      return false;
    }, 6, QStringLiteral("displayed geometry differs for %1 at %2").arg(uid, occurrence));
  } else if (name == QLatin1String("expect-shown-body-meshes")) {
    bool counted = false;
    const int wanted = a.at(2).toInt(&counted);
    if (!counted || wanted < 0) {
      fail(line, QStringLiteral("expect-shown-body-meshes <uid> <occurrence|-> <faces>"));
      return;
    }
    const QString uid = a.at(0);
    const QString occurrence = a.at(1) == QLatin1String("-") ? QString() : a.at(1);
    // The shape the view draws for the body (its stand-in while a preview
    // shows one): each face with a triangulation that lies on it where the
    // view places it (mitcad#1243: a face given another face's mesh in the
    // wrong frame was drawn and picked elsewhere).
    const QString where = occurrence.isEmpty() ? uid : QStringLiteral("%1 in %2").arg(uid, occurrence);
    whenReady([this, uid, occurrence, where, wanted] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || QString::fromStdString(body.occurrence) != occurrence) continue;
        const std::shared_ptr<geometry::Shape> drawn = displayShape(body.shape);
        if (!drawn) break;
        int meshed = 0;
        QStringList missing;
        for (int i = 0; i < drawn->face_count(); ++i) {
          if (io::triangulation_on_face(drawn->face(i))) {
            ++meshed;
          } else {
            missing << (drawn->face_names(i).empty() ? QStringLiteral("face %1").arg(i)
                                                     : QString::fromStdString(drawn->face_names(i).front()));
          }
        }
        const QString counts =
            QStringLiteral("%1 of %2 faces meshed on their faces").arg(meshed).arg(drawn->face_count());
        m_waitDetail = QStringLiteral("shown body %1: %2").arg(where, counts);
        if (!missing.isEmpty()) m_waitDetail += QStringLiteral("; not: ") + missing.join(QStringLiteral(", "));
        if (drawn->face_count() != wanted || !missing.isEmpty()) return false;
        qInfo().noquote() << QStringLiteral("TestDriver: shown body %1: %2").arg(where, counts);
        return true;
      }
      m_waitDetail = QStringLiteral("no displayed shape for %1").arg(where);
      return false;
    }, 6, QStringLiteral("display meshes differ for %1").arg(where));
  } else if (name == QLatin1String("expect-body-edges-unmeshed")) {
    // The body's own edges (not a stand-in's) without a 3D polygon: a job's
    // display meshing gives a solid's edges polygons on their faces' meshes
    // only, and a 3D polygon is what the view's presentations wrote when
    // they meshed an edge by itself (an edge highlight), on the UI thread,
    // while the model's threads read the edge (mitcad#1553).
    const QString uid = a.at(0);
    whenReady([this, uid] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (QString::fromStdString(body.uid) != uid || !body.shape) continue;
        NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
        TopExp::MapShapes(body.shape->occt(), TopAbs_EDGE, edges);
        int polygons = 0;
        for (int i = 1; i <= edges.Extent(); ++i) {
          TopLoc_Location location;
          if (!BRep_Tool::Polygon3D(TopoDS::Edge(edges(i)), location).IsNull()) ++polygons;
        }
        const QString counts = QStringLiteral("%1 of %2 edges with a 3D polygon").arg(polygons).arg(edges.Extent());
        m_waitDetail = QStringLiteral("body %1: %2").arg(uid, counts);
        if (polygons != 0) return false;
        qInfo().noquote() << QStringLiteral("TestDriver: body %1: %2").arg(uid, counts);
        return true;
      }
      m_waitDetail = QStringLiteral("no displayed shape for %1").arg(uid);
      return false;
    }, 6, QStringLiteral("the view meshed edges of %1").arg(uid));
  } else if (name == QLatin1String("expect-view-own-shapes")) {
    // The sketch profiles, sketch entities and datum planes the view shows
    // are drawn from shapes of its own: their presentations mesh them on
    // the UI thread, so none of their faces, edges or vertices may be one
    // of the shown bodies', which the model's threads read meanwhile
    // (mitcad#1586, as mitcad#1553 for the bodies' own presentations).
    whenReady([this] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const auto subShapes = [](const TopoDS_Shape& shape) {
        std::vector<const TopoDS_TShape*> found;
        if (shape.IsNull()) return found;
        for (const TopAbs_ShapeEnum kind : {TopAbs_FACE, TopAbs_EDGE, TopAbs_VERTEX}) {
          for (TopExp_Explorer it(shape, kind); it.More(); it.Next()) {
            found.push_back(it.Current().TShape().get());
          }
        }
        return found;
      };
      std::unordered_set<const TopoDS_TShape*> bodyParts;
      for (const BodyDisplay& body : window->viewer().shownBodies()) {
        if (!body.shape) continue;
        for (const TopoDS_TShape* part : subShapes(body.shape->occt())) bodyParts.insert(part);
      }
      const OcctViewer::OwnShapes own = window->viewer().ownShapes();
      int shared = 0;
      const auto countShared = [&subShapes, &bodyParts, &shared](const std::vector<TopoDS_Shape>& shapes) {
        for (const TopoDS_Shape& shape : shapes) {
          for (const TopoDS_TShape* part : subShapes(shape)) {
            if (bodyParts.count(part) != 0) ++shared;
          }
        }
      };
      countShared(own.profiles);
      countShared(own.sketchEntities);
      countShared(own.datums);
      int planes = 0;
      for (const TopoDS_Shape& datum : own.datums) {
        if (!datum.IsNull() && TopExp_Explorer(datum, TopAbs_FACE).More()) ++planes;
      }
      const QString counts =
          QStringLiteral("%1 profiles, %2 sketch entities and %3 datum planes shown, %4 of the bodies' %5 faces, edges and vertices shared")
              .arg(static_cast<int>(own.profiles.size()))
              .arg(static_cast<int>(own.sketchEntities.size()))
              .arg(planes)
              .arg(shared)
              .arg(static_cast<int>(bodyParts.size()));
      m_waitDetail = counts;
      if (own.profiles.empty() || own.sketchEntities.empty() || planes == 0 || shared != 0) return false;
      qInfo().noquote() << QStringLiteral("TestDriver: view's own shapes: %1").arg(counts);
      return true;
    }, 6, QStringLiteral("the view shows no profile, sketch entity or datum plane, or shares a body's shape"));
  } else if (name == QLatin1String("expect-view-origin")) {
    // expect-view-origin shown|hidden: the design's origin as the view
    // draws it (mitcad#1642): shown, its three planes (their squares
    // meshed), three axes and point displayed with something in their
    // presentations, and the planes' colour in the view's own frame;
    // hidden, none of them and next to none of the colour. The frame is
    // read outside the orientation cube, whose axes are coloured too.
    const bool shown = a.at(0) == QLatin1String("shown");
    if (!shown && a.at(0) != QLatin1String("hidden")) {
      fail(line, QStringLiteral("expect-view-origin shown|hidden"));
      return;
    }
    whenReady(
        [this, shown] {
          auto* window = qobject_cast<MainWindow*>(m_main.data());
          if (!window || window->modelBusy()) return false;
          OcctViewer& view = window->viewer();
          const OcctViewer::OriginDrawing drawing = view.originDrawing();
          view.settleCamera();
          const QImage frame = view.grabFramebuffer();
          if (frame.isNull()) {
            m_waitDetail = QStringLiteral("no frame");
            return false;
          }
          const qreal ratio = view.devicePixelRatioF();
          const QRect cube = view.overlayCubeArea();
          const QRect cubePixels((QPointF(cube.topLeft()) * ratio).toPoint(), (QSizeF(cube.size()) * ratio).toSize());
          // The planes' orange over the grey background: red well above blue.
          long long tinted = 0;
          long long counted = 0;
          for (int y = 0; y < frame.height(); ++y) {
            for (int x = 0; x < frame.width(); ++x) {
              if (cubePixels.contains(x, y)) continue;
              ++counted;
              const QColor c = frame.pixelColor(x, y);
              tinted += c.red() - c.blue() >= 12 ? 1 : 0;
            }
          }
          const double share = counted > 0 ? 100.0 * static_cast<double>(tinted) / static_cast<double>(counted) : 0.0;
          const auto names = [](const QStringList& list) {
            return list.isEmpty() ? QStringLiteral("none") : list.join(QStringLiteral(", "));
          };
          m_waitDetail = QStringLiteral("planes %1; axes %2; point %3; %4 % of the frame in the planes' colour")
                             .arg(names(drawing.planes), names(drawing.axes), names(drawing.points))
                             .arg(share, 0, 'f', 2);
          if (shown) {
            // At least 0.15 %: the default origin's three 80 px squares
            // (mitcad#1748) take about half a percent of a 1280 x 800
            // window, the 100 mm Centered ones of a new design more.
            return drawing.planes.size() == 3 && drawing.axes.size() == 3 && drawing.points.size() == 1 && share >= 0.15;
          }
          return drawing.empty() && share < 0.05;
        },
        6, shown ? QStringLiteral("the view does not draw the design's origin")
                 : QStringLiteral("the view still draws the design's origin"),
        [this] { qDebug().noquote() << QStringLiteral("TestDriver: view origin: %1").arg(m_waitDetail); });
  } else if (name == QLatin1String("expect-view-edges")) {
    // expect-view-edges <body> <segments a quarter turn> [highlighted
    // edges]: the body's lines are drawn from detached copies of its edges
    // (mitcad#1633), its circles and arcs cut in at least that many
    // segments a quarter turn, none sagging more than twice the view's
    // line deviation; and the highlights shown that have its edges (at
    // least the number given) draw them with its own copies, on the same
    // polylines as the body.
    const QString uid = a.at(0);
    double perQuarter = 0;
    double highlighted = 0;
    if (!toNumber(a.at(1), perQuarter) || (a.size() > 2 && !toNumber(a.at(2), highlighted))) {
      fail(line, QStringLiteral("expect-view-edges <body> <segments a quarter turn> [highlighted edges]"));
      return;
    }
    whenReady([this, uid, perQuarter, highlighted] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const OcctViewer::EdgeDrawing drawing = window->viewer().edgeDrawing(uid.toStdString());
      if (!drawing.shown) {
        m_waitDetail = QStringLiteral("body %1 is not drawn from copies of its edges").arg(uid);
        return false;
      }
      const QString counts =
          QStringLiteral("body %1: %2 edges drawn from copies, %3 circles at least %4 segments a quarter turn, "
                         "sag at most %5 mm; %6 highlighted edges, %7 on the body's polylines")
              .arg(uid)
              .arg(drawing.edges)
              .arg(drawing.circles)
              .arg(drawing.fewestPerQuarter, 0, 'f', 1)
              .arg(drawing.largestSag, 0, 'g', 3)
              .arg(drawing.highlightEdges)
              .arg(drawing.highlightShared);
      m_waitDetail = counts;
      if (drawing.edges == 0 || (drawing.circles > 0 && drawing.fewestPerQuarter + 1e-9 < perQuarter) ||
          drawing.largestSag > 2.0 * kViewEdgeDeviation + 1e-6 || drawing.highlightEdges < qRound(highlighted) ||
          drawing.highlightShared != drawing.highlightEdges) {
        return false;
      }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(counts);
      return true;
    }, 6, QStringLiteral("the lines of %1 are not drawn finely from its copies").arg(uid));
  } else if (name == QLatin1String("expect-view-seams")) {
    // expect-view-seams <body> <drawn edges> <seams left out>: the view
    // draws that many of the body's edges and leaves that many out as
    // smooth seams (mitcad#1773, ViewEdges::seamCount); each count exact,
    // or at least the number with a "+" after it ("20+").
    const QString uid = a.at(0);
    const auto count = [](const QString& text, int& value, bool& atLeast) {
      atLeast = text.endsWith(QLatin1Char('+'));
      bool ok = false;
      value = (atLeast ? text.chopped(1) : text).toInt(&ok);
      return ok && value >= 0;
    };
    int drawn = 0;
    int seams = 0;
    bool drawnAtLeast = false;
    bool seamsAtLeast = false;
    if (!count(a.at(1), drawn, drawnAtLeast) || !count(a.at(2), seams, seamsAtLeast)) {
      fail(line, QStringLiteral("expect-view-seams <body> <drawn edges>[+] <seams left out>[+]"));
      return;
    }
    whenReady([this, uid, drawn, seams, drawnAtLeast, seamsAtLeast] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window || window->modelBusy()) return false;
      const OcctViewer::EdgeDrawing drawing = window->viewer().edgeDrawing(uid.toStdString());
      if (!drawing.shown) {
        m_waitDetail = QStringLiteral("body %1 is not drawn from copies of its edges").arg(uid);
        return false;
      }
      const QString counts = QStringLiteral("body %1: %2 of %3 edges drawn, smooth seams left out: %4")
                                 .arg(uid)
                                 .arg(drawing.drawnEdges)
                                 .arg(drawing.drawnEdges + drawing.seams)
                                 .arg(drawing.seams);
      m_waitDetail = counts;
      const bool drawnOk = drawnAtLeast ? drawing.drawnEdges >= drawn : drawing.drawnEdges == drawn;
      const bool seamsOk = seamsAtLeast ? drawing.seams >= seams : drawing.seams == seams;
      if (!drawnOk || !seamsOk) {
        return false;
      }
      qInfo().noquote() << QStringLiteral("TestDriver: %1").arg(counts);
      return true;
    }, 6, QStringLiteral("the view does not draw the expected edges of %1").arg(uid));
  } else if (name == QLatin1String("test-body-highlights")) {
    const QString style = a.at(0);
    if (style != QLatin1String("none") && style != QLatin1String("default") &&
        style != QLatin1String("opaque") && style != QLatin1String("no-boundaries")) {
      fail(line, QStringLiteral("test-body-highlights none|default|opaque|no-boundaries"));
      return;
    }
    queue([this, style, line] {
      auto* window = qobject_cast<MainWindow*>(m_main.data());
      if (!window) { fail(line, QStringLiteral("no document window")); return; }
      const auto bodies = window->viewer().shownBodies();
      if (bodies.size() != 15) {
        fail(line, QStringLiteral("whole-body diagnostic requires exactly 15 displayed bodies")); return;
      }
      // A fixed navigation contract in this test process only: identical
      // middle-button orbit and immediate camera resets in every variant.
      NavigationSettings navigation = window->viewer().navigation();
      navigation.scheme = NavigationScheme::MiddleOrbit;
      navigation.animate = false;
      window->viewer().setNavigation(navigation);
      std::vector<HighlightDisplay> highlights;
      if (style != QLatin1String("none")) {
        for (const BodyDisplay& body : bodies) {
          SelectionItem item;
          item.kind = SelectKind::Body;
          item.owner = QString::fromStdString(body.uid);
          item.occurrence = QString::fromStdString(body.occurrence);
          const TopoDS_Shape shape = window->itemShape(item);
          if (shape.IsNull()) { fail(line, QStringLiteral("a displayed body has no highlight shape")); return; }
          HighlightDisplay highlight{item, shape, viewColor(ColorRole::Select, false)};
          highlight.opaqueWholeBody = style == QLatin1String("opaque");
          highlight.wholeBodyBoundaries = style != QLatin1String("no-boundaries");
          highlights.push_back(std::move(highlight));
        }
      }
      window->viewer().setHighlights(highlights);
      qInfo().noquote() << QStringLiteral("Body highlight diagnostic: %1, 15 bodies, %2 overlays")
                              .arg(style).arg(highlights.size());
    });
  } else if (name == QLatin1String("expect-view-buttons")) {
    const QHash<QString, Aspect_VKeyMouse> buttons = {
      {QStringLiteral("none"), Aspect_VKeyMouse_NONE},
      {QStringLiteral("left"), Aspect_VKeyMouse_LeftButton},
      {QStringLiteral("right"), Aspect_VKeyMouse_RightButton},
      {QStringLiteral("middle"), Aspect_VKeyMouse_MiddleButton}};
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view || !buttons.contains(a.at(0))) {
      fail(line, QStringLiteral("expect-view-buttons none|left|right|middle requires an OCCT view")); return;
    }
    if (view->PressedMouseButtons() != buttons.value(a.at(0))) {
      fail(line, QStringLiteral("OCCT retains mouse buttons %1 instead of %2")
          .arg(int(view->PressedMouseButtons())).arg(a.at(0)));
    }
  } else if (name == QLatin1String("preview-back-faces")) {
    // preview-back-faces cull|draw: see-through previews drawn as the fix
    // draws them, or double-sided as before it (mitcad#1575), so that a
    // test shows its expect-view-stepped catches the old look.
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (view == nullptr || (a.at(0) != QLatin1String("cull") && a.at(0) != QLatin1String("draw"))) {
      fail(line, QStringLiteral("preview-back-faces cull|draw requires a 3D view"));
      return;
    }
    view->setSeeThroughCulling(a.at(0) == QLatin1String("cull"));
  }
}

} // namespace mitcad
