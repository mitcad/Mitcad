// SPDX-License-Identifier: MIT
// 3D mice (mitcad#951): the devices' reports, the mapping of their axes to
// the camera (speed, dead zone, reversed axes, locks, the dominant axis, a
// flat view), the camera's move, the Navigation modes and Lock horizon
// (mitcad#978), the settings, and the controller driven
// by a backend of the test's own: its events move the camera, its buttons
// run their commands, and no device logs "3D mouse: none"; a motion's
// pacing and timing from given frame times (mitcad#1023); a device's split
// reports in padded slots, the raw reports' log, and each rotation axis
// alone with no zoom, the same through HID and spacenavd (mitcad#1034);
// each of the cap's six motions from the device's values to the model in
// Object mode, the motion's axes in the debug log, the cross-talk filter,
// the device's reach and Dominant axis only's hold, which keeps each of a
// user's logged SpaceNavigator motions on its own axis (mitcad#1720); Lock
// Rotation in and out of a sketch and a device's
// default button for its rotation lock key (mitcad#1910); two designs on
// the process's one connection, the input reaching the active one only
// (mitcad#2297).
#include "UnitTests.hpp"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <initializer_list>
#include <memory>
#include <utility>
#include <vector>

#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QtGlobal>

#include <Graphic3d_Camera.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "platform/SpaceMouseDriverReports.hpp"
#include "view/SpaceMouseCamera.hpp"
#include "view/SpaceMouseController.hpp"
#include "view/SpaceMouseNavigation.hpp"
#include "view/SpaceMouseTiming.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "space_mouse.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

bool nearly(double a, double b, double tolerance = 1e-9) { return std::abs(a - b) <= tolerance; }

using mitcad::SpaceMouseVec;
double dot(const SpaceMouseVec& a, const SpaceMouseVec& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
SpaceMouseVec minus(const SpaceMouseVec& a, const SpaceMouseVec& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
SpaceMouseVec unit(const SpaceMouseVec& a) {
  const double length = std::sqrt(dot(a, a));
  return {a[0] / length, a[1] / length, a[2] / length};
}

// The log lines the controller writes, caught while a test runs.
QStringList& caught() {
  static QStringList lines;
  return lines;
}
void catchMessage(QtMsgType, const QMessageLogContext&, const QString& message) { caught().append(message); }

// A backend of the test's: start() answers as told, the test sends its
// events through the callbacks.
class FakeBackend final : public mitcad::SpaceMouseBackend {
public:
  explicit FakeBackend(bool present) : m_present(present) {}
  QString name() const override { return QStringLiteral("Fake"); }
  bool start(QString& device) override {
    ++starts;
    if (m_present) {
      device = QStringLiteral("Test device");
    }
    return m_present;
  }
  void windowPlaced() override { ++placed; }
  int starts = 0;
  int placed = 0;

private:
  bool m_present;
};

void reportTests() {
  using namespace mitcad;
  SpaceMouseHidReport report;
  // Translation: x 350, y -2, z -350 (16-bit little endian).
  const std::uint8_t translation[] = {1, 0x5e, 0x01, 0xfe, 0xff, 0xa2, 0xfe};
  CHECK(parseSpaceMouseHidReport(translation, sizeof(translation), report));
  CHECK(report.translation && !report.rotation && !report.buttons);
  CHECK(report.t == (std::array<int, 3>{350, -2, -350}));
  const std::uint8_t rotation[] = {2, 0x10, 0x00, 0x00, 0x80, 0xff, 0x7f};
  CHECK(parseSpaceMouseHidReport(rotation, sizeof(rotation), report));
  CHECK(report.rotation && !report.translation);
  CHECK(report.r == (std::array<int, 3>{16, -32768, 32767}));
  // The newer devices' one report of 12 bytes.
  const std::uint8_t both[] = {1, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 0xfa, 0xff};
  CHECK(parseSpaceMouseHidReport(both, sizeof(both), report));
  CHECK(report.translation && report.rotation);
  CHECK(report.t == (std::array<int, 3>{1, 2, 3}) && report.r == (std::array<int, 3>{4, 5, -6}));
  const std::uint8_t buttons[] = {3, 0x02, 0x01};
  CHECK(parseSpaceMouseHidReport(buttons, sizeof(buttons), report));
  CHECK(report.buttons && report.pressed == 0x0102u);
  const std::uint8_t shortReport[] = {1, 0, 0};
  CHECK(!parseSpaceMouseHidReport(shortReport, sizeof(shortReport), report));
  const std::uint8_t unknown[] = {9, 0, 0, 0, 0, 0, 0};
  CHECK(!parseSpaceMouseHidReport(unknown, sizeof(unknown), report));
  CHECK(!parseSpaceMouseHidReport(nullptr, 0, report));

  // HID: X right, Y toward the user, Z down. Right, lifted (Z < 0) and
  // pulled (Y > 0) are +x, +y, +z of the view's frame. Its rotations turn
  // by the right-hand rule about the same axes and map as the translation
  // does (mitcad#1720): Ry about Y (toward the user) is a roll
  // counter-clockwise as the user sees it, +rz; Rz about Z (down) negative
  // turns counter-clockwise seen from above, +ry.
  const SpaceMouseAxes hid = spaceMouseFromHid({350, 175, -350}, {0, 350, -175});
  CHECK(nearly(hid[0], 1.0) && nearly(hid[1], 1.0) && nearly(hid[2], 0.5));
  CHECK(nearly(hid[3], 0.0) && nearly(hid[4], 0.5) && nearly(hid[5], 1.0));

  // spacenavd: a motion, a press and a release; its z points into the
  // screen, and its rz turns about that axis.
  SpnavEvent event = parseSpnavEvent({0, 350, 0, 175, 0, 0, -350, 16});
  CHECK(event.kind == SpnavEvent::Kind::Motion);
  const SpaceMouseAxes spnav = spaceMouseFromSpnav(event.motion);
  CHECK(nearly(spnav[0], 1.0) && nearly(spnav[2], -0.5) && nearly(spnav[5], 1.0));
  event = parseSpnavEvent({1, 2, 0, 0, 0, 0, 0, 0});
  CHECK(event.kind == SpnavEvent::Kind::Press && event.button == 2);
  event = parseSpnavEvent({2, 0, 0, 0, 0, 0, 0, 0});
  CHECK(event.kind == SpnavEvent::Kind::Release && event.button == 0);
  CHECK(parseSpnavEvent({7, 0, 0, 0, 0, 0, 0, 0}).kind == SpnavEvent::Kind::Other);

  std::vector<std::pair<int, bool>> changes;
  spaceMouseButtonChanges(0x5u, 0x6u, [&changes](int button, bool pressed) { changes.emplace_back(button, pressed); });
  CHECK(changes.size() == 2 && changes[0] == std::make_pair(0, false) && changes[1] == std::make_pair(1, true));
}

void mappingTests() {
  using namespace mitcad;
  SpaceMouseSettings settings;
  // The dead zone, the full deflection and beyond it, and the curve.
  CHECK(spaceMouseResponse(0.03) == 0.0 && spaceMouseResponse(-0.04) == 0.0);
  CHECK(nearly(spaceMouseResponse(1.0), 1.0) && nearly(spaceMouseResponse(1.7), 1.0) && nearly(spaceMouseResponse(-1.0), -1.0));
  CHECK(spaceMouseResponse(0.5) > 0.0 && spaceMouseResponse(0.5) < 0.5);
  CHECK(spaceMouseResponse(std::nan("")) == 0.0);

  const SpaceMouseAxes full{1.0, 1.0, 1.0, 1.0, 1.0, 1.0};
  SpaceMouseRates rates = spaceMouseRates(settings, full, false);
  CHECK(nearly(rates.panX, kSpaceMousePanRate) && nearly(rates.panY, kSpaceMousePanRate));
  CHECK(nearly(rates.zoom, kSpaceMouseZoomRate));
  CHECK(nearly(rates.pitch, kSpaceMouseTurnRate) && nearly(rates.yaw, kSpaceMouseTurnRate) &&
        nearly(rates.roll, kSpaceMouseTurnRate));
  // The speed scales every rate.
  settings.speed = 2.5;
  rates = spaceMouseRates(settings, full, false);
  CHECK(nearly(rates.panX, 2.5 * kSpaceMousePanRate) && nearly(rates.yaw, 2.5 * kSpaceMouseTurnRate));
  settings.speed = 100.0; // clamped to 4
  CHECK(nearly(spaceMouseRates(settings, full, false).zoom, 4.0 * kSpaceMouseZoomRate));
  settings.speed = 1.0;
  // A reversed axis, an axis off.
  settings.reversed[0] = true;
  settings.axisOn[4] = false;
  rates = spaceMouseRates(settings, full, false);
  CHECK(rates.panX < 0.0 && rates.yaw == 0.0 && rates.pitch > 0.0);
  settings = SpaceMouseSettings();
  // The locks, and a flat view, which turns nothing.
  settings.lockRotation = true;
  rates = spaceMouseRates(settings, full, false);
  CHECK(!rates.turning() && rates.panX > 0.0 && rates.zoom > 0.0);
  settings = SpaceMouseSettings();
  settings.lockTranslation = true;
  rates = spaceMouseRates(settings, full, false);
  CHECK(rates.turning() && rates.panX == 0.0 && rates.panY == 0.0 && rates.zoom == 0.0);
  settings = SpaceMouseSettings();
  rates = spaceMouseRates(settings, full, true);
  CHECK(!rates.turning() && rates.panX > 0.0 && rates.panY > 0.0 && rates.zoom > 0.0);
  // The dominant axis only.
  settings.dominantAxis = true;
  rates = spaceMouseRates(settings, {0.3, -0.2, 0.1, 0.0, -0.8, 0.5}, false);
  CHECK(rates.yaw < 0.0 && rates.panX == 0.0 && rates.panY == 0.0 && rates.zoom == 0.0 && rates.pitch == 0.0 &&
        rates.roll == 0.0);
  // It wins over the cross-talk filter: the strongest axis switched on
  // moves the view, also when a stronger one is switched off.
  settings.crossTalk = 90;
  CHECK(spaceMouseRates(settings, {0.3, -0.2, 0.1, 0.0, -0.8, 0.5}, false).yaw < 0.0);
  settings.axisOn[4] = false;
  rates = spaceMouseRates(settings, {0.3, -0.2, 0.1, 0.0, -0.8, 0.5}, false);
  CHECK(rates.roll > 0.0 && rates.yaw == 0.0 && rates.panX == 0.0 && rates.pitch == 0.0);
  // Off: nothing moves.
  settings = SpaceMouseSettings();
  settings.enabled = false;
  CHECK(!spaceMouseRates(settings, full, false).moving());
}

// The cross-talk filter (mitcad#1720): a cap that couples its axes, moved
// along one, reported 30-80 % of that motion on others. Pushed in and out:
// tz -0.55 with tx 0.43, rx -0.26 and rz -0.24; rolled sideways: rz -0.79
// with rx 0.48 and tx 0.37.
void crossTalkTests() {
  using namespace mitcad;
  const SpaceMouseAxes inOut{0.43, 0.0, -0.55, -0.26, 0.0, -0.24};
  const SpaceMouseAxes rolled{0.37, 0.0, 0.0, 0.48, 0.0, -0.79};
  const auto only = [](const SpaceMouseAxes& axes, std::size_t kept, double value) {
    for (std::size_t axis = 0; axis < 6; ++axis) {
      if (axes[axis] != (axis == kept ? value : 0.0)) {
        return false;
      }
    }
    return true;
  };
  // On by default, at 50 %.
  CHECK(SpaceMouseSettings().crossTalk == 50);
  // A share above the cap's coupling leaves the one axis it was moved along.
  CHECK(only(spaceMouseWithoutCrossTalk(inOut, 85), 2, -0.55));
  CHECK(only(spaceMouseWithoutCrossTalk(rolled, 85), 5, -0.79));
  SpaceMouseSettings settings;
  settings.crossTalk = 85;
  SpaceMouseRates rates = spaceMouseRates(settings, inOut, false);
  CHECK(rates.zoom < 0.0 && rates.panX == 0.0 && rates.panY == 0.0 && !rates.turning());
  rates = spaceMouseRates(settings, rolled, false);
  CHECK(rates.roll < 0.0 && rates.pitch == 0.0 && rates.yaw == 0.0 && rates.panX == 0.0 && rates.zoom == 0.0);
  // The default drops what is less than half the strongest: the in and out
  // keeps its 78 % pan, the roll its 61 % tilt.
  const SpaceMouseAxes inOutHalf = spaceMouseWithoutCrossTalk(inOut, 50);
  CHECK(inOutHalf == (SpaceMouseAxes{0.43, 0.0, -0.55, 0.0, 0.0, 0.0}));
  CHECK(spaceMouseWithoutCrossTalk(rolled, 50) == (SpaceMouseAxes{0.0, 0.0, 0.0, 0.48, 0.0, -0.79}));
  // A deliberate motion along two axes of similar size keeps both at the
  // default, and the small rest of the others goes.
  const SpaceMouseAxes diagonal{0.6, -0.5, 0.05, 0.03, -0.02, 0.04};
  CHECK(spaceMouseWithoutCrossTalk(diagonal, 50) == (SpaceMouseAxes{0.6, -0.5, 0.0, 0.0, 0.0, 0.0}));
  rates = spaceMouseRates(SpaceMouseSettings(), diagonal, false);
  CHECK(rates.panX > 0.0 && rates.panY < 0.0 && rates.zoom == 0.0 && !rates.turning());
  // An axis as strong as the strongest stays at any share; every axis full.
  const SpaceMouseAxes even{0.7, -0.7, 0.0, 0.0, 0.0, 0.0};
  CHECK(spaceMouseWithoutCrossTalk(even, kSpaceMouseCrossTalkMax) == even);
  const SpaceMouseAxes full{1.0, 1.0, 1.0, 1.0, 1.0, 1.0};
  CHECK(spaceMouseWithoutCrossTalk(full, 50) == full);
  // Beyond a full deflection counts as a full one.
  CHECK(spaceMouseWithoutCrossTalk({1.7, 0.9, 0.0, 0.0, 0.0, 0.0}, 85) ==
        (SpaceMouseAxes{1.7, 0.9, 0.0, 0.0, 0.0, 0.0}));
  // Off keeps every axis as it came.
  CHECK(spaceMouseWithoutCrossTalk(inOut, 0) == inOut && spaceMouseWithoutCrossTalk(rolled, 0) == rolled);
  settings.crossTalk = 0;
  rates = spaceMouseRates(settings, inOut, false);
  CHECK(rates.zoom < 0.0 && rates.panX > 0.0 && rates.pitch < 0.0 && rates.roll < 0.0);
  // A share out of range is taken within it; a value that is no number
  // does nothing and does not stop the others.
  CHECK(spaceMouseWithoutCrossTalk(inOut, -20) == inOut);
  CHECK(spaceMouseWithoutCrossTalk(inOut, 400) == spaceMouseWithoutCrossTalk(inOut, kSpaceMouseCrossTalkMax));
  CHECK(only(spaceMouseWithoutCrossTalk({std::nan(""), 0.0, -0.55, 0.0, 0.0, 0.0}, 50), 2, -0.55));
  // The coupling is the cap's: rolled with the roll switched off, its
  // stray tilt and pan do not move the view either.
  settings = SpaceMouseSettings();
  settings.crossTalk = 85;
  settings.axisOn[5] = false;
  CHECK(!spaceMouseRates(settings, rolled, false).moving());
  // In a flat view, a pan with a stray turn pans; a turn with a stray pan
  // does nothing.
  settings = SpaceMouseSettings();
  CHECK(spaceMouseRates(settings, {0.6, 0.0, 0.0, 0.2, 0.0, 0.0}, true).panX > 0.0);
  CHECK(!spaceMouseRates(settings, {0.2, 0.0, 0.0, 0.0, 0.0, 0.6}, true).moving());
  // Dominant axis only is as it was: the strongest axis switched on alone,
  // the filter's share aside.
  settings = SpaceMouseSettings();
  settings.dominantAxis = true;
  for (const int share : {0, 50, 90}) {
    settings.crossTalk = share;
    rates = spaceMouseRates(settings, inOut, false);
    CHECK(rates.zoom < 0.0 && rates.panX == 0.0 && !rates.turning());
  }
  // With the device's reach (mitcad#1720) the axes compare as shares of
  // their kind's: a device that has shown translations up to 0.5 and
  // rotations up to a full deflection keeps a push of 0.2 (two fifths of
  // its reach) beside a twist of 0.6, which the nominal scale drops.
  SpaceMouseReach reach(kSpaceMouseReachStart);
  reach.add({0.5, 0.0, 0.0, 0.0, 1.0, 0.0});
  const SpaceMouseAxes pushAndTwist{0.2, 0.0, 0.0, 0.0, 0.6, 0.0};
  CHECK(spaceMouseWithoutCrossTalk(pushAndTwist, 50) == (SpaceMouseAxes{0.0, 0.0, 0.0, 0.0, 0.6, 0.0}));
  CHECK(spaceMouseWithoutCrossTalk(pushAndTwist, 50, reach) == pushAndTwist);
}

// The rate a view axis (tx … rz) moves the view at.
double rateOf(const mitcad::SpaceMouseRates& rates, std::size_t axis) {
  switch (axis) {
  case 0:
    return rates.panX;
  case 1:
    return rates.panY;
  case 2:
    return rates.zoom;
  case 3:
    return rates.pitch;
  case 4:
    return rates.yaw;
  default:
    return rates.roll;
  }
}

// A motion of a 3D mouse's cap along one axis as a user logged it
// (mitcad#1720, GitHub mitcad/Mitcad#343): the debug log's "3D mouse
// report:" lines of a SpaceNavigator (HID 046d:c626, its translation and
// rotation in reports of their own), about ten a second, each the
// device's X, Y, Z and Rx, Ry, Rz as its reports give them; the cap came
// to rest after each motion. The reports between the motions, where the
// cap was only touched, are left out.
struct LoggedMotion {
  const char* name;
  std::size_t axis; // the view's axis (tx … rz) the cap was moved along
  double sign;      // its direction
  std::vector<std::array<int, 6>> reports;
};

const std::vector<LoggedMotion>& loggedSpaceNavigatorMotions() {
  static const std::vector<LoggedMotion> motions = {
      {"pushed right", 0, 1.0,
       {{33, 12, 0, 0, -3, -3}, {48, 24, -8, 0, -11, -5}, {68, 19, -10, 0, -29, -8}, {89, 16, -13, 0, -43, -3},
        {82, 11, -22, 0, -40, -3}, {88, 9, -12, -12, -30, -8}, {94, 19, -11, -12, -30, -10},
        {100, 0, -10, -17, -38, -13}, {107, 0, -6, -24, -38, -13}, {110, 0, -18, -28, -46, -16},
        {116, 0, -18, -25, -51, -18}, {133, 0, -14, -35, -51, -15}, {133, 0, -27, -32, -56, -15},
        {128, 0, -26, -36, -54, -15}, {132, 10, -7, -33, -43, -16}, {140, 3, -3, -33, -48, -11},
        {111, -1, 0, -41, -40, -13}, {116, -9, -17, -36, -54, -10}, {122, -10, -6, -45, -49, -6},
        {135, -20, 0, -58, -38, -11}, {39, -40, 0, -29, -22, -5}}},
      {"pushed left", 0, -1.0,
       {{-34, 0, -3, -6, 10, 0}, {-53, -24, -8, -4, 2, 0}, {-83, -47, -11, -9, 0, 0}, {-84, -70, 0, -26, 2, 0},
        {-92, -73, 0, -30, 10, 0}, {-99, -84, 0, -26, 7, 15}, {-111, -91, 17, -41, 23, 28},
        {-117, -124, 17, -51, 36, 31}, {-117, -93, 0, -40, 39, 31}, {-113, -75, 0, -29, 36, 36},
        {-121, -72, 0, -31, 34, 36}, {-114, -102, 32, -41, 39, 38}, {-100, -114, 54, -35, 29, 40},
        {-99, -116, 54, -28, 21, 38}, {-101, -106, 47, -27, 26, 41}, {-89, -103, 49, -30, 26, 43},
        {-75, -93, 37, -26, 24, 44}, {-79, -105, 39, -27, 26, 46}, {-79, -106, 45, -27, 21, 41},
        {-81, -102, 36, -29, 24, 38}, {-78, -96, 37, -29, 18, 33}, {-83, -96, 42, -33, 26, 30},
        {-91, -89, 39, -29, 29, 26}, {-90, -88, 34, -27, 26, 30}, {-51, -43, 24, -26, 18, 33},
        {0, 0, -4, -6, 0, -25}}},
      {"lifted", 1, 1.0,
       {{-3, 0, -27, -3, 0, 0}, {0, 3, -47, -3, 0, 0}, {0, 12, -92, 0, -17, 0}, {-2, 11, -115, 0, -14, 0},
        {-7, 0, -126, -12, -12, 0}, {-5, 4, -130, -5, -15, 0}, {-5, 14, -140, 0, -17, 0}, {-3, 20, -163, 0, -31, 0},
        {0, 19, -161, 0, -31, 0}, {0, 20, -152, 0, -28, 0}, {0, 24, -147, 0, -23, -1}, {0, 28, -155, 0, -31, -5},
        {0, 26, -158, 0, -33, -5}, {0, 23, -151, 0, -33, -8}, {0, 24, -158, 0, -36, -8}, {0, 19, -162, 0, -36, -6},
        {0, 23, -166, 0, -41, -5}, {3, 21, -163, 0, -41, -6}, {9, 16, -150, 0, -44, -5}, {24, 9, -133, 0, -49, 0},
        {27, 13, -135, 0, -46, 0}, {28, 13, -125, -1, -44, 0}, {23, 10, -108, -3, -41, 0}, {16, 3, -58, -9, -25, 0},
        {4, 14, -28, -8, -3, 0}, {0, 0, -7, -17, 0, 0}}},
      {"pressed down", 1, -1.0,
       {{0, 0, 25, -8, 2, 0}, {7, -3, 28, 0, 0, 0}, {26, -18, 76, -12, 0, -3}, {50, -22, 107, -5, 0, 0},
        {77, -27, 185, -28, 38, 0}, {53, -54, 114, 0, 9, 0}, {57, -72, 121, 0, 0, 0}, {67, -72, 131, 0, 0, 0},
        {87, -63, 165, -2, 20, 0}, {74, -70, 132, 0, 0, 0}, {79, -66, 169, 0, 12, 0}, {64, -68, 151, 1, 4, 0},
        {61, -62, 148, 0, 7, 0}, {59, -67, 143, 1, 4, 0}, {63, -73, 155, 0, 12, 0}, {66, -72, 158, 0, 12, 0},
        {72, -65, 174, 0, 23, 0}, {70, -60, 166, 0, 18, 0}, {69, -59, 166, 0, 15, 0}, {64, -55, 168, 3, 12, 0},
        {61, -60, 175, 0, 20, 0}, {64, -61, 183, 0, 28, 0}, {61, -62, 172, 0, 25, 0}, {60, -61, 169, 0, 20, 0},
        {59, -63, 174, 0, 23, 0}, {59, -60, 175, 0, 18, 0}, {59, -56, 175, 1, 20, 0}, {55, -55, 172, 1, 15, 0},
        {50, -54, 170, 1, 15, 0}, {44, -46, 166, 4, 10, 0}, {48, -43, 169, 3, 12, 0}, {44, -39, 153, 1, 10, 0},
        {41, -39, 152, 0, 15, 0}, {35, -36, 135, 0, 17, 0}, {37, -37, 151, 0, 23, 0}, {25, -26, 100, 0, 25, 0},
        {26, -27, 101, 0, 14, 0}, {27, -10, 86, 0, 25, 0}, {19, -16, 48, 0, 14, 0}, {4, -5, 34, 0, 8, 0}}},
      {"pushed away", 2, -1.0,
       {{2, 0, -22, 0, -3, 0}, {6, 0, -29, -2, -8, 0}, {15, -28, -33, -19, -11, 0}, {33, -65, -18, -27, -19, 0},
        {57, -140, -34, -61, -44, -6}, {61, -141, -44, -68, -47, -13}, {70, -159, -47, -78, -52, -20},
        {71, -145, -58, -81, -52, -8}, {75, -141, -65, -87, -58, -8}, {75, -141, -67, -90, -58, -11},
        {86, -139, -57, -90, -63, -8}, {108, -144, -45, -86, -76, -6}, {120, -141, -25, -77, -81, -3},
        {137, -141, -7, -73, -83, -1}, {148, -162, 0, -82, -73, 0}, {143, -158, 0, -84, -76, 0},
        {147, -150, 0, -82, -67, 0}, {146, -151, 0, -80, -59, 0}, {148, -157, 4, -88, -57, -6},
        {129, -144, 13, -89, -38, -15}, {129, -153, 8, -78, -41, -35}, {128, -156, 10, -73, -38, -31},
        {144, -189, 0, -90, -46, -49}, {141, -173, 0, -90, -46, -48}, {136, -148, 0, -92, -44, -40},
        {60, -76, 0, -59, -19, -20}}},
      {"pulled closer", 2, 1.0,
       {{-21, 51, 0, 7, 5, 0}, {-49, 88, -10, 35, 11, -8}, {-60, 93, -6, 49, 14, -13}, {-58, 96, 0, 39, 27, -31},
        {-48, 84, 0, 53, 1, -18}, {-49, 98, 0, 52, 14, -21}, {-52, 105, 0, 51, 17, -16}, {-63, 110, 0, 59, 19, -23},
        {-51, 109, 0, 55, 19, -18}, {-48, 109, 0, 51, 17, -21}, {-55, 109, 0, 48, 22, -21}, {-60, 104, 0, 48, 22, -21},
        {-69, 107, 0, 51, 22, -21}, {-79, 111, -3, 63, 22, -23}, {-70, 108, -4, 62, 19, -18},
        {-69, 103, 0, 62, 14, -13}, {-76, 111, 0, 60, 22, -18}, {-67, 136, 0, 62, 25, -21}, {-62, 121, 0, 54, 22, -16},
        {-66, 120, 0, 56, 25, -10}, {-51, 114, 7, 49, 25, -1}, {-57, 123, 5, 46, 25, -5}, {-53, 124, 0, 39, 27, -5},
        {-52, 124, 0, 40, 25, -8}, {-54, 126, 0, 40, 30, -10}, {-53, 125, 0, 37, 30, -11}, {-49, 127, 0, 42, 27, -10},
        {-44, 128, 0, 39, 27, -6}, {-32, 118, 8, 34, 25, -8}, {-10, 90, 2, 23, 16, -8}, {-30, 73, 0, 21, 14, -1}}},
      {"twisted clockwise", 4, -1.0,
       {{14, 0, -12, 0, -5, 44}, {18, -15, -17, -15, -8, 68}, {21, -1, -12, -16, -6, 66}, {17, 0, -6, -9, -8, 76},
        {13, -4, 0, -7, -6, 94}, {10, -7, 0, -15, 0, 116}, {26, -35, 0, -27, 0, 159}, {30, -22, -4, -38, 0, 170},
        {28, -27, -18, -29, -11, 162}, {26, -28, -22, -33, -9, 160}, {29, -30, -4, -22, -30, 160},
        {37, -32, 0, -13, -51, 162}, {49, -29, 0, -19, -45, 165}, {67, -39, -10, -20, -56, 160},
        {75, -39, -5, -27, -54, 159}, {72, -38, 0, -26, -51, 165}, {71, -39, 0, -17, -67, 174},
        {64, -29, 0, -11, -72, 180}, {66, -21, -7, -9, -75, 172}, {85, -39, -13, -19, -75, 157},
        {78, -17, -5, -26, -67, 170}, {85, -38, -7, -20, -67, 202}, {84, -31, 0, -26, -67, 218},
        {89, -27, -2, -28, -70, 210}, {83, -15, -5, -23, -61, 217}, {79, -17, -17, -20, -67, 212},
        {82, -26, -24, -14, -72, 203}, {90, -33, -22, -13, -85, 205}, {103, -51, -19, -18, -93, 203},
        {111, -54, -13, -17, -101, 207}, {113, -52, -14, -15, -104, 205}, {118, -48, -13, -8, -117, 203},
        {115, -47, -10, -3, -120, 208}, {115, -44, -5, -2, -117, 208}, {117, -47, -2, -6, -109, 210},
        {113, -43, -2, -5, -112, 215}, {113, -44, 0, -5, -112, 215}, {109, -47, 0, -2, -117, 220},
        {108, -43, 0, 0, -114, 223}, {48, -54, 0, -43, -19, 136}, {13, -29, 0, -20, -11, 54}}},
      {"twisted counter-clockwise", 4, 1.0,
       {{0, 2, -26, 13, 0, -11}, {13, -16, 0, -7, -5, -61}, {7, -21, 0, -15, -3, -76}, {9, -35, 0, -25, 0, -86},
        {9, -27, 0, -24, 0, -96}, {13, -42, 0, -34, 0, -111}, {33, -49, 0, -41, -1, -126}, {48, -71, 0, -53, -6, -111},
        {55, -67, 0, -69, 0, -116}, {45, -55, 0, -67, -4, -129}, {29, -37, 0, -57, 0, -116},
        {33, -52, 0, -53, 0, -111}, {47, -65, 18, -62, 0, -102}, {45, -62, 18, -52, 0, -111},
        {27, -68, 19, -45, 0, -126}, {20, -28, 51, -19, 0, -127}, {29, -19, 42, -27, 2, -165},
        {7, -14, 20, -7, 0, -150}, {0, -6, 20, -3, 0, -134}, {0, 0, 18, -5, 2, -122}, {0, 0, 12, -5, 8, -121},
        {0, -11, 18, -15, 16, -124}, {0, 0, 14, -15, 10, -112}, {0, 0, 4, -2, 2, -122}, {0, 0, 3, 0, 5, -136},
        {0, 0, 0, -5, 2, -132}, {0, 0, 0, -11, 8, -124}, {0, 0, 0, 0, 5, -114}, {0, 6, 2, -3, 16, -111},
        {0, 19, 6, -18, 26, -121}, {0, 9, 0, -10, 18, -116}, {0, 8, 0, -5, 21, -111}, {0, 0, 8, -18, 21, -111},
        {0, 0, 13, -15, 16, -119}, {0, 0, 6, -18, 15, -136}, {2, 7, 0, 0, 0, -122}, {3, 7, 0, -1, 2, -132},
        {0, 0, 0, -9, 0, -139}, {0, 0, 0, -3, 0, -129}, {0, 6, 3, -11, 2, -126}, {0, 15, 6, -17, 13, -137},
        {0, 7, 4, -17, 13, -129}, {0, 7, 0, -20, 7, -142}, {0, 8, 0, -26, 13, -139}, {0, 25, 14, -23, 13, -142},
        {0, 19, 12, -26, 13, -141}, {1, 22, 6, -26, 7, -149}, {0, 17, 7, -27, 15, -142}, {0, 12, 8, -30, 15, -144},
        {0, 19, 15, -30, 15, -141}, {8, 21, 30, -44, 23, -144}, {4, 8, 20, -50, 28, -150}, {11, 11, 22, -46, 21, -150},
        {0, 11, 18, -40, 15, -162}, {0, 3, 0, -32, 13, -147}, {0, 0, -10, -3, 0, -48}}},
      {"tilted back", 3, 1.0,
       {{7, -13, 0, 29, 0, -16}, {0, -1, 0, 65, 14, -18}, {7, -8, 0, 110, 28, -20}, {0, 0, -7, 160, 44, -35},
        {0, 3, -5, 174, 47, -36}, {0, 0, 0, 158, 47, -35}, {-25, 4, -13, 168, 58, -41}, {6, 0, 0, 253, 62, -43},
        {0, 0, 0, 234, 61, -31}, {-16, 13, -2, 243, 85, -38}, {-11, 27, -21, 287, 102, -48}, {0, 0, -7, 259, 72, -25},
        {-56, 28, -12, 251, 109, -53}, {0, 5, 0, 266, 88, -41}, {-25, 9, 0, 207, 77, -46}, {-16, 8, 0, 213, 82, -46},
        {0, 0, 0, 21, 13, -1}}},
      {"tilted forward", 3, -1.0,
       {{0, -29, 0, -47, -11, 0}, {0, -29, -3, -117, -26, 0}, {12, -29, -6, -141, -50, 0}, {17, -38, -5, -151, -66, 0},
        {23, -41, 0, -154, -66, 0}, {16, -43, 0, -165, -63, 0}, {14, -44, 0, -187, -69, 0}, {7, -43, 0, -208, -69, 0},
        {27, -59, 8, -215, -80, 0}, {22, -59, 0, -224, -80, 1}, {26, -64, 21, -256, -91, 0},
        {53, -73, 12, -289, -126, 10}, {26, -66, 10, -298, -110, 6}, {5, -52, 0, -319, -100, 0},
        {43, -74, 19, -310, -110, 0}, {41, -70, 7, -324, -108, 0}, {20, -50, 0, -341, -105, 6},
        {29, -49, 15, -327, -102, 0}, {9, -10, 11, -216, -77, 0}}},
      {"rolled left", 5, 1.0,
       {{0, 0, 21, 0, 18, 0}, {4, -8, 3, -16, 29, 0}, {0, -32, 15, -40, 90, 0}, {0, -33, 47, -42, 127, 11},
        {0, -45, 51, -58, 161, 15}, {0, -40, 50, -68, 190, 23}, {0, -33, 61, -63, 193, 25}, {0, -37, 59, -78, 214, 30},
        {0, -37, 67, -90, 219, 25}, {4, -37, 47, -89, 206, 25}, {5, -33, 62, -73, 212, 26}, {8, -34, 56, -88, 217, 28},
        {7, -32, 61, -102, 230, 25}, {5, -32, 48, -118, 243, 25}, {0, -28, 48, -126, 245, 28},
        {0, -20, 59, -137, 243, 8}, {0, -25, 54, -128, 243, 10}, {0, -16, 47, -129, 245, 8},
        {0, -14, 39, -129, 245, 15}, {-1, -21, 32, -127, 248, 18}, {0, -36, 34, -149, 253, 18},
        {-2, -29, 36, -165, 266, 15}, {-4, -6, 39, -130, 253, 20}, {0, 0, 0, -28, 29, 0}}},
      {"rolled right", 5, -1.0,
       {{0, 19, -15, 19, -24, 0}, {1, 0, -19, 23, -37, -1}, {31, -22, -28, 19, -85, -3}, {50, -11, -49, 51, -143, -6},
        {35, -12, -44, 66, -153, -8}, {44, 0, -18, 69, -153, -21}, {76, 0, -61, 88, -198, -33},
        {104, -30, -49, 91, -238, -31}, {115, -41, -47, 74, -235, -28}, {120, -31, -42, 116, -272, -26},
        {98, -23, -30, 114, -240, -40}, {105, -29, -43, 102, -235, -73}, {121, -23, -50, 116, -261, -81},
        {112, -12, -41, 136, -269, -71}, {101, -19, -18, 127, -253, -66}, {118, -33, -17, 117, -258, -69},
        {123, -42, -14, 117, -264, -78}, {123, -42, -5, 116, -261, -81}, {114, -61, -11, 98, -245, -76},
        {111, -52, 0, 119, -256, -66}, {103, -53, 0, 134, -250, -86}, {106, -27, 0, 162, -266, -93},
        {76, -36, 0, 133, -207, -91}, {49, -13, 4, 160, -212, -83}, {72, -50, -11, 151, -218, -93},
        {53, -13, 22, 150, -194, -89}, {45, -3, 0, 145, -168, -86}, {31, 0, 0, 154, -157, -102},
        {28, -6, 0, 156, -165, -102}, {30, 0, 2, 152, -151, -88}, {37, 0, 0, 160, -178, -64},
        {48, 0, 0, 141, -154, -76}, {22, 0, 0, 124, -136, -74}, {24, -25, 13, 70, -89, -58},
        {11, -5, 0, 0, -13, -28}}},
  };
  return motions;
}

mitcad::SpaceMouseAxes loggedAxes(const std::array<int, 6>& report) {
  return mitcad::spaceMouseFromHid({report[0], report[1], report[2]}, {report[3], report[4], report[5]});
}

// Each logged motion through `step` (the device's axes in, the rates out),
// with a rest after each: true when every motion moved the view along its
// own axis only, its way, in most of its reports, and nothing at rest.
template <typename Step>
bool followsEachMotion(const std::vector<LoggedMotion>& motions, Step&& step) {
  bool all = true;
  for (const LoggedMotion& motion : motions) {
    std::size_t moved = 0;
    std::size_t stray = 0;
    for (const std::array<int, 6>& report : motion.reports) {
      const mitcad::SpaceMouseRates rates = step(loggedAxes(report));
      for (std::size_t axis = 0; axis < 6; ++axis) {
        const double rate = rateOf(rates, axis);
        if (axis == motion.axis && rate * motion.sign > 0.0) {
          ++moved;
        } else if (rate != 0.0) {
          ++stray;
        }
      }
    }
    const bool rested = !step(mitcad::SpaceMouseAxes{}).moving();
    if (stray > 0 || !rested || moved * 2 <= motion.reports.size()) {
      std::fprintf(stderr,
                   "space_mouse.cpp: the logged motion \"%s\": %d report(s) moved its axis, %d stray rate(s)%s\n",
                   motion.name, static_cast<int>(moved), static_cast<int>(stray), rested ? "" : ", moving at rest");
      all = false;
    }
  }
  return all;
}

// Dominant axis only with its hold (mitcad#1720, GitHub mitcad/Mitcad#343):
// the SpaceNavigator's cap couples its axes heavily (pushed left it
// reported up to 1.34 times as much in and out as left; twisted, a push to
// the right of half the twist). With Dominant axis on, each of the six
// motions, both ways, moves the view along its own axis only, through the
// mapping's memory as through the controller; chosen afresh at every
// frame, as before, the push to the left zoomed too. The axis is chosen
// once the cap is deflected clearly, a stronger axis takes over only past
// the margin, the hold ends at rest, and the axes compare as shares of the
// device's reach.
void dominantHoldTests() {
  using namespace mitcad;
  const std::vector<LoggedMotion>& motions = loggedSpaceNavigatorMotions();
  CHECK(motions.size() == 12);
  SpaceMouseSettings settings;
  settings.dominantAxis = true;
  SpaceMouseMemory memory;
  const auto withMemory = [&settings, &memory](const SpaceMouseAxes& axes) {
    return spaceMouseRates(settings, axes, false, &memory);
  };
  CHECK(followsEachMotion(motions, withMemory));
  // The device's reach as it showed it: its translations about half its
  // rotations.
  CHECK(memory.reach.of(0) > 0.5 && memory.reach.of(0) < 0.6 && memory.reach.of(3) > 0.9);

  // The same through the controller, which keeps the memory itself.
  {
    SpaceMouseRates last;
    SpaceMouseController::Hooks hooks;
    hooks.settings = [&settings] { return settings; };
    hooks.flat = [] { return false; };
    hooks.rates = [&last](const SpaceMouseRates& rates) { last = rates; };
    const auto factory = []() -> std::unique_ptr<SpaceMouseBackend> { return std::make_unique<FakeBackend>(false); };
    SpaceMouseController controller(std::move(hooks), factory);
    const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
    CHECK(followsEachMotion(motions, [&controller, &last](const SpaceMouseAxes& axes) {
      controller.motion(axes, false);
      return last;
    }));
    qInstallMessageHandler(previous);
  }

  // Chosen afresh at every frame (no memory), the push to the left zoomed
  // where the cap reported more in and out than left.
  bool zoomed = false;
  for (const std::array<int, 6>& report : motions[1].reports) {
    zoomed = zoomed || spaceMouseRates(settings, loggedAxes(report), false).zoom != 0.0;
  }
  CHECK(zoomed);

  // The hold on its own.
  SpaceMouseMemory held;
  const auto step = [&settings, &held](const SpaceMouseAxes& axes) {
    return spaceMouseRates(settings, axes, false, &held);
  };
  // Past the dead zone but not yet clearly deflected: nothing chosen.
  CHECK(!step({0.07, 0.0, 0.05, 0.0, 0.0, 0.0}).moving() && held.dominant.held() == -1);
  SpaceMouseRates rates = step({0.3, 0.0, -0.1, 0.0, 0.0, 0.0});
  CHECK(rates.panX > 0.0 && rates.zoom == 0.0 && held.dominant.held() == 0);
  // Another axis 1.33 times as far keeps the held one; 1.67 times takes
  // over.
  rates = step({0.3, 0.0, -0.4, 0.0, 0.0, 0.0});
  CHECK(rates.panX > 0.0 && rates.zoom == 0.0);
  rates = step({0.3, 0.0, -0.5, 0.0, 0.0, 0.0});
  CHECK(rates.zoom < 0.0 && rates.panX == 0.0 && held.dominant.held() == 2);
  // Coming to rest: the held axis back in the dead zone and another still
  // short of a clear deflection move nothing; at rest the hold ends.
  CHECK(!step({0.0, 0.0, -0.03, 0.0, 0.08, 0.0}).moving() && held.dominant.held() == 2);
  CHECK(!step(SpaceMouseAxes{}).moving() && held.dominant.held() == -1);
  rates = step({0.0, 0.0, 0.0, 0.0, 0.3, 0.0});
  CHECK(rates.yaw > 0.0 && rates.panX == 0.0 && rates.zoom == 0.0 && held.dominant.held() == 4);
  // A held axis switched off meanwhile gives way to the strongest one on.
  settings.axisOn[4] = false;
  rates = step({0.2, 0.0, 0.0, 0.0, 0.3, 0.0});
  CHECK(rates.panX > 0.0 && rates.yaw == 0.0 && held.dominant.held() == 0);
  settings.axisOn[4] = true;
  step(SpaceMouseAxes{});
  // Off, Dominant axis only holds nothing.
  settings.dominantAxis = false;
  step({0.3, 0.0, 0.0, 0.0, 0.0, 0.0});
  CHECK(held.dominant.held() == -1);
  settings.dominantAxis = true;
  step(SpaceMouseAxes{});

  // The reach: the nominal one learns nothing; a device's grows from its
  // start with every deflection beyond it, a full one at most, and starts
  // again when reset.
  SpaceMouseReach nominal;
  nominal.add({1.7, 0.2, 0.0, 0.0, 0.0, 0.0});
  CHECK(nominal.of(0) == 1.0 && nominal.of(5) == 1.0 && nearly(nominal.share(1, 0.2), 0.2));
  SpaceMouseReach learning(kSpaceMouseReachStart);
  CHECK(learning.of(2) == kSpaceMouseReachStart && learning.of(4) == kSpaceMouseReachStart);
  learning.add({0.3, -0.62, 0.0, std::nan(""), 0.0, -1.4});
  CHECK(nearly(learning.of(1), 0.62) && learning.of(3) == 1.0 && nearly(learning.share(1, 0.31), 0.5));
  learning.reset();
  CHECK(learning.of(0) == kSpaceMouseReachStart && learning.of(5) == kSpaceMouseReachStart);
  // Dominant axis only compares shares of the reach: a device that has
  // shown translations up to 0.5 and rotations up to a full deflection has
  // a push of 0.4 (four fifths of its reach) win over a twist of 0.6; on
  // the nominal scale the twist wins.
  SpaceMouseMemory device;
  spaceMouseRates(settings, {0.5, 0.0, 0.0, 0.0, 0.0, 0.0}, false, &device);
  spaceMouseRates(settings, SpaceMouseAxes{}, false, &device);
  spaceMouseRates(settings, {0.0, 0.0, 0.0, 0.0, 1.0, 0.0}, false, &device);
  spaceMouseRates(settings, SpaceMouseAxes{}, false, &device);
  CHECK(nearly(device.reach.of(0), 0.5) && nearly(device.reach.of(4), 1.0));
  rates = spaceMouseRates(settings, {0.4, 0.0, 0.0, 0.0, -0.6, 0.0}, false, &device);
  CHECK(rates.panX > 0.0 && rates.yaw == 0.0);
  rates = spaceMouseRates(settings, {0.4, 0.0, 0.0, 0.0, -0.6, 0.0}, false);
  CHECK(rates.yaw < 0.0 && rates.panX == 0.0);
}

void moveTests() {
  using namespace mitcad;
  // Looking along +Y at the origin from 100 mm, Z up: right is +X.
  SpaceMousePose pose;
  pose.eye = {0.0, -100.0, 0.0};
  pose.target = {0.0, 0.0, 0.0};
  pose.up = {0.0, 0.0, 1.0};
  pose.height = 50.0;
  const SpaceMouseVec pivot{0.0, 0.0, 0.0};
  const SpaceMouseStyle objectMode{};
  SpaceMouseStyle horizonLocked;
  horizonLocked.lockHorizon = true;

  // Pushed right for 0.5 s at 1 view height per second: the model moves
  // right, the camera 25 mm to the left.
  SpaceMouseRates rates;
  rates.panX = 1.0;
  SpaceMousePose moved = spaceMouseMove(pose, pivot, rates, 0.5, objectMode);
  CHECK(nearly(moved.target[0], -25.0) && nearly(moved.eye[0], -25.0) && nearly(moved.target[2], 0.0));
  CHECK(nearly(moved.height, 50.0));
  // The pan follows the visible height, the zoom the scale: in a view
  // taller than wide the two differ, and the height stays as it was.
  SpaceMousePose tall = pose;
  tall.panHeight = 80.0;
  moved = spaceMouseMove(tall, pivot, rates, 0.5, objectMode);
  CHECK(nearly(moved.target[0], -40.0) && nearly(moved.height, 50.0) && nearly(moved.panHeight, 80.0));
  // Lifted: the model moves up, the camera down.
  rates = SpaceMouseRates();
  rates.panY = 1.0;
  moved = spaceMouseMove(pose, pivot, rates, 0.5, objectMode);
  CHECK(nearly(moved.target[2], -25.0));
  // Pulled: closer, a smaller view height.
  rates = SpaceMouseRates();
  rates.zoom = 1.0;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, objectMode);
  CHECK(nearly(moved.height, 50.0 * std::exp(-1.0)));
  // Spun counter-clockwise (seen from above) by a quarter turn: the
  // model's front turns to the right (+X), the camera to -X, looking
  // along +X.
  const double quarter = std::acos(0.0);
  rates = SpaceMouseRates();
  rates.yaw = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, objectMode);
  CHECK(nearly(moved.eye[0], -100.0, 1e-6) && nearly(moved.eye[1], 0.0, 1e-6) && nearly(moved.eye[2], 0.0, 1e-6));
  CHECK(nearly(moved.up[2], 1.0, 1e-9));
  // Lock horizon turns the same way about Z.
  SpaceMousePose upright = spaceMouseMove(pose, pivot, rates, 1.0, horizonLocked);
  CHECK(nearly(upright.eye[0], -100.0, 1e-6) && nearly(upright.up[2], 1.0, 1e-9));
  // Tilted toward the user by a quarter turn: the model's top faces the
  // camera, which looks down from +Z with +Y up.
  rates = SpaceMouseRates();
  rates.pitch = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, objectMode);
  CHECK(nearly(moved.eye[2], 100.0, 1e-6) && nearly(moved.up[1], 1.0, 1e-9));
  // Lock horizon stops short of the pole, but goes half way.
  CHECK(spaceMouseMove(pose, pivot, rates, 1.0, horizonLocked).eye == pose.eye);
  moved = spaceMouseMove(pose, pivot, rates, 0.5, horizonLocked);
  CHECK(nearly(moved.eye[2], 100.0 * std::sqrt(0.5), 1e-6) && nearly(moved.eye[1], -100.0 * std::sqrt(0.5), 1e-6));
  // Rolled counter-clockwise (as the user sees it): the model's up points
  // to the left of the view, the camera's up along +X. Not with Lock
  // horizon.
  rates = SpaceMouseRates();
  rates.roll = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, objectMode);
  CHECK(nearly(moved.up[0], 1.0, 1e-9) && nearly(moved.eye[1], -100.0, 1e-9) && nearly(moved.eye[0], 0.0, 1e-9));
  CHECK(nearly(unit(minus(moved.target, moved.eye))[1], 1.0, 1e-9));
  upright = spaceMouseMove(pose, pivot, rates, 1.0, horizonLocked);
  CHECK(upright.eye == pose.eye && nearly(upright.up[2], 1.0, 1e-12));
  // About an orbit centre off the target: the target swings round it.
  rates = SpaceMouseRates();
  rates.yaw = 2.0 * quarter;
  moved = spaceMouseMove(pose, SpaceMouseVec{10.0, 0.0, 0.0}, rates, 1.0, objectMode);
  CHECK(nearly(moved.target[0], 20.0, 1e-6) && nearly(moved.target[1], 0.0, 1e-6));
  // A pose that makes no view, or a time that is no number, stays.
  SpaceMousePose broken = pose;
  broken.eye = broken.target;
  CHECK(spaceMouseMove(broken, pivot, rates, 1.0, objectMode).eye == broken.eye);
  CHECK(spaceMouseMove(pose, pivot, rates, std::nan(""), objectMode).eye == pose.eye);
}

// The Navigation modes and the 3D mouse's Lock horizon (mitcad#978), from
// the device's axes through spaceMouseRates to spaceMouseMove, as the
// controller and OcctViewer take them.
SpaceMouseVec cross(const SpaceMouseVec& a, const SpaceMouseVec& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
bool nearlySame(const SpaceMouseVec& a, const SpaceMouseVec& b, double tolerance = 1e-9) {
  return nearly(a[0], b[0], tolerance) && nearly(a[1], b[1], tolerance) && nearly(a[2], b[2], tolerance);
}
bool samePose(const mitcad::SpaceMousePose& a, const mitcad::SpaceMousePose& b, double tolerance = 1e-9) {
  return nearlySame(a.eye, b.eye, tolerance) && nearlySame(a.target, b.target, tolerance) &&
         nearlySame(unit(a.up), unit(b.up), tolerance) && nearly(a.height, b.height, tolerance);
}
mitcad::SpaceMouseStyle styled(mitcad::SpaceMouseMode mode, bool lockHorizon = false, bool perspective = true) {
  mitcad::SpaceMouseStyle style;
  style.mode = mode;
  style.lockHorizon = lockHorizon;
  style.perspective = perspective;
  return style;
}
// The camera's right is level: no roll against world Z, its up above it.
bool level(const mitcad::SpaceMousePose& pose) {
  const SpaceMouseVec direction = unit(minus(pose.target, pose.eye));
  const SpaceMouseVec right = cross(direction, unit(pose.up));
  return std::abs(right[2]) < 1e-9 && unit(pose.up)[2] > 0.0;
}

void modeTests() {
  using namespace mitcad;
  using Mode = SpaceMouseMode;
  // Looking along +Y at the origin from 100 mm, Z up: right is +X. The
  // orbit's centre is the target.
  SpaceMousePose pose;
  pose.eye = {0.0, -100.0, 0.0};
  pose.target = {0.0, 0.0, 0.0};
  pose.up = {0.0, 0.0, 1.0};
  pose.height = 50.0;
  const SpaceMouseVec pivot{0.0, 0.0, 0.0};
  // The defaults without the cross-talk filter (mitcad#1720): the motions
  // along several axes at once below move along every one of them.
  SpaceMouseSettings defaults;
  defaults.crossTalk = 0;
  const Mode modes[3] = {Mode::Object, Mode::Camera, Mode::Fly};
  const double quarter = std::acos(0.0);
  const double nearPole = std::cos(3.14159265358979323846 / 180.0);

  // The same device input moves the camera opposite ways in Object and
  // Camera: the one undoes the other, axis by axis and for a turn about
  // all three axes at once.
  for (std::size_t axis = 0; axis < 7; ++axis) {
    SpaceMouseAxes axes{};
    if (axis < 6) {
      axes[axis] = 0.7;
    } else {
      axes = {0.0, 0.0, 0.0, 0.5, -0.7, 0.3};
    }
    const SpaceMouseRates rates = spaceMouseRates(defaults, axes, false);
    CHECK(rates.moving());
    const SpaceMousePose object = spaceMouseMove(pose, pivot, rates, 0.3, styled(Mode::Object));
    const SpaceMousePose camera = spaceMouseMove(pose, pivot, rates, 0.3, styled(Mode::Camera));
    CHECK(!samePose(object, pose) && !samePose(camera, pose) && !samePose(object, camera));
    CHECK(samePose(spaceMouseMove(object, pivot, rates, 0.3, styled(Mode::Camera)), pose));
    CHECK(samePose(spaceMouseMove(camera, pivot, rates, 0.3, styled(Mode::Object)), pose));
  }
  // Camera: pushed right, the camera moves right (the model left); pushed
  // away, it comes closer; twisted counter-clockwise (seen from above) by
  // a quarter turn, it turns that way about the orbit's centre, from -Y
  // round to +X, looking along -X.
  SpaceMouseRates rates;
  rates.panX = 1.0;
  SpaceMousePose moved = spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Camera));
  CHECK(nearly(moved.target[0], 25.0) && nearly(moved.eye[0], 25.0));
  moved = spaceMouseMove(pose, pivot, spaceMouseRates(defaults, {0.0, 0.0, -1.0, 0.0, 0.0, 0.0}, false), 0.5,
                         styled(Mode::Camera));
  CHECK(moved.height < pose.height && nearlySame(moved.eye, pose.eye));
  moved = spaceMouseMove(pose, pivot, spaceMouseRates(defaults, {0.0, 0.0, -1.0, 0.0, 0.0, 0.0}, false), 0.5,
                         styled(Mode::Object));
  CHECK(moved.height > pose.height);
  rates = SpaceMouseRates();
  rates.yaw = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, styled(Mode::Camera));
  CHECK(nearlySame(moved.eye, {100.0, 0.0, 0.0}, 1e-6) && nearlySame(moved.target, pivot, 1e-6));
  CHECK(nearlySame(unit(moved.up), {0.0, 0.0, 1.0}));

  // Fly turns about the eye: the eye stays, the target moves. Twisted
  // counter-clockwise it looks to the left (along -X), tilted back it
  // looks up; the distance to the target stays.
  moved = spaceMouseMove(pose, pivot, rates, 1.0, styled(Mode::Fly));
  CHECK(nearlySame(moved.eye, pose.eye) && nearlySame(moved.target, {-100.0, -100.0, 0.0}, 1e-6));
  rates = SpaceMouseRates();
  rates.pitch = quarter / 2.0;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, styled(Mode::Fly));
  CHECK(nearlySame(moved.eye, pose.eye) && moved.target[2] > 0.0);
  CHECK(nearly(std::sqrt(dot(minus(moved.target, moved.eye), minus(moved.target, moved.eye))), 100.0, 1e-9));
  // Turned about all three axes for many frames: the eye never moves.
  rates = spaceMouseRates(defaults, {0.0, 0.0, 0.0, 0.4, 0.9, -0.5}, false);
  moved = pose;
  for (int frame = 0; frame < 240; ++frame) {
    moved = spaceMouseMove(moved, pivot, rates, 1.0 / 60.0, styled(Mode::Fly));
  }
  CHECK(nearlySame(moved.eye, pose.eye) && !nearlySame(moved.target, pose.target, 1.0));
  // Its pan and zoom are Camera's.
  rates = spaceMouseRates(defaults, {0.6, -0.4, 0.8, 0.0, 0.0, 0.0}, false);
  CHECK(samePose(spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Fly)),
                 spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Camera))));
  // In orthographic Fly is Camera, turns and all.
  rates = spaceMouseRates(defaults, {0.6, -0.4, 0.8, 0.4, 0.9, -0.5}, false);
  CHECK(samePose(spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Fly, false, false)),
                 spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Camera, false, false))));
  CHECK(!samePose(spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Fly, false, true)),
                  spaceMouseMove(pose, pivot, rates, 0.5, styled(Mode::Camera, false, true))));

  // Lock horizon, in every mode (the mouse's Constrained orbit plays no
  // part: spaceMouseMove does not know it): over many frames of every
  // axis at once the camera's up stays on world Z and it never looks
  // straight along Z; the roll axis alone does nothing. Without the lock
  // the same input tilts the horizon.
  const SpaceMouseRates everything = spaceMouseRates(defaults, {0.3, -0.2, 0.4, 0.8, 0.6, 0.9}, false);
  SpaceMouseRates roll;
  roll.roll = kSpaceMouseTurnRate;
  for (const Mode mode : modes) {
    for (const bool perspective : {true, false}) {
      SpaceMousePose locked = pose;
      bool alwaysLevel = true;
      bool offPole = true;
      for (int frame = 0; frame < 600; ++frame) {
        locked = spaceMouseMove(locked, pivot, everything, 1.0 / 60.0, styled(mode, true, perspective));
        alwaysLevel = alwaysLevel && level(locked);
        offPole = offPole && std::abs(unit(minus(locked.target, locked.eye))[2]) <= nearPole + 1e-9;
      }
      CHECK(alwaysLevel && offPole && !samePose(locked, pose, 1.0));
      CHECK(samePose(spaceMouseMove(pose, pivot, roll, 1.0, styled(mode, true, perspective)), pose));
      SpaceMousePose loose = pose;
      for (int frame = 0; frame < 60; ++frame) {
        loose = spaceMouseMove(loose, pivot, everything, 1.0 / 60.0, styled(mode, false, perspective));
      }
      CHECK(!level(loose));
      CHECK(!samePose(spaceMouseMove(pose, pivot, roll, 1.0, styled(mode, false, perspective)), pose));
    }
  }
  // Fly with Lock horizon still turns about the eye.
  rates = spaceMouseRates(defaults, {0.0, 0.0, 0.0, 0.5, 0.8, 0.9}, false);
  moved = pose;
  for (int frame = 0; frame < 120; ++frame) {
    moved = spaceMouseMove(moved, pivot, rates, 1.0 / 60.0, styled(Mode::Fly, true));
  }
  CHECK(nearlySame(moved.eye, pose.eye) && level(moved) && !nearlySame(moved.target, pose.target, 1.0));

  // Reverse and the locks apply in every mode: a reversed axis moves the
  // camera the other way than the mode does, a locked rotation never
  // turns it, a locked translation never pans or zooms it. A flat view
  // (sketch mode) pans and zooms only, in the mode's direction.
  const SpaceMouseAxes push{0.8, 0.0, 0.0, 0.0, 0.0, 0.0};
  const SpaceMouseAxes all{0.5, -0.6, 0.7, 0.8, -0.5, 0.6};
  for (const Mode mode : modes) {
    const SpaceMouseStyle style = styled(mode);
    const double way = mode == Mode::Object ? -1.0 : 1.0; // where the camera goes, pushed right
    SpaceMouseSettings settings;
    moved = spaceMouseMove(pose, pivot, spaceMouseRates(settings, push, false), 0.5, style);
    CHECK(moved.target[0] * way > 0.0);
    settings.reversed[0] = true;
    moved = spaceMouseMove(pose, pivot, spaceMouseRates(settings, push, false), 0.5, style);
    CHECK(moved.target[0] * way < 0.0);
    settings = SpaceMouseSettings();
    settings.reversed[4] = true;
    const SpaceMouseAxes spin{0.0, 0.0, 0.0, 0.0, 0.8, 0.0};
    const SpaceMousePose spun = spaceMouseMove(pose, pivot, spaceMouseRates(settings, spin, false), 0.5, style);
    settings = SpaceMouseSettings();
    const SpaceMousePose plain = spaceMouseMove(pose, pivot, spaceMouseRates(settings, spin, false), 0.5, style);
    CHECK(unit(minus(spun.target, spun.eye))[0] * unit(minus(plain.target, plain.eye))[0] < 0.0);
    settings.lockRotation = true;
    moved = spaceMouseMove(pose, pivot, spaceMouseRates(settings, all, false), 0.5, style);
    CHECK(nearlySame(unit(minus(moved.target, moved.eye)), {0.0, 1.0, 0.0}) && moved.height != pose.height &&
          !nearlySame(moved.target, pose.target));
    settings = SpaceMouseSettings();
    settings.lockTranslation = true;
    moved = spaceMouseMove(pose, pivot, spaceMouseRates(settings, all, false), 0.5, style);
    CHECK(nearly(moved.height, pose.height) &&
          nearlySame(mode == Mode::Fly ? moved.eye : moved.target, mode == Mode::Fly ? pose.eye : pose.target) &&
          !nearlySame(unit(minus(moved.target, moved.eye)), {0.0, 1.0, 0.0}));
    settings = SpaceMouseSettings();
    const SpaceMouseRates flat = spaceMouseRates(settings, all, true);
    moved = spaceMouseMove(pose, pivot, flat, 0.5, style);
    CHECK(nearlySame(unit(minus(moved.target, moved.eye)), {0.0, 1.0, 0.0}) &&
          nearlySame(unit(moved.up), {0.0, 0.0, 1.0}));
    CHECK(moved.target[0] * way > 0.0 && moved.target[2] * way < 0.0);
    CHECK(mode == Mode::Object ? moved.height < pose.height : moved.height > pose.height);
    CHECK(samePose(spaceMouseMove(pose, pivot, flat, 0.5, styled(mode, true)), moved));
  }

  // On OCCT's camera: Fly in perspective keeps the eye where it is, in
  // orthographic it moves the camera as Camera does.
  for (const bool perspective : {true, false}) {
    occ::handle<Graphic3d_Camera> fly = new Graphic3d_Camera();
    occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
    for (const occ::handle<Graphic3d_Camera>& each : {fly, camera}) {
      each->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                          : Graphic3d_Camera::Projection_Orthographic);
      each->SetEyeAndCenter(gp_Pnt(0.0, -100.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
      each->SetUp(gp_Dir(0.0, 0.0, 1.0));
      each->SetScale(50.0);
    }
    const gp_Pnt eye = fly->Eye();
    SpaceMouseRates turn;
    turn.yaw = 1.0;
    turn.pitch = 0.3;
    for (int frame = 0; frame < 30; ++frame) {
      CHECK(moveSpaceMouseCamera(*fly, gp_Pnt(0.0, 0.0, 0.0), turn, 1.0 / 60.0, styled(Mode::Fly)));
      moveSpaceMouseCamera(*camera, gp_Pnt(0.0, 0.0, 0.0), turn, 1.0 / 60.0, styled(Mode::Camera));
    }
    if (perspective) {
      CHECK(fly->Eye().Distance(eye) < 1e-6 && fly->Center().Distance(gp_Pnt(0.0, 0.0, 0.0)) > 1.0);
    } else {
      CHECK(fly->Eye().Distance(camera->Eye()) < 1e-6 && fly->Center().Distance(camera->Center()) < 1e-6);
    }
  }
}

// The step on OCCT's camera, frame after frame, as OcctViewer applies it
// (mitcad#951: in a view taller than wide the camera's visible height is
// larger than its scale; taking the one for the other grew the view by
// the aspect at every frame, a ten-thousandfold in a second). Both aspects
// and both projections: a pan and a turn keep the scale and the visible
// height, a zoom in shrinks both by the rate, zero rates change nothing.
void cameraTests() {
  using namespace mitcad;
  const SpaceMouseStyle objectMode{};
  SpaceMouseStyle horizonLocked;
  horizonLocked.lockHorizon = true;
  for (const double aspect : {0.6, 1.6}) {
    for (const bool perspective : {false, true}) {
      occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
      camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                            : Graphic3d_Camera::Projection_Orthographic);
      camera->SetAspect(aspect);
      camera->SetEyeAndCenter(gp_Pnt(0.0, -100.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
      camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
      camera->SetScale(50.0);
      const double scale = camera->Scale();
      const double visible = camera->ViewDimensions().Y();
      CHECK(scale > 0.0 && visible > 0.0);
      const double frame = 1.0 / 60.0;
      // Pushed right for a second: the model moves one visible height to
      // the right, the camera's target as far to the left.
      SpaceMouseRates pan;
      pan.panX = 1.0;
      for (int step = 0; step < 60; ++step) {
        CHECK(moveSpaceMouseCamera(*camera, camera->Center(), pan, frame, objectMode));
      }
      CHECK(nearly(camera->Scale(), scale, 1e-9 * scale));
      CHECK(nearly(camera->ViewDimensions().Y(), visible, 1e-9 * visible));
      CHECK(nearly(camera->Center().X(), -visible, 1e-6 * visible));
      // Turned for two seconds about a pivot off the target, free and
      // with the Z lock.
      SpaceMouseRates turn;
      turn.yaw = 1.0;
      turn.pitch = 0.4;
      turn.roll = 0.3;
      for (int step = 0; step < 120; ++step) {
        moveSpaceMouseCamera(*camera, gp_Pnt(5.0, 5.0, 5.0), turn, frame, step % 2 == 0 ? horizonLocked : objectMode);
      }
      CHECK(nearly(camera->Scale(), scale, 1e-9 * scale));
      CHECK(nearly(camera->ViewDimensions().Y(), visible, 1e-9 * visible));
      // Pulled for a second at one e-fold per second: closer by e.
      SpaceMouseRates zoom;
      zoom.zoom = 1.0;
      for (int step = 0; step < 60; ++step) {
        moveSpaceMouseCamera(*camera, camera->Center(), zoom, frame, objectMode);
      }
      CHECK(nearly(camera->Scale(), scale * std::exp(-1.0), 1e-9 * scale));
      CHECK(nearly(camera->ViewDimensions().Y(), visible * std::exp(-1.0), 1e-9 * visible));
      // At rest: nothing changes.
      const gp_Pnt eye = camera->Eye();
      CHECK(!moveSpaceMouseCamera(*camera, camera->Center(), SpaceMouseRates(), frame, objectMode));
      CHECK(camera->Eye().IsEqual(eye, 0.0) && nearly(camera->Scale(), scale * std::exp(-1.0), 1e-9 * scale));
    }
  }
}

void settingsTests() {
  using namespace mitcad;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString path = directory.filePath(QStringLiteral("settings.ini"));
  {
    QSettings store(path, QSettings::IniFormat);
    SpaceMouseSettings fresh;
    loadSpaceMouseSettings(store, fresh);
    // The defaults: on, speed 1, every axis on and not reversed, Fit and
    // Home on the first two buttons.
    CHECK(fresh.enabled && fresh.speed == 1.0 && !fresh.lockRotation && !fresh.lockTranslation && !fresh.dominantAxis);
    CHECK(fresh.axisOn == (std::array<bool, 6>{true, true, true, true, true, true}));
    CHECK(fresh.reversed == (std::array<bool, 6>{}));
    CHECK(fresh.buttons == (QStringList{QStringLiteral("view.fit"), QStringLiteral("view.home")}));
    // Object mode and no Lock horizon (mitcad#978).
    CHECK(fresh.mode == SpaceMouseMode::Object && !fresh.lockHorizon);
    // The cross-talk filter at 50 % (mitcad#1720).
    CHECK(fresh.crossTalk == 50);
    SpaceMouseSettings chosen;
    chosen.crossTalk = 85;
    chosen.mode = SpaceMouseMode::Fly;
    chosen.lockHorizon = true;
    chosen.enabled = false;
    chosen.speed = 2.25;
    chosen.axisOn[2] = false;
    chosen.reversed[1] = true;
    chosen.reversed[5] = true;
    chosen.lockRotation = true;
    chosen.dominantAxis = true;
    chosen.buttons = {QString(), QStringLiteral("view.top"), QStringLiteral("view.fit")};
    saveSpaceMouseSettings(store, chosen);
  }
  QSettings store(path, QSettings::IniFormat);
  SpaceMouseSettings loaded;
  loadSpaceMouseSettings(store, loaded);
  CHECK(!loaded.enabled && nearly(loaded.speed, 2.25) && loaded.lockRotation && !loaded.lockTranslation &&
        loaded.dominantAxis);
  CHECK(loaded.axisOn == (std::array<bool, 6>{true, true, false, true, true, true}));
  CHECK(loaded.reversed == (std::array<bool, 6>{false, true, false, false, false, true}));
  CHECK(loaded.buttons == (QStringList{QString(), QStringLiteral("view.top"), QStringLiteral("view.fit")}));
  CHECK(loaded.mode == SpaceMouseMode::Fly && loaded.lockHorizon);
  CHECK(store.value(QStringLiteral("view/spaceMouseMode")).toString() == QStringLiteral("fly"));
  CHECK(loaded.crossTalk == 85 && store.value(QStringLiteral("view/spaceMouseCrossTalk")).toInt() == 85);
  // A speed out of range comes back within it, a mode of no name as
  // Object; a cross-talk share out of range within it, Off as Off, and one
  // that is no number as the default.
  store.setValue(QStringLiteral("view/spaceMouseSpeed"), 40.0);
  store.setValue(QStringLiteral("view/spaceMouseMode"), QStringLiteral("sideways"));
  store.setValue(QStringLiteral("view/spaceMouseCrossTalk"), 140);
  loadSpaceMouseSettings(store, loaded);
  CHECK(nearly(loaded.speed, 4.0) && loaded.mode == SpaceMouseMode::Object);
  CHECK(loaded.crossTalk == kSpaceMouseCrossTalkMax);
  store.setValue(QStringLiteral("view/spaceMouseCrossTalk"), 0);
  loadSpaceMouseSettings(store, loaded);
  CHECK(loaded.crossTalk == 0);
  store.setValue(QStringLiteral("view/spaceMouseCrossTalk"), QStringLiteral("much"));
  loadSpaceMouseSettings(store, loaded);
  CHECK(loaded.crossTalk == 50);
  for (const SpaceMouseMode mode : {SpaceMouseMode::Object, SpaceMouseMode::Camera, SpaceMouseMode::Fly}) {
    CHECK(spaceMouseModeFromId(QLatin1String(spaceMouseModeId(mode))) == mode);
  }

  // Lock horizon is the 3D mouse's own: Constrained orbit on and Lock
  // horizon off, or the other way round, come back as they were saved.
  // Settings from before it (Constrained orbit on, no Lock horizon) read
  // it as on, so that the 3D mouse keeps the horizon as it did.
  const QString older = directory.filePath(QStringLiteral("older.ini"));
  {
    QSettings before(older, QSettings::IniFormat);
    before.setValue(QStringLiteral("view/constrainedOrbit"), true);
    before.setValue(QStringLiteral("view/spaceMouseSpeed"), 1.5);
    SpaceMouseSettings upgraded;
    loadSpaceMouseSettings(before, upgraded);
    CHECK(upgraded.lockHorizon && upgraded.mode == SpaceMouseMode::Object && nearly(upgraded.speed, 1.5));
    upgraded.lockHorizon = false;
    saveSpaceMouseSettings(before, upgraded);
  }
  {
    QSettings after(older, QSettings::IniFormat);
    SpaceMouseSettings reread;
    loadSpaceMouseSettings(after, reread);
    CHECK(!reread.lockHorizon && after.value(QStringLiteral("view/constrainedOrbit")).toBool());
    after.setValue(QStringLiteral("view/constrainedOrbit"), false);
    after.setValue(QStringLiteral("view/spaceMouseLockHorizon"), true);
    loadSpaceMouseSettings(after, reread);
    CHECK(reread.lockHorizon);
    after.remove(QStringLiteral("view/spaceMouseLockHorizon"));
    loadSpaceMouseSettings(after, reread);
    CHECK(!reread.lockHorizon);
  }
}

void controllerTests() {
  using namespace mitcad;
  SpaceMouseSettings settings;
  bool flat = false;
  SpaceMouseRates last;
  int rateCalls = 0;
  QStringList commands;
  FakeBackend* backend = nullptr;
  bool present = false;
  int created = 0;
  SpaceMouseController::Hooks hooks;
  hooks.settings = [&settings] { return settings; };
  hooks.flat = [&flat] { return flat; };
  hooks.rates = [&last, &rateCalls](const SpaceMouseRates& rates) {
    last = rates;
    ++rateCalls;
  };
  hooks.runCommand = [&commands](const QString& id) { commands.append(id); };
  const auto factory = [&backend, &present, &created]() -> std::unique_ptr<SpaceMouseBackend> {
    auto made = std::make_unique<FakeBackend>(present);
    backend = made.get();
    ++created;
    return made;
  };

  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  {
    // No device: "3D mouse: none"; the backend stays to find one later.
    SpaceMouseController controller(hooks, factory);
    controller.refresh();
    CHECK(caught().contains(QStringLiteral("3D mouse: none")));
    CHECK(created == 1 && backend != nullptr && backend->starts == 1 && controller.running());
    CHECK(spaceMouseStatus().text == QStringLiteral("No 3D mouse found"));
    // A device that comes later.
    backend->connection(true, QStringLiteral("Later device"));
    CHECK(caught().contains(QStringLiteral("3D mouse: Fake connected (Later device)")));
    // Its translation and rotation move the camera as the mapping says.
    backend->motion({1.0, 0.0, 0.0, 0.0, 1.0, 0.0});
    CHECK(nearly(last.panX, kSpaceMousePanRate) && nearly(last.yaw, kSpaceMouseTurnRate) && last.zoom == 0.0);
    SpaceMousePose pose;
    pose.eye = {0.0, -100.0, 0.0};
    pose.target = {0.0, 0.0, 0.0};
    pose.height = 50.0;
    const SpaceMousePose moved = spaceMouseMove(pose, SpaceMouseVec{0.0, 0.0, 0.0}, last, 0.1, SpaceMouseStyle());
    CHECK(moved.eye != pose.eye && moved.target[0] < 0.0);
    // A reversed axis and the speed follow the settings at once.
    settings.reversed[0] = true;
    settings.speed = 2.0;
    controller.refresh();
    CHECK(nearly(last.panX, -2.0 * kSpaceMousePanRate) && nearly(last.yaw, 2.0 * kSpaceMouseTurnRate));
    // Locked rotation; a flat view (sketch mode).
    settings = SpaceMouseSettings();
    settings.lockRotation = true;
    backend->motion({0.0, 0.0, 1.0, 1.0, 1.0, 1.0});
    CHECK(!last.turning() && last.zoom > 0.0);
    settings = SpaceMouseSettings();
    flat = true;
    backend->motion({0.0, 1.0, 0.0, 1.0, 0.0, 0.0});
    CHECK(!last.turning() && last.panY > 0.0);
    flat = false;
    // Released: zero rates stop the motion.
    backend->motion({});
    CHECK(!last.moving());
    // The buttons: Fit and Home by default, a press runs, a release not.
    backend->button(0, true);
    backend->button(0, false);
    backend->button(1, true);
    CHECK(commands == (QStringList{QStringLiteral("view.fit"), QStringLiteral("view.home")}));
    settings.buttons = {QString(), QStringLiteral("view.top")};
    backend->button(0, true);
    backend->button(1, true);
    backend->button(5, true);
    CHECK(commands.size() == 3 && commands.last() == QStringLiteral("view.top"));
    CHECK(spaceMouseStatus().buttonsSeen == 6);
    // The device goes: zero rates.
    backend->motion({0.5, 0.0, 0.0, 0.0, 0.0, 0.0});
    CHECK(last.moving());
    backend->connection(false, QStringLiteral("Later device"));
    CHECK(caught().contains(QStringLiteral("3D mouse: Fake disconnected")) && !last.moving());
    // Off: the backend goes, "3D mouse: off", input no longer moves.
    settings.enabled = false;
    controller.refresh();
    CHECK(!controller.running() && caught().contains(QStringLiteral("3D mouse: off")));
    controller.motion({1.0, 1.0, 1.0, 1.0, 1.0, 1.0}, false);
    CHECK(!last.moving());
    // On again: a new backend, a device there now.
    present = true;
    settings.enabled = true;
    controller.refresh();
    CHECK(created == 2 && controller.running());
    CHECK(caught().contains(QStringLiteral("3D mouse: Fake connected (Test device)")));
    // A test's input (the test driver's) moves the view as the device's.
    controller.motion({0.0, 0.0, -1.0, 0.0, 0.0, 0.0}, false);
    CHECK(last.zoom < 0.0);
  }
  qInstallMessageHandler(previous);
  CHECK(rateCalls > 0);
}

// A motion's pacing and its timing from given frame times (mitcad#1023):
// frames begun at uneven times move the view by even steps that still add
// up to the time passed, a stall is not made up, and the log line's timing.
void timingTests() {
  using namespace mitcad;
  {
    // The first frame takes one of 60 Hz; a steady rate steps by its own
    // interval (the speed follows real time at any frame rate).
    SpaceMouseFramePacer pacer;
    CHECK(nearly(pacer.step(-1.0), kSpaceMouseFirstStep));
    for (const double interval : {0.02, 0.05}) {
      pacer.reset();
      double stepped = 0.0;
      for (int frame = 0; frame < 30; ++frame) {
        const double step = pacer.step(interval);
        CHECK(nearly(step, interval, 1e-12));
        stepped += step;
      }
      CHECK(nearly(stepped, 30 * interval, 1e-9));
    }
  }
  {
    // Frames begun in bursts (two at once, then a long wait: 5, 5, 40 ms,
    // a 60 Hz display on average): each frame's own time would step 1:8;
    // the paced steps stay within a few milliseconds of the mean, and they
    // add up to the time passed.
    SpaceMouseFramePacer pacer;
    const double pattern[] = {0.005, 0.005, 0.040};
    double passed = 0.0;
    double stepped = 0.0;
    double smallest = 1.0;
    double largest = 0.0;
    for (int frame = 0; frame < 300; ++frame) {
      const double interval = pattern[frame % 3];
      const double step = pacer.step(interval);
      passed += interval;
      stepped += step;
      if (frame >= static_cast<int>(SpaceMouseFramePacer::kWindow)) {
        smallest = std::min(smallest, step);
        largest = std::max(largest, step);
      }
    }
    CHECK(smallest > 0.012 && largest < 0.019);
    CHECK(largest / smallest < 1.4);
    CHECK(std::abs(stepped - passed) < 0.15);
  }
  {
    // A stall of half a second among 16 ms frames counts at most 100 ms,
    // spread over the frames that follow: no jump, nothing more made up.
    SpaceMouseFramePacer pacer;
    double stepped = 0.0;
    for (int frame = 0; frame < 20; ++frame) {
      stepped += pacer.step(0.016);
    }
    const double stall = pacer.step(0.5);
    CHECK(stall < 0.03);
    stepped += stall;
    for (int frame = 0; frame < 20; ++frame) {
      const double step = pacer.step(0.016);
      CHECK(step < 0.03);
      stepped += step;
    }
    CHECK(nearly(stepped, 40 * 0.016 + kSpaceMouseLongestStep, 1e-9));
  }
  {
    // The timing summary: five frames, 99 ms from the first to the last,
    // the longest 50 ms with a grid rebuild in it, two rebuilds in all.
    SpaceMouseTiming timing;
    timing.frame(-1.0, false);
    timing.frame(0.016, false);
    timing.frame(0.017, false);
    timing.frame(0.050, true);
    timing.frame(0.016, false);
    timing.gridRebuilds = 2;
    CHECK(timing.frames == 5);
    CHECK(nearly(timing.seconds, 0.099, 1e-12));
    CHECK(nearly(timing.longest, 0.050, 1e-12) && timing.longestRebuiltGrid);
    CHECK(nearly(timing.framesPerSecond(), 4.0 / 0.099, 1e-9));
    CHECK(timing.text() ==
          QStringLiteral("0.10 s, 40.4 fps, longest frame 50.0 ms with a grid rebuild, 2 grid rebuild(s)"));
    // A longer frame without a rebuild takes its place.
    timing.frame(0.080, false);
    CHECK(nearly(timing.longest, 0.080, 1e-12) && !timing.longestRebuiltGrid);
    // A single frame: no time, no rate.
    SpaceMouseTiming single;
    single.frame(-1.0, true);
    CHECK(single.frames == 1 && single.framesPerSecond() == 0.0);
    CHECK(single.text() == QStringLiteral("0.00 s, 0.0 fps, longest frame 0.0 ms, 0 grid rebuild(s)"));
  }
}

// mitcad#1034: a device that sends its translation (report 1) and its
// rotation (report 2) apart, each in a 13-byte slot as Raw Input gives
// them when the device declares a 13-byte report (dwSizeHid), keeps a
// steady rotation; a device that sends both in one 13-byte report 1 still
// gives both.
void splitReportTests() {
  using namespace mitcad;
  using Where = SpaceMouseRotationReport;
  // Translation 350, 0, 0 with padding that is no rotation (not even zero);
  // rotation Rz 100.
  const std::uint8_t first[13] = {1, 0x5e, 0x01, 0, 0, 0, 0, 7, 7, 7, 7, 7, 7};
  const std::uint8_t second[13] = {2, 0, 0, 0, 0, 0x64, 0, 0, 0, 0, 0, 0, 0};
  const std::array<int, 3> pushed{{350, 0, 0}};
  const std::array<int, 3> twisted{{0, 0, 100}};
  SpaceMouseHidReport report;
  // The parser told that the rotation has a report of its own reads none
  // from report 1.
  CHECK(parseSpaceMouseHidReport(first, sizeof(first), report, false));
  CHECK(report.translation && !report.rotation && report.t == pushed);
  // With the rotation's report known from the descriptor, and unknown (it
  // is learnt from the first report 2): the reports alternate under a
  // steady push and twist, and the rotation stays.
  for (const Where known : {Where::Unknown, Where::OwnReport}) {
    SpaceMouseHidReader reader;
    reader.setRotationReport(known);
    bool rotated = false;
    for (int step = 0; step < 20; ++step) {
      const bool even = step % 2 == 0;
      CHECK(reader.read(even ? first : second, 13, report));
      if (even && known == Where::OwnReport) {
        CHECK(report.translation && !report.rotation);
      }
      rotated = rotated || !even;
      if (rotated) {
        CHECK(reader.translation() == pushed && reader.rotation() == twisted);
        // Rz > 0 twists clockwise seen from above: the view's ry < 0
        // (spaceMouseFromHid, mitcad#1720).
        const SpaceMouseAxes axes = reader.axes();
        CHECK(nearly(axes[0], 1.0) && nearly(axes[3], 0.0) && nearly(axes[4], -100.0 / kSpaceMouseFullScale) &&
              nearly(axes[5], 0.0));
      }
    }
    CHECK(reader.rotationReport() == Where::OwnReport);
    CHECK(reader.reportIds() == (std::vector<int>{1, 2}));
    CHECK(reader.reportIdsText() == QStringLiteral("1, 2"));
  }
  // The newer devices' one report 1 of 13 bytes, known from the
  // descriptor or not: translation and rotation both, every time.
  const std::uint8_t both[13] = {1, 0x5e, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0x64, 0};
  for (const Where known : {Where::Unknown, Where::WithTranslation}) {
    SpaceMouseHidReader reader;
    reader.setRotationReport(known);
    for (int step = 0; step < 5; ++step) {
      CHECK(reader.read(both, sizeof(both), report));
      CHECK(report.translation && report.rotation);
      CHECK(reader.translation() == pushed && reader.rotation() == twisted);
    }
    CHECK(reader.rotationReport() == known);
    CHECK(reader.reportIds() == (std::vector<int>{1}));
  }
  // A report it does not know: false, the axes as they were.
  SpaceMouseHidReader reader;
  CHECK(reader.read(both, sizeof(both), report));
  const std::uint8_t unknown[7] = {9, 1, 2, 3, 4, 5, 6};
  CHECK(!reader.read(unknown, sizeof(unknown), report));
  CHECK(reader.translation() == pushed && reader.rotation() == twisted);
  CHECK(reader.reportIdsText() == QStringLiteral("1, 9"));
}

// The debug log's raw reports (mitcad#1034): the last report of each id,
// at most one line per 100 ms, and at once when the cap comes to rest.
void rawLogTests() {
  using namespace mitcad;
  const std::uint8_t first[7] = {1, 0x5e, 0x01, 0, 0, 0, 0};
  const std::uint8_t second[7] = {2, 0, 0, 0, 0, 0x64, 0};
  const std::array<int, 3> t{{350, 0, 0}};
  const std::array<int, 3> r{{0, 0, 100}};
  SpaceMouseRawLog rawLog;
  CHECK(rawLog.take(0, t, r).isEmpty()); // nothing came
  rawLog.add(first, sizeof(first));
  rawLog.add(second, sizeof(second));
  rawLog.add(first, sizeof(first));
  CHECK(rawLog.take(1000, t, r) ==
        QStringLiteral("3D mouse report: id 1 [01 5e 01 00 00 00 00], id 2 [02 00 00 00 00 64 00]; 3 report(s); "
                       "t 350 0 0, r 0 0 100"));
  CHECK(rawLog.take(1200, t, r).isEmpty()); // nothing came since
  rawLog.add(second, sizeof(second));
  CHECK(rawLog.take(1050, t, r).isEmpty());  // too soon
  CHECK(!rawLog.take(1050, t, r, true).isEmpty()); // at rest: at once
  // A thousand reports in a second: ten lines.
  int lines = 0;
  for (int ms = 2000; ms < 3000; ++ms) {
    rawLog.add(ms % 2 == 0 ? first : second, sizeof(first));
    if (!rawLog.take(ms, t, r).isEmpty()) {
      ++lines;
    }
  }
  CHECK(lines == 10);
}

// The axes each way to the view (mitcad#1034): each rotation axis of the
// device alone turns the view about one axis and never zooms or pans, read
// from the device's split reports as the Windows backend reads them, in
// every Navigation mode, with the camera's scale kept in orthographic and
// perspective; and the HID backends (Windows, macOS) and spacenavd's
// (Linux) give the same axes for the same motion.
void rotationMappingTests() {
  using namespace mitcad;
  const SpaceMouseSettings defaults{};
  const double frame = 1.0 / 60.0;
  for (std::size_t axis = 0; axis < 3; ++axis) {
    for (const int value : {350, -350, 120}) {
      std::array<int, 3> rotation{};
      rotation[axis] = value;
      const SpaceMouseAxes axes = spaceMouseFromHid({0, 0, 0}, rotation);
      CHECK(axes[0] == 0.0 && axes[1] == 0.0 && axes[2] == 0.0);
      const SpaceMouseRates rates = spaceMouseRates(defaults, axes, false);
      CHECK(rates.zoom == 0.0 && rates.panX == 0.0 && rates.panY == 0.0);
      // Rx tilts (pitch), Ry rolls, Rz spins (yaw).
      CHECK((rates.pitch != 0.0) == (axis == 0) && (rates.roll != 0.0) == (axis == 1) &&
            (rates.yaw != 0.0) == (axis == 2));
      // From split reports in 13-byte slots: the same rates at every report
      // after the first rotation.
      std::uint8_t first[13] = {1, 0, 0, 0, 0, 0, 0, 7, 7, 7, 7, 7, 7};
      std::uint8_t second[13] = {2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
      const auto bits = static_cast<std::uint16_t>(static_cast<std::int16_t>(value));
      second[1 + 2 * axis] = static_cast<std::uint8_t>(bits & 0xff);
      second[2 + 2 * axis] = static_cast<std::uint8_t>(bits >> 8);
      SpaceMouseHidReader reader;
      SpaceMouseHidReport report;
      CHECK(reader.read(second, sizeof(second), report));
      for (int step = 0; step < 6; ++step) {
        CHECK(reader.read(step % 2 == 0 ? first : second, sizeof(first), report));
        const SpaceMouseRates fromReports = spaceMouseRates(defaults, reader.axes(), false);
        CHECK(fromReports.zoom == 0.0 && fromReports.panX == 0.0 && fromReports.panY == 0.0);
        CHECK(fromReports.pitch == rates.pitch && fromReports.yaw == rates.yaw && fromReports.roll == rates.roll);
      }
      // Through the camera, in each mode and projection: turned, never
      // zoomed.
      for (const SpaceMouseMode mode : {SpaceMouseMode::Object, SpaceMouseMode::Camera, SpaceMouseMode::Fly}) {
        for (const bool perspective : {false, true}) {
          occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
          camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                                : Graphic3d_Camera::Projection_Orthographic);
          camera->SetEyeAndCenter(gp_Pnt(0.0, -100.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
          camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
          camera->SetScale(50.0);
          const double scale = camera->Scale();
          const gp_Dir direction = camera->Direction();
          const gp_Dir up = camera->Up();
          SpaceMouseStyle style;
          style.mode = mode;
          for (int step = 0; step < 30; ++step) {
            CHECK(moveSpaceMouseCamera(*camera, gp_Pnt(5.0, 5.0, 5.0), rates, frame, style));
          }
          CHECK(nearly(camera->Scale(), scale, 1e-9 * scale));
          CHECK(!camera->Direction().IsEqual(direction, 1e-6) || !camera->Up().IsEqual(up, 1e-6));
        }
      }
    }
  }
  // The same motion through HID (Windows, macOS) and through spacenavd,
  // which takes HID's values into its frame by swapping Y and Z and
  // reversing both, translation and rotation alike.
  const std::array<std::array<int, 6>, 4> motions{{{{350, 0, 0, 0, 0, 0}},
                                                   {{0, 120, -200, 0, 0, 0}},
                                                   {{0, 0, 0, 300, -150, 75}},
                                                   {{10, -20, 30, -40, 50, -60}}}};
  for (const std::array<int, 6>& h : motions) {
    const SpaceMouseAxes fromHid = spaceMouseFromHid({h[0], h[1], h[2]}, {h[3], h[4], h[5]});
    const SpaceMouseAxes fromSpnav = spaceMouseFromSpnav({h[0], -h[2], -h[1], h[3], -h[5], -h[4]});
    for (std::size_t axis = 0; axis < 6; ++axis) {
      CHECK(nearly(fromHid[axis], fromSpnav[axis]));
    }
  }
}

// A point of the model as the user sees it: to the right, up, and toward
// the user (from the eye, along the view's axes).
SpaceMouseVec seen(const mitcad::SpaceMousePose& pose, const SpaceMouseVec& point) {
  const SpaceMouseVec direction = unit(minus(pose.target, pose.eye));
  const SpaceMouseVec up = unit(pose.up);
  const SpaceMouseVec right = cross(direction, up);
  const SpaceMouseVec offset = minus(point, pose.eye);
  return {dot(offset, right), dot(offset, up), -dot(offset, direction)};
}

// Each of the cap's six motions alone (mitcad#1720), from the device's
// values to the model in Object mode: pushed right, lifted, pulled toward
// the user, tilted forward (its top away from the user), twisted clockwise
// seen from above, rolled right (its top to the right). The device's
// values follow its frame (X right, Y toward the user, Z down, the
// rotations by the right-hand rule about the same axes), read as the
// Windows backend reads a combined 13-byte report (a SpaceMouse Compact),
// as IOKit's values come on macOS, and as spacenavd sends them on Linux
// (Y and Z swapped and reversed, the rotation alike). Every way gives the
// same axis in the view's frame, the model does what the cap did, and the
// motion's log line names it.
void capMotionTests() {
  using namespace mitcad;
  struct Motion {
    const char* name;
    std::array<int, 3> t;
    std::array<int, 3> r;
    std::size_t axis; // the view's axis it moves
    bool positive;
    const char* logged; // the debug log's words for it
  };
  const Motion motions[] = {
      {"pushed right", {{280, 0, 0}}, {{0, 0, 0}}, 0, true, "moves right"},
      {"lifted", {{0, 0, -280}}, {{0, 0, 0}}, 1, true, "moves up"},
      {"pulled toward the user", {{0, 280, 0}}, {{0, 0, 0}}, 2, true, "comes closer"},
      {"tilted forward", {{0, 0, 0}}, {{-280, 0, 0}}, 3, false, "tilts its top away from the user"},
      {"twisted clockwise seen from above", {{0, 0, 0}}, {{0, 0, 280}}, 4, false,
       "turns clockwise seen from above"},
      {"rolled right", {{0, 0, 0}}, {{0, -280, 0}}, 5, false, "rolls clockwise as the user sees it"},
  };
  const SpaceMouseSettings defaults{};
  SpaceMousePose pose;
  pose.eye = {0.0, -100.0, 0.0};
  pose.target = {0.0, 0.0, 0.0};
  pose.up = {0.0, 0.0, 1.0};
  pose.height = 50.0;
  const SpaceMouseVec pivot{0.0, 0.0, 0.0};
  const SpaceMouseVec centre{0.0, 0.0, 0.0};
  const SpaceMouseVec top{0.0, 0.0, 10.0};
  const SpaceMouseVec front{0.0, -10.0, 0.0};
  for (const Motion& motion : motions) {
    // Windows: one combined report 1 (translation, then rotation).
    std::uint8_t bytes[13] = {1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    for (std::size_t index = 0; index < 6; ++index) {
      const int value = index < 3 ? motion.t[index] : motion.r[index - 3];
      const auto bits = static_cast<std::uint16_t>(static_cast<std::int16_t>(value));
      bytes[1 + 2 * index] = static_cast<std::uint8_t>(bits & 0xff);
      bytes[2 + 2 * index] = static_cast<std::uint8_t>(bits >> 8);
    }
    SpaceMouseHidReader reader;
    reader.setRotationReport(SpaceMouseRotationReport::WithTranslation);
    SpaceMouseHidReport report;
    CHECK(reader.read(bytes, sizeof(bytes), report));
    const SpaceMouseAxes fromRawInput = reader.axes();
    // macOS: IOKit's values of the same usages.
    const SpaceMouseAxes fromIokit = spaceMouseFromHid(motion.t, motion.r);
    // Linux: spacenavd's event of the same motion.
    const SpaceMouseAxes fromSpnav = spaceMouseFromSpnav(
        {motion.t[0], -motion.t[2], -motion.t[1], motion.r[0], -motion.r[2], -motion.r[1]});
    for (std::size_t axis = 0; axis < 6; ++axis) {
      CHECK(nearly(fromRawInput[axis], fromIokit[axis]) && nearly(fromSpnav[axis], fromIokit[axis]));
      if (axis == motion.axis) {
        CHECK(nearly(fromIokit[axis], motion.positive ? 0.8 : -0.8));
      } else {
        CHECK(fromIokit[axis] == 0.0);
      }
    }
    // The model in Object mode, the default.
    const SpaceMouseRates rates = spaceMouseRates(defaults, fromIokit, false);
    const SpaceMousePose moved = spaceMouseMove(pose, pivot, rates, 0.25, SpaceMouseStyle());
    switch (motion.axis) {
    case 0: // the model's centre moves to the right
      CHECK(seen(moved, centre)[0] > seen(pose, centre)[0] + 1.0);
      break;
    case 1: // ... up
      CHECK(seen(moved, centre)[1] > seen(pose, centre)[1] + 1.0);
      break;
    case 2: // closer: the view shows less around it
      CHECK(moved.height < pose.height * 0.95);
      break;
    case 3: // its top goes away from the user
      CHECK(seen(moved, top)[2] < seen(pose, top)[2] - 0.1);
      break;
    case 4: // its front goes to the left, as the cap's front does turned clockwise from above
      CHECK(seen(moved, front)[0] < seen(pose, front)[0] - 0.1);
      break;
    default: // its top goes to the right
      CHECK(seen(moved, top)[0] > seen(pose, top)[0] + 0.1);
      break;
    }
    // The debug log's line of the motion names what the model did.
    SpaceMouseAxesLog axesLog;
    CHECK(axesLog.take().isEmpty());
    axesLog.add(fromIokit);
    axesLog.add(SpaceMouseAxes{});
    const QString line = axesLog.take();
    CHECK(line.startsWith(QStringLiteral("3D mouse axes: tx ")));
    CHECK(line.endsWith(QStringLiteral("; strongest %1: in Object mode the model %2")
                            .arg(QLatin1String(spaceMouseAxisKey(motion.axis)), QLatin1String(motion.logged))));
    CHECK(line.contains(QStringLiteral("%1 %2").arg(QLatin1String(spaceMouseAxisKey(motion.axis)),
                                                   motion.positive ? QStringLiteral("0.80")
                                                                   : QStringLiteral("-0.80"))));
    CHECK(axesLog.take().isEmpty());
  }
  // The largest deflection of each axis over the motion, its sign kept.
  SpaceMouseAxesLog peaks;
  peaks.add({0.1, 0.0, 0.0, -0.3, 0.0, 0.0});
  peaks.add({0.05, 0.0, 0.0, -0.6, 0.2, 0.0});
  CHECK(peaks.take() == QStringLiteral("3D mouse axes: tx 0.10, ty 0.00, tz 0.00, rx -0.60, ry 0.20, rz 0.00; "
                                   "strongest rx: in Object mode the model tilts its top away from the user"));
}

// The controller writes a motion's axes to the debug log once the cap
// comes to rest (mitcad#1720), and only after a motion that moved the view.
void axesLogControllerTests() {
  using namespace mitcad;
  SpaceMouseRates last;
  SpaceMouseController::Hooks hooks;
  hooks.settings = [] { return SpaceMouseSettings(); };
  hooks.flat = [] { return false; };
  hooks.rates = [&last](const SpaceMouseRates& rates) { last = rates; };
  const auto factory = []() -> std::unique_ptr<SpaceMouseBackend> { return std::make_unique<FakeBackend>(false); };
  SpaceMouseController controller(std::move(hooks), factory);
  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  const auto axesLines = [] {
    int count = 0;
    for (const QString& line : caught()) {
      count += line.startsWith(QStringLiteral("3D mouse axes: ")) ? 1 : 0;
    }
    return count;
  };
  // Below the dead zone: nothing moved, nothing logged.
  controller.motion({0.01, 0.0, 0.0, 0.0, 0.0, 0.0}, false);
  controller.motion(SpaceMouseAxes{}, false);
  CHECK(axesLines() == 0);
  controller.motion({0.0, 0.0, 0.0, 0.0, -0.5, 0.0}, false);
  controller.motion({0.0, 0.0, 0.0, 0.0, -0.7, 0.0}, false);
  CHECK(last.moving() && axesLines() == 0);
  controller.motion(SpaceMouseAxes{}, false);
  qInstallMessageHandler(previous);
  CHECK(!last.moving() && axesLines() == 1);
  CHECK(caught().contains(QStringLiteral("3D mouse axes: tx 0.00, ty 0.00, tz 0.00, rx 0.00, ry -0.70, rz 0.00; "
                                         "strongest ry: in Object mode the model turns clockwise seen from above")));
}

// Lock Rotation (mitcad#1910): outside a sketch the toggle flips the
// settings' Lock rotation (saved through the change hook); in a flat view
// (a sketch) the rotation is off whatever the settings say until the
// toggle frees it for that sketch, and off again for the next sketch or
// the same one entered again; the check mark's hook follows each change.
void rotationLockTests() {
  using namespace mitcad;
  SpaceMouseSettings settings;
  bool flat = false;
  SpaceMouseRates last;
  int changes = 0;
  std::vector<bool> reported;
  SpaceMouseController::Hooks hooks;
  hooks.settings = [&settings] { return settings; };
  hooks.flat = [&flat] { return flat; };
  hooks.rates = [&last](const SpaceMouseRates& rates) { last = rates; };
  hooks.change = [&settings, &changes](const SpaceMouseSettings& changed) {
    settings = changed;
    ++changes;
  };
  hooks.rotationLock = [&reported](bool locked) { reported.push_back(locked); };
  const auto factory = []() -> std::unique_ptr<SpaceMouseBackend> { return std::make_unique<FakeBackend>(false); };
  SpaceMouseController controller(std::move(hooks), factory);
  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  controller.refresh();
  const SpaceMouseAxes spin{{0.0, 0.0, 0.0, 0.0, 0.8, 0.0}};

  // Outside a sketch: free as the settings say; the toggle locks the
  // rotation and saves it, a cap held now stops turning at once, and the
  // toggle again frees it.
  CHECK(!controller.rotationLocked() && !reported.empty() && !reported.back());
  controller.motion(spin, false);
  CHECK(last.turning());
  CHECK(controller.toggleRotationLock());
  CHECK(settings.lockRotation && changes == 1 && controller.rotationLocked() && reported.back());
  CHECK(!last.turning());
  CHECK(caught().contains(QStringLiteral("3D mouse: rotation locked")));
  CHECK(!controller.toggleRotationLock());
  CHECK(!settings.lockRotation && changes == 2 && !controller.rotationLocked() && !reported.back());
  CHECK(last.turning());
  CHECK(caught().contains(QStringLiteral("3D mouse: rotation unlocked")));

  // A sketch: locked although the settings have it free; the cap pans and
  // zooms only.
  flat = true;
  controller.sketchChanged();
  CHECK(controller.rotationLocked() && reported.back() && !last.turning());
  controller.motion({{0.0, 0.8, 0.0, 0.0, 0.8, 0.0}}, false);
  CHECK(!last.turning() && last.panY > 0.0);
  // The toggle frees it for this sketch, the settings untouched: the cap
  // turns the view as outside a sketch.
  controller.motion(spin, false);
  CHECK(!controller.toggleRotationLock());
  CHECK(!controller.rotationLocked() && !reported.back() && last.turning());
  CHECK(changes == 2 && !settings.lockRotation);
  CHECK(caught().contains(QStringLiteral("3D mouse: rotation unlocked in the sketch")));
  // The settings' Lock rotation does not lock it again in this sketch.
  settings.lockRotation = true;
  controller.refresh();
  CHECK(!controller.rotationLocked() && last.turning());
  // Toggled again: locked in the sketch.
  CHECK(controller.toggleRotationLock());
  CHECK(controller.rotationLocked() && !last.turning() && changes == 2 && settings.lockRotation);
  CHECK(caught().contains(QStringLiteral("3D mouse: rotation locked in the sketch")));
  CHECK(!controller.toggleRotationLock());
  CHECK(last.turning());

  // The sketch ends: outside it the settings decide (locked now), and the
  // next sketch starts locked again.
  flat = false;
  controller.sketchChanged();
  CHECK(controller.rotationLocked() && reported.back() && !last.turning());
  settings.lockRotation = false;
  controller.refresh();
  CHECK(!controller.rotationLocked() && !reported.back() && last.turning());
  flat = true;
  controller.sketchChanged();
  CHECK(controller.rotationLocked() && reported.back() && !last.turning());
  // Freed, then the same sketch entered again: locked again.
  CHECK(!controller.toggleRotationLock() && last.turning());
  controller.sketchChanged();
  CHECK(controller.rotationLocked() && !last.turning());
  // A view no longer flat forgets a freed rotation even without a word of
  // the sketch's end.
  CHECK(!controller.toggleRotationLock() && last.turning());
  flat = false;
  controller.motion(spin, false);
  CHECK(last.turning());
  flat = true;
  controller.motion(spin, false);
  CHECK(controller.rotationLocked() && !last.turning());
  CHECK(changes == 2);
  controller.motion(SpaceMouseAxes{}, false);
  qInstallMessageHandler(previous);
}

// A device's default button (mitcad#1910): the HID ids read from a
// backend's device name, the known rotation lock keys by product id, the
// key bound to Lock Rotation only where the settings have no entry for it,
// and the controller binding it when such a device connects.
void deviceButtonTests() {
  using namespace mitcad;
  const QString lock = spaceMouseLockRotationCommand();
  int vendor = 0;
  int product = 0;
  CHECK(spaceMouseHidIds(QStringLiteral("SpaceMouse Pro (HID 046d:c62b)"), vendor, product));
  CHECK(vendor == 0x046d && product == 0xc62b);
  CHECK(spaceMouseHidIds(QStringLiteral("HID 256f:c631"), vendor, product));
  CHECK(vendor == 0x256f && product == 0xc631);
  CHECK(!spaceMouseHidIds(QStringLiteral("/var/run/spnav.sock"), vendor, product));
  CHECK(!spaceMouseHidIds(QStringLiteral("Device (HID 25zz:c631)"), vendor, product));
  CHECK(!spaceMouseHidIds(QStringLiteral("Device (HID 256f)"), vendor, product));
  CHECK(vendor == 0x256f && product == 0xc631); // a name without ids leaves them
  CHECK(spaceMouseLockKeyButton(0x046d, 0xc62b) == 26);
  CHECK(spaceMouseLockKeyButton(0x256f, 0xc631) == 26 && spaceMouseLockKeyButton(0x256f, 0xc632) == 26);
  CHECK(spaceMouseLockKeyButton(0x256f, 0xc638) == 26);
  CHECK(spaceMouseLockKeyButton(0x256f, 0xc635) == -1); // a Compact: no lock key
  CHECK(spaceMouseLockKeyButton(0x256f, 0xc652) == -1); // a universal receiver
  CHECK(spaceMouseLockKeyButton(0x1234, 0xc62b) == -1);

  SpaceMouseSettings settings;
  CHECK(addSpaceMouseDeviceButtons(settings, QStringLiteral("SpaceMouse Pro (HID 046d:c62b)")));
  CHECK(settings.buttons.size() == 27 && settings.buttons.at(26) == lock);
  CHECK(settings.buttons.at(0) == QStringLiteral("view.fit") && settings.buttons.at(1) == QStringLiteral("view.home"));
  CHECK(settings.buttons.at(2).isEmpty() && settings.buttons.at(25).isEmpty());
  // Bound already, or set to None by the user: left as it is.
  CHECK(!addSpaceMouseDeviceButtons(settings, QStringLiteral("SpaceMouse Pro (HID 046d:c62b)")));
  settings.buttons[26] = QString();
  CHECK(!addSpaceMouseDeviceButtons(settings, QStringLiteral("SpaceMouse Pro (HID 046d:c62b)")));
  CHECK(settings.buttons.size() == 27 && settings.buttons.at(26).isEmpty());
  // Other devices, and spacenavd, which names none: the buttons as they are.
  SpaceMouseSettings other;
  CHECK(!addSpaceMouseDeviceButtons(other, QStringLiteral("SpaceMouse Compact (HID 256f:c635)")));
  CHECK(!addSpaceMouseDeviceButtons(other, QStringLiteral("/var/run/spnav.sock")));
  CHECK(other.buttons == SpaceMouseSettings().buttons);
  // The binding survives the settings' round trip.
  {
    QTemporaryDir dir;
    QSettings store(dir.filePath(QStringLiteral("buttons.ini")), QSettings::IniFormat);
    SpaceMouseSettings bound;
    CHECK(addSpaceMouseDeviceButtons(bound, QStringLiteral("HID 256f:c632")));
    saveSpaceMouseSettings(store, bound);
    SpaceMouseSettings reread;
    loadSpaceMouseSettings(store, reread);
    CHECK(reread.buttons == bound.buttons && reread.buttons.value(26) == lock);
  }

  // The controller: such a device connecting binds its key through the
  // change hook, logged; its press runs Lock Rotation; a device without a
  // known key changes nothing.
  SpaceMouseSettings live;
  int changes = 0;
  QStringList commands;
  FakeBackend* backend = nullptr;
  SpaceMouseController::Hooks hooks;
  hooks.settings = [&live] { return live; };
  hooks.flat = [] { return false; };
  hooks.rates = [](const SpaceMouseRates&) {};
  hooks.runCommand = [&commands](const QString& id) { commands.append(id); };
  hooks.change = [&live, &changes](const SpaceMouseSettings& changed) {
    live = changed;
    ++changes;
  };
  const auto factory = [&backend]() -> std::unique_ptr<SpaceMouseBackend> {
    auto made = std::make_unique<FakeBackend>(false);
    backend = made.get();
    return made;
  };
  SpaceMouseController controller(std::move(hooks), factory);
  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  controller.refresh();
  CHECK(backend != nullptr && changes == 0);
  if (backend != nullptr) {
    backend->connection(true, QStringLiteral("SpaceMouse Compact (HID 256f:c635)"));
    CHECK(changes == 0 && live.buttons == SpaceMouseSettings().buttons);
    backend->connection(true, QStringLiteral("SpaceMouse Pro (HID 046d:c62b)"));
    CHECK(changes == 1 && live.buttons.value(26) == lock);
    CHECK(caught().contains(QStringLiteral("3D mouse: button 27 runs view.space_mouse_lock_rotation by default")));
    backend->button(26, true);
    backend->button(26, false);
    CHECK(commands == QStringList(lock));
    // Connected again: bound already, nothing changes.
    backend->connection(true, QStringLiteral("SpaceMouse Pro (HID 046d:c62b)"));
    CHECK(changes == 1);
  }
  qInstallMessageHandler(previous);
}

void driverReportTests() {
  using namespace mitcad;
  using Kind = SiAppDeviceEvent::Kind;
  std::array<std::int32_t, 11> packet{2, 0, 1, 0, 0, -350, 100, -250, 200, -50, 350};
  const auto* bytes = reinterpret_cast<const std::byte*>(packet.data());
  const auto read = [&] { return parseSiAppDeviceEvent(bytes, sizeof(packet)); };
  CHECK(parseSiAppDeviceEvent(bytes, sizeof(packet) - 1).kind == Kind::Other);
  const auto motion = read();
  CHECK(motion.kind == Kind::Motion);
  const std::array<int, 6> expected{-350, 100, -250, 200, -50, 350};
  CHECK(motion.axes == expected);

  packet[0] = 1;
  CHECK(read().kind == Kind::Buttons && read().buttons == 1);
  packet[0] = 4;
  CHECK(read().kind == Kind::Rest);
  packet[0] = 12;
  packet[1] = 32;
  CHECK(read().kind == Kind::ButtonDown && read().button == 31);
  packet[0] = 13;
  packet[1] = 1;
  CHECK(read().kind == Kind::ButtonUp && read().button == 0);
  packet[1] = 0;
  CHECK(read().kind == Kind::Other);
  packet[0] = 14;
  CHECK(read().kind == Kind::DeviceChange);

  const std::array<std::uint16_t, 10> message{9, 17, 65186, 100, 65286, 200, 65486, 350, 16, 0};
  CHECK(magellanMotion(message) == expected);
}

// The design's window shown in its frame (mitcad#2152): the controller
// passes it to its backend, which may then find the native window it found
// none of at start; nothing before a backend runs.
void windowPlacedTests() {
  using namespace mitcad;
  FakeBackend* backend = nullptr;
  SpaceMouseController::Hooks hooks;
  hooks.settings = [] { return SpaceMouseSettings(); };
  hooks.nativeWindow = [] { return std::uintptr_t{0}; };
  const auto factory = [&backend]() -> std::unique_ptr<SpaceMouseBackend> {
    auto made = std::make_unique<FakeBackend>(false);
    backend = made.get();
    return made;
  };
  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  {
    SpaceMouseController controller(hooks, factory);
    controller.windowPlaced();
    CHECK(backend == nullptr);
    controller.refresh();
    CHECK(backend != nullptr && backend->placed == 0 && backend->nativeWindow && backend->nativeWindow() == 0);
    controller.windowPlaced();
    CHECK(backend->placed == 1);
    // A start tried again that finds the device reports it as a device
    // that comes later.
    backend->connection(true, QStringLiteral("Placed device"));
    CHECK(caught().contains(QStringLiteral("3D mouse: Fake connected (Placed device)")));
  }
  qInstallMessageHandler(previous);
}

// Two designs on the process's one connection (mitcad#2297, GitHub
// mitcad/Mitcad#587): one backend is made, not two; the device's motion
// and buttons reach the active design only, and a held motion of the
// design left stops; the backend's window is the active design's and it
// hears each change; turned off in one design the device runs on for the
// other; after the newer design is destroyed the first still gets motion
// on the same backend; the last design gone, the connection goes with it.
void twoDesignTests() {
  using namespace mitcad;
  FakeBackend* backend = nullptr;
  int created = 0;
  const SpaceMouseConnection::Factory factory = [&backend, &created]() -> std::unique_ptr<SpaceMouseBackend> {
    auto made = std::make_unique<FakeBackend>(true);
    backend = made.get();
    ++created;
    return made;
  };
  struct Design {
    SpaceMouseSettings settings;
    SpaceMouseRates last;
    QStringList commands;
    SpaceMouseController::Hooks hooks(std::uintptr_t window) {
      SpaceMouseController::Hooks made;
      made.settings = [this] { return settings; };
      made.flat = [] { return false; };
      made.rates = [this](const SpaceMouseRates& rates) { last = rates; };
      made.runCommand = [this](const QString& id) { commands.append(id); };
      made.nativeWindow = [window] { return window; };
      return made;
    }
  };
  Design first;
  Design second;
  const QtMessageHandler previous = qInstallMessageHandler(catchMessage);
  caught().clear();
  {
    std::shared_ptr<SpaceMouseConnection> shared = SpaceMouseConnection::shared(factory);
    CHECK(SpaceMouseConnection::shared(factory) == shared); // the live one
    SpaceMouseController a(first.hooks(1), shared);
    shared.reset();
    a.refresh();
    CHECK(created == 1 && a.running() && backend != nullptr && backend->starts == 1);
    CHECK(caught().count(QStringLiteral("3D mouse: Fake connected (Test device)")) == 1);
    {
      // The second design: the same connection, no second backend.
      SpaceMouseController b(second.hooks(2), SpaceMouseConnection::shared(factory));
      b.refresh();
      CHECK(&a.connection() == &b.connection());
      CHECK(created == 1 && backend->starts == 1 && b.running());
      CHECK(caught().count(QStringLiteral("3D mouse: Fake connected (Test device)")) == 1);
      // The window manager activates the design it opened: its view moves,
      // the first one's not; the backend's window is its.
      const int placed = backend->placed;
      b.activate();
      CHECK(a.connection().current() == &b && backend->placed == placed + 1);
      CHECK(backend->nativeWindow && backend->nativeWindow() == 2);
      backend->motion({1.0, 0.0, 0.0, 0.0, 0.0, 0.0});
      CHECK(second.last.moving() && !first.last.moving());
      backend->button(0, true);
      backend->button(0, false);
      CHECK(second.commands == QStringList(QStringLiteral("view.fit")) && first.commands.isEmpty());
      // Back to the first while the cap is held: the second stops, the
      // first moves from the next input on.
      a.activate();
      CHECK(!second.last.moving() && backend->placed == placed + 2 && backend->nativeWindow() == 1);
      backend->motion({0.0, 0.0, 0.0, 0.0, 1.0, 0.0});
      CHECK(first.last.moving() && first.last.turning() && !second.last.moving());
      backend->button(1, true);
      CHECK(first.commands == QStringList(QStringLiteral("view.home")) && second.commands.size() == 1);
      backend->motion({});
      CHECK(!first.last.moving());
      // A test's input (the test driver's) goes the same way.
      a.connection().motion({0.0, 0.0, 1.0, 0.0, 0.0, 0.0}, false);
      CHECK(first.last.zoom > 0.0 && !second.last.moving());
      a.connection().motion({}, false);
      // Turned off in the second design: the device runs on for the first.
      caught().clear();
      second.settings.enabled = false;
      b.refresh();
      CHECK(!b.running() && a.running() && created == 1 && !caught().contains(QStringLiteral("3D mouse: off")));
      second.settings.enabled = true;
      b.refresh();
      CHECK(b.running() && created == 1 && backend->starts == 1);
      // The second active again when it closes.
      b.activate();
    }
    // The newer design destroyed: the first gets the input, on the same
    // backend, which follows its window.
    CHECK(created == 1 && a.running() && a.connection().current() == &a && backend->nativeWindow() == 1);
    backend->motion({1.0, 0.0, 0.0, 0.0, 0.0, 0.0});
    CHECK(first.last.moving());
    backend->motion({});
    CHECK(!first.last.moving());
    // The device goes: one line for the process.
    caught().clear();
    backend->connection(false, QStringLiteral("Test device"));
    CHECK(caught().count(QStringLiteral("3D mouse: Fake disconnected")) == 1);
    // Off in the last design that had it on: the backend stops, logged once.
    first.settings.enabled = false;
    a.refresh();
    CHECK(!a.running() && caught().count(QStringLiteral("3D mouse: off")) == 1);
  }
  // The last design gone: so is its connection; the next one makes a new
  // backend.
  {
    SpaceMouseController c(first.hooks(3), SpaceMouseConnection::shared(factory));
    first.settings.enabled = true;
    c.refresh();
    CHECK(created == 2 && c.running());
  }
  qInstallMessageHandler(previous);
}

} // namespace

int spaceMouseTests() {
  reportTests();
  driverReportTests();
  splitReportTests();
  rawLogTests();
  rotationMappingTests();
  capMotionTests();
  axesLogControllerTests();
  mappingTests();
  crossTalkTests();
  dominantHoldTests();
  moveTests();
  modeTests();
  cameraTests();
  settingsTests();
  controllerTests();
  timingTests();
  rotationLockTests();
  deviceButtonTests();
  windowPlacedTests();
  twoDesignTests();
  return failures;
}
MITCAD_UNIT_TEST(spaceMouseTests);
