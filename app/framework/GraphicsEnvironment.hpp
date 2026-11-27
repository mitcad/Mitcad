// SPDX-License-Identifier: MIT
// What a Linux session's graphics run on (mitcad#2031, GitHub
// mitcad/Mitcad#434), for the start-up's "Graphics environment:" line after
// "Display platform:": the session type and desktop, the graphics cards
// the kernel lists (a laptop with Intel and NVIDIA has two), the variables
// that send OpenGL to another card (NVIDIA's render offload, Mesa's
// DRI_PRIME) and those that scale Qt's window. The GPU that draws is the
// "OpenGL renderer:" line once the 3D view starts; this line says what
// else there was to choose from and what the launch asked for.
// Plain Qt Core, so that app.unit tests it on every platform.
#pragma once

#include <QString>
#include <QStringList>

#include <vector>

namespace mitcad {

// A graphics card as the kernel lists it (/sys/class/drm/cardN).
struct GraphicsCard {
  QString name;        // "card1"
  QString vendor;      // "NVIDIA", "Intel", "AMD", or "vendor 0x1234"
  QString driver;      // the kernel driver ("nvidia", "i915"); empty: none named
  bool bootDisplay = false; // the card the firmware started the display on
};

struct GraphicsEnvironment {
  QString sessionType; // XDG_SESSION_TYPE ("wayland", "x11")
  QString desktop;     // XDG_CURRENT_DESKTOP ("GNOME", "KDE")
  std::vector<GraphicsCard> cards;
  // "NAME=value" of the render offload and scaling variables that are set,
  // in a fixed order.
  QStringList offload;
  QStringList scaling;
};

// The cards under `drmRoot` (normally /sys/class/drm): every "cardN"
// directory whose device names a vendor or a driver (a card without PCI,
// as on many ARM boards, has only its driver), in the order of their
// numbers.
// The vendor comes from device/vendor (or device/uevent's PCI_ID), the
// driver from device/uevent's DRIVER, the boot display from
// device/boot_vga. Nothing when the folder does not exist (not Linux).
std::vector<GraphicsCard> graphicsCards(const QString& drmRoot);

// The current process's: its variables and the cards under /sys/class/drm.
GraphicsEnvironment graphicsEnvironment();

// The line after "Graphics environment: ", e.g. "session wayland (GNOME);
// cards: card0 Intel (i915, boot display), card1 NVIDIA (nvidia); render
// offload: __NV_PRIME_RENDER_OFFLOAD=1, __GLX_VENDOR_LIBRARY_NAME=nvidia;
// scaling: Qt's from the screen".
QString graphicsEnvironmentDescription(const GraphicsEnvironment& environment);

} // namespace mitcad
