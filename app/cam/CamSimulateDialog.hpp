// SPDX-License-Identifier: MIT
#pragma once

// Simulate of the CNC group (mitcad#1013, app/COMMANDS.md "The CNC
// group"): a small window beside the 3D view that plays a setup's
// toolpaths back. Its slider runs over the tool's places along the moves
// (cam::PlaybackPoint), Play moves it on by itself; the main window draws
// the tool at the place and highlights its operation, and the stock as
// cut there; at the end the material left in words with the legend of the
// stock's shades.

#include <QColor>
#include <QDialog>
#include <QList>
#include <QString>
#include <QTimer>

class QLabel;
class QPushButton;
class QSlider;

namespace mitcad {

class CamSimulateDialog : public QDialog {
  Q_OBJECT

public:
  explicit CamSimulateDialog(QWidget* parent = nullptr);

  // A playback of `points` places, the slider at the first.
  void setPlayback(int points);
  int position() const;
  // What the place is, in words, under the slider ("Contour 1, move 12
  // of 40").
  void setPositionText(const QString& text);
  // The material left at the end in words under the slider, with the
  // legend of the stock's shades (`colors`: the stock, thin, thick and cut
  // into the part, the view's); an empty text hides both.
  void setRemaining(const QString& text, const QList<QColor>& colors);

signals:
  void positionChanged(int index);

protected:
  void hideEvent(QHideEvent* event) override;

private:
  void togglePlay();
  void step();

  QSlider* m_slider = nullptr;
  QPushButton* m_play = nullptr;
  QLabel* m_label = nullptr;
  QLabel* m_remaining = nullptr;
  QLabel* m_legend = nullptr;
  QTimer m_timer;
};

} // namespace mitcad
