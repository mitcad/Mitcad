// SPDX-License-Identifier: MIT
// Finder's thumbnails of MitCad designs (mitcad#1696): the principal class
// of the Quick Look thumbnail extension MitCadThumbnails.appex
// (Contents/PlugIns of the application; cmake/MacQuickLook.cmake). Quick
// Look starts the extension in its own sandboxed process and asks it for
// the thumbnail of a .mitcad file (the application's exported type,
// QLSupportedContentTypes in QuickLookThumbnails.plist.in); the picture is
// the preview the file carries, read by QuickLookThumbnail.cpp through the
// thumbnailer's C ABI. A design without a preview, or a file that is none,
// gives no reply and an error: Finder shows the file type's icon.
#import <Foundation/Foundation.h>
#import <QuickLookThumbnailing/QuickLookThumbnailing.h>

#include "QuickLookThumbnail.hpp"

#if !__has_feature(objc_arc)
#error "QuickLookThumbnailProvider.mm is compiled with ARC (-fobjc-arc)"
#endif

namespace {

NSString* const kErrorDomain = @"org.mitcad.thumbnails";

// The codes of the errors given to Quick Look: either way it shows the icon.
enum : NSInteger { kNoPreview = 1, kNoThumbnail = 2 };

}  // namespace

@interface MitCadThumbnailProvider : QLThumbnailProvider
@end

@implementation MitCadThumbnailProvider

- (void)provideThumbnailForFileRequest:(QLFileThumbnailRequest*)request
                     completionHandler:(void (^)(QLThumbnailReply* _Nullable, NSError* _Nullable))handler {
  const char* path = request.fileURL.fileSystemRepresentation;
  mitcad::quicklook::Thumbnail thumbnail =
      mitcad::quicklook::thumbnailOfFile(path ? path : "", request.maximumSize, request.scale);
  if (thumbnail.result() != mitcad::quicklook::ThumbnailResult::Picture) {
    const bool none = thumbnail.result() == mitcad::quicklook::ThumbnailResult::None;
    handler(nil, [NSError errorWithDomain:kErrorDomain
                                     code:none ? kNoPreview : kNoThumbnail
                                 userInfo:@{
                                   NSLocalizedDescriptionKey : none ? @"The design has no preview."
                                                                    : @"The file has no MitCad preview."
                                 }]);
    return;
  }
  // The block may run after this method returns: it keeps the image.
  id image = (__bridge id)thumbnail.image();
  const CGSize size = thumbnail.contextSize();
  handler([QLThumbnailReply replyWithContextSize:size
                                    drawingBlock:^BOOL(CGContextRef context) {
                                      // The whole context, in the units it has: Quick Look gives a
                                      // context of contextSize points at the request's scale, whose
                                      // transform does not always carry that scale (macOS 27 gives
                                      // the pixels unscaled).
                                      CGRect bounds = CGContextGetClipBoundingBox(context);
                                      if (CGRectIsEmpty(bounds) || CGRectIsInfinite(bounds)) {
                                        bounds = CGRectMake(0, 0, size.width, size.height);
                                      }
                                      CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
                                      CGContextDrawImage(context, bounds, (__bridge CGImageRef)image);
                                      return YES;
                                    }],
          nil);
}

@end
