// SPDX-License-Identifier: MIT
// The design files the file managers' thumbnail tests read (mitcad#1695,
// mitcad#1696): Explorer's handler (thumbnail_handler_windows.cpp) and the
// Finder's Quick Look extension (quicklook_thumbnail_macos.mm). A design
// with a 256 x 192 preview, red above and blue below, as the application's
// Save writes it (core/model-base/src/design_preview.rs), its PNG written
// by hand, so that the tests need neither the model nor an image library.
#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

namespace mitcad::thumbnail_test {

inline std::uint32_t crc32(const std::uint8_t* data, std::size_t size, std::uint32_t crc = 0) {
  crc = ~crc;
  for (std::size_t i = 0; i < size; ++i) {
    crc ^= data[i];
    for (int bit = 0; bit < 8; ++bit) {
      crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
    }
  }
  return ~crc;
}

inline void putBigEndian(std::vector<std::uint8_t>& out, std::uint32_t value) {
  out.push_back(static_cast<std::uint8_t>(value >> 24));
  out.push_back(static_cast<std::uint8_t>(value >> 16));
  out.push_back(static_cast<std::uint8_t>(value >> 8));
  out.push_back(static_cast<std::uint8_t>(value));
}

inline void putChunk(std::vector<std::uint8_t>& out, const char* type, const std::vector<std::uint8_t>& body) {
  putBigEndian(out, static_cast<std::uint32_t>(body.size()));
  std::vector<std::uint8_t> typed(type, type + 4);
  typed.insert(typed.end(), body.begin(), body.end());
  out.insert(out.end(), typed.begin(), typed.end());
  putBigEndian(out, crc32(typed.data(), typed.size()));
}

// An RGBA PNG of `rgba` (row by row from the top), its pixel data in
// stored (uncompressed) deflate blocks.
inline std::vector<std::uint8_t> png(std::uint32_t width, std::uint32_t height, const std::vector<std::uint8_t>& rgba) {
  std::vector<std::uint8_t> raw;
  for (std::uint32_t y = 0; y < height; ++y) {
    raw.push_back(0);  // filter: none
    const auto row = rgba.begin() + static_cast<std::ptrdiff_t>(y) * width * 4;
    raw.insert(raw.end(), row, row + static_cast<std::ptrdiff_t>(width) * 4);
  }
  std::vector<std::uint8_t> zlib = {0x78, 0x01};
  std::size_t at = 0;
  do {
    const std::size_t length = raw.size() - at < 65535 ? raw.size() - at : 65535;
    zlib.push_back(static_cast<std::uint8_t>(at + length == raw.size() ? 1 : 0));
    zlib.push_back(static_cast<std::uint8_t>(length));
    zlib.push_back(static_cast<std::uint8_t>(length >> 8));
    zlib.push_back(static_cast<std::uint8_t>(~length));
    zlib.push_back(static_cast<std::uint8_t>(~length >> 8));
    zlib.insert(zlib.end(), raw.begin() + static_cast<std::ptrdiff_t>(at),
                raw.begin() + static_cast<std::ptrdiff_t>(at + length));
    at += length;
  } while (at < raw.size());
  std::uint32_t a = 1;
  std::uint32_t b = 0;
  for (std::uint8_t byte : raw) {
    a = (a + byte) % 65521;
    b = (b + a) % 65521;
  }
  putBigEndian(zlib, (b << 16) | a);

  std::vector<std::uint8_t> out = {0x89, 'P', 'N', 'G', '\r', '\n', 0x1a, '\n'};
  std::vector<std::uint8_t> header;
  putBigEndian(header, width);
  putBigEndian(header, height);
  header.insert(header.end(), {8, 6, 0, 0, 0});  // 8 bits, RGBA, not interlaced
  putChunk(out, "IHDR", header);
  putChunk(out, "IDAT", zlib);
  putChunk(out, "IEND", {});
  return out;
}

inline std::string base64(const std::vector<std::uint8_t>& bytes) {
  static const char* const kDigits = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string out;
  for (std::size_t i = 0; i < bytes.size(); i += 3) {
    const std::uint32_t n = (static_cast<std::uint32_t>(bytes[i]) << 16) |
                            (i + 1 < bytes.size() ? static_cast<std::uint32_t>(bytes[i + 1]) << 8 : 0u) |
                            (i + 2 < bytes.size() ? static_cast<std::uint32_t>(bytes[i + 2]) : 0u);
    out += kDigits[(n >> 18) & 63];
    out += kDigits[(n >> 12) & 63];
    out += i + 1 < bytes.size() ? kDigits[(n >> 6) & 63] : '=';
    out += i + 2 < bytes.size() ? kDigits[n & 63] : '=';
  }
  return out;
}

inline constexpr std::array<std::uint8_t, 4> kRed = {200, 0, 0, 255};
inline constexpr std::array<std::uint8_t, 4> kBlue = {0, 0, 200, 255};

// A design file's text with a 256 x 192 preview, red above and blue below,
// as the application's Save writes it (core/model-base/src/design_preview.rs).
inline std::string designWithPreview() {
  const std::uint32_t width = 256;
  const std::uint32_t height = 192;
  std::vector<std::uint8_t> rgba;
  for (std::uint32_t y = 0; y < height; ++y) {
    for (std::uint32_t x = 0; x < width; ++x) {
      const auto& colour = y < height / 2 ? kRed : kBlue;
      rgba.insert(rgba.end(), colour.begin(), colour.end());
    }
  }
  return "{\"format\": \"mitcad\", \"version\": 2, \"parameters\": [], \"features\": [], "
         "\"preview\": {\"format\": \"png\", \"width\": 256, \"height\": 192, \"data\": \"" +
         base64(png(width, height, rgba)) + "\"}}";
}

inline bool write(const std::filesystem::path& path, const std::string& text) {
  std::ofstream out(path, std::ios::binary);
  out << text;
  return static_cast<bool>(out);
}

// A design file's text without a preview, as files were saved before
// MitCad kept them.
inline std::string designWithoutPreview() {
  return "{\"format\": \"mitcad\", \"version\": 2, \"parameters\": [], \"features\": []}";
}

}  // namespace mitcad::thumbnail_test
