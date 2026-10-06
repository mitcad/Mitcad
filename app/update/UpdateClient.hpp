// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9): the requests, with Qt Network. HTTPS only;
// redirects are followed from HTTPS to HTTPS and no further. The requests
// tell nothing but Mitcad's version, the platform and the channel (the
// User-Agent): no cookies, no cache, no languages. What a response says is
// read and verified by the core (core/update), never here.

#include <QByteArray>
#include <QFile>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QSslCertificate>
#include <QString>

#include "UpdateSettings.hpp"

class QNetworkAccessManager;
class QNetworkReply;
class QUrl;

namespace mitcad {

// What a release manifest offers the running version (core/update: check).
struct UpdateOffer {
  QString status; // "available", "skipped" or "current"
  QString version;
  QString date;
  QString notes; // the release's page
  bool prerelease = false;
  // The download for this platform; without one the release is announced
  // only.
  bool hasDownload = false;
  QString url;
  qint64 size = 0;
  // The manifest as received, verified again with the download.
  QByteArray manifest;
};

class UpdateClient : public QObject {
  Q_OBJECT

public:
  explicit UpdateClient(UpdateSource source, QObject* parent = nullptr);
  ~UpdateClient() override;

  const UpdateSource& source() const { return m_source; }

  // Fetches the channel's manifest (the pre-release channel: GitHub's list
  // of releases first) and reads it: checked() or failed().
  void check(UpdateSettings::Channel channel, const QString& skippedVersion);
  // Downloads the offer's file into `path`: progress(), then downloaded() or
  // failed(); a failed or cancelled download leaves no file.
  void download(const UpdateOffer& offer, const QString& path);
  // Stops a check or a download: neither signal follows.
  void cancel();
  bool busy() const { return m_reply != nullptr; }

signals:
  void checked(const UpdateOffer& offer);
  void progress(qint64 received, qint64 total);
  void downloaded(const QString& path);
  void failed(const QString& message);

private:
  enum class Step { Releases, Manifest, Download };

  QNetworkAccessManager* network();
  // A GET of an HTTPS address; null with failed() emitted otherwise.
  QNetworkReply* get(const QUrl& url, const QByteArray& accept = QByteArray());
  void fetch(const QString& url, Step step);
  void onFinished(QNetworkReply* reply, Step step);
  void readManifest(const QByteArray& data);
  void fail(const QString& message);

  UpdateSource m_source;
  QList<QSslCertificate> m_testCertificates;
  QNetworkAccessManager* m_network = nullptr;
  QPointer<QNetworkReply> m_reply;
  UpdateSettings::Channel m_channel = UpdateSettings::Channel::Stable;
  QString m_skipped;
  // The download.
  QFile m_file;
  qint64 m_expected = 0;
};

} // namespace mitcad
