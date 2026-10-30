// SPDX-License-Identifier: MIT
#include "ProjectIndicator.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDir>
#include <QMenu>
#include <QPainter>
#include <QPainterPath>
#include <QStringList>
#include <QtLogging>

#include "../framework/TestSync.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/Theme.hpp"

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
  setAttribute(Qt::WA_Hover);
  m_menu = new QMenu(this);
  setMenu(m_menu);
  connect(m_menu, &QMenu::aboutToShow, this, &ProjectIndicator::fillMenu);
  refresh();
}

QSize ProjectIndicator::sizeHint() const {
  if (chromeStyle() != ChromeStyle::Floating) {
    return QToolButton::sizeHint();
  }
  return QSize(std::min(280, fontMetrics().horizontalAdvance(text()) + 42), 30);
}

void ProjectIndicator::paintEvent(QPaintEvent* event) {
  if (chromeStyle() != ChromeStyle::Floating) {
    QToolButton::paintEvent(event);
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  QColor fill = palette().color(QPalette::WindowText);
  fill.setAlpha(isDown() ? 35 : underMouse() ? 24 : 12);
  painter.setBrush(fill);
  painter.setPen(QPen(cardBorderColor(palette()), 1));
  painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), 10, 10);
  painter.setPen(palette().color(QPalette::WindowText));
  const QRect label = rect().adjusted(12, 0, -28, 0);
  painter.drawText(label, Qt::AlignVCenter | Qt::AlignLeft,
                   fontMetrics().elidedText(text(), Qt::ElideRight, label.width()));
  QPen pen(palette().color(QPalette::WindowText), 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
  painter.setPen(pen);
  const QPointF center(width() - 15, height() / 2.0);
  QPainterPath arrow;
  arrow.moveTo(center + QPointF(-3, -1.5));
  arrow.lineTo(center + QPointF(0, 1.5));
  arrow.lineTo(center + QPointF(3, -1.5));
  painter.drawPath(arrow);
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

void ProjectIndicator::setDebugLog(const QString& text, const QString& tip) {
  m_debugLogText = text;
  m_debugLogTip = tip;
  refresh();
}

void ProjectIndicator::setDebugLogActions(const QList<QAction*>& actions) {
  m_debugLogActions.clear();
  for (QAction* action : actions) {
    m_debugLogActions << action;
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
    tip << tr("The design is in no project: its versions are not kept. Click for Save to Project.");
    break;
  case ProjectState::Kind::NoHistory:
    if (m_project.outer.isEmpty()) {
      shown << name << tr("No versions");
      logged << name << QStringLiteral("no versions");
    } else {
      shown << tr("No versions: inside the git repository %1").arg(QDir::toNativeSeparators(m_project.outer));
      logged << name << QStringLiteral("no versions (inside %1)").arg(m_project.outer);
    }
    tip << tr("The project %1 keeps no versions: it has no repository of its own")
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
    } else if (cls == QLatin1String("untrusted_config")) {
      // The repository's own git configuration names programs (mitcad#920).
      shown << tr("Not synced: untrusted git settings");
      logged << QStringLiteral("untrusted git settings");
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
  if (!m_debugLogText.isEmpty()) {
    shown << m_debugLogText;
    logged << m_debugLogText.toLower();
    tip << m_debugLogTip;
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
  emit presentationChanged();
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
    add(tr("Version &History…"), "file.version_history");
    add(tr("Project &Settings…"), "file.project_settings");
    add(tr("Open in &Browser"), "file.remote_browser");
    add(tr("New &Design…"), "file.new_design"); // mitcad#145
    break;
  case ProjectState::Kind::Local:
    add(tr("Version &History…"), "file.version_history");
    add(tr("Project &Settings…"), "file.project_settings");
    add(tr("New &Design…"), "file.new_design"); // mitcad#145
    break;
  case ProjectState::Kind::None:
  case ProjectState::Kind::NoHistory:
    add(tr("&Save to Project…"), "file.save_to_project");
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
  separated = false;
  for (const QPointer<QAction>& action : std::as_const(m_debugLogActions)) {
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
