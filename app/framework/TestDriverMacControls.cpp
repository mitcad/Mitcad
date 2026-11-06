// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"
#include "CommandPanel.hpp"
#include "MacToolButtonStyle.hpp"

#include <QApplication>
#include <QDebug>
#include <QStyle>
#include <QToolButton>

namespace mitcad {

void TestDriver::testMacToolButtons() {
#ifdef MITCAD_MAC_UI
  QWidget* window = m_main.data();
  if (window == nullptr) {
    fail(m_line, QStringLiteral("no window for macOS tool button audit"));
    return;
  }
  int visible = 0;
  int total = 0;
  int sampled = 0;
  int semanticSurfaces = 0;
  for (QToolButton* button : window->findChildren<QToolButton*>()) {
    ++total;
    visible += button->isVisible();
    if (!button->property("macFlatToolButton").toBool()) {
      QStringList ancestry;
      for (QObject* owner = button->parent(); owner != nullptr; owner = owner->parent()) {
        ancestry << QStringLiteral("%1/%2").arg(QString::fromLatin1(owner->metaObject()->className()),
                                               owner->objectName());
      }
      const QStyle* shared = qApp->findChild<QStyle*>(QStringLiteral("macToolButtonStyle"),
                                                    Qt::FindDirectChildrenOnly);
      qWarning().noquote()
          << QStringLiteral("macOS missing tool style diagnostic: class=%1 name=%2 visible=%3 polished=%4 "
                            "style=%5/%6 isShared=%7 stylesheetAttribute=%8 stylesheetTarget=%9 "
                            "localStylesheetEmpty=%10 recoveryQueued=%11 installing=%12 ancestors=[%13]")
                 .arg(QString::fromLatin1(button->metaObject()->className()), button->objectName())
                 .arg(button->isVisible()).arg(button->testAttribute(Qt::WA_WState_Polished))
                 .arg(QString::fromLatin1(button->style()->metaObject()->className()), button->style()->objectName())
                 .arg(button->style() == shared).arg(button->testAttribute(Qt::WA_StyleSheet))
                 .arg(button->testAttribute(Qt::WA_StyleSheetTarget)).arg(button->styleSheet().isEmpty())
                 .arg(button->property("macToolButtonRecoveryQueued").toBool())
                 .arg(button->property("macToolButtonInstalling").toBool()).arg(ancestry.join(QStringLiteral(" -> ")));
      fail(m_line, QStringLiteral("tool button bypasses shared macOS style: %1 (%2)")
                       .arg(button->objectName(), button->text()));
      return;
    }
    // These two exact badge roles deliberately have colored capsule outlines.
    // Choice tiles likewise have persistent content outlines, unlike ordinary
    // tool controls. No arbitrary stylesheet or custom-painted class is exempt.
    bool choiceTile = false;
    if (button->property("commandChoiceTile").toBool()) {
      for (QObject* parent = button->parent(); parent != nullptr; parent = parent->parent()) {
        if (qobject_cast<CommandPanel*>(parent) != nullptr) {
          choiceTile = true;
          break;
        }
      }
    }
    const bool statusBadge = button->objectName() == QStringLiteral("failuresBadge") ||
                             button->objectName() == QStringLiteral("warningsBadge");
    if (choiceTile || statusBadge) {
      ++semanticSurfaces;
      continue;
    }
    ++sampled;
    if (macToolButtonPaintsIdleSurface(button)) {
      fail(m_line, QStringLiteral("ordinary tool button paints a surface at rest: %1 (%2), stylesheet [%3]")
                       .arg(button->objectName(), button->text(), button->styleSheet()));
      return;
    }
  }
  if (visible == 0) {
    fail(m_line, QStringLiteral("no visible buttons in macOS tool button audit"));
    return;
  }
  // This guard samples the actual style on the running native platform,
  // independently of its enrollment marker. A future native bezel at rest
  // must fail even if someone leaves the marker in place.
  QToolButton dynamic(window);
  dynamic.ensurePolished();
  if (!dynamic.property("macFlatToolButton").toBool()) {
    fail(m_line, QStringLiteral("dynamically created tool button bypasses shared macOS style"));
    return;
  }
  if (macToolButtonPaintsIdleSurface(&dynamic)) {
    fail(m_line, QStringLiteral("macOS tool button style paints a bezel at rest"));
    return;
  }
  qInfo().noquote() << QStringLiteral("macOS tool button audit: %1 visible, %2 enrolled, dynamic idle bezel absent, DPR %3")
                          .arg(visible).arg(total).arg(window->devicePixelRatioF());
  qInfo().noquote() << QStringLiteral("macOS tool button surfaces: %1 actual styles sampled, %2 semantic badge/tile outlines")
                          .arg(sampled).arg(semanticSurfaces);
#else
  fail(m_line, QStringLiteral("macOS tool button audit requires macOS"));
#endif
}

} // namespace mitcad
