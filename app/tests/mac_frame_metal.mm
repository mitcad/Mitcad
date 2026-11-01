// SPDX-License-Identifier: MIT
#include "render/MacFrameMetal.hpp"

#include <Foundation/Foundation.h>
#include <Metal/Metal.h>
#include <mach/kern_return.h>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <unistd.h>

namespace {
struct Completion {
  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  std::atomic<bool> succeeded{false};
  ~Completion() { dispatch_release(semaphore); }
};
} // namespace

int main() {
  @autoreleasepool {
    if (!mitcad::render::MacFrameMetalBuffer::supported()) {
      std::puts("SKIP: the default Metal device has no unified-memory buffer support");
      return 77;
    }
    const auto systemPage = sysconf(_SC_PAGESIZE);
    if (systemPage <= 0) return 1;
    const auto page = static_cast<std::size_t>(systemPage);
    constexpr std::size_t pixels = 16 * 8 * 8;
    const auto allocation = pixels + (pixels % page == 0 ? 0 : page - pixels % page);
    NSDictionary* properties = @{
        (NSString*)kIOSurfaceWidth: @16, (NSString*)kIOSurfaceHeight: @8,
        (NSString*)kIOSurfaceBytesPerElement: @8, (NSString*)kIOSurfaceBytesPerRow: @128,
        (NSString*)kIOSurfacePixelFormat: @0x52476841, (NSString*)kIOSurfaceAllocSize: @(allocation)};
    IOSurfaceRef surface = IOSurfaceCreate((CFDictionaryRef)properties);
    if (!surface) return 1;
    std::fprintf(stderr, "Metal fixture: base=%p allocation=%zu page=%zu packedRow=%zu\n",
        IOSurfaceGetBaseAddress(surface), IOSurfaceGetAllocSize(surface), page, IOSurfaceGetBytesPerRow(surface));
    if (IOSurfaceGetBytesPerRow(surface) != 128 || IOSurfaceGetAllocSize(surface) < allocation ||
        IOSurfaceGetAllocSize(surface) % page != 0) { CFRelease(surface); return 1; }
    auto owner = std::make_unique<mitcad::render::MacFrameMetalBuffer>();
    std::string error;
    if (!owner->wrap(surface, error) || owner->size() != IOSurfaceGetAllocSize(surface)) {
      std::fprintf(stderr, "IOSurface Metal wrap failed: %s\n", error.c_str()); CFRelease(surface); return 2;
    }
    if (owner->wrap(surface, error)) { CFRelease(surface); return 3; }
    id<MTLBuffer> buffer = [(__bridge id<MTLBuffer>)owner->handle() retain];
    id<MTLDevice> device = buffer.device;
    std::fprintf(stderr, "private Metal surface buffer: device=%s allocation=%zu\n",
                 device.name.UTF8String, owner->size());
    // The Metal buffer must retain the CF allocation after both the original
    // surface owner and the helper disappear; verification borrows its CF ref.
    IOSurfaceRef borrowed = surface;
    CFRelease(surface);
    owner.reset();
    NSError* failure = nil;
    id<MTLLibrary> library = [device newLibraryWithSource:
        @"#include <metal_stdlib>\nusing namespace metal;\n"
        @"kernel void marker(device ushort* output [[buffer(0)]], uint i [[thread_position_in_grid]]) {"
        @"if (i < 512) output[i] = ushort(0x2300 + i); }"
        options:nil error:&failure];
    id<MTLFunction> function = [library newFunctionWithName:@"marker"];
    id<MTLComputePipelineState> pipeline = function ? [device newComputePipelineStateWithFunction:function error:&failure] : nil;
    id<MTLCommandQueue> queue = [device newCommandQueue];
    if (!library || !function || !pipeline || !queue) {
      std::fprintf(stderr, "Metal surface pipeline creation failed: %s\n", failure.localizedDescription.UTF8String);
      [queue release]; [pipeline release]; [function release]; [library release]; [buffer release];
      return 4;
    }
    id<MTLCommandBuffer> command = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:buffer offset:0 atIndex:0];
    [encoder dispatchThreads:MTLSizeMake(512, 1, 1) threadsPerThreadgroup:MTLSizeMake(64, 1, 1)];
    [encoder endEncoding];
    const auto completion = std::make_shared<Completion>();
    [command addCompletedHandler:^(id<MTLCommandBuffer> finished) {
      completion->succeeded.store(finished.status == MTLCommandBufferStatusCompleted && finished.error == nil);
      dispatch_semaphore_signal(completion->semaphore);
    }];
    [command commit];
    const bool completed = dispatch_semaphore_wait(completion->semaphore,
        dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC)) == 0;
    bool matches = completed && completion->succeeded.load();
    if (matches) {
      if (IOSurfaceLock(borrowed, kIOSurfaceLockReadOnly, nullptr) != KERN_SUCCESS) matches = false;
      else {
        const auto* pixels = static_cast<const std::uint16_t*>(IOSurfaceGetBaseAddress(borrowed));
        for (unsigned index = 0; index < 512; ++index) matches = matches && pixels[index] == 0x2300 + index;
        matches = IOSurfaceUnlock(borrowed, kIOSurfaceLockReadOnly, nullptr) == KERN_SUCCESS && matches;
      }
    }
    [queue release]; [pipeline release]; [function release]; [library release]; [buffer release];
    if (!matches) { std::fprintf(stderr, "bounded GPU write did not update the same private IOSurface\n"); return 5; }
    std::puts("PASS: Metal GPU writes share the private IOSurface allocation and retained lifetime");
    return 0;
  }
}
