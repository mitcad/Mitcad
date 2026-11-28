// SPDX-License-Identifier: MIT
#pragma once

// The render device (mitcad#50, docs/rendering.md "Devices"): which of the
// renderer's devices a choice means, and a renderer that falls back to the
// CPU when its device fails. Plain C++17, no Qt, no Cycles: the worker
// builds it with its renderer, the unit tests the choice alone.

#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "render/Renderer.hpp"

namespace mitcad::render {

// Cycles enumerates MetalRT capability separately from its automatic
// policy: M1/M2 support Metal compute without opting into MetalRT. Other
// backends retain their existing hardware ray-tracing setting.
inline bool automaticRayTracing(const std::string& backend, bool available, bool metalDefault) {
  return available && (backend != "METAL" || metalDefault);
}

// Cycles' pipeline switch is process-wide: decide once, before shader
// threads start. A paravirtual Metal device needs synchronous callbacks;
// physical-only hosts retain upstream asynchronous creation. A diagnostic
// override uses Cycles' numeric environment-variable convention.
bool metalAsyncPipelineCreation(const std::vector<DeviceInfo>& devices, const char* overrideValue);

// Hardware acceptance uses the completed session's device, not enumeration.
// A recovered render is useful product behaviour, but cannot prove the
// requested GPU path worked without fallback.
inline std::string requiredMetalError(const DeviceInfo& device, bool fellBack) {
  if (fellBack) {
    return "Required Metal validation encountered a rendering or denoising fallback";
  }
  if (device.type != "METAL") {
    return "Required Metal validation executed on " + device.type + " instead of METAL";
  }
  if (device.metalRT != automaticRayTracing("METAL", device.metalRTAvailable, device.metalRTDefault)) {
    return "Required Metal validation did not use the automatic MetalRT policy";
  }
  return {};
}

// The choices besides a device's id.
constexpr const char* kDeviceAutomatic = "auto"; // the best GPU, else the CPU
constexpr const char* kDeviceCpu = "cpu";

// The device a choice means among `devices` (the CPU first, as
// cyclesDevices lists them): "auto" the first of OptiX, CUDA, HIP, Metal
// and oneAPI that there is (GPUs of one kind in the renderer's order), else
// the CPU; "cpu" the CPU; else the device of that id, or the first of that
// type ("cuda", "hip", ...). An empty choice is "auto". A choice that
// matches no device gives the CPU and says why in `message`.
DeviceInfo chooseDevice(const std::string& choice, const std::vector<DeviceInfo>& devices, std::string& message);

// Makes a renderer on a device (makeCyclesRenderer, or a test's). Retries
// force CPU denoising too, even if a test independently requested a GPU.
using RendererFactory =
    std::function<std::unique_ptr<Renderer>(FrameSink& sink, const DeviceInfo& device, bool cpuDenoising,
                                           std::string& error)>;
// Told the device rendering now and, when the renderer fell back to the
// CPU, why. May be called from the renderer's threads.
using DeviceReport = std::function<void(const DeviceInfo& device, const std::string& fallback)>;

// A renderer on the chosen device that falls back to the CPU: when the
// device cannot start, or it fails while rendering (its kernels cannot be
// loaded, its memory runs out), a renderer on the CPU takes over with the
// same scene, environment, view and samples, and `report` says why. A GPU
// that fails while it also denoises on a GPU is first replaced by the same
// GPU with CPU denoising ("Denoising on the CPU: ..."), once, started only
// after the failed renderer has ended (its session holds the GPU's memory
// and CUDA context); only when that one cannot start or fails too does the
// CPU take over, and its message gives both failures. A `testFailure`
// (the worker's MITCAD_RENDER_TEST_DEVICE_FAILURE) makes the first renderer
// fail with that message after its first frame, also on the CPU; it counts
// as the path tracer's failure (straight to the CPU). Null with `error`
// when not even the CPU starts.
std::unique_ptr<Renderer> makeDeviceRenderer(FrameSink& sink, const DeviceInfo& device, RendererFactory factory,
                                             DeviceReport report, std::string testFailure, std::string& error);

} // namespace mitcad::render
