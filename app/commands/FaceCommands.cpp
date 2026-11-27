// SPDX-License-Identifier: MIT
// Shell, Draft, Offset Face, Replace Face, Split Face, Split Body and
// Delete (U4; commands.md, "Face features (F2)").
#include <algorithm>
#include <cmath>
#include <memory>
#include <vector>

#include <QJsonArray>
#include <QStringList>

#include <TopExp_Explorer.hxx>
#include <gp_Pnt.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "SplitToolLinks.hpp"

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

// Shell's faces with their own thickness (mitcad#2443): a list of rows, each
// faces and their Inside and Outside thickness as the shell's Direction has
// them.
const QString kOwnThickness = QStringLiteral("face_thicknesses");

QString ownKey(int row, const char* id) {
  return CommandState::rowKey(kOwnThickness, row, QString::fromLatin1(id));
}

bool shellInside(const CommandState& state) {
  return state.whole().choice(QStringLiteral("direction")) != QStringLiteral("outside");
}

bool shellOutside(const CommandState& state) {
  return state.whole().choice(QStringLiteral("direction")) != QStringLiteral("inside");
}

// An arrow out of the first face along its normal; on a curved face from a
// point on it (mitcad#2478).
std::optional<Manipulator> faceArrow(const CommandState& state, const CommandContext& context, const QString& key) {
  const Selection& faces = state.items(key);
  if (faces.isEmpty()) {
    return std::nullopt;
  }
  const auto at = faceNormal(faces.first(), context);
  if (!at) {
    return std::nullopt;
  }
  // Stepped by the faces' size (mitcad#1431).
  Manipulator handle = arrow(at->first, at->second);
  handle.scale = itemsScale(faces, context);
  return handle;
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
      // Faces whose wall has a thickness of its own (mitcad#2443).
      listInput(kOwnThickness, QObject::tr("Own Thickness"),
                {selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
                     .withoutMeshes()
                     .withTooltip(QObject::tr("Faces whose wall has this thickness")),
                 valueInput(QStringLiteral("inside"), QObject::tr("Inside Thickness"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withStepMinimum(0.0, true)
                     .withVisible(shellInside),
                 valueInput(QStringLiteral("outside"), QObject::tr("Outside Thickness"), ValueKind::Length,
                            QStringLiteral("4 mm"))
                     .withStepMinimum(0.0, true)
                     .withVisible(shellOutside)},
                QObject::tr("Add Faces"), QObject::tr("Faces %1"), 0)
          .withTooltip(QObject::tr("Faces whose wall is thicker or thinner than the shell's")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("faces"));
    const QString body = oneBody(items, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    // Each row's faces with its thickness on the sides the shell has.
    QJsonArray own;
    for (int i = 0; i < state.rows(kOwnThickness); ++i) {
      const CommandState row = state.row(kOwnThickness, i);
      const Selection& faces = row.items(QStringLiteral("faces"));
      for (const SelectionItem& face : faces) {
        if (face.owner != body) {
          return Built::failure(QObject::tr("The faces with their own thickness must be of the shelled body."),
                                ownKey(i, "faces"));
        }
      }
      QJsonObject entry{{QStringLiteral("faces"), names(faces, SelectKind::Face)}};
      if (shellInside(state)) {
        entry.insert(QStringLiteral("inside"), row.expression(QStringLiteral("inside")));
      }
      if (shellOutside(state)) {
        entry.insert(QStringLiteral("outside"), row.expression(QStringLiteral("outside")));
      }
      own.append(entry);
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
    if (!own.isEmpty()) {
      result.def.insert(kOwnThickness, own);
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
    // The faces with their own thickness; a side an entry leaves out shows
    // the shell's.
    const QJsonArray own = feature.value(kOwnThickness).toArray();
    state.setRows(kOwnThickness, static_cast<int>(own.size()));
    for (int i = 0; i < static_cast<int>(own.size()); ++i) {
      QJsonObject entry = own.at(i).toObject();
      entry.insert(QStringLiteral("body"), feature.value(QStringLiteral("body")));
      state.setItems(ownKey(i, "faces"), facesOf(entry, context));
      for (const char* side : {"inside", "outside"}) {
        const QString id = QString::fromLatin1(side);
        loadValue(state, ownKey(i, side), entry.contains(id) ? entry.value(id) : feature.value(id), feature, context);
      }
    }
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const auto removed = names(state.items(QStringLiteral("faces")), SelectKind::Face).size();
    qsizetype own = 0;
    for (int i = 0; i < state.rows(kOwnThickness); ++i) {
      own += names(state.row(kOwnThickness, i).items(QStringLiteral("faces")), SelectKind::Face).size();
    }
    if (own == 0) {
      return QStringLiteral("Added shell removing %1 face(s)").arg(removed);
    }
    return QStringLiteral("Added shell removing %1 face(s), %2 face(s) at their own thickness").arg(removed).arg(own);
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
  def.keywords = {QStringLiteral("press pull"), QStringLiteral("move face"), QStringLiteral("push")};
  def.featureType = QStringLiteral("offset_face");
  def.inputs = {
      // As Shell's, Draft's and Replace Face's (mitcad#2438): the hover
      // shows the faces a pick takes with Tangent Chain on.
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withHoverChain(SelectKind::Face, QStringLiteral("tangent_chain")),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("Positive out of the material"))
          .withManipulator([](const CommandState& state, const CommandContext& model) {
            return faceArrow(state, model, QStringLiteral("faces"));
          }),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true)
          .withTooltip(QObject::tr("Also move the faces that continue the selected ones smoothly")),
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
    // Left out when off: the model's default, the faces alone.
    if (state.checked(QStringLiteral("tangent_chain"))) {
      result.def.insert(QStringLiteral("tangent_chain"), true);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), facesOf(feature, context));
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    // An offset stored without it (every one before mitcad#2438) moves its
    // faces alone and opens with Tangent Chain off.
    state.setChecked(QStringLiteral("tangent_chain"), feature.value(QStringLiteral("tangent_chain")).toBool(false));
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

namespace {

// Split Body's Keep (mitcad#2504): All Pieces, or Selected: the pieces
// whose dots are clicked in the view (their numbers in Pieces, "1, 3"),
// the others removed by the split. The dots come from the preview's
// `piece_choices` (every piece before keeping, with a point inside it and
// what keeps it: its side of a single plane, else that point); the last
// ones stay known while no preview is shown, so that they show once Keep
// is Selected and nothing is picked yet. The definition's `keep` is what
// keeps each piece picked; an edited split keeps its own `keep` until a
// dot is clicked (`loaded_keep`: it and the numbers of the pieces its
// first preview kept).
const QString kKeep = QStringLiteral("keep");
const QString kPieces = QStringLiteral("pieces");
const QString kLoadedKeep = QStringLiteral("loaded_keep");

struct PieceChoices {
  QJsonArray list; // the last preview's `piece_choices`
};

QList<int> numbersIn(const QString& text) {
  QList<int> numbers;
  for (const QString& part : text.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    bool ok = false;
    const int number = part.trimmed().toInt(&ok);
    if (ok && number > 0 && !numbers.contains(number)) {
      numbers.append(number);
    }
  }
  std::sort(numbers.begin(), numbers.end());
  return numbers;
}

QString textOf(const QList<int>& numbers) {
  QStringList texts;
  for (const int number : numbers) {
    texts << QString::number(number);
  }
  return texts.join(QStringLiteral(", "));
}

bool keepsSelected(const CommandState& state) {
  return state.choice(kKeep) == QStringLiteral("selected");
}

// Other bodies or another tool make other pieces: Keep goes back to All
// Pieces, whose preview shows the new ones.
void forgetPieces(CommandState& state, PieceChoices& choices) {
  if (keepsSelected(state)) {
    state.setChoice(kKeep, QStringLiteral("all"));
  }
  state.setText(kPieces, QString());
  state.setObject(kLoadedKeep, QJsonObject());
  choices.list = QJsonArray();
}

// The definition's `keep` of the inputs; empty for All Pieces or while no
// piece is picked.
QJsonArray keepOf(const CommandState& state, const PieceChoices& choices) {
  if (!keepsSelected(state)) {
    return QJsonArray();
  }
  const QList<int> numbers = numbersIn(state.text(kPieces));
  const QJsonObject loaded = state.object(kLoadedKeep);
  const QJsonArray own = loaded.value(QStringLiteral("keep")).toArray();
  if (!own.isEmpty() &&
      (!loaded.contains(QStringLiteral("numbers")) ||
       numbersIn(loaded.value(QStringLiteral("numbers")).toString()) == numbers)) {
    return own; // the edited split's own, no dot clicked since
  }
  QJsonArray keep;
  for (const int number : numbers) {
    if (number <= choices.list.size()) {
      keep.append(choices.list.at(number - 1).toObject().value(QStringLiteral("keep")));
    }
  }
  return keep;
}

} // namespace

CommandDef splitBodyCommand(const CommandContext& context) {
  const auto choices = std::make_shared<PieceChoices>();
  InputDef pieces =
      textInput(kPieces, QObject::tr("Pieces"), QString())
          .withTooltip(QObject::tr("The pieces to keep, by number (1, 3): click their dots in the view"))
          .withVisible(keepsSelected)
          .withToggles([choices](const CommandState& state, const CommandContext&, const QJsonObject& report) {
            std::vector<Manipulator> toggles;
            const QJsonArray list = report.contains(QStringLiteral("piece_choices"))
                                        ? report.value(QStringLiteral("piece_choices")).toArray()
                                        : choices->list;
            const QList<int> on = numbersIn(state.text(kPieces));
            for (int i = 0; i < list.size(); ++i) {
              const QJsonArray xyz = list.at(i).toObject().value(QStringLiteral("point")).toArray();
              if (xyz.size() != 3) {
                continue;
              }
              Manipulator toggle;
              toggle.kind = Manipulator::Kind::Toggle;
              toggle.origin = gp_Pnt(xyz[0].toDouble(), xyz[1].toDouble(), xyz[2].toDouble());
              toggle.element = i + 1;
              toggle.on = on.contains(i + 1);
              toggles.push_back(toggle);
            }
            return toggles;
          });
  pieces.togglesUnpreviewed = true; // the dots also while no piece is picked
  const auto forget = [choices](CommandState& state, const CommandContext&) { forgetPieces(state, *choices); };
  CommandDef def;
  def.id = QStringLiteral("solid.split_body");
  def.name = QObject::tr("Split Body");
  def.icon = QStringLiteral("split");
  def.tooltip = QObject::tr("Cuts bodies in pieces with a plane, a face, a body or sketch curves");
  def.keywords = {QStringLiteral("cut"), QStringLiteral("divide")};
  def.featureType = QStringLiteral("split_body");
  def.extendedToolPreview = true; // the splitting tool as it is extended (mitcad#712)
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies to Split"), SelectKind::Body, 1, 0)
          .withoutMeshes()
          .withOnChange(forget),
      toolInput().withOnChange(forget),
      checkInput(QStringLiteral("extend"), QObject::tr("Extend Splitting Tool"), true),
      toolOffsetInput(),
  };
  def.inputs += directionInputs();
  def.inputs += {
      choiceInput(kKeep, QObject::tr("Keep"),
                  {{QStringLiteral("all"), QStringLiteral("All Pieces")},
                   {QStringLiteral("selected"), QStringLiteral("Selected")}},
                  QStringLiteral("all"))
          .withTooltip(QObject::tr("Every piece, or only the pieces whose dots are clicked in the view"))
          .withOnChange([](CommandState& state, const CommandContext&) {
            // Picked anew: no piece is picked yet.
            state.setText(kPieces, QString());
            state.setObject(kLoadedKeep, QJsonObject());
          }),
      pieces,
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [choices](const CommandState& state, const CommandContext& context) {
    Built result;
    const Selection& bodies = state.items(QStringLiteral("bodies"));
    QJsonObject def{{QStringLiteral("type"), QStringLiteral("split_body")},
                    {QStringLiteral("bodies"), bodyUids(bodies)}};
    if (!addTool(def, state, result)) {
      return result;
    }
    if (keepsSelected(state)) {
      const QJsonArray keep = keepOf(state, *choices);
      if (keep.isEmpty()) {
        return Built::asking(QObject::tr("Click the dots of the pieces to keep in the view."), kPieces);
      }
      def.insert(QStringLiteral("keep"), keep);
    }
    // A face or body of another component as the tool, read where it was
    // picked (mitcad#2299): the split stays in the bodies' component.
    const QJsonObject links =
        splitToolLinks(bodies, state.items(QStringLiteral("tool")),
                       [&context](const QString& path) { return occurrenceComponent(path, context); });
    if (!links.isEmpty()) {
      def.insert(QStringLiteral("tool_links"), links);
    }
    result.def = def;
    return result;
  };
  def.load = [choices](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection bodies = bodiesOf(feature.value(QStringLiteral("bodies")).toArray());
    loadTool(feature, state, context);
    // A linked tool where its link saw it, the bodies where the link saw
    // them (mitcad#2299; the .f3d import's splits too).
    linkSplitPicks(feature.value(QStringLiteral("tool_links")).toObject(), bodies,
                   state.items(QStringLiteral("tool")));
    state.setItems(QStringLiteral("bodies"), bodies);
    // Its own `keep` until a dot is clicked; Pieces numbers the pieces its
    // first preview keeps (previewReported).
    const QJsonArray keep = feature.value(QStringLiteral("keep")).toArray();
    choices->list = QJsonArray();
    state.setChoice(kKeep, keep.isEmpty() ? QStringLiteral("all") : QStringLiteral("selected"));
    state.setText(kPieces, QString());
    state.setObject(kLoadedKeep, keep.isEmpty() ? QJsonObject() : QJsonObject{{QStringLiteral("keep"), keep}});
  };
  // The pieces the preview found, for the dots; an edited split's pieces
  // its own `keep` keeps into Pieces, once.
  def.previewReported = [choices](QJsonObject& report, CommandState& state) {
    if (!report.contains(QStringLiteral("piece_choices"))) {
      return false;
    }
    choices->list = report.value(QStringLiteral("piece_choices")).toArray();
    QJsonObject loaded = state.object(kLoadedKeep);
    if (!keepsSelected(state) || loaded.value(QStringLiteral("keep")).toArray().isEmpty() ||
        loaded.contains(QStringLiteral("numbers"))) {
      return false;
    }
    QList<int> numbers;
    for (int i = 0; i < choices->list.size(); ++i) {
      if (choices->list.at(i).toObject().value(QStringLiteral("kept")).toBool()) {
        numbers.append(i + 1);
      }
    }
    loaded.insert(QStringLiteral("numbers"), textOf(numbers));
    state.setObject(kLoadedKeep, loaded);
    if (state.text(kPieces) == textOf(numbers)) {
      return false;
    }
    state.setText(kPieces, textOf(numbers));
    return true; // the same definition again, the panel showing the numbers
  };
  def.ended = [choices](bool) { choices->list = QJsonArray(); };
  def.canEdit = [&context](const QJsonObject& feature) { return toolEditable(feature, context); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (keepsSelected(state)) {
      return QStringLiteral("Added split body of %1 body(ies) keeping pieces %2")
          .arg(state.items(QStringLiteral("bodies")).size())
          .arg(state.text(kPieces));
    }
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
