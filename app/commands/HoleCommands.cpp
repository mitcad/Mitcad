// SPDX-License-Identifier: MIT
// Hole and Thread (U4; commands.md, "hole", "thread"). A hole is placed by
// clicks on a planar face, each click a row of the position table, or at
// sketch points; it may be counterdrilled and tapered (mitcad#27, as the
// FreeCAD import makes them). A thread's size is suggested from the
// cylinder picked; its free ends may be chamfered (mitcad#304). Sizes, designations and classes are chosen from the
// model's table (`thread_sizes`, P9), ISO metric first (P11); Thread Type
// Custom takes a major diameter and a pitch of its own instead, and a
// modelled one a number of starts (mitcad#893).
#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <optional>

#include <QDebug>
#include <QHash>
#include <QJsonArray>
#include <QKeySequence>

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp_Face.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "../framework/Numbers.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "HoleSizes.hpp"
#include "PlaceInputs.hpp"

namespace mitcad::cmd {
namespace {

// A face hole of several positions written before (mitcad#656), or a hole
// placed at distances from two edges of its face, on a plane or along a
// shaft (as the .f3d import writes them, mitcad#733): kept as it was while
// the placement stays as it is, its distances editable.
const QString kKept = QStringLiteral("kept_placement");
const QString kOffset1 = QStringLiteral("kept_offset1");
const QString kOffset2 = QStringLiteral("kept_offset2");
const QString kShaftOffset = QStringLiteral("kept_offset");

// The type of the placement kept as it was (`face`, `face_offsets`,
// `plane`, `cylinder_offset`); empty when none is.
QString keptType(const CommandState& state) { return str(state.object(kKept), "type"); }

// The placement choice's name of a kept placement.
QString keptName(const QString& type) {
  if (type == QStringLiteral("face_offsets")) {
    return QStringLiteral("From Two Edges");
  }
  if (type == QStringLiteral("cylinder_offset")) {
    return QStringLiteral("Along a Shaft");
  }
  if (type == QStringLiteral("plane")) {
    return QStringLiteral("On a Plane");
  }
  return QStringLiteral("Earlier Positions");
}

// Whether the placement is kept as it was and of this type.
std::function<bool(const CommandState&)> keptIs(const QString& type) {
  return [type](const CommandState& state) {
    return state.choice(QStringLiteral("placement")) == QStringLiteral("positions") && keptType(state) == type;
  };
}

// The component an edited hole belongs to (mitcad#762); a new hole's is
// the active one.
const QString kHoleOwner = QStringLiteral("hole_owner");

// Why a hole cannot end at the object (mitcad#762): a face or body of
// another component than the hole's, which the model reads only in the
// hole's own component, or such a construction plane (mitcad#772); empty
// when it can.
QString foreignObject(const CommandState& state, const CommandContext& context) {
  const Selection& objects = state.items(QStringLiteral("object"));
  if (objects.isEmpty()) {
    return QString();
  }
  const SelectionItem& object = objects.first();
  const bool plane = object.kind == SelectKind::Plane;
  if (object.kind != SelectKind::Face && object.kind != SelectKind::Body && !plane) {
    return QString();
  }
  // A construction plane's component is its feature's (none for an origin
  // plane, every component's own); a face's or body's, where it was picked.
  const QString at = plane ? planeComponent(object, context) : occurrenceComponent(object.occurrence, context);
  if (at.isEmpty()) {
    return QString();
  }
  QString own = state.object(kHoleOwner).value(QStringLiteral("component")).toString();
  if (own.isEmpty()) {
    try {
      own = componentsOf(context).value(QStringLiteral("active")).toString(QStringLiteral("C0"));
    } catch (const std::exception&) {
      return QString();
    }
  }
  if (at == own) {
    return QString();
  }
  return QObject::tr("The object is in %1: a hole ends at a plane, face or body of its own component.")
      .arg(componentName(at, context));
}

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

// ISO metric coarse sizes (ISO 261): nominal diameter and pitch.
struct MetricSize {
  double d;
  double pitch;
};
const MetricSize kCoarse[] = {{1, 0.25},   {1.2, 0.25}, {1.6, 0.35}, {2, 0.4},   {2.5, 0.45},
                              {3, 0.5},    {4, 0.7},    {5, 0.8},    {6, 1.0},   {8, 1.25},
                              {10, 1.5},   {12, 1.75},  {14, 2.0},   {16, 2.0},  {20, 2.5},
                              {24, 3.0},   {30, 3.5},   {36, 4.0},   {42, 4.5},  {48, 5.0},
                              {56, 5.5},   {64, 6.0}};

QString designation(const MetricSize& size) {
  return QStringLiteral("M%1x%2").arg(number(size.d), number(size.pitch));
}

// The coarse size that fits a cylinder: its nominal diameter outside, its
// minor diameter (D - 1.0825 P) inside.
MetricSize sizeFor(double diameter, bool internal) {
  MetricSize best = kCoarse[0];
  double nearest = 1e300;
  for (const MetricSize& size : kCoarse) {
    const double fits = internal ? size.d - 1.082532 * size.pitch : size.d;
    if (std::abs(fits - diameter) < nearest) {
      nearest = std::abs(fits - diameter);
      best = size;
    }
  }
  return best;
}

// A cylindrical face's diameter and whether it is a bore (its normal points
// to the axis).
std::optional<std::pair<double, bool>> cylinderOf(const SelectionItem& item, const CommandContext& context) {
  const TopoDS_Shape shape = context.itemShape(item);
  TopExp_Explorer faces(shape, TopAbs_FACE);
  if (!faces.More()) {
    return std::nullopt;
  }
  const TopoDS_Face face = TopoDS::Face(faces.Current());
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Cylinder) {
    return std::nullopt;
  }
  const gp_Cylinder cylinder = surface.Cylinder();
  const double u = (surface.FirstUParameter() + surface.LastUParameter()) / 2.0;
  const double v = (surface.FirstVParameter() + surface.LastVParameter()) / 2.0;
  gp_Pnt point;
  gp_Vec normal;
  BRepGProp_Face(face).Normal(u, v, point, normal);
  const gp_Ax1 axis = cylinder.Axis();
  const gp_Vec out(axis.Location(), point);
  const gp_Vec radial = out - gp_Vec(axis.Direction()) * out.Dot(gp_Vec(axis.Direction()));
  return std::make_pair(2.0 * cylinder.Radius(), normal.Dot(radial) < 0.0);
}

// The thread size table of the model (`thread_sizes`), read once: the
// standards in the model's order (ISO metric first), each with its sizes
// and their designations (the coarse pitch first) and its classes.
struct ThreadStandard {
  QString id;
  QString title;
  QVector<QPair<QString, QStringList>> sizes;
  // A taper pipe thread's tap drill by size (mitcad#733).
  QHash<QString, double> tapDrills;
  // The major diameter by size (mitcad#903: a tapped hole's countersink).
  QHash<QString, double> majors;
  QStringList external;
  QStringList internal;
  QString defaultExternal;
  QString defaultInternal;
};

const QVector<ThreadStandard>& threadTable(const CommandContext& context) {
  static QVector<ThreadStandard> table;
  if (!table.isEmpty()) {
    return table;
  }
  QJsonObject answer;
  try {
    answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("thread_sizes")}});
  } catch (const std::exception&) {
    return table;
  }
  const auto strings = [](const QJsonValue& value) { return value.toVariant().toStringList(); };
  for (const QJsonValue& value : answer.value(QStringLiteral("standards")).toArray()) {
    const QJsonObject standard = value.toObject();
    ThreadStandard entry;
    entry.id = str(standard, "standard");
    entry.title = str(standard, "title");
    for (const QJsonValue& size : standard.value(QStringLiteral("sizes")).toArray()) {
      const QJsonObject sized = size.toObject();
      entry.sizes.append({str(sized, "size"), strings(sized.value(QStringLiteral("designations")))});
      if (sized.contains(QStringLiteral("tap_drill"))) {
        entry.tapDrills.insert(str(sized, "size"), sized.value(QStringLiteral("tap_drill")).toDouble());
      }
      if (sized.contains(QStringLiteral("major_diameter"))) {
        entry.majors.insert(str(sized, "size"), sized.value(QStringLiteral("major_diameter")).toDouble());
      }
    }
    entry.external = strings(standard.value(QStringLiteral("classes_external")));
    entry.internal = strings(standard.value(QStringLiteral("classes_internal")));
    entry.defaultExternal = str(standard, "default_class_external");
    entry.defaultInternal = str(standard, "default_class_internal");
    table.append(entry);
  }
  return table;
}

const ThreadStandard* standardOf(const CommandContext& context, const QString& id) {
  for (const ThreadStandard& standard : threadTable(context)) {
    if (standard.id == id) {
      return &standard;
    }
  }
  return nullptr;
}

QVector<QPair<QString, QString>> labelled(const QStringList& values) {
  QVector<QPair<QString, QString>> options;
  for (const QString& value : values) {
    options.append({value, value});
  }
  return options;
}

// The size whose designations hold `designation` (a coarse "M10" is the
// size "10"), with the designation as the table writes it.
std::optional<std::pair<QString, QString>> sizeOfDesignation(const ThreadStandard& standard,
                                                             const QString& designation) {
  for (const auto& [size, designations] : standard.sizes) {
    if (designations.contains(designation)) {
      return std::make_pair(size, designation);
    }
  }
  for (const auto& [size, designations] : standard.sizes) {
    if (!designations.isEmpty() && designation.compare(QStringLiteral("M") + size, Qt::CaseInsensitive) == 0) {
      return std::make_pair(size, designations.first());
    }
  }
  return std::nullopt;
}

// A taper pipe thread tapered 1:16 (mitcad#733): the check, shown for the
// NPT and ISO 7-1 standards; on for new threads. Threads and holes saved
// before have it off (their taper pipe threads are straight).
const QString kTaperThread = QStringLiteral("taper_thread");

bool taperStandard(const CommandState& state) {
  const QString standard = state.choice(QStringLiteral("standard"));
  return standard == QStringLiteral("npt") || standard == QStringLiteral("iso_taper");
}

// Whether the thread is a taper pipe thread that tapers.
bool tapers(const CommandState& state) { return taperStandard(state) && state.checked(kTaperThread); }

// A custom thread (mitcad#893): Thread Type Custom (the `thread_sizes`
// query's last standard, without sizes or classes) takes a major diameter
// and a pitch of its own, expressions, instead of a size of the table, and
// a modelled one a number of starts (kCustom, kThreadMajor and
// customThread: HoleSizes.hpp).
const QString kThreadPitch = QStringLiteral("thread_pitch");
const QString kStarts = QStringLiteral("starts");

// A custom thread's size in a definition: `{"major", "pitch"}`.
QJsonObject customSize(const CommandState& state) {
  return {{QStringLiteral("major"), state.expression(kThreadMajor)},
          {QStringLiteral("pitch"), state.expression(kThreadPitch)}};
}

// Fills a custom thread's inputs from its size (`custom`) and starts.
void loadCustomSize(const QJsonObject& custom, const QJsonValue& starts, const QJsonObject& feature,
                    CommandState& state, const CommandContext& context) {
  loadValue(state, kThreadMajor, custom.value(QStringLiteral("major")), feature, context);
  loadValue(state, kThreadPitch, custom.value(QStringLiteral("pitch")), feature, context);
  loadCount(state, kStarts, starts, feature, context);
}

// The size a description names: a designation and its class, or a custom
// thread's diameter and pitch with its starts.
QString sizeText(const CommandState& state) {
  if (!customThread(state)) {
    return QStringLiteral("%1 %2").arg(state.choice(QStringLiteral("designation")), state.choice(QStringLiteral("class")));
  }
  QString text = QStringLiteral("custom %1x%2").arg(number(state.value(kThreadMajor)), number(state.value(kThreadPitch)));
  if (state.checked(QStringLiteral("modeled")) && state.value(kStarts) > 1.0) {
    text += QStringLiteral(", %1 starts").arg(number(state.value(kStarts)));
  }
  return text;
}

// A modelled custom thread's Starts, shown while `shown` holds.
InputDef startsInput(const std::function<bool(const CommandState&)>& shown) {
  return valueInput(kStarts, QObject::tr("Starts"), ValueKind::Unitless, QStringLiteral("1"))
      .withTooltip(QObject::tr("The number of starts: grooves evenly spaced round the axis, the lead that many pitches"))
      .withVisible([shown](const CommandState& s) {
        return (!shown || shown(s)) && customThread(s) && s.checked(QStringLiteral("modeled"));
      });
}

InputDef taperInput(const std::function<bool(const CommandState&)>& shown) {
  return checkInput(kTaperThread, QObject::tr("Tapered"), true)
      .withTooltip(QObject::tr("Taper the thread 1:16 on the diameter; a cosmetic one tapers the bore or the shaft too"))
      .withVisible([shown](const CommandState& s) { return taperStandard(s) && (!shown || shown(s)); });
}

// A tapped hole's Diameter at its taper pipe thread's tap drill (the
// `thread_sizes` query's, mitcad#733), when it tapers.
void startTapDrill(CommandState& state, const CommandContext& model) {
  if (state.choice(QStringLiteral("tap")) != QStringLiteral("tapped") || !tapers(state)) {
    return;
  }
  const ThreadStandard* standard = standardOf(model, state.choice(QStringLiteral("standard")));
  if (standard == nullptr || !standard->tapDrills.contains(state.choice(QStringLiteral("size")))) {
    return;
  }
  const double drill = standard->tapDrills.value(state.choice(QStringLiteral("size")));
  state.setText(QStringLiteral("diameter"), lengthText(std::round(drill * 1000.0) / 1000.0, model));
  state.setValue(QStringLiteral("diameter"), std::numeric_limits<double>::quiet_NaN());
  qDebug().noquote() << QStringLiteral("Hole tap drill: %1 mm").arg(number(std::round(drill * 1000.0) / 1000.0));
}

// The side of the faces a thread goes on (mitcad#691), kept in the state
// beside the inputs: `internal` (hole walls), `external` (shafts), empty
// while no face is picked or the faces are of both sides.
const QString kSide = QStringLiteral("faces_side");
const QString kInternal = QStringLiteral("internal");
const QString kExternal = QStringLiteral("external");

// The side of the cylindrical faces: one side for all, else empty.
QString sideOf(const Selection& faces, const CommandContext& model) {
  QString side;
  for (const SelectionItem& item : faces) {
    const auto cylinder = cylinderOf(item, model);
    if (!cylinder) {
      continue;
    }
    const QString own = cylinder->second ? kInternal : kExternal;
    if (!side.isEmpty() && side != own) {
      return QString();
    }
    side = own;
  }
  return side;
}

// Keeps the class on the faces' side (mitcad#691): a class of the other
// side becomes the standard's default of the faces' side (6g on a hole
// becomes 6H); `internalOnly`: always the internal side (tapped holes).
void keepClassOnSide(CommandState& state, const CommandContext& context, bool internalOnly) {
  const ThreadStandard* standard = standardOf(context, state.choice(QStringLiteral("standard")));
  if (standard == nullptr) {
    return;
  }
  const QString side = internalOnly ? kInternal : state.choice(kSide);
  const QString current = state.choice(QStringLiteral("class"));
  if (side == kInternal && !standard->internal.contains(current)) {
    state.setChoice(QStringLiteral("class"), standard->defaultInternal);
  } else if (side == kExternal && !standard->external.contains(current)) {
    state.setChoice(QStringLiteral("class"), standard->defaultExternal);
  }
}

// The ISO metric coarse size a cylindrical face suggests (its nominal
// diameter outside, its tap drill inside), with 6H for a hole and 6g for a
// shaft; a custom thread's major diameter a shaft's own (mitcad#893);
// nothing for another standard or another surface. A face picked in the
// open command and one selected before it alike (mitcad#691).
void suggestSize(const SelectionItem& item, CommandState& state, const CommandContext& model) {
  const auto cylinder = cylinderOf(item, model);
  if (!cylinder) {
    return;
  }
  if (customThread(state) && !cylinder->second) {
    state.setText(kThreadMajor, lengthText(std::round(cylinder->first * 1000.0) / 1000.0, model));
    state.setValue(kThreadMajor, std::numeric_limits<double>::quiet_NaN());
    return;
  }
  if (state.choice(QStringLiteral("standard")) != QStringLiteral("iso_metric")) {
    return;
  }
  const MetricSize size = sizeFor(cylinder->first, cylinder->second);
  state.setChoice(QStringLiteral("size"), number(size.d));
  state.setChoice(QStringLiteral("designation"), designation(size));
  state.setChoice(QStringLiteral("class"), cylinder->second ? QStringLiteral("6H") : QStringLiteral("6g"));
}

// The size table's inputs: Thread Type, Size, Designation and Class,
// starting at an ISO metric size (M6x1 6H for holes,
// M10x1.5 6g for threads); `internalOnly`: the classes are a nut's (tapped
// holes), else those of the faces' side (`kSide`), both while it is
// unknown (threads).
struct ThreadStart {
  QString size;
  QString designation;
  QString threadClass;
};

QVector<InputDef> threadInputs(const CommandContext& context, const ThreadStart& start, bool internalOnly,
                               const std::function<bool(const CommandState&)>& shown,
                               const std::function<void(CommandState&, const CommandContext&)>& sized = nullptr) {
  const auto standardIn = [&context](const CommandState& state) {
    return standardOf(context, state.choice(QStringLiteral("standard")));
  };
  // A new size or designation keeps the class on the faces' side (and
  // tells `sized`).
  const auto keepClass = [internalOnly, sized](CommandState& state, const CommandContext& model) {
    keepClassOnSide(state, model, internalOnly);
    if (sized) {
      sized(state, model);
    }
  };
  // The designations of the state's size.
  const auto designationsIn = [standardIn](const CommandState& state) {
    if (const ThreadStandard* standard = standardIn(state)) {
      for (const auto& [size, designations] : standard->sizes) {
        if (size == state.choice(QStringLiteral("size"))) {
          return designations;
        }
      }
    }
    return QStringList();
  };
  // Another standard starts at the start size (Unified at 1/4, else its
  // first) with its coarse designation and the default class of the same
  // side.
  const auto startStandard = [&context, standardIn, start, internalOnly, sized](CommandState& state,
                                                                               const CommandContext& model) {
    const ThreadStandard* standard = standardIn(state);
    if (standard == nullptr || standard->sizes.isEmpty()) {
      return;
    }
    auto size = standard->sizes.begin();
    for (const QString& preferred : {start.size, QStringLiteral("1/4")}) {
      const auto found = std::find_if(standard->sizes.begin(), standard->sizes.end(),
                                      [&preferred](const auto& entry) { return entry.first == preferred; });
      if (found != standard->sizes.end()) {
        size = found;
        break;
      }
    }
    state.setChoice(QStringLiteral("size"), size->first);
    state.setChoice(QStringLiteral("designation"), size->second.value(0));
    const QString current = state.choice(QStringLiteral("class"));
    const auto& all = threadTable(context);
    const bool internal = internalOnly || std::any_of(all.begin(), all.end(), [&current](const ThreadStandard& s) {
                            return s.internal.contains(current);
                          });
    state.setChoice(QStringLiteral("class"), internal ? standard->defaultInternal : standard->defaultExternal);
    keepClassOnSide(state, context, internalOnly);
    if (sized) {
      sized(state, model);
    }
  };
  QVector<QPair<QString, QString>> standards;
  for (const ThreadStandard& standard : threadTable(context)) {
    standards.append({standard.id, standard.title});
  }
  if (standards.isEmpty()) {
    standards = {{QStringLiteral("iso_metric"), QStringLiteral("ISO Metric profile")},
                 {QStringLiteral("unified"), QStringLiteral("ANSI Unified Screw Threads")}};
  }
  // A size of the table, or a custom thread's own (mitcad#893).
  const auto tabled = [shown](const CommandState& s) { return (!shown || shown(s)) && !customThread(s); };
  const auto custom = [shown](const CommandState& s) { return (!shown || shown(s)) && customThread(s); };
  QVector<InputDef> inputs = {
      choiceInput(QStringLiteral("standard"), QObject::tr("Thread Type"), standards, standards.first().first)
          .withTooltip(QObject::tr("A standard's table of sizes, or Custom: a major diameter and a pitch of your own"))
          .withVisible(shown)
          .withOnChange(startStandard),
      choiceInput(QStringLiteral("size"), QObject::tr("Size"), {}, start.size)
          .withTooltip(QObject::tr("The nominal size"))
          .withChoices([standardIn](const CommandState& state) {
            QVector<QPair<QString, QString>> options;
            if (const ThreadStandard* standard = standardIn(state)) {
              for (const auto& [size, designations] : standard->sizes) {
                options.append({size, size});
              }
            }
            return options;
          })
          .withOnChange([designationsIn, keepClass](CommandState& state, const CommandContext& model) {
            state.setChoice(QStringLiteral("designation"), designationsIn(state).value(0));
            keepClass(state, model);
          })
          .withVisible(tabled),
      choiceInput(QStringLiteral("designation"), QObject::tr("Designation"), {}, start.designation)
          .withTooltip(QObject::tr("The size and pitch: M10x1.5 (coarse), M10x1.25 (fine), 1/4-20 UNC"))
          .withChoices([designationsIn](const CommandState& state) { return labelled(designationsIn(state)); })
          .withOnChange(keepClass)
          .withVisible(tabled),
      // Once the faces are known, only the classes of their side (mitcad#691).
      choiceInput(QStringLiteral("class"), QObject::tr("Class"), {}, start.threadClass)
          .withTooltip(internalOnly ? QObject::tr("The tolerance class of the tapped hole")
                                    : QObject::tr("The tolerance class: 6g outside, 6H inside"))
          .withChoices([standardIn, internalOnly](const CommandState& state) {
            const ThreadStandard* standard = standardIn(state);
            if (standard == nullptr) {
              return QVector<QPair<QString, QString>>();
            }
            const QString side = internalOnly ? kInternal : state.choice(kSide);
            return labelled(side == kInternal   ? standard->internal
                            : side == kExternal ? standard->external
                                                : standard->external + standard->internal);
          })
          .withVisible(tabled),
      valueInput(kThreadMajor, QObject::tr("Major Diameter"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The custom thread's major diameter, on the 60-degree profile"))
          .withOnChange([sized](CommandState& state, const CommandContext& model) {
            // (A tapped hole's countersink follows it, mitcad#903.)
            if (sized) {
              sized(state, model);
            }
          })
          .withVisible(custom),
      valueInput(kThreadPitch, QObject::tr("Pitch"), ValueKind::Length, QStringLiteral("1.5 mm"))
          .withTooltip(QObject::tr("The distance from one tooth to the next along the axis"))
          .withVisible(custom),
  };
  return inputs;
}

// Fills the size table's inputs from a thread definition (a custom
// thread's size: loadCustomSize).
void loadThreadInputs(const QJsonObject& thread, CommandState& state, const CommandContext& context) {
  const QString id = thread.value(QStringLiteral("standard")).toString(QStringLiteral("iso_metric"));
  state.setChoice(QStringLiteral("standard"), id);
  const QString designation = str(thread, "designation");
  state.setChoice(QStringLiteral("designation"), designation);
  if (const ThreadStandard* standard = standardOf(context, id)) {
    if (const auto found = sizeOfDesignation(*standard, designation)) {
      state.setChoice(QStringLiteral("size"), found->first);
      state.setChoice(QStringLiteral("designation"), found->second);
    }
  }
  state.setChoice(QStringLiteral("class"), str(thread, "class"));
}

// A modelled thread's Extra clearance (mitcad#195): two lengths, 0 by
// default, shown while `shown` holds.
const QString kRadialClearance = QStringLiteral("radial_clearance");
const QString kFlankClearance = QStringLiteral("flank_clearance");

QVector<InputDef> clearanceInputs(const std::function<bool(const CommandState&)>& shown) {
  return {
      valueInput(kRadialClearance, QObject::tr("Radial clearance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withHeading(QObject::tr("Extra clearance"))
          .withTooltip(QObject::tr("Extra clearance per side, along the radius, not on the diameter"))
          .withVisible(shown),
      valueInput(kFlankClearance, QObject::tr("Flank clearance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Extra clearance on each flank, measured perpendicular to the flank"))
          .withVisible(shown),
  };
}

// Keep zero-valued expressions too: their parameter may change later.
void insertClearance(const CommandState& state, QJsonObject& def) {
  for (const QString& key : {kRadialClearance, kFlankClearance}) {
    if (std::isfinite(state.value(key))) {
      def.insert(key, state.expression(key));
    }
  }
}

// Fills the clearance inputs from a thread's definition (`thread`) in a
// feature.
void loadClearance(const QJsonObject& thread, const QJsonObject& feature, CommandState& state,
                   const CommandContext& context) {
  for (const QString& key : {kRadialClearance, kFlankClearance}) {
    loadValue(state, key, thread.value(key), feature, context);
  }
}

// A thread's end chamfers (mitcad#304): the check, the ends, and the size
// from the thread's size (the model's default, which follows the size) or
// given as an angle and a diameter.
const QString kChamfer = QStringLiteral("chamfer");
const QString kChamferEnds = QStringLiteral("chamfer_ends");
const QString kChamferSize = QStringLiteral("chamfer_size");
const QString kChamferAngle = QStringLiteral("chamfer_angle");
const QString kChamferDiameter = QStringLiteral("chamfer_diameter");

QVector<InputDef> chamferInputs(const CommandContext& context) {
  const auto chamfered = [](const CommandState& s) { return s.checked(kChamfer) && !tapers(s); };
  const auto given = [](const CommandState& s) {
    return s.checked(kChamfer) && !tapers(s) && s.choice(kChamferSize) == QStringLiteral("custom");
  };
  // Given values start at the size's defaults (the `thread_chamfer` query),
  // inside when the class is an internal one.
  const auto startValues = [&context](CommandState& state, const CommandContext& model) {
    if (state.choice(kChamferSize) != QStringLiteral("custom")) {
      return;
    }
    const QString standardId = state.choice(QStringLiteral("standard"));
    const ThreadStandard* standard = standardOf(context, standardId);
    // (A custom thread has no classes: the faces' side, mitcad#893.)
    const bool internal = customThread(state)
                              ? state.choice(kSide) == kInternal
                              : standard != nullptr && standard->internal.contains(state.choice(QStringLiteral("class")));
    QJsonObject request{{QStringLiteral("query"), QStringLiteral("thread_chamfer")},
                        {QStringLiteral("standard"), standardId},
                        {QStringLiteral("internal"), internal}};
    if (customThread(state)) {
      request.insert(QStringLiteral("major"), state.value(kThreadMajor));
      request.insert(QStringLiteral("pitch"), state.value(kThreadPitch));
    } else {
      request.insert(QStringLiteral("designation"), state.choice(QStringLiteral("designation")));
    }
    QJsonObject answer;
    try {
      answer = model.queryObject(request);
    } catch (const std::exception&) {
      return;
    }
    const double diameter = answer.value(QStringLiteral("diameter")).toDouble();
    if (diameter > 0.0) {
      state.setText(kChamferDiameter, lengthText(std::round(diameter * 1000.0) / 1000.0, model));
      state.setValue(kChamferDiameter, std::numeric_limits<double>::quiet_NaN());
    }
    state.setText(kChamferAngle, number(answer.value(QStringLiteral("angle")).toDouble(45.0)) + QChar(0x00B0));
    state.setValue(kChamferAngle, std::numeric_limits<double>::quiet_NaN());
  };
  return {
      // A tapered taper pipe thread has no end chamfers (mitcad#733).
      checkInput(kChamfer, QObject::tr("Chamfer Ends"), false)
          .withHeading(QObject::tr("End chamfer"))
          .withTooltip(QObject::tr("Cut a cone at the free ends the thread runs out through"))
          .withVisible([](const CommandState& s) { return !tapers(s); }),
      choiceInput(kChamferEnds, QObject::tr("Ends"),
                  {{QStringLiteral("both"), QStringLiteral("Both")},
                   {QStringLiteral("start"), QStringLiteral("Start")},
                   {QStringLiteral("end"), QStringLiteral("End")}},
                  QStringLiteral("both"))
          .withTooltip(QObject::tr("Which ends are chamfered; the start is where the thread location measures from"))
          .withVisible(chamfered),
      choiceInput(kChamferSize, QObject::tr("Chamfer Size"),
                  {{QStringLiteral("default"), QStringLiteral("From Thread Size")},
                   {QStringLiteral("custom"), QStringLiteral("Custom")}},
                  QStringLiteral("default"))
          .withTooltip(QObject::tr("From the thread size, following it, or an angle and a diameter of your own"))
          .withVisible(chamfered)
          .withOnChange(startValues),
      valueInput(kChamferAngle, QObject::tr("Chamfer Angle"), ValueKind::Angle, QStringLiteral("45") + QChar(0x00B0))
          .withTooltip(QObject::tr("The cone's angle to the axis: 45 degrees is a 90-degree countersink"))
          .withVisible(given),
      valueInput(kChamferDiameter, QObject::tr("Chamfer Diameter"), ValueKind::Length, QStringLiteral("6.5 mm"))
          .withTooltip(QObject::tr("The cone's diameter at the end of the face"))
          .withVisible(given),
  };
}

// Puts the end chamfer into a thread's definition.
void insertChamfer(const CommandState& state, QJsonObject& def) {
  if (!state.checked(kChamfer) || tapers(state)) {
    return;
  }
  def.insert(kChamfer, state.choice(kChamferEnds));
  if (state.choice(kChamferSize) == QStringLiteral("custom")) {
    def.insert(kChamferAngle, state.expression(kChamferAngle));
    def.insert(kChamferDiameter, state.expression(kChamferDiameter));
  }
}

// Fills the end chamfer's inputs from a thread's definition.
void loadChamfer(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  const QString ends = feature.value(kChamfer).toString();
  state.setChecked(kChamfer, !ends.isEmpty());
  if (!ends.isEmpty()) {
    state.setChoice(kChamferEnds, ends);
  }
  const bool custom = feature.contains(kChamferAngle) || feature.contains(kChamferDiameter);
  state.setChoice(kChamferSize, custom ? QStringLiteral("custom") : QStringLiteral("default"));
  loadValue(state, kChamferAngle, feature.value(kChamferAngle), feature, context);
  loadValue(state, kChamferDiameter, feature.value(kChamferDiameter), feature, context);
}

// ---------------------------------------------------------------------------
// Clearance holes and recesses by screw size (mitcad#903, mitcad#980):
// HoleSizes.hpp.

// A new Hole Tap Type: a tapered pipe thread's tap drill, and the size's
// values in every field again (but expressions).
void startTap(CommandState& state, const CommandContext& model) {
  startTapDrill(state, model);
  followSizeAgain(state, model);
}

} // namespace

// ---------------------------------------------------------------------------
// Hole

CommandDef holeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.hole");
  def.name = QObject::tr("Hole");
  def.icon = QStringLiteral("hole");
  def.tooltip = QObject::tr("Drilled holes: simple, counterbore, countersink or counterdrill, tapped or not");
  def.shortcut = QKeySequence(Qt::Key_H);
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("faces");
  def.pinned = true;
  def.keywords = {QStringLiteral("drill"), QStringLiteral("tap"), QStringLiteral("counterbore"),
                  QStringLiteral("countersink"), QStringLiteral("counterdrill"), QStringLiteral("taper"),
                  QStringLiteral("clearance")};
  def.featureType = QStringLiteral("hole");
  const auto tapped = choiceIs(QStringLiteral("tap"), QStringLiteral("tapped"));
  const auto type = [](const char* kind) { return choiceIs(QStringLiteral("type"), QString::fromLatin1(kind)); };
  // One hole where a pick puts it (mitcad#656, PlaceInputs.hpp): on a face
  // at distances from two of its edges, at a point, or into a shaft or a
  // hole along its axis. The hole has its own Flip, on the Placement line.
  PlacePanel place;
  place.command = def.name;
  place.along = false;
  place.offered = choiceIs(QStringLiteral("placement"), QStringLiteral("face"));
  def.inputs = {
      choiceInput(QStringLiteral("placement"), QObject::tr("Placement"), {}, QStringLiteral("face"))
          .withChoices([](const CommandState& state) {
            Choices options{{QStringLiteral("face"), QStringLiteral("By Pick")},
                            {QStringLiteral("sketch"), QStringLiteral("From Sketch")}};
            // A face hole of several positions made before keeps them; a
            // hole at distances from edges, on a plane or along a shaft
            // keeps its placement (mitcad#733).
            if (!state.object(kKept).isEmpty()) {
              options.append({QStringLiteral("positions"), keptName(keptType(state))});
            }
            return options;
          }),
      // Beside the placement (mitcad#656), on a face or from a sketch.
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withBesidePrevious(),
      // A kept placement's distances (mitcad#733).
      valueInput(kOffset1, QObject::tr("Edge Offset 1"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The hole's distance from the line of the first edge"))
          .withVisible(keptIs(QStringLiteral("face_offsets"))),
      valueInput(kOffset2, QObject::tr("Edge Offset 2"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The hole's distance from the line of the second edge"))
          .withVisible(keptIs(QStringLiteral("face_offsets"))),
      valueInput(kShaftOffset, QObject::tr("Offset"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The hole's distance from the plane it is measured from, along the shaft"))
          .withVisible(keptIs(QStringLiteral("cylinder_offset"))),
  };
  def.inputs += placeInputs(place);
  def.inputs += {
      selectionInput(QStringLiteral("points"), QObject::tr("Sketch Points"),
                     SelectKind::SketchPoint | SelectKind::SketchCurve, 1, 0)
          .withTooltip(QObject::tr("Sketch points, or circles and arcs (their centres)"))
          .withAccepts([](const SelectionItem& item) {
            return item.kind == SelectKind::SketchPoint || isCircular(item) ||
                   item.geometry == QStringLiteral("ellipse");
          })
          .withVisible(choiceIs(QStringLiteral("placement"), QStringLiteral("sketch"))),
      choiceInput(QStringLiteral("extent"), QObject::tr("Extents"),
                  {{QStringLiteral("distance"), QStringLiteral("Distance")},
                   {QStringLiteral("to_object"), QStringLiteral("To")},
                   {QStringLiteral("through_all"), QStringLiteral("All")}},
                  QStringLiteral("distance")),
  };
  def.inputs += objectInputs(QString(), choiceIs(QStringLiteral("extent"), QStringLiteral("to_object")));
  def.inputs += {
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(choiceIs(QStringLiteral("extent"), QStringLiteral("to_object"))),
      choiceInput(QStringLiteral("type"), QObject::tr("Hole Type"),
                  {{QStringLiteral("simple"), QStringLiteral("Simple")},
                   {QStringLiteral("counterbore"), QStringLiteral("Counterbore")},
                   {QStringLiteral("countersink"), QStringLiteral("Countersink")},
                   {QStringLiteral("counterdrill"), QStringLiteral("Counterdrill")}},
                  QStringLiteral("simple"))
          .withOnChange(fillSized),
      // Clearance (mitcad#903): the hole and its recesses for a screw size.
      choiceInput(QStringLiteral("tap"), QObject::tr("Hole Tap Type"),
                  {{QStringLiteral("simple"), QStringLiteral("Simple")},
                   {QStringLiteral("tapped"), QStringLiteral("Tapped")},
                   {kClearance, QStringLiteral("Clearance")}},
                  QStringLiteral("simple"))
          .withTooltip(QObject::tr("Simple, Tapped (threaded), or Clearance: sized for a screw, with its recesses"))
          .withOnChange(startTap),
      choiceInput(QStringLiteral("drill"), QObject::tr("Drill Point"),
                  {{QStringLiteral("angle"), QStringLiteral("Angle")}, {QStringLiteral("flat"), QStringLiteral("Flat")}},
                  QStringLiteral("angle")),
      choiceInput(kScrewSize, QObject::tr("Screw Size"), {}, holeSizeTable(context).defaultSize)
          .withTooltip(QObject::tr("The ISO metric screw the hole is for; it sizes the hole and its recesses"))
          .withChoices([&context](const CommandState&) {
            QVector<QPair<QString, QString>> options;
            for (const HoleSize& size : holeSizeTable(context).sizes) {
              options.append({size.size, size.size});
            }
            return options;
          })
          .withOnChange(fillSized)
          .withVisible(choiceIs(QStringLiteral("tap"), kClearance)),
      choiceInput(kFit, QObject::tr("Fit"), {}, holeSizeTable(context).defaultFit)
          .withTooltip(QObject::tr("ISO 273's clearance: Fine, Normal (the medium series) or Coarse"))
          .withChoices([&context](const CommandState&) { return holeSizeTable(context).fits; })
          .withOnChange(fillSized)
          .withVisible(choiceIs(QStringLiteral("tap"), kClearance)),
      // A tapered taper pipe thread's hole keeps its diameter, the tap
      // drill (mitcad#733); another thread sets it.
      sizedValueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("6 mm"))
          .withTooltip(QObject::tr("The hole's diameter; a tapered pipe thread's tap drill; a clearance hole's "
                                   "from its screw"))
          .withVisible([tapped](const CommandState& s) { return !tapped(s) || tapers(s); }),
      // A tapped hole has no taper (the model refuses it).
      valueInput(QStringLiteral("taper"), QObject::tr("Taper Angle"), ValueKind::Angle, QStringLiteral("0") + QChar(0x00B0))
          .withTooltip(QObject::tr("The wall's angle to the axis; positive narrows the hole as it goes deeper"))
          .withVisible([tapped](const CommandState& s) { return !tapped(s); }),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(choiceIs(QStringLiteral("extent"), QStringLiteral("distance")))
          // Raised to fit a recess the size fills until typed (mitcad#980).
          .withOnChange(markDepthTyped)
          .withManipulator([place](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            // From where the hole starts, the way it goes: along the axis
            // into a shaft or a hole, into a face or at a point against
            // the normal; the hole's Flip turns it round.
            const auto frame = placeFrame(state, model, place);
            if (!frame) {
              return std::nullopt;
            }
            const bool along = placeForm(state, place) == PlaceForm::Axis;
            const bool flipped = state.checked(QStringLiteral("flip"));
            Manipulator m = arrow(frame->origin, along != flipped ? frame->normal : frame->normal.Reversed());
            m.valueBox = true; // the depth at its tip (mitcad#656)
            m.boxAtTip = true;
            return m;
          }),
      // The recesses' fields follow the size until typed (mitcad#980).
      sizedValueInput(QStringLiteral("cb_diameter"), QObject::tr("Counterbore Diameter"), ValueKind::Length,
                      QStringLiteral("10 mm"))
          .withVisible(type("counterbore")),
      sizedValueInput(QStringLiteral("cb_depth"), QObject::tr("Counterbore Depth"), ValueKind::Length,
                      QStringLiteral("3 mm"))
          .withVisible(type("counterbore")),
      sizedValueInput(QStringLiteral("cs_diameter"), QObject::tr("Countersink Diameter"), ValueKind::Length,
                      QStringLiteral("10 mm"))
          .withVisible(type("countersink")),
      sizedValueInput(QStringLiteral("cs_angle"), QObject::tr("Countersink Angle"), ValueKind::Angle,
                      QStringLiteral("90") + QChar(0x00B0))
          .withVisible(type("countersink")),
      sizedValueInput(QStringLiteral("cd_diameter"), QObject::tr("Counterdrill Diameter"), ValueKind::Length,
                      QStringLiteral("10 mm"))
          .withVisible(type("counterdrill")),
      sizedValueInput(QStringLiteral("cd_depth"), QObject::tr("Counterdrill Depth"), ValueKind::Length,
                      QStringLiteral("3 mm"))
          .withTooltip(QObject::tr("The depth of its cylinder; the cone narrowing to the hole is below it"))
          .withVisible(type("counterdrill")),
      sizedValueInput(QStringLiteral("cd_angle"), QObject::tr("Counterdrill Angle"), ValueKind::Angle,
                      QStringLiteral("90") + QChar(0x00B0))
          .withTooltip(QObject::tr("The full angle of the cone at its floor"))
          .withVisible(type("counterdrill")),
      valueInput(QStringLiteral("tip_angle"), QObject::tr("Drill Point Angle"), ValueKind::Angle, QStringLiteral("118") + QChar(0x00B0))
          .withVisible(choiceIs(QStringLiteral("drill"), QStringLiteral("angle"))),
  };
  // A new size: a tapered pipe thread's tap drill, the recesses' values
  // (mitcad#903, mitcad#980).
  def.inputs += threadInputs(context, {QStringLiteral("6"), QStringLiteral("M6x1"), QStringLiteral("6H")}, true, tapped,
                             [](CommandState& state, const CommandContext& model) {
                               startTapDrill(state, model);
                               fillSized(state, model);
                             });
  def.inputs.append(taperInput(tapped).withOnChange(startTapDrill));
  // The thread's hand (mitcad#893: a custom thread's too).
  def.inputs.append(choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                                {{QStringLiteral("right"), QStringLiteral("Right Hand")},
                                 {QStringLiteral("left"), QStringLiteral("Left Hand")}},
                                QStringLiteral("right"))
                        .withVisible(tapped));
  def.inputs.append(checkInput(QStringLiteral("modeled"), QObject::tr("Modeled"), false).withVisible(tapped));
  def.inputs.append(startsInput(tapped));
  def.inputs += clearanceInputs(
      [tapped](const CommandState& s) { return tapped(s) && s.checked(QStringLiteral("modeled")); });
  def.inputs += {
      selectionInput(QStringLiteral("objects"), QObject::tr("Objects to Cut"), SelectKind::Body, 0, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("The bodies the holes cut; none: every body they reach")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.init = [place](CommandState& state, const CommandContext& model) { initPlace(state, model, place); };

  def.build = [place](const CommandState& state, const CommandContext& model) {
    Built result;
    QJsonObject placement;
    const QString placed = state.choice(QStringLiteral("placement"));
    if (placed == QStringLiteral("face")) {
      QJsonObject at;
      if (!addPlace(at, state, model, place, result)) {
        return result;
      }
      if (at.contains(QStringLiteral("placement"))) {
        // A position on a face written before, unchanged: as it was.
        placement = at.value(QStringLiteral("placement")).toObject();
      } else if (!at.contains(QStringLiteral("at"))) {
        return Built::failure(QObject::tr("Pick a shaft or a hole of the active component"), QStringLiteral("at"));
      } else {
        placement = {{QStringLiteral("type"), QStringLiteral("at")}, {QStringLiteral("at"), at.value(QStringLiteral("at"))}};
      }
    } else if (placed == QStringLiteral("positions")) {
      placement = state.object(kKept);
      // Its distances as typed (mitcad#733).
      if (str(placement, "type") == QStringLiteral("face_offsets")) {
        placement.insert(QStringLiteral("offset1"), state.expression(kOffset1));
        placement.insert(QStringLiteral("offset2"), state.expression(kOffset2));
      } else if (str(placement, "type") == QStringLiteral("cylinder_offset")) {
        placement.insert(QStringLiteral("offset"), state.expression(kShaftOffset));
      }
    } else {
      const Selection& items = state.items(QStringLiteral("points"));
      const QString sketch = oneSketch(items, result, QStringLiteral("points"));
      if (sketch.isEmpty()) {
        return result;
      }
      QJsonArray points;
      for (const SelectionItem& item : items) {
        points.append(item.name);
      }
      placement = {{QStringLiteral("type"), QStringLiteral("sketch_points")},
                   {QStringLiteral("sketch"), sketch},
                   {QStringLiteral("points"), points}};
    }
    const QString type = state.choice(QStringLiteral("type"));
    QJsonObject kind{{QStringLiteral("type"), type}};
    if (type == QStringLiteral("counterbore")) {
      kind.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("cb_diameter")));
      kind.insert(QStringLiteral("depth"), state.expression(QStringLiteral("cb_depth")));
    } else if (type == QStringLiteral("countersink")) {
      kind.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("cs_diameter")));
      kind.insert(QStringLiteral("angle"), state.expression(QStringLiteral("cs_angle")));
    } else if (type == QStringLiteral("counterdrill")) {
      kind.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("cd_diameter")));
      kind.insert(QStringLiteral("depth"), state.expression(QStringLiteral("cd_depth")));
      kind.insert(QStringLiteral("angle"), state.expression(QStringLiteral("cd_angle")));
    }
    const QString extentType = state.choice(QStringLiteral("extent"));
    QJsonObject extent{{QStringLiteral("type"), extentType}};
    if (extentType == QStringLiteral("distance")) {
      extent.insert(QStringLiteral("depth"), state.expression(QStringLiteral("depth")));
    } else if (extentType == QStringLiteral("to_object")) {
      const QString foreign = foreignObject(state, model);
      if (!foreign.isEmpty()) {
        return Built::failure(foreign, QStringLiteral("object"));
      }
      extent.insert(QStringLiteral("object"), objectOf(state, QString()));
      if (nonZero(state, QStringLiteral("offset"))) {
        extent.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      }
    }
    const bool tapped = state.choice(QStringLiteral("tap")) == QStringLiteral("tapped");
    result.def = {{QStringLiteral("type"), QStringLiteral("hole")},
                  {QStringLiteral("placement"), placement},
                  // A tapped hole's bore comes from its thread (but a taper
                  // pipe thread's, mitcad#733); the model ignores this then.
                  {QStringLiteral("diameter"), state.expression(QStringLiteral("diameter"))},
                  {QStringLiteral("kind"), kind},
                  {QStringLiteral("extent"), extent}};
    if (state.choice(QStringLiteral("drill")) == QStringLiteral("flat")) {
      result.def.insert(QStringLiteral("flat"), true);
    } else {
      result.def.insert(QStringLiteral("tip_angle"), state.expression(QStringLiteral("tip_angle")));
    }
    if (!tapped && nonZero(state, QStringLiteral("taper"))) {
      result.def.insert(QStringLiteral("taper"), state.expression(QStringLiteral("taper")));
    }
    if (tapped) {
      QJsonObject thread{{QStringLiteral("standard"), state.choice(QStringLiteral("standard"))},
                         {QStringLiteral("modeled"), state.checked(QStringLiteral("modeled"))}};
      // A size of the table, or a custom thread's own (mitcad#893).
      if (customThread(state)) {
        thread.insert(QStringLiteral("custom"), customSize(state));
        if (state.checked(QStringLiteral("modeled"))) {
          thread.insert(kStarts, state.expression(kStarts));
        }
      } else {
        thread.insert(QStringLiteral("designation"), state.choice(QStringLiteral("designation")));
        thread.insert(QStringLiteral("class"), state.choice(QStringLiteral("class")));
      }
      if (state.choice(QStringLiteral("direction")) == QStringLiteral("left")) {
        thread.insert(QStringLiteral("right_handed"), false);
      }
      if (state.checked(QStringLiteral("modeled"))) {
        insertClearance(state, thread);
      }
      // A tapered taper pipe thread: drilled at the Diameter, the bore
      // tapered (mitcad#733).
      if (tapers(state)) {
        thread.insert(QStringLiteral("taper"), true);
      }
      result.def.insert(QStringLiteral("thread"), thread);
    }
    // A clearance hole names its screw size and fit (mitcad#903).
    if (state.choice(QStringLiteral("tap")) == kClearance) {
      result.def.insert(QStringLiteral("clearance"), QJsonObject{{QStringLiteral("size"), state.choice(kScrewSize)},
                                                                 {QStringLiteral("fit"), state.choice(kFit)}});
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    const Selection& objects = state.items(QStringLiteral("objects"));
    if (!objects.isEmpty()) {
      result.def.insert(QStringLiteral("participants"), bodyUids(objects));
    }
    return result;
  };

  def.load = [place](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QJsonObject placement = feature.value(QStringLiteral("placement")).toObject();
    state.setObject(kKept, QJsonObject());
    const QString placed = str(placement, "type");
    if (placed == QStringLiteral("at")) {
      state.setChoice(QStringLiteral("placement"), QStringLiteral("face"));
      loadPlace({{QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}, {QStringLiteral("at"), placement.value(QStringLiteral("at"))}},
                state, context, place);
    } else if (placed == QStringLiteral("face")) {
      // Positions on a face written before (mitcad#656): one is placed on
      // the face from the origin axes and saved as it was until it is
      // moved (then as `at`); several stay as they were until Place On is
      // picked again.
      const QJsonArray points = placement.value(QStringLiteral("points")).toArray();
      const QJsonObject face{{QStringLiteral("body"), placement.value(QStringLiteral("body"))},
                             {QStringLiteral("face"), placement.value(QStringLiteral("face"))}};
      loadPlace({{QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}}, state, context, place);
      const QJsonArray first = points.at(0).toArray();
      const bool onFace = placeOnPlaneAt(state, context, face,
                                         gp_Pnt(first.at(0).toDouble(), first.at(1).toDouble(), first.at(2).toDouble()));
      if (points.size() > 1 || !onFace) {
        state.setObject(kKept, placement);
        state.setChoice(QStringLiteral("placement"), QStringLiteral("positions"));
      } else {
        state.setChoice(QStringLiteral("placement"), QStringLiteral("face"));
        logPlacement(state, context, place);
        keepEarlierForm(state, context, place, {{QStringLiteral("placement"), placement}});
      }
    } else if (placed == QStringLiteral("face_offsets") || placed == QStringLiteral("cylinder_offset") ||
               placed == QStringLiteral("plane")) {
      // At distances from two edges, along a shaft or on a plane (the .f3d
      // import's, mitcad#733): kept as it was, its distances editable.
      loadPlace({{QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}}, state, context, place);
      state.setObject(kKept, placement);
      state.setChoice(QStringLiteral("placement"), QStringLiteral("positions"));
      loadValue(state, kOffset1, placement.value(QStringLiteral("offset1")), feature, context);
      loadValue(state, kOffset2, placement.value(QStringLiteral("offset2")), feature, context);
      loadValue(state, kShaftOffset, placement.value(QStringLiteral("offset")), feature, context);
      qDebug().noquote() << QStringLiteral("Hole placement kept: %1").arg(placed);
    } else {
      state.setChoice(QStringLiteral("placement"), QStringLiteral("sketch"));
      const QString sketch = str(placement, "sketch");
      Selection items;
      for (const QJsonValue& point : placement.value(QStringLiteral("points")).toArray()) {
        const QString id = point.toString();
        items.append(id.startsWith(QLatin1Char('p'))
                         ? itemOf(QJsonObject{{QStringLiteral("sketch"), sketch}, {QStringLiteral("point"), id}}, context)
                         : itemOf(QJsonObject{{QStringLiteral("sketch"), sketch}, {QStringLiteral("curve"), id}}, context));
      }
      state.setItems(QStringLiteral("points"), items);
    }
    loadValue(state, QStringLiteral("diameter"), feature.value(QStringLiteral("diameter")), feature, context);
    const QJsonObject kind = feature.value(QStringLiteral("kind")).toObject();
    const QString type = kind.value(QStringLiteral("type")).toString(QStringLiteral("simple"));
    state.setChoice(QStringLiteral("type"), type);
    if (type == QStringLiteral("counterbore")) {
      loadValue(state, QStringLiteral("cb_diameter"), kind.value(QStringLiteral("diameter")), feature, context);
      loadValue(state, QStringLiteral("cb_depth"), kind.value(QStringLiteral("depth")), feature, context);
    } else if (type == QStringLiteral("countersink")) {
      loadValue(state, QStringLiteral("cs_diameter"), kind.value(QStringLiteral("diameter")), feature, context);
      loadValue(state, QStringLiteral("cs_angle"), kind.value(QStringLiteral("angle")), feature, context);
    } else if (type == QStringLiteral("counterdrill")) {
      loadValue(state, QStringLiteral("cd_diameter"), kind.value(QStringLiteral("diameter")), feature, context);
      loadValue(state, QStringLiteral("cd_depth"), kind.value(QStringLiteral("depth")), feature, context);
      loadValue(state, QStringLiteral("cd_angle"), kind.value(QStringLiteral("angle")), feature, context);
    }
    loadValue(state, QStringLiteral("taper"), feature.value(QStringLiteral("taper")), feature, context);
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    state.setChoice(QStringLiteral("extent"), str(extent, "type"));
    loadValue(state, QStringLiteral("depth"), extent.value(QStringLiteral("depth")), feature, context);
    loadValue(state, QStringLiteral("offset"), extent.value(QStringLiteral("offset")), feature, context);
    if (str(extent, "type") == QStringLiteral("to_object")) {
      loadObject(extent.value(QStringLiteral("object")).toObject(), state, QString(), context);
    }
    // The hole's own component, which its object must be of (mitcad#762).
    state.setObject(kHoleOwner, QJsonObject());
    if (feature.contains(QStringLiteral("uid"))) {
      try {
        const QJsonObject stored = context.queryObject(
            {{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}});
        state.setObject(kHoleOwner, {{QStringLiteral("component"),
                                      stored.value(QStringLiteral("component")).toString(QStringLiteral("C0"))}});
      } catch (const std::exception&) {
        // The active component then.
      }
    }
    if (feature.value(QStringLiteral("flat")).toBool()) {
      state.setChoice(QStringLiteral("drill"), QStringLiteral("flat"));
    }
    loadValue(state, QStringLiteral("tip_angle"), feature.value(QStringLiteral("tip_angle")), feature, context);
    const QJsonObject thread = feature.value(QStringLiteral("thread")).toObject();
    if (!thread.isEmpty()) {
      state.setChoice(QStringLiteral("tap"), QStringLiteral("tapped"));
      loadThreadInputs(thread, state, context);
      loadCustomSize(thread.value(QStringLiteral("custom")).toObject(), thread.value(kStarts), feature, state, context);
      state.setChoice(QStringLiteral("direction"),
                      thread.value(QStringLiteral("right_handed")).toBool(true) ? QStringLiteral("right") : QStringLiteral("left"));
      state.setChecked(QStringLiteral("modeled"), thread.value(QStringLiteral("modeled")).toBool());
      loadClearance(thread, feature, state, context);
      // A hole saved before mitcad#733 keeps its straight thread.
      state.setChecked(kTaperThread, thread.value(QStringLiteral("taper")).toBool());
    }
    // A clearance hole's size and fit (mitcad#903); its values that the
    // size does not give stay as they are when the size changes.
    const QJsonObject clearance = feature.value(QStringLiteral("clearance")).toObject();
    if (!clearance.isEmpty()) {
      state.setChoice(QStringLiteral("tap"), kClearance);
      state.setChoice(kScrewSize, str(clearance, "size"));
      state.setChoice(kFit, clearance.value(QStringLiteral("fit")).toString(QStringLiteral("normal")));
    }
    markLoaded(feature, state, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setItems(QStringLiteral("objects"), bodiesOf(feature.value(QStringLiteral("participants")).toArray()));
  };

  // Holes at sketch coordinates or partial threads have no inputs (a
  // left-handed thread has its Direction since mitcad#893).
  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonObject placement = feature.value(QStringLiteral("placement")).toObject();
    const QString type = str(placement, "type");
    if (type == QStringLiteral("at") &&
        !placeShowable({{QStringLiteral("at"), placement.value(QStringLiteral("at"))}}, context)) {
      return false;
    }
    if (type == QStringLiteral("sketch_points")) {
      for (const QJsonValue& point : placement.value(QStringLiteral("points")).toArray()) {
        if (!point.isString()) {
          return false;
        }
      }
    }
    const QJsonObject thread = feature.value(QStringLiteral("thread")).toObject();
    if (thread.contains(QStringLiteral("length")) || thread.contains(QStringLiteral("offset"))) {
      return false;
    }
    // Starts are offered for a custom thread only (mitcad#893).
    if (thread.contains(kStarts) && str(thread, "standard") != kCustom) {
      return false;
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    return str(extent, "type") != QStringLiteral("to_object") ||
           objectShowable(extent.value(QStringLiteral("object")).toObject(), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString placed = state.choice(QStringLiteral("placement"));
    // (A kept placement at distances or along a shaft has one hole.)
    const QJsonArray kept = state.object(kKept).value(QStringLiteral("points")).toArray();
    const int count = placed == QStringLiteral("face")        ? 1
                      : placed == QStringLiteral("positions") ? std::max(1, static_cast<int>(kept.size()))
                                                              : static_cast<int>(state.items(QStringLiteral("points")).size());
    const bool tapped = state.choice(QStringLiteral("tap")) == QStringLiteral("tapped");
    const QString taper = !tapped && nonZero(state, QStringLiteral("taper"))
                              ? QStringLiteral(", taper %1 deg").arg(degrees(state.value(QStringLiteral("taper"))))
                              : QString();
    // (A custom thread's diameter and pitch, mitcad#893.)
    QString tap = !tapped ? QStringLiteral("diameter %1").arg(number(state.value(QStringLiteral("diameter"))))
                  : customThread(state) ? sizeText(state)
                                        : state.choice(QStringLiteral("designation"));
    // (A clearance hole's screw size and fit, mitcad#903.)
    if (state.choice(QStringLiteral("tap")) == kClearance) {
      tap += QStringLiteral(" (clearance %1 %2)").arg(state.choice(kScrewSize), state.choice(kFit));
    }
    return QStringLiteral("Added %1 hole(s) (%2), %3%4").arg(count).arg(state.choice(QStringLiteral("type")), tap, taper);
  };
  return def;
}

// ---------------------------------------------------------------------------
// Thread

CommandDef threadCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.thread");
  def.name = QObject::tr("Thread");
  def.icon = QStringLiteral("thread");
  def.tooltip = QObject::tr("Threads on cylindrical faces: cosmetic, or modelled into the geometry");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("faces");
  def.keywords = {QStringLiteral("tap"), QStringLiteral("screw"), QStringLiteral("bolt")};
  def.featureType = QStringLiteral("thread");
  // The faces are what it threads, not where it goes: the view's context
  // menu offers it first for them (mitcad#494).
  def.completes = [](const Selection&) { return true; };
  const auto partial = [](const CommandState& s) { return !s.checked(QStringLiteral("full_length")); };
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Cylindrical faces, all inside (holes) or all outside"))
          .withAccepts([](const SelectionItem& item) { return item.geometry == QStringLiteral("cylinder"); })
          .withOnPick(
              [](const SelectionItem& item, CommandState& state, const CommandContext& model) {
                // The first face suggests the size.
                if (state.items(QStringLiteral("faces")).size() == 1) {
                  suggestSize(item, state, model);
                }
              },
              false)
          // The faces' side keeps the class on it (mitcad#691).
          .withOnChange([](CommandState& state, const CommandContext& model) {
            state.setChoice(kSide, sideOf(state.items(QStringLiteral("faces")), model));
            keepClassOnSide(state, model, false);
          }),
  };
  // Faces selected before the command (the view's context menu offers
  // Thread first for them) suggest the size as a face picked in the open
  // command does: the first cylindrical one (mitcad#691).
  def.preselected = [](const Selection&, CommandState& state, const CommandContext& model) {
    const Selection& faces = state.items(QStringLiteral("faces"));
    for (const SelectionItem& item : faces) {
      if (cylinderOf(item, model)) {
        suggestSize(item, state, model);
        break;
      }
    }
    state.setChoice(kSide, sideOf(faces, model));
    keepClassOnSide(state, model, false);
  };
  def.inputs += threadInputs(context, {QStringLiteral("10"), QStringLiteral("M10x1.5"), QStringLiteral("6g")}, false,
                             nullptr);
  def.inputs.append(taperInput(nullptr));
  def.inputs += {
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                  {{QStringLiteral("right"), QStringLiteral("Right Hand")},
                   {QStringLiteral("left"), QStringLiteral("Left Hand")}},
                  QStringLiteral("right")),
      checkInput(QStringLiteral("modeled"), QObject::tr("Modeled"), false)
          .withTooltip(QObject::tr("Cut the thread into the geometry; otherwise only listed")),
      startsInput(nullptr),
  };
  def.inputs += clearanceInputs([](const CommandState& s) { return s.checked(QStringLiteral("modeled")); });
  for (InputDef& input : def.inputs) {
    if (input.id == kRadialClearance || input.id == kFlankClearance) {
      input.withPrintedFit(QStringLiteral("thread"));
    }
  }
  def.inputs += {
      checkInput(QStringLiteral("full_length"), QObject::tr("Full Length"), true),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(partial),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(partial),
      // A tapered thread over the whole face starts at the end it names
      // where both ends or neither is free (mitcad#733).
      choiceInput(QStringLiteral("location"), QObject::tr("Thread Location"),
                  {{QStringLiteral("high_end"), QStringLiteral("High End")},
                   {QStringLiteral("low_end"), QStringLiteral("Low End")}},
                  QStringLiteral("high_end"))
          .withTooltip(QObject::tr("The end the thread is measured from; a tapered thread starts at a free end, "
                                   "else at this one"))
          .withVisible([partial](const CommandState& s) { return partial(s) || tapers(s); }),
  };
  def.inputs += chamferInputs(context);
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonArray faces;
    for (const SelectionItem& item : state.items(QStringLiteral("faces"))) {
      faces.append(item.reference());
    }
    QJsonObject thread{{QStringLiteral("standard"), state.choice(QStringLiteral("standard"))}};
    // A size of the table, or a custom thread's own (mitcad#893).
    if (!customThread(state)) {
      thread.insert(QStringLiteral("designation"), state.choice(QStringLiteral("designation")));
      thread.insert(QStringLiteral("class"), state.choice(QStringLiteral("class")));
    }
    if (state.choice(QStringLiteral("direction")) == QStringLiteral("left")) {
      thread.insert(QStringLiteral("right_handed"), false);
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("thread")},
                  {QStringLiteral("faces"), faces},
                  {QStringLiteral("thread"), thread},
                  {QStringLiteral("modeled"), state.checked(QStringLiteral("modeled"))}};
    if (customThread(state)) {
      result.def.insert(QStringLiteral("custom"), customSize(state));
      if (state.checked(QStringLiteral("modeled"))) {
        result.def.insert(kStarts, state.expression(kStarts));
      }
    }
    if (state.checked(QStringLiteral("modeled"))) {
      insertClearance(state, result.def);
    }
    if (!state.checked(QStringLiteral("full_length"))) {
      result.def.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
      result.def.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      result.def.insert(QStringLiteral("location"), state.choice(QStringLiteral("location")));
    } else if (tapers(state) && state.choice(QStringLiteral("location")) != QStringLiteral("high_end")) {
      result.def.insert(QStringLiteral("location"), state.choice(QStringLiteral("location")));
    }
    // A tapered taper pipe thread (mitcad#733).
    if (tapers(state)) {
      result.def.insert(QStringLiteral("taper"), true);
    }
    insertChamfer(state, result.def);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection faces;
    for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
      faces.append(itemOf(face, context));
    }
    state.setItems(QStringLiteral("faces"), faces);
    const QJsonObject thread = feature.value(QStringLiteral("thread")).toObject();
    loadThreadInputs(thread, state, context);
    loadCustomSize(feature.value(QStringLiteral("custom")).toObject(), feature.value(kStarts), feature, state, context);
    state.setChoice(QStringLiteral("direction"),
                    thread.value(QStringLiteral("right_handed")).toBool(true) ? QStringLiteral("right") : QStringLiteral("left"));
    state.setChecked(QStringLiteral("modeled"), feature.value(QStringLiteral("modeled")).toBool());
    loadClearance(feature, feature, state, context);
    const bool partial = feature.contains(QStringLiteral("length"));
    state.setChecked(QStringLiteral("full_length"), !partial);
    loadValue(state, QStringLiteral("length"), feature.value(QStringLiteral("length")), feature, context);
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    state.setChoice(QStringLiteral("location"), feature.value(QStringLiteral("location")).toString(QStringLiteral("high_end")));
    // A thread saved before mitcad#733 stays straight.
    state.setChecked(kTaperThread, feature.value(QStringLiteral("taper")).toBool());
    loadChamfer(feature, state, context);
  };

  // Starts are offered for a custom thread only (mitcad#893): a thread of a
  // standard size with starts has no inputs.
  def.canEdit = [](const QJsonObject& feature) {
    return !feature.contains(kStarts) ||
           str(feature.value(QStringLiteral("thread")).toObject(), "standard") == kCustom;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added %1 thread %2 on %3 face(s)")
        .arg(state.checked(QStringLiteral("modeled")) ? QStringLiteral("modeled") : QStringLiteral("cosmetic"),
             sizeText(state))
        .arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

} // namespace mitcad::cmd
