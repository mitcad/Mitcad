// SPDX-License-Identifier: MIT
#pragma once

#include <memory>
#include <functional>
#include <QColorDialog>
#include <QObject>
#include <QPoint>
#include <QUrl>
#include <QVariant>
#include <QtGlobal>

class QAbstractItemView;
class QAction;
class QDialog;
class QFileDialog;
class QMenu;
class QModelIndex;
class QKeyEvent;
class QListWidget;
class QWidget;

namespace mitcad {

// Optional per-cell edit permission for views whose Qt delegates are
// opened manually. An unset role uses the view's normal edit policy.
inline constexpr int NativeEditableRole = Qt::UserRole + 4096;
// Stable identifiers for native model checkboxes and focused UI tests.
inline constexpr int NativeControlIdentifierRole = Qt::UserRole + 4097;
// A cell whose edited value the model refused (true): the Qt delegate and
// the native cell presenter both show it in the error colours; its reason
// is the cell's tooltip.
inline constexpr int NativeRefusedRole = Qt::UserRole + 4098;
// Optional QHeaderView property: a QVariantList of QObject* widget editors,
// one per logical column. Native tables retain these real editing controls
// in their scrolling header instead of displaying only headerData text.
inline constexpr char NativeColumnEditorsProperty[] = "nativeColumnEditors";
// After a shared header's one-time resizeColumnToContents calls, advance
// this request so AppKit measures its own font/header/cell insets too.
// Resize modes stay unchanged; later native user widths survive ordinary
// refreshes until the next request or an explicit shared section resize.
inline constexpr char NativeColumnContentsRequestProperty[] = "nativeColumnContentsRequest";
inline void requestNativeColumnContents(QObject* header) {
  if (header != nullptr) {
    const quint64 previous = header->property(NativeColumnContentsRequestProperty).toULongLong();
    header->setProperty(NativeColumnContentsRequestProperty, QVariant::fromValue(previous + 1));
  }
}
// Optional QObject* QWidget target: Return commits this field and focuses
// that presenter, rather than pressing the secondary window's default.
inline constexpr char NativeReturnTargetProperty[] = "nativeReturnTarget";
// A NoSelection checklist whose current row is a keyboard cursor, rather
// than a selection. Arrows publish the visible model row; Space reaches
// the shared view's event filter, and Tab reaches its
// current row's index widget. The QObject* next target follows that editor.
inline constexpr char NativeCurrentRowNavigationProperty[] = "nativeCurrentRowNavigation";
inline constexpr char NativeCurrentRowNextProperty[] = "nativeCurrentRowNext";
// Optional positive QListView row limit for a compact multiline list. Native
// rows preserve explicit line breaks and the shared row-size hints; its
// viewport grows up to this many complete rows, then scrolls the remainder.
inline constexpr char NativeListRowLimitProperty[] = "nativeListRowLimit";
// A custom-painted colour button exposes its display data without changing
// its Qt paint/layout or its clicked action. An invalid QColor draws the
// unset swatch; placeholder text names the inherited value or slicer default.
inline constexpr char NativeColorFieldProperty[] = "nativeColorField";
inline constexpr char NativeColorFieldTextProperty[] = "nativeColorFieldText";
inline constexpr char NativeColorFieldPlaceholderProperty[] = "nativeColorFieldPlaceholder";
// Optional QString on a QAbstractItemView that paints a hint in place of
// rows while its model has none ("No rows yet. ..."): the native table or
// outline shows the same text, in its rows' top left corner, then. Its
// label is identified as the view's objectName plus ".empty".
inline constexpr char NativeEmptyTextProperty[] = "nativeEmptyText";

// An optional multiline editor's own key commands and completion list.
// AppKit keeps ordinary editing/IME; the shared editor gets first refusal
// with its text and UTF-16 selection already synchronized. A handled key
// publishes the shared text/cursor back to the same native text view.
class NativeTextEditorCommands {
public:
  virtual ~NativeTextEditorCommands() = default;
  virtual bool nativeTextKey(QKeyEvent* event) = 0;
  virtual QListWidget* nativeTextCompletions() const = 0;
  virtual void nativeTextComplete(int row) = 0;
};

// Secondary AppKit views bind the existing Qt presenter widgets' data,
// signals and models. They do not embed or draw those widgets.
class NativeWidgetView {
public:
  virtual ~NativeWidgetView() = default;
  virtual void* nativeView() const = 0;
  virtual void refresh() = 0;
};

#if defined(Q_OS_MACOS) && defined(MITCAD_MACOS_NATIVE_UI)
void setNativeDialogWindow(void* nativeNSWindow);
void* nativeDialogWindow(); // Borrowed document NSWindow; popup anchoring only.
bool nativeDialogsActive();
// Includes tracked sheets and AppKit's standalone modal alert/file panels.
bool nativeDialogModalActive();
void waitNativeUntil(const std::function<bool()>& completed);
std::unique_ptr<NativeWidgetView> createNativeWidgetView(QWidget* widget);
// In-process regression instrumentation; counts only changed properties.
quint64 nativeWidgetPropertyWrites();
int execNativeDialog(QDialog* dialog);
int execNativeFileDialog(QFileDialog* dialog);
// Borrowed NSSavePanel/NSOpenPanel, valid only during native file-dialog exec.
void* nativeFileDialogPanel();
QList<QUrl> nativeFileDialogUrls(const QFileDialog* dialog);
QColor getNativeColor(const QColor& initial, QWidget* parent, const QString& title,
                     QColorDialog::ColorDialogOptions options = {});
void showNativeWidget(QWidget* widget);
QAction* execNativeMenu(QMenu* menu, const QPoint& point);
#else
inline void setNativeDialogWindow(void*) {}
inline void* nativeDialogWindow() { return nullptr; }
inline bool nativeDialogsActive() { return false; }
inline bool nativeDialogModalActive() { return false; }
inline void* nativeFileDialogPanel() { return nullptr; }
inline std::unique_ptr<NativeWidgetView> createNativeWidgetView(QWidget*) { return {}; }
#endif

// Row drags inside a hierarchical presenter's native outline (mitcad#816).
// The presenter keeps its own rules: `draggable` says whether a row can be
// dragged, `allowed` whether it can be dropped on the row under the
// pointer, `drop` takes a release (`over` invalid outside the rows, a
// refused drop for its own feedback), and `dragging` brackets the drag.
// Nothing leaves the outline. Indexes are the presenter model's.
struct NativeRowMoves {
  std::function<bool(const QModelIndex& row)> draggable;
  std::function<bool(const QModelIndex& dragged, const QModelIndex& over)> allowed;
  std::function<void(const QModelIndex& dragged, const QModelIndex& over)> drop;
  std::function<void(const QModelIndex& dragged, bool dragging)> dragging;
};
#if defined(Q_OS_MACOS) && defined(MITCAD_MACOS_NATIVE_UI)
void setNativeRowMoves(QAbstractItemView* view, NativeRowMoves moves);
#else
inline void setNativeRowMoves(QAbstractItemView*, NativeRowMoves) {}
#endif

} // namespace mitcad
