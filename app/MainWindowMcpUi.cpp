// SPDX-License-Identifier: MIT
// Live MCP's view of the application and its actions in it (mitcad#265):
// ui_state, ui_log, ui_screenshot, ui_select and ui_command. What the user
// can see and do in the window, an agent can read and do, with the same
// refusals and undo.
#include "MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

#include <QApplication>
#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QEventLoop>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QPainter>
#include <QPixmap>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QRect>
#include <QRectF>
#include <QRegion>
#include <QScopedValueRollback>
#include <QStackedWidget>
#include <QScreen>
#include <QSizeF>
#include <QSysInfo>
#include <QTemporaryDir>
#include <QTimer>
#include <QWidget>
#include <QDebug>
#include <QtLogging>

#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <Precision.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "framework/ChromeStyle.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/ResultCache.hpp"
#include "mcp/AgentFolders.hpp"
#include "mcp/EndpointLimits.hpp"
#include "mcp/LiveErrors.hpp"
#include "mcp/LogRing.hpp"
#include "mcp/ModalGuard.hpp"
#include "mcp/ScreenshotBudget.hpp"
#include "mcp/UiCommandList.hpp"
#include "platform/MacChrome.hpp"
#include "sketch/Sketch3dController.hpp"
#include "sketch/Sketch3dPalette.hpp"
#include "sketch/SketchPalette.hpp"
#include "sketch/SketchTool.hpp"
#include "view/ViewReveal.hpp"
#include "view/VersionWatermark.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeForms.hpp"
#endif

namespace mitcad {
namespace {

constexpr double kPi = 3.14159265358979323846;

// The selection's kinds by the names MCP clients use.
const std::pair<SelectKind, const char*> kKinds[] = {
    {SelectKind::Face, "face"},
    {SelectKind::Edge, "edge"},
    {SelectKind::Vertex, "vertex"},
    {SelectKind::Body, "body"},
    {SelectKind::Profile, "profile"},
    {SelectKind::SketchCurve, "sketch_curve"},
    {SelectKind::SketchPoint, "sketch_point"},
    {SelectKind::SketchConstraint, "sketch_constraint"},
    {SelectKind::SketchDimension, "sketch_dimension"},
    {SelectKind::Plane, "plane"},
    {SelectKind::Axis, "axis"},
    {SelectKind::Point, "point"},
    {SelectKind::Component, "component"},
    {SelectKind::Sketch, "sketch"},
    {SelectKind::Feature, "feature"},
};

QString kindKey(SelectKind kind) {
  for (const auto& [value, key] : kKinds) {
    if (value == kind) {
      return QString::fromLatin1(key);
    }
  }
  return QStringLiteral("none");
}

QJsonObject itemJson(const SelectionItem& item) {
  return {{QStringLiteral("kind"), kindKey(item.kind)},
          {QStringLiteral("owner"), item.owner},
          {QStringLiteral("name"), item.name},
          {QStringLiteral("occurrence"), item.occurrence},
          {QStringLiteral("geometry"), item.geometry},
          {QStringLiteral("reference"), item.reference()},
          {QStringLiteral("text"), item.logDescription()}};
}

QJsonArray selectionJson(const Selection& items) {
  QJsonArray list;
  for (const SelectionItem& item : items) {
    list.append(itemJson(item));
  }
  return list;
}

// An entity as ui_select and ui_command's selection inputs take it, with
// the surface or curve type the view gives faces and edges (the inputs'
// filters ask), or none with the reason.
std::optional<SelectionItem> readItem(const QJsonValue& value, const CommandContext& model, QString& problem) {
  const QJsonObject entry = value.toObject();
  SelectionItem item;
  const QString kind = entry.value(QStringLiteral("kind")).toString();
  for (const auto& [known, key] : kKinds) {
    if (kind == QLatin1String(key)) {
      item.kind = known;
    }
  }
  item.owner = entry.value(QStringLiteral("owner")).toString();
  item.name = entry.value(QStringLiteral("name")).toString();
  item.occurrence = entry.value(QStringLiteral("occurrence")).toString();
  item.geometry = entry.value(QStringLiteral("geometry")).toString();
  // ui_select's limits (core/mcp catalog.rs) also for ui_command's
  // selections, which its schema does not check; the refusal repeats only
  // the entry's start (mitcad#935).
  const bool tooLong = item.owner.size() > 1024 || item.name.size() > 4096 || item.occurrence.size() > 1024 ||
                       item.geometry.size() > 1024;
  if (!item.isValid() || item.owner.isEmpty() || tooLong) {
    problem = QStringLiteral("not an entity: %1")
                  .arg(mcp::echoed(QString::fromUtf8(QJsonDocument(entry).toJson(QJsonDocument::Compact))));
    return std::nullopt;
  }
  if (item.geometry.isEmpty() && (item.kind == SelectKind::Face || item.kind == SelectKind::Edge)) {
    try {
      const bool face = item.kind == SelectKind::Face;
      const QJsonArray entries =
          model
              .query({{QStringLiteral("query"), face ? QStringLiteral("faces") : QStringLiteral("edges")},
                      {QStringLiteral("body"), item.owner}})
              .toArray();
      for (const QJsonValue& known : entries) {
        const QJsonObject e = known.toObject();
        const bool same = face ? e.value(QStringLiteral("names")).toArray().contains(QJsonValue(item.name))
                               : e.value(QStringLiteral("name")).toString() == item.name;
        if (same) {
          item.geometry = e.value(face ? QStringLiteral("surface") : QStringLiteral("curve")).toString();
        }
      }
    } catch (const std::exception&) {
      // The input's own test decides without the type.
    }
  }
  return item;
}

QString modeName(DocumentSession::Mode mode) {
  switch (mode) {
  case DocumentSession::Mode::Idle:
    return QStringLiteral("idle");
  case DocumentSession::Mode::PickPlane:
    return QStringLiteral("pick_plane");
  case DocumentSession::Mode::Sketch:
    return QStringLiteral("sketch");
  case DocumentSession::Mode::Command:
    return QStringLiteral("command");
  }
  return QStringLiteral("idle");
}

[[noreturn]] void refuse(const QString& reason) {
  // A refusal can repeat a client's text: its log line keeps the start
  // (mitcad#935).
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw std::runtime_error(reason.toStdString());
}

[[noreturn]] void refuseWith(const QString& reason, const QJsonObject& fields) {
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw mcp::Refusal(reason, fields);
}

// Runs the event loop until `done` holds (true) or the time is up (false):
// a native command panel starts and finishes on the model's worker, a
// preview after its delay.
bool waitUntil(const std::function<bool()>& done, int milliseconds) {
  QElapsedTimer clock;
  clock.start();
  while (!done()) {
    if (clock.elapsed() > milliseconds) {
      return false;
    }
    QEventLoop loop;
    QTimer::singleShot(10, &loop, &QEventLoop::quit);
    loop.exec();
  }
  return true;
}

// The log lines since a sequence number, as text.
QJsonArray logTexts(quint64 since, int limit) {
  QJsonArray lines;
  for (const mcp::LogLine& line : mcp::logLines(since, limit)) {
    lines.append(line.text);
  }
  return lines;
}

quint64 logNext() {
  quint64 next = 0;
  mcp::logLines(std::numeric_limits<quint64>::max(), 0, &next);
  return next;
}

// The camera seen along `camera`'s direction that frames `box` in a
// picture `aspect` wide for its height, with a small margin (ui_camera's
// aspect, mitcad#689): the box's middle at the target, the height the
// larger of the box's height and its width over the aspect; a perspective
// camera moves back to show that height at the target in `fov` degrees.
CameraState framedFor(CameraState camera, const Bnd_Box& box, double aspect, double fov) {
  const gp_Dir direction(gp_Vec(camera.eye, camera.target));
  gp_Vec across = gp_Vec(direction).Crossed(gp_Vec(camera.up));
  if (across.Magnitude() <= 1e-12) {
    across = gp_Vec(direction).Crossed(std::abs(direction.Z()) < 0.9 ? gp_Vec(0, 0, 1) : gp_Vec(0, 1, 0));
  }
  const gp_Dir right(across);
  const gp_Dir up = right.Crossed(direction);
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  double low[3] = {std::numeric_limits<double>::max(), std::numeric_limits<double>::max(),
                   std::numeric_limits<double>::max()};
  double high[3] = {std::numeric_limits<double>::lowest(), std::numeric_limits<double>::lowest(),
                    std::numeric_limits<double>::lowest()};
  for (const double x : {x0, x1}) {
    for (const double y : {y0, y1}) {
      for (const double z : {z0, z1}) {
        const gp_Vec corner(x, y, z);
        const double along[3] = {corner.Dot(gp_Vec(right)), corner.Dot(gp_Vec(up)), corner.Dot(gp_Vec(direction))};
        for (int i = 0; i < 3; ++i) {
          low[i] = std::min(low[i], along[i]);
          high[i] = std::max(high[i], along[i]);
        }
      }
    }
  }
  const gp_XYZ middle = right.XYZ() * ((low[0] + high[0]) / 2.0) + up.XYZ() * ((low[1] + high[1]) / 2.0) +
                        direction.XYZ() * ((low[2] + high[2]) / 2.0);
  const double height = std::max({high[1] - low[1], (high[0] - low[0]) / aspect, 1e-3}) * 1.1;
  double distance = camera.eye.Distance(camera.target);
  if (camera.perspective) {
    distance = height / (2.0 * std::tan(fov * kPi / 360.0)) + (high[2] - low[2]) / 2.0;
  }
  camera.target = gp_Pnt(middle);
  camera.eye = gp_Pnt(middle - direction.XYZ() * distance);
  camera.up = up;
  camera.height = height;
  return camera;
}

// Comments for agents (mitcad#1119): the kinds of a comment's target that
// name an entity of a body or a sketch, by their field and as ui_select's
// kind.
const std::pair<const char*, const char*> kCommentBodyTargets[] = {
    {"face", "face"}, {"edge", "edge"}, {"vertex", "vertex"}};
const std::pair<const char*, const char*> kCommentSketchTargets[] = {{"curve", "sketch_curve"},
                                                                     {"point", "sketch_point"},
                                                                     {"constraint", "sketch_constraint"},
                                                                     {"dimension", "sketch_dimension"}};

// A comment as ui_select and ui_camera show it: the comment, its target as
// an entity as ui_select takes it (none for a free point, a parameter or an
// orphaned target) and its place in design coordinates (none for a
// parameter).
struct CommentShown {
  QJsonObject comment;
  QJsonObject entity;
  std::optional<gp_Pnt> place;
};

QJsonObject commentEntity(const QJsonObject& target) {
  const QString occurrence = target.value(QStringLiteral("occurrence")).toString();
  if (target.contains(QStringLiteral("feature"))) {
    return {{QStringLiteral("kind"), QStringLiteral("feature")},
            {QStringLiteral("owner"), target.value(QStringLiteral("feature")).toString()}};
  }
  if (target.contains(QStringLiteral("body"))) {
    QJsonObject entity{{QStringLiteral("kind"), QStringLiteral("body")},
                       {QStringLiteral("owner"), target.value(QStringLiteral("body")).toString()},
                       {QStringLiteral("occurrence"), occurrence}};
    for (const auto& [field, kind] : kCommentBodyTargets) {
      if (target.contains(QLatin1String(field))) {
        entity.insert(QStringLiteral("kind"), QLatin1String(kind));
        entity.insert(QStringLiteral("name"), target.value(QLatin1String(field)).toString());
      }
    }
    return entity;
  }
  if (target.contains(QStringLiteral("sketch"))) {
    QJsonObject entity{{QStringLiteral("kind"), QStringLiteral("sketch")},
                       {QStringLiteral("owner"), target.value(QStringLiteral("sketch")).toString()}};
    for (const auto& [field, kind] : kCommentSketchTargets) {
      if (target.contains(QLatin1String(field))) {
        entity.insert(QStringLiteral("kind"), QLatin1String(kind));
        entity.insert(QStringLiteral("name"), target.value(QLatin1String(field)).toString());
      }
    }
    return entity;
  }
  // A parameter or a free point: nothing to select.
  return {};
}

// The comment `id` of the design, or none.
std::optional<CommentShown> findComment(const QString& id, const CommandContext& model) {
  const QJsonArray comments =
      model.query({{QStringLiteral("query"), QStringLiteral("comments")}}).toObject().value(QStringLiteral("comments")).toArray();
  for (const QJsonValue& value : comments) {
    const QJsonObject comment = value.toObject();
    if (comment.value(QStringLiteral("id")).toString() != id) {
      continue;
    }
    CommentShown shown;
    shown.comment = comment;
    if (!comment.value(QStringLiteral("orphaned")).toBool()) {
      shown.entity = commentEntity(comment.value(QStringLiteral("target")).toObject());
    }
    const QJsonArray place = comment.value(QStringLiteral("place")).toArray();
    if (place.size() == 3 && place[0].isDouble() && place[1].isDouble() && place[2].isDouble()) {
      shown.place = gp_Pnt(place[0].toDouble(), place[1].toDouble(), place[2].toDouble());
    }
    return shown;
  }
  return std::nullopt;
}

// What a ui_select or ui_camera result says of the comment it showed.
QJsonObject commentJson(const CommentShown& shown) {
  return {{QStringLiteral("id"), shown.comment.value(QStringLiteral("id"))},
          {QStringLiteral("label"), shown.comment.value(QStringLiteral("label"))},
          {QStringLiteral("status"), shown.comment.value(QStringLiteral("status"))},
          {QStringLiteral("orphaned"), shown.comment.value(QStringLiteral("orphaned"))},
          {QStringLiteral("place"), shown.comment.value(QStringLiteral("place"))},
          {QStringLiteral("selected"), !shown.entity.isEmpty()}};
}

// The comment a ui_select or ui_camera call names (mitcad#1119); refused
// when the design has none by that id.
CommentShown mcpComment(const QJsonObject& arguments, const CommandContext& model) {
  const QString id = arguments.value(QStringLiteral("comment")).toString();
  std::optional<CommentShown> shown;
  try {
    shown = findComment(id, model);
  } catch (const std::exception& error) {
    refuse(QStringLiteral("The comments could not be read: %1").arg(QString::fromUtf8(error.what())));
  }
  if (!shown) {
    refuse(QStringLiteral("The design has no comment %1.").arg(mcp::echoed(id)));
  }
  return *shown;
}

// A separate top-level window in a picture of the main window
// (mitcad#1734): what it is for an agent and how high it stacks.
struct TopLevelLayer {
  QWidget* window = nullptr;
  QRect rect; // on the screen, in logical pixels
  QString kind;
  int rank = 0;
};

// The visible top-level windows of the application over a main window:
// popups (menus, open lists), tooltips, dialogs and windows of its own,
// in the order they stack (cards and tool windows, dialogs, popups, then
// tooltips; the active window last within its rank). The window that
// holds the main window (its document frame), any other window that holds
// a design, and what belongs to another design's main window are left out.
std::vector<TopLevelLayer> topLevelLayers(const MainWindow* main) {
  std::vector<TopLevelLayer> layers;
  for (QWidget* window : QApplication::topLevelWidgets()) {
    if (window == main || window == main->window() || !window->isVisible() || window->isMinimized() ||
        window->testAttribute(Qt::WA_DontShowOnScreen) || window->windowOpacity() <= 0.0 || window->width() < 8 ||
        window->height() < 8 ||
        qobject_cast<const MainWindow*>(window) != nullptr || window->findChild<MainWindow*>() != nullptr) {
      continue;
    }
    bool another = false;
    for (const QWidget* owner = window->parentWidget(); owner != nullptr; owner = owner->parentWidget()) {
      if (owner != main && qobject_cast<const MainWindow*>(owner) != nullptr) {
        another = true;
        break;
      }
    }
    if (another) {
      continue;
    }
    TopLevelLayer layer;
    layer.window = window;
    layer.rect = QRect(window->mapToGlobal(QPoint(0, 0)), window->size());
    switch (window->windowType()) {
    case Qt::ToolTip:
      layer.kind = QStringLiteral("tooltip");
      layer.rank = 3;
      break;
    case Qt::Popup:
      layer.kind = QStringLiteral("popup");
      layer.rank = 2;
      break;
    case Qt::Dialog:
    case Qt::Sheet:
      layer.kind = QStringLiteral("dialog");
      layer.rank = 1;
      break;
    default:
      layer.kind = QStringLiteral("window");
      break;
    }
    layers.push_back(layer);
  }
  const QWidget* active = QApplication::activeWindow();
  std::stable_sort(layers.begin(), layers.end(), [active](const TopLevelLayer& a, const TopLevelLayer& b) {
    if (a.rank != b.rank) {
      return a.rank < b.rank;
    }
    return a.window != active && b.window == active;
  });
  return layers;
}

} // namespace

QJsonObject MainWindow::mcpUiState() {
  const DocumentSession::Snapshot snapshot = m_documentSession->snapshot();
  const QJsonObject document = snapshot.document;
  QJsonObject state{
      {QStringLiteral("document"), m_mcpDocumentId},
      {QStringLiteral("name"), snapshot.name},
      {QStringLiteral("path"), snapshot.filePath.isEmpty() ? QJsonValue() : QJsonValue(snapshot.filePath)},
      {QStringLiteral("modified"), snapshot.modified},
      {QStringLiteral("read_only"), windowReadOnly()},
      {QStringLiteral("mode"), modeName(m_mode)},
      {QStringLiteral("busy"), snapshot.busy},
      {QStringLiteral("undo"), document.value(QStringLiteral("undo"))},
      {QStringLiteral("redo"), document.value(QStringLiteral("redo"))},
      {QStringLiteral("undo_depth"), document.value(QStringLiteral("undo_depth"))},
      {QStringLiteral("selection"), selectionJson(m_selection)},
      // While Create Sketch waits for a plane, what the view's banner says
      // (mitcad#474), with the status line beside it when it says more.
      {QStringLiteral("hint"), m_mode == Mode::PickPlane ? pickPlaneText() : m_statusLabel->text()},
      // An error is shown red, a hint plain; the line's level (mitcad#1040).
      {QStringLiteral("hint_is_error"), m_mode != Mode::PickPlane && m_statusLevel == NoticeLevel::Error},
      {QStringLiteral("hint_level"),
       m_mode == Mode::PickPlane ? QStringLiteral("hint") : noticeLevelName(m_statusLevel)},
      {QStringLiteral("agents"), m_mcpAgents},
      {QStringLiteral("agent_clients"), QJsonArray::fromStringList(m_mcpAgentNames)},
      {QStringLiteral("editable"), mcpBlockedReason(true).isEmpty()},
      // The dialog open in the application, which only the user can answer
      // (mitcad#1725): its title, and whether it is the recovery question
      // of the start-up; null without one.
      {QStringLiteral("dialog"), mcp::dialogState()}};
  if (const QString blocked = mcpBlockedReason(true); !blocked.isEmpty()) {
    state.insert(QStringLiteral("blocked_reason"), blocked);
  }
  if (m_mode == Mode::PickPlane && !m_statusLabel->text().isEmpty() && m_statusLabel->text() != pickPlaneText()) {
    state.insert(QStringLiteral("status_line"), m_statusLabel->text());
    state.insert(QStringLiteral("status_line_is_error"), !m_statusLabel->styleSheet().isEmpty());
  }
  if (windowReadOnly()) {
    state.insert(QStringLiteral("read_only_reason"), readOnlyReason());
  }
  if (document.contains(QStringLiteral("active_component"))) {
    state.insert(QStringLiteral("active_component"), document.value(QStringLiteral("active_component")));
  }
  // The sketch being edited, and its Sketch Palette.
  if (m_sketch->isActive()) {
    const sketch::SketchModel& sketchModel = m_sketch->model();
    QJsonArray warnings;
    for (int i = 0; i < sketchModel.warnings.size(); ++i) {
      const QStringList entities = static_cast<std::size_t>(i) < sketchModel.warningEntities.size()
                                       ? sketchModel.warningEntities[static_cast<std::size_t>(i)]
                                       : QStringList();
      warnings.append(QJsonObject{{QStringLiteral("text"), sketchModel.warnings[i]},
                                  {QStringLiteral("entities"), QJsonArray::fromStringList(entities)}});
    }
    QJsonObject paletteState{{QStringLiteral("status"), m_palette != nullptr ? m_palette->statusText() : QString()},
                        {QStringLiteral("warnings"), warnings},
                        {QStringLiteral("clean_up_offered"), m_palette != nullptr && m_palette->cleanUpOffered()}};
    state.insert(QStringLiteral("sketch"),
                 QJsonObject{{QStringLiteral("uid"), m_sketch->uid()},
                             {QStringLiteral("name"), sketchModel.name},
                             {QStringLiteral("dof"), sketchModel.dof},
                             {QStringLiteral("fully_constrained"), sketchModel.fullyConstrained()},
                             {QStringLiteral("existing"), m_sketchExisting},
                             {QStringLiteral("tool"), m_sketch->tool() != nullptr ? QJsonValue(m_sketch->tool()->name())
                                                                                  : QJsonValue()},
                             {QStringLiteral("palette"), paletteState}});
  } else if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    // A 3D sketch (mitcad#1524): its own palette's status; its geometry
    // is the sketch3d query's.
    const sketch::Sketch3dModel& spatial = m_sketch3d->model();
    state.insert(QStringLiteral("sketch"),
                 QJsonObject{{QStringLiteral("uid"), spatial.uid},
                             {QStringLiteral("name"), spatial.name},
                             {QStringLiteral("kind"), QStringLiteral("3d")},
                             {QStringLiteral("dof"), spatial.dof},
                             {QStringLiteral("fully_constrained"), spatial.fullyConstrained},
                             {QStringLiteral("existing"), m_sketchExisting},
                             {QStringLiteral("tool"), m_sketch3d->toolActive() ? QJsonValue(m_sketch3d->toolName())
                                                                               : QJsonValue()},
                             {QStringLiteral("palette"),
                              QJsonObject{{QStringLiteral("status"), m_palette3d->statusText()},
                                          {QStringLiteral("details"),
                                           QJsonArray::fromStringList(sketch::detailLines(spatial))},
                                          // The lists' rows: the words shown, the ids (mitcad#1652).
                                          {QStringLiteral("items"), m_palette3d->listState()}}}});
  } else {
    state.insert(QStringLiteral("sketch"), QJsonValue());
  }
  // The running command with its inputs and panel.
  if (m_session) {
    QJsonObject command = m_session->scriptState();
    command.insert(QStringLiteral("in_sketch"), m_commandInSketch);
    state.insert(QStringLiteral("command"), command);
  } else {
    state.insert(QStringLiteral("command"), QJsonValue());
  }
  // The open comments for an agent (mitcad#1119): mitcad://comments and
  // the comments query list them.
  if (!modelBusy()) {
    try {
      const QJsonObject comments =
          query({{QStringLiteral("query"), QStringLiteral("comments")}, {QStringLiteral("status"), QStringLiteral("open")}})
              .toObject();
      state.insert(QStringLiteral("comments"),
                   QJsonObject{{QStringLiteral("open"), comments.value(QStringLiteral("open")).toInt()}});
    } catch (const std::exception&) {
      // A design the comments query cannot read: no count.
    }
  }
  state.insert(QStringLiteral("app"),
               QJsonObject{{QStringLiteral("version"), QCoreApplication::applicationVersion()},
                           {QStringLiteral("build"), resultStoreBuildId()},
                           {QStringLiteral("qt"), QString::fromLatin1(qVersion())},
                           {QStringLiteral("platform"), QSysInfo::prettyProductName()},
                           {QStringLiteral("ui"), m_nativeUi ? QStringLiteral("native") : QStringLiteral("qt")},
                           {QStringLiteral("chrome"), chromeStyle() == ChromeStyle::Floating ? QStringLiteral("floating")
                                                                                             : QStringLiteral("docked")}});
  return state;
}

QJsonObject MainWindow::mcpLog(const QJsonObject& arguments) const {
  const qint64 since = std::max<qint64>(0, arguments.value(QStringLiteral("since")).toInteger(0));
  const int limit = std::clamp(arguments.value(QStringLiteral("limit")).toInt(200), 1, 2000);
  quint64 next = 0;
  bool dropped = false;
  QJsonArray lines;
  for (const mcp::LogLine& line : mcp::logLines(static_cast<quint64>(since), limit, &next, &dropped)) {
    lines.append(QJsonObject{{QStringLiteral("seq"), static_cast<qint64>(line.sequence)},
                             {QStringLiteral("level"), line.level},
                             {QStringLiteral("text"), line.text},
                             {QStringLiteral("time"), line.time}});
  }
  return {{QStringLiteral("lines"), lines},
          {QStringLiteral("next"), static_cast<qint64>(next)},
          {QStringLiteral("dropped"), dropped}};
}

QJsonObject MainWindow::mcpScreenshot(const QJsonObject& arguments) {
  const bool whole = arguments.value(QStringLiteral("window")).toBool(false);
  const int maxSize = std::clamp(arguments.value(QStringLiteral("max_size")).toInt(1280), 64, 4096);
  if (m_viewer->isPainting()) {
    refuseWith(QStringLiteral("busy: the view is drawing; retry in 100 ms"),
               {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 100}});
  }
  // The format, a file to write, a size of its own (mitcad#689).
  const QString given = arguments.value(QStringLiteral("path")).toString();
  QString format = arguments.value(QStringLiteral("format")).toString();
  if (format.isEmpty()) {
    const QString suffix = QFileInfo(given).suffix().toLower();
    format = suffix == QStringLiteral("jpg") || suffix == QStringLiteral("jpeg") ? QStringLiteral("jpeg")
                                                                                 : QStringLiteral("png");
  }
  if (format != QStringLiteral("png") && format != QStringLiteral("jpeg")) {
    refuse(QStringLiteral("format must be png or jpeg."));
  }
  const bool jpeg = format == QStringLiteral("jpeg");
  const int quality = std::clamp(arguments.value(QStringLiteral("quality")).toInt(90), 1, 100);
  QString target;
  if (!given.isEmpty()) {
    QString reason;
    target = mcp::agentOutputPath(given, arguments.value(QStringLiteral("overwrite")).toBool(false), reason,
                                  jpeg ? QStringList{QStringLiteral("jpg"), QStringLiteral("jpeg")}
                                       : QStringList{QStringLiteral("png")});
    if (target.isEmpty()) {
      refuse(reason);
    }
  }
  const std::optional<QSize> offscreen = mcpScreenshotSize(arguments, maxSize);
  if (offscreen && whole) {
    refuse(QStringLiteral("width, height and aspect render the 3D view, not the whole window."));
  }
  // The view's frame with the widgets over it (the sketch overlay, the
  // orientation cube's buttons), as the user sees it; the version watermark
  // (mitcad#1209) only in a picture of the window, never in one of the view.
  const auto viewImage = [this](bool watermark) {
    QImage frame = m_viewer->grabFramebuffer();
    if (frame.isNull()) {
      return frame;
    }
    QPainter painter(&frame);
    for (QWidget* overlay : m_viewer->findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
      if (overlay->isVisibleTo(m_viewer) && (watermark || qobject_cast<VersionWatermark*>(overlay) == nullptr)) {
        overlay->render(&painter, overlay->pos(), QRegion(), QWidget::DrawChildren);
      }
    }
    return frame;
  };
  QImage image;
  QString what = QStringLiteral("view");
  QJsonArray windows; // a picture of the window: where each window lies in it
  if (offscreen) {
    // Offscreen at that size, so the picture does not take the window's
    // shape (mitcad#689); the model must not be computing meanwhile.
    if (modelBusy()) {
      refuseWith(QStringLiteral("busy: the design is computing; retry in 1000 ms"),
                 {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 1000}});
    }
    image = m_viewer->snapshot(*offscreen, false, true);
  } else if (!whole) {
    image = viewImage(false);
  } else if (m_nativeUi) {
    // AppKit draws the window: as the window server composes it.
    const QTemporaryDir directory;
    const QString path = QDir(directory.path()).filePath(QStringLiteral("window.png"));
    if (directory.isValid() && mac::captureOwnWindows(path)) {
      image.load(path);
    }
    what = QStringLiteral("window");
  } else {
    // The window's grab lacks what the view draws with OpenGL: the view's
    // frame goes in, and the widgets over it again.
    QPixmap shot = grab();
    const QImage frame = viewImage(true);
    if (!frame.isNull()) {
      QPainter painter(&shot);
      painter.drawImage(QRect(m_viewer->mapTo(this, QPoint(0, 0)), m_viewer->size()), frame);
      if (QWidget* parent = m_viewer->parentWidget()) {
        // Floating chrome: the cards over the view are its siblings.
        bool above = false;
        for (QObject* child : parent->children()) {
          auto* widget = qobject_cast<QWidget*>(child);
          if (widget == m_viewer) {
            above = true;
          } else if (above && widget != nullptr && widget->isVisibleTo(parent) && !widget->isWindow()) {
            widget->render(&painter, widget->mapTo(this, QPoint(0, 0)), QRegion(), QWidget::DrawChildren);
          }
        }
      }
    }
    // The application's other top-level windows (popups, open lists,
    // tooltips, dialogs) at their places on the screen over the window, as
    // the window server composes them on macOS (mitcad#1734); the picture
    // takes in what lies outside the window, within the screens.
    const QRect mainRect(mapToGlobal(QPoint(0, 0)), size());
    const QRect desktop = screen() != nullptr ? screen()->virtualGeometry() : QRect();
    std::vector<TopLevelLayer> layers = topLevelLayers(this);
    if (desktop.isValid()) {
      for (TopLevelLayer& layer : layers) {
        layer.rect &= desktop;
      }
    }
    layers.erase(std::remove_if(layers.begin(), layers.end(),
                                [](const TopLevelLayer& layer) { return layer.rect.isEmpty(); }),
                 layers.end());
    QRect bounds = mainRect;
    for (const TopLevelLayer& layer : layers) {
      bounds |= layer.rect;
    }
    if (!layers.empty()) {
      const qreal ratio = shot.devicePixelRatio();
      QPixmap canvas((QSizeF(bounds.size()) * ratio).toSize());
      canvas.setDevicePixelRatio(ratio);
      canvas.fill(Qt::transparent);
      QPainter painter(&canvas);
      painter.drawPixmap(mainRect.topLeft() - bounds.topLeft(), shot);
      for (const TopLevelLayer& layer : layers) {
        // Only the part within the screens.
        const QRect source(layer.rect.topLeft() - layer.window->mapToGlobal(QPoint(0, 0)), layer.rect.size());
        painter.drawPixmap(layer.rect.topLeft() - bounds.topLeft(), layer.window->grab(source));
      }
      painter.end();
      shot = canvas;
    }
    // Where each window lies in the full picture (its pixels), for an agent.
    const qreal scale = shot.devicePixelRatio();
    const auto placed = [&bounds, scale](const QString& kind, const QRect& area, const QString& title) {
      const QRect in = QRectF(QPointF(area.topLeft() - bounds.topLeft()) * scale, QSizeF(area.size()) * scale)
                           .toAlignedRect();
      QJsonObject place{{QStringLiteral("kind"), kind},
                        {QStringLiteral("x"), in.x()},
                        {QStringLiteral("y"), in.y()},
                        {QStringLiteral("width"), in.width()},
                        {QStringLiteral("height"), in.height()}};
      if (!title.isEmpty()) {
        place.insert(QStringLiteral("title"), title);
      }
      return place;
    };
    windows.append(placed(QStringLiteral("main"), mainRect, QString()));
    for (const TopLevelLayer& layer : layers) {
      windows.append(placed(layer.kind, layer.rect, layer.window->windowTitle()));
    }
    image = shot.toImage();
    what = QStringLiteral("window");
  }
  if (image.isNull()) {
    refuse(QStringLiteral("No image of the %1 could be made.").arg(what));
  }
  const int fullWidth = image.width();
  const int fullHeight = image.height();
  // Within max_size (an explicit size is kept) and max_bytes, the encoded
  // image's bytes (mitcad#1188). Returned in the response, 5 MiB by
  // default: well within Mitcad's 8 MiB frame once in base64, though a
  // client may have a lower limit of its own. A file takes it at its size
  // unless max_bytes is given.
  const bool byteLimit = arguments.contains(QStringLiteral("max_bytes"));
  qsizetype budget = target.isEmpty() ? mcp::kScreenshotInlineBytes : std::numeric_limits<qsizetype>::max();
  if (byteLimit) {
    const qint64 asked = arguments.value(QStringLiteral("max_bytes")).toInteger(mcp::kScreenshotInlineBytes);
    budget = static_cast<qsizetype>(std::clamp<qint64>(asked, qint64{mcp::kScreenshotMinBytes},
                                                       qint64{mcp::kScreenshotInlineBytes}));
  }
  const int longest = offscreen ? std::max(image.width(), image.height()) : maxSize;
  const mcp::FittedImage fitted = mcp::fitImage(
      image, longest, budget, [jpeg, quality](const QImage& picture) { return mcp::encodeImage(picture, jpeg, quality); });
  if (fitted.bytes.isEmpty()) {
    refuse(QStringLiteral("The image could not be written as %1.").arg(format.toUpper()));
  }
  if (!fitted.fits) {
    // Compact and actionable; the session stays usable.
    refuseWith(QStringLiteral("The image of the %1 takes %2 bytes as %3 even at %4 × %5 pixels, over the %6 bytes allowed. "
                  "Allow more max_bytes, ask for format jpeg with a lower quality, or write it to a file with path.")
                   .arg(what)
                   .arg(fitted.bytes.size())
                   .arg(format.toUpper())
                   .arg(fitted.image.width())
                   .arg(fitted.image.height())
                   .arg(budget),
               {{QStringLiteral("error_code"), QStringLiteral("screenshot_too_large")},
                {QStringLiteral("max_bytes"), static_cast<qint64>(budget)},
                {QStringLiteral("smallest_bytes"), static_cast<qint64>(fitted.bytes.size())},
                {QStringLiteral("smallest_width"), fitted.image.width()},
                {QStringLiteral("smallest_height"), fitted.image.height()},
                {QStringLiteral("format"), format}});
  }
  image = fitted.image;
  const QByteArray& bytes = fitted.bytes;
  const QString mime = jpeg ? QStringLiteral("image/jpeg") : QStringLiteral("image/png");
  QJsonObject result{{QStringLiteral("format"), format},
                     {QStringLiteral("mime_type"), mime},
                     {QStringLiteral("of"), what},
                     {QStringLiteral("width"), image.width()},
                     {QStringLiteral("height"), image.height()},
                     {QStringLiteral("full_width"), fullWidth},
                     {QStringLiteral("full_height"), fullHeight},
                     {QStringLiteral("bytes"), static_cast<qint64>(bytes.size())},
                     {QStringLiteral("downscaled_for_bytes"), fitted.downscaled}};
  if (byteLimit || target.isEmpty()) {
    result.insert(QStringLiteral("max_bytes"), static_cast<qint64>(budget));
  }
  if (!windows.isEmpty()) {
    // In the full picture's pixels (full_width by full_height); the main
    // window first, then the windows over it as they stack (mitcad#1734).
    result.insert(QStringLiteral("windows"), windows);
  }
  if (!target.isEmpty()) {
    // Into the allowed folder: only the path and the size go back.
    QFile file(target);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate) || file.write(bytes) != bytes.size()) {
      refuse(QStringLiteral("Could not write %1: %2").arg(QDir::toNativeSeparators(target), file.errorString()));
    }
    file.close();
    result.insert(QStringLiteral("path"), target);
  } else {
    // An MCP image block (finish_mcp moves it out of the JSON).
    result.insert(QStringLiteral("image_base64"), QString::fromLatin1(bytes.toBase64()));
  }
  qInfo().noquote() << QStringLiteral("MCP: screenshot of the %1, %2 x %3, %4, %5 bytes%6")
                           .arg(what)
                           .arg(image.width())
                           .arg(image.height())
                           .arg(format)
                           .arg(bytes.size())
                           .arg(target.isEmpty() ? QString() : QStringLiteral(", to ") + target);
  return result;
}

std::optional<QSize> MainWindow::mcpScreenshotSize(const QJsonObject& arguments, int maxSize) const {
  // width and height, or aspect with one of them or with max_size on the
  // longer side (mitcad#689); none: the view as it is.
  const bool hasWidth = arguments.contains(QStringLiteral("width"));
  const bool hasHeight = arguments.contains(QStringLiteral("height"));
  const bool hasAspect = arguments.contains(QStringLiteral("aspect"));
  if (!hasWidth && !hasHeight && !hasAspect) {
    return std::nullopt;
  }
  if (hasWidth && hasHeight && hasAspect) {
    refuse(QStringLiteral("Give width and height, or aspect with at most one of them."));
  }
  const double viewAspect =
      m_viewer->height() > 0 ? static_cast<double>(m_viewer->width()) / m_viewer->height() : 1.0;
  int across = arguments.value(QStringLiteral("width")).toInt(0);
  int down = arguments.value(QStringLiteral("height")).toInt(0);
  const double aspect = hasAspect ? arguments.value(QStringLiteral("aspect")).toDouble()
                        : hasWidth && hasHeight && down > 0 ? static_cast<double>(across) / down
                                                              : viewAspect;
  if (!std::isfinite(aspect) || aspect < 0.1 || aspect > 10.0) {
    refuse(QStringLiteral("aspect must be between 0.1 and 10."));
  }
  if (hasWidth && !hasHeight) {
    down = static_cast<int>(std::lround(across / aspect));
  } else if (hasHeight && !hasWidth) {
    across = static_cast<int>(std::lround(down * aspect));
  } else if (!hasWidth && !hasHeight) {
    across = aspect >= 1.0 ? maxSize : static_cast<int>(std::lround(maxSize * aspect));
    down = aspect >= 1.0 ? static_cast<int>(std::lround(maxSize / aspect)) : maxSize;
  }
  return QSize(std::clamp(across, 16, 4096), std::clamp(down, 16, 4096));
}

QJsonObject MainWindow::mcpSelect(const QJsonObject& arguments) {
  mcpRefuseBusy();
  if (m_mode == Mode::PickPlane) {
    refuse(QStringLiteral("Create Sketch waits for a plane to be picked: pick one or cancel it first."));
  }
  const QString reveal = arguments.value(QStringLiteral("reveal")).toString(QStringLiteral("pan"));
  // A comment's target instead of items (mitcad#1119): its entity when it
  // is still in the design, else only its place.
  std::optional<CommentShown> shown;
  if (arguments.contains(QStringLiteral("comment"))) {
    shown = mcpComment(arguments, *this);
  }
  Selection items;
  for (const QJsonValue& value : arguments.value(QStringLiteral("items")).toArray()) {
    QString problem;
    const std::optional<SelectionItem> item = readItem(value, *this, problem);
    if (!item) {
      refuse(problem);
    }
    if (!items.contains(*item)) {
      items.append(*item);
    }
  }
  if (shown && !shown->entity.isEmpty()) {
    QString problem;
    const std::optional<SelectionItem> item = readItem(shown->entity, *this, problem);
    std::vector<gp_Pnt> corners;
    Selection target;
    if (item) {
      target.append(*item);
    }
    if (item && mcpItemCorners(target, corners, &problem)) {
      items.append(*item);
    } else {
      shown->entity = QJsonObject();
    }
  }
  if (m_session) {
    // A command panel takes the picks in its active input, as clicks in the
    // view and rows picked in the browser do (each a click: one already
    // there goes out again).
    const QPointer<CommandSession> session = m_session;
    // No question waits for an answer the agent cannot give (mitcad#960).
    const QScopedValueRollback<bool> picking(m_mcpPicking, true);
    for (const SelectionItem& item : items) {
      if (session) {
        session->picked({item}, Qt::NoModifier, false);
      }
    }
    waitUntil([&session] { return !session || !session->scriptBusy(); }, 120000);
    qInfo().noquote() << QStringLiteral("MCP: an agent picked %1 into the command panel").arg(summarize(items));
    QJsonObject picked{{QStringLiteral("document"), m_mcpDocumentId},
                       {QStringLiteral("picked"), static_cast<int>(items.size())},
                       {QStringLiteral("into_command"), true}};
    if (session) {
      picked.insert(QStringLiteral("command"), session->scriptState());
    }
    if (shown) {
      picked.insert(QStringLiteral("comment"), commentJson(*shown));
    }
    return picked;
  }
  const bool sketchItems =
      m_mode == Mode::Sketch && !items.isEmpty() && std::all_of(items.begin(), items.end(), [this](const SelectionItem& item) {
        return (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) &&
               item.owner == m_sketch->uid();
      });
  QString moved = QStringLiteral("none");
  if (sketchItems) {
    // The edited sketch's curves and points, as the palette's warnings
    // select them (mitcad#264).
    QStringList ids;
    QStringList missing;
    for (const SelectionItem& item : items) {
      const bool there = item.kind == SelectKind::SketchCurve ? m_sketch->model().curve(item.name) != nullptr
                                                              : m_sketch->model().point(item.name) != nullptr;
      (there ? ids : missing) << item.name;
    }
    if (!missing.isEmpty()) {
      refuse(QStringLiteral("%1 has no %2").arg(m_sketch->model().name, mcp::echoed(missing.join(QStringLiteral(", ")))));
    }
    m_sketch->selectEntities(ids, reveal != QStringLiteral("none"), reveal == QStringLiteral("fit"));
  } else {
    // Faces, edges, bodies, profiles, ...: each must be in the model, as
    // the view shows it.
    std::vector<gp_Pnt> places;
    QString missing;
    if (!mcpItemCorners(items, places, &missing)) {
      refuse(QStringLiteral("Not in the design: %1").arg(mcp::echoed(missing)));
    }
    // A comment on a free point, a parameter or a target no longer there:
    // its place (mitcad#1119).
    if (shown && places.empty() && shown->place) {
      places.push_back(*shown->place);
    }
    setSelection(items);
    if (reveal != QStringLiteral("none")) {
      moved = revealInView(*m_viewer, places, reveal == QStringLiteral("fit"));
    }
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent selected %1").arg(summarize(m_selection));
  QJsonObject result{{QStringLiteral("document"), m_mcpDocumentId},
                     {QStringLiteral("selected"), static_cast<int>(m_selection.size())},
                     {QStringLiteral("selection"), selectionJson(m_selection)}};
  if (!sketchItems) {
    result.insert(QStringLiteral("view"), moved);
  }
  if (shown) {
    result.insert(QStringLiteral("comment"), commentJson(*shown));
  }
  if (reveal != QStringLiteral("none")) {
    // Where the view goes, not where its animation is now: the reply's
    // camera and the next screenshot show it (mitcad#405).
    m_viewer->settleCamera();
    result.insert(QStringLiteral("camera"), mcpCameraJson());
  }
  return result;
}

bool MainWindow::mcpItemCorners(const Selection& items, std::vector<gp_Pnt>& corners, QString* missing) const {
  for (const SelectionItem& item : items) {
    // Geometry must be there; datums, components, features and sketches
    // are shown when they have a shape.
    const bool required = item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
                          item.kind == SelectKind::Vertex || item.kind == SelectKind::Body ||
                          item.kind == SelectKind::Profile || item.kind == SelectKind::SketchCurve ||
                          item.kind == SelectKind::SketchPoint;
    TopoDS_Shape shape;
    try {
      shape = itemShape(item);
    } catch (const std::exception&) {
      shape.Nullify();
    }
    if (shape.IsNull()) {
      if (required && missing != nullptr) {
        *missing = item.logDescription();
        return false;
      }
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(shape, box);
    if (box.IsVoid()) {
      continue;
    }
    double x0 = 0.0;
    double y0 = 0.0;
    double z0 = 0.0;
    double x1 = 0.0;
    double y1 = 0.0;
    double z1 = 0.0;
    box.Get(x0, y0, z0, x1, y1, z1);
    for (const double x : {x0, x1}) {
      for (const double y : {y0, y1}) {
        for (const double z : {z0, z1}) {
          corners.emplace_back(x, y, z);
        }
      }
    }
  }
  return true;
}

QJsonObject MainWindow::mcpCameraJson() const {
  const CameraState camera = m_viewer->camera();
  const auto xyz = [](const gp_XYZ& v) { return QJsonArray{v.X(), v.Y(), v.Z()}; };
  const double distance = camera.eye.Distance(camera.target);
  const gp_XYZ direction = distance > 0.0 ? (camera.target.XYZ() - camera.eye.XYZ()) / distance : gp_XYZ(0, 0, -1);
  const double aspect = m_viewer->height() > 0 ? double(m_viewer->width()) / double(m_viewer->height()) : 1.0;
  return {{QStringLiteral("eye"), xyz(camera.eye.XYZ())},
          {QStringLiteral("target"), xyz(camera.target.XYZ())},
          {QStringLiteral("up"), xyz(camera.up.XYZ())},
          {QStringLiteral("direction"), xyz(direction)},
          {QStringLiteral("distance"), distance},
          {QStringLiteral("height"), camera.height},
          {QStringLiteral("width"), camera.height * aspect},
          {QStringLiteral("projection"), camera.perspective ? QStringLiteral("perspective") : QStringLiteral("orthographic")},
          {QStringLiteral("fov"), m_viewer->fieldOfView()},
          // mm as on a 35 mm frame, the same field of view (mitcad#1717)
          {QStringLiteral("focal_length"), m_viewer->focalLength()},
          {QStringLiteral("view_size"), QJsonArray{m_viewer->width(), m_viewer->height()}}};
}

QJsonObject MainWindow::mcpCamera(const QJsonObject& arguments) {
  if (m_viewer->isPainting()) {
    refuseWith(QStringLiteral("busy: the view is drawing; retry in 100 ms"),
               {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 100}});
  }
  const auto pointOf = [](const QString& key, const QJsonValue& given) {
    const QJsonArray array = given.toArray();
    if (array.size() != 3 || !array[0].isDouble() || !array[1].isDouble() || !array[2].isDouble()) {
      refuse(QStringLiteral("%1 must be three numbers (x, y, z in mm).").arg(key));
    }
    if (!std::isfinite(array[0].toDouble()) || !std::isfinite(array[1].toDouble()) ||
        !std::isfinite(array[2].toDouble())) {
      refuse(QStringLiteral("%1 must contain finite numbers.").arg(key));
    }
    return gp_XYZ(array[0].toDouble(), array[1].toDouble(), array[2].toDouble());
  };
  // A comment's target to select and frame (mitcad#1119): the model must
  // be idle to read it.
  std::optional<CommentShown> shown;
  if (arguments.contains(QStringLiteral("comment"))) {
    mcpRefuseBusy();
    shown = mcpComment(arguments, *this);
  }
  // Stage the camera where the view is going. Even settling an animation
  // changes the viewer, so defer it until every argument has been checked.
  CameraState camera = m_viewer->pendingCamera();
  const bool hasFov = arguments.contains(QStringLiteral("fov"));
  const double fov = hasFov ? arguments.value(QStringLiteral("fov")).toDouble()
                            : m_viewer->fieldOfView();
  if (hasFov && (!std::isfinite(fov) || fov < 1.0 || fov > 170.0)) {
    refuse(QStringLiteral("fov must be between 1 and 170 degrees."));
  }
  const bool hasEye = arguments.contains(QStringLiteral("eye"));
  const bool hasTarget = arguments.contains(QStringLiteral("target"));
  const bool hasHeight = arguments.contains(QStringLiteral("height"));
  bool set = false;
  if (arguments.contains(QStringLiteral("projection"))) {
    const QString projection = arguments.value(QStringLiteral("projection")).toString();
    if (projection != QStringLiteral("orthographic") && projection != QStringLiteral("perspective")) {
      refuse(QStringLiteral("projection must be orthographic or perspective."));
    }
    camera.perspective = projection == QStringLiteral("perspective");
    set = true;
  }
  if (hasEye) {
    camera.eye = gp_Pnt(pointOf(QStringLiteral("eye"), arguments.value(QStringLiteral("eye"))));
  }
  if (hasTarget) {
    camera.target = gp_Pnt(pointOf(QStringLiteral("target"), arguments.value(QStringLiteral("target"))));
  }
  if (!std::isfinite(camera.eye.Distance(camera.target)) || camera.eye.Distance(camera.target) <= Precision::Confusion()) {
    refuse(QStringLiteral("The eye and the target must be apart."));
  }
  if (arguments.contains(QStringLiteral("up"))) {
    const gp_XYZ up = pointOf(QStringLiteral("up"), arguments.value(QStringLiteral("up")));
    const gp_XYZ direction = camera.target.XYZ() - camera.eye.XYZ();
    if (!std::isfinite(up.Modulus()) || !std::isfinite(up.Crossed(direction).Modulus()) ||
        up.Modulus() <= Precision::Confusion() || up.Crossed(direction).Modulus() <= 1e-9 * up.Modulus() * direction.Modulus()) {
      refuse(QStringLiteral("up must be a vector across the view direction."));
    }
    camera.up = gp_Dir(up);
    set = true;
  } else if (gp_Vec(camera.up).Crossed(gp_Vec(camera.eye, camera.target)).Magnitude() <=
             1e-9 * camera.eye.Distance(camera.target)) {
    // Looking along the current up: another up across the new direction.
    const gp_Dir direction(gp_Vec(camera.eye, camera.target));
    camera.up = std::abs(direction.Z()) < 0.9 ? gp_Dir(0, 0, 1) : gp_Dir(0, 1, 0);
  }
  if (hasHeight) {
    const double height = arguments.value(QStringLiteral("height")).toDouble();
    if (!std::isfinite(height) || !(height > 0.0)) {
      refuse(QStringLiteral("height must be more than 0 mm."));
    }
    camera.height = height;
  } else if (camera.perspective && (hasEye || hasTarget)) {
    // The eye given stays where it is: the height follows from its distance.
    camera.height = 2.0 * camera.eye.Distance(camera.target) * std::tan(fov * kPi / 360.0);
  }
  if (!std::isfinite(camera.height) || !(camera.height > 0.0)) {
    refuse(QStringLiteral("The resulting camera height must be finite and more than 0 mm."));
  }
  set = set || hasEye || hasTarget || hasHeight || hasFov;
  // Resolve framing before changing the camera or its field of view.
  QString framed = QStringLiteral("none");
  Bnd_Box box;
  const QString frame = arguments.value(QStringLiteral("frame")).toString(QStringLiteral("none"));
  // A comment's target, selected when the window is idle, and the point
  // centred when the target has no shape (mitcad#1119).
  Selection commentTarget;
  std::optional<gp_Pnt> centre;
  if (shown) {
    if (arguments.contains(QStringLiteral("box")) || frame != QStringLiteral("none")) {
      refuse(QStringLiteral("Give either comment, box or frame."));
    }
    std::vector<gp_Pnt> corners;
    if (!shown->entity.isEmpty()) {
      QString problem;
      const std::optional<SelectionItem> item = readItem(shown->entity, *this, problem);
      if (item) {
        commentTarget.append(*item);
      }
      if (commentTarget.isEmpty() || !mcpItemCorners(commentTarget, corners, &problem)) {
        commentTarget.clear();
        corners.clear();
        shown->entity = QJsonObject();
      }
    }
    for (const gp_Pnt& corner : corners) {
      box.Add(corner);
    }
    if (box.IsVoid()) {
      if (!shown->place) {
        refuse(QStringLiteral("Comment %1 has no place in the view (it is on %2).")
                   .arg(shown->comment.value(QStringLiteral("id")).toString(),
                        shown->comment.value(QStringLiteral("label")).toString()));
      }
      centre = shown->place;
    }
    framed = QStringLiteral("comment");
  } else if (arguments.contains(QStringLiteral("box"))) {
    if (frame != QStringLiteral("none")) {
      refuse(QStringLiteral("Give either box or frame, not both."));
    }
    const QJsonObject given = arguments.value(QStringLiteral("box")).toObject();
    box.Add(gp_Pnt(pointOf(QStringLiteral("box.min"), given.value(QStringLiteral("min")))));
    box.Add(gp_Pnt(pointOf(QStringLiteral("box.max"), given.value(QStringLiteral("max")))));
    framed = QStringLiteral("box");
  } else if (frame == QStringLiteral("shown")) {
    box = m_viewer->bodiesBox();
    if (box.IsVoid()) {
      refuse(QStringLiteral("No body is shown to frame."));
    }
    framed = frame;
  } else if (frame == QStringLiteral("selection")) {
    std::vector<gp_Pnt> corners;
    mcpItemCorners(m_selection, corners, nullptr);
    if (corners.empty()) {
      refuse(QStringLiteral("Nothing with a shape is selected to frame."));
    }
    for (const gp_Pnt& corner : corners) {
      box.Add(corner);
    }
    framed = frame;
  } else if (frame != QStringLiteral("none")) {
    refuse(QStringLiteral("frame must be none, shown or selection."));
  }
  // Framed for a picture of that shape (ui_screenshot's aspect, mitcad#689).
  const bool hasAspect = arguments.contains(QStringLiteral("aspect"));
  const double aspect = arguments.value(QStringLiteral("aspect")).toDouble(0.0);
  if (hasAspect && (!std::isfinite(aspect) || aspect < 0.1 || aspect > 10.0)) {
    refuse(QStringLiteral("aspect must be between 0.1 and 10."));
  }
  if (hasAspect && box.IsVoid() && !centre) {
    refuse(QStringLiteral("aspect frames a box or frame (shown or selection): give one of them."));
  }
  // All refusals above leave the live viewer (including animation) alone.
  m_viewer->settleCamera();
  if (hasFov) {
    m_viewer->setFieldOfView(fov);
  }
  if (set) {
    m_viewer->setCamera(camera);
    m_viewer->settleCamera();
  }
  if (!box.IsVoid() && hasAspect) {
    m_viewer->setCamera(framedFor(m_viewer->pendingCamera(), box, aspect, m_viewer->fieldOfView()));
    m_viewer->settleCamera();
  } else if (!box.IsVoid()) {
    m_viewer->frameBox(box);
    m_viewer->settleCamera();
  } else if (centre) {
    // A point: at the middle of the view, at the zoom it has.
    CameraState now = m_viewer->pendingCamera();
    const gp_Vec shift(now.target, *centre);
    now.eye.Translate(shift);
    now.target.Translate(shift);
    m_viewer->setCamera(now);
    m_viewer->settleCamera();
  }
  // The comment's target selected as ui_select would, when no sketch,
  // command or plane pick takes the selection.
  if (shown && m_mode == Mode::Idle && !m_session) {
    setSelection(commentTarget);
  }
  QJsonObject result{{QStringLiteral("camera"), mcpCameraJson()}, {QStringLiteral("framed"), framed},
                     {QStringLiteral("changed"), set || framed != QStringLiteral("none")}};
  if (hasAspect) {
    result.insert(QStringLiteral("aspect"), aspect);
  }
  if (shown) {
    result.insert(QStringLiteral("comment"), commentJson(*shown));
  }
  if (set || framed != QStringLiteral("none")) {
    const QJsonObject now = result.value(QStringLiteral("camera")).toObject();
    qInfo().noquote() << QStringLiteral("MCP: an agent set the camera: target %1, height %2 mm, %3%4")
                             .arg(QString::fromUtf8(QJsonDocument(now.value(QStringLiteral("target")).toArray())
                                                        .toJson(QJsonDocument::Compact)))
                             .arg(now.value(QStringLiteral("height")).toDouble(), 0, 'f', 2)
                             .arg(now.value(QStringLiteral("projection")).toString())
                             .arg(framed == QStringLiteral("none") ? QString() : QStringLiteral(", framed ") + framed);
  }
  return result;
}

QJsonObject MainWindow::mcpUiCommand(const QJsonObject& arguments) {
  // The commands an agent may run (mitcad#1269): their ids, names and
  // whether ui_command runs them and can now. It reads the registry only,
  // also while the model computes.
  const auto listing = [this](const QString& search, const QString& note) {
    QJsonObject listed = mcp::commandListing(*m_registry, search, [this](const CommandDef& def) {
      return isAvailable(def) && (!windowReadOnly() || readOnlyCommand(def));
    });
    listed.insert(QStringLiteral("document"), m_mcpDocumentId);
    listed.insert(QStringLiteral("mode"), modeName(m_mode));
    if (!note.isEmpty()) {
      listed.insert(QStringLiteral("note"), note);
    }
    return listed;
  };
  if (arguments.contains(QStringLiteral("search"))) {
    return listing(arguments.value(QStringLiteral("search")).toString(), QString());
  }
  mcpRefuseBusy();
  if (const QString waiting = mcpDialogReason(); !waiting.isEmpty()) {
    refuse(waiting);
  }
  const QString id = arguments.value(QStringLiteral("id")).toString();
  const QString edit = arguments.value(QStringLiteral("edit")).toString();
  const QString finish = arguments.value(QStringLiteral("finish")).toString(QStringLiteral("ok"));
  const QJsonObject inputs = arguments.value(QStringLiteral("inputs")).toObject();
  if (!id.isEmpty() && !edit.isEmpty()) {
    refuse(QStringLiteral("Give either id or edit, not both."));
  }
  const quint64 logStart = logNext() - 1;
  const int depthBefore = undoDepth();
  const bool inSketchBefore = m_mode == Mode::Sketch;
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  mcp::ModalGuard guard;
  QJsonObject result{{QStringLiteral("document"), m_mcpDocumentId}};
  bool changes = false; // the command's steps are the agent's (labelled)

  // A selection input's entries: as ui_select's items.
  const CommandSession::ItemReader reader = [this](const QJsonValue& value, QString& problem) {
    return readItem(value, *this, problem);
  };

  // The open panel: inputs, its preview, then OK, Cancel or left open.
  const auto runPanel = [&] {
    const QPointer<CommandSession> session = m_session;
    // Shared: the panel may end after this call, while the guard lives.
    const auto outcome = std::make_shared<std::pair<bool, bool>>(false, false); // ended, committed
    connect(session.data(), &CommandSession::finished, &guard, [outcome](bool ok) {
      outcome->first = true;
      outcome->second = ok;
    });
    const auto ended = [outcome] { return outcome->first; };
    result.insert(QStringLiteral("command"), session->def().id);
    if (!inputs.isEmpty()) {
      QString problem;
      if (!session->scriptInputs(inputs, reader, problem)) {
        QJsonObject fields{{QStringLiteral("panel"), session->scriptState()}};
        if (finish != QStringLiteral("keep_open")) {
          session->cancel();
          waitUntil(ended, 60000);
          fields.insert(QStringLiteral("cancelled"), true);
        }
        refuseWith(problem, fields);
      }
    }
    // The preview runs a moment after the last change.
    waitUntil([&session] { return !session || !session->scriptBusy(); }, 120000);
    if (!session) {
      result.insert(QStringLiteral("finished"), true);
      return;
    }
    const QJsonObject panel = session->scriptState();
    result.insert(QStringLiteral("panel"), panel);
    if (finish == QStringLiteral("keep_open")) {
      result.insert(QStringLiteral("open"), true);
      return;
    }
    const int appliedBefore = session->appliedCount();
    if (finish == QStringLiteral("cancel")) {
      session->cancel();
    } else {
      session->commit();
    }
    waitUntil([&ended, &session] { return ended() || !session || !session->scriptBusy(); }, 120000);
    if (!ended() && session && session->appliedCount() > appliedBefore) {
      // A command that repeats (BUILD's Place, mitcad#375) stays open after
      // its OK: the agent's OK places one and closes it, keeping what it
      // placed.
      session->cancel();
      waitUntil(ended, 60000);
    }
    if (!ended() && session) {
      // OK was refused: the panel stays open with what it says.
      refuseWith(QStringLiteral("OK of %1 was refused: %2")
                     .arg(session->def().name,
                          panel.value(QStringLiteral("message")).toString(
                              panel.value(QStringLiteral("panel")).toObject().value(QStringLiteral("message")).toString())),
                 {{QStringLiteral("panel"), session->scriptState()}, {QStringLiteral("open"), true}});
    }
    result.insert(QStringLiteral("committed"), outcome->second);
    changes = outcome->second;
  };
  // A feature command's panel opens (on the worker in the AppKit frontend).
  const auto panelStarted = [this] {
#ifdef MITCAD_MACOS_NATIVE_UI
    if (m_nativeWindow) {
      return !m_session || m_nativeForm != nullptr;
    }
#endif
    static_cast<void>(this); // the Qt frontend starts it at once
    return true;
  };

  if (id.isEmpty() && edit.isEmpty()) {
    // What is open ends.
    if (m_session) {
      runPanel();
    } else if (m_mode == Mode::Sketch && finish == QStringLiteral("ok")) {
      result.insert(QStringLiteral("command"), QStringLiteral("sketch.finish"));
      if (!finishSketch()) {
        refuse(QStringLiteral("The sketch stays open: computing the features after it was canceled."));
      }
    } else if (m_mode == Mode::Sketch && finish == QStringLiteral("cancel")) {
      // Every change made since it was opened taken back, then left.
      result.insert(QStringLiteral("command"), QStringLiteral("sketch.cancel"));
      const QString name = m_sketch->model().name;
      for (int step = 0; step < 10000 && m_mode == Mode::Sketch && undoDepth() > m_sketchEnterDepth; ++step) {
        const int depth = undoDepth();
        undo();
        if (undoDepth() >= depth) {
          break; // refused
        }
      }
      if (m_mode == Mode::Sketch && !finishSketch()) {
        refuse(QStringLiteral("The sketch stays open: computing the features after it was canceled."));
      }
      qInfo().noquote() << QStringLiteral("MCP: an agent cancelled the sketch %1").arg(name);
    } else {
      // Nothing to end: what could be run instead (mitcad#1269).
      return listing(QString(), QStringLiteral("Nothing is open to finish: give id (one of these commands) or edit."));
    }
  } else if (!edit.isEmpty()) {
    // As a double-click in the timeline: a sketch opens in sketch mode,
    // another feature in its panel.
    if (m_mode == Mode::PickPlane) {
      refuse(QStringLiteral("Create Sketch waits for a plane to be picked: pick one or cancel it first."));
    }
    if (m_mode != Mode::Idle) {
      refuse(QStringLiteral("Finish the sketch or command first."));
    }
    if (windowReadOnly()) {
      refuse(readOnlyReason());
    }
    if (!sketchExists(edit)) {
      refuse(QStringLiteral("No feature %1 in the design.").arg(mcp::echoed(edit)));
    }
    result.insert(QStringLiteral("command"), QStringLiteral("edit"));
    editFeature(edit);
    waitUntil(panelStarted, 60000);
    if (m_session) {
      runPanel();
    } else if (m_mode != Mode::Sketch) {
      refuse(m_statusLabel->text().isEmpty() ? QStringLiteral("%1 could not be edited.").arg(edit) : m_statusLabel->text());
    }
  } else {
    // New Design waits for the user: it names the design in place in the
    // Projects panel, or asks the name (mitcad#1266); document_new makes it
    // without either.
    if (id == QStringLiteral("file.new_design") || id == QStringLiteral("file.new")) {
      refuse(QStringLiteral("New Design asks the user to name the design, which an agent cannot do: call document_new (optional "
                "name and folder) to make an empty design beside the open ones."));
    }
    const CommandDef* def = m_registry->find(id);
    if (def == nullptr) {
      // The nearest ids, not a file the agent cannot read (mitcad#1269).
      refuse(QStringLiteral("No command %1 in the command registry. %2")
                 .arg(mcp::echoed(id), mcp::nearestCommandsText(*m_registry, id)));
    }
    if (mcp::opensDialog(id)) {
      // Shown for viewing by ui_dialog where the run allows it (mitcad#1735).
      const bool viewable = mcp::runAgentFoldersGiven() && mcp::dialogViewRefusal(id).isEmpty();
      refuse(QStringLiteral("%1 opens a file dialog or another window, which an agent cannot use: %2")
                 .arg(def->name, viewable ? QStringLiteral("ui_dialog with open shows it for viewing.")
                                          : QStringLiteral("run it in the application.")));
    }
    if (windowReadOnly() && !readOnlyCommand(*def)) {
      refuse(readOnlyReason());
    }
    if (!isAvailable(*def)) {
      refuse(QStringLiteral("%1 is not available now (mode %2).").arg(def->name, modeName(m_mode)));
    }
    result.insert(QStringLiteral("command"), id);
    if (def->kind == CommandDef::Kind::Feature) {
      trigger(id);
      waitUntil(panelStarted, 60000);
      if (!m_session) {
        refuse(m_statusLabel->text().isEmpty() ? QStringLiteral("%1 did not start.").arg(def->name) : m_statusLabel->text());
      }
      runPanel();
    } else {
      if (!inputs.isEmpty()) {
        refuse(QStringLiteral("%1 has no panel: it takes no inputs.").arg(def->name));
      }
      trigger(id);
      waitUntil([this] { return !modelBusy(); }, 120000);
      // Entering and finishing sketches are the user's frame, not changes;
      // Undo and Redo move along the steps (a label would clear Redo).
      changes = id != QStringLiteral("sketch.finish") && id != QStringLiteral("sketch.create") &&
                id != QStringLiteral("edit.undo") && id != QStringLiteral("edit.redo");
    }
  }
  if (!guard.opened().isEmpty()) {
    refuse(QStringLiteral("%1 opened the dialog %2, which was closed: an agent cannot answer dialogs; run it in the application.")
               .arg(id.isEmpty() ? edit : id, guard.opened()));
  }
  if (changes && undoDepth() > depthBefore) {
    mcpLabelAgentChange(depthBefore);
    if (inSketchBefore && m_mode == Mode::Sketch) {
      m_mcpSketchEdited = true;
    }
  }
  if (id.startsWith(QStringLiteral("view."))) {
    // A view command (Home, Fit, a standard view, ...): the camera where it
    // goes, at once, and in the reply (mitcad#405).
    m_viewer->settleCamera();
    result.insert(QStringLiteral("camera"), mcpCameraJson());
  }
  const QJsonArray lines = logTexts(logStart, 200);
  QJsonArray previews;
  for (const QJsonValue& line : lines) {
    if (line.toString().startsWith(QStringLiteral("Preview "))) {
      previews.append(line);
    }
  }
  const QJsonObject document = queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
  result.insert(QStringLiteral("mode"), modeName(m_mode));
  const QString editedUid = editingSketchUid(); // either kind (mitcad#1524)
  result.insert(QStringLiteral("sketch"), editedUid.isEmpty() ? QJsonValue() : QJsonValue(editedUid));
  result.insert(QStringLiteral("undo"), document.value(QStringLiteral("undo")));
  result.insert(QStringLiteral("previews"), previews);
  result.insert(QStringLiteral("log"), lines);
  result.insert(QStringLiteral("hint"), m_mode == Mode::PickPlane ? pickPlaneText() : m_statusLabel->text());
  qInfo().noquote() << QStringLiteral("MCP: an agent ran the UI command %1").arg(
      result.value(QStringLiteral("command")).toString());
  return result;
}

} // namespace mitcad
