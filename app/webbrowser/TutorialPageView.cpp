// SPDX-License-Identifier: MIT
// The tutorial pane's page (mitcad#2012): see TutorialPageView.hpp.
#include "TutorialPageView.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QPoint>
#include <QStringList>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "cef/CefBridge.hpp"
#include "cef/CefEngine.hpp"
#include "cef/CefView.hpp"

namespace mitcad {

TutorialPageView::TutorialPageView(TutorialPageHost* host, const QString& folder, QWidget* parent)
    : QWidget(parent), m_host(host), m_folder(folder) {
  setObjectName(QStringLiteral("tutorialPage"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  m_view = new CefView(this, folder);
  m_view->setObjectName(QStringLiteral("tutorialPageView"));
  // A tutorial downloads nothing.
  m_view->beforeDownload = [](const QString&, const QUrl&) { return QString(); };
  m_view->bridgeCall = [this](const QString& request, QString* answer, QString* error) {
    return m_host->bridgeCall(request, answer, error);
  };
  layout->addWidget(m_view, 1);
  connect(m_view, &CefView::loadFinished, this, [this](bool ok) {
    const QString url = m_view->url().toString();
    qInfo().noquote() << QStringLiteral("Tutorial page %1: %2 (bridge: %3)")
                             .arg(ok ? QStringLiteral("loaded") : QStringLiteral("failed"), url,
                                  cef::bridgeAllows(m_folder, url) ? QStringLiteral("yes") : QStringLiteral("no"));
  });
  connect(m_view, &CefView::consoleMessage, this, [this](const QString& message) {
    static const QString said = QStringLiteral("mitcad:tutorial:");
    static const QString linkAt = QStringLiteral("mitcad:linkat:");
    if (message.startsWith(said)) {
      qInfo().noquote() << QStringLiteral("Tutorial page says: %1").arg(message.mid(said.size()));
    } else if (message.startsWith(linkAt)) {
      const QStringList parts = message.mid(linkAt.size()).split(QLatin1Char(','));
      if (parts.size() >= 2) {
        qInfo().noquote() << QStringLiteral("Tutorial page: following %1").arg(parts.mid(2).join(QLatin1Char(',')));
        m_view->clickAt(QPoint(parts.at(0).toInt(), parts.at(1).toInt()));
      }
    } else if (message.startsWith(QStringLiteral("mitcad:nolink:"))) {
      qWarning().noquote() << QStringLiteral("Tutorial page: no link %1").arg(message.mid(14));
    }
  });
}

void TutorialPageView::open(const QString& page) { m_view->load(QUrl::fromLocalFile(page)); }

void TutorialPageView::sendEvent(const QString& json) { m_view->sendBridgeEvent(json); }

void TutorialPageView::clickLink(const QString& text) {
  const QString quoted = QString::fromUtf8(QJsonDocument(QJsonArray{text}).toJson(QJsonDocument::Compact));
  m_view->runJavaScript(
      QStringLiteral("(function(t){var a=Array.prototype.find.call(document.links,function(l){"
                     "return l.textContent.indexOf(t)>=0||l.href.indexOf(t)>=0;});"
                     "if(!a){console.log('mitcad:nolink:'+t);return;}"
                     "a.scrollIntoView({block:'center'});var r=a.getBoundingClientRect();"
                     "console.log('mitcad:linkat:'+Math.round(r.left+r.width/2)+','+"
                     "Math.round(r.top+r.height/2)+','+a.href);})(%1[0])")
          .arg(quoted));
}

} // namespace mitcad

extern "C" Q_DECL_EXPORT mitcad::TutorialPage* mitcad_create_tutorial_page(mitcad::TutorialPageHost* host,
                                                                           const QString& folder, QWidget* parent) {
  if (host == nullptr) {
    return nullptr;
  }
  QString error;
  if (!mitcad::cef::startEngine(host->componentFolder(), host->profileFolder(), &error)) {
    qWarning().noquote() << QStringLiteral("Tutorial page: %1").arg(error);
    return nullptr;
  }
  return new mitcad::TutorialPageView(host, folder, parent);
}
