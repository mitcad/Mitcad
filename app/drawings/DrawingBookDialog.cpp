// SPDX-License-Identifier: MIT
#include "DrawingBookDialog.hpp"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QFont>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonValue>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSet>
#include <QStyle>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ModelMessages.hpp"
#include "../framework/PanelTree.hpp"

namespace mitcad::drawings {
namespace {

// What a row holds: a design's row (a group) or a drawing's.
constexpr int kOwnRole = Qt::UserRole;          // bool: of this design
constexpr int kSourceRole = Qt::UserRole + 1;   // the linked design's file as its link saves it
constexpr int kDrawingRole = Qt::UserRole + 2;  // the drawing's id (D3); empty on a design's row
constexpr int kNameRole = Qt::UserRole + 3;     // the drawing's name
constexpr int kProblemRole = Qt::UserRole + 4;  // why it cannot be printed, in words
constexpr int kDesignRole = Qt::UserRole + 5;   // the design as the log writes it

QString jsonString(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

bool isDrawing(const QTreeWidgetItem* item) {
  return item != nullptr && !item->data(0, kDrawingRole).toString().isEmpty();
}

} // namespace

DrawingBookDialog::DrawingBookDialog(const QJsonObject& book, const QString& designTitle, QWidget* parent)
    : QDialog(parent) {
  setObjectName(QStringLiteral("drawingBookDialog"));
  setWindowTitle(tr("Drawing Book"));
  auto* layout = new QVBoxLayout(this);

  auto* form = new QFormLayout;
  m_title = new QLineEdit(jsonString(book, "title"));
  m_title->setObjectName(QStringLiteral("drawingBookTitle"));
  m_title->setPlaceholderText(designTitle);
  m_title->setToolTip(tr("The book's title, on its cover page; the design's name when left empty"));
  form->addRow(tr("&Title:"), m_title);
  layout->addLayout(form);

  m_tree = new QTreeWidget;
  m_tree->setObjectName(QStringLiteral("drawingBookTree"));
  panel::setCompactIndentation(m_tree, panel::Indent::GroupedList);
  m_tree->setColumnCount(2);
  m_tree->setHeaderLabels({tr("Drawing"), tr("Sheets")});
  m_tree->header()->setStretchLastSection(false);
  m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  m_tree->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
  m_tree->setToolTip(tr("Checked drawings go into the book, in this order"));
  m_tree->setMinimumSize(380, 260);

  m_up = new QPushButton(tr("Move &Up"));
  m_up->setObjectName(QStringLiteral("drawingBookUp"));
  m_up->setToolTip(tr("Moves the drawing, or the design with its drawings, one place up"));
  m_down = new QPushButton(tr("Move &Down"));
  m_down->setObjectName(QStringLiteral("drawingBookDown"));
  m_down->setToolTip(tr("Moves the drawing, or the design with its drawings, one place down"));
  auto* moves = new QVBoxLayout;
  moves->addWidget(m_up);
  moves->addWidget(m_down);
  moves->addStretch(1);
  auto* row = new QHBoxLayout;
  row->addWidget(m_tree, 1);
  row->addLayout(moves);
  layout->addLayout(row, 1);

  m_cover = new QCheckBox(tr("&Cover page"));
  m_cover->setObjectName(QStringLiteral("drawingBookCover"));
  m_cover->setToolTip(tr("Prints a page with the title and the contents before the first sheet"));
  m_cover->setChecked(book.value(QStringLiteral("cover")).toBool());
  layout->addWidget(m_cover);

  // The message line takes no room while it is empty (docs/ui-conventions.md).
  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("drawingBookMessage"));
  m_message->setWordWrap(true);
  m_message->hide();
  layout->addWidget(m_message);

  auto* buttons = new QDialogButtonBox;
  m_print = buttons->addButton(tr("&Print…"), QDialogButtonBox::AcceptRole);
  m_print->setObjectName(QStringLiteral("drawingBookPrint"));
  m_save = buttons->addButton(QDialogButtonBox::Save);
  m_save->setObjectName(QStringLiteral("drawingBookSave"));
  m_save->setToolTip(tr("Keeps the book with the design without printing it"));
  buttons->addButton(QDialogButtonBox::Cancel);
  m_print->setDefault(true);
  layout->addWidget(buttons);
  connect(m_print, &QPushButton::clicked, this, [this] {
    m_printAsked = true;
    accept();
  });
  connect(m_save, &QPushButton::clicked, this, [this] {
    m_printAsked = false;
    accept();
  });
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

  build(book);

  connect(m_tree, &QTreeWidget::itemChanged, this, [this] {
    if (!m_building) {
      refresh();
    }
  });
  connect(m_tree, &QTreeWidget::currentItemChanged, this, [this] { refresh(); });
  connect(m_cover, &QCheckBox::toggled, this, [this] { refresh(); });
  connect(m_up, &QPushButton::clicked, this, [this] { moveCurrent(-1); });
  connect(m_down, &QPushButton::clicked, this, [this] { moveCurrent(1); });
  refresh();
  m_tree->setFocus(Qt::OtherFocusReason);
}

QTreeWidgetItem* DrawingBookDialog::addGroup(const QString& label, const QString& tooltip, const QString& problem) {
  auto* group = new QTreeWidgetItem(m_tree);
  group->setText(0, label);
  group->setData(0, kDesignRole, label);
  group->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
  QFont font = group->font(0);
  font.setBold(true);
  group->setFont(0, font);
  group->setToolTip(0, problem.isEmpty() ? tooltip : problem);
  if (!problem.isEmpty()) {
    group->setIcon(0, style()->standardIcon(QStyle::SP_MessageBoxWarning));
    group->setData(0, kProblemRole, problem);
  }
  group->setExpanded(true);
  return group;
}

QTreeWidgetItem* DrawingBookDialog::addDrawing(QTreeWidgetItem* group, const QJsonObject& drawing, bool own,
                                               const QString& source, bool chosen, const QString& problem) {
  auto* item = new QTreeWidgetItem(group);
  const QString name = jsonString(drawing, "name");
  item->setText(0, name);
  item->setData(0, kOwnRole, own);
  item->setData(0, kSourceRole, source);
  item->setData(0, kDrawingRole, jsonString(drawing, "drawing"));
  item->setData(0, kNameRole, name);
  item->setData(0, kDesignRole, group->data(0, kDesignRole));
  item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsUserCheckable);
  item->setCheckState(0, chosen ? Qt::Checked : Qt::Unchecked);
  const int sheets = drawing.value(QStringLiteral("sheets")).toInt();
  if (problem.isEmpty()) {
    item->setText(1, QString::number(sheets));
    const QString component = jsonString(drawing, "component_name");
    if (!component.isEmpty()) {
      item->setToolTip(0, tr("In the Drawings folder of %1").arg(component));
    }
  } else {
    item->setIcon(0, style()->standardIcon(QStyle::SP_MessageBoxWarning));
    item->setData(0, kProblemRole, problem);
    item->setToolTip(0, problem);
  }
  item->setTextAlignment(1, Qt::AlignRight | Qt::AlignVCenter);
  return item;
}

void DrawingBookDialog::build(const QJsonObject& book) {
  m_building = true;
  const QJsonArray sources = book.value(QStringLiteral("sources")).toArray();
  const QJsonArray entries = book.value(QStringLiteral("drawings")).toArray();
  // A design's row by its source's index; designs no longer linked by the
  // path the book keeps.
  QHash<int, QTreeWidgetItem*> groups;
  QHash<QString, QTreeWidgetItem*> unlinked;
  const auto sourceLabel = [&sources](int index) {
    const QJsonObject source = sources.at(index).toObject();
    return source.value(QStringLiteral("source")).isNull() ? tr("This design") : jsonString(source, "design");
  };
  const auto groupOf = [&](int index) {
    if (QTreeWidgetItem* known = groups.value(index)) {
      return known;
    }
    const QJsonObject source = sources.at(index).toObject();
    const bool own = source.value(QStringLiteral("source")).isNull();
    QStringList components;
    for (const QJsonValue& value : source.value(QStringLiteral("components")).toArray()) {
      components << value.toString();
    }
    const QString file = QDir::toNativeSeparators(jsonString(source, "file"));
    const QString tooltip = own ? tr("Drawings of this design")
                                : tr("%1, linked as %2").arg(file.isEmpty() ? jsonString(source, "design") : file,
                                                             components.join(QStringLiteral(", ")));
    const QString problem = jsonString(source, "problem");
    QTreeWidgetItem* group = addGroup(sourceLabel(index), tooltip, problem.isEmpty() ? QString() : modelMessage(problem));
    group->setData(0, kOwnRole, own);
    group->setData(0, kSourceRole, jsonString(source, "source"));
    groups.insert(index, group);
    return group;
  };
  // The book's drawings first, in its order, each design's row where its
  // first drawing comes.
  QHash<int, QSet<QString>> chosen;
  for (const QJsonValue& value : entries) {
    const QJsonObject entry = value.toObject();
    const QString problem = jsonString(entry, "problem");
    const bool own = entry.value(QStringLiteral("source")).isNull();
    const QString path = jsonString(entry, "source");
    QTreeWidgetItem* group = nullptr;
    const QJsonValue index = entry.value(QStringLiteral("source_index"));
    if (index.isDouble()) {
      group = groupOf(index.toInt());
      chosen[index.toInt()].insert(jsonString(entry, "drawing"));
    } else {
      group = unlinked.value(path);
      if (group == nullptr) {
        group = addGroup(jsonString(entry, "design"), QDir::toNativeSeparators(path),
                         problem.isEmpty() ? QString() : modelMessage(problem));
        group->setData(0, kSourceRole, path);
        unlinked.insert(path, group);
      }
    }
    addDrawing(group, entry, own, path, true, problem.isEmpty() ? QString() : modelMessage(problem));
  }
  // Then every design with what it offers besides: a design without
  // drawings is left out, unless its file cannot be read.
  for (int i = 0; i < sources.size(); ++i) {
    const QJsonObject source = sources.at(i).toObject();
    const QJsonArray drawings = source.value(QStringLiteral("drawings")).toArray();
    const bool problem = !jsonString(source, "problem").isEmpty();
    if (drawings.isEmpty() && !problem && !groups.contains(i)) {
      continue;
    }
    QTreeWidgetItem* group = groupOf(i);
    const bool own = source.value(QStringLiteral("source")).isNull();
    for (const QJsonValue& value : drawings) {
      const QJsonObject drawing = value.toObject();
      if (!chosen.value(i).contains(jsonString(drawing, "drawing"))) {
        addDrawing(group, drawing, own, jsonString(source, "source"), false, QString());
      }
    }
  }
  m_tree->expandAll();
  // The first drawing is the current row, so that the keys start there.
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    if (m_tree->topLevelItem(i)->childCount() > 0) {
      m_tree->setCurrentItem(m_tree->topLevelItem(i)->child(0));
      break;
    }
  }
  QStringList offered;
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    const QTreeWidgetItem* group = m_tree->topLevelItem(i);
    QStringList names;
    for (int j = 0; j < group->childCount(); ++j) {
      names << group->child(j)->data(0, kNameRole).toString();
    }
    offered << QStringLiteral("%1 [%2]").arg(group->data(0, kDesignRole).toString(), names.join(QStringLiteral(", ")));
  }
  qInfo().noquote() << QStringLiteral("Drawing Book opened: %1")
                           .arg(offered.isEmpty() ? QStringLiteral("no drawings") : offered.join(QStringLiteral(" | ")));
  m_building = false;
}

QList<QTreeWidgetItem*> DrawingBookDialog::chosenItems() const {
  QList<QTreeWidgetItem*> items;
  for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
    QTreeWidgetItem* group = m_tree->topLevelItem(i);
    for (int j = 0; j < group->childCount(); ++j) {
      QTreeWidgetItem* item = group->child(j);
      if (item->checkState(0) == Qt::Checked) {
        items << item;
      }
    }
  }
  return items;
}

QJsonObject DrawingBookDialog::command() const {
  QJsonArray drawings;
  for (const QTreeWidgetItem* item : chosenItems()) {
    QJsonObject drawing{{QStringLiteral("drawing"), item->data(0, kDrawingRole).toString()},
                        {QStringLiteral("name"), item->data(0, kNameRole).toString()}};
    if (!item->data(0, kOwnRole).toBool()) {
      drawing.insert(QStringLiteral("source"), item->data(0, kSourceRole).toString());
    }
    drawings.append(drawing);
  }
  return {{QStringLiteral("cmd"), QStringLiteral("set_drawing_book")},
          {QStringLiteral("title"), m_title->text().trimmed()},
          {QStringLiteral("cover"), m_cover->isChecked()},
          {QStringLiteral("drawings"), drawings}};
}

QStringList DrawingBookDialog::chosenNames() const {
  QStringList names;
  for (const QTreeWidgetItem* item : chosenItems()) {
    const QString design = item->data(0, kOwnRole).toBool() ? QStringLiteral("This design")
                                                            : item->data(0, kDesignRole).toString();
    names << QStringLiteral("%1/%2").arg(design, item->data(0, kNameRole).toString());
  }
  return names;
}

QStringList DrawingBookDialog::problems() const {
  QStringList problems;
  for (const QTreeWidgetItem* item : chosenItems()) {
    const QString problem = item->data(0, kProblemRole).toString();
    if (!problem.isEmpty()) {
      problems << problem;
    }
  }
  return problems;
}

void DrawingBookDialog::moveCurrent(int step) {
  QTreeWidgetItem* item = m_tree->currentItem();
  if (item == nullptr) {
    return;
  }
  QTreeWidgetItem* parent = item->parent();
  const int count = parent != nullptr ? parent->childCount() : m_tree->topLevelItemCount();
  const int at = parent != nullptr ? parent->indexOfChild(item) : m_tree->indexOfTopLevelItem(item);
  const int to = at + step;
  if (to < 0 || to >= count) {
    return;
  }
  m_building = true;
  const bool expanded = item->isExpanded();
  if (parent != nullptr) {
    parent->insertChild(to, parent->takeChild(at));
  } else {
    m_tree->insertTopLevelItem(to, m_tree->takeTopLevelItem(at));
  }
  item->setExpanded(expanded);
  m_building = false;
  m_tree->setCurrentItem(item);
  qInfo().noquote() << QStringLiteral("Drawing Book moved %1 %2")
                           .arg(isDrawing(item) ? QStringLiteral("%1/%2").arg(item->data(0, kDesignRole).toString(),
                                                                           item->data(0, kNameRole).toString())
                                                : item->data(0, kDesignRole).toString(),
                                step < 0 ? QStringLiteral("up") : QStringLiteral("down"));
  refresh();
}

void DrawingBookDialog::refresh() {
  // Move Up and Move Down within the current row's siblings.
  QTreeWidgetItem* item = m_tree->currentItem();
  int at = -1;
  int count = 0;
  if (item != nullptr) {
    QTreeWidgetItem* parent = item->parent();
    count = parent != nullptr ? parent->childCount() : m_tree->topLevelItemCount();
    at = parent != nullptr ? parent->indexOfChild(item) : m_tree->indexOfTopLevelItem(item);
  }
  m_up->setEnabled(at > 0);
  m_down->setEnabled(at >= 0 && at + 1 < count);

  const QList<QTreeWidgetItem*> chosen = chosenItems();
  const QStringList blocking = problems();
  QString message;
  QString printTip = tr("Keeps the book with the design and prints it");
  if (m_tree->topLevelItemCount() == 0) {
    message = tr("No drawings yet. Create a drawing in this design or in a design it links to put it in the book.");
    printTip = tr("The book has no drawing to print");
  } else if (chosen.isEmpty()) {
    printTip = tr("Check the drawings to print");
  } else if (!blocking.isEmpty()) {
    message = tr("These drawings cannot be printed: %1. Uncheck them, or correct their designs, to print the book.")
                  .arg(blocking.join(QStringLiteral("; ")));
    printTip = tr("Some chosen drawings cannot be printed");
  }
  m_message->setText(message);
  m_message->setVisible(!message.isEmpty());
  m_print->setEnabled(!chosen.isEmpty() && blocking.isEmpty());
  m_print->setToolTip(printTip);

  const QString line = QStringLiteral("Drawing Book: %1; cover %2; print %3")
                           .arg(chosen.isEmpty() ? QStringLiteral("none") : chosenNames().join(QStringLiteral(" | ")),
                                m_cover->isChecked() ? QStringLiteral("on") : QStringLiteral("off"),
                                m_print->isEnabled()
                                    ? QStringLiteral("enabled")
                                    : (blocking.isEmpty() ? QStringLiteral("disabled")
                                                          : QStringLiteral("disabled: %1")
                                                                .arg(blocking.join(QStringLiteral("; ")))));
  if (line != m_logged) {
    m_logged = line;
    qInfo().noquote() << line;
  }
}

} // namespace mitcad::drawings
