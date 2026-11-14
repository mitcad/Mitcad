// SPDX-License-Identifier: MIT
#include "FilamentChangeDialog.hpp"

#include <cmath>

#include <QColor>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHideEvent>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPixmap>
#include <QPushButton>
#include <QRegularExpression>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// A small square of a filament's colour for the Change to list.
QIcon swatch(const QString& color) {
  QPixmap pixmap(12, 12);
  pixmap.fill(QColor(color).isValid() ? QColor(color) : QColor(Qt::lightGray));
  return QIcon(pixmap);
}

// A length for the height field: the number in the design's unit with it.
QString lengthText(double millimetres, const QString& unit, double unitMillimetres) {
  return QStringLiteral("%1 %2").arg(QString::number(millimetres / unitMillimetres, 'g', 6), unit);
}

// Millimetres of a unit typed after a number; 0 for none known.
double unitMm(const QString& unit) {
  if (unit == QStringLiteral("mm")) {
    return 1.0;
  }
  if (unit == QStringLiteral("cm")) {
    return 10.0;
  }
  if (unit == QStringLiteral("m")) {
    return 1000.0;
  }
  if (unit == QStringLiteral("in") || unit == QStringLiteral("\"")) {
    return 25.4;
  }
  if (unit == QStringLiteral("ft")) {
    return 304.8;
  }
  return 0.0;
}

bool hasValue(const QJsonValue& value) { return !value.isUndefined() && !value.isNull(); }

const QString kHeight = QStringLiteral("height");
const QString kToObject = QStringLiteral("to_object");

} // namespace

FilamentChangeDialog::FilamentChangeDialog(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Filament Change"));
  setModal(false);
  auto* layout = new QVBoxLayout(this);
  m_form = new QFormLayout;
  // At: placed as Extrude's extent is, by a distance or to an object.
  m_extent = new QComboBox;
  m_extent->setObjectName(QStringLiteral("filamentChangeExtent"));
  m_extent->setToolTip(tr("A height above the print bed, or a plane or flat face picked in the view"));
  m_extent->addItem(tr("Distance from Bed"), kHeight);
  m_extent->addItem(tr("To Object"), kToObject);
  connect(m_extent, &QComboBox::currentIndexChanged, this, [this] { extentChanged(); });
  m_form->addRow(tr("At"), m_extent);
  m_place = new QLineEdit;
  m_place->setObjectName(QStringLiteral("filamentChangeAt"));
  m_place->setToolTip(
      tr("Measured from the print bed: a number in the design's unit, or with a unit, such as 12 mm"));
  m_form->addRow(tr("Height above bed"), m_place);
  // Plane or face: a selection field, as a panel's, its pick named with
  // the height it gives, and why it would be left out under it.
  auto* object = new QWidget;
  object->setObjectName(QStringLiteral("filamentChangeObjectRow"));
  auto* column = new QVBoxLayout(object);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(1);
  m_object = new QPushButton(swatchIcon(commandInputColor(1)), tr("Select"));
  m_object->setObjectName(QStringLiteral("filamentChangeObject"));
  m_object->setCheckable(true);
  m_object->setFocusPolicy(Qt::NoFocus);
  m_object->setToolTip(tr("A plane, or a flat face level in the print orientation, picked in the view"));
  connect(m_object, &QPushButton::clicked, this, [this] {
    // Checked only while it takes the picks, which the owner says.
    m_object->setChecked(m_picking);
    emit picksWanted(true);
  });
  column->addWidget(m_object);
  m_objectNote = new QLabel;
  m_objectNote->setWordWrap(true);
  m_objectNote->hide();
  column->addWidget(m_objectNote);
  m_form->addRow(tr("Plane or face"), object);
  m_change = new QComboBox;
  m_change->setObjectName(QStringLiteral("filamentChangeTo"));
  m_change->setToolTip(tr("The filament printed from here on, or a pause"));
  connect(m_change, &QComboBox::currentIndexChanged, this, [this] { kindChanged(); });
  m_form->addRow(tr("Change to"), m_change);
  m_message = new QLineEdit;
  m_message->setObjectName(QStringLiteral("filamentChangeMessage"));
  m_message->setToolTip(tr("What the printer shows at the pause"));
  m_message->setMaxLength(120);
  m_form->addRow(tr("Message"), m_message);
  layout->addLayout(m_form);
  m_refusal = new QLabel;
  m_refusal->setWordWrap(true);
  m_refusal->hide();
  layout->addWidget(m_refusal);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, this, [this] { apply(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  extentChanged();
}

void FilamentChangeDialog::edit(int row, const QJsonObject& event, const QString& place, const QString& warning,
                                const QJsonArray& choices, int preferred, const QString& unit,
                                double unitMillimetres) {
  m_row = row;
  m_event = event;
  m_at = event.value(QStringLiteral("at"));
  m_choices = choices;
  m_unit = unit;
  m_unitMillimetres = unitMillimetres > 0.0 ? unitMillimetres : 1.0;
  m_refusal->hide();
  const bool atObject = hasValue(m_at);
  // A height stays in its field also for an event at a plane or face, so
  // that Distance from Bed has one to start from.
  if (event.value(QStringLiteral("height")).isDouble()) {
    m_place->setText(lengthText(event.value(QStringLiteral("height")).toDouble(), m_unit, m_unitMillimetres));
  } else {
    m_place->clear();
  }
  m_atPlace = atObject ? place : QString();
  m_object->setText(m_atPlace.isEmpty() ? tr("Select") : m_atPlace);
  m_objectNote->setText(atObject ? warning : QString());
  m_objectNote->setVisible(atObject && !warning.isEmpty());
  {
    // Edit opens in the way the event was placed.
    const QSignalBlocker block(m_extent);
    m_extent->setCurrentIndex(m_extent->findData(atObject ? kToObject : kHeight));
  }
  // The choices, then Pause; the event's own, else the one preferred.
  const bool pause = event.value(QStringLiteral("kind")).toString() == QStringLiteral("pause");
  const QString material = event.value(QStringLiteral("material")).toString();
  const QJsonValue appearance = event.value(QStringLiteral("appearance"));
  {
    const QSignalBlocker block(m_change);
    m_change->clear();
    int current = -1;
    for (int i = 0; i < m_choices.size(); ++i) {
      const QJsonObject choice = m_choices.at(i).toObject();
      m_change->addItem(swatch(choice.value(QStringLiteral("color")).toString()),
                        choice.value(QStringLiteral("name")).toString());
      if (!pause && !material.isEmpty() && choice.value(QStringLiteral("material")).toString() == material &&
          (!hasValue(appearance) || choice.value(QStringLiteral("appearance")) == appearance)) {
        current = current < 0 ? i : current;
      }
    }
    m_change->addItem(tr("Pause"));
    if (pause) {
      current = m_change->count() - 1;
    } else if (current < 0) {
      current = preferred >= 0 && preferred < m_choices.size() ? preferred : 0;
    }
    m_change->setCurrentIndex(current);
  }
  m_message->setText(event.value(QStringLiteral("message")).toString());
  kindChanged();
  extentChanged();
  if (atObject) {
    m_change->setFocus();
  } else {
    m_place->setFocus();
    m_place->selectAll();
  }
  qInfo().noquote() << QStringLiteral("Filament change dialog: %1, %2, change to %3")
                           .arg(row < 0 ? QStringLiteral("new") : QStringLiteral("row %1").arg(row + 1), placeText(),
                                m_change->currentText());
}

QString FilamentChangeDialog::placeText() const {
  if (toObject()) {
    return QStringLiteral("to object %1").arg(m_atPlace.isEmpty() ? QStringLiteral("none") : m_atPlace);
  }
  return QStringLiteral("height %1").arg(m_place->text().isEmpty() ? QStringLiteral("none") : m_place->text());
}

void FilamentChangeDialog::setAt(const QJsonValue& at, const QString& place, const QString& warning) {
  m_at = at;
  m_atPlace = place;
  m_object->setText(place);
  m_objectNote->setText(warning);
  m_objectNote->setVisible(!warning.isEmpty());
  m_refusal->hide();
  qInfo().noquote() << QStringLiteral("Filament change dialog at: %1").arg(place);
}

void FilamentChangeDialog::setPicking(bool picking) {
  m_object->setChecked(picking);
  if (picking == m_picking) {
    return;
  }
  m_picking = picking;
  qInfo().noquote() << QStringLiteral("Filament change dialog picks: %1")
                           .arg(picking ? QStringLiteral("on") : QStringLiteral("off"));
}

bool FilamentChangeDialog::toObject() const { return m_extent->currentData().toString() == kToObject; }

bool FilamentChangeDialog::wantsPicks() const { return isVisible() && toObject(); }

bool FilamentChangeDialog::isPause() const { return m_change->currentIndex() == m_change->count() - 1; }

void FilamentChangeDialog::showRefusal(const QString& why) {
  m_refusal->setText(why);
  m_refusal->show();
  qInfo().noquote() << QStringLiteral("Filament change dialog refused: %1").arg(why);
}

void FilamentChangeDialog::kindChanged() {
  // Only a pause shows a message.
  m_message->setEnabled(isPause());
}

void FilamentChangeDialog::extentChanged() {
  const bool object = toObject();
  m_form->setRowVisible(m_place, !object);
  m_form->setRowVisible(m_object->parentWidget(), object);
  m_refusal->hide();
  if (isVisible()) {
    qInfo().noquote() << QStringLiteral("Filament change dialog extent: %1")
                             .arg(object ? QStringLiteral("to object") : QStringLiteral("distance from bed"));
    emit picksWanted(object);
  }
}

void FilamentChangeDialog::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  // Closed: the view's picks go back to the panel.
  emit picksWanted(false);
}

double FilamentChangeDialog::typedHeight() const {
  static const QRegularExpression pattern(QStringLiteral("^\\s*([0-9]+(?:[.,][0-9]*)?|[.,][0-9]+)\\s*([a-z\"]*)\\s*$"));
  const QRegularExpressionMatch match = pattern.match(m_place->text().toLower());
  if (!match.hasMatch()) {
    return -1.0;
  }
  QString number = match.captured(1);
  number.replace(QLatin1Char(','), QLatin1Char('.'));
  bool ok = false;
  const double value = number.toDouble(&ok);
  const QString unit = match.captured(2);
  const double scale = unit.isEmpty() ? m_unitMillimetres : unitMm(unit);
  if (!ok || scale <= 0.0 || !std::isfinite(value) || value <= 0.0) {
    return -1.0;
  }
  return value * scale;
}

void FilamentChangeDialog::apply() {
  QJsonObject event;
  if (toObject()) {
    if (!hasValue(m_at)) {
      showRefusal(tr("Pick a plane or a flat face in the view."));
      return;
    }
    event.insert(QStringLiteral("at"), m_at);
    // The body it was measured from stays while the place does.
    if (m_event.value(QStringLiteral("at")) == m_at && m_event.contains(QStringLiteral("body"))) {
      event.insert(QStringLiteral("body"), m_event.value(QStringLiteral("body")));
    }
  } else {
    const double height = typedHeight();
    if (height <= 0.0) {
      showRefusal(tr("Type a height above 0, such as 12 mm."));
      return;
    }
    event.insert(QStringLiteral("height"), height);
  }
  const int index = m_change->currentIndex();
  if (index >= 0 && index < m_choices.size()) {
    const QJsonObject choice = m_choices.at(index).toObject();
    event.insert(QStringLiteral("material"), choice.value(QStringLiteral("material")));
    const QJsonValue appearance = choice.value(QStringLiteral("appearance"));
    if (appearance.isString()) {
      event.insert(QStringLiteral("appearance"), appearance);
    }
  } else {
    event.insert(QStringLiteral("kind"), QStringLiteral("pause"));
    const QString message = m_message->text().trimmed();
    if (!message.isEmpty()) {
      event.insert(QStringLiteral("message"), message);
    }
  }
  emit applied(m_row, event);
}

} // namespace mitcad
