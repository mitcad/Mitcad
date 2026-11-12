// SPDX-License-Identifier: MIT
#pragma once

// A CEF browser shown in a Qt widget (mitcad#1579): windowless (off-screen)
// rendering, CefRenderHandler::OnPaint's software buffer painted into the
// widget (never a GPU texture: no GPU in VMs and test sessions), and the
// widget's mouse, wheel, keys, input method, clipboard and cursor forwarded
// to it. No native child window, so it needs no native embedding on any
// platform (Wayland, macOS). Downloads go where the owner says
// (beforeDownload), with their progress (downloadUpdated).
//
// Sharp at any display scale (mitcad#1657): CEF paints windowless frames at
// a scale of 1 whatever GetScreenInfo says (156.0.27 and .28, at 1.65 and 2),
// so the view is given to CEF in physical pixels at a scale of 1, and the
// page's zoom is the display's scale: the page is laid out at the view's
// size in CSS pixels and painted into every physical pixel. Mouse, wheel,
// clickAt, the input method's and the popup's rectangles are converted
// between the view's (logical) and CEF's (physical) coordinates.

#include <cstdint>
#include <functional>
#include <map>
#include <set>

#include <QImage>
#include <QPointer>
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
class QWindow;

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
  // A left click at a point of the view, as real input (the tests'): a
  // point of the page in its CSS pixels, which are the view's own at any
  // display scale (the page's zoom is the scale).
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
  bool event(QEvent* event) override;
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

private slots:
  // The window's scale or screen changed (mitcad#1654): CEF asks for the
  // screen's information again (GetScreenInfo). Invokable by the tests.
  void screenChanged();

private:
  friend class cef::BrowserClient;

  void createBrowser();
  CefRefPtr<CefBrowserHost> host() const;
  void sendMouse(QMouseEvent* event, bool up, int clicks);
  void sendKey(QKeyEvent* event, bool down);

  // The display's scale handed to CEF (mitcad#1657): the view in physical
  // pixels (viewRect) and the page's zoom (applyZoom).
  double scale() const { return m_scale; }
  // The view's rectangle in CEF's (physical) pixels, at the window's scale,
  // which it takes (the zoom follows soon after when it changed).
  CefRect viewRect();
  // Takes the window's scale and tells CEF: the screen, the size, the zoom.
  void applyScale();
  // The page's zoom set to the scale, unless it is already: after every
  // navigation's commit (Chromium keeps a zoom per site in the browsing
  // profile, which would otherwise apply) and every change of the scale.
  void applyZoom();
  // A point of the view in CEF's pixels, and a rectangle of CEF's in the view's.
  CefMouseEvent mouseEventAt(const QPointF& position, Qt::KeyboardModifiers modifiers,
                             Qt::MouseButtons buttons) const;
  QRect fromCef(const QRect& rect) const;

  // From the client (CEF's browser thread, which is Qt's main thread).
  void browserCreated(CefRefPtr<CefBrowser> browser);
  void browserClosing();
  void painted(bool popup, const void* buffer, int width, int height);
  void popupShown(bool show);
  void popupSized(const QRect& rect);
  void imeBoundsChanged(const QRect& caret);
  void pageCommitted();
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
  // The scale CEF's pixels are of (mitcad#1657): the window's when CEF last
  // asked for the view's rectangle.
  double m_scale = 1.0;
  // The last frame's size and scale the log told (mitcad#1654).
  QString m_frameLogged;
  QPointer<QWindow> m_screenWindow; // whose screenChanged() is connected
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
