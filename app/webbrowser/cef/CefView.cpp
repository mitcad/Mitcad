// SPDX-License-Identifier: MIT
// A CEF browser shown in a Qt widget (mitcad#1579): see CefView.hpp.
#include "CefView.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

#include <QClipboard>
#include <QCursor>
#include <QDir>
#include <QEvent>
#include <QFileInfo>
#include <QFocusEvent>
#include <QGuiApplication>
#include <QHideEvent>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMouseEvent>
#include <QPainter>
#include <QPaintEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QWheelEvent>
#include <QWindow>
#include <QtLogging>

#include "CefEngine.hpp"
#include "include/cef_client.h"
#include "include/cef_context_menu_handler.h"
#include "include/cef_cookie.h"
#include "include/cef_display_handler.h"
#include "include/cef_download_handler.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_load_handler.h"
#include "include/cef_render_handler.h"
#include "include/cef_request.h"
#include "include/cef_request_handler.h"
#include "include/cef_request_context.h"

namespace mitcad {

namespace cef {

// The browser's handlers, held by CEF; they reach the view while it lives
// (detach() when it goes).
class BrowserClient : public CefClient,
                      public CefRenderHandler,
                      public CefDisplayHandler,
                      public CefLoadHandler,
                      public CefLifeSpanHandler,
                      public CefDownloadHandler,
                      public CefContextMenuHandler,
                      public CefRequestHandler {
public:
  explicit BrowserClient(CefView* view) : m_view(view) {}
  void detach() { m_view = nullptr; }

  CefRefPtr<CefRenderHandler> GetRenderHandler() override { return this; }
  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefDownloadHandler> GetDownloadHandler() override { return this; }
  CefRefPtr<CefContextMenuHandler> GetContextMenuHandler() override { return this; }
  CefRefPtr<CefRequestHandler> GetRequestHandler() override { return this; }

  // CefRequestHandler: every navigation of the main frame, for the log (a
  // link that ends in neither a page nor a download shows there).
  bool OnBeforeBrowse(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, CefRefPtr<CefRequest> request, bool userGesture,
                      bool redirect) override {
    if (frame->IsMain()) {
      qDebug().noquote() << QStringLiteral("Component browser navigates to %1 (%2%3)")
                                .arg(QString::fromStdString(request->GetURL().ToString()),
                                     userGesture ? QStringLiteral("a user's gesture") : QStringLiteral("no gesture"),
                                     redirect ? QStringLiteral(", redirected") : QString());
    }
    return false;
  }

  // CefRenderHandler: the view in physical pixels at a scale of 1, the
  // page's zoom the display's scale (mitcad#1657, CefView.hpp). It cannot
  // scale twice where a CEF would honour the screen's scale (macOS's Retina
  // included): the scale CEF is given is 1 everywhere.
  void GetViewRect(CefRefPtr<CefBrowser>, CefRect& rect) override {
    rect = m_view != nullptr ? m_view->viewRect() : CefRect(0, 0, 1, 1);
  }
  bool GetScreenInfo(CefRefPtr<CefBrowser> browser, CefScreenInfo& info) override {
    if (m_view == nullptr) {
      return false;
    }
    CefRect rect;
    GetViewRect(browser, rect);
    info.device_scale_factor = 1.0f;
    info.rect = rect;
    info.available_rect = rect;
    return true;
  }
  bool GetScreenPoint(CefRefPtr<CefBrowser>, int viewX, int viewY, int& screenX, int& screenY) override {
    if (m_view == nullptr) {
      return false;
    }
    // CEF's screen is of physical pixels as its view.
    const double scale = m_view->scale();
    const QPoint point = m_view->mapToGlobal(QPointF(viewX / scale, viewY / scale)).toPoint();
    screenX = static_cast<int>(std::lround(point.x() * scale));
    screenY = static_cast<int>(std::lround(point.y() * scale));
    return true;
  }
  void OnPaint(CefRefPtr<CefBrowser>, PaintElementType type, const RectList&, const void* buffer, int width,
               int height) override {
    if (m_view != nullptr) {
      m_view->painted(type == PET_POPUP, buffer, width, height);
    }
  }
  void OnPopupShow(CefRefPtr<CefBrowser>, bool show) override {
    if (m_view != nullptr) {
      m_view->popupShown(show);
    }
  }
  void OnPopupSize(CefRefPtr<CefBrowser>, const CefRect& rect) override {
    if (m_view != nullptr) {
      m_view->popupSized(m_view->fromCef(QRect(rect.x, rect.y, rect.width, rect.height)));
    }
  }
  void OnImeCompositionRangeChanged(CefRefPtr<CefBrowser>, const CefRange&, const RectList& bounds) override {
    if (m_view != nullptr && !bounds.empty()) {
      const CefRect& last = bounds.back();
      m_view->imeBoundsChanged(m_view->fromCef(QRect(last.x, last.y, last.width, last.height)));
    }
  }

  // CefDisplayHandler
  void OnAddressChange(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, const CefString& url) override {
    if (m_view != nullptr && frame->IsMain()) {
      m_view->addressChanged(QString::fromStdString(url.ToString()));
    }
  }
  void OnTitleChange(CefRefPtr<CefBrowser>, const CefString& title) override {
    if (m_view != nullptr) {
      m_view->titleChangedTo(QString::fromStdString(title.ToString()));
    }
  }
  bool OnConsoleMessage(CefRefPtr<CefBrowser>, cef_log_severity_t, const CefString& message, const CefString&,
                        int) override {
    if (m_view != nullptr) {
      m_view->logged(QString::fromStdString(message.ToString()));
    }
    return false;
  }
  bool OnCursorChange(CefRefPtr<CefBrowser>, CefCursorHandle, cef_cursor_type_t type, const CefCursorInfo&) override {
    if (m_view != nullptr) {
      m_view->cursorChanged(static_cast<int>(type));
    }
    return true;
  }

  // CefLoadHandler
  void OnLoadingStateChange(CefRefPtr<CefBrowser>, bool loading, bool canGoBack, bool canGoForward) override {
    qDebug().noquote() << QStringLiteral("Component browser loading: %1").arg(loading ? QStringLiteral("yes") : QStringLiteral("no"));
    if (m_view != nullptr) {
      m_view->loadingChanged(loading, canGoBack, canGoForward);
    }
  }
  void OnLoadStart(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, TransitionType) override {
    if (m_view != nullptr && frame->IsMain()) {
      m_view->pageCommitted();
    }
  }
  void OnLoadEnd(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, int status) override {
    if (frame->IsMain()) {
      qDebug().noquote() << QStringLiteral("Component browser load end %1 (status %2)")
                                .arg(QString::fromStdString(frame->GetURL().ToString()))
                                .arg(status);
    }
    if (m_view != nullptr && frame->IsMain()) {
      m_view->loadEnded(true);
    }
  }
  void OnLoadError(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame> frame, ErrorCode code, const CefString& text,
                   const CefString& failed) override {
    // A download is no failed page (it aborts the navigation).
    if (m_view != nullptr && frame->IsMain() && code != ERR_ABORTED) {
      qDebug().noquote() << QStringLiteral("Component browser page error %1 (%2): %3")
                                .arg(static_cast<int>(code))
                                .arg(QString::fromStdString(text.ToString()), QString::fromStdString(failed.ToString()));
      m_view->loadEnded(false);
    }
  }

  // CefLifeSpanHandler
  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override {
    browserCreated(browser);
    if (m_view != nullptr) {
      m_view->browserCreated(browser);
    } else {
      // The view went while the browser was being made.
      browser->GetHost()->CloseBrowser(true);
    }
  }
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override {
    if (m_view != nullptr) {
      m_view->browserClosing();
    }
    browserClosed(browser);
  }

  // CefDownloadHandler
  bool OnBeforeDownload(CefRefPtr<CefBrowser>, CefRefPtr<CefDownloadItem> item, const CefString& suggestedName,
                        CefRefPtr<CefBeforeDownloadCallback> callback) override {
    const QString path = m_view != nullptr
                             ? m_view->downloadStarting(item->GetId(), QString::fromStdString(suggestedName.ToString()),
                                                        QUrl(QString::fromStdString(item->GetURL().ToString())))
                             : QString();
    if (!path.isEmpty()) {
      // No dialog: MitCad's own temporary folder (made by the owner), the
      // full path in the platform's own form.
      const QString target = QDir::toNativeSeparators(QFileInfo(path).absoluteFilePath());
      QDir().mkpath(QFileInfo(target).absolutePath());
      qDebug().noquote() << QStringLiteral("Component browser download %1: continued into %2").arg(item->GetId()).arg(target);
      callback->Continue(target.toStdString(), false);
    } else {
      qDebug().noquote() << QStringLiteral("Component browser download %1: not continued (refused)").arg(item->GetId());
    }
    // Handled here either way: a refused one is never continued, and is
    // canceled as soon as it reports.
    return true;
  }
  void OnDownloadUpdated(CefRefPtr<CefBrowser>, CefRefPtr<CefDownloadItem> item,
                         CefRefPtr<CefDownloadItemCallback> callback) override {
    if (m_view == nullptr || m_view->downloadRefused(item->GetId())) {
      if (item->IsInProgress()) {
        callback->Cancel();
      }
      return;
    }
    CefView::DownloadState state = CefView::DownloadState::InProgress;
    QString reason;
    if (item->IsComplete()) {
      state = CefView::DownloadState::Complete;
    } else if (item->IsCanceled()) {
      state = CefView::DownloadState::Canceled;
    } else if (item->IsInterrupted() || !item->IsInProgress()) {
      // Stopped, neither complete nor canceled: interrupted, after any
      // number of bytes (none too).
      state = CefView::DownloadState::Failed;
      reason = QStringLiteral("interrupted after %1 bytes (reason %2)")
                   .arg(item->GetReceivedBytes())
                   .arg(static_cast<int>(item->GetInterruptReason()));
    }
    m_view->downloadChanged(item->GetId(), item->GetReceivedBytes(), item->GetTotalBytes(), state,
                            QString::fromStdString(item->GetFullPath().ToString()), reason);
  }

  // CefContextMenuHandler: no Chromium menu (an off-screen browser cannot
  // show one).
  void OnBeforeContextMenu(CefRefPtr<CefBrowser>, CefRefPtr<CefFrame>, CefRefPtr<CefContextMenuParams>,
                           CefRefPtr<CefMenuModel> model) override {
    model->Clear();
  }

private:
  CefView* m_view;
  IMPLEMENT_REFCOUNTING(BrowserClient);
};

} // namespace cef

namespace {

// The flags of CEF's events from Qt's modifiers and buttons. On macOS, Qt's
// Control is the Command key and Meta the Control key.
std::uint32_t modifiersOf(Qt::KeyboardModifiers modifiers, Qt::MouseButtons buttons = Qt::NoButton) {
  std::uint32_t flags = 0;
  if (modifiers.testFlag(Qt::ShiftModifier)) {
    flags |= EVENTFLAG_SHIFT_DOWN;
  }
#if defined(__APPLE__)
  if (modifiers.testFlag(Qt::ControlModifier)) {
    flags |= EVENTFLAG_COMMAND_DOWN;
  }
  if (modifiers.testFlag(Qt::MetaModifier)) {
    flags |= EVENTFLAG_CONTROL_DOWN;
  }
#else
  if (modifiers.testFlag(Qt::ControlModifier)) {
    flags |= EVENTFLAG_CONTROL_DOWN;
  }
#endif
  if (modifiers.testFlag(Qt::AltModifier)) {
    flags |= EVENTFLAG_ALT_DOWN;
  }
  if (modifiers.testFlag(Qt::KeypadModifier)) {
    flags |= EVENTFLAG_IS_KEY_PAD;
  }
  if (buttons.testFlag(Qt::LeftButton)) {
    flags |= EVENTFLAG_LEFT_MOUSE_BUTTON;
  }
  if (buttons.testFlag(Qt::MiddleButton)) {
    flags |= EVENTFLAG_MIDDLE_MOUSE_BUTTON;
  }
  if (buttons.testFlag(Qt::RightButton)) {
    flags |= EVENTFLAG_RIGHT_MOUSE_BUTTON;
  }
  return flags;
}

// The zoom level of a zoom factor: Chromium's levels are powers of 1.2.
double zoomLevelOf(double factor) { return std::log(factor) / std::log(1.2); }

// The Windows virtual key code of a Qt key, which CEF takes on every
// platform (windows_key_code); 0 for a key it has none for.
int windowsKeyCode(int key) {
  if ((key >= Qt::Key_A && key <= Qt::Key_Z) || (key >= Qt::Key_0 && key <= Qt::Key_9)) {
    return key; // 'A'..'Z', '0'..'9'
  }
  if (key >= Qt::Key_F1 && key <= Qt::Key_F24) {
    return 0x70 + (key - Qt::Key_F1);
  }
  switch (key) {
  case Qt::Key_Backspace: return 0x08;
  case Qt::Key_Tab:
  case Qt::Key_Backtab: return 0x09;
  case Qt::Key_Return:
  case Qt::Key_Enter: return 0x0D;
  case Qt::Key_Shift: return 0x10;
  case Qt::Key_Control: return 0x11;
  case Qt::Key_Alt: return 0x12;
  case Qt::Key_Pause: return 0x13;
  case Qt::Key_CapsLock: return 0x14;
  case Qt::Key_Escape: return 0x1B;
  case Qt::Key_Space: return 0x20;
  case Qt::Key_PageUp: return 0x21;
  case Qt::Key_PageDown: return 0x22;
  case Qt::Key_End: return 0x23;
  case Qt::Key_Home: return 0x24;
  case Qt::Key_Left: return 0x25;
  case Qt::Key_Up: return 0x26;
  case Qt::Key_Right: return 0x27;
  case Qt::Key_Down: return 0x28;
  case Qt::Key_Print: return 0x2C;
  case Qt::Key_Insert: return 0x2D;
  case Qt::Key_Delete: return 0x2E;
  case Qt::Key_Meta: return 0x5B;
  case Qt::Key_Menu: return 0x5D;
  case Qt::Key_NumLock: return 0x90;
  case Qt::Key_ScrollLock: return 0x91;
  case Qt::Key_Semicolon:
  case Qt::Key_Colon: return 0xBA;
  case Qt::Key_Plus:
  case Qt::Key_Equal: return 0xBB;
  case Qt::Key_Comma:
  case Qt::Key_Less: return 0xBC;
  case Qt::Key_Minus:
  case Qt::Key_Underscore: return 0xBD;
  case Qt::Key_Period:
  case Qt::Key_Greater: return 0xBE;
  case Qt::Key_Slash:
  case Qt::Key_Question: return 0xBF;
  case Qt::Key_QuoteLeft:
  case Qt::Key_AsciiTilde: return 0xC0;
  case Qt::Key_BracketLeft:
  case Qt::Key_BraceLeft: return 0xDB;
  case Qt::Key_Backslash:
  case Qt::Key_Bar: return 0xDC;
  case Qt::Key_BracketRight:
  case Qt::Key_BraceRight: return 0xDD;
  case Qt::Key_Apostrophe:
  case Qt::Key_QuoteDbl: return 0xDE;
  default: return 0;
  }
}

Qt::CursorShape cursorShape(int type) {
  switch (static_cast<cef_cursor_type_t>(type)) {
  case CT_CROSS: return Qt::CrossCursor;
  case CT_HAND: return Qt::PointingHandCursor;
  case CT_IBEAM: return Qt::IBeamCursor;
  case CT_WAIT: return Qt::WaitCursor;
  case CT_HELP: return Qt::WhatsThisCursor;
  case CT_EASTWESTRESIZE: return Qt::SizeHorCursor;
  case CT_NORTHSOUTHRESIZE: return Qt::SizeVerCursor;
  case CT_NORTHEASTSOUTHWESTRESIZE: return Qt::SizeBDiagCursor;
  case CT_NORTHWESTSOUTHEASTRESIZE: return Qt::SizeFDiagCursor;
  case CT_MOVE: return Qt::SizeAllCursor;
  case CT_PROGRESS: return Qt::BusyCursor;
  case CT_NOTALLOWED: return Qt::ForbiddenCursor;
  case CT_GRAB: return Qt::OpenHandCursor;
  case CT_GRABBING: return Qt::ClosedHandCursor;
  case CT_NONE: return Qt::BlankCursor;
  default: return Qt::ArrowCursor;
  }
}

// The invalid range: none given.
CefRange noRange() { return CefRange(UINT32_MAX, UINT32_MAX); }

} // namespace

CefView::CefView(QWidget* parent) : QWidget(parent), m_scale(devicePixelRatioF()) {
  setFocusPolicy(Qt::StrongFocus);
  setMouseTracking(true);
  setAttribute(Qt::WA_InputMethodEnabled, true);
  setAttribute(Qt::WA_OpaquePaintEvent, true);
  cef::addView(this);
  createBrowser();
}

CefView::~CefView() {
  closeBrowser();
  cef::removeView(this);
}

void CefView::createBrowser() {
  if (m_creating || m_browser || !cef::engineRunning()) {
    return;
  }
  m_creating = true;
  m_client = new cef::BrowserClient(this);
  CefWindowInfo info;
  info.SetAsWindowless(kNullWindowHandle);
  // Painted from OnPaint's buffer, never a shared GPU texture.
  info.shared_texture_enabled = false;
  CefBrowserSettings settings;
  settings.windowless_frame_rate = 30;
  settings.background_color = CefColorSetARGB(255, 255, 255, 255);
  if (!CefBrowserHost::CreateBrowser(info, m_client, CefString("about:blank"), settings, nullptr,
                                     cef::requestContext())) {
    qWarning().noquote() << QStringLiteral("Component browser: CEF did not make the browser");
    m_creating = false;
  }
  cef::pumpSoon();
}

CefRefPtr<CefBrowserHost> CefView::host() const { return m_browser ? m_browser->GetHost() : nullptr; }

// --- The display's scale (mitcad#1657) -----------------------------------------

CefRect CefView::viewRect() {
  const double ratio = devicePixelRatioF();
  if (std::abs(ratio - m_scale) > 1e-9) {
    m_scale = ratio;
    // Not from within CEF's call: the zoom once it has returned.
    QMetaObject::invokeMethod(this, &CefView::applyZoom, Qt::QueuedConnection);
  }
  return CefRect(0, 0, std::max(1, static_cast<int>(std::lround(width() * m_scale))),
                 std::max(1, static_cast<int>(std::lround(height() * m_scale))));
}

void CefView::applyScale() {
  m_scale = devicePixelRatioF();
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->NotifyScreenInfoChanged();
    browserHost->WasResized();
  }
  applyZoom();
}

void CefView::applyZoom() {
  CefRefPtr<CefBrowserHost> browserHost = host();
  if (!browserHost) {
    return;
  }
  // The display's scale as the page's zoom. Chromium keeps a zoom per site
  // (a stored one applies as a page of the site commits) and none other
  // is offered: the display's is set again.
  const double level = zoomLevelOf(m_scale);
  if (std::abs(browserHost->GetZoomLevel() - level) > 1e-6) {
    browserHost->SetZoomLevel(level);
    const QString address = QString::fromStdString(m_browser->GetMainFrame()->GetURL().ToString()).left(60);
    qDebug().noquote() << QStringLiteral("Component browser zoom: %1 for %2")
                              .arg(m_scale, 0, 'f', 2)
                              .arg(address.isEmpty() ? QStringLiteral("the new browser") : address);
  }
}

CefMouseEvent CefView::mouseEventAt(const QPointF& position, Qt::KeyboardModifiers modifiers,
                                    Qt::MouseButtons buttons) const {
  CefMouseEvent event;
  event.x = static_cast<int>(std::lround(position.x() * m_scale));
  event.y = static_cast<int>(std::lround(position.y() * m_scale));
  event.modifiers = modifiersOf(modifiers, buttons);
  return event;
}

QRect CefView::fromCef(const QRect& rect) const {
  return QRect(static_cast<int>(std::lround(rect.x() / m_scale)), static_cast<int>(std::lround(rect.y() / m_scale)),
               static_cast<int>(std::lround(rect.width() / m_scale)),
               static_cast<int>(std::lround(rect.height() / m_scale)));
}

void CefView::closeBrowser() {
  if (m_client) {
    m_client->detach();
  }
  if (m_browser) {
    if (cef::engineRunning()) {
      m_browser->GetHost()->CloseBrowser(true);
    }
    m_browser = nullptr;
  }
  m_client = nullptr;
}

void CefView::load(const QUrl& url) {
  if (!m_browser) {
    m_pending = url;
    return;
  }
  m_browser->GetMainFrame()->LoadURL(url.toString(QUrl::FullyEncoded).toStdString());
}

void CefView::setHtml(const QString& html) {
  load(QUrl(QStringLiteral("data:text/html;charset=utf-8;base64,") +
            QString::fromLatin1(html.toUtf8().toBase64())));
}

void CefView::back() {
  if (m_browser) {
    m_browser->GoBack();
  }
}

void CefView::forward() {
  if (m_browser) {
    m_browser->GoForward();
  }
}

void CefView::reload() {
  if (m_browser) {
    m_browser->Reload();
  }
}

void CefView::runJavaScript(const QString& script) {
  if (m_browser) {
    m_browser->GetMainFrame()->ExecuteJavaScript(script.toStdString(), CefString("mitcad://script"), 0);
  }
}

void CefView::clickAt(const QPoint& point) {
  CefRefPtr<CefBrowserHost> browserHost = host();
  if (!browserHost) {
    return;
  }
  // Real input: a click the page takes as the user's own (a site's
  // download after the first one needs a user's gesture).
  CefMouseEvent event = mouseEventAt(QPointF(point), Qt::NoModifier, Qt::NoButton);
  browserHost->SendMouseMoveEvent(event, false);
  event.modifiers = EVENTFLAG_LEFT_MOUSE_BUTTON;
  browserHost->SendMouseClickEvent(event, MBT_LEFT, false, 1);
  event.modifiers = 0;
  browserHost->SendMouseClickEvent(event, MBT_LEFT, true, 1);
}

void CefView::clearBrowsingData() {
  CefRefPtr<CefRequestContext> context = cef::requestContext();
  if (context) {
    if (CefRefPtr<CefCookieManager> cookies = context->GetCookieManager(nullptr)) {
      cookies->DeleteCookies(CefString(), CefString(), nullptr);
    }
    context->ClearHttpAuthCredentials(nullptr);
  }
  if (m_browser) {
    m_browser->GetHost()->ExecuteDevToolsMethod(0, "Network.clearBrowserCache", nullptr);
  }
}

// --- From the client ----------------------------------------------------------

void CefView::browserCreated(CefRefPtr<CefBrowser> browser) {
  m_creating = false;
  m_browser = browser;
  applyScale();
  host()->WasHidden(!isVisible());
  if (hasFocus()) {
    host()->SetFocus(true);
  }
  if (m_pending.isValid()) {
    const QUrl pending = m_pending;
    m_pending = QUrl();
    load(pending);
  }
}

void CefView::browserClosing() {
  m_browser = nullptr;
}

void CefView::painted(bool popup, const void* buffer, int width, int height) {
  if (buffer == nullptr || width <= 0 || height <= 0) {
    return;
  }
  // CEF's BGRA pixels, premultiplied: Qt's ARGB32 on a little-endian machine.
  QImage image(static_cast<const uchar*>(buffer), width, height, width * 4, QImage::Format_ARGB32_Premultiplied);
  QImage copy = image.copy();
  // The frame's own scale (mitcad#1654), which need not be the view's: CEF
  // is given the view in physical pixels (mitcad#1657), so a frame is of
  // the view's size times the scale CEF last asked with, which can lag
  // behind the window's. A frame of the view's shape is marked with its own
  // scale, so that it fills the view (blurred only while it is of fewer
  // pixels) and the page lies where the clicks land. A frame of another
  // shape (painted before a resize) keeps the view's scale.
  const QSize logical = popup ? m_popupRect.size() : size();
  double ratio = devicePixelRatioF();
  if (logical.width() > 0 && logical.height() > 0) {
    const double across = double(width) / logical.width();
    const double down = double(height) / logical.height();
    // CEF rounds the frame's pixels up: each way's scale is the frame's to
    // within a pixel of the view.
    if (std::abs(across - down) <= 1.0 / std::min(logical.width(), logical.height()) + 1e-9) {
      ratio = across;
    }
  }
  copy.setDevicePixelRatio(ratio);
  if (!popup) {
    const QString frame = QStringLiteral("%1x%2 px at scale %3 for a view of %4x%5 at scale %6")
                              .arg(width)
                              .arg(height)
                              .arg(ratio, 0, 'f', 2)
                              .arg(logical.width())
                              .arg(logical.height())
                              .arg(devicePixelRatioF(), 0, 'f', 2);
    if (frame != m_frameLogged) {
      m_frameLogged = frame;
      qDebug().noquote() << QStringLiteral("Component browser frame: %1").arg(frame);
    }
  }
  if (popup) {
    m_popup = copy;
    update(m_popupRect);
  } else {
    m_frame = copy;
    update();
  }
}

void CefView::popupShown(bool show) {
  m_popupShown = show;
  if (!show) {
    m_popup = QImage();
    update();
  }
}

void CefView::popupSized(const QRect& rect) { m_popupRect = rect; }

void CefView::imeBoundsChanged(const QRect& caret) {
  m_caret = caret;
  updateMicroFocus();
}

void CefView::pageCommitted() { applyZoom(); }

void CefView::addressChanged(const QString& url) {
  // A page committed (one from the back-forward cache too): its site's zoom.
  applyZoom();
  m_url = QUrl(url);
  emit urlChanged(m_url);
}

void CefView::titleChangedTo(const QString& title) {
  m_title = title;
  emit titleChanged(title);
}

void CefView::loadingChanged(bool /*loading*/, bool canGoBack, bool canGoForward) {
  m_canGoBack = canGoBack;
  m_canGoForward = canGoForward;
  emit navigationChanged();
}

void CefView::loadEnded(bool ok) { emit loadFinished(ok); }

void CefView::cursorChanged(int type) { setCursor(QCursor(cursorShape(type))); }

void CefView::logged(const QString& message) {
  if (message.startsWith(QStringLiteral("mitcad:"))) {
    emit consoleMessage(message);
  }
}

QString CefView::downloadStarting(std::uint32_t id, const QString& suggestedName, const QUrl& url) {
  const QString path = beforeDownload ? beforeDownload(suggestedName, url) : QString();
  if (path.isEmpty()) {
    m_refused.insert(id);
  }
  return path;
}

void CefView::downloadChanged(std::uint32_t id, qint64 received, qint64 total, DownloadState state,
                              const QString& path, const QString& reason) {
  // Each state, and the progress in tenths, for the log (where a download
  // stops shows there); CEF calls this on its browser thread, which is Qt's
  // main thread (the external message pump).
  static const char* const kStates[] = {"in progress", "complete", "canceled", "interrupted"};
  const int tenth = total > 0 ? static_cast<int>(received * 10 / total) : (received > 0 ? 1 : 0);
  DownloadLog& logged = m_downloadLog[id];
  if (!logged.seen || logged.state != state || logged.tenth != tenth) {
    logged = DownloadLog{true, state, tenth};
    qDebug().noquote() << QStringLiteral("Component browser download %1: %2, %3 of %4 bytes%5, %6")
                              .arg(id)
                              .arg(QLatin1String(kStates[static_cast<int>(state)]))
                              .arg(received)
                              .arg(total)
                              .arg(reason.isEmpty() ? QString() : QStringLiteral(", ") + reason)
                              .arg(QDir::toNativeSeparators(path));
  }
  if (state != DownloadState::InProgress) {
    m_downloadLog.erase(id);
  }
  emit downloadUpdated(id, received, total, static_cast<int>(state), path, reason);
}

// --- Qt's events ----------------------------------------------------------------

void CefView::screenChanged() {
  qDebug().noquote() << QStringLiteral("Component browser scale: %1").arg(devicePixelRatioF(), 0, 'f', 2);
  applyScale();
}

bool CefView::event(QEvent* event) {
  // Qt sends it when the window's scale changes (another screen, a display
  // setting, Wayland's fractional scale arriving after the window shows):
  // CEF is told, as the API asks, should it paint at the scale.
  if (event->type() == QEvent::DevicePixelRatioChange) {
    screenChanged();
  }
  return QWidget::event(event);
}

void CefView::paintEvent(QPaintEvent* event) {
  QPainter painter(this);
  painter.fillRect(event->rect(), palette().color(QPalette::Base));
  if (!m_frame.isNull()) {
    painter.drawImage(QPoint(0, 0), m_frame);
  }
  if (m_popupShown && !m_popup.isNull()) {
    painter.drawImage(m_popupRect.topLeft(), m_popup);
  }
}

void CefView::resizeEvent(QResizeEvent* event) {
  QWidget::resizeEvent(event);
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->NotifyScreenInfoChanged();
    browserHost->WasResized();
  }
}

void CefView::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  // The window's screen, which the window has once it shows: another
  // screen may have another scale or size.
  QWindow* screenWindow = window()->windowHandle();
  if (screenWindow != nullptr && screenWindow != m_screenWindow) {
    if (m_screenWindow) {
      disconnect(m_screenWindow, &QWindow::screenChanged, this, &CefView::screenChanged);
    }
    m_screenWindow = screenWindow;
    connect(screenWindow, &QWindow::screenChanged, this, &CefView::screenChanged);
  }
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->WasHidden(false);
    applyScale();
  } else {
    createBrowser();
  }
}

void CefView::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->WasHidden(true);
  }
}

void CefView::sendMouse(QMouseEvent* event, bool up, int clicks) {
  CefRefPtr<CefBrowserHost> browserHost = host();
  if (!browserHost) {
    return;
  }
  cef_mouse_button_type_t button = MBT_LEFT;
  if (event->button() == Qt::RightButton) {
    button = MBT_RIGHT;
  } else if (event->button() == Qt::MiddleButton) {
    button = MBT_MIDDLE;
  } else if (event->button() != Qt::LeftButton) {
    return;
  }
  browserHost->SendMouseClickEvent(mouseEventAt(event->position(), event->modifiers(), event->buttons()), button, up,
                                   clicks);
}

void CefView::mousePressEvent(QMouseEvent* event) {
  setFocus(Qt::MouseFocusReason);
  sendMouse(event, false, 1);
}

void CefView::mouseReleaseEvent(QMouseEvent* event) { sendMouse(event, true, 1); }

void CefView::mouseDoubleClickEvent(QMouseEvent* event) { sendMouse(event, false, 2); }

void CefView::mouseMoveEvent(QMouseEvent* event) {
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->SendMouseMoveEvent(mouseEventAt(event->position(), event->modifiers(), event->buttons()), false);
  }
}

void CefView::leaveEvent(QEvent* event) {
  QWidget::leaveEvent(event);
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->SendMouseMoveEvent(mouseEventAt(mapFromGlobal(QCursor::pos()), Qt::NoModifier, Qt::NoButton), true);
  }
}

void CefView::wheelEvent(QWheelEvent* event) {
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    // In CEF's pixels, so that a notch moves the page as far on the screen
    // at any scale.
    const QPoint delta = event->angleDelta();
    browserHost->SendMouseWheelEvent(mouseEventAt(event->position(), event->modifiers(), event->buttons()),
                                     static_cast<int>(std::lround(delta.x() * m_scale)),
                                     static_cast<int>(std::lround(delta.y() * m_scale)));
    event->accept();
  }
}

void CefView::sendKey(QKeyEvent* event, bool down) {
  CefRefPtr<CefBrowserHost> browserHost = host();
  if (!browserHost) {
    return;
  }
  CefKeyEvent key;
  key.modifiers = modifiersOf(event->modifiers());
  int code = windowsKeyCode(event->key());
#if defined(_WIN32)
  if (code == 0) {
    code = static_cast<int>(event->nativeVirtualKey());
  }
  key.native_key_code = static_cast<int>((event->nativeScanCode() << 16) | 1);
#elif defined(__APPLE__)
  key.native_key_code = static_cast<int>(event->nativeVirtualKey());
#else
  key.native_key_code = static_cast<int>(event->nativeScanCode());
#endif
  key.windows_key_code = code;
  key.is_system_key = event->modifiers().testFlag(Qt::AltModifier) ? 1 : 0;
  if (!down) {
    key.type = KEYEVENT_KEYUP;
    browserHost->SendKeyEvent(key);
    return;
  }
  key.type = KEYEVENT_RAWKEYDOWN;
  browserHost->SendKeyEvent(key);
  // The characters typed (not with Control or Command, which are commands).
  const QString text = event->text();
  const bool command = event->modifiers().testFlag(Qt::ControlModifier);
  if (!command) {
    for (const QChar c : text) {
      if (c.unicode() < 0x20 && c != QLatin1Char('\r') && c != QLatin1Char('\t')) {
        continue;
      }
      CefKeyEvent character = key;
      character.type = KEYEVENT_CHAR;
      character.windows_key_code = c.unicode();
      character.character = c.unicode();
      character.unmodified_character = c.unicode();
      browserHost->SendKeyEvent(character);
    }
  }
}

void CefView::keyPressEvent(QKeyEvent* event) {
  // The clipboard through Qt's, which is the same on every platform
  // (Wayland too): Paste types the clipboard's text, the others are the
  // page's own commands.
  if (m_browser) {
    if (event->matches(QKeySequence::Paste)) {
      const QString text = QGuiApplication::clipboard()->text();
      if (!text.isEmpty()) {
        host()->ImeCommitText(text.toStdString(), noRange(), 0);
      }
      event->accept();
      return;
    }
    if (event->matches(QKeySequence::Copy)) {
      m_browser->GetFocusedFrame()->Copy();
      event->accept();
      return;
    }
    if (event->matches(QKeySequence::Cut)) {
      m_browser->GetFocusedFrame()->Cut();
      event->accept();
      return;
    }
    if (event->matches(QKeySequence::SelectAll)) {
      m_browser->GetFocusedFrame()->SelectAll();
      event->accept();
      return;
    }
    if (event->matches(QKeySequence::Undo)) {
      m_browser->GetFocusedFrame()->Undo();
      event->accept();
      return;
    }
    if (event->matches(QKeySequence::Redo)) {
      m_browser->GetFocusedFrame()->Redo();
      event->accept();
      return;
    }
  }
  sendKey(event, true);
  event->accept();
}

void CefView::keyReleaseEvent(QKeyEvent* event) {
  sendKey(event, false);
  event->accept();
}

void CefView::focusInEvent(QFocusEvent* event) {
  QWidget::focusInEvent(event);
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->SetFocus(true);
  }
}

void CefView::focusOutEvent(QFocusEvent* event) {
  QWidget::focusOutEvent(event);
  if (CefRefPtr<CefBrowserHost> browserHost = host()) {
    browserHost->SetFocus(false);
  }
}

bool CefView::focusNextPrevChild(bool) {
  // Tab moves through the page's fields, not out of the view.
  return false;
}

void CefView::inputMethodEvent(QInputMethodEvent* event) {
  CefRefPtr<CefBrowserHost> browserHost = host();
  if (!browserHost) {
    return;
  }
  if (!event->commitString().isEmpty()) {
    browserHost->ImeCommitText(event->commitString().toStdString(), noRange(), 0);
  }
  const QString preedit = event->preeditString();
  if (!preedit.isEmpty()) {
    std::vector<CefCompositionUnderline> underlines;
    CefCompositionUnderline underline;
    underline.range = CefRange(0, static_cast<std::uint32_t>(preedit.size()));
    underline.color = CefColorSetARGB(255, 0, 0, 0);
    underline.background_color = CefColorSetARGB(0, 0, 0, 0);
    underline.thick = 0;
    underlines.push_back(underline);
    int cursor = static_cast<int>(preedit.size());
    for (const QInputMethodEvent::Attribute& attribute : event->attributes()) {
      if (attribute.type == QInputMethodEvent::Cursor) {
        cursor = attribute.start;
      }
    }
    const auto at = static_cast<std::uint32_t>(cursor);
    browserHost->ImeSetComposition(preedit.toStdString(), underlines, noRange(), CefRange(at, at));
  } else if (event->commitString().isEmpty()) {
    browserHost->ImeCancelComposition();
  }
  event->accept();
}

QVariant CefView::inputMethodQuery(Qt::InputMethodQuery query) const {
  if (query == Qt::ImCursorRectangle || query == Qt::ImAnchorRectangle) {
    return m_caret.isValid() ? m_caret : QRect(0, 0, 1, height());
  }
  if (query == Qt::ImEnabled) {
    return true;
  }
  return QWidget::inputMethodQuery(query);
}

} // namespace mitcad
