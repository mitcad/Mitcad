// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

struct VersionSettings;

// The dialogs of saving versions (P12d): a new project's folder, the
// author of versions, a version's description, how a file outside
// projects gets version history, and a file changed outside Mitcad; and
// Version History's Restore (P12e).

// File > New Project (also when a design moves into a new project): the
// project's name and the folder it goes in (`location`, by default
// Documents/Mitcad). Returns the project's folder, a new one or an empty
// one.
std::optional<QString> askNewProject(QWidget* parent, const QString& title, const QString& name,
                                     const QString& location);

// Who versions are recorded by, shown when the first version is saved and
// asked when neither git nor the settings name one. `gitName` and
// `gitEmail`: git's configured user, empty when it has none. Returns the
// settings chosen (confirmed), or nothing when cancelled.
std::optional<VersionSettings> askVersionAuthor(QWidget* parent, const QString& gitName, const QString& gitEmail,
                                                const VersionSettings& settings);

// File > Save Version: the description of the version (`automatic`: the
// message Save would write). Empty when the description is left empty.
std::optional<QString> askVersionDescription(QWidget* parent, const QString& file, const QString& automatic);

// File > Start Version History for a file outside projects: its folder made
// a project with version history, or the file moved into a new project.
enum class HistoryStart { UseFolder, MoveToProject };
// `outer`: the git repository the folder is in below its root (the folder
// cannot then be a project of its own), or empty; `repository`: the folder
// is the root of a git repository already.
std::optional<HistoryStart> askStartHistory(QWidget* parent, const QString& file, const QString& folder,
                                            const QString& outer, bool repository);

// Save over a file that changed outside Mitcad since it was opened or saved
// (`newerVersion`: the project's history has another version of it; else
// the file in the folder changed).
enum class ExternalChange { NewVersion, SaveAs, Compare };
std::optional<ExternalChange> askExternalChange(QWidget* parent, const QString& file, bool newerVersion,
                                                bool unrecorded);

// A comparison's text (the model's diff) in a window of its own.
void showComparison(QWidget* parent, const QString& title, const QString& text);

// Restore in Version History (P12e): `version` ("v3 (abc1234)", saved
// `when`) of `file` becomes its latest version, number `next`. With
// `modified` (the open design has unsaved changes) the choice is to save
// them as a version first (true) or to drop them (false); without, true.
// Nothing when cancelled.
std::optional<bool> askRestoreVersion(QWidget* parent, const QString& file, const QString& version,
                                      const QString& when, int next, bool modified);

// The message of a version Save makes: "Save part.mitcad: Add Extrude2,
// Change d3", from the undo steps since the last save (`steps`), shortened
// to a summary line with the whole list below it when they are many.
QString automaticVersionMessage(const QString& file, const QStringList& steps);

} // namespace mitcad
