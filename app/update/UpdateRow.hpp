// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9, mitcad#1890): the update's row, one for the
// whole application. Every main window has a bar of its own ("updateBar",
// an UpdateNotice in it) under its toolbar, and each bar shows what the row
// holds: an offer, a check's or a download's progress, or a message. A
// window opened later shows it at once, a button clicked in any window
// acts once for the application, and a window that closes takes only its
// own bar along: the row, and the download it tells of, stay with the
// windows left. Logs "Update row: in N window(s)" as windows come and go
// (each notice logs its own "Update notice (<state>): ...").

#include <QList>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QtGlobal>

#include "framework/NoticeLevel.hpp"

class QMainWindow;
class QToolBar;

namespace mitcad {

class UpdateNotice;

class UpdateRow : public QObject {
  Q_OBJECT

public:
  enum class Kind { Empty, Offer, Progress, Message };

  explicit UpdateRow(QObject* parent = nullptr);

  // `window`'s bar, in its top toolbar area, showing the row as it is now.
  QToolBar* attach(QMainWindow& window);
  // The windows that have a bar.
  int windowCount() const;

  // The row's content, shown in every window (UpdateNotice says what each
  // one is).
  void showOffer(const QString& html, bool installable, bool skippable);
  void showProgress(const QString& plain, qint64 received, qint64 total);
  void showMessage(const QString& plain, NoticeLevel level = NoticeLevel::Information, bool retry = false);
  // Shown again in every window as it is (Check for Updates while a check
  // or a download is under way).
  void show();
  // Hidden in every window ("Update notice closed" when it was shown).
  void hide();

  Kind kind() const { return m_kind; }
  bool shown() const { return m_shown; }
  // The content's text, without markup.
  QString text() const;
  qint64 received() const { return m_received; }
  qint64 total() const { return m_total; }

signals:
  // From the notice of any window.
  void releaseNotesClicked();
  void installClicked();
  void skipClicked();
  void laterClicked();
  void cancelClicked();
  void retryClicked();
  // Close and Later both hide the row (closed() follows each).
  void closed();

private:
  struct Bar {
    QPointer<QToolBar> bar;
    QPointer<UpdateNotice> notice;
  };

  // One window's notice made to show the row.
  void present(UpdateNotice& notice) const;
  void presentAll();
  void prune();

  QList<Bar> m_bars;
  Kind m_kind = Kind::Empty;
  bool m_shown = false;
  // The offer's rich text, or plain text.
  QString m_text;
  bool m_installable = false;
  bool m_skippable = false;
  qint64 m_received = 0;
  qint64 m_total = 0;
  NoticeLevel m_level = NoticeLevel::Information;
  bool m_retry = false;
};

} // namespace mitcad
