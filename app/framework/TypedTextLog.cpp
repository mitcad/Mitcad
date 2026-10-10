// SPDX-License-Identifier: MIT
#include "framework/TypedTextLog.hpp"

#include <QApplication>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QLineEdit>
#include <QSpinBox>
#include <QWidget>
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

} // namespace

void installTypedTextLogIfRequested() {
  if (qEnvironmentVariableIntValue("MITCAD_LOG_TYPED_TEXT") == 0 || qApp == nullptr) {
    return;
  }
  QObject::connect(qApp, &QApplication::focusChanged, qApp, [](QWidget*, QWidget* now) { focusChanged(now); });
}

} // namespace mitcad
