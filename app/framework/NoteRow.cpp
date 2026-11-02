// SPDX-License-Identifier: MIT
#include "NoteRow.hpp"

#include <algorithm>

#include <QCursor>
#include <QFont>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QPaintEvent>
#include <QPainter>
#include <QPushButton>
#include <QSize>
#include <QStyle>
#include <QVBoxLayout>

#include "Command.hpp"
#include "Icons.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

// The note's text: wrapped, or on one line elided in the middle (so a
// count at its end stays), the whole text its tooltip (mitcad#655: 3D
// Print's Materials summary).
class NoteText : public QLabel {
public:
  void setOneLine(bool oneLine) {
    m_oneLine = oneLine;
    setWordWrap(!oneLine);
    updateGeometry();
    update();
  }

  QSize minimumSizeHint() const override {
    QSize size = QLabel::minimumSizeHint();
    if (m_oneLine) {
      size.setWidth(fontMetrics().horizontalAdvance(QStringLiteral("…")) + 8);
    }
    return size;
  }

  QSize sizeHint() const override {
    QSize size = QLabel::sizeHint();
    if (m_oneLine) {
      size.setWidth(minimumSizeHint().width());
    }
    return size;
  }

protected:
  void paintEvent(QPaintEvent* event) override {
    if (!m_oneLine) {
      QLabel::paintEvent(event);
      return;
    }
    const QRect area = contentsRect();
    const QString shown = fontMetrics().elidedText(text(), Qt::ElideMiddle, area.width());
    setToolTip(shown == text() ? QString() : text());
    QPainter painter(this);
    style()->drawItemText(&painter, area, Qt::AlignLeft | Qt::AlignVCenter, palette(), isEnabled(), shown,
                          foregroundRole());
  }

private:
  bool m_oneLine = false;
};


// A link (mitcad#655): underlined in the link colour, on a row of its own
// as wide as the note, wrapped at its words where the row is narrow: never
// cut, never wider than the panel.
class LinkButton : public QPushButton {
public:
  LinkButton() {
    setFlat(true);
    setCursor(Qt::PointingHandCursor);
    QSizePolicy policy(QSizePolicy::Preferred, QSizePolicy::Preferred);
    policy.setHeightForWidth(true);
    setSizePolicy(policy);
  }

  QSize sizeHint() const override {
    const QFontMetrics metrics(linkFont());
    return QSize(metrics.horizontalAdvance(text()) + 2, metrics.height() + 4);
  }

  QSize minimumSizeHint() const override {
    // As narrow as its longest word.
    const QFontMetrics metrics(linkFont());
    int widest = 0;
    for (const QString& word : text().split(QLatin1Char(' '), Qt::SkipEmptyParts)) {
      widest = std::max(widest, metrics.horizontalAdvance(word));
    }
    return QSize(widest + 2, metrics.height() + 4);
  }

  bool hasHeightForWidth() const override { return true; }

  int heightForWidth(int width) const override {
    const QFontMetrics metrics(linkFont());
    const QRect bounds = metrics.boundingRect(QRect(0, 0, std::max(1, width - 2), 100000),
                                              Qt::AlignLeft | Qt::TextWordWrap, text());
    return bounds.height() + 4;
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setFont(linkFont());
    painter.setPen(palette().color(isEnabled() ? QPalette::Link : QPalette::PlaceholderText));
    painter.drawText(rect().adjusted(1, 2, -1, -2), Qt::AlignLeft | Qt::AlignTop | Qt::TextWordWrap, text());
  }

private:
  QFont linkFont() const {
    QFont underlined = font();
    underlined.setUnderline(true);
    return underlined;
  }
};

} // namespace

NoteRow::NoteRow(QWidget* parent) : QWidget(parent) {
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(2);
  // The text and the icon buttons on a line, the text buttons under it.
  m_textRow = new QHBoxLayout;
  m_textRow->setContentsMargins(0, 0, 0, 0);
  m_textRow->setSpacing(4);
  m_label = new NoteText;
  m_label->setWordWrap(true);
  m_label->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_textRow->addWidget(m_label, 1);
  column->addLayout(m_textRow);
  // A notice under the text's line (mitcad#1062), hidden without one.
  m_noticeLabel = new QLabel;
  m_noticeLabel->setObjectName(QStringLiteral("note_notice"));
  m_noticeLabel->setWordWrap(true);
  m_noticeLabel->setTextFormat(Qt::RichText);
  m_noticeLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_noticeLabel->hide();
  column->addWidget(m_noticeLabel);
  // The text buttons in a row of their own, hidden without any: no gap
  // under a note that has none (mitcad#655).
  m_buttonsWidget = new QWidget;
  m_buttonRow = new QHBoxLayout(m_buttonsWidget);
  m_buttonRow->setContentsMargins(0, 0, 0, 0);
  m_buttonRow->setSpacing(6);
  m_buttonRow->addStretch(1);
  m_buttonsWidget->hide();
  column->addWidget(m_buttonsWidget);
  // Links without a button before them, each on a row of its own under
  // the buttons (mitcad#655: 3D Print's own settings of bodies).
  m_links = new QVBoxLayout;
  m_links->setContentsMargins(0, 0, 0, 0);
  m_links->setSpacing(0);
  column->addLayout(m_links);
}

bool NoteRow::setNote(const QString& text, const QStringList& actions, const QStringList& icons, bool oneLine,
                      const QString& notice, NoticeLevel level) {
  if (text == m_text && actions == m_actions && icons == m_icons && oneLine == m_oneLine && notice == m_notice &&
      level == m_noticeLevel) {
    return false;
  }
  if (notice != m_notice || level != m_noticeLevel) {
    m_notice = notice;
    m_noticeLevel = level;
    // As the panel's message (docs/ui-conventions.md): an error or a
    // warning in the level's box with its icon and word; else plain grey.
    if (isProblem(level)) {
      // Both colours follow the palette (mitcad#1152).
      const QString format = QStringLiteral("QLabel { color: %1; background: %2; border-radius: 6px; "
                                            "padding: 4px 6px; }");
      if (level == NoticeLevel::Error) {
        setErrorStyleSheet(m_noticeLabel, format, ColorRole::ErrorFill);
      } else {
        setWarningStyleSheet(m_noticeLabel, format, ColorRole::WarningFill);
      }
      QString shown = notice.toHtmlEscaped();
      shown.replace(QLatin1Char('\n'), QStringLiteral("<br>"));
      m_noticeLabel->setText(
          QStringLiteral("%1&nbsp;<b>%2:</b> %3").arg(noticeIconHtml(level), noticeLevelWord(level), shown));
    } else {
      setRoleStyleSheet(m_noticeLabel, ColorRole::TextSecondary,
                        QStringLiteral("QLabel { color: %1; background: transparent; }"));
      m_noticeLabel->setText(notice.toHtmlEscaped());
    }
    m_noticeLabel->setProperty("noticeLevel", noticeLevelName(level));
    m_noticeLabel->setAccessibleName(noticeLevelText(level, notice));
    m_noticeLabel->setVisible(!notice.isEmpty());
  }
  m_text = text;
  if (oneLine != m_oneLine) {
    m_oneLine = oneLine;
    static_cast<NoteText*>(m_label)->setOneLine(oneLine);
  }
  m_label->setText(text);
  m_label->setVisible(!text.isEmpty());
  if (actions != m_actions || icons != m_icons) {
    m_actions = actions;
    m_icons = icons;
    for (QPushButton* button : m_buttons) {
      button->hide();
      button->deleteLater();
    }
    m_buttons.clear();
    int below = 0;
    for (int i = 0; i < actions.size(); ++i) {
      const QString icon = icons.value(i);
      QPushButton* button = icon == linkActionIcon() ? new LinkButton : new QPushButton;
      button->setAutoDefault(false);
      button->setFocusPolicy(Qt::NoFocus);
      button->setObjectName(QStringLiteral("note_action_%1").arg(i));
      connect(button, &QPushButton::clicked, this, [this, i] { emit actionClicked(i); });
      if (icon == linkActionIcon()) {
        // A link on a row of its own under the buttons, the note's width
        // (mitcad#655: 3D Print's import line under Import from slicer…,
        // opening Details; Assign materials…).
        button->setText(actions.at(i));
        m_links->addWidget(button);
      } else if (icon.isEmpty()) {
        button->setText(actions.at(i));
        // Never clipped: as wide as its text.
        button->setMinimumWidth(button->sizeHint().width());
        // Before the stretch: the buttons start at the left.
        m_buttonRow->insertWidget(below++, button);
      } else {
        // A small icon on the text's line, the action its tooltip.
        button->setIcon(themeIcon(icon));
        button->setIconSize(QSize(16, 16));
        button->setFlat(true);
        button->setToolTip(actions.at(i));
        button->setAccessibleName(actions.at(i));
        m_textRow->addWidget(button, 0, Qt::AlignTop);
      }
      m_buttons.push_back(button);
    }
    m_buttonsWidget->setVisible(below > 0);
  }
  return true;
}

} // namespace mitcad
