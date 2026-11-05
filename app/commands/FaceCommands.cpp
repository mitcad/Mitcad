// SPDX-License-Identifier: MIT
// Shell, Draft, Offset Face, Replace Face, Split Face, Split Body and
// Delete (U4; commands.md, "Face features (F2)").
#include <cmath>

#include <QJsonArray>

#include <TopExp_Explorer.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

Selection facesOf(const QJsonObject& feature, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
    items.append(itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                    {QStringLiteral("face"), face}},
                        context));
  }
  return items;
}

// An arrow out of the first face along its normal.
std::optional<Manipulator> faceArrow(const CommandState& state, const CommandContext& context, const QString& key) {
  const Selection& faces = state.items(key);
  if (faces.isEmpty()) {
    return std::nullopt;
  }
  const auto frame = frameOf(faces.first(), context);
  const auto center = centerOf({faces.first()}, context);
  if (!frame || !center) {
    return std::nullopt;
  }
  return arrow(*center, frame->normal);
}

} // namespace

// ---------------------------------------------------------------------------
// Shell

CommandDef shellCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.shell");
  def.name = QObject::tr("Shell");
  def.icon = QStringLiteral("shell");
  def.tooltip = QObject::tr("Hollows a body: the faces picked are removed, the rest become walls");
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("hollow"), QStringLiteral("thickness")};
  def.featureType = QStringLiteral("shell");
  const auto inside = [](const CommandState& s) { return s.choice(QStringLiteral("direction")) != QStringLiteral("outside"); };
  const auto outside = [](const CommandState& s) { return s.choice(QStringLiteral("direction")) != QStringLiteral("inside"); };
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces/Body"), SelectKind::Face | SelectKind::Body, 1, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Faces to remove, or a body to hollow closed"))
          .withHoverChain(SelectKind::Face, QStringLiteral("tangent_chain")),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                  {{QStringLiteral("inside"), QStringLiteral("Inside")},
                   {QStringLiteral("outside"), QStringLiteral("Outside")},
                   {QStringLiteral("both"), QStringLiteral("Both")}},
                  QStringLiteral("inside")),
      valueInput(QStringLiteral("inside"), QObject::tr("Inside Thickness"), ValueKind::Length, QStringLiteral("2 mm")).withStepMinimum(0.0)
          .withVisible(inside)
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            auto handle = faceArrow(state, model, QStringLiteral("faces"));
            if (handle) {
              handle->direction.Reverse(); // into the material
            }
            return handle;
          }),
      valueInput(QStringLiteral("outside"), QObject::tr("Outside Thickness"), ValueKind::Length, QStringLiteral("2 mm")).withStepMinimum(0.0)
          .withVisible(outside),
      choiceInput(QStringLiteral("shell_type"), QObject::tr("Shell Type"),
                  {{QStringLiteral("sharp"), QStringLiteral("Sharp")}, {QStringLiteral("rounded"), QStringLiteral("Rounded")}},
                  QStringLiteral("sharp")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("faces"));
    const QString body = oneBody(items, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("shell")}, {QStringLiteral("body"), body}};
    const QJsonArray faces = names(items, SelectKind::Face);
    if (!faces.isEmpty()) {
      result.def.insert(QStringLiteral("faces"), faces);
    }
    const QString direction = state.choice(QStringLiteral("direction"));
    if (direction != QStringLiteral("outside")) {
      result.def.insert(QStringLiteral("inside"), state.expression(QStringLiteral("inside")));
    }
    if (direction != QStringLiteral("inside")) {
      result.def.insert(QStringLiteral("outside"), state.expression(QStringLiteral("outside")));
    }
    if (!state.checked(QStringLiteral("tangent_chain"))) {
      result.def.insert(QStringLiteral("tangent_chain"), false);
    }
    if (state.choice(QStringLiteral("shell_type")) == QStringLiteral("rounded")) {
      result.def.insert(QStringLiteral("rounded"), true);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection items = facesOf(feature, context);
    if (items.isEmpty()) {
      items.append({SelectKind::Body, str(feature, "body"), QString(), QString()});
    }
    state.setItems(QStringLiteral("faces"), items);
    const bool in = feature.contains(QStringLiteral("inside"));
    const bool out = feature.contains(QStringLiteral("outside"));
    state.setChoice(QStringLiteral("direction"), in && out ? QStringLiteral("both")
                                                 : out     ? QStringLiteral("outside")
                                                           : QStringLiteral("inside"));
    loadValue(state, QStringLiteral("inside"), feature.value(QStringLiteral("inside")), feature, context);
    loadValue(state, QStringLiteral("outside"), feature.value(QStringLiteral("outside")), feature, context);
    state.setChecked(QStringLiteral("tangent_chain"), feature.value(QStringLiteral("tangent_chain")).toBool(true));
    state.setChoice(QStringLiteral("shell_type"), feature.value(QStringLiteral("rounded")).toBool()
                                                      ? QStringLiteral("rounded")
                                                      : QStringLiteral("sharp"));
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added shell removing %1 face(s)")
        .arg(names(state.items(QStringLiteral("faces")), SelectKind::Face).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Draft

CommandDef draftCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.draft");
  def.name = QObject::tr("Draft");
  def.icon = QStringLiteral("draft");
  def.tooltip = QObject::tr("Tilts faces about where they meet a fixed plane");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("taper"), QStringLiteral("mould"), QStringLiteral("mold")};
  def.featureType = QStringLiteral("draft");
  def.inputs = {
      selectionInput(QStringLiteral("plane"), QObject::tr("Plane"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("The fixed plane: its normal is the pull direction"))
          .withAccepts(isPlanar),
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withHoverChain(SelectKind::Face, QStringLiteral("tangent_chain")),
      choiceInput(QStringLiteral("draft_type"), QObject::tr("Draft Sides"),
                  {{QStringLiteral("one"), QStringLiteral("One Side")},
                   {QStringLiteral("two"), QStringLiteral("Two Sides")},
                   {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}},
                  QStringLiteral("one")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("5 deg")),
      valueInput(QStringLiteral("angle2"), QObject::tr("Angle 2"), ValueKind::Angle, QStringLiteral("5 deg"))
          .withVisible(choiceIs(QStringLiteral("draft_type"), QStringLiteral("two"))),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip Pull Direction")),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("draft")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(faces, SelectKind::Face)},
                  {QStringLiteral("plane"), state.items(QStringLiteral("plane")).first().reference()},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
    const QString type = state.choice(QStringLiteral("draft_type"));
    if (type != QStringLiteral("one")) {
      result.def.insert(QStringLiteral("symmetric"), true);
    }
    if (type == QStringLiteral("two")) {
      result.def.insert(QStringLiteral("angle2"), state.expression(QStringLiteral("angle2")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    if (!state.checked(QStringLiteral("tangent_chain"))) {
      result.def.insert(QStringLiteral("tangent_chain"), false);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
    state.setItems(QStringLiteral("plane"), {itemOf(feature.value(QStringLiteral("plane")), context)});
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    loadValue(state, QStringLiteral("angle2"), feature.value(QStringLiteral("angle2")), feature, context);
    state.setChoice(QStringLiteral("draft_type"), feature.contains(QStringLiteral("angle2"))
                                                      ? QStringLiteral("two")
                                                  : feature.value(QStringLiteral("symmetric")).toBool()
                                                      ? QStringLiteral("symmetric")
                                                      : QStringLiteral("one"));
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setChecked(QStringLiteral("tangent_chain"), feature.value(QStringLiteral("tangent_chain")).toBool(true));
  };

  // The plane's offset (the importer's) has no input.
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("plane")), context) && !feature.contains(QStringLiteral("offset"));
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added draft of %1 face(s), angle %2 deg")
        .arg(state.items(QStringLiteral("faces")).size())
        .arg(degrees(state.value(QStringLiteral("angle"))));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Offset Face

CommandDef offsetFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.offset_face");
  def.name = QObject::tr("Offset Face");
  def.icon = QStringLiteral("face-edit");
  def.tooltip = QObject::tr("Moves faces along their normals; the faces around follow");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("press pull"), QStringLiteral("move face"), QStringLiteral("push")};
  def.featureType = QStringLiteral("offset_face");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0).withoutMeshes(),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("Positive out of the material"))
          .withManipulator([](const CommandState& state, const CommandContext& model) {
            return faceArrow(state, model, QStringLiteral("faces"));
          }),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("offset_face")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(faces, SelectKind::Face)},
                  {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added offset face of %1 face(s), distance %2")
        .arg(state.items(QStringLiteral("faces")).size())
        .arg(number(state.value(QStringLiteral("distance"))));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Replace Face

// A body without solids: a surface body.
bool isSurfaceBody(const SelectionItem& item, const CommandContext& context) {
  if (item.kind != SelectKind::Body) {
    return false;
  }
  const std::shared_ptr<geometry::Shape> shape = context.bodyShape(item.owner);
  return shape && !TopExp_Explorer(shape->occt(), TopAbs_SOLID).More();
}

CommandDef replaceFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.replace_face");
  def.name = QObject::tr("Replace Face");
  def.icon = QStringLiteral("replace-face");
  def.tooltip = QObject::tr("Replaces faces with a plane, a face of another body or a surface body");
  def.group = QStringLiteral("MODIFY");
  def.featureType = QStringLiteral("replace_face");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Source Faces"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withHoverChain(SelectKind::Face, QStringLiteral("tangent_chain")),
      selectionInput(QStringLiteral("target"), QObject::tr("Target Faces"), kPlaneKinds | SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("A plane, a face of another body (also curved) or a surface body"))
          .withAccepts([&context](const SelectionItem& item) {
            return item.kind != SelectKind::Body || isSurfaceBody(item, context);
          }),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Moves a plane target along its normal"))
          .withVisible([](const CommandState& s) {
            const Selection& target = s.items(QStringLiteral("target"));
            return !target.isEmpty() && target.first().kind == SelectKind::Plane;
          }),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    const SelectionItem& target = state.items(QStringLiteral("target")).first();
    result.def = {{QStringLiteral("type"), QStringLiteral("replace_face")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(faces, SelectKind::Face)},
                  {QStringLiteral("target"), target.reference()}};
    if (target.kind == SelectKind::Plane && nonZero(state, QStringLiteral("offset"))) {
      result.def.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
    }
    if (!state.checked(QStringLiteral("tangent_chain"))) {
      result.def.insert(QStringLiteral("tangent_chain"), false);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
    state.setItems(QStringLiteral("target"), {itemOf(feature.value(QStringLiteral("target")), context)});
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    state.setChecked(QStringLiteral("tangent_chain"), feature.value(QStringLiteral("tangent_chain")).toBool(true));
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("target")), context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added replace face of %1 face(s)").arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Split Face and Split Body

CommandDef splitFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.split_face");
  def.name = QObject::tr("Split Face");
  def.icon = QStringLiteral("split-face");
  def.tooltip = QObject::tr("Splits faces along a plane, a face, a body or sketch curves");
  def.group = QStringLiteral("MODIFY");
  def.featureType = QStringLiteral("split_face");
  def.extendedToolPreview = true; // the splitting tool as it is extended (mitcad#712)
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces to Split"), SelectKind::Face, 1, 0).withoutMeshes(),
      toolInput(),
      checkInput(QStringLiteral("extend"), QObject::tr("Extend Splitting Tool"), true),
      toolOffsetInput(),
  };
  def.inputs += directionInputs();
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    QJsonObject def{{QStringLiteral("type"), QStringLiteral("split_face")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("faces"), names(faces, SelectKind::Face)}};
    if (!addTool(def, state, result)) {
      return result;
    }
    result.def = def;
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
    loadTool(feature, state, context);
  };
  def.canEdit = [&context](const QJsonObject& feature) { return toolEditable(feature, context); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added split face of %1 face(s)").arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

CommandDef splitBodyCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.split_body");
  def.name = QObject::tr("Split Body");
  def.icon = QStringLiteral("split");
  def.tooltip = QObject::tr("Cuts bodies in pieces with a plane, a face, a body or sketch curves");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("cut"), QStringLiteral("divide")};
  def.featureType = QStringLiteral("split_body");
  def.extendedToolPreview = true; // the splitting tool as it is extended (mitcad#712)
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies to Split"), SelectKind::Body, 1, 0).withoutMeshes(),
      toolInput(),
      checkInput(QStringLiteral("extend"), QObject::tr("Extend Splitting Tool"), true),
      toolOffsetInput(),
  };
  def.inputs += directionInputs();
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject def{{QStringLiteral("type"), QStringLiteral("split_body")},
                    {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))}};
    if (!addTool(def, state, result)) {
      return result;
    }
    result.def = def;
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    loadTool(feature, state, context);
  };
  def.canEdit = [&context](const QJsonObject& feature) { return toolEditable(feature, context); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added split body of %1 body(ies)").arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Delete (faces)

CommandDef deleteFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.delete_face");
  def.name = QObject::tr("Delete Face");
  def.icon = QStringLiteral("delete");
  def.tooltip = QObject::tr("Removes faces and heals the body: the faces around extend to close it");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("delete"), QStringLiteral("remove"), QStringLiteral("heal"), QStringLiteral("defeature")};
  def.featureType = QStringLiteral("delete_face");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0).withoutMeshes(),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("delete_face")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(faces, SelectKind::Face)}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added delete face of %1 face(s)").arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

} // namespace mitcad::cmd
