// SPDX-License-Identifier: MIT
// The Finder's Quick Look thumbnails of MitCad designs (mitcad#1696):
// the reading code of the extension's provider
// (app/platform/macos/QuickLookThumbnail.cpp), as the provider calls it
// for a request, and the extension in the build's bundle. Checks:
//   - a design with its preview (256 x 192, red above and blue below, in
//     the file's top-level preview as the application's Save writes it):
//     an image of the expected size for requests of 48 points at 1x
//     (48 x 36), 128 points at 2x (256 x 192) and 512 points at 2x
//     (256 x 192, never scaled up), top row first with the right colours,
//     and a context size of the preview's aspect ratio in the request's
//     box (48 x 36, 128 x 96, 512 x 384; 200 x 150 in a 200 x 400 box);
//   - a design without one (as files were saved before MitCad kept
//     previews): no picture and no error;
//   - a file that is no design, a missing file and a request of no size:
//     no picture, an error;
//   - with --appex: the extension MitCadThumbnails.appex in the build's
//     bundle (Contents/PlugIns) is a Quick Look thumbnail extension for the
//     .mitcad type the application exports, its principal class the
//     provider, its identifier under the application's, its executable
//     present, and it and the application are signed, the extension with
//     the App Sandbox.
// The C ABI underneath is tested on designs the model saved
// (core/thumbnailer/tests/capi.rs).
//
//   test_quicklook_thumbnail --work <dir> [--appex <MitCadThumbnails.appex>]

#import <Foundation/Foundation.h>

#include <CoreGraphics/CoreGraphics.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <string>
#include <vector>

#include "QuickLookThumbnail.hpp"
#include "ThumbnailTestDesigns.hpp"

namespace {

using mitcad::quicklook::ThumbnailResult;

int failures = 0;

void check(bool ok, const std::string& message) {
  if (ok) {
    std::printf("ok   %s\n", message.c_str());
  } else {
    std::printf("FAIL: %s\n", message.c_str());
    ++failures;
  }
}

std::string name(ThumbnailResult result) {
  switch (result) {
    case ThumbnailResult::Picture:
      return "a picture";
    case ThumbnailResult::None:
      return "none";
    case ThumbnailResult::Error:
      return "an error";
  }
  return "?";
}

// The image's pixels as RGBA rows from the top, drawn into an sRGB bitmap
// (the preview is opaque, so premultiplied is the same).
std::vector<std::uint8_t> rgbaOf(CGImageRef image) {
  const std::size_t width = CGImageGetWidth(image);
  const std::size_t height = CGImageGetHeight(image);
  std::vector<std::uint8_t> pixels(width * height * 4);
  CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  CGContextRef context = CGBitmapContextCreate(pixels.data(), width, height, 8, width * 4, space,
                                               static_cast<CGBitmapInfo>(kCGImageAlphaPremultipliedLast));
  CGColorSpaceRelease(space);
  if (!context) {
    return {};
  }
  CGContextDrawImage(context, CGRectMake(0, 0, static_cast<CGFloat>(width), static_cast<CGFloat>(height)), image);
  CGContextRelease(context);
  return pixels;
}

bool near(const std::uint8_t* pixel, const std::array<std::uint8_t, 4>& colour) {
  for (std::size_t i = 0; i < 4; ++i) {
    const int difference = static_cast<int>(pixel[i]) - static_cast<int>(colour[i]);
    if (difference < -2 || difference > 2) {
      return false;
    }
  }
  return true;
}

void checkPreviewRequests(const std::filesystem::path& design) {
  struct Request {
    CGSize maximum;
    CGFloat scale;
    std::size_t width, height;
    CGSize context;
  };
  const Request requests[] = {
      {{48, 48}, 1, 48, 36, {48, 36}},
      {{128, 128}, 2, 256, 192, {128, 96}},
      {{512, 512}, 2, 256, 192, {512, 384}},
      {{200, 400}, 1, 256, 192, {200, 150}},
  };
  for (const Request& request : requests) {
    const std::string at = std::to_string(static_cast<int>(request.maximum.width)) + " x " +
                           std::to_string(static_cast<int>(request.maximum.height)) + " at " +
                           std::to_string(static_cast<int>(request.scale)) + "x: ";
    const mitcad::quicklook::Thumbnail thumbnail =
        mitcad::quicklook::thumbnailOfFile(design.string(), request.maximum, request.scale);
    check(thumbnail.result() == ThumbnailResult::Picture,
          at + "a design with its preview gives a picture (" + name(thumbnail.result()) + ")");
    if (!thumbnail.image()) {
      continue;
    }
    CGImageRef image = thumbnail.image();
    check(CGImageGetWidth(image) == request.width && CGImageGetHeight(image) == request.height,
          at + "the image is " + std::to_string(request.width) + " x " + std::to_string(request.height) + " (" +
              std::to_string(CGImageGetWidth(image)) + " x " + std::to_string(CGImageGetHeight(image)) + ")");
    const CGSize context = thumbnail.contextSize();
    check(context.width == request.context.width && context.height == request.context.height,
          at + "the context is " + std::to_string(static_cast<int>(request.context.width)) + " x " +
              std::to_string(static_cast<int>(request.context.height)) + " points (" +
              std::to_string(context.width) + " x " + std::to_string(context.height) + ")");
    const std::vector<std::uint8_t> pixels = rgbaOf(image);
    const std::size_t width = CGImageGetWidth(image);
    const std::size_t height = CGImageGetHeight(image);
    check(!pixels.empty() && near(&pixels[0], mitcad::thumbnail_test::kRed), at + "the top row is red");
    check(!pixels.empty() && near(&pixels[((height - 1) * width + width - 1) * 4], mitcad::thumbnail_test::kBlue),
          at + "the bottom row is blue");
  }
}

void checkNoPicture(const std::filesystem::path& file, ThumbnailResult expected, const std::string& what) {
  const mitcad::quicklook::Thumbnail thumbnail =
      mitcad::quicklook::thumbnailOfFile(file.string(), CGSizeMake(256, 256), 2);
  check(thumbnail.result() == expected && thumbnail.image() == nullptr,
        what + ": " + name(expected) + ", no picture (" + name(thumbnail.result()) + ")");
}

// --- The extension in the bundle -----------------------------------------

// The output of a program and whether it exited 0.
bool run(NSString* program, NSArray<NSString*>* arguments, std::string* output) {
  NSTask* task = [[NSTask alloc] init];
  task.executableURL = [NSURL fileURLWithPath:program];
  task.arguments = arguments;
  NSPipe* pipe = [NSPipe pipe];
  task.standardOutput = pipe;
  task.standardError = pipe;
  NSError* error = nil;
  if (![task launchAndReturnError:&error]) {
    return false;
  }
  NSData* data = [pipe.fileHandleForReading readDataToEndOfFile];
  [task waitUntilExit];
  if (output) {
    output->assign(static_cast<const char*>(data.bytes), data.length);
  }
  return task.terminationStatus == 0;
}

void checkExtension(const std::filesystem::path& appex) {
  NSString* appexPath = [NSString stringWithUTF8String:appex.c_str()];
  NSDictionary* info = [NSDictionary
      dictionaryWithContentsOfURL:[NSURL fileURLWithPath:[appexPath stringByAppendingPathComponent:@"Contents/Info.plist"]]];
  NSString* app = [[appexPath stringByDeletingLastPathComponent] stringByDeletingLastPathComponent];
  app = [app stringByDeletingLastPathComponent];
  NSDictionary* appInfo =
      [NSDictionary dictionaryWithContentsOfURL:[NSURL fileURLWithPath:[app stringByAppendingPathComponent:@"Contents/Info.plist"]]];
  check(info != nil, "the extension has an Info.plist");
  check(appInfo != nil, "the application around it has an Info.plist");
  if (!info || !appInfo) {
    return;
  }
  check([info[@"CFBundlePackageType"] isEqual:@"XPC!"], "the extension is an XPC bundle");
  NSString* appId = appInfo[@"CFBundleIdentifier"];
  NSString* appexId = info[@"CFBundleIdentifier"];
  check(appId && [appexId hasPrefix:[appId stringByAppendingString:@"."]],
        "the extension's identifier lies under the application's");
  NSString* executable = info[@"CFBundleExecutable"];
  check(executable && [[NSFileManager defaultManager]
                          isExecutableFileAtPath:[appexPath stringByAppendingPathComponent:
                                                                [@"Contents/MacOS" stringByAppendingPathComponent:executable]]],
        "the extension's executable is there");
  NSDictionary* extension = info[@"NSExtension"];
  check([extension[@"NSExtensionPointIdentifier"] isEqual:@"com.apple.quicklook.thumbnail"],
        "it is a Quick Look thumbnail extension");
  check([extension[@"NSExtensionPrincipalClass"] isEqual:@"MitCadThumbnailProvider"],
        "its principal class is the provider");
  // The type the application exports for .mitcad.
  NSString* designType = nil;
  for (NSDictionary* type in appInfo[@"UTExportedTypeDeclarations"]) {
    if ([type[@"UTTypeTagSpecification"][@"public.filename-extension"] containsObject:@"mitcad"]) {
      designType = type[@"UTTypeIdentifier"];
    }
  }
  NSArray* supported = extension[@"NSExtensionAttributes"][@"QLSupportedContentTypes"];
  check(designType && [supported containsObject:designType],
        "it is for the .mitcad type the application exports (" +
            std::string(designType ? designType.UTF8String : "none exported") + ")");

  std::string output;
  bool signedOk = run(@"/usr/bin/codesign", @[ @"--verify", @"--strict", appexPath ], &output);
  check(signedOk, "the extension is signed " + output);
  signedOk = run(@"/usr/bin/codesign", @[ @"--verify", @"--strict", app ], &output);
  check(signedOk, "the application is signed with the extension inside " + output);
  output.clear();
  run(@"/usr/bin/codesign", @[ @"--display", @"--entitlements", @"-", @"--xml", appexPath ], &output);
  check(output.find("com.apple.security.app-sandbox") != std::string::npos,
        "the extension's signature has the App Sandbox entitlement");
}

}  // namespace

int main(int argc, char** argv) {
  @autoreleasepool {
    std::filesystem::path work;
    std::filesystem::path appex;
    for (int i = 1; i + 1 < argc; i += 2) {
      const std::string option = argv[i];
      if (option == "--work") {
        work = argv[i + 1];
      } else if (option == "--appex") {
        appex = argv[i + 1];
      }
    }
    if (work.empty()) {
      std::printf("usage: %s --work <dir> [--appex <MitCadThumbnails.appex>]\n", argv[0]);
      return 2;
    }
    std::error_code error;
    std::filesystem::remove_all(work, error);
    std::filesystem::create_directories(work);
    const std::filesystem::path design = work / "preview.mitcad";
    const std::filesystem::path old = work / "old.mitcad";
    const std::filesystem::path other = work / "other.mitcad";
    if (!mitcad::thumbnail_test::write(design, mitcad::thumbnail_test::designWithPreview()) ||
        !mitcad::thumbnail_test::write(old, mitcad::thumbnail_test::designWithoutPreview()) ||
        !mitcad::thumbnail_test::write(other, "{\"format\": \"other\"}")) {
      std::printf("FAIL: cannot write the designs in %s\n", work.c_str());
      return 1;
    }

    checkPreviewRequests(design);
    checkNoPicture(old, ThumbnailResult::None, "a design without a preview");
    checkNoPicture(other, ThumbnailResult::Error, "a file that is no design");
    checkNoPicture(work / "missing.mitcad", ThumbnailResult::Error, "a missing file");
    const mitcad::quicklook::Thumbnail empty =
        mitcad::quicklook::thumbnailOfFile(design.string(), CGSizeZero, 2);
    check(empty.result() == ThumbnailResult::Error && !empty.image(), "a request of no size: an error, no picture");
    check(mitcad::quicklook::requestPixels(CGSizeMake(128, 64), 2) == 256 &&
              mitcad::quicklook::requestPixels(CGSizeMake(31.5, 10), 1) == 32,
          "the pixels asked for are the larger side at the scale, rounded up");

    if (!appex.empty()) {
      checkExtension(appex);
    }
    if (failures == 0) {
      std::filesystem::remove_all(work, error);
      std::printf("All Quick Look thumbnail checks passed.\n");
      return 0;
    }
    std::printf("%d check(s) failed.\n", failures);
    return 1;
  }
}
