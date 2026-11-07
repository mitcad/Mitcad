// SPDX-License-Identifier: MIT
// The command panel's message in a narrow panel (mitcad#198), laid out as
// MainWindow lays out a command panel: in a scroll area that never scrolls
// sideways, at the Docked page's width, and as narrow as the other rows
// allow with a larger font (display scaling); a new message from its first
// line (mitcad#1430). On Qt's offscreen platform.
#include "framework/CommandMessage.hpp"

#include <QAbstractSlider>
#include <QChar>
#include <QCoreApplication>
#include <QElapsedTimer>
#include <QEvent>
#include <QEventLoop>
#include <QFont>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QScrollBar>
#include <QStringList>
#include <QTextDocument>
#include <QVBoxLayout>

#include <cstdio>

namespace {

// Lays out what changed, the text's layout done in steps by a timer too.
void settle() {
  QElapsedTimer clock;
  clock.start();
  while (clock.elapsed() < 150) {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
  }
}

} // namespace

int commandMessageTests() {
  using namespace mitcad;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "command message: %s\n", message);
      ++failures;
    }
  };

  // The reported case: Extrude's diagnostic with a profile's long name, no
  // space in it.
  QString name = QStringLiteral("r{");
  for (int i = 1; i <= 24; ++i) {
    name += QStringLiteral("c%1[c%2,c%3],").arg(i).arg(i + 1).arg(i + 2);
  }
  name += QLatin1Char('}');
  const QString duplicate = QStringLiteral("profile %1 of F1 is listed more than once").arg(name);
  QStringList errors;
  for (int i = 1; i <= 40; ++i) {
    errors << QStringLiteral("Profile %1 of Sketch2: the region is not closed").arg(i);
  }
  const QString many = errors.join(QLatin1Char('\n'));

  // A panel as CommandPanel makes one (a field under its label, the
  // message, OK and Cancel), its size policy the default one: only the
  // message decides whether it asks for more width than the page has.
  QScrollArea page;
  page.setWidgetResizable(true);
  page.setFrameShape(QFrame::NoFrame);
  page.setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  auto* panel = new QWidget;
  auto* layout = new QVBoxLayout(panel);
  auto* form = new QFormLayout;
  form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
  auto* field = new QLineEdit(QStringLiteral("20 mm"));
  form->addRow(QStringLiteral("Distance"), field);
  layout->addLayout(form);
  auto* message = new CommandMessage;
  layout->addWidget(message);
  auto* buttons = new QHBoxLayout;
  auto* ok = new QPushButton(QStringLiteral("OK"));
  auto* cancel = new QPushButton(QStringLiteral("Cancel"));
  buttons->addStretch();
  buttons->addWidget(ok);
  buttons->addWidget(cancel);
  layout->addLayout(buttons);
  layout->addStretch();
  page.setWidget(panel);
  page.resize(300, 800);
  page.show();
  settle();
  // The other rows' fonts are the platform's (a fallback font where Qt finds
  // none): a page they do not fit in is made as wide as they need, so that
  // only the message is on trial.
  const int least = panel->minimumSizeHint().width();
  if (least > page.viewport()->width()) {
    page.resize(least, 800);
    settle();
  }

  // Everything within what the page shows: nothing cut off on the right,
  // the text wrapped within the message's own width. A hidden message is
  // not laid out (it keeps the geometry it was made with) and shows nothing.
  const auto fits = [&] {
    const int room = page.viewport()->width();
    const bool messageFits = message->isHidden() ||
                             (message->geometry().right() < room &&
                              message->document()->idealWidth() <= message->viewport()->width() + 1);
    return panel->width() <= room && field->geometry().right() < room && ok->geometry().right() < room &&
           cancel->geometry().right() < room && messageFits;
  };

  check(message->isHidden(), "no message: nothing shown");
  const int okTop = ok->y();
  check(fits(), "no message: the panel fits");

  message->setMessage(duplicate);
  settle();
  check(message->isVisible(), "a long name: shown");
  check(fits(), "a long name: within the panel's width");
  check(panel->minimumSizeHint().width() == least, "a long name: the panel asks for no more width");
  check(message->toPlainText() == duplicate, "a long name: the whole diagnostic to copy");
  check(message->height() <= message->maximumTextHeight(), "a long name: at most a few lines high");
  check(ok->y() > okTop, "a long name: OK below it");

  message->setMessage(many);
  settle();
  check(fits(), "many errors: within the panel's width");
  check(message->height() <= message->maximumTextHeight(), "many errors: at most a few lines high");
  check(message->verticalScrollBar() != nullptr && message->verticalScrollBar()->maximum() > 0,
        "many errors: the rest scrolls");
  check(message->toPlainText() == many, "many errors: all of them to read and copy");
  // (The message's height and a spacing between it and the buttons.)
  check(ok->y() <= okTop + message->maximumTextHeight() + 24,
        "many errors: OK and Cancel stay close to the fields");
  const int tall = message->height();

  message->setMessage(many);
  settle();
  check(message->height() == tall && message->toPlainText() == many, "the same errors again: unchanged");

  const QString zero = QStringLiteral("extrude distance must not be zero");
  message->setMessage(zero);
  settle();
  check(message->toPlainText() == zero, "a changed error: the new one only");
  check(message->height() < tall && message->height() <= 2 * message->fontMetrics().lineSpacing(),
        "a short error: one line");
  check(message->verticalScrollBar() == nullptr || message->verticalScrollBar()->maximum() == 0,
        "a short error: nothing to scroll");
  check(fits(), "a short error: within the panel's width");

  message->setMessage(QString());
  settle();
  check(message->isHidden(), "cleared: hidden");
  check(ok->y() == okTop, "cleared: its room given back");
  check(panel->minimumSizeHint().width() == least, "cleared: no width left behind");

  // Rich text as a label takes it.
  message->setMessage(QStringLiteral("<b>Distance:</b> %1").arg(name));
  settle();
  check(message->toPlainText() == QStringLiteral("Distance: %1").arg(name), "rich text: shown as text");
  check(fits(), "rich text: within the panel's width");

  // A new message opens at its first line (mitcad#1430). The panel puts an
  // error in a padded box (CommandPanel::setMessage) while the message is
  // still as high as the one-line hint before it: Qt scrolled to the cursor
  // at the start of the text in a viewport a few pixels high, and the box
  // kept that scroll once it was as high as its lines, its first line cut.
  const QString boxed =
      QStringLiteral("QTextEdit { color: #b00020; background: #fbe3e3; border-radius: 6px; padding: 5px 6px; }");
  QScrollBar* bar = message->verticalScrollBar();
  message->setStyleSheet(QString());
  message->setMessage(QStringLiteral("Pick a face."));
  settle();
  check(message->isVisible() && bar->maximum() == 0, "a hint: one line, nothing to scroll");
  message->setStyleSheet(boxed);
  message->setMessage(many);
  settle();
  check(bar->maximum() > 0, "an error box after a hint: the rest scrolls");
  check(bar->value() == 0, "an error box after a hint: opens at its first line");
  // From hidden, made at the size it had when it was hidden.
  message->setMessage(QString());
  settle();
  message->setMessage(QStringLiteral("%1\n%2").arg(duplicate, many));
  settle();
  check(bar->maximum() > 0 && bar->value() == 0, "an error box shown again: opens at its first line");
  // The user's scroll stays while the page is laid out again and the same
  // text comes again; a new text opens at its first line.
  bar->triggerAction(QAbstractSlider::SliderPageStepAdd);
  const int scrolled = bar->value();
  check(scrolled > 0, "scrolled by the user: down its text");
  page.resize(page.width(), 700);
  settle();
  message->setMessage(QStringLiteral("%1\n%2").arg(duplicate, many));
  settle();
  check(bar->value() == scrolled, "scrolled by the user: kept");
  message->setMessage(many);
  settle();
  check(bar->value() == 0, "a new error after the user's scroll: opens at its first line");
  page.resize(page.width(), 800);
  message->setStyleSheet(QString());

  // A larger font (display scaling), the page as narrow as the other rows
  // allow, high enough not to scroll.
  message->setMessage(QString());
  QFont larger = panel->font();
  if (larger.pointSizeF() > 0) {
    larger.setPointSizeF(larger.pointSizeF() * 1.5);
  } else {
    larger.setPixelSize(larger.pixelSize() * 3 / 2);
  }
  panel->setFont(larger);
  settle();
  const int narrowest = panel->minimumSizeHint().width();
  page.resize(narrowest, 800);
  settle();
  message->setMessage(duplicate);
  settle();
  check(panel->minimumSizeHint().width() == narrowest, "scaled: the panel asks for no more width");
  check(fits(), "scaled and narrow: the long name within the panel's width");
  check(message->height() <= message->maximumTextHeight(), "scaled and narrow: at most a few lines high");
  message->setMessage(many);
  settle();
  check(fits(), "scaled and narrow: many errors within the panel's width");

  // The notes under the fields: a long word may break; nothing else changes.
  check(breakableText(QStringLiteral("Enter a value.")) == QStringLiteral("Enter a value."), "notes: short words as they are");
  check(breakableText(QStringLiteral("<b>%1</b>").arg(name)) == QStringLiteral("<b>%1</b>").arg(name),
        "notes: rich text as it is");
  const QString breakable = breakableText(duplicate);
  QString plain = breakable;
  plain.remove(QChar(0x200B));
  check(breakable != duplicate && plain == duplicate, "notes: only zero-width spaces in the long name");
  QLabel whole(duplicate);
  whole.setWordWrap(true);
  QLabel note(breakable);
  note.setWordWrap(true);
  check(note.minimumSizeHint().width() * 4 < whole.minimumSizeHint().width(), "notes: a long name asks for little width");
  return failures;
}
