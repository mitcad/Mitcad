// SPDX-License-Identifier: MIT
#pragma once

// Save of a design that has no file yet (mitcad#2183): when the user has a
// remote project, Save asks whether to save the design on this computer
// (the file dialog) or to a project (the project picker of Save to
// Project), with "Remember my choice"; the answer remembered is the
// preference Preferences › General › Save of a new design (the setting
// documents/newDesignSave: ask, computer or project), which Ask puts back
// to the question. Save As, Save to Project and a design that has a file
// are not affected (MainWindow::saveNewDesign).

#include <optional>

#include <QString>

class QWidget;

namespace mitcad {

enum class NewDesignSave {
  Ask,      // the question, when a remote project is known
  Computer, // the file dialog, as Save As without a project
  Project,  // the project picker of Save to Project
};

// The stored ids: "ask", "computer", "project".
QString newDesignSaveId(NewDesignSave choice);
// The choice of a stored id; none for another text (an edited settings
// file).
std::optional<NewDesignSave> newDesignSaveFromId(const QString& id);
// The preference's text as Preferences shows it: Ask, On this computer,
// To a project.
QString newDesignSaveLabel(NewDesignSave choice);

// The answer to the question: where, and whether Remember my choice was
// checked.
struct NewDesignSaveAnswer {
  NewDesignSave choice = NewDesignSave::Computer;
  bool remember = false;
};

// The question (a Question message box, a sheet on macOS): Save on This
// Computer…, Save to Project… (the default) and Cancel, the check box
// Remember my choice off. `name` is the design's name. None when it is
// canceled (Cancel, Esc, the window closed).
std::optional<NewDesignSaveAnswer> askNewDesignSave(QWidget* parent, const QString& name);

} // namespace mitcad
