// SPDX-License-Identifier: MIT
#pragma once

// The MQTT brokers of live updates the user has answered for (mitcad#89,
// section 9): the trust question's answer per broker address, kept in the
// application settings (live/brokers), which Preferences > Cloud lists as
// Known brokers. A broker is named by a project, and anyone who can push
// writes that, so nothing connects to a broker before the user has said
// Connect for its address; a changed address asks again. Credentials are
// never kept here: they are in the system's keychain (Keychain.hpp), per
// host and port.

#include <optional>

#include <QHash>
#include <QList>
#include <QObject>
#include <QString>

namespace mitcad {

struct KnownBroker {
  QString address;      // as the project names it: "mqtts://broker.example.com:8883"
  bool trusted = false; // Connect (true) or Not Now (false)
};

namespace brokers {

// The answers, in the order they were given.
QList<KnownBroker> known();
// The answer for an address; none when it was never asked (or forgotten).
std::optional<bool> trust(const QString& address);
void setTrust(const QString& address, bool trusted);
// Forgets the answer (Preferences: Forget): the next connection asks again.
void forget(const QString& address);

// Where a broker's credentials are kept and go: its host and port
// ("broker.example.com:8883"; IPv6 in brackets); empty for no address.
QString credentialKey(const QString& address);
// Its host for people ("broker.example.com").
QString hostOf(const QString& address);
// Whether the address is plain text (mqtt://): a password is never sent there.
bool isPlain(const QString& address);

} // namespace brokers

// What happens to brokers outside the live controller (Preferences' Sign
// Out and Forget act at once), and the user names known this session, which
// Preferences shows: one object for the application.
class BrokerNotices : public QObject {
  Q_OBJECT

public:
  static BrokerNotices& instance();

  // The user name signed in as at `key` this session ("" none known).
  QString user(const QString& key) const { return m_users.value(key); }
  void setUser(const QString& key, const QString& user);

signals:
  // Sign Out: the credentials of `key` were removed from the keychain.
  void signedOut(const QString& key);
  // Forget: the answer for `address` and its credentials are gone.
  void forgotten(const QString& address);

private:
  explicit BrokerNotices(QObject* parent) : QObject(parent) {}
  QHash<QString, QString> m_users;
};

} // namespace mitcad
