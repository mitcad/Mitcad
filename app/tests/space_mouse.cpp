// SPDX-License-Identifier: MIT
// 3D mice (mitcad#951): the devices' reports, the mapping of their axes to
// the camera (speed, dead zone, reversed axes, locks, the dominant axis, a
// flat view), the camera's move, the settings, and the controller driven
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

  // Pushed right for 0.5 s at 1 view height per second: the model moves
  // right, the camera 25 mm to the left.
  SpaceMouseRates rates;
  rates.panX = 1.0;
  SpaceMousePose moved = spaceMouseMove(pose, pivot, rates, 0.5, false);
  CHECK(nearly(moved.target[0], -25.0) && nearly(moved.eye[0], -25.0) && nearly(moved.target[2], 0.0));
  CHECK(nearly(moved.height, 50.0));
  // The pan follows the visible height, the zoom the scale: in a view
  // taller than wide the two differ, and the height stays as it was.
  SpaceMousePose tall = pose;
  tall.panHeight = 80.0;
  moved = spaceMouseMove(tall, pivot, rates, 0.5, false);
  CHECK(nearly(moved.target[0], -40.0) && nearly(moved.height, 50.0) && nearly(moved.panHeight, 80.0));
  // Lifted: the model moves up, the camera down.
  rates = SpaceMouseRates();
  rates.panY = 1.0;
  moved = spaceMouseMove(pose, pivot, rates, 0.5, false);
  CHECK(nearly(moved.target[2], -25.0));
  // Pulled: closer, a smaller view height.
  rates = SpaceMouseRates();
  rates.zoom = 1.0;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, false);
  CHECK(nearly(moved.height, 50.0 * std::exp(-1.0)));
  // Spun counter-clockwise (seen from above) by a quarter turn: the
  // model's front turns to the right (+X), the camera to -X, looking
  // along +X.
  const double quarter = std::acos(0.0);
  rates = SpaceMouseRates();
  rates.yaw = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, false);
  CHECK(nearly(moved.eye[0], -100.0, 1e-6) && nearly(moved.eye[1], 0.0, 1e-6) && nearly(moved.eye[2], 0.0, 1e-6));
  CHECK(nearly(moved.up[2], 1.0, 1e-9));
  // The constrained orbit turns the same way about Z.
  SpaceMousePose upright = spaceMouseMove(pose, pivot, rates, 1.0, true);
  CHECK(nearly(upright.eye[0], -100.0, 1e-6) && nearly(upright.up[2], 1.0, 1e-9));
  // Tilted toward the user by a quarter turn: the model's top faces the
  // camera, which looks down from +Z with +Y up.
  rates = SpaceMouseRates();
  rates.pitch = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, false);
  CHECK(nearly(moved.eye[2], 100.0, 1e-6) && nearly(moved.up[1], 1.0, 1e-9));
  // The constrained orbit stops short of the pole, but goes half way.
  CHECK(spaceMouseMove(pose, pivot, rates, 1.0, true).eye == pose.eye);
  moved = spaceMouseMove(pose, pivot, rates, 0.5, true);
  CHECK(nearly(moved.eye[2], 100.0 * std::sqrt(0.5), 1e-6) && nearly(moved.eye[1], -100.0 * std::sqrt(0.5), 1e-6));
  // Rolled counter-clockwise (as the user sees it): the model's up points
  // to the left of the view, the camera's up along +X. Not with the
  // constrained orbit.
  rates = SpaceMouseRates();
  rates.roll = quarter;
  moved = spaceMouseMove(pose, pivot, rates, 1.0, false);
  CHECK(nearly(moved.up[0], 1.0, 1e-9) && nearly(moved.eye[1], -100.0, 1e-9) && nearly(moved.eye[0], 0.0, 1e-9));
  CHECK(nearly(unit(minus(moved.target, moved.eye))[1], 1.0, 1e-9));
  upright = spaceMouseMove(pose, pivot, rates, 1.0, true);
  CHECK(upright.eye == pose.eye && nearly(upright.up[2], 1.0, 1e-12));
  // About an orbit centre off the target: the target swings round it.
  rates = SpaceMouseRates();
  rates.yaw = 2.0 * quarter;
  moved = spaceMouseMove(pose, SpaceMouseVec{10.0, 0.0, 0.0}, rates, 1.0, false);
  CHECK(nearly(moved.target[0], 20.0, 1e-6) && nearly(moved.target[1], 0.0, 1e-6));
  // A pose that makes no view, or a time that is no number, stays.
  SpaceMousePose broken = pose;
  broken.eye = broken.target;
  CHECK(spaceMouseMove(broken, pivot, rates, 1.0, false).eye == broken.eye);
  CHECK(spaceMouseMove(pose, pivot, rates, std::nan(""), false).eye == pose.eye);
}

// The step on OCCT's camera, frame after frame, as OcctViewer applies it
// (mitcad#951: in a view taller than wide the camera's visible height is
// larger than its scale; taking the one for the other grew the view by
// the aspect at every frame, a ten-thousandfold in a second). Both aspects
// and both projections: a pan and a turn keep the scale and the visible
// height, a zoom in shrinks both by the rate, zero rates change nothing.
void cameraTests() {
  using namespace mitcad;
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
        CHECK(moveSpaceMouseCamera(*camera, camera->Center(), pan, frame, false));
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
        moveSpaceMouseCamera(*camera, gp_Pnt(5.0, 5.0, 5.0), turn, frame, step % 2 == 0);
      }
      CHECK(nearly(camera->Scale(), scale, 1e-9 * scale));
      CHECK(nearly(camera->ViewDimensions().Y(), visible, 1e-9 * visible));
      // Pulled for a second at one e-fold per second: closer by e.
      SpaceMouseRates zoom;
      zoom.zoom = 1.0;
      for (int step = 0; step < 60; ++step) {
        moveSpaceMouseCamera(*camera, camera->Center(), zoom, frame, false);
      }
      CHECK(nearly(camera->Scale(), scale * std::exp(-1.0), 1e-9 * scale));
      CHECK(nearly(camera->ViewDimensions().Y(), visible * std::exp(-1.0), 1e-9 * visible));
      // At rest: nothing changes.
      const gp_Pnt eye = camera->Eye();
      CHECK(!moveSpaceMouseCamera(*camera, camera->Center(), SpaceMouseRates(), frame, false));
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
    SpaceMouseSettings chosen;
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
  // A speed out of range comes back within it.
  store.setValue(QStringLiteral("view/spaceMouseSpeed"), 40.0);
  loadSpaceMouseSettings(store, loaded);
  CHECK(nearly(loaded.speed, 4.0));
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
    const SpaceMousePose moved = spaceMouseMove(pose, SpaceMouseVec{0.0, 0.0, 0.0}, last, 0.1, false);
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
  cameraTests();
  settingsTests();
  controllerTests();
  return failures;
}
