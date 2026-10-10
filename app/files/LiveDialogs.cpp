// SPDX-License-Identifier: MIT
#include "LiveDialogs.hpp"

#include <QApplication>
#include <QCheckBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "LiveBrokers.hpp"

namespace mitcad {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("LiveDialogs", text); }

QLabel* plainLabel(const QString& text = QString()) {
  auto* label = new QLabel(text);
  label->setTextFormat(Qt::PlainText);
  label->setWordWrap(true);
  return label;
}

} // namespace

bool askTrustBroker(QWidget* parent, const QString& project, const QString& address) {
  const QString host = brokers::hostOf(address);
  QMessageBox box(QMessageBox::Question, tr("Live Updates"),
                  tr("%1 uses live updates through %2. Your name and the designs you open are sent there.")
                      .arg(project, host),
                  QMessageBox::NoButton, parent);
  box.setTextFormat(Qt::PlainText);
  QString details = tr("The broker %1 is named in the project's settings, which anyone who can send versions "
                       "to the project can change. Connect only to a broker you know.")
                        .arg(address);
  if (brokers::isPlain(address)) {
    details += QLatin1Char('\n') + tr("Its connection is plain text: anyone on the network between this computer "
                                      "and the broker can read the messages.");
  }
  box.setInformativeText(details);
  QPushButton* connect = box.addButton(tr("&Connect"), QMessageBox::AcceptRole);
  QPushButton* notNow = box.addButton(tr("&Not Now"), QMessageBox::RejectRole);
  box.setDefaultButton(connect);
  box.setEscapeButton(notNow);
  prepareModal(&box);
  qInfo().noquote() << QStringLiteral("Live trust question: %1 through %2 [Connect, Not Now]").arg(project, address);
  box.exec();
  const bool trusted = box.clickedButton() == connect;
  qInfo().noquote() << QStringLiteral("Live trust: %1 %2").arg(address, trusted ? QStringLiteral("connect")
                                                                               : QStringLiteral("not now"));
  return trusted;
}

std::optional<BrokerSignIn> askBrokerSignIn(QWidget* parent, const QString& address, const QString& user,
                                            bool keychain, const QString& reason,
                                            const std::function<QString(const BrokerSignIn&)>& check) {
  QDialog dialog(parent);
  dialog.setWindowTitle(tr("Sign In to %1").arg(brokers::hostOf(address)));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(plainLabel(reason.isEmpty() ? tr("Live updates through %1 need a user name and a password.")
                                                         .arg(address)
                                                : reason));
  auto* form = new QFormLayout;
  auto* name = new QLineEdit(user);
  auto* password = new QLineEdit;
  password->setEchoMode(QLineEdit::Password);
  auto* nameLabel = new QLabel(tr("&User:"));
  nameLabel->setBuddy(name);
  auto* passwordLabel = new QLabel(tr("&Password:"));
  passwordLabel->setBuddy(password);
  form->addRow(nameLabel, name);
  form->addRow(passwordLabel, password);
  layout->addLayout(form);
  auto* remember = new QCheckBox(tr("&Remember in the system's keychain"));
  remember->setChecked(keychain);
  remember->setEnabled(keychain);
  layout->addWidget(remember);
  layout->addWidget(plainLabel(keychain ? tr("They go only to %1, and only over an encrypted connection.")
                                              .arg(brokers::credentialKey(address))
                                        : tr("This computer has no keychain Mitcad can use: they are kept until "
                                             "Mitcad closes, and go only to %1.")
                                              .arg(brokers::credentialKey(address))));
  auto* problem = plainLabel();
  setErrorStyleSheet(problem);
  problem->hide();
  layout->addWidget(problem);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  QPushButton* signIn = buttons->addButton(tr("&Sign In"), QDialogButtonBox::AcceptRole);
  layout->addWidget(buttons);
  // Enter signs in (once the button is the dialog's).
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  signIn->setDefault(true);
  (user.isEmpty() ? name : password)->setFocus();
  std::optional<BrokerSignIn> chosen;
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, [&dialog] {
    qInfo().noquote() << QStringLiteral("Live sign-in dialog: Cancel");
    dialog.reject();
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    BrokerSignIn answer;
    answer.credentials = BrokerCredentials{name->text().trimmed(), password->text()};
    answer.remember = remember->isChecked();
    if (answer.credentials.user.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Live sign-in: no user name");
      problem->setText(tr("Give the user name the broker knows you by."));
      problem->show();
      name->setFocus();
      return;
    }
    const QString refused = check ? check(answer) : QString();
    if (!refused.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Live sign-in refused: %1").arg(refused);
      problem->setText(refused);
      problem->show();
      return;
    }
    chosen = answer;
    dialog.accept();
  });
  prepareModal(&dialog);
  qInfo().noquote() << QStringLiteral("Live sign-in dialog: %1, user %2, keychain %3")
                           .arg(address, user.isEmpty() ? QStringLiteral("none") : user,
                                keychain ? QStringLiteral("on") : QStringLiteral("off"));
  dialog.exec();
  qInfo().noquote() << (chosen ? QStringLiteral("Live sign-in: %1 as %2%3")
                                     .arg(address, chosen->credentials.user,
                                          chosen->remember ? QStringLiteral(", remembered") : QString())
                               : QStringLiteral("Live sign-in cancelled"));
  return chosen;
}

LiveNotice::LiveNotice(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("liveNotice"));
  auto* layout = new QHBoxLayout(this);
  layout->setContentsMargins(8, 2, 8, 2);
  auto* icon = new QLabel;
  icon->setPixmap(themeIcon(QStringLiteral("sync")).pixmap(20, 20));
  layout->addWidget(icon);
  m_text = plainLabel();
  layout->addWidget(m_text, 1);
  // No mnemonics: the menus' Alt keys stay theirs.
  signIn = new QPushButton(tr("Sign In..."));
  signIn->setAutoDefault(false);
  notNow = new QPushButton(tr("Not Now"));
  notNow->setAutoDefault(false);
  layout->addWidget(signIn);
  layout->addWidget(notNow);
}

void LiveNotice::present(const QString& text) {
  m_text->setText(text);
  qInfo().noquote() << QStringLiteral("Live notice: %1 [Sign In, Not Now]").arg(text);
  TestSync::singleShot(50, this, [this] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    for (const QPushButton* button : {signIn, notNow}) {
      const QPoint center = button->mapTo(window(), button->rect().center());
      qDebug().noquote() << QStringLiteral("Live notice %1 at %2,%3")
                                .arg(button->text().remove(QLatin1Char('&')).remove(QStringLiteral("...")))
                                .arg(center.x())
                                .arg(center.y());
    }
  });
}

QString LiveNotice::text() const { return m_text->text(); }

QString describeLiveTest(const QJsonObject& answer) {
  QStringList lines;
  QString heading = tr("Live updates through %1, prefix %2").arg(answer.value(QStringLiteral("broker")).toString(),
                                                                 answer.value(QStringLiteral("prefix")).toString());
  if (answer.value(QStringLiteral("user")).isString()) {
    heading += tr(", as %1").arg(answer.value(QStringLiteral("user")).toString());
  }
  lines << heading;
  for (const QJsonValue& value : answer.value(QStringLiteral("steps")).toArray()) {
    const QJsonObject step = value.toObject();
    const QString name = step.value(QStringLiteral("step")).toString();
    const QString what = name == QLatin1String("connect")     ? tr("Connect")
                         : name == QLatin1String("sign_in")   ? tr("Sign in")
                         : name == QLatin1String("subscribe") ? tr("Subscribe")
                         : name == QLatin1String("publish")   ? tr("Publish a test message")
                         : name == QLatin1String("receive")   ? tr("Receive it back")
                                                              : name;
    lines << (step.value(QStringLiteral("ok")).toBool() ? tr("%1: ok (%2 ms)") : tr("%1: failed (%2 ms)"))
                 .arg(what)
                 .arg(step.value(QStringLiteral("ms")).toInt());
  }
  const QString warning = answer.value(QStringLiteral("warning")).toString();
  if (!warning.isEmpty()) {
    lines << tr("Warning: %1.").arg(warning);
  }
  const QString error = answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  lines << (error.isEmpty() ? tr("Live updates work through this broker.") : tr("Failed: %1").arg(error));
  return lines.join(QLatin1Char('\n'));
}

} // namespace mitcad
