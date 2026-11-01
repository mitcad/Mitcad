// SPDX-License-Identifier: MIT
#include "render/MacFrameSurface.hpp"
#include "render/MacFrameControl.hpp"
#include "render/FrameMemory.hpp"

#ifdef __APPLE__
#include <algorithm>
#include <cassert>
#include <mach/kern_return.h>
#include <unistd.h>

namespace mitcad::render {
MacFrameSurfaces::~MacFrameSurfaces() {
  for (IOSurfaceRef surface : m_surfaces) {
    if (surface != nullptr) {
      CFRelease(surface);
    }
  }
}

bool MacFrameSurfaces::create(std::uint32_t width, std::uint32_t height, std::string& error) {
  if (!validMacFrameDimensions(width, height) ||
      std::any_of(m_surfaces.begin(), m_surfaces.end(), [](IOSurfaceRef surface) { return surface != nullptr; })) {
    error = "frame surfaces need a new allocation, aligned rows and supported dimensions";
    return false;
  }
  // A 64-byte-aligned row with no padding: FrameSink writes packed pixels
  // for the current frame, which can be narrower than this allocation.
  // RGhA is the system's RGBA half-float pixel format.
  const auto systemPage = sysconf(_SC_PAGESIZE);
  if (systemPage <= 0) { error = "could not determine the IOSurface allocation page size"; return false; }
  const auto page = static_cast<std::size_t>(systemPage);
  const auto bytes = planeBytes(width, height);
  // Reserve the tail padding in IOSurface itself: Metal's bytes-no-copy
  // buffer requires whole pages, even for a frame smaller than one page.
  // The row stride and packed pixel extent stay unchanged.
  const auto allocation = bytes + (bytes % page == 0 ? 0 : page - bytes % page);
  const std::int64_t values[] = {
      width, height, kBytesPerPixel, static_cast<std::int64_t>(width * kBytesPerPixel), 0x52476841,
      static_cast<std::int64_t>(allocation)};
  const void* keys[] = {kIOSurfaceWidth, kIOSurfaceHeight, kIOSurfaceBytesPerElement,
                        kIOSurfaceBytesPerRow, kIOSurfacePixelFormat, kIOSurfaceAllocSize};
  CFNumberRef numbers[6]{};
  const void* propertyValues[6];
  for (int i = 0; i < 6; ++i) {
    numbers[i] = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt64Type, &values[i]);
    if (numbers[i] == nullptr) {
      for (int previous = 0; previous < i; ++previous) { CFRelease(numbers[previous]); }
      error = "could not allocate IOSurface properties";
      return false;
    }
    propertyValues[i] = numbers[i];
  }
  const auto properties = CFDictionaryCreate(kCFAllocatorDefault, keys, propertyValues,
                                            6, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  if (properties != nullptr) {
    for (auto& surface : m_surfaces) { surface = IOSurfaceCreate(properties); }
    CFRelease(properties);
  }
  for (CFNumberRef number : numbers) {
    CFRelease(number);
  }
  for (IOSurfaceRef surface : m_surfaces) {
    if (surface == nullptr || IOSurfaceGetBytesPerRow(surface) != width * kBytesPerPixel ||
        IOSurfaceGetAllocSize(surface) < allocation || IOSurfaceGetAllocSize(surface) % page != 0 ||
        !IOSurfaceGetBaseAddress(surface) ||
        reinterpret_cast<std::uintptr_t>(IOSurfaceGetBaseAddress(surface)) % page != 0) {
      error = "IOSurface allocation does not match the page-aligned packed half-float frame";
      return false;
    }
  }
  return true;
}
bool MacFrameSurfaces::adopt(std::array<IOSurfaceRef, kFrameSlots * kFramePlanes> surfaces,
                            std::uint32_t width, std::uint32_t height, std::string& error) {
  bool valid = validMacFrameDimensions(width, height) &&
      std::none_of(m_surfaces.begin(), m_surfaces.end(), [](IOSurfaceRef surface) { return surface != nullptr; });
  for (std::size_t index = 0; index < surfaces.size(); ++index) {
    const auto surface = surfaces[index];
    valid = valid && surface && IOSurfaceGetWidth(surface) == width && IOSurfaceGetHeight(surface) == height &&
        IOSurfaceGetBytesPerElement(surface) == kBytesPerPixel && IOSurfaceGetBytesPerRow(surface) == width * kBytesPerPixel &&
        IOSurfaceGetPixelFormat(surface) == 0x52476841 && IOSurfaceGetAllocSize(surface) >= planeBytes(width, height);
    for (std::size_t previous = 0; previous < index && surface; ++previous) {
      if (surfaces[previous] && IOSurfaceGetID(surfaces[previous]) == IOSurfaceGetID(surface)) valid = false;
    }
  }
  if (!valid) {
    for (const auto surface : surfaces) { if (surface) CFRelease(surface); }
    error = "imported surfaces do not match the distinct packed half-float allocation";
    return false;
  }
  m_surfaces = surfaces;
  return true;
}
std::uint16_t* MacFrameSurfaces::pixels(std::uint32_t slot, std::uint32_t plane) const {
  return static_cast<std::uint16_t*>(IOSurfaceGetBaseAddress(surface(slot, plane)));
}
IOSurfaceRef MacFrameSurfaces::surface(std::uint32_t slot, std::uint32_t plane) const {
  return m_surfaces[slot * kFramePlanes + plane];
}
bool MacFrameSurfaces::lock(std::uint32_t slot, std::string& error) {
  if (slot >= kFrameSlots || m_surfaces[slot * kFramePlanes] == nullptr || m_readers[slot].load() != 0) {
    error = "the frame slot is retained by a reader";
    return false;
  }
  for (std::uint32_t plane = 0; plane < kFramePlanes; ++plane) {
    if (IOSurfaceLock(surface(slot, plane), 0, nullptr) != KERN_SUCCESS) {
      for (std::uint32_t previous = 0; previous < plane; ++previous) {
        IOSurfaceUnlock(surface(slot, previous), 0, nullptr);
      }
      error = "could not lock the frame's IOSurface";
      return false;
    }
  }
  return true;
}
bool MacFrameSurfaces::unlock(std::uint32_t slot, std::string& error) {
  if (slot >= kFrameSlots || m_surfaces[slot * kFramePlanes] == nullptr) {
    error = "invalid frame slot";
    return false;
  }
  bool success = true;
  for (std::uint32_t plane = 0; plane < kFramePlanes; ++plane) {
    success = (IOSurfaceUnlock(surface(slot, plane), 0, nullptr) == KERN_SUCCESS) && success;
  }
  if (!success) { error = "could not unlock the frame's IOSurface"; }
  return success;
}
MacFrameLease::MacFrameLease(std::shared_ptr<MacFrameSurfaces> surfaces, std::uint32_t slot,
                             std::shared_ptr<FrameMemory> control)
    : m_surfaces(std::move(surfaces)), m_control(std::move(control)), m_slot(slot) {
  assert(m_surfaces && m_slot < kFrameSlots && m_surfaces->surface(m_slot) != nullptr);
  const std::lock_guard<std::mutex> lock(m_surfaces->m_readerMutex);
  if (m_control) {
    assert(m_control->size() >= sizeof(MacFrameControl));
    assert(!m_surfaces->m_readerControl || m_surfaces->m_readerControl == m_control);
    m_surfaces->m_readerControl = m_control;
  }
  m_surfaces->m_readers[m_slot].fetch_add(1);
  if (m_surfaces->m_readerControl) {
    std::uint32_t active = 0;
    for (std::uint32_t slotIndex = 0; slotIndex < kFrameSlots; ++slotIndex) {
      if (m_surfaces->m_readers[slotIndex].load() != 0) active |= 1u << slotIndex;
    }
    static_cast<MacFrameControl*>(m_surfaces->m_readerControl->data())->readingMask.fetch_or(active);
  }
}
MacFrameLease::~MacFrameLease() {
  const std::lock_guard<std::mutex> lock(m_surfaces->m_readerMutex);
  if (m_surfaces->m_readers[m_slot].fetch_sub(1) == 1 && m_surfaces->m_readerControl) {
    static_cast<MacFrameControl*>(m_surfaces->m_readerControl->data())->readingMask.fetch_and(~(1u << m_slot));
  }
}
std::uint16_t* MacFrameLease::pixels(std::uint32_t plane) const {
  return m_surfaces->pixels(m_slot, plane);
}
IOSurfaceRef MacFrameLease::surface(std::uint32_t plane) const {
  return m_surfaces->surface(m_slot, plane);
}
std::uint64_t MacFrameLease::publication() const {
  return m_control ? static_cast<const MacFrameControl*>(m_control->data())->publications[m_slot] : 0;
}

std::shared_ptr<MacFrameLease> claimMacFrame(const std::shared_ptr<MacFrameSurfaces>& surfaces,
                                           const std::shared_ptr<FrameMemory>& memory, FrameInfo& info) {
  if (!surfaces || !memory || memory->size() < sizeof(MacFrameControl)) return {};
  auto* control = static_cast<MacFrameControl*>(memory->data());
  if (!control || !validMacFrameControl(*control, memory->size()) ||
      !surfaces->surface(0) || IOSurfaceGetWidth(surfaces->surface(0)) != control->capacityWidth ||
      IOSurfaceGetHeight(surfaces->surface(0)) != control->capacityHeight) return {};
  for (int attempt = 0; attempt < 8; ++attempt) {
    const auto slot = control->published.load();
    if (slot >= kFrameSlots) return {};
    auto lease = std::make_shared<MacFrameLease>(surfaces, slot, memory);
    if (control->published.load() != slot) continue;
    info = control->frames[slot];
    if (lease->publication() == 0 || info.width == 0 || info.height == 0 || info.width > control->capacityWidth ||
        info.height > control->capacityHeight || info.catcher > 1 || info.denoised > 1) return {};
    return lease;
  }
  return {};
}
} // namespace mitcad::render
#endif
