// SPDX-License-Identifier: MIT
#include "view/Turntable.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <map>
#include <set>
#include <vector>

#include <QByteArray>
#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>
#include <QIODevice>
#include <QStandardPaths>

#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad {
namespace {

constexpr double kRadians = 3.14159265358979323846 / 180.0;

// A big-endian number of `count` bytes at `at`.
std::uint64_t bigEndian(const QByteArray& bytes, int at, int count) {
  std::uint64_t value = 0;
  for (int i = 0; i < count; ++i) {
    value = (value << 8) | static_cast<std::uint8_t>(bytes.at(at + i));
  }
  return value;
}

constexpr std::uint32_t fourcc(const char (&text)[5]) {
  return (static_cast<std::uint32_t>(static_cast<unsigned char>(text[0])) << 24) |
         (static_cast<std::uint32_t>(static_cast<unsigned char>(text[1])) << 16) |
         (static_cast<std::uint32_t>(static_cast<unsigned char>(text[2])) << 8) |
         static_cast<std::uint32_t>(static_cast<unsigned char>(text[3]));
}

// ---------------------------------------------------------------------------
// MP4 (ISO base media file format): boxes of a 32-bit size (1: a 64-bit one
// follows; 0: to the end) and a four-character type.

struct Mp4Track {
  bool video = false;
  int frames = -1;
  int width = 0;
  int height = 0;
};

struct Box {
  std::uint32_t type = 0;
  qint64 body = 0; // where its contents start
  qint64 next = 0; // where the next box starts
};

bool readBox(QIODevice& file, qint64 at, qint64 end, Box& box) {
  if (at + 8 > end || !file.seek(at)) {
    return false;
  }
  const QByteArray head = file.read(8);
  if (head.size() < 8) {
    return false;
  }
  std::uint64_t size = bigEndian(head, 0, 4);
  box.type = static_cast<std::uint32_t>(bigEndian(head, 4, 4));
  box.body = at + 8;
  if (size == 1) {
    const QByteArray large = file.read(8);
    if (large.size() < 8) {
      return false;
    }
    size = bigEndian(large, 0, 8);
    box.body = at + 16;
  } else if (size == 0) {
    size = static_cast<std::uint64_t>(end - at);
  }
  if (size < static_cast<std::uint64_t>(box.body - at) || size > static_cast<std::uint64_t>(end - at)) {
    return false;
  }
  box.next = at + static_cast<qint64>(size);
  return true;
}

QByteArray readAt(QIODevice& file, qint64 at, qint64 count) {
  return file.seek(at) ? file.read(count) : QByteArray();
}

bool walkMp4(QIODevice& file, qint64 at, qint64 end, int depth, Mp4Track* track, std::vector<Mp4Track>& tracks) {
  if (depth > 8) {
    return false;
  }
  while (at < end) {
    Box box;
    if (!readBox(file, at, end, box)) {
      return false;
    }
    const qint64 length = box.next - box.body;
    if (box.type == fourcc("trak")) {
      Mp4Track found;
      if (!walkMp4(file, box.body, box.next, depth + 1, &found, tracks)) {
        return false;
      }
      tracks.push_back(found);
    } else if (box.type == fourcc("moov") || box.type == fourcc("mdia") || box.type == fourcc("minf") ||
               box.type == fourcc("stbl")) {
      if (!walkMp4(file, box.body, box.next, depth + 1, track, tracks)) {
        return false;
      }
    } else if (track != nullptr && box.type == fourcc("tkhd")) {
      // Version 0: 32-bit times; 1: 64-bit. The width and the height,
      // 16.16 fixed point, end the box.
      const QByteArray body = readAt(file, box.body, length);
      if (body.isEmpty()) {
        return false;
      }
      const int at16 = body.at(0) == 1 ? 88 : 76;
      if (body.size() < at16 + 8) {
        return false;
      }
      track->width = static_cast<int>(bigEndian(body, at16, 4) >> 16);
      track->height = static_cast<int>(bigEndian(body, at16 + 4, 4) >> 16);
    } else if (track != nullptr && box.type == fourcc("hdlr")) {
      const QByteArray body = readAt(file, box.body, std::min<qint64>(length, 12));
      if (body.size() < 12) {
        return false;
      }
      track->video = bigEndian(body, 8, 4) == fourcc("vide");
    } else if (track != nullptr && (box.type == fourcc("stsz") || box.type == fourcc("stz2"))) {
      // The sample count after the version, the flags and the sample size
      // (stz2: three reserved bytes and the field size).
      const QByteArray body = readAt(file, box.body, std::min<qint64>(length, 12));
      if (body.size() < 12) {
        return false;
      }
      track->frames = static_cast<int>(bigEndian(body, 8, 4));
    }
    at = box.next;
  }
  return true;
}

TurntableVideo readMp4(QIODevice& file) {
  TurntableVideo video;
  std::vector<Mp4Track> tracks;
  if (!walkMp4(file, 0, file.size(), 0, nullptr, tracks)) {
    video.error = QStringLiteral("a box of the MP4 file is cut short");
    return video;
  }
  for (const Mp4Track& track : tracks) {
    if (track.video && track.frames >= 0) {
      video.ok = true;
      video.frames = track.frames;
      video.width = track.width;
      video.height = track.height;
      return video;
    }
  }
  video.error = QStringLiteral("the MP4 file has no video track");
  return video;
}

// ---------------------------------------------------------------------------
// WebM (Matroska, EBML): elements of a variable-length id (its length
// marker kept) and a variable-length size (all ones: unknown, to the end of
// its parent). The scan steps into the elements that lead to the tracks and
// the blocks and over everything else.

constexpr std::uint64_t kEbml = 0x1A45DFA3;
constexpr std::uint64_t kSegment = 0x18538067;
constexpr std::uint64_t kTracks = 0x1654AE6B;
constexpr std::uint64_t kTrackEntry = 0xAE;
constexpr std::uint64_t kTrackNumber = 0xD7;
constexpr std::uint64_t kTrackType = 0x83;
constexpr std::uint64_t kVideo = 0xE0;
constexpr std::uint64_t kPixelWidth = 0xB0;
constexpr std::uint64_t kPixelHeight = 0xBA;
constexpr std::uint64_t kCluster = 0x1F43B675;
constexpr std::uint64_t kBlockGroup = 0xA0;
constexpr std::uint64_t kBlock = 0xA1;
constexpr std::uint64_t kSimpleBlock = 0xA3;

struct Vint {
  std::uint64_t value = 0;
  int length = 0;
  bool unknown = false; // a size of all ones
};

bool readVint(QIODevice& file, int longest, bool keepMarker, Vint& out) {
  char first = 0;
  if (!file.getChar(&first)) {
    return false;
  }
  const auto lead = static_cast<std::uint8_t>(first);
  int length = 1;
  while (length <= 8 && (lead & (0x80u >> (length - 1))) == 0) {
    ++length;
  }
  if (length > longest) {
    return false;
  }
  const std::uint8_t mask = static_cast<std::uint8_t>(0xFFu >> length);
  std::uint64_t value = keepMarker ? lead : (lead & mask);
  bool allOnes = (lead & mask) == mask;
  for (int i = 1; i < length; ++i) {
    char next = 0;
    if (!file.getChar(&next)) {
      return false;
    }
    const auto byte = static_cast<std::uint8_t>(next);
    allOnes = allOnes && byte == 0xFF;
    value = (value << 8) | byte;
  }
  out.value = value;
  out.length = length;
  out.unknown = !keepMarker && allOnes;
  return true;
}

struct WebmTrack {
  std::uint64_t number = 0;
  std::uint64_t type = 0;
  std::uint64_t width = 0;
  std::uint64_t height = 0;
};

TurntableVideo readWebm(QIODevice& file) {
  TurntableVideo video;
  std::vector<WebmTrack> tracks;
  std::map<std::uint64_t, int> blocks; // by track number
  const qint64 end = file.size();
  qint64 at = 0;
  while (at < end) {
    Vint id;
    Vint size;
    if (!file.seek(at) || !readVint(file, 4, true, id) || !readVint(file, 8, false, size)) {
      video.error = QStringLiteral("an element of the WebM file is cut short");
      return video;
    }
    const qint64 data = file.pos();
    const std::uint64_t kind = id.value;
    if (kind == kSegment || kind == kTracks || kind == kTrackEntry || kind == kVideo || kind == kCluster ||
        kind == kBlockGroup) {
      if (kind == kTrackEntry) {
        tracks.emplace_back();
      }
      at = data; // into its children
      continue;
    }
    if (size.unknown || size.value > static_cast<std::uint64_t>(end - data)) {
      video.error = QStringLiteral("an element of the WebM file is cut short");
      return video;
    }
    const auto length = static_cast<qint64>(size.value);
    if (kind == kSimpleBlock || kind == kBlock) {
      Vint track;
      if (!readVint(file, 8, false, track)) {
        video.error = QStringLiteral("a block of the WebM file is cut short");
        return video;
      }
      ++blocks[track.value];
    } else if (!tracks.empty() && (kind == kTrackNumber || kind == kTrackType || kind == kPixelWidth ||
                                   kind == kPixelHeight) && length >= 1 && length <= 8) {
      const QByteArray bytes = file.read(length);
      if (bytes.size() != length) {
        video.error = QStringLiteral("an element of the WebM file is cut short");
        return video;
      }
      const std::uint64_t value = bigEndian(bytes, 0, static_cast<int>(length));
      WebmTrack& track = tracks.back();
      if (kind == kTrackNumber) {
        track.number = value;
      } else if (kind == kTrackType) {
        track.type = value;
      } else if (kind == kPixelWidth) {
        track.width = value;
      } else {
        track.height = value;
      }
    }
    at = data + length;
  }
  for (const WebmTrack& track : tracks) {
    if (track.type == 1) { // video
      video.ok = true;
      video.frames = blocks.count(track.number) != 0 ? blocks.at(track.number) : 0;
      video.width = static_cast<int>(track.width);
      video.height = static_cast<int>(track.height);
      return video;
    }
  }
  video.error = QStringLiteral("the WebM file has no video track");
  return video;
}

} // namespace

bool turntableHasVideo(TurntableOutput output) { return output != TurntableOutput::Frames; }

bool turntableKeepsFrames(TurntableOutput output) {
  return output == TurntableOutput::Mp4Frames || output == TurntableOutput::WebMFrames ||
         output == TurntableOutput::Frames;
}

QString turntableExtension(TurntableOutput output) {
  switch (output) {
  case TurntableOutput::Mp4:
  case TurntableOutput::Mp4Frames:
    return QStringLiteral("mp4");
  case TurntableOutput::WebM:
  case TurntableOutput::WebMFrames:
    return QStringLiteral("webm");
  case TurntableOutput::Frames:
    break;
  }
  return QString();
}

QString turntableOutputKey(TurntableOutput output) {
  switch (output) {
  case TurntableOutput::Mp4:
    return QStringLiteral("mp4");
  case TurntableOutput::Mp4Frames:
    return QStringLiteral("mp4+frames");
  case TurntableOutput::WebM:
    return QStringLiteral("webm");
  case TurntableOutput::WebMFrames:
    return QStringLiteral("webm+frames");
  case TurntableOutput::Frames:
    break;
  }
  return QStringLiteral("frames");
}

std::optional<TurntableOutput> turntableOutputOf(const QString& key) {
  for (const TurntableOutput output : {TurntableOutput::Mp4, TurntableOutput::Mp4Frames, TurntableOutput::WebM,
                                       TurntableOutput::WebMFrames, TurntableOutput::Frames}) {
    if (turntableOutputKey(output) == key) {
      return output;
    }
  }
  return std::nullopt;
}

double turntableStep(double angle, int frames) {
  if (frames < 2 || !std::isfinite(angle)) {
    return 0.0;
  }
  const double turns = std::abs(angle) / 360.0;
  const bool whole = turns >= 1.0 - 1e-9 && std::abs(turns - std::round(turns)) <= 1e-9;
  return angle / (whole ? frames : frames - 1);
}

occ::handle<Graphic3d_Camera> turntableCamera(const occ::handle<Graphic3d_Camera>& start, const gp_Ax1& axis,
                                              double degrees) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera(start);
  if (degrees == 0.0 || !std::isfinite(degrees)) {
    return camera;
  }
  gp_Trsf turn;
  turn.SetRotation(axis, degrees * kRadians);
  camera->SetEyeAndCenter(start->Eye().Transformed(turn), start->Center().Transformed(turn));
  camera->SetUp(start->Up().Transformed(turn));
  camera->OrthogonalizeUp();
  return camera;
}

std::optional<double> turnAbout(const gp_Dir& from, const gp_Dir& to, const gp_Dir& axis) {
  const gp_Vec a(axis);
  const gp_Vec f = gp_Vec(from) - a * gp_Vec(from).Dot(a);
  const gp_Vec t = gp_Vec(to) - a * gp_Vec(to).Dot(a);
  if (f.Magnitude() < 1e-9 || t.Magnitude() < 1e-9) {
    return std::nullopt;
  }
  double degrees = std::atan2(f.Crossed(t).Dot(a), f.Dot(t)) / kRadians;
  if (degrees <= -180.0) {
    degrees += 360.0;
  }
  return degrees;
}

QString turntableFrameName(int index) {
  return QStringLiteral("frame-%1.png").arg(index, 4, 10, QLatin1Char('0'));
}

QSize turntableFrameSize(const QSize& size, TurntableOutput output) {
  if (!turntableHasVideo(output)) {
    return QSize(std::max(1, size.width()), std::max(1, size.height()));
  }
  const auto even = [](int side) { return std::max(2, side - side % 2); };
  return QSize(even(size.width()), even(size.height()));
}

QString findFfmpeg(const QString& chosen) {
  const auto usable = [](const QString& path) {
    const QFileInfo file(path);
    return !path.isEmpty() && file.isFile() && file.isExecutable() ? file.absoluteFilePath() : QString();
  };
  if (const QString path = usable(chosen); !path.isEmpty()) {
    return path;
  }
  if (const QString path = usable(qEnvironmentVariable("MITCAD_FFMPEG")); !path.isEmpty()) {
    return path;
  }
  const QString name = QStringLiteral("ffmpeg");
  if (const QString path = QStandardPaths::findExecutable(name); !path.isEmpty()) {
    return path;
  }
  QStringList folders{QCoreApplication::applicationDirPath()};
#ifndef Q_OS_WIN
  // Homebrew and MacPorts on macOS (an application started from the
  // Finder or the Dock gets no shell PATH), /usr/local on Linux.
  folders << QStringLiteral("/opt/homebrew/bin") << QStringLiteral("/usr/local/bin") << QStringLiteral("/opt/local/bin");
#endif
  return QStandardPaths::findExecutable(name, folders);
}

QString turntableEncoder(TurntableOutput output, const QString& encoderListing) {
  // The listing's rows: flags (the first one V for video), the name, a
  // description.
  std::set<QString> listed;
  for (const QString& line : encoderListing.split(QLatin1Char('\n'))) {
    const QStringList words = line.simplified().split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (words.size() >= 2 && words.at(0).size() == 6 && words.at(0).startsWith(QLatin1Char('V'))) {
      listed.insert(words.at(1));
    }
  }
  const bool webm = output == TurntableOutput::WebM || output == TurntableOutput::WebMFrames;
  const QStringList preferred =
      webm ? QStringList{QStringLiteral("libvpx-vp9"), QStringLiteral("libvpx"), QStringLiteral("libaom-av1"),
                         QStringLiteral("libsvtav1")}
           : QStringList{QStringLiteral("libx264"), QStringLiteral("libopenh264"), QStringLiteral("h264_videotoolbox"),
                         QStringLiteral("h264_mf"), QStringLiteral("mpeg4")};
  for (const QString& encoder : preferred) {
    if (listed.count(encoder) != 0) {
      return encoder;
    }
  }
  return QString();
}

QStringList turntableFfmpegArguments(TurntableOutput output, const QString& encoder, double fps, const QSize& size,
                                     int frames, const QString& video) {
  const QString rate = QString::number(fps, 'g', 6);
  // About 0.15 bits a pixel for the encoders that take a bit rate.
  const double bits = std::clamp(static_cast<double>(size.width()) * size.height() * fps * 0.15, 1.0e6, 1.0e8);
  const QString bitRate = QStringLiteral("%1k").arg(static_cast<qint64>(std::llround(bits / 1000.0)));
  // The frames by a relative pattern in the program's working folder: a %
  // in the folder's own name would read as a number's place.
  QStringList arguments{QStringLiteral("-hide_banner"),  QStringLiteral("-nostdin"), QStringLiteral("-loglevel"),
                        QStringLiteral("error"),          QStringLiteral("-y"),       QStringLiteral("-framerate"),
                        rate,                             QStringLiteral("-start_number"), QStringLiteral("1"),
                        QStringLiteral("-i"),             QString::fromLatin1(kTurntableFramePattern),
                        QStringLiteral("-frames:v"),      QString::number(frames),
                        QStringLiteral("-c:v"),           encoder,                    QStringLiteral("-pix_fmt"),
                        QStringLiteral("yuv420p")};
  if (encoder == QLatin1String("libx264")) {
    arguments << QStringLiteral("-crf") << QStringLiteral("18") << QStringLiteral("-preset") << QStringLiteral("medium");
  } else if (encoder == QLatin1String("mpeg4")) {
    arguments << QStringLiteral("-q:v") << QStringLiteral("2");
  } else if (encoder == QLatin1String("libvpx-vp9")) {
    arguments << QStringLiteral("-crf") << QStringLiteral("31") << QStringLiteral("-b:v") << QStringLiteral("0")
              << QStringLiteral("-row-mt") << QStringLiteral("1");
  } else if (encoder == QLatin1String("libvpx")) {
    arguments << QStringLiteral("-crf") << QStringLiteral("10") << QStringLiteral("-b:v") << bitRate;
  } else if (encoder == QLatin1String("libaom-av1")) {
    arguments << QStringLiteral("-crf") << QStringLiteral("30") << QStringLiteral("-b:v") << QStringLiteral("0")
              << QStringLiteral("-cpu-used") << QStringLiteral("6");
  } else if (encoder == QLatin1String("libsvtav1")) {
    arguments << QStringLiteral("-crf") << QStringLiteral("35");
  } else {
    arguments << QStringLiteral("-b:v") << bitRate; // libopenh264, h264_videotoolbox, h264_mf
  }
  if (turntableExtension(output) == QLatin1String("webm")) {
    arguments << QStringLiteral("-f") << QStringLiteral("webm");
  } else {
    arguments << QStringLiteral("-movflags") << QStringLiteral("+faststart") << QStringLiteral("-f")
              << QStringLiteral("mp4");
  }
  // file: keeps a name with a colon from reading as another protocol.
  arguments << QStringLiteral("file:") + video;
  return arguments;
}

TurntableVideo readTurntableVideo(QIODevice& file) {
  TurntableVideo video;
  if (!file.seek(0)) {
    video.error = QStringLiteral("the file cannot be read");
    return video;
  }
  const QByteArray head = file.read(8);
  if (head.size() >= 4 && bigEndian(head, 0, 4) == kEbml) {
    return readWebm(file);
  }
  if (head.size() == 8 && bigEndian(head, 4, 4) == fourcc("ftyp")) {
    return readMp4(file);
  }
  video.error = QStringLiteral("the file is neither an MP4 nor a WebM video");
  return video;
}

TurntableVideo readTurntableVideo(const QString& path) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    TurntableVideo video;
    video.error = QStringLiteral("the file cannot be opened: %1").arg(file.errorString());
    return video;
  }
  return readTurntableVideo(file);
}

} // namespace mitcad
