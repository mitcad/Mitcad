// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QHash>
#include <QPointF>
#include <QRectF>
#include <QString>
#include <QWidget>

#include <gp_Trsf.hxx>

#include "../sketch/SketchModel.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "EventFilterGuard.hpp"

namespace mitcad {

class CommandHost;
class OcctViewer;

// The parameters a click in the view puts into a feature panel's value
// field while that field has the keyboard (mitcad#250): the dimensions of
// the sketches the view shows, drawn as sketch mode draws them, their
// names and values in their boxes (only those: mitcad#587 took out the
// labels of every earlier feature's values, which filled a large design's
// view). A left click on a label names its parameter (picked); the view
// gets no click then. Drawn with QPainter in view pixels over the 3D view;
// the mouse goes through to the view except on a label.
class ParameterPicker : public QWidget {
  Q_OBJECT

public:
  // A sketch the view shows, as the `sketch` query gives it, placed by the
  // occurrence that places it.
  struct Sketch {
    sketch::SketchModel model;
    gp_Trsf placement;
  };
  ParameterPicker(OcctViewer& viewer, const QString& command);
  ~ParameterPicker() override;

  // What it shows; `texts` are the parameters' values in their units, by
  // name ("40 mm"), as the `parameters` query gives them.
  void setContent(std::vector<Sketch> sketches, QHash<QString, QString> texts);
  bool isEmpty() const { return m_sketches.empty(); }

signals:
  // A label was clicked: its parameter, and what it stands for ("Sketch1
  // k5").
  void picked(const QString& name, const QString& source);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Label {
    QString name;
    QString source;
    QString text;
    QRectF rect;
    bool driven = false;
    sketch::DimensionShape shape; // a sketch dimension's lines and arrows
  };

  // The labels as the view shows them now, in the order they are drawn.
  std::vector<Label> layout() const;
  int labelAt(const QPointF& position) const;
  // "Parameter label <source> <name> at x,y": where the labels are for the
  // camera now (logged as it changes, also where nothing is painted, as in
  // a Windows test session); "... drawn at x,y": where a paint put them
  // (mitcad#1179: the drawn labels follow the camera).
  void logPlaces();
  void logDrawn(const std::vector<Label>& labels);

  OcctViewer& m_viewer;
  EventFilterGuard m_viewerFilter{this};
  QString m_command;
  std::vector<Sketch> m_sketches;
  QHash<QString, QString> m_texts;
  int m_hover = -1;
  bool m_pressed = false; // a press on a label: its release is not the view's
  QString m_logged;
  QString m_loggedDrawn;
};

// Gathers what a picker shows from the model: the dimensions with
// parameters of the sketches the view shows (CommandHost::shownSketches);
// `texts` gets every parameter's value in its unit.
void gatherParameterLabels(const CommandHost& host, std::vector<ParameterPicker::Sketch>& sketches,
                           QHash<QString, QString>& texts);

} // namespace mitcad
