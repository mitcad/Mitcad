// SPDX-License-Identifier: MIT
// The edit panel of a derived part (mitcad#1812; app/COMMANDS.md, "Derived
// Component"): a BUILD part placed from a library, or a library part or a
// linked design inserted with options, opened by a double-click on its
// feature (timeline, Browser) and Edit. Its rows:
//
//   Source         the source in words; Open Source Design where the source
//                  can be opened: a linked design's file, else the
//                  library source the design keeps, read-only.
//   Configuration  the source's rows (the `configure` query's `rows`),
//                  shown when it has rows.
//   Length         a cut-to-length BUILD part's length: a value detaches a
//                  bound length (`build_edit`'s `length`).
//   <parameters>   the part's own values of the source's parameters (and an
//                  open table's options): expressions, as the component's
//                  options are written.
//   End A, End B   a cut-to-length part's cuts: angle and the axis it turns
//                  about.
//
// The feature is edited in place (CommandDef::editInPlace: the timeline is
// not rolled back), and OK runs one command, one undo step: `build_edit`
// for a BUILD part, `set_component_parameters` for another component; the
// model recomputes that part. Nothing changed: nothing is run.

#include "Commands.hpp"

#include <cmath>
#include <exception>
#include <limits>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QObject>
#include <QPair>
#include <QStringList>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../build/BuildModel.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Json.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

using cmd::Choices;

const QString kPart = QStringLiteral("part");
const QString kSource = QStringLiteral("source");
const QString kConfig = QStringLiteral("config");
const QString kLength = QStringLiteral("length");
const QStringList kEnds = {QStringLiteral("a"), QStringLiteral("b")};
// The parameters' rows: as many as a source's parameters a part sets
// usually are (a profile's length, a board's width and colour); more are
// left to Edit Options.
constexpr int kSlots = 8;
constexpr double kNaN = std::numeric_limits<double>::quiet_NaN();

// The axis an end's cut turns about (`derived_part`'s `tilt`); the labels
// are BUILD Ends' (translated as choices).
const Choices kTilts = {{QStringLiteral("x"), QStringLiteral("About x")},
                        {QStringLiteral("-x"), QStringLiteral("About x, the other way")},
                        {QStringLiteral("y"), QStringLiteral("About y")},
                        {QStringLiteral("-y"), QStringLiteral("About y, the other way")}};

// A member placed Along an edge (mitcad#1814): its edge frame's edge, face
// and offsets (`build_edit`'s `along`) and its lie.
const QString kAlongEdge = QStringLiteral("along_edge");
const QString kAlongFace = QStringLiteral("along_face");
const QString kLie = QStringLiteral("lie");
// The offsets: the input and the edge frame's slot.
const QList<QPair<QString, QString>> kOffsets = {{QStringLiteral("along_inset"), QStringLiteral("inset")},
                                                 {QStringLiteral("along_start"), QStringLiteral("start")},
                                                 {QStringLiteral("along_end"), QStringLiteral("end")},
                                                 {QStringLiteral("along_lift"), QStringLiteral("lift")}};
const Choices kLies = {{QStringLiteral("flat"), QStringLiteral("Flat")},
                       {QStringLiteral("edge"), QStringLiteral("On edge")}};

QString slotKey(int index) { return QStringLiteral("param%1").arg(index); }
QString angleKey(const QString& end) { return end + QStringLiteral("_angle"); }
QString tiltKey(const QString& end) { return end + QStringLiteral("_tilt"); }

// The part the panel edits, as `load` read it (the state's object `part`):
// `uid`, `component`, `name`, `build` (a BUILD part), `cut` (cut to
// length), `length_parameter`, `source` (in words), `link` (a linked
// design's path, empty otherwise), `rows`, `parameters` (the names of the
// parameter rows, in order) and `start` (each input's text or choice as
// loaded, to send only what changed).
QJsonObject partOf(const CommandState& state) { return state.object(kPart); }

QStringList names(const QJsonValue& value) {
  QStringList list;
  for (const QJsonValue& entry : value.toArray()) {
    list << entry.toString();
  }
  return list;
}

// The component that holds a feature, from the `components` query.
QJsonObject componentOf(const QString& feature, const CommandContext& context) {
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
      const QJsonObject component = value.toObject();
      if (component.value(QStringLiteral("features")).toArray().contains(QJsonValue(feature))) {
        return component;
      }
    }
  } catch (const std::exception&) {
    // None the model can tell.
  }
  return {};
}

// The BUILD part of a component (`build_parts`), empty for another one.
QJsonObject buildPartOf(const QString& component, const CommandContext& context) {
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("build_parts")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("parts")).toArray()) {
      if (str(value.toObject(), "component") == component) {
        return value.toObject();
      }
    }
  } catch (const std::exception&) {
    // Not a BUILD design.
  }
  return {};
}

// A library source of the design (`library_parts`' entry with `source`).
QJsonObject sourceEntry(const QString& source, const CommandContext& context) {
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("library_parts")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("parts")).toArray()) {
      if (str(value.toObject(), "source") == source) {
        return value.toObject();
      }
    }
  } catch (const std::exception&) {
    // None.
  }
  return {};
}

// The source in words: "Profile 20x20 from the library Profiles, version
// v1.0.0", "the linked design shelf.mitcad".
QString sourceText(const QJsonObject& entry, const QString& link) {
  if (!link.isEmpty()) {
    return QObject::tr("The linked design %1").arg(link.section(QLatin1Char('/'), -1));
  }
  if (entry.isEmpty()) {
    return QObject::tr("A component kept in this design");
  }
  const QJsonObject library = entry.value(QStringLiteral("library")).toObject();
  const QString libraryName = str(library, "library");
  const QString version = str(library, "label");
  return version.isEmpty() ? QObject::tr("%1 from the library %2").arg(str(entry, "name"), libraryName)
                           : QObject::tr("%1 from the library %2, version %3").arg(str(entry, "name"), libraryName, version);
}

// The text of an input as loaded, and as it is now.
QString startText(const CommandState& state, const QString& key) {
  return partOf(state).value(QStringLiteral("start")).toObject().value(key).toString();
}
bool changed(const CommandState& state, const QString& key, const QString& now) {
  return now.trimmed() != startText(state, key).trimmed();
}

// An end's cut as `build_edit` takes it: null squares the end.
QJsonValue endCut(const CommandState& state, const QString& end) {
  const double angle = state.value(angleKey(end));
  const QString expression = state.expression(angleKey(end));
  if (expression.isEmpty() || (std::isfinite(angle) && std::abs(angle) < 1e-12)) {
    return QJsonValue(QJsonValue::Null);
  }
  return QJsonObject{{QStringLiteral("angle"), expression}, {QStringLiteral("tilt"), state.choice(tiltKey(end))}};
}

void load(const QJsonObject& def, CommandState& state, const CommandContext& context) {
  const QString uid = str(def, "uid");
  const QJsonObject component = componentOf(uid, context);
  const QString componentUid = str(component, "uid");
  const QJsonObject placed = buildPartOf(componentUid, context);
  const QJsonObject entry = sourceEntry(str(def, "source"), context);
  // A linked design inserted with options; a library part has a record,
  // a BUILD part a library source.
  const QString link = placed.isEmpty() && !component.value(QStringLiteral("library")).isObject()
                           ? str(component, "link")
                           : QString();
  QJsonObject configure;
  try {
    configure = context.queryObject({{QStringLiteral("query"), QStringLiteral("configure")},
                                     {QStringLiteral("component"), componentUid}});
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Derived component %1: its options not read: %2")
                                .arg(componentUid, errorText(error));
  }
  const QStringList rows = names(configure.value(QStringLiteral("rows")));
  QString row = str(def, "config");
  if (row.isEmpty()) {
    row = str(configure, "default_row");
  }
  // A cut-to-length BUILD part's length is its own row.
  const QString lengthParameter = placed.isEmpty() ? QString()
                                                  : str(entry.value(QStringLiteral("build"))
                                                            .toObject()
                                                            .value(QStringLiteral("length"))
                                                            .toObject(),
                                                        "parameter");
  const bool cut = !placed.isEmpty() && !lengthParameter.isEmpty();
  // The parameters the part sets, then an open table's options it does not
  // (a source with rows is chosen by its row).
  const QJsonObject values = def.value(QStringLiteral("parameters")).toObject();
  QStringList parameters;
  for (auto it = values.constBegin(); it != values.constEnd(); ++it) {
    if (it.key() != lengthParameter) {
      parameters << it.key();
    }
  }
  QJsonObject optionExpressions;
  if (rows.isEmpty()) {
    for (const QJsonValue& value : configure.value(QStringLiteral("options")).toArray()) {
      const QJsonObject option = value.toObject();
      const QString name = str(option, "name");
      if (!name.isEmpty() && name != lengthParameter && !parameters.contains(name)) {
        parameters << name;
        optionExpressions.insert(name, str(option, "expression"));
      }
    }
  }
  while (parameters.size() > kSlots) {
    parameters.removeLast();
  }

  QJsonObject start;
  state.setChoice(kConfig, row);
  start.insert(kConfig, row);
  if (cut) {
    const double length = build::lengthOf(placed.value(QStringLiteral("length")));
    const QString text = std::isfinite(length) ? cmd::lengthText(length, context)
                                           : context.valueText(values.value(lengthParameter), uid);
    state.setText(kLength, text);
    state.setValue(kLength, kNaN);
    start.insert(kLength, text);
    const QJsonObject ends = def.value(QStringLiteral("ends")).toObject();
    for (const QString& end : kEnds) {
      const QJsonObject cutAt = ends.value(end).toObject();
      const QString angle = cutAt.isEmpty() ? QStringLiteral("0 deg") : context.valueText(cutAt.value(QStringLiteral("angle")), uid);
      const QString tilt = str(cutAt, "tilt").isEmpty() ? QStringLiteral("x") : str(cutAt, "tilt");
      state.setText(angleKey(end), angle);
      state.setValue(angleKey(end), kNaN);
      state.setChoice(tiltKey(end), tilt);
      start.insert(angleKey(end), angle);
      start.insert(tiltKey(end), tilt);
    }
  }
  for (int i = 0; i < parameters.size(); ++i) {
    const QString name = parameters.at(i);
    const QString text = values.contains(name) ? context.valueText(values.value(name), uid)
                                               : optionExpressions.value(name).toString();
    state.setText(slotKey(i), text);
    start.insert(slotKey(i), text);
  }
  // A member placed Along an edge (mitcad#1814): its edge frame's edge and
  // face as picks (re-pickable), its offsets and its lie.
  const QJsonObject along = placed.value(QStringLiteral("along")).toObject();
  const QString frames = str(along, "feature");
  if (!frames.isEmpty()) {
    QJsonObject frameDef;
    try {
      frameDef = context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), frames}})
                     .value(QStringLiteral("def"))
                     .toObject();
    } catch (const std::exception& error) {
      qWarning().noquote() << QStringLiteral("Derived component %1: its edge frame not read: %2")
                                  .arg(componentUid, errorText(error));
    }
    const QString occurrence = build::rootPath(str(frameDef, "occurrence"), context);
    const auto item = [&occurrence](SelectKind kind, const QJsonObject& reference, const char* key) {
      SelectionItem picked;
      picked.kind = kind;
      picked.owner = str(reference, "body");
      picked.name = str(reference, key);
      picked.geometry = kind == SelectKind::Edge ? QStringLiteral("line") : QStringLiteral("plane");
      picked.occurrence = occurrence;
      return picked;
    };
    const SelectionItem edge = item(SelectKind::Edge, frameDef.value(QStringLiteral("edge")).toObject(), "edge");
    const SelectionItem face = item(SelectKind::Face, frameDef.value(QStringLiteral("face")).toObject(), "face");
    state.setItems(kAlongEdge, {edge});
    state.setItems(kAlongFace, {face});
    start.insert(kAlongEdge, edge.logDescription());
    start.insert(kAlongFace, face.logDescription());
    for (const auto& [key, slot] : kOffsets) {
      const QJsonValue value = frameDef.value(slot);
      const QString text = value.isUndefined() || value.isNull() ? cmd::lengthText(0.0, context)
                                                                 : context.valueText(value, frames);
      state.setText(key, text);
      state.setValue(key, kNaN);
      start.insert(key, text);
    }
    const QString lie = str(along, "lie").isEmpty() ? QStringLiteral("flat") : str(along, "lie");
    state.setChoice(kLie, lie);
    start.insert(kLie, lie);
  }
  const QString name = placed.isEmpty() ? str(component, "name")
                                       : (str(placed, "name").isEmpty() ? str(placed, "designation") : str(placed, "name"));
  // What the panel opened with, for the log (UI tests): load also runs for
  // the feature's hover text, so the inspection logs it.
  const QString summary =
      QStringLiteral("%1 (%2), %3, rows %4, row %5, length %6, parameters %7, source %8")
          .arg(name, componentUid, placed.isEmpty() ? QStringLiteral("options") : QStringLiteral("BUILD"),
               rows.isEmpty() ? QStringLiteral("none") : rows.join(QLatin1Char('|')),
               row.isEmpty() ? QStringLiteral("none") : row, cut ? state.text(kLength) : QStringLiteral("none"),
               parameters.isEmpty() ? QStringLiteral("none") : parameters.join(QLatin1Char('|')),
               link.isEmpty() ? (str(def, "source").isEmpty() ? QStringLiteral("not opened") : QStringLiteral("opens read-only"))
                              : QStringLiteral("opens its file")) +
      (frames.isEmpty() ? QString()
                        : QStringLiteral(", along an edge: inset %1, start %2, end %3, lift %4, lie %5")
                              .arg(state.text(kOffsets.at(0).first), state.text(kOffsets.at(1).first),
                                   state.text(kOffsets.at(2).first), state.text(kOffsets.at(3).first),
                                   state.choice(kLie)));
  const QJsonObject part{{QStringLiteral("uid"), uid},
                         {QStringLiteral("component"), componentUid},
                         {QStringLiteral("name"), name},
                         {QStringLiteral("build"), !placed.isEmpty()},
                         {QStringLiteral("cut"), cut},
                         {QStringLiteral("along"), !frames.isEmpty()},
                         {QStringLiteral("length_parameter"), lengthParameter},
                         {kSource, sourceText(entry, link)},
                         {QStringLiteral("link"), link},
                         {QStringLiteral("source_id"), str(def, "source")},
                         {QStringLiteral("rows"), QJsonArray::fromStringList(rows)},
                         {QStringLiteral("parameters"), QJsonArray::fromStringList(parameters)},
                         {QStringLiteral("summary"), summary},
                         {QStringLiteral("start"), start}};
  state.setObject(kPart, part);
}

// What OK sends: the command, empty when nothing changed; `what` the
// changed fields for the log.
QJsonObject editCommand(const CommandState& state, const CommandContext& context, QStringList& what) {
  const QJsonObject part = partOf(state);
  const bool buildPart = part.value(QStringLiteral("build")).toBool();
  QJsonObject command{{QStringLiteral("cmd"), buildPart ? QStringLiteral("build_edit") : QStringLiteral("set_component_parameters")},
                      {QStringLiteral("component"), str(part, "component")}};
  if (!names(part.value(QStringLiteral("rows"))).isEmpty() && changed(state, kConfig, state.choice(kConfig))) {
    command.insert(kConfig, state.choice(kConfig));
    what << QStringLiteral("row %1").arg(state.choice(kConfig));
  }
  const bool alongEdge = part.value(QStringLiteral("along")).toBool();
  // Along an edge (mitcad#1814): the edge frame's changes and the lie; the
  // edge and its offsets give the length.
  if (alongEdge) {
    QJsonObject along;
    const Selection& edges = state.items(kAlongEdge);
    const Selection& faces = state.items(kAlongFace);
    const bool newEdge = !edges.isEmpty() && changed(state, kAlongEdge, edges.first().logDescription());
    const bool newFace = !faces.isEmpty() && changed(state, kAlongFace, faces.first().logDescription());
    if (newEdge) {
      along.insert(QStringLiteral("edge"), edges.first().reference());
      what << QStringLiteral("edge %1").arg(edges.first().logDescription());
    }
    if (newFace) {
      along.insert(QStringLiteral("face"), faces.first().reference());
      what << QStringLiteral("face %1").arg(faces.first().logDescription());
    }
    if ((newEdge || newFace) && !faces.isEmpty()) {
      if (const std::optional<QString> path = build::pathFromHost(faces.first(), context)) {
        along.insert(QStringLiteral("occurrence"), *path);
      }
    }
    for (const auto& [key, slot] : kOffsets) {
      if (changed(state, key, state.text(key))) {
        along.insert(slot, state.expression(key));
        what << QStringLiteral("%1 %2").arg(slot, state.expression(key));
      }
    }
    if (!along.isEmpty()) {
      command.insert(QStringLiteral("along"), along);
    }
    if (changed(state, kLie, state.choice(kLie))) {
      command.insert(kLie, state.choice(kLie));
      what << QStringLiteral("lie %1").arg(state.choice(kLie));
    }
  }
  if (part.value(QStringLiteral("cut")).toBool()) {
    if (!alongEdge && changed(state, kLength, state.text(kLength))) {
      command.insert(kLength, state.expression(kLength));
      what << QStringLiteral("length %1").arg(state.expression(kLength));
    }
    QJsonObject ends;
    for (const QString& end : kEnds) {
      if (changed(state, angleKey(end), state.text(angleKey(end))) ||
          changed(state, tiltKey(end), state.choice(tiltKey(end)))) {
        ends.insert(end, endCut(state, end));
        what << QStringLiteral("end %1 %2 %3").arg(end, state.expression(angleKey(end)), state.choice(tiltKey(end)));
      }
    }
    if (!ends.isEmpty()) {
      command.insert(QStringLiteral("ends"), ends);
    }
  }
  const QStringList parameters = names(part.value(QStringLiteral("parameters")));
  QJsonObject values;
  for (int i = 0; i < parameters.size(); ++i) {
    const QString text = state.text(slotKey(i)).trimmed();
    if (!text.isEmpty() && changed(state, slotKey(i), text)) {
      values.insert(parameters.at(i), text);
      what << QStringLiteral("%1 = %2").arg(parameters.at(i), text);
    }
  }
  if (!values.isEmpty()) {
    command.insert(QStringLiteral("parameters"), values);
  }
  return what.isEmpty() ? QJsonObject() : command;
}

CommandDef derivedPartCommand(DocumentHost& host) {
  CommandDef def;
  def.id = QStringLiteral("assemble.derived_component");
  def.name = QObject::tr("Derived Component");
  def.icon = QStringLiteral("build-length");
  def.tooltip = QObject::tr("A placed or inserted component's configuration, length, values and end cuts");
  def.tab.clear(); // opened by editing its feature
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("configuration"), QStringLiteral("length"), QStringLiteral("options"),
                  QStringLiteral("build"), QStringLiteral("profile")};
  def.featureType = QStringLiteral("derived_part");
  def.editInPlace = true;
  // Its load asks the model four queries: not for the hover text.
  def.loadForHover = false;
  // Nothing to start without a part: only its feature's edit opens it.
  def.enabled = [] { return false; };
  const auto cut = [](const CommandState& state) { return partOf(state).value(QStringLiteral("cut")).toBool(); };
  def.inputs = {
      noteInput(
          kSource,
          [](const CommandState& state) {
            InputNote note;
            const QJsonObject part = partOf(state);
            note.text = str(part, "source");
            if (!str(part, "link").isEmpty() || !str(part, "source_id").isEmpty()) {
              note.actions << QObject::tr("Open Source Design");
            }
            return note;
          },
          [&host](int action, const CommandState& state, const CommandContext&) {
            const QJsonObject part = partOf(state);
            if (action != 0) {
              return;
            }
            qInfo().noquote() << QStringLiteral("Derived component: Open Source Design of %1").arg(str(part, "component"));
            // A linked design's own file; else the source the design keeps.
            if (!str(part, "link").isEmpty()) {
              host.openLinkedDesign(str(part, "component"));
            } else if (!str(part, "source_id").isEmpty()) {
              host.openLibrarySource(str(part, "source_id"));
            }
          },
          QObject::tr("Source")),
      choiceInput(kConfig, QObject::tr("Configuration"), {}, QString())
          .withChoices([](const CommandState& state) {
            Choices choices;
            for (const QString& row : names(partOf(state).value(QStringLiteral("rows")))) {
              choices.append({row, row});
            }
            return choices;
          })
          .withVisible([](const CommandState& state) { return !names(partOf(state).value(QStringLiteral("rows"))).isEmpty(); })
          .withTooltip(QObject::tr("The source's configuration row the component is made in")),
      valueInput(kLength, QObject::tr("Length"), ValueKind::Length, QStringLiteral("500 mm"))
          .withVisible([](const CommandState& state) {
            return partOf(state).value(QStringLiteral("cut")).toBool() && !partOf(state).value(QStringLiteral("along")).toBool();
          })
          .withTooltip(QObject::tr("The component's length; a length bound to another component becomes this number")),
  };
  // A member placed Along an edge (mitcad#1814): its edge and face picked
  // again (a face clicked in Edge goes to Face and gives Edge its straight
  // edge nearest to the click), its offsets and its lie.
  const auto along = [](const CommandState& state) { return partOf(state).value(QStringLiteral("along")).toBool(); };
  def.inputs.append(selectionInput(kAlongEdge, QObject::tr("Edge"), SelectKind::Edge | SelectKind::Face, 1, 1)
                        .withVisible(along)
                        .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                          if (item.kind != SelectKind::Face) {
                            return;
                          }
                          state.setItems(kAlongFace, {item});
                          build::recordPick(kAlongFace, item, state, context);
                          const std::optional<SelectionItem> edge =
                              build::nearestStraightEdge(item, kAlongFace, state, context);
                          state.setItems(kAlongEdge, edge ? Selection{*edge} : Selection{});
                        },
                                    false)
                        .withTooltip(
                            QObject::tr("The straight edge the member runs along; a clicked face offers its nearest edge")));
  def.inputs.append(selectionInput(kAlongFace, QObject::tr("Face"), SelectKind::Face, 1, 1)
                        .withVisible(along)
                        .withTooltip(QObject::tr("The face the member lies on, beside the edge: it decides which side "
                                                 "of the edge the member is on")));
  const QStringList offsetLabels = {QObject::tr("Inset"), QObject::tr("Start"), QObject::tr("End"), QObject::tr("Lift")};
  for (int i = 0; i < kOffsets.size(); ++i) {
    def.inputs.append(valueInput(kOffsets.at(i).first, offsetLabels.at(i), ValueKind::Length, QStringLiteral("0 mm"))
                          .withVisible(along));
  }
  def.inputs.append(choiceInput(kLie, QObject::tr("Lie"), kLies, QStringLiteral("flat"))
                        .withVisible(along)
                        .withTabCycle({QStringLiteral("flat"), QStringLiteral("edge")})
                        .withTooltip(QObject::tr("Flat: the thinner side across the plane it lies on; on edge: the "
                                                 "wider (Tab turns it)")));
  for (int i = 0; i < kSlots; ++i) {
    def.inputs.append(
        textInput(slotKey(i), QString())
            .withLabel([i](const CommandState& state) {
              const QStringList parameters = names(partOf(state).value(QStringLiteral("parameters")));
              return i < parameters.size() ? parameters.at(i) : QString();
            })
            .withVisible([i](const CommandState& state) {
              return i < names(partOf(state).value(QStringLiteral("parameters"))).size();
            })
            .withTooltip(QObject::tr("The source's value: a constant, or a formula of this design's parameters")));
  }
  def.inputs.append(valueInput(angleKey(QStringLiteral("a")), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                        .withHeading(QObject::tr("End A"))
                        .withVisible(cut)
                        .withTooltip(QObject::tr("From square, 0 up to 90 degrees; 0 is a square end")));
  def.inputs.append(choiceInput(tiltKey(QStringLiteral("a")), QObject::tr("About"), kTilts, QStringLiteral("x")).withVisible(cut));
  def.inputs.append(valueInput(angleKey(QStringLiteral("b")), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                        .withHeading(QObject::tr("End B"))
                        .withVisible(cut)
                        .withTooltip(QObject::tr("From square, 0 up to 90 degrees; 0 is a square end")));
  def.inputs.append(choiceInput(tiltKey(QStringLiteral("b")), QObject::tr("About"), kTilts, QStringLiteral("x")).withVisible(cut));
  def.load = load;
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const QJsonObject part = partOf(state);
    if (part.isEmpty()) {
      result.error = QObject::tr("Edit a placed or inserted component's feature to change it here.");
      return result;
    }
    QStringList what;
    const bool changes = !editCommand(state, context, what).isEmpty();
    QStringList lines{QStringLiteral("<b>%1</b>").arg(str(part, "name").toHtmlEscaped())};
    lines << (changes ? QObject::tr("Changes: %1").arg(what.join(QStringLiteral(", ")).toHtmlEscaped())
                      : QObject::tr("Nothing changed yet"));
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("derived component %1; changes: %2")
                     .arg(str(part, "summary"), changes ? what.join(QStringLiteral(", ")) : QStringLiteral("none"));
    return result;
  };
  def.apply = [&host](CommandState& state, const CommandContext& context) {
    const QJsonObject part = partOf(state);
    QStringList what;
    const QJsonObject command = editCommand(state, context, what);
    if (command.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Derived component %1: nothing changed").arg(str(part, "component"));
      return true;
    }
    if (!host.runModelCommand(command)) {
      return false; // the model's refusal is shown; the panel stays
    }
    qInfo().noquote() << QStringLiteral("Derived component %1 edited: %2")
                             .arg(str(part, "component"), what.join(QStringLiteral(", ")));
    return true;
  };
  return def;
}

} // namespace

void registerDerivedPartCommands(CommandRegistry& registry, DocumentHost& host) {
  registry.add(derivedPartCommand(host));
}

} // namespace mitcad
