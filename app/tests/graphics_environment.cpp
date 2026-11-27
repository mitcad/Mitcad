// SPDX-License-Identifier: MIT
// The start-up's "Graphics environment:" line (mitcad#2031,
// framework/GraphicsEnvironment.hpp): the graphics cards read from a
// folder laid out as /sys/class/drm (a laptop's Intel and NVIDIA cards,
// connectors and render nodes skipped, a card without PCI told by its
// driver), and the line with the session, the cards, the render offload
// and the scaling variables, or what it says when none are set.
#include "UnitTests.hpp"
#include "framework/GraphicsEnvironment.hpp"

#include <cstdio>
#include <vector>

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "graphics_environment.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool writeFile(const QString& path, const QByteArray& text) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(text) == text.size();
}

// A card folder as the kernel has it: device/vendor, device/uevent and
// device/boot_vga, each only when given.
bool addCard(const QDir& root, const QString& name, const QByteArray& vendor, const QByteArray& uevent,
             const QByteArray& bootVga) {
  const QString device = root.filePath(name) + QStringLiteral("/device");
  if (!QDir().mkpath(device)) {
    return false;
  }
  bool written = true;
  if (!vendor.isEmpty()) {
    written = writeFile(device + QStringLiteral("/vendor"), vendor) && written;
  }
  if (!uevent.isEmpty()) {
    written = writeFile(device + QStringLiteral("/uevent"), uevent) && written;
  }
  if (!bootVga.isEmpty()) {
    written = writeFile(device + QStringLiteral("/boot_vga"), bootVga) && written;
  }
  return written;
}

void cardsTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const QDir root(folder.path());
  // The NVIDIA card first by name but second by number: listed by number.
  CHECK(addCard(root, QStringLiteral("card1"), "0x10de\n", "DRIVER=nvidia\nPCI_ID=10DE:1C8D\n", "0\n"));
  CHECK(addCard(root, QStringLiteral("card0"), "0x8086\n", "DRIVER=i915\nPCI_ID=8086:3E9B\n", "1\n"));
  // No vendor file: the vendor from uevent's PCI_ID.
  CHECK(addCard(root, QStringLiteral("card12"), QByteArray(), "DRIVER=amdgpu\nPCI_ID=1002:73BF\n", QByteArray()));
  // No PCI: told by its driver.
  CHECK(addCard(root, QStringLiteral("card2"), QByteArray(), "DRIVER=vc4\n", QByteArray()));
  // Nothing to tell it by, a connector and a render node: skipped.
  CHECK(QDir().mkpath(root.filePath(QStringLiteral("card3/device"))));
  CHECK(addCard(root, QStringLiteral("card0-eDP-1"), "0x8086\n", "DRIVER=i915\n", QByteArray()));
  CHECK(addCard(root, QStringLiteral("renderD128"), "0x8086\n", "DRIVER=i915\n", QByteArray()));

  const std::vector<mitcad::GraphicsCard> cards = mitcad::graphicsCards(folder.path());
  CHECK(cards.size() == 4);
  if (cards.size() == 4) {
    CHECK(cards[0].name == QStringLiteral("card0"));
    CHECK(cards[0].vendor == QStringLiteral("Intel"));
    CHECK(cards[0].driver == QStringLiteral("i915"));
    CHECK(cards[0].bootDisplay);
    CHECK(cards[1].name == QStringLiteral("card1"));
    CHECK(cards[1].vendor == QStringLiteral("NVIDIA"));
    CHECK(cards[1].driver == QStringLiteral("nvidia"));
    CHECK(!cards[1].bootDisplay);
    CHECK(cards[2].name == QStringLiteral("card2"));
    CHECK(cards[2].vendor.isEmpty());
    CHECK(cards[2].driver == QStringLiteral("vc4"));
    CHECK(cards[3].name == QStringLiteral("card12"));
    CHECK(cards[3].vendor == QStringLiteral("AMD"));
    CHECK(cards[3].driver == QStringLiteral("amdgpu"));
  }
  // No such folder (not Linux): none.
  CHECK(mitcad::graphicsCards(root.filePath(QStringLiteral("missing"))).empty());
  CHECK(mitcad::graphicsCards(QString()).empty());
}

void descriptionTests() {
  mitcad::GraphicsEnvironment hybrid;
  hybrid.sessionType = QStringLiteral("wayland");
  hybrid.desktop = QStringLiteral("GNOME");
  hybrid.cards.push_back({QStringLiteral("card0"), QStringLiteral("Intel"), QStringLiteral("i915"), true});
  hybrid.cards.push_back({QStringLiteral("card1"), QStringLiteral("NVIDIA"), QStringLiteral("nvidia"), false});
  hybrid.cards.push_back({QStringLiteral("card2"), QString(), QStringLiteral("vc4"), false});
  hybrid.offload << QStringLiteral("__NV_PRIME_RENDER_OFFLOAD=1") << QStringLiteral("__GLX_VENDOR_LIBRARY_NAME=nvidia");
  hybrid.scaling << QStringLiteral("QT_SCALE_FACTOR=2");
  CHECK(mitcad::graphicsEnvironmentDescription(hybrid) ==
        QStringLiteral("session wayland (GNOME); cards: card0 Intel (i915, boot display), card1 NVIDIA (nvidia), "
                       "card2 (vc4); render offload: __NV_PRIME_RENDER_OFFLOAD=1, "
                       "__GLX_VENDOR_LIBRARY_NAME=nvidia; scaling: QT_SCALE_FACTOR=2"));

  const mitcad::GraphicsEnvironment nothing;
  CHECK(mitcad::graphicsEnvironmentDescription(nothing) ==
        QStringLiteral("session not named; cards: none listed; render offload: none; scaling: Qt's from the screen"));
}

} // namespace

int graphicsEnvironmentTests() {
  cardsTests();
  descriptionTests();
  return failures;
}
MITCAD_UNIT_TEST(graphicsEnvironmentTests);
