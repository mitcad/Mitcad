// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

struct VersionSettings;

// The dialogs of saving versions (P12d): the author of an older project
// without one, a version's description and a file changed outside Mitcad;
// and Version History's Restore (P12e). New Project and the other dialogs
// of projects are in files/ProjectDialogs.hpp (mitcad#89).

// Who records the versions of a project that has no author (an older
// project: neither its repository, git nor Preferences name one): a name
// and an email address, the defaults `settings` offers. Nothing when
// cancelled.
std::optional<VersionSettings> askVersionAuthor(QWidget* parent, const VersionSettings& settings);

// File > Save Version: the description of the version (`automatic`: the
// message Save would write). Empty when the description is left empty.
std::optional<QString> askVersionDescription(QWidget* parent, const QString& file, const QString& automatic);

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
