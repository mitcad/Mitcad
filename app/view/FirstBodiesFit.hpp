// SPDX-License-Identifier: MIT
#pragma once

// When the view fits the bodies that come into it (mitcad#467): the first
// bodies of a design, once per design, so that a newly made design's first
// body (a new design starts empty) and a design opened are in sight. Only
// the design's own bodies count, not the new bodies a command previews: the
// view would move under the items still to be picked.
//
// Never again for the same design: the user hiding every body and showing
// one again (the light bulbs, an isolation left) had the view fit that one
// body alone, and the bodies shown after it were out of sight (found in the
// bundle check of mitcad#529/#530, tools/ui-browser-test.sh's quick light
// bulbs). The view only gets the shown bodies, so it cannot tell a hidden
// body from none; the window arms the fit when it installs a design.
//
// Plain C++, for app.unit.

namespace mitcad {

class FirstBodiesFit {
public:
  // A design is installed: its first bodies are fitted.
  void arm() { m_armed = true; }
  bool armed() const { return m_armed; }
  // The design opened at its saved home view (mitcad#2671): its first
  // bodies are not fitted, also when they come into the view later.
  void disarm() { m_armed = false; }

  // The view's bodies were replaced: `wasEmpty` it showed none before,
  // `designBodies` it shows a design's own bodies now (not only a command's
  // previews). True when they are to be fitted; the first design bodies
  // shown end the design's first fit, fitted or not (an opened design is
  // fitted by the window itself when the view had the last design's).
  bool shown(bool wasEmpty, bool designBodies) {
    if (!designBodies) {
      return false;
    }
    const bool fit = wasEmpty && m_armed;
    m_armed = false;
    return fit;
  }

private:
  bool m_armed = true;
};

} // namespace mitcad
