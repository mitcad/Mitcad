// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QString>

#include "mitcad_bridge/lib.h"

class QCloseEvent;
class QLabel;
class QProgressBar;
class QPushButton;

namespace mitcad {

// The progress of a long computation (P7, MainWindow::runJob), modal to the
// window it blocks: the stage ("Opening part.mitcad"), the feature being
// computed of how many, the time so far, and Cancel. Cancel, Esc and
// closing the dialog all ask the job to stop (Enter does not); the dialog
// then says it is cancelling until the job is back and the dialog goes.
class ComputeProgress : public QDialog {
  Q_OBJECT

public:
  explicit ComputeProgress(QWidget* parent);

  void showProgress(const QString& stage, const JobProgress& progress, qint64 elapsedMs);
  bool isCancelling() const { return m_cancelling; }

signals:
  // The first time the user asks to stop.
  void cancelRequested();

protected:
  void reject() override;
  void closeEvent(QCloseEvent* event) override;

private:
  void requestCancel();

  QLabel* m_stage = nullptr;
  QLabel* m_feature = nullptr;
  QProgressBar* m_bar = nullptr;
  QLabel* m_time = nullptr;
  QPushButton* m_cancel = nullptr;
  bool m_cancelling = false;
};

} // namespace mitcad
