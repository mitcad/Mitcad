// SPDX-License-Identifier: MIT
#include "MessageBand.hpp"

#include <algorithm>

#include <QApplication>
#include <QDateTime>
#include <QEvent>
#include <QFont>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QIcon>
#include <QKeyEvent>
#include <QLabel>
#include <QLocale>
#include <QPixmap>
#include <QScrollArea>
#include <QScrollBar>
#include <QStringList>
#include <QTextLayout>
#include <QTextOption>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "GlassCard.hpp"
#include "PlaceWatcher.hpp"
#include "TestSync.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

// The text on one line: line breaks and runs of spaces as one space.
QString oneLine(const QString& text) { return text.simplified(); }

// `text` broken into at most `lines` lines of `width` pixels in `font`,
// between words where it can and anywhere inside a word longer than a
// line; the last line elided at its end when the rest does not fit.
QString fitLines(const QString& text, const QFont& font, int width, int lines) {
  const QFontMetrics metrics(font);
  width = std::max(width, metrics.averageCharWidth() * 8);
  QTextLayout layout(text, font);
  QTextOption option;
  option.setWrapMode(QTextOption::WrapAtWordBoundaryOrAnywhere);
  layout.setTextOption(option);
  QStringList out;
  layout.beginLayout();
  for (int n = 1; n <= lines; ++n) {
    QTextLine line = layout.createLine();
    if (!line.isValid()) {
      break;
    }
    line.setLineWidth(width);
    if (n == lines) {
      out << metrics.elidedText(text.mid(line.textStart()).trimmed(), Qt::ElideRight, width);
      break;
    }
    out << text.mid(line.textStart(), line.textLength()).trimmed();
  }
  layout.endLayout();
  return out.join(QLatin1Char('\n'));
}

// The level's colour role on a widget's text: the error's and the
// warning's own; the plain text colour for the others.
void styleForLevel(QWidget* widget, NoticeLevel level) {
  if (level == NoticeLevel::Error) {
    setErrorStyleSheet(widget);
  } else if (level == NoticeLevel::Warning) {
    setWarningStyleSheet(widget);
  } else {
    setErrorStyleSheet(widget, QString());
  }
}

// The level's icon as a pixmap of `side` pixels; a transparent one of the
// same size for a level without an icon (`keepRoom`) or a null one.
QPixmap levelPixmap(NoticeLevel level, int side, qreal ratio, bool keepRoom) {
  const QIcon icon = noticeIcon(level);
  if (!icon.isNull()) {
    return icon.pixmap(QSize(side, side), ratio);
  }
  if (!keepRoom) {
    return QPixmap();
  }
  QPixmap blank(QSize(side, side) * ratio);
  blank.setDevicePixelRatio(ratio);
  blank.fill(Qt::transparent);
  return blank;
}

QString placeText(const QWidget* widget) {
  const QPoint at = GlassCard::mapToHost(widget, widget->rect().center());
  return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
}

constexpr int kHistoryRowsLogged = 50;
constexpr int kHistoryMargin = 12; // from the window's edges

} // namespace

// ---------------------------------------------------------------------------
// MessageCenter

MessageCenter::MessageCenter(QObject* parent) : QObject(parent) {
  m_timer.setSingleShot(true);
  connect(&m_timer, &QTimer::timeout, this, [this] {
    if (m_log.expire()) {
      toStub("faded");
      emit changed();
    }
  });
}

void MessageCenter::post(const QString& text, NoticeLevel level, const QString& feature) {
  if (text.isEmpty()) {
    const bool full = m_log.shown() == MessageLog::Shown::Full;
    m_log.add(text, level, feature, QDateTime::currentDateTime());
    if (full && m_log.shown() != MessageLog::Shown::Full) {
      m_timer.stop();
      toStub("cleared");
      emit changed();
    }
    return;
  }
  qDebug().noquote() << QStringLiteral("Message band: %1 %2").arg(noticeLevelName(level), text);
  if (m_log.add(text, level, feature, QDateTime::currentDateTime())) {
    qDebug().noquote() << QStringLiteral("Message band full: %1 %2").arg(noticeLevelName(level), text);
    restartTimer();
  }
  emit changed();
}

void MessageCenter::close() {
  if (m_log.close()) {
    m_timer.stop();
    toStub("closed");
    emit changed();
  }
}

void MessageCenter::commandStarted() {
  if (m_log.commandStarted()) {
    m_timer.stop();
    toStub("next command");
    emit changed();
  }
}

void MessageCenter::clear(const char* reason) {
  m_timer.stop();
  const bool changedBand = m_log.clear();
  qDebug().noquote() << QStringLiteral("Message band cleared (%1)").arg(QString::fromLatin1(reason));
  if (changedBand) {
    emit changed();
  }
}

void MessageCenter::withdraw(const QString& text) {
  if (m_log.withdraw(text)) {
    m_timer.stop();
    toStub("withdrawn");
    emit changed();
  }
}

void MessageCenter::toStub(const char* reason) {
  const MessageEntry* latest = m_log.latest();
  if (latest == nullptr) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Message band stub: %1 %2 (%3)")
                            .arg(noticeLevelName(latest->level), latest->text, QString::fromLatin1(reason));
}

void MessageCenter::restartTimer() {
  const MessageEntry* full = m_log.full();
  const int lifetime = full != nullptr ? MessageLog::lifetime(full->level) : 0;
  if (lifetime > 0) {
    m_timer.start(lifetime);
  } else {
    m_timer.stop(); // an error: until it is closed or the next command starts
  }
}

// ---------------------------------------------------------------------------
// MessageBand

MessageBand::MessageBand(MessageCenter& center, QWidget* parent) : QWidget(parent), m_center(center) {
  setObjectName(QStringLiteral("messageBand"));
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(0);

  m_fullRow = new QWidget;
  auto* row = new QHBoxLayout(m_fullRow);
  row->setContentsMargins(0, 0, 0, 0);
  row->setSpacing(8);
  m_icon = new QLabel;
  m_icon->setObjectName(QStringLiteral("messageBandIcon"));
  m_text = new QLabel;
  m_text->setObjectName(QStringLiteral("messageBandText"));
  m_text->setTextFormat(Qt::PlainText);
  m_text->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
  m_text->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  m_close = new QToolButton;
  m_close->setObjectName(QStringLiteral("messageBandClose"));
  m_close->setAutoRaise(true);
  m_close->setText(QString(QChar(0x00D7)));
  m_close->setAccessibleName(tr("Close message"));
  m_close->setToolTip(tr("Close the message"));
  m_close->setFocusPolicy(Qt::NoFocus);
  row->addWidget(m_icon, 0, Qt::AlignVCenter);
  row->addWidget(m_text, 1);
  row->addWidget(m_close, 0, Qt::AlignTop);
  connect(m_close, &QToolButton::clicked, this, [this] { m_center.close(); });

  m_stub = new QToolButton;
  m_stub->setObjectName(QStringLiteral("messageStub"));
  m_stub->setAutoRaise(true);
  m_stub->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_stub->setAccessibleName(tr("Message history"));
  m_stub->setCursor(Qt::PointingHandCursor);
  m_stub->setFocusPolicy(Qt::NoFocus);
  connect(m_stub, &QToolButton::clicked, this, &MessageBand::historyRequested);

  column->addWidget(m_fullRow);
  column->addWidget(m_stub);
  connect(&m_center, &MessageCenter::changed, this, &MessageBand::refresh);
  // Where to click, logged again when the band moves (the UI tests).
  new PlaceWatcher(m_close, [this] { logPlaces(); });
  new PlaceWatcher(m_stub, [this] { logPlaces(); });
  refresh();
}

int MessageBand::fullHeight() const {
  return std::max(2 * m_text->fontMetrics().lineSpacing(), m_close->sizeHint().height());
}

void MessageBand::setAvailableWidth(int width) {
  if (width == m_width) {
    return;
  }
  m_width = width;
  fitText();
}

QRect MessageBand::stubRect() const {
  const QWidget* shown = isStub() ? static_cast<const QWidget*>(m_stub) : this;
  return QRect(GlassCard::mapToHost(shown, QPoint(0, 0)), shown->size());
}

void MessageBand::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::FontChange) {
    refresh(); // the level's colours and the lines of the new font
  }
}

void MessageBand::refresh() {
  if (m_refreshing) {
    return;
  }
  m_refreshing = true;
  const MessageLog& log = m_center.log();
  const qreal ratio = devicePixelRatioF();
  const int side = m_text->fontMetrics().height();
  switch (log.shown()) {
  case MessageLog::Shown::None:
    m_fullRow->hide();
    m_stub->hide();
    setProperty("state", QStringLiteral("none"));
    setProperty("message", QString());
    break;
  case MessageLog::Shown::Full: {
    const MessageEntry& entry = *log.full();
    const QPixmap pixmap = levelPixmap(entry.level, side, ratio, false);
    m_icon->setPixmap(pixmap);
    m_icon->setVisible(!pixmap.isNull());
    m_icon->setToolTip(noticeLevelWord(entry.level));
    m_icon->setProperty("noticeLevel", noticeLevelName(entry.level));
    styleForLevel(m_text, entry.level);
    m_text->setToolTip(noticeLevelText(entry.level, entry.text));
    m_text->setProperty("noticeLevel", noticeLevelName(entry.level));
    m_stub->hide();
    m_fullRow->show();
    setProperty("state", QStringLiteral("full"));
    setProperty("message", entry.text);
    break;
  }
  case MessageLog::Shown::Stub: {
    const MessageEntry& entry = *log.stub();
    const QPixmap pixmap = levelPixmap(entry.level, side, ratio, false);
    m_stub->setIcon(pixmap.isNull() ? QIcon() : QIcon(pixmap));
    m_stub->setIconSize(QSize(side, side));
    // Dimmed: the secondary text's colour, whatever the level (its icon
    // says it).
    setRoleStyleSheet(m_stub, ColorRole::TextSecondary);
    m_stub->setToolTip(tr("%1\nClick for this session's messages.").arg(noticeLevelText(entry.level, entry.text)));
    m_fullRow->hide();
    m_stub->show();
    setProperty("state", QStringLiteral("stub"));
    setProperty("message", entry.text);
    break;
  }
  }
  fitText();
  updateGeometry();
  m_refreshing = false;
  emit layoutChanged();
}

void MessageBand::fitText() {
  const MessageLog& log = m_center.log();
  const int width = m_width > 0 ? m_width : 600;
  if (const MessageEntry* full = log.full()) {
    int room = width - m_close->sizeHint().width() - 8;
    if (m_icon->isVisibleTo(this)) {
      room -= m_icon->sizeHint().width() + 8;
    }
    m_text->setText(fitLines(oneLine(noticeLevelText(full->level, full->text)), m_text->font(), room, 2));
  } else if (const MessageEntry* latest = log.stub()) {
    // One line: the icon, the word and as much of the text as fits.
    const int room = width - m_stub->iconSize().width() - 24;
    QString text = oneLine(noticeLevelText(latest->level, latest->text));
    if (latest->count > 1) {
      text += QStringLiteral(" (%1%2)").arg(QChar(0x00D7)).arg(latest->count);
    }
    m_stub->setText(m_stub->fontMetrics().elidedText(text, Qt::ElideRight, std::max(60, room)));
    m_stub->updateGeometry();
  }
}

void MessageBand::logPlaces() {
  QString places;
  if (m_close->isVisible()) {
    places = QStringLiteral("Message band close at %1").arg(placeText(m_close));
  } else if (m_stub->isVisible()) {
    places = QStringLiteral("Message stub at %1").arg(placeText(m_stub));
  }
  if (!places.isEmpty() && places != m_loggedPlaces) {
    m_loggedPlaces = places;
    qDebug().noquote() << places;
  }
}

// ---------------------------------------------------------------------------
// MessageHistory

MessageHistory::MessageHistory(const MessageCenter& center, QWidget* window) : QFrame(window), m_center(center) {
  setObjectName(QStringLiteral("messageHistory"));
  setFrameShape(QFrame::StyledPanel);
  setAutoFillBackground(true);
  setBackgroundRole(QPalette::Base);
  setFocusPolicy(Qt::StrongFocus);
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(8, 8, 8, 8);
  column->setSpacing(6);
  m_title = new QLabel(tr("Messages"));
  QFont bold = m_title->font();
  bold.setBold(true);
  m_title->setFont(bold);
  column->addWidget(m_title);
  m_scroll = new QScrollArea;
  m_scroll->setObjectName(QStringLiteral("messageHistoryRows"));
  m_scroll->setFrameShape(QFrame::NoFrame);
  m_scroll->setWidgetResizable(true);
  m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_rows = new QWidget;
  m_rowsLayout = new QVBoxLayout(m_rows);
  m_rowsLayout->setContentsMargins(0, 0, 0, 0);
  m_rowsLayout->setSpacing(0);
  m_scroll->setWidget(m_rows);
  column->addWidget(m_scroll, 1);
  hide();
}

void MessageHistory::open(const QRect& anchor, QWidget* opener) {
  QWidget* window = parentWidget();
  m_opener = opener;
  const int wide = std::max(200, std::min(560, window->width() - 2 * kHistoryMargin));
  resize(wide, height());
  fill();
  // As high as its rows, at most 60 % of the window, above the anchor.
  const int chrome = m_title->sizeHint().height() + 6 + 16 + 2 * frameWidth();
  const int most = std::max(120, window->height() * 3 / 5);
  const int tall = std::min(most, chrome + m_rows->sizeHint().height());
  const int x = std::clamp(anchor.center().x() - wide / 2, kHistoryMargin,
                           std::max(kHistoryMargin, window->width() - wide - kHistoryMargin));
  const int y = std::max(kHistoryMargin, anchor.top() - 6 - tall);
  setGeometry(x, y, wide, tall);
  show();
  raise();
  setFocus(Qt::PopupFocusReason);
  qApp->installEventFilter(this);
  qDebug().noquote() << QStringLiteral("Message history: %1 entries").arg(m_center.log().size());
  // The rows' places once they are laid out.
  TestSync::singleShot(0, this, [this] { logRows(); });
}

void MessageHistory::dismiss() {
  if (isHidden()) {
    return;
  }
  qApp->removeEventFilter(this);
  hide();
  qDebug().noquote() << QStringLiteral("Message history: closed");
}

void MessageHistory::fill() {
  while (QLayoutItem* item = m_rowsLayout->takeAt(0)) {
    delete item->widget();
    delete item;
  }
  const std::deque<MessageEntry>& entries = m_center.log().entries();
  if (entries.empty()) {
    auto* none = new QLabel(tr("No messages yet in this session."));
    none->setEnabled(false);
    m_rowsLayout->addWidget(none);
    m_rowsLayout->addStretch(1);
    return;
  }
  const qreal ratio = devicePixelRatioF();
  const int side = fontMetrics().height();
  const int room = width() - 2 * 8 - 2 * frameWidth() - m_scroll->verticalScrollBar()->sizeHint().width() - side - 24;
  int index = 0;
  for (const MessageEntry& entry : entries) {
    ++index;
    auto* row = new QToolButton;
    row->setObjectName(QStringLiteral("messageHistoryRow"));
    row->setAutoRaise(true);
    row->setFocusPolicy(Qt::NoFocus);
    row->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    row->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
    row->setIcon(QIcon(levelPixmap(entry.level, side, ratio, true)));
    row->setIconSize(QSize(side, side));
    QString text = oneLine(noticeLevelText(entry.level, entry.text));
    if (entry.count > 1) {
      text += QStringLiteral(" (%1%2)").arg(QChar(0x00D7)).arg(entry.count);
    }
    const QString time = QLocale().toString(entry.time.time(), QStringLiteral("HH:mm:ss"));
    row->setText(row->fontMetrics().elidedText(QStringLiteral("%1   %2").arg(time, text), Qt::ElideRight,
                                               std::max(80, room)));
    QString tip = noticeLevelText(entry.level, entry.text);
    if (!entry.feature.isEmpty()) {
      tip += QLatin1Char('\n') + tr("Click to select the feature.");
      row->setCursor(Qt::PointingHandCursor);
    }
    row->setToolTip(tip);
    if (isProblem(entry.level)) {
      styleForLevel(row, entry.level);
    }
    row->setProperty("historyLine", QStringLiteral("Message history row %1: %2 %3")
                                        .arg(index)
                                        .arg(noticeLevelName(entry.level), oneLine(entry.text)));
    const QString feature = entry.feature;
    connect(row, &QToolButton::clicked, this, [this, feature] {
      if (feature.isEmpty()) {
        return; // a message about no feature: nothing to select
      }
      qDebug().noquote() << QStringLiteral("Message history: selected %1").arg(feature);
      dismiss();
      emit featureChosen(feature);
    });
    // At the left edge as a list reads: a wide tool button centres its
    // icon and text.
    m_rowsLayout->addWidget(row, 0, Qt::AlignLeft);
  }
  m_rowsLayout->addStretch(1);
}

void MessageHistory::logRows() {
  if (isHidden()) {
    return;
  }
  const QRect viewport(m_scroll->viewport()->mapTo(this, QPoint(0, 0)), m_scroll->viewport()->size());
  int logged = 0;
  for (int i = 0; i < m_rowsLayout->count() && logged < kHistoryRowsLogged; ++i) {
    QWidget* row = m_rowsLayout->itemAt(i)->widget();
    if (row == nullptr || !qobject_cast<QToolButton*>(row)) {
      continue;
    }
    const QPoint middle = row->mapTo(this, row->rect().center());
    if (!viewport.contains(middle)) {
      continue; // scrolled out of sight
    }
    qDebug().noquote() << QStringLiteral("%1 at %2").arg(row->property("historyLine").toString(), placeText(row));
    ++logged;
  }
}

bool MessageHistory::event(QEvent* event) {
  // Esc closes it rather than cancelling what the window would cancel.
  if (event->type() == QEvent::ShortcutOverride && static_cast<QKeyEvent*>(event)->key() == Qt::Key_Escape) {
    event->accept();
    return true;
  }
  return QFrame::event(event);
}

void MessageHistory::keyPressEvent(QKeyEvent* event) {
  if (event->key() == Qt::Key_Escape) {
    dismiss();
    event->accept();
    return;
  }
  QFrame::keyPressEvent(event);
}

bool MessageHistory::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::MouseButtonPress && isVisible()) {
    auto* widget = qobject_cast<QWidget*>(watched);
    const bool inside = widget != nullptr && (widget == this || isAncestorOf(widget));
    const bool opener =
        widget != nullptr && m_opener != nullptr && (widget == m_opener || m_opener->isAncestorOf(widget));
    if (widget != nullptr && !inside && !opener) {
      dismiss(); // a click elsewhere; the click goes on to where it was
    }
  }
  return QFrame::eventFilter(watched, event);
}

} // namespace mitcad
