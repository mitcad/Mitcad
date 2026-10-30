// SPDX-License-Identifier: MIT
#include "CloudSection.hpp"

#include <utility>

#include <QApplication>
#include <QButtonGroup>
#include <QClipboard>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPointer>
#include <QPushButton>
#include <QRadioButton>
#include <QTimer>
#include <QToolButton>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../report/ReportCenter.hpp"
#include "Projects.hpp"
#include "RemoteTask.hpp"

#include "../platform/MacChrome.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

// The pause after typing before the address is checked.
constexpr int kCheckDelayMs = 600;
// The focus back within this long of the last check checks nothing.
constexpr int kRecheckSeconds = 2;

const CloudService kServices[] = {CloudService::GitHub, CloudService::GitLab, CloudService::Forgejo,
                                  CloudService::SharedFolder, CloudService::Other};

bool knownService(CloudService service) {
  return service == CloudService::GitHub || service == CloudService::GitLab || service == CloudService::Forgejo;
}

// A folder that is a git repository (bare, or with its .git).
bool isRepositoryFolder(const QDir& dir) {
  return dir.exists(QStringLiteral(".git")) ||
         (QFileInfo(dir.filePath(QStringLiteral("HEAD"))).isFile() && dir.exists(QStringLiteral("objects")) &&
          dir.exists(QStringLiteral("refs")));
}

QJsonObject problem(const QString& cls, const QString& message) {
  return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), cls},
                                                {QStringLiteral("message"), message},
                                                {QStringLiteral("detail"), QString()}}}};
}

// The host and port of an SSH address (22 unless ssh://host:port/ says).
std::pair<QString, int> sshHostPort(const QString& url) {
  const QString host = addressHost(url);
  int port = 22;
  if (url.trimmed().startsWith(QLatin1String("ssh://"))) {
    const int given = QUrl(url.trimmed()).port();
    if (given > 0) {
      port = given;
    }
  }
  return {host, port};
}

} // namespace

CloudSection::CloudSection(Checker checker, Describe describe, QWidget* parent)
    : QWidget(parent), m_checker(std::move(checker)), m_describe(std::move(describe)) {
  setObjectName(QStringLiteral("cloudSection"));
  m_delay = new QTimer(this);
  m_delay->setSingleShot(true);
  m_delay->setInterval(kCheckDelayMs);
  TestSync::watch(m_delay);
  connect(m_delay, &QTimer::timeout, this, &CloudSection::startCheck);
  build();
  showFields();
}

CloudSection::~CloudSection() { stopCheck(); }

void CloudSection::build() {
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  m_form = new QFormLayout;
  column->addLayout(m_form);
  // The address comes first: it is what people have (the clone button of
  // their server, the address bar of the repository's page) and paste;
  // the fields under Details are derived from it and stay editable.
  m_url = new QLineEdit;
  m_url->setObjectName(QStringLiteral("cloudAddress"));
  m_url->setMinimumWidth(380);
  m_url->setPlaceholderText(tr("https://… or git@…:owner/repo.git"));
  m_url->setToolTip(tr("The repository's address, as your server shows it, or a folder"));
  m_form->addRow(tr("Repository addr&ess:"), m_url);
  m_details = new QToolButton;
  m_details->setObjectName(QStringLiteral("cloudDetails"));
  m_details->setText(tr("Details"));
  m_details->setCheckable(true);
  m_details->setAutoRaise(true);
  m_details->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
#ifdef Q_OS_MACOS
  m_details->setIcon(mac::symbolIcon(QStringLiteral("chevron.right")));
#else
  m_details->setArrowType(Qt::RightArrow);
#endif
  m_details->setToolTip(tr("Show or change the service, server, owner and repository the address is made of"));
  m_summary = new QLabel;
  m_summary->setObjectName(QStringLiteral("cloudSummary"));
  m_summary->setTextFormat(Qt::PlainText);
  m_summary->setWordWrap(true);
  auto* detailsRow = new QHBoxLayout;
  detailsRow->addWidget(m_details);
  detailsRow->addWidget(m_summary, 1);
  m_form->addRow(QString(), detailsRow);
  m_service = new QComboBox;
  for (const CloudService service : kServices) {
    m_service->addItem(cloudServiceName(service), static_cast<int>(service));
  }
  m_form->addRow(tr("Ser&vice:"), m_service);
  m_server = new QLineEdit;
  m_server->setPlaceholderText(QStringLiteral("codeberg.org"));
  m_serverLabel = new QLabel(tr("Serve&r:"));
  m_serverLabel->setBuddy(m_server);
  m_form->addRow(m_serverLabel, m_server);
  // The owner: a user or an organisation; on GitLab also a group with its
  // subgroups (group/sub), so the field takes slashes.
  m_owner = new QLineEdit;
  m_owner->setPlaceholderText(tr("you or an organisation"));
  m_repository = new QLineEdit;
  m_repository->setPlaceholderText(tr("repository"));
  m_ownerRow = new QWidget;
  auto* ownerLayout = new QHBoxLayout(m_ownerRow);
  ownerLayout->setContentsMargins(0, 0, 0, 0);
  ownerLayout->addWidget(m_owner, 1);
  auto* repositoryLabel = new QLabel(tr("Repositor&y:"));
  repositoryLabel->setBuddy(m_repository);
  ownerLayout->addWidget(repositoryLabel);
  ownerLayout->addWidget(m_repository, 1);
  m_ownerLabel = new QLabel(tr("O&wner:"));
  m_ownerLabel->setBuddy(m_owner);
  m_form->addRow(m_ownerLabel, m_ownerRow);
  m_https = new QRadioButton(QStringLiteral("HTTPS"));
  m_ssh = new QRadioButton(QStringLiteral("SSH"));
  m_https->setChecked(true);
  auto* group = new QButtonGroup(this);
  group->addButton(m_https);
  group->addButton(m_ssh);
  m_connectRow = new QWidget;
  auto* connectLayout = new QHBoxLayout(m_connectRow);
  connectLayout->setContentsMargins(0, 0, 0, 0);
  connectLayout->addWidget(m_https);
  connectLayout->addWidget(m_ssh);
  connectLayout->addStretch(1);
  m_connectLabel = new QLabel(tr("Connect:"));
  m_form->addRow(m_connectLabel, m_connectRow);
  m_folder = new QLineEdit;
  m_folder->setPlaceholderText(tr("A folder others can reach: an empty one, or a repository"));
  auto* browse = new QPushButton(tr("Browse…"));
  browse->setAutoDefault(false);
  m_folderRow = new QWidget;
  auto* folderLayout = new QHBoxLayout(m_folderRow);
  folderLayout->setContentsMargins(0, 0, 0, 0);
  folderLayout->addWidget(m_folder, 1);
  folderLayout->addWidget(browse);
  m_folderLabel = new QLabel(tr("Repository fol&der:"));
  m_folderLabel->setBuddy(m_folder);
  m_form->addRow(m_folderLabel, m_folderRow);
  m_create =new QPushButton(tr("Create Repository in &Browser…"));
  m_create->setAutoDefault(false);
  m_create->setToolTip(tr("Make an empty repository on the service's page, then come back: the address is "
                          "checked again"));
  auto* createRow = new QHBoxLayout;
  createRow->addWidget(m_create);
  createRow->addStretch(1);
  m_form->addRow(QString(), createRow);
  m_status = new QLabel;
  m_status->setObjectName(QStringLiteral("cloudStatus"));
  m_status->setWordWrap(true);
  m_status->setTextFormat(Qt::PlainText);
  column->addWidget(m_status);
  m_fixes = new QWidget;
  auto* fixes = new QVBoxLayout(m_fixes);
  fixes->setContentsMargins(0, 0, 0, 0);
  m_fixText = new QLabel;
  m_fixText->setWordWrap(true);
  m_fixText->setTextFormat(Qt::PlainText);
  fixes->addWidget(m_fixText);
  auto* fixButtons = new QHBoxLayout;
  m_copyKey = new QPushButton(tr("Copy &Public Key"));
  m_keysPage = new QPushButton(tr("SSH &Keys Page…"));
  m_trust = new QPushButton(tr("Trust This Server…"));
  for (QPushButton* button : {m_copyKey, m_keysPage, m_trust}) {
    button->setAutoDefault(false);
    fixButtons->addWidget(button);
  }
  fixButtons->addStretch(1);
  fixes->addLayout(fixButtons);
  column->addWidget(m_fixes);
  m_fixes->hide();

  connect(m_service, &QComboBox::currentIndexChanged, this, [this] {
    if (!m_updating) {
      m_address.service = static_cast<CloudService>(m_service->currentData().toInt());
      showFields();
      fieldsChanged();
    }
  });
  // A repository address pasted into the Server, Owner or Repository field
  // is read as an address, not kept as a host name or a name.
  connect(m_server, &QLineEdit::textEdited, this, [this](const QString& text) {
    if (!addressPasted(m_server, text, true)) {
      fieldsChanged();
    }
  });
  connect(m_owner, &QLineEdit::textEdited, this, [this](const QString& text) {
    if (!addressPasted(m_owner, text, false)) {
      fieldsChanged();
    }
  });
  connect(m_folder, &QLineEdit::textEdited, this, [this] { fieldsChanged(); });
  connect(m_repository, &QLineEdit::textEdited, this, [this](const QString& text) {
    if (addressPasted(m_repository, text, false)) {
      return;
    }
    m_repositoryTyped = true;
    fieldsChanged();
  });
  connect(m_details, &QToolButton::toggled, this, [this](bool open) {
    m_detailsOpen = open;
    showFields();
    // The fields as they stand, for the owner's log.
    qInfo().noquote() << QStringLiteral("Cloud section: details %1 (service %2, server %3, owner %4, repository %5)")
                             .arg(open ? QStringLiteral("shown") : QStringLiteral("hidden"), m_service->currentText(),
                                  m_server->text(), m_owner->text(), m_repository->text());
    // The window as tall as its contents now need, within the screen
    // (adjustSize would stop at two thirds of the screen).
    fitWindowToContents(window());
    emit detailsToggled(open);
  });
  connect(m_https, &QRadioButton::toggled, this, [this] {
    if (!m_updating) {
      fieldsChanged();
    }
  });
  connect(m_url, &QLineEdit::textEdited, this, &CloudSection::addressEdited);
  connect(browse, &QPushButton::clicked, this, [this] {
    const QString chosen = dialogDirectory(this, tr("Repository Folder"), m_folder->text());
    if (!chosen.isEmpty()) {
      m_folder->setText(QDir::toNativeSeparators(chosen));
      fieldsChanged();
    }
  });
  connect(m_create, &QPushButton::clicked, this, [this] {
    openPage(m_address.newRepositoryPage(), QStringLiteral("new repository page"));
  });
  connect(m_copyKey, &QPushButton::clicked, this, &CloudSection::copyPublicKey);
  connect(m_keysPage, &QPushButton::clicked, this,
          [this] { openPage(m_address.sshKeysPage(), QStringLiteral("SSH keys page")); });
  connect(m_trust, &QPushButton::clicked, this, &CloudSection::trustServer);
}

QWidget* CloudSection::firstField() const {
  // The address is what is pasted first; the details follow from it.
  return m_url;
}

void CloudSection::showFields() {
  const CloudService service = m_address.service;
  const bool known = knownService(service);
#ifdef Q_OS_MACOS
  m_details->setIcon(mac::symbolIcon(m_detailsOpen ? QStringLiteral("chevron.down") : QStringLiteral("chevron.right")));
#else
  m_details->setArrowType(m_detailsOpen ? Qt::DownArrow : Qt::RightArrow);
#endif
  m_form->setRowVisible(m_service, m_detailsOpen);
  m_form->setRowVisible(m_serverLabel, m_detailsOpen && service == CloudService::Forgejo);
  m_form->setRowVisible(m_ownerLabel, m_detailsOpen && known);
  m_form->setRowVisible(m_connectLabel, m_detailsOpen && known);
  m_form->setRowVisible(m_folderLabel, m_detailsOpen && service == CloudService::SharedFolder);
  m_create->setVisible(m_detailsOpen && known);
  updateSummary();
}

void CloudSection::updateSummary() {
  const CloudAddress& a = m_address;
  QString text;
  switch (a.service) {
  case CloudService::GitHub:
  case CloudService::GitLab:
  case CloudService::Forgejo:
    if (!a.owner.isEmpty() || !a.repository.isEmpty()) {
      const QString host = cloudServiceHost(a.service, a.server);
      text = tr("%1%2: %3/%4 over %5")
                 .arg(cloudServiceName(a.service), host.isEmpty() || a.service != CloudService::Forgejo
                                                       ? QString()
                                                       : QStringLiteral(" (%1)").arg(host),
                      a.owner, a.repository, a.connect == CloudConnect::Ssh ? QStringLiteral("SSH") : QStringLiteral("HTTPS"));
    }
    break;
  case CloudService::SharedFolder:
    if (!a.folder.isEmpty()) {
      text = tr("Shared folder: %1").arg(a.folder);
    }
    break;
  case CloudService::Other:
    if (!a.other.isEmpty()) {
      text = tr("Other address, used as it is");
    }
    break;
  }
  m_summary->setText(text);
}

void CloudSection::setDefaultConnect(CloudConnect connect) {
  const QSignalBlocker blocker(m_https);
  m_address.connect = connect;
  m_https->setChecked(connect == CloudConnect::Https);
  m_ssh->setChecked(connect == CloudConnect::Ssh);
  fieldsChanged();
}

void CloudSection::setFolderName(const QString& folderName) {
  if (m_repositoryTyped) {
    return;
  }
  m_repository->setText(repositoryNameFor(folderName));
  fieldsChanged();
}

void CloudSection::setField(QLineEdit* edit, const QString& text) {
  edit->setText(text);
  // What the field held when it was last set: a paste is a change by more
  // than one character at once.
  edit->setProperty("pasteBase", text);
}

// The controls from m_address (the address field not included).
void CloudSection::showAddress() {
  m_updating = true;
  m_service->setCurrentIndex(m_service->findData(static_cast<int>(m_address.service)));
  if (knownService(m_address.service)) {
    setField(m_server, m_address.server);
    setField(m_owner, m_address.owner);
    setField(m_repository, m_address.repository);
    m_https->setChecked(m_address.connect == CloudConnect::Https);
    m_ssh->setChecked(m_address.connect == CloudConnect::Ssh);
  }
  if (m_address.service == CloudService::SharedFolder) {
    setField(m_folder, m_address.folder);
  }
  m_updating = false;
}

void CloudSection::setAddress(const QString& url) {
  m_address = parseCloudAddress(url, m_server->text());
  showAddress();
  m_repositoryTyped = !url.trimmed().isEmpty();
  showFields();
  m_url->setText(m_address.service == CloudService::SharedFolder ? QDir::toNativeSeparators(m_address.url())
                                                                 : m_address.url());
  scheduleCheck();
  emit addressChanged(this->url());
}

bool CloudSection::addressPasted(QLineEdit* edit, const QString& text, bool bareHost) {
  const QString before = edit->property("pasteBase").toString();
  edit->setProperty("pasteBase", text);
  if (qAbs(text.size() - before.size()) < 2 || !looksLikeRepositoryAddress(text, bareHost)) {
    return false;
  }
  // A whole address: read as the address field's own. The field gets what
  // it held before back (or the part the address has for it).
  const CloudAddress parsed = parseCloudAddress(text, edit == m_server ? QString() : m_server->text());
  if (!knownService(parsed.service)) {
    setField(edit, before);
  }
  m_address = parsed;
  showAddress();
  if (knownService(parsed.service)) {
    m_repositoryTyped = true;
  }
  showFields();
  m_url->setText(parsed.service == CloudService::SharedFolder ? QDir::toNativeSeparators(parsed.url()) : parsed.url());
  scheduleCheck();
  emit addressChanged(url());
  return true;
}

void CloudSection::fieldsChanged() {
  if (m_updating) {
    return;
  }
  m_address.server = m_server->text().trimmed();
  m_address.owner = m_owner->text().trimmed();
  m_address.repository = m_repository->text().trimmed();
  m_address.folder = m_folder->text().trimmed();
  m_address.connect = m_ssh->isChecked() ? CloudConnect::Ssh : CloudConnect::Https;
  if (m_address.service != CloudService::Other) {
    const QString made = m_address.url();
    m_url->setText(m_address.service == CloudService::SharedFolder ? QDir::toNativeSeparators(made) : made);
  } else {
    // As the address field holds it, read the way git is given it (the
    // command and quotes of a pasted line off, https:// for host/path).
    const CloudAddress typed = parseCloudAddress(m_url->text(), m_server->text());
    m_address.other = typed.service == CloudService::Other ? typed.other : m_url->text().trimmed();
  }
  updateSummary();
  scheduleCheck();
  emit addressChanged(url());
}

void CloudSection::addressEdited(const QString& text) {
  // A typed address fills the fields it can (another service's address
  // switches the service).
  m_address = parseCloudAddress(text, m_server->text());
  showAddress();
  if (knownService(m_address.service)) {
    m_repositoryTyped = true;
  }
  showFields();
  scheduleCheck();
  emit addressChanged(url());
}

QString CloudSection::url() const { return m_address.url(); }

QString CloudSection::statusText() const { return m_status->text(); }

void CloudSection::setStatus(const QString& text, bool problem) {
  m_status->setText(text);
  if (problem) {
    setErrorStyleSheet(m_status);
  } else {
    m_status->setStyleSheet(QString());
  }
}

void CloudSection::scheduleCheck() {
  const QString address = url();
  if (address != m_answerUrl) {
    // Another address: what was found does not hold for it.
    m_answer = QJsonObject();
    m_answerUrl.clear();
    m_answerOk = false;
    m_needsBare = false;
    m_fixes->hide();
    setStatus(address.isEmpty() ? QString() : tr("Checking %1…").arg(address), false);
  }
  m_delay->start();
}

void CloudSection::checkNow() {
  m_delay->stop();
  startCheck();
}

void CloudSection::recheck() {
  if (url().isEmpty() || m_delay->isActive() || m_task != nullptr) {
    return;
  }
  if (m_checkedAt.isValid() && m_checkedAt.secsTo(QDateTime::currentDateTimeUtc()) < kRecheckSeconds) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Cloud check again: %1").arg(url());
  startCheck();
}

void CloudSection::stopCheck() {
  m_delay->stop();
  if (m_task != nullptr) {
    RemoteTask* task = m_task;
    m_task = nullptr;
    disconnect(task, nullptr, this, nullptr);
    task->cancel();
    // Gone once git has stopped; the section may go first (it waits then).
    connect(task, &RemoteTask::finished, task, &QObject::deleteLater);
  }
}

void CloudSection::startCheck() {
  const QString address = url();
  stopCheck();
  if (address.isEmpty()) {
    return;
  }
  if (addressHasPassword(address)) {
    finishCheck(address, problem(QStringLiteral("invalid_url"),
                                 tr("The address holds a password or a token. MitCad never gives them to git: use "
                                    "the address without it, and sign in with an SSH key or a credential helper.")));
    return;
  }
  // The address as the fields read it (the owner's log shows what a pasted
  // address made of them).
  if (knownService(m_address.service)) {
    qInfo().noquote() << QStringLiteral("Cloud address: service %1, server %2, owner %3, repository %4, %5")
                             .arg(cloudServiceName(m_address.service),
                                  m_address.service == CloudService::Forgejo ? m_address.server : QStringLiteral("-"),
                                  m_address.owner, m_address.repository,
                                  m_address.connect == CloudConnect::Ssh ? QStringLiteral("SSH") : QStringLiteral("HTTPS"));
  } else {
    qInfo().noquote() << QStringLiteral("Cloud address: service %1").arg(cloudServiceName(m_address.service));
  }
  // Before git is asked: what is no address at all is said so (git would
  // read it as a path in the program's folder).
  const QString invalid = m_address.service == CloudService::SharedFolder
                              ? (QFileInfo(address).isAbsolute()
                                     ? QString()
                                     : tr("This is not a valid repository address: %1").arg(QDir::toNativeSeparators(address)))
                              : repositoryAddressProblem(address);
  if (!invalid.isEmpty()) {
    finishCheck(address, problem(QStringLiteral("invalid_url"), invalid));
    return;
  }
  if (m_address.service == CloudService::SharedFolder) {
    // A new or empty folder becomes a repository; a repository is checked;
    // anything else is refused.
    const QDir dir(address);
    if (!dir.exists() || dir.isEmpty(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
      finishCheck(address, {{QStringLiteral("url"), address},
                            {QStringLiteral("reachable"), true},
                            {QStringLiteral("empty"), true},
                            {QStringLiteral("new_folder"), true},
                            {QStringLiteral("error"), QJsonValue()}});
      return;
    }
    if (!isRepositoryFolder(dir)) {
      finishCheck(address, problem(QStringLiteral("not_empty"),
                                   tr("%1 has files but no git repository: choose an empty folder, which MitCad "
                                      "makes a repository, or a folder with one.")
                                       .arg(QDir::toNativeSeparators(address))));
      return;
    }
  }
  RemoteTask* task = m_checker(address, this);
  m_task = task;
  m_taskUrl = address;
  connect(task, &RemoteTask::finished, this, [this, task, address] {
    if (task != m_task) {
      return;
    }
    m_task = nullptr;
    const QJsonObject answer = task->answer();
    task->deleteLater();
    finishCheck(address, answer);
  });
  qInfo().noquote() << QStringLiteral("Cloud check started: %1").arg(address);
  task->start();
}

void CloudSection::finishCheck(const QString& address, const QJsonObject& answer) {
  if (address != url()) {
    return; // the address changed meanwhile: its own check comes
  }
  m_checkedAt = QDateTime::currentDateTimeUtc();
  m_answer = answer;
  m_answerUrl = address;
  m_needsBare = answer.value(QStringLiteral("new_folder")).toBool();
  const QString cls = errorClassOf(answer);
  QString text;
  bool ok = cls.isEmpty();
  if (!cls.isEmpty()) {
    text = errorMessageOf(answer);
    if (cls == QLatin1String("not_found")) {
      text = tr("No repository at %1. Make one (Create Repository in Browser), then come back.").arg(address);
    } else if (cls == QLatin1String("git_missing")) {
      text = tr("Cloud projects need the git program, which was not found: install git "
                "(https://git-scm.com/downloads) and start MitCad again.");
    }
  } else if (m_describe) {
    text = m_describe(answer, ok);
  }
  m_answerOk = ok;
  setStatus(text, !ok);
  showFixes(cls);
  qInfo().noquote() << QStringLiteral("Cloud check %1: %2")
                           .arg(address, cls.isEmpty() ? text : QStringLiteral("%1: %2").arg(cls, text));
  emit checked();
}

void CloudSection::showFixes(const QString& cls) {
  m_copyKey->hide();
  m_keysPage->hide();
  m_trust->hide();
  QString text;
  const QString address = url();
  if (cls == QLatin1String("auth_failed")) {
    if (isSshAddress(address)) {
      const QJsonObject key = projectsCommand({{QStringLiteral("cmd"), QStringLiteral("ssh_public_key")}});
      const bool hasKey = !key.value(QStringLiteral("text")).toString().isEmpty();
      text = hasKey ? tr("MitCad signs in with your SSH key: add its public key to your own user account on the "
                         "service (also for an organisation's repository), then come back.")
                    : tr("MitCad signs in with an SSH key, and this computer has none: make one (ssh-keygen -t "
                         "ed25519), add its public key to your own user account on the service (also for an "
                         "organisation's repository), then come back.");
      m_copyKey->setVisible(hasKey);
      m_keysPage->show();
    } else {
      text = tr("For https:// addresses git signs in with a credential helper: Git Credential Manager comes with "
                "Git for Windows; elsewhere set git's credential.helper. MitCad never asks for a password or a "
                "token.");
    }
  } else if (cls == QLatin1String("host_key_unknown")) {
    text = tr("This computer does not know the server's identity yet. Check its fingerprint and trust it.");
    m_trust->show();
  }
  m_fixText->setText(text);
  m_fixes->setVisible(!text.isEmpty());
}

void CloudSection::copyPublicKey() {
  const QJsonObject key = projectsCommand({{QStringLiteral("cmd"), QStringLiteral("ssh_public_key")}});
  const QString text = key.value(QStringLiteral("text")).toString().trimmed();
  if (text.isEmpty()) {
    return;
  }
  QApplication::clipboard()->setText(text);
  qInfo().noquote() << QStringLiteral("Copied the public key %1").arg(key.value(QStringLiteral("path")).toString());
  setStatus(tr("The public key %1 is on the clipboard: paste it on the service's SSH keys page.")
                .arg(QDir::toNativeSeparators(key.value(QStringLiteral("path")).toString())),
            false);
}

void CloudSection::trustServer() {
  const auto [host, port] = sshHostPort(url());
  if (host.isEmpty()) {
    return;
  }
  if (askTrustServer(window(), host, port)) {
    checkNow();
  }
}

void CloudSection::openPage(const QString& page, const QString& what) {
  if (page.isEmpty()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Cloud section: open the %1 %2").arg(what, page);
  openExternalUrl(QUrl(page));
}

// ---------------------------------------------------------------------------
// The server's identity

bool askTrustServer(QWidget* parent, const QString& host, int port) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QCoreApplication::translate("CloudSection", "Trust This Server"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* text = new QLabel(QCoreApplication::translate("CloudSection", "Fetching the identity of %1…").arg(host));
  text->setWordWrap(true);
  text->setTextFormat(Qt::PlainText);
  layout->addWidget(text);
  auto* keys = new QLabel;
  keys->setTextFormat(Qt::PlainText);
  keys->setTextInteractionFlags(Qt::TextSelectableByMouse);
  QFont mono = keys->font();
  mono.setFamily(QStringLiteral("monospace"));
  mono.setStyleHint(QFont::Monospace);
  keys->setFont(mono);
  layout->addWidget(keys);
  auto* verdict = new QLabel;
  verdict->setWordWrap(true);
  verdict->setTextFormat(Qt::PlainText);
  layout->addWidget(verdict);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  QPushButton* trust =
      buttons->addButton(QCoreApplication::translate("CloudSection", "&Trust This Server"), QDialogButtonBox::AcceptRole);
  trust->setEnabled(false);
  layout->addWidget(buttons);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);

  QJsonObject scan;
  RemoteTask* task = RemoteTask::projects(
      {{QStringLiteral("cmd"), QStringLiteral("host_keys")}, {QStringLiteral("host"), host}, {QStringLiteral("port"), port}},
      &dialog);
  QObject::connect(task, &RemoteTask::finished, &dialog, [&] {
    scan = task->answer();
    const QString cls = errorClassOf(scan);
    if (!cls.isEmpty()) {
      text->setText(QCoreApplication::translate("CloudSection", "Could not fetch the identity of %1: %2")
                        .arg(host, errorMessageOf(scan)));
      qInfo().noquote() << QStringLiteral("Trust This Server: %1 failed (%2): %3").arg(host, cls, errorMessageOf(scan));
      return;
    }
    QStringList lines;
    QStringList logged;
    for (const QJsonValue& value : scan.value(QStringLiteral("keys")).toArray()) {
      const QJsonObject key = value.toObject();
      lines << QStringLiteral("%1  %2").arg(key.value(QStringLiteral("type")).toString(),
                                            key.value(QStringLiteral("fingerprint")).toString());
      logged << key.value(QStringLiteral("fingerprint")).toString();
    }
    keys->setText(lines.join(QLatin1Char('\n')));
    const QString service = scan.value(QStringLiteral("service")).toString();
    const QString published = scan.value(QStringLiteral("published")).toString();
    const QString server = port == 22 ? host : QStringLiteral("%1:%2").arg(host).arg(port);
    text->setText(QCoreApplication::translate("CloudSection", "The server %1 identifies itself with these keys:")
                      .arg(server));
    QString shown;
    bool can = !lines.isEmpty();
    if (published == QLatin1String("verified")) {
      shown = QCoreApplication::translate("CloudSection", "Verified: the keys %1 publishes.").arg(service);
    } else if (published == QLatin1String("mismatch")) {
      shown = QCoreApplication::translate("CloudSection",
                                          "Warning: these are not the keys %1 publishes. Someone may be between "
                                          "you and the server; MitCad does not trust it.")
                  .arg(service);
      setErrorStyleSheet(verdict);
      can = false;
    } else {
      shown = QCoreApplication::translate("CloudSection",
                                          "Compare the fingerprint with the one your server's administrator gives.");
    }
    if (scan.value(QStringLiteral("known")).toBool()) {
      shown += QLatin1Char(' ') + QCoreApplication::translate("CloudSection", "A key of it is known already.");
    }
    if (scan.value(QStringLiteral("changed")).toBool()) {
      // known_hosts has another key of the same type for it.
      shown += QLatin1Char(' ') + QCoreApplication::translate("CloudSection",
                                                              "This computer knows another key of the server: it "
                                                              "changed its keys, or someone is between you and it. "
                                                              "Ask its administrator before you trust it.");
      setErrorStyleSheet(verdict);
    }
    verdict->setText(shown);
    trust->setEnabled(can);
    if (can) {
      trust->setDefault(true);
    }
    qInfo().noquote() << QStringLiteral("Trust This Server dialog: %1: %2 (%3)")
                             .arg(server, logged.join(QStringLiteral(", ")),
                                  published.isEmpty() ? QStringLiteral("not published") : published);
  });
  task->start();
  prepareModal(&dialog);
  const bool accepted = execDialog(&dialog) == QDialog::Accepted;
  if (!accepted) {
    qInfo().noquote() << QStringLiteral("Trust This Server cancelled");
    return false;
  }
  // Exactly the keys shown, each only if a new scan still gives it (a
  // short wait, on a thread): never what the second scan adds (mitcad#938).
  QJsonArray shownKeys;
  for (const QJsonValue& value : scan.value(QStringLiteral("keys")).toArray()) {
    const QJsonObject key = value.toObject();
    QJsonObject shownKey;
    shownKey.insert(QStringLiteral("type"), key.value(QStringLiteral("type")));
    shownKey.insert(QStringLiteral("key"), key.value(QStringLiteral("key")));
    shownKeys.append(shownKey);
  }
  QObject waiting;
  bool finished = false;
  RemoteTask* trustTask = RemoteTask::projects({{QStringLiteral("cmd"), QStringLiteral("trust_host_key")},
                                                {QStringLiteral("host"), host},
                                                {QStringLiteral("port"), port},
                                                {QStringLiteral("keys"), shownKeys}},
                                               nullptr);
  const QPointer<RemoteTask> pending = trustTask;
  QObject::connect(trustTask, &RemoteTask::finished, &waiting, [&finished] { finished = true; });
  trustTask->start();
  waitDialogUntil([&] { return finished || pending == nullptr; });
  if (pending == nullptr) {
    return false;
  }
  const QJsonObject answer = trustTask->answer();
  delete trustTask;
  if (!errorClassOf(answer).isEmpty()) {
    qInfo().noquote() << QStringLiteral("Trust This Server: %1 refused: %2").arg(host, errorMessageOf(answer));
    sheetWarning(parent, QCoreApplication::translate("CloudSection", "Trust This Server"), errorMessageOf(answer));
    return false;
  }
  qInfo().noquote() << QStringLiteral("Trusted the keys of %1 in %2").arg(host, answer.value(QStringLiteral("path")).toString());
  return true;
}

} // namespace mitcad
