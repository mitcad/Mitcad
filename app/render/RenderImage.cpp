// SPDX-License-Identifier: MIT
#include "render/RenderImage.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <vector>

namespace mitcad::render {

namespace {

// IEEE 754 binary16 to float.
float halfToFloat(std::uint16_t bits) {
  const float sign = (bits & 0x8000u) != 0 ? -1.0f : 1.0f;
  const int exponent = (bits >> 10) & 0x1f;
  const int mantissa = bits & 0x3ff;
  if (exponent == 0) {
    return sign * std::ldexp(static_cast<float>(mantissa), -24); // subnormal
  }
  if (exponent == 31) {
    return mantissa == 0 ? sign * std::numeric_limits<float>::infinity() : std::numeric_limits<float>::quiet_NaN();
  }
  return sign * std::ldexp(static_cast<float>(mantissa + 1024), exponent - 25);
}

// Every half float's value: frames are converted through tables.
const std::vector<float>& halfTable() {
  static const std::vector<float> table = [] {
    std::vector<float> values(65536);
    for (std::size_t i = 0; i < values.size(); ++i) {
      const float value = halfToFloat(static_cast<std::uint16_t>(i));
      values[i] = std::isfinite(value) ? value : 0.0f;
    }
    return values;
  }();
  return table;
}

float toLinear(float srgb) {
  return srgb <= 0.04045f ? srgb / 12.92f : std::pow((srgb + 0.055f) / 1.055f, 2.4f);
}

// sRGB's encoding of linear values 0..1 in 4096 steps.
constexpr int kEncodeSteps = 4096;
const std::array<std::uint8_t, kEncodeSteps + 1>& encodeTable() {
  static const std::array<std::uint8_t, kEncodeSteps + 1> table = [] {
    std::array<std::uint8_t, kEncodeSteps + 1> values{};
    for (int i = 0; i <= kEncodeSteps; ++i) {
      const float linear = static_cast<float>(i) / kEncodeSteps;
      const float srgb =
          linear <= 0.0031308f ? linear * 12.92f : 1.055f * std::pow(linear, 1.0f / 2.4f) - 0.055f;
      values[static_cast<std::size_t>(i)] = static_cast<std::uint8_t>(std::lround(std::clamp(srgb, 0.0f, 1.0f) * 255.0f));
    }
    return values;
  }();
  return table;
}

std::uint8_t encode(float linear) {
  const float clamped = std::clamp(linear, 0.0f, 1.0f);
  return encodeTable()[static_cast<std::size_t>(clamped * kEncodeSteps + 0.5f)];
}

// sRGB's encoding in 16 bits, computed (the final render's deep images).
std::uint16_t encode16(float linear) {
  const float c = std::clamp(linear, 0.0f, 1.0f);
  const float srgb = c <= 0.0031308f ? c * 12.92f : 1.055f * std::pow(c, 1.0f / 2.4f) - 0.055f;
  return static_cast<std::uint16_t>(std::lround(std::clamp(srgb, 0.0f, 1.0f) * 65535.0f));
}

// The filmic curve of John Hable's "Uncharted 2" talk (GDC 2010): a toe
// and a shoulder, per channel (its shoulder, linear part and toe
// strengths), with its white point at 20 (light above 6.25 is white) and
// an exposure bias that keeps mid grey (0.18) where it was.
float hable(float v) {
  constexpr float a = 0.15f;
  constexpr float b = 0.50f;
  constexpr float c = 0.10f;
  constexpr float d = 0.20f;
  constexpr float e = 0.02f;
  constexpr float f = 0.30f;
  return (v * (a * v + c * b) + d * e) / (v * (a * v + b) + d * f) - e / f;
}

float filmic(float x) {
  static const float white = hable(20.0f);
  constexpr float bias = 3.2f;
  return std::min(hable(std::max(x, 0.0f) * bias) / white, 1.0f);
}

// The Khronos PBR Neutral tone mapper (its specification, 2024): colours up
// to 0.76 keep their values, apart from a small offset in the darks;
// brighter ones are compressed toward white and desaturated.
std::array<float, 3> neutral(std::array<float, 3> rgb) {
  constexpr float startCompression = 0.8f - 0.04f;
  constexpr float desaturation = 0.15f;
  const float lowest = std::min({rgb[0], rgb[1], rgb[2]});
  const float offset = lowest < 0.08f ? lowest - 6.25f * lowest * lowest : 0.04f;
  for (float& c : rgb) {
    c -= offset;
  }
  const float peak = std::max({rgb[0], rgb[1], rgb[2]});
  if (peak < startCompression) {
    return rgb;
  }
  constexpr float d = 1.0f - startCompression;
  const float newPeak = 1.0f - d * d / (peak + d - startCompression);
  const float g = 1.0f - 1.0f / (desaturation * (peak - newPeak) + 1.0f);
  for (float& c : rgb) {
    c = c * newPeak / peak * (1.0f - g) + newPeak * g;
  }
  return rgb;
}

} // namespace

float halfValue(std::uint16_t bits) { return halfTable()[bits]; }

std::array<float, 3> viewTransform(const std::array<float, 3>& rgb, ViewTransform transform) {
  switch (transform) {
  case ViewTransform::Filmic:
    return {filmic(rgb[0]), filmic(rgb[1]), filmic(rgb[2])};
  case ViewTransform::Neutral:
    return neutral({std::max(rgb[0], 0.0f), std::max(rgb[1], 0.0f), std::max(rgb[2], 0.0f)});
  case ViewTransform::Standard:
    break;
  }
  return rgb;
}

namespace {

// A frame's pixels through the look, row by row (the top row first): for
// each pixel the light with the exposure and the view transform applied,
// still premultiplied, its alpha, the background behind it (linear) and
// the ground's catcher factors there (1 without them).
template <typename PerPixel>
void lookPixels(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height, const QColor& top,
                const QColor& bottom, const Look& look, PerPixel&& perPixel) {
  const std::vector<float>& half = halfTable();
  const float topLinear[3] = {toLinear(top.redF()), toLinear(top.greenF()), toLinear(top.blueF())};
  const float bottomLinear[3] = {toLinear(bottom.redF()), toLinear(bottom.greenF()), toLinear(bottom.blueF())};
  const float scale = std::exp2(look.exposure);
  const bool curve = look.transform != ViewTransform::Standard;
  std::array<float, 3> factor{1.0f, 1.0f, 1.0f};
  for (int y = 0; y < height; ++y) {
    // The frame's rows run from the bottom up.
    const std::size_t row = static_cast<std::size_t>(height - 1 - y) * width * 4;
    const std::uint16_t* source = pixels + row;
    const std::uint16_t* factors = catcher != nullptr ? catcher + row : nullptr;
    const float along = height > 1 ? static_cast<float>(y) / static_cast<float>(height - 1) : 0.0f;
    std::array<float, 3> background;
    for (int c = 0; c < 3; ++c) {
      background[static_cast<std::size_t>(c)] = topLinear[c] + (bottomLinear[c] - topLinear[c]) * along;
    }
    for (int x = 0; x < width; ++x, source += 4) {
      const float alpha = std::clamp(half[source[3]], 0.0f, 1.0f);
      std::array<float, 3> light{half[source[0]] * scale, half[source[1]] * scale, half[source[2]] * scale};
      // A curve applies to the colour itself, not to its share of the
      // pixel: premultiplied alpha comes off first and goes back after.
      if (curve && alpha > 1e-4f) {
        light = viewTransform({light[0] / alpha, light[1] / alpha, light[2] / alpha}, look.transform);
        for (float& c : light) {
          c *= alpha;
        }
      }
      if (factors != nullptr) {
        const std::uint16_t* f = factors + static_cast<std::size_t>(x) * 4;
        factor = {std::max(half[f[0]], 0.0f), std::max(half[f[1]], 0.0f), std::max(half[f[2]], 0.0f)};
      }
      perPixel(x, y, light, alpha, background, factor);
    }
  }
}

// What covers a pixel over an unknown background (a transparent image):
// the light, and an alpha that takes the ground's shadow in too (one
// alpha cannot change colours: the factors' mean, at most 1).
float coveringAlpha(float alpha, const std::array<float, 3>& factor) {
  const float shade = 1.0f - std::clamp((factor[0] + factor[1] + factor[2]) / 3.0f, 0.0f, 1.0f);
  return alpha + (1.0f - alpha) * shade;
}

} // namespace

QImage displayImage(const std::uint16_t* pixels, int width, int height, const QColor& top,
                    const QColor& bottom, const Look& look, const std::uint16_t* catcher) {
  QImage image(width, height, QImage::Format_RGB32);
  lookPixels(pixels, catcher, width, height, top, bottom, look,
             [&image](int x, int y, const std::array<float, 3>& light, float alpha,
                      const std::array<float, 3>& background, const std::array<float, 3>& factor) {
               const float rest = 1.0f - alpha;
               reinterpret_cast<QRgb*>(image.scanLine(y))[x] =
                   qRgb(encode(light[0] + rest * factor[0] * background[0]),
                        encode(light[1] + rest * factor[1] * background[1]),
                        encode(light[2] + rest * factor[2] * background[2]));
             });
  return image;
}

QImage outputImage(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height,
                   const QColor& top, const QColor& bottom, const Look& look, bool transparent, bool deep) {
  if (!transparent && !deep) {
    return displayImage(pixels, width, height, top, bottom, look, catcher);
  }
  if (!transparent) {
    QImage image(width, height, QImage::Format_RGBX64);
    lookPixels(pixels, catcher, width, height, top, bottom, look,
               [&image](int x, int y, const std::array<float, 3>& light, float alpha,
                        const std::array<float, 3>& background, const std::array<float, 3>& factor) {
                 const float rest = 1.0f - alpha;
                 reinterpret_cast<QRgba64*>(image.scanLine(y))[x] =
                     QRgba64::fromRgba64(encode16(light[0] + rest * factor[0] * background[0]),
                                         encode16(light[1] + rest * factor[1] * background[1]),
                                         encode16(light[2] + rest * factor[2] * background[2]), 65535);
               });
    return image;
  }
  // Straight alpha: the colour of what covers the pixel, its coverage apart.
  const auto straight = [](const std::array<float, 3>& light, float alpha) {
    return alpha > 1e-4f ? std::array<float, 3>{light[0] / alpha, light[1] / alpha, light[2] / alpha}
                         : std::array<float, 3>{0.0f, 0.0f, 0.0f};
  };
  if (deep) {
    QImage image(width, height, QImage::Format_RGBA64);
    lookPixels(pixels, catcher, width, height, top, bottom, look,
               [&image, &straight](int x, int y, const std::array<float, 3>& light, float alpha,
                                   const std::array<float, 3>&, const std::array<float, 3>& factor) {
                 const float covered = coveringAlpha(alpha, factor);
                 const std::array<float, 3> c = straight(light, covered);
                 reinterpret_cast<QRgba64*>(image.scanLine(y))[x] = QRgba64::fromRgba64(
                     encode16(c[0]), encode16(c[1]), encode16(c[2]),
                     static_cast<std::uint16_t>(std::lround(covered * 65535.0f)));
               });
    return image;
  }
  QImage image(width, height, QImage::Format_RGBA8888);
  lookPixels(pixels, catcher, width, height, top, bottom, look,
             [&image, &straight](int x, int y, const std::array<float, 3>& light, float alpha,
                                 const std::array<float, 3>&, const std::array<float, 3>& factor) {
               const float covered = coveringAlpha(alpha, factor);
               const std::array<float, 3> c = straight(light, covered);
               std::uint8_t* target = image.scanLine(y) + static_cast<std::size_t>(x) * 4;
               target[0] = encode(c[0]);
               target[1] = encode(c[1]);
               target[2] = encode(c[2]);
               target[3] = static_cast<std::uint8_t>(std::lround(covered * 255.0f));
             });
  return image;
}

QRect coverage(const std::uint16_t* pixels, int width, int height) {
  const std::vector<float>& half = halfTable();
  int left = width;
  int right = -1;
  int topRow = height;
  int bottomRow = -1;
  for (int y = 0; y < height; ++y) {
    const std::uint16_t* row = pixels + static_cast<std::size_t>(height - 1 - y) * width * 4;
    for (int x = 0; x < width; ++x) {
      if (half[row[x * 4 + 3]] > 0.5f) {
        left = std::min(left, x);
        right = std::max(right, x);
        topRow = std::min(topRow, y);
        bottomRow = std::max(bottomRow, y);
      }
    }
  }
  if (right < 0) {
    return QRect();
  }
  return QRect(QPoint(left, topRow), QPoint(right, bottomRow));
}

} // namespace mitcad::render
