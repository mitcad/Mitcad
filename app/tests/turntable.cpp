// SPDX-License-Identifier: MIT
// Export Turntable (mitcad#2523, view/Turntable.hpp): a 36-frame turn of
// 360 degrees steps the camera 10 degrees a frame about the axis (the
// directions measured about it, the loop closing from the last frame to
// the first), on both projections and about an axis off the design's Z;
// the eye's distance, the zoom and the target's place on the turned axis
// kept; the step of other angles; the frames' names and sizes; the FFmpeg
// encoder chosen from its listing and its arguments; and the video's
// frame count and size read from synthetic MP4 and WebM containers (the
// boxes and elements FFmpeg writes), a cut one refused.
#include "UnitTests.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <optional>

#include <QBuffer>
#include <QByteArray>
#include <QString>
#include <QStringList>

#include <Graphic3d_Camera.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "view/Turntable.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "turntable.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

bool nearly(double a, double b, double tolerance = 1e-9) { return std::abs(a - b) <= tolerance; }

using mitcad::TurntableOutput;

// A camera as the 3D view has one, looking down at `target` from above
// and to the side.
occ::handle<Graphic3d_Camera> viewCamera(bool perspective, const gp_Pnt& target) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
  camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                        : Graphic3d_Camera::Projection_Orthographic);
  camera->SetAspect(1.6);
  camera->SetEyeAndCenter(gp_Pnt(target.X() + 300.0, target.Y() - 400.0, target.Z() + 250.0), target);
  camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
  camera->OrthogonalizeUp();
  camera->SetScale(120.0);
  return camera;
}

// 36 frames of 360 degrees about `axis`: each camera direction 10 degrees
// on from the one before about the axis, the last one 10 degrees before
// the first; the camera's distance, zoom and projection kept, its target
// on the turned place.
void fullTurn(bool perspective, const gp_Ax1& axis, const gp_Pnt& target) {
  const occ::handle<Graphic3d_Camera> start = viewCamera(perspective, target);
  const int frames = 36;
  const double step = mitcad::turntableStep(360.0, frames);
  CHECK(nearly(step, 10.0));
  std::optional<gp_Dir> previous;
  const double distance = start->Eye().Distance(start->Center());
  const double alongAxis = gp_Vec(start->Direction()).Dot(gp_Vec(axis.Direction()));
  const double fromAxis = gp_Lin(axis).Distance(start->Center());
  int steps = 0;
  for (int frame = 0; frame < frames; ++frame) {
    const occ::handle<Graphic3d_Camera> camera = mitcad::turntableCamera(start, axis, frame * step);
    CHECK(camera != start); // a camera of its own: the start one is left as it is
    CHECK(nearly(camera->Eye().Distance(camera->Center()), distance, 1e-6));
    CHECK(nearly(camera->Scale(), start->Scale(), 1e-9));
    CHECK(camera->IsOrthographic() == !perspective);
    CHECK(nearly(gp_Vec(camera->Direction()).Dot(gp_Vec(axis.Direction())), alongAxis, 1e-9));
    CHECK(nearly(gp_Lin(axis).Distance(camera->Center()), fromAxis, 1e-6));
    CHECK(nearly(gp_Vec(camera->Up()).Dot(gp_Vec(camera->Direction())), 0.0, 1e-9));
    if (previous) {
      const std::optional<double> turned = mitcad::turnAbout(*previous, camera->Direction(), axis.Direction());
      CHECK(turned.has_value());
      if (turned && nearly(*turned, 10.0, 1e-6)) {
        ++steps;
      } else if (turned) {
        std::fprintf(stderr, "turntable.cpp: frame %d turned %.9f degrees\n", frame + 1, *turned);
      }
    }
    previous = camera->Direction();
  }
  CHECK(steps == frames - 1);
  // The loop closes: the first frame follows the last by one step.
  const std::optional<double> back = mitcad::turnAbout(*previous, start->Direction(), axis.Direction());
  CHECK(back.has_value() && nearly(*back, 10.0, 1e-6));
  CHECK(nearly(start->Eye().Distance(start->Center()), distance, 1e-9)); // start unchanged
}

void cameraTests() {
  const gp_Pnt centre(10.0, 20.0, 5.0);
  fullTurn(false, gp_Ax1(centre, gp_Dir(0.0, 0.0, 1.0)), centre);
  fullTurn(true, gp_Ax1(centre, gp_Dir(0.0, 0.0, 1.0)), centre);
  // An axis off the target and tilted (a selected axis).
  fullTurn(false, gp_Ax1(gp_Pnt(-30.0, 0.0, 0.0), gp_Dir(1.0, 0.0, 1.0)), centre);
  // A positive angle turns counterclockwise seen from the axis's tip: the
  // eye east of the axis goes north.
  const occ::handle<Graphic3d_Camera> east = new Graphic3d_Camera();
  east->SetEyeAndCenter(gp_Pnt(100.0, 0.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
  east->SetUp(gp_Dir(0.0, 0.0, 1.0));
  const occ::handle<Graphic3d_Camera> north =
      mitcad::turntableCamera(east, gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), 90.0);
  CHECK(north->Eye().Distance(gp_Pnt(0.0, 100.0, 0.0)) < 1e-9);
  // No turn: the same camera values.
  const occ::handle<Graphic3d_Camera> same =
      mitcad::turntableCamera(east, gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), 0.0);
  CHECK(same->Eye().Distance(east->Eye()) < 1e-12);
}

void stepTests() {
  CHECK(nearly(mitcad::turntableStep(360.0, 36), 10.0));
  CHECK(nearly(mitcad::turntableStep(-360.0, 36), -10.0));
  CHECK(nearly(mitcad::turntableStep(720.0, 36), 20.0));
  // Not a whole turn: the last frame shows the end of the turn.
  CHECK(nearly(mitcad::turntableStep(90.0, 10), 10.0));
  CHECK(nearly(mitcad::turntableStep(180.0, 2), 180.0));
  CHECK(nearly(mitcad::turntableStep(360.0, 1), 0.0));
  // The turn about an axis, in (-180, 180].
  const gp_Dir z(0.0, 0.0, 1.0);
  CHECK(!mitcad::turnAbout(z, gp_Dir(1.0, 0.0, 0.0), z).has_value());
  const std::optional<double> quarter = mitcad::turnAbout(gp_Dir(1.0, 0.0, -1.0), gp_Dir(0.0, 1.0, -1.0), z);
  CHECK(quarter && nearly(*quarter, 90.0, 1e-9));
  const std::optional<double> back = mitcad::turnAbout(gp_Dir(1.0, 0.0, 0.0), gp_Dir(-1.0, -0.001, 0.0), z);
  CHECK(back && *back < -179.0);
}

void nameAndSizeTests() {
  CHECK(mitcad::turntableFrameName(1) == QStringLiteral("frame-0001.png"));
  CHECK(mitcad::turntableFrameName(36) == QStringLiteral("frame-0036.png"));
  CHECK(QString::fromLatin1(mitcad::kTurntableFramePattern) == QStringLiteral("frame-%04d.png"));
  CHECK(mitcad::turntableFrameSize(QSize(641, 481), TurntableOutput::Mp4) == QSize(640, 480));
  CHECK(mitcad::turntableFrameSize(QSize(641, 481), TurntableOutput::WebMFrames) == QSize(640, 480));
  CHECK(mitcad::turntableFrameSize(QSize(641, 481), TurntableOutput::Frames) == QSize(641, 481));
  CHECK(mitcad::turntableFrameSize(QSize(1, 1), TurntableOutput::Mp4) == QSize(2, 2));
  for (const TurntableOutput output : {TurntableOutput::Mp4, TurntableOutput::Mp4Frames, TurntableOutput::WebM,
                                       TurntableOutput::WebMFrames, TurntableOutput::Frames}) {
    CHECK(mitcad::turntableOutputOf(mitcad::turntableOutputKey(output)) == output);
  }
  CHECK(!mitcad::turntableOutputOf(QStringLiteral("gif")).has_value());
  CHECK(mitcad::turntableExtension(TurntableOutput::Mp4Frames) == QStringLiteral("mp4"));
  CHECK(mitcad::turntableExtension(TurntableOutput::WebM) == QStringLiteral("webm"));
  CHECK(mitcad::turntableExtension(TurntableOutput::Frames).isEmpty());
  CHECK(mitcad::turntableKeepsFrames(TurntableOutput::Mp4Frames) && !mitcad::turntableKeepsFrames(TurntableOutput::Mp4));
  CHECK(!mitcad::turntableHasVideo(TurntableOutput::Frames));
}

void ffmpegTests() {
  const QString listing = QStringLiteral("Encoders:\n"
                                         " V..... = Video\n"
                                         " A..... = Audio\n"
                                         " ------\n"
                                         " V....D mpeg4                MPEG-4 part 2\n"
                                         " V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC (codec h264)\n"
                                         " V....D libvpx               libvpx VP8 (codec vp8)\n"
                                         " V....D libvpx-vp9           libvpx VP9 (codec vp9)\n"
                                         " A....D aac                  AAC (Advanced Audio Coding)\n");
  CHECK(mitcad::turntableEncoder(TurntableOutput::Mp4, listing) == QStringLiteral("libx264"));
  CHECK(mitcad::turntableEncoder(TurntableOutput::WebMFrames, listing) == QStringLiteral("libvpx-vp9"));
  const QString plain = QStringLiteral(" V....D mpeg4                MPEG-4 part 2\n"
                                       " A....D libx264              not a video encoder\n");
  CHECK(mitcad::turntableEncoder(TurntableOutput::Mp4Frames, plain) == QStringLiteral("mpeg4"));
  CHECK(mitcad::turntableEncoder(TurntableOutput::WebM, plain).isEmpty());
  CHECK(mitcad::turntableEncoder(TurntableOutput::Mp4, QString()).isEmpty());

  const QStringList mp4 = mitcad::turntableFfmpegArguments(TurntableOutput::Mp4, QStringLiteral("libx264"), 24.0,
                                                           QSize(320, 240), 36, QStringLiteral("/work/turn.mp4"));
  const auto after = [](const QStringList& arguments, const QString& option) {
    const qsizetype at = arguments.indexOf(option);
    return at >= 0 && at + 1 < arguments.size() ? arguments.at(at + 1) : QString();
  };
  CHECK(after(mp4, QStringLiteral("-framerate")) == QStringLiteral("24"));
  CHECK(after(mp4, QStringLiteral("-start_number")) == QStringLiteral("1"));
  CHECK(after(mp4, QStringLiteral("-i")) == QStringLiteral("frame-%04d.png"));
  CHECK(after(mp4, QStringLiteral("-frames:v")) == QStringLiteral("36"));
  CHECK(after(mp4, QStringLiteral("-c:v")) == QStringLiteral("libx264"));
  CHECK(after(mp4, QStringLiteral("-pix_fmt")) == QStringLiteral("yuv420p"));
  CHECK(after(mp4, QStringLiteral("-f")) == QStringLiteral("mp4"));
  CHECK(mp4.contains(QStringLiteral("-y")) && mp4.contains(QStringLiteral("-nostdin")));
  CHECK(mp4.last() == QStringLiteral("file:/work/turn.mp4"));
  const QStringList webm = mitcad::turntableFfmpegArguments(TurntableOutput::WebM, QStringLiteral("libvpx-vp9"), 29.97,
                                                            QSize(1920, 1080), 72, QStringLiteral("/work/turn.webm"));
  CHECK(after(webm, QStringLiteral("-framerate")) == QStringLiteral("29.97"));
  CHECK(after(webm, QStringLiteral("-f")) == QStringLiteral("webm"));
  CHECK(!webm.contains(QStringLiteral("-movflags")));
  CHECK(webm.last() == QStringLiteral("file:/work/turn.webm"));
  // An encoder that takes a bit rate gets one.
  const QStringList openh264 = mitcad::turntableFfmpegArguments(
      TurntableOutput::Mp4, QStringLiteral("libopenh264"), 30.0, QSize(1280, 720), 10, QStringLiteral("/a.mp4"));
  CHECK(after(openh264, QStringLiteral("-b:v")).endsWith(QLatin1Char('k')));
}

// ---------------------------------------------------------------------------
// Synthetic containers.

QByteArray bigEndian(std::uint64_t value, int bytes) {
  QByteArray out;
  for (int i = bytes - 1; i >= 0; --i) {
    out.append(static_cast<char>((value >> (8 * i)) & 0xFF));
  }
  return out;
}

QByteArray box(const char* type, const QByteArray& payload) {
  return bigEndian(static_cast<std::uint64_t>(payload.size()) + 8, 4) + QByteArray(type, 4) + payload;
}

// A box of a 64-bit size (its 32-bit size 1).
QByteArray largeBox(const char* type, const QByteArray& payload) {
  return bigEndian(1, 4) + QByteArray(type, 4) + bigEndian(static_cast<std::uint64_t>(payload.size()) + 16, 8) +
         payload;
}

QByteArray trackHeader(int version, int width, int height) {
  // Version and flags, the times, the track id, the duration (32 or 64
  // bits), reserved, layer, group, volume, reserved, the matrix.
  const int head = version == 1 ? 32 : 20;
  QByteArray body = bigEndian(static_cast<std::uint64_t>(version) << 24, 4) + QByteArray(head, '\0') +
                    QByteArray(16, '\0') + QByteArray(36, '\0');
  body += bigEndian(static_cast<std::uint64_t>(width) << 16, 4) + bigEndian(static_cast<std::uint64_t>(height) << 16, 4);
  return box("tkhd", body);
}

QByteArray track(const char* handler, int version, int width, int height, int samples) {
  const QByteArray hdlr = box("hdlr", bigEndian(0, 4) + bigEndian(0, 4) + QByteArray(handler, 4) + QByteArray(12, '\0'));
  const QByteArray stsz = box("stsz", bigEndian(0, 4) + bigEndian(0, 4) + bigEndian(static_cast<std::uint64_t>(samples), 4));
  const QByteArray stbl = box("stbl", box("stsd", QByteArray(8, '\0')) + stsz);
  return box("trak", trackHeader(version, width, height) +
                         box("mdia", box("mdhd", QByteArray(24, '\0')) + hdlr + box("minf", stbl)));
}

mitcad::TurntableVideo read(QByteArray bytes) {
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::ReadOnly);
  return mitcad::readTurntableVideo(buffer);
}

void mp4Tests() {
  const QByteArray ftyp = box("ftyp", QByteArray("isom") + bigEndian(512, 4) + QByteArray("isomiso2avc1mp41"));
  const QByteArray moov =
      box("moov", box("mvhd", QByteArray(100, '\0')) + track("soun", 0, 0, 0, 100) + track("vide", 0, 320, 240, 36));
  const mitcad::TurntableVideo video = read(ftyp + moov + box("mdat", QByteArray(64, 'x')));
  CHECK(video.ok);
  CHECK(video.frames == 36);
  CHECK(video.width == 320 && video.height == 240);
  // The movie after the media (no faststart), a 64-bit media box, a
  // version 1 track header.
  const QByteArray late = box("moov", track("vide", 1, 1920, 1080, 72));
  const mitcad::TurntableVideo other = read(ftyp + largeBox("mdat", QByteArray(40, 'y')) + late);
  CHECK(other.ok && other.frames == 72 && other.width == 1920 && other.height == 1080);
  // No video track; a box cut short.
  CHECK(!read(ftyp + box("moov", track("soun", 0, 0, 0, 10))).ok);
  const QByteArray whole = ftyp + moov;
  const mitcad::TurntableVideo cut = read(whole.left(whole.size() - 10));
  CHECK(!cut.ok && !cut.error.isEmpty());
  CHECK(!read(QByteArray("not a video at all")).ok);
}

// An EBML element: its id's bytes, an 8-byte size and the payload.
QByteArray element(std::uint32_t id, const QByteArray& payload) {
  const int idBytes = id > 0xFFFFFF ? 4 : id > 0xFFFF ? 3 : id > 0xFF ? 2 : 1;
  return bigEndian(id, idBytes) + bigEndian(0x0100000000000000ull | static_cast<std::uint64_t>(payload.size()), 8) +
         payload;
}

// A master element of unknown size (to the end of its parent), as a
// live stream writes its segment and clusters.
QByteArray openElement(std::uint32_t id) {
  const int idBytes = id > 0xFFFFFF ? 4 : 3;
  return bigEndian(id, idBytes) + bigEndian(0x01FFFFFFFFFFFFFFull, 8);
}

QByteArray simpleBlock(int track) {
  // The track number (a 1-byte vint), the timecode, the flags, the data.
  return element(0xA3, bigEndian(0x80u | static_cast<unsigned>(track), 1) + bigEndian(0, 2) + bigEndian(0x80, 1) +
                           QByteArray(5, 'f'));
}

void webmTests() {
  const QByteArray header = element(0x1A45DFA3, element(0x4282, QByteArray("webm")));
  const QByteArray videoTrack =
      element(0xAE, element(0xD7, bigEndian(1, 1)) + element(0x83, bigEndian(1, 1)) +
                        element(0xE0, element(0xB0, bigEndian(320, 2)) + element(0xBA, bigEndian(240, 2))));
  const QByteArray audioTrack = element(0xAE, element(0xD7, bigEndian(2, 1)) + element(0x83, bigEndian(2, 1)));
  const QByteArray tracks = element(0x1654AE6B, audioTrack + videoTrack);
  QByteArray first = element(0xE7, bigEndian(0, 1));
  for (int i = 0; i < 18; ++i) {
    first += simpleBlock(1) + simpleBlock(2);
  }
  QByteArray second = element(0xE7, bigEndian(600, 2));
  for (int i = 0; i < 17; ++i) {
    second += simpleBlock(1);
  }
  // One frame in a block group.
  second += element(0xA0, element(0xA1, bigEndian(0x81, 1) + bigEndian(0, 2) + bigEndian(0, 1) + QByteArray(3, 'g')));
  const QByteArray segment = openElement(0x18538067) + element(0x114D9B74, QByteArray(12, '\0')) + tracks +
                             element(0x1F43B675, first) + openElement(0x1F43B675) + second;
  const mitcad::TurntableVideo video = read(header + segment);
  CHECK(video.ok);
  CHECK(video.frames == 36);
  CHECK(video.width == 320 && video.height == 240);
  // An element cut short.
  const QByteArray whole = header + segment;
  CHECK(!read(whole.left(whole.size() - 3)).ok);
  // No video track.
  CHECK(!read(header + element(0x18538067, element(0x1654AE6B, audioTrack))).ok);
}

} // namespace

int turntableTests() {
  failures = 0;
  cameraTests();
  stepTests();
  nameAndSizeTests();
  ffmpegTests();
  mp4Tests();
  webmTests();
  if (failures == 0) {
    std::printf("turntable: all checks passed\n");
  }
  return failures;
}

MITCAD_UNIT_TEST(turntableTests);
