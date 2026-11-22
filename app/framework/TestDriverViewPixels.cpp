// SPDX-License-Identifier: MIT
// Test steps for the 3D view's picture: images, pixels and their colours, line widths,
// smooth or stepped shading, the background and the viewport's framebuffer.
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

// The width in pixels of the line drawn through a pixel of a frame
// (view-line-width, mitcad#2063). The line's colour is the pixel near it
// most unlike the frame `reach` pixels away; each pixel across the line
// counts by how far its colour lies from the background on its side
// towards the line's (the multisampling's fringe counts in part). Across
// a line in four directions (the axes and the diagonals), the narrowest
// count is its width, at most 8 % too wide for a line between them.
double lineWidthAt(const QImage& frame, QPoint at, int reach, QPoint& centre, QColor& colour) {
  const auto rgb = [&frame](QPoint p) {
    const QPoint inside(std::clamp(p.x(), 0, frame.width() - 1), std::clamp(p.y(), 0, frame.height() - 1));
    const QColor c = frame.pixelColor(inside);
    return std::array<double, 3>{c.redF(), c.greenF(), c.blueF()};
  };
  const std::array<QPoint, 4> directions = {QPoint(1, 0), QPoint(0, 1), QPoint(1, 1), QPoint(1, -1)};
  std::array<double, 3> farAway = {0.0, 0.0, 0.0};
  for (const QPoint& direction : directions) {
    for (const int side : {-1, 1}) {
      const std::array<double, 3> c = rgb(at + direction * (side * reach));
      for (int i = 0; i < 3; ++i) {
        farAway[i] += c[i] / 8.0;
      }
    }
  }
  const auto distance = [](const std::array<double, 3>& a, const std::array<double, 3>& b) {
    return std::abs(a[0] - b[0]) + std::abs(a[1] - b[1]) + std::abs(a[2] - b[2]);
  };
  centre = at;
  double unlike = -1.0;
  for (int dy = -2; dy <= 2; ++dy) {
    for (int dx = -2; dx <= 2; ++dx) {
      const double d = distance(rgb(at + QPoint(dx, dy)), farAway);
      if (d > unlike) {
        unlike = d;
        centre = at + QPoint(dx, dy);
      }
    }
  }
  const std::array<double, 3> line = rgb(centre);
  colour = frame.pixelColor(centre);
  double width = std::numeric_limits<double>::max();
  for (const QPoint& direction : directions) {
    double count = 1.0;
    bool along = false;
    for (const int side : {-1, 1}) {
      const std::array<double, 3> background = rgb(centre + direction * (side * reach));
      double span = 0.0;
      for (int i = 0; i < 3; ++i) {
        span += (line[i] - background[i]) * (line[i] - background[i]);
      }
      if (span < 1e-4) {
        // The line's own colour that far away: this direction runs along
        // the line, not across it (a line of shallow slope crossed
        // horizontally), and says nothing of its width.
        along = true;
        break;
      }
      for (int step = 1; step < reach; ++step) {
        const std::array<double, 3> c = rgb(centre + direction * (side * step));
        double toward = 0.0;
        for (int i = 0; i < 3; ++i) {
          toward += (c[i] - background[i]) * (line[i] - background[i]);
        }
        const double cover = std::clamp(toward / span, 0.0, 1.0);
        if (cover < 0.05) {
          break;
        }
        count += cover;
      }
    }
    if (along) {
      continue;
    }
    const double length = (direction.x() != 0 && direction.y() != 0) ? std::sqrt(2.0) : 1.0;
    width = std::min(width, count * length);
  }
  return width == std::numeric_limits<double>::max() ? 0.0 : width;
}

const TestDriver::StepSpec kStepSpecs[] = {
    {"capture-view-image", 1, 1, TestDriver::kIdle},
    {"expect-view-image", 2, 2, TestDriver::kIdle},
    {"capture-view-target", 1, 1, TestDriver::kIdle},
    {"capture-view-pixel", 2, 2, TestDriver::kIdle},
    {"expect-view-pixel", 3, 3, TestDriver::kIdle},
    // The framebuffer's colour where a model point is (mitcad#2069).
    {"capture-view-point-pixel", 2, 2, TestDriver::kIdle},
    {"expect-view-point-pixel", 2, 4, TestDriver::kIdle},
    // A line's width in the view's own frame (mitcad#2063).
    {"view-line-width", 3, 4, TestDriver::kIdle},
    // A see-through preview without a pattern of its triangles (mitcad#1575).
    {"expect-view-smooth", 1, 1, TestDriver::kIdle},
    {"expect-view-stepped", 1, 1, TestDriver::kIdle},
    {"expect-viewport", 0, 0, TestDriver::kIdle},
    // The 3D view's background, top and bottom: `dark` or `light` (Follow
    // System's two gradients) or two colours (mitcad#2145).
    {"expect-view-background", 1, 2, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kViewPixelSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runViewPixelStep};

void TestDriver::runViewPixelStep(const QString& name, const QStringList& a, int line) {
  // A regular expression argument.
  const auto regex = [&](const QString& pattern, QRegularExpression& re) {
    re = QRegularExpression(pattern);
    if (!re.isValid()) {
      fail(line, QStringLiteral("bad regular expression '%1': %2").arg(pattern, re.errorString()));
      return false;
    }
    return true;
  };
  if (name == QLatin1String("capture-view-image") || name == QLatin1String("expect-view-image")) {
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view) { fail(line, QStringLiteral("OCCT viewport missing")); return; }
    // Screenshot assertions compare a settled camera, not an intermediate
    // Fit All or animated view frame that may finish while the tooltip opens.
    view->settleCamera();
    if (name == QLatin1String("capture-view-image")) {
      QDir().mkpath(QFileInfo(a.at(0)).absolutePath());
      if (!view->grabFramebuffer().save(a.at(0))) fail(line, QStringLiteral("cannot save viewport image"));
      return;
    }
    const QImage original(a.at(0));
    const bool changed = a.at(1) == QStringLiteral("changed");
    if (original.isNull() || (!changed && a.at(1) != QStringLiteral("same"))) {
      fail(line, QStringLiteral("expect-view-image requires an image and same|changed")); return;
    }
    whenReady([this, view, original, changed] {
      const QImage current = view->grabFramebuffer();
      if (original.size() != current.size()) { m_waitDetail = QStringLiteral("viewport size changed"); return false; }
      int pixels = 0;
      for (int y = 0; y < current.height(); ++y) for (int x = 0; x < current.width(); ++x) {
        const QColor a = original.pixelColor(x, y), b = current.pixelColor(x, y);
        if (std::abs(a.red() - b.red()) + std::abs(a.green() - b.green()) + std::abs(a.blue() - b.blue()) > 30) ++pixels;
      }
      m_waitDetail = QStringLiteral("viewport changed pixels: %1").arg(pixels);
      return changed ? pixels >= 50 : pixels <= 10;
    }, 6, QStringLiteral("hidden feature framebuffer indication"));
  } else if (name == QLatin1String("expect-viewport")) {
    whenReady([this] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (!view || !view->isValid() || view->glVersion().isEmpty()) return false;
      const QImage image = view->grabFramebuffer();
      const qreal scale = view->devicePixelRatioF();
      const bool retina = qEnvironmentVariableIntValue("MITCAD_UI_REQUIRE_RETINA") != 0;
      m_waitDetail = QStringLiteral("viewport logical %1x%2, framebuffer %3x%4, scale %5")
          .arg(view->width()).arg(view->height()).arg(image.width()).arg(image.height()).arg(scale);
      if (image.isNull() || image.width() < 300 || image.height() < 200 ||
          std::abs(image.width() - view->width() * scale) > 1 ||
          std::abs(image.height() - view->height() * scale) > 1 || (retina && scale < 1.9)) return false;
      const QScreen* screen = view->screen();
      qDebug().noquote() << QStringLiteral("TestDriver: %1; screen %2x%3 points, refresh %4 Hz; OpenGL %5")
          .arg(m_waitDetail).arg(screen ? screen->geometry().width() : 0)
          .arg(screen ? screen->geometry().height() : 0).arg(screen ? screen->refreshRate() : 0)
          .arg(view->glVersion());
      return true;
    }, 10, QStringLiteral("the viewport framebuffer does not match its actual backing scale"));
  } else if (name == QLatin1String("capture-view-target")) {
    auto* view = qobject_cast<OcctViewer*>(viewer());
    if (!view || !view->isValid()) { fail(line, QStringLiteral("capture-view-target requires an initialized viewport")); return; }
    const gp_Pnt target = view->camera().target;
    m_vars[a.at(0)] = QStringLiteral("%1,%2,%3").arg(target.X(), 0, 'g', 17)
        .arg(target.Y(), 0, 'g', 17).arg(target.Z(), 0, 'g', 17);
  } else if (name == QLatin1String("capture-view-pixel") || name == QLatin1String("expect-view-pixel")) {
    // The view's own frame (without the widgets over it, such as the
    // annotations' overlay) at a logged pick place (mitcad#1166): its
    // colour into a variable, or compared with one.
    const bool capture = name == QLatin1String("capture-view-pixel");
    QRegularExpression re;
    if (!regex(capture ? a.at(1) : a.at(0), re)) {
      return;
    }
    QColor wanted;
    int tolerance = 0;
    if (!capture) {
      wanted = QColor(a.at(1));
      bool number = false;
      tolerance = a.at(2).toInt(&number);
      if (!wanted.isValid() || !number || tolerance < 0) {
        fail(line, QStringLiteral("expect-view-pixel <regex> <#rrggbb> <tolerance>"));
        return;
      }
    }
    const QString variable = capture ? a.at(0) : QString();
    const auto seen = std::make_shared<QColor>();
    whenReady(
        [this, re, seen, capture, wanted, tolerance] {
          auto* view = qobject_cast<OcctViewer*>(viewer());
          if (view == nullptr || m_main.isNull()) {
            m_waitDetail = QStringLiteral("no 3D view");
            return false;
          }
          // The newest pick places only, as click-pick reads them.
          static const QRegularExpression places(QStringLiteral("^Pick places:"));
          QPoint at;
          if (!loggedPoint(re, std::max(findLastLine(places, 0), 0), at)) {
            m_waitDetail = QStringLiteral("no place logged");
            return false;
          }
          view->settleCamera();
          const QImage frame = view->grabFramebuffer();
          const QPoint pixel = (QPointF(view->mapFrom(topWindow(), at)) * view->devicePixelRatioF()).toPoint();
          if (frame.isNull() || !frame.rect().contains(pixel)) {
            m_waitDetail = QStringLiteral("%1,%2 is outside the view's frame").arg(pixel.x()).arg(pixel.y());
            return false;
          }
          *seen = frame.pixelColor(pixel);
          m_waitDetail = QStringLiteral("%1 at %2,%3").arg(seen->name()).arg(pixel.x()).arg(pixel.y());
          if (capture) {
            return true;
          }
          const int off = std::abs(seen->red() - wanted.red()) + std::abs(seen->green() - wanted.green()) +
                          std::abs(seen->blue() - wanted.blue());
          m_waitDetail += QStringLiteral(", wanted %1, off by %2").arg(wanted.name()).arg(off);
          return off <= tolerance;
        },
        6,
        capture ? QStringLiteral("no view pixel at the place of '%1'").arg(a.at(1))
                : QStringLiteral("the view's pixel at the place of '%1' is not %2").arg(a.at(0), a.at(1)),
        [this, variable, seen, capture] {
          if (capture) {
            m_vars.insert(variable, seen->name());
          }
          qDebug().noquote() << QStringLiteral("TestDriver: view pixel %1").arg(m_waitDetail);
        });
  } else if (name == QLatin1String("view-line-width")) {
    // The width of the line the view draws at a logged pick place or at a
    // sketch point, in the view's own frame (without the widgets over it):
    // at least <least> logical pixels (less half a pixel), and wider than
    // ${narrower} when given (mitcad#2063: lines 1 pixel wide whatever
    // their width in a core profile).
    double least = 0.0;
    if (!toNumber(a.at(1), least) || least <= 0.0) {
      fail(line, QStringLiteral("view-line-width <var> <least px> <place regex>|sketch:<x>,<y> [<narrower var>]"));
      return;
    }
    const QString place = a.at(2);
    QRegularExpression re;
    double sketchX = 0.0;
    double sketchY = 0.0;
    const bool inSketch = place.startsWith(QLatin1String("sketch:"));
    if (inSketch) {
      const QStringList xy = place.mid(7).split(QLatin1Char(','));
      if (xy.size() != 2 || !toNumber(xy.at(0), sketchX) || !toNumber(xy.at(1), sketchY)) {
        fail(line, QStringLiteral("'%1' is not sketch:<x>,<y>").arg(place));
        return;
      }
    } else if (!regex(QStringLiteral("^Pick (?:.*)(?:%1)").arg(place), re)) {
      // Matched against "Pick <kind> <owner>/<name>", the view's own pick
      // points as click-pick-offset finds them (no "Pick places" logged).
      return;
    }
    double narrower = 0.0;
    if (a.size() > 3) {
      if (!m_vars.contains(a.at(3)) || !toNumber(m_vars.value(a.at(3)), narrower)) {
        fail(line, QStringLiteral("no line width in ${%1}").arg(a.at(3)));
        return;
      }
    }
    const QString variable = a.at(0);
    const auto width = std::make_shared<double>(0.0);
    whenReady(
        [this, re, inSketch, sketchX, sketchY, least, narrower, width] {
          auto* view = qobject_cast<OcctViewer*>(viewer());
          if (view == nullptr || m_main.isNull()) {
            m_waitDetail = QStringLiteral("no 3D view");
            return false;
          }
          QPoint at; // in the view
          bool found = false;
          if (inSketch) {
            found = sketchPoint(sketchX, sketchY, at);
            at = view->mapFrom(topWindow(), at);
          } else {
            for (const auto& [item, point] : view->pickPoints(SelectKind::Face | SelectKind::Edge | SelectKind::Vertex)) {
              const QString description = QStringLiteral("Pick %1 %2/%3")
                                              .arg(SelectionItem::kindName(item.kind), item.owner, item.name);
              if (re.match(description).hasMatch()) {
                at = point;
                found = true;
              }
            }
          }
          if (!found) {
            m_waitDetail = inSketch ? QStringLiteral("no sketch view logged") : QStringLiteral("no pick point");
            return false;
          }
          view->settleCamera();
          const QImage frame = view->grabFramebuffer();
          const qreal ratio = view->devicePixelRatioF();
          const QPoint pixel = (QPointF(at) * ratio).toPoint();
          if (frame.isNull() || !frame.rect().contains(pixel)) {
            m_waitDetail = QStringLiteral("%1,%2 is outside the view's frame").arg(pixel.x()).arg(pixel.y());
            return false;
          }
          // Wider than the widest highlight (6 logical pixels) and its fringe.
          const int reach = static_cast<int>(std::ceil(10.0 * ratio));
          QPoint centre;
          QColor colour;
          *width = lineWidthAt(frame, pixel, reach, centre, colour);
          m_waitDetail = QStringLiteral("%1 px at %2,%3 in %4, ratio %5")
                             .arg(*width, 0, 'f', 1)
                             .arg(centre.x())
                             .arg(centre.y())
                             .arg(colour.name())
                             .arg(ratio);
          const double wanted = least * ratio - 0.5;
          if (narrower > 0.0) {
            m_waitDetail += QStringLiteral(", wider than %1 px").arg(narrower, 0, 'f', 1);
          }
          m_waitDetail += QStringLiteral(", at least %1 px wanted").arg(wanted, 0, 'f', 1);
          return *width >= wanted && (narrower <= 0.0 || *width > narrower + 0.5);
        },
        6, QStringLiteral("the view's line at '%1' is too narrow").arg(place),
        [this, variable, width] {
          m_vars.insert(variable, QString::number(*width, 'f', 2));
          qDebug().noquote() << QStringLiteral("TestDriver: view line width %1").arg(m_waitDetail);
        });
  } else if (name == QLatin1String("capture-view-point-pixel") ||
             name == QLatin1String("expect-view-point-pixel")) {
    // The view's own frame where a model point is on the screen
    // (mitcad#2069): its colour into a variable, or a test of its look:
    // blue-over-red (its blue channel above its red), or nearer <a> <b>
    // (nearer colour a than colour b, each #rrggbb or a view colour role's
    // name on a light background, e.g. PreviewRemove).
    const bool capture = name == QLatin1String("capture-view-point-pixel");
    const QStringList coordinates = a.at(capture ? 1 : 0).split(QLatin1Char(','));
    double xyz[3] = {0.0, 0.0, 0.0};
    bool parsed = coordinates.size() == 3;
    for (int i = 0; parsed && i < 3; ++i) {
      parsed = toNumber(coordinates.at(i).trimmed(), xyz[i]);
    }
    const auto colorOf = [](const QString& text) {
      for (const ColorRole role : allColorRoles()) {
        if (colorRoleName(role) == text) {
          return viewColor(role, false);
        }
      }
      return QColor(text);
    };
    const QString rule = capture ? QString() : a.at(1);
    QColor nearColor;
    QColor farColor;
    if (!capture && rule == QLatin1String("nearer") && a.size() == 4) {
      nearColor = colorOf(a.at(2));
      farColor = colorOf(a.at(3));
    }
    if (!parsed || (!capture && !(rule == QLatin1String("blue-over-red") && a.size() == 2) &&
                    !(rule == QLatin1String("nearer") && nearColor.isValid() && farColor.isValid()))) {
      fail(line, capture ? QStringLiteral("capture-view-point-pixel <var> <x,y,z>")
                         : QStringLiteral("expect-view-point-pixel <x,y,z> blue-over-red | nearer <color> <color>"));
      return;
    }
    const gp_Pnt point(xyz[0], xyz[1], xyz[2]);
    const QString variable = capture ? a.at(0) : QString();
    const auto seen = std::make_shared<QColor>();
    whenReady(
        [this, point, seen, capture, rule, nearColor, farColor] {
          auto* view = qobject_cast<OcctViewer*>(viewer());
          if (view == nullptr || m_main.isNull()) {
            m_waitDetail = QStringLiteral("no 3D view");
            return false;
          }
          view->settleCamera();
          const QImage frame = view->grabFramebuffer();
          const QPoint pixel = (QPointF(view->toWidget(point)) * view->devicePixelRatioF()).toPoint();
          if (frame.isNull() || !frame.rect().contains(pixel)) {
            m_waitDetail = QStringLiteral("%1,%2 is outside the view's frame").arg(pixel.x()).arg(pixel.y());
            return false;
          }
          *seen = frame.pixelColor(pixel);
          m_waitDetail = QStringLiteral("%1 at %2,%3").arg(seen->name()).arg(pixel.x()).arg(pixel.y());
          if (capture) {
            return true;
          }
          if (rule == QLatin1String("blue-over-red")) {
            m_waitDetail += QStringLiteral(", blue %1 over red %2").arg(seen->blue()).arg(seen->red());
            return seen->blue() > seen->red();
          }
          const auto off = [&seen](const QColor& to) {
            return std::abs(seen->red() - to.red()) + std::abs(seen->green() - to.green()) +
                   std::abs(seen->blue() - to.blue());
          };
          m_waitDetail += QStringLiteral(", off %1 from %2 and %3 from %4")
                              .arg(off(nearColor))
                              .arg(nearColor.name())
                              .arg(off(farColor))
                              .arg(farColor.name());
          return off(nearColor) < off(farColor);
        },
        6,
        capture ? QStringLiteral("no view pixel at %1").arg(a.at(1))
                : QStringLiteral("the view's pixel at %1 is not %2").arg(a.at(0), a.mid(1).join(QLatin1Char(' '))),
        [this, variable, seen, capture] {
          if (capture) {
            m_vars.insert(variable, seen->name());
          }
          qDebug().noquote() << QStringLiteral("TestDriver: view point pixel %1").arg(m_waitDetail);
        });
  } else if (name == QLatin1String("expect-view-smooth") || name == QLatin1String("expect-view-stepped")) {
    // expect-view-smooth <most %> | expect-view-stepped <least %>: a
    // command's see-through preview drawn without (with) a pattern of its
    // mesh's triangles (mitcad#1575, GitHub #219). In the view's own frame,
    // the middle of the preview's box on the screen (a square of 0.4 times
    // its smaller side, inside a sphere's outline from any side): the share
    // of pixels where the brightness steps, its second difference across or
    // down over kStep levels, among those where the frame without the
    // previews is flat. Shading varies smoothly, with no such steps; the
    // edges of triangles blended over a flat background have them. What
    // lies behind the preview (a picked plane's axis lines and outline, the
    // grid) steps in both frames: a pixel within kNear pixels of a step of
    // the frame without the previews is not counted, whatever direction
    // that line runs in.
    const bool smooth = name == QLatin1String("expect-view-smooth");
    double limit = 0.0;
    if (!toNumber(a.at(0), limit) || limit < 0.0) {
      fail(line, smooth ? QStringLiteral("expect-view-smooth <most stepped pixels, %>")
                        : QStringLiteral("expect-view-stepped <least stepped pixels, %>"));
      return;
    }
    whenReady(
        [this, limit, smooth] {
          constexpr double kStep = 4.0;
          constexpr double kBehindStep = 2.0;
          constexpr int kNear = 2;
          auto* view = qobject_cast<OcctViewer*>(viewer());
          if (view == nullptr) {
            m_waitDetail = QStringLiteral("no 3D view");
            return false;
          }
          view->settleCamera();
          const QRectF box = view->previewScreenBox();
          if (box.isEmpty()) {
            m_waitDetail = QStringLiteral("no preview shown");
            return false;
          }
          const QImage behind = view->grabWithoutPreviews().convertToFormat(QImage::Format_RGB32);
          const QImage frame = view->grabFramebuffer().convertToFormat(QImage::Format_RGB32);
          const qreal scale = view->devicePixelRatioF();
          const double half = 0.2 * std::min(box.width(), box.height()) * scale;
          const QPointF middle = box.center() * scale;
          const QRect area = QRectF(middle.x() - half, middle.y() - half, 2.0 * half, 2.0 * half)
                                 .toAlignedRect()
                                 .intersected(frame.rect().adjusted(kNear + 1, kNear + 1, -kNear - 1, -kNear - 1));
          if (frame.isNull() || behind.size() != frame.size() || area.width() < 40 || area.height() < 40) {
            m_waitDetail = QStringLiteral("the preview's middle is %1 x %2 pixels in the frame")
                               .arg(area.width())
                               .arg(area.height());
            return false;
          }
          const auto brightness = [](const QImage& image, int x, int y) {
            const QRgb c = image.pixel(x, y);
            return 0.299 * qRed(c) + 0.587 * qGreen(c) + 0.114 * qBlue(c);
          };
          const auto step = [&brightness](const QImage& image, int x, int y) {
            const double here = brightness(image, x, y);
            const double across = std::abs(brightness(image, x - 1, y) - 2.0 * here + brightness(image, x + 1, y));
            const double down = std::abs(brightness(image, x, y - 1) - 2.0 * here + brightness(image, x, y + 1));
            return std::max(across, down);
          };
          // The steps of what lies behind, kNear pixels wide on each side.
          const int width = area.width() + 2 * kNear;
          const int height = area.height() + 2 * kNear;
          std::vector<char> busy(static_cast<std::size_t>(width) * static_cast<std::size_t>(height), 0);
          for (int y = 0; y < height; ++y) {
            for (int x = 0; x < width; ++x) {
              if (step(behind, area.left() - kNear + x, area.top() - kNear + y) <= kBehindStep) {
                continue;
              }
              for (int dy = -kNear; dy <= kNear; ++dy) {
                for (int dx = -kNear; dx <= kNear; ++dx) {
                  const int nx = x + dx;
                  const int ny = y + dy;
                  if (nx >= 0 && ny >= 0 && nx < width && ny < height) {
                    busy[static_cast<std::size_t>(ny) * static_cast<std::size_t>(width) + static_cast<std::size_t>(nx)] = 1;
                  }
                }
              }
            }
          }
          long long stepped = 0;
          long long counted = 0;
          double sum = 0.0;
          for (int y = area.top(); y <= area.bottom(); ++y) {
            for (int x = area.left(); x <= area.right(); ++x) {
              const std::size_t at = static_cast<std::size_t>(y - area.top() + kNear) * static_cast<std::size_t>(width) +
                                     static_cast<std::size_t>(x - area.left() + kNear);
              if (busy[at] != 0) {
                continue;
              }
              stepped += step(frame, x, y) > kStep ? 1 : 0;
              ++counted;
              sum += brightness(frame, x, y);
            }
          }
          const long long all = static_cast<long long>(area.width()) * area.height();
          if (counted * 5 < all) {
            m_waitDetail = QStringLiteral("only %1 of %2 pixels have a flat background").arg(counted).arg(all);
            return false;
          }
          const double share = 100.0 * static_cast<double>(stepped) / static_cast<double>(counted);
          m_waitDetail = QStringLiteral("%1 % of %2 pixels stepped (%3 %4 %), brightness %5, "
                                        "%6 x %7 pixels at %8,%9")
                             .arg(share, 0, 'f', 3)
                             .arg(counted)
                             .arg(smooth ? QStringLiteral("most") : QStringLiteral("least"))
                             .arg(limit)
                             .arg(sum / static_cast<double>(counted), 0, 'f', 1)
                             .arg(area.width())
                             .arg(area.height())
                             .arg(area.left())
                             .arg(area.top());
          return smooth ? share <= limit : share >= limit;
        },
        6,
        smooth ? QStringLiteral("the see-through preview's brightness steps (a pattern of triangles)")
               : QStringLiteral("the see-through preview's brightness does not step (no pattern of triangles)"),
        [this, smooth] {
          qDebug().noquote() << QStringLiteral("TestDriver: view %1: %2")
                                    .arg(smooth ? QStringLiteral("smooth") : QStringLiteral("stepped"), m_waitDetail);
        });
  } else if (name == QLatin1String("expect-view-background")) {
    // The view's background as drawn now (mitcad#2145): `dark` and `light`
    // are Follow System's gradients (ViewSettings::automaticColors), else
    // two colours, top and bottom. Waits for a theme change still on its
    // way.
    QString top = a.at(0);
    QString bottom = a.size() > 1 ? a.at(1) : QString();
    if (a.size() == 1 && (top == QLatin1String("dark") || top == QLatin1String("light"))) {
      QColor upper;
      QColor lower;
      ViewSettings::automaticColors(top == QLatin1String("dark"), upper, lower);
      top = upper.name();
      bottom = lower.name();
    }
    if (!QColor::isValidColorName(top) || !QColor::isValidColorName(bottom)) {
      fail(line, QStringLiteral("expect-view-background dark|light|<top> <bottom>, not '%1'")
                     .arg(a.join(QLatin1Char(' '))));
      return;
    }
    const QString wanted = QStringLiteral("%1 %2").arg(QColor(top).name(), QColor(bottom).name());
    whenReady([this, wanted] {
      auto* view = qobject_cast<OcctViewer*>(viewer());
      if (view == nullptr) {
        m_waitDetail = QStringLiteral("no 3D view");
        return false;
      }
      QColor shownTop;
      QColor shownBottom;
      view->backgroundColors(shownTop, shownBottom);
      const QString shown = QStringLiteral("%1 %2").arg(shownTop.name(), shownBottom.name());
      m_waitDetail = QStringLiteral("the view's background is %1, the theme %2")
                         .arg(shown, isDarkTheme() ? QStringLiteral("dark") : QStringLiteral("light"));
      if (shown != wanted) {
        return false;
      }
      qDebug().noquote() << QStringLiteral("TestDriver: view Background %1").arg(shown);
      return true;
    }, 6, QStringLiteral("the view's background is not %1").arg(wanted));
  }
}

} // namespace mitcad
