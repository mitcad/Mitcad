// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Appearances.hpp"
#include "../framework/Selection.hpp"

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QToolButton;

namespace mitcad {

class DocumentHost;

// Edit Appearances (mitcad#46): Mitcad's appearance library and the
// document's own appearances with a preview swatch and their physically
// based parameters (base colour, metalness, roughness, specular,
// transmission, IOR, coat, emission, opacity) and texture (mitcad#53: an
// image file for the base colour, or the image embedded in the design,
// with its size, rotation and projection). The library's are read-only;
// New makes a copy of the selected one in the document, whose parameters
// can then be edited. Each change is a model command and an undo step;
// Assign gives the bodies and faces the dialog was opened for the selected
// appearance. The faces of those bodies with appearances of their own are
// listed, and Clear gives the selected ones their body's again.
class AppearanceDialog : public QDialog {
  Q_OBJECT

public:
  AppearanceDialog(DocumentHost& host, QWidget* parent = nullptr);

  // The bodies and faces Assign gives the selected appearance (bodies and
  // faces of the selection); none disables it. Their own appearance is
  // selected when they share one.
  void setTargets(const Selection& targets);
  // Shows the appearances as the model has them now.
  void refresh();

private:
  struct NumberField {
    const char* key; // the model's parameter
    QDoubleSpinBox* box;
  };
  struct ColorField {
    const char* key;
    QToolButton* button; // shows the colour, opens a colour dialog
    QLineEdit* hex;      // #rrggbb
  };

  void addNumber(QFormLayout* form, const char* key, const QString& label, double low, double high,
                 double step, const QString& tip);
  void addColor(QFormLayout* form, const char* key, const QString& label);
  const Appearance* current() const;
  // Whether the model can change now; else says so.
  bool canChange();
  void select(const QString& id);
  void showCurrent();
  void change(const QString& key, const QJsonValue& value);
  void pickColor(const ColorField& field);
  void newAppearance();
  void deleteAppearance();
  void assign();
  // The texture (mitcad#53): a new image file (typed or chosen), a change
  // of the current one's fields, embedding it or not, removing it.
  void setTexturePath(const QString& path);
  void chooseTexture();
  void changeTexture(const QString& key, const QJsonValue& value);
  void embedTexture(bool embed);
  void sendTexture(const QJsonValue& texture, const QString& what);
  void showTexture(const Appearance* a, bool editable);
  // The faces of the targets' bodies with appearances of their own.
  void showFaces();
  void clearFaces();
  QStringList targetBodies() const;
  void logLayout();

  DocumentHost& m_host;
  QVector<Appearance> m_appearances;
  QString m_current;
  Selection m_targets;
  QListWidget* m_list = nullptr;
  QLabel* m_preview = nullptr;
  QLineEdit* m_name = nullptr;
  QVector<NumberField> m_numbers;
  QVector<ColorField> m_colors;
  QLabel* m_info = nullptr;
  QLineEdit* m_texturePath = nullptr;
  QPushButton* m_textureChoose = nullptr;
  QPushButton* m_textureRemove = nullptr;
  QDoubleSpinBox* m_textureWidth = nullptr;
  QDoubleSpinBox* m_textureHeight = nullptr;
  QDoubleSpinBox* m_textureRotation = nullptr; // degrees
  QComboBox* m_textureProjection = nullptr;
  QCheckBox* m_textureEmbed = nullptr;
  QLabel* m_textureInfo = nullptr;
  QListWidget* m_faces = nullptr;
  QPushButton* m_clearFaces = nullptr;
  QLabel* m_message = nullptr;
  QPushButton* m_new = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_assign = nullptr;
  QPushButton* m_close = nullptr;
  bool m_building = false;
  QString m_logged; // the logged places
};

} // namespace mitcad
