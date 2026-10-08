// SPDX-License-Identifier: MIT
#pragma once

// The render worker's protocol (docs/rendering.md), shared by the
// application (RenderClient) and the worker, the executable mitcad-render
// next to the application's (RenderWorker). Plain C++17, no Qt, no
// renderer.
//
// The final render (mitcad#48) is another way to run the worker:
// `mitcad-render --batch <job.json> [--control]` renders the job (scene,
// environment, view, output settings) to an image file and reports on
// stdout ("hello", "ready", "progress", "preview", "done", "error"); with
// --control, "stop" or the end of stdin cancels it ("cancelled").
//
// Control: one JSON object per line. The application writes commands to
// the worker's stdin ({"cmd": "device" | "memory" | "scene" | "environment" |
// "view" | "samples" | "stop", ...}); the worker answers on stdout
// ({"event": "hello" | "ready" | "device" | "scene" | "frame" | "done" |
// "status" | "error", ...}). A scene names its bodies with their meshes'
// content hashes and brings only the meshes the worker does not hold
// (RenderScene.hpp). Its first line is "hello" with its kProtocolVersion;
// the application stops a worker of another version. End of stdin also
// stops the worker. The first command chooses the render device ({"cmd":
// "device", "id": "auto" | "cpu" | <a device's id>}; mitcad#50), then the
// renderer starts and "ready" names its device and lists the others;
// "device" tells that the CPU took over, and why.
//
// Pixels: a shared memory segment the application creates (FrameMemory;
// the "memory" command hands it over: on POSIX systems its file
// descriptor goes through the socket the worker got with --frame-channel,
// on Windows the command names it): a FrameHeader, then kFrameSlots slots,
// each of kFramePlanes planes of capacity width x height RGBA half floats:
// the render's light (premultiplied alpha, linear scene-referred colour)
// and, when FrameInfo::catcher says so, the ground's catcher factors
// (Renderer.hpp, FrameBuffers; mitcad#54). The worker writes a frame into a
// slot that is neither published nor being read, fills in the slot's
// FrameInfo, publishes the slot and reports it with a "frame" event. The
// application marks the published slot as being read (and checks that it
// is still the published one) before it copies the pixels out.

#include <atomic>
#include <cstddef>
#include <cstdint>

namespace mitcad::render {

// Bumped whenever the header, the commands or the events change
// incompatibly (2: the worker's own executable, "hello", file descriptors;
// 3: "environment", the document's render settings; 4: the final render,
// --batch with its job file and the events "progress", "preview",
// "warning", "done" with the file and "cancelled"; 5: incremental scenes,
// denoised frames; 6: the render device, mitcad#50: the first command
// "device", the devices in "ready", the event "device" when the CPU takes
// over, the job's "device"; 7: materials of faces and textures, the faces'
// triangles in the meshes, mitcad#53; 8: user lights in "environment", the
// frame's second plane with the ground's catcher factors, mitcad#54).
constexpr std::uint32_t kProtocolVersion = 8;
// The worker's executable, next to the application's (".exe" on Windows).
constexpr const char* kWorkerName = "mitcad-render";
constexpr std::uint32_t kFrameMagic = 0x4d524631; // "MRF1"
constexpr std::uint32_t kFrameSlots = 3;
// A slot's planes: the light, the ground's catcher factors.
constexpr std::uint32_t kFramePlanes = 2;
constexpr std::uint32_t kNoSlot = 0xffffffffu;
// RGBA, 16-bit half floats.
constexpr std::size_t kBytesPerPixel = 8;

struct FrameInfo {
  std::uint32_t width = 0;  // of the frame (the render's resolution while it refines)
  std::uint32_t height = 0;
  std::uint32_t fullWidth = 0; // of the view it is rendered for
  std::uint32_t fullHeight = 0;
  std::uint32_t samples = 0;
  std::uint32_t denoised = 0; // 1: a denoised image (a preview or the final one)
  std::uint64_t view = 0; // the "view" command's sequence number it shows
  // 1: the slot's second plane holds the ground's catcher factors (else
  // there is no ground to composite: factors of 1).
  std::uint32_t catcher = 0;
  std::uint32_t reserved = 0;
};

struct FrameHeader {
  std::uint32_t magic = kFrameMagic;
  std::uint32_t version = kProtocolVersion;
  std::uint32_t capacityWidth = 0;
  std::uint32_t capacityHeight = 0;
  std::atomic<std::uint32_t> published{kNoSlot};
  std::atomic<std::uint32_t> reading{kNoSlot};
  FrameInfo frames[kFrameSlots];
};

static_assert(std::atomic<std::uint32_t>::is_always_lock_free,
              "the frame header's atomics must work across processes");

// The header's size rounded up, where the first slot's pixels start.
constexpr std::size_t kPixelsOffset = (sizeof(FrameHeader) + 63) / 64 * 64;

constexpr std::size_t planeBytes(std::uint32_t width, std::uint32_t height) {
  return std::size_t(width) * height * kBytesPerPixel;
}

constexpr std::size_t slotBytes(std::uint32_t width, std::uint32_t height) {
  return kFramePlanes * planeBytes(width, height);
}

constexpr std::size_t segmentBytes(std::uint32_t width, std::uint32_t height) {
  return kPixelsOffset + kFrameSlots * slotBytes(width, height);
}

// A slot's plane (0: the light, 1: the ground's catcher factors).
inline unsigned char* slotPixels(void* segment, std::uint32_t slot, std::uint32_t plane = 0) {
  const auto* header = static_cast<const FrameHeader*>(segment);
  return static_cast<unsigned char*>(segment) + kPixelsOffset +
         slot * slotBytes(header->capacityWidth, header->capacityHeight) +
         plane * planeBytes(header->capacityWidth, header->capacityHeight);
}

} // namespace mitcad::render
