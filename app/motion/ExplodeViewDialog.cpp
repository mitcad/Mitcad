// SPDX-License-Identifier: MIT
#include "motion/ExplodeViewDialog.hpp"

#include <algorithm>
#include <cmath>
#include <exception>

#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QElapsedTimer>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "framework/Command.hpp"
#include "framework/PanelTree.hpp"
#include "framework/TestSync.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

// A frame of the playback, milliseconds.
constexpr int kFrameMs = 40;
// The slider's ticks over the whole animation.
constexpr int kTicks = 1000;

QString secondsText(double value) { return QString::number(std::round(value * 100.0) / 100.0, 'f', 2); }

// What a step moves, in words: its components' and bodies' names.
QString movedNames(const QJsonObject& step) {
  QStringList names;
  for (const QJsonValue& value : step.value(QStringLiteral("occurrences")).toArray()) {
    names << value.toObject().value(QStringLiteral("name")).toString();
  }
  for (const QJsonValue& value : step.value(QStringLiteral("bodies")).toArray()) {
    names << value.toObject().value(QStringLiteral("name")).toString();
  }
  return names.join(QStringLiteral(", "));
}

// A direction or an axis as the list shows it: X, -Y, a vector, or the
// geometry's occurrence.
QString axisText(const QJsonObject& axis) {
  const QJsonArray v = axis.value(QStringLiteral("vector")).toArray();
  if (v.size() == 3) {
    const double sign = axis.value(QStringLiteral("flip")).toBool() ? -1.0 : 1.0;
    const double x = v[0].toDouble() * sign;
    const double y = v[1].toDouble() * sign;
    const double z = v[2].toDouble() * sign;
    const auto only = [](double a, double b, double c) { return std::abs(a) > 1e-9 && std::abs(b) < 1e-9 && std::abs(c) < 1e-9; };
    if (only(x, y, z)) {
      return x > 0 ? QStringLiteral("X") : QStringLiteral("-X");
    }
    if (only(y, x, z)) {
      return y > 0 ? QStringLiteral("Y") : QStringLiteral("-Y");
    }
    if (only(z, x, y)) {
      return z > 0 ? QStringLiteral("Z") : QStringLiteral("-Z");
    }
    return QStringLiteral("(%1, %2, %3)").arg(QString::number(x, 'g', 4), QString::number(y, 'g', 4), QString::number(z, 'g', 4));
  }
  const QString occurrence = axis.value(QStringLiteral("occurrence_name")).toString();
  return occurrence.isEmpty() ? ExplodeViewDialog::tr("geometry") : ExplodeViewDialog::tr("geometry of %1").arg(occurrence);
}

} // namespace

ExplodeViewDialog::ExplodeViewDialog(ExplodeViewHost host, const QString& view, QWidget* parent)
    : QDialog(parent), m_host(std::move(host)), m_view(view) {
  setObjectName(QStringLiteral("explodeViewDialog"));
  setWindowTitle(tr("Explode"));
  setModal(false);

  auto* layout = new QVBoxLayout(this);
  auto* form = new QFormLayout;
  auto* viewRow = new QHBoxLayout;
  m_viewsBox = new QComboBox(this);
  m_viewsBox->setObjectName(QStringLiteral("explodeViews"));
  auto* newView = new QPushButton(tr("&New Exploded View"), this);
  newView->setObjectName(QStringLiteral("explodeNew"));
  newView->setToolTip(tr("A new explode view: its first step's panel opens"));
  mac::auxiliaryButton(newView);
  viewRow->addWidget(m_viewsBox, 1);
  viewRow->addWidget(newView);
  form->addRow(tr("Exploded View"), viewRow);
  layout->addLayout(form);

  m_steps = new QTreeWidget(this);
  panel::setCompactIndentation(m_steps, panel::Indent::Flat);
  m_steps->setObjectName(QStringLiteral("explodeSteps"));
  m_steps->setRootIsDecorated(false);
  m_steps->setHeaderLabels({tr("Step"), tr("Components"), tr("Move"), tr("Turn"), tr("Time")});
  m_steps->header()->setStretchLastSection(false);
  m_steps->header()->setSectionResizeMode(1, QHeaderView::Stretch);
  mac::styleListTree(m_steps);
  layout->addWidget(m_steps, 1);

  auto* stepButtons = new QHBoxLayout;
  m_add = new QPushButton(tr("&Add Step"), this);
  m_add->setObjectName(QStringLiteral("explodeAddStep"));
  m_add->setToolTip(tr("Explode Step: pick components, drag the arrow or the ring, or type a distance or an angle"));
  m_edit = new QPushButton(tr("&Edit Step"), this);
  m_edit->setObjectName(QStringLiteral("explodeEditStep"));
  m_remove = new QPushButton(tr("&Remove Step"), this);
  m_remove->setObjectName(QStringLiteral("explodeRemoveStep"));
  m_up = new QPushButton(tr("&Up"), this);
  m_up->setObjectName(QStringLiteral("explodeStepUp"));
  m_down = new QPushButton(tr("&Down"), this);
  m_down->setObjectName(QStringLiteral("explodeStepDown"));
  for (QPushButton* button : {m_add, m_edit, m_remove, m_up, m_down}) {
    mac::auxiliaryButton(button);
    stepButtons->addWidget(button);
  }
  stepButtons->addStretch(1);
  layout->addLayout(stepButtons);

  auto* options = new QHBoxLayout;
  m_trails = new QCheckBox(tr("&Trail Lines"), this);
  m_trails->setObjectName(QStringLiteral("explodeTrails"));
  m_trails->setToolTip(tr("Thin dashed lines from each moved component back to its place, for every step"));
  m_factor = new QDoubleSpinBox(this);
  m_factor->setObjectName(QStringLiteral("explodeFactor"));
  m_factor->setRange(0.1, 20.0);
  m_factor->setSingleStep(0.5);
  m_factor->setDecimals(2);
  m_factor->setValue(1.0);
  m_factor->setToolTip(tr("Auto Explode moves each component by its size times this"));
  m_auto = new QPushButton(tr("A&uto Explode"), this);
  m_auto->setObjectName(QStringLiteral("explodeAuto"));
  m_auto->setToolTip(tr("Steps that move each component out from the assembly's center, in place of the view's"));
  mac::auxiliaryButton(m_auto);
  auto* factorLabel = new QLabel(tr("&Factor"), this);
  factorLabel->setBuddy(m_factor);
  options->addWidget(m_trails);
  options->addStretch(1);
  options->addWidget(factorLabel);
  options->addWidget(m_factor);
  options->addWidget(m_auto);
  layout->addLayout(options);

  auto* time = new QHBoxLayout;
  m_slider = new QSlider(Qt::Horizontal, this);
  m_slider->setObjectName(QStringLiteral("explodeTime"));
  m_slider->setRange(0, kTicks);
  m_timeLabel = new QLabel(this);
  m_timeLabel->setObjectName(QStringLiteral("explodeTimeLabel"));
  m_timeLabel->setMinimumWidth(fontMetrics().horizontalAdvance(QStringLiteral("000.00 / 000.00 s")));
  time->addWidget(m_slider, 1);
  time->addWidget(m_timeLabel);
  layout->addLayout(time);

  auto* transport = new QHBoxLayout;
  m_back = new QPushButton(tr("Play &Back"), this);
  m_back->setObjectName(QStringLiteral("explodePlayBack"));
  m_back->setToolTip(tr("Plays the steps backwards, to the design assembled"));
  m_previous = new QPushButton(tr("&Previous Step"), this);
  m_previous->setObjectName(QStringLiteral("explodePrevious"));
  m_play = new QPushButton(tr("&Play"), this);
  m_play->setObjectName(QStringLiteral("explodePlay"));
  m_play->setToolTip(tr("Plays the steps in order, to the design exploded; again to pause"));
  m_next = new QPushButton(tr("Ne&xt Step"), this);
  m_next->setObjectName(QStringLiteral("explodeNext"));
  for (QPushButton* button : {m_back, m_previous, m_play, m_next}) {
    mac::auxiliaryButton(button);
    transport->addWidget(button);
  }
  transport->addStretch(1);
  layout->addLayout(transport);

  m_message = new QLabel(this);
  m_message->setWordWrap(true);
  layout->addWidget(m_message);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
  mac::styleDialogButtons(buttons, nullptr, true);
  layout->addWidget(buttons);

  m_timer = new QTimer(this);
  m_timer->setSingleShot(true);
  connect(m_timer, &QTimer::timeout, this, [this] { frame(); });
  // The UI tests' sync waits while it plays.
  TestSync::watch(m_timer);
  m_clock = new QElapsedTimer();

  connect(m_viewsBox, &QComboBox::currentTextChanged, this, [this](const QString& name) {
    if (!m_updating && !name.isEmpty()) {
      showView(name);
    }
  });
  connect(newView, &QPushButton::clicked, this, [this] {
    if (m_host.ready()) {
      play(0);
      m_host.editStep(QString(), -1);
    }
  });
  connect(m_add, &QPushButton::clicked, this, [this] {
    if (m_host.ready()) {
      play(0);
      m_host.editStep(m_view, -1);
    }
  });
  const auto editSelected = [this] {
    const int step = selectedStep();
    if (step >= 0 && m_host.ready()) {
      play(0);
      m_host.editStep(m_view, step);
    }
  };
  connect(m_edit, &QPushButton::clicked, this, editSelected);
  connect(m_steps, &QTreeWidget::itemDoubleClicked, this, editSelected);
  connect(m_remove, &QPushButton::clicked, this, [this] { removeStep(); });
  connect(m_up, &QPushButton::clicked, this, [this] { moveStep(-1); });
  connect(m_down, &QPushButton::clicked, this, [this] { moveStep(1); });
  connect(m_trails, &QCheckBox::toggled, this, [this](bool on) {
    if (!m_updating) {
      setTrails(on);
    }
  });
  connect(m_auto, &QPushButton::clicked, this, [this] { autoExplode(); });
  connect(m_slider, &QSlider::valueChanged, this, [this](int value) {
    if (!m_updating && m_direction == 0) {
      goTo(seconds() * value / kTicks);
    }
  });
  connect(m_play, &QPushButton::clicked, this, [this] { play(m_direction == 1 ? 0 : 1); });
  connect(m_back, &QPushButton::clicked, this, [this] { play(m_direction == -1 ? 0 : -1); });
  connect(m_previous, &QPushButton::clicked, this, [this] { stepTo(-1); });
  connect(m_next, &QPushButton::clicked, this, [this] { stepTo(1); });
  connect(buttons, &QDialogButtonBox::rejected, this, [this] { close(); });

  readViews();
  if (m_view.isEmpty() && m_viewsBox->count() > 0) {
    m_view = m_viewsBox->itemText(0);
  }
  showView(m_view);
  resize(620, 460);
  mac::styleDialog(this);
}

ExplodeViewDialog::~ExplodeViewDialog() { delete m_clock; }

void ExplodeViewDialog::readViews() {
  m_updating = true;
  m_viewsBox->clear();
  try {
    m_views = m_host.model->queryArray(QStringLiteral("explode_views"));
  } catch (const std::exception&) {
    m_views = QJsonArray();
  }
  for (const QJsonValue& value : m_views) {
    m_viewsBox->addItem(value.toObject().value(QStringLiteral("name")).toString());
  }
  m_viewsBox->setCurrentText(m_view);
  m_updating = false;
}

QJsonObject ExplodeViewDialog::entry() const {
  for (const QJsonValue& value : m_views) {
    if (value.toObject().value(QStringLiteral("name")).toString() == m_view) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

double ExplodeViewDialog::seconds() const { return entry().value(QStringLiteral("seconds")).toDouble(); }

int ExplodeViewDialog::selectedStep() const {
  const QTreeWidgetItem* item = m_steps->currentItem();
  return item == nullptr ? -1 : m_steps->indexOfTopLevelItem(const_cast<QTreeWidgetItem*>(item));
}

void ExplodeViewDialog::showView(const QString& name) {
  play(0);
  m_view = name;
  readViews();
  fillSteps();
  const QJsonObject view = entry();
  if (view.isEmpty()) {
    m_message->setText(m_views.isEmpty() ? tr("No exploded view yet: Auto Explode makes one, or New Exploded View.")
                                         : tr("There is no exploded view %1.").arg(name));
    m_host.show(QString(), std::nullopt);
    qDebug().noquote() << QStringLiteral("Explode panel: no view yet");
    return;
  }
  m_message->clear();
  // Shown exploded: every step done.
  goTo(seconds());
  qDebug().noquote() << QStringLiteral("Explode panel %1: %2 step(s), %3 s")
                            .arg(m_view)
                            .arg(view.value(QStringLiteral("steps")).toArray().size())
                            .arg(secondsText(seconds()));
}

void ExplodeViewDialog::refresh() {
  if (m_direction != 0) {
    return;
  }
  const QString before = m_view;
  readViews();
  if (entry().isEmpty()) {
    // A new view (Explode Step's OK on New Exploded View), else the first.
    const QString last = m_views.isEmpty() ? QString() : m_views.last().toObject().value(QStringLiteral("name")).toString();
    if (!last.isEmpty()) {
      showView(last);
    } else {
      showView(QString());
    }
    return;
  }
  fillSteps();
  // The view after the change, exploded.
  m_time = std::min(m_time, seconds());
  goTo(before == m_view ? seconds() : m_time);
}

void ExplodeViewDialog::fillSteps() {
  const int selected = selectedStep();
  m_updating = true;
  m_steps->clear();
  const QJsonArray steps = entry().value(QStringLiteral("steps")).toArray();
  bool allTrails = !steps.isEmpty();
  for (const QJsonValue& value : steps) {
    const QJsonObject step = value.toObject();
    QString move;
    if (step.contains(QStringLiteral("distance"))) {
      move = QStringLiteral("%1 %2").arg(step.value(QStringLiteral("distance")).toString(),
                                         axisText(step.value(QStringLiteral("direction")).toObject()));
    }
    QString turn;
    if (step.contains(QStringLiteral("angle"))) {
      turn = QStringLiteral("%1 %2").arg(step.value(QStringLiteral("angle")).toString(),
                                         axisText(step.value(QStringLiteral("axis")).toObject()));
    }
    auto* item = new QTreeWidgetItem(m_steps, {step.value(QStringLiteral("name")).toString(), movedNames(step), move,
                                               turn, secondsText(step.value(QStringLiteral("seconds")).toDouble()) +
                                                         QStringLiteral(" s")});
    if (!step.value(QStringLiteral("trails")).toBool(true)) {
      allTrails = false;
      item->setToolTip(0, tr("No trail lines"));
    }
  }
  if (selected >= 0 && selected < m_steps->topLevelItemCount()) {
    m_steps->setCurrentItem(m_steps->topLevelItem(selected));
  }
  m_trails->setChecked(allTrails);
  const bool has = !entry().isEmpty();
  for (QWidget* widget : {static_cast<QWidget*>(m_add), static_cast<QWidget*>(m_edit),
                          static_cast<QWidget*>(m_remove), static_cast<QWidget*>(m_up),
                          static_cast<QWidget*>(m_down), static_cast<QWidget*>(m_trails),
                          static_cast<QWidget*>(m_play), static_cast<QWidget*>(m_back),
                          static_cast<QWidget*>(m_previous), static_cast<QWidget*>(m_next),
                          static_cast<QWidget*>(m_slider)}) {
    widget->setEnabled(has);
  }
  m_updating = false;
}

void ExplodeViewDialog::goTo(double time) {
  const double total = seconds();
  m_time = std::clamp(time, 0.0, std::max(total, 0.0));
  m_updating = true;
  m_slider->setValue(total > 0 ? static_cast<int>(std::lround(m_time / total * kTicks)) : kTicks);
  m_timeLabel->setText(QStringLiteral("%1 / %2 s").arg(secondsText(m_time), secondsText(total)));
  m_updating = false;
  if (!m_view.isEmpty()) {
    // At the end: every step done (the explode as kept).
    m_host.show(m_view, m_time >= total - 1e-9 ? std::nullopt : std::optional<double>(m_time));
  }
}

void ExplodeViewDialog::play(int direction) {
  if (direction == 0) {
    if (m_direction != 0) {
      m_timer->stop();
      m_direction = 0;
      m_play->setText(tr("&Play"));
      qDebug().noquote() << QStringLiteral("Explode playback %1 paused at %2 s").arg(m_view, secondsText(m_time));
    }
    return;
  }
  const double total = seconds();
  if (total <= 0) {
    return;
  }
  // From the end it plays from the start again (and back from the start).
  if (direction > 0 && m_time >= total - 1e-9) {
    goTo(0.0);
  } else if (direction < 0 && m_time <= 1e-9) {
    goTo(total);
  }
  m_direction = direction;
  m_play->setText(direction > 0 ? tr("&Pause") : tr("&Play"));
  m_clockStart = m_time;
  m_clock->start();
  qDebug().noquote() << QStringLiteral("Explode playback %1 %2 from %3 s")
                            .arg(m_view, direction > 0 ? QStringLiteral("forward") : QStringLiteral("back"),
                                 secondsText(m_time));
  m_timer->start(kFrameMs);
}

void ExplodeViewDialog::frame() {
  if (m_direction == 0) {
    return;
  }
  const double total = seconds();
  const double elapsed = static_cast<double>(m_clock->elapsed()) / 1000.0;
  double next = m_clockStart + m_direction * elapsed;
  const bool ended = m_direction > 0 ? next >= total : next <= 0.0;
  if (ended) {
    next = m_direction > 0 ? total : 0.0;
  }
  goTo(next);
  if (ended) {
    const int direction = m_direction;
    m_direction = 0;
    m_play->setText(tr("&Play"));
    qDebug().noquote() << QStringLiteral("Explode playback %1 ended %2 at %3 s")
                              .arg(m_view, direction > 0 ? QStringLiteral("exploded") : QStringLiteral("assembled"),
                                   secondsText(m_time));
    return;
  }
  m_timer->start(kFrameMs);
}

void ExplodeViewDialog::stepTo(int by) {
  play(0);
  // The ends of the steps: 0, then each step's end.
  QVector<double> ends{0.0};
  for (const QJsonValue& value : entry().value(QStringLiteral("steps")).toArray()) {
    ends << ends.last() + value.toObject().value(QStringLiteral("seconds")).toDouble(1.0);
  }
  double target = m_time;
  if (by > 0) {
    target = ends.last();
    for (const double end : ends) {
      if (end > m_time + 1e-9) {
        target = end;
        break;
      }
    }
  } else {
    target = 0.0;
    for (const double end : ends) {
      if (end < m_time - 1e-9) {
        target = end;
      }
    }
  }
  goTo(target);
}

bool ExplodeViewDialog::run(const QJsonObject& command) {
  play(0);
  QJsonObject result;
  const bool ok = m_host.run(command, &result);
  refresh();
  return ok;
}

void ExplodeViewDialog::autoExplode() {
  if (!m_host.ready()) {
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("auto_explode")},
                      {QStringLiteral("factor"), m_factor->value()}};
  if (!m_view.isEmpty()) {
    command.insert(QStringLiteral("name"), m_view);
  }
  play(0);
  QJsonObject result;
  if (m_host.run(command, &result)) {
    const QString name = result.value(QStringLiteral("name")).toString();
    qDebug().noquote() << QStringLiteral("Auto Explode %1: %2 step(s) by %3")
                              .arg(name)
                              .arg(result.value(QStringLiteral("steps")).toInt())
                              .arg(m_factor->value());
    showView(name);
  }
}

void ExplodeViewDialog::removeStep() {
  const int step = selectedStep();
  if (step < 0 || !m_host.ready()) {
    return;
  }
  run({{QStringLiteral("cmd"), QStringLiteral("edit_explode_view")},
       {QStringLiteral("name"), m_view},
       {QStringLiteral("remove_step"), step}});
}

void ExplodeViewDialog::moveStep(int by) {
  const int step = selectedStep();
  const int to = step + by;
  if (step < 0 || to < 0 || to >= m_steps->topLevelItemCount() || !m_host.ready()) {
    return;
  }
  if (run({{QStringLiteral("cmd"), QStringLiteral("edit_explode_view")},
           {QStringLiteral("name"), m_view},
           {QStringLiteral("move_step"), step},
           {QStringLiteral("to"), to}})) {
    m_steps->setCurrentItem(m_steps->topLevelItem(to));
  }
}

void ExplodeViewDialog::setTrails(bool on) {
  if (m_view.isEmpty() || !m_host.ready()) {
    return;
  }
  run({{QStringLiteral("cmd"), QStringLiteral("edit_explode_view")},
       {QStringLiteral("name"), m_view},
       {QStringLiteral("trails"), on}});
}

void ExplodeViewDialog::finish() {
  if (m_finished) {
    return;
  }
  m_finished = true;
  play(0);
  // Left shown exploded, every step done.
  if (!entry().isEmpty()) {
    m_host.show(m_view, std::nullopt);
  }
  qDebug().noquote() << QStringLiteral("Explode panel %1 closed").arg(m_view);
  deleteLater();
}

void ExplodeViewDialog::reject() {
  finish();
  QDialog::reject();
}

void ExplodeViewDialog::closeEvent(QCloseEvent* event) {
  finish();
  QDialog::closeEvent(event);
}

} // namespace mitcad
