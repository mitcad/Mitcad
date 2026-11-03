// SPDX-License-Identifier: MIT
#include "PrintValuePicker.hpp"

#include <QDebug>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFont>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPalette>
#include <QPushButton>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/PanelTree.hpp"

namespace mitcad {
namespace {

constexpr int kChoiceRole = Qt::UserRole + 1; // a choice's index; -1 on a heading

QTreeWidgetItem* headingItem(QTreeWidgetItem* parent, QTreeWidget* tree, const QString& title, bool top) {
  auto* item = parent != nullptr ? new QTreeWidgetItem(parent, {title}) : new QTreeWidgetItem(tree, {title});
  item->setFirstColumnSpanned(true);
  item->setFlags(Qt::ItemIsEnabled);
  item->setData(0, kChoiceRole, -1);
  QFont font = item->font(0);
  font.setBold(true);
  if (!top && font.pointSizeF() > 0.0) {
    font.setPointSizeF(font.pointSizeF() * 0.9); // a heading line, as the Properties window's
  }
  item->setFont(0, font);
  return item;
}

} // namespace

PrintValuePicker::PrintValuePicker(const QString& body, const QVector<PrintValueChoice>& choices, QWidget* parent)
    : QDialog(parent), m_choices(choices) {
  setObjectName(QStringLiteral("printValuePicker"));
  setWindowTitle(tr("Add Print Value"));
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(tr("A setting %1 prints with of its own:").arg(body), this);
  intro->setWordWrap(true);
  layout->addWidget(intro);
  m_search = new QLineEdit(this);
  m_search->setObjectName(QStringLiteral("printValueSearch"));
  m_search->setPlaceholderText(tr("Search settings"));
  m_search->setClearButtonEnabled(true);
  m_search->installEventFilter(this);
  layout->addWidget(m_search);
  m_tree = new QTreeWidget(this);
  m_tree->setObjectName(QStringLiteral("printValueList"));
  m_tree->setColumnCount(2);
  m_tree->setHeaderHidden(true);
  m_tree->setRootIsDecorated(false);
  panel::setCompactIndentation(m_tree, panel::Indent::GroupedList);
  m_tree->setUniformRowHeights(true);
  m_tree->header()->setStretchLastSection(false);
  m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_tree->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  layout->addWidget(m_tree, 1);

  // Under Process and Printer, and their category › sub-group headings.
  QString shownSet;
  QString shownHeading;
  QTreeWidgetItem* setItem = nullptr;
  QTreeWidgetItem* heading = nullptr;
  m_items.reserve(m_choices.size());
  for (int i = 0; i < static_cast<int>(m_choices.size()); ++i) {
    const PrintValueChoice& choice = m_choices.at(i);
    if (setItem == nullptr || choice.set != shownSet) {
      shownSet = choice.set;
      shownHeading.clear();
      setItem = headingItem(nullptr, m_tree, choice.set == QLatin1String("printer") ? tr("Printer") : tr("Process"),
                            true);
    }
    if (heading == nullptr || choice.heading != shownHeading) {
      shownHeading = choice.heading;
      heading = headingItem(setItem, m_tree, choice.heading, false);
    }
    auto* item = new QTreeWidgetItem(heading, {choice.name, choice.text.isEmpty() ? tr("Slicer default") : choice.text});
    item->setData(0, kChoiceRole, i);
    const QString starts = choice.text.isEmpty() ? tr("No value yet: type one on the card")
                                                 : tr("Starts at %1, the value it prints with now").arg(choice.text);
    item->setToolTip(0, QStringLiteral("%1\n%2").arg(choice.key, starts));
    item->setToolTip(1, item->toolTip(0));
    if (choice.text.isEmpty()) {
      // Unset: the placeholder's grey and italic, as the Properties window's.
      item->setForeground(1, palette().color(QPalette::PlaceholderText));
      QFont unset = item->font(1);
      unset.setItalic(true);
      item->setFont(1, unset);
    }
    m_items.append(item);
  }
  m_tree->expandAll();

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
  m_ok = buttons->button(QDialogButtonBox::Ok);
  m_ok->setText(tr("Add"));
  m_ok->setDefault(true);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, this, [this] { choose(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_search, &QLineEdit::textChanged, this, [this](const QString& text) { filter(text); });
  connect(m_tree, &QTreeWidget::currentItemChanged, this,
          [this] { m_ok->setEnabled(currentChoiceItem() != nullptr); });
  connect(m_tree, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item) {
    if (item != nullptr && item->data(0, kChoiceRole).toInt() >= 0) {
      m_tree->setCurrentItem(item);
      choose();
    }
  });
  resize(420, 480);
  qDebug().noquote() << QStringLiteral("Add print value opened for %1: %2 settings").arg(body).arg(m_choices.size());
  filter(QString());
  m_search->setFocus(Qt::OtherFocusReason);
}

const PrintValueChoice* PrintValuePicker::chosen() const {
  return m_chosen >= 0 && m_chosen < static_cast<int>(m_choices.size()) ? &m_choices.at(m_chosen) : nullptr;
}

QTreeWidgetItem* PrintValuePicker::currentChoiceItem() const {
  QTreeWidgetItem* item = m_tree->currentItem();
  return item != nullptr && !item->isHidden() && item->data(0, kChoiceRole).toInt() >= 0 ? item : nullptr;
}

void PrintValuePicker::filter(const QString& text) {
  int shown = 0;
  for (int i = 0; i < static_cast<int>(m_items.size()); ++i) {
    const bool match = printValueMatches(m_choices.at(i), text);
    m_items.at(i)->setHidden(!match);
    shown += match ? 1 : 0;
  }
  // A heading without a setting shown goes too.
  for (int top = 0; top < m_tree->topLevelItemCount(); ++top) {
    QTreeWidgetItem* setItem = m_tree->topLevelItem(top);
    bool setShown = false;
    for (int h = 0; h < setItem->childCount(); ++h) {
      QTreeWidgetItem* heading = setItem->child(h);
      bool headingShown = false;
      for (int k = 0; k < heading->childCount(); ++k) {
        headingShown = headingShown || !heading->child(k)->isHidden();
      }
      heading->setHidden(!headingShown);
      setShown = setShown || headingShown;
    }
    setItem->setHidden(!setShown);
  }
  const int best = printValueBestMatch(m_choices, text);
  if (best >= 0) {
    m_tree->setCurrentItem(m_items.at(best));
    m_tree->scrollToItem(m_items.at(best));
  } else {
    m_tree->setCurrentItem(nullptr);
  }
  m_ok->setEnabled(best >= 0);
  qDebug().noquote() << QStringLiteral("Add print value search: %1: %2 shown, current %3")
                            .arg(text.simplified())
                            .arg(shown)
                            .arg(best >= 0 ? m_choices.at(best).key : QStringLiteral("none"));
}

void PrintValuePicker::choose() {
  QTreeWidgetItem* item = currentChoiceItem();
  if (item == nullptr) {
    return;
  }
  m_chosen = item->data(0, kChoiceRole).toInt();
  accept();
}

bool PrintValuePicker::eventFilter(QObject* watched, QEvent* event) {
  // Up and Down in the search field move in the list, over the headings
  // and the settings hidden by the search.
  if (watched == m_search && event->type() == QEvent::KeyPress) {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Up || key == Qt::Key_Down) {
      int current = -1;
      if (QTreeWidgetItem* item = currentChoiceItem()) {
        current = item->data(0, kChoiceRole).toInt();
      }
      const int step = key == Qt::Key_Down ? 1 : -1;
      for (int i = current + step; i >= 0 && i < static_cast<int>(m_items.size()); i += step) {
        if (!m_items.at(i)->isHidden()) {
          m_tree->setCurrentItem(m_items.at(i));
          m_tree->scrollToItem(m_items.at(i));
          break;
        }
      }
      return true;
    }
  }
  return QDialog::eventFilter(watched, event);
}

} // namespace mitcad
