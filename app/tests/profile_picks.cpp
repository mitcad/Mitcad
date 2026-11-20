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

} // namespace

int profilePicksTests() {
  failures = 0;
  takeTests();
  hintTests();
  automaticTests();
  startTests();
  if (failures == 0) {
    std::printf("profile picks tests passed\n");
  }
  return failures;
}
MITCAD_UNIT_TEST(profilePicksTests);
