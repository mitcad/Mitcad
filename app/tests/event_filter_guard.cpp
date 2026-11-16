// SPDX-License-Identifier: MIT
// An event filter on the 3D view from a dialog or an overlay, when the main
// window deletes its children in either order (mitcad#226).
#include "UnitTests.hpp"
#include "framework/EventFilterGuard.hpp"

#include <cstddef>
#include <cstdio>
#include <cstring>
#include <vector>

#include <QCoreApplication>
#include <QDialog>
#include <QResizeEvent>
#include <QStringList>
#include <QWidget>

namespace {

// The 3D view of the test. Its memory is poisoned instead of freed when it
// is deleted (until the test ends), so that a use after its destruction,
// such as removeEventFilter() on it, reads garbage pointers and crashes at
// once instead of now and then.
class PoisonedView : public QWidget {
public:
  using QWidget::QWidget;
  // The class's own new, so that GCC pairs it with the class's delete.
  static void* operator new(std::size_t size) { return ::operator new(size); }
  static void operator delete(void* memory, std::size_t size) {
    std::memset(memory, 0xa5, size);
    kept().push_back(memory);
  }
  static void release() {
    for (void* memory : kept()) {
      ::operator delete(memory);
    }
    kept().clear();
  }

private:
  static std::vector<void*>& kept() {
    static std::vector<void*> memory;
    return memory;
  }
};

// A dialog that watches the view's resizes, as Render Image does.
class WatchingDialog : public QDialog {
public:
  explicit WatchingDialog(QWidget* parent) : QDialog(parent) {}
  void watch(QObject* view) { m_viewFilter.install(view); }
  bool watching() const { return m_viewFilter.active(); }
  void stop() { m_viewFilter.remove(); }
  int resizes = 0;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::Resize) {
      ++resizes;
    }
    return QDialog::eventFilter(watched, event);
  }

private:
  mitcad::EventFilterGuard m_viewFilter{this};
};

void resize(QWidget* view) {
  QResizeEvent event(QSize(40, 30), QSize(20, 10));
  QCoreApplication::sendEvent(view, &event);
}

} // namespace

int eventFilterGuardTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "event filter guard: %s\n", message);
      ++failures;
    }
  };

  // While both live: the filter gets the view's events until it stops.
  {
    QWidget window;
    auto* view = new PoisonedView(&window);
    auto* dialog = new WatchingDialog(&window);
    dialog->watch(view);
    resize(view);
    check(dialog->resizes == 1 && dialog->watching(), "the dialog sees the view's resize");
    dialog->stop();
    resize(view);
    check(dialog->resizes == 1 && !dialog->watching(), "a stopped filter sees no more resizes");
    dialog->watch(view);
    auto* other = new QWidget(&window);
    dialog->watch(other); // the view's filter goes
    resize(view);
    resize(other);
    check(dialog->resizes == 2, "watching another object stops watching the first");
  }

  // The order that crashed at the close: the window deletes the view (its
  // older child) before the dialog that watches it.
  {
    QStringList order;
    auto* window = new QWidget;
    auto* view = new PoisonedView(window);
    auto* dialog = new WatchingDialog(window);
    QObject::connect(view, &QObject::destroyed, [&order] { order << QStringLiteral("view"); });
    QObject::connect(dialog, &QObject::destroyed, [&order] { order << QStringLiteral("dialog"); });
    dialog->watch(view);
    resize(view);
    check(dialog->resizes == 1, "the dialog watches the view");
    delete window; // crashed in the dialog's removal of its filter
    check(order == QStringList({QStringLiteral("view"), QStringLiteral("dialog")}),
          "the view went before the dialog");
  }

  // The other order: the dialog goes first and removes its filter from the
  // view that still lives.
  {
    QStringList order;
    auto* window = new QWidget;
    auto* dialog = new WatchingDialog(window);
    auto* view = new PoisonedView(window);
    QObject::connect(view, &QObject::destroyed, [&order] { order << QStringLiteral("view"); });
    QObject::connect(dialog, &QObject::destroyed, [&order] { order << QStringLiteral("dialog"); });
    dialog->watch(view);
    delete window;
    check(order == QStringList({QStringLiteral("dialog"), QStringLiteral("view")}),
          "the dialog went before the view");
  }

  // A dialog closed (deleted) while the view lives, as in normal use: the
  // view goes on without it.
  {
    QWidget window;
    auto* view = new PoisonedView(&window);
    auto* dialog = new WatchingDialog(&window);
    dialog->watch(view);
    delete dialog;
    resize(view);
  }

  PoisonedView::release();
  return failures;
}
MITCAD_UNIT_TEST(eventFilterGuardTests);
