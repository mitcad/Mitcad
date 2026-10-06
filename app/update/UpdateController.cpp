// SPDX-License-Identifier: MIT
#include "UpdateController.hpp"

#include <cstdint>
#include <exception>
#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDateTime>
#include <QDesktopServices>
#include <QFile>
#include <QMainWindow>
#include <QTimer>
#include <QToolBar>
#include <QUrl>
#include <QtLogging>

#include "UpdateNotice.hpp"
#include "UpdateSettings.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Json.hpp"
#include "mitcad_bridge/update.h"

namespace mitcad {
namespace {

// The automatic check waits this long after start-up, so that it never
// competes with opening a design.
constexpr int kStartDelayMs = 3000;
constexpr qint64 kDaySeconds = 24 * 60 * 60;

} // namespace

UpdateController::UpdateController(QMainWindow& window, Host host, QObject* parent)
    : QObject(parent), m_window(window), m_host(std::move(host)), m_installation(Installation::detect()) {}

void UpdateController::registerCommands(CommandRegistry& registry) {
  CommandDef check;
  check.id = QStringLiteral("help.check_updates");
  check.name = tr("Check for Updates...");
  check.icon = QStringLiteral("update");
  check.kind = CommandDef::Kind::Action;
  check.mode = CommandDef::Mode::Any;
  check.tab.clear();
  check.tooltip = tr("Asks whether a newer Mitcad is out (Tools > Preferences: automatic checks)");
  check.keywords = {QStringLiteral("update"), QStringLiteral("new version"), QStringLiteral("release"),
                    QStringLiteral("upgrade")};
  check.duringCommands = true;
  check.run = [this] { checkNow(); };
  registry.add(check);
}

void UpdateController::startUp() {
  m_bar = new QToolBar(tr("Update"), &m_window);
  m_bar->setObjectName(QStringLiteral("updateBar"));
  m_bar->setMovable(false);
  m_bar->setFloatable(false);
  m_bar->toggleViewAction()->setVisible(false);
  m_notice = new UpdateNotice(m_bar);
  m_bar->addWidget(m_notice);
  m_window.addToolBarBreak(Qt::TopToolBarArea);
  m_window.addToolBar(Qt::TopToolBarArea, m_bar);
  m_bar->hide();
  connect(m_notice, &UpdateNotice::releaseNotesClicked, this, [this] {
    qInfo().noquote() << "Update: release notes at" << m_offer.notes;
    QDesktopServices::openUrl(QUrl(m_offer.notes));
  });
  connect(m_notice, &UpdateNotice::installClicked, this, &UpdateController::install);
  connect(m_notice, &UpdateNotice::skipClicked, this, [this] {
    UpdateSettings settings = UpdateSettings::load();
    settings.skippedVersion = m_offer.version;
    settings.save();
    qInfo().noquote() << "Update: skipping Mitcad" << m_offer.version;
    hideNotice();
  });
  connect(m_notice, &UpdateNotice::laterClicked, this, [] { qInfo() << "Update: later"; });
  connect(m_notice, &UpdateNotice::cancelClicked, this, [this] {
    m_client->cancel();
    m_state = State::Idle;
    qInfo() << "Update: cancelled";
    hideNotice();
  });
  connect(m_notice, &UpdateNotice::closed, this, &UpdateController::hideNotice);

  const UpdateSource source = UpdateSource::fromEnvironment();
  m_client = new UpdateClient(source, this);
  connect(m_client, &UpdateClient::checked, this, &UpdateController::onChecked);
  connect(m_client, &UpdateClient::failed, this, &UpdateController::onFailed);
  connect(m_client, &UpdateClient::progress, this, [this](qint64 received, qint64 total) {
    if (m_state == State::Downloading) {
      m_notice->showProgress(tr("Downloading Mitcad %1...").arg(m_offer.version), received, total);
    }
  });
  connect(m_client, &UpdateClient::downloaded, this, &UpdateController::onDownloaded);
  qInfo().noquote() << QStringLiteral("Updates: Mitcad %1 for %2, %3")
                           .arg(source.currentVersion,
                                source.platform.isEmpty() ? QStringLiteral("an unknown platform") : source.platform,
                                m_installation.describe());

  if (const std::optional<FinishedUpdate> finished = takeFinishedUpdate(source.currentVersion)) {
    if (finished->installed) {
      qInfo().noquote() << QStringLiteral("Update installed: %1 -> %2").arg(finished->from, finished->to);
      m_notice->showMessage(tr("Mitcad was updated to %1.").arg(finished->to));
    } else {
      qWarning().noquote() << QStringLiteral("Update not installed: %1 -> %2 (%3)")
                                  .arg(finished->from, finished->to, finished->detail);
      m_notice->showMessage(finished->detail.isEmpty()
                                ? tr("The update to Mitcad %1 was not installed.").arg(finished->to)
                                : tr("The update to Mitcad %1 was not installed: %2").arg(finished->to, finished->detail),
                            true);
    }
    showNotice();
  }
  m_installation.removeLeftovers();

  if (updatesDisabledByAdministrator()) {
    qInfo() << "Update checks are turned off by the administrator";
    return;
  }
  const UpdateSettings settings = UpdateSettings::load();
  if (!settings.automatic) {
    qInfo() << "Automatic update checks are off";
    return;
  }
  const QDateTime now = QDateTime::currentDateTimeUtc();
  if (settings.lastCheck.isValid() && settings.lastCheck <= now && settings.lastCheck.secsTo(now) < kDaySeconds) {
    qInfo().noquote() << "Update check: the last was at" << settings.lastCheck.toString(Qt::ISODate);
    return;
  }
  QTimer::singleShot(kStartDelayMs, this, [this] { check(false); });
}

void UpdateController::checkNow() {
  if (updatesDisabledByAdministrator()) {
    qInfo() << "Update checks are turned off by the administrator";
    m_notice->showMessage(tr("Update checks are turned off by your administrator."));
    showNotice();
    return;
  }
  check(true);
}

void UpdateController::check(bool manual) {
  if (m_state != State::Idle) {
    if (manual) {
      showNotice(); // the check or the download under way
    }
    return;
  }
  // Without a release key nothing could be verified (docs/updates.md):
  // no request then.
  if (m_client->source().keys.trimmed().isEmpty()) {
    qInfo() << "Update check skipped: this build has no release key";
    if (manual) {
      m_notice->showMessage(tr("This build of Mitcad has no release key, so it cannot verify updates."), true);
      showNotice();
    }
    return;
  }
  m_state = State::Checking;
  m_manual = manual;
  const UpdateSettings settings = UpdateSettings::load();
  if (manual) {
    m_notice->showProgress(tr("Checking for updates..."), 0, 0);
    showNotice();
  }
  m_client->check(settings.channel, settings.skippedVersion);
}

void UpdateController::onChecked(const UpdateOffer& offer) {
  m_state = State::Idle;
  m_offer = offer;
  UpdateSettings settings = UpdateSettings::load();
  settings.lastCheck = QDateTime::currentDateTimeUtc();
  settings.save();
  const QString current = m_client->source().currentVersion;
  if (offer.status == QStringLiteral("current")) {
    qInfo().noquote() << QStringLiteral("Update check: Mitcad %1 is up to date").arg(current);
    if (m_manual) {
      m_notice->showMessage(tr("Mitcad %1 is up to date.").arg(current));
      showNotice();
    } else {
      hideNotice();
    }
    return;
  }
  if (offer.status == QStringLiteral("skipped") && !m_manual) {
    qInfo().noquote() << QStringLiteral("Update check: Mitcad %1 is skipped").arg(offer.version);
    return;
  }
  this->offer(offer);
}

bool UpdateController::installable() const {
  return m_installation.canInstall() && m_offer.hasDownload;
}

void UpdateController::offer(const UpdateOffer& offer) {
  QString text = tr("<b>Mitcad %1</b> is available (this is %2).").arg(offer.version, m_client->source().currentVersion);
  if (offer.prerelease) {
    text += QLatin1Char(' ') + tr("It is a pre-release.");
  }
  if (!installable()) {
    text += QLatin1Char(' ') + tr("Download it from its release page.");
  }
  if (offer.status == QStringLiteral("skipped")) {
    text += QLatin1Char(' ') + tr("You skipped this version.");
  }
  UpdateSettings settings = UpdateSettings::load();
  if (!settings.hintShown) {
    text += QLatin1Char(' ') + tr("Mitcad checks for updates once a day; Tools > Preferences turns that off.");
    settings.hintShown = true;
    settings.save();
  }
  m_notice->showOffer(text, installable());
  showNotice();
}

void UpdateController::onFailed(const QString& message) {
  const State state = m_state;
  m_state = State::Idle;
  if (state == State::Downloading) {
    m_notice->showMessage(tr("Mitcad %1 was not installed: %2").arg(m_offer.version, message), true);
    showNotice();
  } else if (m_manual) {
    m_notice->showMessage(tr("The update check failed: %1").arg(message), true);
    showNotice();
  } else {
    hideNotice(); // an automatic check says nothing
  }
}

void UpdateController::install() {
  if (m_state != State::Idle || !installable()) {
    return;
  }
  m_state = State::Downloading;
  m_notice->showProgress(tr("Downloading Mitcad %1...").arg(m_offer.version), 0, m_offer.size);
  m_client->download(m_offer, m_installation.downloadPath(m_offer.version));
}

void UpdateController::onDownloaded(const QString& path) {
  m_notice->showProgress(tr("Verifying Mitcad %1...").arg(m_offer.version), 0, 0);
  const UpdateSource& source = m_client->source();
  try {
    updates::update_verify_download(
        rust::Slice<const std::uint8_t>(reinterpret_cast<const std::uint8_t*>(m_offer.manifest.constData()),
                                        static_cast<std::size_t>(m_offer.manifest.size())),
        rustStr(source.keys.toUtf8()), rustStr(source.platform.toUtf8()), rustStr(path.toUtf8()));
  } catch (const std::exception& e) {
    QFile::remove(path);
    m_state = State::Idle;
    qWarning().noquote() << QStringLiteral("Update rejected: %1 (deleted %2)").arg(errorText(e), path);
    m_notice->showMessage(tr("Mitcad %1 was not installed: %2. The download was deleted.")
                              .arg(m_offer.version, errorText(e)),
                          true);
    showNotice();
    return;
  }
  qInfo().noquote() << "Update verified:" << path;
  m_state = State::Installing;
  m_host.whenIdle([this, path] { finish(path); });
}

void UpdateController::finish(const QString& path) {
  const QString current = m_client->source().currentVersion;
  QString error;
  if (!stageUpdate(m_installation, path, current, m_offer.version, error)) {
    QFile::remove(path);
    m_state = State::Idle;
    m_notice->showMessage(tr("Mitcad %1 was not installed: %2").arg(m_offer.version, error), true);
    showNotice();
    return;
  }
  m_notice->showProgress(tr("Mitcad closes to install %1...").arg(m_offer.version), 0, 0);
  qInfo() << "Update: closing the window to install";
  if (m_host.closeWindow()) {
    QCoreApplication::quit();
    return;
  }
  // Cancelled when asked to save.
  unstageUpdate();
  QFile::remove(path);
  m_installation.removeLeftovers();
  m_state = State::Idle;
  m_notice->showMessage(tr("Mitcad %1 was not installed: Mitcad stayed open.").arg(m_offer.version));
  showNotice();
}

void UpdateController::showNotice() { m_bar->show(); }

void UpdateController::hideNotice() {
  if (m_bar->isVisible()) {
    qInfo() << "Update notice closed";
  }
  m_bar->hide();
}

} // namespace mitcad
