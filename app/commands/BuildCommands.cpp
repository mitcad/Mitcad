// SPDX-License-Identifier: MIT
// The BUILD tab (mitcad#375; the model's side in commands.md, "BUILD:
// profile assemblies"; app/COMMANDS.md, "BUILD"): frames of slot profiles
// with their connectors, panels and linear parts.
//
//   PROFILE  Profile, Panel, Linear Component: the part as a tile that opens
//            the part picker, then clicks; Place (Enter) keeps the panel
//            open for the next one.
//   TIMBER   Timber (loose members), Wall, Floor / Roof (mitcad#520).
//   CONNECT  Connector (a junction, the fitting connectors), Fasteners.
//   MODIFY   Length (bind, detach), Ends (mitres), Flip, Rotate 90 deg,
//            Spacing Set, Timber Frame.
//   OUTPUT   Cut List, Parts List.
//
// Everything goes through the model's BUILD commands and queries
// (build_place, build_edit, build_connect, build_panel, build_remove;
// build_preview, build_parts, build_junctions, build_cut_list) and the
// library commands (library_build_candidates, library_search,
// library_show): the commands are ordinary panels (apply with inspect as
// their preview, CommandDef), so live MCP fills them as any other.
#include "Commands.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <limits>
#include <memory>
#include <mutex>
#include <optional>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QMap>
#include <QPointer>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopoDS_Compound.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "../browser/DocumentHost.hpp"
#include "../build/BuildCatalog.hpp"
#include "../build/BuildLists.hpp"
#include "../build/BuildModel.hpp"
#include "../build/OpeningsWindow.hpp"
#include "../build/TimberOpenings.hpp"
#include "../files/Libraries.hpp"
#include "../files/LibraryAutoUpdate.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Theme.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../framework/LengthUnits.hpp"
#include "../framework/ModelShapes.hpp"
#include "../library/PartFavourites.hpp"
#include "../library/PartPicker.hpp"
#include "../library/UsedParts.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

using build::BuildPart;
using build::Part;
using cmd::Choices;

const QString kTab = QStringLiteral("BUILD");
constexpr double kNaN = std::numeric_limits<double>::quiet_NaN();
constexpr double kPi = 3.14159265358979323846;
constexpr SelectFilter kPartKinds = SelectKind::Face | SelectKind::Body;
// A member picked to name it (its frame, its set, the walls a roof stands
// on): a click on a face or its body; an edge of it selected before the
// command (the view's context menu offers its BUILD commands for an edge
// as for a face) is taken as its body (InputDef::alsoTakes, mitcad#522),
// wherever it is placed (a frame's own component). The view picks faces
// and bodies only, so a narrow face is not lost to its edges.
SelectionItem memberBody(const SelectionItem& item) {
  if (item.kind != SelectKind::Edge) {
    return item;
  }
  SelectionItem body;
  body.kind = SelectKind::Body;
  body.owner = item.owner;
  body.occurrence = item.occurrence;
  return body;
}
InputDef memberInput(const QString& id, const QString& label, int min, int max) {
  return selectionInput(id, label, kPartKinds, min, max).withAlsoTakes(SelectKind::Edge).withConvert(memberBody);
}

// What the definitions share while the application runs: the parts of the
// pickers, the junctions, the design's BUILD parts, the preview shown last
// (the length's handle stands on it). Read and written on the UI thread;
// an inspection may read it on the model's worker (the AppKit frontend).
struct Shared {
  build::Catalog catalog;
  mutable std::mutex mutex;
  // The preview shown last: the new part's frame and length.
  build::Frame frame;
  double length = kNaN;
  bool bound = false;
  // build_junctions, and each junction's label for the choice.
  QJsonArray junctions;
  Choices junctionChoices;
  // The design's BUILD parts and which component each occurrence path places.
  QVector<BuildPart> parts;
  QHash<QString, QString> components;
  // Where each occurrence path places its component in the design (the
  // Timber tool's snaps to the members, mitcad#478).
  QHash<QString, gp_Trsf> worlds;
  // The part chosen or placed last in this design, per role: the picker
  // starts with it while the design has parts of the role (mitcad#452);
  // `design` is the design they were chosen in (DocumentHost::designId),
  // and another design starts without them.
  QHash<QString, QString> lastPart;
  QString design;
  // The system of the part chosen last, per role (mitcad#427): the part
  // picker opens filtered to it.
  QHash<QString, QString> lastSystem;
  // How the Timber tool placed last (mitcad#478): its next start's.
  QString timberHow;
  // What the TIMBER group's next starts take (mitcad#485, mitcad#520).
  // Shared by Timber, Wall and Floor / Roof: the part chosen last (empty
  // the first time: a dimensioned 48x98, else the picker's first) and the
  // fastener of frames and spacing sets (mitcad#497: the wood fastener,
  // empty the default one, mitcad#516; its size, "auto" or a row; how many
  // at each junction). Each tool's own: Timber's length typed last (empty:
  // the part's default), a ground placement's direction and lie, whether a
  // set is fastened (off the first time); Wall's Fasteners (on) and Corner
  // (mitcad#510: "butt" or "lapped"); Floor / Roof's kind (a floor the
  // first time) and Fasteners (on).
  struct TimberMemory {
    QString length;
    QString axis = QStringLiteral("x");
    QString lie = QStringLiteral("flat");
    QString part;
    QString fastener;
    QString fastenerSize = QStringLiteral("auto");
    QString fastenerCount = QStringLiteral("2");
    bool setFasteners = false;
    bool wallFasteners = true;
    QString wallCorner = QStringLiteral("butt");
    QString floorRoofKind = QStringLiteral("floor");
    bool floorRoofFasteners = true;
  };
  TimberMemory timber;
  // The configuration rows of a source's component (Fasteners' sizes).
  QHash<QString, Choices> rows;
  // The library's preview image of a part, by "<library>|<component>",
  // read once (library_preview); null when it has none.
  QHash<QString, QImage> thumbnails;
  // The side placement shown last (mitcad#417): its reference and offset
  // from it, its width along the slot and the target's length. A change of
  // From keeps the part where this puts it.
  struct Side {
    QString from;
    double offset = kNaN;
    double width = kNaN;
    double length = kNaN;
    // The offset is to a tilted part's foot long point (mitcad#444), not
    // to its faces.
    bool longPoint = false;
    // A set's members as the model lays them out from the offset
    // (mitcad#492); -1 without a set.
    int setCount = -1;
  };
  Side side;
  // Where the part shown last stands on a slot (mitcad#444): the foot's
  // long point, the slot's normal and its direction along the part it is
  // on; the tilt's handle turns about their cross. Invalid off a side.
  struct Foot {
    bool valid = false;
    gp_Pnt point;
    gp_Dir normal;
    gp_Dir along;
  };
  Foot foot;
  // The part picker (mitcad#418): the main window it belongs to, the host
  // its choices go through and the picker once wired. UI thread only.
  QWidget* window = nullptr;
  DocumentHost* host = nullptr;
  QPointer<PartPicker> picker;
  // The Openings window (mitcad#519), wired once like the picker, and the
  // sizes the frame shown last was laid out with: the window checks its
  // rows against them. Written by the inspections (on the model's worker
  // with the AppKit frontend), read on the UI thread.
  QPointer<OpeningsWindow> openings;
  struct OpeningsSizes {
    build::WallSize wall;
    double floorLength = kNaN;
    double floorWidth = kNaN;
  };
  OpeningsSizes openingSizes;

  void setOpeningsSizes(const OpeningsSizes& shown) {
    const std::lock_guard<std::mutex> lock(mutex);
    openingSizes = shown;
  }
  OpeningsSizes openingsSizes() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return openingSizes;
  }

  void setSide(const Side& shown) {
    const std::lock_guard<std::mutex> lock(mutex);
    side = shown;
  }
  Side shownSide() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return side;
  }
  void setFoot(const Foot& shown) {
    const std::lock_guard<std::mutex> lock(mutex);
    foot = shown;
  }
  Foot shownFoot() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return foot;
  }

  QImage thumbnail(const Part& part) {
    // A row with a picture of its own (mitcad#484) by its row.
    const QString row = part.ownPreview ? part.config : QString();
    const QString key = part.library + QLatin1Char('|') + part.component + QLatin1Char('|') + row;
    {
      const std::lock_guard<std::mutex> lock(mutex);
      const auto found = thumbnails.constFind(key);
      if (found != thumbnails.cend()) {
        return found.value();
      }
    }
    QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("library_preview")},
                        {QStringLiteral("rev"), part.rev},
                        {QStringLiteral("component"), part.component}};
    if (!row.isEmpty()) {
      request.insert(QStringLiteral("config"), row);
    }
    if (!part.url.isEmpty()) {
      request.insert(QStringLiteral("url"), part.url);
    } else {
      request.insert(QStringLiteral("id"), part.library);
    }
    const QJsonObject answer = libraryCommand(request);
    QImage image;
    const QJsonValue png = answer.value(QStringLiteral("png"));
    if (png.isString()) {
      image = QImage::fromData(QByteArray::fromBase64(png.toString().toLatin1()), "PNG");
    }
    qInfo().noquote() << QStringLiteral("BUILD preview image of %1/%2%3: %4")
                             .arg(part.library, part.component,
                                  row.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(row),
                                  image.isNull() ? QStringLiteral("none")
                                                 : QStringLiteral("%1x%2").arg(image.width()).arg(image.height()));
    const std::lock_guard<std::mutex> lock(mutex);
    thumbnails.insert(key, image);
    return image;
  }

  void setPreview(const build::Frame& shown, double shownLength, bool shownBound) {
    const std::lock_guard<std::mutex> lock(mutex);
    frame = shown;
    length = shownLength;
    bound = shownBound;
  }
  std::pair<build::Frame, double> preview() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return {frame, length};
  }
  // Whether the preview shown last had its length bound (an auto length
  // with a target): then the length has no handle.
  bool previewBound() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return bound;
  }
  // Nothing previewed (the tool waits for a pick, or the model could not
  // place the part): no handle stays where the part shown before was, such
  // as the length's at the free end of a part just placed, which would take
  // a click on that end (mitcad#1105) though nothing is shown there.
  void clearPreview() {
    const std::lock_guard<std::mutex> lock(mutex);
    frame = build::Frame();
    length = kNaN;
    bound = false;
    foot = Foot();
  }
  // Whether the design has parts a part of `role` goes on (mitcad#394):
  // profiles for a profile, profiles and linear parts (a rail for its
  // carriage) for a linear part. Without them a placement is a ground
  // placement.
  bool hasTargets(const QString& role) const {
    const std::lock_guard<std::mutex> lock(mutex);
    return std::any_of(parts.begin(), parts.end(), [&role](const BuildPart& part) {
      return targets(role, part.role);
    });
  }
  static bool targets(const QString& role, const QString& target) {
    return target == QStringLiteral("profile") ||
           (role == QStringLiteral("linear") && target == QStringLiteral("linear"));
  }
  QJsonObject junction(const QString& id) const {
    const std::lock_guard<std::mutex> lock(mutex);
    for (const QJsonValue& value : junctions) {
      if (str(value.toObject(), "id") == id) {
        return value.toObject();
      }
    }
    return {};
  }
  Choices junctionOptions() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return junctionChoices;
  }
  QVector<BuildPart> designParts() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return parts;
  }
  // The BUILD part an item's occurrence path places (the innermost).
  std::optional<BuildPart> partAt(const QString& occurrence) const {
    const std::lock_guard<std::mutex> lock(mutex);
    QStringList steps = occurrence.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    while (!steps.isEmpty()) {
      const QString component = components.value(steps.join(QLatin1Char('/')));
      for (const BuildPart& part : parts) {
        if (part.component == component) {
          return part;
        }
      }
      steps.removeLast();
    }
    return std::nullopt;
  }

  // Reads the junctions and the design's parts again (a command starts, a
  // part was placed or connected).
  void refreshDesign(const CommandContext& context) {
    const QJsonArray found = build::junctions(context);
    Choices labels;
    for (const QJsonValue& value : found) {
      const QJsonObject entry = value.toObject();
      labels.append({str(entry, "id"), build::junctionText(entry, context)});
    }
    const QVector<BuildPart> designParts = build::buildParts(context);
    QHash<QString, QString> placed;
    QHash<QString, gp_Trsf> placements;
    try {
      std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& level, const QString& parent) {
        for (const QJsonValue& value : level) {
          const QJsonObject node = value.toObject();
          const QString path = parent.isEmpty() ? str(node, "uid") : parent + QLatin1Char('/') + str(node, "uid");
          placed.insert(path, str(node, "component"));
          placements.insert(path, trsfOf(node.value(QStringLiteral("world")).toArray()));
          walk(node.value(QStringLiteral("children")).toArray(), path);
        }
      };
      walk(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
               .value(QStringLiteral("occurrences"))
               .toArray(),
           QString());
    } catch (const std::exception&) {
      // No occurrences then.
    }
    const std::lock_guard<std::mutex> lock(mutex);
    junctions = found;
    junctionChoices = labels;
    parts = designParts;
    components = placed;
    worlds = placements;
  }

  // Whether the design has timber members (mitcad#478), and slot profiles
  // (profiles that are no timber).
  bool hasTimber() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return std::any_of(parts.begin(), parts.end(), [](const BuildPart& part) { return part.isTimber(); });
  }
  bool hasSlotProfiles() const {
    const std::lock_guard<std::mutex> lock(mutex);
    return std::any_of(parts.begin(), parts.end(), [](const BuildPart& part) {
      return part.role == QStringLiteral("profile") && !part.isTimber();
    });
  }
  // The occurrence paths that place a component, and where.
  QVector<std::pair<QString, gp_Trsf>> placementsOf(const QString& component) const {
    const std::lock_guard<std::mutex> lock(mutex);
    QVector<std::pair<QString, gp_Trsf>> found;
    for (auto it = components.cbegin(); it != components.cend(); ++it) {
      if (it.value() == component) {
        found.append({it.key(), worlds.value(it.key())});
      }
    }
    return found;
  }
};

using SharedPtr = std::shared_ptr<Shared>;

// --- The picker

const QString kSearch = QStringLiteral("search");
const QString kSystem = QStringLiteral("system");
const QString kPart = QStringLiteral("part");

bool chosenPart(const Shared& shared, const CommandState& state, Part& part) {
  return shared.catalog.find(state.choice(kPart), part);
}

// A cut-to-length part starts with its default length (its stock length,
// else 500 mm, mitcad#394), unless a length was typed: the length shown
// follows the chosen part while it is the default one.
void followPart(const Shared& shared, CommandState& state, const CommandContext& context) {
  Part part;
  if (!chosenPart(shared, state, part) || !part.cutToLength()) {
    return;
  }
  const QString shown = cmd::lengthText(part.defaultLength(), context);
  const QString before = state.object(QStringLiteral("length_default")).value(QStringLiteral("text")).toString();
  const QString typed = state.text(QStringLiteral("length")).trimmed();
  if (typed.isEmpty() || typed == before) {
    state.setText(QStringLiteral("length"), shown);
    state.setValue(QStringLiteral("length"), kNaN);
    state.setObject(QStringLiteral("length_default"), {{QStringLiteral("text"), shown}});
  }
}

// The first part the search and the system leave, when the chosen one is
// not among them any more.
void followFilter(const Shared& shared, const QString& role, CommandState& state, const CommandContext& context) {
  const QVector<Part> parts = shared.catalog.parts(role);
  const Choices shown = build::partChoices(parts, state.text(kSearch), state.choice(kSystem), QString());
  const bool kept = std::any_of(shown.begin(), shown.end(),
                                [&state](const auto& option) { return option.first == state.choice(kPart); });
  if (!kept && !shown.isEmpty()) {
    state.setChoice(kPart, shown.first().first);
  }
  followPart(shared, state, context);
}

// --- The part picker (mitcad#418; app/COMMANDS.md, "Component picker")
//
// The parts a command takes as pictures, in a floating window of the main
// window: the Part tile (Connector's Connector; mitcad#427, the button
// beside the drop-down where a frontend has no tiles) opens it, a tile
// clicked there is chosen as the drop-down chooses it (the same key, so the
// preview follows at once), and it closes with the command unless pinned;
// a pinned picker that is open follows the next command that chooses
// parts. In this design reads `library_usage` (usedLibraryParts); the main
// window tells it when the design changed.

// The button beside the field that opens it.
const QString kButtonText = QStringLiteral("…");
QString buttonTip() {
  return QObject::tr("Opens the component picker: the components as pictures");
}

// The id and the picker's title of a role's command.
// Timber's pickers (mitcad#478): "timber" is the Timber command's, "sheet"
// Panel's on a member's face.
QString commandOf(const QString& role) {
  return role == QStringLiteral("profile")                                 ? QStringLiteral("build.profile")
         : role == QStringLiteral("panel") || role == QStringLiteral("sheet") ? QStringLiteral("build.panel")
         : role == QStringLiteral("timber")                                ? QStringLiteral("build.timber")
                                                                           : QStringLiteral("build.linear");
}
QString titleOf(const QString& role) {
  return role == QStringLiteral("profile")                                 ? QObject::tr("Profile")
         : role == QStringLiteral("panel") || role == QStringLiteral("sheet") ? QObject::tr("Panel")
         : role == QStringLiteral("timber")                                ? QObject::tr("Timber")
                                                                           : QObject::tr("Linear Component");
}
// What a placement goes on, in the panel's texts: a profile's side, a
// member's side (timber).
QString partWord(const QString& role) {
  return role == QStringLiteral("timber") ? QObject::tr("member") : QObject::tr("profile");
}

// The main window's picker, wired the first time it is met: its choice
// goes into the BUILD command it was asked for (DocumentHost::
// chooseInCommand). Without `create` only a picker that is there already.
PartPicker* partPicker(Shared& shared, bool create) {
  if (shared.window == nullptr || shared.host == nullptr) {
    return nullptr;
  }
  PartPicker* picker = nullptr;
  if (create) {
    picker = PartPicker::of(shared.window);
  } else {
    // The main window's own (PartPicker::of), also in a document frame.
    picker = shared.window->findChild<PartPicker*>(QString(), Qt::FindDirectChildrenOnly);
  }
  if (picker != nullptr && shared.picker.data() != picker) {
    shared.picker = picker;
    DocumentHost* host = shared.host;
    picker->setDesignParts([host] { return usedLibraryParts(host->model()); });
    // Libraries updated in the background at start-up (mitcad#576): an open
    // picker of a role lists the new versions. A closed one reads the
    // catalog when its command starts. `shared` lives as long as the
    // commands do, which outlive the picker (a child of the window).
    QObject::connect(&LibraryAutoUpdate::instance(), &LibraryAutoUpdate::updated, picker, [&shared, picker, host] {
      if (!picker->isVisible() || !picker->input().isEmpty() || picker->role().isEmpty()) {
        return;
      }
      host->whenIdle(picker, [&shared, picker, host] {
        const QString role = picker->role();
        if (!picker->isVisible() || role.isEmpty()) {
          return;
        }
        shared.catalog.refresh(role, host->model());
        picker->refreshParts(pickerParts(shared.catalog.parts(role)));
      });
    });
    QObject::connect(picker, &PartPicker::chosen, picker, [host, picker](const PickerPart& part, const QString& command) {
      if (!command.startsWith(QStringLiteral("build."))) {
        return; // another caller's
      }
      // The part, or another input it was asked for (a fastener, mitcad#497).
      const QString input = picker->input().isEmpty() ? kPart : picker->input();
      if (!host->chooseInCommand(command, input, part.key)) {
        qInfo().noquote() << QStringLiteral("BUILD picker: %1 not chosen, %2 does not run").arg(part.name, command);
      }
    });
  }
  return picker;
}

// The picker while it shows the parts of `command`, else null.
PartPicker* shownPicker(const Shared& shared, const QString& command) {
  PartPicker* picker = shared.picker.data();
  return picker != nullptr && picker->isVisible() && picker->command() == command ? picker : nullptr;
}

// An open, pinned picker: the command that starts takes it over.
PartPicker* pinnedPicker(Shared& shared) {
  PartPicker* picker = partPicker(shared, false);
  return picker != nullptr && picker->isVisible() && picker->isPinned() ? picker : nullptr;
}

// What a role's command asks the picker for: its catalog's parts (the
// design's sources and library_build_candidates of the role) with their
// keys, the part chosen and the panel's System. `command`: the command the
// choice goes into when the role has several (mitcad#520: Timber, Wall and
// Floor / Roof choose timber), else the role's own.
PickerRequest roleRequest(const Shared& shared, const QString& role, const CommandState& state,
                          const QString& command = QString()) {
  PickerRequest request;
  request.command = command.isEmpty() ? commandOf(role) : command;
  request.title = titleOf(role);
  request.parts = pickerParts(shared.catalog.parts(role));
  request.current = state.choice(kPart);
  request.system = state.choice(kSystem);
  request.role = role;
  return request;
}

// The part chosen (in the part picker, or live MCP's `part`) sets the
// hidden System (mitcad#427): the part's own system, unless it fits
// several; the picker opens filtered to it, also the next time.
void systemFromPart(Shared& shared, const QString& role, CommandState& state) {
  Part part;
  if (!chosenPart(shared, state, part)) {
    return;
  }
  const QString own = part.systemText();
  if (!own.isEmpty() && !own.contains(QLatin1Char(','))) {
    state.setChoice(kSystem, own);
  }
  shared.lastPart.insert(role, part.key);
  shared.lastSystem.insert(role, state.choice(kSystem));
  if (role == QStringLiteral("timber")) {
    shared.timber.part = part.key; // mitcad#485
  }
}

// The picker open for the command shows what its panel chose: the part
// and, with `system`, the system (set through live MCP's `system` or
// `search`; a part chosen in the picker leaves the picker's filter alone).
void syncPicker(const Shared& shared, const QString& role, const CommandState& state, bool system,
                const QString& command = QString()) {
  PartPicker* picker = shownPicker(shared, command.isEmpty() ? commandOf(role) : command);
  if (picker != nullptr && picker->input().isEmpty()) { // not a fastener's (mitcad#497)
    if (system) {
      picker->grid()->setSystem(state.choice(kSystem));
    }
    picker->setCurrent(state.choice(kPart));
  }
}

// The tile of the part chosen (mitcad#427): the library's picture of it,
// its name, and its supplier and part number (else its system) under it.
ChoiceTile partTile(Shared& shared, const CommandState& state) {
  Part part;
  if (!chosenPart(shared, state, part)) {
    return {QImage(), QObject::tr("No component"), QString()};
  }
  QString detail = QStringList{part.supplier, part.partNumber}.join(QLatin1Char(' ')).trimmed();
  if (detail.isEmpty()) {
    detail = part.systemText();
  }
  const QString title = part.config.isEmpty() ? part.name : QStringLiteral("%1 (%2)").arg(part.name, part.config);
  return {shared.thumbnail(part), title, detail};
}

// The role of a command's picker in a state: fixed, or Panel's, which
// lists sheets on a timber member's face (mitcad#478).
using RoleOf = std::function<QString(const CommandState&)>;

// `command`: the command the picker's choice goes into, when not the role's
// own (roleRequest).
QVector<InputDef> pickerInputs(const SharedPtr& shared, const RoleOf& roleOf, const QString& command = QString()) {
  // Search and System are not shown (mitcad#427: the part picker has
  // them); they stay as inputs for live MCP: Search chooses the first part
  // its words match, System the first part of that system.
  const auto hidden = [](const CommandState&) { return false; };
  return {
      textInput(kSearch, QObject::tr("Search"))
          .withVisible(hidden)
          .withTooltip(QObject::tr("Words of the component's name, size, system, slots, supplier or part number"))
          .withOnChange([shared, roleOf, command](CommandState& state, const CommandContext& context) {
            const QString role = roleOf(state);
            followFilter(*shared, role, state, context);
            syncPicker(*shared, role, state, true, command);
          }),
      choiceInput(kSystem, QObject::tr("System"), {}, QString())
          .withVisible(hidden)
          .withChoices([shared, roleOf](const CommandState& state) {
            return build::systemChoices(shared->catalog.parts(roleOf(state)));
          })
          .withOnChange([shared, roleOf, command](CommandState& state, const CommandContext& context) {
            const QString role = roleOf(state);
            followFilter(*shared, role, state, context);
            shared->lastSystem.insert(role, state.choice(kSystem));
            syncPicker(*shared, role, state, true, command);
          })
          .withTooltip(QObject::tr("The profile system: type, slot width and grid (B8 30: B-type slot 8, 30 mm)")),
      // The part as a tile (mitcad#427): a click on it, or Enter or Space,
      // opens the part picker, whose choice sets it. Every part of the
      // role is a choice (live MCP's `part`).
      choiceInput(kPart, QObject::tr("Component"), {}, QString())
          .withChoices([shared, roleOf](const CommandState& state) {
            return build::partChoices(shared->catalog.parts(roleOf(state)), QString(), QString(), state.choice(kPart));
          })
          .withChoiceTile([shared](const CommandState& state) { return partTile(*shared, state); })
          .withOnChange([shared, roleOf, command](CommandState& state, const CommandContext& context) {
            const QString role = roleOf(state);
            followPart(*shared, state, context);
            systemFromPart(*shared, role, state);
            syncPicker(*shared, role, state, false, command);
          })
          .withButton(kButtonText, buttonTip(),
                      [shared, roleOf, command](const CommandState& state, const CommandContext&) {
                        if (PartPicker* picker = partPicker(*shared, true)) {
                          picker->ask(roleRequest(*shared, roleOf(state), state, command));
                        }
                      })
          .withTooltip(QObject::tr("The component chosen; a click (or Enter) opens the component picker")),
  };
}

QVector<InputDef> pickerInputs(const SharedPtr& shared, const QString& role, const QString& command = QString()) {
  return pickerInputs(shared, [role](const CommandState&) { return role; }, command);
}

// The command ended: its picker closes unless pinned.
std::function<void(bool)> pickerEnded(const SharedPtr& shared, const QString& command) {
  return [shared, command](bool) {
    if (PartPicker* picker = shared->picker.data()) {
      picker->commandEnded(command);
    }
  };
}

// The part the picker shows first within a system (mitcad#452): its first
// favourite, as the picker lists the starred parts first (pickerOrder, in
// the catalog's order), else its first part; a system none of the parts
// fits leaves them all. A part made for no system fits any, as in the
// picker's filter.
QString pickerFirst(const QVector<Part>& parts, const QString& system) {
  QVector<const Part*> shown;
  QList<OrderedPart> order;
  for (const Part& part : parts) {
    const QStringList systems = part.systemText().split(QStringLiteral(", "), Qt::SkipEmptyParts);
    if (system.isEmpty() || systems.isEmpty() || systems.contains(system)) {
      shown.append(&part);
      order.append({part.library, part.component, QString(), part.config, part.defaultRow});
    }
  }
  if (shown.isEmpty()) {
    return system.isEmpty() ? QString() : pickerFirst(parts, QString());
  }
  const QList<int> ordered = pickerOrder(order, false);
  return shown.at(ordered.isEmpty() ? 0 : ordered.first())->key;
}

// The part of the role the design placed last (build_parts lists them in
// the order they were made), as the catalog's key of its source; empty
// when the design has none of the role. `any`: whether it has one at all.
QString placedLast(const Shared& shared, const QString& role, bool& any) {
  const QVector<BuildPart> placed = shared.designParts();
  const QVector<Part> parts = shared.catalog.parts(role);
  any = false;
  // Timber's pickers (mitcad#478): members are profiles, sheets panels.
  const bool timber = role == QStringLiteral("timber");
  const QString partRole = timber ? QStringLiteral("profile") : role == QStringLiteral("sheet") ? QStringLiteral("panel") : role;
  for (auto it = placed.crbegin(); it != placed.crend(); ++it) {
    if (it->role != partRole || (partRole == QStringLiteral("profile") && it->isTimber() != timber)) {
      continue;
    }
    any = true;
    for (const Part& part : parts) {
      if (!part.source.isEmpty() && part.source == it->source && part.config == it->config) {
        return part.key;
      }
    }
  }
  return {};
}

// The Timber tool's first part (mitcad#485): the first dimensioned timber
// (`material.finish`) 48 x 98 in section; empty when the catalog has none.
QString dimensioned48x98(const QVector<Part>& parts) {
  for (const Part& part : parts) {
    const double thin = std::min(part.sectionWidth(), part.sectionHeight());
    const double wide = std::max(part.sectionWidth(), part.sectionHeight());
    if (part.isTimber() && std::abs(thin - 48.0) < 0.5 && std::abs(wide - 98.0) < 0.5 &&
        part.build.value(QStringLiteral("material")).toObject().value(QStringLiteral("finish")).toString() ==
            QStringLiteral("dimensioned")) {
      return part.key;
    }
  }
  return {};
}

// The picker's parts read again; the part chosen in the panel stays. Else
// (mitcad#452) a design with parts of the role starts with the one chosen
// or placed last in it, and a design without them (a new one) with the
// picker's first part: its first favourite within the system it opens
// with, else its first part. A part of another design does not carry over.
void startPicker(Shared& shared, const QString& role, CommandState& state, const CommandContext& context) {
  shared.catalog.refresh(role, context);
  shared.refreshDesign(context);
  const QString design = shared.host != nullptr ? shared.host->designId() : QString();
  if (design != shared.design) {
    shared.design = design;
    shared.lastPart.clear();
  }
  const QVector<Part> parts = shared.catalog.parts(role);
  // The part picker opens filtered to the system chosen last (mitcad#427).
  if (state.choice(kSystem).isEmpty()) {
    state.setChoice(kSystem, shared.lastSystem.value(role));
  }
  Part known;
  if (!state.choice(kPart).isEmpty() && shared.catalog.find(state.choice(kPart), known)) {
    followPart(shared, state, context);
    return;
  }
  // The Timber tool starts with the part chosen last in it, in any design
  // (mitcad#485).
  const bool timber = role == QStringLiteral("timber");
  if (timber && !shared.timber.part.isEmpty() && shared.catalog.find(shared.timber.part, known)) {
    state.setChoice(kPart, known.key);
    qInfo().noquote() << QStringLiteral("BUILD %1 starts with %2/%3: chosen last in the tool")
                             .arg(role, known.library, known.component);
    followPart(shared, state, context);
    return;
  }
  bool inDesign = false;
  QString chosen = placedLast(shared, role, inDesign);
  QString why = QStringLiteral("placed last in this design");
  const QString last = shared.lastPart.value(role);
  if (inDesign && !last.isEmpty() && shared.catalog.find(last, known)) {
    chosen = last;
    why = QStringLiteral("chosen last in this design");
  } else if (chosen.isEmpty() && timber && !dimensioned48x98(parts).isEmpty()) {
    // The Timber tool's first time (mitcad#485).
    chosen = dimensioned48x98(parts);
    why = QStringLiteral("the first time's dimensioned 48x98");
  } else if (chosen.isEmpty()) {
    chosen = pickerFirst(parts, state.choice(kSystem));
    const bool starred = shared.catalog.find(chosen, known) &&
                         PartFavourites::instance().starred(known.library, known.component, known.config, known.defaultRow);
    why = starred ? QStringLiteral("the first favourite") : QStringLiteral("the picker's first part");
  }
  if (!chosen.isEmpty()) {
    state.setChoice(kPart, chosen);
    if (shared.catalog.find(chosen, known)) {
      qInfo().noquote() << QStringLiteral("BUILD %1 starts with %2/%3: %4").arg(role, known.library, known.component, why);
    }
  }
  followPart(shared, state, context);
}

// Why the picker is empty.
QString noPartsText(const Shared& shared, const QString& role) {
  const QString problem = shared.catalog.problem(role);
  QString text = QObject::tr("No %1 components: add MitCad's component library in Tools › Libraries and fetch it "
                             "(or Update in Insert from Library).")
                     .arg(role);
  if (!problem.isEmpty()) {
    text += QLatin1Char(' ') + problem;
  }
  return text;
}

// --- Placing a part (Profile, Linear Component)
//
// The panel grows as the placement needs it (mitcad#394): at first the
// part, its length and, for a ground placement, its direction (X, Y, Z);
// an end pick adds the corner's direction and the mitre, a side pick the
// offset, a pick on another part rotate and flip; once the design has a
// part to go on, Placement shows as four icon buttons (mitcad#427: no More).
// Only what the placement can use highlights: in a ground placement
// (the design has no part to go on, or Placement "On a plane") planes,
// points and faces that are no BUILD part's, with the origin's planes
// shown and Enter placing the part at the origin (a plane clicked then
// waits for the click that says where on it, mitcad#1196: kOnPlane); else
// only the faces of the parts it goes on, construction geometry hidden. The parts go into
// the BUILD host (the `build_host` query), shown as "Adds to".

// Placement's icon buttons (mitcad#427), their labels the tooltips.
const Choices kPlaces = {{QStringLiteral("auto"), QStringLiteral("Automatic: where the click is")},
                         {QStringLiteral("side"), QStringLiteral("On a profile's side")},
                         {QStringLiteral("end"), QStringLiteral("On a profile's end")},
                         {QStringLiteral("plane"), QStringLiteral("On a plane (as the first part)")}};
const QHash<QString, QString> kPlaceIcons = {{QStringLiteral("auto"), QStringLiteral("build-place-auto")},
                                             {QStringLiteral("side"), QStringLiteral("build-place-side")},
                                             {QStringLiteral("end"), QStringLiteral("build-place-end")},
                                             {QStringLiteral("plane"), QStringLiteral("build-place-plane")}};
// A ground placement's axis (at.direction): X, Y or Z of the host.
const Choices kAxes = {{QStringLiteral("x"), QStringLiteral("Along X")},
                       {QStringLiteral("y"), QStringLiteral("Along Y")},
                       {QStringLiteral("z"), QStringLiteral("Along Z (standing)")}};
const QHash<QString, QString> kAxisIcons = {{QStringLiteral("x"), QStringLiteral("build-axis-x")},
                                            {QStringLiteral("y"), QStringLiteral("build-axis-y")},
                                            {QStringLiteral("z"), QStringLiteral("build-axis-z")}};
// An end placement's axis (at.direction, mitcad#417): Auto (the click's
// zone of the end face decides), Straight, or one of the four turns square
// to the part it meets (`end.turns` of the preview, "turn0".."turn3").
const QString kAuto = QStringLiteral("auto");
const QString kStraight = QStringLiteral("straight");
const QString kTurn = QStringLiteral("turn");
// What a side placement's offset is measured from (at.from).
const Choices kFroms = {{QStringLiteral("a"), QStringLiteral("End A")},
                        {QStringLiteral("b"), QStringLiteral("End B")},
                        {QStringLiteral("middle"), QStringLiteral("Middle")}};
const Choices kEnds = {{QStringLiteral("a"), QStringLiteral("End A")}, {QStringLiteral("b"), QStringLiteral("End B")}};
// State kept besides the inputs (mitcad#417): whether the side's Offset
// and From were typed (sent as at.offset and at.from; a click leaves them
// to the click), what the preview said of the end picked last (its zone,
// direction, turns and mitre), and the side or end face under the mouse.
const QString kOffsetTyped = QStringLiteral("offset_typed");
const QString kEndPick = QStringLiteral("end_pick");
// The face a click would pick: the hover shows only that (mitcad#489,
// mitcad#508).
const QString kHoverPick = QStringLiteral("hover_pick");
// Whether the panel placed something since it opened (mitcad#513): then a
// place that would go to a default (a ground placement at the origin, a
// frame from the origin along Direction) waits for a click instead, and
// nothing is previewed there on top of what was just placed.
const QString kPlacedOne = QStringLiteral("placed_one");
const QString kTyped = QStringLiteral("typed");
const Choices kLengthModes = {{QStringLiteral("face"), QStringLiteral("Auto: to the face")},
                              {QStringLiteral("centre"), QStringLiteral("Auto: to the center")},
                              {QStringLiteral("through"), QStringLiteral("Auto: through")},
                              {kTyped, QStringLiteral("Typed length")}};
// The length mode's icon buttons (mitcad#427), their labels the tooltips;
// kLengthModes names the bound length in the inspection.
const Choices kLengthButtons = {{QStringLiteral("face"), QStringLiteral("Auto: to the face it meets")},
                                {QStringLiteral("centre"), QStringLiteral("Auto: to its center")},
                                {QStringLiteral("through"), QStringLiteral("Auto: through it")},
                                {kTyped, QStringLiteral("Typed length")}};
const QHash<QString, QString> kLengthIcons = {{QStringLiteral("face"), QStringLiteral("build-length-face")},
                                              {QStringLiteral("centre"), QStringLiteral("build-length-centre")},
                                              {QStringLiteral("through"), QStringLiteral("build-length-through")},
                                              {kTyped, QStringLiteral("build-length-typed")}};
const QStringList kAutoModes = {QStringLiteral("face"), QStringLiteral("centre"), QStringLiteral("through")};
// Auto: the model's default (mitcad#427: none on a square corner, as the
// panel's turns are; shared on a corner at another angle at offset 0),
// its label saying which.
const Choices kMitres = {{QStringLiteral("auto"), QStringLiteral("Auto")},
                         {QStringLiteral("none"), QStringLiteral("None")},
                         {QStringLiteral("shared"), QStringLiteral("Shared: both on the bisector")},
                         {QStringLiteral("new"), QStringLiteral("Cut the new component")},
                         {QStringLiteral("existing"), QStringLiteral("Cut the existing component")}};
const Choices kRotations = {{QStringLiteral("0"), QStringLiteral("0°")},
                            {QStringLiteral("90"), QStringLiteral("90°")},
                            {QStringLiteral("180"), QStringLiteral("180°")},
                            {QStringLiteral("270"), QStringLiteral("270°")}};
const Choices kTilts = {{QStringLiteral("x"), QStringLiteral("About x")},
                        {QStringLiteral("-x"), QStringLiteral("About x, the other way")},
                        {QStringLiteral("y"), QStringLiteral("About y")},
                        {QStringLiteral("-y"), QStringLiteral("About y, the other way")}};
const QString kAt = QStringLiteral("at");

// How a placement goes now: on the ground (a plane, a face, a point or the
// origin), waiting for a pick on a part, or on a part's side or end.
enum class Against { Ground, Pick, Side, End };

bool isEndFace(const SelectionItem& item) {
  return item.name.endsWith(QStringLiteral(":end_a")) || item.name.endsWith(QStringLiteral(":end_b"));
}

Against against(const Shared& shared, const QString& role, const CommandState& state) {
  const QString place = state.choice(QStringLiteral("place"));
  if (place == QStringLiteral("plane") || (place != QStringLiteral("side") && place != QStringLiteral("end") &&
                                            !shared.hasTargets(role))) {
    return Against::Ground;
  }
  const Selection& items = state.items(kAt);
  const std::optional<BuildPart> part = items.isEmpty() ? std::nullopt : shared.partAt(items.first().occurrence);
  if (!part) {
    return Against::Pick;
  }
  if (place == QStringLiteral("side")) {
    return Against::Side;
  }
  if (place == QStringLiteral("end")) {
    return Against::End;
  }
  return isEndFace(items.first()) ? Against::End : Against::Side;
}

// The first part on a chosen plane (mitcad#1196; Profile and Linear
// Component): in a ground placement a click on an origin or construction
// plane (its icon in the view or its browser row) chooses the plane, and
// the pick waits for the point: the preview follows the mouse on that
// plane wherever it is in the view (the pointer's ray against the plane,
// CommandDef::cursorPlane), the next click places the part there, Enter
// at the plane's origin, Esc takes the plane out again. Nothing else
// takes a click meanwhile, no other plane and no face. Kept while the
// point is awaited: {item (the plane's describe()), origin, normal, x}, the
// plane in the design. A face keeps its one click.
const QString kOnPlane = QStringLiteral("on_plane");
const QString kAtPick = QStringLiteral("at_pick");

bool awaitsPoint(const CommandState& state) {
  const Selection& items = state.items(kAt);
  return !items.isEmpty() && items.first().kind == SelectKind::Plane &&
         str(state.object(kOnPlane), "item") == items.first().describe();
}

// The chosen plane while its point is awaited.
std::optional<gp_Ax3> awaitedPlane(const CommandState& state) {
  if (!awaitsPoint(state)) {
    return std::nullopt;
  }
  const QJsonObject plane = state.object(kOnPlane);
  const std::optional<gp_Pnt> origin = pointFrom(plane.value(QStringLiteral("origin")));
  const QJsonArray normal = plane.value(QStringLiteral("normal")).toArray();
  const QJsonArray x = plane.value(QStringLiteral("x")).toArray();
  if (!origin || normal.size() < 3 || x.size() < 3) {
    return std::nullopt;
  }
  const gp_Vec n(normal[0].toDouble(), normal[1].toDouble(), normal[2].toDouble());
  const gp_Vec along(x[0].toDouble(), x[1].toDouble(), x[2].toDouble());
  if (n.Magnitude() < 1e-12 || along.Magnitude() < 1e-12 || n.Crossed(along).Magnitude() < 1e-12) {
    return std::nullopt;
  }
  return gp_Ax3(*origin, gp_Dir(n), gp_Dir(along));
}

// A point in tenths of a millimetre, no negative zero, as the inspection
// logs end A.
QString tenthsText(const gp_Pnt& point) {
  const auto tenths = [](double v) { return QString::number(std::round(v * 10.0) / 10.0 + 0.0, 'f', 1); };
  return QStringLiteral("%1, %2, %3").arg(tenths(point.X()), tenths(point.Y()), tenths(point.Z()));
}

// A plane clicked in a ground placement: chosen, its frame kept, its pick
// without a point (the plane's origin until the mouse or a click says
// where). A plane the model cannot tell the frame of stays a pick at the
// click, as before.
void choosePlane(const SelectionItem& item, CommandState& state, const CommandContext& context) {
  const std::optional<gp_Ax3> plane = build::planeOf(item, context);
  if (!plane) {
    state.setObject(kOnPlane, {});
    return;
  }
  const gp_Dir& normal = plane->Direction();
  const gp_Dir& x = plane->XDirection();
  state.setObject(kOnPlane, QJsonObject{{QStringLiteral("item"), item.describe()},
                                        {QStringLiteral("origin"), build::pointJson(plane->Location())},
                                        {QStringLiteral("normal"), QJsonArray{normal.X(), normal.Y(), normal.Z()}},
                                        {QStringLiteral("x"), QJsonArray{x.X(), x.Y(), x.Z()}}});
  state.setObject(kAtPick, QJsonObject{{QStringLiteral("item"), item.describe()}});
  qInfo().noquote() << QStringLiteral("BUILD plane chosen: %1; the next click places on it, Enter at its origin")
                           .arg(item.describe());
}

// The mouse on the chosen plane (none: off the view): the preview there,
// else at the plane's origin, where Enter places it. True when it moved.
bool followOnPlane(const std::optional<gp_Pnt>& point, CommandState& state) {
  if (!awaitsPoint(state)) {
    return false;
  }
  const QString item = state.items(kAt).first().describe();
  const QJsonObject pick = state.object(kAtPick);
  const std::optional<gp_Pnt> was = pointFrom(pick.value(QStringLiteral("point")));
  if (point ? (was && was->Distance(*point) < 1e-6) : !was) {
    return false;
  }
  QJsonObject now{{QStringLiteral("item"), item}};
  if (point) {
    now.insert(QStringLiteral("point"), build::pointJson(*point));
  }
  // Logged when the preview starts to follow and when it goes back to the
  // origin, not on every move.
  if (!was || !point) {
    qInfo().noquote() << (point ? QStringLiteral("BUILD on %1: the preview follows the mouse").arg(item)
                                : QStringLiteral("BUILD on %1: the mouse off the view, the preview at its origin").arg(item));
  }
  state.setObject(kAtPick, now);
  return true;
}

// The click that says where on the chosen plane: the part goes there.
bool placeOnPlane(const gp_Pnt& point, CommandState& state) {
  if (!awaitsPoint(state)) {
    return false;
  }
  const QString item = state.items(kAt).first().describe();
  state.setObject(kAtPick, QJsonObject{{QStringLiteral("item"), item}, {QStringLiteral("point"), build::pointJson(point)}});
  state.setObject(kOnPlane, {});
  qInfo().noquote() << QStringLiteral("BUILD plane click: %1 at %2").arg(item, tenthsText(point));
  return true;
}

// What the pick input takes now (InputDef::withPickRule).
SelectFilter pickKinds(const Shared& shared, const QString& role, const CommandState& state) {
  if (against(shared, role, state) == Against::Ground) {
    return SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex;
  }
  return SelectKind::Face;
}

bool pickTakes(const Shared& shared, const QString& role, const SelectionItem& item, const CommandState& state) {
  if (awaitsPoint(state)) {
    return false; // the click goes to the chosen plane (mitcad#1196)
  }
  const std::optional<BuildPart> part = shared.partAt(item.occurrence);
  if (against(shared, role, state) == Against::Ground) {
    return !part; // planes, points and faces of other components
  }
  return item.kind == SelectKind::Face && part && Shared::targets(role, part->role);
}

// What `build_place` (and its preview) gets from the panel, the auto
// length's target found.
struct Placement {
  QJsonObject fields;
  QString problem;
  QJsonObject target;     // what the auto length runs to, or empty
  QString lengthNote;     // how the length was chosen, for the panel
  Against how = Against::Ground;
};

QString angleValue(const QString& degrees) { return degrees + QStringLiteral(" deg"); }

QJsonArray axisVector(const QString& axis) {
  const double sign = axis.startsWith(QLatin1Char('-')) ? -1.0 : 1.0;
  const QChar letter = axis.isEmpty() ? QLatin1Char('z') : axis.at(axis.size() - 1);
  return QJsonArray{letter == QLatin1Char('x') ? sign : 0.0, letter == QLatin1Char('y') ? sign : 0.0,
                    letter == QLatin1Char('z') ? sign : 0.0};
}

// --- An end's directions (mitcad#417)

double dotOf(const QJsonArray& a, const QJsonArray& b) {
  if (a.size() < 3 || b.size() < 3) {
    return 0.0;
  }
  return a[0].toDouble() * b[0].toDouble() + a[1].toDouble() * b[1].toDouble() + a[2].toDouble() * b[2].toDouble();
}

// The host axis a direction runs along ("+X", "-Y"), else empty.
QString axisName(const QJsonArray& direction) {
  if (direction.size() < 3) {
    return QString();
  }
  const QString letters = QStringLiteral("XYZ");
  for (int i = 0; i < 3; ++i) {
    const double component = direction[i].toDouble();
    if (std::abs(std::abs(component) - 1.0) < 1e-6) {
      return QString(component > 0.0 ? QLatin1Char('+') : QLatin1Char('-')) + letters.at(i);
    }
  }
  return QString();
}

// A turn of an end by the host axis it runs along, else "Side 1".."Side 4".
QString turnName(const QJsonArray& turns, int index) {
  const QString axis = axisName(turns.at(index).toArray());
  return axis.isEmpty() ? QObject::tr("Side %1").arg(index + 1) : axis;
}

// Straight on from an end: the end frame's z, square to its x and y (the
// first two turns), as the preview gives it.
QJsonArray straightOf(const QJsonObject& end) {
  const QJsonArray turns = end.value(QStringLiteral("turns")).toArray();
  if (turns.size() < 2) {
    return str(end, "zone") == kStraight ? end.value(QStringLiteral("direction")).toArray() : QJsonArray();
  }
  const QJsonArray x = turns.at(0).toArray();
  const QJsonArray y = turns.at(1).toArray();
  if (x.size() < 3 || y.size() < 3) {
    return QJsonArray();
  }
  const gp_Vec z = gp_Vec(x[0].toDouble(), x[1].toDouble(), x[2].toDouble())
                       .Crossed(gp_Vec(y[0].toDouble(), y[1].toDouble(), y[2].toDouble()));
  if (z.Magnitude() < 1e-12) {
    return QJsonArray();
  }
  const gp_Vec unit = z.Normalized();
  return QJsonArray{unit.X(), unit.Y(), unit.Z()};
}

// A direction of an end by name: the turn it is, else the axis it runs
// along, else "straight on".
QString directionText(const QJsonArray& direction, const QJsonArray& turns) {
  for (int i = 0; i < turns.size(); ++i) {
    if (dotOf(direction, turns.at(i).toArray()) > 1.0 - 1e-6) {
      return turnName(turns, i);
    }
  }
  const QString axis = axisName(direction);
  return axis.isEmpty() ? QObject::tr("straight on") : axis;
}

// What the preview said of the end picked now (kept by the click), or
// empty when the pick is another one.
QJsonObject endPick(const CommandState& state) {
  const QJsonObject end = state.object(kEndPick);
  const Selection& items = state.items(kAt);
  if (end.isEmpty() || items.isEmpty() || str(end, "item") != items.first().describe() ||
      str(end, "occurrence") != items.first().occurrence) {
    return {};
  }
  return end;
}

// Whether the end placement turns the new part (a corner): a turn chosen,
// or Auto with the click in the band along an edge.
bool turnsCorner(const CommandState& state) {
  const QString direction = state.choice(QStringLiteral("direction"));
  if (direction.startsWith(kTurn)) {
    return true;
  }
  if (direction == kStraight) {
    return false;
  }
  return str(endPick(state), "zone") == QStringLiteral("edge");
}

Choices directionChoices(const CommandState& state) {
  const QJsonObject end = endPick(state);
  const QJsonArray turns = end.value(QStringLiteral("turns")).toArray();
  QString clicked;
  if (!end.isEmpty()) {
    clicked = str(end, "zone") == QStringLiteral("edge")
                  ? directionText(end.value(QStringLiteral("direction")).toArray(), turns)
                  : QObject::tr("straight");
  }
  Choices choices{{kAuto, clicked.isEmpty() ? QObject::tr("Auto: by the click")
                                            : QObject::tr("Auto: by the click (%1)").arg(clicked)},
                  {kStraight, QObject::tr("Straight")}};
  for (int i = 0; i < turns.size() && i < 4; ++i) {
    choices.append({kTurn + QString::number(i), turnName(turns, i)});
  }
  return choices;
}

// The mitre Auto takes: what the click's preview said while Direction and
// the corner's offset are as then, else none: the panel's turns are square
// to the part they meet, and a square corner lies on the end face without
// a mitre (mitcad#427; only a corner at another angle is shared).
QString autoMitre(const CommandState& state) {
  const bool offset = cmd::nonZero(state, QStringLiteral("corner_offset"));
  const QJsonObject end = endPick(state);
  const QString clicked = end.value(QStringLiteral("mitre")).toObject().value(QStringLiteral("cut")).toString();
  if (!clicked.isEmpty() && state.choice(QStringLiteral("direction")) == kAuto && !offset) {
    return clicked;
  }
  return QStringLiteral("none");
}

Choices mitreChoices(const CommandState& state) {
  const bool offset = turnsCorner(state) && cmd::nonZero(state, QStringLiteral("corner_offset"));
  Choices choices;
  for (const auto& option : kMitres) {
    if (option.first == kAuto) {
      choices.append({kAuto, QObject::tr("Auto (%1)").arg(autoMitre(state))});
    } else if (option.first != QStringLiteral("shared") || !offset) {
      // A shared mitre meets only at offset 0.
      choices.append(option);
    }
  }
  return choices;
}

// The auto length: the face the ray from the new end meets, as the preview
// of the fields without a length names it (commands.md, `build_preview`'s
// `length.target`); the fields' length binds to it in `mode`. An empty
// target when nothing is in the way (the fields as they were); false with
// the problem when the preview is refused. Profile's and Timber's.
bool bindAutoLength(QJsonObject& fields, const QString& mode, const CommandContext& context, QJsonObject& target,
                    QString& problem) {
  QJsonObject probe = fields;
  probe.remove(QStringLiteral("length"));
  probe.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
  QJsonObject first;
  try {
    first = context.queryObject(probe);
  } catch (const std::exception& error) {
    problem = errorText(error);
    return false;
  }
  target = first.value(QStringLiteral("length")).toObject().value(QStringLiteral("target")).toObject();
  if (!target.isEmpty()) {
    fields.insert(QStringLiteral("length"), QJsonObject{{QStringLiteral("bind"), target},
                                                        {QStringLiteral("mode"), mode},
                                                        {QStringLiteral("add"), 0}});
  }
  return true;
}

// The panel's fields; with an auto length the preview names its target
// first (commands.md, `build_preview`'s `length.target`), and the length
// binds to it.
Placement placement(const Shared& shared, const QString& role, const CommandState& state,
                    const CommandContext& context, const Part& part, bool profile) {
  Placement result;
  result.how = against(shared, role, state);
  if (result.how == Against::Pick) {
    result.problem = QObject::tr("Click a %1's side or end.").arg(partWord(role));
    return result;
  }
  // After Place (mitcad#513): the next one on the ground waits for a click,
  // not previewed at the origin on top of the one placed there.
  if (result.how == Against::Ground && state.items(kAt).isEmpty() && state.checked(kPlacedOne)) {
    result.problem = QObject::tr("Click a plane or a face where the next one goes.");
    return result;
  }
  QJsonObject fields{{QStringLiteral("part"), part.ref()}};
  std::optional<QJsonObject> at = QJsonObject();
  if (!state.items(kAt).isEmpty()) {
    at = build::atOf(kAt, state, context, result.problem);
    if (!at) {
      return result;
    }
  }
  const QString place = state.choice(QStringLiteral("place"));
  // The click's least snap distance on the screen (mitcad#417).
  if (const std::optional<double> snap = build::pickSnap(kAt, state)) {
    at->insert(QStringLiteral("snap"), *snap);
  }
  if (result.how == Against::Ground) {
    // On the picked plane, face or point, else (Enter) at the origin,
    // along X, Y or Z; a timber member flat or on edge (mitcad#485).
    at->insert(QStringLiteral("direction"), axisVector(state.choice(QStringLiteral("axis"))));
    if (role == QStringLiteral("timber")) {
      const QString lie = state.choice(QStringLiteral("lie"));
      fields.insert(QStringLiteral("lie"), lie.isEmpty() ? QStringLiteral("flat") : lie);
    }
  } else {
    at->insert(QStringLiteral("place"), place == QStringLiteral("side") || place == QStringLiteral("end")
                                            ? place
                                            : QStringLiteral("auto"));
    if (result.how == Against::End) {
      // The new part's axis in the host's coordinates: Straight or a turn
      // (a corner) as chosen; Auto leaves it to the click's zone.
      const QString direction = state.choice(QStringLiteral("direction"));
      const QJsonObject end = endPick(state);
      QJsonArray axis;
      if (direction == kStraight) {
        axis = end.value(QStringLiteral("straight")).toArray();
      } else if (direction.startsWith(kTurn)) {
        axis = end.value(QStringLiteral("turns")).toArray().at(direction.mid(kTurn.size()).toInt()).toArray();
      }
      if (axis.size() >= 3) {
        at->insert(QStringLiteral("direction"), axis);
      }
      // A corner's offset: the new part's end from the target's outer side.
      const QString offset = state.expression(QStringLiteral("corner_offset"));
      if (turnsCorner(state) && !offset.isEmpty()) {
        at->insert(QStringLiteral("offset"), offset);
      }
    }
    // A side's offset as typed (or kept by a change of From) from its
    // reference; else the click decides both (mitcad#417).
    if (result.how == Against::Side && state.checked(kOffsetTyped)) {
      at->insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      at->insert(QStringLiteral("from"), state.choice(QStringLiteral("from")));
    }
    fields.insert(QStringLiteral("rotate"), angleValue(state.choice(QStringLiteral("rotate"))));
    fields.insert(QStringLiteral("flip"), state.checked(QStringLiteral("flip")));
    // A profile on a side leans along the part it stands on (mitcad#444).
    if (profile && result.how == Against::Side && cmd::nonZero(state, QStringLiteral("tilt"))) {
      fields.insert(QStringLiteral("tilt"), state.expression(QStringLiteral("tilt")));
    }
    // Auto (or nothing chosen) leaves the mitre to the model; None says
    // so (a corner at an angle other than 90 degrees defaults to shared).
    const QString mitre = state.choice(QStringLiteral("mitre"));
    if (profile && result.how == Against::End && !mitre.isEmpty() && mitre != kAuto) {
      QJsonObject cut{{QStringLiteral("cut"), mitre}};
      if (mitre == QStringLiteral("new") || mitre == QStringLiteral("existing")) {
        cut.insert(QStringLiteral("angle"), state.expression(QStringLiteral("mitre_angle")));
      }
      fields.insert(QStringLiteral("mitre"), cut);
    }
  }
  fields.insert(kAt, *at);
  if (part.cutToLength()) {
    const QString mode = state.choice(QStringLiteral("length_mode"));
    if (mode == kTyped || result.how == Against::Ground) {
      fields.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
      result.lengthNote = QObject::tr("typed");
    } else {
      if (!bindAutoLength(fields, mode, context, result.target, result.problem)) {
        return result;
      }
      if (result.target.isEmpty()) {
        // Nothing in the way: the typed length.
        fields.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
        result.lengthNote = QObject::tr("typed: nothing in the way for an auto length");
      } else {
        result.lengthNote = QObject::tr("bound, %1").arg(cmd::label(kLengthModes, mode).toLower());
      }
    }
  }
  result.fields = fields;
  return result;
}

QString problemsOf(const QJsonObject& answer) {
  const QJsonValue problems = answer.value(QStringLiteral("problems"));
  if (problems.isString()) {
    return problems.toString();
  }
  QStringList texts;
  for (const QJsonValue& problem : problems.toArray()) {
    texts << (problem.isString() ? problem.toString()
                                 : str(problem.toObject(), "message").isEmpty() ? str(problem.toObject(), "reason")
                                                                                : str(problem.toObject(), "message"));
  }
  return texts.join(QStringLiteral(" "));
}

QStringList warningsOf(const QJsonObject& answer) {
  QStringList texts;
  for (const QJsonValue& warning : answer.value(QStringLiteral("warnings")).toArray()) {
    texts << warning.toString();
  }
  return texts;
}

// The preview's markers (previewMarkers): {"markers": [{label, point}]}.
std::vector<PreviewMarker> markersOf(const QJsonObject& report) {
  std::vector<PreviewMarker> markers;
  for (const QJsonValue& value : report.value(QStringLiteral("markers")).toArray()) {
    const QJsonObject marker = value.toObject();
    if (const std::optional<gp_Pnt> point = pointFrom(marker.value(QStringLiteral("point")))) {
      markers.push_back({str(marker, "label"), *point});
    }
  }
  return markers;
}

QJsonObject marker(const QString& label, const gp_Pnt& point) {
  return {{QStringLiteral("label"), label}, {QStringLiteral("point"), build::pointJson(point)}};
}

// A side snap's name in the view: "flush A", "middle", ...
QString snapText(const QString& kind) {
  if (kind == QStringLiteral("end_a")) {
    return QObject::tr("flush A");
  }
  if (kind == QStringLiteral("end_b")) {
    return QObject::tr("flush B");
  }
  if (kind == QStringLiteral("middle")) {
    return QObject::tr("middle");
  }
  if (kind == QStringLiteral("grid")) {
    return QObject::tr("grid");
  }
  if (kind == QStringLiteral("clamped")) {
    return QObject::tr("kept on the component");
  }
  return kind;
}

QString targetText(const QJsonObject& target, const CommandContext& context) {
  const std::optional<SelectionItem> item = build::itemOfTarget(target, context);
  const QString occurrence = build::rootPath(str(target, "occurrence"), context);
  const QString component = build::componentAt(occurrence, context);
  const QString name = component.isEmpty() ? occurrence : cmd::componentName(component, context);
  return item ? QObject::tr("%1 of %2").arg(item->name.isEmpty() ? item->owner : item->name, name) : name;
}

QString endsText(const QJsonObject& ends) {
  QStringList shown;
  for (const char* end : {"a", "b"}) {
    const QJsonObject cut = ends.value(QLatin1String(end)).toObject();
    // Angles in radians, as the replies give them.
    const double angle = cut.value(QStringLiteral("angle")).toDouble();
    if (!std::isfinite(angle) || std::abs(angle) < 1e-9) {
      continue;
    }
    shown << QStringLiteral("%1 %2 deg%3")
                 .arg(QString::fromLatin1(end).toUpper(), QString::number(angle * 180.0 / kPi, 'g', 4),
                      str(cut, "tilt").isEmpty() ? QString() : QStringLiteral(" (%1)").arg(str(cut, "tilt")));
  }
  return shown.isEmpty() ? QObject::tr("square ends") : QObject::tr("ends %1").arg(shown.join(QStringLiteral(", ")));
}

// A side's reference in a sentence: "end A", "end B", "the middle".
QString fromText(const QString& from) {
  if (from == QStringLiteral("b")) {
    return QObject::tr("end B");
  }
  if (from == QStringLiteral("middle")) {
    return QObject::tr("the middle");
  }
  return QObject::tr("end A");
}

// The side placement of a preview's answer (`side`, mitcad#417), with the
// target's length from the snaps (end B's near face is the length less the
// width from end A), else from the target's own length.
Shared::Side sideOf(const QJsonObject& answer, double targetLength) {
  const QJsonObject side = answer.value(QStringLiteral("side")).toObject();
  Shared::Side shown;
  if (side.isEmpty()) {
    return shown;
  }
  shown.from = str(side, "from").isEmpty() ? QStringLiteral("a") : str(side, "from");
  shown.offset = side.value(QStringLiteral("offset")).toDouble(kNaN);
  shown.width = side.value(QStringLiteral("width")).toDouble(kNaN);
  shown.longPoint = side.value(QStringLiteral("long_point")).toBool();
  shown.length = targetLength;
  for (const QJsonValue& value : answer.value(QStringLiteral("snaps")).toArray()) {
    const QJsonObject snap = value.toObject();
    const double offset = snap.value(QStringLiteral("offset")).toDouble(kNaN);
    if (str(snap, "kind") == QStringLiteral("end_b") && std::isfinite(offset) && std::isfinite(shown.width)) {
      shown.length = offset + shown.width;
    }
  }
  return shown;
}

// Whether the face under the mouse is one a click there places on: a side
// or an end face (as Placement allows) of a part the new one goes on.
bool hoverTakes(const Shared& shared, const QString& role, const SelectionItem& item, const CommandState& state) {
  const QString place = state.choice(QStringLiteral("place"));
  const QString wanted = isEndFace(item) ? QStringLiteral("end") : QStringLiteral("side");
  if (!item.isValid() || item.kind != SelectKind::Face || !item.at ||
      !(place.isEmpty() || place == kAuto || place == wanted) || !shared.hasTargets(role)) {
    return false;
  }
  const std::optional<BuildPart> part = shared.partAt(item.occurrence);
  return part && Shared::targets(role, part->role);
}

// The face a click would pick, and nothing else (mitcad#489, mitcad#508: a
// preview that followed the mouse ran the placement and the inspection on
// every move and lagged): the view's hover highlights the face, the log
// names it once as the mouse comes onto it, and the preview stays where
// the click and the panel's inputs put it, before the first click too.
// Profile, Linear Component and Timber's modes that place on a member's face.
void hoverPickOnly(const Shared& shared, const QString& role, const SelectionItem& item, CommandState& state) {
  const QString shown = state.object(kHoverPick).value(QStringLiteral("item")).toString();
  const QString now = hoverTakes(shared, role, item, state) ? item.describe() : QString();
  if (now != shown) {
    state.setObject(kHoverPick, now.isEmpty() ? QJsonObject() : QJsonObject{{QStringLiteral("item"), now}});
    qInfo().noquote() << (now.isEmpty() ? QStringLiteral("BUILD hover pick: off the parts' faces")
                                        : QStringLiteral("BUILD hover pick: %1, no preview").arg(now));
  }
}

// The end face's zones as markers (mitcad#417): "straight" in its middle,
// "turn <name>" in the middle of the band along each edge a click there
// turns towards; the one the placement takes marked "> ". Nothing when the
// view has no shape for the face.
void addEndMarkers(QJsonArray& markers, const QJsonObject& end, const SelectionItem& face,
                   const CommandContext& context) {
  const QJsonArray turns = end.value(QStringLiteral("turns")).toArray();
  const TopoDS_Shape shape = turns.size() == 4 ? context.itemShape(face) : TopoDS_Shape();
  if (shape.IsNull()) {
    return;
  }
  std::vector<gp_Pnt> corners;
  for (TopExp_Explorer vertices(shape, TopAbs_VERTEX); vertices.More(); vertices.Next()) {
    corners.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices.Current())));
  }
  if (corners.empty()) {
    return;
  }
  gp_XYZ sum(0.0, 0.0, 0.0);
  for (const gp_Pnt& corner : corners) {
    sum += corner.XYZ();
  }
  const gp_Pnt middle(sum / static_cast<double>(corners.size()));
  // The turns are in the host's coordinates, the face in the design's.
  gp_Trsf toDesign;
  if (const std::optional<gp_Trsf> placed = build::worldOf(build::buildHost(context).path, context)) {
    toDesign = *placed;
  }
  const QJsonArray direction = end.value(QStringLiteral("direction")).toArray();
  const bool turned = str(end, "zone") != kStraight && dotOf(direction, straightOf(end)) < 1.0 - 1e-6;
  markers.append(marker(turned ? QObject::tr("straight") : QObject::tr("> straight"), middle));
  for (int i = 0; i < 4; ++i) {
    const QJsonArray turn = turns.at(i).toArray();
    if (turn.size() < 3) {
      continue;
    }
    const gp_Vec along = gp_Vec(turn[0].toDouble(), turn[1].toDouble(), turn[2].toDouble()).Transformed(toDesign);
    if (along.Magnitude() < 1e-12) {
      continue;
    }
    const gp_Vec unit = along.Normalized();
    double reach = 0.0;
    for (const gp_Pnt& corner : corners) {
      reach = std::max(reach, gp_Vec(middle, corner).Dot(unit));
    }
    const bool taken = turned && dotOf(direction, turn) > 1.0 - 1e-6;
    markers.append(marker(QStringLiteral("%1turn %2").arg(taken ? QStringLiteral("> ") : QString(), turnName(turns, i)),
                          middle.Translated(unit * (0.75 * reach))));
  }
}

// How far a shape's vertices reach along an axis: the length of the
// preview's body (mitcad#431), logged beside the length the panel shows.
// NaN without vertices.
double extentAlong(const TopoDS_Shape& shape, const gp_Dir& axis) {
  double low = std::numeric_limits<double>::infinity();
  double high = -low;
  for (TopExp_Explorer it(shape, TopAbs_VERTEX); it.More(); it.Next()) {
    const double at = BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).XYZ().Dot(axis.XYZ());
    low = std::min(low, at);
    high = std::max(high, at);
  }
  return high >= low ? high - low : kNaN;
}

// How many bodies a preview's shape has: a spacing set's members
// (mitcad#487), logged for the UI tests.
int bodiesOf(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

// A spacing set's preview in the log: how many members the model lays out
// and how many bodies the view shows (mitcad#487).
QString setPreviewLog(const QJsonObject& answer, const TopoDS_Shape& shape) {
  const QJsonObject spacing = answer.value(QStringLiteral("spacing")).toObject();
  return QStringLiteral("; set preview %1 member(s), %2 body(ies)")
      .arg(spacing.value(QStringLiteral("count")).toInt())
      .arg(bodiesOf(shape));
}

// The auto length's target highlighted: its face, else (a frame of the part
// in the way) the part.
void highlightTarget(Inspection& result, const QJsonObject& target, const CommandContext& context) {
  if (target.isEmpty()) {
    return;
  }
  if (const std::optional<SelectionItem> item = build::itemOfTarget(target, context)) {
    result.highlights.append(*item);
  } else if (const QString path = build::rootPath(str(target, "occurrence"), context); !path.isEmpty()) {
    result.highlights.append(SelectionItem{SelectKind::Component, QString(), QString(), QString(), path});
  }
}

// The inspection of a placement: what build_preview says, the new part's
// shape (the model's, else its section along the frame), the auto
// length's target highlighted, the length at the free end, the side's
// snaps and an end face's zones, where the part goes ("Adds to") and the
// first part's hint: where the click and the panel's inputs put it, never
// where the mouse is (mitcad#508). With `spacing` (Timber's set between
// two members) the set's every member (mitcad#487).
Inspection inspectPlacement(const SharedPtr& shared, const QString& role, const CommandState& state,
                            const CommandContext& context, bool profile, const QJsonObject& spacing = QJsonObject()) {
  Inspection result;
  Part part;
  if (!chosenPart(*shared, state, part)) {
    result.error = noPartsText(*shared, role);
    shared->clearPreview();
    return result;
  }
  const build::Host host = build::buildHost(context);
  const QString addsTo = QObject::tr("Adds to: %1").arg(host.name);
  const Placement placed = placement(*shared, role, state, context, part, profile);
  if (placed.how == Against::Ground && state.items(kAt).isEmpty()) {
    // The first part: a click or Enter (mitcad#394); Shift + click places
    // it at once (mitcad#451). After Place only a click (mitcad#513).
    result.viewHint = state.checked(kPlacedOne)
                          ? QObject::tr("Click a plane or a face for the next one; Shift+click places at once")
                          : QObject::tr("Click a plane or a face, or press Enter; Shift+click places at once");
  } else if (placed.how == Against::Pick) {
    result.viewHint = QObject::tr("Click a %1's side or end; Shift+click places at once").arg(partWord(role));
  } else if (awaitsPoint(state)) {
    // A chosen plane (mitcad#1196): the preview follows the mouse on it.
    result.viewHint =
        QObject::tr("Click where on the plane it goes, or press Enter for the plane's origin; Esc picks another plane");
  }
  if (!placed.problem.isEmpty()) {
    result.text = QStringLiteral("<b>%1</b><br>%2").arg(part.name.toHtmlEscaped(), addsTo.toHtmlEscaped());
    result.error = placed.problem;
    result.log = QStringLiteral("waiting: %1; adds to %2; part %3").arg(placed.problem, host.name, part.name);
    shared->clearPreview(); // nothing shown: no handles where the last part was
    return result;
  }
  // A set: the model answers its first member and lays out the others.
  QJsonObject fields = placed.fields;
  if (!spacing.isEmpty()) {
    fields.insert(QStringLiteral("spacing"), spacing);
  }
  QJsonObject request = fields;
  request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
  QJsonObject answer;
  try {
    answer = context.queryObject(request);
  } catch (const std::exception& error) {
    result.error = errorText(error);
    shared->clearPreview();
    return result;
  }
  const build::Frame frame = build::frameOf(answer.value(QStringLiteral("frame")));
  double length = build::lengthOf(answer.value(QStringLiteral("length")));
  if (!std::isfinite(length) && placed.target.isEmpty()) {
    length = state.value(QStringLiteral("length"));
  }
  shared->setPreview(frame, length, !placed.target.isEmpty());
  // The model's shape of the new part (a set's every member), else its
  // section along the axis.
  QJsonObject shapeRequest = fields;
  shapeRequest.insert(QStringLiteral("shape"), QStringLiteral("build_preview"));
  result.shape = context.analysisShape(shapeRequest);
  const double bodyLength = result.shape.IsNull() || !frame.valid ? kNaN : extentAlong(result.shape, frame.z);
  if (result.shape.IsNull() && part.sectionWidth() > 0.0) {
    result.shape = build::boxAlong(frame, part.sectionWidth(), part.sectionHeight(), length);
  }
  highlightTarget(result, placed.target, context);
  const QString place = str(answer, "place");
  const QJsonObject ends = answer.value(QStringLiteral("ends")).toObject();
  const QJsonObject side = answer.value(QStringLiteral("side")).toObject();
  const QJsonObject end = answer.value(QStringLiteral("end")).toObject();
  QStringList lines;
  lines << QStringLiteral("<b>%1</b>").arg(part.name.toHtmlEscaped());
  const QString seller = QStringList{part.supplier, part.partNumber}.join(QLatin1Char(' ')).trimmed();
  if (!seller.isEmpty()) {
    lines << seller.toHtmlEscaped();
  }
  lines << addsTo.toHtmlEscaped();
  if (!place.isEmpty()) {
    lines << (place == QStringLiteral("end")        ? QObject::tr("On the end")
              : place == QStringLiteral("side")     ? QObject::tr("On the side")
              : place == QStringLiteral("frame")    ? QObject::tr("On the component's frame")
              : place == QStringLiteral("grounded") ? QObject::tr("At the origin")
                                                    : QObject::tr("On the picked geometry"));
  }
  // On a side (mitcad#417): the offset from the reference the placement is
  // bound to, which a change of From keeps in place.
  QString sideLog;
  if (!side.isEmpty()) {
    Shared::Side shown = sideOf(answer, [&] {
      const Selection& items = state.items(kAt);
      const std::optional<BuildPart> target = items.isEmpty() ? std::nullopt : shared->partAt(items.first().occurrence);
      return target ? target->length : kNaN;
    }());
    shown.setCount = spacing.isEmpty()
                         ? -1
                         : answer.value(QStringLiteral("spacing")).toObject().value(QStringLiteral("count")).toInt(-1);
    shared->setSide(shown);
    const QString kind = str(side, "kind");
    const QString from = fromText(shown.from);
    lines << (kind.isEmpty() ? QObject::tr("%1 mm from %2").arg(shown.offset, 0, 'f', 1).arg(from)
                             : QObject::tr("%1 mm from %2 (%3)").arg(shown.offset, 0, 'f', 1).arg(from, snapText(kind)));
    sideLog = QStringLiteral("; snap %1 at %2 mm from %3")
                  .arg(kind.isEmpty() ? QStringLiteral("none") : kind)
                  .arg(shown.offset, 0, 'f', 1)
                  .arg(shown.from);
    // A tilt (mitcad#444): its handle at the foot.
    const double tilt = answer.value(QStringLiteral("tilt")).toDouble(0.0) * 180.0 / kPi;
    if (std::abs(tilt) > 1e-9) {
      lines << QObject::tr("Tilted %1° towards end %2, its foot cut to lie on the side")
                   .arg(std::abs(tilt), 0, 'f', 1)
                   .arg(tilt > 0.0 ? QStringLiteral("B") : QStringLiteral("A"));
      sideLog += QStringLiteral("; tilt %1 deg").arg(tilt, 0, 'f', 2);
    }
    Shared::Foot foot;
    const std::optional<gp_Pnt> point = pointFrom(side.value(QStringLiteral("foot")));
    const QJsonArray normal = side.value(QStringLiteral("normal")).toArray();
    const QJsonArray along = side.value(QStringLiteral("along")).toArray();
    if (point && normal.size() >= 3 && along.size() >= 3) {
      const gp_Vec n(normal[0].toDouble(), normal[1].toDouble(), normal[2].toDouble());
      const gp_Vec a(along[0].toDouble(), along[1].toDouble(), along[2].toDouble());
      if (n.Magnitude() > 1e-9 && a.Magnitude() > 1e-9) {
        foot = Shared::Foot{true, *point, gp_Dir(n), gp_Dir(a)};
      }
    }
    shared->setFoot(foot);
  } else {
    shared->setFoot({});
  }
  // On an end (mitcad#417): straight on or turned, a corner's offset and
  // the mitre the placement takes.
  QString endLog;
  if (!end.isEmpty()) {
    const QJsonArray turns = end.value(QStringLiteral("turns")).toArray();
    const QString towards = directionText(end.value(QStringLiteral("direction")).toArray(), turns);
    const QJsonObject mitre = end.value(QStringLiteral("mitre")).toObject();
    const QString cut = str(mitre, "cut").isEmpty() ? QStringLiteral("none") : str(mitre, "cut");
    const QJsonValue offset = end.value(QStringLiteral("offset"));
    const bool straight = dotOf(end.value(QStringLiteral("direction")).toArray(), straightOf(end)) > 1.0 - 1e-6;
    lines << (straight ? QObject::tr("Straight on (%1), miter %2").arg(towards, cut)
                       : QObject::tr("Turned to %1 at %2 mm from the outer side, miter %3")
                             .arg(towards)
                             .arg(offset.toDouble(), 0, 'f', 1)
                             .arg(cut));
    endLog = QStringLiteral("; end %1 towards %2, mitre %3%4")
                 .arg(str(end, "zone"), towards, cut,
                      offset.isDouble() ? QStringLiteral(", offset %1 mm").arg(offset.toDouble(), 0, 'f', 1)
                                        : QString());
  }
  if (part.cutToLength()) {
    QString shown = std::isfinite(length) ? QObject::tr("Length %1 mm").arg(length, 0, 'f', 1) : QObject::tr("Length");
    if (!placed.target.isEmpty()) {
      shown += QObject::tr(", to %1 (Tab: face, center, through; typing a length detaches it)")
                   .arg(targetText(placed.target, context).toHtmlEscaped());
    } else if (!placed.lengthNote.isEmpty()) {
      shown += QStringLiteral(" (%1)").arg(placed.lengthNote.toHtmlEscaped());
    }
    lines << shown;
    lines << endsText(ends);
  }
  result.text = lines.join(QStringLiteral("<br>"));
  result.error = problemsOf(answer);
  const QJsonValue snap = placed.fields.value(kAt).toObject().value(QStringLiteral("snap"));
  result.log = QStringLiteral("place %1, length %2 mm (%3%4), %5; adds to %6%7%8")
                   .arg(place.isEmpty() ? QStringLiteral("first") : place,
                        std::isfinite(length) ? QString::number(length, 'f', 2) : QStringLiteral("?"),
                        placed.lengthNote,
                        placed.target.isEmpty() ? QString()
                                                : QStringLiteral(" to ") + targetText(placed.target, context),
                        endsText(ends), host.name, sideLog, endLog);
  if (snap.isDouble()) {
    result.log += QStringLiteral("; least snap %1 mm").arg(snap.toDouble(), 0, 'f', 2);
  }
  if (placed.how == Against::Ground) {
    result.log += QStringLiteral("; along %1").arg(state.choice(QStringLiteral("axis")));
    if (placed.fields.contains(QStringLiteral("lie"))) {
      result.log += QStringLiteral(", %1").arg(str(placed.fields, "lie"));
    }
  }
  // How a member lies (mitcad#485): the design's axis its thickness (the
  // section's smaller side) runs along.
  if (part.isTimber() && frame.valid && part.sectionWidth() > 0.0 &&
      std::abs(part.sectionWidth() - part.sectionHeight()) > 1e-6) {
    const gp_Dir thin = part.sectionWidth() < part.sectionHeight() ? frame.x : frame.z.Crossed(frame.x);
    const QString axis = std::abs(thin.X()) > 1.0 - 1e-6   ? QStringLiteral("X")
                         : std::abs(thin.Y()) > 1.0 - 1e-6 ? QStringLiteral("Y")
                         : std::abs(thin.Z()) > 1.0 - 1e-6 ? QStringLiteral("Z")
                                                           : QStringLiteral("none of X, Y, Z");
    result.log += QStringLiteral("; thickness along %1").arg(axis);
  }
  // Where its end A goes (mitcad#492: a member lying on a plane rests on
  // it), in tenths of a millimetre, no negative zero.
  if (frame.valid) {
    const auto tenths = [](double v) { return QString::number(std::round(v * 10.0) / 10.0 + 0.0, 'f', 1); };
    result.log += QStringLiteral("; end A at %1, %2, %3")
                      .arg(tenths(frame.origin.X()), tenths(frame.origin.Y()), tenths(frame.origin.Z()));
  }
  // A corner the length closes, bound (mitcad#485) or typed to end there
  // (mitcad#492).
  if (const QJsonObject closing = answer.value(QStringLiteral("closing")).toObject(); !closing.isEmpty()) {
    const QString name = cmd::componentName(str(closing, "component"), context);
    const QString atEnd = str(closing, "end").toUpper();
    const bool mitred = str(closing, "mitre") == QStringLiteral("shared");
    lines.append(mitred ? QObject::tr("Closes the corner at end %1 of %2: a shared miter").arg(atEnd, name.toHtmlEscaped())
                        : QObject::tr("Closes the corner at end %1 of %2: a square butt").arg(atEnd, name.toHtmlEscaped()));
    result.text = lines.join(QStringLiteral("<br>"));
    result.log += QStringLiteral("; closes the corner at end %1 of %2, %3")
                      .arg(atEnd, name, mitred ? QStringLiteral("shared mitre") : QStringLiteral("square butt"));
  }
  if (std::isfinite(bodyLength)) {
    result.log += QStringLiteral("; body %1 mm").arg(bodyLength, 0, 'f', 2);
  }
  if (!spacing.isEmpty()) {
    result.log += setPreviewLog(answer, result.shape);
  }
  // The part previewed (mitcad#418: a part chosen in the picker).
  result.log += QStringLiteral("; part %1").arg(part.name);
  // The length at the free end, and the side's snaps: the ends and the
  // middle, the one taken marked; an end face's zones.
  QJsonArray markers;
  if (frame.valid && std::isfinite(length) && part.cutToLength()) {
    markers.append(marker(QStringLiteral("%1 mm").arg(length, 0, 'f', 1),
                          frame.origin.Translated(gp_Vec(frame.z) * length)));
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("snaps")).toArray()) {
    const QJsonObject snapped = value.toObject();
    const QString kind = str(snapped, "kind");
    const std::optional<gp_Pnt> point = pointFrom(snapped.value(QStringLiteral("point")));
    if (point && kind != QStringLiteral("grid") && kind != str(side, "kind")) {
      markers.append(marker(snapText(kind), *point));
    }
  }
  if (const std::optional<gp_Pnt> point = pointFrom(side.value(QStringLiteral("point")));
      point && !str(side, "kind").isEmpty()) {
    markers.append(marker(QStringLiteral("> %1").arg(snapText(str(side, "kind"))), *point));
  }
  if (!end.isEmpty() && !state.items(kAt).isEmpty() && isEndFace(state.items(kAt).first())) {
    addEndMarkers(markers, end, state.items(kAt).first(), context);
  }
  result.report = {{QStringLiteral("markers"), markers}};
  // The box at the cursor shows the auto length while it is bound; typing
  // there detaches it (the length input's onChange).
  if (!placed.target.isEmpty() && std::isfinite(length)) {
    result.report.insert(QStringLiteral("cursor"),
                         QJsonObject{{QStringLiteral("length"), cmd::lengthText(std::round(length * 10.0) / 10.0, context)}});
  }
  return result;
}

bool shownForCut(const Shared& shared, const CommandState& state) {
  Part part;
  return chosenPart(shared, state, part) && part.cutToLength();
}

// A click on a part fills the panel from what the preview makes of it
// (mitcad#417): on a side From and Offset with the reference and the
// offset the click resolved to (a flush end, the middle, else the nearer
// end), left to the click until typed; on an end Direction's choices (the
// end's turns), Direction Auto again.
void fillFromClick(const Shared& shared, const QString& role, bool profile, CommandState& state,
                   const CommandContext& context) {
  const Against how = against(shared, role, state);
  Part part;
  if ((how != Against::Side && how != Against::End) || !chosenPart(shared, state, part)) {
    return;
  }
  if (how == Against::End) {
    state.setChoice(QStringLiteral("direction"), kAuto);
  }
  // Asked with the mitre Auto: the answer's mitre is what Auto takes.
  CommandState probe = state;
  probe.setChoice(QStringLiteral("mitre"), kAuto);
  const Placement placed = placement(shared, role, probe, context, part, profile);
  if (!placed.problem.isEmpty()) {
    return;
  }
  QJsonObject request = placed.fields;
  request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
  QJsonObject answer;
  try {
    answer = context.queryObject(request);
  } catch (const std::exception& error) {
    qInfo().noquote() << QStringLiteral("BUILD click not placed: %1").arg(errorText(error));
    return;
  }
  if (how == Against::Side) {
    const QJsonObject side = answer.value(QStringLiteral("side")).toObject();
    const QJsonValue offset = side.value(QStringLiteral("offset"));
    if (str(side, "from").isEmpty() || !offset.isDouble()) {
      return;
    }
    state.setChoice(QStringLiteral("from"), str(side, "from"));
    state.setText(QStringLiteral("offset"), cmd::lengthText(std::round(offset.toDouble() * 100.0) / 100.0, context));
    state.setValue(QStringLiteral("offset"), kNaN);
    qInfo().noquote() << QStringLiteral("BUILD side click: From %1, Offset %2 (%3)")
                             .arg(cmd::label(kFroms, str(side, "from")), state.text(QStringLiteral("offset")),
                                  str(side, "kind").isEmpty() ? QStringLiteral("between snaps") : str(side, "kind"));
    return;
  }
  QJsonObject end = answer.value(QStringLiteral("end")).toObject();
  if (end.isEmpty()) {
    return;
  }
  const SelectionItem& item = state.items(kAt).first();
  end.insert(QStringLiteral("item"), item.describe());
  end.insert(QStringLiteral("occurrence"), item.occurrence);
  end.insert(QStringLiteral("straight"), straightOf(end));
  state.setObject(kEndPick, end);
  const QJsonArray turns = end.value(QStringLiteral("turns")).toArray();
  QStringList names;
  for (int i = 0; i < turns.size(); ++i) {
    names << turnName(turns, i);
  }
  qInfo().noquote() << QStringLiteral("BUILD end click: %1 towards %2, mitre %3; turns %4")
                           .arg(str(end, "zone"), directionText(end.value(QStringLiteral("direction")).toArray(), turns),
                                str(end.value(QStringLiteral("mitre")).toObject(), "cut"),
                                names.join(QStringLiteral(", ")));
}

// From changed (mitcad#417): the part stays where it is, its offset from
// the new reference recomputed from the side shown last: from end A the
// near face, from end B the far face (towards A), from the middle the
// part's centre (towards B). Sent as typed from then on.
void keepPlace(const Shared& shared, CommandState& state, const CommandContext& context) {
  const Shared::Side side = shared.shownSide();
  const QString to = state.choice(QStringLiteral("from"));
  state.setChecked(kOffsetTyped, true);
  // A tilted part's offset is to its foot's long point, a point (mitcad#444).
  const double width = side.longPoint ? 0.0 : side.width;
  const double length = side.length;
  const bool known = std::isfinite(side.offset) && std::isfinite(width) &&
                     (std::isfinite(length) || (side.from == QStringLiteral("a") && to == QStringLiteral("a")));
  if (side.from.isEmpty() || side.from == to || !known) {
    return;
  }
  // The near face from end A, then the offset from the new reference.
  const double nearFace = side.from == QStringLiteral("b")        ? length - side.offset - width
                          : side.from == QStringLiteral("middle") ? length / 2.0 + side.offset - width / 2.0
                                                                  : side.offset;
  const double offset = to == QStringLiteral("b")        ? length - nearFace - width
                        : to == QStringLiteral("middle") ? nearFace + width / 2.0 - length / 2.0
                                                         : nearFace;
  state.setText(QStringLiteral("offset"), cmd::lengthText(std::round(offset * 100.0) / 100.0, context));
  state.setValue(QStringLiteral("offset"), kNaN);
  qInfo().noquote() << QStringLiteral("BUILD From %1 keeps the part in place: Offset %2")
                           .arg(cmd::label(kFroms, to), state.text(QStringLiteral("offset")));
}

// The inputs where and how a part goes, after the picker; with
// `planeThenPoint` a plane picked for a ground placement waits for the
// click that says where on it (mitcad#1196, kOnPlane).
QVector<InputDef> placementInputs(const SharedPtr& shared, const QString& role, bool profile,
                                  bool planeThenPoint = false) {
  const auto cut = [shared](const CommandState& state) { return shownForCut(*shared, state); };
  const auto how = [shared, role](const CommandState& state) { return against(*shared, role, state); };
  const auto ground = [how](const CommandState& state) { return how(state) == Against::Ground; };
  const auto onSide = [how](const CommandState& state) { return how(state) == Against::Side; };
  const auto onEnd = [how](const CommandState& state) { return how(state) == Against::End; };
  const auto onPart = [how](const CommandState& state) {
    return how(state) == Against::Side || how(state) == Against::End;
  };
  const auto cornered = [onEnd](const CommandState& state) { return onEnd(state) && turnsCorner(state); };
  QVector<InputDef> inputs = {
      selectionInput(kAt, QObject::tr("Place On"),
                     SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex, 0, 1)
          .withOnPick([shared, role, profile, planeThenPoint](const SelectionItem& item, CommandState& state,
                                                              const CommandContext& context) {
            // A new click: where it was, its snap distance, and what it
            // places fills the panel (mitcad#417). A click on the face
            // picked already places it again there (near the other end, on
            // another zone of the end face) rather than taking it out.
            build::recordPick(kAt, item, state, context);
            state.setChecked(kOffsetTyped, false);
            // A plane for the first part waits for where on it (mitcad#1196).
            if (planeThenPoint && item.kind == SelectKind::Plane && against(*shared, role, state) == Against::Ground) {
              choosePlane(item, state, context);
            } else {
              state.setObject(kOnPlane, {});
            }
            fillFromClick(*shared, role, profile, state, context);
          },
                      true)
          .withPickRule([shared, role](const CommandState& state) { return pickKinds(*shared, role, state); },
                        [shared, role](const SelectionItem& item, const CommandState& state) {
                          return pickTakes(*shared, role, item, state);
                        })
          .withHiddenConstruction()
          // Shift + click: the pick and Place in one (mitcad#451).
          .withShiftClickApplies()
          .withTooltip(QObject::tr("Where the component goes: a plane, a point, a face, or a profile's side or end")),
      // Icon buttons (mitcad#427), each one's tooltip naming it; Tab steps
      // face, centre, through.
      choiceInput(QStringLiteral("length_mode"), QObject::tr("Length"), kLengthButtons, QStringLiteral("face"))
          .withChoiceIcons(kLengthIcons)
          .withVisible([cut, ground](const CommandState& state) { return cut(state) && !ground(state); })
          .withTabCycle(kAutoModes)
          .withOnChange([shared](CommandState& state, const CommandContext& context) {
            // Typed from where the auto length is.
            if (state.choice(QStringLiteral("length_mode")) == kTyped) {
              const double shown = shared->preview().second;
              if (std::isfinite(shown) && shown > 0.0) {
                state.setText(QStringLiteral("length"), cmd::lengthText(std::round(shown * 10.0) / 10.0, context));
                state.setValue(QStringLiteral("length"), kNaN);
              }
            }
          }),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QString())
          .withVisible(cut)
          .withCursorBox()
          .withOnChange([](CommandState& state, const CommandContext&) {
            // Typing a length detaches it from the auto length.
            if (state.choice(QStringLiteral("length_mode")) != kTyped) {
              state.setChoice(QStringLiteral("length_mode"), kTyped);
              qInfo().noquote() << QStringLiteral("BUILD length typed: detached from the auto length");
            }
          })
          .withManipulator([shared](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            // A handle at the free end while the length is a number (typed,
            // or nothing in the way): dragging it types the length.
            if (state.choice(QStringLiteral("length_mode")) != kTyped && shared->previewBound()) {
              return std::nullopt;
            }
            const build::Frame frame = shared->preview().first;
            if (!frame.valid) {
              return std::nullopt;
            }
            return cmd::arrow(frame.origin, frame.z);
          })
          .withTooltip(QObject::tr("The component's length: typed, in the box at the cursor, or dragged by its handle")),
      choiceInput(QStringLiteral("axis"), QObject::tr("Direction"), kAxes, QStringLiteral("z"))
          .withChoiceIcons(kAxisIcons)
          .withVisible(ground)
          .withTooltip(QObject::tr("The axis the component runs along from where it goes")),
      // On an end (mitcad#417): the click's zone of the end face decides
      // (its inner half straight on, the band along an edge turned towards
      // that edge), or as chosen.
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                  Choices{{kAuto, QObject::tr("Auto: by the click")}}, kAuto)
          .withChoices([](const CommandState& state) { return directionChoices(state); })
          .withVisible(onEnd)
          .withTooltip(QObject::tr("On an end: by the click, straight on, or turned to a side of the component it meets")),
      valueInput(QStringLiteral("corner_offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(cornered)
          .withOnChange([](CommandState& state, const CommandContext&) {
            // A shared mitre meets only at offset 0.
            if (state.choice(QStringLiteral("mitre")) == QStringLiteral("shared") &&
                cmd::nonZero(state, QStringLiteral("corner_offset"))) {
              state.setChoice(QStringLiteral("mitre"), kAuto);
              qInfo().noquote() << QStringLiteral("BUILD corner offset: the shared mitre goes (it needs offset 0)");
            }
          })
          .withTooltip(QObject::tr("The new component's end from the corner's outer side, positive towards the turn")),
  };
  if (profile) {
    const auto mitred = [onEnd](const CommandState& state) {
      const QString mitre = state.choice(QStringLiteral("mitre"));
      return onEnd(state) && (mitre == QStringLiteral("new") || mitre == QStringLiteral("existing"));
    };
    inputs.append(choiceInput(QStringLiteral("mitre"), QObject::tr("Miter"), kMitres, kAuto)
                      .withChoices([](const CommandState& state) { return mitreChoices(state); })
                      .withVisible([cut, onEnd](const CommandState& state) { return cut(state) && onEnd(state); })
                      .withTooltip(
                          QObject::tr("The end cut: none, shared on the bisector, the new component or the existing one")));
    inputs.append(valueInput(QStringLiteral("mitre_angle"), QObject::tr("Miter Angle"), ValueKind::Angle,
                             QStringLiteral("45 deg"))
                      .withVisible(mitred));
  }
  // On a side (mitcad#417): the click fills both; a typed offset moves
  // the part from its reference, a change of From keeps it in place.
  inputs.append(valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
                    .withVisible(onSide)
                    .withOnChange([](CommandState& state, const CommandContext&) {
                      if (!state.checked(kOffsetTyped)) {
                        state.setChecked(kOffsetTyped, true);
                        qInfo().noquote() << QStringLiteral("BUILD side offset typed: from %1")
                                                 .arg(fromText(state.choice(QStringLiteral("from"))));
                      }
                    })
                    .withTooltip(QObject::tr("The distance from the reference From names; 0 is flush, or centered")));
  inputs.append(choiceInput(QStringLiteral("from"), QObject::tr("From"), kFroms, QStringLiteral("a"))
                    .withVisible(onSide)
                    .withOnChange([shared](CommandState& state, const CommandContext& context) {
                      keepPlace(*shared, state, context);
                    })
                    .withTooltip(QObject::tr("What the offset is measured from; the component stays bound to it when "
                                             "what it is on changes length")));
  inputs.append(choiceInput(QStringLiteral("rotate"), QObject::tr("Rotate"), kRotations, QStringLiteral("0"))
                    .withVisible(onPart)
                    .withTooltip(QObject::tr("About the component's axis, in 90 degree steps")));
  if (profile) {
    // On a side (mitcad#444): a brace leaning along the part it stands on,
    // its foot cut to lie on the side; dragged by the arc at the foot, its
    // angle in the box at the cursor while the arc is dragged.
    inputs.append(valueInput(QStringLiteral("tilt"), QObject::tr("Tilt"), ValueKind::Angle, QStringLiteral("0 deg"))
                      .withVisible([cut, onSide](const CommandState& state) { return cut(state) && onSide(state); })
                      .withCursorBox()
                      .withManipulator([shared](const CommandState&, const CommandContext&) -> std::optional<Manipulator> {
                        const Shared::Foot foot = shared->shownFoot();
                        if (!foot.valid) {
                          return std::nullopt;
                        }
                        // Turning about the slot's cross axis: 0 upright,
                        // positive towards end B of the part it is on.
                        const gp_Vec across = gp_Vec(foot.normal).Crossed(gp_Vec(foot.along));
                        if (across.Magnitude() < 1e-9) {
                          return std::nullopt;
                        }
                        Manipulator handle = cmd::ring(foot.point, gp_Dir(across));
                        handle.reference = foot.normal;
                        return handle;
                      })
                      .withTooltip(
                          QObject::tr("The lean along the component it stands on, up to 80 degrees; 0 stands square")));
  }
  inputs.append(flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withVisible(onPart));
  // Placement as icon buttons (mitcad#427: shown directly, no More), once
  // the design has a part to go on; each button's tooltip names it.
  inputs.append(choiceInput(QStringLiteral("place"), QObject::tr("Placement"), kPlaces, QStringLiteral("auto"))
                    .withChoiceIcons(kPlaceIcons)
                    .withVisible([shared, role](const CommandState&) { return shared->hasTargets(role); })
                    .withOnChange([shared, role](CommandState& state, const CommandContext&) {
                      // What the pick takes changes with it: a pick it no
                      // longer takes is picked again.
                      const Selection& items = state.items(kAt);
                      if (!items.isEmpty() && !pickTakes(*shared, role, items.first(), state)) {
                        state.setItems(kAt, {});
                        state.setObject(QStringLiteral("at_pick"), {});
                        state.setObject(kOnPlane, {});
                      }
                    }));
  return inputs;
}

// Places a part (build_place with the fields): logs it, shows its warnings,
// and readies the panel for the next one (picked again; the part stays).
bool placeNow(Shared& shared, DocumentHost& host, const QString& role, const Part& part, QJsonObject command,
              const QString& lengthNote, CommandState& state, const CommandContext& context) {
  command.insert(QStringLiteral("cmd"), QStringLiteral("build_place"));
  QJsonObject result;
  if (!host.runModelCommand(command, &result)) {
    return false;
  }
  const QStringList warnings = warningsOf(result);
  // A spacing set (mitcad#478): its count.
  const QJsonValue count = result.value(QStringLiteral("count"));
  // A set fastened to its hosts (mitcad#497): the junctions fastened.
  const QJsonValue fastened = result.value(QStringLiteral("fasteners"));
  QString set = count.isDouble() ? QStringLiteral("; a set of %1").arg(count.toInt()) : QString();
  if (fastened.isDouble()) {
    set += QStringLiteral(", %1 junction(s) fastened").arg(fastened.toInt());
  }
  qInfo().noquote() << QStringLiteral("Placed %1 as %2 (%3), length %4%5%6")
                           .arg(part.name, str(result, "name"), str(result, "occurrence"), lengthNote, set,
                                warnings.isEmpty() ? QString() : QStringLiteral("; ") + warnings.join(QStringLiteral("; ")));
  if (!warnings.isEmpty()) {
    host.showStatus(warnings.join(QLatin1Char(' ')), NoticeLevel::Warning);
  }
  // The next one: picked again; the part stays; nothing previewed before
  // the click (mitcad#513).
  state.setChecked(kPlacedOne, true);
  state.setItems(kAt, {});
  state.setObject(QStringLiteral("at_pick"), {});
  state.setObject(kOnPlane, {});
  state.setObject(kHoverPick, {});
  state.setObject(kEndPick, {});
  state.setChecked(kOffsetTyped, false);
  shared.lastPart.insert(role, part.key);
  if (role == QStringLiteral("timber")) {
    shared.timber.part = part.key; // mitcad#485
  }
  shared.catalog.refresh(role, context); // its source is the design's now
  shared.refreshDesign(context);
  return true;
}

CommandDef placeCommand(const SharedPtr& shared, DocumentHost& host, const QString& role) {
  const bool profile = role == QStringLiteral("profile");
  CommandDef def;
  def.id = profile ? QStringLiteral("build.profile") : QStringLiteral("build.linear");
  def.name = profile ? QObject::tr("Profile") : QObject::tr("Linear Component");
  def.icon = profile ? QStringLiteral("build-profile") : QStringLiteral("build-linear");
  def.tooltip = profile ? QObject::tr("Places slot profiles; their length runs to the component in the way")
                        : QObject::tr("Places rails, carriages, bearings and motor plates");
  def.tab = kTab;
  def.group = QStringLiteral("PROFILE");
  def.pinned = true;
  def.keywords = profile ? QStringList{QStringLiteral("aluminium"), QStringLiteral("extrusion"), QStringLiteral("frame"),
                                       QStringLiteral("slot"), QStringLiteral("build")}
                         : QStringList{QStringLiteral("rail"), QStringLiteral("carriage"), QStringLiteral("guide"),
                                       QStringLiteral("bearing"), QStringLiteral("nema"), QStringLiteral("build")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  // A slot profile or a rail is of no timber member's system: on a member's
  // face the view's context menu offers it under More (mitcad#494).
  def.completes = [&host](const Selection& selection) {
    return std::none_of(selection.begin(), selection.end(), [&host](const SelectionItem& item) {
      const std::optional<BuildPart> part = build::partOf(item, host.model());
      return part && part->isTimber();
    });
  };
  def.inputs = pickerInputs(shared, role);
  def.inputs += placementInputs(shared, role, profile, true);
  def.init = [shared, role](CommandState& state, const CommandContext& context) {
    startPicker(*shared, role, state, context);
    qInfo().noquote() << QStringLiteral("BUILD %1 adds to %2%3")
                             .arg(role, build::buildHost(context).name,
                                  shared->hasTargets(role) ? QString() : QStringLiteral(" (a ground placement)"));
    if (PartPicker* picker = pinnedPicker(*shared)) {
      picker->ask(roleRequest(*shared, role, state));
    }
  };
  def.ended = pickerEnded(shared, def.id);
  def.inspect = [shared, role, profile](const CommandState& state, const CommandContext& context) {
    return inspectPlacement(shared, role, state, context, profile);
  };
  def.previewMarkers = markersOf;
  // The mouse on a side or an end face shows only the face a click picks;
  // the preview stays the click's (mitcad#508: following the mouse lagged).
  def.hoverPreview = [shared, role](const SelectionItem& item, CommandState& state, const CommandContext&) {
    hoverPickOnly(*shared, role, item, state);
    return false;
  };
  // A chosen plane (mitcad#1196): the preview follows the mouse on it, the
  // next click places the part there; Esc takes the plane out again.
  def.cursorPlane = [](const CommandState& state) { return awaitedPlane(state); };
  def.cursorPlaneMoved = [](const std::optional<gp_Pnt>& point, CommandState& state, const CommandContext&) {
    return followOnPlane(point, state);
  };
  def.cursorPlaneClicked = [](const gp_Pnt& point, CommandState& state, const CommandContext&) {
    return placeOnPlane(point, state);
  };
  def.escape = [](CommandState& state, const CommandContext&) {
    if (!awaitsPoint(state)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("BUILD plane choice taken out (Esc): %1").arg(state.items(kAt).first().describe());
    state.setItems(kAt, {});
    state.setObject(kAtPick, {});
    state.setObject(kOnPlane, {});
    return true;
  };
  def.apply = [shared, &host, role, profile](CommandState& state, const CommandContext& context) {
    Part part;
    if (!chosenPart(*shared, state, part)) {
      host.showStatus(noPartsText(*shared, role), NoticeLevel::Error);
      return false;
    }
    // Enter (or Place) before the click on a chosen plane: at the plane's
    // origin, wherever the preview follows the mouse (mitcad#1196).
    CommandState placing = state;
    if (awaitsPoint(state)) {
      placing.setObject(kAtPick, QJsonObject{{QStringLiteral("item"), state.items(kAt).first().describe()}});
      qInfo().noquote() << QStringLiteral("BUILD on %1: placed at its origin").arg(state.items(kAt).first().describe());
    }
    const Placement placed = placement(*shared, role, placing, context, part, profile);
    if (!placed.problem.isEmpty()) {
      host.showStatus(placed.problem, NoticeLevel::Error);
      return false;
    }
    return placeNow(*shared, host, role, part, placed.fields, placed.lengthNote, state, context);
  };
  return def;
}

// --- Timber (mitcad#478; app/COMMANDS.md, "Timber")
//
// The BUILD tab's TIMBER group (mitcad#520): Timber for loose members,
// Wall and Floor / Roof for frames (below, "Timber frames"). A timber
// member is mostly drawn from point to point and repeated at a spacing
// (studs at 600, joists at 400, deck boards with a gap), which Profile's
// slot clicks do not need. Timber drives the same model commands as
// Profile (build_place with `to`, `lie` and `spacing`; commands.md, "BUILD:
// timber construction"); the group's pickers list timber only, Profile's
// none. Timber's modes:
//
//   On a member as Profile places: on a side or an end (a free offset, no
//               slot grid), or on a plane (the first one); the default
//               (mitcad#485).
//   Spacing     repeated: between two members (on a member's side, its
//               length bound to the member it meets) or over the picked
//               supporting faces (a board row), at a centre distance or with
//               a gap.
//   Two points  a member from a click to a second click (mitcad#485: the
//               points give its length; nothing follows the mouse); snapped
//               to the ends, the sides and the centre lines of the other
//               members; flat or on edge.

const QString kTimberRole = QStringLiteral("timber");
// The TIMBER group's commands (mitcad#520).
const QString kTimberGroup = QStringLiteral("TIMBER");
const QString kTimberCommand = QStringLiteral("build.timber");
const QString kWallCommand = QStringLiteral("build.wall");
const QString kFloorRoofCommand = QStringLiteral("build.floor_roof");
const QString kHow = QStringLiteral("how");
const QString kPoints = QStringLiteral("points");
const QString kOnMember = QStringLiteral("member");
const QString kSpacing = QStringLiteral("spacing");
// The buttons' order (mitcad#485): On a member first, Two points last.
const Choices kHows = {{kOnMember, QStringLiteral("On a member's side or end, or on a plane")},
                       {kSpacing, QStringLiteral("Spacing: repeated at a center distance, or a board row with a gap")},
                       {kPoints, QStringLiteral("Two points: from a click to a second click")}};
const QHash<QString, QString> kHowIcons = {{kPoints, QStringLiteral("build-timber-points")},
                                           {kOnMember, QStringLiteral("build-place-auto")},
                                           {kSpacing, QStringLiteral("build-timber-spacing")}};
const QString kStart = QStringLiteral("start");
const QString kEndPoint = QStringLiteral("end");
const QString kLie = QStringLiteral("lie");
const Choices kLies = {{QStringLiteral("flat"), QStringLiteral("Flat")}, {QStringLiteral("edge"), QStringLiteral("On edge")}};
const QString kOver = QStringLiteral("over");
const QString kBetween = QStringLiteral("side");
const QString kOverFaces = QStringLiteral("faces");
const Choices kOvers = {{kBetween, QStringLiteral("Between two members: on a member's side, to the member it meets")},
                        {kOverFaces, QStringLiteral("Over the supporting faces: a board row, battens")}};
const QString kFaces = QStringLiteral("faces");
const QString kSpacingMode = QStringLiteral("spacing_mode");
const QString kCentre = QStringLiteral("centre");
const QString kGap = QStringLiteral("gap");
const Choices kSpacingModes = {{kCentre, QStringLiteral("Center distance")}, {kGap, QStringLiteral("Gap: a board row")}};
const QString kDistance = QStringLiteral("distance");
const QString kFit = QStringLiteral("fit");
const Choices kFits = {{QStringLiteral("cut"), QStringLiteral("The last board cut to what is left")},
                       {QStringLiteral("spread"), QStringLiteral("The gaps spread evenly")}};
// The snap distance without a view's (an inspection on the model's worker).
constexpr double kDefaultSnap = 10.0;

// A member of the design as the snaps see it (a timber member or a
// profile): its occurrence path, where it places the member, its length
// and its section (0 when unknown) about its axis (its component's +Z).
struct Member {
  QString path;
  gp_Trsf world;
  double length = 0.0;
  double width = 0.0;  // along its X
  double height = 0.0; // along its Y
};

QVector<Member> membersOf(const Shared& shared) {
  QVector<Member> members;
  for (const BuildPart& part : shared.designParts()) {
    if (part.role != QStringLiteral("profile") || !std::isfinite(part.length) || part.length <= 0.0) {
      continue;
    }
    // The section of its source's row in the catalog, when it is there.
    double width = 0.0;
    double height = 0.0;
    for (const QString& role : {kTimberRole, QStringLiteral("profile")}) {
      for (const Part& known : shared.catalog.parts(role)) {
        if (known.source == part.source && (known.config == part.config || width <= 0.0) && known.sectionWidth() > 0.0) {
          width = known.sectionWidth();
          height = known.sectionHeight();
        }
      }
    }
    for (const auto& placed : shared.placementsOf(part.component)) {
      members.append({placed.first, placed.second, part.length, width, height});
    }
  }
  return members;
}

// The member an occurrence path is in (the longest path it starts with).
std::optional<Member> memberAt(const QVector<Member>& members, const QString& occurrence) {
  std::optional<Member> found;
  for (const Member& member : members) {
    if ((occurrence == member.path || occurrence.startsWith(member.path + QLatin1Char('/'))) &&
        (!found || member.path.size() > found->path.size())) {
      found = member;
    }
  }
  return found;
}

// A point snapped to the members (mitcad#478): to an end's middle within
// `distance`, else to the nearest of their centre lines and their sides'
// centre lines; `kind` says which ("end", "centre line", "side"), empty
// when none is that near.
struct Snapped {
  gp_Pnt point;
  QString kind;
};

Snapped snapToMembers(const QVector<Member>& members, const gp_Pnt& point, double distance) {
  Snapped best{point, QString()};
  double nearest = distance;
  for (const Member& member : members) {
    for (const double z : {0.0, member.length}) {
      const gp_Pnt end = gp_Pnt(0.0, 0.0, z).Transformed(member.world);
      const double d = point.Distance(end);
      if (d <= nearest) {
        nearest = d;
        best = {end, QStringLiteral("end")};
      }
    }
  }
  if (!best.kind.isEmpty()) {
    return best;
  }
  for (const Member& member : members) {
    const gp_Dir axis = gp_Dir(0.0, 0.0, 1.0).Transformed(member.world);
    QVector<std::pair<gp_Pnt, QString>> lines{{gp_Pnt(0.0, 0.0, 0.0), QStringLiteral("center line")}};
    if (member.width > 0.0 && member.height > 0.0) {
      lines.append({gp_Pnt(member.width / 2.0, 0.0, 0.0), QStringLiteral("side")});
      lines.append({gp_Pnt(-member.width / 2.0, 0.0, 0.0), QStringLiteral("side")});
      lines.append({gp_Pnt(0.0, member.height / 2.0, 0.0), QStringLiteral("side")});
      lines.append({gp_Pnt(0.0, -member.height / 2.0, 0.0), QStringLiteral("side")});
    }
    for (const auto& line : lines) {
      const gp_Pnt origin = line.first.Transformed(member.world);
      const double t = std::clamp(gp_Vec(origin, point).Dot(gp_Vec(axis)), 0.0, member.length);
      const gp_Pnt on = origin.Translated(gp_Vec(axis) * t);
      const double d = point.Distance(on);
      if (d <= nearest) {
        nearest = d;
        best = {on, line.second};
      }
    }
  }
  return best;
}

// The plane an item lies in, in the design: a datum plane (moved by its
// occurrence) or a planar face (its outward normal); none for points.
std::optional<cmd::PlaneFrame> planeOfItem(const SelectionItem& item, const CommandContext& context) {
  std::optional<cmd::PlaneFrame> frame = cmd::frameOf(item, context);
  if (frame && item.kind == SelectKind::Plane && !item.occurrence.isEmpty()) {
    if (const std::optional<gp_Trsf> placed = build::worldOf(item.occurrence, context)) {
      frame->origin.Transform(*placed);
      frame->normal.Transform(*placed);
      frame->x.Transform(*placed);
      frame->y.Transform(*placed);
    }
  }
  return frame;
}

// A direction of the design in the BUILD host's coordinates.
gp_Vec vectorInHost(const gp_Vec& world, const CommandContext& context) {
  const QString path = build::buildHost(context).path;
  if (path.isEmpty()) {
    return world;
  }
  const std::optional<gp_Trsf> placement = build::worldOf(path, context);
  return placement ? world.Transformed(placement->Inverted()) : world;
}

QJsonArray vectorJson(const gp_Vec& vector) { return QJsonArray{vector.X(), vector.Y(), vector.Z()}; }

// How many members a set lays out (commands.md, *Spacing sets*), for the
// panel: over `span` with `width` along the set; 0 when it cannot say.
// Boards that join (`cover` above 0, mitcad#477) step by their cover width
// with no gap.
int setCount(const QString& mode, double distance, const QString& fit, double span, double width, double cover = 0.0) {
  constexpr double kTolerance = 1e-6;
  if (!(width > 0.0) || !(span >= width - kTolerance) || !std::isfinite(distance)) {
    return 0;
  }
  if (mode == kGap && cover > kTolerance && cover <= width + kTolerance) {
    return std::max(1, static_cast<int>(std::ceil((span - (width - cover)) / cover - kTolerance)));
  }
  if (mode == kGap) {
    const double step = width + std::max(distance, 0.0);
    return fit == QStringLiteral("spread") ? std::max(1, static_cast<int>(std::floor((span + distance) / step + kTolerance)))
                                           : std::max(1, static_cast<int>(std::ceil(span / step - kTolerance)));
  }
  if (!(distance > 0.0)) {
    return 0;
  }
  const double free = span - width;
  return free <= kTolerance ? 1 : static_cast<int>(std::ceil(free / distance - kTolerance)) + 1;
}

QString howOf(const CommandState& state) {
  const QString how = state.choice(kHow);
  return how == kSpacing || how == kPoints ? how : kOnMember;
}

// Whether the panel places as Profile does: on a member, or a set between
// two members.
bool placesOnMember(const CommandState& state) {
  const QString how = howOf(state);
  return how == kOnMember || (how == kSpacing && state.choice(kOver) != kOverFaces);
}

// The spacing's fields for build_place (commands.md, *Spacing sets*).
QJsonObject spacingFields(const CommandState& state) {
  const QString mode = state.choice(kSpacingMode).isEmpty() ? kCentre : state.choice(kSpacingMode);
  QJsonObject spacing{{QStringLiteral("mode"), mode}, {QStringLiteral("distance"), state.expression(kDistance)}};
  if (mode == kGap) {
    spacing.insert(QStringLiteral("fit"), state.choice(kFit).isEmpty() ? QStringLiteral("cut") : state.choice(kFit));
  }
  return spacing;
}

// The members' width along a set: a flat board's larger side, an edge
// one's smaller.
double widthAcross(const Part& part, const QString& lie) {
  const double w = part.sectionWidth();
  const double t = part.sectionHeight();
  return lie == QStringLiteral("edge") ? std::min(w, t) : std::max(w, t);
}

// A member by two points (mitcad#485): build_place's fields (`at`, `to`,
// `lie`; the points give the length), the points in the design with the
// snaps they took. Nothing until the end is clicked: no direction follows
// the mouse. It runs between the points in 3D (mitcad#509): it lies on the
// start's plane or face only when the end is on that plane too
// (`onPlane`), else it is free between them.
struct PointsPlacement {
  QJsonObject fields;
  QString problem;
  gp_Pnt start;
  gp_Pnt end;
  QString startSnap;
  QString endSnap;
  bool onPlane = false;
};

// How near the end's plane or point must be to the start's plane for the
// member to lie on it (mitcad#509), mm.
constexpr double kCoplanar = 0.01;

// Whether the end clicked is on the start's plane (mitcad#509): the end's
// plane or face is that plane (parallel, through it), or, when the end was
// clicked on no plane, its point is on it.
bool endOnPlane(const cmd::PlaneFrame& plane, const SelectionItem* endItem, const gp_Pnt& end,
                const CommandContext& context) {
  const gp_Vec normal(plane.normal);
  const auto onIt = [&](const gp_Pnt& point) {
    return std::abs(gp_Vec(plane.origin, point).Dot(normal)) <= kCoplanar;
  };
  if (endItem) {
    if (const std::optional<cmd::PlaneFrame> other = planeOfItem(*endItem, context)) {
      return std::abs(gp_Vec(other->normal).Dot(normal)) >= 1.0 - 1e-9 && onIt(other->origin);
    }
  }
  return onIt(end);
}

// `at` for the start's item: the plane or face it lies on (its occurrence
// from the host) when `onPlane`, or only the point (free between the
// points, mitcad#509).
QJsonObject startAt(const SelectionItem& item, const gp_Pnt& start, bool onPlane, const CommandContext& context) {
  QJsonObject at{{QStringLiteral("point"), build::pointJson(build::inHost(start, context))}};
  if (!onPlane || (item.kind != SelectKind::Plane && item.kind != SelectKind::Face)) {
    return at; // a point: grounded through the two points
  }
  const std::optional<QString> path = build::pathFromHost(item, context);
  if (path) {
    at.insert(QStringLiteral("geometry"), item.reference());
    if (!path->isEmpty()) {
      at.insert(QStringLiteral("occurrence"), *path);
    }
  } else if (item.kind == SelectKind::Plane &&
             QStringList{QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz")}.contains(item.owner)) {
    // The root's origin plane while a new design's Component1 is the host
    // (mitcad#421): the host's own plane there.
    at.insert(QStringLiteral("geometry"), item.owner);
  }
  return at;
}

PointsPlacement pointsPlacement(const Shared& shared, const CommandState& state, const CommandContext& context,
                                const Part& part) {
  PointsPlacement result;
  const Selection& starts = state.items(kStart);
  if (starts.isEmpty()) {
    result.problem = QObject::tr("Click where the member starts: a plane, a face or another member.");
    return result;
  }
  const QVector<Member> members = membersOf(shared);
  const std::optional<gp_Pnt> start = build::pickPoint(kStart, state, context);
  if (!start) {
    result.problem = QObject::tr("Click where the member starts.");
    return result;
  }
  const Snapped first = snapToMembers(members, *start, build::pickSnap(kStart, state).value_or(kDefaultSnap));
  result.start = first.point;
  result.startSnap = first.kind;
  const std::optional<gp_Pnt> end = build::pickPoint(kEndPoint, state, context);
  if (!end) {
    result.problem = QObject::tr("Click where the member ends.");
    return result;
  }
  const Snapped last = snapToMembers(members, *end, build::pickSnap(kEndPoint, state).value_or(kDefaultSnap));
  result.end = last.point;
  result.endSnap = last.kind;
  // On the start's plane only when the end is on it too (mitcad#509): then
  // the end, as snapped, is moved onto the plane through the start, as
  // before. Else the member is free between the points in 3D: the end is
  // not moved (a click on another member's side gave a member a few
  // millimetres long, its end moved beside the start).
  const Selection& ends = state.items(kEndPoint);
  if (const std::optional<cmd::PlaneFrame> plane = planeOfItem(starts.first(), context)) {
    result.onPlane = endOnPlane(*plane, ends.isEmpty() ? nullptr : &ends.first(), *end, context);
    if (result.onPlane) {
      const gp_Vec normal(plane->normal);
      result.end.Translate(normal * -gp_Vec(result.start, result.end).Dot(normal));
    }
  }
  if (result.start.Distance(result.end) < 1.0) {
    result.problem = QObject::tr("The end is where the member starts: click where it ends.");
    return result;
  }
  QJsonObject at = startAt(starts.first(), result.start, result.onPlane, context);
  if (const std::optional<double> snap = build::pickSnap(kStart, state)) {
    at.insert(QStringLiteral("snap"), *snap);
  }
  result.fields = {{QStringLiteral("part"), part.ref()},
                   {kAt, at},
                   {QStringLiteral("to"), QJsonObject{{QStringLiteral("point"), build::pointJson(build::inHost(result.end, context))}}},
                   {kLie, state.choice(kLie).isEmpty() ? QStringLiteral("flat") : state.choice(kLie)}};
  return result;
}

// A board row (or battens) over the picked supporting faces: the first
// member by two points across them at their start, repeated along them to
// their end (commands.md, *Spacing sets*: `along` and `length`).
struct RowPlacement {
  QJsonObject fields;
  QString problem;
  double span = kNaN;
  double width = kNaN;
  double cover = 0.0;  // a board that joins the next: its step (mitcad#477)
};

RowPlacement rowPlacement(const Shared& shared, const CommandState& state, const CommandContext& context,
                          const Part& part) {
  RowPlacement result;
  const Selection& faces = state.items(kFaces);
  if (faces.isEmpty()) {
    result.problem = QObject::tr("Click the faces the members lie on (the joists' top faces).");
    return result;
  }
  const QVector<Member> members = membersOf(shared);
  const std::optional<Member> carrier = memberAt(members, faces.first().occurrence);
  const std::optional<cmd::PlaneFrame> plane = planeOfItem(faces.first(), context);
  if (!carrier || !plane) {
    result.problem = QObject::tr("Click flat faces of the members the row lies on.");
    return result;
  }
  // Along the members that carry it, across them in the face's plane.
  gp_Vec along = gp_Vec(gp_Dir(0.0, 0.0, 1.0).Transformed(carrier->world));
  along -= gp_Vec(plane->normal) * along.Dot(gp_Vec(plane->normal));
  if (along.Magnitude() < 1e-9) {
    result.problem = QObject::tr("The members run square to the face: click their side faces.");
    return result;
  }
  along.Normalize();
  const gp_Vec across = gp_Vec(plane->normal).Crossed(along);
  double lowAlong = std::numeric_limits<double>::infinity();
  double highAlong = -lowAlong;
  double lowAcross = lowAlong;
  double highAcross = -lowAlong;
  for (const SelectionItem& face : faces) {
    const TopoDS_Shape shape = context.itemShape(face);
    for (TopExp_Explorer it(shape, TopAbs_VERTEX); it.More(); it.Next()) {
      const gp_Vec p(BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).XYZ());
      lowAlong = std::min(lowAlong, p.Dot(along));
      highAlong = std::max(highAlong, p.Dot(along));
      lowAcross = std::min(lowAcross, p.Dot(across));
      highAcross = std::max(highAcross, p.Dot(across));
    }
  }
  if (!(highAlong > lowAlong) || !(highAcross > lowAcross)) {
    result.problem = QObject::tr("The faces have no extent to lay members over.");
    return result;
  }
  const QString lie = state.choice(kLie).isEmpty() ? QStringLiteral("flat") : state.choice(kLie);
  result.width = widthAcross(part, lie);
  // Boards that join lying flat: edge to edge, the cover width apart.
  if (lie == QStringLiteral("flat") && part.sectionWidth() >= part.sectionHeight()) {
    result.cover = part.joinedCover();
  }
  result.span = highAlong - lowAlong;
  // The first member's axis half its width in from the start, across all
  // the faces, on the first face's plane.
  const gp_Vec origin(plane->origin.XYZ());
  const auto at = [&](double a, double c) {
    return gp_Pnt((origin + along * (a - origin.Dot(along)) + across * (c - origin.Dot(across))).XYZ());
  };
  const double first = lowAlong + (std::isfinite(result.width) ? result.width / 2.0 : 0.0);
  const gp_Pnt start = at(first, lowAcross);
  const gp_Pnt end = at(first, highAcross);
  QJsonObject firstAt = startAt(faces.first(), start, true, context);
  QJsonObject spacing = spacingFields(state);
  spacing.insert(QStringLiteral("along"), vectorJson(vectorInHost(along, context)));
  spacing.insert(QStringLiteral("length"), result.span);
  result.fields = {{QStringLiteral("part"), part.ref()},
                   {kAt, firstAt},
                   {QStringLiteral("to"), QJsonObject{{QStringLiteral("point"), build::pointJson(build::inHost(end, context))}}},
                   {kLie, lie},
                   {QStringLiteral("spacing"), spacing}};
  return result;
}

// The preview's shape and frame of build_place's fields: a set's first
// member's frame, the shape every member's (mitcad#487).
struct Previewed {
  QJsonObject answer;
  TopoDS_Shape shape;
  QString error;
};

Previewed previewOf(QJsonObject fields, const CommandContext& context) {
  Previewed result;
  QJsonObject request = fields;
  request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
  try {
    result.answer = context.queryObject(request);
  } catch (const std::exception& error) {
    result.error = errorText(error);
    return result;
  }
  fields.insert(QStringLiteral("shape"), QStringLiteral("build_preview"));
  result.shape = context.analysisShape(fields);
  result.error = problemsOf(result.answer);
  return result;
}

// The spacing in a sentence: "centre distance 400 mm", "gap 5 mm, the last
// board cut".
QString spacingText(const CommandState& state) {
  const QString mode = state.choice(kSpacingMode).isEmpty() ? kCentre : state.choice(kSpacingMode);
  if (mode == kGap) {
    return QObject::tr("gap %1, %2")
        .arg(state.text(kDistance), state.choice(kFit) == QStringLiteral("spread") ? QObject::tr("the gaps spread")
                                                                                   : QObject::tr("the last board cut"));
  }
  return QObject::tr("center distance %1").arg(state.text(kDistance));
}

// The inspection of a member from a click (its length typed, bound, or to
// the end) or a board row over faces.
Inspection inspectTimber(const SharedPtr& shared, const CommandState& state, const CommandContext& context) {
  Inspection result;
  Part part;
  if (!chosenPart(*shared, state, part)) {
    result.error = noPartsText(*shared, kTimberRole);
    return result;
  }
  const build::Host host = build::buildHost(context);
  const QString addsTo = QObject::tr("Adds to: %1").arg(host.name);
  const QString how = howOf(state);
  const bool row = how == kSpacing;
  QJsonObject fields;
  QString problem;
  QJsonArray markers;
  QStringList lines{QStringLiteral("<b>%1</b>").arg(part.name.toHtmlEscaped()), addsTo.toHtmlEscaped()};
  QString logged;
  PointsPlacement points;
  if (row) {
    const RowPlacement placed = rowPlacement(*shared, state, context, part);
    fields = placed.fields;
    problem = placed.problem;
    if (problem.isEmpty()) {
      const int count = setCount(state.choice(kSpacingMode), state.value(kDistance), state.choice(kFit), placed.span,
                                 placed.width, placed.cover);
      // Boards that join are laid edge to edge (mitcad#477).
      const bool joined = placed.cover > 0.0 && state.choice(kSpacingMode) == kGap;
      const QString spacing = joined ? QObject::tr("edge to edge, cover %1 mm").arg(placed.cover, 0, 'g', 6)
                                     : spacingText(state);
      lines << QObject::tr("Over %1 face(s), %2 mm: %3 member(s), %4")
                   .arg(state.items(kFaces).size())
                   .arg(placed.span, 0, 'f', 1)
                   .arg(count)
                   .arg(spacing.toHtmlEscaped());
      logged = QStringLiteral("row over %1 face(s), span %2 mm, %3 member(s), %4")
                .arg(state.items(kFaces).size())
                .arg(placed.span, 0, 'f', 1)
                .arg(count)
                .arg(spacing);
    } else {
      result.viewHint = QObject::tr("Click the faces the members lie on; Enter places the row");
    }
  } else {
    points = pointsPlacement(*shared, state, context, part);
    fields = points.fields;
    problem = points.problem;
    if (state.items(kStart).isEmpty()) {
      result.viewHint = QObject::tr("Click where the member starts: a plane, a face or a member");
    } else if (state.items(kEndPoint).isEmpty()) {
      result.viewHint = QObject::tr("Click where it ends; Shift+click places at once");
    }
    if (!problem.isEmpty() && !state.items(kStart).isEmpty()) {
      // The start alone: its snap.
      const std::optional<gp_Pnt> start = build::pickPoint(kStart, state, context);
      if (start) {
        const Snapped first =
            snapToMembers(membersOf(*shared), *start, build::pickSnap(kStart, state).value_or(kDefaultSnap));
        markers.append(marker(first.kind.isEmpty() ? QObject::tr("start") : QStringLiteral("> ") + first.kind, first.point));
      }
    }
  }
  result.report = {{QStringLiteral("markers"), markers}};
  if (!problem.isEmpty()) {
    result.text = lines.join(QStringLiteral("<br>"));
    result.error = problem;
    result.log = QStringLiteral("waiting: %1; adds to %2; part %3").arg(problem, host.name, part.name);
    return result;
  }
  const Previewed preview = previewOf(fields, context);
  result.shape = preview.shape;
  const build::Frame frame = build::frameOf(preview.answer.value(QStringLiteral("frame")));
  double length = build::lengthOf(preview.answer.value(QStringLiteral("length")));
  if (!row) {
    // From the start to the end clicked: their distance, as the preview
    // says.
    if (!std::isfinite(length)) {
      length = points.start.Distance(points.end);
    }
    const QString lie = state.choice(kLie).isEmpty() ? QStringLiteral("flat") : state.choice(kLie);
    lines << (std::isfinite(length) ? QObject::tr("Length %1 mm (by two points)").arg(length, 0, 'f', 1)
                                    : QObject::tr("Length"));
    lines << QObject::tr("%1 (Tab: flat or on edge)").arg(cmd::label(kLies, lie));
    // What each point is on, as the model found it, its end cut back to
    // the member it is in, and the plane it lies on or none (mitcad#509).
    const QJsonObject between = preview.answer.value(QStringLiteral("points")).toObject();
    const auto pointLine = [](const QString& what, const QString& kind, const QJsonObject& point) {
      const QString name = str(point, "name");
      QString line = !name.isEmpty()   ? QObject::tr("%1: on %2").arg(what, name.toHtmlEscaped())
                     : kind.isEmpty() ? QObject::tr("%1: where clicked").arg(what)
                                      : QObject::tr("%1: on a member").arg(what);
      if (!kind.isEmpty()) {
        line += QObject::tr(", its %1").arg(kind);
      }
      if (point.value(QStringLiteral("cut")).toBool()) {
        line += QObject::tr("; cut back to its face");
      }
      return line;
    };
    const QJsonObject startOn = between.value(QStringLiteral("start")).toObject();
    const QJsonObject endOn = between.value(QStringLiteral("end")).toObject();
    lines << pointLine(QObject::tr("Start"), points.startSnap, startOn)
          << pointLine(QObject::tr("End"), points.endSnap, endOn);
    lines << (points.onPlane ? QObject::tr("Lies on the start's plane: the end is on it too")
                             : QObject::tr("Free between the points: turned by Lie about its axis, up along +Z"));
    // The log's "<name>[ cut]" or "none" of a point.
    const auto onLog = [](const QJsonObject& point) {
      const QString name = str(point, "name");
      return (name.isEmpty() ? QStringLiteral("none") : name) +
             (point.value(QStringLiteral("cut")).toBool() ? QStringLiteral(" cut") : QString());
    };
    markers.append(marker(points.startSnap.isEmpty() ? QObject::tr("start") : QStringLiteral("> ") + points.startSnap,
                          points.start));
    markers.append(marker(points.endSnap.isEmpty() ? QObject::tr("end") : QStringLiteral("> ") + points.endSnap,
                          points.end));
    markers.append(marker(QStringLiteral("%1 mm").arg(length, 0, 'f', 1),
                          points.start.Translated(gp_Vec(points.start, points.end) * 0.5)));
    result.report.insert(QStringLiteral("markers"), markers);
    logged = QStringLiteral("points, length %1 mm, %2; start snap %3, end snap %4; length by two points; "
                            "start on %5, end on %6; %7")
                 .arg(length, 0, 'f', 2)
                 .arg(lie, points.startSnap.isEmpty() ? QStringLiteral("none") : points.startSnap,
                      points.endSnap.isEmpty() ? QStringLiteral("none") : points.endSnap, onLog(startOn), onLog(endOn),
                      points.onPlane ? QStringLiteral("on the start's plane") : QStringLiteral("free"));
  }
  shared->setPreview(frame, length, false);
  if (result.shape.IsNull() && part.sectionWidth() > 0.0 && std::isfinite(length)) {
    result.shape = build::boxAlong(frame, part.sectionWidth(), part.sectionHeight(), length);
  }
  result.text = lines.join(QStringLiteral("<br>"));
  result.error = preview.error;
  result.log = QStringLiteral("%1; adds to %2; part %3").arg(logged, host.name, part.name);
  if (row) {
    result.log += setPreviewLog(preview.answer, result.shape);
  }
  return result;
}

// Between two members: what Profile's placement previews with the set's
// every member (mitcad#487), and the set's count from the side it stands
// along.
Inspection inspectSetOnSide(const SharedPtr& shared, const CommandState& state, const CommandContext& context) {
  Inspection result = inspectPlacement(shared, kTimberRole, state, context, true, spacingFields(state));
  Part part;
  if (!result.error.isEmpty() || !chosenPart(*shared, state, part)) {
    return result;
  }
  // The set as the model lays it out: from the typed Offset and From
  // (mitcad#492), else the whole side, from end A to end B.
  const Shared::Side side = shared->shownSide();
  // Boards that join step by their cover width when their width runs along
  // the side (mitcad#477).
  const double cover = std::abs(side.width - part.sectionWidth()) < 1e-3 ? part.joinedCover() : 0.0;
  const int count = side.setCount >= 0 ? side.setCount
                                       : setCount(state.choice(kSpacingMode), state.value(kDistance),
                                                  state.choice(kFit), side.length, side.width, cover);
  result.text += QStringLiteral("<br>") +
                 QObject::tr("A set along the member: %1 member(s), %2").arg(count).arg(spacingText(state).toHtmlEscaped());
  result.log += QStringLiteral("; set of %1, %2").arg(count).arg(spacingText(state));
  return result;
}

// Shows an input of Profile's placement only while the Timber panel places
// as Profile does (on a member, or a set between two members).
InputDef onMemberOnly(InputDef input) {
  const std::function<bool(const CommandState&)> own = input.visible;
  input.visible = [own](const CommandState& state) { return placesOnMember(state) && (!own || own(state)); };
  return input;
}

// --- Timber frames (mitcad#479; app/COMMANDS.md, "Timber", Wall and Floor
// / Roof)
//
// The TIMBER group's Wall and Floor / Roof (mitcad#520; the Timber tool's
// Frame mode before) make a wall, a floor or a roof frame from a few values
// (build_frame; commands.md, "BUILD: timber frames"), its members of the
// part chosen and two wood screws at each junction; Timber Frame (MODIFY)
// lays a frame out again with other values. Each tool shows only its
// kind's inputs; Floor / Roof chooses between its two kinds at the top.

const QString kFrameKind = QStringLiteral("frame_kind");
const QString kWall = QStringLiteral("wall");
const QString kFloor = QStringLiteral("floor");
const QString kRoof = QStringLiteral("roof");
// Floor / Roof's kinds (mitcad#520), two icon buttons, their labels the
// tooltips; Wall has no kind to choose.
const Choices kFloorRoofKinds = {{kFloor, QStringLiteral("Floor or ceiling: joists within an outline")},
                                 {kRoof, QStringLiteral("Roof: rafters on two wall frames")}};
const QHash<QString, QString> kFloorRoofIcons = {{kFloor, QStringLiteral("build-floor")},
                                                 {kRoof, QStringLiteral("build-roof")}};
const QString kFrameLength = QStringLiteral("frame_length");
const QString kFrameWidth = QStringLiteral("frame_width");
const QString kFrameAlong = QStringLiteral("frame_along");
const Choices kFrameAlongs = {{QStringLiteral("x"), QStringLiteral("+X")},
                              {QStringLiteral("y"), QStringLiteral("+Y")},
                              {QStringLiteral("-x"), QStringLiteral("-X")},
                              {QStringLiteral("-y"), QStringLiteral("-Y")}};
const QString kFrameHeight = QStringLiteral("frame_height");
const QString kFrameSpacing = QStringLiteral("frame_spacing");
const QString kFrameTop = QStringLiteral("frame_top");
const Choices kFrameTops = {{QStringLiteral("1"), QStringLiteral("One")}, {QStringLiteral("2"), QStringLiteral("Two")}};
const QString kFrameOpenings = QStringLiteral("frame_openings");
const QString kFrameBlocking = QStringLiteral("frame_blocking");
const QString kFrameWalls = QStringLiteral("frame_walls");
const QString kFrameRoof = QStringLiteral("frame_roof");
const Choices kFrameRoofs = {{QStringLiteral("gable"), QStringLiteral("Gable")},
                             {QStringLiteral("mono"), QStringLiteral("Mono-pitch")}};
const QString kFramePitch = QStringLiteral("frame_pitch");
const QString kFramePitchAuto = QStringLiteral("frame_pitch_auto");
const QString kFrameOverhang = QStringLiteral("frame_overhang");
const QString kFrameFasteners = QStringLiteral("frame_fasteners");
// How a wall meets the walls it starts or ends on (mitcad#510).
const QString kFrameCorner = QStringLiteral("frame_corner");
const QString kButt = QStringLiteral("butt");
const QString kLapped = QStringLiteral("lapped");
// Butt, Lapped plates and Straight (mitcad#524: a wall in line beyond an end
// face) as icon buttons with the joints' symbols.
const Choices kFrameCorners = {{kButt, QStringLiteral("Butt")},
                               {kLapped, QStringLiteral("Lapped plates")},
                               {kStraight, QStringLiteral("Straight")}};
const QHash<QString, QString> kFrameCornerIcons = {{kButt, QStringLiteral("build-corner-butt")},
                                                   {kLapped, QStringLiteral("build-corner-lapped")},
                                                   {kStraight, QStringLiteral("build-corner-straight")}};
// Where a wall started on another wall's side goes along it (mitcad#514):
// From and Offset as a side placement's (kFroms); a Start click fills them
// from where the model put it, typed they are sent (`from`, `offset`).
const QString kFrameFrom = QStringLiteral("frame_from");
const QString kFrameOffset = QStringLiteral("frame_offset");
const QString kFrameOffsetTyped = QStringLiteral("frame_offset_typed");
// Its length without an End (mitcad#515): to the wall its run meets, else
// Length; or Length as typed.
const QString kFrameLengthMode = QStringLiteral("frame_length_mode");
const Choices kFrameLengthModes = {{kAuto, QStringLiteral("Auto: to the wall it meets")},
                                   {kTyped, QStringLiteral("Typed length")}};
const QString kWoodRole = QStringLiteral("wood");

QString frameKindOf(const CommandState& state) {
  return state.choice(kFrameKind).isEmpty() ? kWall : state.choice(kFrameKind);
}

// A frame's defaults by its kind: studs and rafters at 600, joists at 400;
// a wall 3600 long, a floor 4000.
QString defaultFrameSpacing(const QString& kind) {
  return kind == kFloor ? QStringLiteral("400 mm") : QStringLiteral("600 mm");
}
QString defaultFrameLength(const QString& kind) {
  return kind == kFloor ? QStringLiteral("4000 mm") : QStringLiteral("3600 mm");
}
// A default as the panel shows it, in the design's unit (mitcad#1165): a
// value that follows a choice while it is the other choice's default
// compares with and writes these.
QString shownDefault(const QString& text, const CommandContext& context) {
  return defaultLengthText(text, context.lengthUnit());
}

// A value input's millimetres: its value, else its text (or the default)
// read as a length.
double millimetresOf(const CommandState& state, const QString& id, double fallback) {
  const double value = state.value(id);
  if (std::isfinite(value)) {
    return value;
  }
  const double typed = build::millimetres(QJsonValue(state.text(id).trimmed()));
  return std::isfinite(typed) ? typed : fallback;
}

// A value input's expression for the model, else its default.
QString expressionOr(const CommandState& state, const QString& id, const QString& fallback) {
  const QString typed = state.expression(id).trimmed();
  return typed.isEmpty() ? fallback : typed;
}

// A frame's openings as the panel's text keeps them, separated by ';': a
// wall's "offset widthxheight [sill]" (no sill, or 0: a door), a floor's "a
// b widthxlength" (from the outline's corner, across and along the joists);
// build/TimberOpenings (mitcad#519) reads and writes them.
QJsonArray openingsOf(const QString& text, bool floor, QString& problem) {
  QString format;
  const std::vector<build::OpeningRow> rows = build::openingRows(text, floor, &format);
  if (!format.isEmpty()) {
    problem = QObject::tr("Openings: %1").arg(format);
    return {};
  }
  return build::openingsJson(rows);
}

// The Openings text of a frame's definition (the reverse).
QString openingsText(const QJsonArray& openings) { return build::openingsText(build::openingRowsOf(openings)); }

// --- The Openings window (mitcad#519; app/COMMANDS.md, "Timber", Openings)
//
// Wall's, Floor / Roof's floor's and Timber Frame's Openings is a tile with
// the openings' summary; a click on it opens the Openings window beside the
// panel, whose rows write the command's text (frame_openings, hidden: the
// command's data, live MCP's), a wall's kind (frame_bearing, hidden) and
// the row selected (frame_opening_selected, hidden: its opening is marked
// in the view) through the main window (DocumentHost::chooseInCommand).

const QString kFrameOpeningsWindow = QStringLiteral("frame_openings_window");
const QString kFrameBearing = QStringLiteral("frame_bearing");
const QString kLoadBearing = QStringLiteral("load_bearing");
const QString kPartition = QStringLiteral("partition");
const Choices kBearings = {{kPartition, QStringLiteral("Non-load-bearing: a flat header, no trimmers")},
                           {kLoadBearing, QStringLiteral("Load-bearing: trimmer studs and a header on edge")}};
const QString kFrameOpeningSelected = QStringLiteral("frame_opening_selected");

bool loadBearingOf(const CommandState& state) { return state.choice(kFrameBearing) == kLoadBearing; }

// The main window's Openings window, wired the first time it is met: what
// it writes goes into the command it shows. Without `create` only one that
// is there already.
OpeningsWindow* openingsWindow(Shared& shared, bool create) {
  if (shared.window == nullptr || shared.host == nullptr) {
    return nullptr;
  }
  OpeningsWindow* window = nullptr;
  if (create) {
    window = OpeningsWindow::of(shared.window);
  } else if (QWidget* top = shared.window->window()) {
    window = top->findChild<OpeningsWindow*>(QString(), Qt::FindDirectChildrenOnly);
  }
  if (window != nullptr && shared.openings.data() != window) {
    shared.openings = window;
    DocumentHost* host = shared.host;
    const auto into = [host](const QString& command, const QString& input, const QString& value) {
      if (!host->chooseInCommand(command, input, value)) {
        qInfo().noquote() << QStringLiteral("BUILD openings: %1 not written, %2 does not run").arg(input, command);
      }
    };
    QObject::connect(window, &OpeningsWindow::textChanged, window, [into](const QString& command, const QString& text) {
      into(command, kFrameOpenings, text);
    });
    QObject::connect(window, &OpeningsWindow::bearingChanged, window, [into](const QString& command, bool bearing) {
      into(command, kFrameBearing, bearing ? kLoadBearing : kPartition);
    });
    QObject::connect(window, &OpeningsWindow::selectedChanged, window, [into](const QString& command, int row) {
      into(command, kFrameOpeningSelected, QString::number(row));
    });
  }
  return window;
}

// What the window shows of a command's state: its openings' text, a
// wall's kind and the sizes the preview shown last laid the frame out
// with (inspectFrame, frameCommand's inspect).
OpeningsWindow::Target openingsTarget(const Shared& shared, const QString& command, const CommandState& state,
                                      bool floor) {
  OpeningsWindow::Target target;
  target.command = command;
  target.floor = floor;
  target.text = state.text(kFrameOpenings);
  target.loadBearing = loadBearingOf(state);
  const Shared::OpeningsSizes sizes = shared.openingsSizes();
  target.wall = sizes.wall;
  target.floorLength = sizes.floorLength;
  target.floorWidth = sizes.floorWidth;
  return target;
}

// The Openings tile, and the hidden inputs the window writes, shown while
// `shown` holds; `floorOf` says whether the state's frame is a floor.
QVector<InputDef> openingsInputs(const SharedPtr& shared, const QString& command,
                                 const std::function<bool(const CommandState&)>& shown,
                                 const std::function<bool(const CommandState&)>& floorOf) {
  const auto hidden = [](const CommandState&) { return false; };
  return {
      choiceInput(kFrameOpeningsWindow, QObject::tr("Openings"), {}, QString())
          .withVisible(shown)
          .withChoiceTile([shared, command, shown, floorOf](const CommandState& state) {
            const bool floor = floorOf(state);
            // The window in step with the panel (live MCP, a member picked);
            // closed while the frame has no openings (Floor / Roof's roof).
            if (OpeningsWindow* window = shared->openings.data()) {
              if (!shown || shown(state)) {
                window->follow(openingsTarget(*shared, command, state, floor));
              } else {
                window->commandEnded(command);
              }
            }
            const std::vector<build::OpeningRow> rows = build::openingRows(state.text(kFrameOpenings), floor);
            const QString title =
                rows.empty() ? QObject::tr("Add openings…") : build::openingsSummary(rows);
            const QString detail = floor || rows.empty() ? QString()
                                   : loadBearingOf(state) ? QObject::tr("Load-bearing")
                                                          : QObject::tr("Non-load-bearing");
            return ChoiceTile{QImage(), title, detail};
          })
          .withButton(kButtonText, QObject::tr("Opens the Openings window: the windows and doors as rows"),
                      [shared, command, floorOf](const CommandState& state, const CommandContext&) {
                        if (OpeningsWindow* window = openingsWindow(*shared, true)) {
                          window->open(openingsTarget(*shared, command, state, floorOf(state)));
                        }
                      })
          .withTooltip(QObject::tr("The openings; a click (or Enter) opens the Openings window")),
      textInput(kFrameOpenings, QObject::tr("Openings Text"))
          .withVisible(hidden)
          .withTooltip(QObject::tr("Openings separated by ';', such as \"600 900x1200 900\"")),
      choiceInput(kFrameBearing, QObject::tr("Wall Kind"), kBearings, kPartition)
          .withVisible(hidden)
          .withTooltip(QObject::tr("A partition's openings get a flat header, a load-bearing wall's trimmers")),
      choiceInput(kFrameOpeningSelected, QObject::tr("Opening Marked"), {}, QStringLiteral("0"))
          .withVisible(hidden)
          .withTooltip(QObject::tr("The opening marked in the view, from 1")),
  };
}

// The preview's openings as markers in the view (mitcad#519): "opening 2"
// at the middle of its framing's members, "> opening 2" for the one the
// Openings window selected.
QJsonArray openingMarkers(const QJsonObject& preview, int selected) {
  QMap<int, std::pair<gp_XYZ, gp_XYZ>> boxes;
  const QString prefix = QStringLiteral("opening.");
  for (const QJsonValue& value : preview.value(QStringLiteral("members")).toArray()) {
    const QJsonObject member = value.toObject();
    const QString key = str(member, "member");
    if (!key.startsWith(prefix)) {
      continue;
    }
    bool ok = false;
    const int number = key.mid(prefix.size()).section(QLatin1Char('.'), 0, 0).toInt(&ok);
    if (!ok) {
      continue;
    }
    for (const char* end : {"start", "end"}) {
      const std::optional<gp_Pnt> point = pointFrom(member.value(QLatin1String(end)));
      if (!point) {
        continue;
      }
      const auto found = boxes.find(number);
      if (found == boxes.end()) {
        boxes.insert(number, {point->XYZ(), point->XYZ()});
        continue;
      }
      gp_XYZ& lo = found->first;
      gp_XYZ& hi = found->second;
      lo.SetCoord(std::min(lo.X(), point->X()), std::min(lo.Y(), point->Y()), std::min(lo.Z(), point->Z()));
      hi.SetCoord(std::max(hi.X(), point->X()), std::max(hi.Y(), point->Y()), std::max(hi.Z(), point->Z()));
    }
  }
  QJsonArray markers;
  for (auto it = boxes.cbegin(); it != boxes.cend(); ++it) {
    const gp_Pnt middle((it.value().first + it.value().second) * 0.5);
    const QString label = QStringLiteral("opening %1").arg(it.key());
    markers.append(marker(it.key() == selected ? QStringLiteral("> ") + label : label, middle));
  }
  return markers;
}

// The openings in the inspection (mitcad#519): "2 openings, non-load-bearing:
// 1 flat header each, no trimmers"; empty without openings.
QString openingsLine(int openings, bool wall, bool loadBearing) {
  if (openings == 0) {
    return {};
  }
  const QString count = openings == 1 ? QObject::tr("1 opening") : QObject::tr("%1 openings").arg(openings);
  if (!wall) {
    return QObject::tr("%1, framed with trimmer and header joists").arg(count);
  }
  return loadBearing ? QObject::tr("%1, load-bearing: a header on edge and trimmer studs each").arg(count)
                     : QObject::tr("%1, non-load-bearing: 1 flat header each, no trimmers").arg(count);
}

// A frame's command ended: its part picker and its Openings window close
// (the picker unless pinned).
std::function<void(bool)> frameEnded(const SharedPtr& shared, const QString& command) {
  return [shared, command](bool) {
    if (PartPicker* picker = shared->picker.data()) {
      picker->commandEnded(command);
    }
    if (OpeningsWindow* window = shared->openings.data()) {
      window->commandEnded(command);
    }
  };
}

// --- The fastener of a frame or a spacing set (mitcad#497; app/COMMANDS.md,
// "Timber", Fasteners)
//
// A wood fastener of the libraries (Fastener: its tile opens the part
// picker of wood fasteners), its size (Auto: sized from the members, or one
// of its rows) and how many at each junction (Count). A frame is fastened
// by default, a spacing set when its Fasteners is on; the choice is
// remembered for the next Timber start (mitcad#485).

const QString kSetFasteners = QStringLiteral("set_fasteners");
const QString kWoodFastener = QStringLiteral("fastener");
const QString kWoodSize = QStringLiteral("fastener_size");
const QString kWoodCount = QStringLiteral("fastener_count");
const QString kWoodAuto = QStringLiteral("auto");

// A wood fastener's key in Fastener: its library and component (its rows
// are Size's).
QString woodKey(const Part& part) { return part.library + QLatin1Char('|') + part.component; }

// The libraries' wood fasteners, each once and without a row, in the
// catalog's order.
QVector<Part> woodFasteners(const Shared& shared) {
  QVector<Part> fasteners;
  for (Part part : shared.catalog.parts(kWoodRole)) {
    const QString key = woodKey(part);
    if (std::any_of(fasteners.begin(), fasteners.end(), [&key](const Part& known) { return known.key == key; })) {
      continue;
    }
    part.config.clear();
    part.partNumber.clear();
    part.ownPreview = false;
    part.key = key;
    fasteners.append(part);
  }
  return fasteners;
}

std::optional<Part> woodFastener(const Shared& shared, const QString& key) {
  for (const Part& part : woodFasteners(shared)) {
    if (part.key == key) {
      return part;
    }
  }
  return std::nullopt;
}

// A wood fastener's rows (Size's choices besides Auto).
QStringList woodRows(const Shared& shared, const QString& key) {
  QStringList rows;
  for (const Part& part : shared.catalog.parts(kWoodRole)) {
    if (woodKey(part) == key && !part.config.isEmpty() && !rows.contains(part.config)) {
      rows << part.config;
    }
  }
  return rows;
}

Choices woodSizeChoices(const Shared& shared, const QString& key) {
  Choices sizes{{kWoodAuto, QObject::tr("Auto: from the members")}};
  for (const QString& row : woodRows(shared, key)) {
    sizes.append({row, row});
  }
  return sizes;
}

// How well a wood fastener suits the default (mitcad#516), 0 best: the one
// a library marks `wood.default` (Mitcad's library: the countersunk wood
// screw), then a screw driven flush (seated by its head's top: `wood.seat`
// head_top, or a countersunk or bugle head without a seat, the model's
// rule, mitcad#512), then another screw, then a nail.
int woodDefaultRank(const Part& part) {
  const QJsonObject wood = part.build.value(QStringLiteral("wood")).toObject();
  if (wood.value(QStringLiteral("default")).toBool()) {
    return 0;
  }
  if (str(wood, "kind") != QStringLiteral("screw")) {
    return 3;
  }
  const QString seat = str(wood, "seat");
  const QString head = str(wood, "head").toLower();
  const bool flush = seat == QStringLiteral("head_top") ||
                     (seat.isEmpty() && (head.contains(QStringLiteral("countersunk")) || head.contains(QStringLiteral("bugle"))));
  return flush ? 1 : 2;
}

// The wood fastener a frame, a spacing set or a timber junction starts
// with where none is chosen (mitcad#516): the first of the best rank, in
// the catalog's order; parts.cend() without one. Not the catalog's order
// alone: by name the pan-head connector screw came first, and every
// frame's screws stood proud of the wood.
QVector<Part>::const_iterator defaultWoodFastener(const QVector<Part>& parts) {
  auto best = parts.cend();
  int bestRank = 4;
  for (auto it = parts.cbegin(); it != parts.cend(); ++it) {
    if (!it->isWoodFastener()) {
      continue;
    }
    const int rank = woodDefaultRank(*it);
    if (rank < bestRank) {
      best = it;
      bestRank = rank;
    }
  }
  return best;
}

// The fastener the tool starts with: the one chosen last, else the
// default wood fastener (defaultWoodFastener).
QString startingWoodFastener(const Shared& shared) {
  const QVector<Part> fasteners = woodFasteners(shared);
  if (std::any_of(fasteners.begin(), fasteners.end(),
                  [&shared](const Part& part) { return part.key == shared.timber.fastener; })) {
    return shared.timber.fastener;
  }
  const auto chosen = defaultWoodFastener(fasteners);
  return chosen == fasteners.cend() ? QString() : chosen->key;
}

// Fastener, Size and Count as the tool remembers them, the wood fasteners
// read (a mode that fastens is shown).
void startFasteners(Shared& shared, CommandState& state, const CommandContext& context) {
  shared.catalog.refresh(kWoodRole, context);
  if (!woodFastener(shared, state.choice(kWoodFastener))) {
    state.setChoice(kWoodFastener, startingWoodFastener(shared));
  }
  const QString size = shared.timber.fastenerSize;
  state.setChoice(kWoodSize, woodRows(shared, state.choice(kWoodFastener)).contains(size) ? size : kWoodAuto);
}

// How many at each junction (at least one; 2 when not a number).
int woodCount(const CommandState& state) {
  double count = state.value(kWoodCount);
  if (!std::isfinite(count)) {
    bool ok = false;
    count = state.text(kWoodCount).trimmed().toDouble(&ok);
    if (!ok) {
      count = 2.0;
    }
  }
  return std::max(1, static_cast<int>(std::lround(count)));
}

// The fastener's `part` for the model: its row unless Auto.
std::optional<QJsonObject> woodFastenerRef(const Shared& shared, const CommandState& state) {
  const std::optional<Part> part = woodFastener(shared, state.choice(kWoodFastener));
  if (!part) {
    return std::nullopt;
  }
  QJsonObject ref = part->ref();
  ref.remove(QStringLiteral("config"));
  const QString size = state.choice(kWoodSize);
  if (!size.isEmpty() && size != kWoodAuto) {
    ref.insert(QStringLiteral("config"), size);
  }
  return ref;
}

// The fastener in words: "3 x Ring nail 3.1x90 at each junction", "2 x
// Wood screw at each junction, sized from the members"; empty without one.
QString woodText(const Shared& shared, const CommandState& state) {
  const std::optional<Part> part = woodFastener(shared, state.choice(kWoodFastener));
  if (!part) {
    return {};
  }
  const QString size = state.choice(kWoodSize);
  const bool sized = size.isEmpty() || size == kWoodAuto;
  return sized ? QObject::tr("%1 × %2 at each junction, sized from the members").arg(woodCount(state)).arg(part->name)
               : QObject::tr("%1 × %2 %3 at each junction").arg(woodCount(state)).arg(part->name, size);
}

// The fastener's tile (as the part's, mitcad#427).
ChoiceTile woodTile(Shared& shared, const CommandState& state) {
  const std::optional<Part> part = woodFastener(shared, state.choice(kWoodFastener));
  if (!part) {
    return {QImage(), QObject::tr("No wood fasteners"), QString()};
  }
  const QString kind = str(part->build.value(QStringLiteral("wood")).toObject(), "kind");
  return {shared.thumbnail(*part), part->name, kind == QStringLiteral("nail") ? QObject::tr("Nail") : QObject::tr("Screw")};
}

// What Fastener asks the part picker for: the wood fasteners, chosen into
// Fastener of `command`.
PickerRequest woodRequest(const Shared& shared, const QString& command, const CommandState& state) {
  PickerRequest request;
  request.command = command;
  request.title = QObject::tr("Fastener");
  request.parts = pickerParts(woodFasteners(shared));
  request.current = state.choice(kWoodFastener);
  request.input = kWoodFastener;
  return request;
}

// Fastener, Size and Count, shown while `shown` holds.
QVector<InputDef> woodInputs(const SharedPtr& shared, const QString& command,
                             const std::function<bool(const CommandState&)>& shown) {
  return {
      choiceInput(kWoodFastener, QObject::tr("Fastener"), {}, QString())
          .withVisible(shown)
          .withChoices([shared](const CommandState&) {
            Choices choices;
            for (const Part& part : woodFasteners(*shared)) {
              choices.append({part.key, part.name});
            }
            return choices;
          })
          .withChoiceTile([shared](const CommandState& state) { return woodTile(*shared, state); })
          .withButton(kButtonText, buttonTip(),
                      [shared, command](const CommandState& state, const CommandContext&) {
                        if (PartPicker* picker = partPicker(*shared, true)) {
                          picker->ask(woodRequest(*shared, command, state));
                        }
                      })
          .withOnChange([shared](CommandState& state, const CommandContext&) {
            // Another fastener: Auto unless it has the row chosen.
            if (!woodRows(*shared, state.choice(kWoodFastener)).contains(state.choice(kWoodSize))) {
              state.setChoice(kWoodSize, kWoodAuto);
            }
            shared->timber.fastener = state.choice(kWoodFastener);
            if (PartPicker* picker = shared->picker.data();
                picker != nullptr && picker->isVisible() && picker->input() == kWoodFastener) {
              picker->setCurrent(state.choice(kWoodFastener));
            }
          })
          .withTooltip(QObject::tr("The wood screw or nail; a click (or Enter) opens the component picker")),
      choiceInput(kWoodSize, QObject::tr("Size"), {{kWoodAuto, QStringLiteral("Auto: from the members")}}, kWoodAuto)
          .withVisible(shown)
          .withChoices([shared](const CommandState& state) {
            return woodSizeChoices(*shared, state.choice(kWoodFastener));
          })
          .withTooltip(QObject::tr("The fastener's size, or Auto: sized from the members it joins")),
      valueInput(kWoodCount, QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("2"))
          .withVisible(shown)
          .withTooltip(QObject::tr("How many at each junction, in a row along the contact")),
  };
}

// The lead of the frame a BUILD part is a member of (empty: none).
QString frameLeadOf(const BuildPart& part) { return str(part.raw, "frame"); }

// A frame's definition as its lead's record keeps it.
QJsonObject frameDefinition(const BuildPart& lead) {
  return lead.raw.value(QStringLiteral("framing")).toObject().value(QStringLiteral("definition")).toObject();
}

// The wall frame a Start or End click is on (a corner or T-junction with
// it): its lead and its base's height in the host.
std::optional<std::pair<QString, double>> wallClicked(const Shared& shared, const CommandState& state,
                                                      const QString& key) {
  const Selection& items = state.items(key);
  const std::optional<BuildPart> member = items.isEmpty() ? std::nullopt : shared.partAt(items.first().occurrence);
  const QString lead = member ? frameLeadOf(*member) : QString();
  if (lead.isEmpty()) {
    return std::nullopt;
  }
  for (const BuildPart& part : shared.designParts()) {
    if (part.component != lead) {
      continue;
    }
    const QJsonObject definition = frameDefinition(part);
    if (str(definition, "kind") != kWall) {
      return std::nullopt;
    }
    const std::optional<gp_Pnt> base = pointFrom(definition.value(QStringLiteral("start")));
    return std::make_pair(lead, base ? base->Z() : 0.0);
  }
  return std::nullopt;
}

// A frame from the panel: build_frame's fields.
struct FramePlacement {
  QJsonObject fields;
  QString problem;
  QString what; // "wall 3600 x 2400 mm", for the panel and the log
};

// A wall's size for the panel and the log: "wall 3600 x 2400 mm".
QString wallWhat(double length, double height) {
  return QObject::tr("wall %1 × %2 mm").arg(length, 0, 'f', 0).arg(height, 0, 'f', 0);
}

// A wall's Corner (mitcad#510): Butt unless Lapped plates or Straight
// (mitcad#524) is chosen.
QString frameCornerOf(const CommandState& state) {
  const QString corner = state.choice(kFrameCorner);
  return corner == kLapped || corner == kStraight ? corner : kButt;
}

// A Start on another body's face (case B, mitcad#524): a face not of a
// timber wall frame's member (those are case A, wallClicked), as the model
// takes it (`face`: the body, the face's name, its occurrence from the
// root).
std::optional<QJsonObject> faceClicked(const Shared& shared, const CommandState& state) {
  const Selection& items = state.items(kStart);
  if (items.isEmpty() || items.first().kind != SelectKind::Face || items.first().name.isEmpty() ||
      wallClicked(shared, state, kStart)) {
    return std::nullopt;
  }
  const SelectionItem& item = items.first();
  return QJsonObject{{QStringLiteral("body"), item.owner},
                     {QStringLiteral("face"), item.name},
                     {QStringLiteral("occurrence"), item.occurrence}};
}

// From as the model takes it: "a", "b" or "middle" (mitcad#514).
QString frameFromOf(const CommandState& state) {
  const QString from = state.choice(kFrameFrom);
  return from == QStringLiteral("b") || from == QStringLiteral("middle") ? from : QStringLiteral("a");
}

// Where along a wall's side (the junction's `from` and `offset`,
// mitcad#514): "flush with end A", "1200 mm from end B", "at the middle",
// "300 mm from the middle"; empty without them.
QString sidePlaceText(const QJsonObject& junction) {
  const QString from = str(junction, "from");
  const QJsonValue offset = junction.value(QStringLiteral("offset"));
  if (from.isEmpty() || !offset.isDouble()) {
    return {};
  }
  const double mm = offset.toDouble();
  const bool zero = std::abs(mm) < 0.05;
  if (from == QStringLiteral("middle")) {
    return zero ? QObject::tr("at the middle") : QObject::tr("%1 mm from the middle").arg(mm, 0, 'f', 0);
  }
  const QString end = from.toUpper();
  return zero ? QObject::tr("flush with end %1").arg(end) : QObject::tr("%1 mm from end %2").arg(mm, 0, 'f', 0).arg(end);
}

// Where a wall started on another wall's side meets it (the answer's
// `junction`, mitcad#493, mitcad#510, mitcad#514): butting "Wall 2: from
// Wall 1's side, flush with end A (butt)", "Wall 2: from Wall 1's side,
// 1200 mm from end B (butt)", "Wall 2: from Wall 1's end A (butt corner)";
// lapped "Wall 2: an L corner at Wall 1's end B (lapped plates)", "Wall 2:
// a T on Wall 1, at the middle"; and the wall its end butts against with
// the wall's length between the faces (mitcad#515): "Wall 4: from Wall 2's
// side, 1200 mm from end A, to Wall 1's side (butt), 2952 mm"; empty
// without one.
QString junctionText(const QJsonValue& value, const QString& name) {
  const QJsonObject junction = value.toObject();
  if (junction.isEmpty()) {
    return {};
  }
  const QString wall = str(junction, "wall");
  const QString kind = str(junction, "kind");
  const QString end = str(junction, "end").toUpper();
  const QString place = sidePlaceText(junction);
  const QJsonObject to = junction.value(QStringLiteral("to")).toObject();
  const QString toWall = str(to, "wall");
  // ", to Wall 1's side" before the corner, ", 2952 mm" after it.
  const QString toText = toWall.isEmpty()                         ? QString()
                         : to.value(QStringLiteral("face")).toBool() ? QObject::tr(", to a face of %1").arg(toWall)
                                                                     : QObject::tr(", to %1's side").arg(toWall);
  const QString run = toWall.isEmpty() ? QString()
                                       : QObject::tr(", %1 mm").arg(junction.value(QStringLiteral("run")).toDouble(), 0, 'f', 0);
  // On another body's face (mitcad#524): where its base is.
  const QJsonObject base = junction.value(QStringLiteral("base")).toObject();
  const double z = base.value(QStringLiteral("z")).toDouble();
  const QString baseText = str(base, "on") == QStringLiteral("floor")
                               ? QObject::tr(", base on %1's top (z %2 mm)").arg(str(base, "body")).arg(z, 0, 'f', 0)
                           : str(base, "on") == QStringLiteral("face")
                               ? QObject::tr(", base at the face's bottom edge (z %1 mm)").arg(z, 0, 'f', 0)
                               : QObject::tr(", base at z %1 mm").arg(z, 0, 'f', 0);
  if (kind == QStringLiteral("face")) {
    return QObject::tr("%1: from a face of %2, %3%4%5%6").arg(name, wall, place, toText, run, baseText);
  }
  if (kind == QStringLiteral("face_end")) {
    return QObject::tr("%1: from the end face of %2 (butt corner)%3%4%5").arg(name, wall, toText, run, baseText);
  }
  if (kind == QStringLiteral("face_straight")) {
    return QObject::tr("%1: in line beyond the end face of %2 (straight)%3%4%5")
        .arg(name, wall, toText, run, baseText);
  }
  if (kind == QStringLiteral("straight")) {
    return QObject::tr("%1: in line beyond %2's end %3 (straight)%4%5").arg(name, wall, end, toText, run);
  }
  if (kind == QStringLiteral("side")) {
    return QObject::tr("%1: from %2's side, %3%4 (butt)%5").arg(name, wall, place, toText, run);
  }
  if (kind == QStringLiteral("end")) {
    return QObject::tr("%1: from %2's end %3%4 (butt corner)%5").arg(name, wall, end, toText, run);
  }
  if (kind == QStringLiteral("L")) {
    return QObject::tr("%1: an L corner at %2's end %3%4 (lapped plates)%5").arg(name, wall, end, toText, run);
  }
  return QObject::tr("%1: a T on %2, %3%4%5").arg(name, wall, place, toText, run);
}

// What Length Auto did (mitcad#536), from the answer's `junction` and the
// wall's `length` as worked out: "Length Auto: the run met a face of Body3,
// the wall 7819 mm", "Length Auto: the run met Wall 2's side, the wall 3453
// mm", "Length Auto: the run met no wall or face, Length 3600 mm"; empty
// without Auto.
QString autoEndText(const QJsonObject& fields, const QJsonObject& answer) {
  if (!fields.value(QStringLiteral("auto_end")).toBool()) {
    return {};
  }
  const QJsonObject junction = answer.value(QStringLiteral("junction")).toObject();
  const QJsonValue worked = answer.value(QStringLiteral("length"));
  const double length = junction.contains(QStringLiteral("length")) ? junction.value(QStringLiteral("length")).toDouble()
                        : worked.isDouble()                         ? worked.toDouble()
                                                                    : kNaN;
  const QString mm = std::isfinite(length) ? QObject::tr("%1 mm").arg(length, 0, 'f', 0) : QObject::tr("?");
  const QJsonObject to = junction.value(QStringLiteral("to")).toObject();
  if (to.value(QStringLiteral("auto")).toBool()) {
    const QString met = to.value(QStringLiteral("face")).toBool()
                            ? QObject::tr("a face of %1").arg(str(to, "wall"))
                            : QObject::tr("%1's side").arg(str(to, "wall"));
    return QObject::tr("Length Auto: the run met %1, the wall %2").arg(met, mm);
  }
  return QObject::tr("Length Auto: the run met no wall or face, Length %1").arg(mm);
}

// A wall's plates made of pieces (the answer's `splices`, mitcad#536):
// "Spliced over studs, pieces up to 5400 mm: top plate at 5400 mm; top
// plate at 4800 mm; bottom plate at 5400 mm"; empty without splices.
// `logged` gets the log's form: "top plate 5400, top plate 4800, bottom
// plate 5400".
QString splicesText(const QJsonValue& value, QString* logged) {
  QStringList parts;
  QStringList logParts;
  double longest = 0.0;
  for (const QJsonValue& entry : value.toArray()) {
    const QJsonObject splice = entry.toObject();
    QStringList at;
    for (const QJsonValue& joint : splice.value(QStringLiteral("at")).toArray()) {
      at << QString::number(joint.toDouble(), 'f', 0);
    }
    const QString role = str(splice, "role");
    parts << QObject::tr("%1 at %2 mm").arg(role, at.join(QStringLiteral(", ")));
    logParts << QStringLiteral("%1 %2").arg(role, at.join(QStringLiteral(" ")));
    longest = std::max(longest, splice.value(QStringLiteral("longest")).toDouble());
  }
  if (logged) {
    *logged = logParts.join(QStringLiteral(", "));
  }
  if (parts.isEmpty()) {
    return {};
  }
  return QObject::tr("Spliced over studs, pieces up to %1 mm: %2")
      .arg(longest, 0, 'f', 0)
      .arg(parts.join(QStringLiteral("; ")));
}

FramePlacement framePlacement(const Shared& shared, const CommandState& state, const CommandContext& context,
                              const Part& part) {
  FramePlacement result;
  const QString kind = frameKindOf(state);
  if (!part.isTimber() || !part.cutToLength()) {
    result.problem = QObject::tr("Choose timber for the frame's members.");
    return result;
  }
  QJsonObject fields{{QStringLiteral("kind"), kind},
                     {QStringLiteral("part"), part.ref()},
                     {QStringLiteral("spacing"), expressionOr(state, kFrameSpacing, defaultFrameSpacing(kind))}};
  // The fastener chosen, its size and count (mitcad#497).
  if (state.checked(kFrameFasteners)) {
    if (const std::optional<QJsonObject> fastener = woodFastenerRef(shared, state)) {
      fields.insert(QStringLiteral("fastener"), *fastener);
      fields.insert(QStringLiteral("fastener_count"), woodCount(state));
    }
  }
  if (kind == kRoof) {
    QStringList walls;
    for (const SelectionItem& item : state.items(kFrameWalls)) {
      const std::optional<BuildPart> member = shared.partAt(item.occurrence);
      const QString lead = member ? frameLeadOf(*member) : QString();
      if (!lead.isEmpty() && !walls.contains(lead)) {
        walls << lead;
      }
    }
    if (walls.size() < 2) {
      result.problem = QObject::tr("Pick a member of each of the two wall frames the roof stands on.");
      return result;
    }
    const QString roof = state.choice(kFrameRoof).isEmpty() ? QStringLiteral("gable") : state.choice(kFrameRoof);
    fields.insert(QStringLiteral("walls"), QJsonArray{walls[0], walls[1]});
    fields.insert(QStringLiteral("roof"), roof);
    fields.insert(QStringLiteral("overhang"), expressionOr(state, kFrameOverhang, QStringLiteral("300 mm")));
    if (roof == QStringLiteral("gable") || !state.checked(kFramePitchAuto)) {
      fields.insert(QStringLiteral("pitch"), expressionOr(state, kFramePitch, QStringLiteral("30 deg")));
    }
    result.what = QObject::tr("%1 roof").arg(cmd::label(kFrameRoofs, roof).toLower());
    result.fields = fields;
    return result;
  }
  // After Place (mitcad#513): the next wall or floor waits for a click, not
  // previewed from the origin on top of the one placed there.
  if (state.checked(kPlacedOne) && state.items(kStart).isEmpty() && state.items(kEndPoint).isEmpty()) {
    result.problem = kind == kFloor ? QObject::tr("Click where the next floor starts.")
                                    : QObject::tr("Click where the next wall starts.");
    return result;
  }
  // From the start clicked (else the host's origin) to the end clicked
  // (else Length along Direction), level with the start.
  gp_Pnt start(0.0, 0.0, 0.0);
  if (const std::optional<gp_Pnt> picked = build::pickPoint(kStart, state, context)) {
    start = build::inHost(*picked, context);
  }
  const QString axis = state.choice(kFrameAlong).isEmpty() ? QStringLiteral("x") : state.choice(kFrameAlong);
  const gp_Vec along = axis == QStringLiteral("y")    ? gp_Vec(0, 1, 0)
                       : axis == QStringLiteral("-x") ? gp_Vec(-1, 0, 0)
                       : axis == QStringLiteral("-y") ? gp_Vec(0, -1, 0)
                                                      : gp_Vec(1, 0, 0);
  const gp_Vec left = gp_Vec(0, 0, 1).Crossed(along);
  const double length = millimetresOf(state, kFrameLength, kind == kFloor ? 4000.0 : 3600.0);
  const double width = millimetresOf(state, kFrameWidth, 3000.0);
  std::optional<gp_Pnt> end;
  if (const std::optional<gp_Pnt> picked = build::pickPoint(kEndPoint, state, context)) {
    end = build::inHost(*picked, context);
    end->SetZ(start.Z());
  }
  QString problem;
  const QJsonArray openings = openingsOf(state.text(kFrameOpenings), kind == kFloor, problem);
  if (!problem.isEmpty()) {
    result.problem = problem;
    return result;
  }
  if (!openings.isEmpty()) {
    fields.insert(QStringLiteral("openings"), openings);
  }
  if (kind == kFloor) {
    QVector<gp_Pnt> corners;
    if (end) {
      corners = {start, gp_Pnt(end->X(), start.Y(), start.Z()), *end, gp_Pnt(start.X(), end->Y(), start.Z())};
    } else {
      corners = {start, start.Translated(along * length), start.Translated(along * length + left * width),
                 start.Translated(left * width)};
    }
    const double a = corners[0].Distance(corners[1]);
    const double b = corners[1].Distance(corners[2]);
    if (a < 1.0 || b < 1.0) {
      result.problem = QObject::tr("Click two opposite corners of the floor, or give its length and width.");
      return result;
    }
    QJsonArray outline;
    for (const gp_Pnt& corner : corners) {
      outline.append(build::pointJson(corner));
    }
    fields.insert(QStringLiteral("outline"), outline);
    if (state.checked(kFrameBlocking)) {
      fields.insert(QStringLiteral("blocking"), 1);
    }
    result.what = QObject::tr("floor %1 × %2 mm").arg(a, 0, 'f', 0).arg(b, 0, 'f', 0);
  } else {
    // A start or end on another wall frame: a corner or a T-junction with
    // it, at its base's height.
    QJsonObject joins;
    const std::optional<std::pair<QString, double>> startWall = wallClicked(shared, state, kStart);
    const std::optional<std::pair<QString, double>> endWall = wallClicked(shared, state, kEndPoint);
    if (startWall) {
      joins.insert(QStringLiteral("start"), startWall->first);
      start.SetZ(startWall->second);
    } else if (endWall) {
      start.SetZ(endWall->second);
    }
    if (endWall) {
      joins.insert(QStringLiteral("end"), endWall->first);
    }
    const QString height = expressionOr(state, kFrameHeight, QStringLiteral("2400 mm"));
    const QJsonObject plates{{QStringLiteral("top"), state.choice(kFrameTop) == QStringLiteral("2") ? 2 : 1}};
    // How it meets the walls it starts or ends on (mitcad#510).
    fields.insert(QStringLiteral("corner"), frameCornerOf(state));
    // Its openings framed by its kind (mitcad#519): a new wall a partition
    // unless Load-bearing is chosen.
    fields.insert(QStringLiteral("load_bearing"), loadBearingOf(state));
    if (startWall) {
      // A start on another wall's side or end face (mitcad#493, mitcad#510):
      // square to it, from the side or the end butting, else an L corner
      // near one of its ends or a T, as the model works it out; as far as
      // the end clicked is along it, else Length.
      fields.insert(QStringLiteral("joins"), joins);
      fields.insert(QStringLiteral("square"), true);
      fields.insert(QStringLiteral("start"), build::pointJson(start));
      if (end) {
        fields.insert(QStringLiteral("end"), build::pointJson(*end));
      } else {
        fields.insert(QStringLiteral("length"), expressionOr(state, kFrameLength, defaultFrameLength(kind)));
        // Auto: to the wall its run meets, else Length (mitcad#515).
        fields.insert(QStringLiteral("auto_end"), state.choice(kFrameLengthMode) != kTyped);
      }
      // Where along that wall: From and Offset once typed, else the click,
      // snapped (mitcad#514).
      if (state.checked(kFrameOffsetTyped)) {
        fields.insert(QStringLiteral("from"), frameFromOf(state));
        fields.insert(QStringLiteral("offset"), expressionOr(state, kFrameOffset, QStringLiteral("0 mm")));
      }
      if (const std::optional<double> snap = build::pickSnap(kStart, state)) {
        fields.insert(QStringLiteral("snap"), *snap);
      }
      fields.insert(QStringLiteral("height"), height);
      fields.insert(QStringLiteral("plates"), plates);
      // The length the model works out replaces it (inspectFrame).
      result.what = wallWhat(length, millimetresOf(state, kFrameHeight, 2400.0));
      result.fields = fields;
      return result;
    }
    if (const std::optional<QJsonObject> face = faceClicked(shared, state)) {
      // A start on another body's face (case B, mitcad#524): square from
      // it on its outward side, on the floor under the click, the body not
      // changed; as far as the end clicked, else to the wall or face its
      // run meets (Auto), else Length.
      fields.insert(QStringLiteral("square"), true);
      fields.insert(QStringLiteral("face"), *face);
      fields.insert(QStringLiteral("start"), build::pointJson(start));
      if (!joins.isEmpty()) {
        fields.insert(QStringLiteral("joins"), joins);
      }
      if (end) {
        fields.insert(QStringLiteral("end"), build::pointJson(*end));
      } else {
        fields.insert(QStringLiteral("length"), expressionOr(state, kFrameLength, defaultFrameLength(kind)));
        fields.insert(QStringLiteral("auto_end"), state.choice(kFrameLengthMode) != kTyped);
      }
      if (state.checked(kFrameOffsetTyped)) {
        fields.insert(QStringLiteral("from"), frameFromOf(state));
        fields.insert(QStringLiteral("offset"), expressionOr(state, kFrameOffset, QStringLiteral("0 mm")));
      }
      if (const std::optional<double> snap = build::pickSnap(kStart, state)) {
        fields.insert(QStringLiteral("snap"), *snap);
      }
      fields.insert(QStringLiteral("height"), height);
      fields.insert(QStringLiteral("plates"), plates);
      result.what = wallWhat(length, millimetresOf(state, kFrameHeight, 2400.0));
      result.fields = fields;
      return result;
    }
    gp_Pnt last = end ? *end : start.Translated(along * length);
    last.SetZ(start.Z());
    if (start.Distance(last) < 1.0) {
      result.problem = QObject::tr("Click where the wall ends, or give its length.");
      return result;
    }
    if (!joins.isEmpty()) {
      fields.insert(QStringLiteral("joins"), joins);
    }
    fields.insert(QStringLiteral("start"), build::pointJson(start));
    fields.insert(QStringLiteral("end"), build::pointJson(last));
    fields.insert(QStringLiteral("height"), height);
    fields.insert(QStringLiteral("plates"), plates);
    result.what = wallWhat(start.Distance(last), millimetresOf(state, kFrameHeight, 2400.0));
  }
  result.fields = fields;
  return result;
}

// A frame's preview (build_preview with `frame`, and `component` for a
// frame laid out again): its members as boxes, their roles counted.
struct FramePreview {
  QJsonObject answer;
  TopoDS_Shape shape;
  QString counts; // "5 stud, 2 king stud, ..."
  QString error;
};

FramePreview previewFrame(const QJsonObject& frame, const QString& component, const CommandContext& context) {
  FramePreview result;
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("build_preview")}, {QStringLiteral("frame"), frame}};
  if (!component.isEmpty()) {
    request.insert(QStringLiteral("component"), component);
  }
  try {
    result.answer = context.queryObject(request);
  } catch (const std::exception& error) {
    result.error = errorText(error);
    return result;
  }
  result.error = problemsOf(result.answer);
  QStringList counts;
  const QJsonObject roles = result.answer.value(QStringLiteral("roles")).toObject();
  for (auto it = roles.begin(); it != roles.end(); ++it) {
    counts << QStringLiteral("%1 %2").arg(it.value().toInt()).arg(it.key());
  }
  result.counts = counts.join(QStringLiteral(", "));
  BRep_Builder builder;
  TopoDS_Compound boxes;
  builder.MakeCompound(boxes);
  bool any = false;
  for (const QJsonValue& value : result.answer.value(QStringLiteral("members")).toArray()) {
    const QJsonObject member = value.toObject();
    const std::optional<gp_Pnt> from = pointFrom(member.value(QStringLiteral("start")));
    const std::optional<gp_Pnt> to = pointFrom(member.value(QStringLiteral("end")));
    const std::optional<gp_Pnt> x = pointFrom(member.value(QStringLiteral("x")));
    const QJsonArray section = member.value(QStringLiteral("section")).toArray();
    if (!from || !to || !x || section.size() < 2 || from->Distance(*to) < 1e-6 ||
        gp_Vec(x->XYZ()).Magnitude() < 1e-9) {
      continue;
    }
    build::Frame at;
    at.origin = *from;
    at.z = gp_Dir(gp_Vec(*from, *to));
    at.x = gp_Dir(gp_Vec(x->XYZ()));
    at.valid = true;
    const TopoDS_Shape box = build::boxAlong(at, section[0].toDouble(), section[1].toDouble(), from->Distance(*to));
    if (!box.IsNull()) {
      builder.Add(boxes, box);
      any = true;
    }
  }
  if (any) {
    result.shape = boxes;
  }
  return result;
}

// The inspection of Wall and Floor / Roof (mitcad#520).
Inspection inspectFrame(const SharedPtr& shared, const CommandState& state, const CommandContext& context) {
  Inspection result;
  Part part;
  if (!chosenPart(*shared, state, part)) {
    result.error = noPartsText(*shared, kTimberRole);
    return result;
  }
  const build::Host host = build::buildHost(context);
  const QString kind = frameKindOf(state);
  QStringList lines{QStringLiteral("<b>%1</b>").arg(part.name.toHtmlEscaped()),
                    QObject::tr("Adds to: %1").arg(host.name).toHtmlEscaped()};
  const FramePlacement placed = framePlacement(*shared, state, context, part);
  if (kind != kRoof && state.items(kStart).isEmpty()) {
    // After Place only a click (mitcad#513).
    const QString what = kind == kFloor ? QObject::tr("floor") : QObject::tr("wall");
    result.viewHint = state.checked(kPlacedOne)
                          ? QObject::tr("Click where the next %1 starts").arg(what)
                          : QObject::tr("Click where the %1 starts, or press Enter: from the origin along Direction")
                                .arg(what);
  } else if (kind == kRoof && state.items(kFrameWalls).size() < 2) {
    result.viewHint = QObject::tr("Click a member of each of the two walls the roof stands on");
  } else if (placed.fields.value(QStringLiteral("square")).toBool() && state.items(kEndPoint).isEmpty()) {
    result.viewHint = QObject::tr("Click where the wall ends, or press Enter: Length square to the wall clicked");
  }
  if (!placed.problem.isEmpty()) {
    result.text = lines.join(QStringLiteral("<br>"));
    result.error = placed.problem;
    result.log = QStringLiteral("frame %1 waiting: %2; adds to %3; part %4").arg(kind, placed.problem, host.name, part.name);
    return result;
  }
  const FramePreview preview = previewFrame(placed.fields, QString(), context);
  result.shape = preview.shape;
  const int count = preview.answer.value(QStringLiteral("count")).toInt();
  const QString name = str(preview.answer, "name");
  // A wall started on another wall's side: the corner or T it makes, its
  // length as the model works it out (mitcad#493).
  const QJsonValue junction = preview.answer.value(QStringLiteral("junction"));
  const QString joined = junctionText(junction, name);
  // Its run meets a wall (mitcad#515): Enter places it to that wall.
  const QJsonObject metWall = junction.toObject().value(QStringLiteral("to")).toObject();
  if (metWall.value(QStringLiteral("auto")).toBool() && state.items(kEndPoint).isEmpty()) {
    // Or another body's face (mitcad#524).
    result.viewHint = metWall.value(QStringLiteral("face")).toBool()
                          ? QObject::tr("Click where the wall ends, or press Enter: to a face of %1").arg(str(metWall, "wall"))
                          : QObject::tr("Click where the wall ends, or press Enter: to %1's side").arg(str(metWall, "wall"));
  }
  const QString what =
      joined.isEmpty() ? placed.what
                       : wallWhat(junction.toObject().value(QStringLiteral("length")).toDouble(),
                                  millimetresOf(state, kFrameHeight, 2400.0));
  // The frame is a component of its own in the host (mitcad#522).
  const QString addsTo = QObject::tr("%1 in %2").arg(name, host.name);
  lines[1] = QObject::tr("Adds to: %1").arg(addsTo).toHtmlEscaped();
  lines << QObject::tr("%1, %2: %3 member(s)").arg(name, what).arg(count).toHtmlEscaped();
  if (!joined.isEmpty()) {
    lines << joined.toHtmlEscaped();
  }
  // What Length Auto ran to and the wall's length, also when the preview
  // is refused (mitcad#536).
  const QString autoEnd = autoEndText(placed.fields, preview.answer);
  if (!autoEnd.isEmpty()) {
    lines << autoEnd.toHtmlEscaped();
  }
  if (!preview.counts.isEmpty()) {
    lines << preview.counts.toHtmlEscaped();
  }
  // Plates longer than their timber comes, spliced over studs (mitcad#536).
  QString splicesLogged;
  const QString spliced = splicesText(preview.answer.value(QStringLiteral("splices")), &splicesLogged);
  if (!spliced.isEmpty()) {
    lines << spliced.toHtmlEscaped();
  }
  // The openings by the wall's kind, marked in the view, and the sizes the
  // Openings window checks its rows against (mitcad#519).
  const int openings = static_cast<int>(placed.fields.value(QStringLiteral("openings")).toArray().size());
  const bool loadBearing = placed.fields.value(QStringLiteral("load_bearing")).toBool(true);
  const QString openingText = openingsLine(openings, kind == kWall, loadBearing);
  if (!openingText.isEmpty()) {
    lines << openingText.toHtmlEscaped();
  }
  const int marked = state.choice(kFrameOpeningSelected).toInt();
  result.report = {{QStringLiteral("markers"), openingMarkers(preview.answer, marked)}};
  {
    Shared::OpeningsSizes sizes;
    if (kind == kWall) {
      const std::optional<gp_Pnt> from = pointFrom(placed.fields.value(QStringLiteral("start")));
      const std::optional<gp_Pnt> to = pointFrom(placed.fields.value(QStringLiteral("end")));
      sizes.wall.length = !joined.isEmpty() ? junction.toObject().value(QStringLiteral("length")).toDouble()
                          : from && to   ? std::hypot(to->X() - from->X(), to->Y() - from->Y())
                                         : kNaN;
      sizes.wall.height = millimetresOf(state, kFrameHeight, 2400.0);
      sizes.wall.topPlates = state.choice(kFrameTop) == QStringLiteral("2") ? 2 : 1;
      sizes.wall.studThick = std::min(part.sectionWidth(), part.sectionHeight());
      sizes.wall.studDepth = std::max(part.sectionWidth(), part.sectionHeight());
      sizes.wall.loadBearing = loadBearing;
    } else if (kind == kFloor) {
      const QJsonArray outline = placed.fields.value(QStringLiteral("outline")).toArray();
      if (outline.size() == 4) {
        const gp_Pnt p0 = pointFrom(outline[0]).value_or(gp_Pnt());
        const gp_Pnt p1 = pointFrom(outline[1]).value_or(gp_Pnt());
        const gp_Pnt p2 = pointFrom(outline[2]).value_or(gp_Pnt());
        sizes.floorLength = p0.Distance(p1);
        sizes.floorWidth = p1.Distance(p2);
      }
    }
    shared->setOpeningsSizes(sizes);
  }
  const bool fastened = placed.fields.contains(QStringLiteral("fastener"));
  const QString fastening = woodText(*shared, state); // mitcad#497
  lines << (fastened ? fastening.toHtmlEscaped()
                     : state.checked(kFrameFasteners) ? QObject::tr("No wood fasteners in the libraries: no fasteners")
                                                      : QObject::tr("No fasteners"));
  QStringList warnings;
  for (const QJsonValue& warning : preview.answer.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString();
    lines << QStringLiteral("<i>%1</i>").arg(warning.toString().toHtmlEscaped());
  }
  result.text = lines.join(QStringLiteral("<br>"));
  result.error = preview.error;
  result.log = QStringLiteral("frame %1 %2, %3, %4 member(s) (%5); fasteners %6%7; adds to %8; part %9")
                   .arg(kind, name, what)
                   .arg(count)
                   .arg(preview.counts, fastened ? QStringLiteral("yes") : QStringLiteral("no"),
                        warnings.isEmpty() ? QString() : QStringLiteral("; warnings: ") + warnings.join(QStringLiteral(" / ")),
                        addsTo, part.name);
  if (!joined.isEmpty()) {
    result.log += QStringLiteral("; %1").arg(joined);
  }
  if (kind == kWall) {
    result.log += QStringLiteral("; corner %1").arg(frameCornerOf(state)); // mitcad#510
  }
  if (openings > 0) {
    // mitcad#519: their framing by the wall's kind, the one marked.
    result.log += QStringLiteral("; openings %1%2%3")
                      .arg(openings)
                      .arg(kind != kWall ? QString()
                           : loadBearing ? QStringLiteral(", load-bearing")
                                         : QStringLiteral(", non-load-bearing"),
                           marked > 0 ? QStringLiteral(", opening %1 marked").arg(marked) : QString());
  }
  if (placed.fields.contains(QStringLiteral("auto_end"))) {
    // Without an End on a wall started on another (mitcad#515).
    result.log += placed.fields.value(QStringLiteral("auto_end")).toBool() ? QStringLiteral("; length auto")
                                                                         : QStringLiteral("; length typed");
  }
  if (!autoEnd.isEmpty()) {
    result.log += QStringLiteral("; %1").arg(autoEnd); // mitcad#536
  }
  if (!splicesLogged.isEmpty()) {
    result.log += QStringLiteral("; spliced %1").arg(splicesLogged); // mitcad#536
  }
  if (!result.error.isEmpty()) {
    // A refused preview's inspection logs only its reason: what it still
    // names, for UI tests (mitcad#536).
    qInfo().noquote() << QStringLiteral("Frame preview refused: %1; reason: %2").arg(result.log, result.error);
  }
  if (fastened) {
    result.log += QStringLiteral("; fastener %1").arg(fastening); // mitcad#497
  }
  return result;
}

// Log measured geometry after the model has placed it, so UI acceptance
// checks seating rather than the requested metadata or preview alone.
void logTimberSeating(const CommandContext& context) {
  const QJsonArray parts = context.queryObject({{QStringLiteral("query"), QStringLiteral("build_parts")}})
                               .value(QStringLiteral("parts")).toArray();
  for (const QJsonValue& value : parts) {
    const QJsonObject part = value.toObject();
    const QJsonObject seat = part.value(QStringLiteral("seating")).toObject();
    if (seat.isEmpty()) {
      continue;
    }
    const double distance = seat.value(QStringLiteral("head_top_distance")).toDouble();
    qInfo().noquote() << QStringLiteral("Timber fastener %1: head top %2 mm from entry face")
                             .arg(str(part, "component"))
                             .arg(std::abs(distance) <= 1e-6 ? 0.0 : distance, 0, 'g', 12);
    // Its shaft through its junction's members (mitcad#521: none beside
    // them).
    if (seat.contains(QStringLiteral("members"))) {
      qInfo().noquote() << QStringLiteral("Fastener shaft %1 through %2 of %3 junction member(s)")
                               .arg(str(part, "component"))
                               .arg(seat.value(QStringLiteral("members")).toInt())
                               .arg(seat.value(QStringLiteral("of")).toInt());
    }
  }
}

// For UI tests (mitcad#522): how many joints each component holds, the
// BUILD host's first (a frame's own component holds its members' joints,
// the host only those between frames).
void logJointsByComponent(const CommandContext& context) {
  const build::Host host = build::buildHost(context);
  QMap<QString, int> counts;
  try {
    const QJsonArray joints =
        context.queryObject({{QStringLiteral("query"), QStringLiteral("joints")}}).value(QStringLiteral("joints")).toArray();
    for (const QJsonValue& value : joints) {
      counts[str(value.toObject(), "component")] += 1;
    }
  } catch (const std::exception&) {
    return;
  }
  QStringList parts{QStringLiteral("%1 %2").arg(host.name).arg(counts.value(host.component))};
  for (auto it = counts.cbegin(); it != counts.cend(); ++it) {
    if (it.key() != host.component) {
      parts << QStringLiteral("%1 %2").arg(cmd::componentName(it.key(), context)).arg(it.value());
    }
  }
  qInfo().noquote() << QStringLiteral("Joints by component: %1").arg(parts.join(QStringLiteral(", ")));
}

// Places the frame Wall or Floor / Roof shows.
bool applyFrame(Shared& shared, DocumentHost& host, CommandState& state, const CommandContext& context,
                const Part& part) {
  const FramePlacement placed = framePlacement(shared, state, context, part);
  if (!placed.problem.isEmpty()) {
    host.showStatus(placed.problem, NoticeLevel::Error);
    return false;
  }
  QJsonObject command = placed.fields;
  command.insert(QStringLiteral("cmd"), QStringLiteral("build_frame"));
  QJsonObject result;
  if (!host.runModelCommand(command, &result)) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Placed frame %1 (%2, lead %3): %4 member(s), %5 junction(s) fastened")
                           .arg(str(result, "name"), str(result, "kind"), str(result, "frame"))
                           .arg(result.value(QStringLiteral("count")).toInt())
                           .arg(result.value(QStringLiteral("fasteners")).toInt());
  const QString joined = junctionText(result.value(QStringLiteral("junction")), str(result, "name"));
  if (!joined.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Frame junction: %1").arg(joined);
  }
  for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
    qInfo().noquote() << QStringLiteral("Frame warning: %1").arg(warning.toString());
  }
  // Its plates made of pieces over studs (mitcad#536).
  QString splicesLogged;
  if (!splicesText(result.value(QStringLiteral("splices")), &splicesLogged).isEmpty()) {
    qInfo().noquote() << QStringLiteral("Frame spliced: %1").arg(splicesLogged);
  }
  // Its own component in the host (mitcad#522).
  const QString component = str(result, "frame_component");
  if (!component.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Frame component %1 (%2) in %3")
                             .arg(cmd::componentName(component, context), component, build::buildHost(context).name);
  }
  logJointsByComponent(context);
  logTimberSeating(context);
  shared.refreshDesign(context);
  // The next one waits for a click (mitcad#513).
  state.setChecked(kPlacedOne, true);
  state.setItems(kStart, {});
  state.setItems(kEndPoint, {});
  state.setObject(kStart + QStringLiteral("_pick"), {});
  state.setObject(kEndPoint + QStringLiteral("_pick"), {});
  state.setItems(kFrameWalls, {});
  // The next wall: where along a wall by its click, its length Auto again
  // (mitcad#514, mitcad#515).
  state.setChecked(kFrameOffsetTyped, false);
  state.setChoice(kFrameLengthMode, kAuto);
  return true;
}

// A Start click on another wall (mitcad#514): From and Offset filled from
// where the model puts the new wall (snapped flush with an end, centred,
// else from the nearer end), left to the click until typed.
void fillFrameStart(const Shared& shared, CommandState& state, const CommandContext& context) {
  state.setChecked(kFrameOffsetTyped, false);
  Part part;
  // On a wall frame (mitcad#514) or another body's face (mitcad#524).
  const bool onStart = wallClicked(shared, state, kStart) || faceClicked(shared, state);
  if (frameKindOf(state) != kWall || !onStart || !chosenPart(shared, state, part)) {
    return;
  }
  const FramePlacement placed = framePlacement(shared, state, context, part);
  if (!placed.problem.isEmpty()) {
    return;
  }
  const FramePreview preview = previewFrame(placed.fields, QString(), context);
  const QJsonObject junction = preview.answer.value(QStringLiteral("junction")).toObject();
  const QJsonValue offset = junction.value(QStringLiteral("offset"));
  if (str(junction, "from").isEmpty() || !offset.isDouble()) {
    qInfo().noquote() << QStringLiteral("BUILD frame start click: %1")
                             .arg(preview.error.isEmpty() ? str(junction, "kind") : preview.error);
    return;
  }
  state.setChoice(kFrameFrom, str(junction, "from"));
  state.setText(kFrameOffset, cmd::lengthText(std::round(offset.toDouble() * 100.0) / 100.0, context));
  state.setValue(kFrameOffset, kNaN);
  qInfo().noquote() << QStringLiteral("BUILD frame start click: From %1, Offset %2 (%3)")
                           .arg(cmd::label(kFroms, str(junction, "from")), state.text(kFrameOffset),
                                str(junction, "kind"));
}

// The wall frame a timber member is in (its lead), else empty (also for
// its fasteners): Floor / Roof's Walls and the view's context menu
// (mitcad#520). A definition without a kind is a wall's.
QString wallLeadOf(const QVector<BuildPart>& parts, const std::optional<BuildPart>& member) {
  const QString lead = member && member->isTimber() ? frameLeadOf(*member) : QString();
  if (lead.isEmpty()) {
    return {};
  }
  for (const BuildPart& part : parts) {
    if (part.component == lead) {
      const QString kind = str(frameDefinition(part), "kind");
      return kind.isEmpty() || kind == kWall ? lead : QString();
    }
  }
  return {};
}

// Start and End of a wall or a floor (mitcad#479): a click each; Start on
// another wall fills From and Offset (mitcad#514).
InputDef frameStartInput(const SharedPtr& shared, const QString& tooltip,
                         const std::function<bool(const CommandState&)>& shown) {
  InputDef input = selectionInput(kStart, QObject::tr("Start"),
                                  SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex, 0, 1)
                       .withOnPick([shared](const SelectionItem& item, CommandState& state,
                                            const CommandContext& context) {
                         build::recordPick(kStart, item, state, context);
                         fillFrameStart(*shared, state, context);
                       },
                                   false)
                       .withTooltip(tooltip);
  if (shown) {
    input.withVisible(shown);
  }
  return input;
}

InputDef frameEndInput(const QString& tooltip, const std::function<bool(const CommandState&)>& shown) {
  InputDef input = selectionInput(kEndPoint, QObject::tr("End"),
                                  SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex, 0, 1)
                       .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                         build::recordPick(kEndPoint, item, state, context);
                       },
                                   false)
                       .withShiftClickApplies()
                       .withTooltip(tooltip);
  if (shown) {
    input.withVisible(shown);
  }
  return input;
}

// The fastener chosen, its size and count, remembered for the TIMBER
// group's next starts (mitcad#497, shared by its tools).
void rememberFastener(Shared& shared, const CommandState& state) {
  shared.timber.fastener = state.choice(kWoodFastener);
  shared.timber.fastenerSize = state.choice(kWoodSize).isEmpty() ? kWoodAuto : state.choice(kWoodSize);
  shared.timber.fastenerCount = QString::number(woodCount(state));
}

// What Wall and Floor / Roof start with: the part (startPicker), whether
// they are fastened and with what (the fastener shared, the switch each
// tool's own), logged; a pinned part picker follows.
void startFrameTool(Shared& shared, const QString& command, bool fastened, CommandState& state,
                    const CommandContext& context) {
  startPicker(shared, kTimberRole, state, context);
  state.setChecked(kFrameFasteners, fastened);
  state.setText(kWoodCount, shared.timber.fastenerCount);
  state.setValue(kWoodCount, kNaN);
  startFasteners(shared, state, context);
  qInfo().noquote() << QStringLiteral("BUILD %1 adds to %2; fasteners %3; %4")
                           .arg(command.mid(6), build::buildHost(context).name,
                                fastened ? QStringLiteral("on") : QStringLiteral("off"), woodText(shared, state));
  if (PartPicker* picker = pinnedPicker(shared)) {
    picker->ask(roleRequest(shared, kTimberRole, state, command));
  }
}

// Fasteners and, under it while it is on, Fastener, Size and Count
// (mitcad#497).
QVector<InputDef> frameFastenerInputs(const SharedPtr& shared, const QString& command, const QString& tooltip) {
  QVector<InputDef> inputs{checkInput(kFrameFasteners, QObject::tr("Fasteners"), true).withTooltip(tooltip)};
  inputs += woodInputs(shared, command, [](const CommandState& state) { return state.checked(kFrameFasteners); });
  return inputs;
}

// --- Wall (mitcad#520; app/COMMANDS.md, "Timber", Wall)

CommandDef wallCommand(const SharedPtr& shared, DocumentHost& host) {
  CommandDef def;
  def.id = kWallCommand;
  def.name = QObject::tr("Wall");
  def.icon = QStringLiteral("build-wall");
  def.tooltip = QObject::tr("Places a timber wall frame: plates, studs and the framing of its openings");
  def.tab = kTab;
  def.group = kTimberGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("frame"),  QStringLiteral("stud"),   QStringLiteral("plate"),
                  QStringLiteral("header"), QStringLiteral("window"), QStringLiteral("door"),
                  QStringLiteral("timber"), QStringLiteral("wood"),   QStringLiteral("build")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  // Without an End clicked; started on another wall (mitcad#514, mitcad#515).
  const auto noEnd = [](const CommandState& state) { return state.items(kEndPoint).isEmpty(); };
  // Or on another body's face (mitcad#524): the same From, Offset and Length.
  const auto onWall = [shared](const CommandState& state) {
    return wallClicked(*shared, state, kStart).has_value() || faceClicked(*shared, state).has_value();
  };
  const auto onFace = [shared](const CommandState& state) { return faceClicked(*shared, state).has_value(); };
  def.inputs = pickerInputs(shared, kTimberRole, def.id);
  def.inputs.append(frameStartInput(
      shared, QObject::tr("Where the wall's center line starts: a plane, a face or another wall's member"), {}));
  def.inputs.append(
      frameEndInput(QObject::tr("Where the wall's center line ends, level with its start (Shift+click places)"), {}));
  def.inputs.append(choiceInput(kFrameLengthMode, QObject::tr("Length"), kFrameLengthModes, kAuto)
                        .withVisible([onWall, noEnd](const CommandState& state) { return onWall(state) && noEnd(state); })
                        .withOnChange([](CommandState& state, const CommandContext&) {
                          qInfo().noquote() << QStringLiteral("BUILD frame length: %1")
                                                   .arg(cmd::label(kFrameLengthModes, state.choice(kFrameLengthMode)));
                        })
                        .withTooltip(QObject::tr("Without an end clicked: to the wall its run meets (else Length), "
                                                 "or Length as typed")));
  def.inputs.append(valueInput(kFrameLength, QObject::tr("Length"), ValueKind::Length, defaultFrameLength(kWall))
                        .withVisible(noEnd)
                        .withOnChange([](CommandState& state, const CommandContext&) {
                          // Typing a length detaches it from the wall the run
                          // meets (mitcad#515).
                          if (state.choice(kFrameLengthMode) != kTyped) {
                            state.setChoice(kFrameLengthMode, kTyped);
                            qInfo().noquote() << QStringLiteral("BUILD frame length typed: Typed length");
                          }
                        })
                        .withTooltip(QObject::tr("Without an end clicked: how far it runs from its start along "
                                                 "Direction, or square to a wall clicked")));
  // A wall started on another wall's side runs square to it (mitcad#493).
  def.inputs.append(choiceInput(kFrameAlong, QObject::tr("Direction"), kFrameAlongs, QStringLiteral("x"))
                        .withVisible([onWall, noEnd](const CommandState& state) { return noEnd(state) && !onWall(state); })
                        .withTooltip(QObject::tr("Without an end clicked: the way the wall runs from its start")));
  // Where along the wall clicked (mitcad#514): the click fills both; typed,
  // they move the wall.
  def.inputs.append(choiceInput(kFrameFrom, QObject::tr("From"), kFroms, QStringLiteral("a"))
                        .withVisible(onWall)
                        .withOnChange([](CommandState& state, const CommandContext&) {
                          state.setChecked(kFrameOffsetTyped, true);
                          qInfo().noquote() << QStringLiteral("BUILD frame From %1, Offset %2: typed")
                                                   .arg(cmd::label(kFroms, frameFromOf(state)), state.text(kFrameOffset));
                        })
                        .withTooltip(QObject::tr("What the offset along the wall clicked is measured from")));
  def.inputs.append(valueInput(kFrameOffset, QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
                        .withVisible(onWall)
                        .withOnChange([](CommandState& state, const CommandContext&) {
                          if (!state.checked(kFrameOffsetTyped)) {
                            state.setChecked(kFrameOffsetTyped, true);
                            qInfo().noquote() << QStringLiteral("BUILD frame offset typed: from %1")
                                                     .arg(cmd::label(kFroms, frameFromOf(state)));
                          }
                        })
                        .withTooltip(QObject::tr("From the end named to the wall's near face, 0 flush; from the "
                                                 "middle to its center")));
  def.inputs.append(valueInput(kFrameHeight, QObject::tr("Height"), ValueKind::Length, QStringLiteral("2400 mm"))
                        .withTooltip(QObject::tr("From the underside of the bottom plate to the top of the top plate")));
  def.inputs.append(valueInput(kFrameSpacing, QObject::tr("Spacing"), ValueKind::Length, defaultFrameSpacing(kWall))
                        .withTooltip(QObject::tr("The studs' center distance, from the start")));
  def.inputs.append(choiceInput(kFrameTop, QObject::tr("Top Plates"), kFrameTops, QStringLiteral("1"))
                        .withTooltip(QObject::tr("One or two top plates")));
  // Butting square, the lapped L corner of mitcad#493 (mitcad#510), or in
  // line beyond an end face (mitcad#524): icon buttons, Lapped plates not
  // offered on another body's face (nothing to lap).
  def.inputs.append(choiceInput(kFrameCorner, QObject::tr("Corner"), kFrameCorners, kButt)
                        .withChoiceIcons(kFrameCornerIcons)
                        .withChoiceOffered([onFace](const CommandState& state, const QString& value) {
                          return value != kLapped || !onFace(state);
                        })
                        .withTooltip(QObject::tr("How the wall meets what it starts or ends on: Butt, Lapped plates or "
                                                 "Straight")));
  // Openings: a tile that opens the Openings window (mitcad#519).
  def.inputs += openingsInputs(shared, def.id, {}, [](const CommandState&) { return false; });
  def.inputs += frameFastenerInputs(shared, def.id, QObject::tr("The fastener below at each junction of the wall's members"));
  def.init = [shared](CommandState& state, const CommandContext& context) {
    const Shared::TimberMemory& memory = shared->timber;
    startFrameTool(*shared, kWallCommand, memory.wallFasteners, state, context);
    // The Corner chosen last (mitcad#510).
    state.setChoice(kFrameCorner, memory.wallCorner);
    // A member's face selected before Wall (the view's context menu, BUILD
    // > Wall, mitcad#520) is where it starts, as a click there.
    const Selection start = state.items(kStart);
    if (start.size() == 1) {
      build::recordPick(kStart, start.first(), state, context);
      fillFrameStart(*shared, state, context);
    }
  };
  def.ended = frameEnded(shared, def.id);
  def.previewMarkers = markersOf; // the openings (mitcad#519)
  def.inspect = [shared](const CommandState& state, const CommandContext& context) {
    return inspectFrame(shared, state, context);
  };
  def.apply = [shared, &host](CommandState& state, const CommandContext& context) {
    Part part;
    if (!chosenPart(*shared, state, part)) {
      host.showStatus(noPartsText(*shared, kTimberRole), NoticeLevel::Error);
      return false;
    }
    // What the next start takes: the Fasteners switch, the Corner and the
    // fastener (mitcad#497, mitcad#510).
    shared->timber.wallFasteners = state.checked(kFrameFasteners);
    shared->timber.wallCorner = frameCornerOf(state);
    if (state.checked(kFrameFasteners)) {
      rememberFastener(*shared, state);
    }
    return applyFrame(*shared, host, state, context, part);
  };
  return def;
}

// --- Floor / Roof (mitcad#520; app/COMMANDS.md, "Timber", Floor / Roof)

CommandDef floorRoofCommand(const SharedPtr& shared, DocumentHost& host) {
  CommandDef def;
  def.id = kFloorRoofCommand;
  def.name = QObject::tr("Floor / Roof");
  def.icon = QStringLiteral("build-floor-roof");
  def.tooltip = QObject::tr("Places a timber floor or ceiling frame, or a roof on two wall frames");
  def.tab = kTab;
  def.group = kTimberGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("floor"),  QStringLiteral("ceiling"), QStringLiteral("joist"),
                  QStringLiteral("roof"),   QStringLiteral("rafter"),  QStringLiteral("gable"),
                  QStringLiteral("frame"),  QStringLiteral("timber"),  QStringLiteral("build")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  // The view's context menu (mitcad#494, mitcad#520) offers it on wall
  // frames' members, for the roof on them: first when the selection names
  // two walls.
  const auto walls = [&host](const Selection& selection) {
    const QVector<BuildPart> parts = build::buildParts(host.model());
    QStringList leads;
    for (const SelectionItem& item : selection) {
      const QString lead = wallLeadOf(parts, build::partOf(item, host.model()));
      if (lead.isEmpty()) {
        return QStringList();
      }
      if (!leads.contains(lead)) {
        leads << lead;
      }
    }
    return leads;
  };
  def.offeredFor = [walls](const Selection& selection) { return !walls(selection).isEmpty(); };
  def.completes = [walls](const Selection& selection) { return walls(selection).size() == 2; };
  const auto kindIs = [](const QString& kind) {
    return [kind](const CommandState& state) { return frameKindOf(state) == kind; };
  };
  const auto floorNoEnd = [](const CommandState& state) {
    return frameKindOf(state) == kFloor && state.items(kEndPoint).isEmpty();
  };
  def.inputs = pickerInputs(shared, kTimberRole, def.id);
  def.inputs.append(choiceInput(kFrameKind, QObject::tr("Kind"), kFloorRoofKinds, kFloor)
                        .withChoiceIcons(kFloorRoofIcons)
                        .withOnChange([shared](CommandState& state, const CommandContext& context) {
                          // The spacing follows the kind while it is the
                          // other kind's default: joists at 400, rafters at
                          // 600.
                          const QString kind = frameKindOf(state);
                          const QString spacing = state.text(kFrameSpacing).trimmed();
                          if (spacing.isEmpty() || spacing == shownDefault(defaultFrameSpacing(kRoof), context) ||
                              spacing == shownDefault(defaultFrameSpacing(kFloor), context)) {
                            state.setText(kFrameSpacing, shownDefault(defaultFrameSpacing(kind), context));
                            state.setValue(kFrameSpacing, kNaN);
                          }
                          shared->timber.floorRoofKind = kind;
                        })
                        .withTooltip(QObject::tr("A floor or ceiling, or a roof on two wall frames")));
  // The roof's walls first: the selection input the view's context menu
  // gives wall members to (mitcad#520).
  def.inputs.append(memberInput(kFrameWalls, QObject::tr("Walls"), 0, 2)
                        .withVisible(kindIs(kRoof))
                        .withTooltip(QObject::tr("A member of each of the two wall frames the roof stands on")));
  def.inputs.append(frameStartInput(shared, QObject::tr("A corner of the floor: a plane, a face or a point"),
                                    kindIs(kFloor)));
  def.inputs.append(frameEndInput(QObject::tr("The opposite corner, level with the first (Shift+click places)"),
                                  kindIs(kFloor)));
  def.inputs.append(valueInput(kFrameLength, QObject::tr("Length"), ValueKind::Length, defaultFrameLength(kFloor))
                        .withVisible(floorNoEnd)
                        .withTooltip(QObject::tr("Without an end clicked: the floor's size along Direction")));
  def.inputs.append(valueInput(kFrameWidth, QObject::tr("Width"), ValueKind::Length, QStringLiteral("3000 mm"))
                        .withVisible(floorNoEnd)
                        .withTooltip(QObject::tr("Without an end clicked: the floor's size to the left of Direction")));
  def.inputs.append(choiceInput(kFrameAlong, QObject::tr("Direction"), kFrameAlongs, QStringLiteral("x"))
                        .withVisible(floorNoEnd)
                        .withTooltip(QObject::tr("Without an end clicked: the way the floor's length runs")));
  def.inputs.append(valueInput(kFrameSpacing, QObject::tr("Spacing"), ValueKind::Length, defaultFrameSpacing(kFloor))
                        .withTooltip(QObject::tr("The joists' or rafters' center distance, from the start")));
  // A floor's Openings: the tile that opens the Openings window
  // (mitcad#519).
  def.inputs += openingsInputs(shared, def.id, kindIs(kFloor), kindIs(kFloor));
  def.inputs.append(checkInput(kFrameBlocking, QObject::tr("Blocking"), false)
                        .withVisible(kindIs(kFloor))
                        .withTooltip(QObject::tr("A row of blocking between the joists, halfway along them")));
  def.inputs.append(choiceInput(kFrameRoof, QObject::tr("Roof"), kFrameRoofs, QStringLiteral("gable"))
                        .withVisible(kindIs(kRoof))
                        .withTooltip(QObject::tr("Two slopes meeting at a ridge, or one slope")));
  def.inputs.append(checkInput(kFramePitchAuto, QObject::tr("Pitch from Walls"), true)
                        .withVisible([](const CommandState& state) {
                          return frameKindOf(state) == kRoof && state.choice(kFrameRoof) == QStringLiteral("mono");
                        })
                        .withTooltip(QObject::tr("A mono-pitch roof's pitch from the walls' heights")));
  def.inputs.append(valueInput(kFramePitch, QObject::tr("Pitch"), ValueKind::Angle, QStringLiteral("30 deg"))
                        .withVisible([](const CommandState& state) {
                          return frameKindOf(state) == kRoof &&
                                 (state.choice(kFrameRoof) != QStringLiteral("mono") || !state.checked(kFramePitchAuto));
                        })
                        .withTooltip(QObject::tr("The roof's slope")));
  def.inputs.append(valueInput(kFrameOverhang, QObject::tr("Overhang"), ValueKind::Length, QStringLiteral("300 mm"))
                        .withVisible(kindIs(kRoof))
                        .withTooltip(QObject::tr("How far the rafters run beyond the walls' outer faces")));
  def.inputs += frameFastenerInputs(shared, def.id,
                                    QObject::tr("The fastener below at each junction of the frame's members"));
  def.init = [shared](CommandState& state, const CommandContext& context) {
    const Shared::TimberMemory& memory = shared->timber;
    startFrameTool(*shared, kFloorRoofCommand, memory.floorRoofFasteners, state, context);
    // What was selected before: a member of each of two walls is a roof's
    // (the view's context menu, mitcad#520); anything else is the floor's
    // start. Else the kind used last.
    const Selection picked = state.items(kFrameWalls);
    QString kind = memory.floorRoofKind == kRoof ? kRoof : kFloor;
    if (!picked.isEmpty()) {
      const QVector<BuildPart> parts = shared->designParts();
      const bool onWalls = std::all_of(picked.begin(), picked.end(), [&](const SelectionItem& item) {
        return !wallLeadOf(parts, shared->partAt(item.occurrence)).isEmpty();
      });
      kind = onWalls ? kRoof : kFloor;
      if (!onWalls) {
        state.setItems(kFrameWalls, {});
        if (state.items(kStart).isEmpty() && picked.first().kind == SelectKind::Face) {
          state.setItems(kStart, {picked.first()});
        }
      }
    }
    state.setChoice(kFrameKind, kind);
    state.setText(kFrameSpacing, shownDefault(defaultFrameSpacing(kind), context));
    state.setValue(kFrameSpacing, kNaN);
    qInfo().noquote() << QStringLiteral("BUILD floor_roof kind %1%2")
                             .arg(kind, picked.isEmpty() ? QString()
                                                         : QStringLiteral(", %1 item(s) selected before").arg(picked.size()));
  };
  def.ended = frameEnded(shared, def.id);
  def.previewMarkers = markersOf; // the openings (mitcad#519)
  def.inspect = [shared](const CommandState& state, const CommandContext& context) {
    return inspectFrame(shared, state, context);
  };
  def.apply = [shared, &host](CommandState& state, const CommandContext& context) {
    Part part;
    if (!chosenPart(*shared, state, part)) {
      host.showStatus(noPartsText(*shared, kTimberRole), NoticeLevel::Error);
      return false;
    }
    shared->timber.floorRoofKind = frameKindOf(state);
    shared->timber.floorRoofFasteners = state.checked(kFrameFasteners);
    if (state.checked(kFrameFasteners)) {
      rememberFastener(*shared, state);
    }
    return applyFrame(*shared, host, state, context, part);
  };
  return def;
}

// --- Timber: loose members (mitcad#478, mitcad#520)

CommandDef timberCommand(const SharedPtr& shared, DocumentHost& host) {
  const QString role = kTimberRole;
  CommandDef def;
  def.id = kTimberCommand;
  def.name = QObject::tr("Timber");
  def.icon = QStringLiteral("build-timber");
  def.tooltip = QObject::tr("Places loose timber: on a member, at a spacing or by two points");
  def.tab = kTab;
  def.group = kTimberGroup;
  def.pinned = true;
  def.keywords = {QStringLiteral("wood"),   QStringLiteral("lumber"), QStringLiteral("stud"),
                  QStringLiteral("joist"),  QStringLiteral("rafter"), QStringLiteral("deck"),
                  QStringLiteral("board"),  QStringLiteral("batten"), QStringLiteral("build")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  const auto how = [](const QString& wanted) {
    return [wanted](const CommandState& state) { return howOf(state) == wanted; };
  };
  const auto overFaces = [](const CommandState& state) {
    return howOf(state) == kSpacing && state.choice(kOver) == kOverFaces;
  };
  def.inputs = pickerInputs(shared, role, def.id);
  // On a member first and the default, Two points last (mitcad#485).
  def.inputs.append(choiceInput(kHow, QObject::tr("Mode"), kHows, kOnMember)
                        .withChoiceIcons(kHowIcons)
                        .withOnChange([shared](CommandState& state, const CommandContext& context) {
                          shared->timberHow = howOf(state);
                          state.setObject(kHoverPick, {});
                          // A set's fastener (mitcad#497).
                          if (howOf(state) == kSpacing) {
                            startFasteners(*shared, state, context);
                          }
                        })
                        .withTooltip(QObject::tr("How the member is placed: on a member, at a spacing or by two "
                                                 "points")));
  // Start and End of a member by two points.
  def.inputs.append(selectionInput(kStart, QObject::tr("Start"),
                                   SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex, 0, 1)
                        .withVisible(how(kPoints))
                        .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                          build::recordPick(kStart, item, state, context);
                        },
                                    false)
                        .withTooltip(
                            QObject::tr("Where the member starts: a plane or a face it lies on, or another member")));
  def.inputs.append(selectionInput(kEndPoint, QObject::tr("End"),
                                   SelectKind::Face | SelectKind::Plane | SelectKind::Point | SelectKind::Vertex, 0, 1)
                        .withVisible(how(kPoints))
                        .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                          build::recordPick(kEndPoint, item, state, context);
                        },
                                    false)
                        .withShiftClickApplies()
                        .withTooltip(QObject::tr("Where the member ends, moved onto the start's plane (Shift+click "
                                                 "places)")));
  def.inputs.append(choiceInput(kOver, QObject::tr("Spacing On"), kOvers, kBetween)
                        .withVisible(how(kSpacing))
                        .withTooltip(QObject::tr("Between two members, or over the faces the members lie on")));
  def.inputs.append(selectionInput(kFaces, QObject::tr("Supporting Faces"), SelectKind::Face, 0, 0)
                        .withVisible(overFaces)
                        .withTooltip(QObject::tr("The faces the row lies on, such as the joists' top faces")));
  // Profile's placement inputs (two points give the length, mitcad#485).
  for (InputDef& input : placementInputs(shared, role, true)) {
    def.inputs.append(onMemberOnly(std::move(input)));
  }
  // Flat or on edge: by two points, over faces, and on the ground
  // (mitcad#485); Tab steps it (one key) where Length's mode, before it, is
  // not shown.
  def.inputs.append(choiceInput(kLie, QObject::tr("Lie"), kLies, QStringLiteral("flat"))
                        .withVisible([shared, overFaces](const CommandState& state) {
                          return howOf(state) == kPoints || overFaces(state) ||
                                 (howOf(state) == kOnMember && against(*shared, kTimberRole, state) == Against::Ground);
                        })
                        .withTabCycle({QStringLiteral("flat"), QStringLiteral("edge")})
                        .withTooltip(QObject::tr("Flat: the thinner side across the plane it lies on; on edge: the "
                                                 "wider (Tab turns it)")));
  def.inputs.append(choiceInput(kSpacingMode, QObject::tr("Spacing"), kSpacingModes, kCentre)
                        .withVisible(how(kSpacing))
                        .withOnChange([](CommandState& state, const CommandContext& context) {
                          // The distance follows the mode while it is the other's default.
                          const bool gap = state.choice(kSpacingMode) == kGap;
                          const QString typed = state.text(kDistance).trimmed();
                          const QString centre = shownDefault(QStringLiteral("400 mm"), context);
                          const QString between = shownDefault(QStringLiteral("5 mm"), context);
                          if (typed.isEmpty() || typed == (gap ? centre : between)) {
                            state.setText(kDistance, gap ? between : centre);
                            state.setValue(kDistance, kNaN);
                          }
                        })
                        .withTooltip(QObject::tr("A center distance (studs, joists, rafters), or a gap between "
                                                 "boards (a deck, a fence, cladding)")));
  def.inputs.append(valueInput(kDistance, QObject::tr("Distance"), ValueKind::Length, QStringLiteral("400 mm"))
                        .withVisible(how(kSpacing))
                        .withTooltip(QObject::tr("The center distance, or the gap between boards")));
  def.inputs.append(choiceInput(kFit, QObject::tr("Last Board"), kFits, QStringLiteral("cut"))
                        .withVisible([](const CommandState& state) {
                          return howOf(state) == kSpacing && state.choice(kSpacingMode) == kGap;
                        }));
  // A spacing set fastened to its hosts (mitcad#497), off by default.
  def.inputs.append(checkInput(kSetFasteners, QObject::tr("Fasteners"), false)
                        .withVisible(how(kSpacing))
                        .withOnChange([shared](CommandState& state, const CommandContext&) {
                          shared->timber.setFasteners = state.checked(kSetFasteners);
                        })
                        .withTooltip(QObject::tr("Each member fastened to the members it stands on, reaches or "
                                                 "crosses")));
  // The fastener, its size and count (mitcad#497): under the set's
  // Fasteners while it is on.
  def.inputs += woodInputs(shared, def.id, [](const CommandState& state) {
    return howOf(state) == kSpacing && state.checked(kSetFasteners);
  });
  def.init = [shared, role](CommandState& state, const CommandContext& context) {
    startPicker(*shared, role, state, context);
    // The profiles' sections for the snaps, when the design has them.
    if (shared->hasSlotProfiles()) {
      shared->catalog.refresh(QStringLiteral("profile"), context);
    }
    if (!shared->timberHow.isEmpty()) {
      state.setChoice(kHow, shared->timberHow);
    }
    // A set's fastener as chosen last (mitcad#497).
    const Shared::TimberMemory& memory = shared->timber;
    state.setChecked(kSetFasteners, memory.setFasteners);
    state.setText(kWoodCount, memory.fastenerCount);
    state.setValue(kWoodCount, kNaN);
    if (howOf(state) == kSpacing) {
      startFasteners(*shared, state, context);
      qInfo().noquote() << QStringLiteral("BUILD timber fasteners: set %1; %2")
                               .arg(state.checked(kSetFasteners) ? QStringLiteral("on") : QStringLiteral("off"),
                                    woodText(*shared, state));
    }
    // A typed length (Auto stays a choice), and what the tool remembers:
    // the length typed last (the first time the part's default), the
    // ground placement's direction and lie (mitcad#485).
    state.setChoice(QStringLiteral("length_mode"), kTyped);
    if (!memory.length.isEmpty()) {
      state.setText(QStringLiteral("length"), memory.length);
      state.setValue(QStringLiteral("length"), kNaN);
    }
    state.setChoice(QStringLiteral("axis"), memory.axis);
    state.setChoice(kLie, memory.lie);
    qInfo().noquote() << QStringLiteral("BUILD timber adds to %1, %2")
                             .arg(build::buildHost(context).name, howOf(state));
    qInfo().noquote() << QStringLiteral("BUILD timber starts: length %1 (%2), typed; along %3, %4")
                             .arg(state.text(QStringLiteral("length")),
                                  memory.length.isEmpty() ? QStringLiteral("the part's default")
                                                          : QStringLiteral("typed last"),
                                  memory.axis, memory.lie);
    if (PartPicker* picker = pinnedPicker(*shared)) {
      picker->ask(roleRequest(*shared, role, state, kTimberCommand));
    }
  };
  def.ended = pickerEnded(shared, def.id);
  def.inspect = [shared, role](const CommandState& state, const CommandContext& context) {
    if (howOf(state) == kOnMember) {
      return inspectPlacement(shared, role, state, context, true);
    }
    Inspection result =
        placesOnMember(state) ? inspectSetOnSide(shared, state, context) : inspectTimber(shared, state, context);
    // A set fastened to its hosts (mitcad#497).
    if (howOf(state) == kSpacing) {
      const QString with = state.checked(kSetFasteners) ? woodText(*shared, state) : QString();
      result.text += QStringLiteral("<br>") +
                     (with.isEmpty() ? QObject::tr("No fasteners")
                                     : QObject::tr("Fastened to its hosts: %1").arg(with).toHtmlEscaped());
      result.log += QStringLiteral("; fasteners %1").arg(with.isEmpty() ? QStringLiteral("no") : with);
    }
    return result;
  };
  def.previewMarkers = markersOf;
  // No mode's preview follows the mouse (mitcad#485, mitcad#489,
  // mitcad#508): Two points and a row over faces show nothing for it; on a
  // member and a set between two members only the face a click picks, the
  // preview staying where the click put it.
  def.hoverPreview = [shared, role](const SelectionItem& item, CommandState& state, const CommandContext&) {
    if (placesOnMember(state)) {
      hoverPickOnly(*shared, role, item, state);
    }
    return false;
  };
  def.apply = [shared, &host, role](CommandState& state, const CommandContext& context) {
    Part part;
    if (!chosenPart(*shared, state, part)) {
      host.showStatus(noPartsText(*shared, role), NoticeLevel::Error);
      return false;
    }
    // What the next start takes (mitcad#485): the length typed, the ground
    // placement's direction and lie.
    if (placesOnMember(state) && state.choice(QStringLiteral("length_mode")) == kTyped &&
        !state.text(QStringLiteral("length")).trimmed().isEmpty()) {
      shared->timber.length = state.text(QStringLiteral("length")).trimmed();
    }
    if (!state.choice(QStringLiteral("axis")).isEmpty()) {
      shared->timber.axis = state.choice(QStringLiteral("axis"));
    }
    if (!state.choice(kLie).isEmpty()) {
      shared->timber.lie = state.choice(kLie);
    }
    // A set's fasteners (mitcad#497): whether it is fastened, with what, in
    // which size and how many.
    if (howOf(state) == kSpacing) {
      shared->timber.setFasteners = state.checked(kSetFasteners);
      if (state.checked(kSetFasteners)) {
        rememberFastener(*shared, state);
      }
    }
    const auto fastenSet = [shared, &state](QJsonObject& fields) {
      if (!state.checked(kSetFasteners)) {
        return;
      }
      const std::optional<QJsonObject> fastener = woodFastenerRef(*shared, state);
      if (!fastener) {
        return;
      }
      QJsonObject spacing = fields.value(QStringLiteral("spacing")).toObject();
      spacing.insert(QStringLiteral("fastener"), *fastener);
      spacing.insert(QStringLiteral("fastener_count"), woodCount(state));
      fields.insert(QStringLiteral("spacing"), spacing);
    };
    if (placesOnMember(state)) {
      const Placement placed = placement(*shared, role, state, context, part, true);
      if (!placed.problem.isEmpty()) {
        host.showStatus(placed.problem, NoticeLevel::Error);
        return false;
      }
      QJsonObject fields = placed.fields;
      if (howOf(state) == kSpacing) {
        fields.insert(QStringLiteral("spacing"), spacingFields(state));
        fastenSet(fields);
      }
      return placeNow(*shared, host, role, part, fields, placed.lengthNote, state, context);
    }
    if (howOf(state) == kSpacing) {
      RowPlacement placed = rowPlacement(*shared, state, context, part);
      if (!placed.problem.isEmpty()) {
        host.showStatus(placed.problem, NoticeLevel::Error);
        return false;
      }
      fastenSet(placed.fields);
      if (!placeNow(*shared, host, role, part, placed.fields, QObject::tr("across the faces"), state, context)) {
        return false;
      }
      state.setItems(kFaces, {});
      return true;
    }
    // Two points: from the start to the end clicked.
    const PointsPlacement placed = pointsPlacement(*shared, state, context, part);
    if (!placed.problem.isEmpty()) {
      host.showStatus(placed.problem, NoticeLevel::Error);
      return false;
    }
    // Its length as the model places it: the points' distance less the ends
    // cut back to the members they are in (mitcad#509).
    double length = placed.start.Distance(placed.end);
    try {
      QJsonObject request = placed.fields;
      request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
      const double shown = build::lengthOf(context.queryObject(request).value(QStringLiteral("length")));
      if (std::isfinite(shown)) {
        length = shown;
      }
    } catch (const std::exception&) {
      // build_place says why.
    }
    const QString note = QObject::tr("by two points, %1 mm").arg(length, 0, 'f', 1);
    if (!placeNow(*shared, host, role, part, placed.fields, note, state, context)) {
      return false;
    }
    state.setItems(kStart, {});
    state.setItems(kEndPoint, {});
    state.setObject(kStart + QStringLiteral("_pick"), {});
    state.setObject(kEndPoint + QStringLiteral("_pick"), {});
    return true;
  };
  // A part's face selected before Timber (the view's context menu, BUILD >
  // Timber, mitcad#494) is where it goes, On a member, as a click there
  // places it; the selection would else wait in Start, hidden.
  std::function<void(const SelectionItem&, CommandState&, const CommandContext&)> pickAt;
  for (const InputDef& input : def.inputs) {
    if (input.id == kAt) {
      pickAt = input.onPick;
    }
  }
  def.init = [init = def.init, pickAt](CommandState& state, const CommandContext& context) {
    init(state, context);
    const Selection start = state.items(kStart);
    if (start.size() != 1 || start.first().kind != SelectKind::Face || !state.items(kAt).isEmpty() ||
        !build::partOf(start.first(), context)) {
      return;
    }
    state.setItems(kStart, {});
    state.setItems(kAt, start);
    state.setChoice(kHow, kOnMember);
    if (pickAt) {
      pickAt(start.first(), state, context);
    }
  };
  return def;
}

// --- A spacing set edited as one (mitcad#478)

// The lead of the set a picked member is in.
std::optional<BuildPart> setLeadOf(const Shared& shared, const CommandState& state) {
  const Selection& items = state.items(kPart);
  const std::optional<BuildPart> picked = items.isEmpty() ? std::nullopt : shared.partAt(items.first().occurrence);
  if (picked && !picked->spacing.isEmpty()) {
    return picked; // the lead itself
  }
  if (!picked || picked->set.isEmpty()) {
    return std::nullopt;
  }
  for (const BuildPart& part : shared.designParts()) {
    if (part.component == picked->set) {
      return part;
    }
  }
  return std::nullopt;
}

CommandDef spacingCommand(const SharedPtr& shared, DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.spacing");
  def.name = QObject::tr("Spacing Set");
  def.icon = QStringLiteral("build-timber-spacing");
  def.tooltip = QObject::tr("Changes a spacing set's distance, count and members' size");
  def.tab = kTab;
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("spacing"), QStringLiteral("joists"), QStringLiteral("studs"),
                  QStringLiteral("boards"), QStringLiteral("set"), QStringLiteral("build")};
  // The view's context menu offers it for members of a set only (mitcad#494).
  def.offeredFor = [&context](const Selection& selection) {
    return std::all_of(selection.begin(), selection.end(), [&context](const SelectionItem& item) {
      const std::optional<BuildPart> part = build::partOf(item, context);
      return part && (!part->set.isEmpty() || !part->spacing.isEmpty());
    });
  };
  const QString kSize = QStringLiteral("size");
  const QString kCount = QStringLiteral("count");
  def.inputs = {
      memberInput(kPart, QObject::tr("Member"), 1, 1)
          .withOnPick([shared, kSize, kCount](const SelectionItem&, CommandState& state, const CommandContext& model) {
            shared->refreshDesign(model);
            const std::optional<BuildPart> lead = setLeadOf(*shared, state);
            if (!lead) {
              return;
            }
            // The set's definition as the panel's values.
            const QJsonObject spacing =
                lead->spacing.value(QStringLiteral("definition")).toObject().value(QStringLiteral("spacing")).toObject();
            const QJsonObject definition = spacing.isEmpty() ? lead->spacing.value(QStringLiteral("definition")).toObject()
                                                             : spacing;
            const QString mode = str(definition, "mode").isEmpty() ? kCentre : str(definition, "mode");
            state.setChoice(kSpacingMode, mode);
            const QJsonValue distance = definition.value(QStringLiteral("distance"));
            if (!distance.isUndefined() && !distance.isNull()) {
              state.setText(kDistance, distance.isDouble() ? cmd::lengthText(distance.toDouble(), model) : distance.toString());
              state.setValue(kDistance, kNaN);
            }
            state.setChoice(kFit, str(definition, "fit").isEmpty() ? QStringLiteral("cut") : str(definition, "fit"));
            state.setText(kCount, QString::number(definition.value(QStringLiteral("count")).toInt(0)));
            state.setValue(kCount, kNaN);
            state.setChoice(kSize, lead->config);
          },
                      false)
          .withTooltip(QObject::tr("A member of the set")),
      choiceInput(kSpacingMode, QObject::tr("Spacing"), kSpacingModes, kCentre),
      valueInput(kDistance, QObject::tr("Distance"), ValueKind::Length, QStringLiteral("400 mm")),
      choiceInput(kFit, QObject::tr("Last Board"), kFits, QStringLiteral("cut"))
          .withVisible([](const CommandState& state) { return state.choice(kSpacingMode) == kGap; }),
      valueInput(kCount, QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("0"))
          .withTooltip(QObject::tr("A fixed count, spread evenly over the span; 0: from the distance")),
      choiceInput(kSize, QObject::tr("Size"), {}, QString())
          .withChoices([shared](const CommandState& state) {
            // The rows of the set's source in the Timber picker's catalog.
            Choices sizes;
            const std::optional<BuildPart> lead = setLeadOf(*shared, state);
            if (!lead) {
              return sizes;
            }
            for (const Part& part : shared->catalog.parts(kTimberRole)) {
              if (part.source == lead->source && !part.config.isEmpty()) {
                sizes.append({part.config, part.lessCommon() ? QObject::tr("%1 (less common)").arg(part.config) : part.config});
              }
            }
            if (sizes.isEmpty() && !lead->config.isEmpty()) {
              sizes.append({lead->config, lead->config});
            }
            return sizes;
          })
          .withTooltip(QObject::tr("The members' size: every member of the set is made in it")),
  };
  def.enabled = [&context] {
    const QVector<BuildPart> parts = build::buildParts(context);
    return std::any_of(parts.begin(), parts.end(), [](const BuildPart& part) { return !part.set.isEmpty(); });
  };
  def.init = [shared](CommandState&, const CommandContext& model) {
    shared->refreshDesign(model);
    shared->catalog.refresh(kTimberRole, model);
  };
  def.inspect = [shared](const CommandState& state, const CommandContext&) {
    Inspection result;
    const std::optional<BuildPart> lead = setLeadOf(*shared, state);
    if (!lead) {
      result.error = QObject::tr("Pick a member of a spacing set.");
      return result;
    }
    const int count = lead->spacing.value(QStringLiteral("count")).toInt();
    result.text = QObject::tr("<b>%1</b><br>A set of %2 member(s); %3")
                      .arg(lead->shownName().toHtmlEscaped())
                      .arg(count)
                      .arg(spacingText(state).toHtmlEscaped());
    result.log = QStringLiteral("set %1 of %2 member(s): %3").arg(lead->component).arg(count).arg(spacingText(state));
    return result;
  };
  def.apply = [shared, &host, kSize, kCount](CommandState& state, const CommandContext& model) {
    const std::optional<BuildPart> lead = setLeadOf(*shared, state);
    if (!lead) {
      host.showStatus(QObject::tr("Pick a member of a spacing set."), NoticeLevel::Error);
      return false;
    }
    QJsonObject spacing = spacingFields(state);
    const double count = state.value(kCount);
    spacing.insert(QStringLiteral("count"), std::isfinite(count) ? std::max(0, static_cast<int>(std::lround(count))) : 0);
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("build_edit")},
                        {QStringLiteral("component"), lead->component},
                        {QStringLiteral("spacing"), spacing}};
    if (!state.choice(kSize).isEmpty() && state.choice(kSize) != lead->config) {
      command.insert(QStringLiteral("config"), state.choice(kSize));
    }
    QJsonObject result;
    if (!host.runModelCommand(command, &result)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("Spacing set of %1: %2 member(s), %3")
                             .arg(lead->shownName())
                             .arg(result.value(QStringLiteral("count")).toInt())
                             .arg(spacingText(state));
    shared->refreshDesign(model);
    return true;
  };
  return def;
}

// --- A timber frame laid out again (mitcad#479)

// The lead of the frame a picked member is in.
std::optional<BuildPart> frameOfPick(const Shared& shared, const CommandState& state) {
  const Selection& items = state.items(kPart);
  const std::optional<BuildPart> picked = items.isEmpty() ? std::nullopt : shared.partAt(items.first().occurrence);
  const QString lead = picked ? frameLeadOf(*picked) : QString();
  if (lead.isEmpty()) {
    return std::nullopt;
  }
  for (const BuildPart& part : shared.designParts()) {
    if (part.component == lead) {
      return part;
    }
  }
  return std::nullopt;
}

// A definition's value as a value input shows it: an expression as it is,
// a number in mm (or radians, shown in degrees).
QString valueText(const QJsonValue& value, const QString& fallback, bool angle, const CommandContext& context) {
  if (value.isString()) {
    return value.toString();
  }
  if (value.isDouble()) {
    return angle ? QStringLiteral("%1 deg").arg(value.toDouble() * 180.0 / kPi, 0, 'g', 10)
                 : cmd::lengthText(value.toDouble(), context);
  }
  return fallback;
}

// A frame's outline is a rectangle (four points, square corners): its
// Length and Width can be changed.
bool isRectangle(const QJsonArray& outline) {
  if (outline.size() != 4) {
    return false;
  }
  QVector<gp_Pnt> p;
  for (const QJsonValue& value : outline) {
    p.append(pointFrom(value).value_or(gp_Pnt()));
  }
  for (int i = 0; i < 4; ++i) {
    const gp_Vec a(p[i], p[(i + 1) % 4]);
    const gp_Vec b(p[(i + 1) % 4], p[(i + 2) % 4]);
    if (a.Magnitude() < 1.0 || b.Magnitude() < 1.0 || std::abs(a.Normalized().Dot(b.Normalized())) > 1e-6) {
      return false;
    }
  }
  return true;
}

// The fields of build_edit's `frame` from the panel.
QJsonObject frameEditFields(const BuildPart& lead, const CommandState& state, QString& problem) {
  const QJsonObject definition = frameDefinition(lead);
  const QString kind = str(definition, "kind").isEmpty() ? kWall : str(definition, "kind");
  QJsonObject frame{{QStringLiteral("spacing"), expressionOr(state, kFrameSpacing, defaultFrameSpacing(kind))}};
  if (kind == kRoof) {
    frame.insert(QStringLiteral("pitch"), expressionOr(state, kFramePitch, QStringLiteral("30 deg")));
    frame.insert(QStringLiteral("overhang"), expressionOr(state, kFrameOverhang, QStringLiteral("300 mm")));
    return frame;
  }
  const QJsonArray openings = openingsOf(state.text(kFrameOpenings), kind == kFloor, problem);
  if (!problem.isEmpty()) {
    return {};
  }
  frame.insert(QStringLiteral("openings"), openings);
  if (kind == kWall) {
    frame.insert(QStringLiteral("length"), expressionOr(state, kFrameLength, defaultFrameLength(kind)));
    frame.insert(QStringLiteral("height"), expressionOr(state, kFrameHeight, QStringLiteral("2400 mm")));
    frame.insert(QStringLiteral("plates"),
                 QJsonObject{{QStringLiteral("top"), state.choice(kFrameTop) == QStringLiteral("2") ? 2 : 1}});
    frame.insert(QStringLiteral("corner"), frameCornerOf(state)); // mitcad#510
    frame.insert(QStringLiteral("load_bearing"), loadBearingOf(state)); // mitcad#519
    return frame;
  }
  frame.insert(QStringLiteral("blocking"), state.checked(kFrameBlocking) ? 1 : 0);
  const QJsonArray outline = definition.value(QStringLiteral("outline")).toArray();
  if (isRectangle(outline)) {
    const gp_Pnt p0 = pointFrom(outline[0]).value_or(gp_Pnt());
    const gp_Dir e0(gp_Vec(p0, pointFrom(outline[1]).value_or(gp_Pnt())));
    const gp_Dir e1(gp_Vec(pointFrom(outline[1]).value_or(gp_Pnt()), pointFrom(outline[2]).value_or(gp_Pnt())));
    const double length = millimetresOf(state, kFrameLength, 4000.0);
    const double width = millimetresOf(state, kFrameWidth, 3000.0);
    if (!(length > 1.0) || !(width > 1.0)) {
      problem = QObject::tr("A floor's length and width must be greater than zero.");
      return {};
    }
    frame.insert(QStringLiteral("outline"),
                 QJsonArray{build::pointJson(p0), build::pointJson(p0.Translated(gp_Vec(e0) * length)),
                            build::pointJson(p0.Translated(gp_Vec(e0) * length + gp_Vec(e1) * width)),
                            build::pointJson(p0.Translated(gp_Vec(e1) * width))});
  }
  return frame;
}

CommandDef frameCommand(const SharedPtr& shared, DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.frame");
  def.name = QObject::tr("Timber Frame");
  def.icon = QStringLiteral("build-timber-frame");
  def.tooltip = QObject::tr("Changes a timber frame's length, height, spacing, openings and members' size");
  def.tab = kTab;
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("frame"), QStringLiteral("wall"), QStringLiteral("floor"), QStringLiteral("roof"),
                  QStringLiteral("studs"), QStringLiteral("joists"), QStringLiteral("rafters"), QStringLiteral("build")};
  // Only a frame's member completes it; another is offered under More in
  // the view's context menu (mitcad#494).
  def.completes = [&context](const Selection& selection) {
    return std::all_of(selection.begin(), selection.end(), [&context](const SelectionItem& item) {
      const std::optional<BuildPart> part = build::partOf(item, context);
      return part && !frameLeadOf(*part).isEmpty();
    });
  };
  const QString kSize = QStringLiteral("size");
  const QString kRectangle = QStringLiteral("frame_rectangle");
  const auto kindIs = [](const QString& kind) {
    return [kind](const CommandState& state) { return state.choice(kFrameKind) == kind; };
  };
  def.inputs = {
      memberInput(kPart, QObject::tr("Member"), 1, 1)
          .withOnPick([shared, kSize, kRectangle](const SelectionItem&, CommandState& state, const CommandContext& model) {
            shared->refreshDesign(model);
            const std::optional<BuildPart> lead = frameOfPick(*shared, state);
            if (!lead) {
              state.setChoice(kFrameKind, QString());
              return;
            }
            // The frame's definition as the panel's values.
            const QJsonObject definition = frameDefinition(*lead);
            const QString kind = str(definition, "kind").isEmpty() ? kWall : str(definition, "kind");
            state.setChoice(kFrameKind, kind);
            const auto set = [&state](const QString& id, const QString& text) {
              state.setText(id, text);
              state.setValue(id, kNaN);
            };
            set(kFrameSpacing, valueText(definition.value(QStringLiteral("spacing")), defaultFrameSpacing(kind), false, model));
            set(kFrameOpenings, openingsText(definition.value(QStringLiteral("openings")).toArray()));
            if (kind == kWall) {
              const std::optional<gp_Pnt> start = pointFrom(definition.value(QStringLiteral("start")));
              const std::optional<gp_Pnt> end = pointFrom(definition.value(QStringLiteral("end")));
              if (start && end) {
                const double length = std::hypot(end->X() - start->X(), end->Y() - start->Y());
                set(kFrameLength, cmd::lengthText(length, model));
              }
              set(kFrameHeight, valueText(definition.value(QStringLiteral("height")), QStringLiteral("2400 mm"), false, model));
              const int top = definition.value(QStringLiteral("plates")).toObject().value(QStringLiteral("top")).toInt(1);
              state.setChoice(kFrameTop, QString::number(top));
              // Left out: lapped, as saved before mitcad#510.
              const QString corner = str(definition, "corner");
              state.setChoice(kFrameCorner, corner == kButt || corner == kStraight ? corner : kLapped);
              // Left out: load-bearing, as saved before mitcad#519.
              state.setChoice(kFrameBearing,
                              definition.value(QStringLiteral("load_bearing")).toBool(true) ? kLoadBearing : kPartition);
            } else if (kind == kFloor) {
              const QJsonArray outline = definition.value(QStringLiteral("outline")).toArray();
              const bool rectangle = isRectangle(outline);
              state.setChoice(kRectangle, rectangle ? QStringLiteral("yes") : QString());
              if (rectangle) {
                const gp_Pnt p0 = pointFrom(outline[0]).value_or(gp_Pnt());
                const gp_Pnt p1 = pointFrom(outline[1]).value_or(gp_Pnt());
                const gp_Pnt p2 = pointFrom(outline[2]).value_or(gp_Pnt());
                set(kFrameLength, cmd::lengthText(p0.Distance(p1), model));
                set(kFrameWidth, cmd::lengthText(p1.Distance(p2), model));
              }
              state.setChecked(kFrameBlocking, definition.value(QStringLiteral("blocking")).toInt() > 0);
            } else {
              set(kFramePitch, valueText(definition.value(QStringLiteral("pitch")), QStringLiteral("30 deg"), true, model));
              set(kFrameOverhang, valueText(definition.value(QStringLiteral("overhang")), QStringLiteral("300 mm"), false, model));
            }
            state.setChoice(kSize, lead->config);
          },
                      false)
          .withTooltip(QObject::tr("A member of the frame")),
      valueInput(kFrameLength, QObject::tr("Length"), ValueKind::Length, QStringLiteral("3600 mm"))
          .withVisible([kRectangle](const CommandState& state) {
            return state.choice(kFrameKind) == kWall ||
                   (state.choice(kFrameKind) == kFloor && !state.choice(kRectangle).isEmpty());
          })
          .withTooltip(QObject::tr("A wall's length from its start (its end moves along it); a floor's first side")),
      valueInput(kFrameWidth, QObject::tr("Width"), ValueKind::Length, QStringLiteral("3000 mm"))
          .withVisible([kRectangle](const CommandState& state) {
            return state.choice(kFrameKind) == kFloor && !state.choice(kRectangle).isEmpty();
          }),
      valueInput(kFrameHeight, QObject::tr("Height"), ValueKind::Length, QStringLiteral("2400 mm")).withVisible(kindIs(kWall)),
      valueInput(kFrameSpacing, QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("600 mm"))
          .withVisible([](const CommandState& state) { return !state.choice(kFrameKind).isEmpty(); }),
      choiceInput(kFrameTop, QObject::tr("Top Plates"), kFrameTops, QStringLiteral("1")).withVisible(kindIs(kWall)),
      choiceInput(kFrameCorner, QObject::tr("Corner"), kFrameCorners, kButt)
          .withChoiceIcons(kFrameCornerIcons)
          .withVisible(kindIs(kWall))
          .withTooltip(QObject::tr("How the wall meets the walls it joins: Butt, Lapped plates or Straight")),
      checkInput(kFrameBlocking, QObject::tr("Blocking"), false).withVisible(kindIs(kFloor)),
      valueInput(kFramePitch, QObject::tr("Pitch"), ValueKind::Angle, QStringLiteral("30 deg")).withVisible(kindIs(kRoof)),
      valueInput(kFrameOverhang, QObject::tr("Overhang"), ValueKind::Length, QStringLiteral("300 mm"))
          .withVisible(kindIs(kRoof)),
      choiceInput(kSize, QObject::tr("Size"), {}, QString())
          .withChoices([shared](const CommandState& state) {
            // The rows of the members' source in the Timber picker's catalog.
            Choices sizes;
            const std::optional<BuildPart> lead = frameOfPick(*shared, state);
            if (!lead) {
              return sizes;
            }
            for (const Part& part : shared->catalog.parts(kTimberRole)) {
              if (part.source == lead->source && !part.config.isEmpty()) {
                sizes.append({part.config, part.lessCommon() ? QObject::tr("%1 (less common)").arg(part.config) : part.config});
              }
            }
            if (sizes.isEmpty() && !lead->config.isEmpty()) {
              sizes.append({lead->config, lead->config});
            }
            return sizes;
          })
          .withVisible([](const CommandState& state) { return !state.choice(kFrameKind).isEmpty(); })
          .withTooltip(QObject::tr("The members' size (studs, joists, rafters)")),
  };
  // A wall's or a floor's Openings, the tile that opens the Openings window
  // as Wall's and Floor / Roof's do (mitcad#519), before Blocking.
  {
    int at = def.inputIndex(kFrameBlocking);
    for (const InputDef& input :
         openingsInputs(shared, def.id,
                        [](const CommandState& state) {
                          return state.choice(kFrameKind) == kWall || state.choice(kFrameKind) == kFloor;
                        },
                        kindIs(kFloor))) {
      def.inputs.insert(at++, input);
    }
  }
  def.ended = frameEnded(shared, def.id);
  def.previewMarkers = markersOf;
  def.enabled = [&context] {
    const QVector<BuildPart> parts = build::buildParts(context);
    return std::any_of(parts.begin(), parts.end(), [](const BuildPart& part) { return !frameLeadOf(part).isEmpty(); });
  };
  def.init = [shared](CommandState&, const CommandContext& model) {
    shared->refreshDesign(model);
    shared->catalog.refresh(kTimberRole, model);
  };
  def.inspect = [shared](const CommandState& state, const CommandContext& model) {
    Inspection result;
    const std::optional<BuildPart> lead = frameOfPick(*shared, state);
    if (!lead) {
      result.error = QObject::tr("Pick a member of a timber frame.");
      return result;
    }
    const QString name = str(frameDefinition(*lead), "name");
    QString problem;
    const QJsonObject frame = frameEditFields(*lead, state, problem);
    if (!problem.isEmpty()) {
      result.error = problem;
      result.log = QStringLiteral("frame %1 waiting: %2").arg(name, problem);
      return result;
    }
    const FramePreview preview = previewFrame(frame, lead->component, model);
    result.shape = preview.shape;
    const int count = preview.answer.value(QStringLiteral("count")).toInt();
    QStringList lines{QStringLiteral("<b>%1</b>").arg(name.toHtmlEscaped()),
                      QObject::tr("%1 member(s): %2").arg(count).arg(preview.counts).toHtmlEscaped()};
    // The openings by the wall's kind, marked in the view, and the sizes the
    // Openings window checks its rows against (mitcad#519).
    const QString kind = state.choice(kFrameKind);
    const int openings = static_cast<int>(frame.value(QStringLiteral("openings")).toArray().size());
    const QString openingText = openingsLine(openings, kind == kWall, loadBearingOf(state));
    if (!openingText.isEmpty()) {
      lines << openingText.toHtmlEscaped();
    }
    const int marked = state.choice(kFrameOpeningSelected).toInt();
    result.report = {{QStringLiteral("markers"), openingMarkers(preview.answer, marked)}};
    Shared::OpeningsSizes sizes;
    const Part* timber = nullptr;
    const QVector<Part> parts = shared->catalog.parts(kTimberRole);
    for (const Part& part : parts) {
      if (part.source == lead->source && (part.config == lead->config || timber == nullptr)) {
        timber = &part;
      }
    }
    if (kind == kWall) {
      sizes.wall.length = millimetresOf(state, kFrameLength, kNaN);
      sizes.wall.height = millimetresOf(state, kFrameHeight, 2400.0);
      sizes.wall.topPlates = state.choice(kFrameTop) == QStringLiteral("2") ? 2 : 1;
      if (timber != nullptr) {
        sizes.wall.studThick = std::min(timber->sectionWidth(), timber->sectionHeight());
        sizes.wall.studDepth = std::max(timber->sectionWidth(), timber->sectionHeight());
      }
      sizes.wall.loadBearing = loadBearingOf(state);
    } else if (kind == kFloor) {
      sizes.floorLength = millimetresOf(state, kFrameLength, kNaN);
      sizes.floorWidth = millimetresOf(state, kFrameWidth, kNaN);
    }
    shared->setOpeningsSizes(sizes);
    for (const QJsonValue& warning : preview.answer.value(QStringLiteral("warnings")).toArray()) {
      lines << QStringLiteral("<i>%1</i>").arg(warning.toString().toHtmlEscaped());
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.error = preview.error;
    result.log = QStringLiteral("frame %1 laid out again: %2 member(s) (%3)").arg(name).arg(count).arg(preview.counts);
    if (openings > 0) {
      result.log += QStringLiteral("; openings %1%2%3")
                        .arg(openings)
                        .arg(kind != kWall          ? QString()
                             : loadBearingOf(state) ? QStringLiteral(", load-bearing")
                                                    : QStringLiteral(", non-load-bearing"),
                             marked > 0 ? QStringLiteral(", opening %1 marked").arg(marked) : QString());
    }
    return result;
  };
  def.apply = [shared, &host, kSize](CommandState& state, const CommandContext& model) {
    const std::optional<BuildPart> lead = frameOfPick(*shared, state);
    if (!lead) {
      host.showStatus(QObject::tr("Pick a member of a timber frame."), NoticeLevel::Error);
      return false;
    }
    QString problem;
    const QJsonObject frame = frameEditFields(*lead, state, problem);
    if (!problem.isEmpty()) {
      host.showStatus(problem, NoticeLevel::Error);
      return false;
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("build_edit")},
                        {QStringLiteral("component"), lead->component},
                        {QStringLiteral("frame"), frame}};
    if (!state.choice(kSize).isEmpty() && state.choice(kSize) != lead->config) {
      command.insert(QStringLiteral("config"), state.choice(kSize));
    }
    QJsonObject result;
    if (!host.runModelCommand(command, &result)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("Timber frame %1: %2 member(s), %3 junction(s) fastened")
                             .arg(str(result, "name"))
                             .arg(result.value(QStringLiteral("count")).toInt())
                             .arg(result.value(QStringLiteral("fasteners")).toInt());
    // Laid out again in its own component (mitcad#522).
    const QString component = str(result, "frame_component");
    if (!component.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Frame component %1 (%2) in %3")
                               .arg(cmd::componentName(component, model), component, build::buildHost(model).name);
    }
    logJointsByComponent(model);
    shared->refreshDesign(model);
    return true;
  };
  return def;
}

// --- Panels

// A panel's thickness is a parameter of its own (else a fixed value).
bool thicknessIsParameter(const Part& part) {
  const QJsonValue thickness = part.build.value(QStringLiteral("panel")).toObject().value(QStringLiteral("thickness"));
  return thickness.isString() && !std::isfinite(build::millimetres(thickness));
}

// Panel's picker (mitcad#478): sheets (`mount: face`) on a timber member's
// face, slot panels on a profile's as before; without a pick, sheets only
// in a design of timber without slot profiles.
const QString kPanelRole = QStringLiteral("panel_role");

QString panelRole(const Shared& shared, const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("at"));
  if (!items.isEmpty()) {
    if (const std::optional<BuildPart> part = shared.partAt(items.first().occurrence)) {
      return part->isTimber() ? QStringLiteral("sheet") : QStringLiteral("panel");
    }
  }
  return shared.hasTimber() && !shared.hasSlotProfiles() ? QStringLiteral("sheet") : QStringLiteral("panel");
}

// A pick that changes the picker's role chooses a part of the new one, as
// the command would start with it.
void followPanelRole(Shared& shared, CommandState& state, const CommandContext& context) {
  const QString role = panelRole(shared, state);
  if (role == str(state.object(kPanelRole), "role")) {
    return;
  }
  state.setObject(kPanelRole, {{QStringLiteral("role"), role}});
  state.setChoice(kPart, QString());
  startPicker(shared, role, state, context);
  if (PartPicker* picker = shownPicker(shared, commandOf(role))) {
    picker->ask(roleRequest(shared, role, state));
  }
  qInfo().noquote() << QStringLiteral("BUILD panel picker: %1").arg(role == QStringLiteral("sheet") ? QStringLiteral("sheets on a face")
                                                                                                   : QStringLiteral("slot panels"));
}

QJsonObject panelFields(const CommandState& state, const CommandContext& context, const Part& part, QString& problem) {
  std::optional<QJsonObject> at = build::atOf(QStringLiteral("at"), state, context, problem);
  if (!at) {
    return {};
  }
  QJsonObject fields{{QStringLiteral("part"), part.ref()}, {QStringLiteral("at"), *at}};
  if (thicknessIsParameter(part)) {
    fields.insert(QStringLiteral("thickness"), state.expression(QStringLiteral("thickness")));
  }
  return fields;
}

CommandDef panelCommand(const SharedPtr& shared, DocumentHost& host) {
  // Slot panels, or sheets on a timber member's face (mitcad#478).
  const RoleOf roleOf = [shared](const CommandState& state) { return panelRole(*shared, state); };
  CommandDef def;
  def.id = QStringLiteral("build.panel");
  def.name = QObject::tr("Panel");
  def.icon = QStringLiteral("build-panel");
  def.tooltip = QObject::tr("Fills a frame with a sheet, in its slots or on its face");
  def.tab = kTab;
  def.group = QStringLiteral("PROFILE");
  def.pinned = true;
  def.keywords = {QStringLiteral("sheet"), QStringLiteral("plate"), QStringLiteral("infill"), QStringLiteral("build")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  def.inputs = pickerInputs(shared, roleOf);
  def.inputs.append(selectionInput(QStringLiteral("at"), QObject::tr("Frame Face"), SelectKind::Face, 1, 1)
                        .withOnPick([shared](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                          build::recordPick(QStringLiteral("at"), item, state, context);
                          followPanelRole(*shared, state, context);
                        },
                                    false)
                        // Shift + click: the pick and Place in one (mitcad#451).
                        .withShiftClickApplies()
                        .withTooltip(
                            QObject::tr("A face of a frame's profile, inside or outside, or a timber member's face")));
  def.inputs.append(valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length,
                               QStringLiteral("3 mm"))
                        .withVisible([shared](const CommandState& state) {
                          Part part;
                          return chosenPart(*shared, state, part) && thicknessIsParameter(part);
                        }));
  def.init = [shared](CommandState& state, const CommandContext& context) {
    shared->refreshDesign(context);
    const QString role = panelRole(*shared, state);
    state.setObject(kPanelRole, {{QStringLiteral("role"), role}});
    startPicker(*shared, role, state, context);
    if (PartPicker* picker = pinnedPicker(*shared)) {
      picker->ask(roleRequest(*shared, role, state));
    }
  };
  def.ended = pickerEnded(shared, def.id);
  def.inspect = [shared](const CommandState& state, const CommandContext& context) {
    Inspection result;
    Part part;
    if (!chosenPart(*shared, state, part)) {
      result.error = noPartsText(*shared, panelRole(*shared, state));
      return result;
    }
    QString problem;
    QJsonObject request = panelFields(state, context, part, problem);
    if (!problem.isEmpty()) {
      result.error = problem;
      return result;
    }
    QJsonObject shapeRequest = request;
    request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
    QJsonObject answer;
    try {
      answer = context.queryObject(request);
    } catch (const std::exception& error) {
      result.error = errorText(error);
      return result;
    }
    shapeRequest.insert(QStringLiteral("shape"), QStringLiteral("build_preview"));
    result.shape = context.analysisShape(shapeRequest);
    QStringList lines{QStringLiteral("<b>%1</b>").arg(part.name.toHtmlEscaped())};
    const QString seller = QStringList{part.supplier, part.partNumber}.join(QLatin1Char(' ')).trimmed();
    if (!seller.isEmpty()) {
      lines << seller.toHtmlEscaped();
    }
    const bool fits = !answer.contains(QStringLiteral("fits")) || answer.value(QStringLiteral("fits")).toBool();
    if (!fits) {
      lines << QObject::tr("Does not fit: %1").arg(str(answer, "reason").toHtmlEscaped());
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.error = problemsOf(answer);
    result.log = QStringLiteral("panel %1%2").arg(part.name, fits ? QString() : QStringLiteral(", does not fit: ") + str(answer, "reason"));
    return result;
  };
  def.apply = [shared, &host](CommandState& state, const CommandContext& context) {
    const QString role = panelRole(*shared, state);
    Part part;
    if (!chosenPart(*shared, state, part)) {
      host.showStatus(noPartsText(*shared, role), NoticeLevel::Error);
      return false;
    }
    QString problem;
    QJsonObject command = panelFields(state, context, part, problem);
    if (!problem.isEmpty()) {
      host.showStatus(problem, NoticeLevel::Error);
      return false;
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("build_panel"));
    QJsonObject result;
    if (!host.runModelCommand(command, &result)) {
      return false;
    }
    const bool fits = !result.contains(QStringLiteral("fits")) || result.value(QStringLiteral("fits")).toBool();
    qInfo().noquote() << QStringLiteral("Placed panel %1 as %2 (%3)%4; %5 clamp(s)")
                             .arg(part.name, str(result, "name").isEmpty() ? str(result, "component") : str(result, "name"),
                                  str(result, "occurrence"),
                                  fits ? QStringLiteral(", fits") : QStringLiteral(", does not fit: ") + str(result, "reason"))
                             .arg(result.value(QStringLiteral("adds")).toArray().size());
    if (!fits) {
      host.showStatus(QObject::tr("%1 does not fit: %2").arg(part.name, str(result, "reason")), NoticeLevel::Information);
    }
    state.setItems(QStringLiteral("at"), {});
    state.setObject(QStringLiteral("at_pick"), {});
    shared->lastPart.insert(role, part.key);
    shared->catalog.refresh(role, context);
    shared->refreshDesign(context);
    return true;
  };
  return def;
}

// --- Connectors

const QString kJunction = QStringLiteral("junction");

// The junction nearest to a point, not connected yet before connected ones.
QString nearestJunction(const Shared& shared, const std::optional<gp_Pnt>& point) {
  QString best;
  double bestDistance = std::numeric_limits<double>::infinity();
  for (const auto& option : shared.junctionOptions()) {
    const QJsonObject junction = shared.junction(option.first);
    const std::optional<gp_Pnt> at = build::junctionPoint(junction);
    double distance = point && at ? point->Distance(*at) : 0.0;
    if (!junction.value(QStringLiteral("connectors")).toArray().isEmpty()) {
      distance += 1e9; // a connected one only when no other is there
    }
    if (best.isEmpty() || distance < bestDistance) {
      best = option.first;
      bestDistance = distance;
    }
  }
  return best;
}

// --- Timber junctions (mitcad#478): a junction of two members (its `wood`)
// is offered wood fasteners, the outer connectors that fit it and the
// joinery kinds, a wood screw (two, sized from the members) by default so
// that one click finishes a joint; a slot junction its connectors only.

const QString kJointPrefix = QStringLiteral("joint:");

bool isWoodJunction(const QJsonObject& junction) { return junction.value(QStringLiteral("wood")).isObject(); }

// The joinery a junction takes: a housing where an end meets a side, a
// notch or a half-lap where members cross; none where ends meet in line.
Choices jointChoices(const QJsonObject& junction) {
  Choices joints;
  if (!isWoodJunction(junction) || str(junction, "kind") == QStringLiteral("inline")) {
    return joints;
  }
  const bool end = str(junction.value(QStringLiteral("a")).toObject(), "at").startsWith(QStringLiteral("end_"));
  if (end) {
    joints.append({kJointPrefix + QStringLiteral("housing"), QObject::tr("Housing: a pocket in the member it meets")});
  } else {
    joints.append({kJointPrefix + QStringLiteral("half_lap"), QObject::tr("Half-lap: both cut over their overlap")});
  }
  joints.append({kJointPrefix + QStringLiteral("notch"), QObject::tr("Notch: a cut the other's size")});
  return joints;
}

bool isJoint(const QString& key) { return key.startsWith(kJointPrefix); }

// Whether the Connector panel has a wood fastener at a timber junction.
bool woodFastenerChosen(const Shared& shared, const CommandState& state) {
  Part part;
  return isWoodJunction(shared.junction(state.choice(kJunction))) && shared.catalog.find(state.choice(kPart), part) &&
         part.isWoodFastener();
}

// The part a junction starts with: of a timber junction the default wood
// fastener (defaultWoodFastener, mitcad#516: the countersunk wood screw,
// not the pan-head connector screw), else an outer connector, else a
// joint; else the best fitting connector.
QString defaultConnector(const QVector<Part>& parts, const QJsonObject& junction) {
  if (isWoodJunction(junction)) {
    const auto fastener = defaultWoodFastener(parts);
    if (fastener != parts.cend()) {
      return fastener->key;
    }
    if (parts.isEmpty()) {
      const Choices joints = jointChoices(junction);
      return joints.isEmpty() ? QString() : joints.first().first;
    }
  }
  return parts.isEmpty() ? QString() : parts.first().key;
}

void chooseConnector(Shared& shared, CommandState& state) {
  const QJsonObject junction = shared.junction(state.choice(kJunction));
  if (junction.isEmpty()) {
    return;
  }
  const QVector<Part> parts = shared.catalog.forJunction(junction);
  const QString chosen = state.choice(kPart);
  const Choices joints = jointChoices(junction);
  const bool kept = std::any_of(parts.begin(), parts.end(), [&chosen](const Part& part) { return part.key == chosen; }) ||
                    std::any_of(joints.begin(), joints.end(), [&chosen](const auto& joint) { return joint.first == chosen; });
  if (!kept) {
    state.setChoice(kPart, defaultConnector(parts, junction));
  }
}

const QString kConnectorCommand = QStringLiteral("build.connector");

// What Connector asks the part picker for (mitcad#418): the connectors
// that fit the junction, best first (the drop-down's keys), then those that
// nearly fit, greyed with the reason (library_build_candidates' `refused`).
PickerRequest connectorRequest(Shared& shared, const CommandState& state) {
  PickerRequest request;
  request.command = kConnectorCommand;
  request.title = QObject::tr("Connector");
  request.current = state.choice(kPart);
  const QJsonObject junction = shared.junction(state.choice(kJunction));
  if (junction.isEmpty()) {
    return request;
  }
  request.parts = pickerParts(shared.catalog.forJunction(junction));
  for (const PickerPart& nearly : buildCandidates(QJsonObject{{kJunction, junction}})) {
    if (!nearly.refused.isEmpty()) {
      request.parts.append(nearly);
    }
  }
  return request;
}

// The picker open for Connector follows it: another junction's connectors
// (`junction`), else the connector chosen.
void syncConnectorPicker(Shared& shared, const CommandState& state, bool junction) {
  if (PartPicker* picker = shownPicker(shared, kConnectorCommand)) {
    if (junction) {
      picker->setParts(connectorRequest(shared, state).parts, state.choice(kPart));
    } else {
      picker->setCurrent(state.choice(kPart));
    }
  }
}

// build_connect's fields, also its preview's.
QJsonObject connectFields(const CommandState& state, const QJsonObject& junction, const Part& part) {
  if (isWoodJunction(junction)) {
    // A timber junction (mitcad#478): a wood fastener with its count (no
    // row: the model sizes it, unless a size is chosen); an outer connector
    // with its screws.
    QJsonObject fields{{kJunction, str(junction, "id")}, {QStringLiteral("part"), part.ref()}};
    if (part.isWoodFastener()) {
      const double count = state.value(QStringLiteral("count"));
      fields.insert(QStringLiteral("count"), std::isfinite(count) && count >= 1.0 ? static_cast<int>(std::lround(count)) : 2);
      const QString size = state.choice(QStringLiteral("size"));
      if (!size.isEmpty() && size != kAuto) {
        QJsonObject ref = part.ref();
        ref.insert(QStringLiteral("config"), size);
        fields.insert(QStringLiteral("part"), ref);
      }
    }
    return fields;
  }
  return {{kJunction, str(junction, "id")},
          {QStringLiteral("part"), part.ref()},
          {QStringLiteral("fasteners"), state.checked(QStringLiteral("fasteners"))},
          {QStringLiteral("machining"), state.checked(QStringLiteral("machining"))}};
}

// build_connect's fields of a joint (mitcad#478): its kind and, when typed,
// its depth.
QJsonObject jointFields(const CommandState& state, const QJsonObject& junction) {
  QJsonObject joint{{QStringLiteral("kind"), state.choice(kPart).mid(kJointPrefix.size())}};
  if (state.checked(QStringLiteral("own_depth"))) {
    joint.insert(QStringLiteral("depth"), state.expression(QStringLiteral("depth")));
  }
  return {{kJunction, str(junction, "id")}, {QStringLiteral("joint"), joint}};
}

// Holes to machine, [{end, hole: {diameter, depth, thread}}] in
// millimetres: "D6.8 x 15 M8 at end a".
QString holesText(const QJsonValue& machining) {
  QStringList holes;
  for (const QJsonValue& value : machining.toArray()) {
    const QJsonObject entry = value.toObject();
    // {end, hole: {...}} (build_edit's, build_parts'), or flat {at, diameter, depth,
    // thread} (build_preview's of a connector, the cut list's).
    const QJsonObject hole = entry.value(QStringLiteral("hole")).isObject() ? entry.value(QStringLiteral("hole")).toObject() : entry;
    // A reply's millimetres, or a value as a command was given it.
    const auto amount = [](const QJsonValue& value) {
      return value.isDouble() ? QString::number(value.toDouble(), 'g', 4) : value.toString();
    };
    QString shown = QStringLiteral("D%1").arg(amount(hole.value(QStringLiteral("diameter"))));
    if (!hole.value(QStringLiteral("depth")).isUndefined()) {
      shown += QStringLiteral(" x %1").arg(amount(hole.value(QStringLiteral("depth"))));
    }
    if (!str(hole, "thread").isEmpty()) {
      shown += QLatin1Char(' ') + str(hole, "thread");
    }
    if (!str(entry, "end").isEmpty()) {
      shown += QStringLiteral(" at end ") + str(entry, "end");
    } else if (!str(entry, "at").isEmpty()) {
      shown += QStringLiteral(" at ") + str(entry, "at");
    }
    holes << shown;
  }
  return holes.join(QStringLiteral("; "));
}

CommandDef connectorCommand(const SharedPtr& shared, DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.connector");
  def.name = QObject::tr("Connector");
  def.icon = QStringLiteral("build-connector");
  def.tooltip = QObject::tr("Connects two profiles where they meet with a connector that fits");
  def.tab = kTab;
  def.group = QStringLiteral("CONNECT");
  def.pinned = true;
  def.keywords = {QStringLiteral("bracket"), QStringLiteral("corner"), QStringLiteral("angle"),
                  QStringLiteral("junction"), QStringLiteral("build")};
  def.okLabel = QObject::tr("Connect");
  def.repeats = true;
  def.inputs = {
      selectionInput(QStringLiteral("at"), QObject::tr("Near"), kPartKinds, 0, 1)
          .withOnPick([shared](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            build::recordPick(QStringLiteral("at"), item, state, model);
            const QString before = state.choice(kJunction);
            state.setChoice(kJunction, nearestJunction(*shared, build::pickPoint(QStringLiteral("at"), state, model)));
            chooseConnector(*shared, state);
            syncConnectorPicker(*shared, state, state.choice(kJunction) != before);
          },
                      false)
          // Shift + click: the junction chosen and Connect in one (mitcad#451).
          .withShiftClickApplies()
          .withTooltip(QObject::tr("A click on a profile near a junction chooses that junction; Shift+click "
                                   "connects it at once")),
      choiceInput(kJunction, QObject::tr("Junction"), {}, QString())
          .withChoices([shared](const CommandState&) { return shared->junctionOptions(); })
          .withOnChange([shared](CommandState& state, const CommandContext&) {
            chooseConnector(*shared, state);
            syncConnectorPicker(*shared, state, true);
          }),
      choiceInput(kPart, QObject::tr("Connector"), {}, QString())
          .withOnChange([shared](CommandState& state, const CommandContext&) {
            syncConnectorPicker(*shared, state, false);
          })
          // A tile that opens the part picker (mitcad#427); a joint (mitcad#478)
          // is no library part: its name.
          .withChoiceTile([shared](const CommandState& state) {
            if (isJoint(state.choice(kPart))) {
              const Choices joints = jointChoices(shared->junction(state.choice(kJunction)));
              return ChoiceTile{QImage(), cmd::label(joints, state.choice(kPart)),
                                QObject::tr("Joinery: cut in the members")};
            }
            return partTile(*shared, state);
          })
          .withButton(kButtonText, buttonTip(),
                      [shared](const CommandState& state, const CommandContext&) {
                        if (PartPicker* picker = partPicker(*shared, true)) {
                          picker->ask(connectorRequest(*shared, state));
                        }
                      })
          .withChoices([shared](const CommandState& state) {
            Choices choices;
            const QJsonObject junction = shared->junction(state.choice(kJunction));
            if (junction.isEmpty()) {
              return choices;
            }
            for (const Part& part : shared->catalog.forJunction(junction)) {
              // A wood fastener without a row is sized from the members.
              choices.append({part.key, isWoodJunction(junction) && part.isWoodFastener()
                                            ? QObject::tr("%1 (sized from the members)").arg(part.label())
                                            : part.label()});
            }
            // A timber junction's joinery (mitcad#478).
            choices += jointChoices(junction);
            return choices;
          })
          // Each connector with the library's picture of it.
          .withChoiceImages([shared](const QString& key) {
            Part part;
            return shared->catalog.find(key, part) ? shared->thumbnail(part) : QImage();
          })
          .withTooltip(QObject::tr("The connector chosen; a click (or Enter) opens the component picker")),
      // A timber junction (mitcad#478): a wood fastener's count and size, a
      // joint's depth; the nuts and the end machining are the slots'.
      valueInput(QStringLiteral("count"), QObject::tr("Count"), ValueKind::Unitless, QStringLiteral("2")).withStepMinimum(1)
          .withVisible([shared](const CommandState& state) { return woodFastenerChosen(*shared, state); })
          .withTooltip(QObject::tr("How many screws or nails, in a row along the contact")),
      choiceInput(QStringLiteral("size"), QObject::tr("Size"), {{kAuto, QStringLiteral("Auto: from the members")}}, kAuto)
          .withChoices([shared](const CommandState& state) {
            Choices sizes{{kAuto, QObject::tr("Auto: from the members")}};
            for (const QString& row : shared->catalog.rowsOf(state.choice(kPart))) {
              sizes.append({row, row});
            }
            return sizes;
          })
          .withVisible([shared](const CommandState& state) { return woodFastenerChosen(*shared, state); })
          .withTooltip(QObject::tr("The fastener's size, or Auto: sized from the members it joins")),
      checkInput(QStringLiteral("own_depth"), QObject::tr("Own Depth"), false)
          .withVisible([](const CommandState& state) { return isJoint(state.choice(kPart)); })
          .withTooltip(QObject::tr("Off: the joint's default depth (a half-lap half and half, a housing a third)")),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible([](const CommandState& state) {
            return isJoint(state.choice(kPart)) && state.checked(QStringLiteral("own_depth"));
          })
          .withTooltip(QObject::tr("The cut's depth")),
      checkInput(QStringLiteral("fasteners"), QObject::tr("With Nuts and Screws"), true)
          .withVisible([shared](const CommandState& state) { return !isWoodJunction(shared->junction(state.choice(kJunction))); }),
      checkInput(QStringLiteral("machining"), QObject::tr("With End Machining"), true)
          .withVisible([shared](const CommandState& state) { return !isWoodJunction(shared->junction(state.choice(kJunction))); }),
      checkInput(QStringLiteral("all_junctions"), QObject::tr("Show All Junctions"), false)
          .withTooltip(QObject::tr("Off: a profile's junctions show while the mouse is over it")),
  };
  def.enabled = [&context] { return build::hasBuildParts(context); };
  def.init = [shared](CommandState& state, const CommandContext& model) {
    shared->catalog.refresh(QStringLiteral("connector"), model);
    shared->refreshDesign(model);
    if (shared->junction(state.choice(kJunction)).isEmpty()) {
      state.setChoice(kJunction, nearestJunction(*shared, std::nullopt));
    }
    chooseConnector(*shared, state);
    if (PartPicker* picker = pinnedPicker(*shared)) {
      picker->ask(connectorRequest(*shared, state));
    }
  };
  def.ended = pickerEnded(shared, def.id);
  def.inspect = [shared](const CommandState& state, const CommandContext& model) {
    Inspection result;
    if (state.items(QStringLiteral("at")).isEmpty()) {
      result.viewHint = QObject::tr("Click a profile near a junction; Shift+click connects at once"); // mitcad#451
    }
    const QJsonObject junction = shared->junction(state.choice(kJunction));
    // The junctions with their parts, for the markers of the hover
    // (connectorMarkers); the chosen one's label first.
    QJsonArray junctions;
    for (const auto& option : shared->junctionOptions()) {
      const QJsonObject other = shared->junction(option.first);
      const std::optional<gp_Pnt> at = build::junctionPoint(other);
      if (!at) {
        continue;
      }
      QString shown = str(other, "kind");
      if (!other.value(QStringLiteral("connectors")).toArray().isEmpty()) {
        shown += QObject::tr(" (connected)");
      }
      if (option.first == state.choice(kJunction)) {
        shown = QStringLiteral("> ") + shown;
      }
      QJsonObject entry = marker(shown, *at);
      entry.insert(QStringLiteral("id"), option.first);
      QJsonArray parts;
      for (const char* side : {"a", "b"}) {
        parts.append(build::rootPath(str(other.value(QLatin1String(side)).toObject(), "occurrence"), model));
      }
      entry.insert(QStringLiteral("parts"), parts);
      junctions.append(entry);
    }
    result.report = {{QStringLiteral("junctions"), junctions}};
    if (junction.isEmpty()) {
      result.error = QObject::tr("No junctions: place profiles that meet first.");
      return result;
    }
    for (const char* side : {"a", "b"}) {
      const QString path = build::rootPath(str(junction.value(QLatin1String(side)).toObject(), "occurrence"), model);
      if (!path.isEmpty()) {
        result.highlights.append(SelectionItem{SelectKind::Component, QString(), QString(), QString(), path});
      }
    }
    QStringList refused;
    const QVector<Part> parts = shared->catalog.forJunction(junction, &refused);
    QStringList lines{QStringLiteral("<b>%1</b>").arg(build::junctionText(junction, model).toHtmlEscaped())};
    QStringList systems;
    for (const QJsonValue& system : junction.value(QStringLiteral("systems")).toArray()) {
      systems << (system.isString() ? system.toString()
                                    : QString::fromUtf8(QJsonDocument(system.toObject()).toJson(QJsonDocument::Compact)));
    }
    if (!systems.isEmpty()) {
      lines << QObject::tr("Systems: %1").arg(systems.join(QStringLiteral(", ")).toHtmlEscaped());
    }
    for (const Part& part : parts) {
      const QString shown = part.label().toHtmlEscaped();
      lines << (part.key == state.choice(kPart) ? QStringLiteral("<b>%1</b>").arg(shown) : shown);
    }
    for (const QString& reason : refused) {
      lines << QStringLiteral("<span style=\"color: %1\">%2</span>")
                   .arg(themeColor(ColorRole::TextSecondary).name(), reason.toHtmlEscaped());
    }
    QString preview;
    Part part;
    const Choices joints = jointChoices(junction);
    if (isWoodJunction(junction)) {
      // A timber junction (mitcad#478): the members' sections and the
      // joinery it takes.
      const QJsonObject sections =
          junction.value(QStringLiteral("wood")).toObject().value(QStringLiteral("sections")).toObject();
      const auto section = [&sections](const char* side) {
        const QJsonArray size = sections.value(QLatin1String(side)).toArray();
        return size.size() == 2 ? QStringLiteral("%1 x %2").arg(size[0].toDouble()).arg(size[1].toDouble()) : QString();
      };
      lines << QObject::tr("Timber: %1 and %2").arg(section("a"), section("b"));
      for (const auto& joint : joints) {
        lines << (joint.first == state.choice(kPart) ? QStringLiteral("<b>%1</b>").arg(joint.second.toHtmlEscaped())
                                                     : joint.second.toHtmlEscaped());
      }
    }
    if (isJoint(state.choice(kPart))) {
      // The joint's cuts (build_preview with `joint`).
      QJsonObject request = jointFields(state, junction);
      request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
      QJsonObject answer;
      try {
        answer = model.queryObject(request);
      } catch (const std::exception& error) {
        result.error = errorText(error);
      }
      QStringList cuts;
      for (const QJsonValue& value : answer.value(QStringLiteral("cuts")).toArray()) {
        const QJsonObject cut = value.toObject();
        QStringList size;
        for (const QJsonValue& side : cut.value(QStringLiteral("size")).toArray()) {
          size << QString::number(side.toDouble(), 'f', 1);
        }
        cuts << QStringLiteral("%1 in %2 (%3 mm)")
                    .arg(str(cut, "kind"), cmd::componentName(str(cut, "component"), model),
                         size.join(QStringLiteral(" x ")));
      }
      lines << QObject::tr("Preview: %1").arg(cuts.isEmpty() ? QObject::tr("no cut") : cuts.join(QStringLiteral("; ")).toHtmlEscaped());
      if (result.error.isEmpty()) {
        result.error = problemsOf(answer);
      }
      const QStringList warnings = warningsOf(answer);
      for (const QString& warning : warnings) {
        lines << QStringLiteral("<span style=\"color: %1\">%2</span>")
                     .arg(warningColor().name(), warning.toHtmlEscaped());
      }
      preview = QStringLiteral(", preview %1: %2").arg(state.choice(kPart).mid(kJointPrefix.size()), cuts.join(QStringLiteral("; ")));
    } else if (parts.isEmpty() && joints.isEmpty()) {
      result.error = QObject::tr("No connector of the fetched libraries fits this junction.");
    } else if (shared->catalog.find(state.choice(kPart), part)) {
      // The chosen connector where it goes, with its fasteners and the
      // machining it needs (build_preview of build_connect's fields).
      const QJsonObject fields = connectFields(state, junction, part);
      QJsonObject request = fields;
      request.insert(QStringLiteral("query"), QStringLiteral("build_preview"));
      QJsonObject answer;
      try {
        answer = model.queryObject(request);
      } catch (const std::exception& error) {
        result.error = errorText(error);
      }
      QJsonObject shapeRequest = fields;
      shapeRequest.insert(QStringLiteral("shape"), QStringLiteral("build_preview"));
      result.shape = model.analysisShape(shapeRequest);
      QJsonArray markers;
      int fasteners = 0;
      for (const QJsonValue& value : answer.value(QStringLiteral("fasteners")).toArray()) {
        const QJsonObject fastener = value.toObject();
        // {frame, at}: the fastener's frame and the connector's frame it
        // goes on; of wood fasteners (mitcad#478) the frame's name and the
        // frame where it goes.
        const bool wood = fastener.value(QStringLiteral("at")).isObject();
        const build::Frame frame =
            build::frameOf(fastener.value(wood ? QStringLiteral("at") : QStringLiteral("frame")));
        if (frame.valid) {
          const QString at = wood ? QString() : str(fastener, "at");
          markers.append(marker(at.isEmpty() ? QObject::tr("fastener") : QObject::tr("fastener %1").arg(at), frame.origin));
          ++fasteners;
        }
      }
      // A wood fastener's size, from the members unless chosen, and the
      // warnings of its embedment and edge distances.
      const QJsonObject size = answer.value(QStringLiteral("size")).toObject();
      if (!size.isEmpty()) {
        lines << QObject::tr("Size %1 (D%2 × %3 mm)")
                     .arg(str(size, "config"))
                     .arg(size.value(QStringLiteral("diameter")).toDouble(), 0, 'g', 3)
                     .arg(size.value(QStringLiteral("length")).toDouble(), 0, 'g', 4);
      }
      for (const QString& warning : warningsOf(answer)) {
        lines << QStringLiteral("<span style=\"color: %1\">%2</span>")
                     .arg(warningColor().name(), warning.toHtmlEscaped());
      }
      // The holes it machines: {component, at, point, diameter, depth, thread}.
      for (const QJsonValue& value : answer.value(QStringLiteral("machining")).toArray()) {
        const QJsonObject hole = value.toObject();
        if (const std::optional<gp_Pnt> point = pointFrom(hole.value(QStringLiteral("point")))) {
          markers.append(marker(QObject::tr("bore D%1").arg(hole.value(QStringLiteral("diameter")).toDouble(), 0, 'g', 4),
                                *point));
        }
      }
      result.report.insert(QStringLiteral("markers"), markers);
      const QString holes = holesText(answer.value(QStringLiteral("machining")));
      lines << QObject::tr("Preview: %1, %2 fastener(s)%3")
                   .arg(part.name.toHtmlEscaped())
                   .arg(fasteners)
                   .arg(holes.isEmpty() ? QString() : QObject::tr(", machining %1").arg(holes.toHtmlEscaped()));
      if (result.error.isEmpty()) {
        result.error = problemsOf(answer);
      }
      preview = QStringLiteral(", preview %1 with %2 fastener(s)%3")
                    .arg(part.name)
                    .arg(fasteners)
                    .arg(holes.isEmpty() ? QString() : QStringLiteral(", machining ") + holes);
      if (!size.isEmpty()) {
        preview += QStringLiteral(", size %1").arg(str(size, "config"));
      }
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("junction %1 (%2): %3 fit, %4 nearly%5")
                     .arg(str(junction, "id"), str(junction, "kind"))
                     .arg(parts.size())
                     .arg(refused.size())
                     .arg(preview);
    return result;
  };
  // The junctions show on the hover of their profiles (all of them with
  // Show All Junctions), the chosen one always; the connector's fasteners.
  def.hover = [](const SelectionItem& item, CommandState& state, const CommandContext&) {
    const QString occurrence = item.isValid() ? item.occurrence : QString();
    if (state.object(QStringLiteral("hover")).value(QStringLiteral("occurrence")).toString() == occurrence) {
      return false;
    }
    state.setObject(QStringLiteral("hover"), {{QStringLiteral("occurrence"), occurrence}});
    return true;
  };
  def.previewMarkersIn = [](const QJsonObject& report, const CommandState& state) {
    std::vector<PreviewMarker> markers = markersOf(report);
    const bool all = state.checked(QStringLiteral("all_junctions"));
    const QString hovered = state.object(QStringLiteral("hover")).value(QStringLiteral("occurrence")).toString();
    for (const QJsonValue& value : report.value(QStringLiteral("junctions")).toArray()) {
      const QJsonObject entry = value.toObject();
      bool shown = all || str(entry, "id") == state.choice(kJunction);
      for (const QJsonValue& part : entry.value(QStringLiteral("parts")).toArray()) {
        const QString path = part.toString();
        shown = shown || (!path.isEmpty() && !hovered.isEmpty() &&
                          (hovered == path || hovered.startsWith(path + QLatin1Char('/'))));
      }
      const std::optional<gp_Pnt> point = pointFrom(entry.value(QStringLiteral("point")));
      if (shown && point) {
        markers.push_back({str(entry, "label"), *point});
      }
    }
    return markers;
  };
  def.apply = [shared, &host](CommandState& state, const CommandContext& model) {
    const QJsonObject junction = shared->junction(state.choice(kJunction));
    Part part;
    const bool joint = !junction.isEmpty() && isJoint(state.choice(kPart));
    if (junction.isEmpty() || (!joint && !shared->catalog.find(state.choice(kPart), part))) {
      host.showStatus(QObject::tr("Choose a junction and a connector."), NoticeLevel::Error);
      return false;
    }
    QJsonObject command = joint ? jointFields(state, junction) : connectFields(state, junction, part);
    command.insert(QStringLiteral("cmd"), QStringLiteral("build_connect"));
    QJsonObject result;
    if (!host.runModelCommand(command, &result)) {
      return false;
    }
    // A timber junction's warnings (embedment, edge distances; mitcad#478).
    const QStringList warnings = isWoodJunction(junction) ? warningsOf(result) : QStringList();
    if (joint) {
      // Joinery (mitcad#478): the members cut, no part made.
      qInfo().noquote() << QStringLiteral("Connected %1 at junction %2 (%3): %4 cut(s)%5")
                               .arg(state.choice(kPart).mid(kJointPrefix.size()), str(junction, "id"),
                                    str(junction, "kind"))
                               .arg(result.value(QStringLiteral("cuts")).toArray().size())
                               .arg(warnings.isEmpty() ? QString() : QStringLiteral("; ") + warnings.join(QStringLiteral("; ")));
    } else {
      qInfo().noquote() << QStringLiteral("Connected %1 at junction %2 (%3) as %4: %5 added%6")
                               .arg(part.name, str(junction, "id"), str(junction, "kind"), str(result, "component"))
                               .arg(result.value(QStringLiteral("adds")).toArray().size())
                               .arg(warnings.isEmpty() ? QString() : QStringLiteral("; ") + warnings.join(QStringLiteral("; ")));
    }
    if (!warnings.isEmpty()) {
      host.showStatus(warnings.join(QLatin1Char(' ')), NoticeLevel::Warning);
    }
    state.setItems(QStringLiteral("at"), {});
    shared->catalog.refresh(QStringLiteral("connector"), model);
    shared->refreshDesign(model);
    // The next junction not connected yet.
    state.setChoice(kJunction, nearestJunction(*shared, build::junctionPoint(junction)));
    chooseConnector(*shared, state);
    syncConnectorPicker(*shared, state, true);
    return true;
  };
  return def;
}

// --- Fasteners

// The connector of a picked part: itself, or the connector that added it
// (a nut, a screw).
std::optional<BuildPart> connectorOf(const Shared& shared, const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("connector"));
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const std::optional<BuildPart> picked = shared.partAt(items.first().occurrence);
  if (!picked) {
    return std::nullopt;
  }
  if (picked->role == QStringLiteral("connector") || picked->addedBy.isEmpty()) {
    return picked;
  }
  for (const BuildPart& part : shared.designParts()) {
    if (part.component == picked->addedBy) {
      return part;
    }
  }
  return picked;
}

// The connector's fasteners, by source and row: "<source>|<row>".
QHash<QString, QVector<BuildPart>> fastenersOf(const Shared& shared, const BuildPart& connector) {
  QHash<QString, QVector<BuildPart>> groups;
  for (const BuildPart& part : shared.designParts()) {
    if (part.addedBy == connector.component && part.role == QStringLiteral("fastener")) {
      groups[part.source + QLatin1Char('|') + part.config].append(part);
    }
  }
  return groups;
}

// The configuration rows of a source's library component (library_show),
// asked once per source.
Choices rowsOfSource(Shared& shared, const QString& source, const CommandContext& context) {
  {
    const std::lock_guard<std::mutex> lock(shared.mutex);
    const auto found = shared.rows.constFind(source);
    if (found != shared.rows.cend()) {
      return found.value();
    }
  }
  Choices rows;
  try {
    for (const QJsonValue& value : context.queryObject({{QStringLiteral("query"), QStringLiteral("library_parts")}})
                                       .value(QStringLiteral("parts"))
                                       .toArray()) {
      const QJsonObject entry = value.toObject();
      if (str(entry, "source") != source) {
        continue;
      }
      const QJsonObject record = entry.value(QStringLiteral("library")).toObject();
      const QJsonObject shown = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_show")},
                                                {QStringLiteral("url"), str(record, "url")},
                                                {QStringLiteral("rev"), str(record, "rev")}});
      for (const QJsonValue& component : shown.value(QStringLiteral("components")).toArray()) {
        if (str(component.toObject(), "id") != str(record, "component")) {
          continue;
        }
        for (const QJsonValue& row : component.toObject()
                                         .value(QStringLiteral("configurations"))
                                         .toObject()
                                         .value(QStringLiteral("rows"))
                                         .toArray()) {
          rows.append({str(row.toObject(), "name"), str(row.toObject(), "name")});
        }
      }
    }
  } catch (const std::exception&) {
    // Its own row then.
  }
  const std::lock_guard<std::mutex> lock(shared.mutex);
  shared.rows.insert(source, rows);
  return rows;
}

CommandDef fastenersCommand(const SharedPtr& shared, DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.fasteners");
  def.name = QObject::tr("Fasteners");
  def.icon = QStringLiteral("build-fasteners");
  def.tooltip = QObject::tr("The nuts and screws of a connector: another size, taken off, or added again");
  def.tab = kTab;
  def.group = QStringLiteral("CONNECT");
  def.pinned = true;
  def.keywords = {QStringLiteral("nut"), QStringLiteral("screw"), QStringLiteral("t-nut"), QStringLiteral("bolt"),
                  QStringLiteral("build")};
  const QString kFastener = QStringLiteral("fastener");
  const QString kSize = QStringLiteral("size");
  def.inputs = {
      selectionInput(QStringLiteral("connector"), QObject::tr("Connector"), kPartKinds, 1, 1)
          .withOnPick([shared, kFastener, kSize](const SelectionItem&, CommandState& state, const CommandContext&) {
            const std::optional<BuildPart> connector = connectorOf(*shared, state);
            const QHash<QString, QVector<BuildPart>> groups =
                connector ? fastenersOf(*shared, *connector) : QHash<QString, QVector<BuildPart>>();
            state.setChecked(QStringLiteral("with"), !groups.isEmpty() || !connector);
            if (!groups.isEmpty()) {
              const QString first = groups.keys().first();
              state.setChoice(kFastener, first);
              state.setChoice(kSize, first.section(QLatin1Char('|'), 1));
            }
          },
                      false)
          .withTooltip(QObject::tr("A connector, or one of its nuts or screws")),
      checkInput(QStringLiteral("with"), QObject::tr("With Nuts and Screws"), true)
          .withTooltip(QObject::tr("Off: they are taken off. On for a connector without them: it is connected again "
                                   "with them")),
      choiceInput(kFastener, QObject::tr("Fastener"), {}, QString())
          .withChoices([shared](const CommandState& state) {
            Choices choices;
            if (const std::optional<BuildPart> connector = connectorOf(*shared, state)) {
              const QHash<QString, QVector<BuildPart>> groups = fastenersOf(*shared, *connector);
              for (auto it = groups.cbegin(); it != groups.cend(); ++it) {
                choices.append({it.key(), QStringLiteral("%1 x %2").arg(it.value().size()).arg(it.value().first().shownName())});
              }
            }
            return choices;
          })
          .withVisible([](const CommandState& state) { return state.checked(QStringLiteral("with")); }),
      choiceInput(kSize, QObject::tr("Size"), {}, QString())
          .withChoices([shared, &context, kFastener](const CommandState& state) {
            const QString source = state.choice(kFastener).section(QLatin1Char('|'), 0, 0);
            return source.isEmpty() ? Choices() : rowsOfSource(*shared, source, context);
          })
          .withVisible([](const CommandState& state) { return state.checked(QStringLiteral("with")); }),
  };
  def.enabled = [&context] { return build::hasBuildParts(context); };
  def.init = [shared](CommandState&, const CommandContext& model) { shared->refreshDesign(model); };
  def.inspect = [shared](const CommandState& state, const CommandContext&) {
    Inspection result;
    const std::optional<BuildPart> connector = connectorOf(*shared, state);
    if (!connector) {
      result.error = QObject::tr("Pick a connector of the Build tab.");
      return result;
    }
    QStringList lines{QStringLiteral("<b>%1</b>").arg(connector->shownName().toHtmlEscaped())};
    const QHash<QString, QVector<BuildPart>> groups = fastenersOf(*shared, *connector);
    for (auto it = groups.cbegin(); it != groups.cend(); ++it) {
      lines << QStringLiteral("%1 x %2").arg(it.value().size()).arg(it.value().first().shownName().toHtmlEscaped());
    }
    if (groups.isEmpty()) {
      lines << QObject::tr("No nuts or screws");
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("connector %1: %2 kind(s) of fastener").arg(connector->component).arg(groups.size());
    return result;
  };
  def.apply = [shared, &host, kFastener, kSize](CommandState& state, const CommandContext& model) {
    const std::optional<BuildPart> connector = connectorOf(*shared, state);
    if (!connector) {
      host.showStatus(QObject::tr("Pick a connector of the Build tab."), NoticeLevel::Error);
      return false;
    }
    const QHash<QString, QVector<BuildPart>> groups = fastenersOf(*shared, *connector);
    QJsonArray commands;
    if (!state.checked(QStringLiteral("with"))) {
      for (const QVector<BuildPart>& group : groups) {
        for (const BuildPart& fastener : group) {
          commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_remove")},
                                      {QStringLiteral("component"), fastener.component}});
        }
      }
    } else if (groups.isEmpty()) {
      // Connected again, with them, where it is.
      QString junction;
      for (const QJsonValue& value : build::junctions(model)) {
        if (value.toObject().value(QStringLiteral("connectors")).toArray().contains(connector->component)) {
          junction = str(value.toObject(), "id");
        }
      }
      if (junction.isEmpty() || connector->source.isEmpty()) {
        host.showStatus(QObject::tr("%1 is at no junction to connect again.").arg(connector->shownName()), NoticeLevel::Error);
        return false;
      }
      QJsonObject part{{QStringLiteral("source"), connector->source}};
      if (!connector->config.isEmpty()) {
        part.insert(QStringLiteral("config"), connector->config);
      }
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_remove")},
                                  {QStringLiteral("component"), connector->component}});
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_connect")},
                                  {kJunction, junction},
                                  {QStringLiteral("part"), part},
                                  {QStringLiteral("fasteners"), true}});
    } else {
      const QString size = state.choice(kSize);
      for (const BuildPart& fastener : groups.value(state.choice(kFastener))) {
        if (!size.isEmpty() && fastener.config != size) {
          commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_edit")},
                                      {QStringLiteral("component"), fastener.component},
                                      {QStringLiteral("config"), size}});
        }
      }
    }
    if (commands.isEmpty()) {
      return true; // nothing to change
    }
    const QString label = QObject::tr("Fasteners of %1").arg(connector->shownName());
    if (!host.runModelCommands(commands, label)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("%1: %2 command(s)").arg(label).arg(commands.size());
    return true;
  };
  return def;
}

// --- Swap Fastener (mitcad#497; app/COMMANDS.md, "Swap Fastener")
//
// The fasteners selected made from another fastener where they are, in
// their joints and junctions (build_swap_fasteners, one undo step). Its
// Fastener offers the compatible ones: wood fasteners for wood screws and
// nails (with Size: Auto, sized from the members, or a row), the other
// fasteners of the first one's slot system for a connector's nuts and
// screws. In the view's context menu under BUILD for fasteners only.

const QString kSwapCommand = QStringLiteral("build.swap_fastener");
const QString kSwapItems = QStringLiteral("fasteners");
const QString kSlotFastenerRole = QStringLiteral("fastener");
// A fastener is small: a right-click on it may hit any of its items.
constexpr SelectFilter kFastenerKinds = SelectKind::Face | SelectKind::Body | SelectKind::Edge | SelectKind::Vertex;

// The fasteners the items belong to, each once (from the design's parts
// as read last).
QVector<BuildPart> swapTargets(const Shared& shared, const Selection& items) {
  QVector<BuildPart> found;
  for (const SelectionItem& item : items) {
    const std::optional<BuildPart> part = shared.partAt(item.occurrence);
    if (part && part->role == QStringLiteral("fastener") &&
        std::none_of(found.begin(), found.end(), [&part](const BuildPart& known) { return known.component == part->component; })) {
      found.append(*part);
    }
  }
  return found;
}

// The libraries' part a fastener of the design is made from (its source):
// a wood fastener's without its row, another's in its row.
std::optional<Part> sourcePart(const Shared& shared, const BuildPart& fastener) {
  for (const Part& part : woodFasteners(shared)) {
    if (!fastener.source.isEmpty() && part.source == fastener.source) {
      return part;
    }
  }
  for (const Part& part : shared.catalog.parts(kWoodRole)) {
    if (!fastener.source.isEmpty() && part.source == fastener.source) {
      return woodFastener(shared, woodKey(part));
    }
  }
  for (const Part& part : shared.catalog.parts(kSlotFastenerRole)) {
    if (!fastener.source.isEmpty() && part.source == fastener.source &&
        (part.config == fastener.config || fastener.config.isEmpty())) {
      return part;
    }
  }
  return std::nullopt;
}

// Whether the fasteners selected are wood screws and nails (a `wood`
// block, or driven at a junction of members).
bool swapsWood(const Shared& shared, const QVector<BuildPart>& targets) {
  if (targets.isEmpty()) {
    return true;
  }
  const std::optional<Part> part = sourcePart(shared, targets.first());
  return part ? part->isWoodFastener() : !str(targets.first().raw, "junction").isEmpty();
}

// What may replace the fasteners selected: wood fasteners for wood ones,
// else the other fasteners that share the first one's slot system.
QVector<Part> swapCandidates(const Shared& shared, const QVector<BuildPart>& targets) {
  if (swapsWood(shared, targets)) {
    return woodFasteners(shared);
  }
  const std::optional<Part> own = targets.isEmpty() ? std::nullopt : sourcePart(shared, targets.first());
  const QStringList systems = own ? own->systemText().split(QStringLiteral(", "), Qt::SkipEmptyParts) : QStringList();
  QVector<Part> candidates;
  for (const Part& part : shared.catalog.parts(kSlotFastenerRole)) {
    if (part.isWoodFastener()) {
      continue;
    }
    const QStringList theirs = part.systemText().split(QStringLiteral(", "), Qt::SkipEmptyParts);
    if (systems.isEmpty() || theirs.isEmpty() ||
        std::any_of(theirs.begin(), theirs.end(), [&systems](const QString& s) { return systems.contains(s); })) {
      candidates.append(part);
    }
  }
  return candidates;
}

std::optional<Part> swapPart(const Shared& shared, const CommandState& state) {
  for (const Part& part : swapCandidates(shared, swapTargets(shared, state.items(kSwapItems)))) {
    if (part.key == state.choice(kPart)) {
      return part;
    }
  }
  return std::nullopt;
}

// The selection read: Fastener stays when it fits, else the fasteners' own
// part (then Size alone changes them).
void followSwapSelection(Shared& shared, CommandState& state) {
  const QVector<BuildPart> targets = swapTargets(shared, state.items(kSwapItems));
  if (swapPart(shared, state)) {
    return;
  }
  const std::optional<Part> own = targets.isEmpty() ? std::nullopt : sourcePart(shared, targets.first());
  const QVector<Part> candidates = swapCandidates(shared, targets);
  state.setChoice(kPart, own ? own->key : candidates.isEmpty() ? QString() : candidates.first().key);
  state.setChoice(kWoodSize, kWoodAuto);
}

PickerRequest swapRequest(const Shared& shared, const CommandState& state) {
  PickerRequest request;
  request.command = kSwapCommand;
  request.title = QObject::tr("Swap Fastener");
  request.parts = pickerParts(swapCandidates(shared, swapTargets(shared, state.items(kSwapItems))));
  request.current = state.choice(kPart);
  return request;
}

CommandDef swapFastenerCommand(const SharedPtr& shared, DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = kSwapCommand;
  def.name = QObject::tr("Swap Fastener");
  def.icon = QStringLiteral("build-fasteners");
  def.tooltip = QObject::tr("Replaces the fasteners selected with another fastener or size, where they are");
  def.tab = kTab;
  def.group = QStringLiteral("CONNECT");
  def.keywords = {QStringLiteral("screw"), QStringLiteral("nail"), QStringLiteral("nut"), QStringLiteral("replace"),
                  QStringLiteral("fastener"), QStringLiteral("build")};
  def.okLabel = QObject::tr("Swap");
  // The view's context menu offers it for fasteners only (mitcad#494).
  def.offeredFor = [&context](const Selection& selection) {
    return !selection.isEmpty() && std::all_of(selection.begin(), selection.end(), [&context](const SelectionItem& item) {
      const std::optional<BuildPart> part = build::partOf(item, context);
      return part && part->role == QStringLiteral("fastener");
    });
  };
  def.inputs = {
      selectionInput(kSwapItems, QObject::tr("Fasteners"), kFastenerKinds, 1, 0)
          .withPickRule([](const CommandState&) { return kFastenerKinds; },
                        [shared](const SelectionItem& item, const CommandState&) {
                          const std::optional<BuildPart> part = shared->partAt(item.occurrence);
                          return part && part->role == QStringLiteral("fastener");
                        })
          .withOnPick([shared](const SelectionItem&, CommandState& state, const CommandContext&) {
            followSwapSelection(*shared, state);
            if (PartPicker* picker = shownPicker(*shared, kSwapCommand)) {
              picker->setParts(swapRequest(*shared, state).parts, state.choice(kPart));
            }
          })
          .withTooltip(QObject::tr("The fasteners to swap: wood screws and nails, or a connector's nuts and screws")),
      choiceInput(kPart, QObject::tr("Fastener"), {}, QString())
          .withChoices([shared](const CommandState& state) {
            Choices choices;
            for (const Part& part : swapCandidates(*shared, swapTargets(*shared, state.items(kSwapItems)))) {
              choices.append({part.key, part.config.isEmpty() || part.isWoodFastener()
                                            ? part.name
                                            : QStringLiteral("%1 (%2)").arg(part.name, part.config)});
            }
            return choices;
          })
          .withChoiceTile([shared](const CommandState& state) {
            const std::optional<Part> part = swapPart(*shared, state);
            if (!part) {
              return ChoiceTile{QImage(), QObject::tr("No fastener"), QString()};
            }
            return ChoiceTile{shared->thumbnail(*part),
                              part->config.isEmpty() ? part->name : QStringLiteral("%1 (%2)").arg(part->name, part->config),
                              part->isWoodFastener() ? QObject::tr("Wood fastener") : part->systemText()};
          })
          .withButton(kButtonText, buttonTip(),
                      [shared](const CommandState& state, const CommandContext&) {
                        if (PartPicker* picker = partPicker(*shared, true)) {
                          picker->ask(swapRequest(*shared, state));
                        }
                      })
          .withOnChange([shared](CommandState& state, const CommandContext&) {
            if (!woodRows(*shared, state.choice(kPart)).contains(state.choice(kWoodSize))) {
              state.setChoice(kWoodSize, kWoodAuto);
            }
            if (PartPicker* picker = shownPicker(*shared, kSwapCommand)) {
              picker->setCurrent(state.choice(kPart));
            }
          })
          .withTooltip(QObject::tr("The fastener they become; a click (or Enter) opens the component picker")),
      choiceInput(kWoodSize, QObject::tr("Size"), {{kWoodAuto, QStringLiteral("Auto: from the members")}}, kWoodAuto)
          .withVisible([shared](const CommandState& state) {
            const std::optional<Part> part = swapPart(*shared, state);
            return part && part->isWoodFastener();
          })
          .withChoices([shared](const CommandState& state) { return woodSizeChoices(*shared, state.choice(kPart)); })
          .withTooltip(QObject::tr("The fastener's size, or Auto: sized from the members each joins")),
  };
  def.enabled = [&context] {
    const QVector<BuildPart> parts = build::buildParts(context);
    return std::any_of(parts.begin(), parts.end(),
                       [](const BuildPart& part) { return part.role == QStringLiteral("fastener"); });
  };
  // The fasteners selected before it (the view's context menu) are its
  // Fasteners, read against the design as it is now.
  def.preselect = [shared](const Selection& selection, CommandState& state, const CommandContext& model) {
    shared->refreshDesign(model);
    Selection items;
    for (const SelectionItem& item : selection) {
      const std::optional<BuildPart> part = shared->partAt(item.occurrence);
      if (part && part->role == QStringLiteral("fastener") && kFastenerKinds.testFlag(item.kind)) {
        items.append(item);
      }
    }
    state.setItems(kSwapItems, items);
  };
  def.init = [shared](CommandState& state, const CommandContext& model) {
    shared->refreshDesign(model);
    shared->catalog.refresh(kWoodRole, model);
    shared->catalog.refresh(kSlotFastenerRole, model);
    followSwapSelection(*shared, state);
    qInfo().noquote() << QStringLiteral("BUILD swap fastener starts with %1 fastener(s), %2")
                             .arg(swapTargets(*shared, state.items(kSwapItems)).size())
                             .arg(state.choice(kPart));
    if (PartPicker* picker = pinnedPicker(*shared)) {
      picker->ask(swapRequest(*shared, state));
    }
  };
  def.ended = pickerEnded(shared, def.id);
  def.inspect = [shared](const CommandState& state, const CommandContext&) {
    Inspection result;
    const QVector<BuildPart> targets = swapTargets(*shared, state.items(kSwapItems));
    if (targets.isEmpty()) {
      result.error = QObject::tr("Select the fasteners to swap: wood screws and nails, or a connector's nuts and screws.");
      return result;
    }
    // What they are now, by kind: "2 × Wood screw 5x80".
    QStringList kinds;
    QHash<QString, int> counts;
    for (const BuildPart& part : targets) {
      const QString shown = part.designation.isEmpty() ? part.shownName() : part.designation;
      if (!counts.contains(shown)) {
        kinds << shown;
      }
      counts[shown] += 1;
    }
    QStringList now;
    for (const QString& kind : kinds) {
      now << QStringLiteral("%1 × %2").arg(counts.value(kind)).arg(kind);
    }
    const std::optional<Part> part = swapPart(*shared, state);
    const bool wood = swapsWood(*shared, targets);
    QString becomes = QObject::tr("no fastener of the libraries fits");
    if (part) {
      const QString size = state.choice(kWoodSize);
      becomes = !part->isWoodFastener() ? (part->config.isEmpty() ? part->name : QStringLiteral("%1 %2").arg(part->name, part->config))
                : size.isEmpty() || size == kWoodAuto ? QObject::tr("%1, sized from the members").arg(part->name)
                                                      : QStringLiteral("%1 %2").arg(part->name, size);
    }
    result.text = QStringLiteral("<b>%1</b><br>%2").arg(QObject::tr("%1 fastener(s)").arg(targets.size()),
                                                        QObject::tr("%1 become %2").arg(now.join(QStringLiteral(", ")), becomes)
                                                            .toHtmlEscaped());
    if (!part) {
      result.error = QObject::tr("No %1 fastener in the libraries to swap them for.")
                         .arg(wood ? QObject::tr("wood") : QObject::tr("slot"));
    }
    result.log = QStringLiteral("swap %1 fastener(s) (%2, %3) for %4")
                     .arg(targets.size())
                     .arg(wood ? QStringLiteral("wood") : QStringLiteral("slot"), now.join(QStringLiteral(", ")), becomes);
    return result;
  };
  def.apply = [shared, &host](CommandState& state, const CommandContext& model) {
    const QVector<BuildPart> targets = swapTargets(*shared, state.items(kSwapItems));
    const std::optional<Part> part = swapPart(*shared, state);
    if (targets.isEmpty() || !part) {
      host.showStatus(QObject::tr("Select the fasteners to swap and the fastener they become."), NoticeLevel::Error);
      return false;
    }
    QJsonObject ref = part->ref();
    if (part->isWoodFastener()) {
      ref.remove(QStringLiteral("config"));
      const QString size = state.choice(kWoodSize);
      if (!size.isEmpty() && size != kWoodAuto) {
        ref.insert(QStringLiteral("config"), size);
      }
    }
    QJsonArray components;
    for (const BuildPart& target : targets) {
      components.append(target.component);
    }
    QJsonObject result;
    if (!host.runModelCommand(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_swap_fasteners")},
                                          {kSwapItems, components},
                                          {QStringLiteral("part"), ref}},
                              &result)) {
      return false;
    }
    // The warnings of the new size (embedment, edges, ends).
    const QStringList warnings = warningsOf(result);
    QStringList sizes;
    for (const QJsonValue& value : result.value(QStringLiteral("sizes")).toArray()) {
      const QString config = str(value.toObject(), "config");
      if (!config.isEmpty() && !sizes.contains(config)) {
        sizes << config;
      }
    }
    qInfo().noquote() << QStringLiteral("Swapped %1 fastener(s) for %2%3%4")
                             .arg(result.value(QStringLiteral("count")).toInt())
                             .arg(str(result, "designation"),
                                  sizes.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(sizes.join(QStringLiteral(", "))),
                                  warnings.isEmpty() ? QString() : QStringLiteral("; ") + warnings.join(QStringLiteral("; ")));
    if (!warnings.isEmpty()) {
      host.showStatus(warnings.join(QLatin1Char(' ')), NoticeLevel::Warning);
    }
    logTimberSeating(model);
    shared->refreshDesign(model);
    shared->catalog.refresh(kWoodRole, model);
    return true;
  };
  return def;
}

// --- MODIFY: a part picked

QString partsText(const QVector<BuildPart>& parts) {
  QStringList names;
  for (const BuildPart& part : parts) {
    names << part.shownName();
  }
  return names.join(QStringLiteral(", "));
}

const Choices kLengthEdits = {{kTyped, QStringLiteral("Detached: this length")},
                              {QStringLiteral("face"), QStringLiteral("Bound: to the face")},
                              {QStringLiteral("centre"), QStringLiteral("Bound: to the center")},
                              {QStringLiteral("through"), QStringLiteral("Bound: through")}};

CommandDef lengthCommand(DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.length");
  def.name = QObject::tr("Length");
  def.icon = QStringLiteral("build-length");
  def.tooltip = QObject::tr("Binds a component's length to the component it runs to, or makes it a number");
  def.tab = kTab;
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("bind"), QStringLiteral("detach"), QStringLiteral("cut"), QStringLiteral("build")};
  const auto typed = [](const CommandState& state) { return state.choice(QStringLiteral("mode")) == kTyped; };
  const auto bound = [](const CommandState& state) { return state.choice(QStringLiteral("mode")) != kTyped; };
  def.inputs = {
      selectionInput(kPart, QObject::tr("Component"), kPartKinds, 1, 1)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            if (const std::optional<BuildPart> part = build::partOf(item, model)) {
              if (std::isfinite(part->length)) {
                state.setText(QStringLiteral("length"), cmd::lengthText(part->length, model));
                state.setValue(QStringLiteral("length"), kNaN);
              }
              const QString mode = str(part->bound, "mode");
              state.setChoice(QStringLiteral("mode"), part->bound.isEmpty() ? kTyped : (mode.isEmpty() ? QStringLiteral("face") : mode));
            }
          },
                      false),
      choiceInput(QStringLiteral("mode"), QObject::tr("Length"), kLengthEdits, kTyped),
      valueInput(QStringLiteral("length"), QObject::tr("Value"), ValueKind::Length, QStringLiteral("500 mm"))
          .withVisible(typed),
      selectionInput(QStringLiteral("target"), QObject::tr("Bound To"), SelectKind::Face, 0, 1)
          .withVisible(bound)
          .withTooltip(QObject::tr("The face the length runs to; none: the one it is bound to now")),
  };
  def.enabled = [&context] { return build::hasBuildParts(context); };
  def.inspect = [](const CommandState& state, const CommandContext& model) {
    Inspection result;
    const QVector<BuildPart> parts = build::partsOf(state.items(kPart), model);
    if (parts.isEmpty()) {
      result.error = QObject::tr("Pick a component of the Build tab.");
      return result;
    }
    const BuildPart& part = parts.first();
    QStringList lines{QStringLiteral("<b>%1</b>").arg(part.shownName().toHtmlEscaped())};
    if (std::isfinite(part.length)) {
      lines << QObject::tr("Length %1 mm").arg(part.length, 0, 'f', 1);
    }
    if (!part.bound.isEmpty()) {
      const QJsonObject to = part.bound.value(QStringLiteral("to")).toObject();
      lines << QObject::tr("Bound (%1) to %2").arg(str(part.bound, "mode"), targetText(to, model).toHtmlEscaped());
      if (const std::optional<SelectionItem> target = build::itemOfTarget(to, model)) {
        result.highlights.append(*target);
      }
    } else {
      lines << QObject::tr("A number (detached)");
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("%1: %2 mm, %3").arg(part.component, QString::number(part.length, 'f', 2),
                                                     part.bound.isEmpty() ? QStringLiteral("detached")
                                                                          : QStringLiteral("bound ") + str(part.bound, "mode"));
    return result;
  };
  def.apply = [&host](CommandState& state, const CommandContext& model) {
    const QVector<BuildPart> parts = build::partsOf(state.items(kPart), model);
    if (parts.isEmpty()) {
      host.showStatus(QObject::tr("Pick a component of the Build tab."), NoticeLevel::Error);
      return false;
    }
    const BuildPart& part = parts.first();
    const QString mode = state.choice(QStringLiteral("mode"));
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("build_edit")}, {QStringLiteral("component"), part.component}};
    if (mode == kTyped) {
      command.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
    } else {
      QJsonObject bind;
      const Selection& targets = state.items(QStringLiteral("target"));
      if (!targets.isEmpty()) {
        const std::optional<QString> path = build::pathFromHost(targets.first(), model);
        if (!path) {
          host.showStatus(QObject::tr("%1 is not in %2, where the Build tab places components.").arg(targets.first().describe(), build::buildHost(model).name), NoticeLevel::Error);
          return false;
        }
        bind.insert(QStringLiteral("geometry"), targets.first().reference());
        if (!path->isEmpty()) {
          bind.insert(QStringLiteral("occurrence"), *path);
        }
      } else {
        bind = part.bound.value(QStringLiteral("to")).toObject();
      }
      if (bind.isEmpty()) {
        host.showStatus(QObject::tr("Pick the face %1's length runs to.").arg(part.shownName()), NoticeLevel::Error);
        return false;
      }
      command.insert(QStringLiteral("length"), QJsonObject{{QStringLiteral("bind"), bind},
                                                           {QStringLiteral("mode"), mode},
                                                           {QStringLiteral("add"), 0}});
    }
    QJsonObject result;
    if (!host.runModelCommand(command, &result)) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("Length of %1: %2").arg(part.shownName(),
                                                               mode == kTyped ? state.expression(QStringLiteral("length"))
                                                                              : QStringLiteral("bound, ") + mode);
    return true;
  };
  return def;
}

// The end of a part a pick is nearer to: its end face's name, else where
// the click was along the part (end A at z = 0 of its component).
QString endOfPick(const SelectionItem& item, const BuildPart& part) {
  if (item.name.endsWith(QStringLiteral(":end_b"))) {
    return QStringLiteral("b");
  }
  if (item.name.endsWith(QStringLiteral(":end_a"))) {
    return QStringLiteral("a");
  }
  if (item.at && std::isfinite(part.length)) {
    return (*item.at)[2] > part.length / 2.0 ? QStringLiteral("b") : QStringLiteral("a");
  }
  return QStringLiteral("a");
}

// The part whose end meets this end (an L or inline junction), and that
// end: for a shared mitre.
std::optional<std::pair<QString, QString>> otherEnd(const QString& component, const QString& end,
                                                    const CommandContext& context) {
  const QString frame = QStringLiteral("end_") + end;
  for (const QJsonValue& value : build::junctions(context)) {
    const QJsonObject junction = value.toObject();
    const QJsonObject a = junction.value(QStringLiteral("a")).toObject();
    const QJsonObject b = junction.value(QStringLiteral("b")).toObject();
    for (const auto& [mine, other] : {std::pair<QJsonObject, QJsonObject>(a, b), std::pair<QJsonObject, QJsonObject>(b, a)}) {
      if (str(mine, "component") == component && str(mine, "at") == frame && str(other, "at").startsWith(QStringLiteral("end_"))) {
        return std::make_pair(str(other, "component"), str(other, "at").mid(4));
      }
    }
  }
  return std::nullopt;
}

CommandDef endsCommand(DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.ends");
  def.name = QObject::tr("Ends");
  def.icon = QStringLiteral("build-ends");
  def.tooltip = QObject::tr("A component's end cut: a miter's angle and the axis it turns about");
  def.tab = kTab;
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("mitre"), QStringLiteral("miter"), QStringLiteral("angle"), QStringLiteral("cut"),
                  QStringLiteral("bore"), QStringLiteral("tap"), QStringLiteral("machining"), QStringLiteral("build")};
  const Choices kMachining = {{QStringLiteral("keep"), QObject::tr("As it is")},
                              {QStringLiteral("bore"), QObject::tr("A bore")},
                              {QStringLiteral("none"), QObject::tr("None")}};
  const auto bore = [](const CommandState& state) { return state.choice(QStringLiteral("machining")) == QStringLiteral("bore"); };
  def.inputs = {
      selectionInput(kPart, QObject::tr("Component"), kPartKinds, 1, 1)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            if (const std::optional<BuildPart> part = build::partOf(item, model)) {
              const QString end = endOfPick(item, *part);
              state.setChoice(QStringLiteral("end"), end);
              const QJsonObject cut = part->ends.value(end).toObject();
              if (!str(cut, "tilt").isEmpty()) {
                state.setChoice(QStringLiteral("tilt"), str(cut, "tilt"));
              }
            }
          },
                      false)
          .withTooltip(QObject::tr("Near the end to cut")),
      choiceInput(QStringLiteral("end"), QObject::tr("End"), kEnds, QStringLiteral("a")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("45 deg"))
          .withTooltip(QObject::tr("From square, 0 up to 90 degrees")),
      choiceInput(QStringLiteral("tilt"), QObject::tr("About"), kTilts, QStringLiteral("x")),
      checkInput(QStringLiteral("shared"), QObject::tr("Also the Component It Meets"), false)
          .withTooltip(QObject::tr("A shared miter: the end of the component that meets this end is cut too")),
      // End machining (build_edit's `machining`): a bore in this end.
      choiceInput(QStringLiteral("machining"), QObject::tr("End Machining"), kMachining, QStringLiteral("keep"))
          .withHeading(QObject::tr("End machining"))
          .withTooltip(QObject::tr("A bore (tapped when a thread is given) in the chosen end; None takes the "
                                   "end machining away")),
      valueInput(QStringLiteral("bore_diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("6.8 mm"))
          .withVisible(bore),
      valueInput(QStringLiteral("bore_depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("15 mm"))
          .withVisible(bore),
      textInput(QStringLiteral("bore_thread"), QObject::tr("Thread"), QStringLiteral("M8"))
          .withVisible(bore)
          .withTooltip(QObject::tr("Empty: a plain bore")),
  };
  def.enabled = [&context] { return build::hasBuildParts(context); };
  def.inspect = [](const CommandState& state, const CommandContext& model) {
    Inspection result;
    const QVector<BuildPart> parts = build::partsOf(state.items(kPart), model);
    if (parts.isEmpty()) {
      result.error = QObject::tr("Pick a component of the Build tab, near the end to cut.");
      return result;
    }
    const BuildPart& part = parts.first();
    QStringList lines{QStringLiteral("<b>%1</b>").arg(part.shownName().toHtmlEscaped()), endsText(part.ends)};
    if (!part.machining.isEmpty()) {
      lines << QObject::tr("Machining: %1").arg(holesText(part.machining).toHtmlEscaped());
    }
    if (state.checked(QStringLiteral("shared"))) {
      const auto other = otherEnd(part.component, state.choice(QStringLiteral("end")), model);
      lines << (other ? QObject::tr("Also %1, end %2").arg(cmd::componentName(other->first, model), other->second.toUpper())
                      : QObject::tr("No component's end meets this end"));
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("%1 end %2: %3").arg(part.component, state.choice(QStringLiteral("end")), endsText(part.ends));
    return result;
  };
  def.apply = [&host](CommandState& state, const CommandContext& model) {
    const QVector<BuildPart> parts = build::partsOf(state.items(kPart), model);
    if (parts.isEmpty()) {
      host.showStatus(QObject::tr("Pick a component of the Build tab."), NoticeLevel::Error);
      return false;
    }
    const BuildPart& part = parts.first();
    const QString end = state.choice(QStringLiteral("end"));
    const QJsonObject cut{{QStringLiteral("angle"), state.expression(QStringLiteral("angle"))},
                          {QStringLiteral("tilt"), state.choice(QStringLiteral("tilt"))}};
    QJsonObject edit{{QStringLiteral("cmd"), QStringLiteral("build_edit")},
                     {QStringLiteral("component"), part.component},
                     {QStringLiteral("ends"), QJsonObject{{end, cut}}}};
    // `machining` is the part's whole list: the other end's entries stay,
    // this end's are replaced (a bore) or left out (none).
    const QString machining = state.choice(QStringLiteral("machining"));
    QJsonArray added;
    if (machining != QStringLiteral("keep")) {
      QJsonArray list;
      for (const QJsonValue& value : part.machining) {
        if (str(value.toObject(), "end") != end) {
          list.append(value);
        }
      }
      if (machining == QStringLiteral("bore")) {
        QJsonObject hole{{QStringLiteral("diameter"), state.expression(QStringLiteral("bore_diameter"))},
                         {QStringLiteral("depth"), state.expression(QStringLiteral("bore_depth"))}};
        if (!state.text(QStringLiteral("bore_thread")).trimmed().isEmpty()) {
          hole.insert(QStringLiteral("thread"), state.text(QStringLiteral("bore_thread")).trimmed());
        }
        const QJsonObject entry{{QStringLiteral("end"), end}, {QStringLiteral("hole"), hole}};
        list.append(entry);
        added.append(entry);
      }
      edit.insert(QStringLiteral("machining"), list);
    }
    QJsonArray commands{edit};
    if (state.checked(QStringLiteral("shared"))) {
      const auto other = otherEnd(part.component, end, model);
      if (!other) {
        host.showStatus(QObject::tr("No component's end meets end %1 of %2.").arg(end.toUpper(), part.shownName()), NoticeLevel::Error);
        return false;
      }
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("build_edit")},
                                  {QStringLiteral("component"), other->first},
                                  {QStringLiteral("ends"), QJsonObject{{other->second, cut}}}});
    }
    if (!host.runModelCommands(commands, QObject::tr("Ends of %1").arg(part.shownName()))) {
      return false;
    }
    qInfo().noquote() << QStringLiteral("Ends of %1: end %2 %3 about %4%5%6")
                             .arg(part.shownName(), end, state.expression(QStringLiteral("angle")),
                                  state.choice(QStringLiteral("tilt")),
                                  state.checked(QStringLiteral("shared")) ? QStringLiteral(", shared") : QString(),
                                  machining == QStringLiteral("keep")
                                      ? QString()
                                      : QStringLiteral(", machining ") +
                                            (machining == QStringLiteral("none")
                                                 ? QStringLiteral("none at end ") + end
                                                 : holesText(added)));
    return true;
  };
  return def;
}

// Flip and Rotate 90 deg: build_edit of each part picked, one undo step.
CommandDef turnCommand(DocumentHost& host, const CommandContext& context, bool rotate) {
  CommandDef def;
  def.id = rotate ? QStringLiteral("build.rotate") : QStringLiteral("build.flip");
  def.name = rotate ? QObject::tr("Rotate 90°") : QObject::tr("Flip");
  def.icon = rotate ? QStringLiteral("build-rotate") : QStringLiteral("flip");
  def.tooltip = rotate ? QObject::tr("Turns components a quarter turn about their axes; Enter turns them again")
                       : QObject::tr("Turns components end for end: the other end first");
  def.tab = kTab;
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = rotate ? QStringList{QStringLiteral("turn"), QStringLiteral("90"), QStringLiteral("build")}
                        : QStringList{QStringLiteral("reverse"), QStringLiteral("other end"), QStringLiteral("build")};
  def.okLabel = rotate ? QObject::tr("Rotate") : QObject::tr("Flip");
  // Rotate turns again on each Enter.
  def.repeats = rotate;
  def.inputs = {selectionInput(QStringLiteral("parts"), QObject::tr("Components"), kPartKinds, 1, 0)};
  def.enabled = [&context] { return build::hasBuildParts(context); };
  def.inspect = [rotate](const CommandState& state, const CommandContext& model) {
    Inspection result;
    const QVector<BuildPart> parts = build::partsOf(state.items(QStringLiteral("parts")), model);
    if (parts.isEmpty()) {
      result.error = QObject::tr("Pick components of the Build tab.");
      return result;
    }
    QStringList lines;
    for (const BuildPart& part : parts) {
      const QString line = rotate ? QObject::tr("%1: %2°, then %3°")
                                        .arg(part.shownName())
                                        .arg(part.rotate)
                                        .arg((part.rotate + 90) % 360)
                                  : QObject::tr("%1: %2").arg(part.shownName(), part.flip ? QObject::tr("flipped, then as placed")
                                                                                          : QObject::tr("the other end first"));
      lines << line.toHtmlEscaped();
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("%1 part(s): %2").arg(parts.size()).arg(partsText(parts));
    return result;
  };
  def.apply = [&host, rotate](CommandState& state, const CommandContext& model) {
    const QVector<BuildPart> parts = build::partsOf(state.items(QStringLiteral("parts")), model);
    if (parts.isEmpty()) {
      host.showStatus(QObject::tr("Pick components of the Build tab."), NoticeLevel::Error);
      return false;
    }
    QJsonArray commands;
    for (const BuildPart& part : parts) {
      QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("build_edit")}, {QStringLiteral("component"), part.component}};
      if (rotate) {
        command.insert(QStringLiteral("rotate"), angleValue(QString::number((part.rotate + 90) % 360)));
      } else {
        command.insert(QStringLiteral("flip"), !part.flip);
      }
      commands.append(command);
    }
    const QString label = rotate ? QObject::tr("Rotate %1 90°").arg(partsText(parts))
                                 : QObject::tr("Flip %1").arg(partsText(parts));
    if (!host.runModelCommands(commands, label)) {
      return false;
    }
    qInfo().noquote() << label;
    return true;
  };
  return def;
}

// --- OUTPUT

CommandDef listCommand(const CommandContext& context, QWidget* window, std::function<QString()> design, bool cut) {
  CommandDef def;
  def.id = cut ? QStringLiteral("build.cut_list") : QStringLiteral("build.parts_list");
  def.name = cut ? QObject::tr("Cut List") : QObject::tr("Bill of Materials");
  def.icon = cut ? QStringLiteral("build-cut-list") : QStringLiteral("parts-list");
  def.tooltip = cut ? QObject::tr("The components to cut, with their lengths, end angles and stock")
                    : QObject::tr("Every component with its quantity, supplier and part number, as CSV or PDF");
  def.tab = kTab;
  def.group = QStringLiteral("OUTPUT");
  def.pinned = true;
  def.keywords = cut ? QStringList{QStringLiteral("saw"), QStringLiteral("lengths"), QStringLiteral("csv"),
                                   QStringLiteral("pdf"), QStringLiteral("build")}
                     : QStringList{QStringLiteral("parts list"), QStringLiteral("bom"), QStringLiteral("csv"),
                                   QStringLiteral("pdf"), QStringLiteral("build")};
  def.kind = CommandDef::Kind::Action;
  def.run = [&context, window, design, cut] {
    build::BuildListsDialog dialog(context, design(),
                                   cut ? build::BuildListsDialog::Page::CutList : build::BuildListsDialog::Page::PartsList,
                                   window);
    prepareModal(&dialog);
    execDialog(&dialog);
  };
  if (cut) {
    def.enabled = [&context] { return build::hasBuildParts(context); };
  }
  return def;
}

} // namespace

void registerBuildCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host,
                           QWidget* window, std::function<QString()> designName) {
  const auto shared = std::make_shared<Shared>();
  // The part picker's window and the host its choices go through (mitcad#418).
  shared->window = window;
  shared->host = &host;
  // PROFILE
  registry.add(placeCommand(shared, host, QStringLiteral("profile")));
  registry.add(panelCommand(shared, host));
  registry.add(placeCommand(shared, host, QStringLiteral("linear")));
  // TIMBER (mitcad#520): loose members, walls, floors and roofs.
  registry.add(timberCommand(shared, host)); // mitcad#478
  registry.add(wallCommand(shared, host));
  registry.add(floorRoofCommand(shared, host));
  // CONNECT
  registry.add(connectorCommand(shared, host, context));
  registry.add(fastenersCommand(shared, host, context));
  registry.add(swapFastenerCommand(shared, host, context)); // mitcad#497
  // MODIFY
  registry.add(lengthCommand(host, context));
  registry.add(endsCommand(host, context));
  registry.add(turnCommand(host, context, false));
  registry.add(turnCommand(host, context, true));
  registry.add(spacingCommand(shared, host, context)); // mitcad#478
  registry.add(frameCommand(shared, host, context));   // mitcad#479
  // OUTPUT
  registry.add(listCommand(context, window, designName, true));
  registry.add(listCommand(context, window, designName, false));
}

} // namespace mitcad
