// SPDX-License-Identifier: MIT
#include "mcp/DialogWatch.hpp"

#include <algorithm>
#include <functional>

#include <QAbstractButton>
#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QAbstractSpinBox>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QDebug>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFileDialog>
#include <QFormLayout>
#include <QHash>
#include <QLabel>
#include <QLineEdit>
#include <QMainWindow>
#include <QMenu>
#include <QMetaObject>
#include <QModelIndex>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QRect>
#include <QTabBar>
#include <QTextEdit>
#include <QToolButton>
#include <QTreeView>
#include <QtLogging>

namespace mitcad::mcp {

namespace {

// The most a read lists: values are cut, long tables and lists end early.
constexpr int kValueChars = 500;
constexpr int kTableRows = 100;
constexpr int kTableColumns = 20;
constexpr int kOptions = 50;
constexpr int kFields = 300;

QString plain(QString text) {
  // A mnemonic's '&' (a literal one is written '&&').
  text.replace(QStringLiteral("&&"), QString(QChar(0x1)));
  text.remove(QLatin1Char('&'));
  text.replace(QChar(0x1), QLatin1Char('&'));
  return text.trimmed();
}

QString cut(const QString& text) {
  return text.size() > kValueChars ? text.left(kValueChars) + QString(QChar(0x2026)) : text;
}

// Inside a control that lists its own parts (a table's editors, a spin
// box's or a combo box's line edit): those are read with the control.
bool insideComposite(const QWidget* widget, const QWidget* window) {
  for (const QWidget* parent = widget->parentWidget(); parent != nullptr && parent != window;
       parent = parent->parentWidget()) {
    if (qobject_cast<const QAbstractItemView*>(parent) != nullptr ||
        qobject_cast<const QAbstractSpinBox*>(parent) != nullptr ||
        qobject_cast<const QComboBox*>(parent) != nullptr) {
      return true;
    }
  }
  return false;
}

// A field's label: the label whose buddy it is, or its form row's.
QString labelOf(const QWidget* widget, const QHash<const QWidget*, QString>& buddies,
                const QList<QFormLayout*>& forms) {
  if (const auto found = buddies.constFind(widget); found != buddies.constEnd()) {
    return found.value();
  }
  for (const QFormLayout* form : forms) {
    if (const auto* label = qobject_cast<const QLabel*>(form->labelForField(const_cast<QWidget*>(widget)))) {
      return plain(label->text());
    }
  }
  return {};
}

QJsonObject tableOf(const QAbstractItemView* view) {
  QJsonObject entry;
  const QAbstractItemModel* model = view->model();
  if (model == nullptr) {
    return entry;
  }
  const int columns = std::min(model->columnCount(), kTableColumns);
  QJsonArray headers;
  for (int column = 0; column < columns; ++column) {
    headers.append(model->headerData(column, Qt::Horizontal, Qt::DisplayRole).toString());
  }
  // The rows of a tree too, each group's rows after it.
  QJsonArray rows;
  int count = 0;
  std::function<void(const QModelIndex&)> walk = [&](const QModelIndex& parent) {
    for (int row = 0; row < model->rowCount(parent); ++row) {
      ++count;
      if (rows.size() < kTableRows) {
        QJsonArray cells;
        for (int column = 0; column < columns; ++column) {
          cells.append(cut(model->index(row, column, parent).data(Qt::DisplayRole).toString()));
        }
        rows.append(cells);
      }
      const QModelIndex first = model->index(row, 0, parent);
      if (model->hasChildren(first)) {
        walk(first);
      }
    }
  };
  walk(QModelIndex());
  entry.insert(QStringLiteral("columns"), headers);
  entry.insert(QStringLiteral("rows"), rows);
  entry.insert(QStringLiteral("row_count"), count);
  return entry;
}

} // namespace

DialogWatch::DialogWatch(QWidget* host) : QObject(host), m_host(host) {
  qApp->installEventFilter(this);
}

DialogWatch::~DialogWatch() {
  stop();
}

void DialogWatch::stop() {
  if (m_watching) {
    m_watching = false;
    qApp->removeEventFilter(this);
  }
}

QWidget* DialogWatch::window() const {
  return m_window && m_window->isVisible() ? m_window.data() : nullptr;
}

bool DialogWatch::eventFilter(QObject* watched, QEvent* event) {
  if (!m_watching || event->type() != QEvent::Show || !watched->isWidgetType()) {
    return false;
  }
  auto* widget = static_cast<QWidget*>(watched);
  if (!isDialogWindow(widget) || widget == m_host.data() || (m_host && widget->window() == m_host->window())) {
    return false;
  }
  stop();
  if (auto* files = qobject_cast<QFileDialog*>(widget)) {
    // Files stay out of reach: closed as cancelled, in its own loop once
    // it has shown (as ModalGuard closes dialogs).
    m_fileDialog = true;
    qInfo().noquote() << QStringLiteral("MCP: closed a file dialog (ui_dialog opens no files)");
    const QPointer<QFileDialog> guarded(files);
    QMetaObject::invokeMethod(
        files,
        [guarded] {
          if (guarded) {
            guarded->reject();
          }
        },
        Qt::QueuedConnection);
    return false;
  }
  m_window = widget;
  qInfo().noquote() << QStringLiteral("MCP: ui_dialog shows the window %1")
                           .arg(widget->windowTitle().isEmpty() ? QString::fromLatin1(widget->metaObject()->className())
                                                                : widget->windowTitle());
  return false;
}

bool isDialogWindow(const QWidget* widget) {
  if (widget == nullptr || !widget->isWindow()) {
    return false;
  }
  const Qt::WindowType type = widget->windowType();
  if (type == Qt::Popup || type == Qt::ToolTip || type == Qt::SplashScreen) {
    return false;
  }
  return qobject_cast<const QMenu*>(widget) == nullptr && qobject_cast<const QMainWindow*>(widget) == nullptr &&
         !widget->inherits("mitcad::GlassCard") && widget->objectName() != QStringLiteral("computeProgress");
}

QString buttonText(const QAbstractButton* button) {
  return plain(button->text());
}

bool closesOnly(const QAbstractButton* button) {
  for (const QWidget* parent = button->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
    if (const auto* box = qobject_cast<const QDialogButtonBox*>(parent)) {
      const QDialogButtonBox::StandardButton standard = box->standardButton(const_cast<QAbstractButton*>(button));
      if (standard == QDialogButtonBox::Close || standard == QDialogButtonBox::Cancel) {
        return true;
      }
      return box->buttonRole(const_cast<QAbstractButton*>(button)) == QDialogButtonBox::RejectRole;
    }
    if (parent->isWindow()) {
      break;
    }
  }
  const QString text = buttonText(button).toLower();
  return text == QStringLiteral("close") || text == QStringLiteral("cancel");
}

QJsonObject describeDialog(const QWidget* window, const QWidget* host) {
  const QRect frame = window->geometry();
  const QPoint corner = host != nullptr ? host->mapFromGlobal(window->mapToGlobal(QPoint(0, 0))) : frame.topLeft();
  QJsonObject result{
      {QStringLiteral("title"), window->windowTitle()},
      {QStringLiteral("name"), window->objectName()},
      {QStringLiteral("class"), QString::fromLatin1(window->metaObject()->className())},
      {QStringLiteral("modal"), window->isModal()},
      {QStringLiteral("place"), QJsonArray{corner.x(), corner.y(), frame.width(), frame.height()}}};

  const QList<QWidget*> widgets = window->findChildren<QWidget*>();
  QHash<const QWidget*, QString> buddies;
  for (const QWidget* widget : widgets) {
    if (const auto* label = qobject_cast<const QLabel*>(widget); label != nullptr && label->buddy() != nullptr) {
      buddies.insert(label->buddy(), plain(label->text()));
    }
  }
  const QList<QFormLayout*> forms = window->findChildren<QFormLayout*>();
  QJsonArray fields;
  QJsonArray buttons;
  for (const QWidget* widget : widgets) {
    if (!widget->isVisibleTo(window) || insideComposite(widget, window) || fields.size() >= kFields) {
      continue;
    }
    QJsonObject field;
    if (const auto* check = qobject_cast<const QCheckBox*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("check")},
               {QStringLiteral("label"), buttonText(check)},
               {QStringLiteral("value"), check->isChecked()}};
    } else if (const auto* radio = qobject_cast<const QRadioButton*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("option")},
               {QStringLiteral("label"), buttonText(radio)},
               {QStringLiteral("value"), radio->isChecked()}};
    } else if (const auto* button = qobject_cast<const QAbstractButton*>(widget)) {
      const bool pushed = qobject_cast<const QPushButton*>(button) != nullptr;
      if (!pushed && buttonText(button).isEmpty()) {
        continue; // an icon of a field or a list (its tooltip says what)
      }
      const auto* push = qobject_cast<const QPushButton*>(button);
      buttons.append(QJsonObject{{QStringLiteral("text"), buttonText(button)},
                                 {QStringLiteral("name"), button->objectName()},
                                 {QStringLiteral("enabled"), button->isEnabled()},
                                 {QStringLiteral("default"), push != nullptr && push->isDefault()},
                                 {QStringLiteral("closes"), closesOnly(button)}});
      continue;
    } else if (const auto* line = qobject_cast<const QLineEdit*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("text")},
               {QStringLiteral("value"), cut(line->text())},
               {QStringLiteral("placeholder"), line->placeholderText()},
               {QStringLiteral("read_only"), line->isReadOnly()}};
    } else if (const auto* spin = qobject_cast<const QAbstractSpinBox*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("number")}, {QStringLiteral("value"), spin->text()}};
    } else if (const auto* combo = qobject_cast<const QComboBox*>(widget)) {
      QJsonArray options;
      for (int i = 0; i < std::min(combo->count(), kOptions); ++i) {
        options.append(combo->itemText(i));
      }
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("choice")},
               {QStringLiteral("value"), combo->currentText()},
               {QStringLiteral("options"), options}};
    } else if (const auto* tabs = qobject_cast<const QTabBar*>(widget)) {
      QJsonArray options;
      for (int i = 0; i < tabs->count(); ++i) {
        options.append(plain(tabs->tabText(i)));
      }
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("tabs")},
               {QStringLiteral("value"), plain(tabs->tabText(tabs->currentIndex()))},
               {QStringLiteral("options"), options}};
    } else if (const auto* view = qobject_cast<const QAbstractItemView*>(widget)) {
      field = tableOf(view);
      field.insert(QStringLiteral("kind"), qobject_cast<const QTreeView*>(view) != nullptr ? QStringLiteral("tree")
                                                                                          : QStringLiteral("table"));
    } else if (const auto* rich = qobject_cast<const QTextEdit*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("text")}, {QStringLiteral("value"), cut(rich->toPlainText())}};
    } else if (const auto* plainEdit = qobject_cast<const QPlainTextEdit*>(widget)) {
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("text")},
               {QStringLiteral("value"), cut(plainEdit->toPlainText())}};
    } else if (const auto* label = qobject_cast<const QLabel*>(widget)) {
      // A text of its own (a field's label goes with its field).
      if (label->buddy() != nullptr || plain(label->text()).isEmpty()) {
        continue;
      }
      bool ofForm = false;
      for (const QFormLayout* form : forms) {
        ofForm = ofForm || form->indexOf(const_cast<QLabel*>(label)) >= 0;
      }
      if (ofForm) {
        continue;
      }
      field = QJsonObject{{QStringLiteral("kind"), QStringLiteral("label")}, {QStringLiteral("value"), cut(label->text())}};
    } else {
      continue;
    }
    if (!field.contains(QStringLiteral("label"))) {
      const QString label = labelOf(widget, buddies, forms);
      if (!label.isEmpty()) {
        field.insert(QStringLiteral("label"), label);
      }
    }
    field.insert(QStringLiteral("name"), widget->objectName());
    field.insert(QStringLiteral("enabled"), widget->isEnabled());
    fields.append(field);
  }
  result.insert(QStringLiteral("fields"), fields);
  result.insert(QStringLiteral("buttons"), buttons);
  return result;
}

} // namespace mitcad::mcp
