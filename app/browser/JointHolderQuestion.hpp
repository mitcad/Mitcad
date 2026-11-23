// SPDX-License-Identifier: MIT
#pragma once

// Joint, As-Built Joint, Rigid Group and Constraint Set on picks that
// lie under another component than the active one (mitcad#1856,
// app/COMMANDS.md "The component that holds the picks"): the feature
// belongs in the lowest component that holds every pick (the model's
// `joint_holder` query). When that is not the active component, a question
// offers to make it there, the active component staying active; its Don't
// ask again stores the answer as a preference that Preferences > Assembly
// sets back to Ask.

#include <QJsonObject>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

// The preference (settings "assembly/jointComponent", Preferences >
// Assembly "When the picks are in another component than the active one"):
// Ask (the default), Use the component that holds both, or Keep the active
// component.
enum class JointHolderChoice { Ask, Holder, Active };

JointHolderChoice jointHolderPreference();
void saveJointHolderPreference(JointHolderChoice choice);
// The value saved ("ask", "holder", "active") and the choice's text as
// Preferences shows it.
QString jointHolderChoiceId(JointHolderChoice choice);
QString jointHolderChoiceText(JointHolderChoice choice);
// A saved value's choice: Ask for anything unknown.
JointHolderChoice jointHolderChoiceOf(const QString& id);

// The inputs whose picks decide where a command's feature goes: Joint's and
// As-Built Joint's "a" and "b", Rigid Group's "occurrences", Assembly
// Constraint's rows' "a", "b" and "b2"; none for other commands. A motion
// origin (As-Built Joint's) is not one.
QStringList jointHolderInputs(const QString& command);

// What the panel does once the `joint_holder` query answered for its picks,
// the same for a pick made in the panel (mitcad#1856) and for the selection
// it opens with (mitcad#1864):
//   None:       nothing changes (the active component holds them, the
//               component was answered for already, or an agent's request
//               is not held by a question);
//   Clear:      the component chosen before no longer holds the picks
//               (they are the active component's again, or no component
//               joins them): the choice and its notice go;
//   Ask:        the question;
//   UseHolder:  the preference makes it in the component that holds them;
//   KeepActive: the preference keeps the active component.
enum class JointHolderStep { None, Clear, Ask, UseHolder, KeepActive };
// `held` and `kept` are the panel's hidden inputs "holder" and "kept" (the
// component chosen, or declined, before); `agent` is true while an agent's
// request (MCP) picks.
JointHolderStep jointHolderStep(const QJsonObject& answer, const QString& held, const QString& kept,
                                JointHolderChoice preference, bool agent);

// What the question says, from the `joint_holder` query's answer.
struct JointHolderQuestion {
  // The component that holds the picks (its uid and name) and the active
  // one's name.
  QString component;
  QString holder;
  QString active;
  // The components the picks lie in under the holder, by name, in the
  // order picked, each once; the picks of components of one name by their
  // occurrences' names instead ("shaft:1", "shaft:2").
  QStringList picked;
  // How many picks there were (two: "both").
  int picks = 0;
  // Where the active component stands to the holder: "inside", "above"
  // or "apart" (the query's `relation`).
  QString relation;
  // With "inside": the component that a feature made in the holder moves
  // as a whole.
  QString whole;
  // What the command makes, as its sentence starts: "A joint" (made by the
  // caller, madeWhat).
  QString made;
  // Whether it is a rigid group, which joins rather than moves.
  bool joins = false;

  // "The two picks are in Frame and in Shaft Roller. A joint made in Shaft
  // Roller can move only Shaft Roller's own parts. Make it in Assembly,
  // which holds both, so that Shaft Roller moves as a whole?"
  QString text() const;
  // "Make it in Assembly", "Keep Shaft Roller" (with their mnemonics).
  QString makeButton() const;
  QString keepButton() const;
  // The panel's notice once the holder is chosen: "Joint goes into
  // Assembly, which holds both picks; Shaft Roller stays active."
  QString notice(const QString& command) const;
};

// "A joint", "An as-built joint", "A rigid group", "A constraint" by the
// command's id; empty for another command.
QString madeWhat(const QString& command);

JointHolderQuestion jointHolderQuestion(const QJsonObject& answer, const QString& command);

// The user's answer: Make it in <holder> (the default button) or Keep
// <active> (also Escape), and the Don't ask again box.
struct JointHolderAnswer {
  bool useHolder = false;
  bool dontAskAgain = false;
};

// The question as a modal message box ("Component for the Joint"), shown
// as the application's other questions are (framework/Dialogs.hpp).
JointHolderAnswer askJointHolder(QWidget* parent, const JointHolderQuestion& question, const QString& command);

} // namespace mitcad
