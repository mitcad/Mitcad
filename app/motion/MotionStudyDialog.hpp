// SPDX-License-Identifier: MIT
#pragma once

// The Motion Study panel (mitcad#454; commands.md, "Motion studies and
// contact sets"): a study of the document with a step slider, its keys per
// joint motion, Add Key and Remove Key at the current step, and Play,
// Pause, Loop and the speed. Playing previews the study frame by frame
// along its steps (the model's `play_study` preview from the step before),
// so the joints are driven through the joint solver as Drive Joint drives
// them and stop at the first contact of a contact set; the document stays
// as it is until Keep, which plays the study to the shown step as one undo
// step.

#include <functional>

#include <QCoreApplication>
#include <QDialog>
#include <QJsonObject>
#include <QString>

class QCheckBox;
class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QSlider;
class QSpinBox;
class QTimer;
class QTreeWidget;

namespace mitcad {

class CommandContext;

// What the panel needs of the window.
struct MotionStudyHost {
  const CommandContext* model = nullptr;
  // A model command as an undo step; false when refused (the window says
  // why).
  std::function<bool(const QJsonObject& command, QJsonObject* result)> run;
  // A motion previewed (the model's `preview`) and shown in the view; the
  // answer, or throws with the reason.
  std::function<QJsonObject(const QJsonObject& request)> preview;
  // The preview gone: the design shown as it is.
  std::function<void()> endPreview;
  // Whether a frame can be computed now (no job, no command or sketch).
  std::function<bool()> ready;
  std::function<void(const QString& message, bool error)> status;
};

class MotionStudyDialog : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::MotionStudyDialog)

public:
  // `study` by name; empty: the first, else a new one.
  MotionStudyDialog(MotionStudyHost host, const QString& study, QWidget* parent);
  ~MotionStudyDialog() override;

  QString study() const { return m_study; }
  // Shows another study of the document (the browser's Play).
  void showStudy(const QString& name);
  // The document changed (undo, another command): the studies read again.
  void refresh();

  // Esc and Close: the preview goes, the panel is deleted.
  void reject() override;

protected:
  void closeEvent(QCloseEvent* event) override;

private:
  // Stops the playback, takes the preview away and deletes the panel once.
  void finish();
  bool m_finished = false;
  void readStudies();
  void fillKeys();
  void fillJoints();
  // The panel's step to `step`, previewed along the study from where it
  // shows now; false when a contact stopped it.
  bool goTo(double step);
  void play(bool on);
  void frame();
  void addKey();
  void removeKey();
  void keep();
  void stop(const QString& why);
  QString valuesText(const QJsonObject& values) const;
  QJsonObject entry() const;

  MotionStudyHost m_host;
  QString m_study;
  QJsonObject m_joints; // the `joints` query's answer
  double m_step = 0.0;  // the step shown (between steps when a contact stopped it)
  bool m_previewed = false;
  QString m_lastValues; // the values the last frame showed, for the log
  bool m_updating = false;
  QTimer* m_timer = nullptr;
  QComboBox* m_studies = nullptr;
  QSpinBox* m_steps = nullptr;
  QSlider* m_slider = nullptr;
  QLabel* m_stepLabel = nullptr;
  QPushButton* m_play = nullptr;
  QCheckBox* m_loop = nullptr;
  QComboBox* m_speed = nullptr;
  QTreeWidget* m_keys = nullptr;
  QComboBox* m_motion = nullptr;
  QLineEdit* m_value = nullptr;
  QLabel* m_message = nullptr;
};

} // namespace mitcad
