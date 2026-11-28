// SPDX-License-Identifier: MIT
// The tutorials' bridge on the browser side (mitcad#2012): see CefBridge.hpp.
#include "CefBridge.hpp"

#include <chrono>
#include <cstddef>
#include <iterator>
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
    qWarning().noquote() << QStringLiteral("Tutorial bridge: refused for %1, outside the tutorial's folder")
                                .arg(tutorialrules::logSafeText(page, 200));
    callback->Failure(403, "window.mitcad is only for the pages of an installed MitCad tutorial");
    return true;
  }
  const std::string bytes = request.ToString();
  // A request larger than any call needs is not read (mitcad#2615).
  if (bytes.size() > static_cast<std::size_t>(tutorialrules::kBridgeRequestBytes)) {
    qInfo().noquote() << QStringLiteral("Tutorial bridge: refused a call of %1 bytes").arg(bytes.size());
    callback->Failure(413, "The call is too large");
    return true;
  }
  const QString text = QString::fromStdString(bytes);
  const QJsonObject call = QJsonDocument::fromJson(text.toUtf8()).object();
  const QString name = call.value(QStringLiteral("call")).toString();
  if (name == QLatin1String("events")) {
    if (!persistent) {
      callback->Failure(400, "events is a persistent query");
      return true;
    }
    // One listener per frame (mitcad#2615): window.mitcad makes one as the
    // page starts; more would multiply every event.
    const std::string frameId = frame->GetIdentifier().ToString();
    if (m_eventFrames.count(frameId) != 0) {
      callback->Failure(409, "The page listens to the events already");
      return true;
    }
    m_eventFrames[frameId] = queryId;
    m_listeners[queryId] = callback;
    return true;
  }
  // At most so many calls a second, a few of each in flight (mitcad#2615).
  QString reason;
  const qint64 now = std::chrono::duration_cast<std::chrono::milliseconds>(
                         std::chrono::steady_clock::now().time_since_epoch())
                         .count();
  if (!m_limiter.admit(name, static_cast<qint64>(bytes.size()), now, &reason)) {
    if (!m_limitLogged) {
      m_limitLogged = true;
      qInfo().noquote() << QStringLiteral("Tutorial bridge: refused %1: %2")
                               .arg(tutorialrules::logSafeText(name, tutorialrules::kLoggedCallName), reason);
    }
    callback->Failure(429, reason.toStdString());
    return true;
  }
  m_limitLogged = false;
  // Answered from the event loop; a view gone meanwhile answers nothing
  // (its browser's close cancels the query).
  BridgeLimiter* limiter = &m_limiter;
  m_post(text, [callback, limiter, name](bool ok, const QString& answer) {
    limiter->finished(name);
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
  for (auto it = m_eventFrames.begin(); it != m_eventFrames.end();) {
    it = it->second == queryId ? m_eventFrames.erase(it) : std::next(it);
  }
}

void BridgeHandler::send(const QString& json) {
  const std::string text = json.toStdString();
  for (auto& listener : m_listeners) {
    listener.second->Success(text);
  }
}

} // namespace mitcad::cef
