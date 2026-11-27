// SPDX-License-Identifier: MIT
#pragma once

// Export Turntable (mitcad#2523, app/COMMANDS.md "Export Turntable"): the
// camera turned about an axis through the model, one frame per step, the
// frames saved as PNG images and made into an MP4 or WebM video by an
// FFmpeg program of the system (MitCad links no video codec). This file
// is what the export works out without the view: the cameras of the
// frames, the frames' names, the FFmpeg program, its encoder and its
// arguments, and a reading of the written video's container (its frame
// count and size). The unit tests are in tests/turntable.cpp.

#include <optional>

#include <QSize>
#include <QString>
#include <QStringList>

#include <Graphic3d_Camera.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>

class QIODevice;

namespace mitcad {

// What the export writes.
enum class TurntableOutput {
  Mp4,        // an MP4 video (H.264 where the FFmpeg program has it)
  Mp4Frames,  // the MP4 video and its frames as PNG images
  WebM,       // a WebM video (VP9 or VP8)
  WebMFrames, // the WebM video and its frames
  Frames,     // only the frames, as PNG images in a folder
};

bool turntableHasVideo(TurntableOutput output);
bool turntableKeepsFrames(TurntableOutput output);
// The video file's extension (mp4, webm); empty for frames only.
QString turntableExtension(TurntableOutput output);
// The output's key in the settings and the log (mp4, mp4+frames, webm,
// webm+frames, frames), and back (nullopt for an unknown key).
QString turntableOutputKey(TurntableOutput output);
std::optional<TurntableOutput> turntableOutputOf(const QString& key);

// The frame count's and the frame rate's limits.
constexpr int kTurntableMaxFrames = 3600;
constexpr double kTurntableMaxFps = 120.0;

// The turn between two frames, in degrees: a whole number of full turns
// is cut into `frames` equal steps, so that the video loops without the
// first picture twice (36 frames of 360 degrees step 10 degrees); any
// other angle ends on its last frame (`frames` - 1 steps). Zero for fewer
// than two frames.
double turntableStep(double angle, int frames);

// The camera of a frame: `start` turned by `degrees` about `axis` (a
// positive angle counterclockwise seen from the axis's tip), its eye,
// target and up turned together, its projection, zoom and aspect kept.
occ::handle<Graphic3d_Camera> turntableCamera(const occ::handle<Graphic3d_Camera>& start, const gp_Ax1& axis,
                                              double degrees);

// The angle in degrees, in (-180, 180], that turns `from` into `to` about
// `axis`, both seen in the plane square to the axis; nullopt when either
// lies along the axis.
std::optional<double> turnAbout(const gp_Dir& from, const gp_Dir& to, const gp_Dir& axis);

// A frame's file name, from 1: frame-0001.png. The FFmpeg program reads
// them by kTurntableFramePattern in their folder.
QString turntableFrameName(int index);
inline const char* const kTurntableFramePattern = "frame-%04d.png";

// The picture's size for the output: a video's sides even (its encoders
// keep the colours at half the resolution), at least 2 pixels.
QSize turntableFrameSize(const QSize& size, TurntableOutput output);

// The FFmpeg program: `chosen` when it is an executable file, else
// MITCAD_FFMPEG, else ffmpeg on the PATH, next to MitCad or in the
// package managers' folders (a macOS application gets no shell PATH).
// Empty when there is none.
QString findFfmpeg(const QString& chosen = QString());

// The encoder for the video among those the FFmpeg program lists
// (`ffmpeg -hide_banner -encoders`): MP4 libx264, libopenh264,
// h264_videotoolbox, h264_mf or else mpeg4; WebM libvpx-vp9, libvpx,
// libaom-av1 or libsvtav1. Empty when the program has none of them.
QString turntableEncoder(TurntableOutput output, const QString& encoderListing);

// The FFmpeg program's arguments: the first `frames` frames of its working
// folder (kTurntableFramePattern, from 1) at `fps`, encoded by `encoder`
// into `video` (an absolute path).
QStringList turntableFfmpegArguments(TurntableOutput output, const QString& encoder, double fps, const QSize& size,
                                     int frames, const QString& video);

// What a written video's container says: its video track's frame count
// and picture size.
struct TurntableVideo {
  bool ok = false;
  int frames = 0;
  int width = 0;
  int height = 0;
  QString error; // why it could not be read
};

// Reads an MP4 (ISO base media: moov/trak/tkhd, mdia/hdlr, stbl/stsz) or
// a WebM (Matroska: Tracks/TrackEntry/Video, Cluster/SimpleBlock and
// BlockGroup/Block) file by its first bytes.
TurntableVideo readTurntableVideo(QIODevice& file);
TurntableVideo readTurntableVideo(const QString& path);

} // namespace mitcad
