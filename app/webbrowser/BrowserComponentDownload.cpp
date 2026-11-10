// SPDX-License-Identifier: MIT
// The web browser component's requests (mitcad#1579): see
// BrowserComponentDownload.hpp.
#include "BrowserComponentDownload.hpp"

#include <QCoreApplication>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QUrl>
#include <QtLogging>

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(BrowserComponent)
};

// The largest update manifest read (it lists a few downloads).
constexpr qint64 kManifestLimit = 1 << 20;

} // namespace

BrowserComponentDownload::BrowserComponentDownload(QObject* parent) : QObject(parent) {}

BrowserComponentDownload::~BrowserComponentDownload() { cancel(); }

QNetworkReply* BrowserComponentDownload::get(const QUrl& url) {
  if (url.scheme() != QStringLiteral("https")) {
    fail(Text::tr("The address %1 is not HTTPS.").arg(url.toDisplayString()));
    return nullptr;
  }
  if (m_network == nullptr) {
    m_network = new QNetworkAccessManager(this);
  }
  QNetworkRequest request(url);
  request.setHeader(QNetworkRequest::UserAgentHeader,
                    QStringLiteral("MitCad/%1 (web browser component)").arg(QCoreApplication::applicationVersion()));
  request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::UserVerifiedRedirectPolicy);
  request.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
  request.setAttribute(QNetworkRequest::CacheSaveControlAttribute, false);
  request.setAttribute(QNetworkRequest::CookieLoadControlAttribute, QNetworkRequest::Manual);
  request.setAttribute(QNetworkRequest::CookieSaveControlAttribute, QNetworkRequest::Manual);
  QNetworkReply* reply = m_network->get(request);
  // Redirects only from HTTPS to HTTPS (GitHub's downloads redirect).
  connect(reply, &QNetworkReply::redirected, reply, [reply](const QUrl& to) {
    if (to.scheme() == QStringLiteral("https")) {
      emit reply->redirectAllowed();
    }
  });
  m_reply = reply;
  return reply;
}

void BrowserComponentDownload::fetchManifest(const QString& url) {
  cancel();
  QNetworkReply* reply = get(QUrl(url));
  if (reply == nullptr) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: reading the update manifest %1").arg(url);
  connect(reply, &QNetworkReply::finished, this, [this, reply] {
    reply->deleteLater();
    if (m_reply != reply) {
      return;
    }
    m_reply.clear();
    if (reply->error() != QNetworkReply::NoError) {
      fail(Text::tr("The update manifest could not be read: %1").arg(reply->errorString()));
      return;
    }
    const QByteArray data = reply->read(kManifestLimit + 1);
    if (data.size() > kManifestLimit) {
      fail(Text::tr("The update manifest is too large."));
      return;
    }
    emit manifestFetched(data);
  });
}

void BrowserComponentDownload::download(const QString& url, const QString& path, qint64 expected) {
  cancel();
  m_file.setFileName(path);
  if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    fail(Text::tr("MitCad could not write %1: %2").arg(path, m_file.errorString()));
    return;
  }
  m_expected = expected;
  QNetworkReply* reply = get(QUrl(url));
  if (reply == nullptr) {
    m_file.remove();
    return;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: downloading the web browser component %1 (%2 bytes)")
                           .arg(url)
                           .arg(expected);
  connect(reply, &QNetworkReply::readyRead, this, [this, reply] {
    if (m_reply != reply) {
      return;
    }
    m_file.write(reply->readAll());
    if (m_expected > 0 && m_file.size() > m_expected) {
      reply->abort();
    }
  });
  connect(reply, &QNetworkReply::downloadProgress, this, [this, reply](qint64 received, qint64 total) {
    if (m_reply == reply) {
      emit progress(received, total > 0 ? total : m_expected);
    }
  });
  connect(reply, &QNetworkReply::finished, this, [this, reply, path] {
    reply->deleteLater();
    if (m_reply != reply) {
      return;
    }
    m_reply.clear();
    m_file.write(reply->readAll());
    m_file.close();
    if (reply->error() != QNetworkReply::NoError || (m_expected > 0 && m_file.size() != m_expected)) {
      const QString reason = reply->error() != QNetworkReply::NoError
                                 ? reply->errorString()
                                 : Text::tr("%1 bytes instead of %2").arg(m_file.size()).arg(m_expected);
      m_file.remove();
      fail(Text::tr("The web browser component could not be downloaded: %1").arg(reason));
      return;
    }
    emit downloaded(path);
  });
}

void BrowserComponentDownload::cancel() {
  if (!m_reply.isNull()) {
    QNetworkReply* reply = m_reply.data();
    m_reply.clear();
    reply->abort();
    reply->deleteLater();
  }
  if (m_file.isOpen()) {
    m_file.close();
    m_file.remove();
  }
}

void BrowserComponentDownload::fail(const QString& message) {
  qWarning().noquote() << QStringLiteral("Component from Web: %1").arg(message);
  emit failed(message);
}

} // namespace mitcad
