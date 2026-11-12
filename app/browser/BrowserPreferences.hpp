// SPDX-License-Identifier: MIT
#pragma once

// The Browser's preferences (mitcad#1699).

#include <QObject>

namespace mitcad {

// Preferences > General > "Show used sketches under their features"
// (settings "browser/consumeSketches"), off by default: while it is on,
// the Browser lists each component's features in a Features folder, with
// the sketches each uses under it (a sketch several features use under each
// of them), and keeps only the sketches no feature uses in the Sketches
// folder (BrowserPanel::setConsumeSketches). Off, the Browser is as before.
bool browserConsumeSketches();
// Saves the choice and tells every window's Browser, which rebuilds its
// rows at once.
void setBrowserConsumeSketches(bool on);

// Tells the windows' Browsers that the preference changed.
class BrowserPreferencesNotifier : public QObject {
  Q_OBJECT

public:
  static BrowserPreferencesNotifier& instance();

signals:
  void consumeSketchesChanged(bool on);
};

} // namespace mitcad
