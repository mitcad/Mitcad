// SPDX-License-Identifier: MIT
#pragma once

// The web browser component's requests (mitcad#1579), with Qt Network as the
// updates make them (UpdateClient): HTTPS only, redirects only to HTTPS, no
// cookies, no cache; the update manifest, then the package into a file.
// What they bring is verified by the bridge (BrowserComponentInstall.hpp),
// never here.

#include <QByteArray>
#include <QFile>
#include <QObject>
#include <QPointer>
#include <QString>

class QNetworkAccessManager;
class QNetworkReply;
class QUrl;

namespace mitcad {

class BrowserComponentDownload : public QObject {
  Q_OBJECT

public:
  explicit BrowserComponentDownload(QObject* parent = nullptr);
  ~BrowserComponentDownload() override;

  // The update manifest at `url`: manifestFetched() or failed().
  void fetchManifest(const QString& url);
  // The package at `url` into `path`, `expected` bytes: progress(), then
  // downloaded() or failed(); a failed or cancelled download leaves no file.
  void download(const QString& url, const QString& path, qint64 expected);
  // Stops the request: no signal follows.
  void cancel();
  bool busy() const { return !m_reply.isNull(); }

signals:
  void manifestFetched(const QByteArray& manifest);
  void progress(qint64 received, qint64 total);
  void downloaded(const QString& path);
  void failed(const QString& message);

private:
  QNetworkReply* get(const QUrl& url);
  void fail(const QString& message);

  QNetworkAccessManager* m_network = nullptr;
  QPointer<QNetworkReply> m_reply;
  QFile m_file;
  qint64 m_expected = 0;
};

} // namespace mitcad
