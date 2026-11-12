// SPDX-License-Identifier: MIT
// Explorer's thumbnail handler of MitCad designs (mitcad#1695,
// app/thumbnails), called as Explorer calls it: the file as a stream
// (IInitializeWithStream), then IThumbnailProvider::GetThumbnail at a
// size. Checks:
//   - a design with its preview (256 x 192, red above and blue below, in
//     the file's top-level preview as the application's Save writes it):
//     a 32-bit bitmap with alpha of the expected size at 96 (96 x 72),
//     256 and 1024 (never scaled up), top row first, the colours right;
//   - a design without one (as files were saved before MitCad kept
//     previews): no picture, and the handler's "nothing to show"
//     (WTS_E_FAILEDEXTRACTION, on which Explorer shows the file type's
//     icon), not an error;
//   - a file that is no design: no picture, an error;
//   - Initialize twice and GetThumbnail before Initialize are refused.
// The C ABI underneath is tested on designs the model saved
// (core/thumbnailer/tests/capi.rs).
//
//   test_thumbnail_handler --dll <mitcad-thumbnails.dll> --work <dir>
//     loads the build's DLL itself (ctest app.thumbnails-windows) and
//     checks DllCanUnloadNow at the end;
//   test_thumbnail_handler --registered --work <dir> [--expect-dll <path>]
//     goes through the registry as the shell does: .mitcad's ShellEx
//     thumbnail entry must name the handler's CLSID, whose InprocServer32
//     (Apartment) is the DLL given; then CoCreateInstance (the installer
//     test, tools/installer-windows-test.ps1).

#include <windows.h>

#include <objbase.h>
#include <propsys.h>
#include <shlwapi.h>
#include <thumbcache.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cwchar>
#include <filesystem>
#include <fstream>
#include <functional>
#include <string>
#include <vector>

#include "ThumbnailHandler.hpp"

namespace {

int failures = 0;

void pass(const std::string& message) {
  std::printf("ok   %s\n", message.c_str());
}

void fail(const std::string& message) {
  std::printf("FAIL: %s\n", message.c_str());
  ++failures;
}

void check(bool ok, const std::string& message) {
  if (ok) {
    pass(message);
  } else {
    fail(message);
  }
}

std::string hex(HRESULT hr) {
  char text[16];
  std::snprintf(text, sizeof text, "0x%08lX", static_cast<unsigned long>(hr));
  return text;
}

// --- A PNG file and a design file, written by hand ---------------------

std::uint32_t crc32(const std::uint8_t* data, std::size_t size, std::uint32_t crc = 0) {
  crc = ~crc;
  for (std::size_t i = 0; i < size; ++i) {
    crc ^= data[i];
    for (int bit = 0; bit < 8; ++bit) {
      crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
    }
  }
  return ~crc;
}

void putBigEndian(std::vector<std::uint8_t>& out, std::uint32_t value) {
  out.push_back(static_cast<std::uint8_t>(value >> 24));
  out.push_back(static_cast<std::uint8_t>(value >> 16));
  out.push_back(static_cast<std::uint8_t>(value >> 8));
  out.push_back(static_cast<std::uint8_t>(value));
}

void putChunk(std::vector<std::uint8_t>& out, const char* type, const std::vector<std::uint8_t>& body) {
  putBigEndian(out, static_cast<std::uint32_t>(body.size()));
  std::vector<std::uint8_t> typed(type, type + 4);
  typed.insert(typed.end(), body.begin(), body.end());
  out.insert(out.end(), typed.begin(), typed.end());
  putBigEndian(out, crc32(typed.data(), typed.size()));
}

// An RGBA PNG of `rgba` (row by row from the top), its pixel data in
// stored (uncompressed) deflate blocks.
std::vector<std::uint8_t> png(std::uint32_t width, std::uint32_t height, const std::vector<std::uint8_t>& rgba) {
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

std::string base64(const std::vector<std::uint8_t>& bytes) {
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

constexpr std::array<std::uint8_t, 4> kRed = {200, 0, 0, 255};
constexpr std::array<std::uint8_t, 4> kBlue = {0, 0, 200, 255};

// A design file's text with a 256 x 192 preview, red above and blue below,
// as the application's Save writes it (core/model-base/src/design_preview.rs).
std::string designWithPreview() {
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

bool write(const std::filesystem::path& path, const std::string& text) {
  std::ofstream out(path, std::ios::binary);
  out << text;
  return static_cast<bool>(out);
}

// --- The handler ---------------------------------------------------------

using Create = std::function<HRESULT(IInitializeWithStream**)>;

struct Result {
  HRESULT hr = E_FAIL;
  WTS_ALPHATYPE alpha = WTSAT_UNKNOWN;
  HBITMAP bitmap = nullptr;
};

// A new handler on the file, asked for a thumbnail of cx pixels.
Result thumbnail(const Create& create, const std::filesystem::path& file, UINT cx) {
  Result result;
  IInitializeWithStream* init = nullptr;
  result.hr = create(&init);
  if (FAILED(result.hr)) {
    return result;
  }
  IStream* stream = nullptr;
  result.hr = SHCreateStreamOnFileEx(file.c_str(), STGM_READ | STGM_SHARE_DENY_WRITE, FILE_ATTRIBUTE_NORMAL, FALSE,
                                     nullptr, &stream);
  if (SUCCEEDED(result.hr)) {
    result.hr = init->Initialize(stream, STGM_READ);
    stream->Release();
  }
  IThumbnailProvider* provider = nullptr;
  if (SUCCEEDED(result.hr)) {
    result.hr = init->QueryInterface(__uuidof(IThumbnailProvider), reinterpret_cast<void**>(&provider));
  }
  if (SUCCEEDED(result.hr)) {
    result.hr = provider->GetThumbnail(cx, &result.bitmap, &result.alpha);
    provider->Release();
  }
  init->Release();
  return result;
}

// The bitmap's pixels as BGRA rows from the top, read back through
// GetDIBits with a top-down request: GetObject's DIBSECTION reports a
// top-down section's height as positive too, so its bits' row order
// cannot be told from it. Empty when they cannot be read.
std::vector<std::uint8_t> topDownBgra(HBITMAP bitmap, LONG width, LONG height) {
  BITMAPINFO info{};
  info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
  info.bmiHeader.biWidth = width;
  info.bmiHeader.biHeight = -height;
  info.bmiHeader.biPlanes = 1;
  info.bmiHeader.biBitCount = 32;
  info.bmiHeader.biCompression = BI_RGB;
  std::vector<std::uint8_t> pixels(static_cast<std::size_t>(width) * static_cast<std::size_t>(height) * 4);
  HDC dc = CreateCompatibleDC(nullptr);
  const int rows = dc ? GetDIBits(dc, bitmap, 0, static_cast<UINT>(height), pixels.data(), &info, DIB_RGB_COLORS) : 0;
  if (dc) {
    DeleteDC(dc);
  }
  if (rows != height) {
    pixels.clear();
  }
  return pixels;
}

std::string bgrText(const std::uint8_t* pixel) {
  return "B " + std::to_string(pixel[0]) + " G " + std::to_string(pixel[1]) + " R " + std::to_string(pixel[2]);
}

// The pixel at (x, y) of top-down BGRA rows has the colour `rgba`; says what
// it has otherwise.
void checkColour(const std::vector<std::uint8_t>& pixels, LONG width, LONG x, LONG y,
                 const std::array<std::uint8_t, 4>& rgba, const std::string& what) {
  const std::uint8_t expected[3] = {rgba[2], rgba[1], rgba[0]};
  const std::uint8_t* got =
      pixels.data() + (static_cast<std::size_t>(y) * static_cast<std::size_t>(width) + static_cast<std::size_t>(x)) * 4;
  check(got[0] == expected[0] && got[1] == expected[1] && got[2] == expected[2],
        what + " at (" + std::to_string(x) + ", " + std::to_string(y) + "): expected " + bgrText(expected) +
            ", got " + bgrText(got));
}

void checkPicture(const Create& create, const std::filesystem::path& design, UINT cx, LONG width, LONG height) {
  const std::string at = "at " + std::to_string(cx) + ": ";
  Result result = thumbnail(create, design, cx);
  if (FAILED(result.hr) || !result.bitmap) {
    fail(at + "no thumbnail of the design with a preview (" + hex(result.hr) + ")");
    return;
  }
  DIBSECTION dib{};
  const int dibSize = static_cast<int>(sizeof dib);
  if (GetObjectW(result.bitmap, dibSize, &dib) != dibSize || !dib.dsBm.bmBits) {
    fail(at + "the thumbnail is no DIB section");
  } else {
    const LONG gotHeight = std::abs(dib.dsBm.bmHeight);
    check(dib.dsBm.bmWidth == width && gotHeight == height && dib.dsBm.bmBitsPixel == 32,
          at + "a 32-bit bitmap of " + std::to_string(dib.dsBm.bmWidth) + " x " + std::to_string(gotHeight) +
              " (expected " + std::to_string(width) + " x " + std::to_string(height) + ")");
    check(result.alpha == WTSAT_ARGB, at + "with alpha (WTSAT_ARGB)");
    if (dib.dsBm.bmWidth == width && gotHeight == height) {
      // Red at the top, blue at the bottom, as in the preview.
      const std::vector<std::uint8_t> pixels = topDownBgra(result.bitmap, width, height);
      if (pixels.empty()) {
        fail(at + "GetDIBits cannot read the thumbnail back");
      } else {
        checkColour(pixels, width, 0, 0, kRed, at + "the top row is red");
        checkColour(pixels, width, width - 1, height - 1, kBlue, at + "the bottom row is blue");
      }
      // The alpha of the handler's own bits (GetDIBits need not keep it):
      // opaque at the first and the last pixel (the preview is opaque).
      const auto* bits = static_cast<const std::uint8_t*>(dib.dsBm.bmBits);
      const std::size_t last = static_cast<std::size_t>(gotHeight - 1) * static_cast<std::size_t>(dib.dsBm.bmWidthBytes) +
                               static_cast<std::size_t>(width - 1) * 4;
      check(bits[3] == 255 && bits[last + 3] == 255,
            at + "opaque: expected alpha 255, got " + std::to_string(bits[3]) + " and " +
                std::to_string(bits[last + 3]));
    }
  }
  DeleteObject(result.bitmap);
}

void checkHandler(const Create& create, const std::filesystem::path& work) {
  std::error_code ignored;
  std::filesystem::create_directories(work, ignored);
  const auto design = work / L"with-preview.mitcad";
  const auto old = work / L"old.mitcad";
  const auto other = work / L"other.mitcad";
  if (!write(design, designWithPreview()) ||
      !write(old, "{\"format\": \"mitcad\", \"version\": 2, \"parameters\": [], \"features\": []}") ||
      !write(other, "{\"format\": \"other\"}")) {
    fail("cannot write the designs in " + work.string());
    return;
  }

  checkPicture(create, design, 96, 96, 72);
  checkPicture(create, design, 256, 256, 192);
  checkPicture(create, design, 1024, 256, 192);

  Result none = thumbnail(create, old, 96);
  check(none.hr == WTS_E_FAILEDEXTRACTION && !none.bitmap,
        "a design without a preview: no picture, nothing to show (" + hex(none.hr) + ")");
  if (none.bitmap) {
    DeleteObject(none.bitmap);
  }
  Result error = thumbnail(create, other, 96);
  check(FAILED(error.hr) && error.hr != WTS_E_FAILEDEXTRACTION && !error.bitmap,
        "no design: no picture, an error (" + hex(error.hr) + ")");
  if (error.bitmap) {
    DeleteObject(error.bitmap);
  }

  // The interfaces' rules: one Initialize, GetThumbnail after it.
  IInitializeWithStream* init = nullptr;
  if (SUCCEEDED(create(&init))) {
    IThumbnailProvider* provider = nullptr;
    if (SUCCEEDED(init->QueryInterface(__uuidof(IThumbnailProvider), reinterpret_cast<void**>(&provider)))) {
      HBITMAP bitmap = nullptr;
      WTS_ALPHATYPE alpha = WTSAT_UNKNOWN;
      check(provider->GetThumbnail(96, &bitmap, &alpha) == E_UNEXPECTED && !bitmap,
            "GetThumbnail before Initialize is refused");
      IStream* stream = nullptr;
      if (SUCCEEDED(SHCreateStreamOnFileEx(design.c_str(), STGM_READ, FILE_ATTRIBUTE_NORMAL, FALSE, nullptr,
                                           &stream))) {
        check(init->Initialize(stream, STGM_READ) == S_OK, "Initialize");
        check(init->Initialize(stream, STGM_READ) == HRESULT_FROM_WIN32(ERROR_ALREADY_INITIALIZED),
              "Initialize again is refused");
        stream->Release();
      }
      provider->Release();
    } else {
      fail("the handler has no IThumbnailProvider");
    }
    init->Release();
  } else {
    fail("no handler");
  }
}

std::wstring registryText(const wchar_t* key, const wchar_t* value) {
  wchar_t text[1024];
  DWORD size = static_cast<DWORD>(sizeof text);
  if (RegGetValueW(HKEY_CLASSES_ROOT, key, value, RRF_RT_REG_SZ, nullptr, text, &size) != ERROR_SUCCESS) {
    return {};
  }
  return text;
}

std::string narrow(const std::wstring& text) {
  std::string out;
  for (wchar_t c : text) {
    out += c < 128 ? static_cast<char>(c) : '?';
  }
  return out;
}

int usage() {
  std::printf(
      "usage: test_thumbnail_handler --dll <mitcad-thumbnails.dll> --work <dir>\n"
      "       test_thumbnail_handler --registered --work <dir> [--expect-dll <path>]\n");
  return 2;
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
  std::wstring dll;
  std::wstring expectDll;
  std::filesystem::path work;
  bool registered = false;
  for (int i = 1; i < argc; ++i) {
    const std::wstring arg = argv[i];
    if (arg == L"--dll" && i + 1 < argc) {
      dll = argv[++i];
    } else if (arg == L"--expect-dll" && i + 1 < argc) {
      expectDll = argv[++i];
    } else if (arg == L"--work" && i + 1 < argc) {
      work = argv[++i];
    } else if (arg == L"--registered") {
      registered = true;
    } else {
      return usage();
    }
  }
  if (work.empty() || registered == !dll.empty()) {
    return usage();
  }
  if (FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) {
    std::printf("FAIL: CoInitializeEx\n");
    return 1;
  }

  if (registered) {
    // As the shell finds the handler: .mitcad's ShellEx entry, then the
    // class's in-process server.
    const std::wstring shellEx =
        std::wstring(L".mitcad\\ShellEx\\") + mitcad::thumbnails::kThumbnailProviderKey;
    const std::wstring clsid = registryText(shellEx.c_str(), nullptr);
    check(_wcsicmp(clsid.c_str(), mitcad::thumbnails::kHandlerClsidText) == 0,
          ".mitcad's thumbnail handler is MitCad's (" + narrow(clsid) + ")");
    const std::wstring server =
        std::wstring(L"CLSID\\") + mitcad::thumbnails::kHandlerClsidText + L"\\InprocServer32";
    const std::wstring path = registryText(server.c_str(), nullptr);
    check(!path.empty() && (expectDll.empty() || _wcsicmp(path.c_str(), expectDll.c_str()) == 0),
          "its server is " + narrow(path));
    check(registryText(server.c_str(), L"ThreadingModel") == L"Apartment", "its threading model is Apartment");
    checkHandler(
        [](IInitializeWithStream** init) {
          return CoCreateInstance(mitcad::thumbnails::kHandlerClsid, nullptr, CLSCTX_INPROC_SERVER,
                                  __uuidof(IInitializeWithStream), reinterpret_cast<void**>(init));
        },
        work);
  } else {
    // As COM loads an in-process server: its dependencies from its folder.
    HMODULE module = LoadLibraryExW(dll.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (!module) {
      std::printf("FAIL: cannot load %s (error %lu)\n", narrow(dll).c_str(), GetLastError());
      CoUninitialize();
      return 1;
    }
    using GetClassObject = HRESULT(STDAPICALLTYPE*)(REFCLSID, REFIID, LPVOID*);
    using CanUnloadNow = HRESULT(STDAPICALLTYPE*)();
    const auto getClassObject =
        reinterpret_cast<GetClassObject>(reinterpret_cast<void*>(GetProcAddress(module, "DllGetClassObject")));
    const auto canUnloadNow =
        reinterpret_cast<CanUnloadNow>(reinterpret_cast<void*>(GetProcAddress(module, "DllCanUnloadNow")));
    if (!getClassObject || !canUnloadNow) {
      std::printf("FAIL: the DLL does not export DllGetClassObject and DllCanUnloadNow\n");
      FreeLibrary(module);
      CoUninitialize();
      return 1;
    }
    IClassFactory* factory = nullptr;
    const HRESULT hr =
        getClassObject(mitcad::thumbnails::kHandlerClsid, IID_IClassFactory, reinterpret_cast<void**>(&factory));
    if (FAILED(hr) || !factory) {
      std::printf("FAIL: no class factory (%s)\n", hex(hr).c_str());
      FreeLibrary(module);
      CoUninitialize();
      return 1;
    }
    IClassFactory* none = nullptr;
    check(getClassObject(IID_IClassFactory, IID_IClassFactory, reinterpret_cast<void**>(&none)) ==
                  CLASS_E_CLASSNOTAVAILABLE &&
              !none,
          "another class is not available");
    checkHandler(
        [factory](IInitializeWithStream** init) {
          return factory->CreateInstance(nullptr, __uuidof(IInitializeWithStream), reinterpret_cast<void**>(init));
        },
        work);
    factory->Release();
    check(canUnloadNow() == S_OK, "DllCanUnloadNow once every handler is released");
    FreeLibrary(module);
  }

  CoUninitialize();
  if (failures == 0) {
    std::printf("thumbnail handler: all passed\n");
  } else {
    std::printf("thumbnail handler: %d failed\n", failures);
  }
  return failures == 0 ? 0 : 1;
}
