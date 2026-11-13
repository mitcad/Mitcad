// SPDX-License-Identifier: MIT
#pragma once

// 3D Print (mitcad#13): bodies to a slicer as one 3MF file (an object with
// a part per body) or as an STL file per body, written into a folder of
// their own and given to the slicer in one start. Here: the slicers found
// on the system, the settings, file and folder names, the panel's rows and the
// Preferences group; MainWindowPrint.cpp defines and runs the command.

#include <QByteArray>
#include <QColor>
#include <QGroupBox>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Selection.hpp"

class QComboBox;
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
  // The program, or the application bundle `open -a` starts (macOS).
  QString location() const;
};

// The dialect of a slicer's print settings (mitcad#640, the model's
// `export` with `dialect`): bambu_studio, orcaslicer, qidi_studio,
// prusaslicer or cura by its name (or its program's), empty for another
// program.
QString slicerDialect(const Slicer& slicer);

// Whether a slicer takes the design's print plates in one 3MF (mitcad#1050,
// the model's `export` with `plates`): the Bambu family (by `dialect`,
// the slicer's or the design's), PrusaSlicer from 2.9 on (a version in its
// program's path or name before 2.9 says otherwise); Cura, another program
// and no slicer take a 3MF per plate.
bool slicerTakesPlates(const Slicer& slicer, const QString& dialect);

// A slicer for a program the user chose: a macOS application bundle
// (Name.app) starts with `open -a`, which hands it the files; any other
// program takes them on its command line.
Slicer slicerForProgram(const QString& name, const QString& program);

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
  // Folders of application bundles (macOS: /Applications and
  // ~/Applications), where the slicers are Name.app.
  QStringList bundleDirectories;

  static SlicerSearch system();
};

// The slicers found, each once: Bambu Studio, OrcaSlicer, QIDI Studio,
// PrusaSlicer and UltiMaker Cura in their usual install folders and the registry
// (Windows), as application bundles (macOS), on PATH, as Flatpaks and in
// desktop files (Linux).
QVector<Slicer> findSlicers(const SlicerSearch& search = SlicerSearch::system());

// The settings of 3D Print (group "print"), remembered between sends.
struct PrintSettings {
  Slicer slicer;              // none: the first one found
  // The last send's Open in was Folder only (mitcad#1726): the panel opens
  // on it again; `slicer` stays what it was, listed in the panel.
  bool folderOnly = false;
  QString format = "3mf";     // "3mf": one file (the first time, mitcad#655); "stl": a file per body
  QString refinement = "high"; // low, medium, high or custom
  double deviation = 0.01;     // custom: mm
  double angle = 8.0;          // custom: degrees
  // Use the open slicer window (mitcad#1162): a send of a design whose
  // slicer MitCad started is still open goes to that window.
  bool reuseWindow = true;

  static PrintSettings load();
  void save() const;
};

// How a slicer window MitCad started takes a newer file of its design
// (mitcad#1162, Use the open slicer window):
// - SingleInstanceSwitch: the slicer started again with --single-instance
//   hands the file to its running window and exits (PrusaSlicer, UltiMaker
//   Cura); the first start of the window takes the switch too, so that it
//   listens (Cura) or can be found (PrusaSlicer on Windows).
// - OwnPreference: the slicer hands a file over by itself only when its
//   own preference to keep one instance is on (Bambu Studio, OrcaSlicer,
//   QIDI Studio: their command lines refuse --single-instance);
//   slicerKeepsOneInstance reads it.
// - BringForward: no way to hand a file over (another program, an
//   application bundle started with `open -a`): MitCad brings the window
//   to the front and says that the file was updated.
enum class SlicerHandOver { BringForward, SingleInstanceSwitch, OwnPreference };
SlicerHandOver slicerHandOver(const Slicer& slicer);

// The slicer's command line for `files`: its arguments, --single-instance
// when `singleInstance` (before a Flatpak's file forwarding, so that the
// application gets it), the files, then argumentsAfter.
QStringList slicerArguments(const Slicer& slicer, const QStringList& files, bool singleInstance);

// Whether an OwnPreference slicer's settings file keeps one instance: its
// "app" section's single_instance true. `text` is the file's content (a
// JSON object, with "#" comment lines such as its checksum); false when it
// cannot be read.
bool slicerSingleInstanceSetting(const QByteArray& text);
// The settings files where an OwnPreference slicer keeps its preferences
// (the user's configuration folder, a Flatpak's own), the first existing
// one read by slicerKeepsOneInstance; empty for other slicers.
QStringList slicerSettingsFiles(const Slicer& slicer);
bool slicerKeepsOneInstance(const Slicer& slicer);

// Send to Slicer's Center on Plate and Drop to Bed (mitcad#1109), kept per
// design (by its file; designs not saved yet share one pair): both on
// until changed.
struct PrintPlacement {
  bool center = true;
  bool drop = true;

  static PrintPlacement load(const QString& design);
  void save(const QString& design) const;
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
// export places bodies; of a body placed more than once only those
// `chosen` (body items with their occurrences, mitcad#234), all when none
// of its placements is.
QVector<PrintPiece> printPieces(const QStringList& bodies, const QStringList& names, const QJsonArray& instances,
                                const Selection& chosen = {});

// How an export put the bodies on the print bed (mitcad#731), for the log:
// the body and face on the bed ("(automatic)" after a face the automatic
// orientation chose, mitcad#1109), the plate's centre and the transform
// after the design's placements (rows "a b c t", three decimals); the
// bodies kept as assembled; or as in the design; then the 3MF file's other
// objects,
// each on its face or kept as assembled ("objects Nut (Nuts:1/Nut:2) on
// F2:top, ...", mitcad#856); then each bed face that cannot be used. `bed`
// is the `export` result's `bed`.
QString printBedLog(const QJsonObject& bed);

// The print bed face's green (mitcad#731): the view shows bed faces in it
// (a face's context menu, 3D Print's choice of mitcad#857).
QColor printBedColor();

// A print bed face a 3MF piece kept as assembled could lie on (mitcad#857):
// an entry of the export's `bed` `choices`.
struct PrintBedFace {
  QString body;           // uid
  QString name;           // the body's name
  QString face;           // as it was assigned
  QString occurrence;     // the placement's path of uids ("O1/O4"); empty: the design itself
  QString occurrenceName; // "Pair:1"
  bool isEmpty() const { return body.isEmpty(); }
  // "Body1 F1:top in Pair:1", for the log.
  QString describe() const;
};

// A piece with several print bed faces to choose from (mitcad#857): the
// object's name (empty for the file's own object), its faces, and the key
// a choice is remembered by (the piece and its faces).
struct PrintBedAsk {
  QString piece;
  QString key;
  QVector<PrintBedFace> faces;
  bool isEmpty() const { return faces.isEmpty(); }
  // The piece by name, for the user: the object's name, else the
  // occurrence its faces lie in ("Pair:1", the names their paths share);
  // empty when they share none.
  QString pieceName() const;
};

// The pieces of an export plan's `bed` (`plan`: true) that have two or
// more faces to choose from: the file's own object's, then each object's.
QVector<PrintBedAsk> printBedAsks(const QJsonObject& bed);

// The texts of the bed face choice (mitcad#857 review): the question
// beside the Bed face row and in the status bar ("Not sent yet: Pair:1 has
// 2 print bed faces. Click the green one to put on the bed."), a face
// under the mouse ("Tall: print bed face"), a click on another face, and
// the last line's account of a face chosen ("Pair:1 on Tall's print bed
// face."). A body with faces in several places of the piece is named with
// the place: "Nut (Nuts:1/Nut:2)".
QString printBedQuestion(const PrintBedAsk& ask);
QString printBedHover(const PrintBedAsk& ask, const PrintBedFace& face);
QString printBedRefused();
QString printBedChosen(const PrintBedAsk& ask, const PrintBedFace& face);

// The last line's account of the print bed (mitcad#856, mitcad#857):
// `message`, then each face chosen (`chosen`, printBedChosen, each counted
// in `onFaces`), then how many other placements lie on their print bed
// faces.
QString printBedLine(const QString& message, int onFaces, const QStringList& chosen = {});

// What the 3D Print panel keeps of the choices while it is open
// (mitcad#857): by a piece's key the face chosen, or an empty one when it
// is sent as assembled (Send as assembled); the piece asked about now
// (none: empty); and the status bar's text while it is asked (the
// question, or the face under the mouse).
struct PrintBedChoices {
  QHash<QString, PrintBedFace> chosen;
  PrintBedAsk asking;
  QString hint;
};

// The folder the files go into, <root>/mitcad-print/<design>: made when
// missing, and emptied, so that no file of an earlier send reaches the
// slicer. Empty, with the reason, when it cannot be.
QString preparePrintFolder(const QString& root, const QString& design, QString* error);

// The 3D Print panel (mitcad#234, MainWindowPrint.cpp).
//
// The bodies it starts with: what the selection made before stands for (a
// face, edge or vertex its body, an occurrence the bodies shown in it), or
// with none of that every body shown (`instances`, the query's answer),
// each placement a body item with its occurrence, once; `selected` tells
// which.
Selection printStartBodies(const Selection& selection, const QJsonArray& instances, bool* selected);

// The Slicer row's choices: none (open the files' folder), `current` (the
// settings' slicer) and `other` (one chosen by hand) unless found, the
// slicers found, and Other program... (otherSlicerChoice), which asks for
// a program. A slicer's choice is slicerChoice's text of it.
QVector<QPair<QString, QString>> slicerChoices(const QVector<Slicer>& found, const Slicer& current,
                                               const Slicer& other);
QString slicerChoice(const Slicer& slicer);
Slicer slicerOfChoice(const QString& choice);
QString otherSlicerChoice();

// The settings' slicer while its program is there (one of `found`, or its
// program or application bundle exists; a bare name is looked for on PATH),
// else none: a slicer removed since the last send is not offered.
Slicer installedSlicer(const Slicer& saved, const QVector<Slicer>& found);

// What Open in starts on (mitcad#1726): none (Folder only) when the last
// send chose it, else the settings' slicer while it is installed, else the
// first one found (none without any).
Slicer panelSlicer(const PrintSettings& settings, const QVector<Slicer>& found);

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
// found ones and `current`, and Browse... (another program); for Preferences.
QComboBox* slicerComboBox(const QVector<Slicer>& found, const Slicer& current, const QString& noneLabel,
                          QWidget* parent);
Slicer slicerOf(const QComboBox* combo);

} // namespace mitcad
