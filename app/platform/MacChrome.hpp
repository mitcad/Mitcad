// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <QIcon>
#include <QString>
#include <QtGlobal>

class QMenu;
class QRectF;
class QWidget;

// The macOS-only parts of the interface that need AppKit. Portable code calls
// these functions without #ifdefs: on other platforms they are inline stubs
// that do nothing. Implementations are in platform/macos/ (MacChrome.mm
// for the interface, MacWork.mm for scheduling and process activities).
namespace mitcad::mac {

enum class ThreadQuality { UserInitiated, Utility };

// Thread-local scheduling and process activity for work the user waits for.
// Activities own their native token and always end it, including on exceptions.
class ProcessActivity {
public:
  ProcessActivity() = default;
  explicit ProcessActivity(const char* reason) { start(reason); }
  ~ProcessActivity() { stop(); }
  ProcessActivity(const ProcessActivity&) = delete;
  ProcessActivity& operator=(const ProcessActivity&) = delete;
  void start(const char* reason);
  void stop();

private:
#ifdef Q_OS_MACOS
  void* m_token = nullptr;
#endif
};

// A shared pool's utility job must restore the thread's previous quality.
class ThreadQualityScope {
public:
  explicit ThreadQualityScope(ThreadQuality quality);
  ~ThreadQualityScope();
  ThreadQualityScope(const ThreadQualityScope&) = delete;
  ThreadQualityScope& operator=(const ThreadQualityScope&) = delete;

private:
#ifdef Q_OS_MACOS
  unsigned m_previous = 0;
  int m_priority = 0;
  bool m_changed = false;
#endif
};

#ifdef Q_OS_MACOS

void setThreadQuality(ThreadQuality quality);
// Before model/section workers start: sets each default OCCT pool thread's
// quality explicitly. Does not change the caller's quality or the pool size.
void initializeComputeThreads();

// An SF Symbol (e.g. "arrow.uturn.backward") as an icon: a template image
// drawn in the application palette's ButtonText colour (the palette's
// disabled text colour in the Disabled mode, HighlightedText in Selected),
// rendered when it is asked for at the size and device pixel ratio it is
// asked for, so that it is sharp on Retina screens and follows palette
// changes (dark mode, accent). `fallback` when the system has no such symbol.
QIcon symbolIcon(const QString& symbol, const QIcon& fallback = {});

// Makes the images of a menu's items (and its submenus') visible: since
// macOS 27 AppKit hides them unless an item asks for them. Best effort: Qt
// creates the native items when the menu is shown or changed, so the menu is
// set up again whenever it is about to show. A no-op before macOS 27.
void showMenuImages(QMenu* menu);

// Hides the native window title text and makes the title bar transparent, for
// a window whose client area extends under the title bar and draws its own
// title (the traffic lights stay). The window must have a native handle.
void hideNativeTitle(QWidget* window);
// The Qt frontend uses the same system document tabs as AppKit. The callback
// follows actual WindowServer occlusion, including a hidden native tab.
void enableDocumentWindowTabs(QWidget* window, std::function<void(bool)> visibility);
// Read-only membership of the actual NSWindow tab group (one when ungrouped).
int documentWindowTabCount(QWidget* window);
// Actual selected tab of the presenter's owned NSWindow group, independent
// of application foreground/key status. An unshared window is selected.
bool documentWindowTabSelected(QWidget* window);
void addDocumentWindowTabActions(QMenu* menu, QWidget* window);
// Programmatic UI-script target-action input through the presenter's actual
// Window menu. Does not require or assert application foreground/key status.
bool triggerDocumentWindowMenu(QWidget* presenter, const QString& entry, QString& error);
void prepareDocumentWindowPresentation(QWidget* window, bool separate);
void minimizeDocumentWindow(QWidget* window);

// Conventions of the menu bar and the standard About panel.

// Hides the images of a menu's items (and its submenus'): the menus of the
// menu bar show none (the Human Interface Guidelines; macOS 27 hides them
// itself) although the actions have icons for the ribbon, whose menus keep
// them. Best effort like showMenuImages: set up again whenever a menu is
// about to show.
void hideMenuImages(QMenu* menu);

// The standard About panel (name, icon and version of the application, the
// copyright of its bundle) with `credits`, rich text as HTML.
void showAboutPanel(const QString& credits);

// Liquid Glass behind a card (macOS 26+). Qt draws the 3D view with OpenGL,
// which a native view in the same window cannot show through, so a card that
// wants real glass is a window of its own: `card` is a frameless, translucent
// Qt top-level window (GlassCard::setWindowed), which is made a child window
// of `mainWindow` here, with an NSGlassEffectView of `cornerRadius` below Qt's
// view in it. The window is without a shadow of Qt's own, and gets the
// system's; it does not take part in the Window menu, window cycling or
// Mission Control. Safe to call again for the same card (as it is shown
// again); both windows must have native handles.
bool glassAvailable();
void attachGlassWindow(QWidget* card, QWidget* mainWindow, qreal cornerRadius);
// The radius of the card's glass (a capsule's follows its height).
void setGlassCornerRadius(QWidget* card, qreal cornerRadius);

// Saves what is on the screen of the application's own windows, as the window
// server composes them (child windows and glass included; the Qt widgets'
// grab() has neither), as a PNG. No screen recording permission is needed for
// a process's own windows. Optional bounds are logical screen coordinates,
// with the window server's top-left origin, including the captured shadows.
// False if it failed.
bool captureOwnWindows(const QString& path, QRectF* capturedBounds = nullptr);

// In-process UI driver only (MITCAD_TEST_INPUT): Qt can show a combo with
// an AppKit menu even in the ordinary Qt frontend. Select its actual
// enabled item after tracking starts. False with no error means that Qt
// displayed its own item view, which the driver clicks instead.
bool chooseTestPopup(const QString& title, const std::function<void()>& open,
                     const std::function<bool()>& qtPopupVisible, QString& error,
                     const std::function<void()>& nativeClosed = {});

#else

inline void setThreadQuality(ThreadQuality) {}
inline void initializeComputeThreads() {}
inline void ProcessActivity::start(const char*) {}
inline void ProcessActivity::stop() {}
inline ThreadQualityScope::ThreadQualityScope(ThreadQuality) {}
inline ThreadQualityScope::~ThreadQualityScope() = default;

inline bool glassAvailable() { return false; }
inline void attachGlassWindow(QWidget*, QWidget*, qreal) {}
inline void setGlassCornerRadius(QWidget*, qreal) {}
inline bool captureOwnWindows(const QString&, QRectF* = nullptr) { return false; }
inline bool chooseTestPopup(const QString&, const std::function<void()>& open,
                            const std::function<bool()>&, QString&, const std::function<void()>& = {}) {
  open();
  return false;
}
inline QIcon symbolIcon(const QString&, const QIcon& fallback = {}) { return fallback; }
inline void showMenuImages(QMenu*) {}
inline void hideNativeTitle(QWidget*) {}
inline void enableDocumentWindowTabs(QWidget*, std::function<void(bool)>) {}
inline int documentWindowTabCount(QWidget*) { return 0; }
inline bool documentWindowTabSelected(QWidget*) { return false; }
inline void addDocumentWindowTabActions(QMenu*, QWidget*) {}
inline bool triggerDocumentWindowMenu(QWidget*, const QString&, QString&) { return false; }
inline void prepareDocumentWindowPresentation(QWidget*, bool) {}
inline void minimizeDocumentWindow(QWidget*) {}
inline void hideMenuImages(QMenu*) {}
inline void showAboutPanel(const QString&) {}

#endif

} // namespace mitcad::mac
