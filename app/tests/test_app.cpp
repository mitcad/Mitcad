// SPDX-License-Identifier: MIT
// Unit tests of the application's own helpers, without windows (ctest runs
// them on Qt's offscreen platform). Plain checks, as in the geometry tests.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <optional>

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

} // namespace

int main(int argc, char* argv[]) {
  QApplication app(argc, argv);
  numberTests();
  cursorTests();
  printTests();
  desktopEntryTests();
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("app unit tests passed\n");
  return 0;
}
