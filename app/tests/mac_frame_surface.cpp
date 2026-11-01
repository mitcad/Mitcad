// SPDX-License-Identifier: MIT
// The IOSurface display shader against the retained CPU image conversion,
// including packed preview rows inside a wider-capacity surface.
#include <QGuiApplication>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QOpenGLFunctions>
#include <QOpenGLFunctions_4_1_Core>
#include <QOpenGLVersionFunctionsFactory>
#include <QSurfaceFormat>
#include <algorithm>
#include <cmath>
#include <iostream>
#include <utility>
#include <new>
#include <sys/stat.h>
#include <unistd.h>
#include "render/FrameMemory.hpp"
#include "render/MacFrameControl.hpp"
#include "render/MacFrameDisplay.hpp"
#include "render/RenderImage.hpp"

namespace render = mitcad::render;
namespace {
float linear(float value) {
  return value <= 0.04045f ? value / 12.92f : std::pow((value + 0.055f) / 1.055f, 2.4f);
}
int srgb(float value) {
  const float encoded = value <= 0.0031308f ? value * 12.92f : 1.055f * std::pow(value, 1.0f / 2.4f) - 0.055f;
  return static_cast<int>(std::lround(std::clamp(encoded, 0.0f, 1.0f) * 255.0f));
}
// Reference the old OCCT texture's sRGB bilinear filtering after displayImage,
// rather than duplicate the new shader's film/ground conversion.
QImage filtered(const QImage& image, int width, int height) {
  QImage result(width, height, QImage::Format_RGB32);
  for (int y = 0; y < height; ++y) {
    for (int x = 0; x < width; ++x) {
      const float px = (static_cast<float>(x) + 0.5f) * image.width() / width - 0.5f;
      const float py = (static_cast<float>(y) + 0.5f) * image.height() / height - 0.5f;
      const int ix = static_cast<int>(std::floor(px)), iy = static_cast<int>(std::floor(py));
      const float fx = px - std::floor(px), fy = py - std::floor(py);
      float sum[3]{};
      for (int dy = 0; dy < 2; ++dy) {
        for (int dx = 0; dx < 2; ++dx) {
          const QColor color = image.pixelColor((ix + dx + image.width()) % image.width(),
                                                 (iy + dy + image.height()) % image.height());
          const float weight = (dx ? fx : 1.0f - fx) * (dy ? fy : 1.0f - fy);
          sum[0] += linear(color.redF()) * weight;
          sum[1] += linear(color.greenF()) * weight;
          sum[2] += linear(color.blueF()) * weight;
        }
      }
      result.setPixel(x, y, qRgb(srgb(sum[0]), srgb(sum[1]), srgb(sum[2])));
    }
  }
  return result;
}

bool controlOwnership() {
  std::string error;
  const auto page = sysconf(_SC_PAGESIZE);
  if (page <= 0) return false;
  // Small, exact-page and multi-page pixel extents all retain packed rows;
  // padding belongs to the allocation, never to the rendered pixel stride.
  for (const auto height : {8u, 128u, 129u}) {
    render::MacFrameSurfaces allocation;
    if (!allocation.create(16, height, error)) return false;
    for (std::uint32_t slot = 0; slot < render::kFrameSlots; ++slot) {
      for (std::uint32_t plane = 0; plane < render::kFramePlanes; ++plane) {
        const auto surface = allocation.surface(slot, plane);
        if (IOSurfaceGetWidth(surface) != 16 || IOSurfaceGetHeight(surface) != height ||
            IOSurfaceGetBytesPerRow(surface) != 128 || IOSurfaceGetAllocSize(surface) < render::planeBytes(16, height) ||
            IOSurfaceGetAllocSize(surface) % std::size_t(page) != 0 || !IOSurfaceGetBaseAddress(surface) ||
            reinterpret_cast<std::uintptr_t>(IOSurfaceGetBaseAddress(surface)) % std::size_t(page) != 0) return false;
      }
    }
  }
  auto surfaces = std::make_shared<render::MacFrameSurfaces>();
  auto memory = std::make_shared<render::FrameMemory>();
  if (!surfaces->create(16, 8, error) || !memory->create(sizeof(render::MacFrameControl), error)) return false;
  struct stat storage{};
  if (page <= 0 || fstat(memory->descriptor(), &storage) != 0 || storage.st_size < 0 ||
      !render::validMacFrameStorage(static_cast<std::size_t>(storage.st_size), static_cast<std::size_t>(page))) return false;
  for (const std::size_t syntheticPage : {std::size_t(4096), std::size_t(16384)}) {
    const auto rounded = render::macFrameControlStorageBytes(syntheticPage);
    if (!render::validMacFrameStorage(sizeof(render::MacFrameControl), syntheticPage) ||
        !render::validMacFrameStorage(rounded, syntheticPage) ||
        render::validMacFrameStorage(sizeof(render::MacFrameControl) - 1, syntheticPage) ||
        render::validMacFrameStorage(rounded + 1, syntheticPage) || render::validMacFrameStorage(rounded, 0)) return false;
  }
  render::FrameMemory oversized;
  if (!oversized.create(render::macFrameControlStorageBytes(static_cast<std::size_t>(page)) + 1, error) ||
      fstat(oversized.descriptor(), &storage) != 0 || storage.st_size < 0 ||
      render::validMacFrameStorage(static_cast<std::size_t>(storage.st_size), static_cast<std::size_t>(page))) return false;
  auto* control = new (memory->data()) render::MacFrameControl();
  control->capacityWidth = 16;
  control->capacityHeight = 8;
  control->generation = 1;
  render::FrameInfo info;
  if (render::claimMacFrame(surfaces, memory, info)) return false;
  for (auto& frame : control->frames) { frame.width = 5; frame.height = 3; }
  control->publicationCounter = 3;
  for (std::uint32_t slot = 0; slot < render::kFrameSlots; ++slot) control->publications[slot] = slot + 1;
  control->published.store(render::kFrameSlots);
  if (render::validMacFrameControl(*control, memory->size()) || render::claimMacFrame(surfaces, memory, info) ||
      control->readingMask.load() != 0) return false;
  control->published.store(0);
  control->version = 0;
  if (render::claimMacFrame(surfaces, memory, info) || control->readingMask.load() != 0) return false;
  control->version = render::kMacFrameVersion;
  control->generation = 0;
  if (render::claimMacFrame(surfaces, memory, info) || control->readingMask.load() != 0) return false;
  control->generation = 1;
  auto first = render::claimMacFrame(surfaces, memory, info);
  auto duplicate = render::claimMacFrame(surfaces, memory, info);
  if (!first || !duplicate || first->publication() != 1 || duplicate->publication() != 1 ||
      info.width != 5 || surfaces->readers(0) != 2 || control->readingMask.load() != 1) return false;
  first.reset();
  if (control->readingMask.load() != 1 || surfaces->readers(0) != 1) return false;
  control->publications[0] = 0;
  if (render::claimMacFrame(surfaces, memory, info) || control->readingMask.load() != 1) return false;
  control->publications[0] = 1;
  control->frames[0].width = 17;
  if (render::claimMacFrame(surfaces, memory, info) || control->readingMask.load() != 1) return false;
  control->frames[0].width = 5;
  control->published.store(1);
  auto second = render::claimMacFrame(surfaces, memory, info);
  control->published.store(2);
  auto third = render::claimMacFrame(surfaces, memory, info);
  if (!second || !third || second->publication() != 2 || third->publication() != 3 ||
      control->readingMask.load() != 7 || render::macWritableSlot(*control) != render::kNoSlot) return false;
  second.reset();
  if (control->readingMask.load() != 5 || render::macWritableSlot(*control) != 1) return false;
  const std::weak_ptr<render::FrameMemory> oldMemory = memory;
  const std::weak_ptr<render::MacFrameSurfaces> oldSurfaces = surfaces;
  memory.reset();
  surfaces.reset();
  if (oldMemory.expired() || oldSurfaces.expired() || control->readingMask.load() != 5) return false;
  duplicate.reset();
  if (control->readingMask.load() != 4) return false;
  third.reset();
  if (!oldMemory.expired() || !oldSurfaces.expired()) return false;

  surfaces = std::make_shared<render::MacFrameSurfaces>();
  if (!surfaces->create(16, 8, error)) return false;
  std::array<IOSurfaceRef, render::kFrameSlots * render::kFramePlanes> imported{};
  for (std::size_t index = 0; index < imported.size(); ++index) {
    imported[index] = surfaces->surface(static_cast<std::uint32_t>(index / render::kFramePlanes),
                                        static_cast<std::uint32_t>(index % render::kFramePlanes));
    CFRetain(imported[index]);
  }
  render::MacFrameSurfaces adopted;
  if (!adopted.adopt(imported, 16, 8, error) || adopted.pixels(0) != surfaces->pixels(0)) return false;
  for (auto& surface : imported) { surface = surfaces->surface(0); CFRetain(surface); }
  render::MacFrameSurfaces duplicateImport;
  if (duplicateImport.adopt(imported, 16, 8, error)) return false;
  for (std::size_t index = 0; index < imported.size(); ++index) {
    imported[index] = surfaces->surface(static_cast<std::uint32_t>(index / render::kFramePlanes),
                                        static_cast<std::uint32_t>(index % render::kFramePlanes));
    CFRetain(imported[index]);
  }
  render::MacFrameSurfaces wrongDimensions;
  if (wrongDimensions.adopt(imported, 24, 8, error)) return false;
  return true;
}
} // namespace
int main(int argc, char** argv) {
  QGuiApplication application(argc, argv);
  if (!controlOwnership()) { std::cerr << "Mac control ownership/import regression failed\n"; return 1; }
  QSurfaceFormat format;
  format.setVersion(4, 1);
  format.setProfile(QSurfaceFormat::CoreProfile);
  QOpenGLContext context;
  context.setFormat(format);
  if (!context.create()) { std::cerr << "OpenGL context creation failed\n"; return 1; }
  QOffscreenSurface target;
  target.setFormat(context.format());
  target.create();
  if (!context.makeCurrent(&target)) { std::cerr << "OpenGL context unavailable\n"; return 1; }
  auto* gl = QOpenGLVersionFunctionsFactory::get<QOpenGLFunctions_4_1_Core>(&context);
  if (gl == nullptr || !gl->initializeOpenGLFunctions()) { return 1; }
  std::cout << "OpenGL renderer: " << reinterpret_cast<const char*>(gl->glGetString(GL_RENDERER))
            << "; version: " << reinterpret_cast<const char*>(gl->glGetString(GL_VERSION))
            << "; context " << context.format().majorVersion() << '.' << context.format().minorVersion() << '\n';
  // Invalid dimensions must be rejected before IOSurface allocation.
  for (const auto& size : {std::pair<std::uint32_t, std::uint32_t>{0, 8}, {8, 0}, {7, 8},
                          {0xffffffffu, 0xffffffffu}, {16384, 16384}}) {
    render::MacFrameSurfaces invalid;
    std::string why;
    if (invalid.create(size.first, size.second, why)) { std::cerr << "invalid surface dimensions accepted\n"; return 1; }
  }
  GLuint sampler = 0;
  gl->glGenSamplers(1, &sampler);
  gl->glSamplerParameteri(sampler, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_LINEAR);
  std::string error;
  auto surfaces = std::make_shared<render::MacFrameSurfaces>();
  if (!surfaces->create(16, 8, error)) {
    std::cerr << error << '\n'; return 1;
  }
  if (surfaces->create(16, 8, error)) { std::cerr << "repeated surface allocation accepted\n"; return 1; }
  render::FrameInfo info;
  info.width = 5;
  info.height = 3;
  info.catcher = 1;
  const std::uint16_t values[] = {0, 0x3000, 0x3400, 0x3800, 0x3c00, 0x4000, 0xbc00, 0x7c00, 0x7e00};
  if (!surfaces->lock(0, error)) { std::cerr << error << '\n'; return 1; }
  auto* pixels = surfaces->pixels(0);
  auto* factors = surfaces->pixels(0, 1);
  for (int p = 0; p < 15; ++p) {
    for (int c = 0; c < 3; ++c) {
      pixels[p * 4 + c] = values[(p + c) % 9];
      factors[p * 4 + c] = values[(p * 2 + c) % 9];
    }
    pixels[p * 4 + 3] = values[p % 5];
    factors[p * 4 + 3] = 0x3c00;
  }
  if (!surfaces->unlock(0, error)) { std::cerr << error << '\n'; return 1; }
  auto lease = std::make_shared<render::MacFrameLease>(surfaces, 0);
  auto secondLease = std::make_shared<render::MacFrameLease>(surfaces, 0);
  lease.reset();
  if (surfaces->readers(0) != 1) { std::cerr << "duplicate frame lease lost ownership\n"; return 1; }
  if (surfaces->lock(0, error)) { std::cerr << "writer acquired a retained frame slot\n"; return 1; }
  // Replacing the allocation drops its original owner; the displayed frame
  // must keep its old raw pointers and IOSurfaces alive until GPU completion.
  const std::weak_ptr<render::MacFrameSurfaces> previous = surfaces;
  surfaces = std::make_shared<render::MacFrameSurfaces>();
  if (!surfaces->create(24, 8, error) || previous.expired()) {
    std::cerr << "replacing the allocation lost the displayed frame\n"; return 1;
  }
  render::MacFrameDisplay display;
  const QColor top(234, 208, 190), bottom(32, 97, 160);
  int maximum = 0;
  for (const int scale : {1, 2}) {
    QOpenGLFramebufferObject framebuffer(5 * scale, 3 * scale);
    if (!framebuffer.bind()) { return 1; }
    context.functions()->glViewport(0, 0, 5 * scale, 3 * scale);
    gl->glBindSampler(0, sampler);
    gl->glBindSampler(1, sampler);
    gl->glActiveTexture(GL_TEXTURE3);
    gl->glEnable(GL_DEPTH_TEST);
    gl->glEnable(GL_BLEND);
    gl->glEnable(GL_CULL_FACE);
    gl->glPolygonMode(GL_FRONT_AND_BACK, GL_LINE);
    const GLenum setupError = gl->glGetError();
    if (setupError != GL_NO_ERROR) {
      std::cerr << "invalid test GL setup: glError=" << setupError << '\n';
      display.release(nullptr);
      return 1;
    }
    for (int transform = 0; transform < 3; ++transform) {
      for (const float exposure : {-1.0f, 0.0f, 1.5f}) {
        for (const bool catcher : {false, true}) {
          info.catcher = catcher ? 1 : 0;
          render::Look look;
          look.transform = static_cast<render::ViewTransform>(transform);
          look.exposure = exposure;
          display.setFrame(secondLease, info, top, bottom, look);
          display.draw();
          GLint active = 0, polygon[2]{}, restoredSampler = 0;
          gl->glGetIntegerv(GL_ACTIVE_TEXTURE, &active);
          gl->glGetIntegerv(GL_POLYGON_MODE, polygon);
          gl->glActiveTexture(GL_TEXTURE0);
          gl->glGetIntegerv(GL_SAMPLER_BINDING, &restoredSampler);
          gl->glActiveTexture(static_cast<GLenum>(active));
          const GLboolean depth = gl->glIsEnabled(GL_DEPTH_TEST);
          const GLboolean blend = gl->glIsEnabled(GL_BLEND);
          const GLboolean cull = gl->glIsEnabled(GL_CULL_FACE);
          const GLenum glError = gl->glGetError();
          if (active != GL_TEXTURE3 || polygon[0] != GL_LINE || restoredSampler != static_cast<GLint>(sampler) ||
              !depth || !blend || !cull || glError != GL_NO_ERROR) {
            std::cerr << "display did not restore the viewport GL state: active=" << active
                      << " expected=" << GL_TEXTURE3 << " polygon=" << polygon[0] << ',' << polygon[1]
                      << " expected=" << GL_LINE << " sampler=" << restoredSampler << " expected=" << sampler
                      << " depth=" << int(depth) << " blend=" << int(blend) << " cull=" << int(cull)
                      << " expected=1,1,1 glError=" << glError << " expected=" << GL_NO_ERROR << '\n';
            display.release(nullptr);
            return 1;
          }
          const QImage actual = framebuffer.toImage().convertToFormat(QImage::Format_RGB32);
          const QImage image = render::displayImage(pixels, 5, 3, top, bottom, look, catcher ? factors : nullptr);
          const QImage expected = scale == 1 ? image : filtered(image, 5 * scale, 3 * scale);
          for (int y = 0; y < expected.height(); ++y) {
            for (int x = 0; x < expected.width(); ++x) {
              const QColor a = actual.pixelColor(x, y), b = expected.pixelColor(x, y);
              const int difference = std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()),
                                              std::abs(a.blue() - b.blue())});
              maximum = std::max(maximum, difference);
              if (difference > 1) {
                std::cerr << "display mismatch transform=" << transform << " exposure=" << exposure
                          << " catcher=" << catcher << " pixel=" << x << ',' << y << " difference=" << difference << '\n';
                display.release(nullptr);
                return 1;
              }
            }
          }
        }
      }
    }
  }
  // Match the viewport's aboutToBeDestroyed cleanup, then reuse its retained
  // frame in an independent Qt context rather than relying on shared GL names.
  display.release(nullptr);
  gl->glDeleteSamplers(1, &sampler);
  context.doneCurrent();
  {
    QOpenGLContext replacement;
    replacement.setFormat(format);
    if (!replacement.create() || QOpenGLContext::areSharing(&context, &replacement)) {
      std::cerr << "independent replacement GL context unavailable\n"; return 1;
    }
    QOffscreenSurface replacementTarget;
    replacementTarget.setFormat(replacement.format());
    replacementTarget.create();
    if (!replacement.makeCurrent(&replacementTarget)) return 1;
    QOpenGLFramebufferObject framebuffer(5, 3);
    if (!framebuffer.bind()) return 1;
    replacement.functions()->glViewport(0, 0, 5, 3);
    display.draw();
    const auto actual = framebuffer.toImage().convertToFormat(QImage::Format_RGB32);
    render::Look look;
    look.transform = render::ViewTransform::Neutral;
    look.exposure = 1.5f;
    const auto expected = render::displayImage(pixels, 5, 3, top, bottom, look, factors);
    bool matches = actual.size() == expected.size();
    for (int y = 0; matches && y < expected.height(); ++y) {
      for (int x = 0; x < expected.width(); ++x) {
        const auto a = actual.pixelColor(x, y), b = expected.pixelColor(x, y);
        matches = matches && std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()),
                                      std::abs(a.blue() - b.blue())}) <= 1;
      }
    }
    if (!matches || replacement.functions()->glGetError() != GL_NO_ERROR) {
      std::cerr << "retained frame did not recover in replacement context\n";
      display.release(nullptr);
      return 1;
    }
    display.setFrame({}, {}, top, bottom, {});
    display.release(nullptr);
  }
  secondLease.reset();
  if (!previous.expired()) { std::cerr << "frame lease leaked its replaced allocation\n"; return 1; }
  std::cout << "PASS: 36 shader comparisons, GL state, replacement context, allocations, duplicate leases, packed preview rows; "
               "maximum channel difference " << maximum << '\n';
  return 0;
}
