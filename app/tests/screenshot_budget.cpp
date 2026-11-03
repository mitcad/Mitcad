// SPDX-License-Identifier: MIT
// ui_screenshot's encoded-image budget (mitcad#1188): a synthetic picture
// under the budget goes as it is, one over it is scaled down until its
// encoding fits, in the format asked for, within a bounded number of
// encodings; one that cannot fit, or an encoder that fails, is reported
// without an endless loop.
#include <algorithm>
#include <cstdio>
#include <cstdlib>

#include <QByteArray>
#include <QImage>
#include <QtGlobal>

#include "mcp/ScreenshotBudget.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "screenshot_budget.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// Noise: PNG cannot compress it, so its encoding is large (about 3 bytes a
// pixel) and shrinks with the area.
QImage noise(int width, int height) {
  QImage image(width, height, QImage::Format_RGB32);
  quint32 state = 12345U;
  for (int y = 0; y < height; ++y) {
    auto* line = reinterpret_cast<quint32*>(image.scanLine(y));
    for (int x = 0; x < width; ++x) {
      state = state * 1664525U + 1013904223U;
      line[x] = 0xff000000U | (state >> 8);
    }
  }
  return image;
}

// Smooth shading, as a view's picture mostly is.
QImage shading(int width, int height) {
  QImage image(width, height, QImage::Format_RGB32);
  for (int y = 0; y < height; ++y) {
    for (int x = 0; x < width; ++x) {
      image.setPixel(x, y, qRgb(x * 255 / width, y * 255 / height, 160));
    }
  }
  return image;
}

bool isPng(const QByteArray& bytes) {
  return bytes.startsWith(QByteArray("\x89PNG", 4));
}

bool isJpeg(const QByteArray& bytes) {
  return bytes.startsWith(QByteArray("\xff\xd8", 2));
}

void underTheBudget() {
  using namespace mitcad::mcp;
  const QImage picture = noise(800, 600);
  const auto png = [](const QImage& image) { return encodeImage(image, false, 90); };
  // The default inline budget: as it is, one encoding.
  const FittedImage whole = fitImage(picture, 1280, kScreenshotInlineBytes, png);
  CHECK(whole.fits);
  CHECK(whole.attempts == 1);
  CHECK(!whole.downscaled);
  CHECK(whole.image.size() == picture.size());
  CHECK(isPng(whole.bytes));
  CHECK(whole.bytes.size() <= kScreenshotInlineBytes);
  // max_size alone scales it, which is not for the budget.
  const FittedImage smaller = fitImage(picture, 400, kScreenshotInlineBytes, png);
  CHECK(smaller.fits);
  CHECK(!smaller.downscaled);
  CHECK(smaller.image.width() == 400 && smaller.image.height() == 300);
  // Never enlarged.
  const FittedImage notLarger = fitImage(picture, 4096, kScreenshotInlineBytes, png);
  CHECK(notLarger.image.size() == picture.size());
  // A budget exactly the encoding's size is met.
  const FittedImage exact = fitImage(picture, 1280, whole.bytes.size(), png);
  CHECK(exact.fits && exact.attempts == 1 && exact.bytes.size() == whole.bytes.size());
}

void overTheBudget() {
  using namespace mitcad::mcp;
  const QImage picture = noise(800, 600);
  // PNG kept as PNG: scaled down until the bytes fit.
  const auto png = [](const QImage& image) { return encodeImage(image, false, 90); };
  const qsizetype pngBudget = 100 * 1024;
  CHECK(png(picture).size() > pngBudget);
  const FittedImage pngFit = fitImage(picture, 1280, pngBudget, png);
  CHECK(pngFit.fits);
  CHECK(pngFit.downscaled);
  CHECK(pngFit.bytes.size() <= pngBudget);
  CHECK(isPng(pngFit.bytes));
  CHECK(pngFit.attempts > 1 && pngFit.attempts <= kScreenshotAttempts);
  CHECK(pngFit.image.width() < 800 && pngFit.image.width() >= kScreenshotMinSide);
  // The aspect is kept.
  CHECK(std::abs(pngFit.image.width() * 3 - pngFit.image.height() * 4) <= 4);
  // What is sent is the encoding of the image reported.
  CHECK(pngFit.bytes == png(pngFit.image));
  // Its base64 in the image block: 4 characters for every 3 bytes.
  CHECK(pngFit.bytes.toBase64().size() == (pngFit.bytes.size() + 2) / 3 * 4);

  // JPEG kept as JPEG.
  const auto jpeg = [](const QImage& image) { return encodeImage(image, true, 90); };
  const qsizetype jpegBudget = 40 * 1024;
  CHECK(jpeg(picture).size() > jpegBudget);
  const FittedImage jpegFit = fitImage(picture, 1280, jpegBudget, jpeg);
  CHECK(jpegFit.fits);
  CHECK(jpegFit.downscaled);
  CHECK(jpegFit.bytes.size() <= jpegBudget);
  CHECK(isJpeg(jpegFit.bytes));
  CHECK(jpegFit.attempts <= kScreenshotAttempts);

  // The smallest budget a caller may ask for fits a smooth picture.
  const FittedImage smallest = fitImage(shading(1280, 960), 1280, kScreenshotMinBytes, jpeg);
  CHECK(smallest.fits);
  CHECK(smallest.bytes.size() <= kScreenshotMinBytes);
}

void cannotFit() {
  using namespace mitcad::mcp;
  const QImage picture = noise(800, 600);
  // Even the smallest picture is over: the last attempt is reported, the
  // encodings bounded and the side not under the floor.
  int calls = 0;
  const auto png = [&calls](const QImage& image) {
    ++calls;
    return encodeImage(image, false, 90);
  };
  const FittedImage tiny = fitImage(picture, 1280, 10, png);
  CHECK(!tiny.fits);
  CHECK(!tiny.bytes.isEmpty());
  CHECK(tiny.bytes.size() > 10);
  CHECK(tiny.attempts == calls);
  CHECK(calls <= kScreenshotAttempts);
  CHECK(std::max(tiny.image.width(), tiny.image.height()) >= kScreenshotMinSide);

  // An encoder that never gets smaller stops after the bounded attempts.
  calls = 0;
  const auto stubborn = [&calls](const QImage&) {
    ++calls;
    return QByteArray(1000000, 'x');
  };
  const FittedImage stuck = fitImage(noise(2000, 200), 4096, 1000, stubborn);
  CHECK(!stuck.fits);
  CHECK(calls == stuck.attempts);
  CHECK(calls <= kScreenshotAttempts);

  // An encoder that fails: one attempt, no bytes.
  calls = 0;
  const auto failing = [&calls](const QImage&) {
    ++calls;
    return QByteArray();
  };
  const FittedImage failed = fitImage(picture, 1280, kScreenshotInlineBytes, failing);
  CHECK(!failed.fits);
  CHECK(failed.bytes.isEmpty());
  CHECK(calls == 1);

  // A picture already at the floor is encoded once.
  calls = 0;
  const FittedImage small = fitImage(noise(48, 32), 1280, 10, png);
  CHECK(!small.fits);
  CHECK(calls == 1);
}

} // namespace

int screenshotBudgetTests() {
  underTheBudget();
  overTheBudget();
  cannotFit();
  return failures;
}
