// SPDX-License-Identifier: MIT
#include "UpdateClient.hpp"

#include <cstdint>
#include <exception>

#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSslConfiguration>
#include <QSslError>
#include <QUrl>
#include <QtLogging>

#include "framework/Json.hpp"
#include "mitcad_bridge/update.h"

namespace mitcad {
namespace {

// A manifest or GitHub's list of releases is small.
constexpr qint64 kMaxListBytes = 2 * 1024 * 1024;
constexpr int kMaxRedirects = 5;
// No data for this long ends a request.
constexpr int kTimeoutMs = 30000;
// Why a request was ended here (the reply's property).
const char* const kWhy = "mitcadWhy";

rust::Slice<const std::uint8_t> bytesOf(const QByteArray& data) {
  return rust::Slice<const std::uint8_t>(reinterpret_cast<const std::uint8_t*>(data.constData()),
                                         static_cast<std::size_t>(data.size()));
}

// Only the host, for the log: an address may carry a signed query.
QString hostOf(const QUrl& url) { return url.scheme() + QStringLiteral("://") + url.host(); }

} // namespace

UpdateClient::UpdateClient(UpdateSource source, QObject* parent) : QObject(parent), m_source(std::move(source)) {
  if (!m_source.testCertificates.isEmpty()) {
    m_testCertificates = QSslCertificate::fromPath(m_source.testCertificates, QSsl::Pem);
    qInfo().noquote() << QStringLiteral("Update requests also trust %1 test certificate(s) of %2")
                             .arg(m_testCertificates.size())
                             .arg(m_source.testCertificates);
  }
}

UpdateClient::~UpdateClient() { cancel(); }

QNetworkAccessManager* UpdateClient::network() {
  // Made on the first request: with checks off, there is none.
  if (m_network == nullptr) {
    m_network = new QNetworkAccessManager(this);
  }
  return m_network;
}

QNetworkReply* UpdateClient::get(const QUrl& url, const QByteArray& accept) {
  if (!url.isValid() || url.scheme() != QStringLiteral("https")) {
    fail(tr("Updates are fetched over HTTPS only, not from %1.").arg(url.toString()));
    return nullptr;
  }
  QNetworkRequest request(url);
  const QString channel =
      m_channel == UpdateSettings::Channel::Prerelease ? QStringLiteral("prerelease") : QStringLiteral("stable");
  request.setHeader(QNetworkRequest::UserAgentHeader,
                    QStringLiteral("Mitcad/%1 (%2; %3)")
                        .arg(m_source.currentVersion,
                             m_source.platform.isEmpty() ? QStringLiteral("unknown") : m_source.platform, channel));
  // Qt would send the user's languages.
  request.setRawHeader("Accept-Language", "*");
  if (!accept.isEmpty()) {
    request.setRawHeader("Accept", accept);
  }
  request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::UserVerifiedRedirectPolicy);
  request.setMaximumRedirectsAllowed(kMaxRedirects);
  request.setTransferTimeout(kTimeoutMs);
  request.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
  request.setAttribute(QNetworkRequest::CacheSaveControlAttribute, false);
  request.setAttribute(QNetworkRequest::CookieLoadControlAttribute, QNetworkRequest::Manual);
  request.setAttribute(QNetworkRequest::CookieSaveControlAttribute, QNetworkRequest::Manual);
  if (!m_testCertificates.isEmpty()) {
    QSslConfiguration ssl = QSslConfiguration::defaultConfiguration();
    ssl.addCaCertificates(m_testCertificates);
    request.setSslConfiguration(ssl);
  }
  QNetworkReply* reply = network()->get(request);
  connect(reply, &QNetworkReply::redirected, reply, [reply](const QUrl& to) {
    if (to.scheme() == QStringLiteral("https")) {
      qInfo().noquote() << "Update request redirected to" << hostOf(to);
      emit reply->redirectAllowed();
    } else {
      reply->setProperty(kWhy, tr("The release server redirected to an address that is not HTTPS (%1).")
                                   .arg(hostOf(to)));
      reply->abort();
    }
  });
  // A test's server: its certificate is trusted for these requests only,
  // whichever TLS backend checks it (OpenSSL takes it as a CA, Schannel
  // may not).
  connect(reply, &QNetworkReply::sslErrors, reply, [this, reply](const QList<QSslError>& errors) {
    if (!m_testCertificates.isEmpty() && m_testCertificates.contains(reply->sslConfiguration().peerCertificate())) {
      reply->ignoreSslErrors(errors);
    }
  });
  return reply;
}

void UpdateClient::check(UpdateSettings::Channel channel, const QString& skippedVersion) {
  cancel();
  m_channel = channel;
  m_skipped = skippedVersion;
  if (channel == UpdateSettings::Channel::Prerelease) {
    fetch(m_source.releasesUrl, Step::Releases);
  } else {
    fetch(m_source.manifestUrl, Step::Manifest);
  }
}

void UpdateClient::fetch(const QString& url, Step step) {
  const QUrl address(url);
  qInfo().noquote() << QStringLiteral("Update check: %1").arg(step == Step::Releases ? QStringLiteral("releases at %1")
                                                                                      : QStringLiteral("manifest at %1"))
                           .arg(address.toString());
  QNetworkReply* reply = get(address, step == Step::Releases ? QByteArray("application/vnd.github+json")
                                                             : QByteArray());
  if (reply == nullptr) {
    return;
  }
  m_reply = reply;
  connect(reply, &QNetworkReply::downloadProgress, reply, [reply](qint64 received, qint64) {
    if (received > kMaxListBytes) {
      reply->setProperty(kWhy, tr("The release server sent more than an update manifest."));
      reply->abort();
    }
  });
  connect(reply, &QNetworkReply::finished, this, [this, reply, step] { onFinished(reply, step); });
}

void UpdateClient::download(const UpdateOffer& offer, const QString& path) {
  cancel();
  m_file.setFileName(path);
  if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    fail(tr("Cannot write %1: %2").arg(path, m_file.errorString()));
    return;
  }
  m_expected = offer.size;
  qInfo().noquote() << QStringLiteral("Update download: %1 (%2 bytes) to %3").arg(offer.url).arg(offer.size).arg(path);
  QNetworkReply* reply = get(QUrl(offer.url), QByteArray("application/octet-stream"));
  if (reply == nullptr) {
    m_file.remove();
    return;
  }
  m_reply = reply;
  reply->setReadBufferSize(1024 * 1024);
  connect(reply, &QNetworkReply::readyRead, this, [this, reply] {
    const QByteArray data = reply->readAll();
    if (m_file.pos() + data.size() > m_expected) {
      reply->setProperty(kWhy, tr("The download is larger than the release says."));
      reply->abort();
      return;
    }
    if (m_file.write(data) != data.size()) {
      reply->setProperty(kWhy, tr("Cannot write %1: %2").arg(m_file.fileName(), m_file.errorString()));
      reply->abort();
    }
  });
  connect(reply, &QNetworkReply::downloadProgress, this,
          [this](qint64 received, qint64) { emit progress(received, m_expected); });
  connect(reply, &QNetworkReply::finished, this, [this, reply] { onFinished(reply, Step::Download); });
}

void UpdateClient::cancel() {
  if (m_reply) {
    QNetworkReply* reply = m_reply;
    m_reply = nullptr;
    reply->disconnect(this);
    reply->abort();
    reply->deleteLater();
  }
  if (m_file.isOpen()) {
    m_file.close();
    m_file.remove();
  }
}

void UpdateClient::fail(const QString& message) {
  qWarning().noquote() << "Update failed:" << message;
  emit failed(message);
}

void UpdateClient::onFinished(QNetworkReply* reply, Step step) {
  reply->deleteLater();
  if (reply != m_reply) {
    return; // cancelled
  }
  m_reply = nullptr;
  const QString why = reply->property(kWhy).toString();
  if (reply->error() != QNetworkReply::NoError || !why.isEmpty()) {
    if (step == Step::Download) {
      m_file.close();
      m_file.remove();
    }
    fail(!why.isEmpty() ? why
                        : tr("The release server could not be reached (%1).").arg(reply->errorString()));
    return;
  }
  if (step == Step::Download) {
    const QByteArray rest = reply->readAll();
    const qint64 received = m_file.pos() + rest.size();
    if (received <= m_expected) {
      m_file.write(rest);
    }
    m_file.close();
    if (received != m_expected || m_file.size() != m_expected) {
      m_file.remove();
      fail(tr("The download has %1 bytes instead of %2.").arg(received).arg(m_expected));
      return;
    }
    emit downloaded(m_file.fileName());
    return;
  }
  const QByteArray data = reply->readAll();
  if (step == Step::Releases) {
    try {
      const rust::String url = updates::update_prerelease_manifest_url(bytesOf(data));
      fetch(QString::fromUtf8(url.data(), static_cast<qsizetype>(url.size())), Step::Manifest);
    } catch (const std::exception& e) {
      fail(tr("The list of releases could not be read: %1.").arg(errorText(e)));
    }
    return;
  }
  readManifest(data);
}

void UpdateClient::readManifest(const QByteArray& data) {
  UpdateOffer offer;
  try {
    const QJsonObject answer = parseObject(updates::update_check(
        bytesOf(data), rustStr(m_source.keys.toUtf8()), rustStr(m_source.currentVersion.toUtf8()),
        rustStr(m_source.platform.toUtf8()), m_channel == UpdateSettings::Channel::Prerelease,
        rustStr(m_skipped.toUtf8())));
    offer.status = answer.value(QStringLiteral("status")).toString();
    offer.version = answer.value(QStringLiteral("version")).toString();
    offer.date = answer.value(QStringLiteral("date")).toString();
    offer.notes = answer.value(QStringLiteral("notes")).toString();
    offer.prerelease = answer.value(QStringLiteral("prerelease")).toBool();
    const QJsonObject asset = answer.value(QStringLiteral("asset")).toObject();
    offer.hasDownload = !asset.isEmpty();
    offer.url = asset.value(QStringLiteral("url")).toString();
    offer.size = static_cast<qint64>(asset.value(QStringLiteral("size")).toDouble());
    offer.manifest = data;
  } catch (const std::exception& e) {
    fail(tr("The update manifest was refused: %1.").arg(errorText(e)));
    return;
  }
  qInfo().noquote() << QStringLiteral("Update manifest: Mitcad %1 of %2 (%3)%4")
                           .arg(offer.version, offer.date, offer.status,
                                offer.hasDownload ? QStringLiteral(", %1 bytes for %2").arg(offer.size).arg(m_source.platform)
                                                  : QStringLiteral(", no download for this platform"));
  emit checked(offer);
}

} // namespace mitcad
