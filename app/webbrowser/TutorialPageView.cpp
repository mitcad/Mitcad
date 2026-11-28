// SPDX-License-Identifier: MIT
// The tutorial pane's page (mitcad#2012): see TutorialPageView.hpp.
#include "TutorialPageView.hpp"

#include <QPoint>
#include <QUrl>
#include <QVariant>
#include <QVBoxLayout>
#include <QtLogging>

#include "TutorialPageRules.hpp"
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
                             .arg(ok ? QStringLiteral("loaded") : QStringLiteral("failed"),
                                  tutorialrules::logSafeText(url, 300),
                                  cef::bridgeAllows(m_folder, url) ? QStringLiteral("yes") : QStringLiteral("no"));
  });
  connect(m_view, &CefView::consoleMessage, this, [this](const QString& message, bool fromMitCad) {
    static const QString said = QStringLiteral("mitcad:tutorial:");
    if (message.startsWith(said)) {
      // The page's own words, cut and escaped: the log goes with Help ›
      // Send Feedback (mitcad#2615).
      qInfo().noquote() << QStringLiteral("Tutorial page says: %1")
                               .arg(tutorialrules::logSafeText(message.mid(said.size()),
                                                               tutorialrules::kLoggedPageText));
      return;
    }
    if (!fromMitCad) {
      return; // a page's line in MitCad's form does nothing (mitcad#2615)
    }
    int x = 0;
    int y = 0;
    QString href;
    if (tutorialrules::parseLinkAt(message, m_clickKey, &x, &y, &href)) {
      // The one link clickLink asked about, once, in a test run.
      m_clickKey.clear();
      qInfo().noquote() << QStringLiteral("Tutorial page: following %1").arg(tutorialrules::logSafeText(href, 300));
      m_view->clickAt(QPoint(x, y));
    } else if (message.startsWith(QStringLiteral("mitcad:nolink:"))) {
      m_clickKey.clear();
      qWarning().noquote() << QStringLiteral("Tutorial page: no link %1")
                                  .arg(tutorialrules::logSafeText(message.mid(14), 300));
    }
  });
  // A link to the web the user clicked (mitcad#2615): the application asks
  // and opens it in the system's browser (TutorialPageRules.hpp's
  // kWebLinkProperty; an application that does not watch it leaves it).
  connect(m_view, &CefView::webLinkClicked, this, [this](const QUrl& url) {
    qInfo().noquote() << QStringLiteral("Tutorial page: a link to the web, %1")
                             .arg(tutorialrules::logSafeText(url.host(), 200));
    setProperty(tutorialrules::kWebLinkProperty, url);
    setProperty(tutorialrules::kWebLinkProperty, QVariant());
  });
}

void TutorialPageView::open(const QString& page) { m_view->load(QUrl::fromLocalFile(page)); }

void TutorialPageView::sendEvent(const QString& json) { m_view->sendBridgeEvent(json); }

void TutorialPageView::clickLink(const QString& text) {
  // Real input in a page only in a test run, and only at the link or
  // button this call found (mitcad#2610): the page cannot ask for a click
  // of its own (mitcad#2615).
  if (!tutorialrules::scriptedClicksAllowed()) {
    qWarning().noquote() << QStringLiteral("Tutorial page: clickLink is for test runs only");
    return;
  }
  m_clickKey = tutorialrules::newClickKey();
  m_view->runJavaScript(tutorialrules::linkAtScript(text, m_clickKey, true));
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

// The rules this module keeps for tutorial pages (mitcad#2615,
// TutorialPageRules.hpp): an application shows tutorials only on a module
// that keeps them.
extern "C" Q_DECL_EXPORT int mitcad_tutorial_page_guards() { return mitcad::tutorialrules::kTutorialPageGuards; }
