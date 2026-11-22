// SPDX-License-Identifier: MIT
// What Extrude's and Revolve's Profiles take with a click (mitcad#868): a
// planar face alone, or sketch profiles and curves; a click on a face
// while they hold a profile (an edited boss whose Start is Object, the arm
// face it starts at clicked while Profiles was still the active input) is
// not taken, and the status bar says to click Start Object first, instead
// of the panel's "not both" error; the face they hold is taken again so
// that its click removes it.
// The profile they take by themselves as they start (mitcad#1226): on a
// sketch on a face, a shape drawn there before the face's leftover region
// around the drawn shapes.
// The profile commands' start (mitcad#2087, GitHub mitcad/Mitcad#487): a
// region of a 3D sketch as the newest unused profile is taken alone,
// without the planar `sketch` query that refuses a 3D sketch (Helix threw
// out of its start after Convert to 3D Sketch); a QR code's region still
// brings the code's other dark regions (mitcad#473).
// Extrude's and Revolve's start and whole sketch picks (mitcad#2194,
// GitHub mitcad/Mitcad#545): the unused members of the sketch's outline
// set (a plate's ring and an island in a hole, not the holes), the region
// alone once they are used or when the sketch's closed curves cross, which
// the panel then says; Show profiles leaves the holes unfilled.
#include "UnitTests.hpp"
#include "commands/ProfilePicks.hpp"

#include <algorithm>
#include <cstdio>
#include <exception>
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include <TopoDS_Shape.hxx>

#include "framework/Command.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "profile_picks.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandContext;
using mitcad::CommandState;
using mitcad::Selection;
using mitcad::SelectionItem;
using mitcad::SelectKind;
using mitcad::cmd::automaticProfile;
using mitcad::cmd::newestProfile;
using mitcad::cmd::ProfilesCommand;
using mitcad::cmd::profilesRefused;
using mitcad::cmd::profilesTake;

SelectionItem face(const char* name, const char* occurrence = "") {
  return {SelectKind::Face, QStringLiteral("F2.b0"), QString::fromLatin1(name), QStringLiteral("plane"),
          QString::fromLatin1(occurrence)};
}

const SelectionItem kProfile{SelectKind::Profile, QStringLiteral("F5"), QStringLiteral("r{c8[c6+c10,c1+c10]}"),
                             QString(), QStringLiteral("O1")};
const SelectionItem kCurve{SelectKind::SketchCurve, QStringLiteral("F5"), QStringLiteral("c3"), QStringLiteral("line"),
                           QStringLiteral("O1")};

void takeTests() {
  const SelectionItem inner = face("F2:side(c9[c15,c16])", "O1");
  const SelectionItem outer = face("F2:side(c4[c7,c15])", "O1");
  // Empty: either is the first pick.
  CHECK(profilesTake(inner, {}));
  CHECK(profilesTake(kProfile, {}));
  CHECK(profilesTake(kCurve, {}));
  // The edited boss: its profile held, the arm's face clicked.
  CHECK(!profilesTake(inner, {kProfile}));
  CHECK(!profilesTake(inner, {kCurve}));
  // More profiles of the sketch still go in.
  CHECK(profilesTake(kProfile, {kProfile}));
  // A face held: no profile beside it, no second face; its own click is
  // taken (and removes it), also when the edited definition's item names
  // no occurrence.
  CHECK(!profilesTake(kProfile, {inner}));
  CHECK(!profilesTake(kCurve, {inner}));
  CHECK(!profilesTake(outer, {inner}));
  CHECK(profilesTake(inner, {inner}));
  CHECK(profilesTake(inner, {face("F2:side(c9[c15,c16])")}));
  // The same face in another occurrence is another face.
  CHECK(!profilesTake(inner, {face("F2:side(c9[c15,c16])", "O2")}));
}

void hintTests() {
  const SelectionItem inner = face("F2:side(c9[c15,c16])", "O1");
  // Taken: no hint.
  CHECK(profilesRefused(inner, {}, true, ProfilesCommand::Extrude, QStringLiteral("object")).isEmpty());
  // A face that is not planar is refused as before, without this hint.
  CHECK(profilesRefused(inner, {kProfile}, false, ProfilesCommand::Extrude, QStringLiteral("object")).isEmpty());
  // Start: Object shown: pick there.
  const QString object = profilesRefused(inner, {kProfile}, true, ProfilesCommand::Extrude, QStringLiteral("object"));
  CHECK(object.contains(QStringLiteral("click Start Object")));
  // Another start: set it to Object.
  const QString plane =
      profilesRefused(inner, {kProfile}, true, ProfilesCommand::Extrude, QStringLiteral("profile_plane"));
  CHECK(plane.contains(QStringLiteral("set Start to Object")));
  // Revolve has no start: clear Profiles to revolve the face.
  const QString revolve = profilesRefused(inner, {kProfile}, true, ProfilesCommand::Revolve);
  CHECK(revolve.contains(QStringLiteral("to revolve the face")) && !revolve.contains(QStringLiteral("Start")));
  CHECK(profilesRefused(face("F2:side(c4[c7,c15])", "O1"), {inner}, true, ProfilesCommand::Extrude)
            .contains(QStringLiteral("one planar face")));
  CHECK(profilesRefused(kProfile, {inner}, true, ProfilesCommand::Extrude).contains(QStringLiteral("extruded alone")));
  CHECK(profilesRefused(kProfile, {inner}, true, ProfilesCommand::Revolve).contains(QStringLiteral("revolved alone")));
}

// A row of the model's `profiles` query.
QJsonObject profile(const char* sketch, const char* region, bool consumed, bool faceBounded = false) {
  QJsonObject row{{QStringLiteral("sketch"), QString::fromLatin1(sketch)},
                  {QStringLiteral("region"), QString::fromLatin1(region)},
                  {QStringLiteral("consumed"), consumed}};
  if (faceBounded) {
    row.insert(QStringLiteral("face_bounded"), true);
  }
  return row;
}

bool takes(const std::optional<SelectionItem>& chosen, const char* sketch, const char* region) {
  return chosen && chosen->kind == SelectKind::Profile && chosen->owner == QString::fromLatin1(sketch) &&
         chosen->name == QString::fromLatin1(region) && chosen->geometry == QStringLiteral("plane");
}

void automaticTests() {
  // A box (F2) from the rectangle of F1; on its top face F3 two closed
  // shapes, a slot and a circle; the face's leftover around them is a
  // region too, bounded in part by the face's edges, and listed after
  // them or between them: one of the drawn shapes, the newest, is taken.
  const char* leftover = "r{b{E{F2:end(r{c1})|F2:side(c1)}},b{E{F2:end(r{c1})|F2:side(c2)}}}";
  CHECK(takes(automaticProfile(QJsonArray{profile("F1", "r{c1[c4,c2]}", true), profile("F3", "r{c1[c2,c4]}", false),
                                          profile("F3", "r{c5}", false), profile("F3", leftover, false, true)}),
              "F3", "r{c5}"));
  CHECK(takes(automaticProfile(QJsonArray{profile("F1", "r{c1[c4,c2]}", true), profile("F3", "r{c1[c2,c4]}", false),
                                          profile("F3", leftover, false, true), profile("F3", "r{c5}", false)}),
              "F3", "r{c5}"));
  // A drawn shape already extruded: the other one.
  CHECK(takes(automaticProfile(QJsonArray{profile("F3", "r{c1[c2,c4]}", false), profile("F3", "r{c5}", true),
                                          profile("F3", leftover, false, true)}),
              "F3", "r{c1[c2,c4]}"));
  // Only the face's pieces (a line across the face): the newest of them,
  // not an older sketch's drawn shape.
  CHECK(takes(automaticProfile(QJsonArray{profile("F1", "r{c1[c4,c2]}", false),
                                          profile("F3", "r{b{c1},c1}", false, true),
                                          profile("F3", leftover, false, true)}),
              "F3", leftover));
  // The newest sketch used up: the one before it, as before.
  CHECK(takes(automaticProfile(QJsonArray{profile("F1", "r{c1[c4,c2]}", false), profile("F3", "r{c5}", true)}), "F1",
              "r{c1[c4,c2]}"));
  // A sketch on a plane: its last profile.
  CHECK(takes(automaticProfile(QJsonArray{profile("F1", "r{c1}", false), profile("F1", "r{c2}", false)}), "F1",
              "r{c2}"));
  // Nothing unused: none.
  CHECK(!automaticProfile(QJsonArray{profile("F1", "r{c1}", true)}));
  CHECK(!automaticProfile(QJsonArray{}));
}

// A model with planar sketches and 3D sketches by uid: its `feature` query
// gives their types, its `sketch` query answers for the planar ones and
// throws for a 3D sketch as the model does (core/model-api sketch.rs), and
// for a sketch it does not know. It records the sketches that query was
// asked for.
class ProfileModel : public CommandContext {
public:
  ProfileModel(QJsonArray profiles, QJsonObject planar, QStringList spatial)
      : m_profiles(std::move(profiles)), m_planar(std::move(planar)), m_spatial(std::move(spatial)) {}

  QJsonValue query(const QJsonObject& request) const override {
    const QString name = request.value(QStringLiteral("query")).toString();
    const QString uid = request.value(QStringLiteral("uid")).toString();
    if (name == QStringLiteral("profiles")) {
      return m_profiles;
    }
    if (name == QStringLiteral("feature")) {
      if (m_spatial.contains(uid)) {
        return QJsonObject{{QStringLiteral("uid"), uid}, {QStringLiteral("type"), QStringLiteral("spatial_sketch")}};
      }
      if (m_planar.contains(uid)) {
        return QJsonObject{{QStringLiteral("uid"), uid}, {QStringLiteral("type"), QStringLiteral("sketch")}};
      }
      throw std::runtime_error("no such feature");
    }
    if (name == QStringLiteral("sketch")) {
      m_asked << uid;
      if (m_spatial.contains(uid)) {
        throw std::runtime_error("Sketch2 is a 3D sketch, which this does not take");
      }
      if (!m_planar.contains(uid)) {
        throw std::runtime_error("no such sketch");
      }
      return m_planar.value(uid);
    }
    return QJsonValue();
  }
  std::shared_ptr<mitcad::geometry::Shape> bodyShape(const QString&) const override { return nullptr; }
  TopoDS_Shape itemShape(const SelectionItem&) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject&) const override { return TopoDS_Shape(); }

  // The sketches the `sketch` query was asked for.
  const QStringList& asked() const { return m_asked; }

private:
  QJsonArray m_profiles;
  QJsonObject m_planar;
  QStringList m_spatial;
  mutable QStringList m_asked;
};

// Profiles as newestProfile leaves them in a new command's state; empty,
// with a failed check, when it throws.
Selection started(const CommandContext& model) {
  CommandState state;
  try {
    newestProfile(state, model);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "profile_picks.cpp: newestProfile threw: %s\n", e.what());
    ++failures;
    return Selection();
  }
  return state.items(QStringLiteral("profiles"));
}

bool holds(const Selection& profiles, const char* sketch, const char* key) {
  return std::any_of(profiles.begin(), profiles.end(), [sketch, key](const SelectionItem& item) {
    return item.kind == SelectKind::Profile && item.owner == QString::fromLatin1(sketch) &&
           item.name == QString::fromLatin1(key);
  });
}

// A region of the `sketch` query's answer, of a text when `text` is set.
QJsonObject sketchRegion(const char* key, const char* text = nullptr, bool letter = false) {
  QJsonObject row{{QStringLiteral("key"), QString::fromLatin1(key)}};
  if (text != nullptr) {
    row.insert(QStringLiteral("text"), QString::fromLatin1(text));
    row.insert(QStringLiteral("letter"), letter);
  }
  return row;
}

void startTests() {
  // GitHub mitcad/Mitcad#487: a rectangle's sketch converted to a 3D
  // sketch (F7) is the newest with an unused region; Helix and Extrude
  // start with that region alone, never asking the planar query for it.
  {
    const QJsonObject planar{{QStringLiteral("regions"), QJsonArray{sketchRegion("r{c1}")}}};
    const ProfileModel model(QJsonArray{profile("F1", "r{c1}", true), profile("F7", "r{c1[c2,c4]}", false)},
                             QJsonObject{{QStringLiteral("F1"), planar}}, QStringList{QStringLiteral("F7")});
    const Selection profiles = started(model);
    CHECK(profiles.size() == 1);
    CHECK(holds(profiles, "F7", "r{c1[c2,c4]}"));
    CHECK(!model.asked().contains(QStringLiteral("F7")));
  }
  // A planar sketch the model no longer answers for (just taken back): its
  // region alone, no throw.
  {
    const ProfileModel model(QJsonArray{profile("F3", "r{c5}", false)}, QJsonObject(), QStringList());
    const Selection profiles = started(model);
    CHECK(profiles.size() == 1);
    CHECK(holds(profiles, "F3", "r{c5}"));
  }
  // A QR code's dark region on a planar sketch brings the code's other
  // free dark regions, not its light ones nor other shapes (mitcad#473).
  {
    const QJsonObject code{{QStringLiteral("id"), QStringLiteral("T1")},
                           {QStringLiteral("code"), QJsonObject{{QStringLiteral("kind"), QStringLiteral("qr")}}}};
    const QJsonArray regions{sketchRegion("r{a}", "T1", true), sketchRegion("r{b}", "T1", true),
                             sketchRegion("r{c}", "T1", false), sketchRegion("r{d}")};
    const QJsonObject sketch{{QStringLiteral("regions"), regions}, {QStringLiteral("texts"), QJsonArray{code}}};
    const ProfileModel model(QJsonArray{profile("F5", "r{c}", false), profile("F5", "r{d}", false),
                                        profile("F5", "r{b}", false), profile("F5", "r{a}", false)},
                             QJsonObject{{QStringLiteral("F5"), sketch}}, QStringList());
    const Selection profiles = started(model);
    CHECK(profiles.size() == 2);
    CHECK(holds(profiles, "F5", "r{a}"));
    CHECK(holds(profiles, "F5", "r{b}"));
  }
  // Profiles already held (an edited feature): left as they are, the
  // model not asked.
  {
    const ProfileModel model(QJsonArray{profile("F7", "r{c1}", false)}, QJsonObject(),
                             QStringList{QStringLiteral("F7")});
    CommandState state;
    state.items(QStringLiteral("profiles")).append(kProfile);
    newestProfile(state, model);
    CHECK(state.items(QStringLiteral("profiles")).size() == 1);
    CHECK(model.asked().isEmpty());
  }
}

// A row of the `profiles` query with the outline set's marks (mitcad#2194):
// `outline` true or false for a sketch whose closed curves do not cross,
// `curves_cross` for one whose do.
enum class Mark { Member, Hole, Crossing, None };

QJsonObject marked(const char* sketch, const char* region, Mark mark, bool consumed = false) {
  QJsonObject row = profile(sketch, region, consumed);
  if (mark == Mark::Crossing) {
    row.insert(QStringLiteral("curves_cross"), true);
  } else if (mark != Mark::None) {
    row.insert(QStringLiteral("outline"), mark == Mark::Member);
  }
  return row;
}

// Profiles as newestOutline (Extrude's and Revolve's start) leaves them,
// with the state for its note.
CommandState outlineStart(const CommandContext& model, const Selection& before = Selection()) {
  CommandState state;
  state.setItems(QStringLiteral("profiles"), before);
  try {
    mitcad::cmd::newestOutline(state, model);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "profile_picks.cpp: newestOutline threw: %s\n", e.what());
    ++failures;
  }
  return state;
}

// The rows of a plate (F1) with two holes, one with an island in it, the
// regions as the model sorts them: the plate's ring, the first hole's
// ring about the island, the second hole's disc, the island.
QJsonArray plate(bool ringUsed = false) {
  return QJsonArray{marked("F1", "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}", Mark::Member, ringUsed),
                    marked("F1", "r{c9}", Mark::Hole), marked("F1", "r{c11}", Mark::Hole),
                    marked("F1", "r{c12[c15,c13],c13[c12,c14],c14[c13,c15],c15[c14,c12]}", Mark::Member, ringUsed)};
}

void outlineTests() {
  using mitcad::cmd::crossingNote;
  using mitcad::cmd::sketchOutline;
  using mitcad::cmd::unfilledProfile;
  const char* ring = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
  const char* island = "r{c12[c15,c13],c13[c12,c14],c14[c13,c15],c15[c14,c12]}";
  // A plate with two holes and an island in one: the ring and the island.
  {
    const mitcad::cmd::OutlinePick pick = sketchOutline(plate(), QStringLiteral("F1"));
    CHECK(!pick.crossing);
    CHECK(pick.items.size() == 2);
    CHECK(holds(pick.items, "F1", ring));
    CHECK(holds(pick.items, "F1", island));
    CHECK(!holds(pick.items, "F1", "r{c9}") && !holds(pick.items, "F1", "r{c11}"));
  }
  // Two separate rectangles, and a rectangle split by a line: all of them.
  {
    const QJsonArray rows{marked("F2", "r{c1[c4,c2]}", Mark::Member), marked("F2", "r{c5[c8,c6]}", Mark::Member),
                          marked("F3", "r{c1[c5,c2]}", Mark::Member), marked("F3", "r{c3[c5,c4]}", Mark::Member)};
    CHECK(sketchOutline(rows, QStringLiteral("F2")).items.size() == 2);
    CHECK(sketchOutline(rows, QStringLiteral("F3")).items.size() == 2);
  }
  // Two overlapping circles: none, the curves cross.
  {
    const QJsonArray rows{marked("F4", "r{c1[c2,c2]}", Mark::Crossing),
                          marked("F4", "r{c1[c2,c2],c2[c1,c1]}", Mark::Crossing),
                          marked("F4", "r{c2[c1,c1]}", Mark::Crossing)};
    const mitcad::cmd::OutlinePick pick = sketchOutline(rows, QStringLiteral("F4"));
    CHECK(pick.crossing);
    CHECK(pick.items.isEmpty());
  }
  // Show profiles: a hole's disc unfilled; members, face-bounded regions
  // (no mark) and regions of crossing curves filled.
  CHECK(unfilledProfile(marked("F1", "r{c9}", Mark::Hole)));
  CHECK(!unfilledProfile(marked("F1", ring, Mark::Member)));
  CHECK(!unfilledProfile(marked("F1", "r{b{c1},c1}", Mark::None)));
  CHECK(!unfilledProfile(marked("F4", "r{c2[c1,c1]}", Mark::Crossing)));

  // Extrude's start: the plate's ring and its island, not a hole.
  {
    const ProfileModel model(plate(), QJsonObject(), QStringList());
    const CommandState state = outlineStart(model);
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    CHECK(profiles.size() == 2);
    CHECK(holds(profiles, "F1", ring) && holds(profiles, "F1", island));
    CHECK(crossingNote(state, model).isEmpty());
  }
  // The ring and the island used: the next start is a hole's disc alone
  // (a boss in it), as before.
  {
    const ProfileModel model(plate(true), QJsonObject(), QStringList());
    const Selection profiles = outlineStart(model).items(QStringLiteral("profiles"));
    CHECK(profiles.size() == 1);
    CHECK(holds(profiles, "F1", "r{c11}"));
  }
  // The other profile commands keep their start: one region.
  CHECK(started(ProfileModel(plate(), QJsonObject(), QStringList())).size() == 1);
  // A newest sketch whose closed curves cross: one region, as before, and
  // the panel says why no more.
  {
    const QJsonArray rows{marked("F1", ring, Mark::Member), marked("F4", "r{c1[c2,c2]}", Mark::Crossing),
                          marked("F4", "r{c2[c1,c1]}", Mark::Crossing)};
    const ProfileModel model(rows, QJsonObject(), QStringList());
    const CommandState state = outlineStart(model);
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    CHECK(profiles.size() == 1);
    CHECK(holds(profiles, "F4", "r{c2[c1,c1]}"));
    CHECK(crossingNote(state, model).contains(QStringLiteral("cross")));
  }
  // A whole sketch selected before the command: its outline set, not the
  // newest sketch's region; one whose curves cross gives nothing, and the
  // panel says why.
  {
    QJsonArray rows = plate();
    rows.append(marked("F4", "r{c1[c2,c2]}", Mark::Crossing));
    const ProfileModel model(rows, QJsonObject(), QStringList());
    const SelectionItem plateSketch{SelectKind::Sketch, QStringLiteral("F1"), QString(), QString()};
    const SelectionItem circlesSketch{SelectKind::Sketch, QStringLiteral("F4"), QString(), QString()};
    const Selection whole = outlineStart(model, {plateSketch}).items(QStringLiteral("profiles"));
    CHECK(whole.size() == 2);
    CHECK(holds(whole, "F1", ring) && holds(whole, "F1", island));
    const CommandState crossing = outlineStart(model, {circlesSketch});
    CHECK(crossing.items(QStringLiteral("profiles")).isEmpty());
    CHECK(crossingNote(crossing, model).contains(QStringLiteral("cross")));
  }
  // A whole sketch picked while the command is open (Profiles' onPick):
  // its unused members after the entries there, none twice; a crossing
  // one adds nothing, and the note stays while Profiles hold what they
  // held then, not after another sketch's region is clicked.
  {
    QJsonArray rows = plate();
    rows.append(marked("F4", "r{c1[c2,c2]}", Mark::Crossing));
    const ProfileModel model(rows, QJsonObject(), QStringList());
    CommandState state;
    Selection& profiles = state.items(QStringLiteral("profiles"));
    profiles.append({SelectKind::Profile, QStringLiteral("F1"), QString::fromLatin1(ring), QStringLiteral("plane")});
    profiles.append({SelectKind::Sketch, QStringLiteral("F1"), QString(), QString()});
    mitcad::cmd::expandWholeSketches(state, model);
    CHECK(state.items(QStringLiteral("profiles")).size() == 2);
    CHECK(holds(state.items(QStringLiteral("profiles")), "F1", island));
    state.items(QStringLiteral("profiles")).append({SelectKind::Sketch, QStringLiteral("F4"), QString(), QString()});
    mitcad::cmd::expandWholeSketches(state, model);
    CHECK(state.items(QStringLiteral("profiles")).size() == 2);
    CHECK(mitcad::cmd::crossingSketch(state) == QStringLiteral("F4"));
    CHECK(!crossingNote(state, model).isEmpty());
    state.items(QStringLiteral("profiles"))
        .append({SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r{c9}"), QStringLiteral("plane")});
    CHECK(crossingNote(state, model).isEmpty());
    // The plate's sketch picked again: the note goes.
    state.items(QStringLiteral("profiles")).append({SelectKind::Sketch, QStringLiteral("F1"), QString(), QString()});
    mitcad::cmd::expandWholeSketches(state, model);
    CHECK(mitcad::cmd::crossingSketch(state).isEmpty());
  }
}

} // namespace

int profilePicksTests() {
  failures = 0;
  takeTests();
  hintTests();
  automaticTests();
  startTests();
  outlineTests();
  if (failures == 0) {
    std::printf("profile picks tests passed\n");
  }
  return failures;
}
MITCAD_UNIT_TEST(profilePicksTests);
