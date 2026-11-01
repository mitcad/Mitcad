// SPDX-License-Identifier: MIT
// GPU-written IOSurfaces presented by the existing OpenGL display adapter.
// Keep OCCT/OpenGL declarations before Qt, as in MacFrameDisplay.cpp.
#include <OpenGl_Context.hxx>
#include <OpenGL/OpenGL.h>
#include <Foundation/Foundation.h>
#include <Metal/Metal.h>
#include <QGuiApplication>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QOpenGLFunctions_4_1_Core>
#include <QOpenGLShaderProgram>
#include <QOpenGLVersionFunctionsFactory>
#include <QSurfaceFormat>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <string>
#include <utility>
#include <unistd.h>
#include "render/MacFrameDisplay.hpp"
#include "render/MacFrameMetal.hpp"
#include "render/RenderImage.hpp"

namespace {
namespace render = mitcad::render;
constexpr std::size_t kValues = 16 * 8 * 4;
constexpr std::array<std::uint16_t, 10> kLight{0, 0x2c00, 0x3000, 0x3400, 0x3800,
                                            0x3a00, 0x3c00, 0x4000, 0x4200, 0xb400};
constexpr std::array<std::uint16_t, 5> kAlpha{0, 0x3400, 0x3800, 0x3a00, 0x3c00};
constexpr std::array<std::uint16_t, 6> kCatcher{0x3000, 0x3400, 0x3800, 0x3a00, 0x3c00, 0x4000};

std::uint16_t lightValue(std::size_t index, unsigned phase) {
  const auto pixel = index / 4, channel = index % 4;
  return channel == 3 ? kAlpha[(pixel + phase * 2) % kAlpha.size()]
                      : kLight[(pixel * 3 + channel * 2 + phase * 5) % kLight.size()];
}
std::uint16_t catcherValue(std::size_t index, unsigned phase) {
  const auto pixel = index / 4, channel = index % 4;
  return channel == 3 ? 0x3c00 : kCatcher[(pixel * 2 + channel + phase * 3) % kCatcher.size()];
}

float linear(float value) {
  return value <= 0.04045f ? value / 12.92f : std::pow((value + 0.055f) / 1.055f, 2.4f);
}
int encoded(float value) {
  const float srgb = value <= 0.0031308f ? value * 12.92f : 1.055f * std::pow(value, 1.0f / 2.4f) - 0.055f;
  return static_cast<int>(std::lround(std::clamp(srgb, 0.0f, 1.0f) * 255.0f));
}
// The retained image quad filters decoded, already quantized sRGB texels.
// This references that CPU image, rather than duplicate the surface shader.
QImage filtered(const QImage& source, int width, int height) {
  QImage result(width, height, QImage::Format_RGB32);
  for (int y = 0; y < height; ++y) for (int x = 0; x < width; ++x) {
    const float px = (static_cast<float>(x) + 0.5f) * source.width() / width - 0.5f;
    const float py = (static_cast<float>(y) + 0.5f) * source.height() / height - 0.5f;
    const int firstX = static_cast<int>(std::floor(px)), firstY = static_cast<int>(std::floor(py));
    const float fx = px - std::floor(px), fy = py - std::floor(py);
    float sum[3]{};
    for (int dy = 0; dy < 2; ++dy) for (int dx = 0; dx < 2; ++dx) {
      const auto color = source.pixelColor((firstX + dx + source.width()) % source.width(),
                                           (firstY + dy + source.height()) % source.height());
      const float weight = (dx ? fx : 1.0f - fx) * (dy ? fy : 1.0f - fy);
      sum[0] += linear(color.redF()) * weight;
      sum[1] += linear(color.greenF()) * weight;
      sum[2] += linear(color.blueF()) * weight;
    }
    result.setPixel(x, y, qRgb(encoded(sum[0]), encoded(sum[1]), encoded(sum[2])));
  }
  return result;
}

struct Completion {
  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  std::atomic<bool> succeeded{false};
  ~Completion() { dispatch_release(semaphore); }
};

class MetalWriter {
public:
  ~MetalWriter() { [m_queue release]; [m_pipeline release]; [m_function release]; [m_library release]; }
  bool create(id<MTLDevice> device, std::string& error) {
    NSError* failure = nil;
    m_library = [device newLibraryWithSource:
        @"#include <metal_stdlib>\nusing namespace metal;\n"
        @"constant ushort lightValues[10] = {0,0x2c00,0x3000,0x3400,0x3800,0x3a00,0x3c00,0x4000,0x4200,0xb400};\n"
        @"constant ushort alphaValues[5] = {0,0x3400,0x3800,0x3a00,0x3c00};\n"
        @"constant ushort catcherValues[6] = {0x3000,0x3400,0x3800,0x3a00,0x3c00,0x4000};\n"
        @"kernel void frame(device ushort* light [[buffer(0)]], device ushort* catcher [[buffer(1)]],"
        @"constant uint& phase [[buffer(2)]], uint i [[thread_position_in_grid]]) {"
        @"if(i>=512) return; uint p=i/4,c=i%4;"
        @"light[i]=c==3?alphaValues[(p+phase*2)%5]:lightValues[(p*3+c*2+phase*5)%10];"
        @"catcher[i]=c==3?ushort(0x3c00):catcherValues[(p*2+c+phase*3)%6]; }"
        options:nil error:&failure];
    m_function = [m_library newFunctionWithName:@"frame"];
    m_pipeline = m_function ? [device newComputePipelineStateWithFunction:m_function error:&failure] : nil;
    m_queue = [device newCommandQueue];
    if (!m_library || !m_function || !m_pipeline || !m_queue) {
      const char* reason = failure.localizedDescription.UTF8String;
      error = reason ? reason : "could not create the Metal coherence pipeline";
      return false;
    }
    return true;
  }
  bool write(const std::shared_ptr<render::MacFrameSurfaces>& surfaces,
             const std::shared_ptr<render::MacFrameMetalBuffer>& light,
             const std::shared_ptr<render::MacFrameMetalBuffer>& catcher, unsigned phase, std::string& error) {
    if (!surfaces->lock(0, error)) return false;
    id<MTLCommandBuffer> command = [m_queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    if (!command || !encoder) { surfaces->unlock(0, error); error = "Metal command encoding failed"; return false; }
    [encoder setComputePipelineState:m_pipeline];
    [encoder setBuffer:(__bridge id<MTLBuffer>)light->handle() offset:0 atIndex:0];
    [encoder setBuffer:(__bridge id<MTLBuffer>)catcher->handle() offset:0 atIndex:1];
    const std::uint32_t generation = phase;
    [encoder setBytes:&generation length:sizeof(generation) atIndex:2];
    [encoder dispatchThreads:MTLSizeMake(kValues, 1, 1) threadsPerThreadgroup:MTLSizeMake(64, 1, 1)];
    [encoder endEncoding];
    const auto completion = std::make_shared<Completion>();
    const auto retainedSurfaces = surfaces;
    const auto retainedLight = light;
    const auto retainedCatcher = catcher;
    [command addCompletedHandler:^(id<MTLCommandBuffer> finished) {
      // Retain every writer owner through actual GPU completion, including a
      // timeout return by the caller. A reader lease is created only afterward.
      (void)retainedLight;
      (void)retainedCatcher;
      std::string unlockError;
      const bool unlocked = retainedSurfaces->unlock(0, unlockError);
      completion->succeeded.store(finished.status == MTLCommandBufferStatusCompleted && finished.error == nil && unlocked,
                                  std::memory_order_release);
      dispatch_semaphore_signal(completion->semaphore);
    }];
    [command commit];
    if (dispatch_semaphore_wait(completion->semaphore, dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC)) != 0) {
      error = "Metal frame did not complete within ten seconds";
      return false;
    }
    if (!completion->succeeded.load(std::memory_order_acquire)) {
      error = "Metal frame failed or its IOSurface could not be unlocked";
      return false;
    }
    return true;
  }
private:
  id<MTLLibrary> m_library = nil;
  id<MTLFunction> m_function = nil;
  id<MTLComputePipelineState> m_pipeline = nil;
  id<MTLCommandQueue> m_queue = nil;
};

constexpr std::array<GLenum, 6> kCapabilities{GL_DEPTH_TEST, GL_BLEND, GL_CULL_FACE, GL_SCISSOR_TEST,
                                            GL_STENCIL_TEST, GL_FRAMEBUFFER_SRGB};
constexpr std::array<unsigned, 3> kUnits{0, 1, 3};
struct GlState {
  GLint program = 0, vao = 0, active = 0, drawFramebuffer = 0, readFramebuffer = 0;
  std::array<GLint, 3> textures{}, samplers{};
  std::array<GLint, 2> polygon{};
  std::array<GLint, 4> viewport{}, scissor{};
  GLboolean depthMask = GL_TRUE;
  std::array<GLboolean, 4> colorMask{};
  std::array<GLboolean, 6> enabled{};
  static GlState capture(QOpenGLFunctions_4_1_Core* gl) {
    GlState state;
    gl->glGetIntegerv(GL_CURRENT_PROGRAM, &state.program);
    gl->glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &state.vao);
    gl->glGetIntegerv(GL_ACTIVE_TEXTURE, &state.active);
    gl->glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &state.drawFramebuffer);
    gl->glGetIntegerv(GL_READ_FRAMEBUFFER_BINDING, &state.readFramebuffer);
    gl->glGetIntegerv(GL_POLYGON_MODE, state.polygon.data());
    gl->glGetIntegerv(GL_VIEWPORT, state.viewport.data());
    gl->glGetIntegerv(GL_SCISSOR_BOX, state.scissor.data());
    gl->glGetBooleanv(GL_DEPTH_WRITEMASK, &state.depthMask);
    gl->glGetBooleanv(GL_COLOR_WRITEMASK, state.colorMask.data());
    for (std::size_t index = 0; index < kUnits.size(); ++index) {
      gl->glActiveTexture(GL_TEXTURE0 + kUnits[index]);
      gl->glGetIntegerv(GL_TEXTURE_BINDING_RECTANGLE, &state.textures[index]);
      gl->glGetIntegerv(GL_SAMPLER_BINDING, &state.samplers[index]);
    }
    gl->glActiveTexture(static_cast<GLenum>(state.active));
    for (std::size_t index = 0; index < kCapabilities.size(); ++index) state.enabled[index] = gl->glIsEnabled(kCapabilities[index]);
    return state;
  }
  bool operator==(const GlState& other) const {
    return program == other.program && vao == other.vao && active == other.active &&
        drawFramebuffer == other.drawFramebuffer && readFramebuffer == other.readFramebuffer &&
        textures == other.textures && samplers == other.samplers && polygon == other.polygon &&
        viewport == other.viewport && scissor == other.scissor && depthMask == other.depthMask &&
        colorMask == other.colorMask && enabled == other.enabled;
  }
};

class GlSentinels {
public:
  explicit GlSentinels(QOpenGLFunctions_4_1_Core* gl) : m_gl(gl) {}
  ~GlSentinels() {
    m_gl->glUseProgram(0);
    m_gl->glBindVertexArray(0);
    for (const auto unit : kUnits) { m_gl->glActiveTexture(GL_TEXTURE0 + unit); m_gl->glBindSampler(unit, 0); m_gl->glBindTexture(GL_TEXTURE_RECTANGLE, 0); }
    m_gl->glDeleteTextures(3, m_textures.data());
    m_gl->glDeleteSamplers(3, m_samplers.data());
    m_gl->glDeleteVertexArrays(1, &m_vao);
  }
  bool create() {
    if (!m_program.addShaderFromSourceCode(QOpenGLShader::Vertex, "#version 410 core\nvoid main(){gl_Position=vec4(0.0);}") ||
        !m_program.addShaderFromSourceCode(QOpenGLShader::Fragment, "#version 410 core\nout vec4 color; void main(){color=vec4(0.0);}") ||
        !m_program.link()) return false;
    m_gl->glGenVertexArrays(1, &m_vao);
    m_gl->glGenTextures(3, m_textures.data());
    m_gl->glGenSamplers(3, m_samplers.data());
    for (const auto sampler : m_samplers) m_gl->glSamplerParameteri(sampler, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_LINEAR);
    return m_gl->glGetError() == GL_NO_ERROR;
  }
  bool install(int width, int height) {
    m_gl->glDisable(GL_SCISSOR_TEST);
    m_gl->glDisable(GL_FRAMEBUFFER_SRGB);
    m_gl->glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
    m_gl->glClearColor(0.1f, 0.2f, 0.3f, 1.0f);
    m_gl->glClear(GL_COLOR_BUFFER_BIT);
    m_gl->glViewport(0, 0, width, height);
    m_gl->glScissor(0, 0, 1, 1);
    for (const auto capability : kCapabilities) m_gl->glEnable(capability);
    m_gl->glDepthMask(GL_FALSE);
    m_gl->glColorMask(GL_FALSE, GL_TRUE, GL_FALSE, GL_FALSE);
    m_gl->glPolygonMode(GL_FRONT_AND_BACK, GL_LINE);
    m_gl->glBindVertexArray(m_vao);
    m_program.bind();
    for (std::size_t index = 0; index < kUnits.size(); ++index) {
      m_gl->glActiveTexture(GL_TEXTURE0 + kUnits[index]);
      m_gl->glBindTexture(GL_TEXTURE_RECTANGLE, m_textures[index]);
      m_gl->glBindSampler(kUnits[index], m_samplers[index]);
    }
    m_gl->glActiveTexture(GL_TEXTURE3);
    return m_gl->glGetError() == GL_NO_ERROR;
  }
private:
  QOpenGLFunctions_4_1_Core* m_gl;
  QOpenGLShaderProgram m_program;
  GLuint m_vao = 0;
  std::array<GLuint, 3> m_textures{}, m_samplers{};
};

struct DisplayOwner {
  render::MacFrameDisplay display;
  ~DisplayOwner() { display.setFrame({}, {}, {}, {}, {}); display.release(nullptr); }
};

int compare(const QImage& actual, const QImage& expected) {
  if (actual.size() != expected.size()) return 256;
  int maximum = 0;
  for (int y = 0; y < expected.height(); ++y) for (int x = 0; x < expected.width(); ++x) {
    const auto a = actual.pixelColor(x, y), e = expected.pixelColor(x, y);
    maximum = std::max({maximum, std::abs(a.red() - e.red()), std::abs(a.green() - e.green()), std::abs(a.blue() - e.blue())});
  }
  return maximum;
}
} // namespace

int main(int argc, char** argv) {
  @autoreleasepool {
    if (!render::MacFrameMetalBuffer::supported()) { std::puts("SKIP: no supported unified-memory Metal device"); return 77; }
    QGuiApplication application(argc, argv);
    QSurfaceFormat format;
    format.setVersion(4, 1);
    format.setProfile(QSurfaceFormat::CoreProfile);
    QOpenGLContext context;
    context.setFormat(format);
    if (!context.create()) { std::fprintf(stderr, "OpenGL 4.1 context creation failed\n"); return 1; }
    QOffscreenSurface target;
    target.setFormat(context.format());
    target.create();
    if (!context.makeCurrent(&target)) { std::fprintf(stderr, "OpenGL context unavailable\n"); return 1; }
    auto* gl = QOpenGLVersionFunctionsFactory::get<QOpenGLFunctions_4_1_Core>(&context);
    if (!gl || !gl->initializeOpenGLFunctions()) return 1;
    GlSentinels sentinels(gl);
    if (!sentinels.create()) { std::fprintf(stderr, "OpenGL sentinel setup failed\n"); return 1; }
    auto surfaces = std::make_shared<render::MacFrameSurfaces>();
    auto light = std::make_shared<render::MacFrameMetalBuffer>();
    auto catcher = std::make_shared<render::MacFrameMetalBuffer>();
    std::string error;
    if (!surfaces->create(16, 8, error) || !light->wrap(surfaces->surface(0), error) ||
        !catcher->wrap(surfaces->surface(0, 1), error)) { std::fprintf(stderr, "%s\n", error.c_str()); return 1; }
    std::printf("Metal-to-GL allocation: base=%p allocation=%zu page=%ld packedRow=%zu\n",
        IOSurfaceGetBaseAddress(surfaces->surface(0)), IOSurfaceGetAllocSize(surfaces->surface(0)),
        sysconf(_SC_PAGESIZE), IOSurfaceGetBytesPerRow(surfaces->surface(0)));
    id<MTLBuffer> lightBuffer = (__bridge id<MTLBuffer>)light->handle();
    id<MTLBuffer> catcherBuffer = (__bridge id<MTLBuffer>)catcher->handle();
    if (lightBuffer.device.registryID != catcherBuffer.device.registryID) return 1;
    MetalWriter writer;
    if (!writer.create(lightBuffer.device, error)) { std::fprintf(stderr, "%s\n", error.c_str()); return 1; }
    std::printf("Metal-to-GL coherence: Metal=%s; OpenGL=%s; version=%s\n", lightBuffer.device.name.UTF8String,
                reinterpret_cast<const char*>(gl->glGetString(GL_RENDERER)),
                reinterpret_cast<const char*>(gl->glGetString(GL_VERSION)));
    const std::array<IOSurfaceID, 2> ids{IOSurfaceGetID(surfaces->surface(0)), IOSurfaceGetID(surfaces->surface(0, 1))};
    DisplayOwner owner;
    const QColor top(234, 208, 190), bottom(32, 97, 160);
    QImage previous;
    int maximum = 0, comparisons = 0;
    for (unsigned phase = 0; phase < 2; ++phase) {
      // The previous draw completed via glFinish. Release its reader, keeping
      // the adapter and GL bindings alive so reused IOSurface IDs are exercised.
      owner.display.setFrame({}, {}, top, bottom, {});
      if (!writer.write(surfaces, light, catcher, phase, error)) { std::fprintf(stderr, "%s\n", error.c_str()); return 1; }
      if (IOSurfaceGetID(surfaces->surface(0)) != ids[0] || IOSurfaceGetID(surfaces->surface(0, 1)) != ids[1]) return 1;
      auto lease = std::make_shared<render::MacFrameLease>(surfaces, 0);
      std::array<std::uint16_t, kValues> expectedLight{}, expectedCatcher{};
      for (std::size_t index = 0; index < kValues; ++index) {
        expectedLight[index] = lightValue(index, phase);
        expectedCatcher[index] = catcherValue(index, phase);
      }
      for (const auto& size : {std::pair<int, int>{5, 3}, {16, 8}}) for (const int scale : {1, 2}) {
        QOpenGLFramebufferObject framebuffer(size.first * scale, size.second * scale);
        if (!framebuffer.isValid() || !framebuffer.bind()) return 1;
        render::FrameInfo info;
        info.width = static_cast<std::uint32_t>(size.first);
        info.height = static_cast<std::uint32_t>(size.second);
        for (int transform = 0; transform < 3; ++transform) for (const float exposure : {-1.0f, 0.0f, 1.5f})
          for (const bool withCatcher : {false, true}) {
            if (!sentinels.install(framebuffer.width(), framebuffer.height())) return 1;
            const auto before = GlState::capture(gl);
            render::Look look;
            look.transform = static_cast<render::ViewTransform>(transform);
            look.exposure = exposure;
            info.catcher = withCatcher ? 1 : 0;
            owner.display.setFrame(lease, info, top, bottom, look);
            owner.display.draw();
            const auto after = GlState::capture(gl);
            const auto glError = gl->glGetError();
            if (!(before == after) || glError != GL_NO_ERROR) {
              std::fprintf(stderr, "Metal-to-GL draw changed caller state or failed: phase=%u glError=%u\n", phase, unsigned(glError));
              return 1;
            }
            const auto actual = framebuffer.toImage().convertToFormat(QImage::Format_RGB32);
            const auto image = render::displayImage(expectedLight.data(), size.first, size.second, top, bottom, look,
                                                     withCatcher ? expectedCatcher.data() : nullptr);
            const auto expected = scale == 1 ? image : filtered(image, framebuffer.width(), framebuffer.height());
            const auto difference = compare(actual, expected);
            maximum = std::max(maximum, difference);
            ++comparisons;
            if (difference > 1) {
              std::fprintf(stderr, "Metal-to-GL coherence mismatch: phase=%u frame=%dx%d scale=%d transform=%d exposure=%g catcher=%d difference=%d\n",
                           phase, size.first, size.second, scale, transform, double(exposure), int(withCatcher), difference);
              return 1;
            }
            if (size.first == 16 && scale == 1 && transform == 0 && exposure == 0.0f && withCatcher) {
              if (phase != 0 && compare(actual, previous) <= 1) {
                std::fprintf(stderr, "rewriting the same Metal buffers did not change the reused GL image\n"); return 1;
              }
              previous = actual;
            }
          }
      }
    }
    owner.display.setFrame({}, {}, top, bottom, {});
    if (surfaces->readers(0) != 0) { std::fprintf(stderr, "the completed display retained a reader\n"); return 1; }
    std::printf("PASS: two Metal writes to reused IOSurface IDs, %d GL/reference comparisons, GL state preserved; maximum channel difference %d\n",
                comparisons, maximum);
    return 0;
  }
}
