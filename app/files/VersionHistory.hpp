// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <vector>

#include <QDateTime>
#include <QDialog>
#include <QJsonObject>
#include <QString>

class QLabel;
class QPlainTextEdit;
class QPushButton;
class QRadioButton;
class QThread;
class QTreeWidget;

namespace mitcad {

// One version of a project file: an entry of the version history's
// `history` command (core/model/src/api/commands.md, "Version history").
struct FileVersion {
  int number = 0; // v1 is the oldest
  QString id;
  QString shortId;
  QDateTime time; // local
  QString authorName;
  QString authorEmail;
  QString summary; // the message's first line
  QString message;
  QString path; // in that version, relative to the project
  QString renamedFrom;
  QString blob; // empty: the version deleted the file

  bool deleted() const { return blob.isEmpty(); }
  // "v3"; "v3 (abc1234)".
  QString label() const;
  QString labelWithId() const;
};

// The versions of a `history` answer, newest first, numbered.
std::vector<FileVersion> fileVersions(const QJsonObject& history);

// Where a version's preview is kept: a PNG named by the project file's blob
// id (the same content, the same picture) in "thumbnails" of the user's
// local application data (MITCAD_THUMBNAIL_DIR for tests), never in a
// project. Empty when the system gives no place.
QString versionThumbnailPath(const QString& blob);

// What the window asks the main window to do (files/MainWindowVersions.cpp).
struct VersionHistoryHost {
  // The comparison of a version with the open design, as text: nothing is
  // computed.
  std::function<QString(const FileVersion& version)> compareWithOpen;
  // The comparison with the bodies' volumes and areas: `version` and
  // `before` computed as a job, or `version` and the open design (`before`
  // null). Nothing when it was cancelled.
  std::function<std::optional<QString>(const FileVersion* before, const FileVersion& version)> compareGeometry;
  // Save Copy As of a version: asks where (over `parent`), writes it there.
  std::function<void(QWidget* parent, const FileVersion& version)> saveCopy;
};

// File > Version History (P12e): the versions of the open project file,
// newest first (when, by whom, the description, what changed against the
// version before, the short id), with the selected version's details, its
// preview, and its comparison with the version before or with the open
// design (parameters, timeline, sketches; the geometry on request,
// computed as a job). Open and Restore end the window with the choice;
// Save Copy As keeps it open. The history is read on a thread of its own:
// the list first, then what each version changed.
class VersionHistoryDialog : public QDialog {
  Q_OBJECT

public:
  enum class Choice { None, Open, Restore };

  // `file`: the open project file (absolute) in a project with history;
  // `modified`: the open design has unsaved changes.
  VersionHistoryDialog(QWidget* parent, const QString& file, bool modified, VersionHistoryHost host);
  ~VersionHistoryDialog() override;

  Choice choice() const { return m_choice; }
  // The version chosen to open or restore.
  const FileVersion& chosen() const { return m_chosen; }
  // The number of versions (the latest's number).
  int versionCount() const { return static_cast<int>(m_versions.size()); }

private:
  void showVersions(const QJsonObject& history);
  void showChanges(const QJsonObject& history);
  void showLoadError(const QString& error);
  void selected();
  void compare();
  void compareGeometry();
  void finish(Choice choice);
  const FileVersion* current() const;
  // The version before `version` in the list (older), or null.
  const FileVersion* before(const FileVersion& version) const;
  void showComparison(const QString& heading, const QString& text);

  QString m_file;
  QString m_name;
  bool m_modified = false;
  VersionHistoryHost m_host;
  std::vector<FileVersion> m_versions;
  std::vector<QString> m_changes; // by row; what changed against the version before
  QThread* m_loader = nullptr;
  Choice m_choice = Choice::None;
  FileVersion m_chosen;

  QLabel* m_heading = nullptr;
  QTreeWidget* m_list = nullptr;
  QPlainTextEdit* m_details = nullptr;
  QLabel* m_preview = nullptr;
  QRadioButton* m_withBefore = nullptr;
  QRadioButton* m_withOpen = nullptr;
  QPushButton* m_geometry = nullptr;
  QPlainTextEdit* m_comparison = nullptr;
  QPushButton* m_open = nullptr;
  QPushButton* m_restore = nullptr;
  QPushButton* m_copy = nullptr;
};

} // namespace mitcad
