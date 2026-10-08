// SPDX-License-Identifier: MIT
#pragma once

#include <cstdint>
#include <string>

#include <QColor>

#include "render/RenderImage.hpp"

namespace mitcad::render {

// The final render's image file (mitcad#48, docs/rendering.md), written by
// the render worker: PNG (8 or 16 bits per channel) and JPEG through Qt,
// with the exposure and the view transform as the view shows them (sRGB);
// OpenEXR through OpenImageIO, the render's linear light as it is (half
// floats, premultiplied alpha, neither exposure nor view transform).
struct OutputFile {
  enum class Format { Png, Png16, Jpeg, Exr };
  std::string path;
  Format format = Format::Png;
  int quality = 90; // JPEG
  // An alpha channel instead of the background (not JPEG).
  bool transparent = false;
  Look look;
  // What is behind the bodies where the frame is transparent: a vertical
  // gradient (sRGB), the view's background or the settings' colour.
  QColor top = Qt::white;
  QColor bottom = Qt::white;
};

// The format of the render settings' `output.format` (png, png16, jpeg,
// exr); false for another name.
bool outputFormatOf(const std::string& name, OutputFile::Format& format);

// Writes a frame (RGBA half floats, premultiplied, linear, rows from the
// bottom up) with the ground's catcher factors (the frame's second plane,
// or null; mitcad#54) as `file`; false with the reason in `error`.
bool writeOutputFile(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height,
                     const OutputFile& file, std::string& error);

} // namespace mitcad::render
