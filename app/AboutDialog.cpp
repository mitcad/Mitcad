// SPDX-License-Identifier: MIT
#include "AboutDialog.hpp"

#include <QApplication>
#include <QMessageBox>
#include <QObject>
#include <QString>
#include <QWidget>
#include <QtGlobal>

#include <Standard_Version.hxx>

#include "platform/MacChrome.hpp"

namespace mitcad {

void showAboutDialog(QWidget* parent) {
  // The program's name as people read it; the application name stays
  // "Mitcad", as the settings and data folders are keyed on it (mitcad#822).
  const QString name = QStringLiteral("MitCad");
  // OCCT's version is the one the program was built against, Qt's the one
  // it runs with.
  const QString occt = QString::fromLatin1(OCC_VERSION_COMPLETE);
  const QString qt = QString::fromLatin1(qVersion());
  // What the About box says besides the name and version.
  const QString body =
      QStringLiteral("<p>%1</p>")
          .arg(QObject::tr("A parametric desktop CAD application with a history-based modeling workflow.")) +
      QStringLiteral("<p>%1</p>")
          .arg(QObject::tr("Licensed under the MIT license. Copyright (c) 2026 MitCad contributors.")) +
      QStringLiteral("<p>%1</p><ul>").arg(QObject::tr("Built with:")) +
      QStringLiteral("<li>%1</li>")
          .arg(QObject::tr("Open CASCADE Technology %1 (LGPL-2.1 with the Open CASCADE exception)")
                   .arg(occt.toHtmlEscaped())) +
      QStringLiteral("<li>%1</li>").arg(QObject::tr("Qt %1 (LGPL-3.0)").arg(qt.toHtmlEscaped())) +
      QStringLiteral("<li>%1</li>").arg(QObject::tr("FreeType (FreeType License)")) +
      QStringLiteral("<li>%1</li>").arg(QObject::tr("Droid Sans fonts, Google Corporation (Apache-2.0)")) +
      // The bundled material dataset (mitcad#1213): its attribution.
      QStringLiteral("<li>%1</li>")
          .arg(QObject::tr("Material data from the material-properties dataset, its contributors (Apache-2.0)")) +
      QStringLiteral("</ul>") +
      QStringLiteral("<p>%1</p>")
          .arg(QObject::tr("Qt is used under the GNU LGPL v3 (<a href=\"https://www.gnu.org/licenses/lgpl-3.0.html\">"
                           "gnu.org/licenses/lgpl-3.0.html</a>)."));
#ifdef Q_OS_MACOS
  // The standard About panel shows the name and version itself.
  Q_UNUSED(parent);
  mac::showAboutPanel(body);
  return;
#else
  const QString text =
      QStringLiteral("<h3>%1 %2</h3>").arg(name.toHtmlEscaped(), QApplication::applicationVersion().toHtmlEscaped()) +
      body;
  QMessageBox::about(parent, QObject::tr("About %1").arg(name), text);
#endif
}

} // namespace mitcad
