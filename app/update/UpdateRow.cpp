// SPDX-License-Identifier: MIT
#include "UpdateRow.hpp"

#include <QAction>
#include <QMainWindow>
#include <QTextDocumentFragment>
#include <QToolBar>
#include <QtLogging>

#include "UpdateNotice.hpp"

namespace mitcad {

UpdateRow::UpdateRow(QObject* parent) : QObject(parent) {}

QToolBar* UpdateRow::attach(QMainWindow& window) {
  auto* bar = new QToolBar(tr("Update"), &window);
  bar->setObjectName(QStringLiteral("updateBar"));
  bar->setMovable(false);
  bar->setFloatable(false);
  bar->toggleViewAction()->setVisible(false);
  auto* notice = new UpdateNotice(bar);
  bar->addWidget(notice);
  window.addToolBarBreak(Qt::TopToolBarArea);
  window.addToolBar(Qt::TopToolBarArea, bar);
  bar->hide();
  connect(notice, &UpdateNotice::releaseNotesClicked, this, &UpdateRow::releaseNotesClicked);
  connect(notice, &UpdateNotice::installClicked, this, &UpdateRow::installClicked);
  connect(notice, &UpdateNotice::skipClicked, this, &UpdateRow::skipClicked);
  connect(notice, &UpdateNotice::laterClicked, this, &UpdateRow::laterClicked);
  connect(notice, &UpdateNotice::cancelClicked, this, &UpdateRow::cancelClicked);
  connect(notice, &UpdateNotice::retryClicked, this, &UpdateRow::retryClicked);
  connect(notice, &UpdateNotice::closed, this, &UpdateRow::closed);
  // The window closes: its bar goes, the row stays with the others.
  connect(bar, &QObject::destroyed, this, [this](QObject* gone) {
    m_bars.removeIf([gone](const Bar& entry) { return entry.bar.isNull() || entry.bar.data() == gone; });
    qInfo().noquote() << QStringLiteral("Update row: in %1 window(s)").arg(m_bars.size());
  });
  prune();
  m_bars.append({bar, notice});
  qInfo().noquote() << QStringLiteral("Update row: in %1 window(s)").arg(m_bars.size());
  // A window opened after the update was accepted shows where it is.
  if (m_kind != Kind::Empty) {
    present(*notice);
  }
  bar->setVisible(m_shown);
  return bar;
}

int UpdateRow::windowCount() const {
  int count = 0;
  for (const Bar& entry : m_bars) {
    if (!entry.bar.isNull()) {
      ++count;
    }
  }
  return count;
}

void UpdateRow::prune() {
  m_bars.removeIf([](const Bar& entry) { return entry.bar.isNull() || entry.notice.isNull(); });
}

void UpdateRow::present(UpdateNotice& notice) const {
  switch (m_kind) {
  case Kind::Offer:
    notice.showOffer(m_text, m_installable, m_skippable);
    break;
  case Kind::Progress:
    notice.showProgress(m_text, m_received, m_total);
    break;
  case Kind::Message:
    notice.showMessage(m_text, m_level, m_retry);
    break;
  case Kind::Empty:
    break;
  }
}

void UpdateRow::presentAll() {
  prune();
  for (const Bar& entry : m_bars) {
    present(*entry.notice);
  }
  show();
}

void UpdateRow::showOffer(const QString& html, bool installable, bool skippable) {
  m_kind = Kind::Offer;
  m_text = html;
  m_installable = installable;
  m_skippable = skippable;
  presentAll();
}

void UpdateRow::showProgress(const QString& plain, qint64 received, qint64 total) {
  m_kind = Kind::Progress;
  m_text = plain;
  m_received = received;
  m_total = total;
  presentAll();
}

void UpdateRow::showMessage(const QString& plain, NoticeLevel level, bool retry) {
  m_kind = Kind::Message;
  m_text = plain;
  m_level = level;
  m_retry = retry;
  presentAll();
}

void UpdateRow::show() {
  m_shown = true;
  prune();
  for (const Bar& entry : m_bars) {
    entry.bar->show();
  }
}

void UpdateRow::hide() {
  if (m_shown) {
    qInfo() << "Update notice closed";
  }
  m_shown = false;
  prune();
  for (const Bar& entry : m_bars) {
    entry.bar->hide();
  }
}

QString UpdateRow::text() const {
  return m_kind == Kind::Offer ? QTextDocumentFragment::fromHtml(m_text).toPlainText() : m_text;
}

} // namespace mitcad
