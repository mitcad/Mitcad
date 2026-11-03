// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QByteArray>
#include <QImage>
#include <QtGlobal>

namespace mitcad::mcp {

// ui_screenshot's encoded-image budget (mitcad#1188). The budget counts the
// encoded image's bytes (the PNG or JPEG file, the result's `bytes`), not
// the base64 text or the MCP response around it: the image block's base64
// is 4 * ceil(bytes / 3) characters, and the response adds less than
// kScreenshotResponseOverhead bytes of metadata and envelope.

// An image returned inline by default: well within Mitcad's 8 MiB transport
// frame once in base64. A client with a lower limit of its own passes a
// smaller max_bytes; Mitcad cannot know an arbitrary client's limit.
constexpr qsizetype kScreenshotInlineBytes = 5 * 1024 * 1024;
// The smallest max_bytes a caller may ask for.
constexpr qsizetype kScreenshotMinBytes = 4096;
// What the response adds to the image block's base64 at most (the text
// block and structuredContent with the metadata, the JSON-RPC envelope).
constexpr qsizetype kScreenshotResponseOverhead = 2048;
// The longer side a picture is never scaled below for the budget.
constexpr int kScreenshotMinSide = 64;
// At most this many encodings per screenshot.
constexpr int kScreenshotAttempts = 8;

// The picture as it is to be sent, or why it could not be.
struct FittedImage {
  QImage image;     // as encoded (the last attempt's when none fits)
  QByteArray bytes; // its encoding; empty when the encoder failed
  int attempts = 0; // encodings made
  bool fits = false;
  // Smaller than `longest` on its longer side only for the budget.
  bool downscaled = false;
};

using ImageEncoder = std::function<QByteArray(const QImage&)>;

// The image as PNG, or as JPEG at that quality; empty when it fails.
QByteArray encodeImage(const QImage& image, bool jpeg, int quality);

// `image` scaled to at most `longest` pixels on its longer side (never
// enlarged), then scaled down further, at most kScreenshotAttempts
// encodings and never below kScreenshotMinSide, until `encode` gives at
// most `budget` bytes. The format is the encoder's: a budget makes the
// picture smaller, never another format. `fits` false: even the smallest
// attempt is over the budget, or the encoder failed (`bytes` empty).
FittedImage fitImage(const QImage& image, int longest, qsizetype budget, const ImageEncoder& encode);

} // namespace mitcad::mcp
