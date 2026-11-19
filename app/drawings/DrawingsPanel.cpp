// SPDX-License-Identifier: MIT
#include "DrawingsPanel.hpp"

#include <algorithm>
#include <utility>

#include <QAbstractItemDelegate>
#include <QFont>
#include <QHeaderView>
#include <QJsonObject>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QStringList>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QVBoxLayout>
#include <QtLogging>

#include "framework/PanelTree.hpp"
#include "framework/PlaceWatcher.hpp"
#include "framework/TestSync.hpp"

namespace mitcad::drawings {
namespace {

constexpr int kDrawingRole = Qt::UserRole;
constexpr int kSheetRole = Qt::UserRole + 1;
constexpr int kComponentRole = Qt::UserRole + 2;
constexpr int kNameRole = Qt::UserRole + 3;
constexpr int kComponentIdRole = Qt::UserRole + 4;

const QString kRoot = QStringLiteral("C0");

} // namespace

DrawingsPanel::DrawingsPanel(Hooks hooks, QWidget* parent) : QWidget(parent), m_hooks(std::move(hooks)) {
  setObjectName(QStringLiteral("drawingsPanel"));
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(6);
  m_list = new QTreeWidget;
  m_list->setObjectName(QStringLiteral("drawingsList"));
  m_list->setHeaderHidden(true);
  m_list->setColumnCount(1);
  m_list->setRootIsDecorated(true);
  // The drawings fold over their sheets: one level under headings.
  panel::setCompactIndentation(m_list, panel::Indent::GroupedList);
  m_list->setFrameShape(QFrame::NoFrame);
  m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_list->setTextElideMode(Qt::ElideRight);
  m_list->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->setContextMenuPolicy(Qt::CustomContextMenu);
  column->addWidget(m_list, 1);
  m_add = new QPushButton(tr("+ Drawing"));
  m_add->setObjectName(QStringLiteral("drawingsAdd"));
  m_add->setToolTip(tr("Adds a drawing of the design and opens its sheet"));
  m_add->setAutoDefault(false);
  column->addWidget(m_add, 0, Qt::AlignLeft);

  connect(m_list, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem* item, int) { chosen(item); });
  connect(m_list, &QTreeWidget::itemActivated, this, [this](QTreeWidgetItem* item, int) { chosen(item); });
  connect(m_list, &QTreeWidget::customContextMenuRequested, this, &DrawingsPanel::contextMenu);
  connect(m_list, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* item, int) {
    if (m_building || sheetOf(item) >= 0) {
      return;
    }
    const QString name = item->text(0).trimmed();
    if (!name.isEmpty() && name != item->data(0, kNameRole).toString() && m_hooks.rename) {
      // After the editor closed: the model's change makes the rows anew.
      const QString drawing = drawingOf(item);
      TestSync::singleShot(0, this, [this, drawing, name] { m_hooks.rename(drawing, name); });
    }
  });
  // An edit ended (taken or cancelled): the rows as the model has them,
  // a component's drawing named with its component again.
  connect(m_list->itemDelegate(), &QAbstractItemDelegate::closeEditor, this, [this] {
    TestSync::singleShot(0, this, [this] {
      const QSignalBlocker blocker(m_list);
      for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
        QTreeWidgetItem* item = m_list->topLevelItem(i);
        const QString component = item->data(0, kComponentRole).toString();
        const QString name = item->data(0, kNameRole).toString();
        item->setFlags(item->flags() & ~Qt::ItemIsEditable);
        item->setText(0, component.isEmpty() ? name : tr("%1 (%2)").arg(name, component));
      }
    });
  });
  connect(m_list, &QTreeWidget::itemExpanded, this, [this] { emit contentHeightChanged(); });
  connect(m_list, &QTreeWidget::itemCollapsed, this, [this] { emit contentHeightChanged(); });
  connect(m_list->verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { logPlaces(); });
  connect(m_add, &QPushButton::clicked, this, [this] {
    qDebug().noquote() << "Drawings card: Create Drawing";
    if (m_hooks.create) {
      m_hooks.create();
    }
  });
  new PlaceWatcher(m_list, [this] { logPlaces(); });
}

QString DrawingsPanel::drawingOf(const QTreeWidgetItem* item) {
  return item != nullptr ? item->data(0, kDrawingRole).toString() : QString();
}

int DrawingsPanel::sheetOf(const QTreeWidgetItem* item) {
  return item != nullptr ? item->data(0, kSheetRole).toInt() : -1;
}

void DrawingsPanel::setDrawings(const QJsonArray& drawings, const QString& open, int sheet) {
  m_open = open;
  m_sheet = sheet;
  m_building = true;
  {
    const QSignalBlocker blocker(m_list);
    // The scroll position stays across a model change.
    const int scrolled = m_list->verticalScrollBar()->value();
    m_list->clear();
    QTreeWidgetItem* current = nullptr;
    QStringList rows;
    for (const QJsonValue& value : drawings) {
      const QJsonObject drawing = value.toObject();
      const QString uid = drawing.value(QStringLiteral("drawing")).toString();
      const QString name = drawing.value(QStringLiteral("name")).toString();
      const QString componentId = drawing.value(QStringLiteral("component")).toString();
      const QString component = componentId.isEmpty() || componentId == kRoot
                                    ? QString()
                                    : drawing.value(QStringLiteral("component_name")).toString();
      const bool cut = drawing.value(QStringLiteral("kind")).toString() == QStringLiteral("cut_sheet");
      const QJsonArray sheets = drawing.value(QStringLiteral("sheets")).toArray();
      auto* item = new QTreeWidgetItem(m_list);
      item->setText(0, component.isEmpty() ? name : tr("%1 (%2)").arg(name, component));
      item->setData(0, kDrawingRole, uid);
      item->setData(0, kSheetRole, -1);
      item->setData(0, kComponentRole, component);
      item->setData(0, kComponentIdRole, componentId.isEmpty() ? kRoot : componentId);
      item->setData(0, kNameRole, name);
      item->setToolTip(0, cut ? tr("%1: a cut sheet").arg(name)
                              : tr("%1: %n sheet(s)", nullptr, static_cast<int>(sheets.size())).arg(name));
      const bool isOpen = uid == open;
      if (isOpen) {
        QFont bold = item->font(0);
        bold.setBold(true);
        item->setFont(0, bold);
        current = item;
      }
      // A cut sheet is one sheet: no rows under it.
      if (!cut) {
        for (int i = 0; i < sheets.size(); ++i) {
          auto* row = new QTreeWidgetItem(item);
          row->setText(0, sheets.at(i).toObject().value(QStringLiteral("name")).toString());
          row->setData(0, kDrawingRole, uid);
          row->setData(0, kSheetRole, i);
          if (isOpen && i == sheet) {
            current = row;
          }
        }
      }
      item->setExpanded(true);
      rows << (isOpen ? cut ? QStringLiteral("%1 (open)").arg(name)
                            : QStringLiteral("%1 (open, sheet %2 of %3)").arg(name).arg(sheet + 1).arg(sheets.size())
                      : name);
    }
    m_list->setCurrentItem(current);
    m_list->verticalScrollBar()->setValue(scrolled);
    const QString logged = QStringLiteral("Drawings card: %1")
                               .arg(rows.isEmpty() ? QStringLiteral("none") : rows.join(QStringLiteral(" | ")));
    if (logged != m_loggedRows) {
      m_loggedRows = logged;
      qDebug().noquote() << logged;
    }
  }
  m_building = false;
  emit contentHeightChanged();
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

int DrawingsPanel::contentHeight() const {
  int rows = 0;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    const QTreeWidgetItem* item = m_list->topLevelItem(i);
    rows += 1 + (item->isExpanded() ? item->childCount() : 0);
  }
  const int rowHeight = std::max(m_list->sizeHintForRow(0), fontMetrics().height() + 6);
  const int spacing = std::max(0, layout()->spacing());
  return std::max(1, rows) * rowHeight + 2 * m_list->frameWidth() + spacing + m_add->sizeHint().height();
}

void DrawingsPanel::chosen(QTreeWidgetItem* item) {
  if (item == nullptr || m_building) {
    return;
  }
  const QString drawing = drawingOf(item);
  const int sheet = sheetOf(item);
  qDebug().noquote() << (sheet < 0 ? QStringLiteral("Drawings card chose %1").arg(drawing)
                                   : QStringLiteral("Drawings card chose %1 sheet %2").arg(drawing).arg(sheet + 1));
  // After the click's handling: opening a drawing makes the rows anew.
  TestSync::singleShot(0, this, [this, drawing, sheet] {
    if (drawing != m_open && m_hooks.open) {
      m_hooks.open(drawing);
    }
    if (sheet >= 0 && m_hooks.showSheet) {
      m_hooks.showSheet(sheet);
    }
  });
}

void DrawingsPanel::contextMenu(const QPoint& at) {
  QTreeWidgetItem* item = m_list->itemAt(at);
  if (item != nullptr && item->parent() != nullptr) {
    item = item->parent(); // a sheet's row: its drawing's actions
  }
  if (item == nullptr) {
    // Beside the rows: a new drawing.
    if (m_hooks.create) {
      m_hooks.create();
    }
    return;
  }
  if (!m_hooks.menu) {
    return;
  }
  const QString drawing = drawingOf(item);
  const auto startRename = [this, drawing] {
    for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
      QTreeWidgetItem* row = m_list->topLevelItem(i);
      if (drawingOf(row) == drawing) {
        const QSignalBlocker blocker(m_list);
        // The name alone while it is edited.
        row->setText(0, row->data(0, kNameRole).toString());
        row->setFlags(row->flags() | Qt::ItemIsEditable);
        m_list->editItem(row, 0);
        qDebug().noquote() << QStringLiteral("Drawings card rename %1").arg(drawing);
        return;
      }
    }
  };
  m_hooks.menu(drawing, item->data(0, kComponentIdRole).toString(), item->data(0, kNameRole).toString(),
               m_list->viewport()->mapToGlobal(at), startRename);
}

void DrawingsPanel::logPlaces() {
  if (!isVisible() || window() == nullptr) {
    return;
  }
  QStringList lines;
  const QRect seen = m_list->viewport()->rect();
  const auto place = [this](const QPoint& point) {
    const QPoint at = m_list->viewport()->mapTo(window(), point);
    return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
  };
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* item = m_list->topLevelItem(i);
    const QString name = item->data(0, kNameRole).toString();
    const QRect rect = m_list->visualItemRect(item);
    if (!rect.isEmpty() && seen.contains(rect.center())) {
      lines << QStringLiteral("Drawings card %1 at %2").arg(name, place(rect.center()));
    }
    for (int j = 0; j < item->childCount(); ++j) {
      QTreeWidgetItem* row = item->child(j);
      const QRect sheetRect = m_list->visualItemRect(row);
      if (!sheetRect.isEmpty() && seen.contains(sheetRect.center())) {
        lines << QStringLiteral("Drawings card %1/%2 at %3").arg(name, row->text(0), place(sheetRect.center()));
      }
    }
  }
  const QPoint add = m_add->mapTo(window(), m_add->rect().center());
  lines << QStringLiteral("Drawings card add at %1,%2").arg(add.x()).arg(add.y());
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = logged;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad::drawings
