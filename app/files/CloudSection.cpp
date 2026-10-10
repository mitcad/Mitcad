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
#include <QEventLoop>
#include <QFileDialog>
#include <QFileInfo>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../report/ReportCenter.hpp"
#include "Projects.hpp"
#include "RemoteTask.hpp"

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
  m_account = new QLineEdit;
  m_account->setPlaceholderText(tr("you"));
  m_repository = new QLineEdit;
  m_repository->setPlaceholderText(tr("repository"));
  m_accountRow = new QWidget;
  auto* accountLayout = new QHBoxLayout(m_accountRow);
  accountLayout->setContentsMargins(0, 0, 0, 0);
  accountLayout->addWidget(m_account, 1);
  auto* repositoryLabel = new QLabel(tr("Repositor&y:"));
  repositoryLabel->setBuddy(m_repository);
  accountLayout->addWidget(repositoryLabel);
  accountLayout->addWidget(m_repository, 1);
  m_accountLabel = new QLabel(tr("Acco&unt:"));
  m_accountLabel->setBuddy(m_account);
  m_form->addRow(m_accountLabel, m_accountRow);
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
  auto* browse = new QPushButton(tr("Browse..."));
  browse->setAutoDefault(false);
  m_folderRow = new QWidget;
  auto* folderLayout = new QHBoxLayout(m_folderRow);
  folderLayout->setContentsMargins(0, 0, 0, 0);
  folderLayout->addWidget(m_folder, 1);
  folderLayout->addWidget(browse);
  m_folderLabel = new QLabel(tr("Repository fol&der:"));
  m_folderLabel->setBuddy(m_folder);
  m_form->addRow(m_folderLabel, m_folderRow);
  m_url = new QLineEdit;
  m_url->setObjectName(QStringLiteral("cloudAddress"));
  m_url->setMinimumWidth(380);
  m_url->setToolTip(tr("The address git uses; type one, or let the fields above make it"));
  m_form->addRow(tr("Addr&ess:"), m_url);
  m_create = new QPushButton(tr("Create Repository in &Browser..."));
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
  m_keysPage = new QPushButton(tr("SSH &Keys Page..."));
  m_trust = new QPushButton(tr("Trust This Server..."));
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
  for (QLineEdit* edit : {m_server, m_account, m_folder}) {
    connect(edit, &QLineEdit::textEdited, this, [this] { fieldsChanged(); });
  }
  connect(m_repository, &QLineEdit::textEdited, this, [this] {
    m_repositoryTyped = true;
    fieldsChanged();
  });
  connect(m_https, &QRadioButton::toggled, this, [this] {
    if (!m_updating) {
      fieldsChanged();
    }
  });
  connect(m_url, &QLineEdit::textEdited, this, &CloudSection::addressEdited);
  connect(browse, &QPushButton::clicked, this, [this] {
    const QString chosen = QFileDialog::getExistingDirectory(this, tr("Repository Folder"), m_folder->text());
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
  switch (m_address.service) {
  case CloudService::GitHub:
  case CloudService::GitLab:
    return m_account;
  case CloudService::Forgejo:
    return m_server->text().isEmpty() ? m_server : m_account;
  case CloudService::SharedFolder:
    return m_folder;
  case CloudService::Other:
    break;
  }
  return m_url;
}

void CloudSection::showFields() {
  const CloudService service = m_address.service;
  const bool known = knownService(service);
  m_form->setRowVisible(m_serverLabel, service == CloudService::Forgejo);
  m_form->setRowVisible(m_accountLabel, known);
  m_form->setRowVisible(m_connectLabel, known);
  m_form->setRowVisible(m_folderLabel, service == CloudService::SharedFolder);
  m_create->setVisible(known);
  // A known service's address is made of the fields, but can be typed.
  m_url->setPlaceholderText(service == CloudService::Other
                                ? tr("ssh://server/project.git, https://server/project.git or a folder")
                                : QString());
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

void CloudSection::setAddress(const QString& url) {
  m_address = parseCloudAddress(url, m_server->text());
  m_updating = true;
  m_service->setCurrentIndex(m_service->findData(static_cast<int>(m_address.service)));
  if (knownService(m_address.service)) {
    m_server->setText(m_address.server);
    m_account->setText(m_address.account);
    m_repository->setText(m_address.repository);
    m_https->setChecked(m_address.connect == CloudConnect::Https);
    m_ssh->setChecked(m_address.connect == CloudConnect::Ssh);
  }
  if (m_address.service == CloudService::SharedFolder) {
    m_folder->setText(m_address.folder);
  }
  m_updating = false;
  m_repositoryTyped = !url.trimmed().isEmpty();
  showFields();
  m_url->setText(m_address.service == CloudService::SharedFolder ? QDir::toNativeSeparators(m_address.url())
                                                                 : m_address.url());
  scheduleCheck();
  emit addressChanged(this->url());
}

void CloudSection::fieldsChanged() {
  if (m_updating) {
    return;
  }
  m_address.server = m_server->text().trimmed();
  m_address.account = m_account->text().trimmed();
  m_address.repository = m_repository->text().trimmed();
  m_address.folder = m_folder->text().trimmed();
  m_address.connect = m_ssh->isChecked() ? CloudConnect::Ssh : CloudConnect::Https;
  if (m_address.service != CloudService::Other) {
    const QString made = m_address.url();
    m_url->setText(m_address.service == CloudService::SharedFolder ? QDir::toNativeSeparators(made) : made);
  } else {
    m_address.other = m_url->text().trimmed();
  }
  scheduleCheck();
  emit addressChanged(url());
}

void CloudSection::addressEdited(const QString& text) {
  // A typed address fills the fields it can (another service's address
  // switches the service).
  const CloudAddress parsed = parseCloudAddress(text, m_server->text());
  m_address = parsed;
  m_updating = true;
  m_service->setCurrentIndex(m_service->findData(static_cast<int>(parsed.service)));
  if (knownService(parsed.service)) {
    m_server->setText(parsed.server);
    m_account->setText(parsed.account);
    m_repository->setText(parsed.repository);
    m_https->setChecked(parsed.connect == CloudConnect::Https);
    m_ssh->setChecked(parsed.connect == CloudConnect::Ssh);
    m_repositoryTyped = true;
  } else if (parsed.service == CloudService::SharedFolder) {
    m_folder->setText(parsed.folder);
  }
  m_updating = false;
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
    setStatus(address.isEmpty() ? QString() : tr("Checking %1...").arg(address), false);
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
                                 tr("The address holds a password or a token. Mitcad never gives them to git: use "
                                    "the address without it, and sign in with an SSH key or a credential helper.")));
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
                                   tr("%1 has files but no git repository: choose an empty folder, which Mitcad "
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
                "(https://git-scm.com/downloads) and start Mitcad again.");
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
      text = hasKey ? tr("Mitcad signs in with your SSH key: add its public key to your account on the service, then "
                         "come back.")
                    : tr("Mitcad signs in with an SSH key, and this computer has none: make one (ssh-keygen -t "
                         "ed25519), add its public key to your account, then come back.");
      m_copyKey->setVisible(hasKey);
      m_keysPage->show();
    } else {
      text = tr("For https:// addresses git signs in with a credential helper: Git Credential Manager comes with "
                "Git for Windows; elsewhere set git's credential.helper. Mitcad never asks for a password or a "
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
  auto* text = new QLabel(QCoreApplication::translate("CloudSection", "Fetching the identity of %1...").arg(host));
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
                                          "you and the server; Mitcad does not trust it.")
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
  const bool accepted = dialog.exec() == QDialog::Accepted;
  if (!accepted) {
    qInfo().noquote() << QStringLiteral("Trust This Server cancelled");
    return false;
  }
  // Every key a new scan still gives (a short wait, on a thread).
  QEventLoop loop;
  RemoteTask* trustTask = RemoteTask::projects({{QStringLiteral("cmd"), QStringLiteral("trust_host_key")},
                                                {QStringLiteral("host"), host},
                                                {QStringLiteral("port"), port}},
                                               nullptr);
  QObject::connect(trustTask, &RemoteTask::finished, &loop, &QEventLoop::quit);
  trustTask->start();
  loop.exec();
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
