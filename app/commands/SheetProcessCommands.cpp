// SPDX-License-Identifier: MIT
// Sheet parts by material and process (mitcad#1939; commands.md "Sheet
// process profiles", app/COMMANDS.md "Sheet process profiles"): the process
// profile's command filter and the commands of laser sheet and board.
#include "SheetProcess.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>

#include <QJsonArray>
#include <QJsonObject>

#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "Commands.hpp"

namespace mitcad::cmd {

bool processAllows(const CommandContext& context, const QString& command, QString* reason) {
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("sheet_process")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("commands")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (entry.value(QStringLiteral("command")).toString() != command) {
        continue;
      }
      if (entry.value(QStringLiteral("available")).toBool(true)) {
        return true;
      }
      if (reason != nullptr) {
        *reason = entry.value(QStringLiteral("reason")).toString();
      }
      return false;
    }
  } catch (const std::exception&) {
  }
  return true;
}

QString processReason(const CommandContext& context, const QString& command) {
  QString reason;
  return processAllows(context, command, &reason) ? QString() : reason;
}

} // namespace mitcad::cmd

namespace mitcad::cmd {
namespace {

// Whether the active component has two bodies or more to join.
bool hasTwoBodies(const CommandContext& context) {
  try {
    return context.queryArray(QStringLiteral("bodies")).size() > 1;
  } catch (const std::exception&) {
    return false;
  }
}

bool hasBody(const CommandContext& context) {
  try {
    return !context.queryArray(QStringLiteral("bodies")).isEmpty();
  } catch (const std::exception&) {
    return false;
  }
}

void filtered(CommandDef& def, const CommandContext& context, std::function<bool()> also) {
  const QString id = def.id;
  def.enabled = [&context, id, also] { return (!also || also()) && processAllows(context, id); };
  def.disabledReason = [&context, id] { return processReason(context, id); };
}

QJsonValue expr(const CommandState& state, const char* id) { return state.expression(QString::fromLatin1(id)); }

// A hidden text the panel's `load` sets: the uid of the feature a model
// command's panel edits (Flat Pack, Standard Box).
InputDef editedInput() {
  return textInput(QStringLiteral("edited"), QString()).withVisible([](const CommandState&) { return false; });
}

// Standard Box's styles (core/model-api/src/sheet_process.rs, BoxStyle): a
// folder (FEFCO 04) has locking tabs and a lid with a tuck or a front flap;
// the reverse tuck end carton has tucks too (mitcad#2269).
bool folderStyle(const QString& style) { return style.startsWith(QStringLiteral("fefco_04")); }

bool tuckStyle(const QString& style) { return folderStyle(style) || style == QStringLiteral("ecma_a20"); }

// A folder's locking tabs on each turned over wall when Tabs is empty, as
// the model takes them (BoxStyle::tabs).
int styleTabs(const QString& style) {
  if (style == QStringLiteral("fefco_0426")) {
    return 3;
  }
  if (style == QStringLiteral("fefco_0470") || style == QStringLiteral("fefco_0471")) {
    return 1;
  }
  return 2;
}

// Whether an Auto value field (InputDef::autoText) is left empty: the
// model's default then.
bool leftEmpty(const CommandState& state, const QString& key) {
  return state.text(key).trimmed().isEmpty() || !std::isfinite(state.value(key));
}

} // namespace

// ---------------------------------------------------------------------------
// Finger Joint

CommandDef fingerJointCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.finger_joint");
  def.name = QObject::tr("Finger Joint");
  def.icon = QStringLiteral("mortise-tenon");
  def.tooltip = QObject::tr("Alternating fingers along the edges where laser-cut plates meet");
  def.keywords = {QStringLiteral("box joint"), QStringLiteral("laser"), QStringLiteral("plywood"),
                  QStringLiteral("joinery"), QStringLiteral("tab")};
  def.featureType = QStringLiteral("finger_joint");
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Plates"), SelectKind::Body, 2, 0)
          .withTooltip(QObject::tr("Two plates, or more: every pair that meets is joined"))
          .withoutMeshes(),
      valueInput(QStringLiteral("finger_width"), QObject::tr("Finger Width"), ValueKind::Length, QStringLiteral("15 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("About 2 to 3 times the sheet's thickness")),
      valueInput(QStringLiteral("count"), QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("0"))
          .withStepMinimum(0)
          .withTooltip(QObject::tr("Fingers along each edge; 0: by the finger width")),
      valueInput(QStringLiteral("clearance"), QObject::tr("Clearance"), ValueKind::Length, QStringLiteral("0.1 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("How much wider each slot is on each side of its finger")),
      valueInput(QStringLiteral("end_margin"), QObject::tr("End Margin"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("Kept whole at each end of an edge")),
      checkInput(QStringLiteral("flip"), QObject::tr("Flip"), false)
          .withTooltip(QObject::tr("The other plate takes the fingers at the ends")),
  };
  filtered(def, context, [&context] { return hasTwoBodies(context); });
  def.build = [](const CommandState& state, const CommandContext&) {
    QJsonObject d{{QStringLiteral("type"), QStringLiteral("finger_joint")},
                  {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                  {QStringLiteral("finger_width"), expr(state, "finger_width")},
                  {QStringLiteral("clearance"), expr(state, "clearance")}};
    if (state.expression(QStringLiteral("count")).trimmed() != QStringLiteral("0")) {
      d.insert(QStringLiteral("count"), expr(state, "count"));
    }
    if (nonZero(state, QStringLiteral("end_margin"))) {
      d.insert(QStringLiteral("end_margin"), expr(state, "end_margin"));
    }
    if (state.checked(QStringLiteral("flip"))) {
      d.insert(QStringLiteral("flip"), true);
    }
    return Built::of(d);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    for (const char* key : {"finger_width", "clearance", "end_margin"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadCount(state, QStringLiteral("count"), feature.value(QStringLiteral("count")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added finger joint of %1 plates").arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// T-Nut Slot

CommandDef tNutSlotCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.t_nut_slot");
  def.name = QObject::tr("T-Nut Slot");
  def.icon = QStringLiteral("mortise-tenon");
  def.tooltip = QObject::tr("A screw slot with a nut pocket in a plate's end, the screw's hole in its mate");
  def.keywords = {QStringLiteral("t-slot"), QStringLiteral("screw"), QStringLiteral("nut"), QStringLiteral("laser"),
                  QStringLiteral("joinery")};
  def.featureType = QStringLiteral("t_nut_slot");
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("End Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The plate's planar end face the slot starts from"))
          .withAccepts(isPlanar)
          .withoutMeshes(),
      selectionInput(QStringLiteral("mating_body"), QObject::tr("Mating Plate"), SelectKind::Body, 0, 1)
          .withTooltip(QObject::tr("The plate that gets the screw's hole (optional)"))
          .withoutMeshes(),
      valueInput(QStringLiteral("screw_diameter"), QObject::tr("Screw Diameter"), ValueKind::Length, QStringLiteral("3 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("slot_length"), QObject::tr("Slot Length"), ValueKind::Length, QStringLiteral("12 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("nut_width"), QObject::tr("Nut Width"), ValueKind::Length, QStringLiteral("5.5 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("Across the nut's flats")),
      valueInput(QStringLiteral("nut_thickness"), QObject::tr("Nut Thickness"), ValueKind::Length, QStringLiteral("2.4 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("nut_distance"), QObject::tr("Nut Distance"), ValueKind::Length, QStringLiteral("6 mm"))
          .withStepMinimum(0.0)
          .withTooltip(QObject::tr("From the end face to the nut's pocket")),
      valueInput(QStringLiteral("clearance"), QObject::tr("Clearance"), ValueKind::Length, QStringLiteral("0.1 mm"))
          .withStepMinimum(0.0),
      valueInput(QStringLiteral("count"), QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("1")).withStepMinimum(1),
  };
  filtered(def, context, [&context] { return hasBody(context); });
  def.build = [](const CommandState& state, const CommandContext&) {
    const SelectionItem& face = state.items(QStringLiteral("face")).first();
    const QJsonObject ref = face.reference().toObject();
    QJsonObject d{{QStringLiteral("type"), QStringLiteral("t_nut_slot")},
                  {QStringLiteral("body"), ref.value(QStringLiteral("body"))},
                  {QStringLiteral("face"), ref.value(QStringLiteral("face"))}};
    const Selection& mating = state.items(QStringLiteral("mating_body"));
    if (!mating.isEmpty()) {
      if (mating.first().owner == face.owner) {
        return Built::failure(QObject::tr("The screw's hole goes into another plate than the slot's."),
                              QStringLiteral("mating_body"));
      }
      d.insert(QStringLiteral("mating_body"), mating.first().owner);
    }
    for (const char* key : {"screw_diameter", "slot_length", "nut_width", "nut_thickness", "nut_distance", "clearance"}) {
      d.insert(QString::fromLatin1(key), expr(state, key));
    }
    if (state.expression(QStringLiteral("count")).trimmed() != QStringLiteral("1")) {
      d.insert(QStringLiteral("count"), expr(state, "count"));
    }
    return Built::of(d);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("face"),
                   {itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                       {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                           context)});
    if (feature.contains(QStringLiteral("mating_body"))) {
      state.setItems(QStringLiteral("mating_body"), bodiesOf({feature.value(QStringLiteral("mating_body"))}));
    }
    for (const char* key : {"screw_diameter", "slot_length", "nut_width", "nut_thickness", "nut_distance", "clearance"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadCount(state, QStringLiteral("count"), feature.value(QStringLiteral("count")), feature, context);
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                  {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                      context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added T-nut slot%1")
        .arg(state.items(QStringLiteral("mating_body")).isEmpty() ? QString() : QStringLiteral(" with the screw's hole"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Kerf Bend: Fold with the slit lattice of a living hinge as its own
// allowance.

CommandDef kerfBendCommand(const CommandContext& context) {
  CommandDef def = sheetFoldCommand(context);
  def.id = QStringLiteral("sheet.kerf_bend");
  def.name = QObject::tr("Kerf Bend");
  def.tooltip = QObject::tr("A living hinge: a plate bent along a sketch line, slit in its flat pattern");
  def.keywords = {QStringLiteral("living hinge"), QStringLiteral("lattice"), QStringLiteral("laser"),
                  QStringLiteral("flex")};
  def.inputs << valueInput(QStringLiteral("slit_length"), QObject::tr("Slit Length"), ValueKind::Length,
                           QStringLiteral("20 mm"))
                    .withStepMinimum(0.0, true)
                    .withHeading(QObject::tr("Slit Lattice"))
             << valueInput(QStringLiteral("slit_gap"), QObject::tr("Slit Gap"), ValueKind::Length, QStringLiteral("3 mm"))
                    .withStepMinimum(0.0, true)
             << valueInput(QStringLiteral("slit_pitch"), QObject::tr("Row Pitch"), ValueKind::Length,
                           QStringLiteral("1.5 mm"))
                    .withStepMinimum(0.0, true)
             << valueInput(QStringLiteral("slit_width"), QObject::tr("Slit Width"), ValueKind::Length,
                           QStringLiteral("0.2 mm"))
                    .withStepMinimum(0.0, true)
                    .withTooltip(QObject::tr("The cut's width: the laser's kerf"));
  const auto fold = def.build;
  def.build = [fold](const CommandState& state, const CommandContext& model) {
    Built result = fold(state, model);
    if (!result.def.isEmpty()) {
      result.def.insert(QStringLiteral("allowance"),
                        QJsonObject{{QStringLiteral("type"), QStringLiteral("kerf_bend")},
                                    {QStringLiteral("slit_length"), expr(state, "slit_length")},
                                    {QStringLiteral("gap"), expr(state, "slit_gap")},
                                    {QStringLiteral("pitch"), expr(state, "slit_pitch")},
                                    {QStringLiteral("slit_width"), expr(state, "slit_width")}});
    }
    return result;
  };
  const auto load = def.load;
  def.load = [load](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    load(feature, state, context);
    const QJsonObject lattice = feature.value(QStringLiteral("allowance")).toObject();
    loadValue(state, QStringLiteral("slit_length"), lattice.value(QStringLiteral("slit_length")), feature, context);
    loadValue(state, QStringLiteral("slit_gap"), lattice.value(QStringLiteral("gap")), feature, context);
    loadValue(state, QStringLiteral("slit_pitch"), lattice.value(QStringLiteral("pitch")), feature, context);
    loadValue(state, QStringLiteral("slit_width"), lattice.value(QStringLiteral("slit_width")), feature, context);
    // The lattice is the panel's: not kept as the fold's own allowance.
    state.setObject(QStringLiteral("own_allowance"), QJsonObject());
  };
  def.canEdit = [](const QJsonObject& feature) {
    return feature.value(QStringLiteral("allowance")).toObject().value(QStringLiteral("type")).toString() ==
           QStringLiteral("kerf_bend");
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added kerf bend, angle %1, pitch %2")
        .arg(state.text(QStringLiteral("angle")), state.text(QStringLiteral("slit_pitch")));
  };
  filtered(def, context, {});
  return def;
}

// ---------------------------------------------------------------------------
// Flat Pack: a box solid as six jointed plates on a cut sheet.

CommandDef flatPackCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.flat_pack");
  def.name = QObject::tr("Flat Pack");
  def.icon = QStringLiteral("cut-sheet");
  def.tooltip = QObject::tr("A box solid as six finger-jointed plates, laid out on a cut sheet");
  def.keywords = {QStringLiteral("box"), QStringLiteral("laser"), QStringLiteral("plywood"),
                  QStringLiteral("finger joint"), QStringLiteral("box maker")};
  def.modelCommand = true;
  // A Flat Pack in the timeline opens here: OK edits it (the cut sheet
  // stays as it is).
  def.featureType = QStringLiteral("flat_pack");
  const auto adding = [](const CommandState& s) { return s.text(QStringLiteral("edited")).isEmpty(); };
  def.inputs = {
      editedInput(),
      selectionInput(QStringLiteral("body"), QObject::tr("Box"), SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("A rectangular block along the component's axes"))
          .withoutMeshes(),
      valueInput(QStringLiteral("thickness"), QObject::tr("Sheet Thickness"), ValueKind::Length, QStringLiteral("6 mm"))
          .withStepMinimum(0.0, true),
      checkInput(QStringLiteral("inside"), QObject::tr("Inside Size"), false)
          .withTooltip(QObject::tr("The box is the room inside the plates")),
      valueInput(QStringLiteral("finger_width"), QObject::tr("Finger Width"), ValueKind::Length, QStringLiteral("15 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("clearance"), QObject::tr("Clearance"), ValueKind::Length, QStringLiteral("0.1 mm"))
          .withStepMinimum(0.0),
      valueInput(QStringLiteral("sheet_width"), QObject::tr("Cut Sheet Width"), ValueKind::Length,
                 QStringLiteral("1000 mm"))
          .withStepMinimum(0.0, true)
          .withHeading(QObject::tr("Cut Sheet"))
          .withVisible(adding),
      valueInput(QStringLiteral("sheet_height"), QObject::tr("Cut Sheet Height"), ValueKind::Length,
                 QStringLiteral("600 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(adding),
  };
  filtered(def, context, [&context] { return hasBody(context); });
  def.build = [](const CommandState& state, const CommandContext&) {
    const QString edited = state.text(QStringLiteral("edited"));
    if (!edited.isEmpty()) {
      QJsonObject d{{QStringLiteral("type"), QStringLiteral("flat_pack")},
                    {QStringLiteral("body"), state.items(QStringLiteral("body")).first().owner},
                    {QStringLiteral("thickness"), expr(state, "thickness")},
                    {QStringLiteral("finger_width"), expr(state, "finger_width")},
                    {QStringLiteral("clearance"), expr(state, "clearance")}};
      if (state.checked(QStringLiteral("inside"))) {
        d.insert(QStringLiteral("inside"), true);
      }
      return Built::of({{QStringLiteral("commands"),
                         QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
                                                {QStringLiteral("uid"), edited},
                                                {QStringLiteral("def"), d}}}},
                        {QStringLiteral("label"), QStringLiteral("Edit Flat Pack")}});
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_flat_pack")},
                        {QStringLiteral("body"), state.items(QStringLiteral("body")).first().owner},
                        {QStringLiteral("thickness"), expr(state, "thickness")},
                        {QStringLiteral("finger_width"), expr(state, "finger_width")},
                        {QStringLiteral("clearance"), expr(state, "clearance")},
                        {QStringLiteral("width"), state.value(QStringLiteral("sheet_width"))},
                        {QStringLiteral("height"), state.value(QStringLiteral("sheet_height"))}};
    if (state.checked(QStringLiteral("inside"))) {
      command.insert(QStringLiteral("inside"), true);
    }
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}}, {QStringLiteral("label"), QStringLiteral("Flat Pack")}});
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setText(QStringLiteral("edited"), feature.value(QStringLiteral("uid")).toString());
    state.setItems(QStringLiteral("body"), bodiesOf({feature.value(QStringLiteral("body"))}));
    for (const char* key : {"thickness", "finger_width", "clearance"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    state.setChecked(QStringLiteral("inside"), feature.value(QStringLiteral("inside")).toBool());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (!state.text(QStringLiteral("edited")).isEmpty()) {
      return QStringLiteral("Edited flat pack: plates of %1").arg(state.text(QStringLiteral("thickness")));
    }
    return QStringLiteral("Flat pack: six plates of %1 on a cut sheet").arg(state.text(QStringLiteral("thickness")));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Glue Flap: Flange's panel for a board box's glue flap.

CommandDef glueFlapCommand(const CommandContext& context) {
  CommandDef def = sheetFlangeCommand(context);
  def.id = QStringLiteral("sheet.glue_flap");
  def.name = QObject::tr("Glue Flap");
  def.tooltip = QObject::tr("A tapered flap creased along a board's edge, glued under the side it meets");
  def.keywords = {QStringLiteral("manufacturer's joint"), QStringLiteral("carton"), QStringLiteral("cardboard"),
                  QStringLiteral("board")};
  // Its flap is made on OK (add_glue_flap): the wall's profile sketch made
  // a trapezoid, then the flange; nothing is previewed.
  def.modelCommand = true;
  def.featureType.clear();
  def.inputs << valueInput(QStringLiteral("taper"), QObject::tr("Taper (°)"), ValueKind::Unitless, QStringLiteral("15"))
                    .withStepMinimum(0.0)
                    .withHeading(QObject::tr("Glue Flap"))
                    .withTooltip(QObject::tr("How far each end of the flap slants in"))
             << valueInput(QStringLiteral("inset"), QObject::tr("End Inset"), ValueKind::Length, QStringLiteral("0 mm"))
                    .withStepMinimum(0.0)
                    .withTooltip(QObject::tr("How far the flap keeps in from each end of the edge"));
  const auto flange = def.build;
  def.build = [flange](const CommandState& state, const CommandContext& model) {
    Built result = flange(state, model);
    if (result.def.isEmpty()) {
      return result;
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_glue_flap")},
                        {QStringLiteral("def"), result.def},
                        {QStringLiteral("taper"), state.value(QStringLiteral("taper"))},
                        {QStringLiteral("inset"), state.value(QStringLiteral("inset"))}};
    result.def = {{QStringLiteral("commands"), QJsonArray{command}}, {QStringLiteral("label"), QStringLiteral("Glue Flap")}};
    return result;
  };
  // Its flanges are Flange's to edit.
  def.load = nullptr;
  def.canEdit = [](const QJsonObject&) { return false; };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Glue flap, taper %1").arg(state.text(QStringLiteral("taper")));
  };
  filtered(def, context, [&context] {
    try {
      return !context.queryArray(QStringLiteral("sheet_bodies")).isEmpty();
    } catch (const std::exception&) {
      return false;
    }
  });
  return def;
}

// ---------------------------------------------------------------------------
// Standard Box: a parametric FEFCO or ECMA style box of board.

CommandDef standardBoxCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("sheet.standard_box");
  def.name = QObject::tr("Standard Box");
  def.icon = QStringLiteral("sheet-face");
  def.tooltip = QObject::tr("A standard box style of board: its blank, creases and folds");
  def.keywords = {QStringLiteral("fefco"), QStringLiteral("ecma"), QStringLiteral("carton"), QStringLiteral("0201"),
                  QStringLiteral("tuck end"), QStringLiteral("packaging"), QStringLiteral("0203"), QStringLiteral("0206"),
                  QStringLiteral("full overlap"), QStringLiteral("0421"), QStringLiteral("0426"), QStringLiteral("0427"),
                  QStringLiteral("0470"), QStringLiteral("0471"), QStringLiteral("folder"), QStringLiteral("mailer")};
  def.modelCommand = true;
  // A standard box's rules open here (mitcad#1939): OK changes its
  // options, and its blank, creases, Face and Folds follow.
  def.featureType = QStringLiteral("sheet_rules");
  def.canEdit = [](const QJsonObject& rules) { return rules.contains(QStringLiteral("standard_box")); };
  const auto custom = [](const CommandState& s) { return s.choice(QStringLiteral("board")) == QStringLiteral("custom"); };
  const auto adding = [](const CommandState& s) { return s.text(QStringLiteral("edited")).isEmpty(); };
  const auto isFolder = [](const CommandState& s) { return folderStyle(s.choice(QStringLiteral("style"))); };
  const auto hasTuck = [](const CommandState& s) { return tuckStyle(s.choice(QStringLiteral("style"))); };
  def.inputs = {
      editedInput(),
      choiceInput(QStringLiteral("style"), QObject::tr("Style"),
                  {{QStringLiteral("fefco_0201"), QStringLiteral("FEFCO 0201 Slotted Box")},
                   {QStringLiteral("ecma_a20"), QStringLiteral("ECMA A20 Reverse Tuck End")},
                   {QStringLiteral("fefco_0203"), QStringLiteral("FEFCO 0203 Full Overlap Slotted Box")},
                   {QStringLiteral("fefco_0206"), QStringLiteral("FEFCO 0206 Full Overlap Sides, Meeting Ends")},
                   {QStringLiteral("fefco_0421"), QStringLiteral("FEFCO 0421 Folder with Tuck Lid")},
                   {QStringLiteral("fefco_0426"), QStringLiteral("FEFCO 0426 Folder with Double Front Wall")},
                   {QStringLiteral("fefco_0427"), QStringLiteral("FEFCO 0427 Folder with Lid Ears")},
                   {QStringLiteral("fefco_0470"), QStringLiteral("FEFCO 0470 Folder, Ends on the Back Wall")},
                   {QStringLiteral("fefco_0471"), QStringLiteral("FEFCO 0471 Folder, Ends on the Back, Front Flap")}},
                  QStringLiteral("fefco_0201")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("300 mm"))
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("Inside")),
      valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("200 mm"))
          .withStepMinimum(0.0, true),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("150 mm"))
          .withStepMinimum(0.0, true),
      choiceInput(QStringLiteral("board"), QObject::tr("Board"),
                  {{QStringLiteral("c_flute"), QStringLiteral("Corrugated C flute 4 mm")},
                   {QStringLiteral("b_flute"), QStringLiteral("Corrugated B flute 3 mm")},
                   {QStringLiteral("e_flute"), QStringLiteral("Corrugated E flute 1.5 mm")},
                   {QStringLiteral("bc_flute"), QStringLiteral("Corrugated BC double wall 7 mm")},
                   {QStringLiteral("folding_carton"), QStringLiteral("Folding carton 0.5 mm")},
                   {QStringLiteral("polypropylene_3"), QStringLiteral("Corrugated polypropylene 3 mm")},
                   {QStringLiteral("polypropylene_4"), QStringLiteral("Corrugated polypropylene 4 mm")},
                   {QStringLiteral("polypropylene_5"), QStringLiteral("Corrugated polypropylene 5 mm")},
                   {QStringLiteral("custom"), QStringLiteral("Custom")}},
                  QStringLiteral("c_flute"))
          .withTooltip(QObject::tr("The board and a crease deduction to start with; calibrate it from a folded sample")),
      valueInput(QStringLiteral("thickness"), QObject::tr("Board Thickness"), ValueKind::Length, QStringLiteral("4 mm"))
          .withStepMinimum(0.0, true)
          .withVisible(custom),
      valueInput(QStringLiteral("deduction"), QObject::tr("Crease Deduction"), ValueKind::Length, QStringLiteral("4 mm"))
          .withTooltip(QObject::tr("The measured deduction of a 90° crease of this board"))
          .withVisible(custom),
      // A folder (FEFCO 04) has no glue flap (mitcad#2226).
      valueInput(QStringLiteral("glue_flap"), QObject::tr("Glue Flap"), ValueKind::Length, QStringLiteral("30 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([isFolder](const CommandState& s) { return !isFolder(s); }),
      // A tuck end's tuck and a folder's lid and tabs (mitcad#2269): empty
      // leaves the style's.
      valueInput(QStringLiteral("tuck"), QObject::tr("Tuck"), ValueKind::Length, QString())
          .withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("How deep the tuck or front flap reaches down from the lid; empty: the style's"))
          .withAuto([](const CommandState&, const CommandContext&) { return QString(); })
          .withVisible(hasTuck),
      valueInput(QStringLiteral("tabs"), QObject::tr("Tabs"), ValueKind::Unitless, QString())
          .withStepMinimum(0)
          .withTooltip(QObject::tr("The locking tabs on each turned over wall; empty: the style's"))
          .withAuto([](const CommandState& s, const CommandContext&) {
            return QString::number(styleTabs(s.choice(QStringLiteral("style"))));
          })
          .withVisible(isFolder),
      checkInput(QStringLiteral("folded"), QObject::tr("Folded"), true)
          .withTooltip(QObject::tr("Off: the flat blank with its creases"))
          .withVisible(adding),
      checkInput(QStringLiteral("component"), QObject::tr("As a Component"), false)
          .withTooltip(QObject::tr("In a component of its own, a part to save into a library"))
          .withVisible(adding),
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    const QString edited = state.text(QStringLiteral("edited"));
    QJsonObject command{{QStringLiteral("style"), state.choice(QStringLiteral("style"))}};
    if (edited.isEmpty()) {
      command.insert(QStringLiteral("cmd"), QStringLiteral("add_standard_box"));
      command.insert(QStringLiteral("folded"), state.checked(QStringLiteral("folded")));
      command.insert(QStringLiteral("component"), state.checked(QStringLiteral("component")));
    } else {
      command.insert(QStringLiteral("cmd"), QStringLiteral("edit_standard_box"));
      command.insert(QStringLiteral("rules"), edited);
    }
    for (const char* key : {"length", "width", "height", "glue_flap"}) {
      command.insert(QString::fromLatin1(key), state.value(QString::fromLatin1(key)));
    }
    // Tuck and Tabs (mitcad#2269): for the styles that have them; empty
    // leaves the style's (an edit takes a value given before away: null).
    const QString style = state.choice(QStringLiteral("style"));
    if (tuckStyle(style)) {
      const QString key = QStringLiteral("tuck");
      if (!leftEmpty(state, key)) {
        if (state.value(key) <= 0.0) {
          return Built::failure(QObject::tr("The tuck must be deeper than 0."), key);
        }
        command.insert(key, state.value(key));
      } else if (!edited.isEmpty()) {
        command.insert(key, QJsonValue::Null);
      }
    }
    if (folderStyle(style)) {
      const QString key = QStringLiteral("tabs");
      if (!leftEmpty(state, key)) {
        const double tabs = state.value(key);
        // The model says how many the blank takes (at most 12).
        if (tabs < 0.0 || std::abs(tabs - std::round(tabs)) > 1e-9) {
          return Built::failure(QObject::tr("Tabs must be a whole number, 0 or more."), key);
        }
        command.insert(key, static_cast<qint64>(std::llround(std::min(tabs, 1e9))));
      } else if (!edited.isEmpty()) {
        command.insert(key, QJsonValue::Null);
      }
    }
    const QString board = state.choice(QStringLiteral("board"));
    if (board == QStringLiteral("custom")) {
      command.insert(QStringLiteral("thickness"), state.value(QStringLiteral("thickness")));
      command.insert(QStringLiteral("deduction"), state.value(QStringLiteral("deduction")));
      if (!edited.isEmpty()) {
        command.insert(QStringLiteral("board"), QJsonValue::Null);
      }
    } else {
      command.insert(QStringLiteral("board"), board);
      if (!edited.isEmpty()) {
        command.insert(QStringLiteral("thickness"), QJsonValue::Null);
        command.insert(QStringLiteral("deduction"), QJsonValue::Null);
      }
    }
    return Built::of({{QStringLiteral("commands"), QJsonArray{command}},
                      {QStringLiteral("label"), edited.isEmpty() ? QStringLiteral("Standard Box")
                                                                 : QStringLiteral("Edit Standard Box")}});
  };
  def.load = [](const QJsonObject& rules, CommandState& state, const CommandContext& context) {
    state.setText(QStringLiteral("edited"), rules.value(QStringLiteral("uid")).toString());
    const QJsonObject box = rules.value(QStringLiteral("standard_box")).toObject();
    state.setChoice(QStringLiteral("style"), box.value(QStringLiteral("style")).toString(QStringLiteral("fefco_0201")));
    const QString board = box.value(QStringLiteral("board")).toString();
    state.setChoice(QStringLiteral("board"), board.isEmpty() ? QStringLiteral("custom") : board);
    for (const char* key : {"length", "width", "height", "glue_flap", "thickness", "deduction"}) {
      const QJsonValue value = box.value(QLatin1String(key));
      if (value.isDouble()) {
        loadValue(state, QString::fromLatin1(key), value, rules, context);
      }
    }
    // Tuck and Tabs as given, empty (the style's) when the box has none.
    const QJsonValue tuck = box.value(QStringLiteral("tuck"));
    if (tuck.isDouble()) {
      loadValue(state, QStringLiteral("tuck"), tuck, rules, context);
    } else {
      state.setText(QStringLiteral("tuck"), QString());
    }
    const QJsonValue tabs = box.value(QStringLiteral("tabs"));
    if (tabs.isDouble()) {
      loadCount(state, QStringLiteral("tabs"), tabs, rules, context);
    } else {
      state.setText(QStringLiteral("tabs"), QString());
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString style = state.choice(QStringLiteral("style"));
    QString line = QStringLiteral("%1 %2, %3 x %4 x %5")
                       .arg(state.text(QStringLiteral("edited")).isEmpty() ? QStringLiteral("Standard box")
                                                                           : QStringLiteral("Edited standard box"))
                       .arg(style, state.text(QStringLiteral("length")), state.text(QStringLiteral("width")),
                            state.text(QStringLiteral("height")));
    // Tuck and Tabs as sent (mitcad#2269): mm, or the style's.
    if (tuckStyle(style)) {
      line += leftEmpty(state, QStringLiteral("tuck"))
                  ? QStringLiteral(", tuck by the style")
                  : QStringLiteral(", tuck %1 mm").arg(state.value(QStringLiteral("tuck")));
    }
    if (folderStyle(style)) {
      line += leftEmpty(state, QStringLiteral("tabs"))
                  ? QStringLiteral(", tabs by the style")
                  : QStringLiteral(", tabs %1").arg(std::llround(state.value(QStringLiteral("tabs"))));
    }
    return line;
  };
  // Offered for every profile: it makes its own board rules.
  (void)context;
  return def;
}

} // namespace mitcad::cmd

namespace mitcad {

void registerSheetProcessCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  // Their places on the toolbar are the layout's (mitcad#1951).
  registry.add(fingerJointCommand(context));
  registry.add(tNutSlotCommand(context));
  registry.add(kerfBendCommand(context));
  registry.add(flatPackCommand(context));
  registry.add(glueFlapCommand(context));
  registry.add(standardBoxCommand(context));
}

} // namespace mitcad
