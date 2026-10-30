// SPDX-License-Identifier: MIT
// 3D mice (mitcad#951): the devices' reports, the mapping of their axes to
// the camera (speed, dead zone, reversed axes, locks, the dominant axis, a
// flat view), the camera's move, the Navigation modes and Lock horizon
// (mitcad#978), the settings, and the controller driven
// by a backend of the test's own: its events move the camera, its buttons
// run their commands, and no device logs "3D mouse: none".
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

#include "view/SpaceMouseCamera.hpp"
#include "view/SpaceMouseController.hpp"
#include "view/SpaceMouseNavigation.hpp"

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
  int starts = 0;

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
  // pulled (Y > 0) are +x, +y, +z of the view's frame.
  const SpaceMouseAxes hid = spaceMouseFromHid({350, 175, -350}, {0, 350, -175});
  CHECK(nearly(hid[0], 1.0) && nearly(hid[1], 1.0) && nearly(hid[2], 0.5));
  CHECK(nearly(hid[3], 0.0) && nearly(hid[4], 0.5) && nearly(hid[5], 1.0));

  // spacenavd: a motion, a press and a release; its z points into the
  // screen.
  SpnavEvent event = parseSpnavEvent({0, 350, 0, 175, 0, 0, -350, 16});
  CHECK(event.kind == SpnavEvent::Kind::Motion);
  const SpaceMouseAxes spnav = spaceMouseFromSpnav(event.motion);
  CHECK(nearly(spnav[0], 1.0) && nearly(spnav[2], -0.5) && nearly(spnav[5], -1.0));
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
  // Off: nothing moves.
  settings = SpaceMouseSettings();
  settings.enabled = false;
  CHECK(!spaceMouseRates(settings, full, false).moving());
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
  const SpaceMouseSettings defaults{};
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
    SpaceMouseSettings chosen;
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
  // A speed out of range comes back within it, a mode of no name as
  // Object.
  store.setValue(QStringLiteral("view/spaceMouseSpeed"), 40.0);
  store.setValue(QStringLiteral("view/spaceMouseMode"), QStringLiteral("sideways"));
  loadSpaceMouseSettings(store, loaded);
  CHECK(nearly(loaded.speed, 4.0) && loaded.mode == SpaceMouseMode::Object);
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

} // namespace

int spaceMouseTests() {
  reportTests();
  mappingTests();
  moveTests();
  modeTests();
  cameraTests();
  settingsTests();
  controllerTests();
  return failures;
}
