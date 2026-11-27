// SPDX-License-Identifier: MIT
// Whether the widgets over the 3D view reach the screen (mitcad#2031,
// GitHub mitcad/Mitcad#434). On native Wayland the 3D view drew but the
// toolbar panel was not seen after the Start page, and that desktop lets no
// screenshot or synthetic input reach the window: the session log has to
// say it. The window composes the view (a QOpenGLWidget's texture) with the
// widgets painted over and around it and presents the frame; this watch
// logs both steps for the widgets it is given (the toolbar panel, the title
// row, the Start page, the Floating layout's rows and cards):
//
// - "Painted: toolbar panel at 12,48 1000x121, scale 2 (the window exposed,
//   platform wayland)": the first paint of the widget or of a widget in it
//   since the watch was armed, where in the window and at which device
//   pixel ratio;
// - "Frame presented with: toolbar panel, title row (platform wayland)": the
//   window composed and presented the first frame after those paints (the
//   view's QOpenGLWidget::frameSwapped, which Qt emits once the window's
//   frame with the view and the widgets is flushed);
// - "Window exposed: yes, 1280x800 at scale 2 on a 2560x1600 screen
//   (platform wayland)" when the window's exposure changes while armed;
// - "Paint check <reason>: painted and presented: …; not painted: …;
//   painted, not presented: …; shown but covered: … (the window exposed,
//   platform wayland, scale 2)": once every widget shown at the arming was
//   painted and presented, else after kCheckMs. "Shown but covered": shown
//   and not painted, with nothing of it unobscured (QWidget::visibleRegion:
//   other widgets over it, or outside the window); the widgets hidden
//   meanwhile are left out.
//
// Armed when the window first shows and when the Start page is left; each
// arming repaints the watched widgets that are shown, so each is painted
// again whether or not its content changed. While armed an application
// event filter sees the paints; otherwise nothing is watched.
#pragma once

#include <QObject>
#include <QPointer>
#include <QString>
#include <QTimer>

#include <vector>

class QOpenGLWidget;
class QWidget;
class QWindow;

namespace mitcad {

class PaintWatch : public QObject {
  Q_OBJECT

public:
  // How long an arming waits for the paints and the frame.
  static constexpr int kCheckMs = 5000;

  // `view`: the 3D view the window composes the widgets with; null where no
  // frame is presented through it (a headless test): a paint then counts as
  // presented.
  explicit PaintWatch(QOpenGLWidget* view, QObject* parent = nullptr);
  ~PaintWatch() override;
  PaintWatch(const PaintWatch&) = delete;
  PaintWatch& operator=(const PaintWatch&) = delete;

  // `widget` (and the widgets in it) logged as `name`; a widget given again
  // keeps its first name.
  void watch(QWidget* widget, const QString& name);
  // A new round (`reason`: "after the Start page was left"), once the
  // layouts queued before it have run.
  void arm(const QString& reason);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Entry {
    QPointer<QWidget> widget;
    QString name;
    bool expected = false; // shown at the arming, or painted since
    bool painted = false;
    bool presented = false;
  };

  void start(const QString& reason);
  void painted(QWidget* widget);
  void framePresented();
  void finishIfDone();
  void finish();
  void logExposure();
  QString windowText() const;
  QWindow* viewWindow() const;

  QPointer<QOpenGLWidget> m_view;
  std::vector<Entry> m_entries;
  QString m_reason;
  bool m_armed = false;
  bool m_filtering = false;
  // The exposure last logged, -1 none yet.
  int m_exposed = -1;
  QTimer m_check;
};

} // namespace mitcad
