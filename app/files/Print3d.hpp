// SPDX-License-Identifier: MIT
#pragma once

// 3D Print (mitcad#13): bodies to a slicer as one 3MF file (an object with
// a part per body) or as an STL file per body, written into a folder of
// their own and given to the slicer in one start. Here: the slicers found
// on the system, the settings, file and folder names, the dialog and the
// Preferences group; MainWindowPrint.cpp runs the command.

#include <optional>

#include <QGroupBox>
#include <QJsonArray>
#include <QString>
#include <QStringList>
#include <QVector>

class QComboBox;
class QPushButton;
class QWidget;

namespace mitcad {

// A slicer that takes the files on its command line: `program` with
// `arguments`, the files, then `argumentsAfter` (a Flatpak: flatpak run
// --file-forwarding <id> @@ <files> @@, so that the sandbox sees them).
struct Slicer {
  QString name;
  QString program;
  QStringList arguments;
  QStringList argumentsAfter;

  bool isValid() const { return !program.isEmpty(); }
  bool operator==(const Slicer& other) const {
    return program == other.program && arguments == other.arguments && argumentsAfter == other.argumentsAfter;
  }
  bool operator!=(const Slicer& other) const { return !(*this == other); }
  // "Bambu Studio (C:\...\bambu-studio.exe)", for lists and logs.
  QString describe() const;
};

// Where findSlicers looks; system() gives the system's places, tests their
// own folders.
struct SlicerSearch {
  // Windows: Program Files and the user's Programs folder, where the
  // slicers' installers put them (Bambu Studio\bambu-studio.exe, ...).
  QStringList programDirectories;
  // Folders of executables (PATH).
  QStringList path;
  // Folders of .desktop files (Linux; an AppImage integrated into the
  // desktop has one).
  QStringList applicationDirectories;
  // Folders of installed Flatpaks (Linux: /var/lib/flatpak/app,
  // ~/.local/share/flatpak/app); found ones need `flatpak` on `path`.
  QStringList flatpakDirectories;
  // Windows: the installed programs listed in the registry (read only).
  bool registry = false;

  static SlicerSearch system();
};

// The slicers found, each once: Bambu Studio, OrcaSlicer, PrusaSlicer and
// UltiMaker Cura in their usual install folders and the registry
// (Windows), on PATH, as Flatpaks and in desktop files (Linux).
QVector<Slicer> findSlicers(const SlicerSearch& search = SlicerSearch::system());

// The settings of 3D Print (group "print"), remembered between sends.
struct PrintSettings {
  Slicer slicer;              // none: the first one found
  QString format = "stl";     // "stl": a file per body; "3mf": one file
  QString refinement = "high"; // low, medium, high or custom
  double deviation = 0.01;     // custom: mm
  double angle = 8.0;          // custom: degrees

  static PrintSettings load();
  void save() const;
};

// A name a file can take on every system: characters that are not allowed
// (or are path separators) become "_", and so does a name Windows reserves
// (CON, NUL, COM1, ...); never empty.
QString safeFileName(const QString& name);

// File names for bodies: safe, with the extension, and unique ignoring
// case ("Body1.stl", "Body1_2.stl").
QStringList printFileNames(const QStringList& bodies, const QString& extension);

// What goes into one STL file of 3D Print (mitcad#17): a body where the
// design shows it, by one occurrence when it shows the body more than once.
struct PrintPiece {
  QString body;       // uid
  QString occurrence; // its path of occurrence uids ("O1/O4"); empty: every placement
  QString name;       // of the file, before the extension: "Pin", "Pin (Arm:2)"
};

// The STL files of 3D Print: one per body (`names` are theirs), and one per
// placement of a body the design places more than once, named with the
// occurrence. The placements are those of the `instances` query (with
// hidden ones): the shown ones, all when none is shown, as the model's
// export places bodies.
QVector<PrintPiece> printPieces(const QStringList& bodies, const QStringList& names, const QJsonArray& instances);

// The folder the files go into, <root>/mitcad-print/<design>: made when
// missing, and emptied, so that no file of an earlier send reaches the
// slicer. Empty, with the reason, when it cannot be.
QString preparePrintFolder(const QString& root, const QString& design, QString* error);

// What the 3D Print dialog chose.
struct PrintChoice {
  QString format;
  QString refinement;
  double deviation = 0.01;
  double angle = 8.0;
  Slicer slicer; // none: open the folder
};

// The 3D Print dialog: what is sent (`bodies`, and `leftOut`, the sheet
// and empty bodies that are not), the format, the refinement and the
// slicer (`found` and the settings' own; another program by hand). The
// choice becomes the settings.
std::optional<PrintChoice> askPrint(QWidget* parent, const QString& bodies, const QString& leftOut,
                                    const QVector<Slicer>& found);

// Preferences' 3D Print group: the slicer the command starts.
class SlicerPreferencesBox : public QGroupBox {
  Q_OBJECT

public:
  explicit SlicerPreferencesBox(QWidget* parent = nullptr);

  // The slicer as chosen (none: the first one found); saved when
  // Preferences is accepted.
  Slicer chosen() const;

private:
  QComboBox* m_slicers = nullptr;
};

// A combo box of slicers: none ("first found" or "open the folder"), the
// found ones and `current`, and Browse... (another program); for the dialog
// and Preferences.
QComboBox* slicerComboBox(const QVector<Slicer>& found, const Slicer& current, const QString& noneLabel,
                          QWidget* parent);
Slicer slicerOf(const QComboBox* combo);

} // namespace mitcad
