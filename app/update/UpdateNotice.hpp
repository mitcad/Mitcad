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
  // `html` is rich text: the caller escapes every value in it
  // (toHtmlEscaped), as values come from the network (mitcad#938).
  void showOffer(const QString& html, bool installable);
  // A download or check under way; total 0: no percentage. Plain text.
  void showProgress(const QString& plain, qint64 received, qint64 total);
  // Plain text, shown as it is: an error may carry a server's text.
  void showMessage(const QString& plain, bool error = false);
  // The notice's text as shown, without markup.
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
  void present(const QString& state, const QString& html);

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
