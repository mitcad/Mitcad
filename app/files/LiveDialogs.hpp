// SPDX-License-Identifier: MIT
#pragma once

// The questions and notices of live updates (mitcad#89, section 9): the
// trust question before the first connection to a broker, the notice under
// the toolbar when a broker needs a sign-in (never blocking an open), the
// sign-in dialog, and Project Settings' Test with its steps. Names and
// addresses are shown as plain text only. Each logs what it shows and what
// was chosen ("Live trust question: ...", "Live notice: ...", "Live sign-in
// dialog: ...", "Live test: ..."), which the UI tests read.

#include <functional>
#include <optional>

#include <QJsonObject>
#include <QString>
#include <QWidget>

#include "Keychain.hpp"

class QLabel;
class QPushButton;

namespace mitcad {

// "Robot arm uses live updates through broker.example.com. Your name and
// the designs you open are sent there." [Connect] [Not Now]; true for
// Connect. A plain-text address (mqtt://) adds why that is unsafe.
bool askTrustBroker(QWidget* parent, const QString& project, const QString& address);

struct BrokerSignIn {
  BrokerCredentials credentials;
  bool remember = false; // kept in the keychain (else for this session)
};

// Signs in to a broker: the user name (`user` filled in) and the password,
// and whether the keychain keeps them (`keychain`: false when there is
// none: they are kept for this session). `reason` says why it is asked
// (the broker refused, or none known). `check` tries the credentials as
// the live controller will (the core refuses a password over plain text,
// for one) and gives its refusal, which the dialog shows, staying open.
std::optional<BrokerSignIn> askBrokerSignIn(QWidget* parent, const QString& address, const QString& user,
                                            bool keychain, const QString& reason,
                                            const std::function<QString(const BrokerSignIn&)>& check);

// The notice under the toolbar: "Live updates need you to sign in to
// broker.example.com" [Sign In] [Not Now].
class LiveNotice : public QWidget {
  Q_OBJECT

public:
  explicit LiveNotice(QWidget* parent = nullptr);
  void present(const QString& text);
  QString text() const;

  QPushButton* signIn = nullptr;
  QPushButton* notNow = nullptr;

private:
  QLabel* m_text = nullptr;
};

// The test's answer (the hub's `test`) as steps for people: "Connect: ok
// (12 ms)", ..., then whether it works.
QString describeLiveTest(const QJsonObject& answer);

} // namespace mitcad
