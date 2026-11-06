// SPDX-License-Identifier: MIT
// Updates (mitcad#1390): the notice of a release tells apart an
// installation that only announces (the release has a download for this
// platform: "Download it from its release page.") from a release that is
// out for other platforms first (no download here yet: said so, no
// Install, no Skip This Version, and an automatic check shows nothing).
#include "update/UpdateOffer.hpp"

#include <cstdio>

#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "update_offer.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QString text(const char* value) { return QString::fromLatin1(value); }

mitcad::UpdateOffer release(bool hasDownload) {
  mitcad::UpdateOffer offer;
  offer.status = text("available");
  offer.version = text("1.2.0");
  offer.date = text("2026-11-06");
  offer.notes = text("https://example.org/notes");
  offer.hasDownload = hasDownload;
  if (hasDownload) {
    offer.url = text("https://example.org/Mitcad.AppImage");
    offer.size = 1000;
  }
  return offer;
}

void platformNameTests() {
  CHECK(mitcad::platformName(text("windows-x64")) == text("Windows"));
  CHECK(mitcad::platformName(text("linux-x64")) == text("Linux"));
  CHECK(mitcad::platformName(text("linux-arm64")) == text("Linux"));
  CHECK(mitcad::platformName(text("macos-arm64")) == text("macOS"));
  CHECK(mitcad::platformName(text("macos-x64")) == text("macOS"));
  CHECK(mitcad::platformName(text("plan9-mips")).isEmpty());
  CHECK(mitcad::platformName(QString()).isEmpty());
}

void noticeTests() {
  const QString current = text("1.1.0");
  // 1. A notify-only installation, the release has this platform's
  // download: the release page is the way to it.
  {
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(release(true), current, text("linux-x64"), false, false, false);
    CHECK(notice.show);
    CHECK(!notice.install);
    CHECK(notice.skip);
    CHECK(notice.html == text("<b>MitCad 1.2.0</b> is available (this is 1.1.0). Download it from its release page."));
  }
  // The same release where it can be installed: Install, no release page.
  {
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(release(true), current, text("linux-x64"), true, false, false);
    CHECK(notice.show);
    CHECK(notice.install);
    CHECK(notice.skip);
    CHECK(notice.html == text("<b>MitCad 1.2.0</b> is available (this is 1.1.0)."));
  }
  // 2. No download for this platform yet, Check for Updates: said so in
  // the user's name of the platform; no Install, no Skip This Version,
  // and no release page as the way to a download.
  {
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(release(false), current, text("macos-arm64"), true, true, false);
    CHECK(notice.show);
    CHECK(!notice.install);
    CHECK(!notice.skip);
    CHECK(notice.html == text("<b>MitCad 1.2.0</b> is out (this is 1.1.0), but there is no download for macOS yet."));
    CHECK(!notice.html.contains(text("Download it from its release page")));
  }
  {
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(release(false), current, text("windows-x64"), false, true, false);
    CHECK(notice.show && !notice.install && !notice.skip);
    CHECK(notice.html == text("<b>MitCad 1.2.0</b> is out (this is 1.1.0), but there is no download for Windows yet."));
  }
  // A platform without a name of its own: this computer.
  {
    const mitcad::OfferNotice notice = mitcad::offerNotice(release(false), current, QString(), false, true, false);
    CHECK(notice.html ==
          text("<b>MitCad 1.2.0</b> is out (this is 1.1.0), but there is no download for this computer yet."));
  }
  // 3. The same release found by the automatic check: no notice.
  {
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(release(false), current, text("linux-x64"), true, false, false);
    CHECK(!notice.show);
    CHECK(!notice.install);
    CHECK(!notice.skip);
  }
  // A release with this platform's download is shown by the automatic
  // check as before.
  CHECK(mitcad::offerNotice(release(true), current, text("linux-x64"), false, false, false).show);

  // A pre-release without a download here, previously skipped, with the
  // hint of where checks are turned off.
  {
    mitcad::UpdateOffer beta = release(false);
    beta.version = text("1.3.0-beta.1");
    beta.prerelease = true;
    beta.status = text("skipped");
    const mitcad::OfferNotice notice = mitcad::offerNotice(beta, current, text("linux-x64"), true, true, true);
    CHECK(!notice.skip);
    CHECK(notice.html.startsWith(text("<b>MitCad 1.3.0-beta.1</b> is out (this is 1.1.0), but there is no download "
                                      "for Linux yet. It is a pre-release. You skipped this version. MitCad checks "
                                      "for updates once a day;")));
  }
  // An expired manifest with this platform's download (mitcad#938): why,
  // and the release page; never Install.
  {
    mitcad::UpdateOffer expired = release(true);
    expired.status = text("current");
    expired.expired = true;
    expired.expires = text("2020-02-01");
    const mitcad::OfferNotice notice = mitcad::offerNotice(expired, current, text("linux-x64"), true, true, false);
    CHECK(notice.show && !notice.install && notice.skip);
    CHECK(notice.html == text("<b>MitCad 1.2.0</b> is available (this is 1.1.0). Its update information expired on "
                              "2020-02-01, so MitCad does not install it; if that date has not passed, check the "
                              "date of this computer. Download it from its release page."));
  }
  // Values from the network are escaped (mitcad#938).
  {
    mitcad::UpdateOffer marked = release(false);
    marked.version = text("<i>9</i>");
    const mitcad::OfferNotice notice =
        mitcad::offerNotice(marked, text("<u>1</u>"), text("linux-x64"), true, true, false);
    CHECK(notice.html.startsWith(text("<b>MitCad &lt;i&gt;9&lt;/i&gt;</b> is out (this is &lt;u&gt;1&lt;/u&gt;)")));
  }
}

} // namespace

int updateOfferTests() {
  platformNameTests();
  noticeTests();
  return failures;
}
