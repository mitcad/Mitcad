// SPDX-License-Identifier: MIT
// The tutorials' bridge on the browser side (mitcad#2012): see CefBridge.hpp.
#include "CefBridge.hpp"

#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUrl>
#include <QtLogging>

namespace mitcad::cef {

std::string bridgePrefix(const QString& folder) {
  const QString root = QDir(folder).absolutePath();
  return QUrl::fromLocalFile(root.endsWith(QLatin1Char('/')) ? root : root + QLatin1Char('/'))
      .toString(QUrl::FullyEncoded)
      .toStdString();
}

bool bridgeAllows(const QString& folder, const QString& url) {
  const QUrl address(url);
  if (!address.isLocalFile()) {
    return false;
  }
  const QString file = QFileInfo(address.toLocalFile()).canonicalFilePath();
  const QString root = QFileInfo(folder).canonicalFilePath();
  if (file.isEmpty() || root.isEmpty()) {
    return false;
  }
#if defined(Q_OS_WIN)
  const Qt::CaseSensitivity sensitivity = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity sensitivity = Qt::CaseSensitive;
#endif
  return file.startsWith(root + QLatin1Char('/'), sensitivity);
}

bool BridgeHandler::OnQuery(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, int64_t queryId,
                            const CefString& request, bool persistent, CefRefPtr<Callback> callback) {
  const QString page = QString::fromStdString(frame->GetURL().ToString());
  if (!bridgeAllows(m_folder, page)) {
    qWarning().noquote() << QStringLiteral("Tutorial bridge: refused for %1, outside the tutorial's folder").arg(page);
    callback->Failure(403, "window.mitcad is only for the pages of an installed MitCad tutorial");
    return true;
  }
  const QString text = QString::fromStdString(request.ToString());
  const QJsonObject call = QJsonDocument::fromJson(text.toUtf8()).object();
  if (call.value(QStringLiteral("call")).toString() == QLatin1String("events")) {
    if (!persistent) {
      callback->Failure(400, "events is a persistent query");
      return true;
    }
    m_listeners[queryId] = callback;
    return true;
  }
  // Answered from the event loop; a view gone meanwhile answers nothing
  // (its browser's close cancels the query).
  m_post(text, [callback](bool ok, const QString& answer) {
    if (ok) {
      callback->Success(answer.toStdString());
    } else {
      callback->Failure(400, answer.toStdString());
    }
  });
  return true;
}

void BridgeHandler::OnQueryCanceled(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame>, int64_t queryId) {
  m_listeners.erase(queryId);
}

void BridgeHandler::send(const QString& json) {
  const std::string text = json.toStdString();
  for (auto& listener : m_listeners) {
    listener.second->Success(text);
  }
}

} // namespace mitcad::cef
