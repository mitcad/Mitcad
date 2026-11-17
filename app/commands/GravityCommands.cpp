// SPDX-License-Identifier: MIT
// Let It Hang (mitcad#1713; app/COMMANDS.md "Hanging cables"; the ASSEMBLE
// group): the panel of a Flex item let hang. The item (a pick field only
// when none was selected), then the design's direction of gravity
// (Direction From: a planar face or a plane, gravity pulling against its
// normal, or an axis, a straight edge or a sketch line, along it; Reverse;
// Clear Gravity), the item's Length (applied once committed: the field left
// or Enter), the Damping (a slider and its field, applied as it moves) and
// its ends pinned or let go. An application panel: each change makes
// the panel's definition so in the view (the main window's hooks,
// gravity/GravityController): the direction set (gravity.set; the item
// goes on hanging and settles again), the item let hang, its length,
// damping and end pins. OK keeps the shape it rests in (flex.hang_lock; the
// panel's direction changes and the lock one undo step); Cancel puts it
// back and takes the direction changes back. The arrow of the direction it
// sets is drawn at the pick.
#include "Commands.hpp"

#include <cmath>
#include <exception>
#include <functional>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../framework/Command.hpp"
#include "../gravity/GravityLogic.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

const QString kItem = QStringLiteral("item");
const QString kSource = QStringLiteral("source");
const QString kReverse = QStringLiteral("reverse");
const QString kClear = QStringLiteral("clear");
const QString kLength = QStringLiteral("length");
const QString kDamping = QStringLiteral("damping");
const QString kStartPinned = QStringLiteral("start_pinned");
const QString kEndPinned = QStringLiteral("end_pinned");
// Not inputs: the `gravity` query's answer (read when the panel opens and
// when the model changed beside it), whether the item came with the
// selection made before the command (its pick field is then not shown),
// the length text the panel filled in (sent only once changed) and the
// length last committed (the field left or Enter pressed in it: each key
// would make the hanging item another length, resampled, its swing
// started again; "1200" typed hung it at 1, 12 and 120 on the way).
const QString kGravity = QStringLiteral("gravity");
const QString kItemGiven = QStringLiteral("item_given");
const QString kLengthShown = QStringLiteral("length_shown");
const QString kLengthCommitted = QStringLiteral("length_committed");

bool takesSource(const SelectionItem& item) { return cmd::isPlanar(item) || cmd::isStraightLine(item); }

bool hasDirection(const CommandState& state) {
  return state.object(kGravity).value(QStringLiteral("direction")).isArray();
}

void readGravity(CommandState& state, const CommandContext& model) {
  try {
    state.setObject(kGravity, model.queryObject({{QStringLiteral("query"), QStringLiteral("gravity")}}));
  } catch (const std::exception&) {
    state.setObject(kGravity, QJsonObject());
  }
}

// The item's length, in the design's unit, as the Length field starts.
void showItemLength(CommandState& state, const QString& flex, const CommandContext& model) {
  if (flex.isEmpty()) {
    return;
  }
  QJsonValue length;
  try {
    length = model.queryObject({{QStringLiteral("query"), QStringLiteral("flex")}, {QStringLiteral("uid"), flex}})
                 .value(QStringLiteral("length"));
  } catch (const std::exception&) {
    return;
  }
  if (!length.isDouble()) {
    return;
  }
  const QString text = cmd::lengthText(length.toDouble(), model);
  state.setText(kLength, text);
  state.setExpression(kLength, text);
  state.setValue(kLength, length.toDouble());
  QJsonObject shown;
  shown.insert(QStringLiteral("text"), text);
  shown.insert(QStringLiteral("mm"), length.toDouble());
  state.setObject(kLengthShown, shown);
  state.setObject(kLengthCommitted, shown);
}

// The Length field's value committed (InputDef::onCommit).
void commitLength(CommandState& state, const CommandContext& /*model*/) {
  QJsonObject committed;
  committed.insert(QStringLiteral("text"), state.text(kLength));
  committed.insert(QStringLiteral("mm"), state.value(kLength));
  state.setObject(kLengthCommitted, committed);
}

// Where the arrow starts and the way gravity pulls, from the pick.
std::optional<std::pair<gp_Pnt, gp_Dir>> pullOf(const SelectionItem& item, bool reverse, const CommandContext& context) {
  std::optional<std::pair<gp_Pnt, gp_Dir>> pull;
  if (cmd::isPlanar(item)) {
    if (const std::optional<cmd::PlaneFrame> frame = cmd::frameOf(item, context)) {
      // From a face's middle, so that the arrow stands on what was picked
      // (a face's frame has its origin at a corner); a plane from its own.
      gp_Pnt from = frame->origin;
      if (item.kind == SelectKind::Face) {
        if (const std::optional<gp_Pnt> middle = cmd::centerOf({item}, context)) {
          from = *middle;
        }
      }
      pull = std::make_pair(from, frame->normal.Reversed()); // against the normal
    }
  } else if (const std::optional<std::pair<gp_Pnt, gp_Dir>> axis = cmd::axisOf(item, context)) {
    pull = axis;
  }
  if (pull && reverse) {
    pull->second.Reverse();
  }
  return pull;
}

} // namespace

bool setLetItHangEnds(CommandState& state, bool start, bool end) {
  if (state.checked(kStartPinned) == start && state.checked(kEndPinned) == end) {
    return false;
  }
  state.setChecked(kStartPinned, start);
  state.setChecked(kEndPinned, end);
  return true;
}

CommandDef letItHangCommand(const CommandContext& context, LetItHangHooks hooks) {
  auto shared = std::make_shared<LetItHangHooks>(std::move(hooks));
  const auto flexOf = [shared](const SelectionItem& item) {
    return shared->flexOf ? shared->flexOf(item) : QString();
  };
  CommandDef def;
  def.id = QStringLiteral("assemble.flex_hang");
  def.name = QObject::tr("Let It Hang");
  def.icon = QStringLiteral("flex-hang");
  def.tooltip = QObject::tr("Lets a cable or hose hang under gravity, then keeps the shape it rests in");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("hang"),  QStringLiteral("drape"),   QStringLiteral("cable"),
                  QStringLiteral("hose"),  QStringLiteral("wire"),    QStringLiteral("flex"),
                  QStringLiteral("sag"),   QStringLiteral("gravity"), QStringLiteral("down"),
                  QStringLiteral("weight")};
  def.inputs = {
      // The item: only when none came with the selection.
      selectionInput(kItem, QObject::tr("Item"),
                     SelectKind::Body | SelectKind::Face | SelectKind::Edge | SelectKind::Feature)
          .withTooltip(QObject::tr("The cable or hose to let hang"))
          .withAccepts([flexOf](const SelectionItem& item) { return !flexOf(item).isEmpty(); })
          .withVisible([](const CommandState& state) { return !state.checked(kItemGiven); })
          .withOnPick([flexOf](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            showItemLength(state, flexOf(item), model);
          }),
      // The direction of gravity: what the design has when the panel opens.
      selectionInput(kSource, QObject::tr("Direction From"),
                     SelectKind::Face | SelectKind::Plane | SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve,
                     0, 1)
          .withHeading(QObject::tr("Gravity"))
          .withTooltip(QObject::tr("A planar face or plane: gravity pulls against its normal. An axis, edge or line: "
                                   "along it."))
          .withAccepts(takesSource),
      checkInput(kReverse, QObject::tr("Reverse"), false)
          .withTooltip(QObject::tr("Gravity pulls the other way"))
          .withEnabled([](const CommandState& state) { return !state.items(kSource).isEmpty(); }),
      // While the design has a direction: cleared (the item is put back).
      noteInput(
          kClear,
          [](const CommandState& state) {
            return hasDirection(state) ? InputNote{QString(), {QObject::tr("Clear Gravity")}, {}} : InputNote{};
          },
          [shared](int, const CommandState&, const CommandContext&) {
            if (shared->clear) {
              shared->clear();
            }
          })
          .withFieldColumn(),
      valueInput(kLength, QObject::tr("Length"), ValueKind::Length, QString())
          .withTooltip(QObject::tr("How long the hanging item is; it stays as it is unless changed here"))
          .withOnCommit(commitLength),
      valueInput(kDamping, QObject::tr("Damping"), ValueKind::Unitless, QStringLiteral("0.5"))
          .withTooltip(QObject::tr("How quickly it stops swinging, from 0 to 1; 0.5 gives a slight swing"))
          .withSlider(0.0, 1.0, 2),
      checkInput(kStartPinned, QObject::tr("Start pinned"), true)
          .withHeading(QObject::tr("Ends"))
          .withTooltip(QObject::tr("The item's first node stays where it is")),
      checkInput(kEndPinned, QObject::tr("End pinned"), true)
          .withTooltip(QObject::tr("The item's last node stays where it is")),
  };
  // A design with a direction: the panel starts from what it was taken
  // from (unless the selection made before gave another); the item that
  // came with the selection: its length.
  def.init = [shared, flexOf](CommandState& state, const CommandContext& model) {
    readGravity(state, model);
    const Selection items = state.items(kItem);
    if (!items.isEmpty()) {
      state.setChecked(kItemGiven, true);
      showItemLength(state, flexOf(items.first()), model);
    }
    if (shared->opened) {
      shared->opened();
    }
    if (!state.items(kSource).isEmpty()) {
      return;
    }
    const QJsonObject source = state.object(kGravity).value(QStringLiteral("source")).toObject();
    if (source.isEmpty()) {
      return;
    }
    SelectionItem item = cmd::itemOf(source.value(QStringLiteral("geometry")), model);
    if (!item.isValid() || !takesSource(item)) {
      return;
    }
    item.occurrence = source.value(QStringLiteral("occurrence")).toString();
    state.setItems(kSource, {item});
    state.setChecked(kReverse, source.value(QStringLiteral("reverse")).toBool());
  };
  // The direction cleared beside the inputs (the Clear Gravity button): the
  // field asks for one again.
  def.modelChanged = [](CommandState& state, const CommandContext& model) {
    readGravity(state, model);
    if (!hasDirection(state)) {
      state.setItems(kSource, {});
      state.setChecked(kReverse, false);
    }
  };
  def.build = [flexOf](const CommandState& state, const CommandContext&) {
    const Selection items = state.items(kItem);
    if (items.isEmpty()) {
      return Built::asking(QObject::tr("Pick the cable or hose to let hang."), kItem);
    }
    gravity::HangWish wish;
    wish.flex = flexOf(items.first());
    if (wish.flex.isEmpty()) {
      return Built::failure(QObject::tr("Only a cable or hose can hang."), kItem);
    }
    const Selection source = state.items(kSource);
    if (!source.isEmpty()) {
      const SelectionItem& item = source.first();
      wish.gravity = gravity::setCommand(item.reference(), item.occurrence, state.checked(kReverse));
    } else if (!hasDirection(state)) {
      return Built::asking(QObject::tr("Pick a planar face, a plane or an axis that gives the way gravity pulls."),
                           kSource);
    }
    // The length as committed once it was changed (a text being typed is
    // checked, not applied); as the field showed it at first, the item's
    // own exactly (not the shown text's rounding).
    const QJsonObject shown = state.object(kLengthShown);
    const QString shownText = shown.value(QStringLiteral("text")).toString().trimmed();
    if (state.text(kLength).trimmed() != shownText) {
      const double length = state.value(kLength);
      if (!(std::isfinite(length) && length > 0.0)) {
        return Built::failure(QObject::tr("The length must be greater than zero."), kLength);
      }
    }
    const QJsonObject committed = state.object(kLengthCommitted);
    if (!committed.isEmpty() && committed.value(QStringLiteral("text")).toString().trimmed() != shownText) {
      const double length = committed.value(QStringLiteral("mm")).toDouble();
      if (std::isfinite(length) && length > 0.0) {
        wish.length = length;
      }
    } else if (shown.value(QStringLiteral("mm")).isDouble()) {
      wish.length = shown.value(QStringLiteral("mm")).toDouble();
    }
    const double damping = state.value(kDamping);
    if (!(std::isfinite(damping) && damping >= 0.0 && damping <= 1.0)) {
      return Built::failure(QObject::tr("The damping must be from 0 to 1."), kDamping);
    }
    wish.damping = damping;
    wish.startPinned = state.checked(kStartPinned);
    wish.endPinned = state.checked(kEndPinned);
    return Built::of(wish.toJson());
  };
  // Each change made so in the view; an empty definition (inputs not
  // complete, a value being typed): the item hangs on as it is.
  def.acceptPreview = [shared](const QJsonObject& definition) {
    return shared->apply ? shared->apply(definition) : QString();
  };
  def.accept = [shared](const CommandState&, const CommandContext&) { return shared->lock && shared->lock(); };
  def.ended = [shared](bool committed) {
    if (shared->ended) {
      shared->ended(committed);
    }
  };
  // Offered at the context menu's top level for one cable or hose.
  def.offeredAtTop = [flexOf](const Selection& selection) {
    return selection.size() == 1 && !flexOf(selection.first()).isEmpty();
  };
  // The arrow of the direction it sets, at the pick.
  def.viewLines = [](const CommandState& state, const CommandContext& model) {
    std::vector<std::vector<gp_Pnt>> lines;
    const Selection items = state.items(kSource);
    if (items.isEmpty()) {
      return lines;
    }
    const std::optional<std::pair<gp_Pnt, gp_Dir>> pull = pullOf(items.first(), state.checked(kReverse), model);
    if (!pull) {
      return lines;
    }
    const gp_Pnt& from = pull->first;
    const gp_Dir& way = pull->second;
    const double pixel = model.pixelSize(from);
    const double size = std::isfinite(pixel) && pixel > 0.0 ? 70.0 * pixel : 30.0;
    const gp_Pnt tip = from.Translated(gp_Vec(way) * size);
    // Across the arrow: square to it, as near the model's Z as it can be.
    const gp_Vec helper = std::abs(way.Z()) < 0.9 ? gp_Vec(0.0, 0.0, 1.0) : gp_Vec(1.0, 0.0, 0.0);
    const gp_Vec across = gp_Vec(way).Crossed(helper).Normalized() * (0.12 * size);
    const gp_Pnt back = tip.Translated(gp_Vec(way) * (-0.28 * size));
    lines.push_back({from, tip});
    lines.push_back({back.Translated(across), tip, back.Translated(across.Reversed())});
    return lines;
  };
  Q_UNUSED(context);
  return def;
}

} // namespace mitcad
