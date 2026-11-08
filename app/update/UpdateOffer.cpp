// SPDX-License-Identifier: MIT
#include "UpdateOffer.hpp"

#include <QChar>
#include <QCoreApplication>

namespace mitcad {
namespace {

// The file's texts in the context UpdateController, where lupdate records
// them too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(UpdateController)
};

} // namespace

QString platformName(const QString& platform) {
  const QString system = platform.section(QLatin1Char('-'), 0, 0);
  if (system == QStringLiteral("windows")) {
    return QStringLiteral("Windows");
  }
  if (system == QStringLiteral("linux")) {
    return QStringLiteral("Linux");
  }
  if (system == QStringLiteral("macos")) {
    return QStringLiteral("macOS");
  }
  return QString();
}

OfferNotice offerNotice(const UpdateOffer& offer, const QString& current, const QString& platform, bool canInstall,
                        bool manual, bool hint) {
  OfferNotice notice;
  // Rich text for the bold version: every value escaped, as they come from
  // the network (mitcad#938).
  const QString version = offer.version.toHtmlEscaped();
  const QString running = current.toHtmlEscaped();
  if (!offer.hasDownload) {
    // Out for other platforms first (mitcad#1390): there is nothing to
    // download here yet. The release page is not offered as the way to
    // it, an automatic check stays silent, and the version is not skipped.
    notice.show = manual;
    notice.skip = false;
    const QString name = platformName(platform);
    notice.html = name.isEmpty()
                      ? Text::tr("<b>MitCad %1</b> is out (this is %2), but there is no download for this computer yet.")
                            .arg(version, running)
                      : Text::tr("<b>MitCad %1</b> is out (this is %2), but there is no download for %3 yet.")
                            .arg(version, running, name.toHtmlEscaped());
    if (offer.prerelease) {
      notice.html += QLatin1Char(' ') + Text::tr("It is a pre-release.");
    }
  } else {
    notice.install = canInstall && !offer.expired;
    notice.html = Text::tr("<b>MitCad %1</b> is available (this is %2).").arg(version, running);
    if (offer.prerelease) {
      notice.html += QLatin1Char(' ') + Text::tr("It is a pre-release.");
    }
    if (offer.expired) {
      notice.html += QLatin1Char(' ') + Text::tr("Its update information expired on %1, so MitCad does not install "
                                           "it; if that date has not passed, check the date of this computer.")
                                            .arg(offer.expires.toHtmlEscaped());
    }
    if (!notice.install) {
      // An installation that announces only (mitcad#1390: the download
      // for this platform is there).
      notice.html += QLatin1Char(' ') + Text::tr("Download it from its release page.");
    }
  }
  if (offer.status == QStringLiteral("skipped")) {
    notice.html += QLatin1Char(' ') + Text::tr("You skipped this version.");
  }
  if (hint) {
    notice.html += QLatin1Char(' ') + Text::tr("MitCad checks for updates once a day; Tools › Preferences turns that off.");
  }
  return notice;
}

} // namespace mitcad
