// SPDX-License-Identifier: MIT
#include "render/RenderOutput.hpp"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <memory>
#include <vector>

#include <QImage>
#include <QImageWriter>
#include <QSaveFile>
#include <QString>

#include <OpenImageIO/imageio.h>

namespace mitcad::render {

namespace {

float toLinear(float srgb) {
  return srgb <= 0.04045f ? srgb / 12.92f : std::pow((srgb + 0.055f) / 1.055f, 2.4f);
}

// OpenEXR: the light as rendered, top row first; opaque over the
// background (in linear light, times the ground's catcher factors) unless
// transparent (then the ground's shadows are in the alpha, as in PNG).
bool writeExr(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height,
              const OutputFile& file, std::string& error) {
  const float topLinear[3] = {toLinear(file.top.redF()), toLinear(file.top.greenF()), toLinear(file.top.blueF())};
  const float bottomLinear[3] = {toLinear(file.bottom.redF()), toLinear(file.bottom.greenF()),
                                 toLinear(file.bottom.blueF())};
  std::vector<float> rows(static_cast<std::size_t>(width) * height * 4);
  for (int y = 0; y < height; ++y) {
    const std::size_t row = static_cast<std::size_t>(height - 1 - y) * width * 4;
    const std::uint16_t* source = pixels + row;
    const std::uint16_t* factors = catcher != nullptr ? catcher + row : nullptr;
    float* target = rows.data() + static_cast<std::size_t>(y) * width * 4;
    const float along = height > 1 ? static_cast<float>(y) / static_cast<float>(height - 1) : 0.0f;
    for (int x = 0; x < width * 4; x += 4) {
      const float alpha = std::clamp(halfValue(source[x + 3]), 0.0f, 1.0f);
      float factor[3] = {1.0f, 1.0f, 1.0f};
      if (factors != nullptr) {
        for (int c = 0; c < 3; ++c) {
          factor[c] = std::max(halfValue(factors[x + c]), 0.0f);
        }
      }
      for (int c = 0; c < 3; ++c) {
        target[x + c] = halfValue(source[x + c]);
        if (!file.transparent) {
          target[x + c] +=
              (1.0f - alpha) * factor[c] * (topLinear[c] + (bottomLinear[c] - topLinear[c]) * along);
        }
      }
      const float shade = 1.0f - std::clamp((factor[0] + factor[1] + factor[2]) / 3.0f, 0.0f, 1.0f);
      target[x + 3] = file.transparent ? alpha + (1.0f - alpha) * shade : 1.0f;
    }
  }
  std::unique_ptr<OIIO::ImageOutput> output = OIIO::ImageOutput::create(file.path);
  if (!output) {
    error = OIIO::geterror();
    return false;
  }
  OIIO::ImageSpec spec(width, height, 4, OIIO::TypeDesc::HALF);
  spec.attribute("compression", "zip");
  spec.attribute("oiio:ColorSpace", "lin_rec709_scene");
  spec.attribute("Software", "Mitcad (Cycles)");
  if (!output->open(file.path, spec) ||
      !output->write_image(OIIO::TypeDesc::FLOAT, rows.data()) || !output->close()) {
    error = output->geterror();
    if (error.empty()) {
      error = "OpenImageIO could not write it";
    }
    return false;
  }
  return true;
}

} // namespace

bool outputFormatOf(const std::string& name, OutputFile::Format& format) {
  if (name == "png") {
    format = OutputFile::Format::Png;
  } else if (name == "png16") {
    format = OutputFile::Format::Png16;
  } else if (name == "jpeg") {
    format = OutputFile::Format::Jpeg;
  } else if (name == "exr") {
    format = OutputFile::Format::Exr;
  } else {
    return false;
  }
  return true;
}

bool writeOutputFile(const std::uint16_t* pixels, const std::uint16_t* catcher, int width, int height,
                     const OutputFile& file, std::string& error) {
  if (pixels == nullptr || width <= 0 || height <= 0) {
    error = "no image";
    return false;
  }
  if (file.format == OutputFile::Format::Exr) {
    return writeExr(pixels, catcher, width, height, file, error);
  }
  const bool jpeg = file.format == OutputFile::Format::Jpeg;
  const QImage image = outputImage(pixels, catcher, width, height, file.top, file.bottom, file.look,
                                   file.transparent && !jpeg, file.format == OutputFile::Format::Png16);
  // Whole or not at all.
  QSaveFile out(QString::fromStdString(file.path));
  if (!out.open(QIODevice::WriteOnly)) {
    error = out.errorString().toStdString();
    return false;
  }
  QImageWriter writer(&out, jpeg ? "jpeg" : "png");
  if (jpeg) {
    writer.setQuality(file.quality);
  }
  writer.setText(QStringLiteral("Software"), QStringLiteral("Mitcad (Cycles)"));
  if (!writer.write(image)) {
    error = writer.errorString().toStdString();
    out.cancelWriting();
    return false;
  }
  if (!out.commit()) {
    error = out.errorString().toStdString();
    return false;
  }
  return true;
}

} // namespace mitcad::render
