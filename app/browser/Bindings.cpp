// SPDX-License-Identifier: MIT
#include "Bindings.hpp"

#include <algorithm>
#include <utility>

#include <QDialogButtonBox>
#include <QFont>
#include <QHBoxLayout>
#include <QIconEngine>
#include <QLabel>
#include <QLineEdit>
#include <QPainter>
#include <QPixmap>
#include <QPushButton>
#include <QShowEvent>
#include <QStringList>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/FormulaEditor.hpp"
#include "../framework/ParameterField.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// An icon with "fx" over its lower right corner.
class BoundIconEngine : public QIconEngine {
public:
  explicit BoundIconEngine(QIcon icon) : m_icon(std::move(icon)) {}

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    m_icon.paint(painter, rect, Qt::AlignCenter, mode, state);
    const int side = std::max(7, rect.height() * 3 / 5);
    const QRect badge(rect.right() + 1 - side, rect.bottom() + 1 - side, side, side);
    QFont font = painter->font();
    font.setItalic(true);
    font.setBold(true);
    font.setPixelSize(std::max(6, side * 4 / 5));
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(Qt::NoPen);
    QColor fill = themeColor(ColorRole::BoundFill, false); // one fx style (mitcad#1041)
    fill.setAlpha(220);
    painter->setBrush(fill);
    painter->drawRoundedRect(badge, 2, 2);
    painter->setFont(font);
    painter->setPen(themeColor(ColorRole::BoundText, false));
    painter->drawText(badge, Qt::AlignCenter, QStringLiteral("fx"));
    painter->restore();
  }

  QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
    QPixmap pixmap(size);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    paint(&painter, QRect(QPoint(), size), mode, state);
    return pixmap;
  }

  QIconEngine* clone() const override { return new BoundIconEngine(m_icon); }

private:
  QIcon m_icon;
};

// "<name> at x,y" in the main window's coordinates, as the other logged
// places.
QString placeOf(const QWidget* dialog, const QWidget* widget) {
  const QWidget* window = dialog->parentWidget() != nullptr ? dialog->parentWidget()->window() : dialog;
  const QPoint at = widget->mapToGlobal(widget->rect().center()) - window->mapToGlobal(QPoint(0, 0));
  return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
}

} // namespace

QIcon boundIcon(const QIcon& icon) { return QIcon(new BoundIconEngine(icon)); }

// Suppress When… and Show When….

ConditionDialog::ConditionDialog(const QString& what, const QString& help, const QString& initial,
                                 QWidget* parent)
    : QDialog(parent) {
  setObjectName(QStringLiteral("conditionDialog"));
  setWindowTitle(what);
  auto* layout = new QVBoxLayout(this);
  auto* intro = new QLabel(help);
  intro->setWordWrap(true);
  layout->addWidget(intro);

  auto* row = new QHBoxLayout;
  m_condition = new QLineEdit(initial);
  m_condition->setObjectName(QStringLiteral("bindingCondition"));
  m_condition->setPlaceholderText(tr("Color = 'Black'"));
  m_condition->setMinimumWidth(m_condition->fontMetrics().horizontalAdvance(QLatin1Char('M')) * 28);
  ParameterFieldOptions options;
  options.kind = ParameterKind::Unitless;
  options.anyKind = true;
  options.label = what;
  ParameterField::attach(m_condition, options);
  row->addWidget(m_condition, 1);
  m_formula = new QPushButton(tr("Formula Editor…"));
  row->addWidget(m_formula);
  layout->addLayout(row);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  m_ok = buttons->button(QDialogButtonBox::Ok);
  m_remove = buttons->addButton(tr("Remove Condition"), QDialogButtonBox::ResetRole);
  m_remove->setEnabled(!initial.trimmed().isEmpty());
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_remove, &QPushButton::clicked, this, [this] {
    m_removed = true;
    accept();
  });
  connect(m_formula, &QPushButton::clicked, this, &ConditionDialog::openFormulaEditor);
  qDebug().noquote() << QStringLiteral("Condition dialog %1: %2")
                            .arg(what, initial.isEmpty() ? QStringLiteral("none") : initial);
}

QString ConditionDialog::condition() const { return m_removed ? QString() : m_condition->text().trimmed(); }

void ConditionDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  m_condition->setFocus();
  QTimer::singleShot(0, this, &ConditionDialog::logPlaces);
}

void ConditionDialog::openFormulaEditor() {
  const ParameterSource* source = parameterSourceOf(this);
  if (source == nullptr || !source->query) {
    return;
  }
  ParameterFieldOptions options;
  options.kind = ParameterKind::Unitless;
  options.anyKind = true;
  options.label = windowTitle();
  FormulaEditor editor(*source, options, m_condition->text(), this);
  if (execDialog(&editor) == QDialog::Accepted) {
    m_condition->setText(editor.expression());
    qDebug().noquote() << QStringLiteral("Condition from the Formula Editor: %1").arg(editor.expression());
  }
}

void ConditionDialog::logPlaces() {
  if (!isVisible()) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Condition field at %1").arg(placeOf(this, m_condition));
  qDebug().noquote() << QStringLiteral("Condition formula at %1").arg(placeOf(this, m_formula));
  qDebug().noquote() << QStringLiteral("Condition remove at %1").arg(placeOf(this, m_remove));
  qDebug().noquote() << QStringLiteral("Condition OK at %1").arg(placeOf(this, m_ok));
}

} // namespace mitcad
