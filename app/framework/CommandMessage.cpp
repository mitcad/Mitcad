// SPDX-License-Identifier: MIT
#include "CommandMessage.hpp"

#include <algorithm>
#include <cmath>
#include <memory>

#include <QChar>
#include <QEvent>
#include <QFontMetrics>
#include <QFrame>
#include <QKeyEvent>
#include <QKeySequence>
#include <QSizePolicy>
#include <QStringView>
#include <QTextDocument>
#include <QTextOption>

namespace mitcad {

CommandMessage::CommandMessage(QWidget* parent) : QTextEdit(parent) {
  setReadOnly(true);
  setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::TextSelectableByKeyboard);
  // A click may select text to copy; it takes no Tab focus from the fields,
  // and Enter and Esc, which it does not use, still reach the panel.
  setFocusPolicy(Qt::ClickFocus);
  setFrameShape(QFrame::NoFrame);
  // A label's look: no frame, the panel's background, no left margin.
  viewport()->setAutoFillBackground(false);
  document()->setDocumentMargin(0);
  // Between words, and inside a word that does not fit on a line.
  setLineWrapMode(QTextEdit::WidgetWidth);
  setWordWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  setVerticalScrollBarPolicy(Qt::ScrollBarAsNeeded);
  // As high as its text for the width it gets. Not Expanding, as a text
  // edit is by default: the panel's free space stays below its buttons.
  QSizePolicy policy(QSizePolicy::Preferred, QSizePolicy::Preferred);
  policy.setHeightForWidth(true);
  setSizePolicy(policy);
  // The pointer over it without a button pressed (mitcad#1327).
  viewport()->setMouseTracking(true);
  hide();
}

void CommandMessage::setMessage(const QString& text) {
  if (text == m_text) {
    return;
  }
  m_text = text;
  if (Qt::mightBeRichText(text)) {
    setHtml(text);
  } else {
    setPlainText(text);
  }
  setVisible(!text.isEmpty());
  updateGeometry();
}

int CommandMessage::maximumTextHeight() const {
  const int margins = static_cast<int>(std::ceil(2 * document()->documentMargin()));
  return kMaxLines * fontMetrics().lineSpacing() + margins + 2 * frameWidth();
}

bool CommandMessage::hasHeightForWidth() const { return true; }

int CommandMessage::heightForWidth(int available) const {
  // The text laid out at that width, on a copy (the shown document keeps
  // its own width and scroll position).
  const int frame = 2 * frameWidth();
  const std::unique_ptr<QTextDocument> copy(document()->clone());
  copy->setTextWidth(std::max(1, available - frame));
  const int text = static_cast<int>(std::ceil(copy->size().height()));
  return std::min(text + frame, maximumTextHeight());
}

QSize CommandMessage::sizeHint() const {
  // No width of its own: it takes the panel's.
  const QSize least = minimumSizeHint();
  return QSize(least.width(), heightForWidth(std::max(width(), least.width())));
}

QSize CommandMessage::minimumSizeHint() const {
  const QFontMetrics metrics = fontMetrics();
  const int frame = 2 * frameWidth();
  return QSize(4 * metrics.averageCharWidth() + frame, metrics.lineSpacing() + frame);
}

bool CommandMessage::event(QEvent* event) {
  if (event->type() == QEvent::ShortcutOverride) {
    auto* key = static_cast<QKeyEvent*>(event);
    if (key->matches(QKeySequence::Copy) || key->matches(QKeySequence::SelectAll)) {
      key->accept();
      return true;
    }
  }
  if (event->type() == QEvent::Leave || event->type() == QEvent::Hide) {
    setHovered(false);
  }
  return QTextEdit::event(event);
}

bool CommandMessage::viewportEvent(QEvent* event) {
  switch (event->type()) {
  case QEvent::Enter:
  case QEvent::MouseMove:
  case QEvent::HoverMove:
    setHovered(!m_text.isEmpty());
    break;
  case QEvent::Leave:
    setHovered(false);
    break;
  default:
    break;
  }
  return QTextEdit::viewportEvent(event);
}

void CommandMessage::setHovered(bool on) {
  if (on == m_hovered) {
    return;
  }
  m_hovered = on;
  emit hovered(on);
}

QString breakableText(const QString& text, int longest) {
  if (text.isEmpty() || Qt::mightBeRichText(text)) {
    return text;
  }
  const QChar zeroWidthSpace(0x200B);
  QString result;
  result.reserve(text.size() + text.size() / 2);
  qsizetype start = 0; // where the current word starts
  for (qsizetype i = 0; i <= text.size(); ++i) {
    if (i < text.size() && !text.at(i).isSpace()) {
      continue;
    }
    const QStringView word = QStringView(text).mid(start, i - start);
    if (word.size() > longest) {
      for (qsizetype k = 0; k < word.size(); ++k) {
        // Not inside a surrogate pair.
        if (k > 0 && !word.at(k).isLowSurrogate()) {
          result.append(zeroWidthSpace);
        }
        result.append(word.at(k));
      }
    } else {
      result.append(word);
    }
    if (i < text.size()) {
      result.append(text.at(i));
    }
    start = i + 1;
  }
  return result;
}

} // namespace mitcad
