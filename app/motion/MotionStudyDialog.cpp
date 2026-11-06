// SPDX-License-Identifier: MIT
#include "motion/MotionStudyDialog.hpp"

#include <algorithm>
#include <cmath>
#include <exception>

#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSlider>
#include <QSpinBox>
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

constexpr double kDegreesPerRadian = 57.29577951308232;
// A frame of the playback: this many milliseconds, one step at 1x.
constexpr int kFrameMs = 40;

QString rounded(double value) { return QString::number(std::round(value * 1000.0) / 1000.0, 'g', 10); }

bool isTurn(const QString& motion) { return motion.startsWith(QLatin1Char('r')); }

QString motionValue(const QString& motion, double value) {
  return isTurn(motion) ? rounded(value * kDegreesPerRadian) + QStringLiteral(" deg")
                        : rounded(value) + QStringLiteral(" mm");
}

// A key's value at a step: linear between its points, held outside them.
double valueAt(const QJsonArray& points, double step) {
  if (points.isEmpty()) {
    return 0.0;
  }
  const auto point = [&points](int i) {
    const QJsonArray p = points.at(i).toArray();
    return std::pair<double, double>(p.at(0).toDouble(), p.at(1).toDouble());
  };
  if (step <= point(0).first) {
    return point(0).second;
  }
  for (int i = 1; i < points.size(); ++i) {
    const auto [s0, v0] = point(i - 1);
    const auto [s1, v1] = point(i);
    if (step <= s1) {
      return s1 - s0 < 1e-12 ? v1 : v0 + (v1 - v0) * (step - s0) / (s1 - s0);
    }
  }
  return point(static_cast<int>(points.size()) - 1).second;
}

} // namespace

MotionStudyDialog::MotionStudyDialog(MotionStudyHost host, const QString& study, QWidget* parent)
    : QDialog(parent), m_host(std::move(host)), m_study(study) {
  setObjectName(QStringLiteral("motionStudyDialog"));
  setWindowTitle(tr("Motion Study"));
  setModal(false);

  auto* layout = new QVBoxLayout(this);
  auto* form = new QFormLayout;
  m_studies = new QComboBox(this);
  m_studies->setObjectName(QStringLiteral("motionStudies"));
  form->addRow(tr("Study"), m_studies);
  m_steps = new QSpinBox(this);
  m_steps->setRange(1, 100000);
  form->addRow(tr("Steps"), m_steps);
  layout->addLayout(form);

  auto* time = new QHBoxLayout;
  m_slider = new QSlider(Qt::Horizontal, this);
  m_slider->setObjectName(QStringLiteral("motionStep"));
  m_stepLabel = new QLabel(this);
  m_stepLabel->setMinimumWidth(fontMetrics().horizontalAdvance(QStringLiteral("100000 / 100000")));
  time->addWidget(m_slider, 1);
  time->addWidget(m_stepLabel);
  layout->addLayout(time);

  auto* transport = new QHBoxLayout;
  m_play = new QPushButton(tr("&Play"), this);
  m_play->setCheckable(true);
  mac::auxiliaryButton(m_play);
  m_loop = new QCheckBox(tr("&Loop"), this);
  m_speed = new QComboBox(this);
  for (const double speed : {0.25, 0.5, 1.0, 2.0, 4.0}) {
    m_speed->addItem(QStringLiteral("%1x").arg(speed), speed);
  }
  m_speed->setCurrentIndex(2);
  transport->addWidget(m_play);
  transport->addWidget(m_loop);
  transport->addWidget(new QLabel(tr("Speed"), this));
  transport->addWidget(m_speed);
  transport->addStretch(1);
  layout->addLayout(transport);

  m_keys = new QTreeWidget(this);
  panel::setCompactIndentation(m_keys, panel::Indent::Flat);
  m_keys->setObjectName(QStringLiteral("motionKeys"));
  m_keys->setRootIsDecorated(false);
  m_keys->setHeaderLabels({tr("Joint"), tr("Motion"), tr("Keys (step: value)"), tr("At step")});
  m_keys->header()->setStretchLastSection(false);
  m_keys->header()->setSectionResizeMode(2, QHeaderView::Stretch);
  mac::styleListTree(m_keys);
  layout->addWidget(m_keys, 1);

  auto* keying = new QHBoxLayout;
  m_motion = new QComboBox(this);
  m_motion->setToolTip(tr("The joint's free motion to key at the current step"));
  m_value = new QLineEdit(this);
  m_value->setObjectName(QStringLiteral("motionKeyValue"));
  m_value->setPlaceholderText(tr("where the joint is"));
  m_value->setToolTip(tr("The value to key (30 deg, 12 mm); empty: where the joint is in the design"));
  auto* add = new QPushButton(tr("Add &Key"), this);
  auto* remove = new QPushButton(tr("&Remove Key"), this);
  auto* valueLabel = new QLabel(tr("&Value"), this);
  valueLabel->setBuddy(m_value);
  keying->addWidget(m_motion, 1);
  keying->addWidget(valueLabel);
  keying->addWidget(m_value, 1);
  mac::auxiliaryButton(add);
  mac::auxiliaryButton(remove);
  mac::styleListButtons(add, remove);
#ifdef MITCAD_MAC_UI
  auto* keyButtons = new QHBoxLayout;
  keyButtons->setSpacing(0);
  keyButtons->addWidget(add);
  keyButtons->addWidget(remove);
  keying->addLayout(keyButtons);
#else
  keying->addWidget(add);
  keying->addWidget(remove);
#endif
  layout->addLayout(keying);

  m_message = new QLabel(this);
  m_message->setWordWrap(true);
  layout->addWidget(m_message);

  auto* buttons = new QDialogButtonBox(this);
  QPushButton* keepButton = buttons->addButton(tr("Keep Posi&tion"), QDialogButtonBox::AcceptRole);
  keepButton->setToolTip(tr("Plays the study to the shown step in the design: one undo step"));
  buttons->addButton(QDialogButtonBox::Close);
  mac::styleDialogButtons(buttons, keepButton, true);
  layout->addWidget(buttons);

  m_timer = new QTimer(this);
  m_timer->setSingleShot(true);
  connect(m_timer, &QTimer::timeout, this, [this] { frame(); });
  // The UI tests' sync waits while it plays.
  TestSync::watch(m_timer);

  connect(m_studies, &QComboBox::currentTextChanged, this, [this](const QString& name) {
    if (!m_updating && !name.isEmpty()) {
      showStudy(name);
    }
  });
  connect(m_steps, &QSpinBox::editingFinished, this, [this] {
    if (m_updating || m_steps->value() == entry().value(QStringLiteral("steps")).toInt()) {
      return;
    }
    m_host.run({{QStringLiteral("cmd"), QStringLiteral("edit_motion_study")},
                {QStringLiteral("name"), m_study},
                {QStringLiteral("steps"), m_steps->value()}},
               nullptr);
    refresh();
  });
  connect(m_slider, &QSlider::valueChanged, this, [this](int value) {
    if (!m_updating && !m_timer->isActive()) {
      goTo(value);
    }
  });
  connect(m_play, &QPushButton::toggled, this, [this](bool on) { play(on); });
  connect(add, &QPushButton::clicked, this, [this] { addKey(); });
  connect(remove, &QPushButton::clicked, this, [this] { removeKey(); });
  connect(keepButton, &QPushButton::clicked, this, [this] { keep(); });
  connect(buttons, &QDialogButtonBox::rejected, this, [this] { close(); });

  readStudies();
  if (m_study.isEmpty()) {
    // The first study, else a new one.
    if (m_studies->count() > 0) {
      m_study = m_studies->itemText(0);
    } else {
      QJsonObject result;
      if (m_host.run({{QStringLiteral("cmd"), QStringLiteral("add_motion_study")}}, &result)) {
        m_study = result.value(QStringLiteral("name")).toString();
        readStudies();
      }
    }
  }
  showStudy(m_study);
  resize(560, 420);
  mac::styleDialog(this);
}

MotionStudyDialog::~MotionStudyDialog() = default;

QJsonObject MotionStudyDialog::entry() const {
  try {
    for (const QJsonValue& value : m_host.model->queryArray(QStringLiteral("motion_studies"))) {
      if (value.toObject().value(QStringLiteral("name")).toString() == m_study) {
        return value.toObject();
      }
    }
  } catch (const std::exception&) {
  }
  return QJsonObject();
}

void MotionStudyDialog::readStudies() {
  m_updating = true;
  m_studies->clear();
  try {
    for (const QJsonValue& value : m_host.model->queryArray(QStringLiteral("motion_studies"))) {
      m_studies->addItem(value.toObject().value(QStringLiteral("name")).toString());
    }
    m_joints = m_host.model->queryObject({{QStringLiteral("query"), QStringLiteral("joints")}});
  } catch (const std::exception&) {
  }
  m_studies->setCurrentText(m_study);
  m_updating = false;
}

void MotionStudyDialog::showStudy(const QString& name) {
  stop(QString());
  if (m_previewed) {
    m_host.endPreview();
    m_previewed = false;
  }
  m_study = name;
  m_step = 0.0;
  readStudies();
  const QJsonObject study = entry();
  if (study.isEmpty()) {
    m_message->setText(tr("There is no motion study %1.").arg(name));
    return;
  }
  m_updating = true;
  m_steps->setValue(study.value(QStringLiteral("steps")).toInt());
  m_slider->setRange(0, study.value(QStringLiteral("steps")).toInt());
  m_slider->setValue(0);
  m_updating = false;
  fillJoints();
  fillKeys();
  m_message->clear();
  qDebug().noquote() << QStringLiteral("Motion study %1 opened: %2 steps, %3 keys")
                            .arg(m_study)
                            .arg(study.value(QStringLiteral("steps")).toInt())
                            .arg(study.value(QStringLiteral("keys")).toArray().size());
}

void MotionStudyDialog::refresh() {
  if (m_timer->isActive()) {
    return;
  }
  readStudies();
  const QJsonObject study = entry();
  if (study.isEmpty()) {
    if (m_studies->count() > 0) {
      showStudy(m_studies->itemText(0));
    } else {
      close();
    }
    return;
  }
  m_updating = true;
  m_steps->setValue(study.value(QStringLiteral("steps")).toInt());
  m_slider->setRange(0, study.value(QStringLiteral("steps")).toInt());
  m_slider->setValue(static_cast<int>(std::lround(m_step)));
  m_updating = false;
  fillJoints();
  fillKeys();
}

void MotionStudyDialog::fillJoints() {
  const QString current = m_motion->currentData().toString();
  m_motion->clear();
  for (const QJsonValue& value : m_joints.value(QStringLiteral("joints")).toArray()) {
    const QJsonObject joint = value.toObject();
    for (const QJsonValue& motion : joint.value(QStringLiteral("motions")).toArray()) {
      const QString uid = joint.value(QStringLiteral("uid")).toString();
      m_motion->addItem(QStringLiteral("%1 %2").arg(joint.value(QStringLiteral("name")).toString(), motion.toString()),
                        QStringLiteral("%1 %2").arg(uid, motion.toString()));
    }
  }
  const int at = m_motion->findData(current);
  if (at >= 0) {
    m_motion->setCurrentIndex(at);
  }
}

void MotionStudyDialog::fillKeys() {
  const QJsonObject study = entry();
  m_keys->clear();
  for (const QJsonValue& value : study.value(QStringLiteral("keys")).toArray()) {
    const QJsonObject key = value.toObject();
    const QString motion = key.value(QStringLiteral("motion")).toString();
    const QJsonArray points = key.value(QStringLiteral("points")).toArray();
    QStringList shown;
    for (const QJsonValue& point : points) {
      const QJsonArray p = point.toArray();
      shown << QStringLiteral("%1: %2").arg(rounded(p.at(0).toDouble()), motionValue(motion, p.at(1).toDouble()));
    }
    auto* item = new QTreeWidgetItem(m_keys);
    item->setText(0, key.value(QStringLiteral("joint_name")).toString());
    item->setText(1, motion);
    item->setText(2, shown.join(QStringLiteral(", ")));
    item->setText(3, motionValue(motion, valueAt(points, m_step)));
    item->setData(0, Qt::UserRole, key.value(QStringLiteral("joint")).toString());
  }
  m_stepLabel->setText(QStringLiteral("%1 / %2").arg(rounded(m_step)).arg(m_slider->maximum()));
}

QString MotionStudyDialog::valuesText(const QJsonObject& values) const {
  QStringList parts;
  for (auto joint = values.begin(); joint != values.end(); ++joint) {
    QString name = joint.key();
    for (const QJsonValue& value : m_joints.value(QStringLiteral("joints")).toArray()) {
      if (value.toObject().value(QStringLiteral("uid")).toString() == joint.key()) {
        name = value.toObject().value(QStringLiteral("name")).toString();
      }
    }
    const QJsonObject motions = joint.value().toObject();
    for (auto motion = motions.begin(); motion != motions.end(); ++motion) {
      parts << QStringLiteral("%1 %2 %3").arg(name, motion.key(), motionValue(motion.key(), motion.value().toDouble()));
    }
  }
  return parts.join(QStringLiteral(", "));
}

bool MotionStudyDialog::goTo(double step) {
  if (!m_host.ready()) {
    stop(tr("the design is busy"));
    return false;
  }
  if (entry().value(QStringLiteral("keys")).toArray().isEmpty()) {
    m_message->setText(tr("Add keys to play the study: a joint's motion and its value at a step."));
    m_step = step;
    fillKeys();
    return true;
  }
  const double from = m_step;
  QJsonObject answer;
  try {
    answer = m_host.preview({{QStringLiteral("cmd"), QStringLiteral("play_study")},
                             {QStringLiteral("study"), m_study},
                             {QStringLiteral("step"), step},
                             {QStringLiteral("from"), from}});
  } catch (const std::exception& e) {
    stop(QString::fromUtf8(e.what()));
    m_message->setText(QString::fromUtf8(e.what()));
    return false;
  }
  m_previewed = true;
  const QJsonObject contact = answer.value(QStringLiteral("contact")).toObject();
  const double fraction = answer.value(QStringLiteral("fraction")).toDouble(1.0);
  m_step = contact.isEmpty() ? step : from + (step - from) * fraction;
  m_updating = true;
  m_slider->setValue(static_cast<int>(std::lround(m_step)));
  m_updating = false;
  fillKeys();
  if (!contact.isEmpty()) {
    const QString message = contact.value(QStringLiteral("message")).toString();
    m_message->setText(message);
    m_host.status(tr("%1: %2").arg(m_study, message), false);
    qDebug().noquote() << QStringLiteral("Playback of %1 stopped at step %2: %3")
                              .arg(m_study, rounded(m_step), message);
    m_lastValues = valuesText(answer.value(QStringLiteral("values")).toObject());
    return false;
  }
  m_message->clear();
  m_lastValues = valuesText(answer.value(QStringLiteral("values")).toObject());
  return true;
}

void MotionStudyDialog::play(bool on) {
  if (!on) {
    if (m_timer->isActive()) {
      stop(QStringLiteral("paused"));
    }
    return;
  }
  m_play->setText(tr("&Pause"));
  if (m_step >= m_slider->maximum() && !m_loop->isChecked()) {
    // From the start again.
    goTo(0.0);
  }
  qDebug().noquote() << QStringLiteral("Playing %1 from step %2").arg(m_study, rounded(m_step));
  m_timer->start(kFrameMs);
}

void MotionStudyDialog::frame() {
  const double steps = m_slider->maximum();
  double next = m_step + m_speed->currentData().toDouble();
  if (m_step >= steps) {
    if (!m_loop->isChecked()) {
      stop(QString());
      return;
    }
    // Loop: back to the start, along the study.
    next = 0.0;
  }
  next = std::min(next, steps);
  if (!goTo(next)) {
    stop(QStringLiteral("contact"));
    return;
  }
  if (m_step >= steps && !m_loop->isChecked()) {
    stop(QString());
    return;
  }
  m_timer->start(kFrameMs);
}

void MotionStudyDialog::stop(const QString& why) {
  const bool playing = m_timer->isActive() || m_play->isChecked();
  m_timer->stop();
  m_updating = true;
  m_play->setChecked(false);
  m_play->setText(tr("&Play"));
  m_updating = false;
  if (!playing) {
    return;
  }
  if (why.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Played %1 to step %2: %3").arg(m_study, rounded(m_step), m_lastValues);
  } else {
    qDebug().noquote() << QStringLiteral("Paused %1 at step %2 (%3): %4")
                              .arg(m_study, rounded(m_step), why, m_lastValues);
  }
}

void MotionStudyDialog::addKey() {
  const QStringList motion = m_motion->currentData().toString().split(QLatin1Char(' '));
  if (motion.size() != 2) {
    m_message->setText(tr("The design has no joint with a free motion to key."));
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_study_key")},
                      {QStringLiteral("study"), m_study},
                      {QStringLiteral("joint"), motion.at(0)},
                      {QStringLiteral("motion"), motion.at(1)},
                      {QStringLiteral("step"), m_step}};
  const QString value = m_value->text().trimmed();
  if (!value.isEmpty()) {
    command.insert(QStringLiteral("value"), value);
  }
  QJsonObject result;
  if (!m_host.run(command, &result)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Key added to %1: %2 at step %3: %4")
                            .arg(m_study, m_motion->currentText(), rounded(m_step),
                                 motionValue(motion.at(1), result.value(QStringLiteral("value")).toDouble()));
  refresh();
}

void MotionStudyDialog::removeKey() {
  QTreeWidgetItem* item = m_keys->currentItem();
  if (item == nullptr) {
    m_message->setText(tr("Select a key in the list."));
    return;
  }
  if (!m_host.run({{QStringLiteral("cmd"), QStringLiteral("remove_study_key")},
                   {QStringLiteral("study"), m_study},
                   {QStringLiteral("joint"), item->data(0, Qt::UserRole).toString()},
                   {QStringLiteral("motion"), item->text(1)},
                   {QStringLiteral("step"), m_step}},
                  nullptr)) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Key removed from %1: %2 %3 at step %4")
                            .arg(m_study, item->text(0), item->text(1), rounded(m_step));
  refresh();
}

void MotionStudyDialog::keep() {
  stop(QStringLiteral("kept"));
  QJsonObject result;
  if (!m_host.run({{QStringLiteral("cmd"), QStringLiteral("play_study")},
                   {QStringLiteral("study"), m_study},
                   {QStringLiteral("step"), m_step}},
                  &result)) {
    return;
  }
  m_previewed = false;
  qDebug().noquote() << QStringLiteral("Kept %1 at step %2: %3")
                            .arg(m_study, rounded(m_step),
                                 valuesText(result.value(QStringLiteral("values")).toObject()));
  close();
}

void MotionStudyDialog::finish() {
  if (m_finished) {
    return;
  }
  m_finished = true;
  stop(QStringLiteral("closed"));
  if (m_previewed) {
    m_host.endPreview();
    m_previewed = false;
  }
  qDebug().noquote() << QStringLiteral("Motion study %1 closed").arg(m_study);
  deleteLater();
}

void MotionStudyDialog::reject() {
  finish();
  QDialog::reject();
}

void MotionStudyDialog::closeEvent(QCloseEvent* event) {
  finish();
  QDialog::closeEvent(event);
}

} // namespace mitcad
