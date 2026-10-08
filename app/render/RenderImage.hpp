// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <cstdint>

#include <QColor>
#include <QImage>
#include <QRect>

namespace mitcad::render {

// How the render's light becomes the screen's colours (the render
// settings' film, mitcad#47).
enum class ViewTransform {
  Standard, // sRGB of the light as it is; above 1 clips
  Filmic,   // a filmic curve: soft highlights, contrast in the darks
  Neutral,  // base colours kept under white light, only highlights compressed
};

struct Look {
  float exposure = 0.0f; // stops: each doubles the light
  ViewTransform transform = ViewTransform::Standard;
};

// A frame's half float as a float (0 for infinities and NaN).
float halfValue(std::uint16_t bits);

// A linear colour through the view transform (exposure not applied): still
// linear, 0..1 unless Standard.
std::array<float, 3> viewTransform(const std::array<float, 3>& rgb, ViewTransform transform);

// A rendered frame (RGBA half floats, premultiplied, linear, rows from the
// bottom up) for the screen: its light scaled by the exposure and put
// through the view transform, composited over a vertical gradient from
// `top` to `bottom` (the view's background or the settings' colour, sRGB,
// which the look does not change) in linear light, then encoded as sRGB,
// top row first. `catcher` (mitcad#54; the frame's second plane, as the
// pixels, or null) are the ground's factors on the background: the
// background behind a pixel is multiplied by them (shadows darken it, the
// bodies' reflections tint it).
QImage displayImage(const std::uint16_t* pixels, int width, int height, const QColor& top,
                    const QColor& bottom, const Look& look = Look(), const std::uint16_t* catcher = nullptr);

// The final render's image (mitcad#48) of a frame, for an image file: the
// same light, exposure, view transform and ground as displayImage. Opaque,
// it is displayImage's image (`deep`: with 16 bits per channel,
// Format_RGBX64); `transparent` leaves the background out: the colours of
// what covers a pixel (the premultiplied alpha taken off) with its
// coverage as straight alpha (Format_RGBA8888, or Format_RGBA64 when
// `deep`), where the ground's shadows are black with the mean of the
// catcher's darkening as alpha (an alpha cannot tint: no coloured
// reflections without a background).
QImage outputImage(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height,
                   const QColor& top, const QColor& bottom, const Look& look, bool transparent, bool deep);

// The bounding box of the pixels a frame covers (alpha above one half), in
// the frame's pixels with the top row first; null when it covers nothing.
QRect coverage(const std::uint16_t* pixels, int width, int height);

} // namespace mitcad::render
