// SPDX-License-Identifier: MIT
// Simulate's window (mitcad#1013; CamSimulateDialog.hpp).
#include "CamSimulateDialog.hpp"

#include <algorithm>

#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QStringList>
#include <QVBoxLayout>
#include <QtLogging>

namespace mitcad {
namespace {

// Play's pace: a step every 30 ms, about 20 seconds from start to end.
constexpr int kStepMs = 30;
constexpr int kPlaySteps = 660;

} // namespace

CamSimulateDialog::CamSimulateDialog(QWidget* parent) : QDialog(parent) {
  setObjectName(QStringLiteral("camSimulateDialog"));
  setWindowTitle(tr("Simulate"));
  auto* layout = new QVBoxLayout(this);
  m_label = new QLabel;
  m_label->setObjectName(QStringLiteral("camSimulatePlace"));
  layout->addWidget(m_label);
  auto* row = new QHBoxLayout;
  m_play = new QPushButton(tr("Play"));
  m_play->setObjectName(QStringLiteral("camSimulatePlay"));
  m_play->setToolTip(tr("Moves the tool along the toolpaths, or stops it"));
  m_play->setAutoDefault(false);
  row->addWidget(m_play);
  m_slider = new QSlider(Qt::Horizontal);
  m_slider->setObjectName(QStringLiteral("camSimulateSlider"));
  m_slider->setMinimumWidth(320);
  m_slider->setToolTip(tr("The tool's place along the setup's toolpaths"));
  row->addWidget(m_slider, 1);
  layout->addLayout(row);
  m_remaining = new QLabel;
  m_remaining->setObjectName(QStringLiteral("camSimulateRemaining"));
  m_remaining->setWordWrap(true);
  m_remaining->hide();
  layout->addWidget(m_remaining);
  m_legend = new QLabel;
  m_legend->setObjectName(QStringLiteral("camSimulateLegend"));
  m_legend->setTextFormat(Qt::RichText);
  m_legend->setWordWrap(true);
  m_legend->hide();
  layout->addWidget(m_legend);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  connect(m_play, &QPushButton::clicked, this, [this] { togglePlay(); });
  connect(m_slider, &QSlider::valueChanged, this, [this](int value) { emit positionChanged(value); });
  m_timer.setInterval(kStepMs);
  connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
}

void CamSimulateDialog::setPlayback(int points) {
  m_timer.stop();
  m_play->setText(tr("Play"));
  m_slider->setRange(0, std::max(0, points - 1));
  m_slider->setPageStep(std::max(1, points / 20));
  m_slider->setValue(0);
  emit positionChanged(0);
}

int CamSimulateDialog::position() const { return m_slider->value(); }

void CamSimulateDialog::setPositionText(const QString& text) { m_label->setText(text); }

void CamSimulateDialog::setRemaining(const QString& text, const QList<QColor>& colors) {
  m_remaining->setText(text);
  m_remaining->setVisible(!text.isEmpty());
  const QStringList words{tr("Nothing left"), tr("A little left"), tr("More left"), tr("Cut into the part")};
  QStringList items;
  for (qsizetype k = 0; k < words.size() && k < colors.size(); ++k) {
    items << QStringLiteral("<span style=\"color:%1\">&#9632;</span>&nbsp;%2")
                 .arg(colors.at(k).name(), words.at(k).toHtmlEscaped());
  }
  m_legend->setText(items.join(QStringLiteral(" &nbsp; ")));
  m_legend->setVisible(!text.isEmpty() && !items.isEmpty());
}

void CamSimulateDialog::togglePlay() {
  if (m_timer.isActive()) {
    m_timer.stop();
    m_play->setText(tr("Play"));
    qDebug().noquote() << QStringLiteral("CAM simulate paused at %1").arg(m_slider->value());
    return;
  }
  if (m_slider->value() >= m_slider->maximum()) {
    m_slider->setValue(0);
  }
  m_play->setText(tr("Pause"));
  m_timer.start();
  qDebug().noquote() << QStringLiteral("CAM simulate playing from %1").arg(m_slider->value());
}

void CamSimulateDialog::step() {
  const int stride = std::max(1, (m_slider->maximum() + 1) / kPlaySteps);
  const int next = std::min(m_slider->maximum(), m_slider->value() + stride);
  m_slider->setValue(next);
  if (next >= m_slider->maximum()) {
    m_timer.stop();
    m_play->setText(tr("Play"));
    qDebug().noquote() << QStringLiteral("CAM simulate played to the end");
  }
}

void CamSimulateDialog::hideEvent(QHideEvent* event) {
  m_timer.stop();
  m_play->setText(tr("Play"));
  QDialog::hideEvent(event);
}

} // namespace mitcad
