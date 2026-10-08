// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <functional>

#include <QDialog>
#include <QJsonArray>
#include <QPointer>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QVector>

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QTabWidget;
class QToolButton;
class QWidget;

namespace mitcad {

class DocumentHost;
class OcctViewer;
namespace render {
class RenderLightsOverlay;
}

// Render Environment (mitcad#47, View menu): the document's render settings
// (core/model/src/api/commands.md, "Render settings"): the environment (a
// studio, outdoor or an HDR image, its strength and rotation, the sun),
// the background, the ground, the film (exposure, view transform) and the
// user's lights (mitcad#54: added, chosen, placed by numbers, at a face
// picked in the view or at the camera; shown as glyphs over the view while
// the dialog is open). Each
// change is a model command and an undo step, and the rendered view
// renders again (or, for the film and the background colour, shows its
// last frame anew). Not modal: it follows undo and redo.
class RenderEnvironmentDialog : public QDialog {
  Q_OBJECT

public:
  // `showRendered` turns View > Rendered on or off.
  RenderEnvironmentDialog(DocumentHost& host, OcctViewer& viewer, std::function<void(bool)> showRendered,
                          QWidget* parent = nullptr);

  // Shows the settings as the model has them now.
  void refresh();
  // Whether View > Rendered is on (the dialog's check box follows it).
  void setRenderedShown(bool shown);

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
  void reset();
  void updateEnabled();
  void logLayout();

  // Lights (mitcad#54).
  QWidget* buildLights();
  QJsonObject chosenLight() const;
  void refreshLights();
  void addLight();
  void deleteLight();
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
  std::function<void(bool)> m_showRendered;
  QJsonObject m_settings;
  QVector<Number> m_numbers;
  QVector<Choice> m_choices;
  QVector<std::pair<QString, QCheckBox*>> m_checks;
  QCheckBox* m_rendered = nullptr;
  QTabWidget* m_pages = nullptr; // the environment, the lights
  QLineEdit* m_image = nullptr;
  QPushButton* m_browse = nullptr;
  QToolButton* m_colorButton = nullptr;
  QLineEdit* m_color = nullptr;
  QCheckBox* m_lowest = nullptr; // the ground under the lowest body
  QLabel* m_message = nullptr;
  QPushButton* m_reset = nullptr;
  QPushButton* m_close = nullptr;
  bool m_building = false;
  bool m_placed = false;
  QString m_logged; // the logged places

  // Lights: the list, the chosen one's fields and the glyphs.
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
  QPointer<render::RenderLightsOverlay> m_overlay;
};

} // namespace mitcad
