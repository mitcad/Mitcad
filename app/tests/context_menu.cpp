// SPDX-License-Identifier: MIT
// The view's context menu's split (mitcad#494): whether a selection alone
// completes a command (its first entries) or the command needs further
// picks (under More), computed from the definition (CommandDef::completedBy);
// and what the menu offers at its top level (mitcad#735, takenWholeByOneOf);
// and which inputs take an opening picked by a free edge (mitcad#1927).
#include "UnitTests.hpp"
#include "framework/Command.hpp"

#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "context_menu.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int contextMenuTests() {
  using namespace mitcad;
  const SelectionItem face{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:end(r1)"),
                           QStringLiteral("plane")};
  const SelectionItem side{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:side(c1)"),
                           QStringLiteral("plane")};
  const SelectionItem edge{SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{a|b}"), QStringLiteral("line")};
  const SelectionItem profile{SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r1"), QStringLiteral("plane")};

  // One selection input and values: the faces complete it.
  CommandDef offset;
  offset.inputs = {selectionInput(QStringLiteral("faces"), QStringLiteral("Faces"), SelectKind::Face, 1, 0),
                   valueInput(QStringLiteral("distance"), QStringLiteral("Distance"), ValueKind::Length,
                              QStringLiteral("2 mm"))};
  CHECK(offset.completedBy({face}));
  CHECK(offset.completedBy({face, side}));
  CHECK(!offset.completedBy({edge}));
  CHECK(!offset.completedBy({}));

  // A second required selection is a further pick; an optional one is not.
  CommandDef draft = offset;
  draft.inputs.insert(1, selectionInput(QStringLiteral("plane"), QStringLiteral("Plane"), SelectKind::Plane, 1, 1));
  CHECK(!draft.completedBy({face}));
  CommandDef optional = offset;
  optional.inputs.append(selectionInput(QStringLiteral("objects"), QStringLiteral("Objects"), SelectKind::Body, 0, 0));
  CHECK(optional.completedBy({face}));

  // A required input shown only in another mode is not needed at the start.
  CommandDef modes = offset;
  modes.inputs.append(choiceInput(QStringLiteral("mode"), QStringLiteral("Mode"),
                                  {{QStringLiteral("faces"), QStringLiteral("Faces")},
                                   {QStringLiteral("points"), QStringLiteral("Points")}},
                                  QStringLiteral("faces")));
  modes.inputs.append(selectionInput(QStringLiteral("points"), QStringLiteral("Points"), SelectKind::SketchPoint, 1, 0)
                          .withVisible([](const CommandState& state) {
                            return state.choice(QStringLiteral("mode")) == QStringLiteral("points");
                          }));
  CHECK(modes.completedBy({face}));
  modes.inputs[2].defaultChoice = QStringLiteral("points");
  CHECK(!modes.completedBy({face}));

  // As many as the first input needs, and no more than it takes.
  CommandDef sections = offset;
  sections.inputs[0].min = 2;
  CHECK(!sections.completedBy({face}));
  CHECK(sections.completedBy({face, side}));
  CommandDef single = offset;
  single.inputs[0].max = 1;
  CHECK(!single.completedBy({face, side}));

  // A list's first row: its other required inputs are further picks
  // (Fillet's edges alone complete it, a constraint's two geometries not).
  CommandDef fillet;
  fillet.inputs = {listInput(QStringLiteral("sets"), QStringLiteral("Sets"),
                             {selectionInput(QStringLiteral("edges"), QStringLiteral("Edges"),
                                             SelectKind::Edge | SelectKind::Face, 1, 0),
                              selectionInput(QStringLiteral("vertex"), QStringLiteral("Vertex"), SelectKind::Vertex, 0, 1)},
                             QStringLiteral("Add"), QStringLiteral("Set %1"))};
  CHECK(fillet.completedBy({edge}));
  CommandDef pair = fillet;
  pair.inputs[0].children[1].min = 1;
  CHECK(!pair.completedBy({edge}));

  // A creation command (one of a CREATE group: `true`) given a face only
  // stands on it; a profile is what it makes, and `completes` decides in
  // place of the rule.
  CommandDef box;
  box.inputs = {selectionInput(QStringLiteral("plane"), QStringLiteral("Plane"), SelectKind::Plane | SelectKind::Face, 1, 1)};
  CHECK(!box.completedBy({face}, true));
  CommandDef extrude;
  extrude.inputs = {selectionInput(QStringLiteral("profiles"), QStringLiteral("Profiles"), SelectKind::Profile, 1, 0)};
  CHECK(extrude.completedBy({profile}, true));
  CommandDef thread = offset;
  CHECK(!thread.completedBy({face}, true));
  thread.completes = [](const Selection&) { return true; };
  CHECK(thread.completedBy({face}, true));
  CommandDef narrowed = offset;
  narrowed.completes = [](const Selection& items) { return items.size() == 1; };
  CHECK(narrowed.completedBy({face}) && !narrowed.completedBy({face, side}));

  // A placement input (mitcad#656): a shaft's edge, an axis or a point is
  // only where the part goes too, so the creation command is under More;
  // outside CREATE the rule does not apply.
  const SelectionItem rim{SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{c|d}"), QStringLiteral("circle")};
  const SelectionItem axis{SelectKind::Axis, QStringLiteral("z"), QString(), QStringLiteral("axis")};
  CommandDef pulley;
  pulley.inputs = {selectionInput(QStringLiteral("at"), QStringLiteral("Place On"),
                                  SelectKind::Face | SelectKind::Edge | SelectKind::Axis | SelectKind::Point, 1, 1)
                       .withPlaces()};
  CHECK(!pulley.completedBy({rim}, true));
  CHECK(!pulley.completedBy({axis}, true));
  CHECK(!pulley.completedBy({face}, true));
  CommandDef unplaced = pulley;
  unplaced.inputs[0].places = false;
  CHECK(unplaced.completedBy({rim}, true));
  CommandDef placedElsewhere = pulley;
  CHECK(placedElsewhere.completedBy({rim}));

  // At the top (mitcad#735): Move/Copy's objects, bodies or placed
  // components, each kind whole; not a mix, faces, or the root.
  const InputDef bodyObjects =
      selectionInput(QStringLiteral("bodies"), QStringLiteral("Objects"), SelectKind::Body, 1, 0);
  const InputDef componentObjects =
      selectionInput(QStringLiteral("components"), QStringLiteral("Objects"), SelectKind::Component | SelectKind::Body,
                     1, 0)
          .withAccepts([](const SelectionItem& item) { return !item.occurrence.isEmpty(); });
  const auto atTop = takenWholeByOneOf({bodyObjects, componentObjects});
  const SelectionItem body{SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()};
  SelectionItem placedBody{SelectKind::Body, QStringLiteral("F3.b0"), QString(), QString()};
  placedBody.occurrence = QStringLiteral("O1");
  SelectionItem occurrence{SelectKind::Component, QStringLiteral("C1"), QString(), QString()};
  occurrence.occurrence = QStringLiteral("O1");
  SelectionItem nested = occurrence;
  nested.occurrence = QStringLiteral("O1/O2");
  const SelectionItem root{SelectKind::Component, QStringLiteral("C0"), QString(), QString()};
  CHECK(atTop({body}));
  CHECK(atTop({body, placedBody}));
  CHECK(atTop({occurrence, nested}));
  CHECK(!atTop({body, occurrence}));
  CHECK(!atTop({face}));
  CHECK(!atTop({root}));
  CHECK(!atTop({occurrence, root}));
  CHECK(!atTop({}));

  // The state a command starts with: defaults and a list's first rows.
  const CommandState start = modes.initialState();
  CHECK(start.text(QStringLiteral("distance")) == QStringLiteral("2 mm"));
  CHECK(start.choice(QStringLiteral("mode")) == QStringLiteral("points"));
  CHECK(fillet.initialState().rows(QStringLiteral("sets")) == 1);

  // An opening picked by a free edge (mitcad#1927): an edge of the body
  // named by the opening; only an input that takes openings takes it.
  const QJsonObject wall{{QStringLiteral("faces"), QJsonArray{QStringLiteral("F2:side(c1)")}},
                         {QStringLiteral("index"), 1}};
  const SelectionItem opening = openingEdgeItem(QStringLiteral("F2.b0"), wall);
  CHECK(opening.kind == SelectKind::Edge && opening.owner == QStringLiteral("F2.b0"));
  CHECK(isOpeningEdge(opening) && !isOpeningEdge(edge) && !isOpeningEdge(face));
  CHECK(openingOf(opening) == wall);
  CHECK(openingOf(edge).isEmpty());
  CHECK(opening == openingEdgeItem(QStringLiteral("F2.b0"), wall));
  const InputDef edges = selectionInput(QStringLiteral("edges"), QStringLiteral("Edges"), SelectKind::Edge, 1, 0);
  CHECK(edges.accepts(edge) && !edges.accepts(opening));
  InputDef openings = edges;
  openings.withOpenings();
  CHECK(openings.accepts(edge) && openings.accepts(opening));

  // A single free edge (mitcad#1955): the opening and a point on the edge;
  // taken by the same inputs, kept as its whole opening while their Whole
  // Opening is on, and back as the edge when it goes off.
  const SelectionItem freeEdge = freeEdgeItem(QStringLiteral("F2.b0"), wall, {10.0, 0.0, 5.0});
  CHECK(freeEdge.kind == SelectKind::Edge && isOpeningEdge(freeEdge) && isFreeEdge(freeEdge) && !isFreeEdge(opening));
  CHECK(freeEdge.name.startsWith(QStringLiteral("{\"at\":[10,0,5],\"opening\":")));
  CHECK(openingOf(freeEdge) == wall);
  const std::array<double, 3> middle{10.0, 0.0, 5.0};
  CHECK(freeEdgePoint(freeEdge) == middle);
  CHECK(!freeEdgePoint(opening));
  CHECK(openings.accepts(freeEdge) && !edges.accepts(freeEdge));
  const SelectionItem whole = wholeOpeningItem(freeEdge);
  CHECK(whole == opening && whole.at == freeEdgePoint(freeEdge));
  CHECK(singleFreeEdgeItem(whole) == freeEdge);
  CHECK(singleFreeEdgeItem(opening) == opening); // no edge to go back to
  CHECK(wholeOpeningItem(edge) == edge && singleFreeEdgeItem(edge) == edge);
  InputDef checked = edges;
  checked.withOpenings(QStringLiteral("whole_opening"));
  CommandState state;
  state.setChecked(QStringLiteral("whole_opening"), true);
  CHECK(checked.takesWholeOpenings(state) && openings.takesWholeOpenings(state) && !edges.takesWholeOpenings(state));
  state.setChecked(QStringLiteral("whole_opening"), false);
  CHECK(!checked.takesWholeOpenings(state) && openings.takesWholeOpenings(state));
  return failures;
}
MITCAD_UNIT_TEST(contextMenuTests);
