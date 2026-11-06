// SPDX-License-Identifier: MIT
#include "MakeBodyCard.hpp"

#include <algorithm>
#include <utility>

#include <QApplication>
#include <QComboBox>
#include <QDebug>
#include <QGridLayout>
#include <QLabel>
#include <QLayoutItem>
#include <QLineEdit>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QStandardItem>
#include <QStandardItemModel>
#include <QToolButton>
#include <QVector>
#include <QWidget>
#include <QtLogging>

#include "../framework/Command.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PickerComboBox.hpp"
#include "PrintLibrary.hpp"

namespace mitcad {
namespace {

// The value of a picker's last row, Printers… or Processes… (mitcad#1239).
QString managerRow() { return QStringLiteral("__manager"); }

// A level and its owner in words, as the Properties window's Source
// column names them: "body Body1", "component Root".
QString levelWords(const QString& level, const QString& owner) {
  if (level.isEmpty()) {
    return MakeBodyCard::tr("default");
  }
  return owner.isEmpty() ? level : QStringLiteral("%1 %2").arg(level, owner);
}

QString at(const QWidget* widget) {
  const QPoint point = GlassCard::mapToHost(widget, widget->rect().center());
  return QStringLiteral("%1,%2").arg(point.x()).arg(point.y());
}

// A drop-down as narrow as the card is (mitcad#1178): its width is not its
// longest item's, which made the section wider than the selection card and
// cut its frame on the right; a long name is elided.
void fitToCard(QComboBox* box) {
  box->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
  box->setMinimumContentsLength(6);
  box->setSizePolicy(QSizePolicy::Expanding, box->sizePolicy().verticalPolicy());
}

} // namespace

QString makeCardLevelIcon(const QString& level) {
  if (level == QLatin1String("body")) {
    return QStringLiteral("body");
  }
  if (level == QLatin1String("occurrence")) {
    return QStringLiteral("insert-component");
  }
  if (level == QLatin1String("component")) {
    return QStringLiteral("component");
  }
  return QStringLiteral("settings");
}

QString makeCardSummary(const MakeCardData& data) {
  QStringList own;
  for (const MakeCardValue& value : data.values) {
    own << QStringLiteral("%1 = %2 (%3)").arg(value.name, value.text, levelWords(value.level, value.owner));
  }
  return QStringLiteral("%1: material %2 (%3); printer %4 (%5); process %6 (%7); own: %8")
      .arg(data.body, data.material,
           data.materialLevel == QLatin1String("component") || data.materialLevel == QLatin1String("body")
               ? data.materialLevel
               : QStringLiteral("default"),
           data.printer, levelWords(data.printerLevel, data.printerOwner), data.process,
           levelWords(data.processLevel, data.processOwner),
           own.isEmpty() ? QStringLiteral("none") : own.join(QStringLiteral("; ")));
}

QString makeCardChoiceList(const QVector<ProfilePickerRow>& rows) {
  QStringList groups;
  QString group;
  QStringList names;
  const auto closeGroup = [&groups, &group, &names] {
    if (!names.isEmpty()) {
      groups << (group.isEmpty() ? names.join(QStringLiteral(", "))
                                 : QStringLiteral("%1: %2").arg(group, names.join(QStringLiteral(", "))));
    }
    names.clear();
  };
  for (int i = 0; i < rows.size(); ++i) {
    if (i == 0 || rows.at(i).group != group) {
      closeGroup();
      group = rows.at(i).group;
    }
    names << rows.at(i).name;
  }
  closeGroup();
  return groups.join(QStringLiteral(" | "));
}

MakeBodyCard::MakeBodyCard(QWidget* parent) : QGroupBox(tr("Make"), parent) {
  setObjectName(QStringLiteral("makeBodyCard"));
  m_grid = new QGridLayout(this);
  m_grid->setContentsMargins(6, 6, 6, 6);
  m_grid->setHorizontalSpacing(6);
  m_grid->setVerticalSpacing(4);
  m_grid->setColumnStretch(2, 1);
  setVisible(false);
}

bool MakeBodyCard::setData(const MakeCardData& data) {
  if (m_hasData && data == m_data) {
    return false;
  }
  // A pending setting stays with its body until it has a value.
  if (!m_pending.key.isEmpty()) {
    const bool set = std::any_of(data.values.cbegin(), data.values.cend(),
                                 [this](const MakeCardValue& value) { return value.key == m_pending.key; });
    if (set || data.body != m_data.body) {
      m_pending = MakeCardValue();
    }
  }
  m_data = data;
  m_hasData = true;
  rebuild();
  return true;
}

void MakeBodyCard::addPending(const MakeCardValue& value) {
  m_pending = value;
  m_pending.level = QStringLiteral("body");
  m_pending.owner = m_data.body;
  rebuild();
}

bool MakeBodyCard::focusValue(const QString& key) {
  QLineEdit* field = m_fields.value(key);
  if (field == nullptr) {
    return false;
  }
  field->setFocus(Qt::OtherFocusReason);
  field->selectAll();
  return true;
}

QLabel* MakeBodyCard::levelIcon(const QString& level, const QString& owner) {
  auto* icon = new QLabel(this);
  const int side = fontMetrics().height();
  icon->setPixmap(themeIcon(makeCardLevelIcon(level)).pixmap(side, side));
  icon->setToolTip(level.isEmpty() ? tr("Set by default") : tr("Set on %1").arg(levelWords(level, owner)));
  return icon;
}

PickerComboBox* MakeBodyCard::profilePicker(const QString& kind, const QVector<ProfilePickerRow>& rows,
                                            const QString& current, const QString& currentName) {
  const bool printer = kind == QLatin1String("printer");
  auto* box = new PickerComboBox(printer ? tr("Printer") : tr("Process"), this);
  box->setObjectName(printer ? QStringLiteral("makeCardPrinter") : QStringLiteral("makeCardProcess"));
  fitToCard(box);
  {
    const QSignalBlocker blocker(box);
    // Slicer default while none is chosen; then the rows in the pickers'
    // order under their headings (the picker carries them), as Send to
    // Slicer's.
    if (current.isEmpty()) {
      box->addItem(currentName, QString());
    }
    QVector<ChoicePickerRow> picks;
    for (const ProfilePickerRow& row : rows) {
      box->addItem(row.name, row.key);
      picks.append(ChoicePickerRow{row.key, row.group, row.shortName, row.source, QString()});
    }
    if (!current.isEmpty() && box->findData(current) < 0) {
      box->addItem(currentName, current); // a profile no list has (none of the kind's)
    }
    // Last, below a line: the kind's manager on the one chosen. The wheel
    // and the arrow keys on the closed field pass over both (not enabled
    // in its list; the picker lists them all the same).
    box->addItem(QString(), choiceSeparatorPrefix() + managerRow());
    box->addItem(printer ? tr("Printers…") : tr("Processes…"), managerRow());
    if (auto* model = qobject_cast<QStandardItemModel*>(box->model())) {
      for (int i = box->count() - 2; i < box->count(); ++i) {
        QStandardItem* item = model->item(i);
        item->setFlags(item->flags() & ~Qt::ItemIsEnabled);
      }
    }
    box->setRows(picks);
    box->setCurrentIndex(std::max(0, box->findData(current)));
  }
  connect(box, &QComboBox::activated, this, [this, box, kind, printer, current](int index) {
    const QString key = box->itemData(index).toString();
    if (key == managerRow() || isChoiceSeparator(key) || key.startsWith(choiceHeadingPrefix())) {
      // Not a profile: the choice stays.
      const QSignalBlocker blocker(box);
      box->setCurrentIndex(std::max(0, box->findData(current)));
      if (key == managerRow()) {
        emit managerRequested(kind, current);
      }
      return;
    }
    if (!key.isEmpty() && key != current) {
      if (printer) {
        emit printerChosen(key);
      } else {
        emit processChosen(key);
      }
    }
  });
  return box;
}

void MakeBodyCard::rebuild() {
  // A field that had the keyboard goes with the rows: the keyboard goes
  // back to the window, whose shortcuts (Undo) then work.
  if (QWidget* focused = QApplication::focusWidget(); focused != nullptr && isAncestorOf(focused)) {
    window()->setFocus(Qt::OtherFocusReason);
  }
  // The rows made again from the data.
  while (QLayoutItem* item = m_grid->takeAt(0)) {
    if (QWidget* widget = item->widget()) {
      widget->hide();
      widget->deleteLater();
    }
    delete item;
  }
  m_fields.clear();
  m_removes.clear();
  m_icons.clear();
  m_add = nullptr;
  m_send = nullptr;
  m_printer = nullptr;
  m_process = nullptr;
  m_material = nullptr;
  setTitle(tr("Make: %1").arg(m_data.body));
  int row = 0;

  // From the widest choice to the body's own (mitcad#1239): the printer,
  // the process, the material, then the values.
  // The printer: chosen for the whole design (its root), its icon the
  // level's that chose it.
  m_grid->addWidget(levelIcon(m_data.printerLevel, m_data.printerOwner), row, 0);
  m_grid->addWidget(new QLabel(tr("Printer"), this), row, 1);
  m_printer = profilePicker(QStringLiteral("printer"), m_data.printers, m_data.printerId, m_data.printer);
  m_printer->setToolTip(tr("The printer of the whole design, for every body"));
  m_grid->addWidget(m_printer, row++, 2, 1, 2);

  // The process profile, chosen for the body.
  m_grid->addWidget(levelIcon(m_data.processLevel, m_data.processOwner), row, 0);
  m_grid->addWidget(new QLabel(tr("Process"), this), row, 1);
  m_process = profilePicker(QStringLiteral("process"), m_data.processes, m_data.processId, m_data.process);
  m_process->setToolTip(tr("The process profile the body prints with; another one is chosen for the body"));
  m_grid->addWidget(m_process, row++, 2, 1, 2);

  // The material, chosen from the design's materials.
  m_grid->addWidget(levelIcon(m_data.materialLevel == QLatin1String("default") ? QString() : m_data.materialLevel,
                              QString()),
                    row, 0);
  m_grid->addWidget(new QLabel(tr("Material"), this), row, 1);
  m_material = new QComboBox(this);
  m_material->setObjectName(QStringLiteral("makeCardMaterial"));
  fitToCard(m_material);
  m_material->setToolTip(tr("The body's material; another one is set on the body"));
  {
    const QSignalBlocker blocker(m_material);
    for (const auto& [id, name] : m_data.materials) {
      m_material->addItem(name, id);
    }
    int current = m_material->findData(m_data.materialId);
    if (current < 0) {
      m_material->addItem(m_data.material, m_data.materialId);
      current = m_material->count() - 1;
    }
    m_material->setCurrentIndex(current);
  }
  connect(m_material, &QComboBox::activated, this, [this](int index) {
    const QString id = m_material->itemData(index).toString();
    if (id != m_data.materialId) {
      emit materialChosen(id);
    }
  });
  m_grid->addWidget(m_material, row++, 2, 1, 2);

  // The values that differ from the profiles, each with its level's icon;
  // a value typed is set on the body. A value set on the body has a remove
  // button (mitcad#1178); a pending one (added without a value) follows.
  QVector<MakeCardValue> rows = m_data.values;
  if (!m_pending.key.isEmpty()) {
    rows.append(m_pending);
  }
  for (const MakeCardValue& value : std::as_const(rows)) {
    const bool pending = !m_pending.key.isEmpty() && &value == &rows.constLast();
    m_grid->addWidget(levelIcon(value.level, value.owner), row, 0);
    m_icons.insert(value.key, makeCardLevelIcon(value.level));
    auto* label = new QLabel(value.name, this);
    label->setToolTip(value.key);
    label->setWordWrap(true);
    m_grid->addWidget(label, row, 1);
    auto* field = new QLineEdit(value.text, this);
    field->setObjectName(QStringLiteral("makeCardValue_") + value.key);
    field->setMinimumWidth(40);
    if (pending) {
      field->setPlaceholderText(tr("Slicer default"));
      field->setToolTip(tr("Not set yet: a value typed here is set on the body"));
    } else {
      field->setToolTip(tr("Set on %1; a value typed here is set on the body")
                            .arg(levelWords(value.level, value.owner)));
    }
    const QString set = value.set;
    const QString key = value.key;
    const QString name = value.name;
    const QString shown = value.text;
    connect(field, &QLineEdit::editingFinished, this, [this, field, set, key, shown] {
      const QString typed = field->text().trimmed();
      if (typed != shown) {
        emit valueEdited(set, key, typed);
      }
    });
    m_fields.insert(key, field);
    const bool own = value.level == QLatin1String("body");
    m_grid->addWidget(field, row, 2, 1, own ? 1 : 2);
    if (own) {
      auto* remove = new QToolButton(this);
      remove->setObjectName(QStringLiteral("makeCardRemove_") + key);
      remove->setIcon(themeIcon(QStringLiteral("close")));
      remove->setAutoRaise(true);
      remove->setFocusPolicy(Qt::NoFocus);
      remove->setAccessibleName(tr("Remove %1").arg(name));
      remove->setToolTip(pending ? tr("Drop %1: nothing was set on the body").arg(name)
                                 : tr("Remove the body's own %1: the value it inherits applies again").arg(name));
      connect(remove, &QToolButton::clicked, this, [this, pending, set, key, name] {
        if (pending) {
          qDebug().noquote() << QStringLiteral("Make card %1 dropped").arg(name);
          m_pending = MakeCardValue();
          rebuild();
          return;
        }
        emit valueRemoved(set, key, name);
      });
      m_removes.insert(key, remove);
      m_grid->addWidget(remove, row, 3);
    }
    ++row;
  }
  if (rows.isEmpty()) {
    auto* none = new QLabel(tr("No print values of its own: Add Print Value… adds one."), this);
    none->setWordWrap(true);
    none->setEnabled(false);
    m_grid->addWidget(none, row++, 0, 1, 4);
  }
  // A setting of the body's profiles added as the body's own (mitcad#1178).
  auto* add = new QPushButton(themeIcon(QStringLiteral("add")), tr("Add Print Value…"), this);
  add->setObjectName(QStringLiteral("makeCardAddValue"));
  add->setToolTip(tr("Add a printer or process setting to %1, starting from the value it prints with now")
                      .arg(m_data.body));
  connect(add, &QPushButton::clicked, this, [this] { emit addValueRequested(); });
  m_add = add;
  m_grid->addWidget(add, row++, 0, 1, 4, Qt::AlignLeft);
  // Send to Slicer (mitcad#1377): the command's panel with this body alone
  // in Bodies, named and drawn as the command (MainWindow::print3dCommand).
  auto* send = new QPushButton(themeIcon(QStringLiteral("print-3d")), tr("Send to Slicer"), this);
  send->setObjectName(QStringLiteral("makeCardSend"));
  send->setToolTip(tr("Send %1 alone to a slicer: opens Send to Slicer with it in Bodies").arg(m_data.body));
  send->setEnabled(m_sendEnabled);
  connect(send, &QPushButton::clicked, this, [this] { emit sendRequested(); });
  m_send = send;
  m_grid->addWidget(send, row++, 0, 1, 4, Qt::AlignLeft);
}

void MakeBodyCard::setSendEnabled(bool enabled) {
  m_sendEnabled = enabled;
  if (m_send != nullptr) {
    m_send->setEnabled(enabled);
  }
}

QStringList MakeBodyCard::places() const {
  QStringList lines;
  if (!isVisible()) {
    return lines;
  }
  if (m_printer != nullptr) {
    lines << QStringLiteral("Make card printer at %1").arg(at(m_printer));
  }
  if (m_process != nullptr) {
    lines << QStringLiteral("Make card process at %1").arg(at(m_process));
  }
  if (m_material != nullptr) {
    lines << QStringLiteral("Make card material at %1").arg(at(m_material));
  }
  // The rows' names as laid out, from the top (mitcad#1239): the labels of
  // the name column, not a line across the card.
  QStringList names;
  for (int row = 0; row < m_grid->rowCount(); ++row) {
    QLayoutItem* item = m_grid->itemAtPosition(row, 1);
    const auto* label = item != nullptr ? qobject_cast<const QLabel*>(item->widget()) : nullptr;
    int top = 0;
    int column = 0;
    int rows = 0;
    int columns = 0;
    if (label != nullptr) {
      m_grid->getItemPosition(m_grid->indexOf(item->widget()), &top, &column, &rows, &columns);
    }
    if (label != nullptr && column == 1) {
      names << label->text();
    }
  }
  lines << QStringLiteral("Make card rows: %1").arg(names.join(QStringLiteral(", ")));
  for (auto it = m_fields.cbegin(); it != m_fields.cend(); ++it) {
    lines << QStringLiteral("Make card value %1 at %2").arg(it.key(), at(it.value()));
    lines << QStringLiteral("Make card value %1 icon %2").arg(it.key(), m_icons.value(it.key()));
  }
  for (auto it = m_removes.cbegin(); it != m_removes.cend(); ++it) {
    lines << QStringLiteral("Make card remove %1 at %2").arg(it.key(), at(it.value()));
  }
  if (m_add != nullptr) {
    lines << QStringLiteral("Make card add value at %1").arg(at(m_add));
  }
  if (m_send != nullptr) {
    lines << QStringLiteral("Make card send to slicer at %1%2")
                 .arg(at(m_send), m_send->isEnabled() ? QString() : QStringLiteral(" (disabled)"));
  }
  return lines;
}

} // namespace mitcad
