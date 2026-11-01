// SPDX-License-Identifier: MIT
#include "PurgeVolumesDialog.hpp"

#include <exception>
#include <utility>

#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QBrush>
#include <QColor>
#include <QFont>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QJsonValue>
#include <QLabel>
#include <QLineEdit>
#include <QModelIndex>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QPoint>
#include <QPushButton>
#include <QRect>
#include <QScreen>
#include <QSize>
#include <QSignalBlocker>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QTableWidget>
#include <QTableWidgetItem>
#include <QVBoxLayout>
#include <QVariant>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "PrintKeys.hpp"

namespace mitcad {
namespace {

// A cell's role that says whether the design sets its volume by hand.
constexpr int kEditedRole = Qt::UserRole + 1;

// A typed volume as a number: the number before the unit, `,` or `.` as the
// decimal point; none when it is not one.
bool parseVolume(QString text, double* volume) {
  text = text.trimmed();
  text.remove(QStringLiteral("mm³"));
  text.remove(QStringLiteral("mm3"));
  text = text.trimmed();
  text.replace(QLatin1Char(','), QLatin1Char('.'));
  bool ok = false;
  const double value = text.toDouble(&ok);
  if (ok) {
    *volume = value;
  }
  return ok;
}

// A cell's volume as data, for both frontends (mitcad#1078): the number
// is its edit role, its text with the unit its display role ("140 mm³").
// A text set (a typed one) that is a volume becomes the number, so an
// unchanged one changes nothing; another text stays as it is, and the
// window refuses it.
class VolumeItem : public QTableWidgetItem {
public:
  QVariant data(int role) const override {
    const QVariant value = QTableWidgetItem::data(role);
    if (role == Qt::DisplayRole && value.typeId() == QMetaType::Double) {
      return QStringLiteral("%1 mm³").arg(printNumberText(value.toDouble()));
    }
    return value;
  }

  void setData(int role, const QVariant& value) override {
    double volume = 0.0;
    if ((role == Qt::EditRole || role == Qt::DisplayRole) && value.typeId() == QMetaType::QString &&
        parseVolume(value.toString(), &volume)) {
      QTableWidgetItem::setData(role, volume);
      return;
    }
    QTableWidgetItem::setData(role, value);
  }
};

// The Qt editor of a cell: the number alone, in a frameless line.
class VolumeDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  QWidget* createEditor(QWidget* parent, const QStyleOptionViewItem&, const QModelIndex&) const override {
    auto* editor = new QLineEdit(parent);
    editor->setFrame(false);
    return editor;
  }

  void setEditorData(QWidget* editor, const QModelIndex& index) const override {
    auto* line = static_cast<QLineEdit*>(editor);
    line->setText(printNumberText(index.data(Qt::EditRole).toDouble()));
    line->selectAll();
  }

  void setModelData(QWidget* editor, QAbstractItemModel* model, const QModelIndex& index) const override {
    // The text as typed, as the AppKit table commits it: the cell and the
    // window take it from there.
    model->setData(index, static_cast<QLineEdit*>(editor)->text(), Qt::EditRole);
  }
};

// A small square of a material's colour for the headers.
QIcon swatch(const QColor& color) {
  QPixmap pixmap(12, 12);
  pixmap.fill(Qt::transparent);
  if (color.isValid()) {
    QPainter painter(&pixmap);
    painter.setPen(QColor(0, 0, 0, 90));
    painter.setBrush(color);
    painter.drawRect(QRect(0, 0, 11, 11));
  }
  return QIcon(pixmap);
}

} // namespace

PurgeVolumesDialog::PurgeVolumesDialog(DocumentHost& host, QWidget* parent) : QDialog(parent), m_host(host) {
  setWindowTitle(tr("Purge Volumes"));
  setWindowIcon(themeIcon(QStringLiteral("print-3d")));
  auto* layout = new QVBoxLayout(this);
  m_grid = new QTableWidget;
  m_grid->setObjectName(QStringLiteral("purgeVolumesTable"));
  // The rows' "From …" labels in the AppKit table too (mitcad#1078).
  m_grid->setProperty(NativeRowHeaderProperty, true);
  m_grid->setItemDelegate(new VolumeDelegate(m_grid));
  m_grid->setEditTriggers(QAbstractItemView::DoubleClicked | QAbstractItemView::EditKeyPressed |
                          QAbstractItemView::AnyKeyPressed);
  m_grid->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_grid->horizontalHeader()->setSectionResizeMode(QHeaderView::ResizeToContents);
  m_grid->verticalHeader()->setSectionResizeMode(QHeaderView::ResizeToContents);
  m_grid->setCornerButtonEnabled(false);
  layout->addWidget(m_grid, 1);
  m_message = new QLabel;
  m_message->setObjectName(QStringLiteral("purgeVolumesMessage"));
  m_message->setWordWrap(true);
  setErrorStyleSheet(m_message);
  m_message->hide();
  layout->addWidget(m_message);
  auto* buttons = new QHBoxLayout;
  m_reset = new QPushButton(tr("Reset"));
  m_reset->setEnabled(false);
  buttons->addWidget(m_reset);
  buttons->addStretch(1);
  m_close = new QPushButton(tr("Close"));
  buttons->addWidget(m_close);
  layout->addLayout(buttons);
  // Enter in a cell saves it: no button takes Enter.
  mac::auxiliaryButton(m_reset);
  mac::auxiliaryButton(m_close);
  mac::styleDialog(this);
  connect(m_grid, &QTableWidget::cellChanged, this, &PurgeVolumesDialog::cellEdited);
  connect(m_grid, &QTableWidget::itemSelectionChanged, this, &PurgeVolumesDialog::updateReset);
  connect(m_reset, &QPushButton::clicked, this, &PurgeVolumesDialog::resetSelected);
  connect(m_close, &QPushButton::clicked, this, &QDialog::reject);
}

PurgeVolumesDialog::~PurgeVolumesDialog() = default;

void PurgeVolumesDialog::setBodies(const QStringList& bodies) {
  m_bodies = bodies;
  m_loggedCells.clear();
  refresh();
  QStringList names;
  for (int slot = 0; slot < m_grid->rowCount(); ++slot) {
    names << slotName(slot);
  }
  qDebug().noquote() << QStringLiteral("Purge volumes opened: %1").arg(names.join(QStringLiteral(", ")));
}

QString PurgeVolumesDialog::slotName(int slot) const {
  return m_table.value(QStringLiteral("slots")).toArray().at(slot).toObject().value(QStringLiteral("name")).toString();
}

QString PurgeVolumesDialog::slotMaterial(int slot) const {
  return m_table.value(QStringLiteral("slots")).toArray().at(slot).toObject().value(QStringLiteral("material")).toString();
}

void PurgeVolumesDialog::refresh() {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("purge_volumes")}};
  query.insert(QStringLiteral("bodies"), QJsonArray::fromStringList(m_bodies));
  try {
    m_table = m_host.model().queryObject(query);
  } catch (const std::exception& e) {
    // A body printed is gone: the table of the design's bodies.
    try {
      m_table = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("purge_volumes")}});
    } catch (const std::exception&) {
      setMessage(QString::fromUtf8(e.what()), true);
      return;
    }
  }
  const QJsonArray filaments = m_table.value(QStringLiteral("slots")).toArray();
  const QJsonArray volumes = m_table.value(QStringLiteral("volumes")).toArray();
  const int n = static_cast<int>(filaments.size());
  const int currentRow = m_grid->currentRow();
  const int currentColumn = m_grid->currentColumn();
  m_filling = true;
  {
    const QSignalBlocker block(m_grid);
    m_grid->clear();
    m_grid->setRowCount(n);
    m_grid->setColumnCount(n);
    QColor tint = palette().color(QPalette::Highlight);
    tint.setAlpha(40);
    QStringList cells;
    for (int i = 0; i < n; ++i) {
      const QJsonObject slot = filaments.at(i).toObject();
      const QIcon icon = swatch(QColor(slot.value(QStringLiteral("color")).toString()));
      auto* from = new QTableWidgetItem(icon, tr("From %1").arg(slotName(i)));
      // The rows' labels in the AppKit table's leading column (mitcad#1078).
      from->setData(NativeControlIdentifierRole, QStringLiteral("purgeVolumes.from.%1").arg(i));
      m_grid->setVerticalHeaderItem(i, from);
      m_grid->setHorizontalHeaderItem(i, new QTableWidgetItem(icon, tr("To %1").arg(slotName(i))));
      const QJsonArray row = volumes.at(i).toArray();
      for (int j = 0; j < n; ++j) {
        QTableWidgetItem* item = i == j ? new QTableWidgetItem : new VolumeItem;
        item->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        item->setData(NativeControlIdentifierRole, QStringLiteral("purgeVolumes.%1.%2").arg(i).arg(j));
        if (i == j) {
          // Nothing from a filament to itself.
          item->setData(Qt::DisplayRole, QStringLiteral("—"));
          item->setFlags(Qt::ItemIsEnabled);
          item->setForeground(palette().brush(QPalette::Disabled, QPalette::Text));
          m_grid->setItem(i, j, item);
          continue;
        }
        const QJsonObject cell = row.at(j).toObject();
        const double volume = cell.value(QStringLiteral("volume")).toDouble();
        const double computed = cell.value(QStringLiteral("computed")).toDouble();
        const bool edited = cell.value(QStringLiteral("edited")).toBool();
        item->setData(Qt::EditRole, volume);
        item->setData(kEditedRole, edited);
        // Two colours of one material (mitcad#1055): their colours' volume,
        // which no one sets by hand (the design's volumes are by material).
        const bool oneMaterial = slotMaterial(i) == slotMaterial(j);
        item->setFlags(oneMaterial ? Qt::ItemIsEnabled | Qt::ItemIsSelectable
                                   : Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
        if (edited) {
          QFont font = item->font();
          font.setBold(true);
          item->setFont(font);
          item->setBackground(tint);
          item->setToolTip(tr("Computed: %1 mm³").arg(printNumberText(computed)));
        }
        m_grid->setItem(i, j, item);
        cells << QStringLiteral("Purge volume %1 → %2 = %3 mm³%4")
                     .arg(slotName(i), slotName(j), printNumberText(volume),
                          edited ? QStringLiteral(" (set by hand)") : QString());
      }
    }
    if (currentRow >= 0 && currentRow < n && currentColumn >= 0 && currentColumn < n) {
      m_grid->setCurrentCell(currentRow, currentColumn);
    }
    const QString logged = cells.join(QLatin1Char('\n'));
    if (logged != m_loggedCells) {
      m_loggedCells = logged;
      for (const QString& line : std::as_const(cells)) {
        qDebug().noquote() << line;
      }
    }
  }
  m_filling = false;
  updateReset();
  fitGrid();
  adjustSize();
  scheduleLogPlaces();
}

// The whole table at once, as far as the screen allows: a table view's own
// size hint does not grow with its cells, and a cell left outside would
// have to be scrolled to.
void PurgeVolumesDialog::fitGrid() {
  m_grid->resizeColumnsToContents();
  m_grid->resizeRowsToContents();
  const int frame = 2 * m_grid->frameWidth();
  const int width = m_grid->verticalHeader()->sizeHint().width() + m_grid->horizontalHeader()->length() + frame;
  const int height = m_grid->horizontalHeader()->sizeHint().height() + m_grid->verticalHeader()->length() + frame;
  QSize most(width, height);
  if (const QScreen* shown = screen()) {
    most = most.boundedTo(shown->availableGeometry().size() * 3 / 4);
  }
  m_grid->setMinimumSize(most);
}

void PurgeVolumesDialog::cellEdited(int row, int column) {
  if (m_filling || row == column) {
    return;
  }
  QTableWidgetItem* item = m_grid->item(row, column);
  if (item == nullptr) {
    return;
  }
  // The number, or the text typed when it is none (VolumeItem), from
  // either frontend: one path for both.
  const QString value = item->data(Qt::EditRole).toString();
  const QString from = slotMaterial(row);
  const QString to = slotMaterial(column);
  const QString pair = QStringLiteral("%1 → %2").arg(slotName(row), slotName(column));
  // The model command rebuilds the table: not while the cell commits.
  TestSync::singleShot(0, this, [this, value, from, to, pair] {
    double volume = 0.0;
    if (!parseVolume(value, &volume)) {
      setMessage(tr("%1 is not a volume in mm³.").arg(value), true);
      qDebug().noquote() << QStringLiteral("Purge volumes refused: %1 = %2").arg(pair, value);
      refresh();
      return;
    }
    const QJsonArray volumes{
        QJsonObject{{QStringLiteral("from"), from}, {QStringLiteral("to"), to}, {QStringLiteral("volume"), volume}}};
    if (run(volumes)) {
      qDebug().noquote() << QStringLiteral("Purge volume set: %1 = %2 mm³").arg(pair, printNumberText(volume));
    }
    refresh();
  });
}

void PurgeVolumesDialog::resetSelected() {
  QJsonArray volumes;
  for (QTableWidgetItem* item : m_grid->selectedItems()) {
    if (item->data(kEditedRole).toBool()) {
      volumes.append(QJsonObject{{QStringLiteral("from"), slotMaterial(item->row())},
                                 {QStringLiteral("to"), slotMaterial(item->column())}});
    }
  }
  if (volumes.isEmpty()) {
    return;
  }
  if (run(volumes)) {
    qDebug().noquote() << QStringLiteral("Purge volumes reset: %1").arg(volumes.size());
  }
  refresh();
}

void PurgeVolumesDialog::updateReset() {
  bool edited = false;
  for (QTableWidgetItem* item : m_grid->selectedItems()) {
    edited = edited || item->data(kEditedRole).toBool();
  }
  m_reset->setEnabled(edited);
}

bool PurgeVolumesDialog::run(const QJsonArray& volumes) {
  if (!m_host.canChangePrintSettings()) {
    setMessage(m_host.changeBlockedReason(), true);
    return false;
  }
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_purge_volumes")},
                            {QStringLiteral("volumes"), volumes}};
  if (m_host.runModelCommand(command)) {
    setMessage(QString(), false);
    m_host.commandInputsChanged();
    return true;
  }
  if (!m_host.lastCancelled()) {
    setMessage(m_host.lastError(), true);
    qDebug().noquote() << QStringLiteral("Purge volumes refused: %1").arg(m_host.lastError());
  }
  return false;
}

void PurgeVolumesDialog::setMessage(const QString& message, bool error) {
  m_message->setText(message);
  m_message->setVisible(error && !message.isEmpty());
}

void PurgeVolumesDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_loggedPlaces.clear();
  scheduleLogPlaces();
}

void PurgeVolumesDialog::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  scheduleLogPlaces();
}

void PurgeVolumesDialog::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void PurgeVolumesDialog::logPlaces() {
  if (!isVisible() || parentWidget() == nullptr) {
    return;
  }
  // In the main window's coordinates, as the other logged places.
  const QPoint origin = parentWidget()->window()->mapToGlobal(QPoint(0, 0));
  const auto at = [&origin](QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  for (int i = 0; i < m_grid->rowCount(); ++i) {
    for (int j = 0; j < m_grid->columnCount(); ++j) {
      QTableWidgetItem* item = m_grid->item(i, j);
      const QRect rect = item != nullptr ? m_grid->visualItemRect(item) : QRect();
      if (i == j || !rect.isValid() || !m_grid->viewport()->rect().contains(rect.center())) {
        continue;
      }
      lines << QStringLiteral("Purge volumes cell %1.%2 at %3").arg(i).arg(j).arg(at(m_grid->viewport(), rect.center()));
    }
  }
  const std::pair<const char*, QWidget*> widgets[] = {{"reset", m_reset}, {"close", m_close}};
  for (const auto& [name, widget] : widgets) {
    lines << QStringLiteral("Purge volumes %1 at %2").arg(QString::fromLatin1(name), at(widget, widget->rect().center()));
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
