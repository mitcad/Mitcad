// SPDX-License-Identifier: MIT
// Wood Joint (mitcad#2604; commands.md and app/COMMANDS.md, "Wood Joint"):
// the Build tab's woodworking joint between two parts, in the Connect
// group, chosen by its Type. Dado, Rabbet, Tongue and Groove, Bridle, Box
// Joint, Loose Tenon and Dowel add a `wood_joint` feature; Mortise and
// Tenon and Dogbone add the features of their own commands (Solid › Modify
// › Joinery), with those commands' inputs and definitions, and are edited
// there. Values left at 0 are left out, so that the model's defaults apply.
#include <limits>

#include "../framework/LengthUnits.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kJointTypes = {{QStringLiteral("dado"), QStringLiteral("Dado")},
                             {QStringLiteral("rabbet"), QStringLiteral("Rabbet")},
                             {QStringLiteral("tongue_and_groove"), QStringLiteral("Tongue and Groove")},
                             {QStringLiteral("bridle"), QStringLiteral("Bridle")},
                             {QStringLiteral("box_joint"), QStringLiteral("Box Joint")},
                             {QStringLiteral("loose_tenon"), QStringLiteral("Loose Tenon")},
                             {QStringLiteral("dowel"), QStringLiteral("Dowel")},
                             {QStringLiteral("mortise_tenon"), QStringLiteral("Mortise and Tenon")},
                             {QStringLiteral("dogbone"), QStringLiteral("Dogbone")}};

const QString kMortiseTenon = QStringLiteral("mortise_tenon");
const QString kDogbone = QStringLiteral("dogbone");

std::function<bool(const CommandState&)> jointTypeIn(const QStringList& types) {
  return [types](const CommandState& state) { return types.contains(state.choice(QStringLiteral("type"))); };
}

bool severalPieces(const CommandState& state) { return state.value(QStringLiteral("count")) > 1.0; }

// Which wood joint types use each of Mortise and Tenon's inputs, besides
// Mortise and Tenon itself; an input not listed is Mortise and Tenon's
// alone.
QStringList woodTypesOf(const QString& input) {
  const QString loose = QStringLiteral("loose_tenon");
  const QString dowel = QStringLiteral("dowel");
  const QString tongue = QStringLiteral("tongue_and_groove");
  const QString bridle = QStringLiteral("bridle");
  if (input == QStringLiteral("face") || input == QStringLiteral("mortise_body") ||
      input == QStringLiteral("clearance")) {
    return {QStringLiteral("dado"), QStringLiteral("rabbet"), tongue, bridle, QStringLiteral("box_joint"), loose,
            dowel};
  }
  if (input == QStringLiteral("width")) {
    return {loose};
  }
  if (input == QStringLiteral("thickness")) {
    return {tongue, bridle, loose};
  }
  if (input == QStringLiteral("offset_thickness")) {
    return {tongue, bridle, loose, dowel};
  }
  if (input == QStringLiteral("offset_width") || input == QStringLiteral("count") ||
      input == QStringLiteral("start_offset") || input == QStringLiteral("end_offset")) {
    return {loose, dowel};
  }
  return {};
}

// The depth a new joint of the chosen type starts with, in the design's
// unit.
void jointTypeDefaults(CommandState& state, const CommandContext& context) {
  const QString type = state.choice(QStringLiteral("type"));
  QString depth;
  if (type == QStringLiteral("dado") || type == QStringLiteral("rabbet")) {
    depth = QStringLiteral("6 mm");
  } else if (type == QStringLiteral("tongue_and_groove")) {
    depth = QStringLiteral("10 mm");
  } else if (type == QStringLiteral("loose_tenon") || type == QStringLiteral("dowel")) {
    depth = QStringLiteral("20 mm");
  }
  if (!depth.isEmpty()) {
    state.setText(QStringLiteral("depth"), defaultLengthText(depth, context.lengthUnit()));
    state.setValue(QStringLiteral("depth"), std::numeric_limits<double>::quiet_NaN());
  }
}

} // namespace

CommandDef woodJointCommand(const CommandContext& context) {
  const CommandDef mortise = mortiseTenonCommand(context);
  const CommandDef dogbone = dogboneCommand(context);
  CommandDef def;
  def.id = QStringLiteral("build.joint");
  def.name = QObject::tr("Wood Joint");
  def.icon = QStringLiteral("wood-joint");
  def.tooltip = QObject::tr("A woodworking joint between two parts, both cut to fit");
  def.keywords = {QStringLiteral("joinery"),     QStringLiteral("woodworking"), QStringLiteral("dado"),
                  QStringLiteral("housing"),     QStringLiteral("rabbet"),      QStringLiteral("rebate"),
                  QStringLiteral("tongue"),      QStringLiteral("groove"),      QStringLiteral("bridle"),
                  QStringLiteral("box joint"),   QStringLiteral("loose tenon"),
                  QStringLiteral("dowel"),       QStringLiteral("mortise"),     QStringLiteral("tenon"),
                  QStringLiteral("dogbone")};
  def.featureType = QStringLiteral("wood_joint");
  const auto mortiseType = jointTypeIn({kMortiseTenon});
  const auto dogboneType = jointTypeIn({kDogbone});

  def.inputs.push_back(choiceInput(QStringLiteral("type"), QObject::tr("Type"), kJointTypes, QStringLiteral("dado"))
                           .withOnChange([](CommandState& state, const CommandContext& model) {
                             jointTypeDefaults(state, model);
                           }));
  for (InputDef input : mortise.inputs) {
    const QStringList woodTypes = woodTypesOf(input.id);
    const auto own = input.visible;
    const QString id = input.id;
    // Mortise and Tenon shows its inputs as its own command does; a wood
    // joint type shows those it uses, Count's start and end offsets with
    // several pieces and the offset along the width with one.
    input.visible = [own, mortiseType, woodTypes, id](const CommandState& s) {
      if (mortiseType(s)) {
        return !own || own(s);
      }
      if (!woodTypes.contains(s.choice(QStringLiteral("type")))) {
        return false;
      }
      if (id == QStringLiteral("start_offset") || id == QStringLiteral("end_offset")) {
        return severalPieces(s);
      }
      if (id == QStringLiteral("offset_width")) {
        return !severalPieces(s);
      }
      return true;
    };
    if (id == QStringLiteral("mortise_body")) {
      input.label = QObject::tr("Other Part");
      input.tooltip = QObject::tr("The part the joint is cut into");
      input.labelFor = [mortiseType](const CommandState& s) {
        return mortiseType(s) ? QObject::tr("Mortise Body") : QObject::tr("Other Part");
      };
    }
    if (id == QStringLiteral("count")) {
      input.tooltip = QObject::tr("How many tenons, loose tenons or dowels along the end face's longest edge");
    }
    def.inputs.push_back(input);
    if (id == QStringLiteral("mortise_body")) {
      // The wood joint's own sizes, after the parts.
      def.inputs.push_back(
          valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("6 mm"))
              .withStepMinimum(0.0, true)
              .withTooltip(QObject::tr("How far the joint goes into the other part"))
              .withVisible(jointTypeIn({QStringLiteral("dado"), QStringLiteral("rabbet"),
                                        QStringLiteral("tongue_and_groove"), QStringLiteral("loose_tenon"),
                                        QStringLiteral("dowel")})));
      def.inputs.push_back(
          valueInput(QStringLiteral("body_depth"), QObject::tr("Back Depth"), ValueKind::Length,
                     QStringLiteral("0 mm"))
              .withStepMinimum(0.0)
              .withTooltip(QObject::tr("How far the pieces go into the end face's part; 0: as deep as Depth"))
              .withVisible(jointTypeIn({QStringLiteral("loose_tenon"), QStringLiteral("dowel")})));
      def.inputs.push_back(valueInput(QStringLiteral("dowel_diameter"), QObject::tr("Dowel Diameter"),
                                      ValueKind::Length, QStringLiteral("8 mm"))
                               .withStepMinimum(0.0, true)
                               .withVisible(jointTypeIn({QStringLiteral("dowel")})));
      def.inputs.push_back(
          valueInput(QStringLiteral("fingers"), QObject::tr("Fingers"), ValueKind::Unitless, QStringLiteral("0"))
              .withStepMinimum(0)
              .withTooltip(QObject::tr("Both parts' fingers along the end face; 0: an odd count near its length "
                                       "over its thickness"))
              .withVisible(jointTypeIn({QStringLiteral("box_joint")})));
      def.inputs.push_back(checkInput(QStringLiteral("flip"), QObject::tr("Flip"), false)
                               .withTooltip(QObject::tr("The other part takes the fingers at the ends"))
                               .withVisible(jointTypeIn({QStringLiteral("box_joint")})));
    }
  }
  for (InputDef input : dogbone.inputs) {
    const auto own = input.visible;
    input.visible = [own, dogboneType](const CommandState& s) { return dogboneType(s) && (!own || own(s)); };
    if (input.id == QStringLiteral("dogbone_type")) {
      input.label = QObject::tr("Relief");
    }
    def.inputs.push_back(input);
  }
  def.enabled = [&context] { return hasBodies(context); };

  const auto mortiseBuild = mortise.build;
  const auto dogboneBuild = dogbone.build;
  def.build = [mortiseBuild, dogboneBuild](const CommandState& state, const CommandContext& model) {
    const QString type = state.choice(QStringLiteral("type"));
    if (type == kMortiseTenon) {
      return mortiseBuild(state, model);
    }
    if (type == kDogbone) {
      return dogboneBuild(state, model);
    }
    Built result;
    const SelectionItem& face = state.items(QStringLiteral("face")).first();
    const SelectionItem& other = state.items(QStringLiteral("mortise_body")).first();
    if (other.owner == face.owner) {
      return Built::failure(QObject::tr("The joint goes into another part than the end face's."),
                            QStringLiteral("mortise_body"));
    }
    // The feature reads both parts in its own component (no links).
    if (other.occurrence != face.occurrence) {
      return Built::failure(QObject::tr("Pick both parts in the same component."), QStringLiteral("mortise_body"));
    }
    const QJsonObject ref = face.reference().toObject();
    QJsonObject d{{QStringLiteral("type"), QStringLiteral("wood_joint")},
                  {QStringLiteral("kind"), type},
                  {QStringLiteral("body"), ref.value(QStringLiteral("body"))},
                  {QStringLiteral("face"), ref.value(QStringLiteral("face"))},
                  {QStringLiteral("other"), other.owner}};
    const auto value = [&](const char* key, const char* input) {
      d.insert(QString::fromLatin1(key), state.expression(QString::fromLatin1(input)));
    };
    const bool loose = type == QStringLiteral("loose_tenon") || type == QStringLiteral("dowel");
    const bool tongue = type == QStringLiteral("tongue_and_groove") || type == QStringLiteral("bridle");
    if (type != QStringLiteral("bridle") && type != QStringLiteral("box_joint")) {
      value("depth", "depth");
    }
    if (tongue || type == QStringLiteral("loose_tenon")) {
      value("thickness", "thickness");
    }
    if (type == QStringLiteral("loose_tenon")) {
      value("width", "width");
    }
    if (type == QStringLiteral("dowel")) {
      value("diameter", "dowel_diameter");
    }
    if (loose && nonZero(state, QStringLiteral("body_depth"))) {
      value("body_depth", "body_depth");
    }
    if ((tongue || loose) && nonZero(state, QStringLiteral("offset_thickness"))) {
      value("offset_thickness", "offset_thickness");
    }
    if (loose) {
      // A count of 1 is left out with its start and end offsets; with
      // several pieces the offset along the width is not used.
      if (state.expression(QStringLiteral("count")).trimmed() != QStringLiteral("1")) {
        value("count", "count");
        value("start_offset", "start_offset");
        value("end_offset", "end_offset");
      } else if (nonZero(state, QStringLiteral("offset_width"))) {
        value("offset_width", "offset_width");
      }
    }
    if (type == QStringLiteral("box_joint")) {
      if (nonZero(state, QStringLiteral("fingers"))) {
        value("count", "fingers");
      }
      if (state.checked(QStringLiteral("flip"))) {
        d.insert(QStringLiteral("flip"), true);
      }
    }
    value("clearance", "clearance");
    result.def = d;
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString kind = feature.value(QStringLiteral("kind")).toString();
    state.setChoice(QStringLiteral("type"), kind);
    state.setItems(QStringLiteral("face"),
                   {itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                       {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                           context)});
    state.setItems(QStringLiteral("mortise_body"), bodiesOf({feature.value(QStringLiteral("other"))}));
    for (const char* key : {"depth", "body_depth", "width", "thickness", "offset_width", "offset_thickness",
                            "start_offset", "end_offset", "clearance"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadValue(state, QStringLiteral("dowel_diameter"), feature.value(QStringLiteral("diameter")), feature, context);
    if (kind == QStringLiteral("box_joint")) {
      loadCount(state, QStringLiteral("fingers"), feature.value(QStringLiteral("count")), feature, context);
    } else {
      loadCount(state, QStringLiteral("count"), feature.value(QStringLiteral("count")), feature, context);
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };

  // A face the panel cannot show (a body of another component).
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                  {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                      context);
  };

  const auto mortiseDescribe = mortise.describe;
  const auto dogboneDescribe = dogbone.describe;
  def.describe = [mortiseDescribe, dogboneDescribe](const CommandState& state, const CommandContext& model) {
    const QString type = state.choice(QStringLiteral("type"));
    if (type == kMortiseTenon) {
      return mortiseDescribe(state, model);
    }
    if (type == kDogbone) {
      return dogboneDescribe(state, model);
    }
    return QStringLiteral("Added wood joint (%1)").arg(label(kJointTypes, type));
  };
  return def;
}

} // namespace mitcad::cmd
