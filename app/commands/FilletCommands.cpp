// SPDX-License-Identifier: MIT
// Press Pull, Fillet and Chamfer (U4): fillets and chamfers with sets of
// edges, each set its own size (commands.md, "fillet",
// "chamfer"); Press Pull turns into the feature its selection asks for,
// and on a fillet's face opens the fillet (P6).
#include <algorithm>
#include <cmath>
#include <optional>

#include <QJsonArray>
#include <QKeySequence>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const QString kSets = QStringLiteral("sets");
const QString kMid = QStringLiteral("mid");

QString rowKey(int row, const char* id) { return CommandState::rowKey(kSets, row, QString::fromLatin1(id)); }
QString midKey(int row, const char* id) { return CommandState::rowKey(kMid, row, QString::fromLatin1(id)); }

// Where a fillet rounds an edge: the edge's middle, the way from it to the
// middle of the rounding (into the material at a convex edge, out of it at
// a concave one) and the angle between the faces' normals there; placed
// as the view shows the body.
struct EdgeBend {
  gp_Pnt middle;
  gp_Dir toward;
  double angle = 0.0;
};

std::optional<EdgeBend> edgeBend(const SelectionItem& item, const CommandContext& model) {
  if (item.kind != SelectKind::Edge) {
    return std::nullopt;
  }
  const auto body = model.bodyShape(item.owner);
  if (!body) {
    return std::nullopt;
  }
  const std::vector<int> found = body->find_edges(item.name.toStdString());
  if (found.empty()) {
    return std::nullopt;
  }
  const int index = found.front();
  const std::array<int, 2> faces = body->edge_faces(index);
  if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
    return std::nullopt;
  }
  const TopoDS_Edge& edge = body->edge(index);
  const BRepAdaptor_Curve curve(edge);
  const double t = (curve.FirstParameter() + curve.LastParameter()) / 2;
  const gp_Pnt middle = curve.Value(t);
  gp_Vec normals[2];
  for (int k = 0; k < 2; ++k) {
    const TopoDS_Face& face = body->face(faces[static_cast<std::size_t>(k)]);
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull()) {
      return std::nullopt;
    }
    const gp_Pnt2d uv = pcurve->Value(t);
    const BRepAdaptor_Surface surface(face);
    BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1.0e-9);
    if (!props.IsNormalDefined()) {
      return std::nullopt;
    }
    normals[k] = gp_Vec(props.Normal());
    if (face.Orientation() == TopAbs_REVERSED) {
      normals[k].Reverse();
    }
  }
  const gp_Vec sum = normals[0] + normals[1];
  if (sum.Magnitude() < 1.0e-9) {
    return std::nullopt;
  }
  const gp_Dir bisector(sum);
  // Just off a convex edge along the normals is outside the material.
  BRepClass3d_SolidClassifier inside(body->occt(), middle.Translated(gp_Vec(bisector) * 1.0e-3), 1.0e-7);
  const bool convex = inside.State() == TopAbs_OUT;
  const gp_Trsf placement = model.itemShape(item).Location().Transformation();
  return EdgeBend{middle.Transformed(placement),
                  (convex ? bisector.Reversed() : bisector).Transformed(placement),
                  normals[0].Angle(normals[1])};
}

// Every edge and face of the sets, for the body they must share.
Selection allItems(const CommandState& state) {
  Selection items;
  for (int i = 0; i < state.rows(kSets); ++i) {
    items += state.items(rowKey(i, "edges"));
  }
  return items;
}

// Edges and faces of a set of the model's definition.
Selection setItems(const QJsonObject& set, const QString& body) {
  Selection items;
  for (const QJsonValue& edge : set.value(QStringLiteral("edges")).toArray()) {
    items.append({SelectKind::Edge, body, edge.toString(), QString()});
  }
  for (const QJsonValue& face : set.value(QStringLiteral("faces")).toArray()) {
    items.append({SelectKind::Face, body, face.toString(), QString()});
  }
  return items;
}

void addEdges(QJsonObject& set, const Selection& items) {
  const QJsonArray edges = names(items, SelectKind::Edge);
  const QJsonArray faces = names(items, SelectKind::Face);
  if (!edges.isEmpty()) {
    set.insert(QStringLiteral("edges"), edges);
  }
  if (!faces.isEmpty()) {
    set.insert(QStringLiteral("faces"), faces);
  }
}

// The sets of a definition: its short form is one set.
QJsonArray setsOf(const QJsonObject& feature) {
  if (feature.contains(kSets)) {
    return feature.value(kSets).toArray();
  }
  QJsonObject set = feature;
  set.remove(QStringLiteral("type"));
  set.remove(QStringLiteral("body"));
  set.remove(QStringLiteral("uid"));
  return {set};
}

bool sameChain(const QJsonArray& sets) {
  bool first = true;
  bool chain = true;
  for (const QJsonValue& value : sets) {
    const bool on = value.toObject().value(QStringLiteral("tangent_chain")).toBool(true);
    if (!first && on != chain) {
      return false;
    }
    chain = on;
    first = false;
  }
  return true;
}

const Choices kFilletSizes = {{QStringLiteral("constant"), QStringLiteral("Constant")},
                              {QStringLiteral("chord_length"), QStringLiteral("Chord Length")},
                              {QStringLiteral("variable"), QStringLiteral("Variable")},
                              {QStringLiteral("asymmetric"), QStringLiteral("Asymmetric")}};

// Whether any set of the panel has a variable radius (mid radii are theirs).
bool anyVariable(const CommandState& state) {
  for (int i = 0; i < state.rows(kSets); ++i) {
    if (state.row(kSets, i).choice(QStringLiteral("size")) == QStringLiteral("variable")) {
      return true;
    }
  }
  return false;
}

const Choices kChamferTypes = {{QStringLiteral("equal_distance"), QStringLiteral("Equal Distance")},
                               {QStringLiteral("two_distances"), QStringLiteral("Two Distances")},
                               {QStringLiteral("distance_angle"), QStringLiteral("Distance and Angle")}};

} // namespace

// ---------------------------------------------------------------------------
// Fillet

CommandDef filletCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.fillet");
  def.name = QObject::tr("Fillet");
  def.icon = QStringLiteral("fillet");
  def.tooltip = QObject::tr("Rounds edges; a face rounds all its edges. Sets of edges can each "
                            "have their own radius");
  def.shortcut = QKeySequence(Qt::Key_F);
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("round"), QStringLiteral("blend")};
  def.featureType = QStringLiteral("fillet");
  def.describeBefore = true; // the edges are gone after it
  const auto sizeIs = [](const QString& size) {
    return [size](const CommandState& row) { return row.choice(QStringLiteral("size")) == size; };
  };
  def.inputs = {
      listInput(kSets, QObject::tr("Edge Sets"),
                {selectionInput(QStringLiteral("edges"), QObject::tr("Edges"),
                                SelectKind::Edge | SelectKind::Face, 1, 0)
                     .withTooltip(QObject::tr("Edges to round; a face rounds all its edges")),
                 choiceInput(QStringLiteral("size"), QObject::tr("Radius Type"), kFilletSizes,
                             QStringLiteral("constant")),
                 valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length,
                            QStringLiteral("2 mm"))
                     .withTooltip(QObject::tr("The radius; the chord length; the start radius of a "
                                              "variable one; or the first distance of an asymmetric "
                                              "one"))
                     .withManipulator([](const CommandState& row,
                                         const CommandContext& model) -> std::optional<Manipulator> {
                       // A handle at the middle of the rounding of the set's
                       // first edge, for a constant radius.
                       if (row.choice(QStringLiteral("size")) != QStringLiteral("constant")) {
                         return std::nullopt;
                       }
                       for (const SelectionItem& item : row.items(QStringLiteral("edges"))) {
                         if (const auto bend = edgeBend(item, model)) {
                           Manipulator handle = arrow(bend->middle, bend->toward);
                           handle.factor = 1.0 / std::cos(bend->angle / 2) - 1.0;
                           return handle;
                         }
                       }
                       return std::nullopt;
                     }),
                 valueInput(QStringLiteral("end"), QObject::tr("End Radius"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withVisible(sizeIs(QStringLiteral("variable"))),
                 valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withTooltip(QObject::tr("The distance from the edge on the other face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 selectionInput(QStringLiteral("start_vertex"), QObject::tr("Start Vertex"),
                                SelectKind::Vertex, 0, 1)
                     .withTooltip(QObject::tr("The end of the chain where the radius starts; "
                                              "none: the geometry kernel's start"))
                     .withVisible(sizeIs(QStringLiteral("variable"))),
                 selectionInput(QStringLiteral("reference"), QObject::tr("Reference Face"),
                                SelectKind::Face, 0, 1)
                     .withTooltip(QObject::tr("The face the first distance is measured on; none: "
                                              "each edge's first face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                     .withTooltip(QObject::tr("Measure the first distance on the other face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"),
                             {{QStringLiteral("tangent"), QStringLiteral("Tangent (G1)")},
                              {QStringLiteral("curvature"), QStringLiteral("Curvature (G2)")}},
                             QStringLiteral("tangent")),
                 valueInput(QStringLiteral("weight"), QObject::tr("Tangency Weight"), ValueKind::Unitless,
                            QStringLiteral("1"))
                     .withTooltip(QObject::tr("0.1 to 2: how far the curvature continuous rounding "
                                              "keeps to the faces before it turns"))
                     .withVisible([](const CommandState& row) {
                       return row.choice(QStringLiteral("continuity")) == QStringLiteral("curvature");
                     })},
                QObject::tr("Add Set"), QObject::tr("Set %1")),
      listInput(kMid, QObject::tr("Mid Radii"),
                {valueInput(QStringLiteral("set"), QObject::tr("Set"), ValueKind::Unitless, QStringLiteral("1"))
                     .withTooltip(QObject::tr("The variable set (1, 2, ...) the radius is on")),
                 valueInput(QStringLiteral("position"), QObject::tr("Position"), ValueKind::Unitless,
                            QStringLiteral("0.5"))
                     .withTooltip(QObject::tr("Where along the chain, 0 at the start to 1 at the end")),
                 valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length,
                            QStringLiteral("3 mm"))},
                QObject::tr("Add Mid Radius"), QObject::tr("Mid %1"), 0)
          .withVisible(anyVariable),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true)
          .withTooltip(QObject::tr("Also round the edges that continue the selected ones smoothly")),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner Type"),
                  {{QStringLiteral("rolling_ball"), QStringLiteral("Rolling Ball")},
                   {QStringLiteral("setback"), QStringLiteral("Setback")}},
                  QStringLiteral("rolling_ball")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = oneBody(allItems(state), result, rowKey(0, "edges"));
    if (body.isEmpty()) {
      return result;
    }
    const bool chain = state.checked(QStringLiteral("tangent_chain"));
    const bool rolling = state.choice(QStringLiteral("corner")) == QStringLiteral("rolling_ball");
    result.def = {{QStringLiteral("type"), QStringLiteral("fillet")}, {QStringLiteral("body"), body}};
    const int count = state.rows(kSets);
    const CommandState first = state.row(kSets, 0);
    if (count == 1 && chain && rolling && first.choice(QStringLiteral("size")) == QStringLiteral("constant") &&
        first.choice(QStringLiteral("continuity")) == QStringLiteral("tangent") &&
        names(first.items(QStringLiteral("edges")), SelectKind::Face).isEmpty()) {
      // The short form: one set of edges with a constant radius.
      result.def.insert(QStringLiteral("edges"), names(first.items(QStringLiteral("edges")), SelectKind::Edge));
      result.def.insert(QStringLiteral("radius"), first.expression(QStringLiteral("radius")));
      return result;
    }
    // The mid radii of each variable set, by position.
    std::vector<std::vector<std::pair<double, QJsonValue>>> mids(static_cast<std::size_t>(count));
    for (int k = 0; k < state.rows(kMid) && anyVariable(state); ++k) {
      const double set = state.value(midKey(k, "set"));
      const double position = state.value(midKey(k, "position"));
      const int index = static_cast<int>(std::lround(set)) - 1;
      if (!(std::abs(set - std::round(set)) < 1e-9) || index < 0 || index >= count ||
          state.row(kSets, index).choice(QStringLiteral("size")) != QStringLiteral("variable")) {
        return Built::failure(QObject::tr("Mid radius %1 is not on a variable set.").arg(k + 1), midKey(k, "set"));
      }
      if (!(position > 0.0 && position < 1.0)) {
        return Built::failure(QObject::tr("Mid radius %1: the position must be between 0 and 1.").arg(k + 1),
                              midKey(k, "position"));
      }
      mids[static_cast<std::size_t>(index)].emplace_back(position, state.expression(midKey(k, "radius")));
    }
    QJsonArray sets;
    for (int i = 0; i < count; ++i) {
      const CommandState row = state.row(kSets, i);
      const QString size = row.choice(QStringLiteral("size"));
      QJsonObject sizeDef{{QStringLiteral("type"), size}};
      QJsonObject set;
      if (size == QStringLiteral("chord_length")) {
        sizeDef.insert(QStringLiteral("length"), row.expression(QStringLiteral("radius")));
      } else if (size == QStringLiteral("variable")) {
        sizeDef.insert(QStringLiteral("start"), row.expression(QStringLiteral("radius")));
        sizeDef.insert(QStringLiteral("end"), row.expression(QStringLiteral("end")));
        const Selection& vertex = row.items(QStringLiteral("start_vertex"));
        if (!vertex.isEmpty()) {
          if (vertex.first().owner != body) {
            return Built::failure(QObject::tr("The start vertex must be on the filleted body."),
                                  rowKey(i, "start_vertex"));
          }
          sizeDef.insert(QStringLiteral("start_vertex"), vertex.first().name);
        }
        auto& mid = mids[static_cast<std::size_t>(i)];
        std::sort(mid.begin(), mid.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
        QJsonArray radii;
        for (const auto& [position, radius] : mid) {
          radii.append(QJsonObject{{QStringLiteral("position"), position}, {QStringLiteral("radius"), radius}});
        }
        if (!radii.isEmpty()) {
          sizeDef.insert(QStringLiteral("mid"), radii);
        }
      } else if (size == QStringLiteral("asymmetric")) {
        sizeDef.insert(QStringLiteral("distance1"), row.expression(QStringLiteral("radius")));
        sizeDef.insert(QStringLiteral("distance2"), row.expression(QStringLiteral("distance2")));
        if (row.checked(QStringLiteral("flip"))) {
          sizeDef.insert(QStringLiteral("flip"), true);
        }
        const Selection& reference = row.items(QStringLiteral("reference"));
        if (!reference.isEmpty()) {
          if (reference.first().owner != body) {
            return Built::failure(QObject::tr("The reference face must be on the filleted body."),
                                  rowKey(i, "reference"));
          }
          set.insert(QStringLiteral("reference_face"), reference.first().name);
        }
      } else {
        sizeDef.insert(QStringLiteral("radius"), row.expression(QStringLiteral("radius")));
      }
      set.insert(QStringLiteral("size"), sizeDef);
      addEdges(set, row.items(QStringLiteral("edges")));
      if (!chain) {
        set.insert(QStringLiteral("tangent_chain"), false);
      }
      if (row.choice(QStringLiteral("continuity")) == QStringLiteral("curvature")) {
        set.insert(QStringLiteral("continuity"), QStringLiteral("curvature"));
        set.insert(QStringLiteral("tangency_weight"), countValue(row, QStringLiteral("weight")));
      }
      sets.append(set);
    }
    result.def.insert(kSets, sets);
    if (!rolling) {
      result.def.insert(QStringLiteral("rolling_ball_corners"), false);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    const QJsonArray sets = setsOf(feature);
    state.setRows(kSets, static_cast<int>(sets.size()));
    int mids = 0;
    for (int i = 0; i < sets.size(); ++i) {
      const QJsonObject set = sets[i].toObject();
      state.setItems(rowKey(i, "edges"), setItems(set, body));
      if (!feature.contains(kSets)) {
        state.setChoice(rowKey(i, "size"), QStringLiteral("constant"));
        loadValue(state, rowKey(i, "radius"), feature.value(QStringLiteral("radius")), feature, context);
        continue;
      }
      const QJsonObject size = set.value(QStringLiteral("size")).toObject();
      const QString type = str(size, "type");
      state.setChoice(rowKey(i, "size"), type);
      loadValue(state, rowKey(i, "radius"),
                type == QStringLiteral("chord_length") ? size.value(QStringLiteral("length"))
                : type == QStringLiteral("variable")   ? size.value(QStringLiteral("start"))
                : type == QStringLiteral("asymmetric") ? size.value(QStringLiteral("distance1"))
                                                       : size.value(QStringLiteral("radius")),
                feature, context);
      loadValue(state, rowKey(i, "end"), size.value(QStringLiteral("end")), feature, context);
      loadValue(state, rowKey(i, "distance2"), size.value(QStringLiteral("distance2")), feature, context);
      state.setChecked(rowKey(i, "flip"), size.value(QStringLiteral("flip")).toBool());
      if (set.contains(QStringLiteral("reference_face"))) {
        state.setItems(rowKey(i, "reference"),
                       {itemOf(QJsonObject{{QStringLiteral("body"), body},
                                           {QStringLiteral("face"), set.value(QStringLiteral("reference_face"))}},
                               context)});
      }
      if (size.contains(QStringLiteral("start_vertex"))) {
        state.setItems(rowKey(i, "start_vertex"),
                       {{SelectKind::Vertex, body, str(size, "start_vertex"), QStringLiteral("point")}});
      }
      for (const QJsonValue& value : size.value(QStringLiteral("mid")).toArray()) {
        const QJsonObject mid = value.toObject();
        state.setRows(kMid, mids + 1);
        state.setText(midKey(mids, "set"), QString::number(i + 1));
        state.setText(midKey(mids, "position"), number(mid.value(QStringLiteral("position")).toDouble()));
        loadValue(state, midKey(mids, "radius"), mid.value(QStringLiteral("radius")), feature, context);
        ++mids;
      }
      state.setChoice(rowKey(i, "continuity"),
                      set.value(QStringLiteral("continuity")).toString(QStringLiteral("tangent")));
      if (set.contains(QStringLiteral("tangency_weight"))) {
        loadCount(state, rowKey(i, "weight"), set.value(QStringLiteral("tangency_weight")), feature, context);
      }
      state.setChecked(QStringLiteral("tangent_chain"), set.value(QStringLiteral("tangent_chain")).toBool(true));
    }
    state.setChoice(QStringLiteral("corner"),
                    feature.value(QStringLiteral("rolling_ball_corners")).toBool(true)
                        ? QStringLiteral("rolling_ball")
                        : QStringLiteral("setback"));
  };

  // The tangent chain is one setting for all sets in the panel.
  def.canEdit = [](const QJsonObject& feature) { return sameChain(setsOf(feature)); };

  def.describe = [](const CommandState& state, const CommandContext& context) {
    const int count = state.rows(kSets);
    return QStringLiteral("Added fillet on %1 edge(s)%2")
        .arg(context.edgeCount(allItems(state)))
        .arg(count > 1 ? QStringLiteral(" in %1 sets").arg(count) : QString());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Chamfer

CommandDef chamferCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.chamfer");
  def.name = QObject::tr("Chamfer");
  def.icon = QStringLiteral("chamfer");
  def.tooltip = QObject::tr("Bevels edges; a face bevels all its edges. Sets of edges can each "
                            "have their own size");
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("bevel")};
  def.featureType = QStringLiteral("chamfer");
  def.describeBefore = true;
  const auto typeIs = [](const char* type) {
    return [value = QString::fromLatin1(type)](const CommandState& row) {
      return row.choice(QStringLiteral("type")) == value;
    };
  };
  const auto notEqual = [](const CommandState& row) {
    return row.choice(QStringLiteral("type")) != QStringLiteral("equal_distance");
  };
  def.inputs = {
      listInput(kSets, QObject::tr("Edge Sets"),
                {selectionInput(QStringLiteral("edges"), QObject::tr("Edges"),
                                SelectKind::Edge | SelectKind::Face, 1, 0)
                     .withTooltip(QObject::tr("Edges to bevel; a face bevels all its edges")),
                 choiceInput(QStringLiteral("type"), QObject::tr("Chamfer Type"), kChamferTypes,
                             QStringLiteral("equal_distance")),
                 valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length,
                            QStringLiteral("1 mm")),
                 valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length,
                            QStringLiteral("1 mm"))
                     .withVisible(typeIs("two_distances")),
                 valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle,
                            QStringLiteral("45 deg"))
                     .withVisible(typeIs("distance_angle")),
                 selectionInput(QStringLiteral("reference"), QObject::tr("Reference Face"),
                                SelectKind::Face, 0, 1)
                     .withTooltip(QObject::tr("The face the first distance and the angle are "
                                              "measured on; none: each edge's first face"))
                     .withVisible(notEqual),
                 flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                     .withTooltip(QObject::tr("Measure the first distance on the other face"))
                     .withVisible(notEqual)},
                QObject::tr("Add Set"), QObject::tr("Set %1")),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner Type"),
                  {{QStringLiteral("chamfer"), QStringLiteral("Chamfer")},
                   {QStringLiteral("miter"), QStringLiteral("Miter")},
                   {QStringLiteral("blend"), QStringLiteral("Blend")}},
                  QStringLiteral("chamfer"))
          .withTooltip(QObject::tr("Where three bevelled edges meet: a small face of its own "
                                   "(Chamfer), the bevels running on until they meet (Miter), or a "
                                   "smooth patch (Blend)")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = oneBody(allItems(state), result, rowKey(0, "edges"));
    if (body.isEmpty()) {
      return result;
    }
    const bool chain = state.checked(QStringLiteral("tangent_chain"));
    const QString corner = state.choice(QStringLiteral("corner"));
    result.def = {{QStringLiteral("type"), QStringLiteral("chamfer")}, {QStringLiteral("body"), body}};
    const int count = state.rows(kSets);
    QJsonArray sets;
    for (int i = 0; i < count; ++i) {
      const CommandState row = state.row(kSets, i);
      const QString type = row.choice(QStringLiteral("type"));
      QJsonObject size{{QStringLiteral("type"), type}};
      if (type == QStringLiteral("two_distances")) {
        size.insert(QStringLiteral("distance1"), row.expression(QStringLiteral("distance")));
        size.insert(QStringLiteral("distance2"), row.expression(QStringLiteral("distance2")));
      } else {
        size.insert(QStringLiteral("distance"), row.expression(QStringLiteral("distance")));
        if (type == QStringLiteral("distance_angle")) {
          size.insert(QStringLiteral("angle"), row.expression(QStringLiteral("angle")));
        }
      }
      const bool equal = type == QStringLiteral("equal_distance");
      QJsonObject set{{QStringLiteral("size"), size}};
      addEdges(set, row.items(QStringLiteral("edges")));
      const Selection& reference = row.items(QStringLiteral("reference"));
      if (!equal && !reference.isEmpty()) {
        if (reference.first().owner != body) {
          return Built::failure(QObject::tr("The reference face must be on the chamfered body."),
                                rowKey(i, "reference"));
        }
        set.insert(QStringLiteral("reference_face"), reference.first().name);
      }
      if (!equal && row.checked(QStringLiteral("flip"))) {
        set.insert(QStringLiteral("flip"), true);
      }
      if (!chain) {
        set.insert(QStringLiteral("tangent_chain"), false);
      }
      sets.append(set);
    }
    const QJsonObject only = sets.first().toObject();
    if (count == 1 && chain && corner == QStringLiteral("chamfer") && !only.contains(QStringLiteral("faces")) &&
        !only.contains(QStringLiteral("reference_face"))) {
      // The short form: one set of edges.
      result.def.insert(QStringLiteral("edges"), only.value(QStringLiteral("edges")));
      result.def.insert(QStringLiteral("size"), only.value(QStringLiteral("size")));
      if (only.value(QStringLiteral("flip")).toBool()) {
        result.def.insert(QStringLiteral("flip"), true);
      }
      return result;
    }
    result.def.insert(kSets, sets);
    if (corner != QStringLiteral("chamfer")) {
      result.def.insert(QStringLiteral("corner"), corner);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    const QJsonArray sets = setsOf(feature);
    state.setRows(kSets, static_cast<int>(sets.size()));
    for (int i = 0; i < sets.size(); ++i) {
      const QJsonObject set = sets[i].toObject();
      state.setItems(rowKey(i, "edges"), setItems(set, body));
      const QJsonObject size = set.value(QStringLiteral("size")).toObject();
      state.setChoice(rowKey(i, "type"), str(size, "type"));
      loadValue(state, rowKey(i, "distance"),
                size.contains(QStringLiteral("distance1")) ? size.value(QStringLiteral("distance1"))
                                                          : size.value(QStringLiteral("distance")),
                feature, context);
      loadValue(state, rowKey(i, "distance2"), size.value(QStringLiteral("distance2")), feature, context);
      loadValue(state, rowKey(i, "angle"), size.value(QStringLiteral("angle")), feature, context);
      state.setChecked(rowKey(i, "flip"), set.value(QStringLiteral("flip")).toBool());
      if (set.contains(QStringLiteral("reference_face"))) {
        state.setItems(rowKey(i, "reference"),
                       {itemOf(QJsonObject{{QStringLiteral("body"), body},
                                           {QStringLiteral("face"), set.value(QStringLiteral("reference_face"))}},
                               context)});
      }
      state.setChecked(QStringLiteral("tangent_chain"), set.value(QStringLiteral("tangent_chain")).toBool(true));
    }
    state.setChoice(QStringLiteral("corner"), feature.value(QStringLiteral("corner")).toString(QStringLiteral("chamfer")));
  };

  def.canEdit = [](const QJsonObject& feature) { return sameChain(setsOf(feature)); };

  def.describe = [](const CommandState& state, const CommandContext& context) {
    const int count = state.rows(kSets);
    return QStringLiteral("Added chamfer (%1) on %2 edge(s)%3")
        .arg(label(kChamferTypes, state.choice(rowKey(0, "type"))))
        .arg(context.edgeCount(allItems(state)))
        .arg(count > 1 ? QStringLiteral(" in %1 sets").arg(count) : QString());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Press Pull

CommandDef pressPullCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.press_pull");
  def.name = QObject::tr("Press Pull");
  def.icon = QStringLiteral("press-pull");
  def.tooltip = QObject::tr("Pushes or pulls what is selected: faces are offset, edges rounded, "
                            "profiles extruded; a fillet's face opens the fillet");
  def.shortcut = QKeySequence(Qt::Key_Q);
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("push"), QStringLiteral("pull"), QStringLiteral("offset face")};
  def.inputs = {
      selectionInput(QStringLiteral("target"), QObject::tr("Faces, Edges or Profiles"),
                     SelectKind::Face | SelectKind::Edge | SelectKind::Profile, 1, 0)
          .withTooltip(QObject::tr("Faces to offset, edges to fillet or profiles to extrude, one "
                                   "kind at a time")),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length,
                 QStringLiteral("5 mm"))
          .withTooltip(QObject::tr("Out of the material for faces, the radius for edges, the "
                                   "height for profiles"))
          .withManipulator([](const CommandState& state,
                              const CommandContext& model) -> std::optional<Manipulator> {
            const Selection& items = state.items(QStringLiteral("target"));
            if (items.isEmpty()) {
              return std::nullopt;
            }
            if (items.first().kind == SelectKind::Profile) {
              const auto base = profileNormal(items, model);
              return base ? std::optional<Manipulator>(arrow(base->first, base->second)) : std::nullopt;
            }
            if (items.first().kind == SelectKind::Face) {
              const auto frame = frameOf(items.first(), model);
              const auto center = centerOf({items.first()}, model);
              if (frame && center) {
                return arrow(*center, frame->normal);
              }
            }
            return std::nullopt;
          }),
  };
  def.enabled = [&context] { return hasBodies(context) || hasProfiles(context); };

  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const Selection& items = state.items(QStringLiteral("target"));
    const SelectKind kind = items.first().kind;
    if (items.size() == 1 && kind == SelectKind::Face && items.first().name.contains(QStringLiteral(":fillet("))) {
      // A fillet's face: the fillet opens for its radius,
      // on the set that rounds the face's edge.
      const QString uid = items.first().creatingFeature();
      try {
        const QJsonObject feature = context.queryObject(
            {{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
        const QJsonObject fillet = feature.value(QStringLiteral("def")).toObject();
        if (str(fillet, "type") == QStringLiteral("fillet")) {
          const QString name = items.first().name;
          const int open = name.indexOf(QStringLiteral(":fillet(")) + 8;
          const QString edge = name.mid(open, name.lastIndexOf(QLatin1Char(')')) - open);
          const QJsonArray sets = setsOf(fillet);
          int set = 0;
          for (int i = 0; i < sets.size(); ++i) {
            if (sets[i].toObject().value(QStringLiteral("edges")).toArray().contains(edge)) {
              set = i;
            }
          }
          return Built::editing(uid, rowKey(set, "radius"));
        }
      } catch (const std::exception&) {
        // not a feature of this document: as any face
      }
    }
    for (const SelectionItem& item : items) {
      if (item.kind != kind) {
        return Built::failure(QObject::tr("Select faces, edges or profiles: one kind at a time."),
                              QStringLiteral("target"));
      }
    }
    const QString distance = state.expression(QStringLiteral("distance"));
    if (kind == SelectKind::Profile) {
      if (oneSketch(items, result, QStringLiteral("target")).isEmpty()) {
        return result;
      }
      // Join, which makes a new body where it touches nothing: Press Pull
      // picks the operation.
      result.def = {{QStringLiteral("type"), QStringLiteral("extrude")},
                    {QStringLiteral("profiles"), refsOf(items)},
                    {QStringLiteral("extent"), QJsonObject{{QStringLiteral("type"), QStringLiteral("distance")},
                                                           {QStringLiteral("distance"), distance}}},
                    {QStringLiteral("operation"), QStringLiteral("join")}};
      return result;
    }
    const QString body = oneBody(items, result, QStringLiteral("target"));
    if (body.isEmpty()) {
      return result;
    }
    if (kind == SelectKind::Face) {
      result.def = {{QStringLiteral("type"), QStringLiteral("offset_face")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("faces"), names(items, SelectKind::Face)},
                    {QStringLiteral("distance"), distance}};
    } else {
      result.def = {{QStringLiteral("type"), QStringLiteral("fillet")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("edges"), names(items, SelectKind::Edge)},
                    {QStringLiteral("radius"), distance}};
    }
    return result;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const Selection& items = state.items(QStringLiteral("target"));
    const QString what = items.first().kind == SelectKind::Face   ? QStringLiteral("offset face")
                         : items.first().kind == SelectKind::Edge ? QStringLiteral("fillet")
                                                                  : QStringLiteral("extrude");
    return QStringLiteral("Press Pull made %1, distance %2")
        .arg(what, number(state.value(QStringLiteral("distance"))));
  };
  return def;
}

} // namespace mitcad::cmd
