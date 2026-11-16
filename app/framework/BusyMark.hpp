// SPDX-License-Identifier: MIT
#pragma once

#include <QElapsedTimer>
#include <QSize>
#include <QString>
#include <QWidget>

class QTimer;

namespace mitcad {

// A small busy mark, an animated spinner the height of a line of text
// (mitcad#1907, GitHub mitcad/Mitcad#361): a command panel shows it beside
// OK and Cancel while its preview computes, instead of a message line that
// took height and made the panel grow and shrink with each preview. It
// keeps its room whether shown or not, so it changes no size. It shows only
// once the work has lasted kDelayMs, so quick work does not flicker it; when
// the work ends it hides, kGraceMs later, unless new work has started
// meanwhile (a drag's previews one after another keep it shown). Its text
// (Computing preview…) is its accessible name and its tooltip.
class BusyMark : public QWidget {
  Q_OBJECT

public:
  // How long work lasts before the mark shows.
  static constexpr int kDelayMs = 300;
  // How long a shown mark waits for new work before it hides.
  static constexpr int kGraceMs = 150;

  explicit BusyMark(QWidget* parent = nullptr);

  // Work under way, described by `text`: the mark shows once it has lasted
  // kDelayMs. Empty: the work ended (the mark hides, kGraceMs later if it
  // shows). The same work again changes nothing (its time goes on).
  void setBusy(const QString& text);
  // Whether work is under way, whether the mark shows yet or not.
  bool busy() const { return !m_text.isEmpty(); }
  // Whether the mark shows (also during its grace after the work ended).
  bool shown() const { return m_shown; }
  // How long the work under way has lasted, in ms; -1 when there is none.
  qint64 busyMs() const;

signals:
  // The mark came into sight (`on` true) or went: `ms` how long the
  // work had lasted when it showed, or how long the mark was shown.
  void shownChanged(bool on, qint64 ms);

protected:
  void paintEvent(QPaintEvent* event) override;
  // A new font (Text size) gives the mark the new line's height.
  void changeEvent(QEvent* event) override;

private:
  void updateSize();
  void setShown(bool on);

  QString m_text;
  bool m_shown = false;
  int m_step = 0; // the spinner's position, in steps of a turn
  QElapsedTimer m_since; // since the work under way started
  QElapsedTimer m_shownSince;
  QTimer* m_delay = nullptr;
  QTimer* m_grace = nullptr;
  QTimer* m_spin = nullptr;
};

} // namespace mitcad
