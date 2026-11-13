// SPDX-License-Identifier: MIT
// Live MCP's input tools (mitcad#1735): ui_click and ui_type post mouse and
// key events to the window as Qt events, as the user's hand gives them, for
// screenshots of the real UI (a manual's pictures); ui_dialog opens a
// window that ui_command refuses (Edit Parameters, Send to Slicer's panel)
// for viewing, lists its fields and buttons, and closes it. Only in a run
// started with --mcp-folder (the live server lists them only then, and
// never to the chat's own connection); never in the headless server.
//
// The events are posted, not sent: they run right after the call has
// answered, so that a dialog a click opens runs its own event loop while
// the connection goes on (ui_dialog then reads and closes it), instead of
// holding the call until someone closes it. ui_dialog's action runs the
// same way, right after its answer.
#include "MainWindow.hpp"

#include <functional>
#include <optional>
#include <stdexcept>
#include <vector>

#include <QAbstractButton>
#include <QApplication>
#include <QCoreApplication>
#include <QDebug>
#include <QDialog>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QPointingDevice>
#include <QTimer>
#include <QWidget>
#include <QtLogging>

#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
#include "framework/GlassCard.hpp"
#include "mcp/AgentFolders.hpp"
#include "mcp/DialogWatch.hpp"
#include "mcp/EndpointLimits.hpp"
#include "mcp/LiveErrors.hpp"
#include "mcp/ModalGuard.hpp"
#include "mcp/UiCommandList.hpp"
#include "mcp/UiKeys.hpp"

namespace mitcad {
namespace {

[[noreturn]] void refuse(const QString& reason) {
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw std::runtime_error(reason.toStdString());
}

[[noreturn]] void refuseWith(const QString& reason, const QJsonObject& fields) {
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw mcp::Refusal(reason, fields);
}

// Runs the event loop until `done` holds or the time is up.
bool waitFor(const std::function<bool()>& done, int milliseconds) {
  QElapsedTimer clock;
  clock.start();
  while (!done()) {
    if (clock.elapsed() > milliseconds) {
      return false;
    }
    QEventLoop loop;
    QTimer::singleShot(10, &loop, &QEventLoop::quit);
    loop.exec();
  }
  return true;
}

// What a click or the keys reach, for the agent: its class, its object
// name, a button's or a field's text and its tooltip.
QJsonObject widgetJson(const QWidget* widget) {
  QJsonObject entry{{QStringLiteral("class"), QString::fromLatin1(widget->metaObject()->className())},
                    {QStringLiteral("name"), widget->objectName()}};
  if (const auto* button = qobject_cast<const QAbstractButton*>(widget)) {
    entry.insert(QStringLiteral("text"), mcp::buttonText(button));
  } else if (const auto* line = qobject_cast<const QLineEdit*>(widget)) {
    entry.insert(QStringLiteral("text"), line->text());
    entry.insert(QStringLiteral("field"), true);
  } else if (const auto* label = qobject_cast<const QLabel*>(widget)) {
    entry.insert(QStringLiteral("text"), label->text());
  }
  if (!widget->toolTip().isEmpty()) {
    entry.insert(QStringLiteral("tooltip"), widget->toolTip());
  }
  return entry;
}

QString titleOf(const QWidget* window) {
  return window->windowTitle().isEmpty() ? QString::fromLatin1(window->metaObject()->className())
                                         : window->windowTitle();
}

// Activates a window as a click on it does: a window the system did not
// activate (a session without a user at the console) has no focused widget
// for Qt. Qt's own activation, as the test driver does it.
void activate(QWidget* window) {
  if (QApplication::activePopupWidget() != nullptr || QApplication::activeWindow() == window) {
    return;
  }
  QT_WARNING_PUSH
  QT_WARNING_DISABLE_DEPRECATED
  QApplication::setActiveWindow(window);
  QT_WARNING_POP
}

// What a press gives the keyboard to, as Qt does for the system's clicks
// (QApplication's focus by the focus policy): the first enabled widget from
// the one clicked up to its window that takes the focus on a click.
void giveClickFocus(QWidget* target) {
  for (QWidget* widget = target; widget != nullptr; widget = widget->parentWidget()) {
    if (widget->isEnabled() && (widget->focusPolicy() & Qt::ClickFocus) == Qt::ClickFocus) {
      widget->setFocus(Qt::MouseFocusReason);
      return;
    }
    if (widget->isWindow()) {
      return;
    }
  }
}

} // namespace

QWidget* MainWindow::mcpShownDialog() const {
  if (const auto* watch = static_cast<const mcp::DialogWatch*>(m_mcpDialogWatch.data())) {
    if (QWidget* opened = watch->window()) {
      return opened;
    }
  }
  QWidget* modal = QApplication::activeModalWidget();
  return modal != nullptr && mcp::isDialogWindow(modal) ? modal : nullptr;
}

void MainWindow::mcpRequireInput(bool whileDialog) const {
  if (!mcp::runAgentFoldersGiven()) {
    refuse(QStringLiteral("ui_click, ui_type and ui_dialog work only in an application started with --mcp-folder."));
  }
  if (m_nativeUi) {
    refuse(QStringLiteral("ui_click, ui_type and ui_dialog work only in the Qt window, not in the AppKit one."));
  }
  if (whileDialog) {
    return;
  }
  if (m_mcpDialogQueued) {
    refuseWith(QStringLiteral("busy: the window ui_dialog opens is opening; retry in 100 ms"),
               {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 100}});
  }
  if (const QWidget* dialog = mcpShownDialog()) {
    refuse(QStringLiteral("The window %1 is open, for viewing only: close it with ui_dialog press first.")
               .arg(titleOf(dialog)));
  }
  if (m_session && mcp::opensDialog(m_session->def().id)) {
    refuse(QStringLiteral("The panel of %1 is open, for viewing only: close it with ui_dialog press Cancel first.")
               .arg(m_session->def().name));
  }
}

QJsonObject MainWindow::mcpClick(const QJsonObject& arguments) {
  mcpRequireInput(false);
  QWidget* host = window();
  QPoint at;
  const QString input = arguments.value(QStringLiteral("input")).toString();
  if (!input.isEmpty()) {
    if (!m_session) {
      refuse(QStringLiteral("No command panel is open, so no input has a place (ui_command with finish keep_open opens "
                            "one)."));
    }
    const std::optional<QPoint> place = m_session->inputPlace(input);
    if (!place) {
      refuse(QStringLiteral("The panel of %1 shows no input %2 now (ui_state's command lists its inputs).")
                 .arg(m_session->def().name, mcp::echoed(input)));
    }
    at = *place;
  } else {
    const QJsonArray given = arguments.value(QStringLiteral("at")).toArray();
    if (given.size() != 2) {
      refuse(QStringLiteral("Give input or at [x, y]."));
    }
    at = QPoint(qRound(given.at(0).toDouble()), qRound(given.at(1).toDouble()));
  }
  if (!host->rect().contains(at)) {
    refuse(QStringLiteral("%1, %2 is outside the window, which is %3 × %4 pixels.")
               .arg(at.x())
               .arg(at.y())
               .arg(host->width())
               .arg(host->height()));
  }
  const QString buttonName = arguments.value(QStringLiteral("button")).toString(QStringLiteral("left"));
  const Qt::MouseButton button = buttonName == QStringLiteral("right")    ? Qt::RightButton
                                 : buttonName == QStringLiteral("middle") ? Qt::MiddleButton
                                                                          : Qt::LeftButton;
  const bool twice = arguments.value(QStringLiteral("double")).toBool(false);
  const QPoint global = host->mapToGlobal(at);
  QJsonObject result{{QStringLiteral("at"), QJsonArray{at.x(), at.y()}},
                     {QStringLiteral("button"), buttonName},
                     {QStringLiteral("double"), twice}};
  if (!input.isEmpty()) {
    result.insert(QStringLiteral("input"), input);
  }

  // An open list or menu: a click on it is for it, one outside it closes it
  // (and goes no further), as a real click does.
  QWidget* top = host;
  if (QWidget* popup = QApplication::activePopupWidget()) {
    if (!popup->geometry().contains(global)) {
      result.insert(QStringLiteral("closed"), widgetJson(popup));
      result.insert(QStringLiteral("posted"), false);
      popup->close();
      qInfo().noquote() << QStringLiteral("MCP: an agent's click at %1,%2 closed an open list").arg(at.x()).arg(at.y());
      return result;
    }
    top = popup;
  } else {
    // With native glass a floating panel is a window of its own over the
    // main window.
    for (GlassCard* card : host->findChildren<GlassCard*>()) {
      if (card->isWindow() && card->isVisible() && !card->testAttribute(Qt::WA_TransparentForMouseEvents) &&
          card->geometry().contains(global)) {
        top = card;
        break;
      }
    }
  }
  QWidget* target = top->childAt(top->mapFromGlobal(global));
  if (target == nullptr) {
    target = top;
  }
  activate(top->window());
  giveClickFocus(target);

  const QPointF local(target->mapFromGlobal(global));
  const QPointF scene(top->mapFromGlobal(global));
  const auto post = [&](QEvent::Type type, Qt::MouseButton which, Qt::MouseButtons held) {
    QCoreApplication::postEvent(target, new QMouseEvent(type, local, scene, QPointF(global), which, held,
                                                        Qt::NoModifier, QPointingDevice::primaryPointingDevice()));
  };
  post(QEvent::MouseMove, Qt::NoButton, Qt::NoButton);
  post(QEvent::MouseButtonPress, button, button);
  post(QEvent::MouseButtonRelease, button, Qt::NoButton);
  if (twice) {
    post(QEvent::MouseButtonDblClick, button, button);
    post(QEvent::MouseButtonRelease, button, Qt::NoButton);
  }
  result.insert(QStringLiteral("posted"), true);
  result.insert(QStringLiteral("target"), widgetJson(target));
  qInfo().noquote() << QStringLiteral("MCP: an agent clicked at %1,%2 (%3%4%5)")
                           .arg(at.x())
                           .arg(at.y())
                           .arg(QString::fromLatin1(target->metaObject()->className()),
                                twice ? QStringLiteral(", double") : QString(),
                                input.isEmpty() ? QString() : QStringLiteral(", input ") + input);
  return result;
}

QJsonObject MainWindow::mcpType(const QJsonObject& arguments) {
  mcpRequireInput(false);
  const QString text = arguments.value(QStringLiteral("text")).toString();
  std::vector<mcp::KeyStroke> strokes = mcp::typedKeys(text);
  QString problem;
  if (!mcp::parseKeys(arguments.value(QStringLiteral("keys")).toString(), strokes, problem)) {
    refuse(problem);
  }
  if (strokes.empty()) {
    refuse(QStringLiteral("Give text or keys to type."));
  }
  // What has the keyboard: an open list, else the focused widget of this
  // window or of one of its panels' windows.
  QWidget* host = window();
  QWidget* receiver = QApplication::activePopupWidget();
  if (receiver != nullptr) {
    if (receiver->focusWidget() != nullptr) {
      receiver = receiver->focusWidget();
    }
  } else if (QWidget* focused = QApplication::focusWidget();
             focused != nullptr && GlassCard::hostWindow(focused) == host) {
    receiver = focused;
  } else {
    receiver = host->focusWidget();
  }
  if (receiver == nullptr) {
    refuse(QStringLiteral("Nothing in the window has the keyboard: click a field first with ui_click."));
  }
  QJsonArray keys;
  for (const mcp::KeyStroke& stroke : strokes) {
    QCoreApplication::postEvent(receiver, new QKeyEvent(QEvent::KeyPress, stroke.key, stroke.modifiers, stroke.text));
    QCoreApplication::postEvent(receiver, new QKeyEvent(QEvent::KeyRelease, stroke.key, stroke.modifiers, stroke.text));
  }
  std::vector<mcp::KeyStroke> named;
  mcp::parseKeys(arguments.value(QStringLiteral("keys")).toString(), named, problem);
  for (const mcp::KeyStroke& stroke : named) {
    keys.append(stroke.name);
  }
  const qsizetype characters = static_cast<qsizetype>(strokes.size() - named.size());
  qInfo().noquote() << QStringLiteral("MCP: an agent typed %1 character(s) and %2 key(s) into %3")
                           .arg(characters)
                           .arg(keys.size())
                           .arg(QString::fromLatin1(receiver->metaObject()->className()));
  return {{QStringLiteral("posted"), true},
          {QStringLiteral("characters"), static_cast<qint64>(characters)},
          {QStringLiteral("keys"), keys},
          {QStringLiteral("target"), widgetJson(receiver)}};
}

QJsonObject MainWindow::mcpDialog(const QJsonObject& arguments) {
  mcpRequireInput(true);
  const QString open = arguments.value(QStringLiteral("open")).toString();
  const QString press = arguments.value(QStringLiteral("press")).toString().trimmed();
  QWidget* host = window();
  // A panel open for viewing (Send to Slicer's): its inputs and buttons.
  const auto viewedPanel = [this] { return m_session && mcp::opensDialog(m_session->def().id); };
  const auto panelRead = [this] {
    const CommandDef& def = m_session->def();
    return QJsonObject{
        {QStringLiteral("kind"), QStringLiteral("panel")},
        {QStringLiteral("command"), def.id},
        {QStringLiteral("title"), def.name},
        {QStringLiteral("panel"), m_session->scriptState()},
        {QStringLiteral("buttons"),
         QJsonArray{QJsonObject{{QStringLiteral("text"), def.okLabel.isEmpty() ? QStringLiteral("OK") : def.okLabel},
                                {QStringLiteral("closes"), false}},
                    QJsonObject{{QStringLiteral("text"), QStringLiteral("Cancel")}, {QStringLiteral("closes"), true}}}}};
  };

  if (!open.isEmpty()) {
    mcpRefuseBusy();
    if (m_closePending) {
      refuse(QStringLiteral("The design is closing."));
    }
    if (m_mcpDialogQueued) {
      refuseWith(QStringLiteral("busy: the window ui_dialog opens is opening; retry in 100 ms"),
                 {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 100}});
    }
    if (const QWidget* shown = mcpShownDialog()) {
      refuse(QStringLiteral("The window %1 is open: close it with press first.").arg(titleOf(shown)));
    }
    if (viewedPanel()) {
      refuse(QStringLiteral("The panel of %1 is open: close it with press Cancel first.").arg(m_session->def().name));
    }
    const CommandDef* def = m_registry->find(open);
    if (def == nullptr) {
      refuse(QStringLiteral("No command %1 in the command registry. %2")
                 .arg(mcp::echoed(open), mcp::nearestCommandsText(*m_registry, open)));
    }
    if (const QString why = mcp::dialogViewRefusal(open); !why.isEmpty()) {
      refuse(QStringLiteral("ui_dialog does not open %1: %2.").arg(def->name, why));
    }
    if (windowReadOnly() && !readOnlyCommand(*def)) {
      refuse(readOnlyReason());
    }
    if (!isAvailable(*def)) {
      refuse(QStringLiteral("%1 is not available now.").arg(def->name));
    }
    if (m_mcpDialogWatch) {
      m_mcpDialogWatch->deleteLater();
    }
    m_mcpDialogWatch = new mcp::DialogWatch(this);
    m_mcpDialogCommand = open;
    qInfo().noquote() << QStringLiteral("MCP: an agent opens %1 for viewing").arg(def->name);
    if (def->kind == CommandDef::Kind::Feature) {
      // A command panel opens at once (the Qt frontend); a dialog it would
      // open on its start is closed (ModalGuard), as ui_command does.
      const mcp::ModalGuard guard;
      trigger(open);
      if (!m_session || m_session->def().id != open) {
        refuse(m_statusLabel->text().isEmpty() ? QStringLiteral("%1 did not start.").arg(def->name)
                                               : m_statusLabel->text());
      }
      QJsonObject result = panelRead();
      result.insert(QStringLiteral("opened"), true);
      return result;
    }
    // A dialog may run its own event loop until it closes: right after this
    // answer, so that the connection goes on meanwhile.
    m_mcpDialogQueued = true;
    QTimer::singleShot(0, this, [this, open] {
      m_mcpDialogQueued = false;
      trigger(open);
    });
    return {{QStringLiteral("opening"), true},
            {QStringLiteral("command"), open},
            {QStringLiteral("name"), def->name},
            {QStringLiteral("note"), QStringLiteral("The window opens right after this result: ui_dialog with read lists it.")}};
  }

  const auto* watch = static_cast<const mcp::DialogWatch*>(m_mcpDialogWatch.data());
  QWidget* shown = mcpShownDialog();
  if (shown == nullptr && !viewedPanel()) {
    if (m_mcpDialogQueued) {
      refuseWith(QStringLiteral("busy: the window ui_dialog opens is opening; retry in 100 ms"),
                 {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), 100}});
    }
    if (watch != nullptr && watch->closedFileDialog()) {
      refuse(QStringLiteral("It opened a file dialog, which was closed: ui_dialog reads and writes no files."));
    }
    refuse(QStringLiteral("No dialog is open (ui_dialog with open opens one)."));
  }

  if (press.isEmpty()) {
    // read
    if (shown == nullptr) {
      return panelRead();
    }
    QJsonObject result = mcp::describeDialog(shown, host);
    result.insert(QStringLiteral("kind"), QStringLiteral("dialog"));
    if (watch != nullptr && watch->window() == shown) {
      result.insert(QStringLiteral("command"), m_mcpDialogCommand);
    }
    return result;
  }

  const bool closing = press.compare(QStringLiteral("close"), Qt::CaseInsensitive) == 0 ||
                       press.compare(QStringLiteral("cancel"), Qt::CaseInsensitive) == 0;
  if (shown == nullptr) {
    // The panel: Cancel only; its OK would act (Send writes a file and
    // starts the slicer).
    if (!closing) {
      refuse(QStringLiteral("ui_dialog closes the panel of %1 only with Cancel: %2 would act on the design or write "
                            "files.")
                 .arg(m_session->def().name, mcp::echoed(press)));
    }
    const QString name = m_session->def().name;
    m_session->cancel();
    waitFor([this] { return !m_session; }, 60000);
    qInfo().noquote() << QStringLiteral("MCP: an agent closed the panel of %1").arg(name);
    return {{QStringLiteral("closed"), !m_session},
            {QStringLiteral("kind"), QStringLiteral("panel")},
            {QStringLiteral("button"), QStringLiteral("Cancel")},
            {QStringLiteral("title"), name}};
  }

  const QString title = titleOf(shown);
  QAbstractButton* chosen = nullptr;
  QStringList closers;
  for (QAbstractButton* button : shown->findChildren<QAbstractButton*>()) {
    if (!button->isVisibleTo(shown)) {
      continue;
    }
    if (mcp::closesOnly(button)) {
      closers << mcp::buttonText(button);
    }
    if (chosen == nullptr && mcp::buttonText(button).compare(press, Qt::CaseInsensitive) == 0) {
      chosen = button;
    }
  }
  if (chosen != nullptr && !mcp::closesOnly(chosen)) {
    refuse(QStringLiteral("%1 would act on the window: ui_dialog presses only a button that closes it (%2).")
               .arg(mcp::buttonText(chosen), closers.isEmpty() ? QStringLiteral("Close") : closers.join(QStringLiteral(", "))));
  }
  if (chosen == nullptr && !closing) {
    refuse(QStringLiteral("The window %1 has no button %2: press Close or one of %3.")
               .arg(title, mcp::echoed(press), closers.isEmpty() ? QStringLiteral("Close") : closers.join(QStringLiteral(", "))));
  }
  if (chosen != nullptr && !chosen->isEnabled()) {
    refuse(QStringLiteral("%1 is not enabled now.").arg(mcp::buttonText(chosen)));
  }
  const QString pressed = chosen != nullptr ? mcp::buttonText(chosen) : press;
  const QPointer<QWidget> guardedWindow(shown);
  {
    // A question the window asks on closing is not answered.
    const mcp::ModalGuard guard;
    if (chosen != nullptr) {
      chosen->click();
    } else if (auto* dialog = qobject_cast<QDialog*>(shown)) {
      // As the title bar's close button.
      dialog->reject();
    } else {
      shown->close();
    }
  }
  const bool closed = !guardedWindow || !guardedWindow->isVisible();
  qInfo().noquote() << QStringLiteral("MCP: an agent pressed %1 in the window %2").arg(pressed, title);
  return {{QStringLiteral("closed"), closed},
          {QStringLiteral("kind"), QStringLiteral("dialog")},
          {QStringLiteral("button"), pressed},
          {QStringLiteral("title"), title}};
}

} // namespace mitcad
