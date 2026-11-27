// SPDX-License-Identifier: MIT
#include "ParameterInsert.hpp"

#include <exception>
#include <memory>
#include <utility>

#include <QAction>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QObject>
#include <QPlainTextEdit>
#include <QPointer>
#include <QPushButton>
#include <QTextCursor>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/PanelTree.hpp"
#include "ParameterReferences.hpp"

namespace mitcad::drawings {
namespace {

// The field's name in the log: its object name, else its class's.
QString fieldName(const QWidget* field) {
  return field->objectName().isEmpty() ? QString::fromLatin1(field->metaObject()->className()) : field->objectName();
}

// The design's parameters; none when the model cannot be asked.
QJsonArray parametersOf(const ParameterList& parameters) {
  if (!parameters) {
    return QJsonArray();
  }
  try {
    return parameters();
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Insert Parameter: the parameters could not be read: %1")
                                .arg(QString::fromUtf8(e.what()));
    return QJsonArray();
  }
}

// Up and Down in the filter move through the list, as in a search field
// over a list.
class ListKeys : public QObject {
public:
  ListKeys(QLineEdit* filter, QTreeWidget* list) : QObject(filter), m_list(list) { filter->installEventFilter(this); }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() != QEvent::KeyPress) {
      return QObject::eventFilter(watched, event);
    }
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key != Qt::Key_Up && key != Qt::Key_Down) {
      return false;
    }
    QTreeWidgetItem* current = m_list->currentItem();
    QTreeWidgetItem* next = current == nullptr ? nullptr
                            : key == Qt::Key_Down ? m_list->itemBelow(current)
                                                  : m_list->itemAbove(current);
    if (current == nullptr) {
      for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
        if (!m_list->topLevelItem(i)->isHidden()) {
          next = m_list->topLevelItem(i);
          break;
        }
      }
    }
    if (next != nullptr) {
      m_list->setCurrentItem(next);
    }
    return true;
  }

private:
  QTreeWidget* m_list;
};

// The menu's own entry, opened after the menu has closed.
QAction* addInsertAction(QMenu* menu, bool enabled) {
  menu->addSeparator();
  QAction* insert = menu->addAction(QObject::tr("&Insert Parameter…"));
  insert->setObjectName(QStringLiteral("insertParameter"));
  insert->setEnabled(enabled);
  return insert;
}

} // namespace

std::optional<QString> askParameter(QWidget* parent, const QJsonArray& parameters) {
  const QList<ParameterEntry> entries = parameterEntries(parameters);
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert Parameter"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* filter = new QLineEdit(&dialog);
  filter->setObjectName(QStringLiteral("insertParameterFilter"));
  filter->setPlaceholderText(QObject::tr("Filter by name"));
  filter->setClearButtonEnabled(true);
  layout->addWidget(filter);
  auto* list = new QTreeWidget(&dialog);
  list->setObjectName(QStringLiteral("insertParameterList"));
  list->setRootIsDecorated(false);
  // Rows without children (docs/ui-conventions.md, lists and trees).
  panel::setCompactIndentation(list, panel::Indent::Flat);
  list->setColumnCount(2);
  list->setHeaderLabels({QObject::tr("Name"), QObject::tr("Value")});
  list->header()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  list->setMinimumWidth(320);
  for (const ParameterEntry& entry : entries) {
    auto* item = new QTreeWidgetItem(list, {entry.name, entry.value});
    item->setData(0, Qt::UserRole, entry.name);
    item->setToolTip(0, entry.user ? QObject::tr("User parameter") : QObject::tr("Model parameter"));
  }
  layout->addWidget(list);
  auto* none = new QLabel(QObject::tr("This design has no parameters. Add them in Edit Parameters."), &dialog);
  none->setWordWrap(true);
  none->setVisible(entries.isEmpty());
  layout->addWidget(none);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  layout->addWidget(buttons);
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  ok->setText(QObject::tr("Insert"));
  ok->setDefault(true);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(list, &QTreeWidget::itemDoubleClicked, &dialog, &QDialog::accept);
  const auto update = [list, ok] { ok->setEnabled(list->currentItem() != nullptr && !list->currentItem()->isHidden()); };
  QObject::connect(list, &QTreeWidget::currentItemChanged, &dialog, update);
  // The filter keeps the names that contain what is typed, the first of
  // them current.
  QObject::connect(filter, &QLineEdit::textChanged, &dialog, [list, update](const QString& typed) {
    QTreeWidgetItem* first = nullptr;
    for (int i = 0; i < list->topLevelItemCount(); ++i) {
      QTreeWidgetItem* item = list->topLevelItem(i);
      const bool shown = item->text(0).contains(typed.trimmed(), Qt::CaseInsensitive);
      item->setHidden(!shown);
      if (shown && first == nullptr) {
        first = item;
      }
    }
    list->setCurrentItem(first);
    update();
  });
  new ListKeys(filter, list);
  if (list->topLevelItemCount() > 0) {
    list->setCurrentItem(list->topLevelItem(0));
  }
  update();
  filter->setFocus();
  qDebug().noquote() << QStringLiteral("Insert Parameter dialog opened: %1 parameters").arg(entries.size());
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted || list->currentItem() == nullptr ||
      list->currentItem()->isHidden()) {
    return std::nullopt;
  }
  const QString name = list->currentItem()->data(0, Qt::UserRole).toString();
  qDebug().noquote() << QStringLiteral("Insert Parameter dialog: %1").arg(name);
  return name;
}

void addInsertParameter(QLineEdit* edit, ParameterList parameters, std::function<void()> inserted) {
  edit->setContextMenuPolicy(Qt::CustomContextMenu);
  QObject::connect(
      edit, &QWidget::customContextMenuRequested, edit,
      [edit, parameters = std::move(parameters), inserted = std::move(inserted)](const QPoint& at) {
        const std::unique_ptr<QMenu> menu(edit->createStandardContextMenu());
        QAction* insert = addInsertAction(menu.get(), edit->isEnabled() && !edit->isReadOnly());
        qDebug().noquote() << QStringLiteral("Insert Parameter menu: %1").arg(fieldName(edit));
        const QPointer<QLineEdit> alive(edit);
        QAction* chosen = execMenu(menu.get(), edit->mapToGlobal(at));
        if (!alive || chosen == nullptr || chosen != insert) {
          return;
        }
        QTimer::singleShot(0, edit, [edit, parameters, inserted] {
          const QPointer<QLineEdit> field(edit);
          const std::optional<QString> name = askParameter(edit->window(), parametersOf(parameters));
          if (!field || !name) {
            return;
          }
          const QString reference = parameterReference(*name);
          edit->setFocus(Qt::OtherFocusReason);
          edit->insert(reference);
          qDebug().noquote() << QStringLiteral("Parameter inserted in %1: %2").arg(fieldName(edit), reference);
          if (inserted) {
            inserted();
          }
        });
      });
}

void addInsertParameter(QPlainTextEdit* edit, ParameterList parameters) {
  edit->setContextMenuPolicy(Qt::CustomContextMenu);
  QObject::connect(
      edit, &QWidget::customContextMenuRequested, edit, [edit, parameters = std::move(parameters)](const QPoint& at) {
        const std::unique_ptr<QMenu> menu(edit->createStandardContextMenu());
        QAction* insert = addInsertAction(menu.get(), edit->isEnabled() && !edit->isReadOnly());
        qDebug().noquote() << QStringLiteral("Insert Parameter menu: %1").arg(fieldName(edit));
        const QPointer<QPlainTextEdit> alive(edit);
        // A scroll area's menu position is its viewport's.
        QAction* chosen = execMenu(menu.get(), edit->viewport()->mapToGlobal(at));
        if (!alive || chosen == nullptr || chosen != insert) {
          return;
        }
        QTimer::singleShot(0, edit, [edit, parameters] {
          const QPointer<QPlainTextEdit> field(edit);
          const std::optional<QString> name = askParameter(edit->window(), parametersOf(parameters));
          if (!field || !name) {
            return;
          }
          const QString reference = parameterReference(*name);
          edit->setFocus(Qt::OtherFocusReason);
          QTextCursor cursor = edit->textCursor();
          cursor.insertText(reference);
          edit->setTextCursor(cursor);
          qDebug().noquote() << QStringLiteral("Parameter inserted in %1: %2").arg(fieldName(edit), reference);
        });
      });
}

QString missingParametersText(const QStringList& missing) {
  if (missing.isEmpty()) {
    return QString();
  }
  if (missing.size() == 1) {
    return QObject::tr("This design has no parameter %1: the sheet shows its reference as written.")
        .arg(missing.first());
  }
  return QObject::tr("This design has no parameters %1: the sheet shows their references as written.")
      .arg(missing.join(QStringLiteral(", ")));
}

QLabel* missingParametersLabel(const QString& name) {
  auto* label = new QLabel;
  label->setObjectName(name);
  label->setWordWrap(true);
  label->setTextFormat(Qt::RichText);
  label->hide();
  return label;
}

void showMissingParameters(QLabel* label, const QString& text, const QJsonArray& parameters) {
  const QStringList missing = missingParameters(text, parameters);
  const QString message = missingParametersText(missing);
  const QString shown = message.isEmpty() ? QString() : noticeHtml(NoticeLevel::Warning, message, label->palette());
  if (shown == label->text()) {
    return;
  }
  label->setText(shown);
  label->setVisible(!message.isEmpty());
  if (!missing.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Missing parameters under %1: %2")
                              .arg(label->objectName(), missing.join(QStringLiteral(", ")));
  }
}

} // namespace mitcad::drawings
