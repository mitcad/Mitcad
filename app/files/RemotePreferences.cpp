// SPDX-License-Identifier: MIT
#include "RemotePreferences.hpp"

#include <QCheckBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QPointer>
#include <QPushButton>
#include <QSpinBox>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "Keychain.hpp"
#include "LiveBrokers.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

RemotePreferencesBox::RemotePreferencesBox(QWidget* parent)
    : QGroupBox(tr("Cloud"), parent), m_settings(RemoteSettings::load()), m_author(VersionSettings::load()) {
  auto* form = new QFormLayout(this);
  m_git = new QLineEdit(QDir::toNativeSeparators(m_settings.git));
  m_git->setPlaceholderText(tr("Found automatically"));
  m_git->setToolTip(tr("The git program that sends and gets the versions of Cloud projects; signing in is its own "
                       "(SSH keys, a credential helper). Local projects need no git program."));
  auto* browse = new QPushButton(tr("Bro&wse..."));
  auto* gitRow = new QHBoxLayout;
  gitRow->addWidget(m_git, 1);
  gitRow->addWidget(browse);
  auto* gitLabel = new QLabel(tr("Gi&t program:"));
  gitLabel->setBuddy(m_git);
  form->addRow(gitLabel, gitRow);
  // One line (a wrapping label would take room it does not use).
  m_found = new QLabel;
  m_found->setTextInteractionFlags(Qt::TextSelectableByMouse);
  form->addRow(QString(), m_found);
  // Who versions are recorded by when neither the project nor git names
  // anyone.
  m_name = new QLineEdit(m_author.name);
  m_name->setPlaceholderText(tr("Your Name"));
  m_email = new QLineEdit(m_author.email);
  m_email->setPlaceholderText(tr("you@example.com"));
  auto* authorRow = new QHBoxLayout;
  authorRow->addWidget(m_name, 1);
  authorRow->addWidget(m_email, 1);
  auto* authorLabel = new QLabel(tr("Default &author:"));
  authorLabel->setBuddy(m_name);
  form->addRow(authorLabel, authorRow);
  auto* authorNote = new QLabel(tr("When git has none. The name and email address go into every version of a "
                                   "project; anyone who gets its history sees them."));
  authorNote->setWordWrap(true);
  form->addRow(QString(), authorNote);
  m_push = new QCheckBox(tr("Send each saved version at &once (default for projects)"));
  m_push->setChecked(m_settings.autoPush);
  m_push->setToolTip(tr("Without a connection the versions wait, and go when the remote can be reached again; "
                        "off: only Sync sends them. Project Settings can set it for a project."));
  form->addRow(QString(), m_push);
  m_minutes = new QSpinBox;
  m_minutes->setRange(0, RemoteSettings::kMaxCheckMinutes);
  m_minutes->setSuffix(tr(" min"));
  m_minutes->setSpecialValueText(tr("never"));
  m_minutes->setValue(m_settings.checkMinutes);
  m_minutes->setToolTip(tr("When a design of a Cloud project opens, and then this often, Mitcad asks the remote for "
                           "newer versions (git fetch); never: only Sync and Check for Newer Versions do. Project "
                           "Settings can set it for a project."));
  auto* minutesRow = new QHBoxLayout;
  minutesRow->addWidget(m_minutes);
  minutesRow->addWidget(new QLabel(tr("(default for projects)")));
  minutesRow->addStretch(1);
  auto* minutesLabel = new QLabel(tr("Check for newer versions e&very:"));
  minutesLabel->setBuddy(m_minutes);
  form->addRow(minutesLabel, minutesRow);
  m_live = new QCheckBox(tr("Allow &live updates (MQTT)"));
  m_live->setChecked(m_settings.allowLive);
  m_live->setToolTip(tr("Edit locks and new versions reach the others at once through the broker a project names; "
                        "off: no project connects to a broker"));
  form->addRow(QString(), m_live);
  m_broker = new QLineEdit(m_settings.defaultBroker);
  m_broker->setPlaceholderText(QStringLiteral("mqtts://broker.example.com:8883"));
  m_broker->setEnabled(m_settings.allowLive);
  auto* brokerLabel = new QLabel(tr("Default b&roker for new Cloud projects:"));
  brokerLabel->setBuddy(m_broker);
  form->addRow(brokerLabel, m_broker);
  connect(m_live, &QCheckBox::toggled, m_broker, &QWidget::setEnabled);
  // The brokers answered for (mitcad#89): the trust question's answer and
  // the user signed in as; the buttons act at once.
  m_brokers = new QTreeWidget;
  m_brokers->setColumnCount(3);
  m_brokers->setHeaderLabels({tr("Broker"), tr("User"), tr("Answer")});
  m_brokers->setRootIsDecorated(false);
  m_brokers->setMaximumHeight(110);
  m_brokers->setToolTip(tr("Brokers you answered the question for that comes before the first connection to one. "
                           "Sign Out removes the user name and password from the system's keychain; Forget also "
                           "the answer, so that the next connection asks again."));
  m_signOut = new QPushButton(tr("Si&gn Out"));
  m_forget = new QPushButton(tr("&Forget"));
  for (QPushButton* button : {m_signOut, m_forget}) {
    button->setAutoDefault(false);
  }
  auto* brokerButtons = new QVBoxLayout;
  brokerButtons->addWidget(m_signOut);
  brokerButtons->addWidget(m_forget);
  brokerButtons->addStretch(1);
  auto* brokersRow = new QHBoxLayout;
  brokersRow->addWidget(m_brokers, 1);
  brokersRow->addLayout(brokerButtons);
  auto* brokersLabel = new QLabel(tr("&Known brokers:"));
  brokersLabel->setBuddy(m_brokers);
  form->addRow(brokersLabel, brokersRow);
  connect(m_signOut, &QPushButton::clicked, this, &RemotePreferencesBox::signOut);
  connect(m_forget, &QPushButton::clicked, this, &RemotePreferencesBox::forget);
  connect(m_brokers, &QTreeWidget::currentItemChanged, this, [this] {
    const bool chosen = m_brokers->currentItem() != nullptr;
    m_signOut->setEnabled(chosen);
    m_forget->setEnabled(chosen);
  });
  listBrokers();
  connect(browse, &QPushButton::clicked, this, [this] {
    const QString chosen = QFileDialog::getOpenFileName(this, tr("Git Program"), QFileInfo(m_git->text()).path());
    if (!chosen.isEmpty()) {
      m_git->setText(QDir::toNativeSeparators(chosen));
      detect();
    }
  });
  connect(m_git, &QLineEdit::editingFinished, this, &RemotePreferencesBox::detect);
  detect();
}

void RemotePreferencesBox::detect() {
  const QString program = QDir::fromNativeSeparators(m_git->text().trimmed());
  if (program.isEmpty() && !m_settings.git.isEmpty()) {
    // What remote work finds now is still the program set before.
    m_found->setText(tr("The git program on PATH (or where Git for Windows installs itself), once saved."));
    return;
  }
  const rust::String json = git_info_at(rustStr(program.toUtf8()));
  const QJsonObject info = parseObject(json);
  QString text;
  if (!info.value(QStringLiteral("error")).isNull()) {
    text = info.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  } else {
    const QString lfs = info.value(QStringLiteral("lfs")).toString();
    text = tr("git %1 at %2").arg(info.value(QStringLiteral("version")).toString(),
                                  QDir::toNativeSeparators(info.value(QStringLiteral("path")).toString()));
    text += lfs.isEmpty() ? tr(", without git-lfs") : tr(", git-lfs %1").arg(lfs);
    if (!info.value(QStringLiteral("supported")).toBool()) {
      text += QLatin1Char(' ') + tr("(older than %1, which Mitcad is tested with)")
                                     .arg(info.value(QStringLiteral("minimum")).toString());
    }
  }
  m_found->setText(text);
  qInfo().noquote() << QStringLiteral("Preferences: cloud: %1").arg(text);
}

void RemotePreferencesBox::listBrokers() {
  m_brokers->clear();
  QStringList logged;
  for (const KnownBroker& broker : brokers::known()) {
    const QString key = brokers::credentialKey(broker.address);
    const QString user = BrokerNotices::instance().user(key);
    auto* item = new QTreeWidgetItem(m_brokers, {broker.address, user, broker.trusted ? tr("trusted") : tr("Not Now")});
    item->setData(0, Qt::UserRole, broker.address);
    logged << QStringLiteral("%1 (%2%3)").arg(broker.address,
                                              broker.trusted ? QStringLiteral("trusted") : QStringLiteral("not now"),
                                              user.isEmpty() ? QString() : QStringLiteral(", user %1").arg(user));
    if (user.isEmpty() && broker.trusted && !brokers::isPlain(broker.address)) {
      // Not used this session: the keychain tells.
      const QPointer<QTreeWidget> list(m_brokers);
      const QString address = broker.address;
      keychain::read(key, this,
                     [list, address](keychain::Read result, const BrokerCredentials& credentials, const QString&) {
                       if (list == nullptr || result != keychain::Read::Found) {
                         return;
                       }
                       for (int i = 0; i < list->topLevelItemCount(); ++i) {
                         QTreeWidgetItem* row = list->topLevelItem(i);
                         if (row->data(0, Qt::UserRole).toString() == address) {
                           row->setText(1, credentials.user);
                         }
                       }
                     });
    }
  }
  for (int column = 0; column < m_brokers->columnCount(); ++column) {
    m_brokers->resizeColumnToContents(column);
  }
  if (m_brokers->topLevelItemCount() > 0) {
    m_brokers->setCurrentItem(m_brokers->topLevelItem(0));
  }
  const bool chosen = m_brokers->currentItem() != nullptr;
  m_signOut->setEnabled(chosen);
  m_forget->setEnabled(chosen);
  qInfo().noquote() << QStringLiteral("Preferences: known brokers: %1")
                           .arg(logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(", ")));
}

void RemotePreferencesBox::signOut() {
  const QTreeWidgetItem* item = m_brokers->currentItem();
  if (item == nullptr) {
    return;
  }
  const QString key = brokers::credentialKey(item->data(0, Qt::UserRole).toString());
  qInfo().noquote() << QStringLiteral("Preferences: sign out of %1").arg(key);
  BrokerNotices::instance().setUser(key, QString());
  keychain::remove(key, this, [](const QString&) {});
  emit BrokerNotices::instance().signedOut(key);
  listBrokers();
}

void RemotePreferencesBox::forget() {
  const QTreeWidgetItem* item = m_brokers->currentItem();
  if (item == nullptr) {
    return;
  }
  const QString address = item->data(0, Qt::UserRole).toString();
  const QString key = brokers::credentialKey(address);
  qInfo().noquote() << QStringLiteral("Preferences: forget %1").arg(address);
  brokers::forget(address);
  BrokerNotices::instance().setUser(key, QString());
  keychain::remove(key, this, [](const QString&) {});
  emit BrokerNotices::instance().forgotten(address);
  listBrokers();
}

RemoteSettings RemotePreferencesBox::chosen() const {
  RemoteSettings settings = m_settings;
  settings.git = QDir::fromNativeSeparators(m_git->text().trimmed());
  settings.checkMinutes = m_minutes->value();
  settings.autoPush = m_push->isChecked();
  settings.allowLive = m_live->isChecked();
  settings.defaultBroker = m_broker->text().trimmed();
  return settings;
}

VersionSettings RemotePreferencesBox::chosenAuthor() const {
  VersionSettings author = m_author;
  author.name = m_name->text().trimmed();
  author.email = m_email->text().trimmed();
  return author;
}

} // namespace mitcad
