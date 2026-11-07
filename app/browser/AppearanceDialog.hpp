// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <vector>

#include <QDialog>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Appearances.hpp"
#include "../framework/Selection.hpp"
#include "../materials/TextureSource.hpp"

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QFormLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QStackedWidget;
class QTimer;
class QToolButton;

namespace mitcad {

class DocumentHost;
class TextureBrowser;

// Edit Appearances (mitcad#46): Mitcad's appearance library and the
// document's own appearances with a preview swatch and their physically
// based parameters (base colour, metalness, roughness, specular,
// transmission, IOR, coat, emission, opacity) and texture (mitcad#53: an
// image file for the base colour, or the image embedded in the design,
// with its size, rotation and projection). The library's are read-only;
// New makes a copy of the selected one in the document, whose parameters
// can then be edited. Each change is a model command and an undo step.
// The window assigns nothing: the Appearance tool does (mitcad#1059), also
// to faces and to clear a face's own.
//
// The list is grouped by class (mitcad#1133): the design's own first, then
// the library's under Metal, General Plastic, FDM Plastic, Resin and Other
// (appearanceSections, shared with the pickers), each heading with its
// count; a click on a heading (or Space, Return, Left, Right on it)
// collapses or expands its group, remembered per user (the setting
// `appearances/collapsedGroups`). The search hides the groups without a
// match.
//
// Texture sources (mitcad#315): a selector above the list chooses between
// the appearances (the library and the design's) and the texture sources
// (app/materials/TextureSource.hpp), with a search field and a category
// filter for both. A source's assets show as a grid of thumbnails; Use
// fetches the chosen one's maps and makes them a new appearance of the
// design with the images embedded, which is then selected.
class AppearanceDialog : public QDialog {
  Q_OBJECT

public:
  AppearanceDialog(DocumentHost& host, QWidget* parent = nullptr);
  ~AppearanceDialog() override;

  // Shows the appearances as the model has them now.
  void refresh();
  // The appearance `id` selected and in sight, its group expanded
  // (mitcad#1415: Edit Appearances… of the Appearance tool's picker, on its
  // current appearance); logged `Appearances on <id>`. Nothing changes for
  // an id it does not list (Default, empty).
  void showAppearance(const QString& id);

  // Called once when the window closes next (mitcad#1415: the Appearance
  // tool reads its appearances again); taken before it is called. The
  // closing is logged `Appearances closed`.
  std::function<void()> whenClosed;

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
  // The texture (mitcad#53): a new image file (typed or chosen), a change
  // of the current one's fields, embedding it or not, removing it.
  void setTexturePath(const QString& path);
  void chooseTexture();
  void changeTexture(const QString& key, const QJsonValue& value);
  void embedTexture(bool embed);
  void sendTexture(const QJsonValue& texture, const QString& what);
  void showTexture(const Appearance* a, bool editable);
  void logLayout();
  // Texture sources (mitcad#315).
  void loadSources();
  TextureSource* currentSource() const;
  void sourceChosen();
  void showCategories();
  void runSearch();
  void filterList();
  // A group of the list (mitcad#1133) collapsed or expanded, remembered.
  void setGroupCollapsed(const QString& key, bool collapsed);
  bool eventFilter(QObject* watched, QEvent* event) override;
  void useTexture(const TextureAsset& asset, const QString& resolution);
  void finishUse(const QString& sourceId, const TextureAsset& asset, const TextureFiles& files,
                 const QString& error);
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  // The places are logged again when the dialog's size changes.
  void resizeEvent(QResizeEvent* event) override;

  DocumentHost& m_host;
  QVector<Appearance> m_appearances;
  QString m_current;
  QListWidget* m_list = nullptr;
  QLabel* m_preview = nullptr;
  QLineEdit* m_name = nullptr;
  QComboBox* m_class = nullptr; // what it is a look of (mitcad#1102)
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
  QLabel* m_message = nullptr;
  QPushButton* m_new = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_close = nullptr;
  QWidget* m_bodyView = nullptr;       // the scrolling body's viewport
  bool m_building = false;
  bool m_fitted = false; // sized to the screen at its first show (mitcad#818)
  QString m_logged; // the logged places
  QString m_loggedTexture; // the texture line last logged
  QStringList m_collapsed; // the list's collapsed groups by key (mitcad#1133)
  QString m_loggedGroups;  // the groups last logged
  // Texture sources (mitcad#315): the selector's entry 0 is the
  // appearances, entry i + 1 is m_sources[i].
  TextureSettings m_textureSettings;
  std::vector<std::unique_ptr<TextureSource>> m_sources;
  QComboBox* m_source = nullptr;
  QLineEdit* m_search = nullptr;
  QComboBox* m_category = nullptr;
  QStackedWidget* m_pages = nullptr;
  TextureBrowser* m_textures = nullptr;
  QTimer* m_searchTimer = nullptr;
  int m_shownSource = 0;
  bool m_using = false;
};

} // namespace mitcad
