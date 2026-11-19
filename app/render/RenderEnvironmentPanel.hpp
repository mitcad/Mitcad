// SPDX-License-Identifier: MIT
#pragma once

#include <array>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QPointer>
#include <QString>
#include <QVector>
#include <QWidget>

#include "framework/EventFilterGuard.hpp"

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QTabWidget;
class QToolButton;

namespace mitcad {

class DocumentHost;
class OcctViewer;
namespace render {
class RenderLightsOverlay;
}

// The document's render settings as a panel (mitcad#47, mitcad#54;
// core/model-api/src/commands.md, "Render settings" and "Render lights"):
// the environment (a studio, outdoor or an HDR image, its strength and
// rotation, the sun), the background, the ground, the film (exposure, view
// transform) on one page, the user's lights (added, chosen, placed by
// numbers, at a face picked in the view or at the camera; shown as glyphs
// over the view while the panel shows) on another. Each change is a model
// command and an undo step, and the rendered view renders again (or, for
// the film and the background colour, shows its last frame anew).
//
// Render Environment (View menu, RenderEnvironmentDialog) shows it whole,
// with both pages as tabs; the Render workspace's Light stage (mitcad#2013,
// RenderWorkspace) shows one page at a time in its settings panel, chosen
// in its Scene panel, which lists the lights instead of the page's own
// list (setLightListShown). The places of its fields are logged as
// "Render settings field <section.field> at x,y" and "Render light field
// <field> at x,y", in the main window's coordinates.
class RenderEnvironmentPanel : public QWidget {
  Q_OBJECT

public:
  // All: both pages as tabs; Environment or Lights: that page alone.
  enum class Part { All, Environment, Lights };

  RenderEnvironmentPanel(DocumentHost& host, OcctViewer& viewer, QWidget* parent = nullptr);

  // Shows the settings as the model has them now.
  void refresh();
  void setPart(Part part);
  Part part() const { return m_part; }
  // The lights' own list, its Delete and the Add row (the Scene panel
  // lists the lights instead in the Render workspace).
  void setLightListShown(bool shown);
  // The light whose settings the Lights page shows; the first one when
  // `id` is no light.
  void chooseLight(const QString& id);
  QString chosenLightId() const { return m_lightId; }
  // The settings as the model had them at the last refresh.
  const QJsonObject& settings() const { return m_settings; }
  // A message under the fields: the last refusal, or a hint while aiming.
  QLabel* message() const { return m_message; }

public slots:
  // The default settings again (one undo step; the shots stay).
  void reset();
  // Adds a light of `type` (point, spot, area, sun; the kind chosen in the
  // page's Add row when empty) at the camera, shining where it looks; it
  // becomes the chosen one.
  void addLight(const QString& type = QString());
  void deleteLight();
  // Turns the light `id` on or off (a model command, an undo step).
  void setLightEnabled(const QString& id, bool on);

signals:
  // A model command of the panel ran (the settings changed).
  void changed();
  // Another light is chosen (also after Add).
  void lightChosen(const QString& id);

protected:
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  // The click on a face while a light is being aimed (aimAtFace).
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Number {
    QString key; // "section.field"
    QDoubleSpinBox* box;
    double factor; // the model's value times this is shown (degrees for radians)
  };
  struct Choice {
    QString key;
    QComboBox* box; // item data: the model's value
  };

  QDoubleSpinBox* addNumber(const QString& key, double low, double high, double step, int decimals,
                            const QString& suffix, double factor, const QString& tip);
  QComboBox* addChoice(const QString& key, const QVector<std::pair<QString, QString>>& items);
  QCheckBox* addCheck(const QString& key, const QString& text, const QString& tip);
  QJsonValue value(const QString& key) const;
  // Sets one field of the settings (a model command, an undo step).
  void change(const QString& key, const QJsonValue& value);
  void pickColor();
  void browseImage();
  void updateEnabled();
  void logLayout();

  // Lights (mitcad#54).
  QWidget* buildLights();
  QJsonObject chosenLight() const;
  void refreshLights();
  // Changes fields of the chosen light (a model command, an undo step).
  void changeLight(const QJsonObject& fields, const QString& what);
  void pickLightColor();
  // The next click in the view on a body's face places the light at a
  // distance along the face's normal, shining at the point.
  void aimAtFace();
  void stopAiming();
  void placeAtCamera();
  void setLightRelativeToCamera(bool camera);

  DocumentHost& m_host;
  OcctViewer& m_viewer;
  Part m_part = Part::All;
  QJsonObject m_settings;
  QVector<Number> m_numbers;
  QVector<Choice> m_choices;
  QVector<std::pair<QString, QCheckBox*>> m_checks;
  QTabWidget* m_pages = nullptr; // the environment, the lights
  QLineEdit* m_image = nullptr;
  QPushButton* m_browse = nullptr;
  QToolButton* m_colorButton = nullptr;
  QLineEdit* m_color = nullptr;
  QCheckBox* m_lowest = nullptr; // the ground under the lowest body
  QLabel* m_message = nullptr;
  bool m_building = false;
  QString m_logged; // the logged places

  // Lights: the list, the chosen one's fields and the glyphs.
  QWidget* m_lightListRows = nullptr; // the list and the Add row
  QComboBox* m_lightList = nullptr;
  QComboBox* m_newLightKind = nullptr;
  QPushButton* m_addLight = nullptr;
  QPushButton* m_deleteLight = nullptr;
  QWidget* m_lightEditor = nullptr;
  QLineEdit* m_lightName = nullptr;
  QComboBox* m_lightType = nullptr;
  QCheckBox* m_lightOn = nullptr;
  QCheckBox* m_lightCamera = nullptr;
  std::array<QDoubleSpinBox*, 3> m_lightPosition{};
  std::array<QDoubleSpinBox*, 3> m_lightDirection{};
  QToolButton* m_lightColorButton = nullptr;
  QLineEdit* m_lightColor = nullptr;
  QDoubleSpinBox* m_lightPower = nullptr;
  QDoubleSpinBox* m_lightSize = nullptr;
  QDoubleSpinBox* m_lightSizeY = nullptr;
  QComboBox* m_lightShape = nullptr;
  QDoubleSpinBox* m_spotAngle = nullptr;
  QDoubleSpinBox* m_spotBlend = nullptr;
  QDoubleSpinBox* m_sunAngle = nullptr;
  QDoubleSpinBox* m_aimDistance = nullptr;
  QPushButton* m_aim = nullptr;
  QPushButton* m_fromCamera = nullptr;
  QString m_lightId; // the chosen light
  bool m_aiming = false;
  EventFilterGuard m_aimFilter{this}; // the view while aiming; it may go first (mitcad#226)
  QPointer<render::RenderLightsOverlay> m_overlay;
};

// A light's kind as the interface names it: Point, Spot, Area, Sun.
QString renderLightTypeName(const QString& type);

} // namespace mitcad
