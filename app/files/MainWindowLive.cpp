// SPDX-License-Identifier: MIT
// Live updates through an MQTT broker (mitcad#89): the live controller
// (files/LiveController.hpp) with the window's ends: the current project
// and its open design, the indicator's live text, the remote's checks and
// the versions it sends. The lock controller meets it through
// files/LiveLink.hpp.
#include "../MainWindow.hpp"

#include <utility>

#include <QtLogging>

#include "../view/ViewController.hpp"
#include "Autosave.hpp"
#include "LiveController.hpp"
#include "LockController.hpp"
#include "ProjectIndicator.hpp"
#include "RemoteController.hpp"

namespace mitcad {

void MainWindow::createLive() {
  LiveController::Host host;
  host.session = [this] { return m_autosave->session(); };
  host.status = [this](const QString& message, bool error) {
    if (error) {
      showError(message);
    } else {
      showHint(message);
    }
  };
  m_live = new LiveController(*this, std::move(host), this);
  // The indicator's live text and the remote's timer follow the current
  // project's connection.
  connect(m_live, &LiveController::stateChanged, this, [this](const QString& root) {
    if (!m_project.hasHistory() || root != m_project.root) {
      return;
    }
    m_indicator->setLiveText(m_live->liveText(root));
    m_remote->setLive(m_live->liveConnected(root));
  });
  // Another session sent a version: the remote is checked at once (the
  // indicator's newer versions and the notice of a newer one follow).
  connect(m_live, &LiveController::versionAnnounced, this, [this](const QString& root) {
    if (m_project.hasHistory() && root == m_project.root) {
      m_remote->versionAnnounced();
    }
  });
  // What this session sends, others learn at once.
  connect(m_remote, &RemoteController::versionsSent, m_live,
          [this](const QString& root, const QString& branch, const QString& commit) {
            m_live->publishVersion(root, branch, commit);
          });
  // Edit locks (files/LockController.hpp): what others should learn at once
  // goes through the live controller, whose events come back to the lock
  // controller, which publishes who has the design open from then on.
  m_locks->setLiveLink(m_live);
  m_live->setLockEvents(m_locks);
  // Preferences (live updates allowed, Known brokers).
  std::function<void()> previous = m_viewController->generalChanged;
  m_viewController->generalChanged = [this, previous] {
    if (previous) {
      previous();
    }
    m_live->preferencesChanged();
  };
}

void MainWindow::updateLive() {
  if (m_live == nullptr) {
    return;
  }
  m_live->setProject(m_project);
  m_live->setDesign(m_filePath, m_openingReadOnly);
  const QString root = m_project.kind == ProjectState::Kind::Cloud ? m_project.root : QString();
  m_indicator->setLiveText(m_live->liveText(root));
  m_remote->setLive(m_live->liveConnected(root));
}

} // namespace mitcad
