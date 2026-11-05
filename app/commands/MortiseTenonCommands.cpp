// SPDX-License-Identifier: MIT
// Mortise and Tenon (mitcad#1012; commands.md, "Mortise and Tenon"): a
// tenon on a planar end face of one body and its mortise cut into another
// body, larger by the clearance; square corners, or rounded by a tool's
// diameter. Offsets left at 0 are left out (the tenon is centred). A
// Count above 1 spaces that many tenons, each with its mortise, between a
// Start Offset and an End Offset along the end face (mitcad#1063).
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kCorners = {{QStringLiteral("square"), QStringLiteral("Square")},
                          {QStringLiteral("rounded"), QStringLiteral("Rounded")}};

} // namespace

CommandDef mortiseTenonCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mortise_tenon");
  def.name = QObject::tr("Mortise and Tenon");
  def.icon = QStringLiteral("mortise-tenon");
  def.tooltip = QObject::tr("A tenon on a body's end face and its mortise in another body");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("joint"), QStringLiteral("joinery"), QStringLiteral("tenon"),
                  QStringLiteral("mortise"), QStringLiteral("woodworking")};
  def.featureType = QStringLiteral("mortise_tenon");
  const auto blind = [](const CommandState& s) { return !s.checked(QStringLiteral("through")); };
  const auto rounded = [](const CommandState& s) {
    return s.choice(QStringLiteral("corners")) == QStringLiteral("rounded");
  };
  // Several tenons (mitcad#1063): the start and end offsets place them
  // along the end face's longest edge, instead of the offset along it
  // (along the width, or the thickness when turned).
  const auto several = [](const CommandState& s) { return s.value(QStringLiteral("count")) > 1.0; };
  const auto alongWidthShown = [several](const CommandState& s) {
    return !several(s) || s.checked(QStringLiteral("turned"));
  };
  const auto alongThicknessShown = [several](const CommandState& s) {
    return !several(s) || !s.checked(QStringLiteral("turned"));
  };
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("End Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The planar face the tenon stands on"))
          .withAccepts(isPlanar)
          .withoutMeshes(),
      selectionInput(QStringLiteral("mortise_body"), QObject::tr("Mortise Body"), SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("The body the mortise is cut into"))
          .withoutMeshes(),
      valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("20 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("Along the end face's longest edge")),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("6 mm")).withStepMinimum(0.0, true),
      checkInput(QStringLiteral("through"), QObject::tr("Through"), false)
          .withTooltip(QObject::tr("The tenon reaches the mortise body's far face")),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("20 mm")).withStepMinimum(0.0, true)
          .withVisible(blind),
      valueInput(QStringLiteral("offset_width"), QObject::tr("Offset Along Width"), ValueKind::Length,
                 QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("The tenon's middle from the end face's middle"))
          .withVisible(alongWidthShown),
      valueInput(QStringLiteral("offset_thickness"), QObject::tr("Offset Along Thickness"), ValueKind::Length,
                 QStringLiteral("0 mm"))
          .withVisible(alongThicknessShown),
      valueInput(QStringLiteral("count"), QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("1")).withStepMinimum(1)
          .withTooltip(QObject::tr("How many tenons along the end face's longest edge, each with its mortise")),
      valueInput(QStringLiteral("start_offset"), QObject::tr("Start Offset"), ValueKind::Length,
                 QStringLiteral("10 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("From the end face's start to the first tenon"))
          .withVisible(several),
      valueInput(QStringLiteral("end_offset"), QObject::tr("End Offset"), ValueKind::Length, QStringLiteral("10 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("From the last tenon to the end face's other end"))
          .withVisible(several),
      checkInput(QStringLiteral("turned"), QObject::tr("Turned"), false)
          .withTooltip(QObject::tr("The width across the end face's longest edge")),
      valueInput(QStringLiteral("clearance"), QObject::tr("Clearance"), ValueKind::Length, QStringLiteral("0.1 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("How much larger the mortise is on each side")),
      choiceInput(QStringLiteral("corners"), QObject::tr("Corners"), kCorners, QStringLiteral("square")),
      valueInput(QStringLiteral("tool_diameter"), QObject::tr("Tool Diameter"), ValueKind::Length,
                 QStringLiteral("6 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The cutter that rounds the mortise's corners"))
          .withVisible(rounded),
  };
  def.enabled = [&context] { return context.queryArray(QStringLiteral("bodies")).size() > 1; };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem& face = state.items(QStringLiteral("face")).first();
    const SelectionItem& other = state.items(QStringLiteral("mortise_body")).first();
    if (other.owner == face.owner) {
      return Built::failure(QObject::tr("The mortise goes into another body than the tenon's."),
                            QStringLiteral("mortise_body"));
    }
    // The feature reads both bodies in its own component (no links).
    if (other.occurrence != face.occurrence) {
      return Built::failure(QObject::tr("Pick both bodies in the same component."), QStringLiteral("mortise_body"));
    }
    const QJsonObject ref = face.reference().toObject();
    QJsonObject d{{QStringLiteral("type"), QStringLiteral("mortise_tenon")},
                  {QStringLiteral("body"), ref.value(QStringLiteral("body"))},
                  {QStringLiteral("face"), ref.value(QStringLiteral("face"))},
                  {QStringLiteral("mortise_body"), other.owner}};
    const auto value = [&](const char* key) {
      d.insert(QString::fromLatin1(key), state.expression(QString::fromLatin1(key)));
    };
    value("width");
    value("thickness");
    if (state.checked(QStringLiteral("through"))) {
      d.insert(QStringLiteral("through"), true);
    } else {
      value("length");
    }
    // With several tenons the offset along the longest edge is hidden and
    // not used: left out.
    const bool turned = state.checked(QStringLiteral("turned"));
    const bool manyTenons = state.value(QStringLiteral("count")) > 1.0;
    if (nonZero(state, QStringLiteral("offset_width")) && (!manyTenons || turned)) {
      value("offset_width");
    }
    if (nonZero(state, QStringLiteral("offset_thickness")) && (!manyTenons || !turned)) {
      value("offset_thickness");
    }
    if (turned) {
      d.insert(QStringLiteral("turned"), true);
    }
    value("clearance");
    if (state.choice(QStringLiteral("corners")) == QStringLiteral("rounded")) {
      d.insert(QStringLiteral("corners"), QStringLiteral("rounded"));
      value("tool_diameter");
    }
    // A count of 1 is left out, so that a single tenon is defined as
    // before (mitcad#1063); any other count, or an expression, is written
    // with the start and end offsets.
    if (state.expression(QStringLiteral("count")).trimmed() != QStringLiteral("1")) {
      value("count");
      value("start_offset");
      value("end_offset");
    }
    result.def = d;
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("face"),
                   {itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                       {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                           context)});
    state.setItems(QStringLiteral("mortise_body"), bodiesOf({feature.value(QStringLiteral("mortise_body"))}));
    for (const char* key : {"width", "thickness", "length", "offset_width", "offset_thickness", "clearance",
                            "tool_diameter", "start_offset", "end_offset"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadCount(state, QStringLiteral("count"), feature.value(QStringLiteral("count")), feature, context);
    state.setChecked(QStringLiteral("through"), feature.value(QStringLiteral("through")).toBool());
    state.setChecked(QStringLiteral("turned"), feature.value(QStringLiteral("turned")).toBool());
    state.setChoice(QStringLiteral("corners"),
                    feature.value(QStringLiteral("corners")).toString(QStringLiteral("square")));
  };

  // A face the panel cannot show (a body of another component).
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                  {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                      context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString kind =
        state.checked(QStringLiteral("through")) ? QStringLiteral("through") : QStringLiteral("blind");
    const double count = state.value(QStringLiteral("count"));
    if (count > 1.0) {
      return QStringLiteral("Added mortise and tenon (%1, %2 tenons)").arg(kind).arg(qRound(count));
    }
    return QStringLiteral("Added mortise and tenon (%1)").arg(kind);
  };
  return def;
}

} // namespace mitcad::cmd
