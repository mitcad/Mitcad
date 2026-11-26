// SPDX-License-Identifier: MIT
// The tutorials' input demonstrations and the learner's bindings
// (mitcad#2366, tutorials/GuideDemonstration and tutorials/GuideBindings): a
// demonstration read from its JSON with the page's pictures (a file of the
// tutorial's folder, an SVG or a PNG as a data: address) drawn at their
// size and the window's pixel ratio, refused with the reason; where its
// frames are at a moment over the target (a glide, a pause between plays,
// its end); a command's keys and the view's gestures of a navigation
// scheme as the page gets them, the mouse's and the trackpad's each on its
// own, and a key changed in the registry in the next answer (mitcad#2439).
#include "UnitTests.hpp"
#include "framework/CommandRegistry.hpp"
#include "tutorials/GuideBindings.hpp"
#include "tutorials/GuideDemonstration.hpp"

#include <cmath>
#include <cstdio>
#include <utility>

#include <QBuffer>
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "guide_demonstration.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// A JSON comparison that prints both values when it fails.
void checkJson(const QJsonArray& actual, const QJsonArray& expected, const char* expression, int line) {
  if (actual != expected) {
    std::fprintf(stderr, "guide_demonstration.cpp:%d: check failed: %s (actual %s, expected %s)\n", line,
                 expression, QJsonDocument(actual).toJson(QJsonDocument::Compact).constData(),
                 QJsonDocument(expected).toJson(QJsonDocument::Compact).constData());
    ++failures;
  }
}

#define CHECK_JSON(actual, expected) checkJson((actual), (expected), #actual " == " #expected, __LINE__)

using namespace mitcad;

QJsonObject json(const QByteArray& text) { return QJsonDocument::fromJson(text).object(); }

bool near(const QPointF& a, const QPointF& b) {
  return std::abs(a.x() - b.x()) < 1e-6 && std::abs(a.y() - b.y()) < 1e-6;
}

// A 20 x 10 SVG, as a page writes it into a data: address.
const char kSvg[] = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='20' height='10'%3E"
                    "%3Crect width='20' height='10' fill='red'/%3E%3C/svg%3E";

// A 4 x 4 PNG as a base64 data: address.
QString pngAddress() {
  QImage image(4, 4, QImage::Format_ARGB32);
  image.fill(Qt::blue);
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  image.save(&buffer, "PNG");
  return QStringLiteral("data:image/png;base64,") + QString::fromLatin1(bytes.toBase64());
}

// The tutorial's folder: one picture, "images/pointer.svg"; counts its reads.
int reads = 0;
bool loader(const QString& path, QByteArray* bytes, QString* error) {
  ++reads;
  if (path != QStringLiteral("images/pointer.svg")) {
    *error = QStringLiteral("no %1").arg(path);
    return false;
  }
  *bytes = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='24'><path d='M0 0 L16 16 L0 24 Z'/></svg>";
  return true;
}

void dataAddresses() {
  QByteArray bytes;
  QString mime;
  QString error;
  CHECK(guideDataUrl(QString::fromLatin1(kSvg), &bytes, &mime, &error));
  CHECK(mime == QStringLiteral("image/svg+xml"));
  CHECK(bytes.startsWith("<svg "));
  CHECK(guideDataUrl(QStringLiteral("data:text/plain;base64,aGVsbG8="), &bytes, &mime, &error));
  CHECK(bytes == "hello");
  CHECK(!guideDataUrl(QStringLiteral("data:image/png;base64,***"), &bytes, &mime, &error));
  CHECK(!guideDataUrl(QStringLiteral("data:image/png"), &bytes, &mime, &error));
  CHECK(!guideDataUrl(QStringLiteral("images/a.png"), &bytes, &mime, &error));
}

void readDemonstration() {
  reads = 0;
  QJsonObject given = json(R"({
    "label": "Click the row with the left button",
    "repeat": 2, "pause": 300,
    "frames": [
      {"image": "images/pointer.svg", "offset": [60, 40], "ms": 400},
      {"image": "images/pointer.svg", "move": true, "ms": 600},
      {"image": "SVG", "size": [40, 20], "anchor": [0.5, 0.5], "ms": 500}
    ]})");
  QJsonArray frames = given.value(QStringLiteral("frames")).toArray();
  QJsonObject third = frames.at(2).toObject();
  third.insert(QStringLiteral("image"), QString::fromLatin1(kSvg));
  frames.replace(2, third);
  given.insert(QStringLiteral("frames"), frames);
  QString error;
  const GuideDemonstration demonstration = GuideDemonstration::parse(given, loader, 2.0, &error);
  CHECK(error.isEmpty());
  CHECK(demonstration.frames.size() == 3);
  CHECK(demonstration.repeat == 2);
  CHECK(demonstration.pauseMs == 300);
  CHECK(demonstration.label == QStringLiteral("Click the row with the left button"));
  CHECK(demonstration.cycleMs() == 1500);
  // The same picture is read once.
  CHECK(reads == 1);
  // Its own size, at the pixel ratio; or the size asked for.
  const GuideDemonstrationFrame& pointer = demonstration.frames.at(0);
  CHECK(pointer.size == QSizeF(16, 24));
  CHECK(pointer.image.size() == QSize(32, 48));
  CHECK(pointer.image.devicePixelRatio() == 2.0);
  CHECK(demonstration.frames.at(2).size == QSizeF(40, 20));
  CHECK(demonstration.frames.at(2).image.size() == QSize(80, 40));
  // The SVG's red is drawn.
  CHECK(demonstration.frames.at(2).image.pixelColor(40, 20).red() > 200);

  // Where the frames are over a target at (100, 200), 200 x 20.
  const QRectF target(100, 200, 200, 20);
  const QPointF centre(200, 210);
  GuideDemonstration::Pose pose = demonstration.poseAt(0, target);
  CHECK(pose.frame == 0 && !pose.ended);
  CHECK(near(pose.point, centre + QPointF(60, 40)));
  // Half way through the glide: half way there, eased.
  pose = demonstration.poseAt(700, target);
  CHECK(pose.frame == 1);
  CHECK(near(pose.point, centre + QPointF(30, 20)));
  pose = demonstration.poseAt(1200, target);
  CHECK(pose.frame == 2);
  CHECK(near(pose.point, centre));
  // Drawn with its middle there.
  CHECK(demonstration.frameRect(2, centre) == QRectF(180, 200, 40, 20));
  CHECK(demonstration.frameRect(0, centre) == QRectF(200, 210, 16, 24));
  // The pause between the two plays, the second play, the end (the last
  // pause is not waited for).
  pose = demonstration.poseAt(1600, target);
  CHECK(pose.frame == -1 && !pose.ended);
  pose = demonstration.poseAt(1900, target);
  CHECK(pose.frame == 0 && !pose.ended);
  CHECK(demonstration.poseAt(3299, target).frame == 2);
  CHECK(demonstration.poseAt(3300, target).ended);
  // The target moved (the window resized, a pane moved): the frames follow.
  CHECK(near(demonstration.poseAt(1200, target.translated(-50, 30)).point, centre + QPointF(-50, 30)));

  // Until stopped.
  QString unused;
  const GuideDemonstration endless = GuideDemonstration::parse(
      json(QByteArray("{\"repeat\": 0, \"frames\": [{\"image\": \"") + pngAddress().toLatin1() + "\"}]}"), loader, 1.0,
      &unused);
  CHECK(unused.isEmpty());
  CHECK(endless.frames.size() == 1);
  CHECK(endless.frames.at(0).size == QSizeF(4, 4));
  CHECK(endless.frames.at(0).ms == 500);
  CHECK(!endless.poseAt(1000000, target).ended);
  CHECK(endless.poseAt(1000000, target).frame == 0);
}

void refusals() {
  QString error;
  const auto refused = [&error](const QByteArray& text, const char* reason) {
    const GuideDemonstration demonstration = GuideDemonstration::parse(json(text), loader, 1.0, &error);
    return demonstration.frames.isEmpty() && error.contains(QString::fromLatin1(reason));
  };
  CHECK(refused(R"({})", "\"frames\""));
  CHECK(refused(R"({"frames": []})", "\"frames\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg"}], "speed": 2})", "\"speed\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "colour": "red"}]})", "\"colour\""));
  CHECK(refused(R"({"frames": [{}]})", "no \"image\""));
  CHECK(refused(R"({"frames": [{"image": "images/none.png"}]})", "no images/none.png"));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "ms": 0}]})", "\"ms\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "ms": 1.5}]})", "\"ms\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "at": [0.5]}]})", "\"at\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "anchor": [2, 0]}]})", "\"anchor\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "size": [0, 10]}]})", "\"size\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg", "move": "yes"}]})", "\"move\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg"}], "repeat": -1})", "\"repeat\""));
  CHECK(refused(R"({"frames": [{"image": "images/pointer.svg"}], "label": 3})", "\"label\""));
  CHECK(refused(R"({"frames": [{"image": "data:image/png;base64,AAAA"}]})", "could not be read"));
  CHECK(refused(R"({"frames": [{"image": "data:image/svg+xml,%3Csvg"}]})", "SVG"));
}

QJsonObject gesture(const char* input, QStringList modifiers) {
  QJsonArray names;
  for (const QString& name : modifiers) {
    names.append(name);
  }
  return {{QStringLiteral("input"), QLatin1String(input)}, {QStringLiteral("modifiers"), names}};
}

void bindings() {
  NavigationSettings settings;
  settings.scheme = NavigationScheme::MiddlePan;
  QJsonObject gestures = guideNavigationJson(settings);
  CHECK(gestures.value(QStringLiteral("orbit")).toArray() == QJsonArray({gesture("middle", {QStringLiteral("shift")})}));
  CHECK(gestures.value(QStringLiteral("pan")).toArray() == QJsonArray({gesture("middle", {})}));
  // Every scroll zooms: the plain one stands for them.
  CHECK(gestures.value(QStringLiteral("zoom")).toArray() == QJsonArray({gesture("scroll", {})}));
  CHECK(!gestures.value(QStringLiteral("reverse_zoom")).toBool());
  CHECK(!gestures.value(QStringLiteral("custom")).toBool());

  settings.scheme = NavigationScheme::AltButtons;
  settings.reverseZoom = true;
  gestures = guideNavigationJson(settings);
  CHECK(gestures.value(QStringLiteral("orbit")).toArray() == QJsonArray({gesture("left", {QStringLiteral("alt")})}));
  // The middle pans with or without Alt: the plain one.
  CHECK(gestures.value(QStringLiteral("pan")).toArray() == QJsonArray({gesture("middle", {})}));
  CHECK(gestures.value(QStringLiteral("zoom")).toArray() ==
        QJsonArray({gesture("right", {QStringLiteral("alt")}), gesture("scroll", {})}));
  CHECK(gestures.value(QStringLiteral("reverse_zoom")).toBool());

  // F4 held with the left button, after the buttons.
  settings.scheme = NavigationScheme::MiddlePanF4;
  QJsonObject f4 = gesture("left", {});
  f4.insert(QStringLiteral("key"), QStringLiteral("F4"));
  CHECK(guideNavigationJson(settings).value(QStringLiteral("orbit")).toArray() ==
        QJsonArray({gesture("middle", {QStringLiteral("shift")}), f4}));

  settings.scheme = NavigationScheme::Trackpad;
  CHECK(guideNavigationJson(settings).value(QStringLiteral("orbit")).toArray() ==
        QJsonArray({gesture("left", {QStringLiteral("alt")}), gesture("middle", {QStringLiteral("shift")}),
                    gesture("right", {}), gesture("scroll", {QStringLiteral("alt")})}));

  // A custom table: what the learner set.
  settings.scheme = NavigationScheme::Custom;
  settings.customBindings = navigationBindings(NavigationScheme::Custom);
  for (NavigationBinding& binding : settings.customBindings) {
    if (binding.input == NavigationInput::RightButton && binding.modifiers == Qt::ControlModifier && !binding.f4) {
      binding.action = NavigationAction::Orbit;
    }
  }
  gestures = guideNavigationJson(settings);
  CHECK(gestures.value(QStringLiteral("orbit")).toArray() == QJsonArray({gesture("right", {QStringLiteral("ctrl")})}));
  CHECK(gestures.value(QStringLiteral("pan")).toArray().isEmpty());
  CHECK(gestures.value(QStringLiteral("custom")).toBool());

  // A command's keys; none is an empty list.
  const QJsonArray keys =
      guideShortcutsJson({QKeySequence(QStringLiteral("Ctrl+Shift+E")), QKeySequence(QStringLiteral("1"))});
  CHECK(keys.size() == 2);
  const QJsonArray first = keys.at(0).toObject().value(QStringLiteral("keys")).toArray();
  CHECK(first.size() == 1);
  CHECK(first.at(0).toObject().value(QStringLiteral("key")).toString() == QStringLiteral("E"));
  CHECK(first.at(0).toObject().value(QStringLiteral("modifiers")).toArray() ==
        QJsonArray({QStringLiteral("shift"), QStringLiteral("ctrl")}));
  CHECK(!keys.at(0).toObject().value(QStringLiteral("text")).toString().isEmpty());
  const QJsonObject second = keys.at(1).toObject().value(QStringLiteral("keys")).toArray().at(0).toObject();
  CHECK(second.value(QStringLiteral("key")).toString() == QStringLiteral("1"));
  CHECK(guideShortcutsJson({}).isEmpty());
  CHECK(guideShortcutsJson({QKeySequence()}).isEmpty());
}

// A gesture of the device lists (mitcad#2439).
QJsonObject onDevice(const char* device, const char* input, QStringList modifiers, int fingers = 0) {
  QJsonObject result = gesture(input, std::move(modifiers));
  result.insert(QStringLiteral("device"), QLatin1String(device));
  if (fingers > 0) {
    result.insert(QStringLiteral("fingers"), fingers);
  }
  return result;
}

QJsonArray deviceGestures(const QJsonObject& navigation, const char* device, const char* action) {
  return navigation.value(QLatin1String(device)).toObject().value(QLatin1String(action)).toArray();
}

// The mouse's and the trackpad's gestures each on its own (mitcad#2439,
// GitHub mitcad/Mitcad#627) for each kind of scheme: the wheel and the
// pinch zoom in every scheme, the two-finger slide as the scheme's table
// has it, the pad pressed and moved as the left button.
void deviceBindings() {
  const QString shift = QStringLiteral("shift");
  const QString ctrl = QStringLiteral("ctrl");
  const QString alt = QStringLiteral("alt");
  const QJsonObject wheel = onDevice("mouse", "wheel", {});
  const QJsonObject pinch = onDevice("trackpad", "pinch", {}, 2);
  NavigationSettings settings;

  // The middle button's scheme: every slide zooms; no trackpad gesture
  // orbits or pans.
  settings.scheme = NavigationScheme::MiddlePan;
  QJsonObject navigation = guideNavigationJson(settings);
  CHECK(deviceGestures(navigation, "mouse", "orbit") == QJsonArray({onDevice("mouse", "middle", {shift})}));
  CHECK(deviceGestures(navigation, "mouse", "pan") == QJsonArray({onDevice("mouse", "middle", {})}));
  CHECK(deviceGestures(navigation, "mouse", "zoom") == QJsonArray({wheel}));
  CHECK(deviceGestures(navigation, "trackpad", "orbit").isEmpty());
  CHECK(deviceGestures(navigation, "trackpad", "pan").isEmpty());
  CHECK(deviceGestures(navigation, "trackpad", "zoom") == QJsonArray({onDevice("trackpad", "slide", {}, 2), pinch}));
  // The first form stays as it was.
  CHECK(navigation.value(QStringLiteral("zoom")).toArray() == QJsonArray({gesture("scroll", {})}));
#if defined(Q_OS_MACOS)
  CHECK(navigation.value(QStringLiteral("platform")).toString() == QStringLiteral("macos"));
#elif defined(Q_OS_WIN)
  CHECK(navigation.value(QStringLiteral("platform")).toString() == QStringLiteral("windows"));
#else
  CHECK(navigation.value(QStringLiteral("platform")).toString() == QStringLiteral("linux"));
#endif
  CHECK(navigation.value(QStringLiteral("platform")).toString() == guidePlatformName());

  // Alt with the buttons: the pad pressed with Alt orbits.
  settings.scheme = NavigationScheme::AltButtons;
  navigation = guideNavigationJson(settings);
  CHECK(deviceGestures(navigation, "mouse", "orbit") == QJsonArray({onDevice("mouse", "left", {alt})}));
  CHECK(deviceGestures(navigation, "mouse", "zoom") == QJsonArray({onDevice("mouse", "right", {alt}), wheel}));
  CHECK(deviceGestures(navigation, "trackpad", "orbit") == QJsonArray({onDevice("trackpad", "drag", {alt}, 1)}));
  CHECK(deviceGestures(navigation, "trackpad", "pan").isEmpty());

  // The trackpad scheme: the plain slide pans, with Alt (Option) it
  // orbits, with Shift or Ctrl (Command) it zooms; the mouse's wheel still
  // zooms with any key.
  settings.scheme = NavigationScheme::Trackpad;
  navigation = guideNavigationJson(settings);
  CHECK(deviceGestures(navigation, "trackpad", "orbit") ==
        QJsonArray({onDevice("trackpad", "slide", {alt}, 2), onDevice("trackpad", "drag", {alt}, 1)}));
  CHECK(deviceGestures(navigation, "trackpad", "pan") ==
        QJsonArray({onDevice("trackpad", "slide", {}, 2), onDevice("trackpad", "drag", {shift, alt}, 1)}));
  CHECK(deviceGestures(navigation, "trackpad", "zoom") ==
        QJsonArray({onDevice("trackpad", "slide", {shift}, 2), onDevice("trackpad", "slide", {ctrl}, 2), pinch}));
  CHECK(deviceGestures(navigation, "mouse", "orbit") ==
        QJsonArray({onDevice("mouse", "left", {alt}), onDevice("mouse", "middle", {shift}), onDevice("mouse", "right", {})}));
  CHECK(deviceGestures(navigation, "mouse", "pan") ==
        QJsonArray({onDevice("mouse", "left", {shift, alt}), onDevice("mouse", "middle", {}), onDevice("mouse", "right", {shift})}));
  CHECK(deviceGestures(navigation, "mouse", "zoom") == QJsonArray({wheel}));

  // Shift orbits with the slide, Ctrl or Alt zooms.
  settings.scheme = NavigationScheme::TrackpadShift;
  navigation = guideNavigationJson(settings);
  CHECK(deviceGestures(navigation, "trackpad", "orbit") ==
        QJsonArray({onDevice("trackpad", "slide", {shift}, 2), onDevice("trackpad", "drag", {alt}, 1)}));
  CHECK(deviceGestures(navigation, "trackpad", "zoom") ==
        QJsonArray({onDevice("trackpad", "slide", {ctrl}, 2), onDevice("trackpad", "slide", {alt}, 2), pinch}));

  // F4 held with the left button, on either device.
  settings.scheme = NavigationScheme::MiddlePanF4;
  navigation = guideNavigationJson(settings);
  QJsonObject f4 = onDevice("mouse", "left", {});
  f4.insert(QStringLiteral("key"), QStringLiteral("F4"));
  CHECK(deviceGestures(navigation, "mouse", "orbit") == QJsonArray({onDevice("mouse", "middle", {shift}), f4}));
  QJsonObject padF4 = onDevice("trackpad", "drag", {}, 1);
  padF4.insert(QStringLiteral("key"), QStringLiteral("F4"));
  CHECK(deviceGestures(navigation, "trackpad", "orbit") == QJsonArray({padF4}));

  // A custom table: the learner's slide with Ctrl orbits; the table starts
  // with every slide zooming (navigationBindings(Custom)), so the plain one
  // still zooms and stands for the other keys.
  settings.scheme = NavigationScheme::Custom;
  settings.customBindings = navigationBindings(NavigationScheme::Custom);
  for (NavigationBinding& binding : settings.customBindings) {
    if (binding.input == NavigationInput::TrackpadScroll && binding.modifiers == Qt::ControlModifier) {
      binding.action = NavigationAction::Orbit;
    }
  }
  navigation = guideNavigationJson(settings);
  CHECK(deviceGestures(navigation, "trackpad", "orbit") == QJsonArray({onDevice("trackpad", "slide", {ctrl}, 2)}));
  CHECK(deviceGestures(navigation, "trackpad", "pan").isEmpty());
  CHECK(deviceGestures(navigation, "trackpad", "zoom") == QJsonArray({onDevice("trackpad", "slide", {}, 2), pinch}));
  CHECK(deviceGestures(navigation, "mouse", "orbit").isEmpty());
  CHECK(deviceGestures(navigation, "mouse", "zoom") == QJsonArray({wheel}));
}

// The test's own settings: the registry saves the keys there.
struct OwnSettings {
  QTemporaryDir directory;
  QString organization = QCoreApplication::organizationName();
  QString application = QCoreApplication::applicationName();
  QSettings::Format format = QSettings::defaultFormat();
  OwnSettings() {
    QCoreApplication::setOrganizationName(QStringLiteral("MitcadGuideBindingsTest"));
    QCoreApplication::setApplicationName(QStringLiteral("MitcadGuideBindingsTest"));
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  ~OwnSettings() {
    QCoreApplication::setOrganizationName(organization);
    QCoreApplication::setApplicationName(application);
    QSettings::setDefaultFormat(format);
  }
  OwnSettings(const OwnSettings&) = delete;
  OwnSettings& operator=(const OwnSettings&) = delete;
};

CommandDef command(const char* id, const char* name, const QKeySequence& key) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = QString::fromLatin1(name);
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Model;
  def.shortcut = key;
  def.icon = QStringLiteral("extrude");
  return def;
}

// The bridge's `shortcut` answer (mitcad#2439): the command's key now, a
// key the learner changes in the next answer with no restart, none as an
// empty list, an unknown command refused in words.
void shortcutAnswers() {
  OwnSettings own;
  CHECK(own.directory.isValid());
  CommandRegistry registry;
  registry.add(command("view.front", "Front View", QKeySequence(Qt::Key_1)));
  registry.add(command("x.none", "No Key", QKeySequence()));
  const auto keysOf = [](const QJsonObject& answer) {
    QJsonArray keys;
    for (const QJsonValue& shortcut : answer.value(QStringLiteral("shortcuts")).toArray()) {
      keys.append(shortcut.toObject().value(QStringLiteral("keys")));
    }
    return keys;
  };
  // One shortcut of one key, as keysOf lists it: [[{key, modifiers}]].
  // Built by append: QJsonArray({inner}) with a QJsonArray inner is a copy
  // of inner under MSVC, not a list of one (0.74.0-rc3's app.unit).
  const auto oneKey = [](const char* name, QStringList modifiers) {
    QJsonArray names;
    for (const QString& modifier : modifiers) {
      names.append(modifier);
    }
    QJsonArray keys;
    keys.append(QJsonObject{{QStringLiteral("key"), QLatin1String(name)}, {QStringLiteral("modifiers"), names}});
    QJsonArray shortcuts;
    shortcuts.append(keys);
    return shortcuts;
  };

  QJsonObject answer;
  QString error;
  CHECK(guideShortcutAnswer(registry, QStringLiteral("view.front"), &answer, &error));
  CHECK(answer.value(QStringLiteral("command")).toString() == QStringLiteral("view.front"));
  CHECK(answer.value(QStringLiteral("name")).toString() == QStringLiteral("Front View"));
  CHECK(answer.value(QStringLiteral("platform")).toString() == guidePlatformName());
  CHECK_JSON(keysOf(answer), oneKey("1", {}));

  // Changed in Keyboard Shortcuts: the next answer has it.
  registry.setShortcut(QStringLiteral("view.front"), QKeySequence(QStringLiteral("Ctrl+Shift+F")));
  CHECK(guideShortcutAnswer(registry, QStringLiteral("view.front"), &answer, &error));
  CHECK_JSON(keysOf(answer), oneKey("F", {QStringLiteral("shift"), QStringLiteral("ctrl")}));
  // Cleared: no key, the page names the command instead.
  registry.setShortcut(QStringLiteral("view.front"), QKeySequence());
  CHECK(guideShortcutAnswer(registry, QStringLiteral("view.front"), &answer, &error));
  CHECK(answer.value(QStringLiteral("shortcuts")).toArray().isEmpty());
  // Back to its default.
  registry.setShortcut(QStringLiteral("view.front"), QKeySequence(Qt::Key_1));
  CHECK(guideShortcutAnswer(registry, QStringLiteral("view.front"), &answer, &error));
  CHECK_JSON(keysOf(answer), oneKey("1", {}));

  CHECK(guideShortcutAnswer(registry, QStringLiteral("x.none"), &answer, &error));
  CHECK(answer.value(QStringLiteral("shortcuts")).toArray().isEmpty());

  answer = {};
  CHECK(!guideShortcutAnswer(registry, QStringLiteral("no.such"), &answer, &error));
  CHECK(error == QStringLiteral("MitCad has no command no.such"));
  CHECK(answer.isEmpty());
}

} // namespace

int guideDemonstrationTests() {
  failures = 0;
  dataAddresses();
  readDemonstration();
  refusals();
  bindings();
  deviceBindings();
  shortcutAnswers();
  return failures;
}
MITCAD_UNIT_TEST(guideDemonstrationTests);
