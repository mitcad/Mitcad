// SPDX-License-Identifier: MIT
#pragma once

// The Explode panel (mitcad#2113; commands.md, "Exploded views"): an
// explode view of the design with its steps in a list (Add Step, Edit Step
// and a double-click open Explode Step's panel; Remove, Up and Down change
// the list), Trail Lines for every step, Auto Explode with its factor, and
// a play bar: Play and Play Back run the steps in order from assembled to
// exploded and back, each in its own time, Previous and Next go a step,
// and the slider scrubs. The view shows the explode at the panel's time;
// closing the panel leaves the view shown exploded (the Browser's row
// shows it assembled again).

#include <functional>
#include <optional>

#include <QCoreApplication>
#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QElapsedTimer;
class QLabel;
class QPushButton;
class QSlider;
class QTimer;
class QTreeWidget;

namespace mitcad {

class CommandContext;

// What the panel needs of the window.
struct ExplodeViewHost {
  const CommandContext* model = nullptr;
  // A model command as an undo step; false when refused (the window says
  // why).
  std::function<bool(const QJsonObject& command, QJsonObject* result)> run;
  // The view shown at a time of its animation (none: every step done); an
  // empty name shows the design assembled.
  std::function<void(const QString& view, std::optional<double> time)> show;
  // Explode Step's panel for a step of a view (-1: a new step; an empty
  // view: a new view with it).
  std::function<void(const QString& view, int step)> editStep;
  // Whether a command can start now (no command, sketch or job).
  std::function<bool()> ready;
  std::function<void(const QString& message, bool error)> status;
};

class ExplodeViewDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::ExplodeViewDialog)

public:
  // `view` by name; empty: the first, else none yet (Auto Explode or Add
  // Step makes one).
  ExplodeViewDialog(ExplodeViewHost host, const QString& view, QWidget* parent);
  ~ExplodeViewDialog() override;

  QString view() const { return m_view; }
  // Shows another view of the document (the Browser's Edit).
  void showView(const QString& name);
  // The document changed (a step added, undo, another command): the views
  // read again.
  void refresh();

  // Esc and Close: the playback stops, the panel is deleted.
  void reject() override;

protected:
  void closeEvent(QCloseEvent* event) override;

private:
  void finish();
  bool m_finished = false;
  void readViews();
  void fillSteps();
  QJsonObject entry() const;
  double seconds() const;
  int selectedStep() const;
  // The view shown at `time` (seconds), the slider and label following.
  void goTo(double time);
  // Plays towards the end (+1) or the start (-1); 0 stops.
  void play(int direction);
  void frame();
  void autoExplode();
  void removeStep();
  void moveStep(int by);
  void setTrails(bool on);
  void stepTo(int by);
  bool run(const QJsonObject& command);

  ExplodeViewHost m_host;
  QString m_view;
  QJsonArray m_views; // the `explode_views` query's answer
  double m_time = 0.0;
  int m_direction = 0;
  bool m_updating = false;
  QTimer* m_timer = nullptr;
  QElapsedTimer* m_clock = nullptr;
  double m_clockStart = 0.0;
  QComboBox* m_viewsBox = nullptr;
  QTreeWidget* m_steps = nullptr;
  QPushButton* m_add = nullptr;
  QPushButton* m_edit = nullptr;
  QPushButton* m_remove = nullptr;
  QPushButton* m_up = nullptr;
  QPushButton* m_down = nullptr;
  QCheckBox* m_trails = nullptr;
  QDoubleSpinBox* m_factor = nullptr;
  QPushButton* m_auto = nullptr;
  QPushButton* m_play = nullptr;
  QPushButton* m_back = nullptr;
  QPushButton* m_previous = nullptr;
  QPushButton* m_next = nullptr;
  QSlider* m_slider = nullptr;
  QLabel* m_timeLabel = nullptr;
  QLabel* m_message = nullptr;
};

} // namespace mitcad
