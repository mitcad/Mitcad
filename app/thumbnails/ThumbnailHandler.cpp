// SPDX-License-Identifier: MIT
// mitcad-thumbnails.dll (mitcad#1695, Windows): Explorer's thumbnails of
// MitCad designs. An in-process COM server whose one class
// (ThumbnailHandler.hpp's CLSID) is a thumbnail handler: Explorer, or the
// isolated process it runs thumbnail handlers in, gives it the .mitcad
// file as a stream (IInitializeWithStream) and asks for a picture of at
// most cx pixels a side (IThumbnailProvider::GetThumbnail). The handler
// reads the stream and asks the thumbnailer's C ABI
// (core/thumbnailer-capi/include/mitcad_thumbnailer.h, the same reader as
// the Linux thumbnailer's) for the preview the design file carries, scaled
// down to fit and never up; it returns those pixels as a 32-bit top-down
// DIB section with alpha (WTSAT_ARGB).
//
// A design without a preview (saved before MitCad kept them, or by a
// program without a 3D view) gives no picture and no error of its own:
// WTS_E_FAILEDEXTRACTION, on which Explorer shows the file type's icon. A
// file that is no design, or whose preview cannot be read, gives
// HRESULT_FROM_WIN32(ERROR_INVALID_DATA); a file over kMaxFileBytes
// HRESULT_FROM_WIN32(ERROR_FILE_TOO_LARGE). Nothing is shown or logged.
//
// The installer registers the class for .mitcad and removes it on
// uninstall (packaging/windows/filetype.nsh); the DLL needs no
// DllRegisterServer. Its exports: DllGetClassObject, DllCanUnloadNow
// (mitcad-thumbnails.def).

#include "ThumbnailHandler.hpp"

#include <objbase.h>
#include <propsys.h>
#include <thumbcache.h>

#include <atomic>
#include <cstdint>
#include <new>
#include <vector>

#include "mitcad_thumbnailer.h"

namespace {

using mitcad::thumbnails::kHandlerClsid;
using mitcad::thumbnails::kMaxFileBytes;

// The objects alive and the class factory's locks: the DLL may be unloaded
// only when both are zero.
std::atomic<long> gObjects{0};
std::atomic<long> gLocks{0};

// The whole stream from its current position; false when it cannot be
// read or is larger than kMaxFileBytes (tooLarge then true).
bool readAll(IStream* stream, std::vector<std::uint8_t>& bytes, bool& tooLarge) {
  tooLarge = false;
  STATSTG stat{};
  if (SUCCEEDED(stream->Stat(&stat, STATFLAG_NONAME))) {
    if (stat.cbSize.QuadPart > kMaxFileBytes) {
      tooLarge = true;
      return false;
    }
    bytes.reserve(static_cast<std::size_t>(stat.cbSize.QuadPart));
  }
  std::uint8_t chunk[64 * 1024];
  for (;;) {
    ULONG read = 0;
    const HRESULT hr = stream->Read(chunk, static_cast<ULONG>(sizeof chunk), &read);
    if (FAILED(hr)) {
      return false;
    }
    if (read > 0) {
      if (bytes.size() + read > kMaxFileBytes) {
        tooLarge = true;
        return false;
      }
      bytes.insert(bytes.end(), chunk, chunk + read);
    }
    // Nothing more: the end of the stream.
    if (read == 0) {
      return true;
    }
  }
}

// RGBA pixels, row by row from the top, as a 32-bit top-down DIB section
// (BGRA, alpha not premultiplied, as WTSAT_ARGB wants); null on failure.
HBITMAP toBitmap(const MitcadThumbnail& thumbnail) {
  BITMAPINFO info{};
  info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
  info.bmiHeader.biWidth = static_cast<LONG>(thumbnail.width);
  info.bmiHeader.biHeight = -static_cast<LONG>(thumbnail.height);
  info.bmiHeader.biPlanes = 1;
  info.bmiHeader.biBitCount = 32;
  info.bmiHeader.biCompression = BI_RGB;
  void* bits = nullptr;
  HBITMAP bitmap = CreateDIBSection(nullptr, &info, DIB_RGB_COLORS, &bits, nullptr, 0);
  if (!bitmap || !bits) {
    if (bitmap) {
      DeleteObject(bitmap);
    }
    return nullptr;
  }
  auto* out = static_cast<std::uint8_t*>(bits);
  const std::size_t count = static_cast<std::size_t>(thumbnail.width) * thumbnail.height;
  for (std::size_t i = 0; i < count; ++i) {
    const std::uint8_t* rgba = thumbnail.pixels + i * 4;
    out[i * 4 + 0] = rgba[2];
    out[i * 4 + 1] = rgba[1];
    out[i * 4 + 2] = rgba[0];
    out[i * 4 + 3] = rgba[3];
  }
  GdiFlush();
  return bitmap;
}

class ThumbnailHandler final : public IInitializeWithStream, public IThumbnailProvider {
 public:
  ThumbnailHandler() { ++gObjects; }
  ThumbnailHandler(const ThumbnailHandler&) = delete;
  ThumbnailHandler& operator=(const ThumbnailHandler&) = delete;

  // IUnknown
  IFACEMETHODIMP QueryInterface(REFIID riid, void** object) override {
    if (!object) {
      return E_POINTER;
    }
    if (riid == IID_IUnknown || riid == __uuidof(IInitializeWithStream)) {
      *object = static_cast<IInitializeWithStream*>(this);
    } else if (riid == __uuidof(IThumbnailProvider)) {
      *object = static_cast<IThumbnailProvider*>(this);
    } else {
      *object = nullptr;
      return E_NOINTERFACE;
    }
    AddRef();
    return S_OK;
  }
  IFACEMETHODIMP_(ULONG) AddRef() override { return ++references_; }
  IFACEMETHODIMP_(ULONG) Release() override {
    const ULONG left = --references_;
    if (left == 0) {
      delete this;
    }
    return left;
  }

  // IInitializeWithStream: the file, once.
  IFACEMETHODIMP Initialize(IStream* stream, DWORD /*mode*/) override {
    if (!stream) {
      return E_INVALIDARG;
    }
    if (stream_) {
      return HRESULT_FROM_WIN32(ERROR_ALREADY_INITIALIZED);
    }
    stream_ = stream;
    stream_->AddRef();
    return S_OK;
  }

  // IThumbnailProvider: the preview at most cx pixels a side.
  IFACEMETHODIMP GetThumbnail(UINT cx, HBITMAP* bitmap, WTS_ALPHATYPE* alpha) override {
    if (!bitmap || !alpha) {
      return E_POINTER;
    }
    *bitmap = nullptr;
    *alpha = WTSAT_UNKNOWN;
    if (!stream_) {
      return E_UNEXPECTED;
    }
    if (cx == 0) {
      return E_INVALIDARG;
    }
    try {
      std::vector<std::uint8_t> bytes;
      bool tooLarge = false;
      if (!readAll(stream_, bytes, tooLarge)) {
        return tooLarge ? HRESULT_FROM_WIN32(ERROR_FILE_TOO_LARGE) : E_FAIL;
      }
      MitcadThumbnail thumbnail{};
      const std::int32_t result =
          mitcad_thumbnail_pixels(bytes.data(), bytes.size(), static_cast<std::uint32_t>(cx), &thumbnail);
      if (result == MITCAD_THUMBNAIL_NONE) {
        return WTS_E_FAILEDEXTRACTION;
      }
      if (result != MITCAD_THUMBNAIL_PICTURE || !thumbnail.pixels ||
          thumbnail.length != static_cast<std::size_t>(thumbnail.width) * thumbnail.height * 4) {
        mitcad_thumbnail_free(&thumbnail);
        return HRESULT_FROM_WIN32(ERROR_INVALID_DATA);
      }
      HBITMAP made = toBitmap(thumbnail);
      mitcad_thumbnail_free(&thumbnail);
      if (!made) {
        return E_OUTOFMEMORY;
      }
      *bitmap = made;
      *alpha = WTSAT_ARGB;
      return S_OK;
    } catch (const std::bad_alloc&) {
      return E_OUTOFMEMORY;
    } catch (...) {
      return E_FAIL;
    }
  }

 private:
  ~ThumbnailHandler() {
    if (stream_) {
      stream_->Release();
    }
    --gObjects;
  }

  std::atomic<ULONG> references_{1};
  IStream* stream_ = nullptr;
};

class ClassFactory final : public IClassFactory {
 public:
  IFACEMETHODIMP QueryInterface(REFIID riid, void** object) override {
    if (!object) {
      return E_POINTER;
    }
    if (riid == IID_IUnknown || riid == IID_IClassFactory) {
      *object = static_cast<IClassFactory*>(this);
      AddRef();
      return S_OK;
    }
    *object = nullptr;
    return E_NOINTERFACE;
  }
  // One static factory: its references keep nothing alive (LockServer
  // does).
  IFACEMETHODIMP_(ULONG) AddRef() override { return 2; }
  IFACEMETHODIMP_(ULONG) Release() override { return 1; }

  IFACEMETHODIMP CreateInstance(IUnknown* outer, REFIID riid, void** object) override {
    if (!object) {
      return E_POINTER;
    }
    *object = nullptr;
    if (outer) {
      return CLASS_E_NOAGGREGATION;
    }
    auto* handler = new (std::nothrow) ThumbnailHandler();
    if (!handler) {
      return E_OUTOFMEMORY;
    }
    const HRESULT hr = handler->QueryInterface(riid, object);
    handler->Release();
    return hr;
  }
  IFACEMETHODIMP LockServer(BOOL lock) override {
    if (lock) {
      ++gLocks;
    } else {
      --gLocks;
    }
    return S_OK;
  }
};

ClassFactory gFactory;

}  // namespace

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID /*reserved*/) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(instance);
  }
  return TRUE;
}

STDAPI DllGetClassObject(REFCLSID clsid, REFIID riid, LPVOID* object) {
  if (!object) {
    return E_POINTER;
  }
  *object = nullptr;
  if (clsid != kHandlerClsid) {
    return CLASS_E_CLASSNOTAVAILABLE;
  }
  return gFactory.QueryInterface(riid, object);
}

STDAPI DllCanUnloadNow() {
  return gObjects == 0 && gLocks == 0 ? S_OK : S_FALSE;
}
