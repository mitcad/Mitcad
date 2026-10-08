// SPDX-License-Identifier: MIT
// Unit tests of the application's own helpers, without windows (ctest runs
// them on Qt's offscreen platform). Plain checks, as in the geometry tests.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include <QApplication>
#include <QColor>
#include <QCursor>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPixmap>
#include <QString>
#include <QTemporaryDir>

#include "files/Print3d.hpp"
#include "framework/Cursors.hpp"
#include "framework/DesktopEntry.hpp"
#include "framework/Numbers.hpp"
#include "render/RenderDevices.hpp"
#include "render/RenderImage.hpp"
#include "report/ReportText.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_app.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// Numbers typed with a decimal comma or point (mitcad#2).
void numberTests() {
  using mitcad::parsePlainNumber;
  CHECK(parsePlainNumber(QStringLiteral("12.5")) == 12.5);
  CHECK(parsePlainNumber(QStringLiteral("12,5")) == 12.5);
  CHECK(parsePlainNumber(QStringLiteral(" ,5 ")) == 0.5);
  CHECK(parsePlainNumber(QStringLiteral("5,")) == 5.0);
  CHECK(parsePlainNumber(QStringLiteral("-2,5e3")) == -2500.0);
  CHECK(parsePlainNumber(QStringLiteral("20")) == 20.0);
  // No thousands separators: as the model reads it.
  CHECK(parsePlainNumber(QStringLiteral("1,000")) == 1.0);
  // Expressions and the like are no plain numbers.
  for (const char* text : {"", "d1 * 2", "20 mm", "1,5 mm", "1,000.5", "1,2,3", "max(1, 5)", "inf", "nan"}) {
    if (parsePlainNumber(QString::fromLatin1(text))) {
      std::fprintf(stderr, "'%s' is no plain number\n", text);
      ++failures;
    }
  }
  QString normalized;
  CHECK(parsePlainNumber(QStringLiteral(" 12,5"), &normalized) && normalized == QStringLiteral("12.5"));

  mitcad::DecimalSpinBox box;
  box.setRange(0.0, 1000.0);
  box.setDecimals(2);
  box.setSuffix(QStringLiteral(" mm"));
  QString typed = QStringLiteral("12,5 mm");
  int position = 4;
  CHECK(box.validate(typed, position) == QValidator::Acceptable);
  CHECK(typed == QStringLiteral("12.5 mm"));
  CHECK(box.valueFromText(QStringLiteral("12,5 mm")) == 12.5);
  CHECK(box.valueFromText(QStringLiteral("12.5 mm")) == 12.5);
  CHECK(box.textFromValue(1234.5) == QStringLiteral("1234.50"));
}

// Pixels of `image` in `rect` that are not transparent.
int opaquePixels(const QImage& image, const QRect& rect) {
  int count = 0;
  const QRect inside = rect.intersected(image.rect());
  for (int y = inside.top(); y <= inside.bottom(); ++y) {
    for (int x = inside.left(); x <= inside.right(); ++x) {
      count += image.pixelColor(x, y).alpha() > 0 ? 1 : 0;
    }
  }
  return count;
}

bool closeTo(double a, double b) { return std::abs(a - b) <= 1.0; }

// The sketch tools' precision cursor (mitcad#3), at 100, 150 and 200 %.
void cursorTests() {
  const QString line = QStringLiteral("line");
  for (const qreal dpr : {1.0, 1.5, 2.0}) {
    std::fprintf(stderr, "cursor at ratio %g\n", dpr);
    const mitcad::PrecisionCursorLayout layout = mitcad::precisionCursorLayout(dpr);
    const QPixmap pixmap = mitcad::precisionCursorPixmap(line, dpr, false);
    const QImage image = pixmap.toImage().convertToFormat(QImage::Format_ARGB32);
    // 32 x 32 logical pixels, the hot spot in the middle of the cross.
    CHECK(pixmap.devicePixelRatio() == dpr);
    CHECK(pixmap.width() == layout.size && pixmap.height() == layout.size);
    CHECK(layout.size == qRound(32 * dpr));
    CHECK(pixmap.deviceIndependentSize() == QSizeF(32, 32));
    CHECK(layout.hotSpot == QPoint(10, 10));
    const QPoint hot(qRound(layout.hotSpot.x() * dpr), qRound(layout.hotSpot.y() * dpr));
    CHECK(layout.gap.contains(hot) && layout.arms.contains(hot));
    // The cross: 19 logical pixels of arms, 21 with the halo, around the
    // hot spot; the 5 px gap in its middle is empty.
    CHECK(closeTo(layout.arms.width(), 21 * dpr) && layout.arms.height() == layout.arms.width());
    CHECK(closeTo(layout.arms.center().x(), hot.x()) && closeTo(layout.arms.center().y(), hot.y()));
    CHECK(closeTo(layout.gap.width(), 5 * dpr) && layout.gap.height() == layout.gap.width());
    CHECK(closeTo(layout.gap.center().x(), hot.x()) && closeTo(layout.gap.center().y(), hot.y()));
    CHECK(opaquePixels(image, layout.gap) == 0);
    // Whole pixels: a dark arm, its light halo beside it, nothing between.
    const int width = std::max(1, static_cast<int>(dpr)); // the arms' width
    const QColor arm = image.pixelColor(layout.arms.left() + width, hot.y());
    const QColor halo = image.pixelColor(layout.arms.left() + width, hot.y() - 1);
    CHECK(arm == QColor(0x1b, 0x1e, 0x23));
    CHECK(halo == QColor(255, 255, 255, 217));
    CHECK(image.pixelColor(layout.arms.left(), hot.y()) == halo);
    CHECK(image.pixelColor(layout.arms.left() + width, hot.y() + width) == halo);
    CHECK(image.pixelColor(layout.arms.left() + width, hot.y() - width - 1).alpha() == 0);
    // The badge: 12 px, 10 px right of and below the hot spot, clear of
    // the gap and the arms, the tool's icon in it.
    CHECK(closeTo(layout.badge.width(), 12 * dpr) && layout.badge.height() == layout.badge.width());
    CHECK(closeTo(layout.badge.left() - hot.x(), 10 * dpr) && closeTo(layout.badge.top() - hot.y(), 10 * dpr));
    CHECK(!layout.badge.intersects(layout.gap));
    CHECK(layout.badge.left() > hot.x() + 2 * width && layout.badge.top() > hot.y() + 2 * width);
    CHECK(image.rect().contains(layout.badge));
    CHECK(opaquePixels(image, layout.badge) > layout.badge.width() * layout.badge.height() / 2);
    const QImage bare = mitcad::precisionCursorPixmap(QString(), dpr, false).toImage();
    CHECK(opaquePixels(bare, layout.badge) == 0);
    // Snapping: a hollow square in the gap; the pick point stays visible.
    const QImage snapped =
        mitcad::precisionCursorPixmap(line, dpr, true).toImage().convertToFormat(QImage::Format_ARGB32);
    CHECK(snapped.pixelColor(layout.gap.topLeft()) == arm);
    CHECK(snapped.pixelColor(layout.gap.bottomRight()) == arm);
    CHECK(snapped.pixelColor(hot).alpha() == 0);
    CHECK(opaquePixels(snapped, layout.gap.adjusted(width, width, -width, -width)) == 0);
    // The cursor: a bitmap cursor with that hot spot, built once.
    const QCursor cursor = mitcad::precisionCursor(line, dpr);
    CHECK(cursor.shape() == Qt::BitmapCursor);
    CHECK(cursor.hotSpot() == mitcad::precisionCursorLayout(dpr, mitcad::pointerScale()).hotSpot);
    CHECK(cursor.pixmap().cacheKey() == mitcad::precisionCursor(line, dpr).pixmap().cacheKey());
    CHECK(cursor.pixmap().cacheKey() != mitcad::precisionCursor(line, dpr, true).pixmap().cacheKey());
  }
  // A larger system pointer makes a larger cursor.
  const mitcad::PrecisionCursorLayout large = mitcad::precisionCursorLayout(1.0, 1.5);
  CHECK(large.size == 48 && large.hotSpot == QPoint(15, 15));
  CHECK(mitcad::pointerScale() >= 1.0 && mitcad::pointerScale() <= 2.0);
}

// An empty file, executable.
void touch(const QString& path) {
  QDir().mkpath(QFileInfo(path).path());
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write("#!/bin/sh\n");
  file.close();
  file.setPermissions(file.permissions() | QFile::ExeOwner | QFile::ExeUser);
}

// 3D Print (mitcad#13): the slicers found in their install folders, on
// PATH, as Flatpaks and in desktop files; file and folder names.
void printTests() {
  using mitcad::Slicer;
  QTemporaryDir root;
  CHECK(root.isValid());
  const QString dir = root.path();
  // Install folders (Windows' layout; the search does not care where).
  touch(dir + QStringLiteral("/programs/Bambu Studio/bambu-studio.exe"));
  touch(dir + QStringLiteral("/programs/Prusa3D/PrusaSlicer/prusa-slicer.exe"));
  touch(dir + QStringLiteral("/programs/UltiMaker Cura 5.7.0/UltiMaker-Cura.exe"));
  touch(dir + QStringLiteral("/programs/UltiMaker Cura 5.8.1/UltiMaker-Cura.exe"));
  // PATH, a Flatpak and the desktop file of an AppImage.
#ifdef _WIN32
  touch(dir + QStringLiteral("/bin/orca-slicer.exe"));
  touch(dir + QStringLiteral("/bin/flatpak.exe"));
#else
  touch(dir + QStringLiteral("/bin/orca-slicer"));
  touch(dir + QStringLiteral("/bin/flatpak"));
#endif
  QDir().mkpath(dir + QStringLiteral("/flatpak/com.bambulab.BambuStudio/current"));
  touch(dir + QStringLiteral("/apps/OrcaSlicer.AppImage"));
  QDir().mkpath(dir + QStringLiteral("/applications"));
  QFile desktop(dir + QStringLiteral("/applications/orca.desktop"));
  CHECK(desktop.open(QIODevice::WriteOnly));
  desktop.write(QStringLiteral("[Desktop Entry]\nName=OrcaSlicer\nExec=\"%1\" --single-instance %U\n"
                               "[Desktop Action New]\nName=Other\nExec=other\n")
                    .arg(dir + QStringLiteral("/apps/OrcaSlicer.AppImage"))
                    .toUtf8());
  desktop.close();
  mitcad::SlicerSearch search;
  search.programDirectories = {dir + QStringLiteral("/programs")};
  search.path = {dir + QStringLiteral("/bin")};
  search.flatpakDirectories = {dir + QStringLiteral("/flatpak")};
  search.applicationDirectories = {dir + QStringLiteral("/applications")};
  const QVector<Slicer> found = mitcad::findSlicers(search);
  QStringList described;
  for (const Slicer& slicer : found) {
    described << slicer.describe();
    std::fprintf(stderr, "slicer: %s\n", qPrintable(slicer.describe()));
  }
  CHECK(found.size() == 6);
  if (found.size() == 6) {
    const auto native = [&dir](const char* path) { return QDir::toNativeSeparators(dir + QLatin1String(path)); };
    CHECK(found[0].name == QStringLiteral("Bambu Studio"));
    CHECK(found[0].program == native("/programs/Bambu Studio/bambu-studio.exe"));
    CHECK(found[1].name == QStringLiteral("OrcaSlicer") && found[1].arguments.isEmpty());
    CHECK(found[2].program == native("/programs/Prusa3D/PrusaSlicer/prusa-slicer.exe"));
    // The newest version of a folder that has one.
    CHECK(found[3].program == native("/programs/UltiMaker Cura 5.8.1/UltiMaker-Cura.exe"));
    CHECK(found[4].name == QStringLiteral("Bambu Studio (Flatpak)"));
    CHECK(found[4].arguments == QStringList({QStringLiteral("run"), QStringLiteral("--file-forwarding"),
                                             QStringLiteral("com.bambulab.BambuStudio"), QStringLiteral("@@")}));
    CHECK(found[4].argumentsAfter == QStringList{QStringLiteral("@@")});
    CHECK(found[5].program == native("/apps/OrcaSlicer.AppImage"));
    CHECK(found[5].arguments == QStringList{QStringLiteral("--single-instance")});
  }
  CHECK(mitcad::findSlicers(mitcad::SlicerSearch{}).isEmpty());

  CHECK(mitcad::safeFileName(QStringLiteral("Body1")) == QStringLiteral("Body1"));
  CHECK(mitcad::safeFileName(QStringLiteral("a/b\\c:d*e?\"<>|")) == QStringLiteral("a_b_c_d_e_____"));
  CHECK(mitcad::safeFileName(QStringLiteral(" Lid. ")) == QStringLiteral("Lid"));
  CHECK(mitcad::safeFileName(QStringLiteral("con")) == QStringLiteral("_con"));
  CHECK(mitcad::safeFileName(QStringLiteral("COM1.part")) == QStringLiteral("_COM1.part"));
  CHECK(mitcad::safeFileName(QStringLiteral("Console")) == QStringLiteral("Console"));
  CHECK(mitcad::safeFileName(QStringLiteral("..")) == QStringLiteral("Body"));
  CHECK(mitcad::printFileNames({QStringLiteral("Body1"), QStringLiteral("body1"), QStringLiteral("Body1"),
                                QStringLiteral("Pin/2")},
                               QStringLiteral("stl")) ==
        QStringList({QStringLiteral("Body1.stl"), QStringLiteral("body1_2.stl"), QStringLiteral("Body1_3.stl"),
                     QStringLiteral("Pin_2.stl")}));

  // STL files (mitcad#17): a body placed once one file, a body shown twice
  // one per occurrence, a hidden placement left out unless none is shown.
  const auto instance = [](const char* body, QStringList path, const char* occurrence, bool visible) {
    return QJsonObject{{QStringLiteral("body"), QString::fromLatin1(body)},
                       {QStringLiteral("path"), QJsonArray::fromStringList(path)},
                       {QStringLiteral("occurrence"), QString::fromLatin1(occurrence)},
                       {QStringLiteral("visible"), visible}};
  };
  const QJsonArray instances{instance("F1.b0", {}, "", true),
                             instance("F2.b0", {QStringLiteral("O1")}, "Arm:1", true),
                             instance("F2.b0", {QStringLiteral("O2"), QStringLiteral("O4")}, "Base:1/Arm:2", true),
                             instance("F2.b0", {QStringLiteral("O3")}, "Arm:3", false),
                             instance("F3.b0", {QStringLiteral("O5")}, "Lid:1", false),
                             instance("F3.b0", {QStringLiteral("O6")}, "Lid:2", false)};
  const QVector<mitcad::PrintPiece> pieces =
      mitcad::printPieces({QStringLiteral("F1.b0"), QStringLiteral("F2.b0"), QStringLiteral("F3.b0")},
                          {QStringLiteral("Plate"), QStringLiteral("Pin"), QStringLiteral("Lid")}, instances);
  QStringList files;
  for (const mitcad::PrintPiece& piece : pieces) {
    files << QStringLiteral("%1 %2 %3").arg(piece.body, piece.occurrence, piece.name);
  }
  CHECK(files == QStringList({QStringLiteral("F1.b0  Plate"), QStringLiteral("F2.b0 O1 Pin (Arm:1)"),
                                  QStringLiteral("F2.b0 O2/O4 Pin (Base:1/Arm:2)"),
                                  QStringLiteral("F3.b0 O5 Lid (Lid:1)"), QStringLiteral("F3.b0 O6 Lid (Lid:2)")}));

  // The folder of a design, emptied for each send.
  QString error;
  const QString folder = mitcad::preparePrintFolder(dir, QStringLiteral("My: part"), &error);
  CHECK(folder == QDir(dir).filePath(QStringLiteral("mitcad-print/My_ part")));
  touch(folder + QStringLiteral("/old.stl"));
  CHECK(mitcad::preparePrintFolder(dir, QStringLiteral("My: part"), &error) == folder);
  CHECK(QDir(folder).isEmpty());
#ifndef _WIN32
  // A link put in the shared temporary folder decides nothing.
  QTemporaryDir shared;
  touch(dir + QStringLiteral("/kept/part/file.txt"));
  CHECK(QFile::link(dir + QStringLiteral("/kept"), shared.path() + QStringLiteral("/mitcad-print")));
  error.clear();
  CHECK(mitcad::preparePrintFolder(shared.path(), QStringLiteral("part"), &error).isEmpty());
  CHECK(error.contains(QStringLiteral("not a folder of yours")));
  CHECK(QFileInfo::exists(dir + QStringLiteral("/kept/part/file.txt")));
#endif
}

// The AppImage's desktop file and icons in the user's data folder.
void desktopEntryTests() {
  // Exec runs the AppImage, quoted, with the specification's escapes.
  const QString bundled = QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=mitcad\nIcon=mitcad\n");
  CHECK(mitcad::desktopEntryFor(bundled, QStringLiteral("/home/a b/Mitcad.AppImage")) ==
        QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=\"/home/a b/Mitcad.AppImage\"\nIcon=mitcad\n"));
  CHECK(mitcad::desktopEntryFor(bundled, QStringLiteral("/x/$a\"b")) ==
        QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=\"/x/\\\\$a\\\\\"b\"\nIcon=mitcad\n"));

  QTemporaryDir temp;
  const QString appDir = temp.filePath(QStringLiteral("AppDir"));
  const QString data = temp.filePath(QStringLiteral("share"));
  const auto write = [](const QString& path, const QByteArray& content) {
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile file(path);
    return file.open(QIODevice::WriteOnly) && file.write(content) == content.size();
  };
  CHECK(write(appDir + QStringLiteral("/usr/share/applications/mitcad.desktop"), bundled.toUtf8()));
  CHECK(write(appDir + QStringLiteral("/usr/share/icons/hicolor/16x16/apps/mitcad.png"), "16"));
  CHECK(write(appDir + QStringLiteral("/usr/share/icons/hicolor/256x256/apps/mitcad.png"), "256"));
  const auto read = [](const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
  };
  mitcad::DesktopIntegration result = mitcad::integrateAppImage(QStringLiteral("/apps/Mitcad.AppImage"), appDir, data);
  CHECK(result.error.isEmpty());
  CHECK(result.written.size() == 3);
  CHECK(result.written.last() == data + QStringLiteral("/applications/mitcad.desktop"));
  CHECK(read(data + QStringLiteral("/applications/mitcad.desktop")).contains("\nExec=\"/apps/Mitcad.AppImage\"\n"));
  CHECK(read(data + QStringLiteral("/icons/hicolor/256x256/apps/mitcad.png")) == "256");
  // Started again: nothing to write; the AppImage moved: only the desktop file.
  CHECK(mitcad::integrateAppImage(QStringLiteral("/apps/Mitcad.AppImage"), appDir, data).written.isEmpty());
  result = mitcad::integrateAppImage(QStringLiteral("/opt/Mitcad.AppImage"), appDir, data);
  CHECK(result.written == QStringList{data + QStringLiteral("/applications/mitcad.desktop")});
  // No AppDir: an error, nothing written.
  result = mitcad::integrateAppImage(QStringLiteral("/a"), temp.filePath(QStringLiteral("none")), data);
  CHECK(!result.error.isEmpty() && result.written.isEmpty());
}

// IEEE 754 binary16 of a float in the normal range (for frames).
std::uint16_t half(float value) {
  if (value == 0.0f) {
    return 0;
  }
  int exponent = 0;
  const float mantissa = std::frexp(value, &exponent); // value = mantissa * 2^exponent, 0.5 <= mantissa < 1
  const auto bits = static_cast<std::uint16_t>(std::lround((mantissa * 2.0f - 1.0f) * 1024.0f));
  return static_cast<std::uint16_t>(((exponent + 14) << 10) | bits);
}

// The rendered view's display (mitcad#47): exposure and view transforms
// change the render's light, not the background it is composited over.
void renderLookTests() {
  using mitcad::render::Look;
  using mitcad::render::ViewTransform;
  using mitcad::render::viewTransform;
  const auto near = [](float a, float b, float tolerance) { return std::abs(a - b) <= tolerance; };
  // Standard leaves the light as it is; the curves keep it in 0..1, keep
  // mid grey about where it was, and grow with the light up to white.
  CHECK(viewTransform({0.5f, 2.0f, 0.1f}, ViewTransform::Standard)[1] == 2.0f);
  for (const ViewTransform transform : {ViewTransform::Filmic, ViewTransform::Neutral}) {
    float last = -1.0f;
    for (const float light : {0.0f, 0.05f, 0.18f, 0.5f, 1.0f, 2.0f, 8.0f, 100.0f}) {
      const float shown = viewTransform({light, light, light}, transform)[0];
      CHECK((shown > last || (shown == last && shown > 0.99f)) && shown >= 0.0f && shown <= 1.0001f);
      last = shown;
    }
    CHECK(near(viewTransform({0.18f, 0.18f, 0.18f}, transform)[0], 0.16f, 0.03f));
  }
  // Neutral keeps a base colour's hue and values below its compression.
  const std::array<float, 3> red = viewTransform({0.6f, 0.1f, 0.05f}, ViewTransform::Neutral);
  CHECK(near(red[0], 0.56f, 0.01f) && near(red[1], 0.06f, 0.01f));
  // Filmic compresses white more than Neutral does.
  CHECK(viewTransform({1.0f, 1.0f, 1.0f}, ViewTransform::Filmic)[0] <
        viewTransform({1.0f, 1.0f, 1.0f}, ViewTransform::Neutral)[0]);

  // A frame of two pixels: a grey body (opaque) and a half shadow
  // (transparent black, alpha 0.5) over a white background.
  const std::uint16_t frame[8] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, half(0.5f)};
  const auto shown = [&frame](const Look& look) {
    const QImage image = mitcad::render::displayImage(frame, 2, 1, Qt::white, Qt::white, look);
    return std::make_pair(QColor(image.pixel(0, 0)), QColor(image.pixel(1, 0)));
  };
  const auto plain = shown(Look());
  CHECK(plain.first.red() == 124);  // sRGB of 0.2
  CHECK(plain.second.red() == 188); // white at half: sRGB of 0.5
  const auto brighter = shown(Look{1.0f, ViewTransform::Standard});
  CHECK(brighter.first.red() == 170); // 0.4
  CHECK(brighter.second == plain.second); // the shadow over the background stays
  const auto filmic = shown(Look{0.0f, ViewTransform::Filmic});
  CHECK(filmic.first.red() != plain.first.red() && filmic.second == plain.second);
}

// The final render's image (mitcad#48): opaque 8 bits as the view shows
// it, 16 bits the same colours finer, transparent with straight alpha.
void renderOutputTests() {
  using mitcad::render::Look;
  using mitcad::render::ViewTransform;
  using mitcad::render::outputImage;
  const std::uint16_t frame[8] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, half(0.5f)};
  const Look look{0.5f, ViewTransform::Filmic};
  const QImage shown = mitcad::render::displayImage(frame, 2, 1, Qt::white, Qt::white, look);
  const QImage opaque = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, look, false, false);
  CHECK(opaque == shown);
  const QImage deep = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, look, false, true);
  CHECK(deep.format() == QImage::Format_RGBX64);
  for (int x = 0; x < 2; ++x) {
    CHECK(std::abs(deep.pixelColor(x, 0).red() - shown.pixelColor(x, 0).red()) <= 1);
  }
  const QImage transparent = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, Look(), true, false);
  CHECK(transparent.hasAlphaChannel());
  CHECK(transparent.pixelColor(0, 0).red() == 124 && transparent.pixelColor(0, 0).alpha() == 255);
  // The shadow: black at half coverage, the background left out.
  CHECK(transparent.pixelColor(1, 0).red() == 0 && transparent.pixelColor(1, 0).alpha() == 128);
  const QImage transparentDeep = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, Look(), true, true);
  CHECK(transparentDeep.format() == QImage::Format_RGBA64 &&
        transparentDeep.pixel(1, 0) == transparent.pixel(1, 0));
}

// The ground's catcher factors (mitcad#54): the background behind the
// ground is multiplied by them in the view and the final render's
// images; a transparent image takes their mean darkening as alpha.
void renderCatcherTests() {
  using mitcad::render::Look;
  using mitcad::render::outputImage;
  // A grey body (opaque) and the bare ground with a red reflection
  // (factors 1, 0.5, 0.5) and a half shadow (0.5 all).
  const std::uint16_t frame[12] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, 0, 0, 0, 0, 0};
  const std::uint16_t catcher[12] = {half(0.1f), half(0.1f), half(0.1f), half(1.0f),
                                     half(1.0f), half(0.5f), half(0.5f), half(1.0f),
                                     half(0.5f), half(0.5f), half(0.5f), half(1.0f)};
  const QImage shown = mitcad::render::displayImage(frame, 3, 1, Qt::white, Qt::white, Look(), catcher);
  CHECK(shown.pixelColor(0, 0).red() == 124); // the body covers the ground: no factor
  CHECK(shown.pixelColor(1, 0).red() == 255 && shown.pixelColor(1, 0).green() == 188 &&
        shown.pixelColor(1, 0).blue() == 188);
  CHECK(shown.pixelColor(2, 0).red() == 188 && shown.pixelColor(2, 0).green() == 188);
  // Over a dark background the reflection tints it as much.
  const QImage dark = mitcad::render::displayImage(frame, 3, 1, QColor(60, 60, 60), QColor(60, 60, 60), Look(), catcher);
  CHECK(dark.pixelColor(1, 0).red() > dark.pixelColor(1, 0).green() + 10);
  CHECK(outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), false, false) == shown);
  const QImage deep = outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), false, true);
  CHECK(std::abs(deep.pixelColor(1, 0).green() - 188) <= 1 && deep.pixelColor(1, 0).red() == 255);
  // Transparent: black with the mean darkening as alpha (1/3, 1/2).
  const QImage transparent = outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), true, false);
  CHECK(transparent.pixelColor(1, 0).alpha() == 85 && transparent.pixelColor(1, 0).red() == 0);
  CHECK(transparent.pixelColor(2, 0).alpha() == 128 && transparent.pixelColor(0, 0).alpha() == 255);
}

// The render device's choice (mitcad#50): automatic prefers a GPU, an
// unknown one gives the CPU and says so.
void renderDeviceChoiceTests() {
  using mitcad::render::chooseDevice;
  using mitcad::render::DeviceInfo;
  const DeviceInfo cpu{"CPU", "CPU", "A processor", false};
  const DeviceInfo cuda{"CUDA_GPU_0000:07:00", "CUDA", "A GPU", false};
  const DeviceInfo hip{"HIP_GPU_0000:08:00", "HIP", "Another GPU", true};
  std::string message;
  CHECK(chooseDevice("auto", {cpu}, message).id == "CPU" && message.empty());
  CHECK(chooseDevice("", {cpu, hip, cuda}, message).id == cuda.id && message.empty());
  CHECK(chooseDevice("auto", {cpu, hip}, message).id == hip.id);
  CHECK(chooseDevice("cpu", {cpu, cuda}, message).id == "CPU" && message.empty());
  CHECK(chooseDevice("CPU", {cpu, cuda}, message).id == "CPU");
  CHECK(chooseDevice(hip.id, {cpu, cuda, hip}, message).id == hip.id && message.empty());
  CHECK(chooseDevice("hip", {cpu, cuda, hip}, message).id == hip.id);
  CHECK(chooseDevice("CUDA_gone", {cpu, hip}, message).id == "CPU" &&
        message == "There is no render device \"CUDA_gone\"; rendering on the CPU");
  // Without a list (none known yet) the CPU still is the choice.
  CHECK(chooseDevice("auto", {}, message).type == "CPU");
}

// A renderer that only records what it is given (the fallback's tests).
class FakeRenderer : public mitcad::render::Renderer {
public:
  FakeRenderer(mitcad::render::FrameSink& sink, mitcad::render::DeviceInfo device, std::string startError)
      : m_sink(sink), m_device(std::move(device)), m_error(std::move(startError)) {}
  std::string description() const override { return "fake on " + m_device.type; }
  mitcad::render::SceneChanges updateScene(const mitcad::render::SceneUpdate& update) override {
    meshes = static_cast<int>(update.meshes.size());
    bodies = static_cast<int>(update.bodies.size());
    return {};
  }
  void setView(const mitcad::render::ViewData& view) override {
    sequence = view.sequence;
    // A frame of the view.
    if (std::uint16_t* pixels = m_sink.beginFrame(1, 1, 1, 1, false).light) {
      pixels[0] = 0;
      m_sink.endFrame(view.sequence, 1, false);
    }
  }
  void setSamples(int count) override { samples = count; }
  void setPreviews(const std::vector<int>&) override {}
  std::string setEnvironment(const mitcad::render::EnvironmentData& environment) override {
    lights = static_cast<int>(environment.lights.size());
    return {};
  }
  void wait() override {}
  void setFinal(bool, double) override {}
  int currentSample() const override { return samples; }
  mitcad::render::DeviceInfo device() const override { return m_device; }
  std::string deviceError() const override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    return m_error;
  }
  void fail(const std::string& why) {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_error = why;
  }

  int meshes = 0;
  int bodies = 0;
  int lights = 0;
  std::uint64_t sequence = 0;
  int samples = 0;

private:
  mitcad::render::FrameSink& m_sink;
  mitcad::render::DeviceInfo m_device;
  mutable std::mutex m_mutex;
  std::string m_error;
};

class CountingSink : public mitcad::render::FrameSink {
public:
  mitcad::render::FrameBuffers beginFrame(int, int, int, int, bool) override { return {&m_pixel, nullptr}; }
  void endFrame(std::uint64_t, int, bool) override { ++frames; }
  void finished(std::uint64_t, int, double) override {}
  int frames = 0;

private:
  std::uint16_t m_pixel = 0;
};

// The CPU takes over from a device that cannot start or fails while it
// renders, with what the device had (mitcad#50).
void renderFallbackTests() {
  using mitcad::render::DeviceInfo;
  const DeviceInfo gpu{"CUDA_GPU", "CUDA", "A GPU", false};
  struct Made {
    std::vector<FakeRenderer*> renderers;
    std::vector<std::string> reports;
    std::mutex mutex;
  };
  const auto factoryFor = [](Made& made, std::string gpuStartError, bool cpuStarts) {
    return [&made, gpuStartError, cpuStarts](mitcad::render::FrameSink& sink, const DeviceInfo& device,
                                             std::string& error) -> std::unique_ptr<mitcad::render::Renderer> {
      const bool onCpu = device.id.empty() || device.type == "CPU";
      if (onCpu && !cpuStarts) {
        error = "no CPU";
        return nullptr;
      }
      DeviceInfo on = device;
      if (onCpu) {
        on = DeviceInfo{"CPU", "CPU", "A processor", false};
      }
      auto renderer = std::make_unique<FakeRenderer>(sink, on, onCpu ? std::string() : gpuStartError);
      made.renderers.push_back(renderer.get());
      return renderer;
    };
  };
  const auto reportTo = [](Made& made) {
    return [&made](const DeviceInfo&, const std::string& message) {
      const std::lock_guard<std::mutex> lock(made.mutex);
      made.reports.push_back(message);
    };
  };
  CountingSink sink;
  std::string error;
  {
    // The GPU cannot start: the CPU renders, and the report says why.
    Made made;
    auto renderer = mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, "no kernel", true), reportTo(made),
                                                       {}, error);
    CHECK(renderer && renderer->device().type == "CPU");
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A GPU (CUDA) failed (no kernel)");
  }
  {
    // The GPU fails while rendering: the CPU gets the whole scene (the
    // meshes of all updates not released, the last bodies), the view and
    // the samples, and the wait lasts until it has rendered them.
    Made made;
    auto renderer =
        mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, {}, true), reportTo(made), {}, error);
    CHECK(renderer && renderer->device().type == "CUDA" && made.reports.empty());
    mitcad::render::SceneUpdate first;
    first.meshes = {{"a", {}}, {"b", {}}};
    first.bodies.resize(2);
    mitcad::render::SceneUpdate second;
    second.meshes = {{"c", {}}};
    second.released = {"a"};
    second.bodies.resize(3);
    mitcad::render::ViewData view;
    view.sequence = 7;
    // The user's lights (mitcad#54) are part of the environment.
    mitcad::render::EnvironmentData environment;
    environment.lights.resize(2);
    environment.lights[1].kind = mitcad::render::LightData::Kind::Sun;
    renderer->setEnvironment(environment);
    renderer->setSamples(16);
    renderer->updateScene(first);
    renderer->updateScene(second);
    renderer->setView(view);
    made.renderers.front()->fail("out of memory");
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && made.renderers.size() == 2);
    const FakeRenderer* cpu = made.renderers.back();
    CHECK(cpu->samples == 16 && cpu->meshes == 2 && cpu->bodies == 3 && cpu->sequence == 7 && cpu->lights == 2);
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A GPU (CUDA) failed (out of memory)");
  }
  {
    // The test hook fails the first renderer, even on the CPU, after its
    // first frame.
    Made made;
    auto renderer = mitcad::render::makeDeviceRenderer(sink, DeviceInfo{"CPU", "CPU", "A processor", false},
                                                       factoryFor(made, {}, true), reportTo(made), "simulated", error);
    mitcad::render::ViewData view;
    view.sequence = 3;
    renderer->setView(view);
    renderer->wait();
    CHECK(made.renderers.size() == 2 && made.renderers.back()->sequence == 3);
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A processor failed (simulated)");
  }
  {
    // Neither starts.
    Made made;
    auto renderer =
        mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, "no kernel", false), reportTo(made), {}, error);
    CHECK(!renderer && error == "no kernel; the CPU: no CPU");
  }
}

// Feedback and error reports (mitcad#61, mitcad#62): masking, crash files,
// duplicate keys, the prefilled issue form.
void reportMaskTests() {
  using mitcad::report::mask;
  const mitcad::report::MaskContext alice{QStringLiteral("/home/alice"), QStringLiteral("alice"),
                                          QStringLiteral("alice-laptop.example.org")};
  const auto masked = [&alice](const char* text) { return mask(QString::fromUtf8(text), alice); };
  CHECK(masked("Could not open /home/alice/designs/bracket.f3d.") ==
        QStringLiteral("Could not open <path>/<file>.f3d."));
  CHECK(masked("/usr/lib/x86_64-linux-gnu/libc.so.6(+0x45330) [0x7f0012345678]") ==
        QStringLiteral("<path>/libc.so.6(+0x45330) [0x7f0012345678]"));
  CHECK(masked("/home/alice/src/build/app/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x55]") ==
        QStringLiteral("<path>/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x55]"));
  CHECK(masked("at /opt/src/geometry/src/guard.cpp:42") == QStringLiteral("at <path>/guard.cpp:42"));
  CHECK(masked("~/designs/plate.mitcad") == QStringLiteral("<path>/<file>.mitcad"));
  CHECK(masked("\\\\fileserver\\share\\plate.step") == QStringLiteral("<path>/<file>.step"));
  CHECK(masked("in /srv/projects/") == QStringLiteral("in <path>"));
  CHECK(masked("home is /home/alice") == QStringLiteral("home is <home>"));
  // Another user's home is a path like any other; the user's name stays a
  // word of its own.
  CHECK(masked("/home/alice2/x.f3d by alice2") == QStringLiteral("<path>/<file>.f3d by alice2"));
  CHECK(masked("user alice on alice-laptop.example.org (alice-laptop)") ==
        QStringLiteral("user <user> on <host> (<host>)"));
  CHECK(masked("ALICE wrote") == QStringLiteral("<user> wrote"));
  // Ordinary text, numbers, units and addresses stay.
  CHECK(masked("Mitcad 0.1.0: and/or 1/2 mm/s, 2026/10/08, https://example.com/a/b") ==
        QStringLiteral("Mitcad 0.1.0: and/or 1/2 mm/s, 2026/10/08, https://example.com/a/b"));
  // Windows: the home folder with either slash and any case.
  const mitcad::report::MaskContext jane{QStringLiteral("C:/Users/Jane Doe"), QStringLiteral("jdoe"),
                                         QStringLiteral("DESKTOP-1234")};
  CHECK(mask(QStringLiteral("C:\\Users\\Jane Doe\\Documents\\part.step"), jane) ==
        QStringLiteral("<path>/<file>.step"));
  CHECK(mask(QStringLiteral("c:/users/jane doe/AppData/Local/Mitcad/mitcad.exe"), jane) ==
        QStringLiteral("<path>/mitcad.exe"));
  CHECK(mask(QStringLiteral("D:\\Work\\gear.f3d on DESKTOP-1234 by jdoe"), jane) ==
        QStringLiteral("<path>/<file>.f3d on <host> by <user>"));
  // Names that would mask ordinary words are left to the path masking.
  const mitcad::report::MaskContext developer{QStringLiteral("/home/mitcad"), QStringLiteral("mitcad"),
                                              QStringLiteral("localhost")};
  CHECK(mask(QStringLiteral("Mitcad 0.1 on localhost: /home/mitcad/a.f3d"), developer) ==
        QStringLiteral("Mitcad 0.1 on localhost: <path>/<file>.f3d"));
}

void reportCrashTests() {
  using namespace mitcad::report;
  const QByteArray file =
      "Mitcad crash report 1\nprocess: import-worker\nversion: 0.1.0\nplatform: Linux 6.8.0 x86_64\npid: 4242\n"
      "parent: 4000\ntime: 1760000000\nsignal: SIGSEGV\naddress: 0x0\nmessage: \nstack:\n"
      "/opt/mitcad/bin/mitcad(_ZN6mitcad5crash6detail8onSignalEiP9siginfo_tPv+0x3c) [0x5555555a1000]\n"
      "/lib/x86_64-linux-gnu/libc.so.6(+0x45330) [0x7ffff7845330]\n"
      "/opt/mitcad/bin/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x5555555a2000]\n"
      "/opt/mitcad/bin/mitcad(main+0x55) [0x5555555a3000]\n"
      "/lib/x86_64-linux-gnu/libc.so.6(__libc_start_main+0x8b) [0x7ffff782a28b]\n"
      "actions:\ncommand help.send_feedback\nmodel add_feature\nend\n";
  const CrashReport report = parseCrashReport(file);
  CHECK(report.valid);
  CHECK(report.process == QStringLiteral("import-worker"));
  CHECK(report.parent == 4000 && report.pid == 4242 && report.time == 1760000000);
  CHECK(report.signal == QStringLiteral("SIGSEGV"));
  CHECK(report.stack.size() == 5);
  CHECK(report.actions == QStringList({QStringLiteral("command help.send_feedback"), QStringLiteral("model add_feature")}));
  CHECK(!parseCrashReport("something else\nsignal: SIGSEGV\n").valid);

  const Frame frame = parseFrame(report.stack[2]);
  CHECK(frame.module == QStringLiteral("mitcad"));
#if __has_include(<cxxabi.h>)
  CHECK(frame.symbol == QStringLiteral("mitcad::crash::crashNow()"));
  // The handler and the signal's trampoline are left out.
  CHECK(normalizedFrames(report.stack, 2) ==
        QStringList({QStringLiteral("mitcad!mitcad::crash::crashNow()"), QStringLiteral("mitcad!main")}));
#endif
  // The handler's frame is left out without a demangler too (mitcad#73).
  CHECK(trimmedStack(report.stack) == report.stack.mid(2));
  CHECK(parseFrame(QStringLiteral("C:\\Mitcad\\bin\\TKernel.dll+0x1a2b")).module == QStringLiteral("TKernel.dll"));
  CHECK(parseFrame(QStringLiteral("C:\\Mitcad\\bin\\TKernel.dll+0x1a2b")).offset == QStringLiteral("0x1a2b"));
  const Frame mac = parseFrame(QStringLiteral("3   mitcad    0x0000000100003f40 _ZN6mitcad5crash8crashNowEv + 52"));
  CHECK(mac.module == QStringLiteral("mitcad") && mac.offset == QStringLiteral("0x34"));

  // The same crash at other addresses (another run, another install
  // folder): the same key. Another place: another key.
  QStringList moved = report.stack;
  for (QString& line : moved) {
    line.replace(QStringLiteral("0x55555"), QStringLiteral("0x56789"));
    line.replace(QStringLiteral("/opt/mitcad/bin/"), QStringLiteral("/home/x/Mitcad/"));
  }
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) == crashKey(QStringLiteral("SIGSEGV"), moved));
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack).size() == 12);
  QStringList other = report.stack;
  other[2] = QStringLiteral("/opt/mitcad/bin/mitcad(_ZN6mitcad10MainWindow4saveEv+0x10) [0x5555555a2000]");
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) != crashKey(QStringLiteral("SIGSEGV"), other));
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) != crashKey(QStringLiteral("SIGBUS"), report.stack));
  // Without the handler's frames on top (a stack from another platform's
  // handler): still the same key.
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) ==
        crashKey(QStringLiteral("SIGSEGV"), report.stack.mid(2)));

  // Internal errors: numbers, addresses and paths do not matter.
  CHECK(errorKey(QStringLiteral("recompute"), QStringLiteral("F6 Fillet1 (fillet): SIGSEGV at 0x7f12 in /home/a/x.f3d")) ==
        errorKey(QStringLiteral("recompute"), QStringLiteral("F9 Fillet3 (fillet): SIGSEGV at 0x0 in /tmp/y/z.f3d")));
  CHECK(isKernelCrashMessage(QStringLiteral("Recompute failed: F6 Fillet1: fillet: SIGSEGV 'segmentation violation' detected. Address 0.")));
  CHECK(isKernelCrashMessage(QStringLiteral("fillet: ACCESS VIOLATION at address 0x00000000 during 'READ' operation")));
  CHECK(!isKernelCrashMessage(QStringLiteral("fillet: BRep_API: command not done")));
  // A worker's crash has a report of its own.
  CHECK(!isKernelCrashMessage(QStringLiteral("Mitcad crashed: SIGSEGV; crash report: /tmp/x.crash")));
  CHECK(errorKey(QStringLiteral("recompute"), QStringLiteral("fillet: SIGSEGV")) !=
        errorKey(QStringLiteral("recompute"), QStringLiteral("chamfer: SIGSEGV")));
}

void reportLinkTests() {
  using namespace mitcad::report;
  const QString tracker = QStringLiteral("https://github.com/mitcad/Mitcad/issues");
  IssueLink link = issueLink(tracker, QStringLiteral("Bug: a & b"), QStringLiteral("line 1\nline+2 #3"));
  CHECK(!link.shortened && link.clipboard.isEmpty());
  CHECK(QString::fromLatin1(link.url.toEncoded()) ==
        QStringLiteral("https://github.com/mitcad/Mitcad/issues/new?title=Bug%3A%20a%20%26%20b&body=line%201%0Aline%2B2%20%233"));
  CHECK(issueLink(tracker + QStringLiteral("/new/"), QStringLiteral("t"), QStringLiteral("b")).url.toEncoded() ==
        QByteArray("https://github.com/mitcad/Mitcad/issues/new?title=t&body=b"));
  // A query of the tracker's own stays.
  CHECK(issueLink(QStringLiteral("https://example.org/o/r/issues/new?template=bug.md"), QStringLiteral("t"),
                  QStringLiteral("b"))
            .url.toEncoded() == QByteArray("https://example.org/o/r/issues/new?template=bug.md&title=t&body=b"));
  // A template.
  CHECK(issueLink(QStringLiteral("https://tracker.example/new?summary={title}&text={body}"), QStringLiteral("a b"),
                  QStringLiteral("c/d"))
            .url.toEncoded() == QByteArray("https://tracker.example/new?summary=a%20b&text=c%2Fd"));
  // Too long: cut at a line, the whole on the clipboard.
  QStringList lines;
  for (int i = 0; i < 2000; ++i) {
    lines << QStringLiteral("frame %1: mitcad::something::long_function_name(int, double) + 0x%2").arg(i).arg(i, 0, 16);
  }
  const QString body = lines.join(QLatin1Char('\n'));
  link = issueLink(tracker, QStringLiteral("Crash"), body);
  CHECK(link.shortened);
  CHECK(link.clipboard == body);
  CHECK(link.url.toEncoded().size() <= kMaxUrlLength);
  CHECK(link.url.toEncoded().size() > kMaxUrlLength - 200);
  const QString sent = QUrl::fromPercentEncoding(link.url.toEncoded());
  CHECK(sent.contains(QStringLiteral("The whole report is on the clipboard")));
  CHECK(sent.contains(QStringLiteral("frame 0: ")) && !sent.contains(QStringLiteral("frame 1999: ")));
  CHECK(sent.contains(QStringLiteral("[... cut: the rest is on the clipboard]")));

  // The body: included sections with text, code fenced, the key last.
  Report report;
  report.key = QStringLiteral("0123456789ab");
  report.sections = {Section{QStringLiteral("description"), QStringLiteral("What happened"), QStringLiteral("It broke."), true, false, true},
                     Section{QStringLiteral("stack"), QStringLiteral("Stack"), QStringLiteral("a\nb"), true, true, true},
                     Section{QStringLiteral("diagnostics"), QStringLiteral("Diagnostics"), QStringLiteral("x"), false, true, true},
                     Section{QStringLiteral("contact"), QStringLiteral("Contact"), QString(), true, false, false}};
  CHECK(reportBody(report, QStringLiteral("0.1.0")) ==
        QStringLiteral("### What happened\n\nIt broke.\n\n### Stack\n\n```\na\nb\n```\n\n"
                       "<sub>Sent from Mitcad 0.1.0 · duplicate key `mitcad-0123456789ab`</sub>\n"));
}

} // namespace

int main(int argc, char* argv[]) {
  QApplication app(argc, argv);
  numberTests();
  cursorTests();
  printTests();
  desktopEntryTests();
  renderLookTests();
  renderOutputTests();
  renderCatcherTests();
  renderDeviceChoiceTests();
  renderFallbackTests();
  reportMaskTests();
  reportCrashTests();
  reportLinkTests();
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("app unit tests passed\n");
  return 0;
}
