// SPDX-License-Identifier: MIT
#include "TextureFetcher.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QMetaObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QtLogging>

#include "TextureSource.hpp"

namespace mitcad {
namespace {

constexpr int kMaxRedirects = 5;
// No data for this long ends a request.
constexpr int kTimeoutMs = 30000;
// Why a request was ended here (the reply's property): a TextureReply::Error.
const char* const kWhy = "mitcadWhy";

// Only the host, for messages.
QString hostOf(const QUrl& url) { return url.scheme() + QStringLiteral("://") + url.host(); }

} // namespace

void TextureFetcher::later(const QPointer<QObject>& context, std::function<void()> call) {
  QCoreApplication* app = QCoreApplication::instance();
  if (context.isNull() || app == nullptr) {
    return;
  }
  QMetaObject::invokeMethod(
      app,
      [context, call = std::move(call)] {
        if (!context.isNull()) {
          call();
        }
      },
      Qt::QueuedConnection);
}

QString TextureFetcher::refusal(const QUrl& url) const {
  if (!(m_allowed ? m_allowed() : TextureSettings::load().allowNetwork)) {
    return tr("Texture sources on the internet are off: the Textures page of Tools › Preferences allows them.");
  }
  if (!url.isValid() || url.scheme() != QLatin1String("https") || url.host().isEmpty()) {
    return tr("Textures are fetched over HTTPS only, not from %1.").arg(url.toString());
  }
  return QString();
}

QString NetworkTextureFetcher::userAgent() {
  const QString version = QCoreApplication::applicationVersion();
  return QStringLiteral("MitCad/%1").arg(version.isEmpty() ? QStringLiteral("dev") : version);
}

void NetworkTextureFetcher::get(const QUrl& url, qint64 maxBytes, QObject* context, Progress progress, Done done) {
  const QPointer<QObject> guard(context);
  const QString refused = refusal(url);
  if (!refused.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Texture request refused: %1").arg(refused);
    TextureReply reply;
    reply.error = TextureReply::Error::Refused;
    reply.detail = refused;
    later(guard, [reply, done = std::move(done)] { done(reply); });
    return;
  }
  if (m_network == nullptr) {
    // Made on the first request: with the network off there is none.
    m_network = new QNetworkAccessManager(this);
  }
  QNetworkRequest request(url);
  request.setHeader(QNetworkRequest::UserAgentHeader, userAgent());
  // Qt would send the user's languages.
  request.setRawHeader("Accept-Language", "*");
  request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::UserVerifiedRedirectPolicy);
  request.setMaximumRedirectsAllowed(kMaxRedirects);
  request.setTransferTimeout(kTimeoutMs);
  request.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
  request.setAttribute(QNetworkRequest::CacheSaveControlAttribute, false);
  request.setAttribute(QNetworkRequest::CookieLoadControlAttribute, QNetworkRequest::Manual);
  request.setAttribute(QNetworkRequest::CookieSaveControlAttribute, QNetworkRequest::Manual);
  qDebug().noquote() << QStringLiteral("Texture request %1").arg(url.toString());
  QNetworkReply* reply = m_network->get(request);
  if (context != nullptr) {
    // The asker is gone: the request stops.
    connect(context, &QObject::destroyed, reply, [reply] {
      reply->setProperty("mitcadGone", true);
      reply->abort();
    });
  }
  connect(reply, &QNetworkReply::redirected, reply, [reply](const QUrl& to) {
    if (to.scheme() == QLatin1String("https")) {
      emit reply->redirectAllowed();
    } else {
      reply->setProperty(kWhy, static_cast<int>(TextureReply::Error::Refused));
      reply->setProperty("mitcadDetail", tr("The server redirected to an address that is not HTTPS (%1).")
                                             .arg(hostOf(to)));
      reply->abort();
    }
  });
  connect(reply, &QNetworkReply::downloadProgress, reply, [reply, guard, maxBytes, progress](qint64 received,
                                                                                             qint64 total) {
    if (received > maxBytes || total > maxBytes) {
      reply->setProperty(kWhy, static_cast<int>(TextureReply::Error::TooLarge));
      reply->abort();
      return;
    }
    if (progress && !guard.isNull()) {
      progress(received, total);
    }
  });
  connect(reply, &QNetworkReply::finished, reply, [reply, guard, done = std::move(done)] {
    reply->deleteLater();
    if (guard.isNull() || reply->property("mitcadGone").toBool()) {
      return;
    }
    TextureReply answer;
    answer.status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QVariant why = reply->property(kWhy);
    if (why.isValid()) {
      answer.error = static_cast<TextureReply::Error>(why.toInt());
      answer.detail = reply->property("mitcadDetail").toString();
    } else if (answer.status >= 400) {
      answer.error = TextureReply::Error::Http;
      answer.detail = reply->attribute(QNetworkRequest::HttpReasonPhraseAttribute).toString();
    } else if (reply->error() == QNetworkReply::TimeoutError ||
               reply->error() == QNetworkReply::OperationCanceledError) {
      // The transfer timeout cancels the request.
      answer.error = TextureReply::Error::Timeout;
      answer.detail = reply->errorString();
    } else if (reply->error() != QNetworkReply::NoError) {
      answer.error = TextureReply::Error::Offline;
      answer.detail = reply->errorString();
    } else {
      answer.data = reply->readAll();
    }
    if (!answer.ok()) {
      qDebug().noquote() << QStringLiteral("Texture request %1 failed: %2 (HTTP %3)")
                                .arg(hostOf(reply->url()), answer.detail)
                                .arg(answer.status);
    }
    done(answer);
  });
}

FileTextureFetcher::FileTextureFetcher(QString folder, Allowed allowed, QObject* parent)
    : TextureFetcher(std::move(allowed), parent), m_folder(QDir::cleanPath(std::move(folder))) {}

void FileTextureFetcher::get(const QUrl& url, qint64 maxBytes, QObject* context, Progress progress, Done done) {
  const QPointer<QObject> guard(context);
  TextureReply reply;
  reply.detail = refusal(url);
  if (!reply.detail.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Texture request refused: %1").arg(reply.detail);
    reply.error = TextureReply::Error::Refused;
  } else {
    qDebug().noquote() << QStringLiteral("Texture request %1").arg(url.toString());
    const QString path = QDir::cleanPath(m_folder + QLatin1Char('/') + url.host() + url.path());
    QFile file(path);
    if (!path.startsWith(m_folder + QLatin1Char('/')) || !QFileInfo(path).isFile() ||
        !file.open(QIODevice::ReadOnly)) {
      reply.error = TextureReply::Error::Http;
      reply.status = 404;
      reply.detail = QStringLiteral("Not Found");
    } else if (file.size() > maxBytes) {
      reply.error = TextureReply::Error::TooLarge;
    } else {
      reply.data = file.readAll();
      reply.status = 200;
    }
  }
  later(guard, [reply, progress = std::move(progress), done = std::move(done)] {
    if (reply.ok() && progress) {
      progress(reply.data.size(), reply.data.size());
    }
    done(reply);
  });
}

std::unique_ptr<TextureFetcher> makeTextureFetcher() {
  const QString fixtures = qEnvironmentVariable("MITCAD_TEXTURE_FIXTURES");
  if (!fixtures.isEmpty()) {
    return std::make_unique<FileTextureFetcher>(fixtures);
  }
  return std::make_unique<NetworkTextureFetcher>();
}

} // namespace mitcad
