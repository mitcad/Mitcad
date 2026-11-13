// SPDX-License-Identifier: MIT
#include "ModalGuard.hpp"

#include <QApplication>
#include <QDebug>
#include <QDialog>
#include <QEvent>
#include <QJsonObject>
#include <QLatin1String>
#include <QMetaObject>
#include <QPointer>
#include <QVariant>
#include <QWidget>
#include <QtLogging>

namespace mitcad::mcp {

ModalGuard::ModalGuard() {
  qApp->installEventFilter(this);
}

ModalGuard::~ModalGuard() {
  qApp->removeEventFilter(this);
}

bool ModalGuard::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() != QEvent::Show || !watched->isWidgetType()) {
    return false;
  }
  auto* widget = static_cast<QWidget*>(watched);
  if (!widget->isWindow() || !widget->isModal() || widget->objectName() == QStringLiteral("computeProgress")) {
    return false;
  }
  const QString name = dialogName(widget);
  if (m_opened.isEmpty()) {
    m_opened = name;
  }
  qInfo().noquote() << QStringLiteral("MCP: closed the dialog %1 (an agent cannot answer it)").arg(name);
  // In the dialog's own event loop, once it has shown.
  const QPointer<QWidget> guarded(widget);
  QMetaObject::invokeMethod(
      widget,
      [guarded] {
        if (!guarded) {
          return;
        }
        if (auto* dialog = qobject_cast<QDialog*>(guarded.data())) {
          dialog->reject();
        } else {
          guarded->close();
        }
      },
      Qt::QueuedConnection);
  return false;
}

QWidget* openDialog() {
  return QApplication::activeModalWidget();
}

QString dialogName(const QWidget* dialog) {
  return dialog->windowTitle().isEmpty() ? QString::fromLatin1(dialog->metaObject()->className())
                                         : dialog->windowTitle();
}

bool recoveryAtStart(const QWidget* dialog) {
  for (const QWidget* widget = dialog; widget != nullptr; widget = widget->parentWidget()) {
    if (widget->objectName() == QLatin1String(kRecoveryDialogName)) {
      return widget->property(kRecoveryAtStartProperty).toBool();
    }
  }
  return false;
}

QString dialogRefusal() {
  const QWidget* dialog = openDialog();
  if (dialog == nullptr) {
    return QString();
  }
  // For the agent, which relays it to its user: the dialog by its title.
  if (dialog->objectName() == QStringLiteral("computeProgress")) {
    return QStringLiteral("The design is computing (its progress dialog \"%1\" is open); retry when it has finished.")
        .arg(dialogName(dialog));
  }
  if (recoveryAtStart(dialog)) {
    return QStringLiteral("The dialog \"%1\" is open in MitCad: at start-up it asks what to do with the unsaved work "
                          "of an earlier session, and an agent cannot answer it. Ask the user to answer it in the "
                          "application (Later keeps that work), or start MitCad with --no-recovery when an agent "
                          "drives it; queries and ui_state work meanwhile.")
        .arg(dialogName(dialog));
  }
  return QStringLiteral("The dialog \"%1\" is open in MitCad and waits for the user's answer, which an agent cannot "
                        "give: ask the user to answer it in the application, then retry; queries and ui_state work "
                        "meanwhile.")
      .arg(dialogName(dialog));
}

QJsonValue dialogState() {
  const QWidget* dialog = openDialog();
  if (dialog == nullptr) {
    return QJsonValue();
  }
  return QJsonObject{{QStringLiteral("title"), dialogName(dialog)},
                     {QStringLiteral("recovery_at_start"), recoveryAtStart(dialog)}};
}

} // namespace mitcad::mcp
