// SPDX-License-Identifier: MIT
#include "MakeBodyCard.hpp"

#include <algorithm>

#include <QApplication>
#include <QComboBox>
#include <QGridLayout>
#include <QLabel>
#include <QLayoutItem>
#include <QLineEdit>
#include <QSignalBlocker>
#include <QWidget>

#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"

namespace mitcad {
namespace {

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
  m_data = data;
  m_hasData = true;
  rebuild();
  return true;
}

QLabel* MakeBodyCard::levelIcon(const QString& level, const QString& owner) {
  auto* icon = new QLabel(this);
  const int side = fontMetrics().height();
  icon->setPixmap(themeIcon(makeCardLevelIcon(level)).pixmap(side, side));
  icon->setToolTip(level.isEmpty() ? tr("Set by default") : tr("Set on %1").arg(levelWords(level, owner)));
  return icon;
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
  m_material = nullptr;
  m_process = nullptr;
  setTitle(tr("Make: %1").arg(m_data.body));
  int row = 0;

  // The material, chosen from the design's materials.
  m_grid->addWidget(levelIcon(m_data.materialLevel == QLatin1String("default") ? QString() : m_data.materialLevel,
                              QString()),
                    row, 0);
  m_grid->addWidget(new QLabel(tr("Material"), this), row, 1);
  m_material = new QComboBox(this);
  m_material->setObjectName(QStringLiteral("makeCardMaterial"));
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
  m_grid->addWidget(m_material, row++, 2);

  // The printer: chosen for the whole design (its root), shown here.
  m_grid->addWidget(levelIcon(m_data.printerLevel, m_data.printerOwner), row, 0);
  m_grid->addWidget(new QLabel(tr("Printer"), this), row, 1);
  auto* printer = new QLabel(m_data.printer, this);
  printer->setObjectName(QStringLiteral("makeCardPrinter"));
  printer->setToolTip(tr("The design's printer: Edit Printers and Processes… changes it"));
  m_grid->addWidget(printer, row++, 2);

  // The process profile, chosen for the body.
  m_grid->addWidget(levelIcon(m_data.processLevel, m_data.processOwner), row, 0);
  m_grid->addWidget(new QLabel(tr("Process"), this), row, 1);
  m_process = new QComboBox(this);
  m_process->setObjectName(QStringLiteral("makeCardProcess"));
  m_process->setToolTip(tr("The process profile the body prints with; another one is chosen for the body"));
  {
    const QSignalBlocker blocker(m_process);
    if (m_data.processId.isEmpty()) {
      m_process->addItem(m_data.process, QString());
    }
    for (const auto& [id, name] : m_data.processes) {
      m_process->addItem(name, id);
    }
    m_process->setCurrentIndex(std::max(0, m_process->findData(m_data.processId)));
  }
  connect(m_process, &QComboBox::activated, this, [this](int index) {
    const QString id = m_process->itemData(index).toString();
    if (!id.isEmpty() && id != m_data.processId) {
      emit processChosen(id);
    }
  });
  m_grid->addWidget(m_process, row++, 2);

  // The values that differ from the profiles, each with its level's icon;
  // a value typed is set on the body.
  for (const MakeCardValue& value : m_data.values) {
    m_grid->addWidget(levelIcon(value.level, value.owner), row, 0);
    auto* label = new QLabel(value.name, this);
    label->setToolTip(value.key);
    m_grid->addWidget(label, row, 1);
    auto* field = new QLineEdit(value.text, this);
    field->setObjectName(QStringLiteral("makeCardValue_") + value.key);
    field->setToolTip(tr("Set on %1; a value typed here is set on the body")
                          .arg(levelWords(value.level, value.owner)));
    const QString set = value.set;
    const QString key = value.key;
    const QString shown = value.text;
    connect(field, &QLineEdit::editingFinished, this, [this, field, set, key, shown] {
      const QString typed = field->text().trimmed();
      if (typed != shown) {
        emit valueEdited(set, key, typed);
      }
    });
    m_fields.insert(key, field);
    m_grid->addWidget(field, row++, 2);
  }
  if (m_data.values.isEmpty()) {
    auto* none = new QLabel(tr("No print values of its own"), this);
    none->setEnabled(false);
    m_grid->addWidget(none, row++, 1, 1, 2);
  }
}

QStringList MakeBodyCard::places() const {
  QStringList lines;
  if (!isVisible()) {
    return lines;
  }
  if (m_material != nullptr) {
    lines << QStringLiteral("Make card material at %1").arg(at(m_material));
  }
  if (m_process != nullptr) {
    lines << QStringLiteral("Make card process at %1").arg(at(m_process));
  }
  for (auto it = m_fields.cbegin(); it != m_fields.cend(); ++it) {
    lines << QStringLiteral("Make card value %1 at %2").arg(it.key(), at(it.value()));
  }
  return lines;
}

} // namespace mitcad
