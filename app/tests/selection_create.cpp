// SPDX-License-Identifier: MIT
// The selection card's Create row (mitcad#619): kinds of selection to the
// buttons (at most four, Extrude first, Loft only with two or more
// profiles, none for an empty or mixed selection), whether a command can
// use the whole selection (its first selection input), and the row's
// buttons, changed in place.
#include "framework/SelectionCreate.hpp"

#include <algorithm>
#include <cstdio>

#include <QLabel>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QToolButton>
#include <QVariant>
#include <QVector>

#include "framework/Command.hpp"
#include "framework/MacToolButtonStyle.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "selection_create.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int selectionCreateTests() {
  using namespace mitcad;
  const SelectionItem profileA{SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r1"), QStringLiteral("plane")};
  const SelectionItem profileB{SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r2"), QStringLiteral("plane")};
  const SelectionItem top{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:end(r1)"), QStringLiteral("plane")};
  const SelectionItem bottom{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:start(r1)"),
                             QStringLiteral("plane")};
  const SelectionItem side{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:side(c1)"),
                           QStringLiteral("cylinder")};
  const SelectionItem cone{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:side(c2)"),
                           QStringLiteral("cone")};
  const SelectionItem mesh{SelectKind::Face, QStringLiteral("F5.b0"), QStringLiteral("F5:mesh(0)"),
                           QStringLiteral("mesh")};
  const SelectionItem rim{SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{a|b}"), QStringLiteral("circle")};
  const SelectionItem straight{SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{c|d}"),
                               QStringLiteral("line")};
  const SelectionItem curve{SelectKind::SketchCurve, QStringLiteral("F1"), QStringLiteral("c1"), QStringLiteral("line")};
  const SelectionItem point{SelectKind::SketchPoint, QStringLiteral("F1"), QStringLiteral("p1"), QString()};
  const SelectionItem body{SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()};
  const SelectionItem other{SelectKind::Body, QStringLiteral("F3.b0"), QString(), QString()};
  const SelectionItem plane{SelectKind::Plane, QStringLiteral("xy"), QString(), QString()};

  // Kinds of selection: every item of one kind, else none.
  CHECK(selectionCreateKind({}) == SelectionCreateKind::None);
  CHECK(selectionCreateKind({profileA}) == SelectionCreateKind::Profiles);
  CHECK(selectionCreateKind({profileA, profileB}) == SelectionCreateKind::Profiles);
  CHECK(selectionCreateKind({top}) == SelectionCreateKind::PlanarFaces);
  CHECK(selectionCreateKind({side}) == SelectionCreateKind::Round);
  CHECK(selectionCreateKind({rim}) == SelectionCreateKind::Round);
  CHECK(selectionCreateKind({side, rim}) == SelectionCreateKind::Round);
  CHECK(selectionCreateKind({point}) == SelectionCreateKind::SketchPoints);
  CHECK(selectionCreateKind({curve}) == SelectionCreateKind::Path);
  CHECK(selectionCreateKind({straight}) == SelectionCreateKind::Path);
  CHECK(selectionCreateKind({curve, straight, rim}) == SelectionCreateKind::Path);
  CHECK(selectionCreateKind({body, other}) == SelectionCreateKind::Bodies);
  CHECK(selectionCreateKind({plane}) == SelectionCreateKind::Planes);
  CHECK(selectionCreateKind({profileA, top}) == SelectionCreateKind::None);
  CHECK(selectionCreateKind({top, side}) == SelectionCreateKind::None);
  CHECK(selectionCreateKind({body, plane}) == SelectionCreateKind::None);
  CHECK(selectionCreateKind({cone}) == SelectionCreateKind::None);
  CHECK(selectionCreateKind({mesh}) == SelectionCreateKind::None);

  // The lists, each after Extrude; Loft only with two or more profiles.
  const QString extrude = QStringLiteral("solid.extrude");
  CHECK(selectionCreateCandidates({profileA}) ==
        QStringList({extrude, QStringLiteral("solid.revolve"), QStringLiteral("solid.sweep")}));
  CHECK(selectionCreateCandidates({profileA, profileB}) ==
        QStringList({extrude, QStringLiteral("solid.revolve"), QStringLiteral("solid.sweep"),
                     QStringLiteral("solid.loft")}));
  CHECK(selectionCreateCandidates({top}) == QStringList({extrude, QStringLiteral("sketch.create"),
                                                         QStringLiteral("solid.hole"), QStringLiteral("solid.emboss")}));
  const QStringList roundList = {extrude, QStringLiteral("solid.thread"), QStringLiteral("solid.pulley"),
                             QStringLiteral("solid.gear"), QStringLiteral("solid.bearing")};
  CHECK(selectionCreateCandidates({side}) == roundList);
  CHECK(selectionCreateCandidates({rim}) == roundList);
  CHECK(selectionCreateCandidates({point}) ==
        QStringList({extrude, QStringLiteral("solid.hole"), QStringLiteral("solid.pattern_at_points")}));
  CHECK(selectionCreateCandidates({curve, straight}) ==
        QStringList({extrude, QStringLiteral("solid.sweep"), QStringLiteral("solid.pipe"),
                     QStringLiteral("solid.path_pattern")}));
  CHECK(selectionCreateCandidates({body}) ==
        QStringList({extrude, QStringLiteral("solid.rectangular_pattern"), QStringLiteral("solid.circular_pattern"),
                     QStringLiteral("solid.mirror"), QStringLiteral("solid.combine")}));
  CHECK(selectionCreateCandidates({plane}) == QStringList({extrude, QStringLiteral("sketch.create")}));
  CHECK(selectionCreateCandidates({}).isEmpty());
  CHECK(selectionCreateCandidates({profileA, top}).isEmpty());

  // The row: the candidates that fit, at most four, Extrude first.
  const auto every = [](const QString&) { return true; };
  CHECK(selectionCreateCommands({profileA}, every) ==
        QStringList({extrude, QStringLiteral("solid.revolve"), QStringLiteral("solid.sweep")}));
  CHECK(selectionCreateCommands({profileA, profileB}, every).size() == kSelectionCreateButtons);
  CHECK(selectionCreateCommands({profileA, profileB}, every).first() == extrude);
  CHECK(selectionCreateCommands({profileA, profileB}, every).last() == QStringLiteral("solid.loft"));
  CHECK(!selectionCreateCommands({profileA}, every).contains(QStringLiteral("solid.loft")));
  CHECK(selectionCreateCommands({side}, every) == roundList.mid(0, kSelectionCreateButtons));
  const auto noExtrude = [&extrude](const QString& id) { return id != extrude; };
  CHECK(selectionCreateCommands({side}, noExtrude) == roundList.mid(1));
  CHECK(selectionCreateCommands({side}, noExtrude).first() == QStringLiteral("solid.thread"));
  CHECK(selectionCreateCommands({body}, noExtrude).size() == 4);
  CHECK(selectionCreateCommands({}, every).isEmpty());
  CHECK(selectionCreateCommands({profileA, top}, every).isEmpty());
  CHECK(selectionCreateCommands({body, plane}, every).isEmpty());
  CHECK(selectionCreateCommands({profileA}, [](const QString&) { return false; }).isEmpty());
  CHECK(selectionCreateCommands({profileA}, {}).isEmpty());
  CHECK(selectionCreateLogText({QStringLiteral("Extrude"), QStringLiteral("Revolve"), QStringLiteral("Sweep")}) ==
        QStringLiteral("Extrude | Revolve | Sweep"));
  CHECK(selectionCreateLogText({}) == QStringLiteral("none"));

  // Whether a command can use the whole selection: its first selection
  // input takes every item. Extrude's: profiles, sketch curves or one
  // planar face (alone).
  CommandDef extrudeDef;
  extrudeDef.id = extrude;
  extrudeDef.inputs = {
      choiceInput(QStringLiteral("type"), QStringLiteral("Type"),
                  {{QStringLiteral("solid"), QStringLiteral("Extrude")}, {QStringLiteral("thin"), QStringLiteral("Thin")}},
                  QStringLiteral("solid")),
      selectionInput(QStringLiteral("profiles"), QStringLiteral("Profiles"),
                     SelectKind::Profile | SelectKind::SketchCurve | SelectKind::Face, 1, 0)
          .withAccepts([](const SelectionItem& item) {
            return item.kind != SelectKind::Face || item.geometry == QStringLiteral("plane");
          })
          .withPickRule(
              [](const CommandState& state) -> SelectFilter {
                return state.choice(QStringLiteral("type")) == QStringLiteral("thin")
                           ? SelectKind::Profile | SelectKind::SketchCurve
                           : SelectKind::Profile | SelectKind::Face;
              },
              [](const SelectionItem&, const CommandState&) { return true; }),
  };
  extrudeDef.offeredFor = [](const Selection& selection) {
    const auto faces = std::count_if(selection.begin(), selection.end(),
                                     [](const SelectionItem& item) { return item.kind == SelectKind::Face; });
    return faces == 0 || (faces == 1 && selection.size() == 1);
  };
  CHECK(takesWholeSelection(extrudeDef, {profileA}));
  CHECK(takesWholeSelection(extrudeDef, {profileA, profileB}));
  // Sketch curves only as a thin extrusion, which is not the default: the
  // selection would not go into Profiles as the command starts.
  CHECK(!takesWholeSelection(extrudeDef, {curve}));
  CommandDef thinDef = extrudeDef;
  thinDef.inputs[0].defaultChoice = QStringLiteral("thin");
  CHECK(takesWholeSelection(thinDef, {curve}));
  CHECK(!takesWholeSelection(thinDef, {top}));
  CHECK(takesWholeSelection(extrudeDef, {top}));
  CHECK(!takesWholeSelection(extrudeDef, {top, bottom})); // offeredFor: one face alone
  CHECK(!takesWholeSelection(extrudeDef, {side}));        // not planar
  CHECK(!takesWholeSelection(extrudeDef, {profileA, body}));
  CHECK(!takesWholeSelection(extrudeDef, {}));

  // A further required input (Revolve's axis) is picked next: it does not
  // keep the command out.
  CommandDef revolveDef;
  revolveDef.id = QStringLiteral("solid.revolve");
  revolveDef.inputs = {selectionInput(QStringLiteral("profiles"), QStringLiteral("Profiles"),
                                      SelectKind::Profile | SelectKind::Face, 1, 0),
                       selectionInput(QStringLiteral("axis"), QStringLiteral("Axis"), SelectKind::Axis, 1, 1)};
  CHECK(takesWholeSelection(revolveDef, {profileA, profileB}));
  CHECK(!takesWholeSelection(revolveDef, {plane}));

  // As many as the first input takes (Hole's one face), only when it is
  // shown at the start (Hole's sketch points are a further choice's).
  CommandDef holeDef;
  holeDef.id = QStringLiteral("solid.hole");
  const auto placement = [](const char* value) {
    return [value](const CommandState& state) {
      return state.choice(QStringLiteral("placement")) == QString::fromLatin1(value);
    };
  };
  holeDef.inputs = {
      choiceInput(QStringLiteral("placement"), QStringLiteral("Placement"),
                  {{QStringLiteral("face"), QStringLiteral("On Face")}, {QStringLiteral("sketch"), QStringLiteral("From Sketch")}},
                  QStringLiteral("face")),
      selectionInput(QStringLiteral("face"), QStringLiteral("Face"), SelectKind::Face, 1, 1)
          .withAccepts([](const SelectionItem& item) { return item.geometry == QStringLiteral("plane"); })
          .withVisible(placement("face")),
      selectionInput(QStringLiteral("points"), QStringLiteral("Sketch Points"), SelectKind::SketchPoint, 1, 0)
          .withVisible(placement("sketch")),
  };
  CHECK(takesWholeSelection(holeDef, {top}));
  CHECK(!takesWholeSelection(holeDef, {top, bottom})); // one face only
  CHECK(!takesWholeSelection(holeDef, {point}));       // not its first input's
  CommandDef hidden = holeDef;
  hidden.inputs[0].defaultChoice = QStringLiteral("sketch");
  CHECK(!takesWholeSelection(hidden, {top})); // its face input hidden at the start

  // Kinds taken in another form (a member's edge as its body).
  CommandDef bodies;
  bodies.id = QStringLiteral("solid.mirror");
  bodies.inputs = {selectionInput(QStringLiteral("bodies"), QStringLiteral("Objects"), SelectKind::Body, 1, 0)
                       .withAlsoTakes(SelectKind::Edge)
                       .withConvert([](const SelectionItem& item) {
                         return SelectionItem{SelectKind::Body, item.owner, QString(), QString()};
                       })};
  CHECK(takesWholeSelection(bodies, {body, other}));
  CHECK(takesWholeSelection(bodies, {straight}));
  CHECK(!takesWholeSelection(bodies, {top}));
  CommandDef surfaces = bodies;
  surfaces.surfaceBodiesOnly = true;
  CHECK(!takesWholeSelection(surfaces, {body}));
  CommandDef noSelection;
  noSelection.id = QStringLiteral("solid.box");
  noSelection.inputs = {choiceInput(QStringLiteral("operation"), QStringLiteral("Operation"),
                                    {{QStringLiteral("new_body"), QStringLiteral("New Body")}},
                                    QStringLiteral("new_body"))};
  CHECK(!takesWholeSelection(noSelection, {body}));

  // Create Sketch: one plane or planar face.
  CommandDef sketch;
  sketch.id = QStringLiteral("sketch.create");
  sketch.kind = CommandDef::Kind::Action;
  CHECK(takesWholeSelection(sketch, {top}));
  CHECK(takesWholeSelection(sketch, {plane}));
  CHECK(!takesWholeSelection(sketch, {side}));
  CHECK(!takesWholeSelection(sketch, {top, bottom}));
  CHECK(!takesWholeSelection(sketch, {mesh}));
  CommandDef undo = sketch;
  undo.id = QStringLiteral("edit.undo");
  CHECK(!takesWholeSelection(undo, {top}));

  // The rule with these definitions: a planar face gives Extrude, Create
  // Sketch and Hole; two faces nothing.
  const QVector<const CommandDef*> defs = {&extrudeDef, &revolveDef, &holeDef, &sketch, &bodies};
  const auto fitsWith = [&defs](const Selection& selection) {
    return [&defs, selection](const QString& id) {
      const auto def = std::find_if(defs.begin(), defs.end(), [&id](const CommandDef* d) { return d->id == id; });
      return def != defs.end() && takesWholeSelection(**def, selection);
    };
  };
  CHECK(selectionCreateCommands({top}, fitsWith({top})) ==
        QStringList({extrude, QStringLiteral("sketch.create"), QStringLiteral("solid.hole")}));
  CHECK(selectionCreateCommands({top, bottom}, fitsWith({top, bottom})).isEmpty());
  CHECK(selectionCreateCommands({profileA}, fitsWith({profileA})) ==
        QStringList({extrude, QStringLiteral("solid.revolve")}));
  CHECK(selectionCreateCommands({plane}, fitsWith({plane})) == QStringList({QStringLiteral("sketch.create")}));
  CHECK(selectionCreateCommands({body}, fitsWith({body})) == QStringList({QStringLiteral("solid.mirror")}));

  // Names on one or two lines.
  CHECK(SelectionCreatePanel::buttonText(QStringLiteral("Extrude")) == QStringLiteral("Extrude"));
  CHECK(SelectionCreatePanel::buttonText(QStringLiteral("Create Sketch")) == QStringLiteral("Create\nSketch"));
  CHECK(SelectionCreatePanel::buttonText(QStringLiteral("Rectangular Pattern")) ==
        QStringLiteral("Rectangular\nPattern"));
  CHECK(SelectionCreatePanel::buttonText(QStringLiteral("Pattern at Points")) == QStringLiteral("Pattern\nat Points"));

  // The row's buttons: made once, changed in place, hidden with nothing.
  SelectionCreatePanel panel;
  installMacToolButtonStyle(&panel);
  CHECK(panel.isHidden());
  CHECK(panel.findChildren<QToolButton*>().size() == kSelectionCreateButtons);
  CHECK(panel.button(-1) == nullptr && panel.button(kSelectionCreateButtons) == nullptr);
  CHECK(!panel.setEntries({}));
  using Entry = SelectionCreatePanel::Entry;
  const Entry extrudeEntry{extrude, QStringLiteral("Extrude"), QStringLiteral("extrude"), QStringLiteral("Pushes")};
  const Entry revolveEntry{QStringLiteral("solid.revolve"), QStringLiteral("Revolve"), QStringLiteral("revolve"),
                           QString()};
  const Entry sweepEntry{QStringLiteral("solid.sweep"), QStringLiteral("Sweep"), QStringLiteral("sweep"), QString()};
  const Entry sketchEntry{QStringLiteral("sketch.create"), QStringLiteral("Create Sketch"), QStringLiteral("sketch"),
                          QString()};
  const Entry loftEntry{QStringLiteral("solid.loft"), QStringLiteral("Loft"), QStringLiteral("loft"), QString()};
  CHECK(panel.setEntries({extrudeEntry, revolveEntry, sweepEntry}));
  CHECK(!panel.isHidden());
  CHECK(!panel.button(0)->isHidden() && !panel.button(1)->isHidden() && !panel.button(2)->isHidden());
  CHECK(panel.button(3)->isHidden());
  CHECK(panel.button(0)->text() == QStringLiteral("Extrude"));
  CHECK(panel.button(0)->accessibleName() == QStringLiteral("Extrude"));
  CHECK(panel.button(0)->toolTip() == QStringLiteral("Extrude\nPushes"));
  CHECK(panel.button(1)->toolTip() == QStringLiteral("Revolve"));
  CHECK(panel.button(0)->toolButtonStyle() == Qt::ToolButtonTextUnderIcon);
  CHECK(panel.button(0)->autoRaise());
  CHECK(panel.button(0)->iconSize() == QSize(24, 24));
#ifdef MITCAD_MAC_UI
  CHECK(panel.button(0)->property("macFlatToolButton").toBool());
#endif
  CHECK(!panel.setEntries({extrudeEntry, revolveEntry, sweepEntry})); // the same: nothing changes
  QToolButton* first = panel.button(0);
  CHECK(panel.setEntries({extrudeEntry, sketchEntry, revolveEntry, sweepEntry, loftEntry}));
  CHECK(panel.entries().size() == kSelectionCreateButtons); // the first four
  CHECK(panel.button(0) == first);                          // the same button, in place
  CHECK(panel.findChildren<QToolButton*>().size() == kSelectionCreateButtons);
  CHECK(!panel.button(3)->isHidden());
  CHECK(panel.button(1)->text() == QStringLiteral("Create\nSketch"));
  CHECK(panel.button(1)->accessibleName() == QStringLiteral("Create Sketch"));
  QStringList requested;
  QObject::connect(&panel, &SelectionCreatePanel::commandRequested, &panel,
                   [&requested](const QString& id) { requested << id; });
  panel.button(1)->click();
  panel.button(0)->click();
  CHECK(requested == QStringList({QStringLiteral("sketch.create"), extrude}));
  CHECK(panel.setEntries({}));
  CHECK(panel.isHidden());
  CHECK(panel.button(0)->isHidden() && panel.button(0)->text().isEmpty());
  requested.clear();
  panel.button(0)->click(); // a hidden button asks for nothing
  CHECK(requested.isEmpty());

  // Whole-body shortcuts reuse those same controls and route their exact
  // command IDs even when two-word captions are wrapped below their icons.
  const Entry rectangularEntry{QStringLiteral("solid.rectangular_pattern"), QStringLiteral("Rectangular Pattern"),
                               QStringLiteral("rectangular-pattern"), QStringLiteral("Copies in rows")};
  const Entry circularEntry{QStringLiteral("solid.circular_pattern"), QStringLiteral("Circular Pattern"),
                            QStringLiteral("circular-pattern"), QString()};
  const Entry mirrorEntry{QStringLiteral("solid.mirror"), QStringLiteral("Mirror"), QStringLiteral("mirror"), QString()};
  const Entry combineEntry{QStringLiteral("solid.combine"), QStringLiteral("Combine"), QStringLiteral("combine"), QString()};
  CHECK(panel.setEntries({rectangularEntry, circularEntry, mirrorEntry, combineEntry}));
  CHECK(panel.button(0) == first);
  CHECK(panel.button(0)->text() == QStringLiteral("Rectangular\nPattern"));
  CHECK(panel.button(1)->text() == QStringLiteral("Circular\nPattern"));
  CHECK(panel.button(0)->accessibleName() == QStringLiteral("Rectangular Pattern"));
  CHECK(panel.button(0)->toolTip() == QStringLiteral("Rectangular Pattern\nCopies in rows"));
  for (int index = 0; index < kSelectionCreateButtons; ++index) {
    CHECK(!panel.button(index)->isHidden());
    CHECK(panel.button(index)->autoRaise());
    CHECK(panel.button(index)->toolButtonStyle() == Qt::ToolButtonTextUnderIcon);
#ifdef MITCAD_MAC_UI
    CHECK(panel.button(index)->property("macFlatToolButton").toBool());
#endif
    panel.button(index)->click();
  }
  CHECK(requested == QStringList({rectangularEntry.id, circularEntry.id, mirrorEntry.id, combineEntry.id}));
  CHECK(panel.setEntries({sketchEntry}));
  CHECK(panel.button(0) == first);
  CHECK(panel.button(1)->isHidden() && panel.button(1)->accessibleName().isEmpty());
  requested.clear();
  panel.button(1)->click();
  CHECK(requested.isEmpty());
  panel.button(0)->click();
  CHECK(requested == QStringList({sketchEntry.id}));

  // The Docked chrome's section title in place of the frame's.
  SelectionCreatePanel docked;
  docked.setSectionTitle(new QLabel(QStringLiteral("CREATE")));
  CHECK(docked.title().isEmpty() && docked.isFlat());
  CHECK(docked.findChild<QLabel*>() != nullptr);

  return failures;
}
