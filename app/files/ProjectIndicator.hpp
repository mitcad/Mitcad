// SPDX-License-Identifier: MIT
#pragma once

// The project indicator (mitcad#89, section 8): the current project, its
// kind and its state in one button, where the status bar's version was in
// the docked layout and beside the document's name in the title bar row of
// the floating one. Its parts are separated by middle dots:
//
//   Robot arm . Local . v14
//   Robot arm . Cloud . (a check mark) . Live   (up arrow 2, down arrow 1,
//                                                Offline, 2 waiting, ...)
//   ... . Editing / Read-only (Alex)            (the open design's edit lock)
//   Not in a project
//
// A click opens its menu: Sync Now, Check for Newer Versions, Version
// History..., Project Settings..., Open in Browser; for a loose file Move to
// a Project.... The lock controller (a later change) adds the lock's text,
// the live updates' state and its own entries through the hooks below.

#include <functional>

#include <QDateTime>
#include <QList>
#include <QPointer>
#include <QString>
#include <QToolButton>

#include "Projects.hpp"

class QAction;
class QMenu;

namespace mitcad {

class ProjectIndicator : public QToolButton {
  Q_OBJECT

public:
  // What the indicator knows of a Cloud project's remote
  // (RemoteController's state).
  struct Remote {
    int ahead = -1; // -1: not known yet
    int behind = -1;
    QString problem; // the class of the latest failure, "" none
    QString problemMessage;
    QString busy;    // a running task ("sync", "push", "fetch", ...), "" none
    int percent = -1;
    bool conflict = false;
    QDateTime lastDone;
    QString newest; // " (the newest by Name, 14:02)"
  };

  // `trigger` runs a command of the registry by its id (file.sync, ...).
  ProjectIndicator(std::function<void(const QString& id)> trigger, QWidget* parent = nullptr);

  void setProject(const ProjectState& project);
  // The open design's versions (its history's length), 0 when it has none.
  void setVersions(int versions);
  void setRemote(const Remote& remote);
  // Hooks of the lock controller: the open design's lock ("Editing",
  // "Read-only (Alex)"; "" none), live updates' state ("Live", "Live offline
  // (polling)"; "" none), and its entries at the end of the menu (with a
  // lock: Release Edit Lock, the lock's details).
  void setLockText(const QString& text);
  void setLiveText(const QString& text);
  void setLockActions(const QList<QAction*>& actions);

  const ProjectState& project() const { return m_project; }
  // The text shown, and the one logged ("bracket, cloud, synced").
  QString shownText() const { return text(); }
  const QString& loggedText() const { return m_logged; }

protected:
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  void refresh();
  void fillMenu();
  void logPlace();

  std::function<void(const QString& id)> m_trigger;
  ProjectState m_project;
  int m_versions = 0;
  Remote m_remote;
  QString m_lockText;
  QString m_liveText;
  QList<QPointer<QAction>> m_lockActions;
  QMenu* m_menu = nullptr;
  QString m_logged;
  QString m_loggedPlace;
  bool m_placePending = false;
};

} // namespace mitcad
