// SPDX-License-Identifier: MIT
// The view's context menu outside sketches and commands (mitcad#494;
// app/COMMANDS.md, "The view's context menu"): the commands that fit the
// selection in submenus by kind (Solid, BUILD, Construct), in each first
// those the selection alone completes and the others under More; Delete
// for whole things (a BUILD member, its spacing set, a body, a component,
// the feature that made a face) and Hide (mitcad#347).
#include "MainWindow.hpp"

#include <algorithm>
#include <functional>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QAction>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QPair>
#include <QtLogging>

#include "browser/Bindings.hpp"
#include "browser/BrowserController.hpp"
#include "build/BuildModel.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Icons.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {
namespace {

// The submenus, in the menu's order; Inspect's commands (Measure) are
// entries of the menu itself. The Make tab's (mitcad#1049: Send to Slicer,
// Print Analysis, the annotations) a submenu of their own.
enum class Kind { Solid, Build, Construct, Inspect, Make };

Kind kindOf(const CommandDef& def) {
  if (def.tab == QStringLiteral("BUILD")) {
    return Kind::Build;
  }
  if (def.tab == QStringLiteral("MAKE")) {
    return Kind::Make;
  }
  if (def.group == QStringLiteral("CONSTRUCT")) {
    return Kind::Construct;
  }
  if (def.group == QStringLiteral("INSPECT")) {
    return Kind::Inspect;
  }
  return Kind::Solid;
}

bool onBody(const SelectionItem& item) {
  return item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex ||
         item.kind == SelectKind::Body;
}

// What Property Sets shows for a picked item (mitcad#496): a body (also of
// a face, edge or vertex) with its occurrence, or a component with its
// occurrence; nothing for other items.
PropertyTarget propertyTarget(const SelectionItem& item) {
  if (onBody(item)) {
    return {item.owner, item.occurrence, QString()};
  }
  if (item.kind == SelectKind::Component) {
    return {QString(), item.occurrence, item.occurrence.isEmpty() ? item.owner : QString()};
  }
  return {};
}

// A plane to sketch on: a datum plane or a planar face.
bool isSketchPlane(const SelectionItem& item) {
  return item.kind == SelectKind::Plane ||
         (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
}

// What the menu's Look At looks at for an item (mitcad#1212): a datum
// plane, a planar face, a sketch or its profile, or the sketch of a
// sketch curve or point; nothing for any other face, an edge or the rest.
std::optional<SelectionItem> lookAtTarget(const SelectionItem& item) {
  if (isSketchPlane(item) || item.kind == SelectKind::Sketch || item.kind == SelectKind::Profile) {
    return item;
  }
  if (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) {
    SelectionItem sketch = item;
    sketch.kind = SelectKind::Sketch;
    sketch.name.clear();
    sketch.geometry.clear();
    sketch.at.reset();
    return sketch;
  }
  return std::nullopt;
}

// A submenu whose entries are logged as "Context menu <title>: A | B".
class Submenu {
public:
  Submenu(const QString& title, QMenu* parent) : m_menu(new QMenu(title, parent)) {}
  QMenu* menu() const { return m_menu; }
  bool isEmpty() const { return m_entries.isEmpty(); }
  void add(QAction* action) {
    m_menu->addAction(action);
    m_entries << action->text().remove(QLatin1Char('&'));
  }
  void addCall(const QString& text, std::function<void()> call) {
    QAction* action = m_menu->addAction(text);
    QObject::connect(action, &QAction::triggered, m_menu, std::move(call));
    m_entries << text;
  }
  void addMenu(QMenu* menu) {
    m_menu->addMenu(menu);
    m_entries << menu->title();
  }
  void log(const QString& name) const {
    qDebug().noquote() << QStringLiteral("Context menu %1: %2").arg(name, m_entries.join(QStringLiteral(" | ")));
  }

private:
  QMenu* m_menu;
  QStringList m_entries;
};

} // namespace

bool MainWindow::isPrintBedFace(const SelectionItem& face, const QStringList& bedFaces,
                                const std::shared_ptr<geometry::Shape>& shape) {
  if (bedFaces.contains(face.name)) {
    return true;
  }
  if (!shape) {
    return false;
  }
  const std::vector<int> picked = shape->find_faces(face.name.toStdString());
  for (const QString& name : bedFaces) {
    for (const int index : shape->find_faces(name.toStdString())) {
      if (std::find(picked.begin(), picked.end(), index) != picked.end()) {
        return true;
      }
    }
  }
  return false;
}

void MainWindow::fillModelContextMenu(QMenu& menu, QStringList& entries, const SelectionItem& picked) {
  const auto add = [&menu, &entries](QAction* action) {
    if (menu.actions().contains(action)) {
      return; // offered already
    }
    menu.addAction(action);
    entries << action->text().remove(QLatin1Char('&'));
  };
  const auto addCall = [&menu, &add](const QString& icon, const QString& text, std::function<void()> call) {
    QAction* action = new QAction(themeIcon(icon), text, &menu);
    connect(action, &QAction::triggered, &menu, std::move(call));
    add(action);
  };
  const auto addSubmenu = [&menu, &entries](const Submenu& submenu) {
    menu.addMenu(submenu.menu());
    entries << submenu.menu()->title();
  };

  if (const CommandDef* last = m_registry->find(m_lastCommand); last && isAvailable(*last)) {
    const QString id = last->id;
    addCall(last->icon, tr("Repeat %1").arg(last->name), [this, id] { trigger(id); });
    menu.addSeparator();
  }
  if (m_selection.isEmpty()) {
    add(m_registry->action(QStringLiteral("edit.undo")));
    return;
  }
  const Selection selection = m_selection;
  const SelectionItem& first = selection.first();

  // The BUILD parts the items belong to: the BUILD submenu is only theirs.
  QVector<build::BuildPart> parts;
  bool allParts = true;
  for (const SelectionItem& item : selection) {
    const std::optional<build::BuildPart> part = onBody(item) ? build::partOf(item, model()) : std::nullopt;
    allParts = allParts && part.has_value();
    if (part && std::none_of(parts.begin(), parts.end(),
                             [&part](const build::BuildPart& other) { return other.component == part->component; })) {
      parts.append(*part);
    }
  }
  const bool surfaces = std::all_of(selection.begin(), selection.end(), [this](const SelectionItem& item) {
    const std::shared_ptr<geometry::Shape> shape = onBody(item) ? bodyShape(item.owner) : nullptr;
    return shape && geometry::body_kind(shape->occt()) == geometry::BodyKind::Sheet;
  });

  // The feature commands whose first selection input takes everything
  // selected, by kind, in the registry's (the ribbon's) order.
  QVector<QPair<const CommandDef*, bool>> fitting[5];
  // Those the menu offers at its top level (mitcad#735: Move/Copy).
  QVector<const CommandDef*> atTop;
  for (const auto& def : m_registry->commands()) {
    if (def->kind == CommandDef::Kind::Feature && def->offeredAtTop && isAvailable(*def) &&
        def->offeredAtTop(selection)) {
      atTop.append(def.get());
      continue;
    }
    const InputDef* input = def->primarySelection();
    if (def->kind != CommandDef::Kind::Feature || input == nullptr || !isAvailable(*def) ||
        !std::all_of(selection.begin(), selection.end(),
                     [input](const SelectionItem& s) { return input->takes(s); }) ||
        (def->surfaceBodiesOnly && !surfaces) || (def->offeredFor && !def->offeredFor(selection))) {
      continue;
    }
    const Kind kind = kindOf(*def);
    if (kind == Kind::Build && !allParts) {
      continue;
    }
    fitting[static_cast<int>(kind)].append({def.get(), def->completedBy(selection)});
  }
  // `key` names the submenu in the log, "Context menu Solid: A | More" and
  // "Context menu Solid More: B | C".
  const auto addKind = [this, &menu, &addSubmenu, &fitting](Kind kind, const QString& title, const QString& key) {
    const QVector<QPair<const CommandDef*, bool>>& commands = fitting[static_cast<int>(kind)];
    if (commands.isEmpty()) {
      return;
    }
    Submenu submenu(title, &menu);
    Submenu more(tr("More"), submenu.menu());
    for (const QPair<const CommandDef*, bool>& command : commands) {
      (command.second ? submenu : more).add(m_registry->action(command.first->id));
    }
    if (!more.isEmpty()) {
      if (!submenu.isEmpty()) {
        submenu.menu()->addSeparator();
      }
      submenu.addMenu(more.menu());
    }
    addSubmenu(submenu);
    submenu.log(key);
    if (!more.isEmpty()) {
      more.log(key + QStringLiteral(" More"));
    }
  };
  addKind(Kind::Solid, tr("Solid"), QStringLiteral("Solid"));
  addKind(Kind::Build, tr("Build"), QStringLiteral("Build"));
  addKind(Kind::Construct, tr("Construct"), QStringLiteral("Construct"));
  addKind(Kind::Make, tr("Make"), QStringLiteral("Make"));
  menu.addSeparator();

  if (selection.size() == 1 && isSketchPlane(first)) {
    add(m_registry->action(QStringLiteral("sketch.create")));
  }
  const QString feature = first.creatingFeature();
  QString name;
  if (selection.size() == 1 && !feature.isEmpty() && editorOf(feature, &name) != nullptr) {
    addCall(QStringLiteral("edit"), tr("Edit %1").arg(name), [this, feature] { editFeature(feature); });
  }
  // Move/Copy for bodies and components (mitcad#735), started with the
  // selection as its own action is.
  for (const CommandDef* command : atTop) {
    add(m_registry->action(command->id));
  }
  const QString sketch = sketchOf(first);
  if (selection.size() == 1 && !sketch.isEmpty()) {
    addCall(QStringLiteral("sketch"), tr("Edit Sketch"), [this, sketch] { editSketch(sketch); });
    // Another plane or face, an offset and a rotation on it (mitcad#787).
    addCall(QStringLiteral("offset-plane"), tr("Redefine Sketch Plane"), [this, sketch] {
      startCommand(QStringLiteral("sketch.redefine_plane"),
                   {SelectionItem{SelectKind::Sketch, sketch, QString(), QString()}}, nullptr);
    });
    addCall(QStringLiteral("export"), tr("Export DXF…"), [this, sketch] { exportSketch(sketch); });
  }
  // A planar face's outline for cutting (mitcad#1049): the Make tab's
  // Export Face as DXF…, here too.
  if (selection.size() == 1 && first.kind == SelectKind::Face && first.geometry == QStringLiteral("plane")) {
    add(m_registry->action(QStringLiteral("make.export_face_dxf")));
  }
  // Look At the right-clicked plane, planar face (also of an occurrence, a
  // linked or an imported model) or sketch, the menu's target, near Measure
  // (mitcad#1212); or the one item selected when the click was on none.
  if (const std::optional<SelectionItem> target = lookAtTarget(picked.isValid() ? picked : first);
      target && (picked.isValid() || selection.size() == 1)) {
    const SelectionItem item = *target;
    addCall(QStringLiteral("look-at"), tr("Look At"), [this, item] { lookAt(item); });
  }
  for (const QPair<const CommandDef*, bool>& command : fitting[static_cast<int>(Kind::Inspect)]) {
    add(m_registry->action(command.first->id));
  }
  // Appearances of the bodies selected or picked on (mitcad#46): Appearance…
  // starts the Appearance tool with them (mitcad#1059).
  if (const Selection targets = appearanceTargets(); !targets.isEmpty()) {
    // The bodies' material (mitcad#640): a face stands for its body.
    QStringList bodies;
    // An fx mark on an entry while a target follows an expression
    // (mitcad#861): a face's own appearance, a body's appearance or
    // material. The tools' fx shows and edits it (mitcad#1059).
    bool appearanceBound = false;
    bool materialBound = false;
    for (const SelectionItem& target : targets) {
      if (!bodies.contains(target.owner)) {
        bodies << target.owner;
      }
      const DocumentSnapshot::Body bodyState = m_snapshot.bodies.value(target.owner);
      appearanceBound = appearanceBound || (target.kind == SelectKind::Face
                                                ? bodyState.faceAppearanceExprs.contains(target.name)
                                                : !bodyState.appearanceExpr.isEmpty());
      materialBound = materialBound || !bodyState.materialExpr.isEmpty();
    }
    QStringList bound;
    const auto addBound = [&menu, &entries, &bound](const QString& icon, const QString& text,
                                                    std::function<void()> call, bool fx) {
      QAction* action = new QAction(fx ? boundIcon(themeIcon(icon)) : themeIcon(icon),
                                    fx ? text + QStringLiteral("\tfx") : text, &menu);
      connect(action, &QAction::triggered, &menu, std::move(call));
      menu.addAction(action);
      entries << text;
      if (fx) {
        bound << text;
      }
    };
    addBound(QStringLiteral("appearance"), tr("Appearance…"), [this, targets] { m_controller->openAppearances(targets); },
             appearanceBound);
    // Material ▸ (mitcad#1046): the design's materials, the library ones
    // assigned last, More… (the Material tool, mitcad#1059).
    menu.addMenu(m_controller->materialMenu(&menu, bodies, QString(), materialBound));
    entries << tr("Material");
    if (materialBound) {
      bound << tr("Material");
    }
    if (!bound.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Context menu fx: %1").arg(bound.join(QStringLiteral(" | ")));
    }
    // Shown while a condition holds (mitcad#809).
    if (bodies.size() == 1) {
      const QString body = bodies.first();
      addCall(QStringLiteral("eye"), tr("Show When…"), [this, body] { m_controller->showBodyWhen(body); });
    }
  } else if (selection.size() == 1 && first.kind == SelectKind::Component) {
    // A component's default material (mitcad#640), in Material ▸
    // (mitcad#1046).
    menu.addMenu(m_controller->materialMenu(&menu, {}, first.owner));
    entries << tr("Material");
  }
  // The properties of the first item's body, occurrence and component
  // (mitcad#496).
  if (const PropertyTarget target = propertyTarget(first); !target.isEmpty()) {
    addCall(QStringLiteral("parameters"), tr("Property Sets…"),
            [this, target] { m_controller->openPropertySets(target); });
  }
  // The face that lies on the print bed (mitcad#731): a planar face of a
  // solid sets it, the body's own clears it. While the menu is open, the
  // body's bed face elsewhere shows in the bed's colour. A surface body is
  // not printed: a face it keeps (it became a surface body, or a file
  // stored one) is only cleared, from any of its faces (mitcad#737).
  if (selection.size() == 1 && first.kind == SelectKind::Face && !windowReadOnly() &&
      (!surfaces || !m_snapshot.bodies.value(first.owner).printBedFace.isEmpty())) {
    const DocumentSnapshot::Body body = m_snapshot.bodies.value(first.owner);
    const QString uid = first.owner;
    const auto setBedFace = [this, uid](const QString& face) {
      const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_print_bed_face")},
                                {QStringLiteral("uid"), uid},
                                {QStringLiteral("face"), face.isEmpty() ? QJsonValue() : QJsonValue(face)}};
      if (runModelCommand(command)) {
        qInfo().noquote() << QStringLiteral("Print bed face of %1: %2")
                                 .arg(bodyName(uid), face.isEmpty() ? QStringLiteral("none") : face);
      }
    };
    // A stored face that is gone or not planar, or a surface body's, is
    // cleared from any face of the body; on a solid a planar face can still
    // take its place.
    const bool broken = !body.printBedFace.isEmpty() && (surfaces || !body.printBedError.isEmpty());
    const bool onBed = !broken && isPrintBedFace(first, body.printBedFaces, bodyShape(uid));
    if (!surfaces && !onBed && first.geometry == QStringLiteral("plane")) {
      const QString face = first.name;
      addCall(QStringLiteral("print-3d"), tr("Set as Print Bed Face"), [setBedFace, face] { setBedFace(face); });
    }
    if (onBed || broken) {
      addCall(QStringLiteral("print-3d"), tr("Clear Print Bed Face"), [setBedFace] { setBedFace(QString()); });
    }
    if (!surfaces && !onBed && body.printBedError.isEmpty() && !body.printBedFaces.isEmpty()) {
      m_printBedShown = SelectionItem{SelectKind::Face, uid, body.printBedFaces.first(), QStringLiteral("plane"),
                                      first.occurrence};
      // Only the highlights change; the card stays as the selection made
      // it (mitcad#981: nothing of it computed twice per right click).
      if (m_mode == Mode::Idle) {
        updateIdleHighlights();
      } else {
        updateIdleView();
      }
      qDebug().noquote() << QStringLiteral("Print bed face shown: %1/%2").arg(uid, m_printBedShown.name);
    }
  }
  // A filament change at a construction or origin plane, or at a flat face
  // of a printed body (mitcad#1643): Send to Slicer opens with a change
  // row at it.
  if (selection.size() == 1 && !windowReadOnly() &&
      (first.kind == SelectKind::Plane ||
       (first.kind == SelectKind::Face && first.geometry == QStringLiteral("plane") && !surfaces))) {
    const SelectionItem item = first;
    addCall(QStringLiteral("print-3d"), tr("Filament Change Here…"), [this, item] { filamentChangeHere(item); });
  }
  // Why a planar face of a surface body offers no print bed face
  // (mitcad#1182: a body a feature left open).
  if (selection.size() == 1 && first.kind == SelectKind::Face && surfaces &&
      first.geometry == QStringLiteral("plane") && m_snapshot.bodies.value(first.owner).printBedFace.isEmpty()) {
    const QString hint =
        tr("%1 is a surface body, not a closed solid: it is not printed and has no print bed face")
            .arg(bodyName(first.owner));
    showHint(hint);
    // (Hints are not logged; this one is, for the UI tests.)
    qInfo().noquote() << QStringLiteral("Print bed face hint: %1").arg(hint);
  }

  // Delete: whole things, each shown when the selection has it, each one
  // undo step. A BUILD part goes as a member (with what it added), never
  // by the features that make it.
  const QStringList bodies = BrowserController::hideableBodies(selection);
  const QStringList occurrences = BrowserController::hideableOccurrences(selection);
  if (!windowReadOnly()) {
    Submenu remove(tr("Delete"), &menu);
    if (!parts.isEmpty()) {
      QStringList members;
      QStringList sets;
      for (const build::BuildPart& part : parts) {
        members << part.component;
        const QString lead = part.set.isEmpty() && !part.spacing.isEmpty() ? part.component : part.set;
        if (!lead.isEmpty() && !sets.contains(lead)) {
          sets << lead;
        }
      }
      remove.addCall(members.size() == 1 ? tr("Delete Member") : tr("Delete %1 Members").arg(members.size()),
                     [this, members] { deleteBuildParts(members, false); });
      if (!sets.isEmpty()) {
        remove.addCall(sets.size() == 1 ? tr("Delete Spacing Set") : tr("Delete %1 Spacing Sets").arg(sets.size()),
                       [this, sets] { deleteBuildParts(sets, true); });
      }
    }
    if (!bodies.isEmpty()) {
      remove.addCall(bodies.size() == 1 ? tr("Delete Body") : tr("Delete %1 Bodies").arg(bodies.size()),
                     [this, selection] { m_controller->deleteBodies(selection); });
    }
    if (!occurrences.isEmpty()) {
      remove.addCall(occurrences.size() == 1 ? tr("Delete Component")
                                             : tr("Delete %1 Components").arg(occurrences.size()),
                     [this, selection] { m_controller->deleteOccurrences(selection); });
    }
    // The feature that made the items, when one did and it is in the
    // timeline (a face, an edge, a body; mitcad#28's feature).
    QStringList features;
    for (const SelectionItem& item : selection) {
      const QString uid = item.kind == SelectKind::Feature ? item.owner : item.creatingFeature();
      if (m_snapshot.feature(uid) != nullptr && !features.contains(uid)) {
        features << uid;
      }
    }
    if (parts.isEmpty() && !features.isEmpty()) {
      remove.addCall(features.size() == 1 ? tr("Delete Feature") : tr("Delete %1 Features").arg(features.size()),
                     [this, features] { m_controller->deleteFeatures(features); });
    }
    if (!remove.isEmpty()) {
      addSubmenu(remove);
      remove.log(QStringLiteral("Delete"));
    }
  }
  // Hide Body and Hide Component: the bodies of the faces, edges, vertices
  // and bodies selected, and the occurrences that place them (mitcad#347).
  if (!bodies.isEmpty()) {
    Submenu hide(tr("Hide"), &menu);
    hide.addCall(bodies.size() == 1 ? tr("Hide Body") : tr("Hide %1 Bodies").arg(bodies.size()),
                 [this, selection] { m_controller->hideBodies(selection); });
    if (!occurrences.isEmpty()) {
      hide.addCall(occurrences.size() == 1 ? tr("Hide Component") : tr("Hide %1 Components").arg(occurrences.size()),
                   [this, selection] { m_controller->hideOccurrences(selection); });
    }
    addSubmenu(hide);
    hide.log(QStringLiteral("Hide"));
  }
  // Transparent (mitcad#1093): the bodies of the faces, edges, vertices and
  // bodies selected and the components selected, checked while all of
  // them are transparent.
  if (BrowserController::offersTransparent(selection)) {
    const bool all = m_controller->allTransparent(selection);
    QAction* action = new QAction(tr("Transparent"), &menu);
    action->setCheckable(true);
    action->setChecked(all);
    connect(action, &QAction::triggered, &menu,
            [this, selection, all] { m_controller->setTransparent(selection, !all); });
    add(action);
    if (all) {
      qDebug().noquote() << "Context menu checked: Transparent";
    }
  }
  // A feature selected as its faces (mitcad#28) has no row of its own.
  if (selection.size() == 1 && first.kind != SelectKind::Feature && (onBody(first) || !feature.isEmpty())) {
    addCall(QStringLiteral("browser"), tr("Find in Browser"), [this, first] { m_controller->findInBrowser(first); });
  }
  // A linked design's occurrence, or a body of it (mitcad#1416): opened in
  // a tab of its own, its file read again now, or every linked design's.
  if (selection.size() == 1 && (first.kind == SelectKind::Component || onBody(first))) {
    const QString component =
        first.kind == SelectKind::Component ? first.owner : m_snapshot.bodies.value(first.owner).component;
    for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
      if (!component.isEmpty() && c.uid == component && c.linked && !c.library) {
        addCall(QStringLiteral("open"), tr("Open Linked Design"), [this, component] { openLinkedDesign(component); });
        if (!windowReadOnly()) {
          addCall(QStringLiteral("update"), tr("Update Link"), [this, component] { updateLink(component); });
          addCall(QStringLiteral("update"), tr("Update All Links"), [this] { updateLink(QString()); });
        }
        break;
      }
    }
  }
  // Save As Mesh (mitcad#734): the bodies of the bodies, faces, edges,
  // vertices and components selected to mesh files; disabled with the
  // reason when none is a solid or mesh body.
  if (std::all_of(selection.begin(), selection.end(),
                  [](const SelectionItem& item) { return onBody(item) || item.kind == SelectKind::Component; })) {
    const QString meshBlocked = saveAsMeshBlocked(selection);
    QAction* meshAction = new QAction(themeIcon(QStringLiteral("export")), tr("Save as Mesh…"), &menu);
    connect(meshAction, &QAction::triggered, &menu, [this, selection] { saveAsMesh(selection); });
    if (!meshBlocked.isEmpty()) {
      meshAction->setEnabled(false);
      meshAction->setToolTip(meshBlocked);
      menu.setToolTipsVisible(true);
      qDebug().noquote() << QStringLiteral("Save As Mesh disabled: %1").arg(meshBlocked);
    }
    add(meshAction);
  }
  menu.addSeparator();
  addCall(QString(), tr("Clear Selection"), [this] { clearSelection(); });
  add(m_registry->action(QStringLiteral("edit.undo")));
}

void MainWindow::deleteBuildParts(const QStringList& components, bool sets) {
  if (components.isEmpty()) {
    return;
  }
  if (!canChangeModel()) {
    const QString readOnly = readOnlyReason();
    showStatus(readOnly.isEmpty() ? changeBlockedReason() : readOnly, NoticeLevel::Error);
    return;
  }
  QJsonArray commands;
  QStringList names;
  const QVector<build::BuildPart> parts = build::buildParts(model());
  for (const QString& component : components) {
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("build_remove")},
                        {QStringLiteral("component"), component}};
    if (sets) {
      command.insert(QStringLiteral("set"), true);
    }
    commands.append(command);
    const auto part = std::find_if(parts.begin(), parts.end(),
                                   [&component](const build::BuildPart& p) { return p.component == component; });
    names << (part != parts.end() ? part->shownName() : component);
  }
  const QString label = sets ? tr("Delete Spacing Sets") : tr("Delete Members");
  if (runModelCommands(commands, label)) {
    qDebug().noquote() << QStringLiteral("Deleted %1 %2")
                              .arg(sets ? QStringLiteral("spacing set of") : QStringLiteral("member"),
                                   names.join(QStringLiteral(", ")));
  }
}

} // namespace mitcad
