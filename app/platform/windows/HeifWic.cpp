// SPDX-License-Identifier: MIT
// HEIF images on Windows (mitcad#1304, platform/PlatformHeif.hpp): the
// Windows Imaging Component with the system's HEIF decoder. Windows has it
// when Microsoft's HEIF Image Extensions are installed (some Windows
// versions ship them), and it decodes HEVC images with the HEVC Video
// Extensions; MitCad ships neither. WIC finds its decoder when a file is
// opened: nothing is looked for before.
#include "../PlatformHeif.hpp"

#include <windows.h>

#include <objbase.h>
#include <wincodec.h>
#include <wrl/client.h>

#include <QImage>
#include <QImageReader>
#include <QString>

namespace mitcad {
namespace {

using Microsoft::WRL::ComPtr;

QString hresult(HRESULT result) {
  return QStringLiteral("0x%1").arg(static_cast<quint32>(result), 8, 16, QLatin1Char('0'));
}

// COM for this thread while a file is decoded: the GUI thread has it from
// Qt already (S_FALSE, or RPC_E_CHANGED_MODE for another apartment, which
// WIC's factory works in as well).
class ComScope {
public:
  ComScope() : m_result(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED)) {}
  ~ComScope() {
    if (SUCCEEDED(m_result)) {
      CoUninitialize();
    }
  }
  ComScope(const ComScope&) = delete;
  ComScope& operator=(const ComScope&) = delete;

private:
  HRESULT m_result;
};

} // namespace

HeifDecoded platformDecodeHeif(const QByteArray& bytes) {
  const ComScope com;
  ComPtr<IWICImagingFactory> factory;
  HRESULT result = CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory));
  if (FAILED(result)) {
    return HeifDecoded::missing(QStringLiteral("no imaging factory (%1)").arg(hresult(result)));
  }
  ComPtr<IWICStream> stream;
  result = factory->CreateStream(&stream);
  if (SUCCEEDED(result)) {
    // WIC only reads the bytes; they outlive the stream.
    result = stream->InitializeFromMemory(reinterpret_cast<BYTE*>(const_cast<char*>(bytes.constData())),
                                          static_cast<DWORD>(bytes.size()));
  }
  if (FAILED(result)) {
    return HeifDecoded::failed(QStringLiteral("Windows cannot read the file (%1)").arg(hresult(result)));
  }
  // No decoder of the file's container: the HEIF Image Extensions are not
  // installed.
  ComPtr<IWICBitmapDecoder> decoder;
  result = factory->CreateDecoderFromStream(stream.Get(), nullptr, WICDecodeMetadataCacheOnDemand, &decoder);
  if (FAILED(result)) {
    return HeifDecoded::missing(QStringLiteral("no HEIF decoder (%1)").arg(hresult(result)));
  }
  // The decoder is there but the frame does not decode: the HEVC Video
  // Extensions are missing (WIC's HEIF decoder decodes HEVC through Media
  // Foundation, which says so with errors that differ between Windows
  // versions), so every failure from here on asks for them.
  ComPtr<IWICBitmapFrameDecode> frame;
  result = decoder->GetFrame(0, &frame);
  UINT width = 0;
  UINT height = 0;
  if (SUCCEEDED(result)) {
    result = frame->GetSize(&width, &height);
  }
  if (FAILED(result) || width == 0 || height == 0) {
    return HeifDecoded::missing(QStringLiteral("the HEIF image does not decode (%1)").arg(hresult(result)));
  }
  // The limit Qt's own readers keep to (QImageReader::allocationLimit, MB).
  const int limit = QImageReader::allocationLimit();
  if (limit > 0 && static_cast<qint64>(width) * height * 4 > (static_cast<qint64>(limit) << 20)) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image of %1 x %2 pixels needs more than the %3 MB an image may take")
                                   .arg(width)
                                   .arg(height)
                                   .arg(limit));
  }
  ComPtr<IWICFormatConverter> converter;
  result = factory->CreateFormatConverter(&converter);
  if (SUCCEEDED(result)) {
    result = converter->Initialize(frame.Get(), GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, nullptr, 0.0,
                                   WICBitmapPaletteTypeCustom);
  }
  if (FAILED(result)) {
    return HeifDecoded::missing(QStringLiteral("the HEIF image does not convert (%1)").arg(hresult(result)));
  }
  // 32bppBGRA, straight alpha: QImage's ARGB32 on a little-endian machine.
  QImage image(static_cast<int>(width), static_cast<int>(height), QImage::Format_ARGB32);
  if (image.isNull()) {
    return HeifDecoded::failed(
        QStringLiteral("there is no memory for the image's %1 x %2 pixels").arg(width).arg(height));
  }
  result = converter->CopyPixels(nullptr, static_cast<UINT>(image.bytesPerLine()),
                                 static_cast<UINT>(image.sizeInBytes()), image.bits());
  if (FAILED(result)) {
    return HeifDecoded::missing(QStringLiteral("the HEIF image does not decode (%1)").arg(hresult(result)));
  }
  HeifDecoded out;
  out.result = HeifDecoded::Result::Decoded;
  out.image = image;
  // WIC's HEIF decoder applies the file's irot and imir itself (and its
  // /heifProps/Orientation then says 1): the pixels come upright, and an
  // EXIF orientation in the file must not turn them again.
  out.orientation = 1;
  return out;
}

} // namespace mitcad
