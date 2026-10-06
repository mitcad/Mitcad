// SPDX-License-Identifier: MIT
#pragma once

// Sketch mode (U2): the sketch being edited, its drawing tools, snapping,
// the constraints and dimensions drawn over the view, dragging with the
// solver, and the values typed in the view. The model does the geometry
// (`sketch.*` commands); this class turns mouse and keyboard input into
// those commands, one undo step per user action.

#include <functional>
#include <memory>
#include <optional>

#include <QHash>
#include <QJsonObject>
#include <QMouseEvent>
#include <QObject>
#include <QPointF>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QVector>

#include "../framework/CommandSession.hpp"
#include "SketchModel.hpp"
#include "SketchSnap.hpp"

class QLineEdit;

namespace mitcad {

class OcctViewer;

// What sketch mode needs from the main window besides a command host.
class SketchHost : public CommandHost {
public:
  virtual const Selection& selection() const = 0;
  virtual void setSelection(const Selection& items) = 0;
  // A click on items, with the selection rules (Ctrl and Shift add or
  // remove).
  virtual void pick(const Selection& items, Qt::KeyboardModifiers modifiers) = 0;
  virtual void showError(const QString& message) = 0;
  // Starts a command of the registry (an edit panel opened by a
  // double-click).
  virtual void trigger(const QString& id) = 0;
};

namespace sketch {

class SketchOverlay;
class SketchTool;

// A value typed in the view while drawing (heads-up input).
struct FieldSpec {
  enum class Kind { Length, Angle, Count, Text };
  QString id;
  QString label;
  Kind kind = Kind::Length;
};

// Entities that a picking tool or a drag accepts.
enum EntityFilter : unsigned {
  PickPoints = 1u << 0,
  PickCurves = 1u << 1,
  PickAll = PickPoints | PickCurves,
};

class SketchController;

// One user action: several model commands that undo as one step, with
// the constraints that keep new points where they snapped.
class SketchOp {
public:
  explicit SketchOp(SketchController& controller);
  ~SketchOp();
  SketchOp(const SketchOp&) = delete;
  SketchOp& operator=(const SketchOp&) = delete;

  // A `sketch.*` command of the edited sketch.
  QJsonObject command(const QString& name) const;
  // Runs a command; false, keeping the model's reason, when it is refused.
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  // A command that may be refused, such as an inferred constraint: the
  // refusal is only logged.
  bool attempt(const QJsonObject& command);
  // An existing point where the cursor snapped to one (made at the origin
  // when it snapped there), else the position.
  QJsonValue pointInput(const Snap& snap);
  // The constraints that keep `point` where it snapped.
  void constrain(const QString& point, const Snap& snap);
  void addConstraint(const QJsonObject& constraint);
  // A dimension with a typed value; refused ones are left out.
  void addDimension(const QJsonObject& dimension, const QString& value);
  // The sketch after the commands so far.
  const SketchModel& model();
  const QString& error() const { return m_error; }
  // Ends the action as one undo step and shows the result; or takes it
  // all back.
  bool commit();
  void rollback();

private:
  SketchController& m_c;
  int m_depth = 0;
  QString m_label;
  QString m_error;
  bool m_finished = false;
  bool m_stale = false;
};

class SketchController : public QObject {
  Q_OBJECT

public:
  SketchController(SketchHost& host, OcctViewer& viewer, QObject* parent = nullptr);
  ~SketchController() override;

  // --- Sketch mode
  void enter(const QString& uid);
  void leave();
  bool isActive() const { return !m_uid.isEmpty(); }
  const QString& uid() const { return m_uid; }
  const SketchModel& model() const { return m_model; }
  // Reads the sketch again after the model changed.
  void reload();
  // While a command's panel runs in the sketch, its picks are the view's:
  // no tools, drags or picking of constraints and dimensions.
  void setPaused(bool paused) { m_paused = paused; }
  // Turns the view to look at the sketch plane.
  void lookAt();
  // Logs where the sketch origin and axes are in the window, when that
  // changed ("Sketch view x,y x,y x,y" for (0, 0), (100, 0) and (0, 100)).
  void logView();

  // --- Tools
  void startTool(std::unique_ptr<SketchTool> tool);
  void stopTool();
  // The icon of the tool command named `toolName` (as its tool names
  // itself), which the tool's cursor shows (mitcad#3).
  void setToolIcon(const QString& toolName, const QString& icon) { m_toolIcons.insert(toolName, icon); }
  SketchTool* tool() const { return m_tool.get(); }
  // Enter and Esc; false when sketch mode had nothing to do with them.
  bool confirm();
  bool cancel();
  // Takes back what the tool has half done (before an undo); the tool
  // stays.
  void resetTool();
  // Commands on the selection.
  void deleteSelection();
  void toggleConstruction();
  void toggleCenterline();
  void toggleFixed();
  void editDimension(const QString& id);

  // --- Options of the sketch palette
  bool construction() const { return m_construction; }
  void setConstruction(bool on);
  bool gridSnap() const { return m_gridSnap; }
  void setGridSnap(bool on);
  // The grid's fixed spacing (mm), or 0: snap steps follow the zoom (U5).
  void setGridStep(double step) { m_gridStep = step; }
  bool showDimensions() const { return m_showDimensions; }
  void setShowDimensions(bool on);
  bool showConstraints() const { return m_showConstraints; }
  void setShowConstraints(bool on);
  bool showProfiles() const { return m_showProfiles; }
  void setShowProfiles(bool on);
  // The bodies shown cut at the sketch plane, without what is in front of
  // it (on its normal's side), so that the sketch is not hidden.
  bool hideAbove() const { return m_hideAbove; }
  void setHideAbove(bool on);

  // --- For tools and the overlay
  SketchHost& host() { return m_host; }
  const SketchHost& host() const { return m_host; }
  OcctViewer& viewer() { return m_viewer; }
  QPointF toScreen(const V2& p) const;
  // The sketch plane point under a view position.
  std::optional<V2> sketchAt(const QPointF& screen) const;
  // Millimetres per pixel near the sketch origin.
  double pixel() const;
  const Snap& snapped() const { return m_snap; }
  const QPointF& cursor() const { return m_cursor; }
  bool cursorInView() const { return m_cursorValid; }
  // The point or curve under a view position.
  QString entityAt(const QPointF& screen, unsigned filter = PickAll) const;
  // Highlights for picking tools.
  void setHover(const QString& entity);
  const QString& hover() const { return m_hover; }
  void setPicked(const QStringList& entities);
  const QStringList& picked() const { return m_picked; }
  QString hoverAnnotation() const { return m_hoverAnnotation; }
  // A dimension's text being dragged.
  std::optional<std::pair<QString, V2>> textOverride() const { return m_textOverride; }
  void refreshView();
  void hint(const QString& message);
  void error(const QString& message);

  // Values typed in the view.
  void setFields(const QVector<FieldSpec>& fields);
  void clearFields();
  bool typed(const QString& id) const;
  // The typed value (mm, radians, a count) when it is valid.
  std::optional<double> fieldValue(const QString& id) const;
  // The typed expression as the model takes it ("20" gives "20 mm").
  QString fieldExpression(const QString& id) const;
  QString fieldText(const QString& id) const;
  // Shows what the cursor gives, while nothing is typed.
  void setLive(const QString& id, double value);
  // A value editor at a view position (a dimension's value).
  void openValueEditor(const QPointF& at, const QString& text, FieldSpec::Kind kind,
                       std::function<void(const QString&)> accept);
  bool valueEditorOpen() const { return m_editor != nullptr; }

  // Evaluates an expression; the expression the model takes ("20 mm") and
  // its value, or the reason it is invalid.
  bool evaluate(const QString& text, FieldSpec::Kind kind, QString& expression, double& value,
                QString& problem) const;
  QString formatLength(double mm) const;
  QString formatAngle(double radians) const;
  const QString& lengthUnit() const { return m_unit; }
  // Millimetres per document length unit.
  double lengthScale() const { return m_unitScale; }

  // Model access for SketchOp: throws the model's error.
  QJsonObject runModel(const QJsonObject& command);
  int undoDepth() const;
  QString undoLabel() const;

signals:
  // The sketch or its status changed (the palette shows the DOF).
  void changed();
  // The option toggles changed.
  void optionsChanged();
  void toolChanged(const QString& name);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Field {
    FieldSpec spec;
    QPointer<QLineEdit> edit;
    bool typed = false;
    bool valid = false;
    double value = 0.0;
    QString expression;
  };

  bool viewerEvent(QEvent* event);
  bool fieldEvent(QLineEdit* edit, QEvent* event);
  void updateCursor(const QPointF& position);
  void placeFields();
  void fieldEdited(int index);
  void acceptEditor();
  void closeEditor();
  void selectAnnotation(const QString& kind, const QString& id, Qt::KeyboardModifiers modifiers);
  void startDrag();
  void applyDrag();
  void endDrag();
  // A modal dialog took the input mid-drag (a long computation's progress,
  // P7), and with it the release: the drag ends when the model is free.
  void interruptDrag();
  void forwardClick(const QMouseEvent* release);
  int fieldIndex(const QString& id) const;
  QString describeStatus() const;

  SketchHost& m_host;
  OcctViewer& m_viewer;
  QString m_uid;
  SketchModel m_model;
  QString m_unit = QStringLiteral("mm");
  double m_unitScale = 1.0; // mm per unit
  QString m_status;         // last logged status
  QString m_loggedView;     // last logged view placement
  SketchOverlay* m_overlay = nullptr;
  std::unique_ptr<SketchTool> m_tool;
  QHash<QString, QString> m_toolIcons; // by tool name

  bool m_paused = false;
  bool m_construction = false;
  bool m_gridSnap = true;
  double m_gridStep = 0.0;
  bool m_showDimensions = true;
  bool m_showConstraints = true;
  bool m_showProfiles = true;
  bool m_hideAbove = false;

  Snap m_snap;
  QPointF m_cursor;
  bool m_cursorValid = false;
  QString m_hover;
  QStringList m_picked;
  QString m_hoverAnnotation;

  // A press on geometry: a click (selection) or the start of a drag.
  bool m_pressed = false;
  bool m_dragging = false;
  bool m_forwarding = false;
  QPointF m_pressPosition;
  Qt::KeyboardModifiers m_pressModifiers;
  QString m_dragEntity;
  QString m_dragDimension; // a dimension's text
  V2 m_dragStart;
  V2 m_dragLast;
  V2 m_dragTarget;
  int m_dragDepth = 0;
  int m_dragSteps = 0;
  bool m_dragPending = false;
  std::optional<std::pair<QString, V2>> m_textOverride;
  QTimer m_dragTimer;

  QVector<Field> m_fields;
  QPointer<QLineEdit> m_editor;
  FieldSpec::Kind m_editorKind = FieldSpec::Kind::Length;
  std::function<void(const QString&)> m_editorAccept;
};

} // namespace sketch
} // namespace mitcad
