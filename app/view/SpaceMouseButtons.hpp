// SPDX-License-Identifier: MIT
#pragma once

// A 3D mouse's labeled keys (mitcad#2642, GitHub mitcad/Mitcad#772): the
// QuickView keys of a SpaceMouse Enterprise, whose tap and hold run two
// opposite standard views (T/B: Top, held Bottom; F/BK: Front, held Back;
// R/L: Right, held Left), and the pure tap-or-hold decision the
// controller (SpaceMouseController) drives with its own clock, so that the
// unit tests check it from given times.
//
// The keys' numbers are the device's virtual key numbers, which its HID
// button report and the 3DxWare client's button mask both use (button 1
// the first, bit 0): Menu 1, Fit 2, Top 3, Left 4, Right 5, Front 6,
// Bottom 7, Back 8, ..., Rotation 27 (the rotation lock key, which the
// SpaceMouse Pro's numbering shows: SpaceMouseNavigation.hpp). A route
// that tells a hold from a tap itself sends the held view's own number
// (Bottom 7, Left 4, Back 8): those buttons run their view at once, as
// any other button does. A route that sends one press and one release for
// the key (the device's own report, a driver that leaves the key to the
// application) gives the key a tap and a hold: the tap's view runs at the
// release when the key was held less than kSpaceMouseHoldMs, the hold's
// view runs once when the key has been held that long, still down, and
// the release then runs nothing; the tap's view never runs first.

#include <cstdint>
#include <vector>

#include <QString>

namespace mitcad {

// How long a QuickView key must be held to run its hold view, in ms.
constexpr std::int64_t kSpaceMouseHoldMs = 500;

// The six standard views' commands (ViewController's kStandardViews).
inline bool isSpaceMouseStandardView(const QString& id) {
  return id == QLatin1String("view.top") || id == QLatin1String("view.bottom") ||
         id == QLatin1String("view.front") || id == QLatin1String("view.back") ||
         id == QLatin1String("view.right") || id == QLatin1String("view.left");
}

// The standard view opposite a standard view's command (Top and Bottom,
// Front and Back, Right and Left); empty for any other command.
inline QString spaceMouseOppositeView(const QString& id) {
  static const char* const pairs[][2] = {
      {"view.top", "view.bottom"}, {"view.front", "view.back"}, {"view.right", "view.left"}};
  for (const auto& pair : pairs) {
    if (id == QLatin1String(pair[0])) {
      return QString::fromLatin1(pair[1]);
    }
    if (id == QLatin1String(pair[1])) {
      return QString::fromLatin1(pair[0]);
    }
  }
  return {};
}

// Whether the HID ids are a SpaceMouse Enterprise's (vendor 046d or 256f,
// product c633).
inline bool isSpaceMouseEnterprise(int vendor, int product) {
  return (vendor == 0x046d || vendor == 0x256f) && product == 0xc633;
}

// The buttons (0 the first) of a device's QuickView keys that send one
// press and release for both of their views, as a bit mask: T/B (button
// 3), R/L (5) and F/BK (6) on a SpaceMouse Enterprise; none elsewhere.
inline std::uint32_t spaceMouseQuickViewKeys(int vendor, int product) {
  if (!isSpaceMouseEnterprise(vendor, product)) {
    return 0;
  }
  return (std::uint32_t(1) << 2) | (std::uint32_t(1) << 4) | (std::uint32_t(1) << 5);
}

// A button's view when its key is held (mitcad#2642): the standard view
// opposite its tap's when the button is one of the device's QuickView keys
// (`quickViewKeys`, spaceMouseQuickViewKeys) and its command is a standard
// view; empty otherwise (the button runs its command at the press, as
// every button did before). A user's other command for the key, or None,
// leaves it without a hold.
inline QString spaceMouseHoldCommand(std::uint32_t quickViewKeys, int button, const QString& tap) {
  if (button < 0 || button >= 32 || (quickViewKeys & (std::uint32_t(1) << button)) == 0) {
    return {};
  }
  return spaceMouseOppositeView(tap);
}

// The keys pressed with a tap and a hold, each waiting until its release
// or its hold time tells which (mitcad#2642). Times are a steady clock's
// milliseconds. A key's views run once per press: the hold's when due()
// or the release finds the key held kSpaceMouseHoldMs or longer, the
// tap's at a release before that. cancel() forgets every waiting key:
// the device went, the design lost the input, the application went to the
// background; a release that comes after it runs nothing.
class SpaceMouseKeyGestures {
public:
  struct Run {
    int button = -1;
    QString command;
    bool held = false;
  };

  // A key with a tap and a hold went down. A key still waiting (its
  // release lost) starts again.
  void press(int button, const QString& tap, const QString& hold, std::int64_t nowMs) {
    forget(button);
    m_waiting.push_back({button, tap, hold, nowMs, false});
  }

  // Whether `button` is a key waiting for its release (pressed with a tap
  // and a hold, not yet released).
  bool waiting(int button) const {
    for (const Key& key : m_waiting) {
      if (key.button == button) {
        return true;
      }
    }
    return false;
  }

  // The key went up: the tap's view when it was held less than the hold
  // time; nothing when the hold ran already, or when it was not waiting.
  // A release found past the hold time (its timer late) runs the hold.
  Run release(int button, std::int64_t nowMs) {
    Run run;
    for (auto at = m_waiting.begin(); at != m_waiting.end(); ++at) {
      if (at->button != button) {
        continue;
      }
      if (!at->held) {
        run.button = button;
        run.held = nowMs - at->pressedMs >= kSpaceMouseHoldMs;
        run.command = run.held ? at->hold : at->tap;
      }
      m_waiting.erase(at);
      break;
    }
    return run;
  }

  // The holds due at `nowMs`, each once; their keys wait for their
  // release, which then runs nothing.
  std::vector<Run> due(std::int64_t nowMs) {
    std::vector<Run> runs;
    for (Key& key : m_waiting) {
      if (!key.held && nowMs - key.pressedMs >= kSpaceMouseHoldMs) {
        key.held = true;
        runs.push_back({key.button, key.hold, true});
      }
    }
    return runs;
  }

  // The time the next hold is due, or -1 when none waits.
  std::int64_t nextDueMs() const {
    std::int64_t next = -1;
    for (const Key& key : m_waiting) {
      if (!key.held && (next < 0 || key.pressedMs + kSpaceMouseHoldMs < next)) {
        next = key.pressedMs + kSpaceMouseHoldMs;
      }
    }
    return next;
  }

  bool empty() const { return m_waiting.empty(); }
  void cancel() { m_waiting.clear(); }

private:
  struct Key {
    int button;
    QString tap;
    QString hold;
    std::int64_t pressedMs;
    bool held;
  };

  // Forgets a key without running anything.
  void forget(int button) {
    for (auto at = m_waiting.begin(); at != m_waiting.end(); ++at) {
      if (at->button == button) {
        m_waiting.erase(at);
        return;
      }
    }
  }

  std::vector<Key> m_waiting;
};

} // namespace mitcad
