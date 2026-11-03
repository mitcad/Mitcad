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
#include "commands/ProfilePicks.hpp"

#include <cstdio>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "profile_picks.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::SelectionItem;
using mitcad::SelectKind;
using mitcad::cmd::automaticProfile;
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

} // namespace

int profilePicksTests() {
  failures = 0;
  takeTests();
  hintTests();
  automaticTests();
  if (failures == 0) {
    std::printf("profile picks tests passed\n");
  }
  return failures;
}
