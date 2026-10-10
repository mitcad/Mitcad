// SPDX-License-Identifier: MIT
#pragma once

// The user name and password of an MQTT broker of live updates in the
// system's keychain (mitcad#89, section 9): Windows' Credential Manager,
// the macOS Keychain, on Linux the Secret Service or KWallet, through
// QtKeychain (cmake/Keychain.cmake). One entry per broker's host and port
// ("broker.example.com:8883", LiveBrokers.hpp's credentialKey) in the
// service "Mitcad live updates"; never in a project or a settings file.
// Without a keychain (none on the system, or it refuses) the live
// controller asks when it connects and keeps the answer for the session.
//
// The calls are asynchronous: `done` runs on the UI thread once the
// keychain answered, unless `context` is gone by then. Logged as
// "Keychain: ..." (never the password).

#include <functional>

#include <QObject>
#include <QString>

namespace mitcad {

struct BrokerCredentials {
  QString user;
  QString password;
  bool isEmpty() const { return user.isEmpty(); }
};

namespace keychain {

enum class Read {
  Found,
  NotFound,    // the keychain answered: nothing for this broker
  Unavailable, // no keychain, or it refused (the message says why)
};

void read(const QString& key, QObject* context,
          std::function<void(Read result, const BrokerCredentials& credentials, const QString& error)> done);
// `error` is empty when it was written (or removed).
void write(const QString& key, const BrokerCredentials& credentials, QObject* context,
           std::function<void(const QString& error)> done);
void remove(const QString& key, QObject* context, std::function<void(const QString& error)> done);

} // namespace keychain
} // namespace mitcad
