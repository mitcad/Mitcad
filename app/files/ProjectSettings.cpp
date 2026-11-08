// SPDX-License-Identifier: MIT
#include "ProjectSettings.hpp"
#include "ProjectSettingsFields.hpp"

#include <algorithm>
#include <exception>
#include <initializer_list>
#include <utility>

#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QFileInfo>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPointer>
#include <QProgressBar>
#include <QPushButton>
#include <QRadioButton>
#include <QSpinBox>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/Json.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "CloudAddress.hpp"
#include "CloudSection.hpp"
#include "ProjectDialogs.hpp"
#include "Projects.hpp"
#include "RemoteDialogs.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

// The file's texts in the context ProjectSettings, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ProjectSettings)
};

QLabel* note(const QString& text = QString()) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  label->setTextFormat(Qt::PlainText);
  return label;
}

// A command of the project in `root` on this thread (no network); a
// failure as {"error": {"class": "other", "message"}}.
QJsonObject projectCommand(const QString& root, const QJsonObject& command) {
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    return parseObject(project->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                  {QStringLiteral("message"), errorText(e)},
                                                  {QStringLiteral("detail"), QString()}}}};
  }
}

QJsonObject named(const char* name) { return {{QStringLiteral("cmd"), QLatin1String(name)}}; }

// The intervals offered for checking the remote; -1 is the application's
// default.
const int kCheckChoices[] = {-1, 1, 5, 10, 30, 60, 240, 0};

QString checkText(int minutes) {
  if (minutes == 0) {
    return Text::tr("never");
  }
  if (minutes == 1) {
    return Text::tr("every minute");
  }
  if (minutes % 60 == 0) {
    return minutes == 60 ? Text::tr("every hour") : Text::tr("every %1 hours").arg(minutes / 60);
  }
  return Text::tr("every %1 minutes").arg(minutes);
}

// What sharing to a remote does, for the Cloud section (a project's
// remote_check).
QString describeForSharing(const QJsonObject& answer, bool& ok) {
  if (answer.value(QStringLiteral("new_folder")).toBool()) {
    return Text::tr("✓ An empty repository is made in this folder: the project's versions go there.");
  }
  if (answer.value(QStringLiteral("empty")).toBool()) {
    return Text::tr("✓ Reachable and empty: the project's versions go there.");
  }
  if (answer.value(QStringLiteral("related")).toBool()) {
    return Text::tr("✓ The repository has this project's history: they are brought together.");
  }
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    ok = false;
    return Text::tr("The repository holds another project, or another history of this one. Choose an empty repository, "
              "or open that project with Open from Cloud.");
  }
  QStringList files;
  for (const QJsonValue& file : answer.value(QStringLiteral("files")).toArray()) {
    files << file.toString();
  }
  return Text::tr("✓ The project's versions go after the repository's files (%1).")
      .arg(files.mid(0, 5).join(QStringLiteral(", ")));
}

class ProjectSettingsDialog : public QDialog {
public:
  ProjectSettingsDialog(QWidget* parent, QString root, const ProjectSettingsHost& host);

  bool syncRequested() const { return m_syncRequested; }

protected:
  void changeEvent(QEvent* event) override;
  void reject() override;

private:
  void read();
  void showKind();
  void storageChosen();
  void stopSyncing();
  void share();
  void shareFinished(RemoteTask* task, const QString& url, const QString& author, const QJsonObject& resolutions);
  void changeAddress();
  void followRemote();
  // Several remotes and none followed: Project Settings asks which.
  bool choosingRemote() const;
  void writeShared();
  void writeLocal();
  void writeAuthor();
  void writeDisplayName();
  // The web viewer's update on each Save (mitcad#423): for everyone, a
  // version when changed.
  void writeWebViewer();
  // The name the Projects panel shows without a display name: a Cloud
  // project's repository address without the scheme, else the folder's
  // name (mitcad#173).
  QString defaultName() const;
  void listAuthors();
  QJsonObject sharedChosen() const;
  QJsonObject localChosen() const;
  void logState() const;
  // Who records a version of the settings: the project's author, else
  // Preferences' default author ("" when neither has one).
  QString author() const;

  QString m_root;
  ProjectSettingsHost m_host;
  QJsonObject m_settings; // project_settings
  QString m_kind;         // "local" or "cloud"
  QString m_url;
  QString m_remoteName;
  QStringList m_remotes; // every remote's name (remote_info's remotes)
  bool m_syncRequested = false;
  bool m_reading = false;
  QPointer<RemoteTask> m_task;

  QRadioButton* m_local = nullptr;
  QRadioButton* m_cloud = nullptr;
  QLabel* m_addressLabel = nullptr;
  QWidget* m_addressRow = nullptr;
  QLabel* m_address = nullptr;
  QPushButton* m_change = nullptr;
  QPushButton* m_browser = nullptr;
  QLabel* m_followLabel = nullptr;
  QWidget* m_followRow = nullptr;
  QComboBox* m_followChoice = nullptr;
  QPushButton* m_follow = nullptr;
  QLabel* m_followNote = nullptr;
  QLabel* m_statusLabel = nullptr;
  QWidget* m_statusRow = nullptr;
  QLabel* m_status = nullptr;
  QFormLayout* m_top = nullptr;
  ProjectSettingsFields* m_fields = nullptr;
  QGroupBox* m_shareBox = nullptr;
  CloudSection* m_section = nullptr;
  QLabel* m_authors = nullptr;
  QPushButton* m_share = nullptr;
  QPushButton* m_close = nullptr;
  QWidget* m_progress = nullptr;
  QLabel* m_progressText = nullptr;
  bool m_probed = false;
  QLabel* m_problem = nullptr;
  QGroupBox* m_webBox = nullptr;
  QCheckBox* m_webUpdate = nullptr;
  QLabel* m_webNote = nullptr;
};

ProjectSettingsDialog::ProjectSettingsDialog(QWidget* parent, QString root, const ProjectSettingsHost& host)
    : QDialog(parent), m_root(std::move(root)), m_host(host) {
  setWindowTitle(Text::tr("Project Settings - %1").arg(QFileInfo(m_root).fileName()));
  auto* layout = new QVBoxLayout(this);
  m_top = new QFormLayout;
  auto* folder = new QLabel(QDir::toNativeSeparators(m_root));
  folder->setTextInteractionFlags(Qt::TextSelectableByMouse);
  auto* show = new QPushButton(Text::tr("Show in &Folder"));
  show->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(folder, 1);
  folderRow->addWidget(show);
  m_top->addRow(Text::tr("Folder:"), folderRow);
  m_local = new QRadioButton(Text::tr("&Local"));
  m_cloud = new QRadioButton(Text::tr("Cl&oud (git)"));
  auto* storage = new QButtonGroup(this);
  storage->addButton(m_local);
  storage->addButton(m_cloud);
  auto* storageRow = new QHBoxLayout;
  storageRow->addWidget(m_local);
  storageRow->addWidget(m_cloud);
  storageRow->addStretch(1);
  m_top->addRow(Text::tr("Storage:"), storageRow);
  m_address = new QLabel;
  m_address->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_address->setTextFormat(Qt::PlainText);
  m_change = new QPushButton(Text::tr("C&hange…"));
  m_browser = new QPushButton(Text::tr("Open in &Browser"));
  m_addressRow = new QWidget;
  auto* addressLayout = new QHBoxLayout(m_addressRow);
  addressLayout->setContentsMargins(0, 0, 0, 0);
  addressLayout->addWidget(m_address, 1);
  addressLayout->addWidget(m_change);
  addressLayout->addWidget(m_browser);
  m_addressLabel = new QLabel(Text::tr("Address:"));
  m_top->addRow(m_addressLabel, m_addressRow);
  // Several remotes and none followed (mitcad#89): the one to sync with.
  m_followChoice = new QComboBox;
  m_follow = new QPushButton(Text::tr("Follo&w"));
  m_followNote = note();
  m_followRow = new QWidget;
  auto* followColumn = new QVBoxLayout(m_followRow);
  followColumn->setContentsMargins(0, 0, 0, 0);
  auto* followLayout = new QHBoxLayout;
  followLayout->addWidget(m_followChoice, 1);
  followLayout->addWidget(m_follow);
  followColumn->addLayout(followLayout);
  followColumn->addWidget(m_followNote);
  m_followLabel = new QLabel(Text::tr("Re&mote:"));
  m_followLabel->setBuddy(m_followChoice);
  m_top->addRow(m_followLabel, m_followRow);
  m_status = new QLabel;
  m_status->setTextFormat(Qt::PlainText);
  auto* syncNow = new QPushButton(Text::tr("S&ync Now"));
  m_statusRow = new QWidget;
  auto* statusLayout = new QHBoxLayout(m_statusRow);
  statusLayout->setContentsMargins(0, 0, 0, 0);
  statusLayout->addWidget(m_status, 1);
  statusLayout->addWidget(syncNow);
  m_statusLabel = new QLabel(Text::tr("Status:"));
  m_top->addRow(m_statusLabel, m_statusRow);
  for (QPushButton* button : {m_change, m_browser, m_follow, syncNow}) {
    button->setAutoDefault(false);
  }
  layout->addLayout(m_top);

  // Local -> Cloud: the address, then Share.
  m_shareBox = new QGroupBox(Text::tr("Share"));
  mac::styleSection(m_shareBox);
  auto* shareColumn = new QVBoxLayout(m_shareBox);
  m_section = new CloudSection(
      [this](const QString& url, QObject* owner) {
        return RemoteTask::command(m_root,
                                   {{QStringLiteral("cmd"), QStringLiteral("remote_check")}, {QStringLiteral("url"), url}},
                                   owner);
      },
      describeForSharing);
  m_section->setFolderName(QFileInfo(m_root).fileName());
  shareColumn->addWidget(m_section);
  m_authors = note();
  shareColumn->addWidget(m_authors);
  m_share = new QPushButton(Text::tr("&Share"));
  m_share->setAutoDefault(false);
  auto* shareRow = new QHBoxLayout;
  shareRow->addStretch(1);
  shareRow->addWidget(m_share);
  shareColumn->addLayout(shareRow);
  layout->addWidget(m_shareBox);
  m_progress = new QWidget;
  auto* progressRow = new QHBoxLayout(m_progress);
  progressRow->setContentsMargins(0, 0, 0, 0);
  m_progressText = new QLabel;
  auto* bar = new QProgressBar;
  bar->setRange(0, 0);
  bar->setMaximumWidth(160);
  progressRow->addWidget(m_progressText, 1);
  progressRow->addWidget(bar);
  layout->addWidget(m_progress);
  m_progress->hide();

  m_fields = new ProjectSettingsFields(ProjectSettingsFields::Host::ProjectSettings, this);
  layout->addWidget(m_fields);

  // The project's web viewer (mitcad#423): File > Project > Web Viewer
  // turns it on; whether a Save updates it is for everyone (a version).
  m_webBox = new QGroupBox(Text::tr("Web viewer (saved as a version)"));
  mac::styleSection(m_webBox);
  auto* webColumn = new QVBoxLayout(m_webBox);
  m_webUpdate = new QCheckBox(Text::tr("&Update the web viewer on each Save"));
  m_webUpdate->setObjectName(QStringLiteral("projectWebViewerUpdate"));
  m_webUpdate->setToolTip(Text::tr("Each Save that records a version also updates the design's web page"));
  m_webNote = note();
  webColumn->addWidget(m_webUpdate);
  webColumn->addWidget(m_webNote);
  layout->addWidget(m_webBox);

  m_problem = note();
  setErrorStyleSheet(m_problem);
  layout->addWidget(m_problem);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  m_close = buttons->button(QDialogButtonBox::Close);
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons, nullptr, true);
  mac::styleDialog(this);

  read();

  connect(show, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Project Settings: show in folder %1").arg(m_root);
    QDesktopServices::openUrl(QUrl::fromLocalFile(m_root));
  });
  connect(m_cloud, &QRadioButton::toggled, this, [this] { storageChosen(); });
  connect(m_change, &QPushButton::clicked, this, [this] { changeAddress(); });
  connect(m_follow, &QPushButton::clicked, this, [this] { followRemote(); });
  connect(m_browser, &QPushButton::clicked, this, [this] {
    if (m_host.openInBrowser) {
      m_host.openInBrowser();
    }
  });
  connect(syncNow, &QPushButton::clicked, this, [this] {
    m_syncRequested = true;
    qInfo().noquote() << QStringLiteral("Project Settings: Sync Now");
    QDialog::accept();
  });
  connect(m_section, &CloudSection::checked, this, [this] {
    m_share->setEnabled(m_section->answerOk() && m_task == nullptr);
  });
  connect(m_section, &CloudSection::addressChanged, this, [this] { m_share->setEnabled(false); });
  connect(m_share, &QPushButton::clicked, this, [this] { share(); });
  connect(m_fields->m_locks, &QCheckBox::toggled, this, [this] { writeShared(); });
  connect(m_fields->m_idle, &QSpinBox::editingFinished, this, [this] { writeShared(); });
  connect(m_fields->m_poll, &QSpinBox::editingFinished, this, [this] { writeShared(); });
  connect(m_fields->m_liveNone, &QRadioButton::toggled, this, [this](bool on) {
    m_fields->m_broker->setEnabled(!on);
    m_fields->m_prefix->setEnabled(!on);
    if (on || !m_fields->m_broker->text().trimmed().isEmpty()) {
      writeShared();
    } else {
      m_fields->m_broker->setFocus();
    }
  });
  connect(m_fields->m_broker, &QLineEdit::editingFinished, this, [this] { writeShared(); });
  connect(m_fields->m_prefix, &QLineEdit::editingFinished, this, [this] { writeShared(); });
  connect(m_fields->m_test, &QPushButton::clicked, this, [this] {
    if (m_host.testBroker) {
      m_host.testBroker(this, m_fields->m_broker->text().trimmed(), m_fields->m_prefix->text().trimmed());
    }
  });
  connect(m_fields->m_displayName, &QLineEdit::editingFinished, this, [this] { writeDisplayName(); });
  connect(m_fields->m_name, &QLineEdit::editingFinished, this, [this] { writeAuthor(); });
  connect(m_fields->m_email, &QLineEdit::editingFinished, this, [this] { writeAuthor(); });
  connect(m_fields->m_sendAtOnce, &QCheckBox::toggled, this, [this] { writeLocal(); });
  connect(m_fields->m_check, &QComboBox::currentIndexChanged, this, [this] { writeLocal(); });
  connect(m_fields->m_liveHere, &QComboBox::currentIndexChanged, this, [this] {
    m_fields->m_hereRow->setVisible(m_fields->m_liveHere->currentData().toString() == QLatin1String("broker"));
    if (m_fields->m_liveHere->currentData().toString() != QLatin1String("broker") ||
        !m_fields->m_hereBroker->text().trimmed().isEmpty()) {
      writeLocal();
    } else {
      m_fields->m_hereBroker->setFocus();
    }
  });
  connect(m_fields->m_hereBroker, &QLineEdit::editingFinished, this, [this] { writeLocal(); });
  connect(m_fields->m_herePrefix, &QLineEdit::editingFinished, this, [this] { writeLocal(); });
  connect(m_webUpdate, &QCheckBox::toggled, this, [this] { writeWebViewer(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &ProjectSettingsDialog::reject);
  logState();
}

void ProjectSettingsDialog::read() {
  m_reading = true;
  m_settings = projectCommand(m_root, named("project_settings"));
  m_kind = m_settings.value(QStringLiteral("kind")).toString();
  const QJsonObject remote = projectCommand(m_root, named("remote_info"));
  m_url = remote.value(QStringLiteral("url")).toString();
  m_remoteName = remote.value(QStringLiteral("name")).toString();
  m_remotes.clear();
  m_followChoice->clear();
  for (const QJsonValue& value : remote.value(QStringLiteral("remotes")).toArray()) {
    const QJsonObject entry = value.toObject();
    const QString name = entry.value(QStringLiteral("name")).toString();
    m_remotes << name;
    m_followChoice->addItem(QStringLiteral("%1 (%2)").arg(name, entry.value(QStringLiteral("url")).toString()), name);
  }
  const QJsonObject shared = m_settings.value(QStringLiteral("shared")).toObject();
  const QJsonObject locks = shared.value(QStringLiteral("edit_locks")).toObject();
  m_fields->m_locks->setChecked(locks.value(QStringLiteral("enabled")).toBool(true));
  m_fields->m_idle->setValue(locks.value(QStringLiteral("idle_minutes")).toInt(10));
  m_fields->m_poll->setValue(locks.value(QStringLiteral("poll_seconds")).toInt(10));
  const QJsonObject live = shared.value(QStringLiteral("live_updates")).toObject();
  m_fields->m_liveBroker->setChecked(!live.isEmpty());
  m_fields->m_liveNone->setChecked(live.isEmpty());
  m_fields->m_broker->setText(live.value(QStringLiteral("broker")).toString());
  m_fields->m_prefix->setText(live.value(QStringLiteral("prefix")).toString());
  m_fields->m_broker->setEnabled(!live.isEmpty());
  m_fields->m_prefix->setEnabled(!live.isEmpty());
  const QJsonValue web = shared.value(QStringLiteral("web_viewer"));
  m_webUpdate->setEnabled(web.isObject());
  m_webUpdate->setChecked(web.isObject() &&
                          web.toObject().value(QStringLiteral("update_on_save")).toBool(true));
  m_webNote->setText(web.isObject()
                         ? Text::tr("Its pages are in the project's web folder: open web/index.html in a browser, or let a "
                              "static web host serve the repository.")
                         : Text::tr("Not turned on: File › Project › Web Viewer writes a page per design into the "
                              "project's web folder."));
  m_fields->m_test->setVisible(true);
  m_fields->m_test->setEnabled(static_cast<bool>(m_host.testBroker));
  const QJsonObject local = m_settings.value(QStringLiteral("local")).toObject();
  m_fields->m_displayName->setText(local.value(QStringLiteral("name")).toString());
  m_fields->m_displayName->setModified(false);
  const QJsonObject sync = local.value(QStringLiteral("sync")).toObject();
  const RemoteSettings defaults = RemoteSettings::load();
  m_fields->m_sendAtOnce->setChecked(sync.value(QStringLiteral("send_at_once")).isBool()
                                       ? sync.value(QStringLiteral("send_at_once")).toBool()
                                       : defaults.autoPush);
  const QJsonValue minutes = sync.value(QStringLiteral("check_minutes"));
  int index = m_fields->m_check->findData(minutes.isDouble() ? minutes.toInt() : -1);
  if (index < 0) {
    m_fields->m_check->addItem(checkText(minutes.toInt()), minutes.toInt());
    index = m_fields->m_check->count() - 1;
  }
  m_fields->m_check->setCurrentIndex(index);
  const QJsonObject liveHere = local.value(QStringLiteral("live")).toObject();
  const QString mode = liveHere.value(QStringLiteral("mode")).toString(QStringLiteral("project"));
  m_fields->m_liveHere->setCurrentIndex(std::max(0, m_fields->m_liveHere->findData(mode)));
  m_fields->m_hereBroker->setText(liveHere.value(QStringLiteral("broker")).toString());
  m_fields->m_herePrefix->setText(liveHere.value(QStringLiteral("prefix")).toString());
  m_fields->m_hereRow->setVisible(mode == QLatin1String("broker"));
  const QJsonObject identity = projectCommand(m_root, named("identity"));
  m_fields->m_name->setText(identity.value(QStringLiteral("name")).toString());
  m_fields->m_email->setText(identity.value(QStringLiteral("email")).toString());
  if (!errorClassOf(m_settings).isEmpty()) {
    m_problem->setText(errorMessageOf(m_settings));
  }
  showKind();
  m_reading = false;
}

void ProjectSettingsDialog::showKind() {
  const bool cloud = m_kind == QLatin1String("cloud");
  {
    const QSignalBlocker blocker(m_cloud);
    m_cloud->setChecked(cloud);
    m_local->setChecked(!cloud);
  }
  m_address->setText(m_url);
  m_browser->setEnabled(!addressWebPage(m_url).isEmpty());
  m_fields->m_displayName->setPlaceholderText(defaultName());
  m_fields->m_displayName->setToolTip(
      Text::tr("The name the Projects panel shows for this project on this computer; empty: %1").arg(defaultName()));
  m_status->setText(m_host.status ? m_host.status() : QString());
  // Several remotes and none followed (mitcad#89): which one first; the
  // project neither syncs nor has edit locks before.
  const bool choosing = choosingRemote();
  m_top->setRowVisible(m_addressLabel, cloud && !choosing);
  m_top->setRowVisible(m_statusLabel, cloud && !choosing);
  m_top->setRowVisible(m_followLabel, choosing);
  m_local->setEnabled(!choosing);
  m_local->setToolTip(choosing ? Text::tr("Choose the remote to follow first") : QString());
  if (choosing) {
    m_followNote->setText(Text::tr("The project's repository has the remotes %1 and follows none of them: choose the one "
                             "this project sends its versions to and takes others' from.")
                              .arg(m_remotes.join(QStringLiteral(", "))));
    qInfo().noquote() << QStringLiteral("Project Settings: several remotes, none followed: %1")
                             .arg(m_remotes.join(QStringLiteral(", ")));
  }
  m_fields->m_everyone->setVisible(cloud);
  m_shareBox->setVisible(false);
  if (cloud && !choosing && !m_probed && m_host.probeLocks) {
    // Whether the remote takes Mitcad's lock refs (mitcad#89).
    m_probed = true;
    m_host.probeLocks(this, [this](bool accepted, const QString& message) {
      qInfo().noquote() << (accepted ? QStringLiteral("Project Settings: edit locks accepted by the remote")
                                     : QStringLiteral("Project Settings: edit locks: %1").arg(message));
      for (QWidget* widget : std::initializer_list<QWidget*>{m_fields->m_locks, m_fields->m_idle, m_fields->m_poll}) {
        widget->setEnabled(accepted);
      }
      m_fields->m_locksNote->setText(accepted ? QString() : message);
      m_fields->m_locksNote->setVisible(!accepted);
    });
  }
  fitWindowToContents(this);
}

void ProjectSettingsDialog::logState() const {
  const QJsonObject shared = sharedChosen();
  const QJsonObject locks = shared.value(QStringLiteral("edit_locks")).toObject();
  const QJsonObject live = shared.value(QStringLiteral("live_updates")).toObject();
  qInfo().noquote() << QStringLiteral("Project Settings dialog: %1, %2%3; edit locks %4, idle %5 min, poll %6 s; "
                                      "live updates %7; author %8; send at once %9; check %10; live here %11")
                           .arg(QFileInfo(m_root).fileName(), m_kind,
                                m_url.isEmpty() ? QString() : QStringLiteral(" %1").arg(m_url),
                                locks.value(QStringLiteral("enabled")).toBool() ? QStringLiteral("on")
                                                                                : QStringLiteral("off"))
                           .arg(locks.value(QStringLiteral("idle_minutes")).toInt())
                           .arg(locks.value(QStringLiteral("poll_seconds")).toInt())
                           .arg(live.isEmpty() ? QStringLiteral("none")
                                               : QStringLiteral("%1 %2").arg(
                                                     live.value(QStringLiteral("broker")).toString(),
                                                     live.value(QStringLiteral("prefix")).toString()),
                                QStringLiteral("%1 <%2>").arg(m_fields->m_name->text(), m_fields->m_email->text()),
                                m_fields->m_sendAtOnce->isChecked() ? QStringLiteral("on") : QStringLiteral("off"),
                                m_fields->m_check->currentText(), m_fields->m_liveHere->currentData().toString());
  qInfo().noquote() << QStringLiteral("Project Settings web viewer: %1")
                           .arg(!m_webUpdate->isEnabled()   ? QStringLiteral("not turned on")
                                : m_webUpdate->isChecked() ? QStringLiteral("on, updated on each Save")
                                                           : QStringLiteral("on, not updated on Save"));
}

void ProjectSettingsDialog::changeEvent(QEvent* event) {
  QDialog::changeEvent(event);
  if (event->type() == QEvent::ActivationChange && isActiveWindow() && m_shareBox->isVisible() && m_task == nullptr) {
    m_section->recheck();
  }
}

void ProjectSettingsDialog::reject() {
  if (m_task != nullptr) {
    m_task->cancel();
    return;
  }
  m_section->stopCheck();
  // What was typed last is kept, as the window keeps changes at once.
  if (m_fields->m_name->isModified() || m_fields->m_email->isModified()) {
    writeAuthor();
  }
  if (m_fields->m_displayName->isModified()) {
    writeDisplayName();
  }
  qInfo().noquote() << QStringLiteral("Project Settings closed");
  QDialog::reject();
}

void ProjectSettingsDialog::storageChosen() {
  if (m_reading) {
    return;
  }
  const bool cloud = m_cloud->isChecked();
  if (m_kind == QLatin1String("local")) {
    // Local -> Cloud: the address, then Share; nothing changes before.
    m_shareBox->setVisible(cloud);
    m_share->setEnabled(m_section->hasAnswer() && m_section->answerOk());
    m_share->setDefault(cloud);
    if (!cloud) { m_close->setDefault(true); }
    qInfo().noquote() << QStringLiteral("Project Settings: storage %1").arg(cloud ? QStringLiteral("cloud (share)")
                                                                                  : QStringLiteral("local"));
    if (cloud) {
      listAuthors();
      if (QWidget* first = m_section->firstField()) {
        first->setFocus();
      }
      m_section->recheck();
    }
    fitWindowToContents(this);
    return;
  }
  if (!cloud) {
    stopSyncing();
  }
}

void ProjectSettingsDialog::stopSyncing() {
  QMessageBox box(QMessageBox::Question, Text::tr("Stop Syncing"),
                  Text::tr("Stop syncing %1?").arg(QFileInfo(m_root).fileName()), QMessageBox::Cancel, this);
  box.setInformativeText(Text::tr("The versions stay here and in the repository; nothing is deleted. Saved versions are "
                            "no longer sent, and newer ones are not fetched."));
  QPushButton* stop = box.addButton(Text::tr("&Stop Syncing"), QMessageBox::DestructiveRole);
  qInfo().noquote() << QStringLiteral("Project Settings: stop syncing asked");
  execDialog(&box);
  if (box.clickedButton() != stop || (m_host.settle && !m_host.settle(Text::tr("stop syncing")))) {
    qInfo().noquote() << QStringLiteral("Project Settings: stop syncing cancelled");
    showKind();
    return;
  }
  if (m_host.stoppingSync) {
    m_host.stoppingSync();
  }
  const QJsonObject removed = projectCommand(m_root, named("remote_remove"));
  if (!errorClassOf(removed).isEmpty()) {
    m_problem->setText(errorMessageOf(removed));
    qInfo().noquote() << QStringLiteral("Project Settings: stop syncing failed: %1").arg(errorMessageOf(removed));
    showKind();
    return;
  }
  qInfo().noquote() << QStringLiteral("Project Settings: stopped syncing %1 (was %2)").arg(m_root, m_url);
  m_problem->clear();
  read();
  if (m_host.projectChanged) {
    m_host.projectChanged();
  }
  logState();
}

void ProjectSettingsDialog::listAuthors() {
  // Every version keeps its author, also once shared (history is never
  // rewritten): who made the versions sharing publishes.
  QStringList authors;
  for (const QJsonValue& value : m_settings.value(QStringLiteral("authors")).toArray()) {
    const QJsonObject author = value.toObject();
    authors << QStringLiteral("%1 <%2>").arg(author.value(QStringLiteral("name")).toString(),
                                             author.value(QStringLiteral("email")).toString());
  }
  m_authors->setText(authors.isEmpty()
                         ? QString()
                         : Text::tr("The whole history is sent, with the name and email of each version's author: %1. "
                              "Anyone who can read the repository sees them.")
                               .arg(authors.join(QStringLiteral(", "))));
  qInfo().noquote() << QStringLiteral("Project Settings: authors to publish: %1").arg(authors.join(QStringLiteral(", ")));
}

void ProjectSettingsDialog::share() {
  const QString url = m_section->url();
  if (url.isEmpty() || !m_section->answerOk()) {
    return;
  }
  const QString name = m_fields->m_name->text().trimmed();
  const QString email = m_fields->m_email->text().trimmed();
  if (name.isEmpty() || !email.contains(QLatin1Char('@'))) {
    m_problem->setText(Text::tr("Sharing needs the project's author below: a name and an email address."));
    m_fields->m_name->setFocus();
    return;
  }
  if (m_host.settle && !m_host.settle(Text::tr("share the project"))) {
    return;
  }
  writeAuthor();
  shareFinished(nullptr, url, QStringLiteral("%1 <%2>").arg(name, email), {});
}

void ProjectSettingsDialog::shareFinished(RemoteTask* done, const QString& url, const QString& author,
                                          const QJsonObject& resolutions) {
  if (done == nullptr) {
    // Start (again, with the choices of conflicts).
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("connect")},
                        {QStringLiteral("url"), url},
                        {QStringLiteral("author"), author},
                        {QStringLiteral("onto_files"), true}};
    if (!resolutions.isEmpty()) {
      command.insert(QStringLiteral("resolutions"), resolutions);
    }
    const bool bare = m_section->needsBareRepository();
    const QByteArray json = compactJson(command);
    const QByteArray bareJson =
        compactJson({{QStringLiteral("cmd"), QStringLiteral("init_bare")}, {QStringLiteral("dir"), url}});
    const QByteArray root = m_root.toUtf8();
    RemoteTask* task = RemoteTask::custom(
        QStringLiteral("connect"),
        [json, bareJson, bare, root](const SyncControl& control) {
          if (bare) {
            const QJsonObject made = parseObject(projects_command(rustStr(bareJson), control));
            if (!errorClassOf(made).isEmpty()) {
              return made;
            }
          }
          const rust::Box<Project> project = open_project(rustStr(root));
          return parseObject(project->command_with(rustStr(json), control));
        },
        this);
    m_task = task;
    m_section->stopCheck();
    m_share->setEnabled(false);
    m_shareBox->setEnabled(false);
    m_progressText->setText(Text::tr("Sending the project's versions to %1…").arg(url));
    m_progress->show();
    connect(task, &RemoteTask::finished, this,
            [this, task, url, author] { shareFinished(task, url, author, QJsonObject()); });
    qInfo().noquote() << QStringLiteral("Project Settings: share to %1").arg(url);
    task->start();
    return;
  }
  const QJsonObject answer = done->answer();
  m_task = nullptr;
  done->deleteLater();
  m_progress->hide();
  m_shareBox->setEnabled(true);
  const QString cls = errorClassOf(answer);
  // Onto a repository's files: the sync's conflicts (the remote is removed
  // again until they have choices).
  QJsonArray conflicts = answer.value(QStringLiteral("conflicts")).toArray();
  if (conflicts.isEmpty()) {
    conflicts = answer.value(QStringLiteral("sync")).toObject().value(QStringLiteral("conflicts")).toArray();
  }
  if (!conflicts.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Project Settings: share stopped for %1 conflict(s)").arg(conflicts.size());
    const std::optional<QJsonObject> choices = askResolveConflicts(this, conflicts, nullptr);
    if (choices) {
      shareFinished(nullptr, url, author, *choices);
      return;
    }
  }
  const bool set = answer.value(QStringLiteral("set")).isObject() && !answer.value(QStringLiteral("undone")).toBool();
  if (!set) {
    qInfo().noquote() << QStringLiteral("Project Settings: share failed (%1): %2").arg(cls, errorMessageOf(answer));
    m_problem->setText(cls == QLatin1String("cancelled") ? Text::tr("Canceled: nothing changed.")
                                                         : errorMessageOf(answer));
    m_share->setEnabled(true);
    return;
  }
  const bool sent = answer.value(QStringLiteral("push")).toObject().value(QStringLiteral("pushed")).toBool();
  qInfo().noquote() << QStringLiteral("Project Settings: shared %1 to %2: %3%4")
                           .arg(QFileInfo(m_root).fileName(), url,
                                sent ? QStringLiteral("versions sent") : QStringLiteral("nothing sent"),
                                cls.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(cls));
  m_problem->setText(cls.isEmpty() ? QString()
                                   : Text::tr("The project is shared to %1, but: %2 Its versions are sent when the "
                                        "repository can be reached.")
                                         .arg(url, errorMessageOf(answer)));
  read();
  if (m_host.projectChanged) {
    m_host.projectChanged();
  }
  showKind();
  logState();
}

void ProjectSettingsDialog::changeAddress() {
  const std::optional<QString> url = askChangeAddress(this, m_root, m_url);
  if (!url || (m_host.settle && !m_host.settle(Text::tr("change the address")))) {
    return;
  }
  const QJsonObject answer = projectCommand(
      m_root, {{QStringLiteral("cmd"), QStringLiteral("remote_set")},
               {QStringLiteral("url"), *url},
               {QStringLiteral("name"), m_remoteName.isEmpty() ? QStringLiteral("origin") : m_remoteName},
               {QStringLiteral("author"), author()}});
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    qInfo().noquote() << QStringLiteral("Project Settings: change address failed: %1").arg(errorMessageOf(answer));
    return;
  }
  qInfo().noquote() << QStringLiteral("Project Settings: address changed to %1").arg(*url);
  read();
  if (m_host.projectChanged) {
    m_host.projectChanged();
  }
}

bool ProjectSettingsDialog::choosingRemote() const {
  return m_kind == QLatin1String("cloud") && m_remoteName.isEmpty() && !m_remotes.isEmpty();
}

void ProjectSettingsDialog::followRemote() {
  const QString name = m_followChoice->currentData().toString();
  if (name.isEmpty() || (m_host.settle && !m_host.settle(Text::tr("follow a remote")))) {
    return;
  }
  const QJsonObject answer = projectCommand(
      m_root, {{QStringLiteral("cmd"), QStringLiteral("remote_follow")}, {QStringLiteral("name"), name}});
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    qInfo().noquote() << QStringLiteral("Project Settings: follow %1 failed: %2").arg(name, errorMessageOf(answer));
    return;
  }
  qInfo().noquote() << QStringLiteral("Project Settings: following %1 (%2), %3")
                           .arg(name, answer.value(QStringLiteral("url")).toString(),
                                answer.value(QStringLiteral("upstream")).toString());
  m_problem->clear();
  read();
  if (m_host.projectChanged) {
    m_host.projectChanged();
  }
  logState();
}

QString ProjectSettingsDialog::author() const {
  const QString name = m_fields->m_name->text().trimmed();
  const QString email = m_fields->m_email->text().trimmed();
  if (!name.isEmpty() && email.contains(QLatin1Char('@'))) {
    return QStringLiteral("%1 <%2>").arg(name, email);
  }
  const VersionSettings defaults = VersionSettings::load();
  return defaults.complete() ? defaults.author() : QString();
}

QJsonObject ProjectSettingsDialog::sharedChosen() const { return m_fields->sharedChosen(); }

QJsonObject ProjectSettingsDialog::localChosen() const { return m_fields->localChosen(); }

void ProjectSettingsDialog::writeShared() {
  if (m_reading || m_kind != QLatin1String("cloud")) {
    return;
  }
  const QJsonObject shared = sharedChosen();
  if (shared == m_settings.value(QStringLiteral("shared")).toObject()) {
    return;
  }
  if (author().isEmpty()) {
    m_problem->setText(Text::tr("A change for everyone is recorded as a version: give the project's author first."));
    return;
  }
  const QJsonObject answer =
      projectCommand(m_root, {{QStringLiteral("cmd"), QStringLiteral("set_project_settings")},
                              {QStringLiteral("shared"), shared},
                              {QStringLiteral("author"), author()}});
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    qInfo().noquote() << QStringLiteral("Project Settings: shared settings refused: %1").arg(errorMessageOf(answer));
    return;
  }
  m_problem->clear();
  const QString commit = answer.value(QStringLiteral("commit")).toString();
  m_settings = projectCommand(m_root, named("project_settings"));
  {
    // The broker as the project keeps it (mqtts://host:8883).
    const QJsonObject live =
        m_settings.value(QStringLiteral("shared")).toObject().value(QStringLiteral("live_updates")).toObject();
    m_reading = true;
    if (!live.isEmpty()) {
      m_fields->m_broker->setText(live.value(QStringLiteral("broker")).toString());
      m_fields->m_prefix->setText(live.value(QStringLiteral("prefix")).toString());
    }
    m_reading = false;
  }
  qInfo().noquote() << QStringLiteral("Project Settings: shared settings recorded: %1").arg(
      commit.isEmpty() ? QStringLiteral("no change") : commit.left(7));
  if (!commit.isEmpty() && m_host.versionRecorded) {
    m_host.versionRecorded();
  }
  if (m_host.settingsChanged) {
    m_host.settingsChanged(m_settings);
  }
}

void ProjectSettingsDialog::writeLocal() {
  if (m_reading) {
    return;
  }
  const QJsonObject local = localChosen();
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_project_settings")}, {QStringLiteral("local"), local}};
  if (!author().isEmpty()) {
    command.insert(QStringLiteral("author"), author());
  }
  const QJsonObject answer = projectCommand(m_root, command);
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    qInfo().noquote() << QStringLiteral("Project Settings: settings here refused: %1").arg(errorMessageOf(answer));
    return;
  }
  m_problem->clear();
  m_settings = projectCommand(m_root, named("project_settings"));
  qInfo().noquote() << QStringLiteral("Project Settings: here: send at once %1, check %2, live %3")
                           .arg(m_fields->m_sendAtOnce->isChecked() ? QStringLiteral("on") : QStringLiteral("off"),
                                m_fields->m_check->currentText(), m_fields->m_liveHere->currentData().toString());
  if (m_host.settingsChanged) {
    m_host.settingsChanged(m_settings);
  }
}

QString ProjectSettingsDialog::defaultName() const {
  const QString cloud = m_kind == QLatin1String("cloud") && !m_url.isEmpty() ? repositoryAddressOf(m_url) : QString();
  return cloud.isEmpty() ? QFileInfo(m_root).fileName() : cloud;
}

void ProjectSettingsDialog::writeDisplayName() {
  if (m_reading) {
    return;
  }
  m_fields->m_displayName->setModified(false);
  const QString name = m_fields->m_displayName->text().trimmed();
  const QString old = m_settings.value(QStringLiteral("local")).toObject().value(QStringLiteral("name")).toString();
  if (name == old) {
    return;
  }
  const QJsonObject answer =
      projectCommand(m_root, {{QStringLiteral("cmd"), QStringLiteral("set_project_settings")},
                              {QStringLiteral("local"), QJsonObject{{QStringLiteral("name"),
                                                                     name.isEmpty() ? QJsonValue() : QJsonValue(name)}}}});
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    qInfo().noquote() << QStringLiteral("Project Settings: display name refused: %1").arg(errorMessageOf(answer));
    return;
  }
  m_problem->clear();
  m_settings = projectCommand(m_root, named("project_settings"));
  qInfo().noquote() << (name.isEmpty() ? QStringLiteral("Project Settings: display name cleared: %1").arg(defaultName())
                                       : QStringLiteral("Project Settings: display name %1").arg(name));
  if (m_host.settingsChanged) {
    m_host.settingsChanged(m_settings);
  }
}

void ProjectSettingsDialog::writeWebViewer() {
  if (m_reading) {
    return;
  }
  const bool on = m_webUpdate->isChecked();
  const auto back = [this, on](const QString& problem) {
    m_problem->setText(problem);
    const QSignalBlocker blocker(m_webUpdate);
    m_webUpdate->setChecked(!on);
  };
  if (author().isEmpty()) {
    back(Text::tr("A change for everyone is recorded as a version: give the project's author first."));
    return;
  }
  const QJsonObject answer = projectCommand(
      m_root, {{QStringLiteral("cmd"), QStringLiteral("set_project_settings")},
               {QStringLiteral("shared"),
                QJsonObject{{QStringLiteral("web_viewer"), QJsonObject{{QStringLiteral("update_on_save"), on}}}}},
               {QStringLiteral("author"), author()}});
  if (!errorClassOf(answer).isEmpty()) {
    qInfo().noquote() << QStringLiteral("Project Settings: web viewer refused: %1").arg(errorMessageOf(answer));
    back(errorMessageOf(answer));
    return;
  }
  m_problem->clear();
  m_settings = projectCommand(m_root, named("project_settings"));
  const QString commit = answer.value(QStringLiteral("commit")).toString();
  qInfo().noquote() << QStringLiteral("Project Settings: web viewer updated on each Save %1, recorded: %2")
                           .arg(on ? QStringLiteral("on") : QStringLiteral("off"),
                                commit.isEmpty() ? QStringLiteral("no change") : commit.left(7));
  if (!commit.isEmpty() && m_host.versionRecorded) {
    m_host.versionRecorded();
  }
  if (m_host.settingsChanged) {
    m_host.settingsChanged(m_settings);
  }
}

void ProjectSettingsDialog::writeAuthor() {
  if (m_reading) {
    return;
  }
  const QString name = m_fields->m_name->text().trimmed();
  const QString email = m_fields->m_email->text().trimmed();
  m_fields->m_name->setModified(false);
  m_fields->m_email->setModified(false);
  if ((name.isEmpty() != email.isEmpty()) || (!email.isEmpty() && !email.contains(QLatin1Char('@')))) {
    m_problem->setText(Text::tr("The author is a name and an email address."));
    return;
  }
  const QJsonObject answer = projectCommand(m_root, {{QStringLiteral("cmd"), QStringLiteral("set_identity")},
                                                     {QStringLiteral("name"), name},
                                                     {QStringLiteral("email"), email}});
  if (!errorClassOf(answer).isEmpty()) {
    m_problem->setText(errorMessageOf(answer));
    return;
  }
  m_problem->clear();
  qInfo().noquote() << QStringLiteral("Project Settings: author %1 <%2>").arg(name, email);
}

} // namespace

ProjectSettingsFields::ProjectSettingsFields(Host host, QWidget* parent) : QWidget(parent) {
  // New Project uses S F L O I N C and the Cloud section V R Y W D E B P K:
  // there the fields take T U A H M, or none.
  const bool creating = host == Host::NewProject;
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  // For everyone.
  m_everyone = new QGroupBox(Text::tr("For everyone in this project (saved as a version)"));
  mac::styleSection(m_everyone);
  auto* everyone = new QFormLayout(m_everyone);
  m_locks = new QCheckBox(creating ? Text::tr("Edi&t locks") : Text::tr("&Edit locks"));
  m_locks->setToolTip(Text::tr("A design has one editor at a time; the others open it read-only and can ask for it"));
  m_idle = new QSpinBox;
  m_idle->setRange(1, 120);
  m_idle->setSuffix(Text::tr(" min"));
  m_poll = new QSpinBox;
  m_poll->setRange(5, 600);
  m_poll->setSuffix(Text::tr(" s"));
  auto* locksRow = new QHBoxLayout;
  locksRow->addWidget(m_locks);
  locksRow->addSpacing(12);
  auto* idleLabel = new QLabel(creating ? Text::tr("Idle time:") : Text::tr("Idle &time:"));
  idleLabel->setBuddy(m_idle);
  locksRow->addWidget(idleLabel);
  locksRow->addWidget(m_idle);
  auto* pollLabel = new QLabel(creating ? Text::tr("Poll:") : Text::tr("&Poll:"));
  pollLabel->setBuddy(m_poll);
  locksRow->addWidget(pollLabel);
  locksRow->addWidget(m_poll);
  locksRow->addStretch(1);
  everyone->addRow(locksRow);
  m_locksNote = note();
  setErrorStyleSheet(m_locksNote);
  m_locksNote->hide();
  everyone->addRow(m_locksNote);
  m_liveNone = new QRadioButton(creating ? Text::tr("None") : Text::tr("&None"));
  m_liveBroker = new QRadioButton(creating ? Text::tr("Broker:") : Text::tr("B&roker:"));
  auto* liveGroup = new QButtonGroup(this);
  liveGroup->addButton(m_liveNone);
  liveGroup->addButton(m_liveBroker);
  m_broker = new QLineEdit;
  m_broker->setPlaceholderText(QStringLiteral("mqtts://broker.example.com:8883"));
  m_prefix = new QLineEdit;
  m_prefix->setPlaceholderText(QStringLiteral("mitcad"));
  m_prefix->setMaximumWidth(120);
  m_test = new QPushButton(creating ? Text::tr("Test") : Text::tr("Te&st"));
  m_test->setAutoDefault(false);
  auto* liveColumn = new QVBoxLayout;
  liveColumn->addWidget(m_liveNone);
  auto* brokerRow = new QHBoxLayout;
  brokerRow->addWidget(m_liveBroker);
  brokerRow->addWidget(m_broker, 1);
  brokerRow->addWidget(new QLabel(Text::tr("Prefix:")));
  brokerRow->addWidget(m_prefix);
  brokerRow->addWidget(m_test);
  liveColumn->addLayout(brokerRow);
  everyone->addRow(Text::tr("Live updates:"), liveColumn);
  layout->addWidget(m_everyone);

  // On this computer.
  auto* here = new QGroupBox(Text::tr("On this computer"));
  mac::styleSection(here);
  auto* hereForm = new QFormLayout(here);
  // The name the Projects panel shows (mitcad#152): this computer's, a
  // display name only.
  m_displayName = new QLineEdit;
  m_displayName->setObjectName(QStringLiteral("projectDisplayName"));
  m_displayName->setMaxLength(200);
  m_displayName->setClearButtonEnabled(true);
  auto* displayLabel = new QLabel(creating ? Text::tr("Display na&me:") : Text::tr("&Display name:"));
  displayLabel->setBuddy(m_displayName);
  hereForm->addRow(displayLabel, m_displayName);
  m_name = new QLineEdit;
  m_name->setObjectName(QStringLiteral("projectAuthorName"));
  m_name->setPlaceholderText(Text::tr("Your Name"));
  m_email = new QLineEdit;
  m_email->setObjectName(QStringLiteral("projectAuthorEmail"));
  m_email->setPlaceholderText(Text::tr("you@example.com"));
  auto* authorRow = new QHBoxLayout;
  authorRow->addWidget(m_name, 1);
  authorRow->addWidget(m_email, 1);
  auto* authorLabel = new QLabel(creating ? Text::tr("A&uthor:") : Text::tr("&Author:"));
  authorLabel->setBuddy(m_name);
  hereForm->addRow(authorLabel, authorRow);
  m_sendAtOnce = new QCheckBox(Text::tr("Send each version at once"));
  hereForm->addRow(QString(), m_sendAtOnce);
  m_check = new QComboBox;
  const RemoteSettings defaults = RemoteSettings::load();
  for (const int minutes : kCheckChoices) {
    m_check->addItem(minutes < 0 ? Text::tr("as in Preferences (%1)").arg(checkText(defaults.checkMinutes))
                                 : checkText(minutes),
                     minutes);
  }
  auto* checkLabel = new QLabel(creating ? Text::tr("C&heck for newer versions:") : Text::tr("&Check for newer versions:"));
  checkLabel->setBuddy(m_check);
  hereForm->addRow(checkLabel, m_check);
  m_liveHere = new QComboBox;
  m_liveHere->addItem(Text::tr("Use the project's"), QStringLiteral("project"));
  m_liveHere->addItem(Text::tr("Off"), QStringLiteral("off"));
  m_liveHere->addItem(Text::tr("Another broker"), QStringLiteral("broker"));
  m_hereBroker = new QLineEdit;
  m_hereBroker->setPlaceholderText(QStringLiteral("mqtts://broker.example.com:8883"));
  m_herePrefix = new QLineEdit;
  m_herePrefix->setPlaceholderText(QStringLiteral("mitcad"));
  m_herePrefix->setMaximumWidth(120);
  m_hereRow = new QWidget;
  auto* hereRow = new QHBoxLayout(m_hereRow);
  hereRow->setContentsMargins(0, 0, 0, 0);
  hereRow->addWidget(m_hereBroker, 1);
  hereRow->addWidget(new QLabel(Text::tr("Prefix:")));
  hereRow->addWidget(m_herePrefix);
  auto* liveHereLabel = new QLabel(creating ? Text::tr("Live upd&ates here:") : Text::tr("L&ive updates here:"));
  liveHereLabel->setBuddy(m_liveHere);
  hereForm->addRow(liveHereLabel, m_liveHere);
  hereForm->addRow(QString(), m_hereRow);
  layout->addWidget(here);

  m_locks->setObjectName(QStringLiteral("projectEditLocks"));
  m_idle->setObjectName(QStringLiteral("projectLockIdle"));
  m_poll->setObjectName(QStringLiteral("projectLockPoll"));
  m_liveNone->setObjectName(QStringLiteral("projectLiveNone"));
  m_liveBroker->setObjectName(QStringLiteral("projectLiveBroker"));
  m_broker->setObjectName(QStringLiteral("projectBroker"));
  m_prefix->setObjectName(QStringLiteral("projectBrokerPrefix"));
  m_sendAtOnce->setObjectName(QStringLiteral("projectSendAtOnce"));
  m_check->setObjectName(QStringLiteral("projectCheckMinutes"));
  m_liveHere->setObjectName(QStringLiteral("projectLiveHere"));
  m_hereBroker->setObjectName(QStringLiteral("projectHereBroker"));
  m_herePrefix->setObjectName(QStringLiteral("projectHerePrefix"));
  m_displayName->setToolTip(Text::tr("The name shown on this computer only. Empty uses the repository address "
                             "or folder name."));
  m_name->setMaxLength(200);
  m_email->setMaxLength(200);
  m_locks->setChecked(true);
  m_idle->setValue(10);
  m_poll->setValue(10);
  m_liveNone->setChecked(true);
  m_sendAtOnce->setChecked(defaults.autoPush);
  m_test->hide(); // Existing-project action; its host enables it in Project Settings.
  m_broker->setEnabled(false);
  m_prefix->setEnabled(false);
  m_hereRow->hide();
  connect(m_liveNone, &QRadioButton::toggled, this, [this](bool none) {
    m_broker->setEnabled(!none);
    m_prefix->setEnabled(!none);
  });
  connect(m_liveHere, &QComboBox::currentIndexChanged, this, [this] {
    m_hereRow->setVisible(m_liveHere->currentData().toString() == QLatin1String("broker"));
  });
}

QJsonObject ProjectSettingsFields::sharedChosen() const {
  QJsonObject shared{{QStringLiteral("edit_locks"), QJsonObject{{QStringLiteral("enabled"), m_locks->isChecked()},
                                                                {QStringLiteral("idle_minutes"), m_idle->value()},
                                                                {QStringLiteral("poll_seconds"), m_poll->value()}}}};
  if (m_liveBroker->isChecked()) {
    QJsonObject live{{QStringLiteral("broker"), m_broker->text().trimmed()}};
    if (!m_prefix->text().trimmed().isEmpty()) {
      live.insert(QStringLiteral("prefix"), m_prefix->text().trimmed());
    }
    shared.insert(QStringLiteral("live_updates"), live);
  } else {
    shared.insert(QStringLiteral("live_updates"), QJsonValue());
  }
  return shared;
}

QJsonObject ProjectSettingsFields::localChosen(bool includeName) const {
  const QString mode = m_liveHere->currentData().toString();
  QJsonObject live{{QStringLiteral("mode"), mode}};
  if (mode == QLatin1String("broker")) {
    live.insert(QStringLiteral("broker"), m_hereBroker->text().trimmed());
    if (!m_herePrefix->text().trimmed().isEmpty()) {
      live.insert(QStringLiteral("prefix"), m_herePrefix->text().trimmed());
    }
  }
  const RemoteSettings defaults = RemoteSettings::load();
  const int minutes = m_check->currentData().toInt();
  QJsonObject sync;
  // A choice equal to the default stays the default (Preferences can
  // change it later).
  sync.insert(QStringLiteral("send_at_once"),
              m_sendAtOnce->isChecked() == defaults.autoPush ? QJsonValue() : QJsonValue(m_sendAtOnce->isChecked()));
  sync.insert(QStringLiteral("check_minutes"), minutes < 0 ? QJsonValue() : QJsonValue(minutes));
  QJsonObject local{{QStringLiteral("live"), live}, {QStringLiteral("sync"), sync}};
  if (includeName) {
    const QString name = m_displayName->text().trimmed();
    local.insert(QStringLiteral("name"), name.isEmpty() ? QJsonValue() : QJsonValue(name));
  }
  return local;
}

void showProjectSettings(QWidget* parent, const QString& root, const ProjectSettingsHost& host) {
  bool sync = false;
  {
    ProjectSettingsDialog dialog(parent, root, host);
    prepareModal(&dialog);
    execDialog(&dialog);
    sync = dialog.syncRequested();
  }
  if (sync && host.sync) {
    host.sync();
  }
}

ProjectSync projectSync(const QString& root) {
  const RemoteSettings defaults = RemoteSettings::load();
  ProjectSync sync;
  sync.sendAtOnce = defaults.autoPush;
  sync.checkMinutes = defaults.checkMinutes;
  if (root.isEmpty()) {
    return sync;
  }
  const QJsonObject settings = projectCommand(root, named("project_settings"));
  const QJsonObject local =
      settings.value(QStringLiteral("local")).toObject().value(QStringLiteral("sync")).toObject();
  if (local.value(QStringLiteral("send_at_once")).isBool()) {
    sync.sendAtOnce = local.value(QStringLiteral("send_at_once")).toBool();
  }
  if (local.value(QStringLiteral("check_minutes")).isDouble()) {
    sync.checkMinutes = local.value(QStringLiteral("check_minutes")).toInt();
  }
  return sync;
}

} // namespace mitcad
