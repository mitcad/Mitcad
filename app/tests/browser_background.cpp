// SPDX-License-Identifier: MIT
// The Browser's background in the Floating layout (mitcad#419): Rows without
// a saved choice on every platform (mitcad#2144), a saved Card kept, Restore
// Defaults forgetting the choice (the default applies, Rows), a settings
// import without a choice writing none, and a Browser card that follows the
// background changing with each of them.
#include "UnitTests.hpp"
#include "browser/BrowserBackground.hpp"
#include "browser/BrowserPanel.hpp"
#include "framework/GlassCard.hpp"

#include <cstdio>
#include <optional>

#include <QCoreApplication>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_background.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kKey = QStringLiteral("appearance/browserBackground");

// What the settings file holds now, as the next start reads it.
QString savedValue() {
  QSettings settings;
  settings.sync();
  return settings.value(kKey).toString();
}

void choiceTests() {
  using mitcad::BrowserBackground;
  // The default: Rows, no choice saved.
  CHECK(mitcad::defaultBrowserBackground() == BrowserBackground::Rows);
  CHECK(!mitcad::chosenBrowserBackground().has_value());
  CHECK(mitcad::savedBrowserBackground() == BrowserBackground::Rows);
  CHECK(!QSettings().contains(kKey));

  // The user's Card is kept, as the next start reads it.
  mitcad::saveBrowserBackground(BrowserBackground::Card);
  CHECK(savedValue() == QStringLiteral("card"));
  CHECK(mitcad::chosenBrowserBackground() == BrowserBackground::Card);
  CHECK(mitcad::savedBrowserBackground() == BrowserBackground::Card);

  // Restore Defaults: no choice saved, Rows.
  mitcad::clearBrowserBackground();
  CHECK(!QSettings().contains(kKey));
  CHECK(!mitcad::chosenBrowserBackground().has_value());
  CHECK(mitcad::savedBrowserBackground() == BrowserBackground::Rows);

  // Rows chosen is a choice of its own, saved.
  mitcad::saveBrowserBackground(BrowserBackground::Rows);
  CHECK(savedValue() == QStringLiteral("rows"));
  CHECK(mitcad::chosenBrowserBackground() == BrowserBackground::Rows);

  // A settings import: a choice in it is saved; none in it writes none.
  mitcad::restoreBrowserBackground(BrowserBackground::Card);
  CHECK(savedValue() == QStringLiteral("card"));
  mitcad::restoreBrowserBackground(std::nullopt);
  CHECK(!QSettings().contains(kKey));
  CHECK(mitcad::savedBrowserBackground() == BrowserBackground::Rows);

  // An unknown value is no choice: the default.
  QSettings().setValue(kKey, QStringLiteral("glass"));
  CHECK(!mitcad::chosenBrowserBackground().has_value());
  CHECK(mitcad::savedBrowserBackground() == BrowserBackground::Rows);
  QSettings().remove(kKey);
}

void followerTests() {
  using mitcad::BrowserBackground;
  mitcad::GlassCard card;
  auto* browser = new mitcad::BrowserPanel;
  card.setContent(browser);
  // Without a choice the card is bare (Rows) and has no width grip.
  mitcad::followBrowserBackground(&card, browser);
  CHECK(card.isBare());
  CHECK(!card.isWidthAdjustable());
  // Card chosen: the card with its width grip.
  mitcad::saveBrowserBackground(BrowserBackground::Card);
  CHECK(!card.isBare());
  CHECK(card.isWidthAdjustable());
  // Restore Defaults: Rows again, at once.
  mitcad::clearBrowserBackground();
  CHECK(card.isBare());
  CHECK(!card.isWidthAdjustable());
}

} // namespace

int browserBackgroundTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("browser_background"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());

  choiceTests();
  followerTests();

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

MITCAD_UNIT_TEST(browserBackgroundTests);
