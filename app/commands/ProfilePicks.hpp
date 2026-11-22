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

// The outline set of a sketch (mitcad#2194, core/model-api commands.md
// "The outline set"): the regions its own curves bound at an even nesting
// depth, a plate less its holes, as the model's `profiles` query marks them
// (`outline`), or none when its closed curves cross (`curves_cross`).
struct OutlinePick {
  // Its unused members, as Profiles entries of the sketch.
  Selection items;
  // The sketch's closed curves cross: it has no outline set.
  bool crossing = false;
};

// The outline pick of `sketch` in `profiles`, the model's `profiles` query;
// the entries name `occurrence` (where the sketch is shown).
OutlinePick sketchOutline(const QJsonArray& profiles, const QString& sketch,
                          const QString& occurrence = QString());

// The state's text (CommandState::text) that names the sketch whose closed
// curves cross, set when Extrude or Revolve started on it or a whole sketch
// so was picked for Profiles (mitcad#2194); empty otherwise.
QString crossingSketch(const CommandState& state);

// The `init` of Extrude and Revolve (mitcad#2194): a whole sketch selected
// before the command becomes its outline pick (expandWholeSketches); an
// empty Profiles takes the unused members of the outline set of the sketch
// newestProfile would take a region of, else newestProfile's region (the
// set all used, or the sketch's closed curves cross, which the panel then
// says: crossingNote). A QR code's region keeps newestProfile's rule.
void newestOutline(CommandState& state, const CommandContext& context);

// Each whole sketch (SelectKind::Sketch) in Profiles, picked on its Browser
// row or its timeline cell, replaced by the unused members of its outline
// set, after the entries already there; for a sketch whose closed curves
// cross, by nothing, and crossingSketch names it. Profiles' onPick and
// newestOutline's first step.
void expandWholeSketches(CommandState& state, const CommandContext& context);

// The panel's words while Profiles hold nothing but regions of a sketch
// whose closed curves cross (crossingSketch), or nothing at all: that its
// areas are to be clicked. Empty otherwise. Extrude's and Revolve's note
// and the empty Profiles' message.
QString crossingNote(const CommandState& state, const CommandContext& context);

// Whether a row of the `profiles` query is drawn without fill by Show
// profiles (mitcad#2194): a region the sketch's own curves bound outside
// its outline set (a hole's disc), of a sketch whose closed curves do not
// cross.
bool unfilledProfile(const QJsonObject& row);

} // namespace mitcad::cmd
