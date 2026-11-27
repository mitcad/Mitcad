// SPDX-License-Identifier: MIT
// Point cloud tools (mitcad#2516; app/COMMANDS.md "Point Cloud Tools";
// commands.md "Point cloud tools"):
// - Align Point Cloud moves a cloud onto a body (some of its faces, or a
//   mesh body's triangles) or another cloud, from where it is, from its
//   shape or from point pairs clicked in the view (mitcad#2565: a shown
//   point of the cloud, then where it belongs on the target), refined by
//   the best fit. The panel shows the remaining deviation while the inputs
//   change (the model's `point_cloud_alignment`, a model job with its
//   progress and Cancel); OK runs `align_point_cloud`.
// - Filter Point Cloud makes a new cloud without the outliers and noise of
//   one, cropped by a lasso clicked round the points in the view or by a
//   box set by its handles or values (mitcad#2565; `filter_point_cloud`);
//   the cloud it came from is kept, hidden.
// - Reconstruct Mesh builds a closed mesh body of a cloud
//   (`reconstruct_point_cloud`), which the mesh tools then work on.
// They are reached from a cloud's row in the Browser and the command
// search, as Move Point Cloud is.
#include "PointCloudTools.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <exception>
#include <limits>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QStringList>
#include <QtLogging>

#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/CommandSession.hpp"
#include "../framework/Json.hpp"
#include "../framework/LengthUnits.hpp"
#include "../view/PointClouds.hpp"
#include "CommandSupport.hpp"
#include "SketchNormal.hpp"

namespace mitcad {
namespace {

const QString kCloud = QStringLiteral("cloud");
const QString kClouds = QStringLiteral("clouds");
const QString kTarget = QStringLiteral("target");
const QString kBody = QStringLiteral("body");
const QString kFaces = QStringLiteral("faces");
const QString kTargetCloud = QStringLiteral("target_cloud");
const QString kStart = QStringLiteral("start");
const QString kBestFit = QStringLiteral("best_fit");
const QString kMaxDistance = QStringLiteral("max_distance");
const QString kOutliers = QStringLiteral("outliers");
const QString kNeighbours = QStringLiteral("neighbors");
const QString kRatio = QStringLiteral("ratio");
const QString kRadius = QStringLiteral("radius");
const QString kSmooth = QStringLiteral("smooth");
const QString kCrop = QStringLiteral("crop");
const QString kLasso = QStringLiteral("lasso");
const QString kKeepShown = QStringLiteral("keep_shown");
const QString kDetail = QStringLiteral("detail");
const QString kPairs = QStringLiteral("pairs");
const QString kPairPicks = QStringLiteral("pair_points");
const QString kPairPlane = QStringLiteral("pair_plane");
const QString kBoxFor = QStringLiteral("box_for");
// A pair's point picked on a cloud is owned by "cloud:<name>", one picked
// on a body by the body's uid.
const QString kCloudOwner = QStringLiteral("cloud:");

// The design's point clouds' names and boxes (in the design, at the
// marker), kept in the panel's state.
void keepClouds(CommandState& state, const CommandContext& context) {
  QJsonArray names;
  QJsonObject boxes;
  try {
    for (const QJsonValue& value : context.queryArray(QStringLiteral("point_clouds"))) {
      const QJsonObject cloud = value.toObject();
      const QString name = cloud.value(QStringLiteral("name")).toString();
      names.append(name);
      boxes.insert(name, cloud.value(QStringLiteral("bounds")));
    }
  } catch (const std::exception&) {
  }
  state.setObject(kClouds, {{QStringLiteral("names"), names}, {QStringLiteral("bounds"), boxes}});
}

QJsonArray cloudNames(const CommandState& state) {
  return state.object(kClouds).value(QStringLiteral("names")).toArray();
}

QVector<QPair<QString, QString>> cloudChoices(const CommandState& state) {
  QVector<QPair<QString, QString>> choices;
  for (const QJsonValue& name : cloudNames(state)) {
    choices.append({name.toString(), name.toString()});
  }
  return choices;
}

// The cloud a panel starts with: the one its Browser row gave, else the
// design's first.
void startCloud(CommandState& state, const CommandContext& context, const ChosenCloud& chosen) {
  keepClouds(state, context);
  const QString given = chosen ? chosen() : QString();
  if (!given.isEmpty() && cloudNames(state).contains(given)) {
    state.setChoice(kCloud, given);
  } else if (state.choice(kCloud).isEmpty() && !cloudNames(state).isEmpty()) {
    state.setChoice(kCloud, cloudNames(state).first().toString());
  }
}

// The middle of the chosen cloud's box in the design.
std::optional<gp_Pnt> cloudMiddle(const CommandState& state) {
  const QJsonArray bounds =
      state.object(kClouds).value(QStringLiteral("bounds")).toObject().value(state.choice(kCloud)).toArray();
  if (bounds.size() != 2) {
    return std::nullopt;
  }
  const QJsonArray low = bounds.at(0).toArray();
  const QJsonArray high = bounds.at(1).toArray();
  if (low.size() != 3 || high.size() != 3) {
    return std::nullopt;
  }
  return gp_Pnt(0.5 * (low.at(0).toDouble() + high.at(0).toDouble()),
                0.5 * (low.at(1).toDouble() + high.at(1).toDouble()),
                0.5 * (low.at(2).toDouble() + high.at(2).toDouble()));
}

QString noCloud() { return QObject::tr("Insert a point cloud first (Insert › Point Cloud)."); }

QJsonArray pointJson(const gp_Pnt& p) { return QJsonArray{p.X(), p.Y(), p.Z()}; }

std::optional<gp_Pnt> pointOf(const QJsonValue& value) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() != 3) {
    return std::nullopt;
  }
  return gp_Pnt(xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble());
}

// The points of the design's clouds as the view draws them (the model's
// `point_cloud_points`), each read again when its entry of `point_clouds`
// changes (it moved, or another cloud took its name). A hidden cloud has
// none: only what the view shows is picked.
class ShownClouds {
public:
  const PointCloudDrawn* of(const QString& name, const CommandContext& context) {
    QJsonObject entry;
    try {
      for (const QJsonValue& value : context.queryArray(QStringLiteral("point_clouds"))) {
        if (value.toObject().value(QStringLiteral("name")).toString() == name) {
          entry = value.toObject();
          break;
        }
      }
    } catch (const std::exception&) {
      return nullptr;
    }
    if (entry.isEmpty() || !entry.value(QStringLiteral("visible")).toBool(true)) {
      return nullptr;
    }
    const QByteArray key = compactJson(entry);
    const auto known = m_read.find(name);
    if (known != m_read.end() && known->first == key) {
      return &known->second;
    }
    try {
      PointCloudDrawn drawn = pointCloudDrawnOf(context.queryObject(
          {{QStringLiteral("query"), QStringLiteral("point_cloud_points")}, {QStringLiteral("name"), name}}));
      // A large scan is kept once: the clouds of the panel that is open.
      if (m_read.size() > 2) {
        m_read.clear();
      }
      m_read.insert(name, {key, std::move(drawn)});
    } catch (const std::exception&) {
      return nullptr;
    }
    return &m_read.find(name)->second;
  }

private:
  QHash<QString, std::pair<QByteArray, PointCloudDrawn>> m_read;
};

// The cloud's point that the view shows under the pointer: of its points
// within a few pixels of the pointer's ray (through `onRay`, towards the
// viewer), the one nearest the ray, or of those about as near the one
// nearest the viewer, which the view draws in front. None off the cloud or
// without a view.
std::optional<gp_Pnt> pointUnderPointer(const PointCloudDrawn& cloud, const gp_Pnt& onRay,
                                        const CommandContext& context) {
  const std::optional<gp_Dir> toward = context.towardViewer(onRay);
  const double pixel = context.pixelSize(onRay);
  if (!toward || !std::isfinite(pixel) || pixel <= 0.0) {
    return std::nullopt;
  }
  constexpr double kReach = 8.0; // pixels from the pointer
  constexpr double kAsNear = 1.5; // pixels: about as near the ray
  const gp_XYZ way = toward->XYZ();
  // Measured in millimetres first, with room for a perspective's larger
  // pixels farther away; in pixels at each point then.
  const double wide = 4.0 * kReach * pixel;
  struct Near {
    gp_Pnt at;
    double pixels;
    double along;
  };
  std::vector<Near> candidates;
  const auto& m = cloud.linear;
  const auto& t = cloud.translation;
  for (std::size_t i = 0; i < cloud.count(); ++i) {
    const double x = cloud.points[3 * i];
    const double y = cloud.points[3 * i + 1];
    const double z = cloud.points[3 * i + 2];
    const gp_Pnt q(m[0][0] * x + m[0][1] * y + m[0][2] * z + t[0], m[1][0] * x + m[1][1] * y + m[1][2] * z + t[1],
                   m[2][0] * x + m[2][1] * y + m[2][2] * z + t[2]);
    const gp_XYZ from = q.XYZ() - onRay.XYZ();
    const double along = from.Dot(way);
    const double off = (from - way * along).Modulus();
    if (off > wide) {
      continue;
    }
    const double size = context.pixelSize(q);
    const double pixels = off / (std::isfinite(size) && size > 0.0 ? size : pixel);
    if (pixels <= kReach) {
      candidates.push_back({q, pixels, along});
    }
  }
  if (candidates.empty()) {
    return std::nullopt;
  }
  double nearest = candidates.front().pixels;
  for (const Near& each : candidates) {
    nearest = std::min(nearest, each.pixels);
  }
  const Near* front = nullptr;
  for (const Near& each : candidates) {
    if (each.pixels <= nearest + kAsNear && (front == nullptr || each.along > front->along)) {
      front = &each;
    }
  }
  return front->at;
}

// A distance with three decimals, for the panel and the log.
QString mm(double value) { return QStringLiteral("%1 mm").arg(value, 0, 'f', 3); }

// A count from a value box: a whole number from `low` to `high`, or -1.
int wholeIn(double value, int low, int high) {
  if (!std::isfinite(value) || value < low || value > high || std::floor(value) != value) {
    return -1;
  }
  return static_cast<int>(value);
}

// Align Point Cloud ----------------------------------------------------------

bool byPairs(const CommandState& state) { return state.choice(kStart) == QStringLiteral("pairs"); }

bool toCloud(const CommandState& state) { return state.choice(kTarget) == QStringLiteral("cloud"); }

// The points picked for the pairs, in turn on the cloud and on the target:
// {"points": [[x, y, z], ...], "owners": ["cloud:Scan", "F1.b0", ...]}.
QJsonArray pairPoints(const CommandState& state) {
  return state.object(kPairPicks).value(QStringLiteral("points")).toArray();
}

QJsonArray pairOwners(const CommandState& state) {
  return state.object(kPairPicks).value(QStringLiteral("owners")).toArray();
}

void setPairPicks(CommandState& state, const QJsonArray& points, const QJsonArray& owners) {
  state.setObject(kPairPicks, QJsonObject{{QStringLiteral("points"), points}, {QStringLiteral("owners"), owners}});
}

QString chosenBody(const CommandState& state) {
  const Selection& bodies = state.items(kBody);
  return bodies.isEmpty() ? QString() : bodies.first().owner;
}

// Whether the next click of the pairs is where a point belongs on the
// target body (a face of it picked, with where the click hit it).
bool bodyClickNext(const CommandState& state) {
  return byPairs(state) && !toCloud(state) && pairPoints(state).size() % 2 == 1;
}

// The way towards the viewer at the cloud's middle, which the plane of the
// clicks on clouds faces: kept while the view does not turn.
void keepPairPlane(CommandState& state, const CommandContext& context) {
  const std::optional<gp_Pnt> middle = cloudMiddle(state);
  const std::optional<gp_Dir> toward = middle ? context.towardViewer(*middle) : std::nullopt;
  if (!toward) {
    return;
  }
  const QJsonArray normal{toward->X(), toward->Y(), toward->Z()};
  if (state.object(kPairPlane).value(QStringLiteral("normal")).toArray() != normal) {
    state.setObject(kPairPlane, QJsonObject{{QStringLiteral("normal"), normal}});
  }
}

// The picked pairs as the model takes them ({"cloud", "target"}), or what
// the panel waits for.
Built pairsOf(const CommandState& state, const QString& cloudOwner, const QString& targetOwner,
              const QString& target) {
  const QJsonArray points = pairPoints(state);
  const QJsonArray owners = pairOwners(state);
  const QString wrongWay = QObject::tr(
      "Pick each pair's first point on the cloud and its second on %1; Backspace takes points back.")
                               .arg(target);
  QJsonArray pairs;
  for (int k = 0; k + 1 < points.size(); k += 2) {
    if (owners.at(k).toString() != cloudOwner || owners.at(k + 1).toString() != targetOwner) {
      return Built::failure(wrongWay, kPairs);
    }
    pairs.append(QJsonObject{{QStringLiteral("cloud"), points.at(k)}, {QStringLiteral("target"), points.at(k + 1)}});
  }
  if (points.isEmpty()) {
    return Built::asking(
        QObject::tr("Click a point of the cloud, then where it belongs on %1; three pairs or more.").arg(target),
        kPairs);
  }
  if (points.size() % 2 == 1) {
    if (owners.last().toString() != cloudOwner) {
      return Built::failure(wrongWay, kPairs);
    }
    return Built::asking(QObject::tr("Click where that point belongs on %1.").arg(target), kPairs);
  }
  if (pairs.size() < 3) {
    return Built::asking(
        QObject::tr("%1 of 3 point pairs picked: click a point of the cloud, then where it belongs on %2.")
            .arg(pairs.size())
            .arg(target),
        kPairs);
  }
  return Built::of({{kPairs, pairs}});
}

QString targetName(const QJsonObject& fields, const CommandContext& context) {
  return fields.contains(kTargetCloud) ? fields.value(kTargetCloud).toString()
                                       : context.bodyName(fields.value(kBody).toString());
}

// The fields of `point_cloud_alignment` and `align_point_cloud` of the
// panel, or what is missing.
Built alignFields(const CommandState& state, const CommandContext& context) {
  const QString cloud = state.choice(kCloud);
  if (cloud.isEmpty()) {
    return Built::failure(noCloud(), kCloud);
  }
  QJsonObject fields{{QStringLiteral("name"), cloud}};
  if (state.choice(kTarget) == QStringLiteral("cloud")) {
    const QString other = state.choice(kTargetCloud);
    if (other.isEmpty() || other == cloud) {
      return Built::failure(QObject::tr("Choose another point cloud to align to."), kTargetCloud);
    }
    fields.insert(kTargetCloud, other);
  } else {
    const Selection bodies = state.items(kBody);
    if (bodies.isEmpty()) {
      return Built::failure(QObject::tr("Pick the body to align the cloud to."), kBody);
    }
    const QString body = bodies.first().owner;
    fields.insert(kBody, body);
    QJsonArray faces;
    for (const SelectionItem& item : state.items(kFaces)) {
      if (item.owner != body) {
        return Built::failure(QObject::tr("Pick faces of the chosen body, or none to match all of them."), kFaces);
      }
      faces.append(item.name);
    }
    if (!faces.isEmpty()) {
      fields.insert(kFaces, faces);
    }
  }
  const bool bestFit = state.checked(kBestFit);
  const QString start = state.choice(kStart);
  if (start == QStringLiteral("pairs")) {
    // The pairs give the start, the best fit (when on) refines it.
    const QString targetOwner = fields.contains(kTargetCloud) ? kCloudOwner + fields.value(kTargetCloud).toString()
                                                              : fields.value(kBody).toString();
    const Built pairs = pairsOf(state, kCloudOwner + cloud, targetOwner, targetName(fields, context));
    if (!pairs.error.isEmpty()) {
      return pairs;
    }
    fields.insert(kStart, start);
    fields.insert(kPairs, pairs.def.value(kPairs));
  } else {
    if (start != QStringLiteral("shape") && !bestFit) {
      return Built::failure(QObject::tr("Turn Best Fit on, or start by the shape or by point pairs."), kBestFit);
    }
    fields.insert(kStart, start == QStringLiteral("shape") ? start : QStringLiteral("placed"));
  }
  fields.insert(kBestFit, bestFit);
  const double distance = state.value(kMaxDistance);
  if (!std::isfinite(distance) || distance < 0.0) {
    return Built::failure(QObject::tr("Largest Distance: zero or more."), kMaxDistance);
  }
  if (distance > 0.0) {
    fields.insert(kMaxDistance, distance);
  }
  return Built::of(fields);
}

// A query that may take long (a large cloud's best fit) as a model job
// with its progress and Cancel, as Point Cloud Deviation's.
QJsonObject slowQuery(const CommandContext& context, const QJsonObject& request) {
  if (const auto* host = dynamic_cast<const CommandHost*>(&context)) {
    return const_cast<CommandHost*>(host)->jobQuery(request, QObject::tr("Aligning the points"));
  }
  return context.queryObject(request);
}

// The answer in words, for the panel.
QString alignmentText(const QJsonObject& fields, const QJsonObject& answer, const CommandContext& context) {
  const int matched = answer.value(QStringLiteral("matched")).toInt();
  const int sampled = answer.value(QStringLiteral("sampled")).toInt();
  QStringList lines;
  lines << QObject::tr("%1 onto %2: %3 of %4 points matched.")
               .arg(fields.value(QStringLiteral("name")).toString().toHtmlEscaped(),
                    targetName(fields, context).toHtmlEscaped())
               .arg(matched)
               .arg(sampled);
  lines << QObject::tr("Remaining deviation: RMS %1, mean %2, largest %3.")
               .arg(mm(answer.value(QStringLiteral("rms_mm")).toDouble()),
                    mm(answer.value(QStringLiteral("mean_mm")).toDouble()),
                    mm(answer.value(QStringLiteral("max_mm")).toDouble()));
  if (answer.contains(QStringLiteral("pairs_rms_mm"))) {
    lines << QObject::tr("The picked pairs lie RMS %1 apart after the move.")
                 .arg(mm(answer.value(QStringLiteral("pairs_rms_mm")).toDouble()));
  }
  if (answer.value(QStringLiteral("best_fit")).toBool()) {
    const int rounds = answer.value(QStringLiteral("iterations")).toInt();
    lines << (answer.value(QStringLiteral("converged")).toBool()
                  ? QObject::tr("The best fit settled in %n round(s).", nullptr, rounds)
                  : QObject::tr("The best fit did not settle in %n round(s): its result may still improve.", nullptr,
                                rounds));
  }
  return lines.join(QStringLiteral("<br>"));
}

// The answer in a line, for the log.
QString alignmentLog(const QJsonObject& fields, const QJsonObject& answer, const CommandContext& context) {
  // With pairs, their distance after the move too.
  const QString pairs =
      answer.contains(QStringLiteral("pairs_rms_mm"))
          ? QStringLiteral(", pairs RMS %1").arg(mm(answer.value(QStringLiteral("pairs_rms_mm")).toDouble()))
          : QString();
  return QStringLiteral("%1 to %2: RMS %3, mean %4, max %5, %6 of %7 points, %8 rounds%9")
      .arg(fields.value(QStringLiteral("name")).toString(), targetName(fields, context),
           mm(answer.value(QStringLiteral("rms_mm")).toDouble()),
           mm(answer.value(QStringLiteral("mean_mm")).toDouble()),
           mm(answer.value(QStringLiteral("max_mm")).toDouble()))
      .arg(answer.value(QStringLiteral("matched")).toInt())
      .arg(answer.value(QStringLiteral("sampled")).toInt())
      .arg(answer.value(QStringLiteral("iterations")).toInt())
      .arg(answer.value(QStringLiteral("converged")).toBool() ? QStringLiteral(", settled") : QString()) +
         pairs;
}

// The body the selection made before stands for (a body, or a face's
// body), else the first shown body; with the faces of it selected.
void startBody(const Selection& before, CommandState& state, const CommandContext& context) {
  QString body;
  for (const SelectionItem& item : before) {
    if (item.kind == SelectKind::Body || item.kind == SelectKind::Face) {
      body = item.owner;
      break;
    }
  }
  if (body.isEmpty()) {
    try {
      for (const QJsonValue& value : context.queryArray(QStringLiteral("bodies"))) {
        const QJsonObject entry = value.toObject();
        if (entry.value(QStringLiteral("visible")).toBool(true)) {
          body = entry.value(QStringLiteral("uid")).toString();
          break;
        }
      }
    } catch (const std::exception&) {
    }
  }
  if (body.isEmpty()) {
    return;
  }
  state.setItems(kBody, cmd::bodiesOf(QJsonArray{body}));
  Selection faces;
  for (const SelectionItem& item : before) {
    if (item.kind == SelectKind::Face && item.owner == body) {
      faces.append(item);
    }
  }
  state.setItems(kFaces, faces);
}

// Filter Point Cloud ---------------------------------------------------------

bool cropping(const CommandState& state) {
  return state.choice(kCrop) == QStringLiteral("keep") || state.choice(kCrop) == QStringLiteral("remove");
}

QJsonArray lassoPoints(const CommandState& state) {
  return state.object(kLasso).value(QStringLiteral("points")).toArray();
}

// The crop box (mitcad#2565): its six sides as value inputs, From and To
// along each of the design's axes.
bool boxCropping(const CommandState& state) {
  return state.choice(kCrop) == QStringLiteral("keep_box") || state.choice(kCrop) == QStringLiteral("remove_box");
}

QString boxKey(int axis, bool high) {
  const QString name = axis == 0 ? QStringLiteral("x") : axis == 1 ? QStringLiteral("y") : QStringLiteral("z");
  return QStringLiteral("box_") + name + (high ? QStringLiteral("_max") : QStringLiteral("_min"));
}

QString boxLabel(int axis, bool high) {
  switch (axis) {
  case 0:
    return high ? QObject::tr("X To") : QObject::tr("X From");
  case 1:
    return high ? QObject::tr("Y To") : QObject::tr("Y From");
  default:
    return high ? QObject::tr("Z To") : QObject::tr("Z From");
  }
}

// The box's corners as the values give them; none while one is no number.
std::optional<std::pair<gp_XYZ, gp_XYZ>> boxCorners(const CommandState& state) {
  gp_XYZ low;
  gp_XYZ high;
  for (int axis = 0; axis < 3; ++axis) {
    const double from = state.value(boxKey(axis, false));
    const double to = state.value(boxKey(axis, true));
    if (!std::isfinite(from) || !std::isfinite(to)) {
      return std::nullopt;
    }
    low.SetCoord(axis + 1, from);
    high.SetCoord(axis + 1, to);
  }
  return std::make_pair(low, high);
}

// The box round the chosen cloud, a little larger so that no point lies on
// its sides, as the values start (once per cloud).
void startBox(CommandState& state, const CommandContext& context) {
  const QString cloud = state.choice(kCloud);
  if (!boxCropping(state) || cloud.isEmpty() || state.object(kBoxFor).value(kCloud).toString() == cloud) {
    return;
  }
  const QJsonArray bounds = state.object(kClouds).value(QStringLiteral("bounds")).toObject().value(cloud).toArray();
  if (bounds.size() != 2) {
    return;
  }
  const std::optional<gp_Pnt> low = pointOf(bounds.at(0));
  const std::optional<gp_Pnt> high = pointOf(bounds.at(1));
  if (!low || !high) {
    return;
  }
  const gp_XYZ size = high->XYZ() - low->XYZ();
  const double margin = std::max(0.01, 0.01 * std::max({size.X(), size.Y(), size.Z()}));
  const QString unit = context.lengthUnit();
  for (int axis = 1; axis <= 3; ++axis) {
    const QString from = boxKey(axis - 1, false);
    const QString to = boxKey(axis - 1, true);
    state.setText(from, lengthInUnit(low->Coord(axis) - margin, unit));
    state.setText(to, lengthInUnit(high->Coord(axis) + margin, unit));
    // Evaluated again from the text (after a choice's onChange).
    state.setValue(from, std::numeric_limits<double>::quiet_NaN());
    state.setValue(to, std::numeric_limits<double>::quiet_NaN());
  }
  state.setObject(kBoxFor, QJsonObject{{kCloud, cloud}});
}

// A side's value input, with its handle: an arrow at the middle of that
// side of the box, pointing out of it.
InputDef boxInput(int axis, bool high) {
  const QString key = boxKey(axis, high);
  return valueInput(key, boxLabel(axis, high), ValueKind::Length, QStringLiteral("0 mm"))
      .withTooltip(high ? QObject::tr("Where the box ends along this axis of the design; drag its arrow in the view")
                        : QObject::tr("Where the box starts along this axis of the design; drag its arrow in the view"))
      .withVisible(boxCropping)
      .withManipulator([axis, high](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
        const auto corners = boxCorners(state);
        if (!boxCropping(state) || !corners) {
          return std::nullopt;
        }
        const gp_XYZ& low = corners->first;
        const gp_XYZ& top = corners->second;
        // From the opposite side, which the drag does not move: the arrow's
        // knob is at the value, the To side's no lower than the From side.
        gp_XYZ origin = (low + top) * 0.5;
        const double other = high ? low.Coord(axis + 1) : top.Coord(axis + 1);
        origin.SetCoord(axis + 1, other);
        gp_XYZ way(0.0, 0.0, 0.0);
        way.SetCoord(axis + 1, high ? 1.0 : -1.0);
        Manipulator handle = cmd::arrow(gp_Pnt(origin), gp_Dir(way), other);
        if (high) {
          handle.minimum = other;
        } else {
          handle.factor = -1.0;
        }
        return handle;
      });
}

// The panel's `filter_point_cloud`, or what is missing.
Built filterCommand(const CommandState& state) {
  const QString cloud = state.choice(kCloud);
  if (cloud.isEmpty()) {
    return Built::failure(noCloud(), kCloud);
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("filter_point_cloud")}, {QStringLiteral("name"), cloud}};
  const QString outliers = state.choice(kOutliers);
  const bool smooth = state.checked(kSmooth);
  const bool crop = cropping(state);
  const bool box = boxCropping(state);
  if (outliers == QStringLiteral("none") && !smooth && !crop && !box) {
    return Built::failure(
        QObject::tr("Choose what to take out: outliers, noise, or the points of a lasso or a box."), kOutliers);
  }
  const int neighbours = wholeIn(state.value(kNeighbours), 1, 200);
  if ((outliers != QStringLiteral("none") || smooth) && neighbours < 0) {
    return Built::failure(QObject::tr("Neighbors: a whole number from 1 to 200."), kNeighbours);
  }
  if (outliers == QStringLiteral("statistical")) {
    const double ratio = state.value(kRatio);
    if (!std::isfinite(ratio) || ratio <= 0.0) {
      return Built::failure(QObject::tr("Spread: more than zero."), kRatio);
    }
    command.insert(QStringLiteral("statistical"),
                   QJsonObject{{kNeighbours, neighbours}, {QStringLiteral("ratio"), ratio}});
  } else if (outliers == QStringLiteral("radius")) {
    const double radius = state.value(kRadius);
    if (!std::isfinite(radius) || radius <= 0.0) {
      return Built::failure(QObject::tr("Radius: more than zero."), kRadius);
    }
    command.insert(QStringLiteral("radius"), QJsonObject{{QStringLiteral("radius"), radius}, {kNeighbours, neighbours}});
  }
  if (smooth) {
    if (neighbours < 3) {
      return Built::failure(QObject::tr("Smoothing the noise needs 3 neighbors or more."), kNeighbours);
    }
    command.insert(QStringLiteral("smooth"), QJsonObject{{kNeighbours, neighbours}});
  }
  if (crop) {
    const QJsonObject lasso = state.object(kLasso);
    if (lassoPoints(state).size() < 3) {
      return Built::asking(QObject::tr("Click round the points in the view, three points or more."));
    }
    command.insert(QStringLiteral("lasso"),
                   QJsonObject{{QStringLiteral("normal"), lasso.value(QStringLiteral("normal"))},
                               {QStringLiteral("points"), lasso.value(QStringLiteral("points"))},
                               {QStringLiteral("inside"), state.choice(kCrop) == QStringLiteral("keep")}});
  }
  if (box) {
    const auto corners = boxCorners(state);
    if (!corners) {
      return Built::failure(QObject::tr("Check the box's values."), boxKey(0, false));
    }
    QJsonArray low;
    QJsonArray high;
    for (int axis = 0; axis < 3; ++axis) {
      const double from = corners->first.Coord(axis + 1);
      const double to = corners->second.Coord(axis + 1);
      if (from >= to) {
        return Built::failure(
            QObject::tr("%1 must be less than %2.").arg(boxLabel(axis, false), boxLabel(axis, true)),
            boxKey(axis, true));
      }
      low.append(from);
      high.append(to);
    }
    command.insert(QStringLiteral("crop_box"),
                   QJsonObject{{QStringLiteral("min"), low},
                               {QStringLiteral("max"), high},
                               {QStringLiteral("inside"), state.choice(kCrop) == QStringLiteral("keep_box")}});
  }
  if (state.checked(kKeepShown)) {
    command.insert(kKeepShown, true);
  }
  return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
}

} // namespace

CommandDef pointCloudAlignCommand(DocumentHost& host, ChosenCloud chosen) {
  CommandDef def;
  def.id = QStringLiteral("insert.align_point_cloud");
  def.name = QObject::tr("Align Point Cloud");
  def.icon = QStringLiteral("point-cloud-align");
  def.tooltip = QObject::tr("Moves a point cloud onto a body or another cloud by point pairs and a best fit");
  def.keywords = {QStringLiteral("scan"),      QStringLiteral("align"), QStringLiteral("register"),
                  QStringLiteral("best fit"),  QStringLiteral("icp"),   QStringLiteral("point cloud"),
                  QStringLiteral("registration"), QStringLiteral("point pairs")};
  const QVector<QPair<QString, QString>> targets = {{QStringLiteral("body"), QObject::tr("Body")},
                                                    {QStringLiteral("cloud"), QObject::tr("Point Cloud")}};
  const QVector<QPair<QString, QString>> starts = {{QStringLiteral("placed"), QObject::tr("Where It Is")},
                                                   {QStringLiteral("shape"), QObject::tr("By Shape")},
                                                   {QStringLiteral("pairs"), QObject::tr("Point Pairs")}};
  // The points of the clouds clicked for the pairs, read once a panel.
  const auto shown = std::make_shared<ShownClouds>();
  const auto toBody = [](const CommandState& state) { return state.choice(kTarget) != QStringLiteral("cloud"); };
  def.inputs = {
      choiceInput(kCloud, QObject::tr("Point Cloud"), {}, QString())
          .withChoices(cloudChoices)
          .withTooltip(QObject::tr("The scanned points to move")),
      choiceInput(kTarget, QObject::tr("Align To"), targets, QStringLiteral("body"))
          .withTooltip(QObject::tr("What the points are moved onto: a body's faces or another point cloud")),
      selectionInput(kBody, QObject::tr("Body"), SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("The body the points are moved onto; a mesh body's triangles"))
          .withVisible(toBody),
      selectionInput(kFaces, QObject::tr("Faces"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Match the points to these faces of the body only; none: to all of them"))
          .withVisible(toBody),
      choiceInput(kTargetCloud, QObject::tr("Target Cloud"), {}, QString())
          .withChoices([](const CommandState& state) {
            QVector<QPair<QString, QString>> choices;
            for (const QJsonValue& name : cloudNames(state)) {
              if (name.toString() != state.choice(kCloud)) {
                choices.append({name.toString(), name.toString()});
              }
            }
            return choices;
          })
          .withTooltip(QObject::tr("The point cloud the points are moved onto"))
          .withVisible([](const CommandState& state) { return state.choice(kTarget) == QStringLiteral("cloud"); }),
      choiceInput(kStart, QObject::tr("Start"), starts, QStringLiteral("placed"))
          .withTooltip(QObject::tr("Where the best fit starts: where the cloud lies, by its shape, or by point "
                                   "pairs clicked in the view")),
      // The pairs' clicks: a shown point of the cloud (on the plane of the
      // clicks, cursorPlaneClicked), then where it belongs: a face of the
      // target body picked here, or a shown point of the target cloud.
      selectionInput(kPairs, QObject::tr("Point Pairs"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("A point of the cloud, then where it belongs on the target; three pairs or more"))
          .withVisible(byPairs)
          .withActiveWhenShown()
          .withPickRule(
              [](const CommandState& state) {
                return bodyClickNext(state) ? SelectFilter(SelectKind::Face) : SelectFilter();
              },
              [](const SelectionItem& item, const CommandState& state) {
                return bodyClickNext(state) && item.owner == chosenBody(state);
              })
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& context) {
            if (!item.at || !bodyClickNext(state)) {
              return;
            }
            // The click hit the face in its component's coordinates; the
            // pairs are in the design's, as the cloud's points (mitcad#2566):
            // a body of a placed component is taken through the placement
            // of the occurrence it was clicked in.
            QJsonArray occurrences;
            if (!item.occurrence.isEmpty()) {
              try {
                occurrences = cmd::componentsOf(context).value(QStringLiteral("occurrences")).toArray();
              } catch (const std::exception&) {
              }
            }
            const std::optional<gp_Pnt> at = cmd::clickInDesign(*item.at, occurrences, item.occurrence);
            if (!at) {
              qDebug().noquote() << QStringLiteral("Align Point Cloud: the click's occurrence %1 is not known")
                                        .arg(item.occurrence);
              return;
            }
            QJsonArray points = pairPoints(state);
            QJsonArray owners = pairOwners(state);
            points.append(pointJson(*at));
            owners.append(item.owner);
            setPairPicks(state, points, owners);
            qDebug().noquote() << QStringLiteral("Align Point Cloud target point: %1 at %2, %3, %4 in the design")
                                      .arg(item.owner)
                                      .arg(at->X(), 0, 'f', 3)
                                      .arg(at->Y(), 0, 'f', 3)
                                      .arg(at->Z(), 0, 'f', 3);
            qDebug().noquote() << QStringLiteral("Align Point Cloud points: %1").arg(points.size());
          }),
      checkInput(kBestFit, QObject::tr("Best Fit"), true)
          .withTooltip(QObject::tr("Refines the start by matching each point to the nearest point of the target")),
      valueInput(kMaxDistance, QObject::tr("Largest Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Points farther than this from the target are left out; 0: the far ones")),
  };
  def.preselect = [chosen](const Selection& before, CommandState& state, const CommandContext& context) {
    startCloud(state, context, chosen);
    startBody(before, state, context);
    keepPairPlane(state, context);
  };
  // The pairs' clicks on clouds: on a plane through the cloud's middle
  // facing the view, the click's point there standing for the pointer's
  // ray; the cloud's point the view shows under it is taken.
  def.cursorPlane = [](const CommandState& state) -> std::optional<gp_Ax3> {
    if (!byPairs(state)) {
      return std::nullopt;
    }
    const std::optional<gp_Pnt> middle = cloudMiddle(state);
    if (!middle) {
      return std::nullopt;
    }
    const std::optional<gp_Pnt> normal = pointOf(state.object(kPairPlane).value(QStringLiteral("normal")));
    if (normal && gp_Vec(normal->XYZ()).Magnitude() > 1e-12) {
      return gp_Ax3(*middle, gp_Dir(normal->XYZ()));
    }
    return gp_Ax3(*middle, gp_Dir(0.0, 0.0, 1.0));
  };
  def.cursorPlaneMoved = [](const std::optional<gp_Pnt>&, CommandState& state, const CommandContext& context) {
    keepPairPlane(state, context); // the view may have turned
    return false;
  };
  def.cursorPlaneClicked = [shown](const gp_Pnt& point, CommandState& state, const CommandContext& context) {
    if (!byPairs(state)) {
      return false;
    }
    QJsonArray points = pairPoints(state);
    QJsonArray owners = pairOwners(state);
    const bool onCloud = points.size() % 2 == 0;
    const QString cloud = onCloud ? state.choice(kCloud) : toCloud(state) ? state.choice(kTargetCloud) : QString();
    if (cloud.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Align Point Cloud: the click is not on the target body");
      return false;
    }
    const PointCloudDrawn* drawn = shown->of(cloud, context);
    const std::optional<gp_Pnt> hit = drawn != nullptr ? pointUnderPointer(*drawn, point, context) : std::nullopt;
    if (!hit) {
      qDebug().noquote() << QStringLiteral("Align Point Cloud: no point of %1 under the pointer").arg(cloud);
      return false;
    }
    points.append(pointJson(*hit));
    owners.append(kCloudOwner + cloud);
    setPairPicks(state, points, owners);
    keepPairPlane(state, context);
    qDebug().noquote() << QStringLiteral("Align Point Cloud points: %1 (%2 at %3, %4, %5)")
                              .arg(points.size())
                              .arg(cloud)
                              .arg(hit->X(), 0, 'f', 3)
                              .arg(hit->Y(), 0, 'f', 3)
                              .arg(hit->Z(), 0, 'f', 3);
    return true;
  };
  // Backspace takes the last picked point back.
  def.backspace = [](CommandState& state, const CommandContext&) {
    QJsonArray points = pairPoints(state);
    QJsonArray owners = pairOwners(state);
    if (!byPairs(state) || points.isEmpty()) {
      return false;
    }
    points.removeLast();
    if (!owners.isEmpty()) {
      owners.removeLast();
    }
    setPairPicks(state, points, owners);
    qDebug().noquote() << QStringLiteral("Align Point Cloud points: %1").arg(points.size());
    return true;
  };
  // Each pair a line from the cloud's point to the target's, each point a
  // small cross.
  def.viewLines = [](const CommandState& state, const CommandContext& context) {
    std::vector<std::vector<gp_Pnt>> lines;
    if (!byPairs(state)) {
      return lines;
    }
    const QJsonArray points = pairPoints(state);
    for (int k = 0; k < points.size(); ++k) {
      const std::optional<gp_Pnt> at = pointOf(points.at(k));
      if (!at) {
        continue;
      }
      if (k % 2 == 1) {
        if (const std::optional<gp_Pnt> from = pointOf(points.at(k - 1))) {
          lines.push_back({*from, *at});
        }
      }
      const double pixel = context.pixelSize(*at);
      const double arm = std::isfinite(pixel) && pixel > 0.0 ? 5.0 * pixel : 0.5;
      for (int axis = 1; axis <= 3; ++axis) {
        gp_XYZ step(0.0, 0.0, 0.0);
        step.SetCoord(axis, arm);
        lines.push_back({gp_Pnt(at->XYZ() - step), gp_Pnt(at->XYZ() + step)});
      }
    }
    return lines;
  };
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const Built built = alignFields(state, context);
    if (!built.error.isEmpty()) {
      result.error = built.error;
      if (built.asks) {
        result.viewHint = built.error; // the next click, over the view
      }
      result.log = QStringLiteral("waiting: %1").arg(built.error);
      return result;
    }
    QJsonObject request = built.def;
    request.insert(QStringLiteral("query"), QStringLiteral("point_cloud_alignment"));
    QJsonObject answer;
    try {
      answer = slowQuery(context, request);
    } catch (const std::exception& error) {
      result.error = errorText(error);
      result.log = QStringLiteral("not aligned: %1").arg(modelText(error));
      return result;
    }
    result.text = alignmentText(built.def, answer, context);
    result.log = alignmentLog(built.def, answer, context);
    return result;
  };
  // OK moves the cloud: one undo step, "Align PointCloud1".
  def.apply = [&host](CommandState& state, const CommandContext& context) {
    const Built built = alignFields(state, context);
    if (!built.error.isEmpty()) {
      host.showStatus(built.error, NoticeLevel::Error);
      return false;
    }
    QJsonObject command = built.def;
    command.insert(QStringLiteral("cmd"), QStringLiteral("align_point_cloud"));
    QJsonObject answer;
    if (!host.runModelCommand(command, &answer)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("Aligned point cloud %1").arg(alignmentLog(built.def, answer, context));
    return true;
  };
  return def;
}

CommandDef pointCloudFilterCommand(ChosenCloud chosen) {
  CommandDef def;
  def.id = QStringLiteral("insert.filter_point_cloud");
  def.name = QObject::tr("Filter Point Cloud");
  def.icon = QStringLiteral("point-cloud-filter");
  def.tooltip = QObject::tr("Makes a new point cloud without outliers and noise, or cropped by a lasso or box");
  def.keywords = {QStringLiteral("scan"),  QStringLiteral("outliers"), QStringLiteral("noise"),
                  QStringLiteral("clean"), QStringLiteral("crop"),     QStringLiteral("lasso"),
                  QStringLiteral("box"),   QStringLiteral("point cloud")};
  def.modelCommand = true;
  const QVector<QPair<QString, QString>> outliers = {
      {QStringLiteral("none"), QObject::tr("None")},
      {QStringLiteral("statistical"), QObject::tr("Statistical")},
      {QStringLiteral("radius"), QObject::tr("By Radius")}};
  const QVector<QPair<QString, QString>> crops = {{QStringLiteral("none"), QObject::tr("None")},
                                                  {QStringLiteral("keep"), QObject::tr("Keep Inside Lasso")},
                                                  {QStringLiteral("remove"), QObject::tr("Remove Inside Lasso")},
                                                  {QStringLiteral("keep_box"), QObject::tr("Keep Inside Box")},
                                                  {QStringLiteral("remove_box"), QObject::tr("Remove Inside Box")}};
  const auto byOutliers = [](const CommandState& state) {
    return state.choice(kOutliers) != QStringLiteral("none") || state.checked(kSmooth);
  };
  def.inputs = {
      choiceInput(kCloud, QObject::tr("Point Cloud"), {}, QString())
          .withChoices(cloudChoices)
          .withTooltip(QObject::tr("The scanned points to clean; they are kept, hidden"))
          .withOnChange(startBox),
      choiceInput(kOutliers, QObject::tr("Outliers"), outliers, QStringLiteral("statistical"))
          .withTooltip(
              QObject::tr("The points taken out: those far from their neighbors, or with few within a radius")),
      valueInput(kNeighbours, QObject::tr("Neighbors"), ValueKind::Unitless, QStringLiteral("16"))
          .withTooltip(QObject::tr("The nearest points each point is compared with"))
          .withVisible(byOutliers),
      valueInput(kRatio, QObject::tr("Spread"), ValueKind::Unitless, QStringLiteral("2"))
          .withTooltip(QObject::tr("Standard deviations beyond the average a point's neighbors may lie"))
          .withVisible([](const CommandState& state) { return state.choice(kOutliers) == QStringLiteral("statistical"); }),
      valueInput(kRadius, QObject::tr("Radius"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("A point with fewer neighbors than Neighbors within this goes"))
          .withVisible([](const CommandState& state) { return state.choice(kOutliers) == QStringLiteral("radius"); }),
      checkInput(kSmooth, QObject::tr("Smooth Noise"), false)
          .withTooltip(QObject::tr("Moves each point onto the plane of its neighbors")),
      choiceInput(kCrop, QObject::tr("Crop"), crops, QStringLiteral("none"))
          .withTooltip(QObject::tr("Click round points in the view, or set a box: the points inside are kept, or "
                                   "taken out"))
          .withOnChange(startBox),
      boxInput(0, false),
      boxInput(0, true),
      boxInput(1, false),
      boxInput(1, true),
      boxInput(2, false),
      boxInput(2, true),
      checkInput(kKeepShown, QObject::tr("Keep Original Shown"), false)
          .withTooltip(QObject::tr("The cloud the new one is made of stays shown")),
  };
  def.preselect = [chosen](const Selection&, CommandState& state, const CommandContext& context) {
    startCloud(state, context, chosen);
  };
  // The lasso: clicks on a plane through the cloud's middle, facing the
  // view when the lasso began, as Remove Region's.
  def.cursorPlane = [](const CommandState& state) -> std::optional<gp_Ax3> {
    if (!cropping(state)) {
      return std::nullopt;
    }
    const std::optional<gp_Pnt> centre = cloudMiddle(state);
    if (!centre) {
      return std::nullopt;
    }
    const QJsonArray normal = state.object(kLasso).value(QStringLiteral("normal")).toArray();
    if (normal.size() == 3) {
      const gp_Vec n(normal.at(0).toDouble(), normal.at(1).toDouble(), normal.at(2).toDouble());
      if (n.Magnitude() > 1e-12) {
        return gp_Ax3(*centre, gp_Dir(n));
      }
    }
    return gp_Ax3(*centre, gp_Dir(0.0, 0.0, 1.0));
  };
  def.cursorPlaneMoved = [](const std::optional<gp_Pnt>&, CommandState&, const CommandContext&) { return false; };
  def.cursorPlaneClicked = [](const gp_Pnt& point, CommandState& state, const CommandContext& context) {
    QJsonObject lasso = state.object(kLasso);
    QJsonArray points = lasso.value(QStringLiteral("points")).toArray();
    if (points.isEmpty()) {
      const std::optional<gp_Pnt> centre = cloudMiddle(state);
      const std::optional<gp_Dir> toward = centre ? context.towardViewer(*centre) : std::nullopt;
      if (!toward) {
        return false;
      }
      lasso.insert(QStringLiteral("normal"), QJsonArray{toward->X(), toward->Y(), toward->Z()});
    }
    points.append(pointJson(point));
    lasso.insert(QStringLiteral("points"), points);
    state.setObject(kLasso, lasso);
    qDebug().noquote() << QStringLiteral("Filter Point Cloud lasso: %1 points").arg(points.size());
    return true;
  };
  // Backspace takes the last lasso point back.
  def.backspace = [](CommandState& state, const CommandContext&) {
    QJsonObject lasso = state.object(kLasso);
    QJsonArray points = lasso.value(QStringLiteral("points")).toArray();
    if (!cropping(state) || points.isEmpty()) {
      return false;
    }
    points.removeLast();
    if (points.isEmpty()) {
      lasso = QJsonObject();
    } else {
      lasso.insert(QStringLiteral("points"), points);
    }
    state.setObject(kLasso, lasso);
    return true;
  };
  // The crop box's edges.
  def.viewLines = [](const CommandState& state, const CommandContext&) {
    std::vector<std::vector<gp_Pnt>> lines;
    const auto corners = boxCorners(state);
    if (!boxCropping(state) || !corners) {
      return lines;
    }
    const gp_XYZ& a = corners->first;
    const gp_XYZ& b = corners->second;
    const auto corner = [&a, &b](bool x, bool y, bool z) {
      return gp_Pnt(x ? b.X() : a.X(), y ? b.Y() : a.Y(), z ? b.Z() : a.Z());
    };
    for (const bool z : {false, true}) {
      lines.push_back({corner(false, false, z), corner(true, false, z), corner(true, true, z), corner(false, true, z),
                       corner(false, false, z)});
    }
    for (const bool x : {false, true}) {
      for (const bool y : {false, true}) {
        lines.push_back({corner(x, y, false), corner(x, y, true)});
      }
    }
    return lines;
  };
  def.build = [](const CommandState& state, const CommandContext&) { return filterCommand(state); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    QString crop;
    const auto corners = boxCorners(state);
    if (cropping(state)) {
      crop = QStringLiteral(", cropped by a lasso of %1 points").arg(lassoPoints(state).size());
    } else if (boxCropping(state) && corners) {
      const auto xyz = [](const gp_XYZ& p) {
        return QStringLiteral("%1,%2,%3").arg(p.X(), 0, 'f', 3).arg(p.Y(), 0, 'f', 3).arg(p.Z(), 0, 'f', 3);
      };
      crop = QStringLiteral(", cropped by a box from %1 to %2, inside %3")
                 .arg(xyz(corners->first), xyz(corners->second),
                      state.choice(kCrop) == QStringLiteral("keep_box") ? QStringLiteral("kept")
                                                                        : QStringLiteral("removed"));
    }
    return QStringLiteral("Filtered point cloud %1: outliers %2%3%4")
        .arg(state.choice(kCloud), state.choice(kOutliers),
             state.checked(kSmooth) ? QStringLiteral(", noise smoothed") : QString(), crop);
  };
  return def;
}

CommandDef pointCloudReconstructCommand(ChosenCloud chosen) {
  CommandDef def;
  def.id = QStringLiteral("insert.reconstruct_point_cloud");
  def.name = QObject::tr("Reconstruct Mesh");
  def.icon = QStringLiteral("point-cloud-mesh");
  def.tooltip = QObject::tr("Builds a closed mesh body of a point cloud's points");
  def.keywords = {QStringLiteral("scan"),    QStringLiteral("mesh"),    QStringLiteral("surface"),
                  QStringLiteral("rebuild"), QStringLiteral("reconstruct"), QStringLiteral("point cloud"),
                  QStringLiteral("triangulate")};
  def.modelCommand = true;
  def.inputs = {
      choiceInput(kCloud, QObject::tr("Point Cloud"), {}, QString())
          .withChoices(cloudChoices)
          .withTooltip(QObject::tr("The scanned points to build the mesh of")),
      valueInput(kDetail, QObject::tr("Detail"), ValueKind::Unitless, QStringLiteral("64"))
          .withTooltip(QObject::tr("Cubes along the cloud's longest side, 8 to 256: more gives a finer mesh and "
                                   "takes longer")),
      valueInput(kNeighbours, QObject::tr("Normal Neighbors"), ValueKind::Unitless, QStringLiteral("12"))
          .withTooltip(QObject::tr("The nearest points each point's direction across the surface is found of")),
  };
  def.preselect = [chosen](const Selection&, CommandState& state, const CommandContext& context) {
    startCloud(state, context, chosen);
  };
  def.build = [](const CommandState& state, const CommandContext&) -> Built {
    const QString cloud = state.choice(kCloud);
    if (cloud.isEmpty()) {
      return Built::failure(noCloud(), kCloud);
    }
    const int detail = wholeIn(state.value(kDetail), 8, 256);
    if (detail < 0) {
      return Built::failure(QObject::tr("Detail: a whole number from 8 to 256."), kDetail);
    }
    const int neighbours = wholeIn(state.value(kNeighbours), 3, 100);
    if (neighbours < 0) {
      return Built::failure(QObject::tr("Normal Neighbors: a whole number from 3 to 100."), kNeighbours);
    }
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("reconstruct_point_cloud")},
                              {QStringLiteral("name"), cloud},
                              {kDetail, detail},
                              {kNeighbours, neighbours}};
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}});
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Reconstructed a mesh of point cloud %1, detail %2")
        .arg(state.choice(kCloud))
        .arg(state.value(kDetail));
  };
  return def;
}

} // namespace mitcad
