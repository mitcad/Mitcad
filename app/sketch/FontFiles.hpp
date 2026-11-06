// SPDX-License-Identifier: MIT
#pragma once

// Font files for sketch text (mitcad#473): the folders of fonts Preferences
// lists (its Fonts page), the fonts found in them, and the font chooser's
// previews. A text in a font file keeps the file's outlines in the design
// (the model's `font_file`, commands.md), so a design opens the same where
// the file is not; the folders only offer fonts to choose.

#include <QGroupBox>
#include <QImage>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

class QListWidget;
class QPushButton;

namespace mitcad::sketch {

// The font folders of Preferences (settings "fonts/folders"), absolute.
QStringList savedFontFolders();
void saveFontFolders(const QStringList& folders);

// The font files of a folder and its subfolders: .ttf, .otf, .ttc, .woff
// and .woff2, sorted.
QStringList fontFilesIn(const QString& folder);

// A font file of the folders: its path and its family (the file's name
// when the family cannot be read).
struct FolderFont {
  QString path;
  QString family;
};
QVector<FolderFont> folderFonts(const QStringList& folders);
// The fonts of the saved folders, found once until `forgetFolderFonts`
// (Edit Text looks again each time it starts).
const QVector<FolderFont>& savedFolderFonts();
void forgetFolderFonts();

// The family of a font file (loaded into the application's fonts once),
// empty when Qt cannot read it.
QString fontFileFamily(const QString& path);

// The installed font families, as the system has them before font files
// are loaded into the application (those are not installed for the model).
const QStringList& installedFontFamilies();

// Where a text's font file is now: its path when it is there, else a file
// of the same name in the font folders, else the path as it was.
QString findFontFile(const QString& path, const QStringList& folders);

// The font chooser's choice for a file ("file:<path>") and back.
QString fontFileChoice(const QString& path);
QString fontFileOfChoice(const QString& choice); // empty for an installed font
// The chooser's label of a file: "<family> (<file name>)".
QString fontFileLabel(const QString& path);

// A preview of a font for the chooser: "Ag" in it. `choice` is an installed
// family or a file's choice.
QImage fontPreview(const QString& choice);

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none: a plain widget.
#ifdef MITCAD_MAC_UI
using FontPreferencesBase = QWidget;
#else
using FontPreferencesBase = QGroupBox;
#endif

// The Fonts group of Preferences: the font folders.
class FontPreferencesBox : public FontPreferencesBase {
  Q_OBJECT

public:
  explicit FontPreferencesBox(QWidget* parent = nullptr);

  // The folders as the box shows them, to save when Preferences is
  // accepted.
  QStringList chosen() const;

signals:
  void changed();

protected:
  void showEvent(QShowEvent* event) override;

private:
  void addFolder(const QString& folder);

  QListWidget* m_folders = nullptr;
  QPushButton* m_add = nullptr;
  QPushButton* m_remove = nullptr;
};

} // namespace mitcad::sketch
