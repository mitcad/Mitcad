// SPDX-License-Identifier: MIT
// The notices' levels (mitcad#1040, docs/ui-conventions.md "Messages and
// severity"): the model's levels read, the worst of several, each level's
// word, colour role, highlight box fill and lifetime, and its badge, which
// differs by shape as well as colour; hints and progress have none.
#include "framework/NoticeLevel.hpp"

#include <cstdio>
#include <cstdlib>
#include <initializer_list>

#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPainter>
#include <QPalette>
#include <QRectF>
#include <QString>

#include "framework/Theme.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "notice_level.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QPalette windowPalette(const QColor& window, const QColor& text) {
  QPalette palette;
  palette.setColor(QPalette::Window, window);
  palette.setColor(QPalette::WindowText, text);
  return palette;
}

QJsonArray notices(std::initializer_list<const char*> levels) {
  QJsonArray array;
  for (const char* level : levels) {
    array.append(QJsonObject{{QStringLiteral("level"), QString::fromLatin1(level)},
                             {QStringLiteral("message"), QStringLiteral("m")}});
  }
  return array;
}

void modelTests() {
  CHECK(noticeLevelFromModel(QStringLiteral("error")) == NoticeLevel::Error);
  CHECK(noticeLevelFromModel(QStringLiteral("warning")) == NoticeLevel::Warning);
  CHECK(noticeLevelFromModel(QStringLiteral("info")) == NoticeLevel::Information);
  CHECK(noticeLevelFromModel(QString(), NoticeLevel::Hint) == NoticeLevel::Hint);
  CHECK(worstNoticeLevel(notices({"info", "error", "warning"})) == NoticeLevel::Error);
  CHECK(worstNoticeLevel(notices({"info", "warning"})) == NoticeLevel::Warning);
  CHECK(worstNoticeLevel(notices({"info"})) == NoticeLevel::Information);
  CHECK(worstNoticeLevel(QJsonArray(), NoticeLevel::Hint) == NoticeLevel::Hint);
}

void wordTests() {
  CHECK(noticeLevelName(NoticeLevel::Error) == QStringLiteral("error"));
  CHECK(noticeLevelName(NoticeLevel::Warning) == QStringLiteral("warning"));
  CHECK(noticeLevelName(NoticeLevel::Information) == QStringLiteral("info"));
  CHECK(noticeLevelName(NoticeLevel::Success) == QStringLiteral("success"));
  CHECK(noticeLevelName(NoticeLevel::Hint) == QStringLiteral("hint"));
  CHECK(noticeLevelName(NoticeLevel::Progress) == QStringLiteral("progress"));
  CHECK(noticeLevelText(NoticeLevel::Error, QStringLiteral("no result")) == QStringLiteral("Failed: no result"));
  CHECK(noticeLevelText(NoticeLevel::Warning, QStringLiteral("lost 2 edges")) ==
        QStringLiteral("Warning: lost 2 edges"));
  CHECK(noticeLevelText(NoticeLevel::Warning, QString()) == QStringLiteral("Warning"));
  CHECK(noticeLevelText(NoticeLevel::Information, QStringLiteral("Done.")) == QStringLiteral("Done."));
  CHECK(noticeLevelText(NoticeLevel::Hint, QStringLiteral("Pick a face")) == QStringLiteral("Pick a face"));
  CHECK(isProblem(NoticeLevel::Error) && isProblem(NoticeLevel::Warning));
  CHECK(!isProblem(NoticeLevel::Information) && !isProblem(NoticeLevel::Hint));
  CHECK(noticeTimeout(NoticeLevel::Error) == 10000 && noticeTimeout(NoticeLevel::Warning) == 10000);
  CHECK(noticeTimeout(NoticeLevel::Hint) == 6000 && noticeTimeout(NoticeLevel::Information) == 6000);
  CHECK(noticeSymbolName(NoticeLevel::Error) == QStringLiteral("exclamationmark.circle.fill"));
  CHECK(noticeSymbolName(NoticeLevel::Warning) == QStringLiteral("exclamationmark.triangle.fill"));
  CHECK(noticeSymbolName(NoticeLevel::Hint).isEmpty());
}

void colourTests() {
  const QPalette night = windowPalette(QColor(0x14, 0x15, 0x18), QColor(0xe6, 0xe8, 0xec));
  const QPalette day = windowPalette(QColor(0xe4, 0xe7, 0xec), QColor(0x1d, 0x20, 0x26));
  // The text colours are the theme's roles.
  CHECK(noticeColor(NoticeLevel::Error, day) == errorColor(day));
  CHECK(noticeColor(NoticeLevel::Warning, night) == warningColor(night));
  // The highlight boxes' fills (mitcad#852 comment 3); none for the others.
  CHECK(noticeFill(NoticeLevel::Error, day) == QColor(0xfb, 0xe3, 0xe3));
  CHECK(noticeFill(NoticeLevel::Error, night) == QColor(0x4a, 0x20, 0x25));
  CHECK(noticeFill(NoticeLevel::Warning, day) == QColor(0xff, 0xf1, 0xc8));
  CHECK(noticeFill(NoticeLevel::Warning, night) == QColor(0x4a, 0x3a, 0x14));
  CHECK(!noticeFill(NoticeLevel::Information, day).isValid());
  CHECK(!noticeFill(NoticeLevel::Hint, night).isValid());
}

bool red(const QColor& c) { return c.red() > 180 && c.green() < 90 && c.blue() < 90; }
bool amber(const QColor& c) { return c.red() > 200 && c.green() > 140 && c.blue() < 60; }
bool blue(const QColor& c) { return c.blue() > 180 && c.red() < 80; }
bool green(const QColor& c) { return c.green() > 100 && c.red() < 60 && c.blue() < 90; }

void badgeTests() {
  const auto painted = [](NoticeLevel level) {
    QImage image(24, 24, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    paintNoticeBadge(painter, QRectF(6, 6, 12, 12), level, QColor(Qt::transparent));
    painter.end();
    return image;
  };
  // A circle for an error, information and a success, each its own colour;
  // a triangle (wide at its base) for a warning.
  CHECK(red(painted(NoticeLevel::Error).pixelColor(8, 12)));
  CHECK(painted(NoticeLevel::Error).pixelColor(6, 17).alpha() < 128);
  CHECK(amber(painted(NoticeLevel::Warning).pixelColor(6, 17)));
  CHECK(blue(painted(NoticeLevel::Information).pixelColor(8, 12)));
  // Above the check mark: its round cap reaches (8, 12) by antialiasing.
  CHECK(green(painted(NoticeLevel::Success).pixelColor(12, 8)));
  // Nothing for a hint or progress.
  CHECK(painted(NoticeLevel::Hint).pixelColor(12, 12).alpha() == 0);
  CHECK(noticeIcon(NoticeLevel::Progress).isNull());
  CHECK(!noticeIcon(NoticeLevel::Warning).isNull());
  // In rich text: the icon, the word and the message in the level's colour.
  const QPalette day = windowPalette(QColor(0xe4, 0xe7, 0xec), QColor(0x1d, 0x20, 0x26));
  const QString html = noticeHtml(NoticeLevel::Warning, QStringLiteral("a < b"), day);
  CHECK(html.startsWith(QStringLiteral("<img src=\"data:image/png;base64,")));
  CHECK(html.endsWith(QStringLiteral("<span style=\"color:%1\">Warning: a &lt; b</span>").arg(warningColor(day).name())));
  CHECK(noticeHtml(NoticeLevel::Hint, QStringLiteral("a < b"), day) == QStringLiteral("a &lt; b"));
}

} // namespace

int noticeLevelTests() {
  failures = 0;
  modelTests();
  wordTests();
  colourTests();
  badgeTests();
  return failures;
}
