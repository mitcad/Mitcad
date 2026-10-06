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

namespace mitcad {

UpdateNotice::UpdateNotice(QWidget* parent) : QWidget(parent) {
  setObjectName(QStringLiteral("updateNotice"));
  auto* layout = new QHBoxLayout(this);
  layout->setContentsMargins(8, 2, 8, 2);
  auto* icon = new QLabel;
  icon->setPixmap(themeIcon(QStringLiteral("update")).pixmap(20, 20));
  layout->addWidget(icon);
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
                     tr("Downloads and verifies the update, then closes Mitcad (asking to save your work), "
                        "installs it and starts Mitcad again"),
                     &UpdateNotice::installClicked);
  m_skip = button(tr("S&kip This Version"), tr("No more notices of this version"), &UpdateNotice::skipClicked);
  m_later = button(tr("&Later"), tr("Hides the notice until the next check"), &UpdateNotice::laterClicked);
  m_cancel = button(tr("&Cancel"), QString(), &UpdateNotice::cancelClicked);
  m_close = button(tr("Cl&ose"), QString(), &UpdateNotice::closed);
  connect(m_later, &QPushButton::clicked, this, &UpdateNotice::closed);
}

void UpdateNotice::present(const QString& state, const QString& text) {
  m_text->setText(text);
  QStringList buttons;
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel, m_close}) {
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
    for (const QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel, m_close}) {
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

void UpdateNotice::showOffer(const QString& text, bool installable) {
  m_progress->hide();
  m_notes->show();
  m_install->setVisible(installable);
  m_skip->show();
  m_later->show();
  m_cancel->hide();
  m_close->hide();
  present(QStringLiteral("offer"), text);
}

void UpdateNotice::showProgress(const QString& text, qint64 received, qint64 total) {
  m_progress->show();
  if (total > 0) {
    m_progress->setRange(0, 1000);
    m_progress->setValue(static_cast<int>(received * 1000 / total));
  } else {
    m_progress->setRange(0, 0);
  }
  const bool changed = !m_cancel->isVisible() || m_text->text() != text;
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_close}) {
    b->hide();
  }
  m_cancel->show();
  if (changed) {
    present(QStringLiteral("progress"), text);
  }
}

void UpdateNotice::showMessage(const QString& text, bool error) {
  m_progress->hide();
  for (QPushButton* b : {m_notes, m_install, m_skip, m_later, m_cancel}) {
    b->hide();
  }
  m_close->show();
  present(error ? QStringLiteral("error") : QStringLiteral("message"), text);
}

} // namespace mitcad
