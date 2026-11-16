// SPDX-License-Identifier: MIT
// One update for the whole application (mitcad#1890): the update's row is
// the application's, and every main window's bar shows it. With two
// windows, Install clicked in the first acts once; the first window closes
// and the second keeps the download's state and its progress, which goes
// on there; a window opened later shows it at once; the download reaches
// "downloaded" in the windows left; Skip This Version clicked in one window
// acts once and hides the row in every window. Qt Widgets on the offscreen
// platform; the controller's part (Install starts the download) is played
// by the test.
#include "UnitTests.hpp"
#include "update/UpdateNotice.hpp"
#include "update/UpdateRow.hpp"

#include <cstdio>

#include <QMainWindow>
#include <QObject>
#include <QProgressBar>
#include <QPushButton>
#include <QString>
#include <QToolBar>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "update_row.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::UpdateNotice* noticeOf(QToolBar* bar) {
  return bar != nullptr ? bar->findChild<mitcad::UpdateNotice*>() : nullptr;
}

QString textOf(QToolBar* bar) {
  const mitcad::UpdateNotice* notice = noticeOf(bar);
  return notice != nullptr ? notice->text() : QString();
}

int progressOf(QToolBar* bar) {
  mitcad::UpdateNotice* notice = noticeOf(bar);
  const QProgressBar* progress = notice != nullptr ? notice->findChild<QProgressBar*>() : nullptr;
  return progress != nullptr && !progress->isHidden() ? progress->value() : -1;
}

QPushButton* buttonOf(QToolBar* bar, const char* name) {
  mitcad::UpdateNotice* notice = noticeOf(bar);
  return notice != nullptr ? notice->findChild<QPushButton*>(QLatin1String(name)) : nullptr;
}

bool buttonShown(QToolBar* bar, const char* name) {
  const QPushButton* button = buttonOf(bar, name);
  return button != nullptr && !button->isHidden();
}

const QString kDownloading = QStringLiteral("Downloading MitCad 1.2.0");

} // namespace

int updateRowTests() {
  using mitcad::UpdateRow;
  UpdateRow row;
  int installs = 0;
  int skips = 0;
  int closes = 0;
  // The controller's part: Install starts the download, Skip and Close
  // hide the row.
  QObject::connect(&row, &UpdateRow::installClicked, &row, [&row, &installs] {
    ++installs;
    row.showProgress(kDownloading, 0, 1000);
  });
  QObject::connect(&row, &UpdateRow::skipClicked, &row, [&row, &skips] {
    ++skips;
    row.hide();
  });
  QObject::connect(&row, &UpdateRow::closed, &row, [&row, &closes] {
    ++closes;
    row.hide();
  });

  auto* first = new QMainWindow;
  auto* second = new QMainWindow;
  QToolBar* firstBar = row.attach(*first);
  QToolBar* secondBar = row.attach(*second);
  CHECK(row.windowCount() == 2);
  CHECK(firstBar->objectName() == QStringLiteral("updateBar"));
  CHECK(firstBar->isHidden() && secondBar->isHidden());
  CHECK(row.kind() == UpdateRow::Kind::Empty);

  // The offer, in both windows.
  row.showOffer(QStringLiteral("<b>MitCad 1.2.0</b> is available (this is 1.1.0)."), true, true);
  CHECK(row.shown());
  CHECK(!firstBar->isHidden() && !secondBar->isHidden());
  CHECK(textOf(firstBar) == QStringLiteral("MitCad 1.2.0 is available (this is 1.1.0)."));
  CHECK(textOf(secondBar) == textOf(firstBar));
  CHECK(buttonShown(firstBar, "updateNoticeInstall") && buttonShown(secondBar, "updateNoticeInstall"));

  // Install clicked in the first window: once for the application, the
  // download's progress in both.
  QPushButton* install = buttonOf(firstBar, "updateNoticeInstall");
  CHECK(install != nullptr);
  if (install != nullptr) {
    install->click();
  }
  CHECK(installs == 1);
  CHECK(row.kind() == UpdateRow::Kind::Progress);
  CHECK(textOf(firstBar) == kDownloading && textOf(secondBar) == kDownloading);
  CHECK(buttonShown(secondBar, "updateNoticeCancel") && !buttonShown(secondBar, "updateNoticeInstall"));
  row.showProgress(kDownloading, 250, 1000);
  CHECK(progressOf(firstBar) == 250 && progressOf(secondBar) == 250);

  // The window that accepted the update closes: the second keeps the
  // state and its progress, and the download goes on there.
  delete first;
  CHECK(row.windowCount() == 1);
  CHECK(row.shown());
  CHECK(row.kind() == UpdateRow::Kind::Progress);
  CHECK(row.received() == 250 && row.total() == 1000);
  CHECK(!secondBar->isHidden());
  CHECK(textOf(secondBar) == kDownloading);
  CHECK(progressOf(secondBar) == 250);
  row.showProgress(kDownloading, 600, 1000);
  CHECK(progressOf(secondBar) == 600);

  // A window opened now shows where the download is, at once.
  auto* third = new QMainWindow;
  QToolBar* thirdBar = row.attach(*third);
  CHECK(row.windowCount() == 2);
  CHECK(!thirdBar->isHidden());
  CHECK(textOf(thirdBar) == kDownloading);
  CHECK(progressOf(thirdBar) == 600);
  CHECK(buttonShown(thirdBar, "updateNoticeCancel"));

  // Downloaded: in every window left.
  row.showProgress(kDownloading, 1000, 1000);
  row.showMessage(QStringLiteral("MitCad 1.2.0 is downloaded."), mitcad::NoticeLevel::Success);
  CHECK(row.kind() == UpdateRow::Kind::Message);
  CHECK(textOf(secondBar) == QStringLiteral("MitCad 1.2.0 is downloaded."));
  CHECK(textOf(thirdBar) == textOf(secondBar));
  CHECK(progressOf(secondBar) == -1 && progressOf(thirdBar) == -1);
  CHECK(buttonShown(secondBar, "updateNoticeClose") && buttonShown(thirdBar, "updateNoticeClose"));

  // Skip This Version in one window: once, and hidden in every window.
  row.showOffer(QStringLiteral("<b>MitCad 1.3.0</b> is available (this is 1.1.0)."), true, true);
  CHECK(!secondBar->isHidden() && !thirdBar->isHidden());
  QPushButton* skip = buttonOf(thirdBar, "updateNoticeSkip");
  CHECK(skip != nullptr);
  if (skip != nullptr) {
    skip->click();
  }
  CHECK(skips == 1);
  CHECK(!row.shown());
  CHECK(secondBar->isHidden() && thirdBar->isHidden());

  // Later in one window: closed once, hidden in every window.
  row.showOffer(QStringLiteral("<b>MitCad 1.3.0</b> is available (this is 1.1.0)."), true, true);
  QPushButton* later = buttonOf(secondBar, "updateNoticeLater");
  CHECK(later != nullptr);
  if (later != nullptr) {
    later->click();
  }
  CHECK(closes == 1);
  CHECK(secondBar->isHidden() && thirdBar->isHidden());

  delete second;
  delete third;
  CHECK(row.windowCount() == 0);
  // With no window the row keeps its content (a window made later shows it).
  CHECK(row.kind() == UpdateRow::Kind::Offer);
  return failures;
}
MITCAD_UNIT_TEST(updateRowTests);
