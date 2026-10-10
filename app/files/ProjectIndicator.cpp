// SPDX-License-Identifier: MIT
#include "ProjectIndicator.hpp"

#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDir>
#include <QMenu>
#include <QStringList>
#include <QtLogging>

#include "../framework/TestSync.hpp"

namespace mitcad {
namespace {

// "14:02" today, else "2026-10-05 14:02".
QString shortTime(const QDateTime& time) {
  const QDateTime local = time.toLocalTime();
  return local.date() == QDate::currentDate() ? local.toString(QStringLiteral("HH:mm"))
                                              : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

} // namespace

ProjectIndicator::ProjectIndicator(std::function<void(const QString& id)> trigger, QWidget* parent)
    : QToolButton(parent), m_trigger(std::move(trigger)) {
  setObjectName(QStringLiteral("projectIndicator"));
  setAutoRaise(true);
  setToolButtonStyle(Qt::ToolButtonTextOnly);
  setPopupMode(QToolButton::InstantPopup);
  setFocusPolicy(Qt::NoFocus);
  m_menu = new QMenu(this);
  setMenu(m_menu);
  connect(m_menu, &QMenu::aboutToShow, this, &ProjectIndicator::fillMenu);
  refresh();
}

void ProjectIndicator::setProject(const ProjectState& project) {
  if (project != m_project) {
    m_project = project;
    m_remote = Remote();
    refresh();
  }
}

void ProjectIndicator::setVersions(int versions) {
  if (versions != m_versions) {
    m_versions = versions;
    refresh();
  }
}

void ProjectIndicator::setRemote(const Remote& remote) {
  m_remote = remote;
  refresh();
}

void ProjectIndicator::setLockText(const QString& text) {
  m_lockText = text;
  refresh();
}

void ProjectIndicator::setLiveText(const QString& text) {
  m_liveText = text;
  refresh();
}

void ProjectIndicator::setLockActions(const QList<QAction*>& actions) {
  m_lockActions.clear();
  for (QAction* action : actions) {
    m_lockActions << action;
  }
}

void ProjectIndicator::refresh() {
  const QString dot = QStringLiteral(" ") + QChar(0x00B7) + QStringLiteral(" ");
  const QString name = m_project.name();
  QStringList shown;
  QStringList logged;
  QStringList tip;
  switch (m_project.kind) {
  case ProjectState::Kind::None:
    shown << tr("Not in a project");
    logged << QStringLiteral("not in a project");
    tip << tr("The design is in no project: its versions are not kept. Click for Move to a Project.");
    break;
  case ProjectState::Kind::NoHistory:
    if (m_project.outer.isEmpty()) {
      shown << name << tr("No versions");
      logged << name << QStringLiteral("no versions");
    } else {
      shown << tr("No versions: inside the git repository %1").arg(QDir::toNativeSeparators(m_project.outer));
      logged << name << QStringLiteral("no versions (inside %1)").arg(m_project.outer);
    }
    tip << tr("The project %1 keeps no versions: Mitcad records versions only in a repository of the project's "
              "own. Click for Move to a Project.")
               .arg(QDir::toNativeSeparators(m_project.root));
    break;
  case ProjectState::Kind::Local:
    if (!m_project.unfollowedRemotes.isEmpty()) {
      // Remotes, none followed (mitcad#89): Project Settings asks which.
      shown << name << tr("Cloud") << tr("No remote chosen");
      logged << name << QStringLiteral("cloud") << QStringLiteral("no remote chosen");
      tip << tr("The repository of %1 has the remotes %2 and follows none of them: choose one in Project Settings")
                 .arg(QDir::toNativeSeparators(m_project.root), m_project.unfollowedRemotes.join(QStringLiteral(", ")));
      break;
    }
    shown << name << tr("Local");
    logged << name << QStringLiteral("local");
    if (m_versions > 0) {
      shown << tr("v%1").arg(m_versions);
      logged << QStringLiteral("v%1").arg(m_versions);
    }
    tip << tr("Local project %1: versions on this computer").arg(QDir::toNativeSeparators(m_project.root));
    break;
  case ProjectState::Kind::Cloud: {
    shown << name << tr("Cloud");
    logged << name << QStringLiteral("cloud");
    tip << tr("Cloud project %1, shared through %2").arg(QDir::toNativeSeparators(m_project.root), m_project.remoteUrl);
    const Remote& remote = m_remote;
    const QString cls = remote.problem;
    const QString up = QString(QChar(0x2191));
    const QString down = QString(QChar(0x2193));
    if (!remote.busy.isEmpty()) {
      const QString verb = remote.busy == QLatin1String("sync")      ? tr("Syncing")
                           : remote.busy == QLatin1String("push")    ? tr("Sending")
                           : remote.busy == QLatin1String("connect") ? tr("Connecting")
                                                                     : tr("Checking");
      shown << (remote.percent >= 0 ? tr("%1 %2%").arg(verb).arg(remote.percent) : verb);
      logged << remote.busy;
    } else if (remote.conflict) {
      shown << tr("Conflict");
      logged << QStringLiteral("conflict");
      tip << tr("Files changed both here and on the remote: Sync again to choose what to keep");
    } else if (cls == QLatin1String("network") || cls == QLatin1String("timed_out")) {
      shown << (remote.ahead > 0 ? tr("Offline, %1%2 waiting").arg(up).arg(remote.ahead) : tr("Offline"));
      logged << QStringLiteral("offline");
      if (remote.ahead > 0) {
        logged << QStringLiteral("ahead %1").arg(remote.ahead);
      }
      tip << remote.problemMessage;
    } else if (cls == QLatin1String("auth_failed") || cls == QLatin1String("host_key_unknown")) {
      shown << tr("Sign-in needed");
      logged << QStringLiteral("sign-in needed");
      tip << remote.problemMessage;
    } else if (cls == QLatin1String("not_found")) {
      shown << (remote.ahead > 0 ? tr("Remote not found, %1%2 waiting").arg(up).arg(remote.ahead)
                                 : tr("Remote not found"));
      logged << QStringLiteral("remote not found");
      if (remote.ahead > 0) {
        logged << QStringLiteral("ahead %1").arg(remote.ahead);
      }
      tip << remote.problemMessage;
    } else if (cls == QLatin1String("git_missing")) {
      shown << tr("git missing");
      logged << QStringLiteral("git missing");
      tip << remote.problemMessage;
    } else {
      if (remote.ahead > 0) {
        shown << up + QString::number(remote.ahead);
        logged << QStringLiteral("ahead %1").arg(remote.ahead);
        tip << tr("%n version(s) not sent yet: each save sends them, or Sync", nullptr, remote.ahead);
      }
      if (remote.behind > 0) {
        shown << down + QString::number(remote.behind);
        logged << QStringLiteral("behind %1").arg(remote.behind);
        tip << tr("%n newer version(s) on the remote%1: Sync takes them", nullptr, remote.behind).arg(remote.newest);
      }
      if (remote.ahead == 0 && remote.behind == 0) {
        shown << QString(QChar(0x2713));
        logged << QStringLiteral("synced");
        tip << (remote.lastDone.isValid() ? tr("Up to date, last sync %1").arg(shortTime(remote.lastDone))
                                          : tr("Up to date"));
      } else if (remote.ahead < 0 || remote.behind < 0) {
        shown << tr("Not synced yet");
        logged << QStringLiteral("not synced");
      }
    }
    break;
  }
  }
  if (!m_liveText.isEmpty() && m_project.kind == ProjectState::Kind::Cloud) {
    shown << m_liveText;
    logged << m_liveText.toLower();
  }
  if (!m_lockText.isEmpty()) {
    shown << m_lockText;
    logged << m_lockText.toLower();
  }
  tip << tr("Click for the project's menu");
  setText(shown.join(dot));
  setToolTip(tip.join(QLatin1Char('\n')));
  const QString line = logged.join(QStringLiteral(", "));
  if (line != m_logged) {
    m_logged = line;
    qInfo().noquote() << QStringLiteral("Project indicator: %1").arg(line);
  }
  logPlace();
}

void ProjectIndicator::moveEvent(QMoveEvent* event) {
  QToolButton::moveEvent(event);
  logPlace();
}

void ProjectIndicator::resizeEvent(QResizeEvent* event) {
  QToolButton::resizeEvent(event);
  logPlace();
}

void ProjectIndicator::logPlace() {
  if (m_placePending) {
    return;
  }
  m_placePending = true;
  // Where to click once the layout has placed it (UI tests).
  TestSync::singleShot(50, this, [this] {
    m_placePending = false;
    if (!isVisible() || window() == nullptr) {
      return;
    }
    const QPoint center = mapTo(window(), rect().center());
    const QString place = QStringLiteral("%1,%2").arg(center.x()).arg(center.y());
    if (place != m_loggedPlace) {
      m_loggedPlace = place;
      qDebug().noquote() << QStringLiteral("Project indicator at %1").arg(place);
    }
  });
}

void ProjectIndicator::fillMenu() {
  m_menu->clear();
  const auto add = [this](const QString& text, const char* id) {
    QAction* action = m_menu->addAction(text);
    const QString command = QString::fromLatin1(id);
    connect(action, &QAction::triggered, this, [this, command] {
      qInfo().noquote() << QStringLiteral("Project indicator menu chose %1").arg(command);
      if (m_trigger) {
        m_trigger(command);
      }
    });
    action->setMenuRole(QAction::NoRole);
  };
  switch (m_project.kind) {
  case ProjectState::Kind::Cloud:
    add(tr("S&ync Now"), "file.sync");
    add(tr("&Check for Newer Versions"), "file.check_remote");
    add(tr("Version &History..."), "file.version_history");
    add(tr("Project &Settings..."), "file.project_settings");
    add(tr("Open in &Browser"), "file.remote_browser");
    break;
  case ProjectState::Kind::Local:
    add(tr("Version &History..."), "file.version_history");
    add(tr("Project &Settings..."), "file.project_settings");
    break;
  case ProjectState::Kind::None:
  case ProjectState::Kind::NoHistory:
    add(tr("&Move to a Project..."), "file.move_to_project");
    break;
  }
  bool separated = false;
  for (const QPointer<QAction>& action : std::as_const(m_lockActions)) {
    if (action != nullptr) {
      if (!separated) {
        m_menu->addSeparator();
        separated = true;
      }
      m_menu->addAction(action);
    }
  }
  QStringList entries;
  for (QAction* action : m_menu->actions()) {
    if (!action->isSeparator()) {
      entries << action->text().remove(QLatin1Char('&'));
    }
  }
  qInfo().noquote() << QStringLiteral("Project indicator menu: %1").arg(entries.join(QStringLiteral(" | ")));
}

} // namespace mitcad
