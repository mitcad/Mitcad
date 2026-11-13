// SPDX-License-Identifier: MIT
#pragma once

// 3D mice (6-DOF devices, mitcad#951): the small interface the view's
// SpaceMouseController reads them through, one backend per platform, and
// the pure parsing of the devices' reports, here so that the unit tests
// check it on every platform. No vendor SDK: Windows reads the device's
// HID reports through Raw Input (platform/windows/SpaceMouseWindows.cpp),
// Linux and the BSDs the spacenavd daemon's socket
// (platform/unix/SpaceMouseSpnav.cpp), macOS the HID values through
// IOKit's HID Manager (platform/macos/SpaceMouseMac.cpp).
//
// The axes come in the view's frame, normalized to about ±1 at full
// deflection: x to the right, y up and z toward the user (out of the
// screen), right-handed; the translations first (tx, ty, tz), then the
// rotations (rx, ry, rz) about the same axes, positive counter-clockwise
// seen from the axis' tip. Each backend maps its device's frame into this
// one; Preferences can reverse any axis.

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <utility>
#include <vector>

#include <QString>

namespace mitcad {

using SpaceMouseAxes = std::array<double, 6>;

enum class SpaceMouseAxis : int { Tx, Ty, Tz, Rx, Ry, Rz };

// The raw count of a full deflection: about 350 for the common devices
// (their reports reach a little beyond; the view's mapping clamps).
constexpr double kSpaceMouseFullScale = 350.0;

// One report of a 3D mouse's HID interface (usage page 1, usage 8, the
// multi-axis controller), the report id first: id 1 the translation (three
// 16-bit little-endian integers), id 2 the rotation, id 1 with 12 bytes
// both (newer devices), id 3 the buttons as a bit mask (bit 0 the first
// button, at most 32).
struct SpaceMouseHidReport {
  bool translation = false;
  bool rotation = false;
  bool buttons = false;
  std::array<int, 3> t{};
  std::array<int, 3> r{};
  std::uint32_t pressed = 0;
};

// `rotationInFirst` false: the device sends its rotation in a report 2 of
// its own, and bytes after a report 1's translation are padding, never a
// rotation (SpaceMouseHidReader, mitcad#1034).
inline bool parseSpaceMouseHidReport(const std::uint8_t* data, std::size_t size, SpaceMouseHidReport& report,
                                     bool rotationInFirst = true) {
  report = SpaceMouseHidReport();
  if (data == nullptr || size < 2) {
    return false;
  }
  const auto int16At = [data](std::size_t offset) {
    const auto bits = static_cast<std::uint16_t>(data[offset] | (data[offset + 1] << 8));
    return static_cast<int>(static_cast<std::int16_t>(bits));
  };
  switch (data[0]) {
  case 1:
    if (size >= 7) {
      report.translation = true;
      report.t = {int16At(1), int16At(3), int16At(5)};
    }
    if (rotationInFirst && size >= 13) {
      report.rotation = true;
      report.r = {int16At(7), int16At(9), int16At(11)};
    }
    return report.translation;
  case 2:
    if (size >= 7) {
      report.rotation = true;
      report.r = {int16At(1), int16At(3), int16At(5)};
    }
    return report.rotation;
  case 3:
    report.buttons = true;
    for (std::size_t byte = 1; byte < size && byte <= 4; ++byte) {
      report.pressed |= static_cast<std::uint32_t>(data[byte]) << (8 * (byte - 1));
    }
    return true;
  default:
    return false;
  }
}

// HID's axes in the view's frame. IOKit's HID values on macOS have the
// same axes. The translation: X to the right, Y toward the user, Z down, a
// right-handed frame. The rotation Rx, Ry, Rz is the cap's turn about those
// same axes by the right-hand rule (positive counter-clockwise seen from
// the axis' tip), so it goes into the view's frame exactly as the
// translation does (mitcad#1720). The cap's six motions, each read alone:
//   pushed right: X > 0          the view's tx > 0
//   lifted: Z < 0                ty > 0
//   pulled toward the user: Y > 0  tz > 0
//   tilted forward (its top away from the user): Rx < 0  rx < 0
//   twisted clockwise seen from above: Rz > 0  ry < 0
//   rolled right (its top to the right): Ry < 0  rz < 0
// and in Object mode the model does what the cap did (the unit test
// follows each motion from the device's values to the model).
// mitcad#1034 had reversed every rotation (-Rx, Rz, -Ry) to agree with
// spaceMouseFromSpnav as it then was, which had them reversed itself; a
// user then saw every turn go the wrong way (GitHub mitcad/Mitcad#294,
// mitcad#1720). spacenavd takes the same devices' HID
// values into its own frame by swapping Y and Z and reversing both,
// translation and rotation alike (DF_SWAPYZ | DF_INVYZ, for every USB
// device of the vendor), and spaceMouseFromSpnav below then gives the
// view's frame; going the two ways must give the same axes (the unit test
// checks it).
//   view tx = X, ty = -Z, tz = Y; view rx = Rx, ry = -Rz, rz = Ry.
inline SpaceMouseAxes spaceMouseFromHid(const std::array<int, 3>& t, const std::array<int, 3>& r) {
  const double scale = 1.0 / kSpaceMouseFullScale;
  return {t[0] * scale, -t[2] * scale, t[1] * scale, r[0] * scale, -r[2] * scale, r[1] * scale};
}

// Where a device sends its rotation (mitcad#1034): in report 1 after the
// translation (the newer devices' 13-byte report), in a report 2 of its
// own (the older devices, a Space Navigator among them), or not known.
enum class SpaceMouseRotationReport { Unknown, WithTranslation, OwnReport };

inline const char* spaceMouseRotationReportText(SpaceMouseRotationReport where) {
  switch (where) {
  case SpaceMouseRotationReport::WithTranslation:
    return "in report 1 with the translation";
  case SpaceMouseRotationReport::OwnReport:
    return "in report 2 of its own";
  case SpaceMouseRotationReport::Unknown:
    break;
  }
  return "not known";
}

// One device's reports read in turn, with the axes they leave (mitcad#1034).
// Windows's Raw Input gives every report of a device in a slot as long as
// the device's longest input report (RAWHID's dwSizeHid), the shorter ones
// padded. A device that sends its translation (report 1, 7 bytes) and its
// rotation (report 2) apart but declares a longer report (13 bytes) gave a
// report 1 whose padding, read as the newer devices' combined report, was
// a zero rotation between each two real ones: the rotation jumped between
// its value and none (GitHub mitcad/Mitcad#68). The reader takes the
// rotation from report 1 only when the device sends it there: as its
// report descriptor says (setRotationReport), or, when that is not known,
// until the device sends a report 2. Between reports it keeps the last
// translation and rotation.
class SpaceMouseHidReader {
public:
  void setRotationReport(SpaceMouseRotationReport where) { m_rotation = where; }
  SpaceMouseRotationReport rotationReport() const { return m_rotation; }

  // Reads one report as Raw Input gives it (`size` bytes, padding
  // included); false for a report it does not know (`report` cleared).
  bool read(const std::uint8_t* data, std::size_t size, SpaceMouseHidReport& report) {
    if (data != nullptr && size >= 1) {
      const int id = data[0];
      const auto at = std::lower_bound(m_ids.begin(), m_ids.end(), id);
      if (at == m_ids.end() || *at != id) {
        m_ids.insert(at, id);
      }
      if (id == 2 && m_rotation == SpaceMouseRotationReport::Unknown) {
        m_rotation = SpaceMouseRotationReport::OwnReport;
      }
    }
    if (!parseSpaceMouseHidReport(data, size, report, m_rotation != SpaceMouseRotationReport::OwnReport)) {
      return false;
    }
    if (report.translation) {
      m_t = report.t;
    }
    if (report.rotation) {
      m_r = report.r;
    }
    return true;
  }

  const std::array<int, 3>& translation() const { return m_t; }
  const std::array<int, 3>& rotation() const { return m_r; }
  SpaceMouseAxes axes() const { return spaceMouseFromHid(m_t, m_r); }
  // The report ids the device has sent, in order.
  const std::vector<int>& reportIds() const { return m_ids; }
  QString reportIdsText() const {
    QString text;
    for (const int id : m_ids) {
      if (!text.isEmpty()) {
        text += QStringLiteral(", ");
      }
      text += QString::number(id);
    }
    return text;
  }

private:
  SpaceMouseRotationReport m_rotation = SpaceMouseRotationReport::Unknown;
  std::array<int, 3> m_t{};
  std::array<int, 3> m_r{};
  std::vector<int> m_ids;
};

// The debug log's raw reports while a device moves (mitcad#1034), at most
// one line per interval: the last report of each id since the previous
// line in hexadecimal (padding included), how many reports came, and the
// translation and rotation read from them. For example "3D mouse report:
// id 1 [01 5e 01 00 00 00 00], id 2 [02 00 00 10 00 00 00]; 12 report(s);
// t 350 0 0, r 0 16 0".
class SpaceMouseRawLog {
public:
  static constexpr std::int64_t kIntervalMs = 100;

  void add(const std::uint8_t* data, std::size_t size) {
    if (data == nullptr || size == 0) {
      return;
    }
    ++m_count;
    const int id = data[0];
    for (auto& entry : m_last) {
      if (entry.first == id) {
        entry.second.assign(data, data + size);
        return;
      }
    }
    m_last.emplace_back(id, std::vector<std::uint8_t>(data, data + size));
  }

  // The line to log at `nowMs` (a steady clock's milliseconds): empty when
  // nothing came since the last line, or when that line is less than an
  // interval old and `now` is false (`now`: the cap came to rest).
  QString take(std::int64_t nowMs, const std::array<int, 3>& t, const std::array<int, 3>& r, bool now = false) {
    if (m_count == 0 || (!now && m_logged && nowMs - m_lastMs < kIntervalMs)) {
      return QString();
    }
    QString text = QStringLiteral("3D mouse report: ");
    for (std::size_t entry = 0; entry < m_last.size(); ++entry) {
      if (entry > 0) {
        text += QStringLiteral(", ");
      }
      text += QStringLiteral("id %1 [").arg(m_last[entry].first);
      const std::vector<std::uint8_t>& bytes = m_last[entry].second;
      for (std::size_t at = 0; at < bytes.size(); ++at) {
        if (at > 0) {
          text += QLatin1Char(' ');
        }
        text += QStringLiteral("%1").arg(static_cast<uint>(bytes[at]), 2, 16, QLatin1Char('0'));
      }
      text += QLatin1Char(']');
    }
    text += QStringLiteral("; %1 report(s); t %2 %3 %4, r %5 %6 %7")
                .arg(m_count)
                .arg(t[0])
                .arg(t[1])
                .arg(t[2])
                .arg(r[0])
                .arg(r[1])
                .arg(r[2]);
    m_last.clear();
    m_count = 0;
    m_lastMs = nowMs;
    m_logged = true;
    return text;
  }

private:
  std::vector<std::pair<int, std::vector<std::uint8_t>>> m_last;
  int m_count = 0;
  std::int64_t m_lastMs = 0;
  bool m_logged = false;
};

// One event of spacenavd's socket (its original protocol, which the daemon
// speaks to every client that asks for no other): eight 32-bit integers in
// the machine's order. data[0] is the kind (0 a motion, 1 a button press,
// 2 a release); a motion's axes follow (x, y, z, rx, ry, rz) and then its
// period in ms, a button event's number follows.
struct SpnavEvent {
  enum class Kind { Motion, Press, Release, Other };
  Kind kind = Kind::Other;
  std::array<int, 6> motion{};
  int button = -1;
};

inline SpnavEvent parseSpnavEvent(const std::array<std::int32_t, 8>& data) {
  SpnavEvent event;
  switch (data[0]) {
  case 0:
    event.kind = SpnavEvent::Kind::Motion;
    for (std::size_t axis = 0; axis < 6; ++axis) {
      event.motion[axis] = data[axis + 1];
    }
    break;
  case 1:
  case 2:
    event.kind = data[0] == 1 ? SpnavEvent::Kind::Press : SpnavEvent::Kind::Release;
    event.button = data[1];
    break;
  default:
    break;
  }
  return event;
}

// spacenavd's frame (left-handed: x to the right, y up, z into the
// screen, as libspnav documents it) in the view's. Its rotation is the
// HID rotation taken through the same swap and reversal as its
// translation (see spaceMouseFromHid), so it comes into the view's frame
// as the translation does: z reversed, rz reversed (mitcad#1720; before,
// rx and ry were reversed and rz kept, every turn the wrong way).
inline SpaceMouseAxes spaceMouseFromSpnav(const std::array<int, 6>& motion) {
  const double scale = 1.0 / kSpaceMouseFullScale;
  return {motion[0] * scale, motion[1] * scale, -motion[2] * scale,
          motion[3] * scale, motion[4] * scale, -motion[5] * scale};
}

// Calls `changed(button, pressed)` for each bit that differs between two
// button masks, the lowest first.
template <typename Changed>
void spaceMouseButtonChanges(std::uint32_t before, std::uint32_t after, Changed&& changed) {
  for (int bit = 0; bit < 32; ++bit) {
    const std::uint32_t mask = std::uint32_t(1) << bit;
    if (((before ^ after) & mask) != 0) {
      changed(bit, (after & mask) != 0);
    }
  }
}

// A platform's way to a 3D mouse. It runs on the GUI thread and reports
// through the callbacks; a backend that found nothing at start() keeps
// listening for a device (or the daemon) that comes later.
class SpaceMouseBackend {
public:
  virtual ~SpaceMouseBackend() = default;
  // Its name in the log: "Raw Input", "spacenavd", "IOKit HID".
  virtual QString name() const = 0;
  // Starts listening. True when a device (or spacenavd) is there now, with
  // its name in `device`.
  virtual bool start(QString& device) = 0;

  // The axes moved (the view's frame, see above).
  std::function<void(const SpaceMouseAxes&)> motion;
  // A button (0 the first) went down or up.
  std::function<void(int button, bool pressed)> button;
  // A device came or went after start().
  std::function<void(bool connected, const QString& device)> connection;
};

// This platform's backend (never null; it may find nothing).
std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend();

} // namespace mitcad
