// SPDX-License-Identifier: MIT
#include "framework/TypedTextLog.hpp"

#include "platform/WindowsTestSync.hpp"

#include <QAbstractNativeEventFilter>
#include <QAction>
#include <QApplication>
#include <QByteArray>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLineEdit>
#include <QShortcut>
#include <QShortcutEvent>
#include <QSpinBox>
#include <QStringList>
#include <QWidget>
#include <QWindow>
#include <QWindowStateChangeEvent>
#include <QtLogging>

#include <functional>
#include <utility>

namespace mitcad {
namespace {

constexpr const char* kWatched = "mitcadTypedTextLogged";

// The text field a widget that took the keyboard edits: itself, or the line
// edit of a spin box or an editable combo box.
QLineEdit* textFieldOf(QWidget* widget) {
  if (auto* edit = qobject_cast<QLineEdit*>(widget)) {
    return edit;
  }
  return widget->findChild<QLineEdit*>(QString(), Qt::FindDirectChildrenOnly);
}

// The id of the command panel's input a text field belongs to (its row is
// named "input_<id>", the panel "commandPanel"; framework/CommandPanel.cpp),
// empty if none.
QString panelInputOf(const QWidget* edit) {
  const QString prefix = QStringLiteral("input_");
  for (const QWidget* widget = edit; widget != nullptr; widget = widget->parentWidget()) {
    if (!widget->objectName().startsWith(prefix)) {
      continue;
    }
    for (const QWidget* panel = widget->parentWidget(); panel != nullptr; panel = panel->parentWidget()) {
      if (panel->objectName() == QLatin1String("commandPanel")) {
        return widget->objectName().mid(prefix.size());
      }
    }
    return {};
  }
  return {};
}

// Logs the text of the field a widget that took the keyboard edits, each
// time the user edits it, after the prefix (asked for each time: a dialog
// may change its title). A spin box's text is its number without its
// prefix and suffix; its line edit tells of typing only as textChanged.
void logEdits(QWidget* focused, QLineEdit* edit, std::function<QString()> prefix) {
  const auto log = [focused, edit, prefix = std::move(prefix)] {
    if (edit->echoMode() != QLineEdit::Normal) {
      return;
    }
    QString text = edit->text();
    if (const auto* decimal = qobject_cast<const QDoubleSpinBox*>(focused)) {
      text = decimal->cleanText();
    } else if (const auto* whole = qobject_cast<const QSpinBox*>(focused)) {
      text = whole->cleanText();
    }
    qInfo().noquote() << prefix() + text;
  };
  if (focused == edit) {
    QObject::connect(edit, &QLineEdit::textEdited, edit, log);
  } else {
    QObject::connect(edit, &QLineEdit::textChanged, edit, log);
  }
}

void watchFileDialog(QFileDialog* dialog, QLineEdit* edit) {
  QObject::connect(edit, &QLineEdit::textChanged, edit, [dialog](const QString& text) {
    qInfo().noquote() << QStringLiteral("File dialog %1: %2").arg(dialog->windowTitle(), text);
  });
  if (!dialog->property(kWatched).toBool()) {
    dialog->setProperty(kWatched, true);
    QObject::connect(dialog, &QDialog::finished, dialog, [dialog](int result) {
      qInfo().noquote() << QStringLiteral("File dialog %1: %2")
                               .arg(dialog->windowTitle(), result == QDialog::Accepted ? QStringLiteral("accepted")
                                                                                       : QStringLiteral("rejected"));
    });
  }
}

void focusChanged(QWidget* now) {
  if (now == nullptr) {
    return;
  }
  QLineEdit* edit = textFieldOf(now);
  if (edit == nullptr || edit->property(kWatched).toBool()) {
    return;
  }
  QWidget* window = edit->window();
  if (auto* files = qobject_cast<QFileDialog*>(window)) {
    // Qt's own file dialog names its file name field so.
    if (edit->objectName() == QLatin1String("fileNameEdit")) {
      edit->setProperty(kWatched, true);
      watchFileDialog(files, edit);
    }
    return;
  }
  if (qobject_cast<QDialog*>(window) != nullptr) {
    edit->setProperty(kWatched, true);
    logEdits(now, edit, [window] { return QStringLiteral("Dialog field %1: ").arg(window->windowTitle()); });
    return;
  }
  const QString input = panelInputOf(edit);
  if (!input.isEmpty()) {
    edit->setProperty(kWatched, true);
    logEdits(now, edit, [input] { return QStringLiteral("Panel field %1: ").arg(input); });
  }
}

// Logs each key press once: a shortcut's ShortcutOverride and the key press
// that follows when no shortcut takes the key reach the event filter for
// every widget they pass, so the same key (its time, codes and modifiers)
// is logged only the first time.
class KeyLog : public QObject {
public:
  explicit KeyLog(QObject* parent) : QObject(parent) {}

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::ShortcutOverride || event->type() == QEvent::KeyPress) {
      log(*static_cast<const QKeyEvent*>(event));
    }
    return QObject::eventFilter(watched, event);
  }

private:
  struct Press {
    quint64 time = 0;
    int key = 0;
    quint32 native = 0;
    quint32 scanCode = 0;
    int modifiers = 0;
    bool operator==(const Press& other) const {
      return time == other.time && key == other.key && native == other.native && scanCode == other.scanCode &&
             modifiers == other.modifiers;
    }
  };

  void log(const QKeyEvent& event) {
    if (event.isAutoRepeat()) {
      return;
    }
    const Qt::KeyboardModifiers modifiers = event.modifiers();
    const Press press{event.timestamp(), event.key(), event.nativeVirtualKey(), event.nativeScanCode(),
                      static_cast<int>(modifiers.toInt())};
    if (press == m_last) {
      return;
    }
    m_last = press;
    QStringList held;
    const std::pair<Qt::KeyboardModifier, const char*> names[] = {{Qt::ControlModifier, "ctrl"},
                                                                  {Qt::AltModifier, "alt"},
                                                                  {Qt::ShiftModifier, "shift"},
                                                                  {Qt::MetaModifier, "meta"}};
    for (const auto& [modifier, name] : names) {
      if (modifiers.testFlag(modifier)) {
        held << QString::fromLatin1(name);
      }
    }
    // A character the window got without a key (WM_CHAR) has no key code:
    // its text instead.
    const QString key = event.key() != 0 ? QKeySequence(event.key()).toString(QKeySequence::PortableText)
                                         : QStringLiteral("'%1'").arg(event.text());
    qInfo().noquote() << QStringLiteral("Key press: %1%2 (native %3)")
                             .arg(key.isEmpty() ? QString::number(event.key()) : key,
                                  held.isEmpty() ? QString() : QStringLiteral(" with ") + held.join(QLatin1Char('+')))
                             .arg(event.nativeVirtualKey());
  }

  Press m_last;
};

// The shortcut diagnosis's names of windows and widgets: the class, the
// object name and a window's title.
QString windowName(const QWindow* window) {
  if (window == nullptr) {
    return QStringLiteral("none");
  }
  QString name = window->objectName().isEmpty() ? QString::fromLatin1(window->metaObject()->className())
                                                 : window->objectName();
  if (!window->title().isEmpty()) {
    name += QStringLiteral(" '%1'").arg(window->title());
  }
  return name;
}

QString widgetName(const QWidget* widget) {
  if (widget == nullptr) {
    return QStringLiteral("none");
  }
  QString name = QString::fromLatin1(widget->metaObject()->className());
  if (!widget->objectName().isEmpty()) {
    name += QLatin1Char(' ') + widget->objectName();
  }
  if (widget->isWindow() && !widget->windowTitle().isEmpty()) {
    name += QStringLiteral(" '%1'").arg(widget->windowTitle());
  }
  return name;
}

// A native window by the Qt window it is, if it is one of the application's.
QString nativeName(quintptr handle) {
  if (handle == 0) {
    return QStringLiteral("none");
  }
  const QWindowList windows = QGuiApplication::allWindows();
  for (const QWindow* window : windows) {
    // winId() would create a native window for one that has none yet.
    if (window->handle() != nullptr && window->winId() == handle) {
      return windowName(window);
    }
  }
  return QStringLiteral("0x%1 (not the application's)").arg(static_cast<qulonglong>(handle), 0, 16);
}

QString stateNames(Qt::WindowStates states) {
  QStringList names;
  if (states.testFlag(Qt::WindowMinimized)) names << QStringLiteral("minimized");
  if (states.testFlag(Qt::WindowMaximized)) names << QStringLiteral("maximized");
  if (states.testFlag(Qt::WindowFullScreen)) names << QStringLiteral("full screen");
  if (states.testFlag(Qt::WindowActive)) names << QStringLiteral("active");
  return names.isEmpty() ? QStringLiteral("normal") : names.join(QLatin1Char('+'));
}

QString contextName(Qt::ShortcutContext context) {
  switch (context) {
  case Qt::WidgetShortcut:
    return QStringLiteral("widget");
  case Qt::WidgetWithChildrenShortcut:
    return QStringLiteral("widget with children");
  case Qt::WindowShortcut:
    return QStringLiteral("window");
  case Qt::ApplicationShortcut:
    return QStringLiteral("application");
  }
  return QString::number(static_cast<int>(context));
}

QString applicationStateName(Qt::ApplicationState state) {
  switch (state) {
  case Qt::ApplicationSuspended:
    return QStringLiteral("suspended");
  case Qt::ApplicationHidden:
    return QStringLiteral("hidden");
  case Qt::ApplicationInactive:
    return QStringLiteral("inactive");
  case Qt::ApplicationActive:
    return QStringLiteral("active");
  }
  return QString::number(static_cast<int>(state));
}

// The shortcut diagnosis (MITCAD_LOG_SHORTCUTS=1, mitcad#1016): for each
// key press, what decides whether a shortcut takes it (Qt matches a window
// shortcut only while its window is the active window), the actions and
// shortcuts that have the key, and what came of it; the windows'
// activation and state changes, Qt's and (on Windows) the native messages.
class ShortcutLog : public QObject, public QAbstractNativeEventFilter {
public:
  explicit ShortcutLog(QObject* parent) : QObject(parent) {}

  bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr*) override {
    if (eventType != "windows_generic_MSG") {
      return false;
    }
    QString what;
    quintptr window = 0;
    quintptr other = 0;
    if (win::activationMessage(message, &what, &window, &other)) {
      qInfo().noquote() << QStringLiteral("Native %1: %2%3")
                               .arg(what, nativeName(window),
                                    other != 0 ? QStringLiteral(" (other %1)").arg(nativeName(other)) : QString());
    }
    return false;
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    switch (event->type()) {
    case QEvent::ShortcutOverride:
      checked(watched, *static_cast<const QKeyEvent*>(event));
      break;
    case QEvent::KeyPress:
      pressed(watched, *static_cast<const QKeyEvent*>(event));
      break;
    case QEvent::Shortcut:
      shortcut(watched, *static_cast<const QShortcutEvent*>(event));
      break;
    case QEvent::WindowActivate:
    case QEvent::WindowDeactivate:
    case QEvent::WindowStateChange:
      windowChanged(watched, event);
      break;
    default:
      break;
    }
    return QObject::eventFilter(watched, event);
  }

private:
  struct Press {
    quint64 time = 0;
    int key = 0;
    quint32 native = 0;
    int modifiers = 0;
    bool operator==(const Press& other) const {
      return time == other.time && key == other.key && native == other.native && modifiers == other.modifiers;
    }
  };

  static Press pressOf(const QKeyEvent& event) {
    return {event.timestamp(), event.key(), event.nativeVirtualKey(), static_cast<int>(event.modifiers().toInt())};
  }

  static QString keyName(const QKeyEvent& event) {
    return QKeySequence(event.keyCombination()).toString(QKeySequence::PortableText);
  }

  // The key before Qt tries the shortcuts (its ShortcutOverride, which
  // goes to the focus object and its parents: logged once).
  void checked(QObject* receiver, const QKeyEvent& event) {
    if (event.isAutoRepeat() || event.key() == 0 || event.key() == Qt::Key_Pause) {
      return; // the sync key (Pause) is no shortcut
    }
    const Press press = pressOf(event);
    if (press == m_pending) {
      return;
    }
    m_pending = press;
    m_taken = false;
    m_reported = false;
    const QString key = keyName(event);
    const QObject* to = receiver;
    QString state = QStringLiteral("Shortcut check %1 to %2: active window %3, focus window %4%5, focus widget %6, "
                                   "popup %7, modal %8, application %9")
                        .arg(key, qobject_cast<const QWidget*>(to) != nullptr
                                      ? widgetName(static_cast<const QWidget*>(to))
                                      : QString::fromLatin1(to->metaObject()->className()))
                        .arg(widgetName(QApplication::activeWindow()), windowName(QGuiApplication::focusWindow()),
                             QGuiApplication::focusWindow() != nullptr && QGuiApplication::focusWindow()->isActive()
                                 ? QStringLiteral(" (active)")
                                 : QString(),
                             widgetName(QApplication::focusWidget()), widgetName(QApplication::activePopupWidget()),
                             widgetName(QApplication::activeModalWidget()),
                             applicationStateName(QGuiApplication::applicationState()));
    quintptr active = 0;
    quintptr focus = 0;
    quintptr foreground = 0;
    if (win::nativeActivation(&active, &focus, &foreground)) {
      state += QStringLiteral("; native active %1, focus %2, foreground %3")
                   .arg(nativeName(active), nativeName(focus), nativeName(foreground));
    }
    qInfo().noquote() << state;
    logBindings(QKeySequence(event.keyCombination()), key);
  }

  // The actions and shortcuts of every window that have the key.
  static void logBindings(const QKeySequence& sequence, const QString& key) {
    int found = 0;
    const QWidgetList windows = QApplication::topLevelWidgets();
    for (const QWidget* window : windows) {
      for (const QAction* action : window->findChildren<QAction*>()) {
        if (!action->shortcuts().contains(sequence)) {
          continue;
        }
        ++found;
        const auto* owner = qobject_cast<const QWidget*>(action->parent());
        const QWidget* ownerWindow = owner != nullptr ? owner->window() : nullptr;
        qInfo().noquote() << QStringLiteral("Shortcut %1: action %2, %3, %4, %5 context, on %6%7")
                                 .arg(key, action->objectName().isEmpty() ? action->text() : action->objectName(),
                                      action->isEnabled() ? QStringLiteral("enabled") : QStringLiteral("disabled"),
                                      action->isVisible() ? QStringLiteral("visible") : QStringLiteral("hidden"),
                                      contextName(action->shortcutContext()), widgetName(ownerWindow),
                                      ownerWindow != nullptr && ownerWindow == QApplication::activeWindow()
                                          ? QStringLiteral(" (the active window)")
                                          : QStringLiteral(" (not the active window)"));
      }
      for (const QShortcut* bound : window->findChildren<QShortcut*>()) {
        if (!bound->keys().contains(sequence)) {
          continue;
        }
        ++found;
        const auto* owner = qobject_cast<const QWidget*>(bound->parent());
        qInfo().noquote() << QStringLiteral("Shortcut %1: shortcut %2, %3, %4 context, on %5")
                                 .arg(key, bound->objectName(),
                                      bound->isEnabled() ? QStringLiteral("enabled") : QStringLiteral("disabled"),
                                      contextName(bound->context()), widgetName(owner));
      }
    }
    if (found == 0) {
      qInfo().noquote() << QStringLiteral("Shortcut %1: no action or shortcut has the key").arg(key);
    }
  }

  // A shortcut took the key.
  void shortcut(QObject* receiver, const QShortcutEvent& event) {
    m_taken = true;
    const auto* action = qobject_cast<const QAction*>(receiver);
    const QString name = receiver->objectName().isEmpty() && action != nullptr ? action->text()
                                                                                 : receiver->objectName();
    qInfo().noquote() << QStringLiteral("Shortcut event %1 to %2 %3%4")
                             .arg(event.key().toString(QKeySequence::PortableText),
                                  QString::fromLatin1(receiver->metaObject()->className()), name,
                                  event.isAmbiguous() ? QStringLiteral(" (ambiguous)") : QString());
  }

  // No shortcut took the key (or the focus widget kept it): it goes on as
  // a key press.
  void pressed(QObject* receiver, const QKeyEvent& event) {
    if (m_taken || m_reported || !(pressOf(event) == m_pending)) {
      return;
    }
    m_reported = true;
    const auto* widget = qobject_cast<const QWidget*>(receiver);
    qInfo().noquote() << QStringLiteral("Shortcut none for %1: a key press to %2")
                             .arg(keyName(event), widget != nullptr
                                                      ? widgetName(widget)
                                                      : QString::fromLatin1(receiver->metaObject()->className()));
  }

  static void windowChanged(QObject* watched, const QEvent* event) {
    const auto* widget = qobject_cast<const QWidget*>(watched);
    if (widget == nullptr || !widget->isWindow()) {
      return;
    }
    if (event->type() == QEvent::WindowStateChange) {
      qInfo().noquote() << QStringLiteral("Window state %1: %2 (was %3)")
                               .arg(widgetName(widget), stateNames(widget->windowState()),
                                    stateNames(static_cast<const QWindowStateChangeEvent*>(event)->oldState()));
    } else {
      qInfo().noquote() << QStringLiteral("Window %1: %2")
                               .arg(event->type() == QEvent::WindowActivate ? QStringLiteral("activated")
                                                                            : QStringLiteral("deactivated"),
                                    widgetName(widget));
    }
  }

  Press m_pending;
  bool m_taken = false;
  bool m_reported = false;
};

} // namespace

void installKeyLogIfRequested() {
  if (qEnvironmentVariableIntValue("MITCAD_LOG_KEYS") == 0 || qApp == nullptr) {
    return;
  }
  qApp->installEventFilter(new KeyLog(qApp));
  qInfo().noquote() << QStringLiteral("Key log on");
}

void installShortcutLogIfRequested() {
  if (qEnvironmentVariableIntValue("MITCAD_LOG_SHORTCUTS") == 0 || qApp == nullptr) {
    return;
  }
  auto* log = new ShortcutLog(qApp);
  qApp->installEventFilter(log);
  qApp->installNativeEventFilter(log);
  QObject::connect(qApp, &QGuiApplication::focusWindowChanged, log, [](QWindow* window) {
    qInfo().noquote() << QStringLiteral("Focus window: %1").arg(windowName(window));
  });
  QObject::connect(qApp, &QGuiApplication::applicationStateChanged, log, [](Qt::ApplicationState state) {
    qInfo().noquote() << QStringLiteral("Application state: %1").arg(applicationStateName(state));
  });
  qInfo().noquote() << QStringLiteral("Shortcut log on");
}

void installTypedTextLogIfRequested() {
  if (qEnvironmentVariableIntValue("MITCAD_LOG_TYPED_TEXT") == 0 || qApp == nullptr) {
    return;
  }
  QObject::connect(qApp, &QApplication::focusChanged, qApp, [](QWidget*, QWidget* now) { focusChanged(now); });
}

} // namespace mitcad
