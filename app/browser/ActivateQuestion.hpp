// SPDX-License-Identifier: MIT
#pragma once

// Create Sketch and the Construct commands on geometry of a component
// other than the active one (mitcad#960, app/COMMANDS.md "Activating the
// picked component"): right after the pick, a question whether to activate
// that component, so that the sketch or the construction feature goes there
// rather than into the active component. Asked for a face, plane, edge or
// point of another component's body or datum, also of an occurrence placed
// in the active component; never for the active component's own geometry
// (any occurrence of it) or its origin planes, axes and point, nor for a
// pick the design policy refuses. A preference turns it off.

#include <QString>

#include "../framework/Selection.hpp"
#include "DocumentSnapshot.hpp"

class QWidget;

namespace mitcad {

// The preference (settings "general/askActivateComponent", Preferences >
// General "Ask to activate the component of a picked face"), on by default;
// the question's Don't ask again turns it off. With no saved value it is
// off when MITCAD_ASK_ACTIVATE_COMPONENT is 0: the UI test libraries set
// that, so that their sketches on other components' faces stay as they were
// (a test of the question sets it to 1).
bool askToActivateComponent();
void saveAskToActivateComponent(bool on);

// What a pick asks, worked out from the document's structure.
struct ActivateQuestion {
  bool ask = false;
  // The component the picked geometry belongs to, and the occurrence it
  // was picked at ("O1/O4"; empty for the root): the one activated, and
  // where the command goes on with the same pick.
  QString component;
  QString occurrence;
  // Their names as the browser shows them: the picked occurrence's
  // ("M6 rod bracket:1"; the root's component name) and the active
  // component's, by the occurrence the pick lies under when there is one,
  // else its first ("Corner brace:1"; the root's component name).
  QString picked;
  QString active;
  // What was picked, in words: "face", "plane", "edge", ...
  QString kind;

  // "The face belongs to M6 rod bracket:1, but Corner brace:1 is active."
  QString text() const;
};

// Whether `item` asks (ActivateQuestion::ask) and with what names.
// `refused`: the design policy's reason the pick is refused (Create
// Sketch's sketch_supports), empty when it is not; a refused pick asks
// nothing (it is refused as before). `enabled`: the preference.
ActivateQuestion activateQuestion(const DocumentSnapshot& snapshot, const SelectionItem& item,
                                  const QString& refused, bool enabled);

// The user's answer: Activate <picked> (the default button) or Keep in
// <active> (also Escape), and the Don't ask again box.
struct ActivateAnswer {
  bool activate = false;
  bool dontAskAgain = false;
};

// The question as a modal message box ("Activate Component"), shown as the
// application's other questions are (framework/Dialogs.hpp: a sheet on
// macOS, the native dialog in the AppKit frontend).
ActivateAnswer askToActivate(QWidget* parent, const ActivateQuestion& question);

} // namespace mitcad
