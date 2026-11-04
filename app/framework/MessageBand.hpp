// SPDX-License-Identifier: MIT
#pragma once

#include <QFrame>
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QTimer>
#include <QWidget>

#include "MessageLog.hpp"
#include "NoticeLevel.hpp"

class QLabel;
class QScrollArea;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

// The window's messages (mitcad#1042, docs/ui-conventions.md "The message
// band"): the status line's messages at their levels, what the band shows
// of them and how long (MessageLog), and the session's history. One per
// window, for both layouts: the Floating message band shows it, the Docked
// status bar's history button opens its history.
//
// Logged for the UI tests: `Message band: <level> <text>` for each message,
// `Message band full: <level> <text>` when the band shows it in full and
// `Message band stub: <level> <text> (faded|closed|next command|cleared|withdrawn)`
// when the band turns into the stub of the latest message, `Message band
// cleared (another design)` when another design empties it (mitcad#1174).
class MessageCenter : public QObject {
  Q_OBJECT

public:
  explicit MessageCenter(QObject* parent = nullptr);

  const MessageLog& log() const { return m_log; }

  // A message of the status line; `feature`: the uid of the feature it is
  // about (its history row selects it), or empty. An empty text clears a
  // message shown in full that is no problem.
  void post(const QString& text, NoticeLevel level, const QString& feature = QString());
  // The user closed the message shown in full (the band's ×).
  void close();
  // The next command started: an error shown in full gives way.
  void commandStarted();
  // Another design in the window (New, Open, recovery; mitcad#1174): the
  // band shows nothing, neither a message in full nor the stub, until the
  // next message; the history keeps them. Logged `Message band cleared
  // (<reason>)`.
  void clear(const char* reason);
  // What the message with this text was about is over (mitcad#1230): shown
  // in full, it becomes the stub at once, a warning too (logged `Message
  // band stub: <level> <text> (withdrawn)`); otherwise nothing changes.
  void withdraw(const QString& text);

signals:
  // What the band shows changed (another message, or the stub).
  void changed();

private:
  void toStub(const char* reason);
  void restartTimer();

  MessageLog m_log;
  QTimer m_timer; // the message shown in full: until it fades (not watched: it is long)
};

// The message band's content (Floating, mitcad#1042): a message in full,
// with its level's icon, word and colour role, the text wrapping to two
// lines before it is elided (the whole text in its tooltip) and a close
// button (×); or, once the message faded or was closed, the dimmed
// one-line stub of the latest message, which opens the history. Its object
// name is `messageBand`, its property `message` the text shown (in full or
// in the stub), its property `state` full, stub or none.
class MessageBand : public QWidget {
  Q_OBJECT

public:
  explicit MessageBand(MessageCenter& center, QWidget* parent = nullptr);

  bool isStub() const { return m_center.log().shown() == MessageLog::Shown::Stub; }
  bool hasMessage() const { return m_center.log().shown() != MessageLog::Shown::None; }
  // The content height of the band showing a message in full: two lines
  // of text (the band keeps that room for the stub too, so that the cards
  // above it do not move).
  int fullHeight() const;
  // The width the band has (the layout's), less the card's margins: the
  // text wraps and the stub elides to it.
  void setAvailableWidth(int width);
  // The stub's place in the window's coordinates, for the history to open
  // above it.
  QRect stubRect() const;

signals:
  // The stub was clicked: the history opens.
  void historyRequested();
  // Its size or what it shows changed: placed again.
  void layoutChanged();

protected:
  void changeEvent(QEvent* event) override;

private:
  void refresh();
  void fitText();
  void logPlaces();

  MessageCenter& m_center;
  QWidget* m_fullRow = nullptr;
  QLabel* m_icon = nullptr;
  QLabel* m_text = nullptr;
  QToolButton* m_close = nullptr;
  QToolButton* m_stub = nullptr;
  int m_width = 0;
  bool m_refreshing = false;
  QString m_loggedPlaces;
};

// The session's message history (mitcad#1042): a panel over the window,
// above what opened it (the band's stub, the Docked status bar's button),
// listing the messages newest first, each with its time, level icon and
// text. A message about a feature selects it when clicked. It closes on
// Esc, on a click outside it and when a feature is chosen. A child widget
// of the window, not a popup: popups cannot grab the mouse in every test
// session (tools/ui-windows-lib.ps1).
//
// Logged: `Message history: <n> entries` when it opens, then each row's
// place `Message history row <i>: <level> <text> at x,y` (the newest is
// row 1; the first 50 rows), `Message history: selected <uid>` for a row
// of a feature, `Message history: closed`.
class MessageHistory : public QFrame {
  Q_OBJECT

public:
  MessageHistory(const MessageCenter& center, QWidget* window);

  // Opens it above `anchor` (in the window's coordinates); `opener` is the
  // control that opened it (a click on it is not a click outside).
  void open(const QRect& anchor, QWidget* opener);
  void dismiss();

signals:
  void featureChosen(const QString& uid);

protected:
  bool event(QEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void fill();
  void logRows();

  const MessageCenter& m_center;
  QLabel* m_title = nullptr;
  QScrollArea* m_scroll = nullptr;
  QWidget* m_rows = nullptr;
  QVBoxLayout* m_rowsLayout = nullptr;
  QPointer<QWidget> m_opener;
};

} // namespace mitcad
