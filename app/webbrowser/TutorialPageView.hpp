// SPDX-License-Identifier: MIT
#pragma once

// The tutorial pane's page in the web browser component's module
// (mitcad#2012, ComponentBrowserHost.hpp's TutorialPage): a CEF view whose
// pages in the tutorial package's folder get window.mitcad. What a page
// logs on its console as "mitcad:tutorial:<text>" goes to MitCad's log as
// "Tutorial page says: <text>" (the tutorials' own diagnostics, and the UI
// tests'; cut and escaped, mitcad#2615); each page loaded is logged with
// whether it has the bridge. The page stays in the package's folder
// (CefView, mitcad#2615); a link to the web the user clicked is set as the
// widget's property TutorialPageRules.hpp's kWebLinkProperty, which the
// application watches.

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
  // follows the first link whose text or address contains `text`, or
  // presses the first button whose text contains it (mitcad#2610), as a
  // click there would. Only in a test run (MITCAD_TEST_INPUT), and only
  // the link or button it found, by a one-off key (mitcad#2615).
  Q_INVOKABLE void clickLink(const QString& text);

private:
  TutorialPageHost* m_host = nullptr;
  QString m_folder;
  CefView* m_view = nullptr;
  // The key of the clickLink under way; empty when none.
  QString m_clickKey;
};

} // namespace mitcad
