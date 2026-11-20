// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QJsonArray>
#include <QString>

#include "../framework/Selection.hpp"

namespace mitcad {
class CommandContext;
class CommandState;
} // namespace mitcad

namespace mitcad::cmd {

// What Extrude's and Revolve's Profiles take with a click (mitcad#868).
// They hold profiles or curves of sketches, or one planar face of a body
// alone (mitcad#535, mitcad#543): a click that would mix the two, or add a
// second face, is not taken, and the status bar says what to click
// instead. Taken, such a click made the panel's error "Select one planar
// face, or profiles of one sketch, not both." while the click most often
// meant another input: the face an extrusion starts at (Start: Object)
// while Profiles was still the active input.

// Whether a click on `item` goes into Profiles that hold `held`: a face
// only when they hold no profile or curve and no other face (the face they
// hold is taken, so that its click removes it); a profile or curve only
// when they hold no face.
bool profilesTake(const SelectionItem& item, const Selection& held);

enum class ProfilesCommand { Extrude, Revolve };

// The status bar's text for a click on `item` that profilesTake refuses,
// or an empty text for one it takes or one refused for another reason (a
// face that is not `planar`). `start` is Extrude's Start choice
// (`profile_plane`, `offset`, `object`): a planar face refused beside
// sketch profiles is most likely the plane to start at.
QString profilesRefused(const SelectionItem& item, const Selection& held, bool planar, ProfilesCommand command,
                        const QString& start = QString());

// The profile that Extrude and the other profile commands take by
// themselves when they start (mitcad#1226), of `profiles`, the model's
// `profiles` query (the newest sketch's last). Of the newest sketch with an
// unused profile, the last unused one that the sketch's own curves bound;
// one that the face the sketch lies on bounds in part (`face_bounded`: the
// face's leftover region around the drawn shapes) only when the sketch has
// no other unused profile. None when every profile is used.
std::optional<SelectionItem> automaticProfile(const QJsonArray& profiles);

// The `init` of the profile commands (Extrude, Revolve, Sweep, Helix,
// Emboss, the sheet and surface commands): an empty Profiles takes
// automaticProfile's pick of the model's `profiles` query; a region of a
// QR code brings the code's other dark regions, so that the code extrudes
// whole (mitcad#473). A region of a 3D sketch (mitcad#1550) is taken alone
// (mitcad#2087): the planar `sketch` query, which refuses a 3D sketch, is
// asked only for a planar sketch.
void newestProfile(CommandState& state, const CommandContext& context);

} // namespace mitcad::cmd
