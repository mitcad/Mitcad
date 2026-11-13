// SPDX-License-Identifier: MIT
// The renderer on Cycles (docs/rendering.md). Cycles' headers need C++20,
// so this file is compiled on its own (mitcad_cycles_renderer) and nothing
// else includes them.
#include "render/RenderCamera.hpp"
#include "render/RenderDevices.hpp"
#include "render/Renderer.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <initializer_list>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <OpenImageIO/imageio.h>

#include "device/device.h"
#include "integrator/denoiser.h"
#include "scene/background.h"
#include "scene/camera.h"
#include "scene/film.h"
#include "scene/integrator.h"
#include "scene/light.h"
#include "scene/mesh.h"
#include "scene/object.h"
#include "scene/pass.h"
#include "scene/scene.h"
#include "scene/shader.h"
#include "scene/shader_graph.h"
#include "scene/shader_nodes.h"
#include "session/buffers.h"
#include "session/display_driver.h"
#include "session/output_driver.h"
#include "session/session.h"
#include "util/colorspace.h"
#include "util/debug.h"
#include "util/log.h"
#include "util/path.h"
#include "util/tbb.h"
#include "util/transform.h"
#include "util/version.h"

#ifdef WITH_OPENIMAGEDENOISE
#include <OpenImageDenoise/oidn.h>
#endif

namespace mitcad::render {

namespace {

using Clock = std::chrono::steady_clock;

// Frames of the interactive render. Cycles calls it from its render thread
// (update_begin, map, unmap, update_end) before it counts the batch's
// samples, so the frame goes to a buffer of the renderer's (`begin`) and
// the renderer passes it on once it knows the samples (`end`, then
// CyclesRenderer::publishFrame).
class SinkDisplayDriver : public ccl::DisplayDriver {
public:
  SinkDisplayDriver(std::function<std::uint16_t*(int, int, int, int)> begin, std::function<void()> end,
                    std::function<std::shared_ptr<DirectFrame>()> direct, bool metal, std::function<void()> used)
      : m_begin(std::move(begin)), m_end(std::move(end)), m_direct(std::move(direct)),
        m_metal(metal), m_used(std::move(used)) {}

  void next_tile_begin() override {}

  bool update_begin(const Params& params, const int width, const int height) override {
    m_pixels = m_begin(width, height, params.full_size.x, params.full_size.y);
    m_update = m_direct();
    if (m_pixels && m_metal && (!m_update || m_update->nativeMetalBuffer() == nullptr)) {
      if (m_update) m_update->discard();
      m_update.reset();
      m_pixels = nullptr;
    }
    return m_pixels != nullptr;
  }

  void update_end() override {
    m_pixels = nullptr;
    m_end();
    // Cycles has replaced the old raw interop handle and synchronized its film
    // conversion queue before update_end. The previous allocation may now go.
    m_previous.reset();
    m_update.reset();
  }

  ccl::GraphicsInteropDevice graphics_interop_get_device() override {
    ccl::GraphicsInteropDevice result;
    if (m_metal) result.type = ccl::GraphicsInteropDevice::METAL;
    return result;
  }
  void graphics_interop_update_buffer() override {
    if (!m_update || !m_update->nativeMetalBuffer()) { graphics_interop_buffer_.clear(); return; }
    m_previous = m_current;
    m_current = m_update;
    graphics_interop_buffer_.assign(ccl::GraphicsInteropDevice::METAL,
        static_cast<std::int64_t>(reinterpret_cast<std::intptr_t>(m_current->nativeMetalBuffer())),
        m_current->nativeMetalBufferSize());
    m_used();
  }

  ccl::half4* map_texture_buffer() override { return reinterpret_cast<ccl::half4*>(m_pixels); }
  void unmap_texture_buffer() override {}
  void zero() override {}
  void draw(const Params&) override {}

private:
  std::function<std::uint16_t*(int, int, int, int)> m_begin;
  std::function<void()> m_end;
  std::function<std::shared_ptr<DirectFrame>()> m_direct;
  bool m_metal = false;
  std::function<void()> m_used;
  // The driver owns the external handles even after frame publication. Cycles'
  // device interop object removes residency before destroying this driver.
  std::shared_ptr<DirectFrame> m_update, m_current, m_previous;
  std::uint16_t* m_pixels = nullptr;
};

// Cycles' tiles of the render's passes (OutputDriver): the image so far
// every second or two and the finished one of a non-interactive render
// (the final render, benchmarks), and in the application, before each
// display update, the passes the display driver does not carry (the
// ground's catcher factors, CyclesRenderer::readPasses).
class TileOutputDriver : public ccl::OutputDriver {
public:
  using Deliver = std::function<bool(const Tile& tile, bool last)>;
  explicit TileOutputDriver(Deliver deliver) : m_deliver(std::move(deliver)) {}
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
  using Display = std::function<void(const Tile&, const DisplayInfo&)>;
  TileOutputDriver(Deliver deliver, Display display)
      : m_deliver(std::move(deliver)), m_display(std::move(display)) {}
  void update_display_tile(const Tile& tile, const DisplayInfo& display) override {
    if (m_display) m_display(tile, display);
  }
#endif

  void write_render_tile(const Tile& tile) override { m_deliver(tile, true); }
  bool update_render_tile(const Tile& tile) override { return m_deliver(tile, false); }

private:
  Deliver m_deliver;
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
  Display m_display;
#endif
};

// Float pixels (4 channels) as the half floats of a frame; in parallel (a
// frame of the whole view at every update, about 0.15 s on one core at
// 1920 x 1080).
void toHalf(const float* pixels, std::size_t count, std::uint16_t* target) {
  constexpr std::size_t kBlock = std::size_t(1) << 16;
  ccl::parallel_for(std::size_t(0), (count + kBlock - 1) / kBlock, [&](std::size_t block) {
    const std::size_t end = std::min(count, (block + 1) * kBlock);
    for (std::size_t i = block * kBlock; i < end; ++i) {
      target[i] = static_cast<std::uint16_t>(ccl::float_to_half_display(pixels[i]));
    }
  });
}

// The ground's catcher factors composited in the worker (the camera sees
// the environment): light + (1 - alpha) * catcher * background, opaque.
void compositeOverEnvironment(float* light, const float* catcher, const float* background, std::size_t pixels) {
  for (std::size_t p = 0; p < pixels; ++p) {
    float* out = light + p * 4;
    const float rest = 1.0f - std::clamp(out[3], 0.0f, 1.0f);
    for (int c = 0; c < 3; ++c) {
      out[c] += rest * catcher[p * 4 + c] * background[p * 4 + c];
    }
    out[3] = 1.0f;
  }
}

ccl::float3 float3Of(const std::array<double, 3>& v) {
  return ccl::make_float3(static_cast<float>(v[0]), static_cast<float>(v[1]), static_cast<float>(v[2]));
}

// A light of a built-in setup: a sun, soft as a light at infinity of the
// given angular diameter, so that the setup does not depend on the scene's
// size. Directions are where the light comes from (degrees: azimuth
// counter-clockwise from X, elevation above the horizon).
struct StageLight {
  float azimuth;
  float elevation;
  float angle; // radians
  float strength; // irradiance where it falls straight
  ccl::float3 color;
};

// A built-in studio: its surroundings as a gradient over the height of the
// direction (below the horizon, at it, above it, at the zenith), their
// strength, its lights, and the albedo of the ground as bodies see it.
struct Studio {
  ccl::float3 nadir;
  ccl::float3 belowHorizon;
  ccl::float3 horizon;
  ccl::float3 zenith;
  float worldStrength;
  std::vector<StageLight> lights;
  float groundAlbedo;
};

const Studio& studioOf(EnvironmentData::Preset preset) {
  const ccl::float3 white = ccl::make_float3(1.0f, 1.0f, 1.0f);
  const ccl::float3 warm = ccl::make_float3(1.0f, 0.97f, 0.93f);
  const ccl::float3 cool = ccl::make_float3(0.92f, 0.96f, 1.0f);
  const auto grey = [](float v) { return ccl::make_float3(v, v, v); };
  // Soft key light from the front left above, a dim fill from the front
  // right, a top light from behind; light grey surroundings, a little
  // darker below the horizon (a seamless studio backdrop).
  static const Studio studio{grey(0.5f), grey(0.58f), grey(0.8f), grey(0.95f), 0.45f,
                             {{225.0f, 50.0f, 0.35f, 2.6f, warm},
                              {320.0f, 15.0f, 1.0f, 0.5f, cool},
                              {90.0f, 70.0f, 0.8f, 0.7f, white}},
                             0.6f};
  // High key: bright white all around, one very soft light.
  static const Studio studioWhite{grey(0.85f), grey(0.92f), grey(1.0f), grey(1.0f), 1.0f,
                                  {{225.0f, 60.0f, 1.2f, 1.0f, white}},
                                  0.85f};
  // Dark surroundings, a narrower key light and two low rim lights from
  // behind.
  static const Studio studioDark{grey(0.01f), grey(0.012f), grey(0.03f), grey(0.05f), 1.0f,
                                 {{225.0f, 40.0f, 0.3f, 2.4f, warm},
                                  {45.0f, 10.0f, 0.25f, 1.5f, cool},
                                  {135.0f, 10.0f, 0.25f, 1.5f, cool}},
                                 0.05f};
  switch (preset) {
  case EnvironmentData::Preset::StudioWhite:
    return studioWhite;
  case EnvironmentData::Preset::StudioDark:
    return studioDark;
  default:
    return studio;
  }
}

float smooth(float edge0, float edge1, float x) {
  const float t = std::clamp((x - edge0) / (edge1 - edge0), 0.0f, 1.0f);
  return t * t * (3.0f - 2.0f * t);
}

ccl::float3 blend(const ccl::float3& a, const ccl::float3& b, float t) { return a + (b - a) * t; }

// The studio's surroundings at the height z (-1 down, 1 up) of a direction.
ccl::float3 studioColor(const Studio& studio, float z) {
  if (z < 0.0f) {
    return blend(studio.belowHorizon, studio.nadir, smooth(0.0f, 0.3f, -z));
  }
  return blend(studio.horizon, studio.zenith, smooth(0.0f, 1.0f, z));
}

// The unit vector toward a direction given by azimuth and elevation
// (radians).
ccl::float3 directionOf(float azimuth, float elevation) {
  return ccl::make_float3(std::cos(elevation) * std::cos(azimuth), std::cos(elevation) * std::sin(azimuth),
                          std::sin(elevation));
}

constexpr float kDegree = 3.14159265358979f / 180.0f;
// The passes the renderer reads besides the display's (mitcad#54).
constexpr const char* kCatcherDenoised = "catcher";
constexpr const char* kCatcherNoisy = "catcher_noisy";
constexpr const char* kBackground = "background";
// The outdoor preset: the sky's strength (Cycles' Hosek-Wilkie sky is not
// in physical units), the sun's irradiance and the ground's radiance below
// the horizon, as factors of the sky's.
constexpr float kSkyStrength = 1.6f;
constexpr float kSunStrength = 4.0f;
constexpr float kOutdoorGround = 0.12f;

// Devices (mitcad#50): Cycles' paths and the devices it finds; defined
// below.
void initCycles(const std::string& kernelFolder);
bool findCyclesDevice(const std::string& id, ccl::DeviceInfo& found);
ccl::vector<ccl::DeviceInfo> allCyclesDevices();
DeviceInfo deviceInfoOf(const ccl::DeviceInfo& info);
std::string probeOidn(const ccl::DeviceInfo& info);

// What Cycles' GPU denoiser says when Open Image Denoise gives it no device.
constexpr const char* kOidnDeviceFailed = "Failed to create OIDN device";

class CyclesRenderer : public Renderer {
public:
  CyclesRenderer(FrameSink& sink, const RendererOptions& options)
      : m_sink(sink), m_interactive(options.interactive), m_options(options) {}

  ~CyclesRenderer() override {
    if (m_session) {
      m_session->cancel(true);
      m_session.reset();
    }
    m_frame.direct.reset();
  }

  bool start(std::string& error) {
    initCycles(m_options.kernelFolder);
    if (!findCyclesDevice(m_options.device.id, m_params.device)) {
      error = m_options.device.id.empty() ? "Cycles found no CPU device"
                                          : "Cycles has no device " + m_options.device.id;
      return false;
    }
    m_device = m_params.device.description;
    m_deviceInfo = deviceInfoOf(m_params.device);
    m_params.device.use_hardware_raytracing = automaticRayTracing(
        m_deviceInfo.type, m_params.device.use_hardware_raytracing, m_params.device.use_metalrt_by_default);
    // SessionParams defaults to a CPU denoising device even when rendering
    // on Metal. Select it explicitly and let Cycles validate OIDN support.
    if (!findCyclesDevice({}, m_params.denoise_device)) {
      error = "Cycles found no CPU denoising device";
      return false;
    }
    const std::string& denoising = m_options.denoisingDevice;
    if (!denoising.empty() && denoising != "auto" && denoising != "cpu" && denoising != "metal") {
      error = "Invalid test denoising device: " + denoising;
      return false;
    }
    if (!m_options.cpuDenoisingFallback && denoising != "cpu") {
      if (denoising == "metal") {
        for (const ccl::DeviceInfo& candidate : allCyclesDevices()) {
          if (candidate.type == ccl::DEVICE_METAL && (candidate.denoisers & ccl::DENOISER_OPENIMAGEDENOISE) != 0) {
            m_params.denoise_device = candidate;
            break;
          }
        }
        if (m_params.denoise_device.type == ccl::DEVICE_CPU) {
          m_deviceInfo.denoiser.message = "Metal OIDN is unavailable; using CPU denoising";
        }
      } else if (m_deviceInfo.denoiserAvailable) {
        m_params.denoise_device = m_params.device;
      } else if (m_params.device.type != ccl::DEVICE_CPU) {
        m_deviceInfo.denoiser.message = "OIDN is unavailable on the render device; using CPU denoising";
      }
    }
    m_params.denoise_device.use_hardware_raytracing = automaticRayTracing(
        ccl::Device::string_from_type(m_params.denoise_device.type),
        m_params.denoise_device.use_hardware_raytracing, m_params.denoise_device.use_metalrt_by_default);
    m_deviceInfo.denoiser.id = m_params.denoise_device.id;
    m_deviceInfo.denoiser.type = ccl::Device::string_from_type(m_params.denoise_device.type);
    m_deviceInfo.denoiser.name = m_params.denoise_device.description;
    m_params.background = !m_interactive;
    // Not headless: a non-interactive render hands its image so far to the
    // output driver every second or two (the final render's preview).
    m_params.headless = false;
    m_params.samples = m_samples;
    m_params.use_auto_tile = false;
    // In the application, one core stays free for the user interface.
    const unsigned cores = std::thread::hardware_concurrency();
    m_params.threads = m_interactive && cores > 2 ? static_cast<int>(cores) - 1 : 0;
    ccl::SceneParams sceneParams;
    sceneParams.shadingsystem = ccl::SHADINGSYSTEM_SVM;
    sceneParams.bvh_type = m_interactive ? ccl::BVH_TYPE_DYNAMIC : ccl::BVH_TYPE_STATIC;
    sceneParams.background = !m_interactive;
    m_session = std::make_unique<ccl::Session>(m_params, sceneParams);
    if (m_params.device.type == ccl::DEVICE_METAL && m_session->device) {
      // Query the device after construction: Cycles' CYCLES_METALRT
      // environment override can change its state from the default policy.
      m_deviceInfo.metalRT = (m_session->device->get_bvh_layout_mask(0) & ccl::BVH_LAYOUT_METAL) != 0;
    }
    if (m_interactive) {
      m_session->set_display_driver(std::make_unique<SinkDisplayDriver>(
          [this](int width, int height, int fullWidth, int fullHeight) {
            return frameBuffer(width, height, fullWidth, fullHeight);
          },
          [this] {
            const std::lock_guard<std::mutex> frame(m_frameMutex);
            m_frame.pending = true;
          }, [this] { const std::lock_guard<std::mutex> frame(m_frameMutex); return m_frame.direct; },
          m_params.device.type == ccl::DEVICE_METAL && m_sink.metalDisplayBuffers(),
          [this] { m_sink.displayInteropUsed(); }));
      // The passes the display driver does not carry, read just before
      // each display update.
      m_session->set_output_driver(std::make_unique<TileOutputDriver>(
          [this](const ccl::OutputDriver::Tile& tile, bool last) {
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
            if (m_sink.directDisplayBuffers()) return !last;
#endif
            return !last && readPasses(tile);
          }
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
          , [this](const ccl::OutputDriver::Tile& tile, const ccl::OutputDriver::DisplayInfo& display) {
            if (m_sink.directDisplayBuffers()) readNativePasses(tile, display);
          }
#endif
          ));
    } else {
      m_session->set_output_driver(std::make_unique<TileOutputDriver>(
          [this](const ccl::OutputDriver::Tile& tile, bool last) { return deliver(tile, last); }));
    }
    m_session->progress.set_update_callback([this] { progressed(); });
    setUpScene();
    return true;
  }

  std::string description() const override {
    return std::string("Cycles ") + CYCLES_VERSION_STRING + " on " + m_device;
  }

  SceneChanges updateScene(const SceneUpdate& update) override {
    SceneChanges changes;
    {
      const ccl::thread_scoped_lock lock(m_session->scene->mutex);
      ccl::Scene& scene = *m_session->scene;
      noteSceneBuilt(scene);
      // Texture images are checked again in each update that gives a body
      // their material (mitcad#538, textureReadable).
      m_textures.clear();
      for (const auto& [key, mesh] : update.meshes) {
        if (!mesh.indices.empty() && m_meshes.find(key) == m_meshes.end()) {
          m_meshes.emplace(key, MeshEntry{std::make_shared<const MeshData>(mesh), boundsOf(mesh)});
        }
      }
      // The bodies by id: new ones, another mesh or material, another
      // placement, the same.
      std::map<std::string, const BodyData*> wanted;
      for (const BodyData& body : update.bodies) {
        if (m_meshes.find(body.mesh) != m_meshes.end()) {
          wanted.emplace(body.id, &body);
        }
      }
      for (auto it = m_bodies.begin(); it != m_bodies.end();) {
        if (wanted.find(it->first) == wanted.end()) {
          changes.removed.push_back(it->first);
          removeObject(scene, it->second);
          it = m_bodies.erase(it);
        } else {
          ++it;
        }
      }
      for (const auto& [id, body] : wanted) {
        auto existing = m_bodies.find(id);
        if (existing == m_bodies.end()) {
          m_bodies.emplace(id, addObject(scene, *body));
          changes.added.push_back(id);
          continue;
        }
        BodyEntry& entry = existing->second;
        if (entry.mesh != body->mesh || entry.lookKey != body->lookKey()) {
          removeObject(scene, entry);
          entry = addObject(scene, *body);
          changes.changed.push_back(id);
        } else if (entry.transform != body->transform) {
          entry.transform = body->transform;
          entry.object->set_tfm(transformOf(body->transform));
          entry.object->tag_update(&scene);
          changes.moved.push_back(id);
        } else {
          ++changes.kept;
        }
      }
      // Meshes the application no longer keeps for the worker.
      for (const std::string& key : update.released) {
        const bool used = std::any_of(m_bodies.begin(), m_bodies.end(),
                                      [&key](const auto& body) { return body.second.mesh == key; });
        if (!used) {
          m_meshes.erase(key);
        }
      }
      const bool bodiesChanged =
          !changes.added.empty() || !changes.changed.empty() || !changes.moved.empty() || !changes.removed.empty();
      if (bodiesChanged || update.ground != m_sceneGround) {
        // The ground again only when the bodies' bounds changed.
        ccl::BoundBox bounds = ccl::BoundBox::empty;
        for (const auto& [id, body] : m_bodies) {
          const ccl::BoundBox& local = m_meshes.at(body.mesh).bounds;
          if (local.valid()) {
            bounds.grow(local.transformed(&body.object->get_tfm()));
          }
        }
        const bool sameBounds = bounds.valid() == m_bounds.valid() &&
                                (!bounds.valid() || (ccl::len(bounds.min - m_bounds.min) == 0.0f &&
                                                     ccl::len(bounds.max - m_bounds.max) == 0.0f));
        if (!sameBounds || update.ground != m_sceneGround) {
          m_bounds = bounds;
          m_sceneGround = update.ground;
          buildGround(scene);
          placeCamera(scene); // an orthographic one behind the new bounds
        }
        changes.restarted = true;
      }
      changes.meshes = static_cast<int>(m_meshes.size());
      changes.warnings = std::move(m_warnings);
      m_warnings.clear();
      for (const auto& [key, geometry] : m_geometry) {
        if (geometry.users > 1) {
          changes.instanced += geometry.users;
        }
      }
      if (changes.restarted) {
        restart();
      }
    }
    return changes;
  }

  std::string setEnvironment(const EnvironmentData& environment) override {
    std::string warning;
    {
      const ccl::thread_scoped_lock lock(m_session->scene->mutex);
      ccl::Scene& s = *m_session->scene;
      noteSceneBuilt(s);
      m_environment = environment;
      m_seesEnvironment = environment.background;
      if (m_environment.preset == EnvironmentData::Preset::Image) {
        std::string why;
        if (m_environment.image.empty()) {
          why = "no environment image is chosen";
        } else if (!readable(m_environment.image, why)) {
          why = "cannot read the environment image " + m_environment.image + ": " + why;
        }
        if (!why.empty()) {
          warning = why + "; the studio lights the scene instead";
          m_environment.preset = EnvironmentData::Preset::Studio;
        }
      }
      buildWorld(s);
      buildGround(s);
      placeCamera(s); // the ground may have come or gone
      buildLights(s);
      restart();
    }
    return warning;
  }

  void setPaused(bool paused) override { m_session->set_pause(paused); }
  bool isPaused() const override {
    std::string status;
    std::string detail;
    m_session->progress.get_status(status, detail);
    return status == "Rendering Paused";
  }

  void setView(const ViewData& view) override {
    if (view.width <= 0 || view.height <= 0) {
      return;
    }
    {
      const ccl::thread_scoped_lock lock(m_session->scene->mutex);
      noteSceneBuilt(*m_session->scene);
      ccl::Camera& camera = *m_session->scene->camera;
      m_viewData = view;
      m_hasView = true;
      const ccl::float3 eye = float3Of(view.eye);
      const float aspect = static_cast<float>(view.width) / static_cast<float>(view.height);
      const auto halfHeight = static_cast<float>(view.halfHeight);
      if (view.perspective) {
        // Nothing the 3D view shows is nearer to the eye than its own near
        // plane, which is farther than this.
        const auto distance = static_cast<float>(ccl::len(float3Of(view.target) - eye));
        camera.set_camera_type(ccl::CAMERA_PERSPECTIVE);
        camera.set_fov(2.0f * std::atan(halfHeight / std::max(distance, 1e-6f)));
        camera.set_viewplane_left(-aspect);
        camera.set_viewplane_right(aspect);
        camera.set_viewplane_bottom(-1.0f);
        camera.set_viewplane_top(1.0f);
        camera.set_nearclip(std::max(1e-3f, distance * 1e-4f));
      } else {
        // An orthographic camera's view plane is in scene units (mm); the
        // camera stands behind the scene (placeCamera).
        camera.set_camera_type(ccl::CAMERA_ORTHOGRAPHIC);
        camera.set_viewplane_left(-halfHeight * aspect);
        camera.set_viewplane_right(halfHeight * aspect);
        camera.set_viewplane_bottom(-halfHeight);
        camera.set_viewplane_top(halfHeight);
        camera.set_nearclip(0.0f);
      }
      camera.set_farclip(1e8f);
      camera.set_full_width(view.width);
      camera.set_full_height(view.height);
      m_width = view.width;
      m_height = view.height;
      m_view = view.sequence;
      if (groundFromBelow() != m_groundFromBelow) {
        buildGround(*m_session->scene); // the camera went through the ground's plane
      }
      placeCamera(*m_session->scene);
      // Lights relative to the camera follow it.
      if (std::any_of(m_environment.lights.begin(), m_environment.lights.end(),
                      [](const LightData& light) { return light.camera; })) {
        buildLights(*m_session->scene);
      }
      restart();
    }
  }

  void setSamples(int samples) override {
    m_samples = std::max(1, samples);
    m_params.samples = m_samples;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      planDenoising();
    }
    const ccl::thread_scoped_lock lock(m_session->scene->mutex);
    m_session->scene->integrator->set_aa_samples(m_samples);
    applyStage(*m_session->scene->integrator, 0);
    m_session->set_samples(m_samples);
    restart();
  }

  void setPreviews(const std::vector<int>& samples) override {
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      m_previews = samples;
      std::sort(m_previews.begin(), m_previews.end());
      planDenoising();
    }
    const ccl::thread_scoped_lock lock(m_session->scene->mutex);
    applyStage(*m_session->scene->integrator, 0);
  }

  void wait() override { m_session->wait(); }

  void setFinal(bool denoise, double timeLimit) override {
    m_params.time_limit = std::max(0.0, timeLimit);
    const ccl::thread_scoped_lock lock(m_session->scene->mutex);
    m_session->scene->integrator->set_use_denoise(denoise);
    m_denoise = denoise;
    updatePasses(*m_session->scene);
    m_session->set_time_limit(m_params.time_limit);
    restart();
  }

  int currentSample() const override { return m_session->progress.get_current_sample(); }

  DeviceInfo device() const override {
    const std::lock_guard<std::mutex> lock(m_deviceMutex);
    return m_deviceInfo;
  }

  // Cycles stops the render on a device error (Session::run_main_render_loop)
  // and keeps the message in its progress.
  std::string deviceError() const override {
    if (!m_session->progress.get_error()) {
      return {};
    }
    std::string message = m_session->progress.get_error_message();
    if (message.empty()) {
      return "unknown device error";
    }
    // Cycles does not read Open Image Denoise's own error (mitcad#202):
    // OIDN's answer to the same call, once.
    if (message.find(kOidnDeviceFailed) != std::string::npos && m_params.denoise_device.type != ccl::DEVICE_CPU) {
      std::call_once(m_oidnProbed, [this] { m_oidnProbe = probeOidn(m_params.denoise_device); });
      message += "; Open Image Denoise's own probe: " + m_oidnProbe;
    }
    return message;
  }

private:
  // The patched dependency exposes the actual pass state. Older prefixes
  // retain the original scheduling flag without confirming a device.
  bool hasDenoisedResult(bool scheduled) const {
#ifdef MITCAD_CYCLES_DENOISER_DIAGNOSTICS
    return scheduled && m_session->has_denoised_result();
#else
    return scheduled;
#endif
  }

  // Called on Cycles' render thread after successful denoising, never from
  // device enumeration. The optional getter exposes the actual denoiser,
  // including Cycles' own CPU fallback, instead of predicting execution.
  bool recordDenoising(bool denoised) {
    if (!denoised) {
      return true;
    }
    if (!m_options.cpuDenoisingFallback && !m_options.testDenoisingFailure.empty()) {
#ifdef MITCAD_CYCLES_DENOISER_DIAGNOSTICS
      const ccl::Denoiser* denoiser = m_session->get_denoiser();
      if (denoiser != nullptr && denoiser->get_denoiser_device() != nullptr) {
        // Let Cycles forward this error through its normal render loop,
        // including a denoiser on a device separate from path tracing.
        denoiser->get_denoiser_device()->set_error("OIDN test failure: " + m_options.testDenoisingFailure);
      }
#else
      m_session->device->set_error("OIDN test failure: " + m_options.testDenoisingFailure);
#endif
      return false;
    }
    if (m_denoisingReported) {
      return true;
    }
    m_denoisingReported = true;
#ifdef MITCAD_CYCLES_DENOISER_DIAGNOSTICS
    const ccl::Denoiser* denoiser = m_session->get_denoiser();
    if (denoiser == nullptr || denoiser->get_denoiser_device() == nullptr) {
      return true;
    }
    const ccl::DeviceInfo& info = denoiser->get_denoiser_device()->info;
    DeviceInfo device;
    {
      const std::lock_guard<std::mutex> lock(m_deviceMutex);
      m_deviceInfo.denoiser.id = info.id;
      m_deviceInfo.denoiser.type = ccl::Device::string_from_type(info.type);
      m_deviceInfo.denoiser.name = info.description;
      m_deviceInfo.denoiser.executed = true;
      m_deviceInfo.denoisesOnDevice = info.type != ccl::DEVICE_CPU && info.id == m_deviceInfo.id;
      device = m_deviceInfo;
    }
    m_sink.denoisingDevice(device);
#endif
    return true;
  }

  // The world (the default environment until the application sends its
  // own, buildWorld) and the film; denoising with Open Image Denoise.
  void setUpScene() {
    const ccl::thread_scoped_lock lock(m_session->scene->mutex);
    ccl::Scene& scene = *m_session->scene;
    // The ground is Cycles' shadow catcher with its own passes (mitcad#54):
    // the display pass is the bodies over a transparent ground (the
    // shadow catcher's matte), and the catcher pass the factor by which the
    // ground changes what is behind it, per colour (readPasses). The
    // approximate shadow catcher would fold the ground into the alpha, the
    // average of the colours: reflections only grey and darker.
    scene.film->set_use_approximate_shadow_catcher(false);
    buildWorld(scene);

    ccl::Integrator& integrator = *scene.integrator;
    integrator.set_aa_samples(m_samples);
    integrator.set_use_denoise(true);
    integrator.set_denoise_use_gpu(m_params.denoise_device.type != ccl::DEVICE_CPU);
    integrator.set_denoiser_type(ccl::DENOISER_OPENIMAGEDENOISE);
    integrator.set_denoiser_prefilter(ccl::DENOISER_PREFILTER_FAST);
    integrator.set_denoiser_passes(ccl::DENOISER_PASS_ALBEDO | ccl::DENOISER_PASS_NORMAL);
    // At the points of the schedule only (planDenoising): Cycles' own
    // interactive denoising, at every update once it started, makes a CPU
    // render two to four times slower (1920 x 1080, 64 samples: 13 s with
    // one denoise, 29 s denoising at intervals with OIDN's fast quality,
    // 40 s with its high quality).
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      planDenoising();
    }
    applyStage(integrator, 0);
    integrator.set_max_bounce(6);
    // Adaptive sampling would hold back display updates (and denoise
    // batches that are not shown); with Cycles' automatic minimum of 64
    // samples it stops no pixel early at the application's sample counts.
    integrator.set_use_adaptive_sampling(!m_interactive);

    // The display pass of the final render (denoised once there is a
    // denoised result: a pass's mode is denoised unless set), and the
    // ground's catcher factors, denoised and noisy (the application shows
    // denoised and noisy frames alike).
    // The catcher's passes come with a ground (updatePasses).
    addPass(scene, "combined", ccl::PASS_COMBINED, ccl::PassMode::DENOISED);
  }

  // The passes besides the display's (under the scene's lock): the
  // catcher's with a ground (denoised too when the render denoises), the
  // background's when the camera sees the environment. Passes cost memory,
  // and Open Image Denoise denoises the catcher's once more.
  void updatePasses(ccl::Scene& scene) {
    const bool ground = m_hasGround.load();
    keepPass(scene, m_catcherNoisy, ground, kCatcherNoisy, ccl::PASS_SHADOW_CATCHER, ccl::PassMode::NOISY);
    keepPass(scene, m_catcherDenoised, ground && m_denoise, kCatcherDenoised, ccl::PASS_SHADOW_CATCHER,
             ccl::PassMode::DENOISED);
    keepPass(scene, m_backgroundPass, ground && m_environment.background, kBackground, ccl::PASS_BACKGROUND,
             ccl::PassMode::NOISY);
  }

  static ccl::Pass* addPass(ccl::Scene& scene, const char* name, ccl::PassType type, ccl::PassMode mode) {
    ccl::Pass* pass = scene.create_node<ccl::Pass>();
    pass->set_name(ccl::ustring(name));
    pass->set_type(type);
    pass->set_mode(mode);
    return pass;
  }

  // A pass that is there or not (under the scene's lock): Cycles' film
  // makes passes of the same kind noisy when nothing denoises, and two of
  // them would not share the kernel's buffer.
  static void keepPass(ccl::Scene& scene, ccl::Pass*& pass, bool wanted, const char* name, ccl::PassType type,
                       ccl::PassMode mode) {
    if (wanted && pass == nullptr) {
      pass = addPass(scene, name, type, mode);
    } else if (!wanted && pass != nullptr) {
      scene.delete_node(pass);
      pass = nullptr;
    }
  }

  struct MeshEntry {
    std::shared_ptr<const MeshData> data; // kept to make Cycles meshes of other materials
    ccl::BoundBox bounds = ccl::BoundBox::empty;
  };
  // A Cycles mesh: a mesh with a material (Cycles' shaders belong to the
  // geometry), shared by the bodies that show both.
  struct GeometryEntry {
    ccl::Mesh* mesh = nullptr;
    int users = 0;
  };
  struct BodyEntry {
    ccl::Object* object = nullptr;
    std::string mesh;
    std::string lookKey; // the materials of the body and its faces
    std::array<float, 12> transform{};
  };

  static ccl::Transform transformOf(const std::array<float, 12>& t) {
    return ccl::make_transform(t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7], t[8], t[9], t[10], t[11]);
  }

  static ccl::BoundBox boundsOf(const MeshData& data) {
    ccl::BoundBox bounds = ccl::BoundBox::empty;
    for (std::size_t v = 0; v + 2 < data.positions.size(); v += 3) {
      bounds.grow(ccl::make_float3(data.positions[v], data.positions[v + 1], data.positions[v + 2]));
    }
    return bounds;
  }

  // Which images of a material's texture set can be read (textureReadable):
  // a shader is made for a material with the images that can be read.
  struct ImagesRead {
    bool color = false;
    bool normal = false;
    bool roughness = false;
    bool metalness = false;
    bool occlusion = false;

    bool any() const { return color || normal || roughness || metalness || occlusion; }
    std::string key() const {
      std::string text = "images ";
      for (const bool found : {color, normal, roughness, metalness, occlusion}) {
        text += found ? '1' : '0';
      }
      return text;
    }
  };

  ImagesRead imagesRead(const MaterialData& m) {
    ImagesRead found;
    found.color = !m.texture.empty() && textureReadable(m.texture, false);
    const auto mapImage = [this](const std::string& file) { return !file.empty() && textureReadable(file, true); };
    found.normal = mapImage(m.normalMap);
    found.roughness = mapImage(m.roughnessMap);
    found.metalness = mapImage(m.metalnessMap);
    found.occlusion = mapImage(m.occlusionMap);
    return found;
  }

  // The shader of a material, shared by the meshes that use it; made once
  // per material and set of images that can be read (Cycles never deletes
  // shaders). The images are checked whenever a Cycles mesh takes the
  // material (mitcad#538): one that cannot be read any more gives its
  // warning, as at the start, and a shader without it, instead of the one
  // made while it could be read.
  //
  // A shader made earlier may have had no user when Cycles last compiled
  // the shaders (the body's material changed to another and now back: an
  // undo, a texture removed and given back); Cycles compiles such a shader
  // to nothing (SVMCompiler::compile_type skips a shader without
  // references), so a mesh that takes it again rendered black until
  // something else made Cycles compile all shaders (mitcad#538). Cycles'
  // tag_used marks it for compiling when it has no user yet, before the
  // mesh references it (set_used_shaders in geometryOf).
  ccl::Shader* shaderOf(ccl::Scene& scene, const MaterialData& material, const std::string& key) {
    const ImagesRead found = imagesRead(material);
    const std::string shaderKey = key + '\n' + found.key();
    auto existing = m_shaders.find(shaderKey);
    if (existing != m_shaders.end()) {
      existing->second->tag_used(&scene);
      return existing->second;
    }
    ccl::Shader* shader = addMaterial(scene, material, found);
    m_shaders.emplace(shaderKey, shader);
    return shader;
  }

  // The Cycles mesh of a body's mesh and materials (the body's and its
  // faces'), made when no other body shows the same.
  ccl::Mesh* geometryOf(ccl::Scene& scene, const BodyData& body) {
    GeometryEntry& entry = m_geometry[{body.mesh, body.lookKey()}];
    ++entry.users;
    if (entry.mesh != nullptr) {
      return entry.mesh;
    }
    const MeshData& data = *m_meshes.at(body.mesh).data;
    const auto vertices = static_cast<int>(data.positions.size() / 3);
    const auto triangles = static_cast<int>(data.indices.size() / 3);
    ccl::Mesh* mesh = scene.create_node<ccl::Mesh>();
    // The body's material first, then each face group's (mitcad#53); each
    // triangle the shader of its face.
    ccl::array<ccl::Node*> used;
    used.push_back_slow(shaderOf(scene, body.material, body.materialKey));
    std::vector<int> faceShader(data.faceTriangles.size(), 0);
    for (const FaceMaterial& group : body.faces) {
      used.push_back_slow(shaderOf(scene, group.material, group.materialKey));
      for (const std::uint32_t face : group.faces) {
        if (face < faceShader.size()) {
          faceShader[face] = static_cast<int>(used.size()) - 1;
        }
      }
    }
    mesh->set_used_shaders(used);
    mesh->resize_mesh(vertices, triangles);
    ccl::packed_float3* positions = mesh->get_position_for_write();
    for (int v = 0; v < vertices; ++v) {
      positions[v] = ccl::make_float3(data.positions[v * 3], data.positions[v * 3 + 1], data.positions[v * 3 + 2]);
    }
    int* indices = mesh->get_triangles().data();
    for (std::size_t i = 0; i < data.indices.size(); ++i) {
      indices[i] = static_cast<int>(data.indices[i]);
    }
    const bool smooth = data.normals.size() == data.positions.size();
    std::fill(mesh->get_smooth().begin(), mesh->get_smooth().end(), smooth);
    std::fill(mesh->get_shader().begin(), mesh->get_shader().end(), 0);
    if (!body.faces.empty()) {
      int* shader = mesh->get_shader().data();
      std::size_t triangle = 0;
      for (std::size_t face = 0; face < data.faceTriangles.size(); ++face) {
        for (std::uint32_t t = 0; t < data.faceTriangles[face] && triangle < static_cast<std::size_t>(triangles);
             ++t) {
          shader[triangle++] = faceShader[face];
        }
      }
    }
    mesh->tag_triangles_modified();
    mesh->tag_shader_modified();
    mesh->tag_smooth_modified();
    if (smooth) {
      ccl::Attribute* attribute = mesh->attributes.add(ccl::ATTR_STD_VERTEX_NORMAL);
      auto* normals = attribute->data_for_write<ccl::packed_normal>();
      for (int v = 0; v < vertices; ++v) {
        normals[v] = ccl::packed_normal(
            ccl::make_float3(data.normals[v * 3], data.normals[v * 3 + 1], data.normals[v * 3 + 2]));
      }
    }
    // New nodes of a scene that is rendering: the managers must hear of them.
    mesh->tag_update(&scene, true);
    entry.mesh = mesh;
    return mesh;
  }

  BodyEntry addObject(ccl::Scene& scene, const BodyData& body) {
    BodyEntry entry;
    entry.mesh = body.mesh;
    entry.lookKey = body.lookKey();
    entry.transform = body.transform;
    entry.object = scene.create_node<ccl::Object>();
    entry.object->set_geometry(geometryOf(scene, body));
    entry.object->set_tfm(transformOf(body.transform));
    entry.object->tag_update(&scene);
    return entry;
  }

  // Deletes a body's object, and retires its Cycles mesh when no other body
  // shows it (retire; the mesh's data stays until the application releases
  // it).
  void removeObject(ccl::Scene& scene, const BodyEntry& entry) {
    scene.delete_node(entry.object);
    auto geometry = m_geometry.find({entry.mesh, entry.lookKey});
    if (geometry != m_geometry.end() && --geometry->second.users <= 0) {
      retire(geometry->second.mesh);
      m_geometry.erase(geometry);
    }
  }

  // Geometry that no object shows any more, deleted only once the render
  // cannot trace it (mitcad#1432). Cycles' CPU device gives Embree a mesh's
  // own vertex and triangle arrays (rtcSetSharedGeometryBuffer), and its
  // render thread traces the BVH of its last scene update without the
  // scene's lock, until the reset that follows a change takes effect after
  // the batch of samples it is rendering. A mesh deleted at once (a body
  // cut again by a section analysis shown or hidden, the ground built
  // again) freed the arrays that batch still traversed, and the worker
  // crashed in rtcIntersect1. Objects go at once: Embree copies their
  // transforms, and the next BVH leaves them out.
  void retire(ccl::Geometry* geometry) { m_retired.push_back({geometry, nullptr}); }

  // Under the scene's lock, at the start of a change: when Cycles has no
  // object change left to apply, its render thread has built the scene
  // again since the last change, so no BVH it traces holds the geometry
  // retired until now. (ObjectManager clears its flags only at the end of
  // a scene update that got through the BVH's build; a cancelled update
  // leaves them, and the geometry waits for the next.)
  void noteSceneBuilt(ccl::Scene& scene) {
    if (!scene.object_manager->need_update()) {
      m_unused = m_retired.size();
    }
  }

  // Deletes the retired nodes noteSceneBuilt found unused, with the
  // render's restart, so that the scene changes only together with the
  // reset as for any other change: the geometry, then the shaders of a
  // ground, a light or the world (deleteNodes' order).
  void deleteRetired(ccl::Scene& scene) {
    if (m_unused == 0) {
      return;
    }
    ccl::set<ccl::Geometry*> geometry;
    ccl::set<ccl::Shader*> shaders;
    const auto end = m_retired.begin() + static_cast<std::ptrdiff_t>(m_unused);
    for (auto it = m_retired.begin(); it != end; ++it) {
      if (it->geometry != nullptr) {
        geometry.insert(it->geometry);
      } else {
        shaders.insert(it->shader);
      }
    }
    m_retired.erase(m_retired.begin(), end);
    m_unused = 0;
    if (!geometry.empty()) {
      scene.delete_nodes(geometry);
    }
    if (!shaders.empty()) {
      scene.delete_nodes(shaders);
    }
    // For the log and the UI tests.
    std::fprintf(stderr, "render-retired: %zu geometries deleted that the render no longer traces, %zu waiting\n",
                 geometry.size(), m_retired.size());
  }

  // A body's material (Renderer.hpp, MaterialData) as a Principled BSDF:
  // the appearance's parameters map one to one, except the specular
  // weight, which scales Cycles' specular IOR level (0.5: as the IOR
  // gives). A texture gives the base colour where its image can be read,
  // and the other maps of its set (mitcad#314) where theirs can: the
  // normal map the shading normal (textureNormal), the roughness and
  // metalness maps their values (instead of the parameters), and the
  // occlusion map multiplies the base colour. Each map that cannot be
  // read (`found`, imagesRead) is left out alone.
  ccl::Shader* addMaterial(ccl::Scene& scene, const MaterialData& m, const ImagesRead& found) {
    auto graph = std::make_unique<ccl::ShaderGraph>();
    ccl::PrincipledBsdfNode* bsdf = graph->create_node<ccl::PrincipledBsdfNode>();
    const ccl::float3 baseColor = ccl::make_float3(m.baseColor[0], m.baseColor[1], m.baseColor[2]);
    bsdf->set_base_color(baseColor);
    const bool colorImage = found.color;
    const bool normalImage = found.normal;
    const bool roughnessImage = found.roughness;
    const bool metalnessImage = found.metalness;
    const bool occlusionImage = found.occlusion;
    if (found.any()) {
      ccl::ShaderGraph& g = *graph;
      const Projection projection = projectionOf(g, m);
      // Grey maps are data: no colour transform.
      const auto grey = [&](const std::string& file) {
        return blend(g, projection, images(g, projection, file, ccl::u_colorspace_data), false);
      };
      // The colour image is sRGB: Cycles' scene linear (Rec.709) with the
      // sRGB transfer function, which the kernel decodes per lookup. Not
      // u_colorspace_srgb: Cycles 5.2 encodes such pixels to sRGB once more
      // on loading, which renders them washed out (mitcad#386).
      ccl::ShaderOutput* color =
          colorImage
              ? blend(g, projection, images(g, projection, m.texture, ccl::u_colorspace_scene_linear_srgb), false)
              : nullptr;
      if (occlusionImage) {
        auto* occluded = g.create_node<ccl::MixColorNode>();
        occluded->set_blend_type(ccl::NODE_MIX_MUL);
        occluded->set_fac(1.0f);
        occluded->set_a(baseColor);
        if (color != nullptr) {
          g.connect(color, occluded->input("A"));
        }
        g.connect(grey(m.occlusionMap), occluded->input("B"));
        color = occluded->output("Result");
      }
      if (color != nullptr) {
        g.connect(color, bsdf->input("Base Color"));
      }
      if (roughnessImage) {
        g.connect(grey(m.roughnessMap), bsdf->input("Roughness"));
      }
      if (metalnessImage) {
        g.connect(grey(m.metalnessMap), bsdf->input("Metallic"));
      }
      if (normalImage) {
        g.connect(textureNormal(g, projection, m.normalMap), bsdf->input("Normal"));
      }
    }
    bsdf->set_metallic(m.metallic);
    bsdf->set_roughness(m.roughness);
    bsdf->set_specular_ior_level(0.5f * m.specular);
    bsdf->set_transmission_weight(m.transmission);
    bsdf->set_ior(m.ior);
    bsdf->set_coat_weight(m.coat);
    bsdf->set_coat_roughness(m.coatRoughness);
    bsdf->set_emission_color(ccl::make_float3(m.emissionColor[0], m.emissionColor[1], m.emissionColor[2]));
    bsdf->set_emission_strength(m.emission);
    bsdf->set_alpha(m.opacity);
    graph->connect(bsdf->output("BSDF"), graph->output()->input("Surface"));
    ccl::Shader* shader = scene.create_node<ccl::Shader>();
    shader->set_graph(std::move(graph));
    shader->tag_update(&scene);
    return shader;
  }

  // Whether the image of a texture can be read; once per file in a scene
  // update (updateScene starts again), with a warning when it cannot (the
  // base colour is used instead, a map is left out). An image read at the
  // start of a session may be gone when a body takes its material again
  // later (mitcad#538): the update that gives the material says so.
  bool textureReadable(const std::string& path, bool map) {
    auto known = m_textures.find(path);
    if (known == m_textures.end()) {
      std::string why;
      const bool ok = readable(path, why);
      known = m_textures.emplace(path, ok).first;
      if (!ok) {
        m_warnings.push_back("cannot read the texture image " + path + ": " + why +
                             (map ? "; the map is left out" : "; the base colour is shown"));
      }
    }
    return known->second;
  }

  // How the images of a texture lie on a body (Renderer.hpp, MaterialData;
  // mitcad#53): projected in the body's own coordinates (Cycles' object
  // coordinates, mm), so that a mesh needs no texture coordinates and a
  // change of the texture makes no new mesh. Each projection takes two of
  // the coordinates as the image's (u, v), turns them by the rotation and
  // divides them by the size of one repeat. Planar projects along Z; box
  // projects X-facing surfaces from (y, z), Y-facing from (x, z) and
  // Z-facing from (x, y), each point from the axis its surface faces most
  // (by the object's normal there). Every image of the set (the base
  // colour and the maps, mitcad#314) uses the same coordinates.
  struct Projection {
    struct Plane {
      ccl::ShaderOutput* coordinates = nullptr; // the image's (u, v)
      // The directions in object coordinates in which the image's u and v
      // grow (the rotation turns them): a normal map's tangent frame.
      std::array<float, 3> u{1, 0, 0};
      std::array<float, 3> v{0, 1, 0};
    };
    std::vector<Plane> planes; // planar: one; box: along X, Y and Z
    ccl::ShaderOutput* normal = nullptr; // the object's normal, object coordinates
    // Box: 1 where the surface faces X less than Y or Z, and where it faces
    // Y less than Z.
    ccl::ShaderOutput* notX = nullptr;
    ccl::ShaderOutput* notY = nullptr;
  };

  static Projection projectionOf(ccl::ShaderGraph& graph, const MaterialData& m) {
    Projection projection;
    auto* coordinates = graph.create_node<ccl::TextureCoordinateNode>();
    auto* position = graph.create_node<ccl::SeparateXYZNode>();
    graph.connect(coordinates->output("Object"), position->input("Vector"));
    projection.normal = coordinates->output("Normal");
    const float cosine = std::cos(m.textureRotation);
    const float sine = std::sin(m.textureRotation);
    using Axis = std::array<float, 3>;
    const auto turned = [](const Axis& a, float fa, const Axis& b, float fb) {
      return Axis{a[0] * fa + b[0] * fb, a[1] * fa + b[1] * fb, a[2] * fa + b[2] * fb};
    };
    const auto plane = [&](const char* u, const char* v, const Axis& uAxis, const Axis& vAxis) {
      auto* combined = graph.create_node<ccl::CombineXYZNode>();
      graph.connect(position->output(u), combined->input("X"));
      graph.connect(position->output(v), combined->input("Y"));
      // Texture mapping: (rotation^-1 (p - location)) / scale.
      auto* mapping = graph.create_node<ccl::MappingNode>();
      mapping->set_mapping_type(ccl::NODE_MAPPING_TYPE_TEXTURE);
      mapping->set_rotation(ccl::make_float3(0.0f, 0.0f, m.textureRotation));
      mapping->set_scale(ccl::make_float3(m.textureSize[0], m.textureSize[1], 1.0f));
      graph.connect(combined->output("Vector"), mapping->input("Vector"));
      Projection::Plane result;
      result.coordinates = mapping->output("Vector");
      // p = R (s * uv): u grows along R's first column, v along its second.
      result.u = turned(uAxis, cosine, vAxis, sine);
      result.v = turned(uAxis, -sine, vAxis, cosine);
      return result;
    };
    const Axis x{1, 0, 0};
    const Axis y{0, 1, 0};
    const Axis z{0, 0, 1};
    if (m.texturePlanar) {
      projection.planes = {plane("X", "Y", x, y)};
      return projection;
    }
    projection.planes = {plane("Y", "Z", y, z), plane("X", "Z", x, z), plane("X", "Y", x, y)};
    auto* normal = graph.create_node<ccl::SeparateXYZNode>();
    graph.connect(projection.normal, normal->input("Vector"));
    const auto math = [&](ccl::NodeMathType type, ccl::ShaderOutput* a, ccl::ShaderOutput* b) {
      auto* node = graph.create_node<ccl::MathNode>();
      node->set_math_type(type);
      graph.connect(a, node->input("Value1"));
      if (b != nullptr) {
        graph.connect(b, node->input("Value2"));
      }
      return node->output("Value");
    };
    ccl::ShaderOutput* nx = math(ccl::NODE_MATH_ABSOLUTE, normal->output("X"), nullptr);
    ccl::ShaderOutput* ny = math(ccl::NODE_MATH_ABSOLUTE, normal->output("Y"), nullptr);
    ccl::ShaderOutput* nz = math(ccl::NODE_MATH_ABSOLUTE, normal->output("Z"), nullptr);
    projection.notX = math(ccl::NODE_MATH_LESS_THAN, nx, math(ccl::NODE_MATH_MAXIMUM, ny, nz));
    projection.notY = math(ccl::NODE_MATH_LESS_THAN, ny, nz);
    return projection;
  }

  // An image of a texture (Image Texture nodes, linear interpolation,
  // repeated), looked up at each plane of the projection: its colour per
  // plane.
  static std::vector<ccl::ShaderOutput*> images(ccl::ShaderGraph& graph, const Projection& projection,
                                                const std::string& file, const ccl::ustring& colorspace) {
    std::vector<ccl::ShaderOutput*> colors;
    for (const Projection::Plane& plane : projection.planes) {
      auto* image = graph.create_node<ccl::ImageTextureNode>();
      image->set_filename(ccl::ustring(file));
      image->set_colorspace(colorspace);
      graph.connect(plane.coordinates, image->input("Vector"));
      colors.push_back(image->output("Color"));
    }
    return colors;
  }

  // The values of the planes as one: planar's own, box's the plane's the
  // surface faces most (Mix Color nodes, Mix Vector nodes for `vectors`).
  static ccl::ShaderOutput* blend(ccl::ShaderGraph& graph, const Projection& projection,
                                  const std::vector<ccl::ShaderOutput*>& perPlane, bool vectors) {
    if (perPlane.size() == 1) {
      return perPlane.front();
    }
    const auto mix = [&](ccl::ShaderOutput* factor, ccl::ShaderOutput* a, ccl::ShaderOutput* b) {
      ccl::ShaderNode* node = vectors ? static_cast<ccl::ShaderNode*>(graph.create_node<ccl::MixVectorNode>())
                                      : static_cast<ccl::ShaderNode*>(graph.create_node<ccl::MixColorNode>());
      graph.connect(factor, node->input("Factor"));
      graph.connect(a, node->input("A"));
      graph.connect(b, node->input("B"));
      return node->output("Result");
    };
    return mix(projection.notX, perPlane[0], mix(projection.notY, perPlane[1], perPlane[2]));
  }

  // The shading normal of a tangent-space normal map (OpenGL convention:
  // red along the image's u, green along its v; mitcad#314), in world
  // coordinates. Cycles' Normal Map node takes its tangents from a UV
  // map, which the meshes do not have (the images are projected), so the
  // tangent frame comes from the projection: at each point the plane's u
  // and v directions made perpendicular to the object's normal there, the
  // normal itself the third axis. A mirrored plane (a surface facing -X
  // seen through (y, z)) gives a mirrored frame, as its images are.
  static ccl::ShaderOutput* textureNormal(ccl::ShaderGraph& graph, const Projection& projection,
                                          const std::string& file) {
    const auto vectorMath = [&graph](ccl::NodeVectorMathType type) {
      auto* node = graph.create_node<ccl::VectorMathNode>();
      node->set_math_type(type);
      return node;
    };
    auto* unit = vectorMath(ccl::NODE_VECTOR_MATH_NORMALIZE);
    graph.connect(projection.normal, unit->input("Vector1"));
    ccl::ShaderOutput* normal = unit->output("Vector");
    // An axis without its part along the normal, of unit length.
    const auto along = [&](const std::array<float, 3>& axis) {
      const ccl::float3 a = ccl::make_float3(axis[0], axis[1], axis[2]);
      auto* onNormal = vectorMath(ccl::NODE_VECTOR_MATH_PROJECT);
      onNormal->set_vector1(a);
      graph.connect(normal, onNormal->input("Vector2"));
      auto* across = vectorMath(ccl::NODE_VECTOR_MATH_SUBTRACT);
      across->set_vector1(a);
      graph.connect(onNormal->output("Vector"), across->input("Vector2"));
      auto* length1 = vectorMath(ccl::NODE_VECTOR_MATH_NORMALIZE);
      graph.connect(across->output("Vector"), length1->input("Vector1"));
      return length1->output("Vector");
    };
    const auto scaled = [&](ccl::ShaderOutput* vector, ccl::ShaderOutput* factor) {
      auto* node = vectorMath(ccl::NODE_VECTOR_MATH_SCALE);
      graph.connect(vector, node->input("Vector1"));
      graph.connect(factor, node->input("Scale"));
      return node->output("Vector");
    };
    const auto sum = [&](ccl::ShaderOutput* a, ccl::ShaderOutput* b) {
      auto* node = vectorMath(ccl::NODE_VECTOR_MATH_ADD);
      graph.connect(a, node->input("Vector1"));
      graph.connect(b, node->input("Vector2"));
      return node->output("Vector");
    };
    const std::vector<ccl::ShaderOutput*> colors = images(graph, projection, file, ccl::u_colorspace_data);
    std::vector<ccl::ShaderOutput*> normals;
    for (std::size_t i = 0; i < projection.planes.size(); ++i) {
      // The colour's [0, 1] as [-1, 1] along u, v and the normal.
      auto* unpacked = vectorMath(ccl::NODE_VECTOR_MATH_MULTIPLY_ADD);
      graph.connect(colors[i], unpacked->input("Vector1"));
      unpacked->set_vector2(ccl::make_float3(2.0f, 2.0f, 2.0f));
      unpacked->set_vector3(ccl::make_float3(-1.0f, -1.0f, -1.0f));
      auto* parts = graph.create_node<ccl::SeparateXYZNode>();
      graph.connect(unpacked->output("Vector"), parts->input("Vector"));
      const Projection::Plane& plane = projection.planes[i];
      normals.push_back(sum(sum(scaled(along(plane.u), parts->output("X")), scaled(along(plane.v), parts->output("Y"))),
                            scaled(normal, parts->output("Z"))));
    }
    auto* world = graph.create_node<ccl::VectorTransformNode>();
    world->set_transform_type(ccl::NODE_VECTOR_TRANSFORM_TYPE_NORMAL);
    world->set_convert_from(ccl::NODE_VECTOR_TRANSFORM_CONVERT_SPACE_OBJECT);
    world->set_convert_to(ccl::NODE_VECTOR_TRANSFORM_CONVERT_SPACE_WORLD);
    graph.connect(blend(graph, projection, normals, true), world->input("Vector"));
    auto* result = vectorMath(ccl::NODE_VECTOR_MATH_NORMALIZE);
    graph.connect(world->output("Vector"), result->input("Vector1"));
    return result->output("Vector");
  }

  // Removes the nodes of one part of the scene: its objects at once, its
  // geometry and then its shaders once the render no longer traces them
  // (retire).
  void deleteNodes(ccl::Scene& scene, ccl::set<ccl::Object*>& objects, ccl::set<ccl::Geometry*>& geometry,
                   ccl::set<ccl::Shader*>& shaders) {
    if (!objects.empty()) {
      scene.delete_nodes(objects);
      objects.clear();
    }
    for (ccl::Geometry* g : geometry) {
      retire(g);
    }
    geometry.clear();
    for (ccl::Shader* shader : shaders) {
      m_retired.push_back({nullptr, shader});
    }
    shaders.clear();
  }

  ccl::Shader* addShader(ccl::Scene& scene, std::unique_ptr<ccl::ShaderGraph> graph, ccl::set<ccl::Shader*>& owner) {
    ccl::Shader* shader = scene.create_node<ccl::Shader>();
    shader->set_graph(std::move(graph));
    shader->tag_update(&scene);
    owner.insert(shader);
    return shader;
  }

  // Whether OpenImageIO opens the image (Cycles would show a missing one
  // in magenta).
  static bool readable(const std::string& path, std::string& why) {
    auto input = OIIO::ImageInput::open(path);
    if (!input) {
      why = OIIO::geterror();
      if (why.empty()) {
        why = "not an image OpenImageIO reads";
      }
      return false;
    }
    input->close();
    return true;
  }

  // The environment (EnvironmentData): the world's shader, the lights of a
  // built-in setup, and whether the camera sees the world.
  void buildWorld(ccl::Scene& scene) {
    deleteNodes(scene, m_worldObjects, m_worldGeometry, m_worldShaders);
    const EnvironmentData& e = m_environment;
    auto graph = std::make_unique<ccl::ShaderGraph>();
    ccl::BackgroundNode* background = graph->create_node<ccl::BackgroundNode>();
    background->set_strength(e.strength);
    graph->connect(background->output("Background"), graph->output()->input("Surface"));
    // The direction's height, for gradients and the ground below a sky.
    const auto height = [&graph]() {
      ccl::TextureCoordinateNode* coordinates = graph->create_node<ccl::TextureCoordinateNode>();
      ccl::SeparateXYZNode* separate = graph->create_node<ccl::SeparateXYZNode>();
      graph->connect(coordinates->output("Generated"), separate->input("Vector"));
      return separate->output("Z");
    };
    std::vector<StageLight> lights;
    float rotation = e.rotation;
    switch (e.preset) {
    case EnvironmentData::Preset::Image: {
      ccl::EnvironmentTextureNode* image = graph->create_node<ccl::EnvironmentTextureNode>();
      image->set_filename(ccl::ustring(e.image));
      // The image turns with the rotation (its coordinates the other way).
      image->tex_mapping.rotation = ccl::make_float3(0.0f, 0.0f, -e.rotation);
      graph->connect(image->output("Color"), background->input("Color"));
      m_groundAlbedo = 0.4f;
      break;
    }
    case EnvironmentData::Preset::Outdoor: {
      const ccl::float3 sun = directionOf(e.sunAzimuth, e.sunElevation);
      ccl::SkyTextureNode* sky = graph->create_node<ccl::SkyTextureNode>();
      sky->set_sky_type(ccl::NODE_SKY_HOSEK);
      sky->set_sun_direction(sun);
      sky->set_turbidity(2.5f);
      sky->set_ground_albedo(0.3f);
      // Below the horizon, the ground the sky lights.
      ccl::MathNode* below = graph->create_node<ccl::MathNode>();
      below->set_math_type(ccl::NODE_MATH_LESS_THAN);
      below->set_value2(0.0f);
      graph->connect(height(), below->input("Value1"));
      ccl::MixColorNode* ground = graph->create_node<ccl::MixColorNode>();
      ground->set_blend_type(ccl::NODE_MIX_BLEND);
      ground->set_b(ccl::make_float3(kOutdoorGround, kOutdoorGround, kOutdoorGround * 0.9f));
      graph->connect(below->output("Value"), ground->input("Factor"));
      graph->connect(sky->output("Color"), ground->input("A"));
      graph->connect(ground->output("Result"), background->input("Color"));
      background->set_strength(e.strength * kSkyStrength);
      lights.push_back({e.sunAzimuth / kDegree, e.sunElevation / kDegree, 0.035f, kSunStrength,
                        ccl::make_float3(1.0f, 0.96f, 0.9f)});
      rotation = 0.0f; // the sun's azimuth is the world's
      m_groundAlbedo = 0.3f;
      break;
    }
    default: {
      const Studio& studio = studioOf(e.preset);
      // The gradient as a ramp over the direction's height mapped to 0..1.
      ccl::MathNode* fac = graph->create_node<ccl::MathNode>();
      fac->set_math_type(ccl::NODE_MATH_MULTIPLY_ADD);
      fac->set_value2(0.5f);
      fac->set_value3(0.5f);
      graph->connect(height(), fac->input("Value1"));
      ccl::RGBRampNode* ramp = graph->create_node<ccl::RGBRampNode>();
      constexpr int kRamp = 256;
      ccl::array<ccl::packed_float3> colors;
      ccl::array<float> alphas;
      for (int i = 0; i < kRamp; ++i) {
        colors.push_back_slow(ccl::packed_float3(studioColor(studio, 2.0f * i / (kRamp - 1) - 1.0f)));
        alphas.push_back_slow(1.0f);
      }
      ramp->set_ramp(colors);
      ramp->set_ramp_alpha(alphas);
      graph->connect(fac->output("Value"), ramp->input("Fac"));
      graph->connect(ramp->output("Color"), background->input("Color"));
      background->set_strength(e.strength * studio.worldStrength);
      lights = studio.lights;
      m_groundAlbedo = studio.groundAlbedo;
      break;
    }
    }
    scene.default_background->set_graph(std::move(graph));
    scene.default_background->tag_update(&scene);
    // The film is transparent unless the camera sees the environment;
    // glass then lets the view's background through.
    scene.background->set_transparent(!e.background);
    scene.background->set_transparent_glass(!e.background);
    scene.background->set_transparent_roughness_threshold(0.2f);
    scene.background->tag_update(&scene);

    // Importance sampling of the world (gradients, skies and images vary
    // over the directions).
    ccl::BackgroundLight* map = scene.create_node<ccl::BackgroundLight>();
    map->set_map_resolution(e.preset == EnvironmentData::Preset::Image ? 1024 : 256);
    map->set_use_mis(true);
    ccl::array<ccl::Node*> mapShaders;
    mapShaders.push_back_slow(scene.default_background);
    map->set_used_shaders(mapShaders);
    addLightObject(scene, map, ccl::transform_identity());

    if (lights.empty()) {
      return;
    }
    auto emissionGraph = std::make_unique<ccl::ShaderGraph>();
    ccl::EmissionNode* emission = emissionGraph->create_node<ccl::EmissionNode>();
    emission->set_color(ccl::make_float3(1.0f, 1.0f, 1.0f));
    emission->set_strength(1.0f);
    emissionGraph->connect(emission->output("Emission"), emissionGraph->output()->input("Surface"));
    ccl::Shader* emissionShader = addShader(scene, std::move(emissionGraph), m_worldShaders);
    for (const StageLight& light : lights) {
      ccl::SunLight* sun = scene.create_node<ccl::SunLight>();
      sun->set_angle(light.angle);
      sun->set_use_mis(true);
      sun->set_strength(light.color * (light.strength * e.strength));
      ccl::array<ccl::Node*> used;
      used.push_back_slow(emissionShader);
      sun->set_used_shaders(used);
      // A sun shines along its object's -Z.
      const ccl::float3 from = directionOf(light.azimuth * kDegree + rotation, light.elevation * kDegree);
      const ccl::float3 x = std::abs(from.z) > 0.999f ? ccl::make_float3(1, 0, 0)
                                                      : ccl::normalize(ccl::cross(ccl::make_float3(0, 0, 1), from));
      const ccl::float3 y = ccl::cross(from, x);
      addLightObject(scene, sun,
                     ccl::make_transform(x.x, y.x, from.x, 0, x.y, y.y, from.y, 0, x.z, y.z, from.z, 0));
    }
  }

  void addLightObject(ccl::Scene& scene, ccl::Light* light, const ccl::Transform& tfm) {
    ccl::Object* object = scene.create_node<ccl::Object>();
    object->set_geometry(light);
    object->set_tfm(tfm);
    object->set_visibility(ccl::PATH_RAY_VISIBILITY_ALL & ~ccl::PATH_RAY_VISIBILITY_CAMERA);
    // Lights of shadow catcher objects count in the shadow catcher's pass
    // (the ground's light without the bodies), so that the ground shows
    // the bodies' shadows of them; other lights would only light it.
    object->set_is_shadow_catcher(true);
    light->tag_update(&scene);
    object->tag_update(&scene);
    m_worldGeometry.insert(light);
    m_worldObjects.insert(object);
  }

  // The user's lights (mitcad#54, EnvironmentData::lights), made again
  // with the environment and, for those relative to the camera, with the
  // view. Like the environment's, they count in the ground's catcher pass.
  void buildLights(ccl::Scene& scene) {
    deleteNodes(scene, m_lightObjects, m_lightGeometry, m_lightShaders);
    if (m_environment.lights.empty()) {
      return;
    }
    auto graph = std::make_unique<ccl::ShaderGraph>();
    ccl::EmissionNode* emission = graph->create_node<ccl::EmissionNode>();
    emission->set_color(ccl::make_float3(1.0f, 1.0f, 1.0f));
    emission->set_strength(1.0f);
    graph->connect(emission->output("Emission"), graph->output()->input("Surface"));
    ccl::Shader* shader = addShader(scene, std::move(graph), m_lightShaders);
    ccl::array<ccl::Node*> used;
    used.push_back_slow(shader);
    for (const LightData& data : m_environment.lights) {
      // Where it is and where it shines, in the scene.
      ccl::float3 position = float3Of(data.position);
      ccl::float3 direction = float3Of(data.direction);
      if (data.camera) {
        const ccl::float3 right = ccl::make_float3(m_camera.x.x, m_camera.y.x, m_camera.z.x);
        const ccl::float3 up = ccl::make_float3(m_camera.x.y, m_camera.y.y, m_camera.z.y);
        const ccl::float3 back = -ccl::make_float3(m_camera.x.z, m_camera.y.z, m_camera.z.z);
        const ccl::float3 eye = ccl::make_float3(m_camera.x.w, m_camera.y.w, m_camera.z.w);
        position = eye + right * position.x + up * position.y + back * position.z;
        direction = right * direction.x + up * direction.y + back * direction.z;
      }
      if (ccl::len(direction) < 1e-9f) {
        direction = ccl::make_float3(0.0f, 0.0f, -1.0f);
      }
      // A light shines along its object's -Z.
      const ccl::float3 z = -ccl::normalize(direction);
      const ccl::float3 x = std::abs(z.z) > 0.999f ? ccl::make_float3(1, 0, 0)
                                                   : ccl::normalize(ccl::cross(ccl::make_float3(0, 0, 1), z));
      const ccl::float3 y = ccl::cross(z, x);
      const ccl::Transform frame =
          ccl::make_transform(x.x, y.x, z.x, position.x, x.y, y.y, z.y, position.y, x.z, y.z, z.z, position.z);
      const ccl::float3 color = ccl::make_float3(data.color[0], data.color[1], data.color[2]);
      // Watts as in a design in metres: the scene is in millimetres, so
      // that the same light at the same distance in metres lights as much.
      constexpr float kSquareMillimetres = 1.0e6f;
      ccl::Light* light = nullptr;
      switch (data.kind) {
      case LightData::Kind::Point:
      case LightData::Kind::Spot: {
        ccl::PointLight* point = nullptr;
        if (data.kind == LightData::Kind::Spot) {
          auto* spot = scene.create_node<ccl::SpotLight>();
          spot->set_angle(std::clamp(data.spotAngle, 1e-3f, 3.14159f));
          spot->set_smooth(std::clamp(data.spotBlend, 0.0f, 1.0f));
          point = spot;
        } else {
          point = scene.create_node<ccl::PointLight>();
        }
        point->set_radius(std::max(0.0f, data.size / 2.0f));
        point->set_strength(color * (data.power * kSquareMillimetres));
        light = point;
        break;
      }
      case LightData::Kind::Area: {
        auto* area = scene.create_node<ccl::AreaLight>();
        area->set_sizeu(std::max(data.size, 1e-3f));
        area->set_sizev(std::max(data.disc ? data.size : data.sizeY, 1e-3f));
        area->set_ellipse(data.disc);
        area->set_strength(color * (data.power * kSquareMillimetres));
        light = area;
        break;
      }
      case LightData::Kind::Sun: {
        auto* sun = scene.create_node<ccl::SunLight>();
        sun->set_angle(std::max(0.0f, data.angle));
        sun->set_strength(color * data.power);
        light = sun;
        break;
      }
      }
      light->set_use_mis(true);
      light->set_used_shaders(used);
      ccl::Object* object = scene.create_node<ccl::Object>();
      object->set_geometry(light);
      object->set_tfm(frame);
      object->set_visibility(ccl::PATH_RAY_VISIBILITY_ALL & ~ccl::PATH_RAY_VISIBILITY_CAMERA);
      object->set_is_shadow_catcher(true); // as addLightObject's
      light->tag_update(&scene);
      object->tag_update(&scene);
      m_lightGeometry.insert(light);
      m_lightObjects.insert(object);
    }
  }

  // The camera of the last view (m_viewData): its frame from the view's eye,
  // target and up (m_camera, which the lights that follow the camera use),
  // and Cycles' camera there, except that an orthographic one stands behind
  // the bodies and the ground (mitcad#215, orthographicEye): its rays go
  // forward from its plane only. Again whenever their bounds change.
  void placeCamera(ccl::Scene& scene) {
    if (!m_hasView) {
      return;
    }
    const ViewData& view = m_viewData;
    const ccl::float3 eye = float3Of(view.eye);
    const ccl::float3 forward = ccl::normalize(float3Of(view.target) - eye);
    const ccl::float3 right = ccl::normalize(ccl::cross(forward, float3Of(view.up)));
    const ccl::float3 up = ccl::cross(right, forward);
    // Cycles' camera looks along its +Z with +Y up and +X to the right.
    ccl::Transform matrix;
    matrix.x = ccl::make_float4(right.x, up.x, forward.x, eye.x);
    matrix.y = ccl::make_float4(right.y, up.y, forward.y, eye.y);
    matrix.z = ccl::make_float4(right.z, up.z, forward.z, eye.z);
    m_camera = matrix;
    ccl::BoundBox bounds = m_bounds;
    if (m_groundBounds.valid()) {
      bounds.grow(m_groundBounds);
    }
    if (!view.perspective && bounds.valid()) {
      const std::array<double, 3> placed =
          orthographicEye(view, {bounds.min.x, bounds.min.y, bounds.min.z}, {bounds.max.x, bounds.max.y, bounds.max.z});
      matrix.x.w = static_cast<float>(placed[0]);
      matrix.y.w = static_cast<float>(placed[1]);
      matrix.z.w = static_cast<float>(placed[2]);
    }
    ccl::Camera& camera = *scene.camera;
    camera.set_matrix(matrix);
    camera.need_flags_update = true;
    camera.need_device_update = true;
  }

  // A ground under the bodies (or at the asked height), much larger than
  // they are, that only shows their shadows and, when glossy, their
  // reflections (Cycles' shadow catcher). Glass does not see it: refracted
  // rays go through to the environment, so clear glass stays clear.
  // None while the camera looks at it from below: the camera's rays would
  // meet the catcher's matte first and the bodies above it would vanish
  // from the image (groundFromBelow).
  void buildGround(ccl::Scene& scene) {
    deleteNodes(scene, m_groundObjects, m_groundGeometry, m_groundShaders);
    const EnvironmentData& e = m_environment;
    const bool fromBelow = groundFromBelow();
    const bool wanted = m_sceneGround && e.groundShadows && m_bounds.valid();
    if (wanted && fromBelow != m_groundFromBelow) {
      // For the log (the application logs the worker's stderr) and the UI
      // tests (mitcad#320).
      std::fputs(fromBelow ? "render-ground off: the camera looks at the ground from below\n"
                           : "render-ground on: the camera is above the ground again\n",
                 stderr);
    }
    m_groundFromBelow = fromBelow;
    m_hasGround = wanted && !m_groundFromBelow;
    // The catcher's passes with the ground (and the environment's behind
    // it when the camera sees it).
    updatePasses(scene);
    m_groundBounds = ccl::BoundBox::empty;
    if (!m_hasGround) {
      return;
    }
    const ccl::float3 size = m_bounds.size();
    const float extent = 50.0f * std::max({size.x, size.y, size.z, 1.0f});
    const ccl::float3 middle = m_bounds.center();
    const float z = groundHeight();
    m_groundBounds = ccl::BoundBox(ccl::make_float3(middle.x - extent, middle.y - extent, z),
                                   ccl::make_float3(middle.x + extent, middle.y + extent, z));
    auto graph = std::make_unique<ccl::ShaderGraph>();
    ccl::DiffuseBsdfNode* diffuse = graph->create_node<ccl::DiffuseBsdfNode>();
    diffuse->set_color(ccl::make_float3(m_groundAlbedo, m_groundAlbedo, m_groundAlbedo));
    if (e.groundReflections) {
      // A polished floor: a sharp reflection over the diffuse ground.
      ccl::GlossyBsdfNode* glossy = graph->create_node<ccl::GlossyBsdfNode>();
      glossy->set_color(ccl::make_float3(1.0f, 1.0f, 1.0f));
      glossy->set_roughness(0.05f);
      ccl::MixClosureNode* mixed = graph->create_node<ccl::MixClosureNode>();
      mixed->set_fac(0.35f);
      graph->connect(diffuse->output("BSDF"), mixed->input("Closure1"));
      graph->connect(glossy->output("BSDF"), mixed->input("Closure2"));
      graph->connect(mixed->output("Closure"), graph->output()->input("Surface"));
    } else {
      graph->connect(diffuse->output("BSDF"), graph->output()->input("Surface"));
    }
    ccl::Shader* shader = addShader(scene, std::move(graph), m_groundShaders);
    ccl::Mesh* mesh = scene.create_node<ccl::Mesh>();
    ccl::array<ccl::Node*> used;
    used.push_back_slow(shader);
    mesh->set_used_shaders(used);
    mesh->resize_mesh(4, 2);
    ccl::packed_float3* positions = mesh->get_position_for_write();
    positions[0] = ccl::make_float3(middle.x - extent, middle.y - extent, z);
    positions[1] = ccl::make_float3(middle.x + extent, middle.y - extent, z);
    positions[2] = ccl::make_float3(middle.x + extent, middle.y + extent, z);
    positions[3] = ccl::make_float3(middle.x - extent, middle.y + extent, z);
    int* indices = mesh->get_triangles().data();
    const int quad[6] = {0, 1, 2, 0, 2, 3};
    std::copy(quad, quad + 6, indices);
    std::fill(mesh->get_smooth().begin(), mesh->get_smooth().end(), false);
    std::fill(mesh->get_shader().begin(), mesh->get_shader().end(), 0);
    mesh->tag_triangles_modified();
    mesh->tag_shader_modified();
    mesh->tag_smooth_modified();
    m_groundGeometry.insert(mesh);
    ccl::Object* object = scene.create_node<ccl::Object>();
    object->set_geometry(mesh);
    object->set_tfm(ccl::transform_identity());
    object->set_is_shadow_catcher(true);
    object->set_visibility(ccl::PATH_RAY_VISIBILITY_ALL & ~ccl::PATH_RAY_VISIBILITY_TRANSMIT);
    mesh->tag_update(&scene, true);
    object->tag_update(&scene);
    m_groundObjects.insert(object);
  }

  // The ground's height: a hair under the asked one or under the bottom of
  // the bodies. A surface in the ground's own plane (a sheet drawn on the
  // XY plane, a face that lies on the ground) would meet the shadow catcher
  // there and come out black from above (mitcad#1738); the hair, a
  // ten-thousandth of the bodies' size and at least a few hundred float
  // steps at their distance from the origin, is far below a pixel.
  float groundHeight() const {
    const float z =
        m_environment.groundAtHeight ? static_cast<float>(m_environment.groundHeight) : m_bounds.min.z;
    if (!m_bounds.valid()) {
      return z;
    }
    const ccl::float3 size = m_bounds.size();
    const float reach = std::max({std::fabs(m_bounds.min.x), std::fabs(m_bounds.min.y), std::fabs(m_bounds.min.z),
                                  std::fabs(m_bounds.max.x), std::fabs(m_bounds.max.y), std::fabs(m_bounds.max.z),
                                  std::fabs(z)});
    const float hair = std::max(1e-4f * std::max({size.x, size.y, size.z, 1.0f}), 1e-5f * reach);
    return z - hair;
  }

  // Whether the last view looks at the ground's plane from below: a
  // perspective eye under it, an orthographic view looking up.
  bool groundFromBelow() const {
    if (!m_hasView || !m_bounds.valid()) {
      return false;
    }
    const ViewData& view = m_viewData;
    if (view.perspective) {
      return view.eye[2] < static_cast<double>(groundHeight());
    }
    return view.target[2] > view.eye[2];
  }

  // The sample counts at which the image is denoised (the last with Open
  // Image Denoise's high quality, the others with its fast one): in the
  // application, the first frame (at a lower resolution while the render
  // starts: Cycles counts it as one sample), the previews asked for
  // (setPreviews) and all of them; otherwise only all of them. Under
  // m_scheduleMutex.
  void planDenoising() {
    m_denoiseAt.clear();
    if (m_interactive) {
      std::vector<int> points{1};
      points.insert(points.end(), m_previews.begin(), m_previews.end());
      for (const int at : points) {
        if (at >= 1 && at < m_samples && (m_denoiseAt.empty() || at > m_denoiseAt.back())) {
          m_denoiseAt.push_back(at);
        }
      }
    }
    m_denoiseAt.push_back(m_samples);
    m_stage = 0;
    m_shownDenoised = false;
  }

  // A point of the schedule into the integrator (under the scene's lock).
  // Cycles reads it before its next batch of samples; a change does not
  // restart the render.
  void applyStage(ccl::Integrator& integrator, std::size_t stage) {
    std::size_t last = 0;
    int from = m_samples;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      last = m_denoiseAt.size() - 1;
      stage = std::min(stage, last);
      from = m_denoiseAt[stage];
    }
    integrator.set_denoise_start_sample(from);
    integrator.set_denoiser_quality(stage == last ? ccl::DENOISER_QUALITY_HIGH : ccl::DENOISER_QUALITY_FAST);
  }

  // The passes besides the display's of the batch about to be displayed
  // (the render thread, right before SinkDisplayDriver gets the frame):
  // the ground's catcher factors, noisy and denoised (until a denoised
  // result is there, the noisy ones; publishFrame takes those of the
  // frame's kind), and the environment behind the ground when the camera
  // sees it. Read whole: at a lower resolution the frame's pixels are the
  // first ones.
  bool readPasses(const ccl::OutputDriver::Tile& tile) {
    const bool ground = m_hasGround.load();
    const bool environment = m_seesEnvironment.load();
    const std::lock_guard<std::mutex> frame(m_frameMutex);
    m_frame.passes = false;
    m_frame.denoised = hasDenoisedResult(m_denoise);
    if (!recordDenoising(m_frame.denoised)) {
      return false;
    }
    m_frame.environment = environment;
    if (!ground) {
      return true;
    }
    const std::size_t count = static_cast<std::size_t>(tile.size.x) * tile.size.y * 4;
    m_frame.catcherNoisy.resize(count);
    if (!tile.get_pass_pixels(kCatcherNoisy, 4, m_frame.catcherNoisy.data())) {
      return false;
    }
    // The denoised factors only when this batch may reach the schedule's
    // next denoising point (a batch at most about doubles the samples):
    // reading the whole pass at every update costs much of a CPU render's
    // time. Should a denoised frame come without them, it shows the noisy
    // ones.
    int next = m_samples;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      next = m_denoiseAt[std::min(m_stage, m_denoiseAt.size() - 1)];
    }
    const int done = m_session->progress.get_current_sample();
    m_frame.catcherDenoised.clear();
    if (next <= 2 * done + 4) {
      m_frame.catcherDenoised.resize(count);
      if (!tile.get_pass_pixels(kCatcherDenoised, 4, m_frame.catcherDenoised.data())) {
        m_frame.catcherDenoised.clear();
      }
    }
    if (environment) {
      m_frame.background.resize(count);
      if (!tile.get_pass_pixels(kBackground, 4, m_frame.background.data())) {
        return false;
      }
    }
    m_frame.passes = true;
    return true;
  }

#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
  bool nativePassReference(const ccl::OutputDriver::Tile& tile,
                           const ccl::OutputDriver::DisplayInfo& display, bool composite,
                           std::vector<std::uint16_t>& result) {
    const auto count = static_cast<std::size_t>(display.size.x) * display.size.y * 4;
    const auto sourceCount = static_cast<std::size_t>(tile.size.x) * tile.size.y * 4;
    // This diagnostic is intentionally limited to small controlled regressions.
    if (sourceCount > 16 * 1024 * 1024 || count > sourceCount) return false;
    std::vector<float> catcher(sourceCount);
    if (!tile.get_pass_pixels(display.denoised ? kCatcherDenoised : kCatcherNoisy, 4, catcher.data())) return false;
    result.resize(count);
    if (!composite) {
      toHalf(catcher.data(), count, result.data());
      return true;
    }
    std::vector<float> background(sourceCount);
    if (!tile.get_pass_pixels(kBackground, 4, background.data())) return false;
    std::vector<float> light(count);
    const auto beauty = m_frame.direct->buffers().light;
    for (std::size_t index = 0; index < count; ++index) light[index] = ccl::half_to_float(ccl::half(beauty[index]));
    compositeOverEnvironment(light.data(), catcher.data(), background.data(), count / 4);
    toHalf(light.data(), count, result.data());
    return true;
  }

  // This callback runs after the beauty's film conversion/GPU synchronization,
  // while the batch's auxiliary passes remain stable. Keep the write lease until
  // both conversions finish; a concurrent progress callback must not publish it.
  void readNativePasses(const ccl::OutputDriver::Tile& tile,
                        const ccl::OutputDriver::DisplayInfo& display) {
    const std::lock_guard<std::mutex> frame(m_frameMutex);
    if (!m_frame.direct) return; // No writable transport slot for this update.
    m_frame.denoised = display.denoised;
    m_frame.nativeCatcher = false;
    const bool ground = m_hasGround.load();
    const bool composite = ground && m_seesEnvironment.load();
    bool accepted = display.size.x == m_frame.width && display.size.y == m_frame.height &&
                    recordDenoising(display.denoised);
    if (accepted && ground) {
      const auto buffers = m_frame.direct->buffers();
      ccl::OutputDriver::NativePassDestination target;
      target.size = display.size;
      target.pixels = reinterpret_cast<ccl::half4*>(composite ? buffers.light : buffers.catcher);
      target.size_bytes = static_cast<std::size_t>(display.size.x) * display.size.y * sizeof(ccl::half4);
      target.composite_over_environment = composite;
      ccl::GraphicsInteropBuffer interop;
      if (m_params.device.type == ccl::DEVICE_METAL && m_sink.metalDisplayBuffers()) {
        void* handle = composite ? m_frame.direct->nativeMetalBuffer() : m_frame.direct->nativeCatcherMetalBuffer();
        const auto bytes = composite ? m_frame.direct->nativeMetalBufferSize() : m_frame.direct->nativeCatcherMetalBufferSize();
        if (handle && bytes >= target.size_bytes) {
          interop.assign(ccl::GraphicsInteropDevice::METAL,
              static_cast<std::int64_t>(reinterpret_cast<std::intptr_t>(handle)), bytes);
          target.graphics_interop = &interop;
          target.size_bytes = bytes;
        } else accepted = false;
      }
      // Explicit test mode only: compare against the old float accessor using
      // this exact batch. In production no full-plane reference is allocated or
      // read back, and no float getter is called on the native display path.
      std::vector<std::uint16_t> reference;
      if (accepted && m_options.testNativePassComparison) {
        accepted = nativePassReference(tile, display, composite, reference);
      }
      bool graphics = false;
      target.used_graphics_interop = &graphics;
      accepted = accepted && target.pixels && tile.get_pass_pixels_native(
          display.denoised ? kCatcherDenoised : kCatcherNoisy, target);
      if (accepted && m_options.testNativePassComparison) {
        unsigned maximum = 0;
        for (std::size_t index = 0; index < reference.size(); ++index) {
          // Catcher alpha is unused by the compositor; RGB is the contract.
          // Environment output is beauty RGBA, including its opaque alpha.
          if (!composite && index % 4 == 3) continue;
          const auto actual = reinterpret_cast<const std::uint16_t*>(target.pixels)[index];
          const auto expected = reference[index];
          if (!std::isfinite(ccl::half_to_float(ccl::half(actual))) ||
              !std::isfinite(ccl::half_to_float(ccl::half(expected)))) { accepted = false; break; }
          const auto ordered = [](std::uint16_t bits) -> unsigned {
            return (bits & 0x8000u) ? unsigned(0xffffu - bits) : unsigned(bits | 0x8000u);
          };
          const auto left = ordered(actual), right = ordered(expected);
          maximum = std::max(maximum, left > right ? left - right : right - left);
        }
        // CPU uses the same converter and must match bit for bit. GPU film
        // arithmetic may contract float multiply/add: permit one half ULP, and
        // report the observed maximum rather than changing this bound on failure.
        accepted = accepted && maximum <= (graphics ? 1u : 0u);
        m_sink.auxiliaryNativeCompared(maximum, composite);
      }
      if (accepted) {
        m_frame.nativeCatcher = !composite;
        m_sink.auxiliaryNativeUsed(graphics, composite);
      }
    }
    if (!accepted) {
      m_frame.direct->discard();
      m_frame.direct.reset();
      m_frame.pending = false;
      m_session->device->set_error("native auxiliary display conversion failed (dimensions, pass or device unsupported)");
      return;
    }
    m_frame.nativeReady = true;
  }
#endif

  // A tile of a non-interactive render (the final render, benchmarks): the
  // image so far, or the finished one (`last`, denoised), with the ground's
  // catcher factors, to the sink.
  bool deliver(const ccl::OutputDriver::Tile& tile, bool last) {
    if (!deviceError().empty()) {
      return false;
    }
    const bool denoised = hasDenoisedResult(m_denoise && last);
    if (!recordDenoising(denoised)) {
      return false;
    }
    const int width = tile.size.x;
    const int height = tile.size.y;
    const std::size_t count = static_cast<std::size_t>(width) * height * 4;
    std::vector<float> light(count);
    if (!tile.get_pass_pixels("combined", 4, light.data())) {
      return false;
    }
    std::vector<float> catcher;
    if (m_hasGround.load()) {
      catcher.resize(count);
      // The denoised pass gives the noisy factors until the image is
      // denoised; without denoising there is only the noisy one.
      if (!tile.get_pass_pixels(m_catcherDenoised != nullptr ? kCatcherDenoised : kCatcherNoisy, 4,
                                catcher.data())) {
        catcher.clear();
      }
    }
    bool composite = false;
    if (!catcher.empty() && m_seesEnvironment.load()) {
      std::vector<float> background(count);
      if (tile.get_pass_pixels(kBackground, 4, background.data())) {
        compositeOverEnvironment(light.data(), catcher.data(), background.data(), count / 4);
      }
      composite = true;
    }
    const FrameBuffers target =
        m_sink.beginFrame(width, height, tile.full_size.x, tile.full_size.y, !catcher.empty() && !composite);
    if (target.light == nullptr) {
      return false;
    }
    toHalf(light.data(), count, target.light);
    if (target.catcher != nullptr && !catcher.empty()) {
      toHalf(catcher.data(), count, target.catcher);
    }
    m_sink.endFrame(m_view.load(), m_session->progress.get_current_sample(), denoised);
    return true;
  }

  // The render thread's frame buffer (SinkDisplayDriver).
  std::uint16_t* frameBuffer(int width, int height, int fullWidth, int fullHeight) {
    const std::lock_guard<std::mutex> frame(m_frameMutex);
    m_frame.width = width;
    m_frame.height = height;
    m_frame.fullWidth = fullWidth;
    m_frame.fullHeight = fullHeight;
    m_frame.pending = false;
    m_frame.nativeReady = false;
    m_frame.nativeCatcher = false;
    if (m_frame.direct) m_frame.direct->discard();
    m_frame.direct.reset();
    if (m_sink.directDisplayBuffers()) {
      m_frame.direct = m_sink.beginDisplayFrame(width, height, fullWidth, fullHeight);
      return m_frame.direct ? m_frame.direct->buffers().light : nullptr;
    }
    m_frame.pixels.resize(static_cast<std::size_t>(width) * height * 4);
    return m_frame.pixels.data();
  }

  // Passes the frame Cycles last displayed on to the sink once its batch's
  // samples are counted (the progress callback after the batch): whether
  // Cycles denoised it (it does once a batch reaches the start sample; a
  // frame at a lower resolution counts as one sample) and whether to show
  // it: once a denoised preview after the first frame is shown, noisy
  // frames are not. A denoised
  // frame moves the start sample to the schedule's next point; the last
  // one reports the view's samples done.
  void publishFrame() {
    const std::lock_guard<std::mutex> frame(m_frameMutex);
    if (!m_frame.pending) {
      return;
    }
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
    if (m_frame.direct && !m_frame.nativeReady) return;
#endif
    m_frame.pending = false;
    const int samples = m_session->progress.get_current_sample();
    const bool lowResolution = m_frame.width < m_frame.fullWidth;
    bool denoised = false;
    bool shown = false;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      const int from = m_denoiseAt[m_stage];
      denoised = m_denoise && (lowResolution ? from <= 1 : samples >= from) && m_frame.denoised;
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
      if (m_frame.direct) denoised = m_frame.denoised;
#endif
      shown = denoised || !m_shownDenoised;
      if (denoised) {
        // Noisy frames still follow the denoised first frame (at a lower
        // resolution, or of one sample), not a later preview.
        m_shownDenoised = m_shownDenoised || m_stage > 0;
        if (m_stage + 1 < m_denoiseAt.size()) {
          ++m_stage;
          m_stageChanged = true;
        }
      }
    }
    applyStageChange();
    if (!shown) {
      if (m_frame.direct) m_frame.direct->discard();
      m_frame.direct.reset();
      return;
    }
#ifdef MITCAD_CYCLES_NATIVE_DISPLAY_PASSES
    if (m_frame.direct) {
      const std::uint64_t view = m_view.load();
      m_frame.direct->publish(view, samples, denoised, m_frame.nativeCatcher);
      m_frame.direct.reset();
      if (denoised && !lowResolution && samples >= m_samples && m_reported.exchange(view) != view) {
        const double seconds = std::chrono::duration<double>(Clock::now() - m_started.load()).count();
        m_sink.finished(view, samples, seconds);
      }
      return;
    }
#endif
    // The ground's catcher factors of the same batch (readPasses): their
    // first pixels are the frame's, also at a lower resolution (Cycles'
    // buffers keep the frame's pixels first, row after row).
    const std::size_t count = m_frame.direct ? static_cast<std::size_t>(m_frame.width) * m_frame.height * 4 : m_frame.pixels.size();
    const std::vector<float>* catcher = nullptr;
    if (m_frame.passes) {
      catcher = denoised && !m_frame.catcherDenoised.empty() ? &m_frame.catcherDenoised : &m_frame.catcherNoisy;
      if (catcher->size() < count) {
        catcher = nullptr;
      }
    }
    const bool composite = catcher != nullptr && m_frame.environment && m_frame.background.size() >= count;
    const FrameBuffers target = m_frame.direct ? m_frame.direct->buffers() :
        m_sink.beginFrame(m_frame.width, m_frame.height, m_frame.fullWidth, m_frame.fullHeight, catcher != nullptr && !composite);
    if (target.light == nullptr) {
      return;
    }
    if (m_frame.direct && composite) {
      // The display driver already wrote the light into its final surface.
      // Composite in place using the same float/half operations as the retained
      // path, without allocating or copying another full light plane.
      for (std::size_t pixel = 0; pixel < count / 4; ++pixel) {
        const auto base = pixel * 4;
        const float rest = 1.0f - std::clamp(ccl::half_to_float(ccl::half(target.light[base + 3])), 0.0f, 1.0f);
        for (int channel = 0; channel < 3; ++channel) {
          const auto index = base + channel;
          const float value = ccl::half_to_float(ccl::half(target.light[index])) +
              rest * (*catcher)[index] * m_frame.background[index];
          target.light[index] = static_cast<std::uint16_t>(ccl::float_to_half_display(value));
        }
        target.light[base + 3] = static_cast<std::uint16_t>(ccl::float_to_half_display(1.0f));
      }
    } else if (composite) {
      std::vector<float> light(count);
      for (std::size_t i = 0; i < count; ++i) {
        light[i] = ccl::half_to_float(ccl::half(m_frame.pixels[i]));
      }
      compositeOverEnvironment(light.data(), catcher->data(), m_frame.background.data(), count / 4);
      toHalf(light.data(), count, target.light);
    } else {
      if (!m_frame.direct) std::memcpy(target.light, m_frame.pixels.data(), count * sizeof(std::uint16_t));
      if (target.catcher != nullptr && catcher != nullptr) {
        toHalf(catcher->data(), count, target.catcher);
      }
    }
    const std::uint64_t view = m_view.load();
    if (m_frame.direct) {
      m_frame.direct->publish(view, samples, denoised, catcher != nullptr && !composite);
      m_frame.direct.reset();
    } else m_sink.endFrame(view, samples, denoised);
    if (denoised && !lowResolution && samples >= m_samples && m_reported.exchange(view) != view) {
      const double seconds = std::chrono::duration<double>(Clock::now() - m_started.load()).count();
      m_sink.finished(view, samples, seconds);
    }
  }

  // The schedule's new point into the integrator, unless the scene is
  // locked (Cycles updates the scene under its lock and reports progress
  // meanwhile): then at the next progress.
  void applyStageChange() {
    std::size_t stage = 0;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      if (!m_stageChanged) {
        return;
      }
      stage = m_stage;
    }
    std::unique_lock<ccl::thread_mutex> lock(m_session->scene->mutex, std::try_to_lock);
    if (!lock.owns_lock()) {
      return;
    }
    applyStage(*m_session->scene->integrator, stage);
    const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
    if (m_stage == stage) {
      m_stageChanged = false;
    }
  }

  // Restarts the render with the change just made, under the scene's lock
  // as the change was (mitcad#146): Cycles updates the scene between two
  // batches of samples, and an update without the reset (another set of
  // passes, another size) would have the kernels write past the render
  // buffers, which only the reset makes anew.
  void restart() {
    deleteRetired(*m_session->scene);
    if (m_width <= 0 || m_height <= 0) {
      return; // no view yet
    }
    ccl::BufferParams buffer;
    buffer.width = m_width;
    buffer.height = m_height;
    buffer.full_width = m_width;
    buffer.full_height = m_height;
    m_started = Clock::now();
    m_reported = 0;
    m_restarted = true;
    {
      const std::lock_guard<std::mutex> schedule(m_scheduleMutex);
      m_stage = 0;
      m_shownDenoised = false;
      m_stageChanged = false;
    }
    applyStage(*m_session->scene->integrator, 0);
    m_session->reset(m_params, buffer);
    if (!m_running) {
      m_running = true;
      m_session->start();
    }
  }

  // Cycles' progress changed (its render thread): passes a frame on
  // (interactive), or reports the end of a view's samples once.
  void progressed() {
    if (!deviceError().empty()) {
      return;
    }
    if (m_interactive) {
      publishFrame();
      return;
    }
    std::string status;
    std::string substatus;
    m_session->progress.get_status(status, substatus);
    const std::uint64_t view = m_view.load();
    // Right after a restart the status still tells of the last view.
    if (status != "Rendering Done") {
      m_restarted = false;
    }
    if (status == "Rendering Done" && !m_restarted && m_reported != view) {
      m_reported = view;
      const double seconds = std::chrono::duration<double>(Clock::now() - m_started.load()).count();
      m_sink.finished(view, m_session->progress.get_current_sample(), seconds);
    }
  }

  FrameSink& m_sink;
  bool m_interactive;
  RendererOptions m_options;
  std::string m_device;
  DeviceInfo m_deviceInfo;
  mutable std::mutex m_deviceMutex;
  bool m_denoisingReported = false; // render thread only
  // Open Image Denoise's own answer once its device failed (deviceError).
  mutable std::once_flag m_oidnProbed;
  mutable std::string m_oidnProbe;
  ccl::SessionParams m_params;
  std::unique_ptr<ccl::Session> m_session;
  // The scene: meshes by key, Cycles meshes by mesh and material, shaders
  // by material, bodies by id, and the ground.
  std::map<std::string, MeshEntry> m_meshes;
  std::map<std::pair<std::string, std::string>, GeometryEntry> m_geometry;
  std::map<std::string, ccl::Shader*> m_shaders;
  std::map<std::string, BodyEntry> m_bodies;
  // Nodes left out of the scene that the render may still trace (retire),
  // in the order they left it; the first m_unused of them it no longer
  // does (noteSceneBuilt).
  struct Retired {
    ccl::Geometry* geometry;
    ccl::Shader* shader;
  };
  std::vector<Retired> m_retired;
  std::size_t m_unused = 0;
  // Texture images by path: whether they can be read, in the scene update
  // being made; and its warnings. Shaders are by material and the images
  // of it that could be read (shaderOf).
  std::map<std::string, bool> m_textures;
  std::vector<std::string> m_warnings;
  // The environment's lights and the ground, rebuilt on their own.
  EnvironmentData m_environment;
  ccl::set<ccl::Object*> m_worldObjects;
  ccl::set<ccl::Geometry*> m_worldGeometry;
  ccl::set<ccl::Shader*> m_worldShaders;
  ccl::set<ccl::Object*> m_groundObjects;
  ccl::set<ccl::Geometry*> m_groundGeometry;
  ccl::set<ccl::Shader*> m_groundShaders;
  ccl::set<ccl::Object*> m_lightObjects; // the user's lights
  ccl::set<ccl::Geometry*> m_lightGeometry;
  ccl::set<ccl::Shader*> m_lightShaders;
  // The camera (its frame: right, up, forward and the eye), for the lights
  // that follow it.
  ccl::Transform m_camera = ccl::transform_identity();
  // The catcher's passes (with a ground; the denoised one when the render
  // denoises) and the background's (when the camera sees the environment).
  ccl::Pass* m_catcherNoisy = nullptr;
  ccl::Pass* m_catcherDenoised = nullptr;
  ccl::Pass* m_backgroundPass = nullptr;
  bool m_denoise = true; // the render denoises (setFinal)
  ccl::BoundBox m_bounds = ccl::BoundBox::empty; // of the bodies
  ccl::BoundBox m_groundBounds = ccl::BoundBox::empty; // of the ground's quad, when there is one
  // The last view (setView), for placeCamera.
  ViewData m_viewData;
  bool m_hasView = false;
  bool m_sceneGround = true;
  bool m_groundFromBelow = false; // the view of the last buildGround (groundFromBelow)
  float m_groundAlbedo = 0.5f;
  int m_samples = 64;
  // The denoising schedule (planDenoising) and where the render is in it.
  std::mutex m_scheduleMutex;
  std::vector<int> m_previews; // full-resolution previews (setPreviews)
  std::vector<int> m_denoiseAt;
  std::size_t m_stage = 0;
  bool m_shownDenoised = false;
  bool m_stageChanged = false; // not in the integrator yet
  // The frame Cycles displayed last, until publishFrame passes it on.
  struct DisplayedFrame {
    std::vector<std::uint16_t> pixels;
    std::shared_ptr<DirectFrame> direct;
    // The batch's other passes (readPasses), floats of the whole view.
    bool nativeReady = false; // post-display auxiliary conversion completed
    bool nativeCatcher = false;
    bool passes = false;
    bool denoised = false; // captured on the render thread alongside the passes
    bool environment = false; // composite the ground over the background pass
    std::vector<float> catcherNoisy;
    std::vector<float> catcherDenoised;
    std::vector<float> background;
    int width = 0;
    int height = 0;
    int fullWidth = 0;
    int fullHeight = 0;
    bool pending = false;
  };
  std::mutex m_frameMutex;
  DisplayedFrame m_frame;
  int m_width = 0;
  int m_height = 0;
  bool m_running = false;
  std::atomic<std::uint64_t> m_view{0};
  // Whether there is a ground and the camera sees the environment: which
  // passes frames have (read on the render thread).
  std::atomic<bool> m_hasGround{false};
  std::atomic<bool> m_seesEnvironment{false};
  std::atomic<std::uint64_t> m_reported{0};
  std::atomic<bool> m_restarted{false};
  std::atomic<Clock::time_point> m_started{Clock::now()};
};

// ---------------------------------------------------------------------------
// Devices (mitcad#50, docs/rendering.md "Devices")

// Cycles' log and paths, once per process: the kernels of its GPU devices
// are in <kernelFolder>/lib (CUDA) or <kernelFolder>/source (Metal).
void initCycles(const std::string& kernelFolder) {
  static std::once_flag once;
  std::call_once(once, [&kernelFolder] {
    ccl::log_init(nullptr);
    ccl::path_init(kernelFolder);
#ifdef WITH_METAL
    // This Cycles flag is process-wide. Set it before any session starts
    // and leave it unchanged while shader threads may read it. A host
    // exposing both physical and paravirtual devices stays conservative.
    std::vector<DeviceInfo> metalDevices;
    for (const ccl::DeviceInfo& info : ccl::Device::available_devices(ccl::DEVICE_MASK_METAL)) {
      metalDevices.push_back(deviceInfoOf(info));
    }
    if (!metalDevices.empty()) {
      const char* overrideValue = std::getenv("CYCLES_METAL_ASYNC_PSO_CREATION");
      const bool asynchronous = metalAsyncPipelineCreation(metalDevices, overrideValue);
      ccl::DebugFlags().metal.use_async_pso_creation = asynchronous;
      std::fprintf(stderr, "Metal pipeline creation: %s (%s)\n", asynchronous ? "asynchronous" : "synchronous",
                   overrideValue != nullptr ? "environment override" : "device policy");
    }
#endif
  });
}

// Every device of the devices this build has, without Cycles' combined
// ones (several GPUs as one, which Mitcad does not offer).
ccl::vector<ccl::DeviceInfo> allCyclesDevices() {
  ccl::vector<ccl::DeviceInfo> devices;
  for (const ccl::DeviceInfo& info : ccl::Device::available_devices(static_cast<ccl::uint>(ccl::DEVICE_MASK_ALL))) {
    if (info.type != ccl::DEVICE_MULTI && info.type != ccl::DEVICE_DUMMY && info.type != ccl::DEVICE_NONE) {
      devices.push_back(info);
    }
  }
  // The CPU first.
  std::stable_partition(devices.begin(), devices.end(),
                        [](const ccl::DeviceInfo& info) { return info.type == ccl::DEVICE_CPU; });
  return devices;
}

// The device of the id; an empty id is the CPU.
bool findCyclesDevice(const std::string& id, ccl::DeviceInfo& found) {
  for (const ccl::DeviceInfo& info : allCyclesDevices()) {
    if (id.empty() ? info.type == ccl::DEVICE_CPU : info.id == id) {
      found = info;
      return true;
    }
  }
  return false;
}

// Open Image Denoise's own answer on the GPU (mitcad#202): its device
// created and committed with the arguments of Cycles' GPU denoiser
// (OIDNDenoiserGPU::denoise_create_if_needed: the device's number, a null
// stream), and OIDN's error when that fails, which Cycles does not read.
// On the calling thread: Cycles' own call runs on its render thread.
std::string probeOidn(const ccl::DeviceInfo& info) {
#ifdef WITH_OPENIMAGEDENOISE
  OIDNDevice device = nullptr;
  const char* call = nullptr;
#  if defined(OIDN_DEVICE_CUDA) && defined(WITH_CUDA)
  if (info.type == ccl::DEVICE_CUDA || info.type == ccl::DEVICE_OPTIX) {
    cudaStream_t stream = nullptr;
    call = "oidnNewCUDADevice";
    device = oidnNewCUDADevice(&info.num, &stream, 1);
  }
#  endif
#  if defined(OIDN_DEVICE_HIP) && defined(WITH_HIP)
  if (info.type == ccl::DEVICE_HIP) {
    hipStream_t stream = nullptr;
    call = "oidnNewHIPDevice";
    device = oidnNewHIPDevice(&info.num, &stream, 1);
  }
#  endif
  if (call == nullptr) {
    return std::string("not probed: this build has no Open Image Denoise device for ") +
           ccl::Device::string_from_type(info.type);
  }
  const char* message = nullptr;
  if (device == nullptr) {
    // The error of a device that was not created is the thread's.
    oidnGetDeviceError(nullptr, &message);
    return std::string(call) + " failed: " + (message != nullptr ? message : "no error message");
  }
  oidnCommitDevice(device);
  const OIDNError error = oidnGetDeviceError(device, &message);
  oidnReleaseDevice(device);
  if (error != OIDN_ERROR_NONE) {
    return std::string(call) + " created a device, committing it failed: " +
           (message != nullptr ? message : "no error message");
  }
  return std::string(call) + " works";
#else
  (void)info;
  return "not probed: this build has no Open Image Denoise";
#endif
}

DeviceInfo deviceInfoOf(const ccl::DeviceInfo& info) {
  DeviceInfo device;
  device.id = info.id;
  device.type = ccl::Device::string_from_type(info.type);
  device.name = info.description;
  if (info.type == ccl::DEVICE_METAL) {
    device.metalRTAvailable = info.use_hardware_raytracing;
    device.metalRTDefault = info.use_metalrt_by_default;
    device.metalRT = automaticRayTracing(device.type, device.metalRTAvailable, device.metalRTDefault);
  }
  // Cycles denoises with Open Image Denoise on the GPU when the device
  // supports it (OIDN's device module for it is there, Turing and newer
  // for CUDA), else on the CPU (Denoiser::create).
  device.denoiserAvailable =
      info.type != ccl::DEVICE_CPU && (info.denoisers & ccl::DENOISER_OPENIMAGEDENOISE) != 0;
  return device;
}

} // namespace

std::vector<DeviceInfo> cyclesDevices(const std::string& kernelFolder) {
  initCycles(kernelFolder);
  std::vector<DeviceInfo> devices;
  for (const ccl::DeviceInfo& info : allCyclesDevices()) {
    devices.push_back(deviceInfoOf(info));
  }
  return devices;
}

std::vector<std::string> probeDenoisers(const std::string& kernelFolder) {
  initCycles(kernelFolder);
  std::vector<std::string> lines;
#ifdef WITH_OPENIMAGEDENOISE
  const std::map<int, const char*> types = {{OIDN_DEVICE_TYPE_CPU, "CPU"},
                                            {OIDN_DEVICE_TYPE_SYCL, "SYCL"},
                                            {OIDN_DEVICE_TYPE_CUDA, "CUDA"},
                                            {OIDN_DEVICE_TYPE_HIP, "HIP"},
                                            {OIDN_DEVICE_TYPE_METAL, "METAL"}};
  std::string physical = "Open Image Denoise " OIDN_VERSION_STRING " finds:";
  const int count = oidnGetNumPhysicalDevices();
  for (int i = 0; i < count; ++i) {
    const auto type = types.find(oidnGetPhysicalDeviceInt(i, "type"));
    const char* name = oidnGetPhysicalDeviceString(i, "name");
    std::string label = name != nullptr ? name : "";
    label.erase(label.find_last_not_of(' ') + 1); // CPU names come padded
    physical += std::string(i == 0 ? " " : ", ") + (type != types.end() ? type->second : "another type") + " \"" +
                label + "\"";
  }
  lines.push_back(count == 0 ? physical + " no devices" : physical);
#endif
  for (const ccl::DeviceInfo& info : allCyclesDevices()) {
    if (info.type != ccl::DEVICE_CPU) {
      lines.push_back(info.description + " (" + ccl::Device::string_from_type(info.type) + "): " + probeOidn(info));
    }
  }
  return lines;
}

std::unique_ptr<Renderer> makeCyclesRenderer(FrameSink& sink, const RendererOptions& options, std::string& error) {
  auto renderer = std::make_unique<CyclesRenderer>(sink, options);
  if (!renderer->start(error)) {
    return nullptr;
  }
  return renderer;
}

} // namespace mitcad::render
