// SPDX-License-Identifier: MIT
#include "ComputeProgress.hpp"

#include <algorithm>

#include <QCloseEvent>
#include <QDialogButtonBox>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QVBoxLayout>

namespace mitcad {

ComputeProgress::ComputeProgress(QWidget* parent)
    : QDialog(parent), m_stage(new QLabel(this)), m_feature(new QLabel(this)),
      m_bar(new QProgressBar(this)), m_time(new QLabel(this)) {
  setObjectName(QStringLiteral("computeProgress"));
  setWindowTitle(tr("Computing"));
  setWindowModality(Qt::WindowModal);
  setMinimumWidth(420);
  auto* layout = new QVBoxLayout(this);
  layout->addWidget(m_stage);
  layout->addWidget(m_feature);
  m_bar->setTextVisible(false);
  layout->addWidget(m_bar);
  layout->addWidget(m_time);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, this);
  m_cancel = buttons->button(QDialogButtonBox::Cancel);
  m_cancel->setText(tr("Cancel"));
  // Enter or Space meant for the window (OK of a panel) must not cancel:
  // only a click and Esc do.
  m_cancel->setAutoDefault(false);
  m_cancel->setDefault(false);
  m_cancel->setFocusPolicy(Qt::NoFocus);
  connect(m_cancel, &QPushButton::clicked, this, &ComputeProgress::requestCancel);
  layout->addWidget(buttons);
}

void ComputeProgress::showProgress(const QString& stage, const JobProgress& progress, qint64 elapsedMs) {
  m_stage->setText(stage);
  const QString feature = QString::fromUtf8(progress.feature.data(), static_cast<qsizetype>(progress.feature.size()));
  if (progress.total > 0) {
    const int total = static_cast<int>(progress.total);
    const int position = static_cast<int>(std::min(progress.position, progress.total));
    m_bar->setRange(0, total);
    m_bar->setValue(position);
    const int shown = std::min(position + 1, total);
    m_feature->setText(feature.isEmpty() ? tr("Feature %1 of %2").arg(shown).arg(total)
                                         : tr("%1 (%2 of %3)").arg(feature).arg(shown).arg(total));
  } else {
    m_bar->setRange(0, 0); // busy, nothing counted yet
    m_feature->setText(QString());
  }
  const qint64 seconds = elapsedMs / 1000;
  m_time->setText(m_cancelling ? tr("Cancelling... (%1 s)").arg(seconds) : tr("%1 s elapsed").arg(seconds));
}

void ComputeProgress::reject() { requestCancel(); }

void ComputeProgress::closeEvent(QCloseEvent* event) {
  event->ignore();
  requestCancel();
}

void ComputeProgress::requestCancel() {
  if (m_cancelling) {
    return;
  }
  m_cancelling = true;
  m_cancel->setEnabled(false);
  m_time->setText(tr("Cancelling..."));
  emit cancelRequested();
}

} // namespace mitcad
