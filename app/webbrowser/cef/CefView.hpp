// SPDX-License-Identifier: MIT
#pragma once

// A CEF browser shown in a Qt widget (mitcad#1579): windowless (off-screen)
// rendering, CefRenderHandler::OnPaint's software buffer painted into the
// widget (never a GPU texture: no GPU in VMs and test sessions), and the
// widget's mouse, wheel, keys, input method, clipboard and cursor forwarded
// to it. No native child window, so it needs no native embedding on any
// platform (Wayland, macOS). Downloads go where the owner says
// (beforeDownload), with their progress (downloadUpdated).

#include <cstdint>
#include <functional>
#include <map>
#include <set>

#include <QImage>
#include <QRect>
#include <QString>
#include <QUrl>
#include <QVariant>
#include <QWidget>

#include "include/cef_browser.h"

class QEvent;
class QFocusEvent;
class QHideEvent;
class QInputMethodEvent;
class QKeyEvent;
class QMouseEvent;
class QPaintEvent;
class QResizeEvent;
class QShowEvent;
class QWheelEvent;

namespace mitcad {

namespace cef {
class BrowserClient;
}

class CefView : public QWidget {
  Q_OBJECT

public:
  // A download's state for downloadUpdated.
  enum class DownloadState { InProgress, Complete, Canceled, Failed };

  explicit CefView(QWidget* parent = nullptr);
  ~CefView() override;

  void load(const QUrl& url);
  // A page of its own (a data: address).
  void setHtml(const QString& html);
  void back();
  void forward();
  void reload();
  bool canGoBack() const { return m_canGoBack; }
  bool canGoForward() const { return m_canGoForward; }
  QUrl url() const { return m_url; }
  QString title() const { return m_title; }
  // Runs a script in the page; what it tells MitCad comes back as
  // consoleMessage() of the lines it logs that start with "mitcad:".
  void runJavaScript(const QString& script);
  // A left click at a point of the page, as real input (the tests').
  void clickAt(const QPoint& point);
  // Cookies and site sign-ins, HTTP authentication, the cache.
  void clearBrowsingData();
  // Before CEF ends (CefEngine): the browser closed and let go.
  void closeBrowser();

  // Where a download goes: its full path, or empty to refuse it. Called
  // with the file's suggested name and the address.
  std::function<QString(const QString& suggestedName, const QUrl& url)> beforeDownload;

signals:
  void urlChanged(const QUrl& url);
  void titleChanged(const QString& title);
  void loadFinished(bool ok);
  void navigationChanged();
  void consoleMessage(const QString& message);
  // state: a DownloadState.
  void downloadUpdated(quint32 id, qint64 received, qint64 total, int state, const QString& path,
                       const QString& reason);

protected:
  void paintEvent(QPaintEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void wheelEvent(QWheelEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void keyReleaseEvent(QKeyEvent* event) override;
  void focusInEvent(QFocusEvent* event) override;
  void focusOutEvent(QFocusEvent* event) override;
  void inputMethodEvent(QInputMethodEvent* event) override;
  QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;
  bool focusNextPrevChild(bool next) override;

private:
  friend class cef::BrowserClient;

  void createBrowser();
  CefRefPtr<CefBrowserHost> host() const;
  void sendMouse(QMouseEvent* event, bool up, int clicks);
  void sendKey(QKeyEvent* event, bool down);

  // From the client (CEF's browser thread, which is Qt's main thread).
  void browserCreated(CefRefPtr<CefBrowser> browser);
  void browserClosing();
  void painted(bool popup, const void* buffer, int width, int height);
  void popupShown(bool show);
  void popupSized(const QRect& rect);
  void imeBoundsChanged(const QRect& caret);
  void addressChanged(const QString& url);
  void titleChangedTo(const QString& title);
  void loadingChanged(bool loading, bool canGoBack, bool canGoForward);
  void loadEnded(bool ok);
  void cursorChanged(int type);
  void logged(const QString& message);
  QString downloadStarting(std::uint32_t id, const QString& suggestedName, const QUrl& url);
  bool downloadRefused(std::uint32_t id) const { return m_refused.count(id) != 0; }
  void downloadChanged(std::uint32_t id, qint64 received, qint64 total, DownloadState state, const QString& path,
                       const QString& reason);

  CefRefPtr<cef::BrowserClient> m_client;
  CefRefPtr<CefBrowser> m_browser;
  bool m_creating = false;
  QUrl m_pending; // loaded once the browser exists
  QUrl m_url;
  QString m_title;
  bool m_canGoBack = false;
  bool m_canGoForward = false;
  QImage m_frame;
  QImage m_popup;
  QRect m_popupRect;
  bool m_popupShown = false;
  QRect m_caret;
  std::set<std::uint32_t> m_refused;
  // What the log said last of each download.
  struct DownloadLog {
    bool seen = false;
    DownloadState state = DownloadState::InProgress;
    int tenth = 0;
  };
  std::map<std::uint32_t, DownloadLog> m_downloadLog;
};

} // namespace mitcad
