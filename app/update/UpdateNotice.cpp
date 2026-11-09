// SPDX-License-Identifier: MIT
#include "UpdateNotice.hpp"

#include <QCoreApplication>
#include <QEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QStringList>
#include <QTextDocumentFragment>
#include <QtLogging>

#include "framework/Icons.hpp"
#include "framework/TestSync.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {

UpdateNotice::UpdateNotice(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("updateNotice"));
  auto* layout = new QHBoxLayout(this);
  layout->setContentsMargins(8, 2, 8, 2);
  m_icon = new QLabel;
  m_icon->setPixmap(themeIcon(QStringLiteral("update")).pixmap(uiIconSize(20, 20)));
  layout->addWidget(m_icon);
  m_text = new QLabel;
  m_text->setTextFormat(Qt::RichText);
  m_text->setWordWrap(true);
  m_text->setTextInteractionFlags(Qt::NoTextInteraction);
  layout->addWidget(m_text, 1);
  m_progress = new QProgressBar;
  m_progress->setMaximumWidth(240);
  layout->addWidget(m_progress);
  const auto button = [this, layout](const QString& text, const QString& tip, void (UpdateNotice::*signal)()) {
    auto* b = new QPushButton(text);
    b->setToolTip(tip);
    b->setAutoDefault(false);
    connect(b, &QPushButton::clicked, this, signal);
    layout->addWidget(b);
    return b;
  };
  m_notes = button(tr("&Release Notes"), tr("Opens the release's page"), &UpdateNotice::releaseNotesClicked);
  m_install = button(tr("&Install"),
                     tr("Downloads, verifies and installs the update, then starts MitCad again"),
                     &UpdateNotice::installClicked);
  m_skip = button(tr("S&kip This Version"), tr("No more notices of this version"), &UpdateNotice::skipClicked);
  m_later = button(tr("&Later"), tr("Hides the notice until the next check"), &UpdateNotice::laterClicked);
  m_cancel = button(tr("&Cancel"), QString(), &UpdateNotice::cancelClicked);
  m_retry = button(tr("&Try Again"), tr("Checks for the update again and offers to install it"),
                   &UpdateNotice::retryClicked);
  m_close = button(tr("Cl&ose"), QString(), &UpdateNotice::closed);
  connect(m_later, &QPushButton::clicked, this, &UpdateNotice::closed);
}

void UpdateNotice::present(const QString& state, const QString& html) {
  if (state == QStringLiteral("offer") || state == QStringLiteral("progress")) {
    m_icon->setPixmap(themeIcon(QStringLiteral("update")).pixmap(uiIconSize(20, 20)));
  }
  m_text->setText(html);
  QStringList buttons;
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel, m_retry, m_close}) {
    if (!b->isHidden()) {
      buttons << b->text().remove(QLatin1Char('&'));
    }
  }
  qInfo().noquote() << QStringLiteral("Update notice (%1): %2 [%3]")
                           .arg(state, this->text(), buttons.join(QStringLiteral(", ")));
  // Where the buttons are, once laid out: the Windows tests click them, as
  // their keys (Alt+I) do not reach the bar's buttons there in session 0.
  TestSync::singleShot(50, this, [this] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    for (const QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel, m_retry, m_close}) {
      if (!b->isHidden()) {
        const QPoint center = b->mapTo(window(), b->rect().center());
        qDebug().noquote() << QStringLiteral("Update notice %1 at %2,%3")
                                  .arg(b->text().remove(QLatin1Char('&')))
                                  .arg(center.x())
                                  .arg(center.y());
      }
    }
  });
}

QString UpdateNotice::text() const { return QTextDocumentFragment::fromHtml(m_text->text()).toPlainText(); }

void UpdateNotice::showOffer(const QString& html, bool installable, bool skippable) {
  m_progress->hide();
  m_notes->show();
  m_install->setVisible(installable);
  m_skip->setVisible(skippable);
  m_later->show();
  m_cancel->hide();
  m_retry->hide();
  m_close->hide();
  present(QStringLiteral("offer"), html);
}

void UpdateNotice::showProgress(const QString& plain, qint64 received, qint64 total) {
  // Plain text: a message can carry what a server sent (mitcad#938).
  const QString html = plain.toHtmlEscaped();
  m_progress->show();
  if (total > 0) {
    m_progress->setRange(0, 1000);
    m_progress->setValue(static_cast<int>(received * 1000 / total));
  } else {
    m_progress->setRange(0, 0);
  }
  const bool changed = !m_cancel->isVisible() || m_text->text() != html;
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_retry, m_close}) {
    b->hide();
  }
  m_cancel->show();
  if (changed) {
    present(QStringLiteral("progress"), html);
  }
}

void UpdateNotice::showMessage(const QString& plain, NoticeLevel level, bool retry) {
  m_progress->hide();
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel}) {
    b->hide();
  }
  m_retry->setVisible(retry);
  m_close->show();
  // An error does not look like a success (mitcad#1040): the level's icon
  // and colour; information keeps the update icon.
  const QIcon icon = level == NoticeLevel::Information ? QIcon() : noticeIcon(level);
  m_icon->setPixmap((icon.isNull() ? themeIcon(QStringLiteral("update")) : icon).pixmap(uiIconSize(20, 20)));
  // Plain text: an error carries what a server sent, which must not be
  // read as markup (mitcad#938).
  QString html = plain.toHtmlEscaped();
  if (isProblem(level)) {
    html = QStringLiteral("<span style=\"color:%1\">%2</span>").arg(noticeColor(level, palette()).name(), html);
  }
  const QString state = level == NoticeLevel::Error     ? QStringLiteral("error")
                        : level == NoticeLevel::Warning ? QStringLiteral("warning")
                                                        : QStringLiteral("message");
  present(state, html);
}

} // namespace mitcad
