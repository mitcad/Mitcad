// SPDX-License-Identifier: MIT
#include "UpdateController.hpp"

#include <algorithm>
#include <cstdint>
#include <exception>
#include <utility>

#include <QCoreApplication>
#include <QDate>
#include <QDateTime>
#include <QDesktopServices>
#include <QFile>
#include <QMainWindow>
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QTimer>
#include <QUrl>
#include <QtLogging>

#include "UpdateOffer.hpp"
#include "UpdateRow.hpp"
#include "UpdateSettings.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "mitcad_bridge/update.h"

namespace mitcad {
namespace {

// The automatic check waits this long after start-up, so that it never
// competes with opening a design.
constexpr int kStartDelayMs = 3000;
constexpr qint64 kDaySeconds = 24 * 60 * 60;
// The update tests' fake offer (mitcad#1890): its version, above any
// release, its size, and the steps its "download" takes.
constexpr char kFakeVersion[] = "999.0.0";
constexpr qint64 kFakeSize = 20 * 1000 * 1000;
constexpr int kFakeSteps = 20;

bool checkedRecently(const UpdateSettings& settings, const QDateTime& now) {
  return settings.lastCheck.isValid() && settings.lastCheck <= now && settings.lastCheck.secsTo(now) < kDaySeconds;
}

UpdateOffer fakeOffer() {
  UpdateOffer offer;
  offer.status = QStringLiteral("available");
  offer.version = QString::fromLatin1(kFakeVersion);
  offer.date = QDate::currentDate().toString(Qt::ISODate);
  offer.notes = QStringLiteral("https://127.0.0.1/mitcad-update-test-notes.html");
  offer.hasDownload = true;
  offer.url = QStringLiteral("https://127.0.0.1/mitcad-update-test-download");
  offer.size = kFakeSize;
  return offer;
}

} // namespace

UpdateController& UpdateController::application() {
  static QPointer<UpdateController> instance;
  if (instance.isNull()) {
    instance = new UpdateController(QCoreApplication::instance());
  }
  return *instance;
}

UpdateController::UpdateController(QObject* parent)
    : QObject(parent), m_installation(Installation::detect()), m_row(new UpdateRow(this)) {
  connect(m_row, &UpdateRow::releaseNotesClicked, this, [this] {
    qInfo().noquote() << "Update: release notes at" << m_offer.notes;
    QDesktopServices::openUrl(QUrl(m_offer.notes));
  });
  connect(m_row, &UpdateRow::installClicked, this, &UpdateController::install);
  connect(m_row, &UpdateRow::skipClicked, this, [this] {
    UpdateSettings settings = UpdateSettings::load();
    settings.skippedVersion = m_offer.version;
    settings.save();
    qInfo().noquote() << "Update: skipping MitCad" << m_offer.version;
    m_row->hide();
  });
  connect(m_row, &UpdateRow::laterClicked, this, [] { qInfo() << "Update: later"; });
  connect(m_row, &UpdateRow::cancelClicked, this, [this] {
    cancelDownload();
    qInfo() << "Update: cancelled";
    m_row->hide();
  });
  connect(m_row, &UpdateRow::closed, this, [this] { m_row->hide(); });
  connect(m_row, &UpdateRow::retryClicked, this, [this] {
    qInfo() << "Update: try again";
    checkNow();
  });

  const UpdateSource source = UpdateSource::fromEnvironment();
  // A test's source never elevates an installation for all users
  // (mitcad#921).
  restrictToUserRights(m_installation, source.testSource, processHasAdministratorRights());
  m_client = new UpdateClient(source, this);
  connect(m_client, &UpdateClient::checked, this, &UpdateController::onChecked);
  connect(m_client, &UpdateClient::failed, this, &UpdateController::onFailed);
  connect(m_client, &UpdateClient::progress, this, &UpdateController::onProgress);
  connect(m_client, &UpdateClient::downloaded, this, &UpdateController::onDownloaded);
  m_fakeTimer = new QTimer(this);
  connect(m_fakeTimer, &QTimer::timeout, this, &UpdateController::fakeStep);
}

void UpdateController::setHost(Host host) {
  if (!m_host.closeWindows) {
    m_host = std::move(host);
  }
}

void UpdateController::registerCommands(CommandRegistry& registry) {
  CommandDef check;
  check.id = QStringLiteral("help.check_updates");
  check.name = tr("Check for Updates…");
  check.icon = QStringLiteral("update");
  check.kind = CommandDef::Kind::Action;
  check.mode = CommandDef::Mode::Any;
  check.tab.clear();
  check.tooltip = tr("Asks whether a newer MitCad is out (Tools › Preferences: automatic checks)");
  check.keywords = {QStringLiteral("update"), QStringLiteral("new version"), QStringLiteral("release"),
                    QStringLiteral("upgrade")};
  check.duringCommands = true;
  check.run = [this] { checkNow(); };
  registry.add(check);
}

void UpdateController::attach(QMainWindow& window) {
  m_row->attach(window);
  if (!m_started) {
    m_started = true;
    startUp();
  }
}

bool UpdateController::fake() const { return m_fakeSeconds > 0; }

void UpdateController::useTestFakeOffer(int seconds) {
  m_fakeSeconds = seconds;
  qInfo().noquote() << QStringLiteral("Update: the test driver's fake offer, downloaded in %1 s").arg(seconds);
}

bool UpdateController::canInstall() const { return m_installation.canInstall() || fake(); }

void UpdateController::startUp() {
  const UpdateSource& source = m_client->source();
  qInfo().noquote() << QStringLiteral("Updates: MitCad %1 for %2, %3")
                           .arg(source.currentVersion,
                                source.platform.isEmpty() ? QStringLiteral("an unknown platform") : source.platform,
                                m_installation.describe());
  for (const QString& line : source.describe()) {
    qInfo().noquote() << line;
  }

  if (const std::optional<FinishedUpdate> finished = takeFinishedUpdate(source.currentVersion)) {
    if (finished->installed) {
      qInfo().noquote() << QStringLiteral("Update installed: %1 -> %2").arg(finished->from, finished->to);
      m_row->showMessage(tr("MitCad was updated to %1.").arg(finished->to));
    } else {
      qWarning().noquote() << QStringLiteral("Update not installed: %1 -> %2 (%3)")
                                  .arg(finished->from, finished->to, finished->detail);
      // Try Again checks and offers the update anew (mitcad#1454: once the
      // programs that held the installation's files are closed).
      m_row->showMessage(finished->detail.isEmpty()
                             ? tr("The update to MitCad %1 was not installed.").arg(finished->to)
                             : tr("The update to MitCad %1 was not installed: %2").arg(finished->to, finished->detail),
                         NoticeLevel::Error, true);
    }
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
  if (checkedRecently(settings, now)) {
    qInfo().noquote() << "Update check: the last was at" << settings.lastCheck.toString(Qt::ISODate);
    return;
  }
  QTimer::singleShot(kStartDelayMs, this, [this] {
    // A manual check can finish before this timer fires. Do not start a
    // second silent check while the user is choosing Install in its offer.
    const UpdateSettings current = UpdateSettings::load();
    if (!current.automatic || updatesDisabledByAdministrator() ||
        checkedRecently(current, QDateTime::currentDateTimeUtc())) {
      return;
    }
    check(false);
  });
}

void UpdateController::checkNow() {
  // The tests' fake offer makes no request: the UI tests keep the
  // administrator's switch on (no automatic check, nothing sent).
  if (!fake() && updatesDisabledByAdministrator()) {
    qInfo() << "Update checks are turned off by the administrator";
    m_row->showMessage(tr("Update checks are turned off by your administrator."));
    return;
  }
  check(true);
}

void UpdateController::check(bool manual) {
  if (m_state != State::Idle) {
    if (manual && m_state == State::Checking && !m_manual) {
      // The silent automatic check under way: its answer is shown now.
      m_manual = true;
      m_row->showProgress(tr("Checking for updates…"), 0, 0);
    } else if (manual) {
      m_row->show(); // the check or the download under way
    }
    return;
  }
  // Without a release key built in nothing could be verified
  // (docs/updates.md): no request then, whatever test key is set
  // (mitcad#921). The tests' fake offer verifies nothing.
  if (!fake() && m_client->source().builtInKeys.trimmed().isEmpty()) {
    qInfo() << "Update check skipped: this build has no release key";
    if (manual) {
      m_row->showMessage(tr("This build of MitCad has no release key, so it cannot verify updates."),
                         NoticeLevel::Error);
    }
    return;
  }
  m_state = State::Checking;
  m_manual = manual;
  const UpdateSettings settings = UpdateSettings::load();
  if (manual) {
    m_row->showProgress(tr("Checking for updates…"), 0, 0);
  }
  if (fake()) {
    qInfo() << "Update check: the test's fake offer, no request";
    QTimer::singleShot(0, this, [this] {
      if (m_state == State::Checking) {
        onChecked(fakeOffer());
      }
    });
    return;
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
  if (offer.expired) {
    // A newer release whose manifest has expired (mitcad#938): an
    // automatic check offers nothing; asked, it is announced with why,
    // without Install, so that a computer whose clock is wrong is not cut
    // off from news of the release.
    qInfo().noquote() << QStringLiteral("Update check: the manifest of MitCad %1 expired on %2")
                             .arg(offer.version, offer.expires);
    if (m_manual) {
      this->offer(offer);
    } else {
      m_row->hide();
    }
    return;
  }
  if (offer.status == QStringLiteral("current")) {
    qInfo().noquote() << QStringLiteral("Update check: MitCad %1 is up to date").arg(current);
    if (m_manual) {
      // The answer to Check for Updates: a success (mitcad#1040).
      m_row->showMessage(tr("MitCad %1 is up to date.").arg(current), NoticeLevel::Success);
    } else {
      m_row->hide();
    }
    return;
  }
  if (offer.status == QStringLiteral("skipped") && !m_manual) {
    qInfo().noquote() << QStringLiteral("Update check: MitCad %1 is skipped").arg(offer.version);
    return;
  }
  if (!offer.hasDownload && !m_manual) {
    // Out for other platforms first (mitcad#1390): an automatic check
    // says nothing until the manifest has this platform's download.
    qInfo().noquote() << QStringLiteral("Update check: MitCad %1 has no download for %2 yet")
                             .arg(offer.version, m_client->source().platform);
    m_row->hide();
    return;
  }
  this->offer(offer);
}

bool UpdateController::installable() const { return canInstall() && m_offer.hasDownload && !m_offer.expired; }

void UpdateController::offer(const UpdateOffer& offer) {
  UpdateSettings settings = UpdateSettings::load();
  const OfferNotice notice = offerNotice(offer, m_client->source().currentVersion, m_client->source().platform,
                                         canInstall(), m_manual, !settings.hintShown);
  if (!notice.show) {
    m_row->hide();
    return;
  }
  if (!settings.hintShown) {
    settings.hintShown = true;
    settings.save();
  }
  m_row->showOffer(notice.html, notice.install, notice.skip);
}

void UpdateController::onFailed(const QString& message) {
  const State state = m_state;
  m_state = State::Idle;
  if (state == State::Downloading) {
    m_row->showMessage(tr("MitCad %1 was not installed: %2").arg(m_offer.version, message), NoticeLevel::Error);
  } else if (m_manual) {
    m_row->showMessage(tr("The update check failed: %1").arg(message), NoticeLevel::Error);
  } else {
    m_row->hide(); // an automatic check says nothing
  }
}

void UpdateController::install() {
  if (m_state != State::Idle || !installable()) {
    return;
  }
  m_state = State::Downloading;
  m_loggedTenths = -1;
  m_row->showProgress(tr("Downloading MitCad %1…").arg(m_offer.version), 0, m_offer.size);
  if (fake()) {
    qInfo().noquote() << QStringLiteral("Update download: the test's fake offer of MitCad %1 in %2 s")
                             .arg(m_offer.version)
                             .arg(m_fakeSeconds);
    m_fakeReceived = 0;
    m_fakeTimer->start(m_fakeSeconds * 1000 / kFakeSteps);
    return;
  }
  m_client->download(m_offer, m_installation.downloadPath(m_offer.version));
}

void UpdateController::onProgress(qint64 received, qint64 total) {
  if (m_state != State::Downloading) {
    return;
  }
  m_row->showProgress(tr("Downloading MitCad %1…").arg(m_offer.version), received, total);
  // Every tenth for the log: the UI tests see the download going on.
  if (total > 0) {
    const int tenths = static_cast<int>(std::min<qint64>(10, received * 10 / total));
    if (tenths != m_loggedTenths) {
      m_loggedTenths = tenths;
      qInfo().noquote() << QStringLiteral("Update download: %1%").arg(tenths * 10);
    }
  }
}

void UpdateController::fakeStep() {
  if (m_state != State::Downloading) {
    m_fakeTimer->stop();
    return;
  }
  m_fakeReceived = std::min(kFakeSize, m_fakeReceived + kFakeSize / kFakeSteps);
  onProgress(m_fakeReceived, kFakeSize);
  if (m_fakeReceived < kFakeSize) {
    return;
  }
  m_fakeTimer->stop();
  m_state = State::Idle;
  qInfo().noquote() << QStringLiteral("Update downloaded: MitCad %1, the test's fake offer (nothing verified or "
                                      "installed)")
                           .arg(m_offer.version);
  m_row->showMessage(tr("MitCad %1 is downloaded. A test's fake update is not installed.").arg(m_offer.version),
                     NoticeLevel::Success);
}

void UpdateController::cancelDownload() {
  m_fakeTimer->stop();
  m_client->cancel();
  m_state = State::Idle;
}

void UpdateController::onDownloaded(const QString& path) {
  m_row->showProgress(tr("Verifying MitCad %1…").arg(m_offer.version), 0, 0);
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
    m_row->showMessage(tr("MitCad %1 was not installed: %2. The download was deleted.")
                           .arg(m_offer.version, errorText(e)),
                       NoticeLevel::Error);
    return;
  }
  qInfo().noquote() << "Update verified:" << path;
  m_state = State::Installing;
  m_verified = path;
  if (m_host.whenIdle) {
    m_host.whenIdle([this, path] { finish(path); });
  } else {
    finish(path);
  }
}

void UpdateController::finish(const QString& path) {
  // Staged already when the last window closed first (mayCloseLastWindow).
  if (m_state != State::Installing || m_verified != path) {
    return;
  }
  if (m_asking) {
    // The last window's question is open: its answer first.
    QTimer::singleShot(200, this, [this, path] { finish(path); });
    return;
  }
  const QString current = m_client->source().currentVersion;
  QString error;
  if (!stageUpdate(m_installation, path, m_offer.sha256, current, m_offer.version, error)) {
    QFile::remove(path);
    m_state = State::Idle;
    m_verified.clear();
    m_row->showMessage(tr("MitCad %1 was not installed: %2").arg(m_offer.version, error), NoticeLevel::Error);
    return;
  }
  m_row->showProgress(tr("MitCad closes to install %1…").arg(m_offer.version), 0, 0);
  qInfo() << "Update: closing the windows to install";
  m_closingToInstall = true;
  const bool closed = m_host.closeWindows && m_host.closeWindows();
  m_closingToInstall = false;
  if (closed) {
    QCoreApplication::quit();
    return;
  }
  // Cancelled when asked to save.
  unstageUpdate();
  QFile::remove(path);
  m_installation.removeLeftovers();
  m_state = State::Idle;
  m_verified.clear();
  m_row->showMessage(tr("MitCad %1 was not installed: MitCad stayed open.").arg(m_offer.version));
}

bool UpdateController::mayCloseLastWindow(QWidget* window) {
  if (m_closingToInstall) {
    return true;
  }
  const auto stageOnQuit = [this] {
    // Verified and waiting for the model: it installs once MitCad has quit,
    // as an update does when it closes the windows itself.
    QString error;
    const QString path = m_verified;
    m_state = State::Idle;
    m_verified.clear();
    if (stageUpdate(m_installation, path, m_offer.sha256, m_client->source().currentVersion, m_offer.version,
                    error)) {
      qInfo().noquote() << QStringLiteral("Update: MitCad %1 installs once MitCad has quit").arg(m_offer.version);
      m_row->showMessage(tr("MitCad %1 installs when MitCad closes.").arg(m_offer.version));
    } else {
      QFile::remove(path);
      qWarning().noquote() << QStringLiteral("Update: MitCad %1 not staged: %2").arg(m_offer.version, error);
    }
  };
  switch (m_state) {
  case State::Idle:
    return true;
  case State::Checking:
    m_client->cancel();
    m_state = State::Idle;
    return true;
  case State::Installing:
    stageOnQuit();
    return true;
  case State::Downloading:
    break;
  }
  // A download under way: not dropped without asking.
  qInfo().noquote() << QStringLiteral("Update question: MitCad %1 is still downloading").arg(m_offer.version);
  QMessageBox box(QMessageBox::Question, tr("MitCad"), tr("MitCad %1 is still downloading.").arg(m_offer.version),
                  QMessageBox::NoButton, window);
  box.setInformativeText(tr("Keep MitCad open until the update is downloaded and installed, or stop the download "
                            "and close MitCad."));
  QPushButton* keep = box.addButton(tr("&Keep Downloading"), QMessageBox::RejectRole);
  QPushButton* stop = box.addButton(tr("&Stop and Close"), QMessageBox::DestructiveRole);
  box.setDefaultButton(keep);
  box.setEscapeButton(keep);
  m_asking = true;
  prepareModal(&box);
  execDialog(&box);
  m_asking = false;
  const bool stopped = box.clickedButton() == stop;
  if (!stopped) {
    // The window stays open; the update installs once it is downloaded.
    qInfo() << "Update: the download goes on, MitCad stays open";
    m_row->show();
    return false;
  }
  qInfo() << "Update: the download stopped, MitCad closes";
  if (m_state == State::Downloading) {
    cancelDownload();
  } else if (m_state == State::Installing) {
    // Done meanwhile: dropped as asked.
    QFile::remove(m_verified);
    m_verified.clear();
    m_state = State::Idle;
  }
  m_row->hide();
  return true;
}

} // namespace mitcad
