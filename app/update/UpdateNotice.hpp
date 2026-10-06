// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9): the notice under the toolbar. It never
// blocks the window: an offer (Release Notes, Install, Skip This Version,
// Later), a download's progress (Cancel) or a message (Close). Each state
// is logged ("Update notice: ..."), the UI tests read it.

#include <QString>
#include <QWidget>

class QLabel;
class QProgressBar;
class QPushButton;

namespace mitcad {

class UpdateNotice : public QWidget {
  Q_OBJECT

public:
  explicit UpdateNotice(QWidget* parent = nullptr);

  // A newer release; `installable`: Install, else the release page only.
  void showOffer(const QString& text, bool installable);
  // A download or check under way; total 0: no percentage.
  void showProgress(const QString& text, qint64 received, qint64 total);
  void showMessage(const QString& text, bool error = false);
  QString text() const;

signals:
  void releaseNotesClicked();
  void installClicked();
  void skipClicked();
  void laterClicked();
  void cancelClicked();
  // Close and Later both hide the notice (closed() follows each).
  void closed();

private:
  void present(const QString& state, const QString& text);

  QLabel* m_text = nullptr;
  QProgressBar* m_progress = nullptr;
  QPushButton* m_notes = nullptr;
  QPushButton* m_install = nullptr;
  QPushButton* m_skip = nullptr;
  QPushButton* m_later = nullptr;
  QPushButton* m_cancel = nullptr;
  QPushButton* m_close = nullptr;
};

} // namespace mitcad
