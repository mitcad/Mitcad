// SPDX-License-Identifier: MIT
// Hole and Thread (U4; commands.md, "hole", "thread"). A hole is placed by
// clicks on a planar face, each click a row of the position table, or at
// sketch points; it may be counterdrilled and tapered (mitcad#27, as the
// FreeCAD import makes them). A thread's size is suggested from the
// cylinder picked. Sizes, designations and classes are chosen from the
// model's table (`thread_sizes`, P9), ISO metric first (P11).
#include <algorithm>
#include <cmath>
#include <limits>

#include <QJsonArray>
#include <QKeySequence>

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp_Face.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const QString kPositions = QStringLiteral("positions");

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
      entry.sizes.append({str(size.toObject(), "size"), strings(size.toObject().value(QStringLiteral("designations")))});
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

// The size table's inputs: Thread Type, Size, Designation and Class,
// starting at an ISO metric size (M6x1 6H for holes,
// M10x1.5 6g for threads); `internalOnly`: the classes are a nut's (tapped
// holes), else both (threads).
struct ThreadStart {
  QString size;
  QString designation;
  QString threadClass;
};

QVector<InputDef> threadInputs(const CommandContext& context, const ThreadStart& start, bool internalOnly,
                               const std::function<bool(const CommandState&)>& shown) {
  const auto standardIn = [&context](const CommandState& state) {
    return standardOf(context, state.choice(QStringLiteral("standard")));
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
  const auto startStandard = [&context, standardIn, start, internalOnly](CommandState& state, const CommandContext&) {
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
  };
  QVector<QPair<QString, QString>> standards;
  for (const ThreadStandard& standard : threadTable(context)) {
    standards.append({standard.id, standard.title});
  }
  if (standards.isEmpty()) {
    standards = {{QStringLiteral("iso_metric"), QStringLiteral("ISO Metric profile")},
                 {QStringLiteral("unified"), QStringLiteral("ANSI Unified Screw Threads")}};
  }
  QVector<InputDef> inputs = {
      choiceInput(QStringLiteral("standard"), QObject::tr("Thread Type"), standards, standards.first().first)
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
          .withOnChange([designationsIn](CommandState& state, const CommandContext&) {
            state.setChoice(QStringLiteral("designation"), designationsIn(state).value(0));
          }),
      choiceInput(QStringLiteral("designation"), QObject::tr("Designation"), {}, start.designation)
          .withTooltip(QObject::tr("The size and pitch: M10x1.5 (coarse), M10x1.25 (fine), 1/4-20 UNC"))
          .withChoices([designationsIn](const CommandState& state) { return labelled(designationsIn(state)); }),
      choiceInput(QStringLiteral("class"), QObject::tr("Class"), {}, start.threadClass)
          .withTooltip(internalOnly ? QObject::tr("The tolerance class of the tapped hole")
                                    : QObject::tr("The tolerance class: 6g outside, 6H inside"))
          .withChoices([standardIn, internalOnly](const CommandState& state) {
            const ThreadStandard* standard = standardIn(state);
            if (standard == nullptr) {
              return QVector<QPair<QString, QString>>();
            }
            return labelled(internalOnly ? standard->internal : standard->external + standard->internal);
          }),
  };
  for (InputDef& input : inputs) {
    input.withVisible(shown);
  }
  return inputs;
}

// Fills the size table's inputs from a thread definition.
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

} // namespace

// ---------------------------------------------------------------------------
// Hole

CommandDef holeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.hole");
  def.name = QObject::tr("Hole");
  def.icon = QStringLiteral("hole");
  def.tooltip = QObject::tr("Drilled holes: simple, counterbore, countersink or counterdrill, straight or "
                            "tapered, tapped or not, clicked onto a face or at sketch points");
  def.shortcut = QKeySequence(Qt::Key_H);
  def.group = QStringLiteral("CREATE");
  def.pinned = true;
  def.keywords = {QStringLiteral("drill"), QStringLiteral("tap"), QStringLiteral("counterbore"),
                  QStringLiteral("countersink"), QStringLiteral("counterdrill"), QStringLiteral("taper")};
  def.featureType = QStringLiteral("hole");
  const auto onFace = choiceIs(QStringLiteral("placement"), QStringLiteral("face"));
  const auto tapped = choiceIs(QStringLiteral("tap"), QStringLiteral("tapped"));
  const auto type = [](const char* kind) { return choiceIs(QStringLiteral("type"), QString::fromLatin1(kind)); };
  def.inputs = {
      choiceInput(QStringLiteral("placement"), QObject::tr("Placement"),
                  {{QStringLiteral("face"), QStringLiteral("On Face")},
                   {QStringLiteral("sketch"), QStringLiteral("From Sketch")}},
                  QStringLiteral("face")),
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A planar face; each click on it places a hole"))
          .withAccepts([](const SelectionItem& item) { return item.geometry == QStringLiteral("plane"); })
          .withVisible(onFace)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            if (!item.at) {
              return;
            }
            const int row = state.rows(kPositions);
            state.setRows(kPositions, row + 1);
            const char* axes[] = {"x", "y", "z"};
            for (int i = 0; i < 3; ++i) {
              const QString key = CommandState::rowKey(kPositions, row, QString::fromLatin1(axes[i]));
              state.setText(key, lengthText(std::round((*item.at)[static_cast<std::size_t>(i)] * 100.0) / 100.0, model));
              state.setValue(key, std::numeric_limits<double>::quiet_NaN());
            }
          }),
      listInput(kPositions, QObject::tr("Positions"),
                {valueInput(QStringLiteral("x"), QObject::tr("X"), ValueKind::Length, QStringLiteral("0 mm")),
                 valueInput(QStringLiteral("y"), QObject::tr("Y"), ValueKind::Length, QStringLiteral("0 mm")),
                 valueInput(QStringLiteral("z"), QObject::tr("Z"), ValueKind::Length, QStringLiteral("0 mm"))},
                QObject::tr("Add Position"), QObject::tr("Hole %1"), 0)
          .withTooltip(QObject::tr("Where the holes are: put onto the face's plane"))
          .withVisible(onFace),
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
                  QStringLiteral("simple")),
      choiceInput(QStringLiteral("tap"), QObject::tr("Hole Tap Type"),
                  {{QStringLiteral("simple"), QStringLiteral("Simple")},
                   {QStringLiteral("tapped"), QStringLiteral("Tapped")}},
                  QStringLiteral("simple")),
      choiceInput(QStringLiteral("drill"), QObject::tr("Drill Point"),
                  {{QStringLiteral("angle"), QStringLiteral("Angle")}, {QStringLiteral("flat"), QStringLiteral("Flat")}},
                  QStringLiteral("angle")),
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("6 mm"))
          .withVisible([tapped](const CommandState& s) { return !tapped(s); }),
      // A tapped hole has no taper (the model refuses it).
      valueInput(QStringLiteral("taper"), QObject::tr("Taper Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("The wall's angle to the axis; positive narrows the hole as it goes deeper"))
          .withVisible([tapped](const CommandState& s) { return !tapped(s); }),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(choiceIs(QStringLiteral("extent"), QStringLiteral("distance")))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            // Into the face from the first position.
            if (state.choice(QStringLiteral("placement")) != QStringLiteral("face") ||
                state.rows(kPositions) == 0 || state.items(QStringLiteral("face")).isEmpty()) {
              return std::nullopt;
            }
            const auto frame = frameOf(state.items(QStringLiteral("face")).first(), model);
            const double x = state.value(CommandState::rowKey(kPositions, 0, QStringLiteral("x")));
            const double y = state.value(CommandState::rowKey(kPositions, 0, QStringLiteral("y")));
            const double z = state.value(CommandState::rowKey(kPositions, 0, QStringLiteral("z")));
            if (!frame || !std::isfinite(x + y + z)) {
              return std::nullopt;
            }
            // The position put onto the face's plane.
            gp_Pnt at(x, y, z);
            at.Translate(-gp_Vec(frame->normal) * gp_Vec(frame->origin, at).Dot(gp_Vec(frame->normal)));
            return arrow(at, frame->normal.Reversed());
          }),
      valueInput(QStringLiteral("cb_diameter"), QObject::tr("Counterbore Diameter"), ValueKind::Length,
                 QStringLiteral("10 mm"))
          .withVisible(type("counterbore")),
      valueInput(QStringLiteral("cb_depth"), QObject::tr("Counterbore Depth"), ValueKind::Length, QStringLiteral("3 mm"))
          .withVisible(type("counterbore")),
      valueInput(QStringLiteral("cs_diameter"), QObject::tr("Countersink Diameter"), ValueKind::Length,
                 QStringLiteral("10 mm"))
          .withVisible(type("countersink")),
      valueInput(QStringLiteral("cs_angle"), QObject::tr("Countersink Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible(type("countersink")),
      valueInput(QStringLiteral("cd_diameter"), QObject::tr("Counterdrill Diameter"), ValueKind::Length,
                 QStringLiteral("10 mm"))
          .withVisible(type("counterdrill")),
      valueInput(QStringLiteral("cd_depth"), QObject::tr("Counterdrill Depth"), ValueKind::Length, QStringLiteral("3 mm"))
          .withTooltip(QObject::tr("The depth of its cylinder; the cone narrowing to the hole is below it"))
          .withVisible(type("counterdrill")),
      valueInput(QStringLiteral("cd_angle"), QObject::tr("Counterdrill Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withTooltip(QObject::tr("The full angle of the cone at its floor"))
          .withVisible(type("counterdrill")),
      valueInput(QStringLiteral("tip_angle"), QObject::tr("Drill Point Angle"), ValueKind::Angle, QStringLiteral("118 deg"))
          .withVisible(choiceIs(QStringLiteral("drill"), QStringLiteral("angle"))),
  };
  def.inputs += threadInputs(context, {QStringLiteral("6"), QStringLiteral("M6x1"), QStringLiteral("6H")}, true, tapped);
  def.inputs += {
      checkInput(QStringLiteral("modeled"), QObject::tr("Modeled"), false).withVisible(tapped),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      selectionInput(QStringLiteral("objects"), QObject::tr("Objects to Cut"), SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("The bodies the holes cut; none: every body they reach")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject placement;
    if (state.choice(QStringLiteral("placement")) == QStringLiteral("face")) {
      const int rows = state.rows(kPositions);
      if (rows == 0) {
        return Built::failure(QObject::tr("Click on the face where a hole goes."), QStringLiteral("face"));
      }
      QJsonArray points;
      for (int i = 0; i < rows; ++i) {
        const CommandState row = state.row(kPositions, i);
        points.append(QJsonArray{row.value(QStringLiteral("x")), row.value(QStringLiteral("y")),
                                 row.value(QStringLiteral("z"))});
      }
      const SelectionItem& face = state.items(QStringLiteral("face")).first();
      placement = {{QStringLiteral("type"), QStringLiteral("face")},
                   {QStringLiteral("body"), face.owner},
                   {QStringLiteral("face"), face.name},
                   {QStringLiteral("points"), points}};
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
      extent.insert(QStringLiteral("object"), objectOf(state, QString()));
      if (nonZero(state, QStringLiteral("offset"))) {
        extent.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      }
    }
    const bool tapped = state.choice(QStringLiteral("tap")) == QStringLiteral("tapped");
    result.def = {{QStringLiteral("type"), QStringLiteral("hole")},
                  {QStringLiteral("placement"), placement},
                  // A tapped hole's bore comes from its thread; the model ignores this then.
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
      result.def.insert(QStringLiteral("thread"),
                        QJsonObject{{QStringLiteral("standard"), state.choice(QStringLiteral("standard"))},
                                    {QStringLiteral("designation"), state.choice(QStringLiteral("designation"))},
                                    {QStringLiteral("class"), state.choice(QStringLiteral("class"))},
                                    {QStringLiteral("modeled"), state.checked(QStringLiteral("modeled"))}});
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

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QJsonObject placement = feature.value(QStringLiteral("placement")).toObject();
    if (str(placement, "type") == QStringLiteral("face")) {
      state.setChoice(QStringLiteral("placement"), QStringLiteral("face"));
      state.setItems(QStringLiteral("face"),
                     {itemOf(QJsonObject{{QStringLiteral("body"), placement.value(QStringLiteral("body"))},
                                         {QStringLiteral("face"), placement.value(QStringLiteral("face"))}},
                             context)});
      const QJsonArray points = placement.value(QStringLiteral("points")).toArray();
      state.setRows(kPositions, static_cast<int>(points.size()));
      for (int i = 0; i < points.size(); ++i) {
        const QJsonArray point = points[i].toArray();
        const char* axes[] = {"x", "y", "z"};
        for (int k = 0; k < 3; ++k) {
          state.setText(CommandState::rowKey(kPositions, i, QString::fromLatin1(axes[k])),
                        lengthText(point.at(k).toDouble(), context));
        }
      }
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
    if (feature.value(QStringLiteral("flat")).toBool()) {
      state.setChoice(QStringLiteral("drill"), QStringLiteral("flat"));
    }
    loadValue(state, QStringLiteral("tip_angle"), feature.value(QStringLiteral("tip_angle")), feature, context);
    const QJsonObject thread = feature.value(QStringLiteral("thread")).toObject();
    if (!thread.isEmpty()) {
      state.setChoice(QStringLiteral("tap"), QStringLiteral("tapped"));
      loadThreadInputs(thread, state, context);
      state.setChecked(QStringLiteral("modeled"), thread.value(QStringLiteral("modeled")).toBool());
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setItems(QStringLiteral("objects"), bodiesOf(feature.value(QStringLiteral("participants")).toArray()));
  };

  // Holes at offsets from two edges, sketch coordinates, left-handed or
  // partial threads have no inputs.
  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonObject placement = feature.value(QStringLiteral("placement")).toObject();
    const QString type = str(placement, "type");
    if (type == QStringLiteral("face_offsets")) {
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
    if (thread.contains(QStringLiteral("length")) || thread.contains(QStringLiteral("offset")) ||
        !thread.value(QStringLiteral("right_handed")).toBool(true)) {
      return false;
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    return str(extent, "type") != QStringLiteral("to_object") ||
           objectShowable(extent.value(QStringLiteral("object")).toObject(), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const int count = state.choice(QStringLiteral("placement")) == QStringLiteral("face")
                          ? state.rows(kPositions)
                          : static_cast<int>(state.items(QStringLiteral("points")).size());
    const bool tapped = state.choice(QStringLiteral("tap")) == QStringLiteral("tapped");
    const QString taper = !tapped && nonZero(state, QStringLiteral("taper"))
                              ? QStringLiteral(", taper %1 deg").arg(degrees(state.value(QStringLiteral("taper"))))
                              : QString();
    return QStringLiteral("Added %1 hole(s) (%2), %3%4")
        .arg(count)
        .arg(state.choice(QStringLiteral("type")),
             tapped ? state.choice(QStringLiteral("designation"))
                    : QStringLiteral("diameter %1").arg(number(state.value(QStringLiteral("diameter")))),
             taper);
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
  def.keywords = {QStringLiteral("tap"), QStringLiteral("screw"), QStringLiteral("bolt")};
  def.featureType = QStringLiteral("thread");
  const auto partial = [](const CommandState& s) { return !s.checked(QStringLiteral("full_length")); };
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Cylindrical faces, all inside (holes) or all outside"))
          .withAccepts([](const SelectionItem& item) { return item.geometry == QStringLiteral("cylinder"); })
          .withOnPick(
              [](const SelectionItem& item, CommandState& state, const CommandContext& model) {
                // The first face suggests the size.
                if (state.items(QStringLiteral("faces")).size() != 1 ||
                    state.choice(QStringLiteral("standard")) != QStringLiteral("iso_metric")) {
                  return;
                }
                const auto cylinder = cylinderOf(item, model);
                if (!cylinder) {
                  return;
                }
                const MetricSize size = sizeFor(cylinder->first, cylinder->second);
                state.setChoice(QStringLiteral("size"), number(size.d));
                state.setChoice(QStringLiteral("designation"), designation(size));
                state.setChoice(QStringLiteral("class"), cylinder->second ? QStringLiteral("6H") : QStringLiteral("6g"));
              },
              false),
  };
  def.inputs += threadInputs(context, {QStringLiteral("10"), QStringLiteral("M10x1.5"), QStringLiteral("6g")}, false,
                             nullptr);
  def.inputs += {
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                  {{QStringLiteral("right"), QStringLiteral("Right Hand")},
                   {QStringLiteral("left"), QStringLiteral("Left Hand")}},
                  QStringLiteral("right")),
      checkInput(QStringLiteral("modeled"), QObject::tr("Modeled"), false)
          .withTooltip(QObject::tr("Cut the thread into the geometry; otherwise only listed")),
      checkInput(QStringLiteral("full_length"), QObject::tr("Full Length"), true),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(partial),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(partial),
      choiceInput(QStringLiteral("location"), QObject::tr("Thread Location"),
                  {{QStringLiteral("high_end"), QStringLiteral("High End")},
                   {QStringLiteral("low_end"), QStringLiteral("Low End")}},
                  QStringLiteral("high_end"))
          .withVisible(partial),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonArray faces;
    for (const SelectionItem& item : state.items(QStringLiteral("faces"))) {
      faces.append(item.reference());
    }
    QJsonObject thread{{QStringLiteral("standard"), state.choice(QStringLiteral("standard"))},
                       {QStringLiteral("designation"), state.choice(QStringLiteral("designation"))},
                       {QStringLiteral("class"), state.choice(QStringLiteral("class"))}};
    if (state.choice(QStringLiteral("direction")) == QStringLiteral("left")) {
      thread.insert(QStringLiteral("right_handed"), false);
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("thread")},
                  {QStringLiteral("faces"), faces},
                  {QStringLiteral("thread"), thread},
                  {QStringLiteral("modeled"), state.checked(QStringLiteral("modeled"))}};
    if (!state.checked(QStringLiteral("full_length"))) {
      result.def.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
      result.def.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      result.def.insert(QStringLiteral("location"), state.choice(QStringLiteral("location")));
    }
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
    state.setChoice(QStringLiteral("direction"),
                    thread.value(QStringLiteral("right_handed")).toBool(true) ? QStringLiteral("right") : QStringLiteral("left"));
    state.setChecked(QStringLiteral("modeled"), feature.value(QStringLiteral("modeled")).toBool());
    const bool partial = feature.contains(QStringLiteral("length"));
    state.setChecked(QStringLiteral("full_length"), !partial);
    loadValue(state, QStringLiteral("length"), feature.value(QStringLiteral("length")), feature, context);
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    state.setChoice(QStringLiteral("location"), feature.value(QStringLiteral("location")).toString(QStringLiteral("high_end")));
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added %1 thread %2 %3 on %4 face(s)")
        .arg(state.checked(QStringLiteral("modeled")) ? QStringLiteral("modeled") : QStringLiteral("cosmetic"),
             state.choice(QStringLiteral("designation")), state.choice(QStringLiteral("class")))
        .arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

} // namespace mitcad::cmd
