// SPDX-License-Identifier: MIT
#include "FavoriteParametersCard.hpp"

#include <algorithm>
#include <exception>
#include <iterator>
#include <utility>

#include <QEvent>
#include <QFont>
#include <QGridLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPainter>
#include <QSettings>
#include <QSignalBlocker>
#include <QSlider>
#include <QStyle>
#include <QStyleOptionSlider>
#include <QToolTip>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/GlassCard.hpp"
#include "../framework/IdleCall.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

// The Parameters window's sort (ParametersDialog, mitcad#1866): the
// Favorites group's order.
const char* const kSortSetting = "parameters/sort";

constexpr const char* kNameProperty = "favoriteParameter";

// A parameter's name as plain text, elided at the end when the card is
// narrow (the whole name in its tooltip).
class NameLabel : public QLabel {
public:
  explicit NameLabel(const QString& text, QWidget* parent = nullptr) : QLabel(text, parent) {
    setToolTip(text);
    setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  }
  QSize minimumSizeHint() const override {
    return {fontMetrics().horizontalAdvance(QStringLiteral("MMM")), QLabel::minimumSizeHint().height()};
  }

protected:
  void paintEvent(QPaintEvent* /*event*/) override {
    QPainter painter(this);
    const QRect area = contentsRect();
    const QString shown = fontMetrics().elidedText(text(), Qt::ElideRight, area.width());
    style()->drawItemText(&painter, area, Qt::AlignLeft | Qt::AlignVCenter, palette(), isEnabled(), shown,
                          foregroundRole());
  }
};

// A small line of the card (a formula's value, a refusal's reason).
QLabel* smallLabel(QWidget* parent) {
  auto* label = new QLabel(parent);
  QFont font = label->font();
  font.setPointSizeF(font.pointSizeF() * 0.85);
  label->setFont(font);
  label->setWordWrap(true);
  label->hide();
  return label;
}

} // namespace

FavoriteParametersCard::FavoriteParametersCard(DocumentHost& host, QWidget* parent) : QWidget(parent), m_host(host) {
  setObjectName(QStringLiteral("favoriteParametersCard"));
  m_layout = new QVBoxLayout(this);
  m_layout->setContentsMargins(0, 0, 0, 0);
  m_layout->setSpacing(8);
  // Without favorites: how to star them, and the window that does it.
  m_empty = new QWidget(this);
  m_empty->setObjectName(QStringLiteral("favoriteParametersEmpty"));
  auto* empty = new QVBoxLayout(m_empty);
  empty->setContentsMargins(0, 0, 0, 0);
  empty->setSpacing(8);
  auto* hint = new QLabel(tr("Star parameters in the Parameters window to edit them here."), m_empty);
  hint->setWordWrap(true);
  setRoleStyleSheet(hint, ColorRole::TextSecondary);
  empty->addWidget(hint);
  auto* open = new CapsuleButton(tr("Open Parameters…"), false, m_empty);
  open->setObjectName(QStringLiteral("favoriteParametersOpen"));
  open->setToolTip(tr("Opens the Parameters window, whose star column marks the favorites"));
  connect(open, &QToolButton::clicked, this, [this] {
    qDebug().noquote() << QStringLiteral("Favorite parameters: Open Parameters");
    emit openParametersRequested();
  });
  empty->addWidget(open, 0, Qt::AlignLeft);
  m_openButton = open;
  m_layout->addWidget(m_empty);
  m_layout->addStretch(1);
  new PlaceWatcher(this, [this] { logLayout(); });
}

void FavoriteParametersCard::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  if (m_stale) {
    TestSync::singleShot(0, this, [this] { refresh(); });
  }
}

void FavoriteParametersCard::refresh() {
  if (!isVisible()) {
    m_stale = true;
    return;
  }
  if (m_host.modelBusy()) {
    // Not while a job computes the model (it owns the document).
    m_stale = true;
    m_host.whenIdle(this, [this] { refresh(); });
    return;
  }
  m_stale = false;
  QVector<FavoriteRow> rows;
  try {
    rows = favoriteRows(m_host.model().queryArray(QStringLiteral("parameters")),
                        QSettings().value(QLatin1String(kSortSetting)).toString());
  } catch (const std::exception&) {
    // A model that cannot list its parameters: nothing to edit.
    rows.clear();
  }
  // The rows are made again only when the favorites change (another
  // parameter, a slider or a formula more or less); otherwise their values
  // are shown in them.
  QStringList structure;
  for (const FavoriteRow& row : std::as_const(rows)) {
    structure << QStringLiteral("%1|%2|%3|%4|%5")
                     .arg(row.name)
                     .arg(row.formula ? 1 : 0)
                     .arg(row.slider ? row.slider->positions : -1)
                     .arg(row.reference ? 1 : 0)
                     .arg(row.text ? 1 : 0);
  }
  const QString key = structure.join(QLatin1Char('\n'));
  QVector<Row> previous = std::move(m_rows);
  m_rows.clear();
  for (const FavoriteRow& favorite : std::as_const(rows)) {
    Row row;
    row.data = favorite;
    for (const Row& old : std::as_const(previous)) {
      if (old.data.name == favorite.name) {
        row.block = old.block;
        row.name = old.name;
        row.field = old.field;
        row.value = old.value;
        row.error = old.error;
        row.slider = old.slider;
      }
    }
    m_rows << row;
  }
  // Refusals of parameters no longer starred are forgotten.
  for (auto it = m_refused.begin(); it != m_refused.end();) {
    it = this->row(it.key()) != nullptr ? std::next(it) : m_refused.erase(it);
  }
  if (key != m_structure) {
    m_structure = key;
    for (const Row& old : std::as_const(previous)) {
      if (old.block != nullptr) {
        m_layout->removeWidget(old.block);
        old.block->hide();
        old.block->deleteLater();
      }
    }
    rebuild();
  }
  showValues();
  emit contentChanged();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

FavoriteParametersCard::Row* FavoriteParametersCard::row(const QString& name) {
  for (Row& row : m_rows) {
    if (row.data.name == name) {
      return &row;
    }
  }
  return nullptr;
}

void FavoriteParametersCard::rebuild() {
  m_empty->setVisible(m_rows.isEmpty());
  int index = 0;
  for (Row& row : m_rows) {
    // The widgets made below, none of the old ones.
    Row fresh;
    fresh.data = row.data;
    row = fresh;
    const QString name = row.data.name;
    row.block = new QWidget(this);
    row.block->setObjectName(QStringLiteral("favoriteParameter_%1").arg(name));
    auto* grid = new QGridLayout(row.block);
    grid->setContentsMargins(0, 0, 0, 0);
    grid->setHorizontalSpacing(8);
    grid->setVerticalSpacing(2);
    grid->setColumnStretch(0, 1);
    grid->setColumnStretch(1, 1);
    row.name = new NameLabel(name, row.block);
    grid->addWidget(row.name, 0, 0);
    row.field = new QLineEdit(row.block);
    row.field->setObjectName(QStringLiteral("favoriteParameterField_%1").arg(name));
    row.field->setProperty(kNameProperty, name);
    row.field->setAccessibleName(tr("%1 value").arg(name));
    row.field->setMinimumWidth(72);
    row.field->setFocusPolicy(Qt::StrongFocus);
    // A driven dimension's reference is what its dimension measures
    // (mitcad#231): shown, not set.
    row.field->setReadOnly(row.data.reference);
    row.field->installEventFilter(this);
    connect(row.field, &QLineEdit::editingFinished, this, [this, name] { fieldEdited(name); });
    grid->addWidget(row.field, 0, 1);
    row.value = smallLabel(row.block);
    row.value->setObjectName(QStringLiteral("favoriteParameterValue_%1").arg(name));
    setRoleStyleSheet(row.value, ColorRole::TextSecondary);
    grid->addWidget(row.value, 1, 1);
    row.error = smallLabel(row.block);
    row.error->setObjectName(QStringLiteral("favoriteParameterError_%1").arg(name));
    setErrorStyleSheet(row.error);
    grid->addWidget(row.error, 2, 0, 1, 2);
    if (row.data.slider) {
      // On a row of its own under the name and the field, the card's width.
      auto* slider = new QSlider(Qt::Horizontal, row.block);
      slider->setObjectName(QStringLiteral("favoriteParameterSlider_%1").arg(name));
      slider->setProperty(kNameProperty, name);
      slider->setRange(0, row.data.slider->positions);
      slider->setSingleStep(1);
      slider->setPageStep(std::max(1, row.data.slider->positions / 10));
      slider->setFocusPolicy(Qt::StrongFocus);
      slider->setAccessibleName(tr("%1 slider").arg(name));
      // A drag sets the value when it is let go; the keys and a click on
      // the groove at once.
      connect(slider, &QSlider::sliderMoved, this, [this, name](int position) { handleMoved(name, position); });
      connect(slider, &QSlider::sliderReleased, this, [this, name, slider] { sliderSet(name, slider->value()); });
      connect(slider, &QSlider::valueChanged, this, [this, name, slider](int position) {
        if (!slider->isSliderDown()) {
          sliderSet(name, position);
        }
      });
      row.slider = slider;
      grid->addWidget(slider, 3, 0, 1, 2);
    }
    m_layout->insertWidget(index++, row.block);
  }
}

void FavoriteParametersCard::showValues() {
  m_empty->setVisible(m_rows.isEmpty());
  QStringList state;
  for (Row& row : m_rows) {
    const FavoriteRow& favorite = row.data;
    const auto refused = m_refused.constFind(favorite.name);
    // The row as the model has it, and a refused value the field keeps.
    state << favoriteRowLog(favorite) + (refused != m_refused.cend()
                                         ? QStringLiteral(" [typed %1, refused: %2]")
                                               .arg(refused.value().first, refused.value().second)
                                         : QString());
    const bool typing = row.field->hasFocus() && row.field->isModified();
    if (refused != m_refused.cend()) {
      // As typed, with a red frame and the model's reason under it.
      if (!typing) {
        row.field->setText(refused.value().first);
      }
      setErrorStyleSheet(row.field, QStringLiteral("QLineEdit { border: 1px solid %1; }"));
      row.error->setText(refused.value().second);
      row.error->show();
    } else {
      if (!typing) {
        row.field->setText(favorite.expression);
        row.field->setModified(false);
        if (!row.field->hasFocus()) {
          // Shown from its start: a long expression keeps its first
          // characters in sight.
          row.field->setCursorPosition(0);
        }
      }
      setErrorStyleSheet(row.field, QString());
      row.error->clear();
      row.error->hide();
    }
    QString tip = favorite.reference ? tr("Measured by a driven dimension; make it driving to set it")
                                 : tr("Enter sends the value; Esc restores it");
    if (favorite.formula) {
      tip = tr("%1 = %2").arg(favorite.expression, favorite.valueText);
    }
    row.field->setToolTip(tip);
    row.value->setText(tr("= %1").arg(favorite.valueText));
    row.value->setVisible(favorite.formula);
    if (row.slider != nullptr && favorite.slider) {
      const QSignalBlocker blocker(row.slider);
      if (!row.slider->isSliderDown()) {
        row.slider->setValue(favorite.slider->position);
      }
      row.slider->setToolTip(tr("%1 to %2").arg(favorite.slider->minText, favorite.slider->maxText));
    }
  }
  const QString logged = state.isEmpty() ? QStringLiteral("none") : state.join(QStringLiteral("; "));
  if (logged != m_loggedState) {
    m_loggedState = logged;
    qDebug().noquote() << QStringLiteral("Favorite parameters card: %1").arg(logged);
  }
}

bool FavoriteParametersCard::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::KeyPress || event->type() == QEvent::ShortcutOverride) {
    const auto* key = static_cast<QKeyEvent*>(event);
    const QString name = watched->property(kNameProperty).toString();
    const bool field = !name.isEmpty() && qobject_cast<QLineEdit*>(watched) != nullptr;
    if (field && event->type() == QEvent::ShortcutOverride &&
        (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter)) {
      // Enter is the field's (it sends the value), not the window's OK or
      // Repeat, also while a command's panel is open.
      event->accept();
      return true;
    }
    if (key->key() == Qt::Key_Escape && key->modifiers() == Qt::NoModifier && field) {
      // Esc is the field's (it restores the value), not the window's.
      event->accept();
      if (event->type() == QEvent::KeyPress) {
        restore(name);
      }
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void FavoriteParametersCard::fieldEdited(const QString& name) {
  Row* edited = row(name);
  if (edited == nullptr || !edited->field->isModified() || edited->data.reference) {
    return; // the focus left without an edit
  }
  edited->field->setModified(false);
  const QString typed = edited->field->text().trimmed();
  qDebug().noquote() << QStringLiteral("Favorite parameter %1 typed: %2").arg(name, typed);
  if (typed.isEmpty() || (typed == edited->data.expression && !m_refused.contains(name))) {
    restore(name);
    return;
  }
  send(name, favoriteExpression(typed, edited->data.unit), typed);
}

void FavoriteParametersCard::restore(const QString& name) {
  Row* restored = row(name);
  if (restored == nullptr) {
    return;
  }
  m_refused.remove(name);
  restored->field->setModified(false);
  qDebug().noquote() << QStringLiteral("Favorite parameter %1 restored: %2").arg(name, restored->data.expression);
  showValues();
  emit contentChanged();
}

void FavoriteParametersCard::handleMoved(const QString& name, int position) {
  Row* moved = row(name);
  if (moved == nullptr || !moved->data.slider) {
    return;
  }
  // The value at the handle in the field and in a tooltip at the handle,
  // while it is dragged; sent when it is let go.
  const QString text = sliderExpression(*moved->data.slider, position, moved->data.unit);
  moved->field->setText(text);
  moved->field->setCursorPosition(0);
  showHandleTip(*moved);
  const QString line = QStringLiteral("Favorite parameters %1 at the handle: %2").arg(name, text);
  if (line != m_loggedHandle) {
    m_loggedHandle = line;
    qDebug().noquote() << line;
  }
}

void FavoriteParametersCard::sliderSet(const QString& name, int position) {
  Row* set = row(name);
  if (set == nullptr || !set->data.slider) {
    return;
  }
  m_loggedHandle.clear();
  QToolTip::hideText();
  if (position == set->data.slider->position) {
    showValues(); // the handle clicked, not moved
    return;
  }
  const QString expression = sliderExpression(*set->data.slider, position, set->data.unit);
  set->field->setText(expression);
  send(name, expression, expression);
}

void FavoriteParametersCard::send(const QString& name, const QString& expression, const QString& typed) {
  const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
                            {QStringLiteral("name"), name},
                            {QStringLiteral("expression"), expression}};
  // After the field's or the slider's own handling: the command refreshes
  // the card, which may make the rows again.
  afterEventWhenIdle(m_host, this, [this, name, expression, typed, command] {
    const QString blocked = m_host.readOnlyReason();
    if (!blocked.isEmpty()) {
      m_refused.insert(name, {typed, blocked});
      qDebug().noquote() << QStringLiteral("Favorite parameter %1 refused: %2: %3").arg(name, typed, blocked);
      refresh();
      return;
    }
    if (m_host.runParameterCommand(command)) {
      m_refused.remove(name);
      QString value;
      try {
        for (const QJsonValue& parameter : m_host.model().queryArray(QStringLiteral("parameters"))) {
          if (parameter.toObject().value(QStringLiteral("name")).toString() == name) {
            value = parameter.toObject().value(QStringLiteral("text")).toString();
          }
        }
      } catch (const std::exception&) {
        value.clear(); // the card shows what the model has
      }
      qDebug().noquote() << QStringLiteral("Favorite parameter %1 set: %2 = %3").arg(name, expression, value);
    } else if (m_host.lastCancelled()) {
      // Cancelled while it computed (P7): no refusal, the field shows the
      // parameter as it still is.
      m_refused.remove(name);
      qDebug().noquote() << QStringLiteral("Favorite parameter %1 cancelled: %2").arg(name, expression);
    } else {
      // The model's reason stays with the field; the parameter keeps its
      // value.
      const QString problem = mitcad::modelMessage(m_host.lastError());
      m_refused.insert(name, {typed, problem});
      qDebug().noquote() << QStringLiteral("Favorite parameter %1 refused: %2: %3").arg(name, typed, problem);
    }
    refresh();
  });
}

QPoint FavoriteParametersCard::handleCenter(const QSlider* slider) const {
  QStyleOptionSlider option;
  option.initFrom(slider);
  option.orientation = slider->orientation();
  option.minimum = slider->minimum();
  option.maximum = slider->maximum();
  option.sliderPosition = slider->sliderPosition();
  option.sliderValue = slider->value();
  option.singleStep = slider->singleStep();
  option.pageStep = slider->pageStep();
  option.upsideDown = slider->invertedAppearance();
  option.subControls = QStyle::SC_SliderGroove | QStyle::SC_SliderHandle;
  return slider->style()
      ->subControlRect(QStyle::CC_Slider, &option, QStyle::SC_SliderHandle, slider)
      .center();
}

void FavoriteParametersCard::showHandleTip(const Row& row) {
  if (row.slider == nullptr) {
    return;
  }
  const QPoint handle = handleCenter(row.slider);
  QToolTip::showText(row.slider->mapToGlobal(QPoint(handle.x(), 0)), row.field->text(), row.slider);
}

int FavoriteParametersCard::contentHeight() const {
  if (m_layout->hasHeightForWidth() && width() > 0) {
    return m_layout->totalHeightForWidth(width());
  }
  return sizeHint().height();
}

void FavoriteParametersCard::logLayout() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  const auto place = [&lines](const QString& what, const QWidget* widget, const QPoint& point) {
    if (widget == nullptr || !widget->isVisible()) {
      return;
    }
    const QPoint at = GlassCard::mapToHost(widget, point);
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  for (const Row& row : std::as_const(m_rows)) {
    const QString what = QStringLiteral("Favorite parameters %1").arg(row.data.name);
    if (row.field != nullptr) {
      place(what, row.field, row.field->rect().center());
    }
    if (row.slider != nullptr) {
      place(what + QStringLiteral(" slider"), row.slider, row.slider->rect().center());
      place(what + QStringLiteral(" slider handle"), row.slider, handleCenter(row.slider));
    }
  }
  if (m_openButton != nullptr) {
    place(QStringLiteral("Favorite parameters open"), m_openButton, m_openButton->rect().center());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
