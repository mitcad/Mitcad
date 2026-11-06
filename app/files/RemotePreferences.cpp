// SPDX-License-Identifier: MIT
#include "RemotePreferences.hpp"

#include <algorithm>

#include <QCheckBox>
#include <QDebug>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHeaderView>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QPointer>
#include <QPushButton>
#include <QResizeEvent>
#include <QShowEvent>
#include <QSize>
#include <QSizePolicy>
#include <QSpinBox>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "Keychain.hpp"
#include "LiveBrokers.hpp"
#include "mitcad_bridge/lib.h"

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/PreferencesPageLayout.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/NativeDialogs.hpp"

namespace mitcad {

namespace {

// The git program found, under its field (mitcad#690): as wide as its text
// up to about 60 characters and wrapped beyond, so that a long message (git
// not installed: some 330 characters) neither widens Preferences nor sets
// its minimum width, and a short one stays on one line. A single word
// longer than the room (a path) is cut off; the tooltip has the whole text.
class GitStatusLabel : public QLabel {
public:
  GitStatusLabel() {
    setTextFormat(Qt::PlainText); // what git answered is shown as it is
    setWordWrap(true);
    setTextInteractionFlags(Qt::TextSelectableByMouse);
    // Fills the field's column where the form lets fields grow; elsewhere
    // (macOS) it stays at its size hint.
    QSizePolicy policy = sizePolicy(); // keeps the height for width of the wrap
    policy.setHorizontalPolicy(QSizePolicy::Expanding);
    setSizePolicy(policy);
  }

  QSize sizeHint() const override {
    const int width = std::min(fontMetrics().horizontalAdvance(text()) + extraWidth(), charsWide(kWidest));
    return {width, heightForWidth(width)};
  }

  QSize minimumSizeHint() const override {
    return {std::min(sizeHint().width(), charsWide(kNarrowest)), QLabel::minimumSizeHint().height()};
  }

private:
  static constexpr int kWidest = 60;
  static constexpr int kNarrowest = 20;

  // The margins around the text, and two pixels for the text layout's
  // rounding, which would otherwise wrap a line that just fits.
  int extraWidth() const {
    const QMargins margins = contentsMargins();
    return margins.left() + margins.right() + 2 * margin() + 2;
  }
  int charsWide(int count) const { return fontMetrics().averageCharWidth() * count + extraWidth(); }
};

// The known brokers: the list asks for no more than its minimum height, so
// that the page's natural height stays the one it had (at most 110 pixels
// for the list), and takes what the page has to spare (mitcad#1394).
class KnownBrokersList : public QTreeWidget {
public:
  static constexpr int kMinimumHeight = 110;
  QSize sizeHint() const override { return {QTreeWidget::sizeHint().width(), minimumHeight()}; }
};

} // namespace

RemotePreferencesBox::RemotePreferencesBox(QWidget* parent)
    : QGroupBox(tr("Cloud"), parent), m_settings(RemoteSettings::load()), m_author(VersionSettings::load()) {
  auto* form = new QFormLayout(this);
  m_git = new QLineEdit(QDir::toNativeSeparators(m_settings.git));
  m_git->setPlaceholderText(tr("Found automatically"));
  m_git->setToolTip(tr("The git program that sends and gets the versions of Cloud projects"));
  auto* browse = new QPushButton(tr("Bro&wse…"));
  auto* gitRow = new QHBoxLayout;
  gitRow->addWidget(m_git, 1);
  gitRow->addWidget(browse);
  auto* gitLabel = new QLabel(tr("Gi&t program:"));
  gitLabel->setBuddy(m_git);
  form->addRow(gitLabel, gitRow);
  m_found = new GitStatusLabel;
  m_found->setObjectName(QStringLiteral("cloudGitStatus"));
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
  authorNote->setObjectName(QStringLiteral("cloudAuthorNote"));
  authorNote->setWordWrap(true);
  form->addRow(QString(), authorNote);
  m_push = new QCheckBox(tr("Send each saved version at &once (default for projects)"));
  m_push->setChecked(m_settings.autoPush);
  m_push->setToolTip(tr("Off: only Sync sends the versions"));
  form->addRow(QString(), m_push);
  m_minutes = new QSpinBox;
  m_minutes->setRange(0, RemoteSettings::kMaxCheckMinutes);
  m_minutes->setSuffix(tr(" min"));
  m_minutes->setSpecialValueText(tr("never"));
  m_minutes->setValue(m_settings.checkMinutes);
  m_minutes->setToolTip(tr("How often MitCad asks the remote for newer versions; never: only when you ask"));
  auto* minutesRow = new QHBoxLayout;
  minutesRow->addWidget(m_minutes);
  minutesRow->addWidget(new QLabel(tr("(default for projects)")));
  minutesRow->addStretch(1);
  auto* minutesLabel = new QLabel(tr("Check for newer versions e&very:"));
  minutesLabel->setBuddy(m_minutes);
  form->addRow(minutesLabel, minutesRow);
  m_live = new QCheckBox(tr("Allow &live updates (MQTT)"));
  m_live->setChecked(m_settings.allowLive);
  m_live->setToolTip(tr("Edit locks and new versions reach the others at once through a broker"));
  form->addRow(QString(), m_live);
  m_broker = new QLineEdit(m_settings.defaultBroker);
  m_broker->setPlaceholderText(QStringLiteral("mqtts://broker.example.com:8883"));
  m_broker->setEnabled(m_settings.allowLive);
  auto* brokerLabel = new QLabel(tr("Default b&roker for new Cloud projects:"));
  brokerLabel->setBuddy(m_broker);
  form->addRow(brokerLabel, m_broker);
  connect(m_live, &QCheckBox::toggled, m_broker, &QWidget::setEnabled);
  // The brokers answered for (mitcad#89): the trust question's answer and
  // the user signed in as; the buttons act at once. The page's spare height
  // goes to the list (mitcad#1394).
  m_brokers = new KnownBrokersList;
  panel::setCompactIndentation(m_brokers, panel::Indent::Flat);
  m_brokers->setObjectName(QStringLiteral("cloudKnownBrokers"));
  m_brokers->setColumnCount(3);
  m_brokers->setHeaderLabels({tr("Broker"), tr("User"), tr("Answer")});
  m_brokers->setRootIsDecorated(false);
  m_brokers->setMinimumHeight(KnownBrokersList::kMinimumHeight);
  QSizePolicy brokersPolicy = m_brokers->sizePolicy();
  brokersPolicy.setVerticalPolicy(QSizePolicy::Expanding);
  brokersPolicy.setVerticalStretch(1);
  m_brokers->setSizePolicy(brokersPolicy);
  m_brokers->setToolTip(tr("The brokers you answered the trust question for, with the user you signed in as"));
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
    const QString start = dialogStartFolder(DialogGroup::Program, QFileInfo(m_git->text()).path());
    const QString chosen = dialogOpenFileName(this, tr("Git Program"), start);
    if (!chosen.isEmpty()) {
      rememberDialogFolder(DialogGroup::Program, chosen);
      m_git->setText(QDir::toNativeSeparators(chosen));
      detect();
    }
  });
  connect(m_git, &QLineEdit::editingFinished, this, &RemotePreferencesBox::detect);
  detect();
}

void RemotePreferencesBox::showEvent(QShowEvent* event) {
  QGroupBox::showEvent(event);
  logGeometry();
}

void RemotePreferencesBox::resizeEvent(QResizeEvent* event) {
  QGroupBox::resizeEvent(event);
  logGeometry();
}

void RemotePreferencesBox::logGeometry() {
  if (m_geometryLogPending) {
    return;
  }
  m_geometryLogPending = true;
  TestSync::singleShot(0, this, [this] {
    m_geometryLogPending = false;
    if (!isVisible() || !m_brokers->isVisible()) {
      return;
    }
    // The list is the page's last line.
    logPreferencesPageLayout(this, QStringLiteral("Cloud page: known brokers list"), m_brokers, m_brokers,
                             QStringLiteral("cloud"));
  });
}

void RemotePreferencesBox::detect() {
  const QString program = QDir::fromNativeSeparators(m_git->text().trimmed());
  if (program.isEmpty() && !m_settings.git.isEmpty()) {
    // What remote work finds now is still the program set before.
    m_found->setText(tr("The git program on PATH (or where Git for Windows installs itself), once saved."));
    m_found->setToolTip(m_found->text());
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
      text += QLatin1Char(' ') + tr("(older than %1, which MitCad is tested with)")
                                     .arg(info.value(QStringLiteral("minimum")).toString());
    }
  }
  m_found->setText(text);
  m_found->setToolTip(text);
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
  requestNativeColumnContents(m_brokers->header());
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
