// SPDX-License-Identifier: MIT
// What a tutorial page may make MitCad do (mitcad#2615,
// webbrowser/TutorialPageRules.hpp): a page's text in the log cut and with
// its line breaks and direction marks escaped, so that it cannot make a
// line look like MitCad's own; the bridge's calls at most 20 a second, 4
// of each in flight and none larger than 4 MB; a scripted click only for
// the key of the link MitCad asked about; only http and https links go to
// the system's browser.
#include "UnitTests.hpp"
#include "webbrowser/TutorialPageRules.hpp"

#include <cstdio>

#include <QChar>
#include <QString>
#include <QUrl>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "tutorial_page_rules.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expression) check((expression), #expression, __LINE__)

namespace rules = mitcad::tutorialrules;

void logTextTests() {
  // A forged second line, a tab and a backslash: one line, readable.
  CHECK(rules::logSafeText(QStringLiteral("done\nGuide: step x done"), 100) == QStringLiteral("done\\nGuide: step x done"));
  CHECK(rules::logSafeText(QStringLiteral("a\tb\\c\r"), 100) == QStringLiteral("a\\tb\\\\c\\r"));
  // A direction override and an escape character as escapes.
  const QString turned = QStringLiteral("ok ") + QChar(0x202E) + QStringLiteral("txt") + QChar(0x1B);
  CHECK(rules::logSafeText(turned, 100) == QStringLiteral("ok \\u{202e}txt\\u{1b}"));
  // Plain text, accents and quotes stay as they are.
  const QString plain = QStringLiteral("done extrude {\"started\":\"solid.extrude\"} ") + QChar(0xE4);
  CHECK(rules::logSafeText(plain, 100) == plain);
  // A long text is cut, with its length.
  const QString cut = rules::logSafeText(QString(500, QLatin1Char('x')), 10);
  CHECK(cut.startsWith(QString(10, QLatin1Char('x'))));
  CHECK(cut.endsWith(QStringLiteral(" (500 characters)")));
  CHECK(cut.size() < 40);
  // Never half a character outside the Basic Multilingual Plane.
  const QString emoji = QStringLiteral("a") + QString::fromUcs4(U"\U0001F600") + QStringLiteral("b");
  CHECK(!rules::logSafeText(emoji, 2).at(1).isHighSurrogate());
}

void limiterTests() {
  rules::BridgeLimiter limiter;
  QString reason;
  // At most 4 of a call in flight (a page's Promise.all of two queries is
  // fine): the fifth state waits for an answer.
  for (int i = 0; i < 4; ++i) {
    CHECK(limiter.admit(QStringLiteral("state"), 10, 0, &reason));
  }
  CHECK(!limiter.admit(QStringLiteral("state"), 10, 1, &reason));
  CHECK(reason.contains(QStringLiteral("still being answered")));
  limiter.finished(QStringLiteral("state"));
  CHECK(limiter.admit(QStringLiteral("state"), 10, 2, &reason));
  for (int i = 0; i < 4; ++i) {
    limiter.finished(QStringLiteral("state"));
  }
  // At most 20 a second (5 counted so far), then again a second later.
  for (int i = 0; i < 15; ++i) {
    CHECK(limiter.admit(QStringLiteral("call%1").arg(i), 10, 100, &reason));
  }
  CHECK(!limiter.admit(QStringLiteral("query"), 10, 500, &reason));
  CHECK(reason.contains(QStringLiteral("20 a second")));
  CHECK(limiter.admit(QStringLiteral("query"), 10, 1100, &reason));
  // A request larger than 4 MB.
  rules::BridgeLimiter fresh;
  CHECK(!fresh.admit(QStringLiteral("demonstrate"), rules::kBridgeRequestBytes + 1, 0, &reason));
  CHECK(reason.contains(QStringLiteral("4 MB")));
  CHECK(fresh.admit(QStringLiteral("demonstrate"), rules::kBridgeRequestBytes, 0, &reason));
}

void linkTests() {
  const QString key = rules::newClickKey();
  CHECK(key.size() == 32);
  CHECK(key != rules::newClickKey());
  const QString script = rules::linkAtScript(QStringLiteral("Next'); alert(1); ('"), key);
  // The text stays a string of the script; the key is in its answer.
  CHECK(script.contains(QStringLiteral("[\"Next'); alert(1); ('\"]")));
  CHECK(script.contains(QStringLiteral("'mitcad:linkat:") + key + QStringLiteral(":'")));
  int x = 0;
  int y = 0;
  QString href;
  CHECK(rules::parseLinkAt(QStringLiteral("mitcad:linkat:") + key + QStringLiteral(":12,34,file:///a,b.html"), key, &x,
                           &y, &href));
  CHECK(x == 12 && y == 34 && href == QStringLiteral("file:///a,b.html"));
  // A page's own line: no key, another key, or none asked for.
  CHECK(!rules::parseLinkAt(QStringLiteral("mitcad:linkat:12,34,https://example.com"), key, &x, &y, &href));
  CHECK(!rules::parseLinkAt(QStringLiteral("mitcad:linkat:") + rules::newClickKey() + QStringLiteral(":1,2,x"), key, &x,
                            &y, &href));
  CHECK(!rules::parseLinkAt(QStringLiteral("mitcad:linkat::1,2,x"), QString(), &x, &y, &href));
  CHECK(!rules::parseLinkAt(QStringLiteral("mitcad:linkat:") + key + QStringLiteral(":a,2,x"), key, &x, &y, &href));
}

void webLinkTests() {
  CHECK(rules::webLinkAddress(QUrl(QStringLiteral("https://example.com/page"))));
  CHECK(rules::webLinkAddress(QUrl(QStringLiteral("HTTP://example.com"))));
  CHECK(!rules::webLinkAddress(QUrl(QStringLiteral("file:///home/user/design.mitcad"))));
  CHECK(!rules::webLinkAddress(QUrl(QStringLiteral("javascript:alert(1)"))));
  CHECK(!rules::webLinkAddress(QUrl(QStringLiteral("data:text/html,hello"))));
  CHECK(!rules::webLinkAddress(QUrl(QStringLiteral("mailto:someone@example.com"))));
  CHECK(!rules::webLinkAddress(QUrl()));
}

} // namespace

int tutorialPageRulesTests() {
  logTextTests();
  limiterTests();
  linkTests();
  webLinkTests();
  return failures;
}
MITCAD_UNIT_TEST(tutorialPageRulesTests);
