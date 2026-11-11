// SPDX-License-Identifier: MIT
#include "FilamentChangeDialog.hpp"

#include <cmath>

#include <QColor>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPixmap>
#include <QPushButton>
#include <QRegularExpression>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

namespace mitcad {
namespace {

// A small square of a filament's colour for the Change to list.
QIcon swatch(const QString& color) {
  QPixmap pixmap(12, 12);
  pixmap.fill(QColor(color).isValid() ? QColor(color) : QColor(Qt::lightGray));
  return QIcon(pixmap);
}

// A length for the At field: the number in the design's unit with it.
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

} // namespace

FilamentChangeDialog::FilamentChangeDialog(QWidget* parent) : QDialog(parent) {
  setWindowTitle(tr("Filament Change"));
  setModal(false);
  auto* layout = new QVBoxLayout(this);
  auto* form = new QFormLayout;
  m_place = new QLineEdit;
  m_place->setObjectName(QStringLiteral("filamentChangeAt"));
  m_place->setToolTip(tr("A height above the print bed, or a plane or flat face picked in the view"));
  m_place->setPlaceholderText(tr("Height, or pick a plane or face"));
  // Typing takes a picked plane or face away: the text is a height then.
  connect(m_place, &QLineEdit::textEdited, this, [this] { m_at = QJsonValue(); });
  form->addRow(tr("At"), m_place);
  m_change = new QComboBox;
  m_change->setObjectName(QStringLiteral("filamentChangeTo"));
  m_change->setToolTip(tr("The filament printed from here on, or a pause"));
  connect(m_change, &QComboBox::currentIndexChanged, this, [this] { kindChanged(); });
  form->addRow(tr("Change to"), m_change);
  m_message = new QLineEdit;
  m_message->setObjectName(QStringLiteral("filamentChangeMessage"));
  m_message->setToolTip(tr("What the printer shows at the pause"));
  m_message->setMaxLength(120);
  form->addRow(tr("Message"), m_message);
  layout->addLayout(form);
  auto* hint = new QLabel(tr("Or pick a plane or flat face in the view with Change At."));
  hint->setWordWrap(true);
  hint->setEnabled(false);
  layout->addWidget(hint);
  m_refusal = new QLabel;
  m_refusal->setWordWrap(true);
  m_refusal->hide();
  layout->addWidget(m_refusal);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, this, [this] { apply(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
}

void FilamentChangeDialog::edit(int row, const QJsonObject& event, const QString& place, const QJsonArray& choices,
                                int preferred, const QString& unit, double unitMillimetres) {
  m_row = row;
  m_event = event;
  m_at = event.value(QStringLiteral("at"));
  m_choices = choices;
  m_unit = unit;
  m_unitMillimetres = unitMillimetres > 0.0 ? unitMillimetres : 1.0;
  m_refusal->hide();
  if (!m_at.isUndefined() && !m_at.isNull()) {
    m_place->setText(place);
  } else if (event.value(QStringLiteral("height")).isDouble()) {
    m_place->setText(lengthText(event.value(QStringLiteral("height")).toDouble(), m_unit, m_unitMillimetres));
  } else {
    m_place->clear();
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
          (appearance.isUndefined() || appearance.isNull() ||
           choice.value(QStringLiteral("appearance")) == appearance)) {
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
  m_place->setFocus();
  m_place->selectAll();
  qInfo().noquote() << QStringLiteral("Filament change dialog: %1, at %2, change to %3")
                           .arg(row < 0 ? QStringLiteral("new") : QStringLiteral("row %1").arg(row + 1),
                                m_place->text().isEmpty() ? QStringLiteral("none") : m_place->text(),
                                m_change->currentText());
}

void FilamentChangeDialog::setAt(const QJsonValue& at, const QString& place) {
  m_at = at;
  m_place->setText(place);
  m_refusal->hide();
  qInfo().noquote() << QStringLiteral("Filament change dialog at: %1").arg(place);
}

void FilamentChangeDialog::showRefusal(const QString& why) {
  m_refusal->setText(why);
  m_refusal->show();
  qInfo().noquote() << QStringLiteral("Filament change dialog refused: %1").arg(why);
}

void FilamentChangeDialog::kindChanged() {
  // Only a pause shows a message.
  m_message->setEnabled(m_change->currentIndex() == m_change->count() - 1);
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
  if (!m_at.isUndefined() && !m_at.isNull()) {
    event.insert(QStringLiteral("at"), m_at);
    // The body it was measured from stays while the place does.
    if (m_event.value(QStringLiteral("at")) == m_at && m_event.contains(QStringLiteral("body"))) {
      event.insert(QStringLiteral("body"), m_event.value(QStringLiteral("body")));
    }
  } else {
    const double height = typedHeight();
    if (height <= 0.0) {
      showRefusal(tr("Type a height above 0, such as 12 mm, or pick a plane or flat face in the view."));
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
