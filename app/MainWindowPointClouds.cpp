// SPDX-License-Identifier: MIT
// Point clouds (mitcad#2240, app/COMMANDS.md "Point Clouds"): Insert >
// Point Cloud reads a scan's PLY, XYZ, PTS or CSV file into a point cloud
// item of the design (the model's `insert_point_cloud`), Move Point Cloud
// moves one in its component, and the shown clouds are drawn in the view
// (view/PointCloudView.cpp), asked of the model again only when a cloud's
// entry changes. Of a cloud that a shown Point Cloud Deviation colours the
// deviation draws the measured points (MainWindowPrintAnalysis.cpp) and
// the window the others, dimmed (mitcad#2265).
#include "MainWindow.hpp"

#include <cmath>
#include <exception>
#include <memory>
#include <utility>
#include <vector>

#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QtLogging>
#include <QtMath>

#include "commands/CommandSupport.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/DialogFolders.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "view/PointClouds.hpp"

namespace mitcad {
namespace {

const QString kInsertCloudId = QStringLiteral("insert.point_cloud");
const QString kMoveCloudId = QStringLiteral("insert.move_point_cloud");

// The length units a point cloud file may be in.
QVector<QPair<QString, QString>> unitChoices() {
  return {{QStringLiteral("mm"), QObject::tr("Millimeters")},
          {QStringLiteral("cm"), QObject::tr("Centimeters")},
          {QStringLiteral("m"), QObject::tr("Meters")},
          {QStringLiteral("in"), QObject::tr("Inches")},
          {QStringLiteral("ft"), QObject::tr("Feet")}};
}

// The design's point clouds' names, for a panel's Point Cloud choice.
QVector<QPair<QString, QString>> cloudChoices(const CommandState& state) {
  QVector<QPair<QString, QString>> choices;
  for (const QJsonValue& name : state.object(QStringLiteral("clouds")).value(QStringLiteral("names")).toArray()) {
    choices.append({name.toString(), name.toString()});
  }
  return choices;
}

void keepNames(CommandState& state, const CommandContext& context) {
  QJsonArray names;
  try {
    for (const QJsonValue& cloud : context.queryArray(QStringLiteral("point_clouds"))) {
      names.append(cloud.toObject().value(QStringLiteral("name")).toString());
    }
  } catch (const std::exception&) {
  }
  state.setObject(QStringLiteral("clouds"), {{QStringLiteral("names"), names}});
}

} // namespace

void MainWindow::registerPointCloudCommands() {
  // Insert > Point Cloud: the file, its unit and the most points kept.
  auto chosen = std::make_shared<QString>();
  CommandDef insert;
  insert.id = kInsertCloudId;
  insert.name = tr("Point Cloud");
  insert.icon = QStringLiteral("point-cloud");
  insert.tooltip = tr("Brings a scan's points in as a reference: PLY, XYZ, PTS or CSV");
  insert.keywords = {QStringLiteral("scan"), QStringLiteral("ply"), QStringLiteral("xyz"), QStringLiteral("pts"),
                     QStringLiteral("csv"), QStringLiteral("lidar"), QStringLiteral("points"),
                     QStringLiteral("insert point cloud")};
  insert.modelCommand = true;
  insert.inputs = {
      textInput(QStringLiteral("file"), tr("File")),
      noteInput(
          QStringLiteral("choose"),
          [](const CommandState&) { return InputNote{QString(), {tr("Choose File…")}, {}}; },
          [this, chosen](int, const CommandState&, const CommandContext&) {
            const QString path = dialogOpenFileName(
                this, tr("Insert Point Cloud"), dialogFolder(DialogGroup::Import),
                tr("Point clouds (*.ply *.xyz *.pts *.csv *.txt);;All files (*)"));
            if (!path.isEmpty()) {
              rememberDialogFolder(DialogGroup::Import, path);
              *chosen = path;
              qDebug().noquote() << QStringLiteral("Point Cloud file chosen: %1").arg(QFileInfo(path).fileName());
            }
          })
          .withFieldColumn(),
      choiceInput(QStringLiteral("unit"), tr("Unit"), unitChoices(), QStringLiteral("mm"))
          .withTooltip(tr("The length unit the file's numbers are in: point cloud files do not say")),
      valueInput(QStringLiteral("max_points"), tr("Most Points"), ValueKind::Unitless, QStringLiteral("2000000"))
          .withTooltip(tr("A file with more points is thinned evenly to this many, with a warning")),
  };
  insert.modelChanged = [chosen](CommandState& state, const CommandContext&) {
    if (!chosen->isEmpty()) {
      state.setText(QStringLiteral("file"), *chosen);
      chosen->clear();
    }
  };
  insert.build = [](const CommandState& state, const CommandContext&) -> Built {
    const QString file = state.text(QStringLiteral("file")).trimmed();
    if (file.isEmpty()) {
      return Built::failure(tr("Choose a point cloud file."), QStringLiteral("file"));
    }
    if (!QFileInfo(file).isFile()) {
      return Built::failure(tr("There is no file %1.").arg(file), QStringLiteral("file"));
    }
    const double most = state.value(QStringLiteral("max_points"));
    if (!(most >= 1.0 && most <= 20000000.0)) {
      return Built::failure(tr("Most Points: from 1 to 20000000."), QStringLiteral("max_points"));
    }
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("insert_point_cloud")},
                              {QStringLiteral("path"), QFileInfo(file).absoluteFilePath()},
                              {QStringLiteral("unit"), state.choice(QStringLiteral("unit"))},
                              {QStringLiteral("max_points"), static_cast<qint64>(std::llround(most))}};
    // One command: its undo step is the model's ("Insert PointCloud1").
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  insert.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Inserted point cloud from %1")
        .arg(QFileInfo(state.text(QStringLiteral("file")).trimmed()).fileName());
  };
  m_registry->add(insert);

  // Move Point Cloud: distances along the design's axes, then a turn about
  // Z through the design's origin, in the cloud's component.
  CommandDef move;
  move.id = kMoveCloudId;
  move.name = tr("Move Point Cloud");
  move.icon = QStringLiteral("point-cloud");
  move.tooltip = tr("Moves a point cloud onto the design: a turn about Z, then along X, Y and Z");
  move.keywords = {QStringLiteral("scan"), QStringLiteral("align"), QStringLiteral("place"),
                   QStringLiteral("point cloud")};
  move.modelCommand = true;
  move.editedKind = QStringLiteral("point cloud");
  move.inputs = {
      choiceInput(QStringLiteral("cloud"), tr("Point Cloud"), {}, QString()).withChoices(cloudChoices),
      valueInput(QStringLiteral("angle"), tr("Turn About Z"), ValueKind::Angle, QStringLiteral("0 deg")),
      valueInput(QStringLiteral("dx"), tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("dy"), tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("dz"), tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm")),
  };
  move.preselect = [](const Selection&, CommandState& state, const CommandContext& context) {
    keepNames(state, context);
    const QJsonArray names = state.object(QStringLiteral("clouds")).value(QStringLiteral("names")).toArray();
    if (state.choice(QStringLiteral("cloud")).isEmpty() && !names.isEmpty()) {
      state.setChoice(QStringLiteral("cloud"), names.first().toString());
    }
  };
  move.load = [](const QJsonObject& cloud, CommandState& state, const CommandContext& context) {
    keepNames(state, context);
    state.setChoice(QStringLiteral("cloud"), cloud.value(QStringLiteral("name")).toString());
  };
  move.build = [](const CommandState& state, const CommandContext&) -> Built {
    const QString cloud = state.choice(QStringLiteral("cloud"));
    if (cloud.isEmpty()) {
      return Built::failure(tr("Insert a point cloud first (Insert › Point Cloud)."), QStringLiteral("cloud"));
    }
    const double angle = state.value(QStringLiteral("angle"));
    const double dx = state.value(QStringLiteral("dx"));
    const double dy = state.value(QStringLiteral("dy"));
    const double dz = state.value(QStringLiteral("dz"));
    if (!std::isfinite(angle) || !std::isfinite(dx) || !std::isfinite(dy) || !std::isfinite(dz)) {
      return Built::failure(tr("Check the values."), QStringLiteral("dx"));
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("move_point_cloud")},
                        {QStringLiteral("name"), cloud},
                        {QStringLiteral("translate"), QJsonArray{dx, dy, dz}}};
    if (angle != 0.0) {
      command.insert(QStringLiteral("rotate"), QJsonObject{{QStringLiteral("axis"), QJsonArray{0, 0, 1}},
                                                           {QStringLiteral("angle"), qRadiansToDegrees(angle)}});
    }
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  move.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Moved point cloud %1").arg(state.choice(QStringLiteral("cloud")));
  };
  m_registry->add(move);
}

void MainWindow::movePointCloud(const QString& name) {
  if (m_mode != Mode::Idle || !canChangeModel()) {
    return;
  }
  const CommandDef* def = m_registry->find(kMoveCloudId);
  if (def == nullptr) {
    return;
  }
  startFeatureCommand(*def, QString(), {{QStringLiteral("name"), name}});
}

void MainWindow::followPointClouds() {
  // Each shown cloud's points, asked again only when its entry (its
  // placement, its occurrence's, its points' count) changed.
  QHash<QString, PointCloudDrawn> points;
  QHash<QString, QByteArray> keys;
  for (const QJsonValue& value : m_snapshot.pointClouds) {
    const QJsonObject entry = value.toObject();
    if (!entry.value(QStringLiteral("visible")).toBool(true)) {
      continue;
    }
    const QString name = entry.value(QStringLiteral("name")).toString();
    const QByteArray key = compactJson(entry);
    if (m_pointCloudKeys.value(name) == key && m_pointCloudPoints.contains(name)) {
      points.insert(name, m_pointCloudPoints.value(name));
      keys.insert(name, key);
      continue;
    }
    try {
      points.insert(name, pointCloudDrawnOf(queryObject(
                              {{QStringLiteral("query"), QStringLiteral("point_cloud_points")},
                               {QStringLiteral("name"), name}})));
      keys.insert(name, key);
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("Point cloud %1 not read: %2").arg(name, errorText(e));
    }
  }
  m_pointCloudPoints = points;
  m_pointCloudKeys = keys;
  drawPointClouds();
}

void MainWindow::drawPointClouds() {
  std::vector<PointCloudDrawn> drawn;
  for (const QString& name : shownPointClouds(m_snapshot.pointClouds)) {
    if (!m_pointCloudPoints.contains(name)) {
      continue;
    }
    if (name == m_deviationSample.cloud) {
      // The deviation draws the points it measured; the rest dimmed, so
      // the whole cloud stays in sight (mitcad#2265).
      PointCloudDrawn rest = unmeasuredPoints(m_pointCloudPoints.value(name), m_deviationSample);
      if (rest.count() > 0) {
        drawn.push_back(std::move(rest));
      }
      continue;
    }
    drawn.push_back(m_pointCloudPoints.value(name));
  }
  m_viewer->setPointClouds(drawn);
}

} // namespace mitcad
