// SPDX-License-Identifier: MIT
#pragma once

#include <QSize>
#include <QString>
#include <QTextEdit>

namespace mitcad {

// The status line of a command panel (mitcad#198): an error, a warning or a
// hint, as long as the model makes it. It fits the width the panel gives it
// instead of asking for more: lines break between words and, inside a word
// longer than the line (a profile's or a face's name), anywhere, so a long
// identifier cannot push the panel's fields and buttons out of sight. It is
// as high as its text up to kMaxLines lines and scrolls beyond them, so a
// long list of errors stays readable without taking over the panel. A new
// text shows from its first line until the user scrolls it (mitcad#1430).
// The text can be selected and copied whole. Empty, it is hidden and takes
// no room.
class CommandMessage : public QTextEdit {
  Q_OBJECT

public:
  // The lines shown before it scrolls.
  static constexpr int kMaxLines = 6;

  explicit CommandMessage(QWidget* parent = nullptr);

  // Shows the text (plain, or rich text as a label would take it) from its
  // first line; empty hides it. The same text again changes nothing (its
  // scroll position and selection stay).
  void setMessage(const QString& text);
  QString message() const { return m_text; }

  // The height for kMaxLines lines, where it starts to scroll.
  int maximumTextHeight() const;

  bool hasHeightForWidth() const override;
  int heightForWidth(int available) const override;
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

signals:
  // The pointer came over the message (true) or left it, or the message
  // went (mitcad#1327: the places its notices name are painted meanwhile).
  void hovered(bool on);

protected:
  // Copy and Select All go to the text while it has the keyboard, not to
  // the window's actions of the same keys; the pointer leaving it ends its
  // hover.
  bool event(QEvent* event) override;
  // The pointer moving over the text starts its hover, leaving it ends it.
  bool viewportEvent(QEvent* event) override;

private:
  void setHovered(bool on);

  QString m_text;
  bool m_hovered = false;
  // The text stays at its first line: set with a new text, cleared when the
  // user scrolls it, clicks in it or uses the wheel over it.
  bool m_fromTop = true;
};

// Plain text with a zero-width space between the characters of each word
// longer than `longest` characters, so that a word-wrapped QLabel can break
// a long identifier rather than grow as wide as it (the notes under a
// command panel's fields; not for text that is copied). Rich text and short
// words are returned as they are.
QString breakableText(const QString& text, int longest = 16);

} // namespace mitcad
