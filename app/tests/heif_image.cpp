// SPDX-License-Identifier: MIT
// HEIF images of image references (mitcad#850, mitcad#1304;
// view/ImageFile.hpp, view/HeifDecoder.hpp): HEIF files told from the
// other formats; through a decoder of the test's own, a HEIF file with no
// decoder giving the message and nothing else, and one decoded embedded
// as an upright PNG that reads back with no decoder; no decoder asked and
// nothing logged for other images; the EXIF orientations; the platform's
// decoder on a HEIF file the test makes (macOS decodes it upright; Windows
// and Linux decode it upright or have no decoder, never another failure);
// the run-time libheif with a library that is not there; and in a build
// with MITCAD_HEIF its libheif decoding the test's files as they are shown
// (a quarter turn applied), with 8 and 10 bits per channel, a file cut
// short and other bytes refused with the reason.
#include "view/ImageFile.hpp"

#include <cstddef>
#include <cstdio>
#include <array>
#include <cstdlib>
#include <initializer_list>
#include <utility>

#include <QBuffer>
#include <QByteArray>
#include <QColor>
#include <QDir>
#include <QImage>
#include <QString>
#include <QTransform>
#include <QtGlobal>
#include <QtLogging>

#include "platform/PlatformHeif.hpp"
#include "view/HeifDecoder.hpp"
#include "view/HeifLibrary.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "heif_image.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// The HEVC data of tools/heif-test-fixture.py, which made it from pixels of
// its own: a 64 x 32 picture of flat quarters (top left red, top right
// green, bottom left blue, bottom right white), full range, BT.601; per bit
// depth the hvcC property's content (with the parameter sets) and the image
// item's data (the slice, its length before it).
const unsigned char kHvcc8[] = {
    0x01, 0x01, 0x60, 0x00, 0x00, 0x00, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1e, 0xf0, 0x00, 0xfc,
    0xfd, 0xf8, 0xf8, 0x00, 0x00, 0x0f, 0x03, 0xa0, 0x00, 0x01, 0x00, 0x18, 0x40, 0x01, 0x0c, 0x01,
    0xff, 0xff, 0x01, 0x60, 0x00, 0x00, 0x03, 0x00, 0x90, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00,
    0x1e, 0x95, 0x98, 0x09, 0xa1, 0x00, 0x01, 0x00, 0x2b, 0x42, 0x01, 0x01, 0x01, 0x60, 0x00, 0x00,
    0x03, 0x00, 0x90, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x1e, 0xa0, 0x20, 0x82, 0x16, 0x59,
    0x59, 0xae, 0x4c, 0xae, 0x6e, 0x04, 0x04, 0x0c, 0x08, 0x00, 0x00, 0x03, 0x00, 0x08, 0x00, 0x00,
    0x03, 0x00, 0xc8, 0x40, 0xa2, 0x00, 0x01, 0x00, 0x06, 0x44, 0x01, 0xc1, 0x71, 0xa1, 0x12,
};
const unsigned char kData8[] = {
    0x00, 0x00, 0x00, 0x65, 0x28, 0x01, 0xaf, 0x04, 0x78, 0x3c, 0x86, 0x2f, 0xf4, 0x39, 0x48, 0xb3,
    0x20, 0x28, 0x0c, 0xb8, 0xe4, 0x65, 0xd1, 0xcc, 0x14, 0xb8, 0x94, 0x50, 0x58, 0x2c, 0x9f, 0xaf,
    0x6c, 0x9a, 0xac, 0x2c, 0xff, 0xb1, 0xec, 0x4c, 0x14, 0x9b, 0x6d, 0x7f, 0x70, 0x72, 0xa2, 0x83,
    0xfb, 0x63, 0xc8, 0x90, 0xba, 0x6c, 0xe9, 0xe9, 0x21, 0x69, 0x5a, 0x8f, 0xcb, 0xfd, 0x38, 0xcb,
    0x86, 0xdc, 0xc4, 0xab, 0x7b, 0xa9, 0x7d, 0xad, 0x65, 0x3b, 0x81, 0x52, 0x44, 0x78, 0x68, 0xfc,
    0x34, 0xb4, 0x87, 0x52, 0xef, 0xff, 0x86, 0xa8, 0x72, 0x83, 0x1b, 0x68, 0x7e, 0xf4, 0x80, 0x10,
    0x95, 0x93, 0x05, 0x8a, 0xbf, 0x6a, 0x3d, 0xba, 0x7e,
};
const unsigned char kHvcc10[] = {
    0x01, 0x02, 0x20, 0x00, 0x00, 0x00, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1e, 0xf0, 0x00, 0xfc,
    0xfd, 0xfa, 0xfa, 0x00, 0x00, 0x0f, 0x03, 0xa0, 0x00, 0x01, 0x00, 0x18, 0x40, 0x01, 0x0c, 0x01,
    0xff, 0xff, 0x02, 0x20, 0x00, 0x00, 0x03, 0x00, 0x90, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00,
    0x1e, 0x95, 0x98, 0x09, 0xa1, 0x00, 0x01, 0x00, 0x2a, 0x42, 0x01, 0x01, 0x02, 0x20, 0x00, 0x00,
    0x03, 0x00, 0x90, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x1e, 0xa0, 0x20, 0x82, 0x13, 0x65,
    0x95, 0x9a, 0xe4, 0xca, 0xe6, 0xe0, 0x40, 0x40, 0xc0, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00,
    0x00, 0x0c, 0x84, 0xa2, 0x00, 0x01, 0x00, 0x06, 0x44, 0x01, 0xc1, 0x71, 0xa1, 0x12,
};
const unsigned char kData10[] = {
    0x00, 0x00, 0x00, 0x60, 0x28, 0x01, 0xaf, 0x04, 0x78, 0x25, 0xe3, 0x17, 0xf3, 0xe3, 0x3f, 0xff,
    0x08, 0x7e, 0xcb, 0xdb, 0x86, 0x21, 0x57, 0x10, 0x21, 0xef, 0x38, 0x73, 0x1c, 0x47, 0xbc, 0x44,
    0x81, 0x36, 0xf3, 0x92, 0x3c, 0x7c, 0x67, 0x75, 0xd4, 0x82, 0x1f, 0xbd, 0xd7, 0x46, 0xb1, 0x38,
    0xf9, 0x65, 0x1f, 0xe7, 0xab, 0x8a, 0xc5, 0x3f, 0xd7, 0xe3, 0x54, 0x48, 0x07, 0xf3, 0x09, 0x73,
    0xf8, 0x05, 0xbf, 0xfa, 0xd6, 0x63, 0x5a, 0xff, 0xa3, 0x13, 0x50, 0x28, 0x8f, 0xfe, 0xaa, 0x94,
    0xba, 0x6f, 0xe7, 0x90, 0x7b, 0x4f, 0xf4, 0x08, 0xb7, 0x51, 0x6d, 0x4c, 0x7b, 0xab, 0x65, 0x09,
    0xf1, 0xdc, 0x4b, 0x90,
};

template <std::size_t N>
QByteArray bytesOf(const unsigned char (&data)[N]) {
  return QByteArray(reinterpret_cast<const char*>(data), static_cast<qsizetype>(N));
}

void put16(QByteArray& bytes, unsigned value) {
  bytes.append(static_cast<char>((value >> 8) & 0xffu));
  bytes.append(static_cast<char>(value & 0xffu));
}

void put32(QByteArray& bytes, unsigned value) {
  put16(bytes, (value >> 16) & 0xffffu);
  put16(bytes, value & 0xffffu);
}

QByteArray box(const char* kind, const QByteArray& body) {
  QByteArray bytes;
  put32(bytes, static_cast<unsigned>(8 + body.size()));
  bytes.append(kind, 4);
  bytes.append(body);
  return bytes;
}

// A full box: its version, no flags.
QByteArray fullBox(const char* kind, int version, const QByteArray& body) {
  QByteArray content(4, '\0');
  content[0] = static_cast<char>(version);
  return box(kind, content + body);
}

// A HEIF file of one HEVC image item, 64 x 32 pixels coded, turned `turns`
// quarter turns anticlockwise (an irot property; none for 0): the files
// tools/heif-test-fixture.py writes; then mirrored when `mirror` is 0 or 1
// (an imir property of that axis, after the irot).
QByteArray heifFile(const QByteArray& hvcc, const QByteArray& data, int turns, int mirror = -1) {
  QByteArray brands("heic", 4);
  put32(brands, 0);
  brands.append("mif1heic", 8);
  const QByteArray ftyp = box("ftyp", brands);
  QByteArray ispe;
  put32(ispe, 64);
  put32(ispe, 32);
  // Full range BT.601, as coded.
  QByteArray colr("nclx", 4);
  put16(colr, 1);
  put16(colr, 13);
  put16(colr, 6);
  colr.append(static_cast<char>(0x80));
  QByteArray properties = box("hvcC", hvcc) + fullBox("ispe", 0, ispe) + box("colr", colr);
  // The essential bit, then the property's index from 1.
  QByteArray associations;
  associations.append(static_cast<char>(0x81));
  associations.append(static_cast<char>(0x02));
  associations.append(static_cast<char>(0x03));
  if (turns != 0) {
    properties += box("irot", QByteArray(1, static_cast<char>(turns)));
    associations.append(static_cast<char>(0x84));
  }
  if (mirror >= 0) {
    properties += box("imir", QByteArray(1, static_cast<char>(mirror)));
    associations.append(static_cast<char>(0x80 | (turns != 0 ? 5 : 4)));
  }
  const auto metaBox = [&](unsigned offset) {
    QByteArray hdlr(4, '\0');
    hdlr.append("pict", 4);
    hdlr.append(QByteArray(13, '\0'));
    QByteArray pitm;
    put16(pitm, 1);
    // Offsets and lengths of 4 bytes, no base offset: item 1, one extent.
    QByteArray iloc;
    iloc.append(static_cast<char>(0x44));
    iloc.append('\0');
    put16(iloc, 1);
    put16(iloc, 1);
    put16(iloc, 0);
    put16(iloc, 1);
    put32(iloc, offset);
    put32(iloc, static_cast<unsigned>(data.size()));
    QByteArray infe;
    put16(infe, 1);
    put16(infe, 0);
    infe.append("hvc1", 4);
    infe.append('\0');
    QByteArray iinf;
    put16(iinf, 1);
    iinf += fullBox("infe", 2, infe);
    QByteArray ipma;
    put32(ipma, 1);
    put16(ipma, 1);
    ipma.append(static_cast<char>(associations.size()));
    ipma += associations;
    const QByteArray iprp = box("iprp", box("ipco", properties) + fullBox("ipma", 0, ipma));
    return fullBox("meta", 0,
                   fullBox("hdlr", 0, hdlr) + fullBox("pitm", 0, pitm) + fullBox("iloc", 0, iloc) +
                       fullBox("iinf", 0, iinf) + iprp);
  };
  const unsigned offset = static_cast<unsigned>(ftyp.size() + metaBox(0).size() + 8);
  return ftyp + metaBox(offset) + box("mdat", data);
}

// Whether a pixel is the colour expected, within the coding's rounding.
bool closeTo(QRgb actual, QRgb expected) {
  return std::abs(qRed(actual) - qRed(expected)) <= 24 && std::abs(qGreen(actual) - qGreen(expected)) <= 24 &&
         std::abs(qBlue(actual) - qBlue(expected)) <= 24;
}

const QRgb kRed = qRgb(255, 0, 0);
const QRgb kGreen = qRgb(0, 255, 0);
const QRgb kBlue = qRgb(0, 0, 255);
const QRgb kWhite = qRgb(255, 255, 255);

// The middles of the image's quarters: top left, top right, bottom left,
// bottom right.
bool quarters(const QImage& image, QRgb topLeft, QRgb topRight, QRgb bottomLeft, QRgb bottomRight) {
  const int w = image.width();
  const int h = image.height();
  return closeTo(image.pixel(w / 4, h / 4), topLeft) && closeTo(image.pixel(3 * w / 4, h / 4), topRight) &&
         closeTo(image.pixel(w / 4, 3 * h / 4), bottomLeft) &&
         closeTo(image.pixel(3 * w / 4, 3 * h / 4), bottomRight);
}

// The quarters' middles (top left, top right, bottom left, bottom right) as
// the nearest of red, green, blue and white: "RG/BW" for the fixture as
// coded, whatever YCbCr matrix the decoder took.
QString nearestQuarters(const QImage& image) {
  const auto nearest = [](QRgb pixel) {
    const std::pair<QRgb, char> colours[] = {{kRed, 'R'}, {kGreen, 'G'}, {kBlue, 'B'}, {kWhite, 'W'}};
    char best = '?';
    int distance = 0;
    for (const auto& colour : colours) {
      const int dr = qRed(pixel) - qRed(colour.first);
      const int dg = qGreen(pixel) - qGreen(colour.first);
      const int db = qBlue(pixel) - qBlue(colour.first);
      const int d = dr * dr + dg * dg + db * db;
      if (best == '?' || d < distance) {
        best = colour.second;
        distance = d;
      }
    }
    return QChar::fromLatin1(best);
  };
  const int w = image.width();
  const int h = image.height();
  return QString(nearest(image.pixel(w / 4, h / 4))) + nearest(image.pixel(3 * w / 4, h / 4)) + QLatin1Char('/') +
         nearest(image.pixel(w / 4, 3 * h / 4)) + nearest(image.pixel(3 * w / 4, 3 * h / 4));
}

QByteArray pngFile() {
  QImage image(4, 2, QImage::Format_RGB32);
  image.fill(Qt::gray);
  QByteArray bytes;
  QBuffer buffer(&bytes);
  if (!buffer.open(QIODevice::WriteOnly) || !image.save(&buffer, "PNG")) {
    return {};
  }
  return bytes;
}

void formatTests() {
  CHECK(mitcad::isHeifFile(heifFile(bytesOf(kHvcc8), bytesOf(kData8), 0)));
  CHECK(!mitcad::isHeifFile(pngFile()));
  // An MP4 file's ftyp names no HEIF brand; too short a file is none.
  QByteArray mp4("\0\0\0\x18" "ftypisom\0\0\0\0isomiso2", 24);
  CHECK(!mitcad::isHeifFile(mp4));
  CHECK(!mitcad::isHeifFile(QByteArray("\0\0\0\x18" "ftyp", 8)));
  // An AVIF file is of the family (image_header refuses it with the reason).
  QByteArray avif("\0\0\0\x18" "ftypavif\0\0\0\0mif1avif", 24);
  CHECK(mitcad::isHeifFile(avif));

  // Other formats through Qt's readers as before.
  QString why;
  const QImage png = mitcad::decodeImageFile(pngFile(), &why);
  CHECK(!png.isNull() && png.width() == 4 && png.height() == 2 && why.isEmpty());
  CHECK(mitcad::decodeImageFile(QByteArray("no image"), &why).isNull());
  CHECK(why == QStringLiteral("Qt cannot read the image"));

  // The file dialog lists HEIF on every platform (mitcad#1304).
  const QString filter = mitcad::imageFileFilter();
  CHECK(filter.contains(QStringLiteral("*.heic")) && filter.contains(QStringLiteral("*.heif")));
  CHECK(filter.contains(QStringLiteral("*.png")) && filter.contains(QStringLiteral("*.jpg")));
}

// The Qt messages logged while a check runs (none expected).
int g_logged = 0;
void countMessage(QtMsgType, const QMessageLogContext&, const QString&) { ++g_logged; }

// 64 x 32 pixels of flat quarters as the fixture's (top left red, top
// right green, bottom left blue, bottom right white), as a decoder that
// leaves the orientation to its caller gives them.
QImage quartersImage(QImage::Format format = QImage::Format_ARGB32) {
  QImage image(64, 32, format);
  for (int y = 0; y < 32; ++y) {
    for (int x = 0; x < 64; ++x) {
      image.setPixel(x, y, y < 16 ? (x < 32 ? kRed : kGreen) : (x < 32 ? kBlue : kWhite));
    }
  }
  return image;
}

mitcad::HeifDecoded decodedAs(const QImage& image, int orientation) {
  mitcad::HeifDecoded decoded;
  decoded.result = mitcad::HeifDecoded::Result::Decoded;
  decoded.image = image;
  decoded.orientation = orientation;
  return decoded;
}

void orientationTests() {
  using mitcad::orientedImage;
  const QImage stored = quartersImage(QImage::Format_RGB32);
  CHECK(orientedImage(stored, 1) == stored);
  CHECK(orientedImage(stored, 0) == stored && orientedImage(stored, 9) == stored);
  CHECK(quarters(orientedImage(stored, 2), kGreen, kRed, kWhite, kBlue));
  CHECK(quarters(orientedImage(stored, 3), kWhite, kBlue, kGreen, kRed));
  CHECK(quarters(orientedImage(stored, 4), kBlue, kWhite, kRed, kGreen));
  for (int turned : {5, 6, 7, 8}) {
    const QImage image = orientedImage(stored, turned);
    CHECK(image.width() == 32 && image.height() == 64);
  }
  CHECK(quarters(orientedImage(stored, 5), kRed, kBlue, kGreen, kWhite));
  // 6: to be turned a quarter clockwise, the left side to the top.
  CHECK(quarters(orientedImage(stored, 6), kBlue, kRed, kWhite, kGreen));
  CHECK(quarters(orientedImage(stored, 7), kWhite, kGreen, kBlue, kRed));
  // 8: a quarter anticlockwise, the right side to the top (as an irot of 1).
  CHECK(quarters(orientedImage(stored, 8), kGreen, kWhite, kRed, kBlue));
}

void injectedDecoderTests() {
  const QByteArray file = heifFile(bytesOf(kHvcc8), bytesOf(kData8), 0);
  int asked = 0;
  const QtMessageHandler previous = qInstallMessageHandler(countMessage);
  g_logged = 0;

  // No decoder: the message names what to install, and nothing else is
  // said (no log line) or embedded.
  mitcad::setHeifDecoderForTests([&asked](const QByteArray&) {
    ++asked;
    return mitcad::HeifDecoded::missing();
  });
  const mitcad::HeifPng none = mitcad::heifAsPng(file);
  CHECK(none.result == mitcad::HeifDecoded::Result::NoDecoder);
  CHECK(none.png.isEmpty() && none.width == 0 && none.height == 0);
  QString why;
  bool noDecoder = false;
  CHECK(mitcad::decodeImageFile(file, &why, &noDecoder).isNull());
  CHECK(noDecoder && why.contains(QStringLiteral("no HEIF decoder")));
  CHECK(asked == 2);
  const QString message = mitcad::heifDecoderMissingMessage();
#if defined(Q_OS_WIN)
  CHECK(message.contains(QStringLiteral("HEIF Image Extensions")) &&
        message.contains(QStringLiteral("HEVC Video Extensions")) && message.contains(QStringLiteral("Microsoft Store")));
#elif defined(Q_OS_MACOS)
  CHECK(message.contains(QStringLiteral("macOS")));
#else
  CHECK(message.contains(QStringLiteral("libheif")) && message.contains(QStringLiteral("HEVC plugin")));
#endif

  // Other images never ask the HEIF decoder, nor log a word.
  asked = 0;
  CHECK(!mitcad::decodeImageFile(pngFile(), &why, &noDecoder).isNull() && !noDecoder);
  CHECK(g_logged == 0);
  qInstallMessageHandler(previous);
  CHECK(mitcad::decodeImageFile(QByteArray("no image"), &why, &noDecoder).isNull() && !noDecoder);
  CHECK(asked == 0);

  // A decoder that gives the pixels as stored and the orientation 8 (a
  // quarter anticlockwise): embedded as a PNG file of 32 x 64 pixels,
  // upright, with no alpha channel (every pixel opaque), at 96 pixels to
  // the inch.
  mitcad::setHeifDecoderForTests([](const QByteArray&) { return decodedAs(quartersImage(), 8); });
  const mitcad::HeifPng png = mitcad::heifAsPng(file);
  CHECK(png.result == mitcad::HeifDecoded::Result::Decoded);
  CHECK(png.width == 32 && png.height == 64);
  CHECK(png.png.startsWith(QByteArray("\x89PNG\r\n\x1a\n", 8)));
  CHECK(!mitcad::isHeifFile(png.png));
  // The design opens without a decoder: its PNG goes through Qt.
  mitcad::setHeifDecoderForTests([](const QByteArray&) { return mitcad::HeifDecoded::missing(); });
  const QImage embedded = mitcad::decodeImageFile(png.png, &why, &noDecoder);
  CHECK(why.isEmpty() && !noDecoder);
  CHECK(embedded.width() == 32 && embedded.height() == 64);
  CHECK(!embedded.isNull() && quarters(embedded, kGreen, kWhite, kRed, kBlue));
  CHECK(!embedded.hasAlphaChannel());
  CHECK(embedded.dotsPerMeterX() == 3780 && embedded.dotsPerMeterY() == 3780);

  // An image with a transparent pixel keeps its alpha channel.
  QImage clear = quartersImage();
  clear.setPixel(0, 0, qRgba(0, 0, 0, 0));
  mitcad::setHeifDecoderForTests([clear](const QByteArray&) { return decodedAs(clear, 1); });
  const QImage kept = QImage::fromData(mitcad::heifAsPng(file).png);
  CHECK(kept.hasAlphaChannel() && qAlpha(kept.pixel(0, 0)) == 0 && qAlpha(kept.pixel(40, 20)) == 255);

  // A decoder that refuses the file: its reason, not the message.
  mitcad::setHeifDecoderForTests(
      [](const QByteArray&) { return mitcad::HeifDecoded::failed(QStringLiteral("the HEIF file is cut short")); });
  const mitcad::HeifPng refused = mitcad::heifAsPng(file);
  CHECK(refused.result == mitcad::HeifDecoded::Result::Failed);
  CHECK(refused.error == QStringLiteral("the HEIF file is cut short") && refused.png.isEmpty());
  CHECK(mitcad::decodeImageFile(file, &why, &noDecoder).isNull() && !noDecoder &&
        why == QStringLiteral("the HEIF file is cut short"));
  mitcad::setHeifDecoderForTests({});

  // MITCAD_TEST_HEIF_DECODER=none (the UI tests'): no decoder whatever the
  // build and the platform have.
  qputenv("MITCAD_TEST_HEIF_DECODER", "none");
  CHECK(mitcad::decodeHeif(file).result == mitcad::HeifDecoded::Result::NoDecoder);
  qunsetenv("MITCAD_TEST_HEIF_DECODER");
}

// The platform's own decoder on the fixture turned a quarter
// anticlockwise (irot 1): macOS always decodes it; Windows and Linux
// decode it or have no decoder (the extensions, or libheif with its HEVC
// plugin, not installed), but never fail on it otherwise. One that decodes
// it gets every irot and imir upright.
void platformDecoderTests() {
  const QByteArray file = heifFile(bytesOf(kHvcc8), bytesOf(kData8), 1);
  const mitcad::HeifDecoded decoded = mitcad::platformDecodeHeif(file);
  const char* result = decoded.result == mitcad::HeifDecoded::Result::Decoded     ? "decoded"
                       : decoded.result == mitcad::HeifDecoded::Result::NoDecoder ? "no decoder"
                                                                                  : "failed";
  std::fprintf(stderr, "heif_image.cpp: the platform's HEIF decoder: %s%s%s\n", result,
               decoded.error.isEmpty() ? "" : ": ", qPrintable(decoded.error));
#if defined(Q_OS_MACOS)
  CHECK(decoded.result == mitcad::HeifDecoded::Result::Decoded);
#else
  CHECK(decoded.result != mitcad::HeifDecoded::Result::Failed);
#endif
  if (decoded.result != mitcad::HeifDecoded::Result::Decoded) {
    return;
  }
  // Every turn (irot 0 to 3), unmirrored and mirrored about either axis
  // (imir 0 and 1 after the irot): upright exactly once, the decoder's own
  // transform and the orientation it reports together (WIC and ImageIO
  // apply the file's irot and imir themselves and report 1). Each quarter
  // is taken as the nearest of the four colours: WIC decodes with BT.709
  // although the file says BT.601 (red comes as 255, 25, 0, green as 0,
  // 216, 0).
  std::array<QString, 3> mirrored{};
  for (int mirror = -1; mirror <= 1; ++mirror) {
    for (int turns = 0; turns < 4; ++turns) {
      const mitcad::HeifDecoded each =
          mitcad::platformDecodeHeif(heifFile(bytesOf(kHvcc8), bytesOf(kData8), turns, mirror));
      CHECK(each.result == mitcad::HeifDecoded::Result::Decoded);
      if (each.result != mitcad::HeifDecoded::Result::Decoded) {
        continue;
      }
      const QImage upright = mitcad::orientedImage(each.image, each.orientation);
      const QImage turned = quartersImage().transformed(QTransform().rotate(-90.0 * turns));
      // imir 0 mirrors top to bottom (about the horizontal axis, as
      // libheif and WIC read it), 1 left to right; a decoder that reads the
      // axes the other way round passes too, but the two must differ.
      const QString top = nearestQuarters(turned.transformed(QTransform(1, 0, 0, -1, 0, 0)));
      const QString side = nearestQuarters(turned.transformed(QTransform(-1, 0, 0, 1, 0, 0)));
      const QString got = nearestQuarters(upright);
      const bool sized = upright.width() == turned.width() && upright.height() == turned.height();
      const bool placed = mirror < 0 ? got == nearestQuarters(turned) : (got == top || got == side);
      if (!sized || !placed) {
        std::fprintf(stderr, "heif_image.cpp: irot %d imir %d: %dx%d %s (orientation %d), not %dx%d %s\n", turns,
                     mirror, upright.width(), upright.height(), qPrintable(got), each.orientation, turned.width(),
                     turned.height(), qPrintable(mirror < 0 ? nearestQuarters(turned) : top + QStringLiteral(" or ") + side));
      }
      CHECK(sized && placed);
      if (turns == 0) {
        mirrored[static_cast<std::size_t>(mirror + 1)] = got;
      }
    }
  }
  CHECK(mirrored[1] != mirrored[2] && mirrored[1] != mirrored[0] && mirrored[2] != mirrored[0]);
}

// The run-time libheif (Linux's, view/HeifLibrary.hpp) with a library that
// is not there: no decoder, and on Linux the platform's decoder and the
// embedding say so too (MITCAD_TEST_HEIF_LIBRARY, the UI test's).
void libraryTests() {
  const QByteArray file = heifFile(bytesOf(kHvcc8), bytesOf(kData8), 0);
  const QString absent = QDir::temp().filePath(QStringLiteral("mitcad-no-such-folder/libheif.so.1"));
  const mitcad::HeifDecoded decoded = mitcad::decodeHeifWithLibrary(absent, file);
  CHECK(decoded.result == mitcad::HeifDecoded::Result::NoDecoder);
  CHECK(!decoded.error.isEmpty() && decoded.image.isNull());
  // Asked again: looked for again, still not there.
  CHECK(mitcad::decodeHeifWithLibrary(absent, file).result == mitcad::HeifDecoded::Result::NoDecoder);
#if !defined(Q_OS_WIN) && !defined(Q_OS_MACOS)
  qputenv("MITCAD_TEST_HEIF_LIBRARY", absent.toLocal8Bit());
  CHECK(mitcad::platformDecodeHeif(file).result == mitcad::HeifDecoded::Result::NoDecoder);
  if (!mitcad::heifDecoderBuilt()) {
    CHECK(mitcad::heifAsPng(file).result == mitcad::HeifDecoded::Result::NoDecoder);
  }
  qunsetenv("MITCAD_TEST_HEIF_LIBRARY");
#endif
}

// The libheif and libde265 a build with MITCAD_HEIF links (mitcad#850).
void bundledDecoderTests() {
  QString why;
  const QByteArray file = heifFile(bytesOf(kHvcc8), bytesOf(kData8), 0);
  if (!mitcad::heifDecoderBuilt()) {
    return;
  }
  const QImage upright = mitcad::decodeImageFile(file, &why);
  CHECK(why.isEmpty());
  if (!why.isEmpty()) {
    std::fprintf(stderr, "heif_image.cpp: %s\n", qPrintable(why));
  }
  CHECK(upright.width() == 64 && upright.height() == 32);
  CHECK(!upright.isNull() && quarters(upright, kRed, kGreen, kBlue, kWhite));

  // A quarter turn anticlockwise: 32 x 64, the right side on top.
  const QImage turned = mitcad::decodeImageFile(heifFile(bytesOf(kHvcc8), bytesOf(kData8), 1), &why);
  CHECK(turned.width() == 32 && turned.height() == 64);
  CHECK(!turned.isNull() && quarters(turned, kGreen, kWhite, kRed, kBlue));
  // Three quarters: the left side on top.
  const QImage back = mitcad::decodeImageFile(heifFile(bytesOf(kHvcc8), bytesOf(kData8), 3), &why);
  CHECK(back.width() == 32 && back.height() == 64);
  CHECK(!back.isNull() && quarters(back, kBlue, kRed, kWhite, kGreen));

  // 10 bits per channel, drawn with 8.
  const QImage deep = mitcad::decodeImageFile(heifFile(bytesOf(kHvcc10), bytesOf(kData10), 0), &why);
  CHECK(why.isEmpty());
  if (!why.isEmpty()) {
    std::fprintf(stderr, "heif_image.cpp: 10 bits: %s\n", qPrintable(why));
  }
  CHECK(deep.width() == 64 && deep.height() == 32);
  CHECK(!deep.isNull() && quarters(deep, kRed, kGreen, kBlue, kWhite));

  // Cut inside the image's data, or its data not HEVC: refused with the
  // decoder's reason.
  CHECK(mitcad::decodeImageFile(file.left(file.size() - 40), &why).isNull());
  CHECK(why.startsWith(QStringLiteral("the HEIF")));
  QByteArray garbled = file;
  garbled.replace(garbled.size() - 60, 56, QByteArray(56, '\0'));
  const QImage broken = mitcad::decodeImageFile(garbled, &why);
  CHECK(broken.isNull() == !why.isEmpty());
}

} // namespace

int heifImageTests() {
  failures = 0;
  formatTests();
  orientationTests();
  injectedDecoderTests();
  platformDecoderTests();
  libraryTests();
  bundledDecoderTests();
  return failures;
}
