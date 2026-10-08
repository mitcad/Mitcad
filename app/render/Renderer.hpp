// SPDX-License-Identifier: MIT
#pragma once

// What the render worker asks of a renderer (docs/rendering.md): a scene of
// triangle meshes, a view (camera and image size) and a sample count; it
// gets progressive frames back through a FrameSink. Plain C++17: the
// renderer behind it (Cycles, CyclesRenderer.cpp) is compiled on its own,
// so nothing else includes the renderer's headers.

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace mitcad::render {

// A body's physically based material: its appearance's parameters
// (core/model/src/appearance.rs), mapped to Cycles' Principled BSDF.
// Colours are linear RGB.
struct MaterialData {
  std::array<float, 3> baseColor{0.6f, 0.6f, 0.62f};
  float metallic = 0.0f;
  float roughness = 0.4f;
  float specular = 1.0f; // the weight of a dielectric's specular reflection
  float transmission = 0.0f;
  float ior = 1.5f;
  float coat = 0.0f;
  float coatRoughness = 0.03f;
  std::array<float, 3> emissionColor{1.0f, 1.0f, 1.0f};
  float emission = 0.0f; // strength; 0 emits nothing
  float opacity = 1.0f;
  // An image for the base colour (mitcad#53): a file the renderer reads
  // (absolute; empty: none), projected in the body's own coordinates
  // ("Materials" in docs/rendering.md): one repeat of the image is
  // textureSize mm, turned by textureRotation (radians) about the
  // projection's axis; planar along Z, else from the three axes.
  std::string texture;
  std::array<float, 2> textureSize{100.0f, 100.0f};
  float textureRotation = 0.0f;
  bool texturePlanar = false;
};

// A body's triangles in its own coordinates (mm), with smooth normals per
// vertex (faces keep their own vertices, so their edges stay sharp).
struct MeshData {
  std::string name;
  std::vector<float> positions; // x, y, z per vertex
  std::vector<float> normals;   // per vertex, as positions
  std::vector<std::uint32_t> indices; // three per triangle, counter-clockwise seen from outside
  // How many triangles each face of the shape has, in the order of its
  // faces (mitcad#53; the triangles come face by face): faces with a
  // material of their own are named by their index here. Empty: one face.
  std::vector<std::uint32_t> faceTriangles;
};

// Faces of a body with a material of their own (mitcad#53: appearances of
// faces), which overrides the body's.
struct FaceMaterial {
  std::vector<std::uint32_t> faces; // indices into the mesh's faceTriangles
  MaterialData material;
  std::string materialKey;
};

// A body in the scene: one of its meshes, placed and with a material.
// Bodies that show the same mesh (occurrences of a component) share it.
struct BodyData {
  std::string id;   // the body's uid, with "@<occurrence>" when placed by one
  std::string mesh; // the key of a mesh the renderer holds (its content hash)
  // Where the body is placed: a row-major 3 x 4 matrix (rotation and
  // translation, mm).
  std::array<float, 12> transform{1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};
  MaterialData material;
  // Identifies the material: bodies with the same key share a shader (the
  // worker uses the material's JSON text).
  std::string materialKey;
  // Faces with materials of their own.
  std::vector<FaceMaterial> faces;

  // Identifies the materials of the body and its faces: bodies of the same
  // mesh with the same look share a Cycles mesh.
  std::string lookKey() const {
    std::string key = materialKey;
    for (const FaceMaterial& group : faces) {
      key += '|';
      for (const std::uint32_t face : group.faces) {
        key += std::to_string(face) + ',';
      }
      key += group.materialKey;
    }
    return key;
  }
};

// A change of the scene (docs/rendering.md, "Scene updates"): the meshes
// the renderer does not hold yet, the meshes it may forget, and every body
// of the scene. The renderer keeps the meshes it holds and the bodies that
// did not change, and touches only what did.
struct SceneUpdate {
  std::vector<std::pair<std::string, MeshData>> meshes; // new ones, by key
  std::vector<std::string> released;                    // keys no body uses any more
  std::vector<BodyData> bodies;
  // A floor under the bodies that only catches their shadows.
  bool ground = true;
};

// What an update changed, by body id.
struct SceneChanges {
  std::vector<std::string> added;   // new bodies
  std::vector<std::string> changed; // another mesh or material
  std::vector<std::string> moved;   // only another placement
  std::vector<std::string> removed;
  int kept = 0;          // bodies left as they were
  int meshes = 0;        // meshes the renderer holds
  int instanced = 0;     // bodies that share their mesh with another body
  bool restarted = false; // something changed: the render started again
  // What could not be as asked (a texture image the renderer cannot read:
  // the base colour instead).
  std::vector<std::string> warnings;
};

// A light of the user's own (the render settings' `lights`, mitcad#54):
// where it is, where it shines, its colour and power. Lengths in mm, Z up;
// with `camera` the position and direction are relative to the camera (x
// to the right, y up, z toward the viewer, from the eye) and follow the
// view.
struct LightData {
  enum class Kind { Point, Spot, Area, Sun };
  Kind kind = Kind::Point;
  bool camera = false;
  std::array<double, 3> position{0, 0, 200};
  std::array<double, 3> direction{0, 0, -1}; // where it shines
  std::array<float, 3> color{1, 1, 1};       // linear RGB
  // Watts (point, spot, area: a 5 W light 300 mm away gives 4.4 W/m²), or
  // W/m² (sun).
  float power = 5.0f;
  float size = 20.0f;  // the ball's diameter, the area's width or diameter (mm)
  float sizeY = 20.0f; // the rectangle's height (mm)
  bool disc = false;   // an area light's shape
  float spotAngle = 0.7853982f; // the cone's full angle
  float spotBlend = 0.15f;
  float angle = 0.02f; // a sun's angular diameter
};

// The light around the bodies, what the camera sees behind them and the
// ground (the document's render settings, mitcad#47,
// core/model/src/render_settings.rs; exposure and the view transform are
// the display's, RenderImage.hpp). Angles in radians, lengths in mm, Z up.
struct EnvironmentData {
  enum class Preset { Studio, StudioWhite, StudioDark, Outdoor, Image };
  Preset preset = Preset::Studio;
  float strength = 1.0f;
  float rotation = 0.0f; // the studio's lights or the image, about Z
  float sunElevation = 0.7853982f;
  float sunAzimuth = 3.9269908f; // counter-clockwise from X
  std::string image; // an equirectangular .hdr or .exr, absolute
  // The camera sees the environment; otherwise the film is transparent and
  // the display composites the bodies over its own background.
  bool background = false;
  bool groundShadows = true;
  bool groundReflections = false;
  bool groundAtHeight = false; // else under the lowest body
  double groundHeight = 0.0;
  // The user's lights that are on (mitcad#54).
  std::vector<LightData> lights;
};

// The camera of the 3D view and the image size.
struct ViewData {
  std::uint64_t sequence = 0; // frames say which view they show
  bool perspective = false;
  std::array<double, 3> eye{0, -1, 0};
  std::array<double, 3> target{0, 0, 0};
  std::array<double, 3> up{0, 0, 1};
  // Half the height of the view at the target (mm): with the eye's
  // distance the field of view of a perspective camera, the scale of an
  // orthographic one.
  double halfHeight = 100.0;
  int width = 0; // pixels
  int height = 0;
};

// A frame's buffers (FrameSink::beginFrame), each width x height RGBA half
// floats, rows from the bottom up as the renderer writes them:
// - `light`: the render's light, premultiplied, linear: the bodies (and the
//   environment when the camera sees it) over a transparent ground;
// - `catcher` (mitcad#54), when the frame has a ground over a background
//   the display composites: per pixel the factor the ground changes the
//   light behind it by (Cycles' shadow catcher pass: the ground's light
//   with the bodies divided by its light without them; RGB, A unused):
//   below 1 in shadows, coloured in the bodies' reflections. The display
//   shows light + (1 - alpha) * catcher * background.
struct FrameBuffers {
  std::uint16_t* light = nullptr;
  std::uint16_t* catcher = nullptr;
};

// Where the renderer's frames go. Called from the renderer's own thread.
class FrameSink {
public:
  virtual ~FrameSink() = default;
  // The buffers for a frame (null `light` to skip it; `catcher` only when
  // asked for with `withCatcher`). `full` is the size the view asked for;
  // a frame is smaller while it refines from a lower resolution.
  virtual FrameBuffers beginFrame(int width, int height, int fullWidth, int fullHeight, bool withCatcher) = 0;
  // The frame is complete; `denoised` when it is a denoised image (a
  // preview or the final one).
  virtual void endFrame(std::uint64_t view, int samples, bool denoised) = 0;
  // All samples of the view are rendered.
  virtual void finished(std::uint64_t view, int samples, double seconds) = 0;
};

// A device the renderer can render on (mitcad#50, docs/rendering.md
// "Devices").
struct DeviceInfo {
  std::string id;   // unique on the machine: "CPU", or Cycles' id of a GPU ("CUDA_<name>_<bus id>")
  std::string type; // CPU, CUDA, OPTIX, HIP, METAL or ONEAPI
  std::string name; // as its vendor names it
  // Open Image Denoise runs on the device itself (else on the CPU).
  bool denoisesOnDevice = false;
};

class Renderer {
public:
  virtual ~Renderer() = default;
  // Name, version and device, for the log.
  virtual std::string description() const = 0;
  // Each call changes the scene, the view or the samples and restarts the
  // render (from a lower resolution while the view changes); a scene
  // update that changes nothing does not.
  virtual SceneChanges updateScene(const SceneUpdate& update) = 0;
  virtual void setView(const ViewData& view) = 0;
  virtual void setSamples(int samples) = 0;
  // Sample counts at which the render also shows a denoised preview at
  // full resolution (besides its first frame and the last one); none by
  // default: each costs a denoise of the full image (docs/rendering.md,
  // "Previews").
  virtual void setPreviews(const std::vector<int>& samples) = 0;
  // The environment, background and ground; empty, or why it is not quite
  // as asked (an image that cannot be read: the studio instead).
  virtual std::string setEnvironment(const EnvironmentData& environment) = 0;
  // Blocks until the current view's samples are done (benchmarks, the
  // final render).
  virtual void wait() = 0;

  // The final render (mitcad#48): whether the image is denoised once its
  // samples are done (the default), and a time limit in seconds that ends
  // the render before them (0: none).
  virtual void setFinal(bool denoise, double timeLimit) = 0;
  // The samples rendered so far of the current view.
  virtual int currentSample() const = 0;

  // The device it renders on (mitcad#50).
  virtual DeviceInfo device() const = 0;
  // Why the device stopped rendering (empty while it works): the render
  // stops, and only another renderer helps.
  virtual std::string deviceError() const = 0;
};

// How the renderer starts.
struct RendererOptions {
  bool interactive = false;
  // The device (one of cyclesDevices()); an empty id is the CPU.
  DeviceInfo device;
  // The folder Cycles finds its GPU kernels in (lib/kernel_sm_61.cubin.zst
  // and so on); empty: next to the executable.
  std::string kernelFolder;
};

// The devices of this build of Cycles on this machine, the CPU first;
// after it the GPUs in Cycles' order. Initialises Cycles' paths (with
// `kernelFolder`) the first time.
std::vector<DeviceInfo> cyclesDevices(const std::string& kernelFolder);

// Cycles on the options' device; null with the reason in `error` when it
// cannot start. Interactive: each change restarts from a lower resolution,
// and its first frame is denoised as a preview (Open Image Denoise's fast
// quality), as are the full-resolution previews asked for, and the last
// frame (its high quality); once such a preview is shown, noisy frames are
// not. Otherwise (benchmarks, the final render) all samples are rendered at
// full size, the image as it is so far comes every second or two, and it
// is denoised once at the end; on a GPU that supports it, on the GPU. A device
// that fails later (its kernels cannot load, its memory runs out) stops
// the render; deviceError() then says why (RenderDevices.hpp falls back to
// the CPU).
std::unique_ptr<Renderer> makeCyclesRenderer(FrameSink& sink, const RendererOptions& options, std::string& error);

} // namespace mitcad::render
