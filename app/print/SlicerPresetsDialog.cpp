// SPDX-License-Identifier: MIT
#include "SlicerPresetsDialog.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFont>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QItemSelectionModel>
#include <QJsonValue>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QModelIndex>
#include <QPainter>
#include <QPen>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QRectF>
#include <QStyle>
#include <QStyleOptionViewItem>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVariant>
#include <QVBoxLayout>
#include <QVector>
#include <QWidget>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "PrintKeys.hpp"
#include "PrintLibrary.hpp"

namespace mitcad {
namespace {

constexpr int kPathRole = Qt::UserRole;       // a preset row's file; empty on a group
constexpr int kKindRole = Qt::UserRole + 1;   // printer, process, material
constexpr int kChoiceRole = Qt::UserRole + 2; // a same-name row's copy or override; empty else
constexpr int kSystemRole = Qt::UserRole + 3; // a row of the slicer's own presets (mitcad#871)
// One filament's presets as variants (mitcad#1054):
constexpr int kFilamentRole = Qt::UserRole + 4; // a filament's row and its variant rows: the filament
constexpr int kPrintersRole = Qt::UserRole + 5; // a filament preset's printers [{printer, nozzle}]; a printer row's
constexpr int kNameRole = Qt::UserRole + 6;     // a preset row's preset name

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

// The preset rows of the list, in their order.
QVector<QTreeWidgetItem*> presetRows(QTreeWidget* list) {
  QVector<QTreeWidgetItem*> rows;
  for (QTreeWidgetItemIterator it(list); *it != nullptr; ++it) {
    if (!(*it)->data(0, kPathRole).toString().isEmpty()) {
      rows.append(*it);
    }
  }
  return rows;
}

// The rows of a kind (`material` filaments, `printer_nozzle`), in order.
QVector<QTreeWidgetItem*> rowsOf(QTreeWidget* list, const QString& kind) {
  QVector<QTreeWidgetItem*> rows;
  for (QTreeWidgetItemIterator it(list); *it != nullptr; ++it) {
    if ((*it)->data(0, kKindRole).toString() == kind && (*it)->data(0, kPathRole).toString().isEmpty()) {
      rows.append(*it);
    }
  }
  return rows;
}

// The rows with a same-name choice: presets', and filaments' of variants.
QVector<QTreeWidgetItem*> choiceRows(QTreeWidget* list) {
  QVector<QTreeWidgetItem*> rows;
  for (QTreeWidgetItemIterator it(list); *it != nullptr; ++it) {
    if (!(*it)->data(0, kChoiceRole).toString().isEmpty()) {
      rows.append(*it);
    }
  }
  return rows;
}

// A row's name in the logs: a variant's "<filament> › <row>".
QString rowName(const QTreeWidgetItem* row) {
  if (row->data(0, kKindRole).toString() == QStringLiteral("variant")) {
    return QStringLiteral("%1 › %2").arg(row->data(0, kFilamentRole).toString(), row->text(0));
  }
  return row->text(0);
}

// Whether a row is in sight of the filter: it and the rows above it shown.
bool shownRow(const QTreeWidgetItem* row) {
  for (const QTreeWidgetItem* at = row; at != nullptr; at = at->parent()) {
    if (at->isHidden()) {
      return false;
    }
  }
  return true;
}

// Whether a preset row comes in: ticked, and of a printer in use.
bool takenRow(const QTreeWidgetItem* row) { return row->checkState(0) == Qt::Checked && !row->isDisabled(); }

// A printer and nozzle as the rows name them: "Bench X, 0.4 mm nozzle".
QString printerLabel(const QJsonObject& place) {
  const QString printer = text(place, "printer").trimmed();
  const QJsonValue nozzle = place.value(QStringLiteral("nozzle"));
  if (nozzle.isNull() || nozzle.isUndefined()) {
    return QObject::tr("%1, any nozzle").arg(printer);
  }
  return QObject::tr("%1, %2 mm nozzle").arg(printer, printNumberText(nozzle.toDouble()));
}

// Whether a preset's printer and nozzle is a printer row's (a nozzle not
// known is any), as the model's choice takes it.
bool samePlace(const QJsonObject& preset, const QJsonObject& chosen) {
  if (text(preset, "printer").simplified().compare(text(chosen, "printer").simplified(), Qt::CaseInsensitive) != 0) {
    return false;
  }
  const QJsonValue a = preset.value(QStringLiteral("nozzle"));
  const QJsonValue b = chosen.value(QStringLiteral("nozzle"));
  if (!a.isDouble() || !b.isDouble()) {
    return true;
  }
  return std::abs(a.toDouble() - b.toDouble()) < 1e-6;
}

// The choices of a same-name row and of All same names, in their order.
void addChoices(QComboBox* box) {
  box->addItem(QObject::tr("Copy"), QStringLiteral("copy"));
  box->setItemData(0, QObject::tr("Imports it as a new item under the next free name"), Qt::ToolTipRole);
  box->addItem(QObject::tr("Override"), QStringLiteral("override"));
  box->setItemData(1, QObject::tr("Updates the existing item with its values"), Qt::ToolTipRole);
}

// The list: its current row framed while the keyboard is in it or in the
// row's box (mitcad#830 review). The rows are not selectable, so a
// same-name row keeps its warning colour; the frame marks where Space
// and Tab act.
class PresetList : public QTreeWidget {
public:
  using QTreeWidget::QTreeWidget;

protected:
  void keyPressEvent(QKeyEvent* event) override {
    // Cocoa's item-view style does not move a NoSelection cursor. This
    // checklist deliberately keeps selection off to preserve warnings,
    // but still promises a current row for Space and the row's choice.
    // Keep Qt's own visible-row/expansion navigation, publishing only that
    // cursor independently of the style's selection policy.
    if (selectionMode() == QAbstractItemView::NoSelection &&
        !(event->modifiers() & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier))) {
      CursorAction action;
      switch (event->key()) {
      case Qt::Key_Up: action = MoveUp; break;
      case Qt::Key_Down: action = MoveDown; break;
      case Qt::Key_Left: action = MoveLeft; break;
      case Qt::Key_Right: action = MoveRight; break;
      case Qt::Key_Home: action = MoveHome; break;
      case Qt::Key_End: action = MoveEnd; break;
      case Qt::Key_PageUp: action = MovePageUp; break;
      case Qt::Key_PageDown: action = MovePageDown; break;
      default: QTreeWidget::keyPressEvent(event); return;
      }
      const QModelIndex next = moveCursor(action, event->modifiers());
      if (next.isValid() && next.flags().testFlag(Qt::ItemIsEnabled)) {
        selectionModel()->setCurrentIndex(next, QItemSelectionModel::NoUpdate);
        scrollTo(next);
      }
      event->accept();
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

  // The name column at least as wide as the group titles (mitcad#871
  // review): they span both columns, so the tree's own hint leaves them
  // out and a closed group's long title was cut off.
  int sizeHintForColumn(int column) const override {
    int width = QTreeWidget::sizeHintForColumn(column);
    if (column != 0) {
      return width;
    }
    QStyleOptionViewItem option;
    initViewItemOption(&option);
    const int indent = rootIsDecorated() ? indentation() : 0;
    for (int i = 0; i < topLevelItemCount(); ++i) {
      const QTreeWidgetItem* group = topLevelItem(i);
      if (group->isHidden()) {
        continue;
      }
      const QModelIndex index = indexFromItem(group, 0);
      width = std::max(width, itemDelegateForIndex(index)->sizeHint(option, index).width() + indent);
    }
    return width;
  }

  void drawRow(QPainter* painter, const QStyleOptionViewItem& options, const QModelIndex& index) const override {
    QTreeWidget::drawRow(painter, options, index);
    const QModelIndex current = currentIndex();
    if (!current.isValid() || current.row() != index.row() || current.parent() != index.parent()) {
      return;
    }
    QWidget* box = itemWidget(itemFromIndex(current.siblingAtColumn(0)), 1);
    if (!hasFocus() && (box == nullptr || !box->hasFocus())) {
      return;
    }
    painter->save();
    QPen pen(palette().color(QPalette::Highlight));
    pen.setWidth(2);
    painter->setPen(pen);
    painter->setBrush(Qt::NoBrush);
    painter->drawRect(QRectF(options.rect).adjusted(1, 1, -1, -1));
    painter->restore();
  }
};

} // namespace

SlicerPresetsDialog::SlicerPresetsDialog(const QJsonArray& slicers, int index, const QString& into, QWidget* parent,
                                         const QJsonArray& merge)
    : QDialog(parent), m_slicers(slicers), m_index(index), m_into(into), m_merge(merge) {
  setObjectName(QStringLiteral("slicerPresetsDialog"));
  setWindowTitle(tr("Import Presets"));
  setWindowIcon(themeIcon(QStringLiteral("print-3d")));
  resize(560, 560);
  auto* layout = new QVBoxLayout(this);

  // The slicer: its name, a box when several were found.
  auto* top = new QHBoxLayout;
  m_slicerLabel = new QLabel;
  m_slicer = new QComboBox;
  m_slicer->setObjectName(QStringLiteral("slicerPresetsSlicer"));
  m_slicer->setAccessibleName(tr("Slicer"));
  for (const QJsonValue& value : slicers) {
    m_slicer->addItem(slicerTitle(value.toObject()));
  }
  const bool several = slicers.size() > 1;
  m_slicer->setVisible(several);
  if (several) {
    m_slicerLabel->setText(tr("Slicer:"));
  } else {
    QFont bold = m_slicerLabel->font();
    bold.setBold(true);
    m_slicerLabel->setFont(bold);
  }
  top->addWidget(m_slicerLabel);
  top->addWidget(m_slicer);
  top->addStretch(1);
  // All same names: every same-name row's choice at once; no value when
  // the rows differ.
  m_sameLabel = new QLabel(tr("All same names:"));
  m_sameAll = new QComboBox;
  m_sameAll->setObjectName(QStringLiteral("slicerPresetsSameName"));
  m_sameAll->setAccessibleName(tr("All same names"));
  addChoices(m_sameAll);
  m_sameLabel->setBuddy(m_sameAll);
  top->addWidget(m_sameLabel);
  top->addWidget(m_sameAll);
  layout->addLayout(top);

  // The filter (mitcad#871 review): the rows whose name has its text,
  // before the list in the Tab order.
  m_filter = new QLineEdit;
  m_filter->setPlaceholderText(tr("Filter presets"));
  m_filter->setAccessibleName(tr("Filter presets"));
  m_filter->setClearButtonEnabled(true);
  m_filter->installEventFilter(this);
  mac::styleSearchField(m_filter, tr("Shows the presets whose name contains the text"));
  layout->addWidget(m_filter);

  // The presets by kind, a box each. The name column as wide as its
  // names, so each row's choice sits beside its name.
  m_list = new PresetList;
  panel::setCompactIndentation(m_list, panel::Indent::Tree);
  m_list->setObjectName(QStringLiteral("slicerPresetsList"));
  m_list->setProperty(NativeCurrentRowNavigationProperty, true);
  m_list->setColumnCount(2);
  m_list->setHeaderHidden(true);
  m_list->setUniformRowHeights(true);
  m_list->setSelectionMode(QAbstractItemView::NoSelection);
  m_list->header()->setStretchLastSection(false);
  m_list->header()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  m_list->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
  m_list->setAccessibleName(tr("Presets"));
  m_list->installEventFilter(this);
  mac::styleListTree(m_list);
  layout->addWidget(m_list, 1);

  // The target's separate materials of one filament kept from before
  // (mitcad#1054): offered for a merge, unticked.
  m_mergeBox = new QCheckBox;
  m_mergeBox->setObjectName(QStringLiteral("slicerPresetsMerge"));
  QStringList offered;
  QStringList tooltip;
  for (const QJsonValue& value : std::as_const(m_merge)) {
    const QJsonObject group = value.toObject();
    QStringList names;
    for (const QJsonValue& item : group.value(QStringLiteral("items")).toArray()) {
      names << text(item.toObject(), "name");
    }
    offered << QStringLiteral("%1 (%2)").arg(text(group, "name"), names.join(QStringLiteral(", ")));
    tooltip << tr("%1: %2").arg(text(group, "name"), names.join(QStringLiteral(", ")));
  }
  const auto filaments = static_cast<int>(m_merge.size());
  m_mergeBox->setText(filaments == 1 ? tr("Merge the separate materials of 1 filament")
                                     : tr("Merge the separate materials of %1 filaments").arg(filaments));
  m_mergeBox->setToolTip(tr("Each filament becomes one material with a variant per printer and nozzle:\n%1")
                             .arg(tooltip.join(QLatin1Char('\n'))));
  m_mergeBox->setVisible(!m_merge.isEmpty());
  layout->addWidget(m_mergeBox);
  if (!m_merge.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Slicer presets merge offered: %1: %2")
                              .arg(filaments)
                              .arg(offered.join(QStringLiteral("; ")));
  }

  auto* buttons = new QDialogButtonBox;
  m_import = new QPushButton(tr("&Import"));
  m_import->setObjectName(QStringLiteral("slicerPresetsImport"));
  m_cancel = new QPushButton(tr("Cancel"));
  m_cancel->setObjectName(QStringLiteral("slicerPresetsCancel"));
  buttons->addButton(m_import, QDialogButtonBox::AcceptRole);
  buttons->addButton(m_cancel, QDialogButtonBox::RejectRole);
  m_import->setDefault(true);
  m_list->setProperty(NativeCurrentRowNextProperty, QVariant::fromValue(static_cast<QObject*>(m_import)));
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons, m_import);
  mac::styleDialog(this);

  connect(m_import, &QPushButton::clicked, this, &QDialog::accept);
  connect(m_cancel, &QPushButton::clicked, this, &QDialog::reject);
  connect(m_slicer, &QComboBox::activated, this, [this](int chosen) { showSlicer(chosen); });
  connect(m_sameAll, &QComboBox::activated, this, [this] {
    // Only the choices: which rows are ticked stays as it is.
    const QString choice = m_sameAll->currentData().toString();
    for (QTreeWidgetItem* row : choiceRows(m_list)) {
      setRowChoice(row, choice);
    }
    qDebug().noquote() << QStringLiteral("Slicer presets same name: %1").arg(choice);
    sameChoicesChanged();
  });
  connect(m_mergeBox, &QCheckBox::toggled, this, [this](bool on) {
    qDebug().noquote() << QStringLiteral("Slicer presets merge: %1")
                              .arg(on ? QStringLiteral("ticked") : QStringLiteral("unticked"));
    ticksChanged();
  });
  connect(m_list, &QTreeWidget::itemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building) {
      return;
    }
    // A printer and nozzle in use (mitcad#1054): its variants with it.
    if (item->data(0, kKindRole).toString() == QStringLiteral("printer_nozzle")) {
      const bool inUse = item->checkState(0) == Qt::Checked;
      if (m_loggedTicks.value(item, !inUse) != inUse) {
        m_loggedTicks.insert(item, inUse);
        qDebug().noquote() << QStringLiteral("Slicer presets printer tick: %1 %2")
                                  .arg(item->text(0), inUse ? QStringLiteral("ticked") : QStringLiteral("unticked"));
        applyPrinters();
      }
      return;
    }
    // A preset row's tick, once per change (a choice's data changes the
    // row too).
    if (!item->data(0, kPathRole).toString().isEmpty()) {
      const bool ticked = item->checkState(0) == Qt::Checked;
      if (ticked && item->isDisabled()) {
        // The variant of a printer not in use stays unticked when its
        // filament's box ticks the others (mitcad#1054).
        m_building = true;
        item->setCheckState(0, Qt::Unchecked);
        m_building = false;
        return;
      }
      if (!shownRow(item) && m_loggedTicks.contains(item) && m_loggedTicks.value(item) != ticked) {
        // A group's box ticks only the rows the filter shows: a hidden
        // row keeps its tick (its change comes back here, unlogged).
        item->setCheckState(0, m_loggedTicks.value(item) ? Qt::Checked : Qt::Unchecked);
        return;
      }
      if (m_loggedTicks.value(item, !ticked) != ticked) {
        m_loggedTicks.insert(item, ticked);
        qDebug().noquote() << QStringLiteral("Slicer presets tick: %1 %2")
                                  .arg(rowName(item), ticked ? QStringLiteral("ticked") : QStringLiteral("unticked"));
      }
    }
    ticksChanged();
  });
  connect(m_list, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* current) {
    // The frame of the current row moves with it.
    m_list->viewport()->update();
    if (!m_building && current != nullptr) {
      qDebug().noquote() << QStringLiteral("Slicer presets current: %1").arg(rowName(current));
    }
  });
  connect(m_filter, &QLineEdit::textChanged, this, [this] { applyFilter(); });
  if (!slicers.isEmpty()) {
    showSlicer(index >= 0 && index < slicers.size() ? index : 0);
  }
  // The keyboard starts in the list, on its first preset.
  m_list->setFocus();
}

bool SlicerPresetsDialog::eventFilter(QObject* watched, QEvent* event) {
  const auto* key = event->type() == QEvent::KeyPress ? static_cast<QKeyEvent*>(event) : nullptr;
  const bool plain = key != nullptr && !key->modifiers().testFlag(Qt::ControlModifier) &&
                     !key->modifiers().testFlag(Qt::AltModifier);
  const bool backtab = plain && (key->key() == Qt::Key_Backtab ||
                                 (key->key() == Qt::Key_Tab && key->modifiers().testFlag(Qt::ShiftModifier)));
  const bool tab = plain && !backtab && key->key() == Qt::Key_Tab;
  if (watched == m_filter) {
    // Down or Enter in the filter goes to the rows it shows (Enter does
    // not import).
    if (plain && (key->key() == Qt::Key_Down || key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter)) {
      m_list->setFocus(Qt::TabFocusReason);
      return true;
    }
    return QDialog::eventFilter(watched, event);
  }
  if (watched == m_list) {
    if (event->type() == QEvent::FocusIn || event->type() == QEvent::FocusOut) {
      m_list->viewport()->update();
    }
    QTreeWidgetItem* current = m_list->currentItem();
    // Space ticks or unticks the current row (a group: all of it).
    if (plain && key->key() == Qt::Key_Space && current != nullptr &&
        current->flags().testFlag(Qt::ItemIsUserCheckable)) {
      current->setCheckState(0, current->checkState(0) == Qt::Checked ? Qt::Unchecked : Qt::Checked);
      return true;
    }
    // Tab goes to the current row's choice, when it has one.
    if (tab && current != nullptr) {
      if (QWidget* box = m_list->itemWidget(current, 1)) {
        box->setFocus(Qt::TabFocusReason);
        return true;
      }
    }
    return QDialog::eventFilter(watched, event);
  }
  QTreeWidgetItem* row = m_boxRows.value(watched, nullptr);
  if (row == nullptr) {
    return QDialog::eventFilter(watched, event);
  }
  if (event->type() == QEvent::FocusIn) {
    // A row's choice in use: its row the current one, framed.
    if (m_list->currentItem() != row) {
      m_list->setCurrentItem(row);
    }
    m_list->viewport()->update();
    qDebug().noquote() << QStringLiteral("Slicer presets focus: %1 choice").arg(row->text(0));
  } else if (event->type() == QEvent::FocusOut) {
    m_list->viewport()->update();
  } else if (backtab) {
    // Back to its row in the list.
    m_list->setFocus(Qt::BacktabFocusReason);
    return true;
  } else if (tab) {
    // On past the list, as Tab from the list.
    m_list->setFocus(Qt::TabFocusReason);
    focusNextPrevChild(true);
    return true;
  }
  return QDialog::eventFilter(watched, event);
}

QJsonObject SlicerPresetsDialog::shownSlicer() const {
  return m_index >= 0 && m_index < m_slicers.size() ? m_slicers.at(m_index).toObject() : QJsonObject();
}

QString SlicerPresetsDialog::slicerTitle(const QJsonObject& slicer) {
  const QString name = text(slicer, "name");
  return slicer.value(QStringLiteral("flatpak")).toBool() ? tr("%1 (Flatpak)").arg(name) : name;
}

QString SlicerPresetsDialog::slicerName() const { return text(shownSlicer(), "name"); }

QString SlicerPresetsDialog::sameTooltip(const QString& kind, const QJsonObject& same) const {
  const QString what = kind == QStringLiteral("printer")   ? tr("printer")
                       : kind == QStringLiteral("process") ? tr("process")
                                                           : tr("material");
  return tr("%1 has a %2 of this name").arg(placeName(text(same, "place")), what);
}

void SlicerPresetsDialog::addSameChoice(QTreeWidgetItem* row, const QString& kind, const QString& name,
                                        const QJsonObject& same) {
  const QString tooltip = sameTooltip(kind, same);
  const QString choices = tr("Copy imports it as a new item under the next free name; Override updates the "
                             "existing item with its values.");
  row->setForeground(0, warningColor(palette()));
  row->setToolTip(0, tooltip);
  // Not by colour alone: what it meets and its choice told to a screen
  // reader too.
  row->setData(0, Qt::AccessibleDescriptionRole, tr("%1. Tab reaches its choice: %2").arg(tooltip, choices));
  row->setData(0, kChoiceRole, QStringLiteral("copy"));
  row->setCheckState(0, Qt::Unchecked);
  m_loggedTicks.insert(row, false);
  // Its choice beside it, labelled "Same name:" (the cue without colour).
  auto* cell = new QWidget;
  auto* cellLayout = new QHBoxLayout(cell);
  cellLayout->setContentsMargins(0, 0, 0, 0);
  auto* sameLabel = new QLabel(tr("Same name:"));
  sameLabel->setObjectName(QStringLiteral("slicerPresetSameName.") + kind + QLatin1Char('.') + name);
  sameLabel->setToolTip(tooltip);
  auto* choice = new QComboBox;
  choice->setObjectName(QStringLiteral("slicerPresetChoice.") + kind + QLatin1Char('.') + name);
  choice->setAccessibleName(tr("%1: same name").arg(name));
  choice->setAccessibleDescription(tr("%1. %2").arg(tooltip, choices));
  choice->setToolTip(tooltip);
  sameLabel->setBuddy(choice);
  cellLayout->addWidget(sameLabel);
  cellLayout->addWidget(choice);
  // Out of the window's Tab order: Tab from its row reaches it.
  choice->setFocusPolicy(Qt::ClickFocus);
  cell->setFocusProxy(choice);
  addChoices(choice);
  m_list->setItemWidget(row, 1, cell);
  choice->installEventFilter(this);
  m_boxRows.insert(choice, row);
  connect(choice, &QComboBox::activated, this, [this, row, choice] {
    setRowChoice(row, choice->currentData().toString());
    qDebug().noquote() << QStringLiteral("Slicer presets choice: %1 %2")
                              .arg(rowName(row), row->data(0, kChoiceRole).toString());
    // A choice made is a preset to import: its row ticked (a filament's:
    // its variants of printers in use).
    row->setCheckState(0, Qt::Checked);
    sameChoicesChanged();
  });
}

void SlicerPresetsDialog::applyPrinters() {
  const QVector<QTreeWidgetItem*> printerRows = rowsOf(m_list, QStringLiteral("printer_nozzle"));
  if (printerRows.isEmpty()) {
    return;
  }
  QVector<QJsonObject> inUse;
  for (QTreeWidgetItem* row : printerRows) {
    if (row->checkState(0) == Qt::Checked) {
      inUse.append(row->data(0, kPrintersRole).toJsonObject());
    }
  }
  m_building = true;
  for (QTreeWidgetItem* row : presetRows(m_list)) {
    const QJsonArray places = row->data(0, kPrintersRole).toJsonArray();
    if (places.isEmpty()) {
      continue;
    }
    bool used = false;
    for (const QJsonValue& place : places) {
      for (const QJsonObject& chosen : std::as_const(inUse)) {
        used = used || samePlace(place.toObject(), chosen);
      }
    }
    if (!used && !row->isDisabled()) {
      row->setCheckState(0, Qt::Unchecked);
      row->setDisabled(true);
    } else if (used && row->isDisabled()) {
      row->setDisabled(false);
      row->setCheckState(0, Qt::Checked);
    }
    const bool ticked = row->checkState(0) == Qt::Checked;
    if (m_loggedTicks.value(row, !ticked) != ticked) {
      m_loggedTicks.insert(row, ticked);
      qDebug().noquote() << QStringLiteral("Slicer presets tick: %1 %2")
                                .arg(rowName(row), ticked ? QStringLiteral("ticked") : QStringLiteral("unticked"));
    }
  }
  m_building = false;
  ticksChanged();
}

void SlicerPresetsDialog::showSlicer(int index) {
  m_index = index;
  const QJsonObject slicer = m_slicers.at(index).toObject();
  if (m_slicers.size() <= 1) {
    m_slicerLabel->setText(slicerTitle(slicer));
  }
  m_building = true;
  m_boxRows.clear();
  m_loggedTicks.clear();
  m_openBeforeFilter.clear();
  m_filtering = false;
  m_list->clear();
  const QJsonArray presets = slicer.value(QStringLiteral("presets")).toArray();
  // The user's presets, then the slicer's own the user enabled there
  // (mitcad#871), in groups of their own, collapsed at first; each
  // group named by whose presets it holds.
  const QString slicerShown = text(slicer, "name");
  struct Group {
    QString kind;
    QString title;
    bool system;
  };
  const Group groups[] = {{QStringLiteral("printer"), tr("My printers"), false},
                          {QStringLiteral("process"), tr("My processes"), false},
                          {QStringLiteral("material"), tr("My filaments"), false},
                          {QStringLiteral("printer"), tr("%1 printers").arg(slicerShown), true},
                          {QStringLiteral("process"), tr("%1 processes").arg(slicerShown), true},
                          {QStringLiteral("material"), tr("%1 filaments").arg(slicerShown), true}};
  bool anySame = false;
  QTreeWidgetItem* first = nullptr;      // the current row at first
  QTreeWidgetItem* firstGroup = nullptr; // else the first group
  QVector<QTreeWidgetItem*> openGroups;
  // One filament's presets in a group (mitcad#1054): how many each
  // filament has there, by the user's or the slicer's own.
  QHash<QString, int> filamentPresets;
  for (const QJsonValue& value : presets) {
    const QJsonObject preset = value.toObject();
    if (text(preset, "kind") == QStringLiteral("material")) {
      const QString system = preset.value(QStringLiteral("system")).toBool() ? QStringLiteral("1") : QStringLiteral("0");
      ++filamentPresets[system + text(preset, "filament").toLower()];
    }
  }
  const auto ofVariants = [&filamentPresets](const QJsonObject& preset) {
    const QString system = preset.value(QStringLiteral("system")).toBool() ? QStringLiteral("1") : QStringLiteral("0");
    return text(preset, "kind") == QStringLiteral("material") &&
           filamentPresets.value(system + text(preset, "filament").toLower()) > 1;
  };
  // The printers and nozzles in use, first, when there are variants of
  // two or more to choose from: each ticked at first.
  const QJsonArray printers = slicer.value(QStringLiteral("printers")).toArray();
  bool anyVariants = false;
  for (const QJsonValue& value : presets) {
    anyVariants = anyVariants || ofVariants(value.toObject());
  }
  if (anyVariants && printers.size() > 1) {
    auto* inUse = new QTreeWidgetItem(m_list, {tr("Printers and nozzles in use")});
    inUse->setFlags(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable | Qt::ItemIsAutoTristate);
    inUse->setFirstColumnSpanned(true);
    inUse->setData(0, kKindRole, QStringLiteral("printers"));
    QFont bold = inUse->font(0);
    bold.setBold(true);
    inUse->setFont(0, bold);
    inUse->setToolTip(0, tr("Only the variants of the printers and nozzles ticked are imported"));
    for (const QJsonValue& value : printers) {
      const QJsonObject place = value.toObject();
      const QString label = printerLabel(place);
      auto* row = new QTreeWidgetItem(inUse, {label});
      row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable);
      row->setData(0, kKindRole, QStringLiteral("printer_nozzle"));
      row->setData(0, kPrintersRole, place);
      row->setData(0, NativeControlIdentifierRole, QStringLiteral("slicerPrinter.") + label);
      row->setCheckState(0, Qt::Checked);
      m_loggedTicks.insert(row, true);
    }
    openGroups.append(inUse);
  }
  for (const Group& kindGroup : groups) {
    const QString& kind = kindGroup.kind;
    QTreeWidgetItem* group = nullptr;
    QHash<QString, QTreeWidgetItem*> filamentRows; // a filament of variants' row, by its name
    for (const QJsonValue& value : presets) {
      const QJsonObject preset = value.toObject();
      const bool system = preset.value(QStringLiteral("system")).toBool();
      if (text(preset, "kind") != kind || system != kindGroup.system) {
        continue;
      }
      if (group == nullptr) {
        // The group's box ticks all or none of it.
        group = new QTreeWidgetItem(m_list, {kindGroup.title});
        group->setFlags(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable | Qt::ItemIsAutoTristate);
        group->setCheckState(0, Qt::Unchecked);
        group->setFirstColumnSpanned(true);
        QFont bold = group->font(0);
        bold.setBold(true);
        group->setFont(0, bold);
        if (!kindGroup.system) {
          openGroups.append(group);
        }
        if (firstGroup == nullptr) {
          firstGroup = group;
        }
      }
      const QString name = text(preset, "name");
      const QJsonObject same = preset.value(QStringLiteral("same")).toObject();
      const QJsonArray presetPrinters = preset.value(QStringLiteral("printers")).toArray();
      // One filament's presets (mitcad#1054): a row of the filament, one
      // material, and under it a row per preset by its printers.
      QTreeWidgetItem* holder = group;
      QString shown = name;
      const bool variant = ofVariants(preset);
      const QString filament = text(preset, "filament");
      if (variant) {
        QTreeWidgetItem*& filamentRow = filamentRows[filament.toLower()];
        if (filamentRow == nullptr) {
          filamentRow = new QTreeWidgetItem(group, {filament});
          filamentRow->setData(0, NativeControlIdentifierRole, QStringLiteral("slicerPreset.material.") + filament);
          filamentRow->setFlags(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable | Qt::ItemIsAutoTristate);
          filamentRow->setCheckState(0, Qt::Unchecked);
          filamentRow->setData(0, kKindRole, kind);
          filamentRow->setData(0, kFilamentRole, filament);
          filamentRow->setData(0, kSystemRole, system);
          filamentRow->setToolTip(0, tr("One material, a variant per printer and nozzle"));
          openGroups.append(filamentRow);
          if (first == nullptr && !system) {
            first = filamentRow;
          }
          if (!same.isEmpty()) {
            // Its name the target has: the filament's choice is its
            // presets'.
            anySame = true;
            if (!openGroups.contains(group)) {
              openGroups.append(group);
            }
            addSameChoice(filamentRow, kind, filament, same);
          }
        }
        holder = filamentRow;
        QStringList places;
        for (const QJsonValue& place : presetPrinters) {
          places << printerLabel(place.toObject());
        }
        if (!places.isEmpty()) {
          shown = places.join(QStringLiteral("; "));
        }
      }
      auto* row = new QTreeWidgetItem(holder, {shown});
      row->setData(0, NativeControlIdentifierRole,
                   variant ? QStringLiteral("slicerPreset.variant.%1.%2").arg(filament, shown)
                           : QStringLiteral("slicerPreset.") + kind + QLatin1Char('.') + name);
      row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable);
      row->setData(0, kPathRole, text(preset, "path"));
      row->setData(0, kKindRole, variant ? QStringLiteral("variant") : kind);
      row->setData(0, kSystemRole, system);
      row->setData(0, kNameRole, name);
      if (!presetPrinters.isEmpty()) {
        row->setData(0, kPrintersRole, presetPrinters);
      }
      if (variant) {
        row->setData(0, kFilamentRole, filament);
        row->setToolTip(0, name);
      }
      if (first == nullptr && !system) {
        first = row;
      }
      if (same.isEmpty() || variant) {
        // New ones of the user's are ticked at first; the slicer's own
        // not (a user preset mostly inherits from them already). A
        // variant whose filament's name the target has is not.
        const bool ticked = !system && same.isEmpty();
        row->setCheckState(0, ticked ? Qt::Checked : Qt::Unchecked);
        m_loggedTicks.insert(row, ticked);
        continue;
      }
      // A name the target has: the warning colour, what it meets as the
      // tooltip, unticked, Copy at first (it loses nothing). Its group
      // open, so that All same names acts on rows in sight (mitcad#871
      // review).
      anySame = true;
      if (!openGroups.contains(group)) {
        openGroups.append(group);
      }
      addSameChoice(row, kind, name, same);
    }
  }
  // The user's groups open, the slicer's own closed (they can be long)
  // unless they have a same-name row; a filament's variants in sight.
  for (QTreeWidgetItem* group : std::as_const(openGroups)) {
    group->setExpanded(true);
  }
  m_sameLabel->setVisible(anySame);
  m_sameAll->setVisible(anySame);
  m_loggedSameAll.clear();
  if (first != nullptr) {
    m_list->setCurrentItem(first);
  } else if (firstGroup != nullptr) {
    m_list->setCurrentItem(firstGroup);
  }
  m_building = false;
  qDebug().noquote() << QStringLiteral("Slicer presets opened: %1, %2 presets")
                            .arg(slicerTitle(slicer))
                            .arg(presets.size());
  m_loggedTicked = -1;
  ticksChanged();
  logRows();
  sameChoicesChanged();
  // A filter typed before another slicer was chosen applies to its rows.
  if (!m_filter->text().trimmed().isEmpty()) {
    applyFilter();
  }
  scheduleLogPlaces();
}

void SlicerPresetsDialog::applyFilter() {
  const QString needle = m_filter->text().trimmed();
  const bool filtering = !needle.isEmpty();
  if (filtering && !m_filtering) {
    // Which groups were open, for when the filter is cleared.
    m_openBeforeFilter.clear();
    for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
      QTreeWidgetItem* group = m_list->topLevelItem(i);
      m_openBeforeFilter.insert(group, group->isExpanded());
    }
  }
  // The rows whose name has the text; a group without one hidden, one
  // with them open. Hidden rows keep their ticks.
  int shown = 0;
  for (int i = 0; i < m_list->topLevelItemCount(); ++i) {
    QTreeWidgetItem* group = m_list->topLevelItem(i);
    int matches = 0;
    for (int j = 0; j < group->childCount(); ++j) {
      QTreeWidgetItem* row = group->child(j);
      const bool match = !filtering || row->text(0).contains(needle, Qt::CaseInsensitive);
      row->setHidden(!match);
      matches += match ? 1 : 0;
    }
    group->setHidden(matches == 0);
    if (filtering && matches > 0) {
      group->setExpanded(true);
    } else if (!filtering && m_filtering) {
      group->setExpanded(m_openBeforeFilter.value(group, group->isExpanded()));
    }
    shown += matches;
  }
  m_filtering = filtering;
  // The current row one in sight: the first row shown when it was
  // hidden, its group when that closed again.
  QTreeWidgetItem* current = m_list->currentItem();
  QTreeWidgetItem* owner = current != nullptr ? current->parent() : nullptr;
  if (current == nullptr || !shownRow(current)) {
    for (QTreeWidgetItem* row : presetRows(m_list)) {
      if (shownRow(row)) {
        m_list->setCurrentItem(row);
        break;
      }
    }
  } else if (owner != nullptr && !owner->isExpanded()) {
    m_list->setCurrentItem(owner);
  }
  if (filtering) {
    qDebug().noquote() << QStringLiteral("Slicer presets filter: %1, %2 shown").arg(needle).arg(shown);
  } else {
    qDebug().noquote() << QStringLiteral("Slicer presets filter cleared, %1 shown").arg(shown);
  }
  scheduleLogPlaces();
}

void SlicerPresetsDialog::sameChoicesChanged() {
  // The rows' one choice when they agree; no value when they differ.
  QString shared;
  bool differ = false;
  for (QTreeWidgetItem* row : choiceRows(m_list)) {
    const QString choice = row->data(0, kChoiceRole).toString();
    if (shared.isEmpty()) {
      shared = choice;
    } else if (choice != shared) {
      differ = true;
    }
  }
  const QString shown = differ || shared.isEmpty() ? QString() : shared;
  m_sameAll->setCurrentIndex(shown.isEmpty() ? -1 : m_sameAll->findData(shown));
  if (m_sameAll->isVisibleTo(this) && shown != m_loggedSameAll) {
    m_loggedSameAll = shown;
    qDebug().noquote() << QStringLiteral("Slicer presets all same names: %1")
                              .arg(shown.isEmpty() ? QStringLiteral("none") : shown);
  }
}

QStringList SlicerPresetsDialog::tickedNames() const {
  QStringList names;
  for (QTreeWidgetItem* row : presetRows(m_list)) {
    if (takenRow(row)) {
      names << row->data(0, kNameRole).toString();
    }
  }
  return names;
}

bool SlicerPresetsDialog::mergeChosen() const { return m_mergeBox->isVisibleTo(this) && m_mergeBox->isChecked(); }

void SlicerPresetsDialog::setRowChoice(QTreeWidgetItem* row, const QString& choice) {
  row->setData(0, kChoiceRole, choice);
  QWidget* cell = m_list->itemWidget(row, 1);
  if (auto* box = cell != nullptr ? cell->findChild<QComboBox*>() : nullptr) {
    const int at = box->findData(choice);
    if (at >= 0 && at != box->currentIndex()) {
      box->setCurrentIndex(at);
    }
  }
}

int SlicerPresetsDialog::tickedCount() const {
  int ticked = 0;
  for (QTreeWidgetItem* row : presetRows(m_list)) {
    if (takenRow(row)) {
      ++ticked;
    }
  }
  return ticked;
}

void SlicerPresetsDialog::ticksChanged() {
  const int ticked = tickedCount();
  // A merge of the target's separate materials alone is work too
  // (mitcad#1054).
  const bool merge = mergeChosen();
  m_import->setEnabled(ticked > 0 || merge);
  // The button says how many come in, those in closed groups or hidden by
  // the filter too (mitcad#871 review).
  m_import->setText(ticked == 0   ? (merge ? tr("&Merge") : tr("&Import"))
                    : ticked == 1 ? tr("&Import 1 Preset")
                                  : tr("&Import %1 Presets").arg(ticked));
  if (ticked != m_loggedTicked) {
    m_loggedTicked = ticked;
    QString button = m_import->text();
    button.remove(QLatin1Char('&'));
    qDebug().noquote() << QStringLiteral("Slicer presets ticked: %1, %2").arg(QString::number(ticked), button);
    // The button's width changed with its text.
    scheduleLogPlaces();
  }
}

QJsonObject SlicerPresetsDialog::command() const {
  const QJsonObject slicer = shownSlicer();
  QJsonArray presets;
  for (QTreeWidgetItem* row : presetRows(m_list)) {
    if (!takenRow(row)) {
      continue;
    }
    QJsonObject preset{{QStringLiteral("path"), row->data(0, kPathRole).toString()}};
    // A variant's choice is its filament's (mitcad#1054).
    QString choice = row->data(0, kChoiceRole).toString();
    if (choice.isEmpty() && row->data(0, kKindRole).toString() == QStringLiteral("variant") &&
        row->parent() != nullptr) {
      choice = row->parent()->data(0, kChoiceRole).toString();
    }
    if (!choice.isEmpty()) {
      preset.insert(QStringLiteral("same_name"), choice);
    }
    presets.append(preset);
  }
  QJsonObject out{{QStringLiteral("cmd"), QStringLiteral("import_slicer_presets")},
                      {QStringLiteral("dialect"), slicer.value(QStringLiteral("dialect"))},
                      {QStringLiteral("folder"), slicer.value(QStringLiteral("folder"))},
                      {QStringLiteral("presets"), presets}};
  // The printers and nozzles in use, when the list asks (mitcad#1054).
  const QVector<QTreeWidgetItem*> printerRows = rowsOf(m_list, QStringLiteral("printer_nozzle"));
  if (!printerRows.isEmpty()) {
    QJsonArray inUse;
    for (QTreeWidgetItem* row : printerRows) {
      if (row->checkState(0) == Qt::Checked) {
        inUse.append(row->data(0, kPrintersRole).toJsonObject());
      }
    }
    out.insert(QStringLiteral("printers"), inUse);
  }
  if (mergeChosen()) {
    out.insert(QStringLiteral("merge_separate"), true);
  }
  return out;
}

void SlicerPresetsDialog::logRows() {
  // The printers and nozzles in use (mitcad#1054).
  for (QTreeWidgetItem* row : rowsOf(m_list, QStringLiteral("printer_nozzle"))) {
    qDebug().noquote() << QStringLiteral("Slicer presets printer: %1, %2")
                              .arg(row->text(0), row->checkState(0) == Qt::Checked ? QStringLiteral("ticked")
                                                                                     : QStringLiteral("unticked"));
  }
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    const bool preset = !row->data(0, kPathRole).toString().isEmpty();
    // A filament of variants' own row (mitcad#1054): its count of them.
    const bool filament = !preset && row->data(0, kKindRole).toString() == QStringLiteral("material");
    if (!preset && !filament) {
      continue;
    }
    const QString choice = row->data(0, kChoiceRole).toString();
    const QString ticked = filament ? tr("%1 variants").arg(row->childCount())
                           : row->checkState(0) == Qt::Checked ? QStringLiteral("ticked")
                                                               : QStringLiteral("unticked");
    const QString system = row->data(0, kSystemRole).toBool() ? QStringLiteral(", system") : QString();
    qDebug().noquote() << QStringLiteral("Slicer presets row: %1 %2, %3%4%5")
                              .arg(row->data(0, kKindRole).toString(), rowName(row), ticked, system,
                                   choice.isEmpty() ? QString()
                                                    : QStringLiteral(", same name: %1, %2").arg(row->toolTip(0), choice));
  }
}

void SlicerPresetsDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_loggedPlaces.clear();
  scheduleLogPlaces();
}

void SlicerPresetsDialog::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void SlicerPresetsDialog::logPlaces() {
  if (!isVisible()) {
    return;
  }
  // In the main window's coordinates, as the other logged places: the
  // window this one's chain of parents starts from.
  QWidget* top = this;
  while (top->window()->parentWidget() != nullptr) {
    top = top->window()->parentWidget();
  }
  const QPoint origin = top->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  const std::pair<const char*, QWidget*> widgets[] = {{"slicer", m_slicer},
                                                      {"same_all", m_sameAll},
                                                      {"merge", m_mergeBox},
                                                      {"import", m_import},
                                                      {"cancel", m_cancel}};
  for (const auto& [name, widget] : widgets) {
    if (widget->isVisibleTo(this)) {
      // A check box spans the row: its box, not the row's middle, which
      // lies past its text and takes no click (mitcad#1054).
      const QPoint point =
          widget == m_mergeBox
              ? QPoint(widget->style()->pixelMetric(QStyle::PM_IndicatorWidth, nullptr, widget) / 2 + 2,
                       widget->rect().center().y())
              : widget->rect().center();
      lines << QStringLiteral("Slicer presets %1 at %2").arg(QLatin1String(name), at(widget, point));
    }
  }
  // A row's box: at the start of its rect; a filament of variants' too,
  // and a printer's in use (mitcad#1054).
  const int box = m_list->style()->pixelMetric(QStyle::PM_IndicatorWidth, nullptr, m_list) / 2 + 3;
  for (QTreeWidgetItemIterator it(m_list); *it != nullptr; ++it) {
    QTreeWidgetItem* row = *it;
    const QString kind = row->data(0, kKindRole).toString();
    const bool preset = !row->data(0, kPathRole).toString().isEmpty();
    const bool printer = kind == QStringLiteral("printer_nozzle");
    if (!preset && !printer && kind != QStringLiteral("material")) {
      continue;
    }
    const QRect rect = m_list->visualItemRect(row);
    if (rect.isValid() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Slicer presets %1 %2 at %3")
                   .arg(printer ? QStringLiteral("printer") : QStringLiteral("item"), rowName(row),
                        at(m_list->viewport(), QPoint(rect.left() + box, rect.center().y())));
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
