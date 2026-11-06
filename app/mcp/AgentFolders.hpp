// SPDX-License-Identifier: MIT
#pragma once

// The folders for designs, which an agent connected through live MCP saves
// designs to and opens them from (mitcad#402): live mode's equivalent of headless mode's
// --workspace. None by default: the user adds them in Preferences > AI
// Assistants (the setting mcp/folders), or for one run with --mcp-folder DIR. A path
// is inside one when its folder, with symbolic links resolved, is that
// folder or below it; '..' is refused, and a relative path is relative to
// the first folder.

#include <QGroupBox>
#include <QString>
#include <QStringList>
#include <QWidget>

class QListWidget;
class QPushButton;
class QShowEvent;

namespace mitcad::mcp {

// The setting mcp/folders: absolute, cleaned, without duplicates.
QStringList savedAgentFolders();
void saveAgentFolders(const QStringList& folders);
// Folders allowed for this run only (--mcp-folder), besides the setting.
void setRunAgentFolders(const QStringList& folders);
// Every allowed folder: the setting's, then this run's.
QStringList agentFolders();

// A design file (.mitcad) in an allowed folder, as an absolute path with
// its folder's links resolved; empty with the reason when it is not one.
// `forWriting`: the folder must exist, an existing file must be a regular
// file (not a link out of the folder) and needs `overwrite`; otherwise the
// file must exist.
QString agentDesignPath(const QString& given, bool forWriting, bool overwrite, QString& reason);
// A file an agent writes in an allowed folder (mitcad#689: model_export,
// ui_screenshot's path), by the rules of agentDesignPath's `forWriting`:
// a new file, or with `overwrite` an existing regular one; with one of
// `suffixes` (any when empty). Empty with the reason when it is not one.
QString agentOutputPath(const QString& given, bool overwrite, QString& reason, const QStringList& suffixes = {});
// Where document_new makes a design (mitcad#1266). agentDesignFolder: an
// existing folder that is an allowed one or below one (a relative `given`
// relative to the first; '..' refused), with its links resolved; empty with
// the reason when it is not one. agentNewDesignPath: the design's file in
// such a folder, `name` with .mitcad added when it lacks it, or with an
// empty `name` the first free DesignN.mitcad (names compared without case);
// empty with the reason, which names the file, when the name is not a plain
// file name or that file exists.
QString agentDesignFolder(const QString& given, QString& reason);
QString agentNewDesignPath(const QString& folder, const QString& name, QString& reason);

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none: a plain widget.
#ifdef MITCAD_MAC_UI
using AgentPreferencesBase = QWidget;
#else
using AgentPreferencesBase = QGroupBox;
#endif

// The AI Assistants group of Preferences: the list of allowed folders.
class AgentPreferencesBox : public AgentPreferencesBase {
  Q_OBJECT

public:
  explicit AgentPreferencesBox(QWidget* parent = nullptr);

  // The folders as the box shows them, to save when Preferences is accepted.
  QStringList chosen() const;
  void includeSavedFolders();

signals:
  void changed();
  void connectRequested();

protected:
  void showEvent(QShowEvent* event) override;

private:
  void addFolder(const QString& folder);

  QPushButton* m_connect = nullptr;
  QListWidget* m_folders = nullptr;
  QPushButton* m_remove = nullptr;
};

} // namespace mitcad::mcp
