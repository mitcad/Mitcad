// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QString>

#include "PrintItemsHost.hpp"

class QComboBox;
class QLabel;
class QTabWidget;

namespace mitcad {

class MaterialsPanel;
class ProfilesPanel;
class SlicerImportBar;
struct SlicerImportResult;

// My Library (mitcad#704): the user's own materials, printers and
// processes, managed without a design. Tools › My Library… and Edit
// Library… of the Materials and Printers and Processes windows
// (mitcad#1019) open it; a non-modal window.
//
// Tabs Materials, Printers and Processes, each the panel of the design's
// window (MaterialsPanel, ProfilesPanel) on the library: its items, then
// the built-in ones under Built-in; New, Duplicate (also of a built-in
// one), Delete under the list, the name and the values edited in the same
// editor. In a project a place switch at the top, My library or This
// project's library. Import from Slicer… beside it reads a slicer's
// project or preset into the library (`import_print_profile` with
// `into`), its line and Details… before it (SlicerImportBar, mitcad#829);
// after an import the first item that came in shows on its tab,
// selected.
//
// It works through a model of its own (LibraryItemsHost): the open
// design does not change, and learns of the library's changes as of
// another design's ("changed in another design", asked once).
//
// It opens at once (mitcad#1018): read() reads the library's items, the
// built-in ones and the print keys off the UI thread (readAhead), a
// "Reading …" line shows meanwhile, and the panels fill from those answers
// when they come, each file read once per opening. Logs
// "My Library loaded: read <n> ms, materials <n> ms, printers <n> ms,
// processes <n> ms" then (the reading off the UI thread, each panel's
// filling on it).
class MyLibraryDialog : public QDialog {
  Q_OBJECT

public:
  explicit MyLibraryDialog(QWidget* parent = nullptr);
  ~MyLibraryDialog() override;

  // The open design's project (its library the second place); empty
  // outside one.
  void setProjectFolder(const QString& folder);
  // Reads the place's items off the UI thread; the panels show them when
  // they come (an opening).
  void read();
  // Shows the libraries' items as they are now, read on this thread
  // (after an import).
  void refresh();

signals:
  // The window closed after it changed a library: the design's windows
  // read the libraries again.
  void libraryChanged();

protected:
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // The place shown: `user` or `project`.
  QString place() const;
  void placeChosen();
  // The answers read ahead came: the panels show the place's items.
  void fill(const QString& library, const QueryAnswers& answers, qint64 readMs);
  // After an import: the tab of the first item that came in, with that
  // item selected.
  void showImported(const SlicerImportResult& result);
  void scheduleLogPlaces();
  void logPlaces();

  LibraryItemsHost m_host;
  QComboBox* m_place = nullptr;
  QLabel* m_reading = nullptr; // "Reading My library…" while read() reads
  int m_reads = 0;             // read()'s count: only the latest one's answers are shown
  SlicerImportBar* m_importBar = nullptr; // Import from Slicer…, its line and Details…
  QTabWidget* m_tabs = nullptr;
  MaterialsPanel* m_materials = nullptr;
  ProfilesPanel* m_printers = nullptr;
  ProfilesPanel* m_processes = nullptr;
  bool m_changed = false;
  QString m_loggedPlaces;
};

} // namespace mitcad
