// SPDX-License-Identifier: MIT
// What a Linux session's graphics run on (mitcad#2031): see
// GraphicsEnvironment.hpp.
#include "framework/GraphicsEnvironment.hpp"

#include <QDir>
#include <QFile>
#include <QRegularExpression>
#include <QtGlobal>

#include <algorithm>
#include <iterator>
#include <utility>

namespace mitcad {

namespace {

// The variables that send OpenGL to another card (NVIDIA's render offload
// and its GLX and EGL vendor choice, Mesa's DRI_PRIME, a software renderer
// asked for) and those that scale Qt's window, in the order logged.
constexpr const char* kOffloadVariables[] = {
    "__NV_PRIME_RENDER_OFFLOAD", "__NV_PRIME_RENDER_OFFLOAD_PROVIDER", "__GLX_VENDOR_LIBRARY_NAME",
    "__EGL_VENDOR_LIBRARY_FILENAMES", "__VK_LAYER_NV_optimus", "DRI_PRIME", "LIBGL_ALWAYS_SOFTWARE",
    "MESA_LOADER_DRIVER_OVERRIDE"};
constexpr const char* kScalingVariables[] = {"QT_SCALE_FACTOR", "QT_SCREEN_SCALE_FACTORS",
                                             "QT_SCALE_FACTOR_ROUNDING_POLICY", "QT_ENABLE_HIGHDPI_SCALING",
                                             "QT_FONT_DPI"};

QString fileText(const QString& path) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
    return QString();
  }
  return QString::fromUtf8(file.readAll()).trimmed();
}

QString vendorName(const QString& id) {
  const QString key = id.toLower();
  if (key == QLatin1String("0x10de")) {
    return QStringLiteral("NVIDIA");
  }
  if (key == QLatin1String("0x8086")) {
    return QStringLiteral("Intel");
  }
  if (key == QLatin1String("0x1002") || key == QLatin1String("0x1022")) {
    return QStringLiteral("AMD");
  }
  if (key == QLatin1String("0x1af4")) {
    return QStringLiteral("virtio");
  }
  if (key == QLatin1String("0x15ad")) {
    return QStringLiteral("VMware");
  }
  if (key == QLatin1String("0x1414")) {
    return QStringLiteral("Microsoft");
  }
  return key.isEmpty() ? QString() : QStringLiteral("vendor %1").arg(key);
}

QStringList setVariables(const char* const* begin, const char* const* end) {
  QStringList set;
  for (const char* const* name = begin; name != end; ++name) {
    if (qEnvironmentVariableIsSet(*name)) {
      set << QStringLiteral("%1=%2").arg(QString::fromLatin1(*name), qEnvironmentVariable(*name));
    }
  }
  return set;
}

} // namespace

std::vector<GraphicsCard> graphicsCards(const QString& drmRoot) {
  const QDir root(drmRoot);
  if (drmRoot.isEmpty() || !root.exists()) {
    return {};
  }
  static const QRegularExpression cardName(QStringLiteral("^card(\\d+)$"));
  std::vector<std::pair<int, GraphicsCard>> found;
  for (const QString& entry : root.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    const QRegularExpressionMatch match = cardName.match(entry);
    if (!match.hasMatch()) {
      continue; // a connector ("card0-eDP-1") or a render node
    }
    const QString device = root.filePath(entry) + QStringLiteral("/device");
    QString vendorId = fileText(device + QStringLiteral("/vendor"));
    QString driver;
    const QStringList uevent = fileText(device + QStringLiteral("/uevent")).split(QLatin1Char('\n'));
    for (const QString& line : uevent) {
      if (line.startsWith(QLatin1String("DRIVER="))) {
        driver = line.mid(7).trimmed();
      } else if (vendorId.isEmpty() && line.startsWith(QLatin1String("PCI_ID="))) {
        vendorId = QStringLiteral("0x") + line.mid(7).section(QLatin1Char(':'), 0, 0).trimmed();
      }
    }
    GraphicsCard card;
    card.name = entry;
    card.vendor = vendorName(vendorId);
    card.driver = driver;
    card.bootDisplay = fileText(device + QStringLiteral("/boot_vga")) == QLatin1String("1");
    if (card.vendor.isEmpty() && card.driver.isEmpty()) {
      continue; // nothing to tell it by
    }
    found.emplace_back(match.captured(1).toInt(), std::move(card));
  }
  std::sort(found.begin(), found.end(),
            [](const std::pair<int, GraphicsCard>& a, const std::pair<int, GraphicsCard>& b) {
              return a.first < b.first;
            });
  std::vector<GraphicsCard> cards;
  cards.reserve(found.size());
  for (auto& entry : found) {
    cards.push_back(std::move(entry.second));
  }
  return cards;
}

GraphicsEnvironment graphicsEnvironment() {
  GraphicsEnvironment environment;
  environment.sessionType = qEnvironmentVariable("XDG_SESSION_TYPE");
  environment.desktop = qEnvironmentVariable("XDG_CURRENT_DESKTOP");
  environment.cards = graphicsCards(QStringLiteral("/sys/class/drm"));
  environment.offload = setVariables(std::begin(kOffloadVariables), std::end(kOffloadVariables));
  environment.scaling = setVariables(std::begin(kScalingVariables), std::end(kScalingVariables));
  return environment;
}

QString graphicsEnvironmentDescription(const GraphicsEnvironment& environment) {
  QString session = environment.sessionType.isEmpty() ? QStringLiteral("session not named")
                                                      : QStringLiteral("session %1").arg(environment.sessionType);
  if (!environment.desktop.isEmpty()) {
    session += QStringLiteral(" (%1)").arg(environment.desktop);
  }
  QStringList cards;
  for (const GraphicsCard& card : environment.cards) {
    QStringList notes;
    if (!card.driver.isEmpty()) {
      notes << card.driver;
    }
    if (card.bootDisplay) {
      notes << QStringLiteral("boot display");
    }
    QString text = card.vendor.isEmpty() ? card.name : QStringLiteral("%1 %2").arg(card.name, card.vendor);
    if (!notes.isEmpty()) {
      text += QStringLiteral(" (%1)").arg(notes.join(QStringLiteral(", ")));
    }
    cards << text;
  }
  return QStringLiteral("%1; cards: %2; render offload: %3; scaling: %4")
      .arg(session, cards.isEmpty() ? QStringLiteral("none listed") : cards.join(QStringLiteral(", ")),
           environment.offload.isEmpty() ? QStringLiteral("none") : environment.offload.join(QStringLiteral(", ")),
           environment.scaling.isEmpty() ? QStringLiteral("Qt's from the screen")
                                         : environment.scaling.join(QStringLiteral(", ")));
}

} // namespace mitcad
