// SPDX-License-Identifier: MIT
// CEF on macOS (mitcad#1579) runs in an application whose NSApplication
// follows CefAppProtocol: it must tell whether it is inside sendEvent, so
// that CEF's nested loops and Qt's do not trip over each other. MitCad's
// NSApplication is Qt's, so the web browser component's module adds the
// protocol to it here, as a category loaded with the module.

#import <Cocoa/Cocoa.h>

#include "include/cef_application_mac.h"

namespace {
BOOL handlingSendEvent = NO;
}

@interface NSApplication (MitCadCefApplication) <CefAppProtocol>
@end

@implementation NSApplication (MitCadCefApplication)

- (BOOL)isHandlingSendEvent {
  return handlingSendEvent;
}

- (void)setHandlingSendEvent:(BOOL)handling {
  handlingSendEvent = handling;
}

@end
