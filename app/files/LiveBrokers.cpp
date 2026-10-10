// SPDX-License-Identifier: MIT
#include "LiveBrokers.hpp"

#include <QCoreApplication>
#include <QSettings>
#include <QUrl>

namespace mitcad {
namespace {

const QString kArray = QStringLiteral("live/brokers");

void store(const QList<KnownBroker>& list) {
  QSettings settings;
  settings.remove(kArray);
  settings.beginWriteArray(kArray, static_cast<int>(list.size()));
  for (int i = 0; i < list.size(); ++i) {
    settings.setArrayIndex(i);
    settings.setValue(QStringLiteral("address"), list.at(i).address);
    settings.setValue(QStringLiteral("trusted"), list.at(i).trusted);
  }
  settings.endArray();
}

} // namespace

namespace brokers {

QList<KnownBroker> known() {
  QSettings settings;
  QList<KnownBroker> list;
  const int size = settings.beginReadArray(kArray);
  for (int i = 0; i < size; ++i) {
    settings.setArrayIndex(i);
    KnownBroker broker;
    broker.address = settings.value(QStringLiteral("address")).toString().trimmed();
    broker.trusted = settings.value(QStringLiteral("trusted")).toBool();
    if (!broker.address.isEmpty()) {
      list << broker;
    }
  }
  settings.endArray();
  return list;
}

std::optional<bool> trust(const QString& address) {
  for (const KnownBroker& broker : known()) {
    if (broker.address == address) {
      return broker.trusted;
    }
  }
  return std::nullopt;
}

void setTrust(const QString& address, bool trusted) {
  QList<KnownBroker> list = known();
  for (KnownBroker& broker : list) {
    if (broker.address == address) {
      broker.trusted = trusted;
      store(list);
      return;
    }
  }
  list << KnownBroker{address, trusted};
  store(list);
}

void forget(const QString& address) {
  QList<KnownBroker> list = known();
  list.removeIf([&address](const KnownBroker& broker) { return broker.address == address; });
  store(list);
}

QString credentialKey(const QString& address) {
  const QUrl url(address);
  if (!url.isValid() || url.host().isEmpty()) {
    return {};
  }
  const bool plain = url.scheme().compare(QLatin1String("mqtt"), Qt::CaseInsensitive) == 0;
  const QString host = url.host().contains(QLatin1Char(':')) ? QStringLiteral("[%1]").arg(url.host()) : url.host();
  return QStringLiteral("%1:%2").arg(host).arg(url.port(plain ? 1883 : 8883));
}

QString hostOf(const QString& address) {
  const QUrl url(address);
  return url.host().isEmpty() ? address : url.host();
}

bool isPlain(const QString& address) {
  return address.startsWith(QLatin1String("mqtt://"), Qt::CaseInsensitive);
}

} // namespace brokers

BrokerNotices& BrokerNotices::instance() {
  // The application's (it goes with it).
  static BrokerNotices* notices = new BrokerNotices(QCoreApplication::instance());
  return *notices;
}

void BrokerNotices::setUser(const QString& key, const QString& user) {
  if (user.isEmpty()) {
    m_users.remove(key);
  } else {
    m_users.insert(key, user);
  }
}

} // namespace mitcad
