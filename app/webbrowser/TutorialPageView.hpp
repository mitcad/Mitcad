// SPDX-License-Identifier: MIT
#pragma once

// The tutorial pane's page in the web browser component's module
// (mitcad#2012, ComponentBrowserHost.hpp's TutorialPage): a CEF view whose
// pages in the tutorial package's folder get window.mitcad. What a page
// logs on its console as "mitcad:tutorial:<text>" goes to MitCad's log as
// "Tutorial page says: <text>" (the tutorials' own diagnostics, and the UI
// tests'); each page loaded is logged with whether it has the bridge.

#include <QString>
#include <QWidget>

#include "ComponentBrowserHost.hpp"

namespace mitcad {

class CefView;

class TutorialPageView : public QWidget, public TutorialPage {
  Q_OBJECT

public:
  TutorialPageView(TutorialPageHost* host, const QString& folder, QWidget* parent);

  QWidget* widget() override { return this; }
  void open(const QString& page) override;
  void sendEvent(const QString& json) override;

  // For the tests (TestDriver's invoke-widget tutorialPage clickLink …):
  // follows the first link whose text or address contains `text`, as a
  // click there would.
  Q_INVOKABLE void clickLink(const QString& text);

private:
  TutorialPageHost* m_host = nullptr;
  QString m_folder;
  CefView* m_view = nullptr;
};

} // namespace mitcad
