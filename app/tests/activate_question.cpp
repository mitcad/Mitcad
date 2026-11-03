// SPDX-License-Identifier: MIT
// Create Sketch's and the Construct commands' question whether to activate
// the picked geometry's component (mitcad#960): when it asks (a face of
// the active component, of its child occurrence, of another top-level
// component, an origin plane, a pick the design policy refuses, the
// preference off), the names it gives, and the message box with its
// buttons, default and check box.
#include "browser/ActivateQuestion.hpp"

#include <cstdio>

#include <QApplication>
#include <QCheckBox>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QTemporaryDir>
#include <QTimer>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "activate_question.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

DocumentSnapshot::Occurrence occurrence(const QString& uid, const QString& name, const QString& path,
                                        const QString& component) {
  DocumentSnapshot::Occurrence made;
  made.uid = uid;
  made.name = name;
  made.path = path;
  made.component = component;
  return made;
}

DocumentSnapshot::Body body(const QString& uid, const QString& component) {
  DocumentSnapshot::Body made;
  made.uid = uid;
  made.name = uid;
  made.component = component;
  return made;
}

// Root (C0) with a body of its own (F9.b0); Corner brace:1 (O1, C1) with
// a plate (F2.b0) and in it M6 rod bracket:1 (O1/O2, C2) with a block
// (F4.b0) and a construction plane (F7); Plate:1 (O3, C3) with F6.b0; a
// second M6 rod bracket, :2 (O4), at the top. Corner brace is active.
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  const auto component = [](const QString& uid, const QString& name) {
    DocumentSnapshot::Component made;
    made.uid = uid;
    made.name = name;
    return made;
  };
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root")));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Corner brace")));
  snapshot.components.append(component(QStringLiteral("C2"), QStringLiteral("M6 rod bracket")));
  snapshot.components.append(component(QStringLiteral("C3"), QStringLiteral("Plate")));
  DocumentSnapshot::Occurrence brace =
      occurrence(QStringLiteral("O1"), QStringLiteral("Corner brace:1"), QStringLiteral("O1"), QStringLiteral("C1"));
  brace.children.append(occurrence(QStringLiteral("O2"), QStringLiteral("M6 rod bracket:1"), QStringLiteral("O1/O2"),
                                   QStringLiteral("C2")));
  snapshot.occurrences.append(brace);
  snapshot.occurrences.append(
      occurrence(QStringLiteral("O3"), QStringLiteral("Plate:1"), QStringLiteral("O3"), QStringLiteral("C3")));
  snapshot.occurrences.append(occurrence(QStringLiteral("O4"), QStringLiteral("M6 rod bracket:2"), QStringLiteral("O4"),
                                         QStringLiteral("C2")));
  snapshot.bodies.insert(QStringLiteral("F2.b0"), body(QStringLiteral("F2.b0"), QStringLiteral("C1")));
  snapshot.bodies.insert(QStringLiteral("F4.b0"), body(QStringLiteral("F4.b0"), QStringLiteral("C2")));
  snapshot.bodies.insert(QStringLiteral("F6.b0"), body(QStringLiteral("F6.b0"), QStringLiteral("C3")));
  snapshot.bodies.insert(QStringLiteral("F9.b0"), body(QStringLiteral("F9.b0"), QStringLiteral("C0")));
  DocumentSnapshot::Feature plane;
  plane.uid = QStringLiteral("F7");
  plane.name = QStringLiteral("Plane1");
  plane.type = QStringLiteral("construction_plane");
  plane.component = QStringLiteral("C2");
  snapshot.features.append(plane);
  snapshot.activeComponent = QStringLiteral("C1");
  return snapshot;
}

SelectionItem face(const QString& body, const QString& occurrence) {
  SelectionItem item;
  item.kind = SelectKind::Face;
  item.owner = body;
  item.name = QStringLiteral("F4:end(r1)");
  item.geometry = QStringLiteral("plane");
  item.occurrence = occurrence;
  return item;
}

SelectionItem datum(const QString& uid, const QString& occurrence) {
  SelectionItem item;
  item.kind = SelectKind::Plane;
  item.owner = uid;
  item.occurrence = occurrence;
  return item;
}

void decisionTests() {
  const DocumentSnapshot snapshot = design();

  // The active component's own face: nothing asked.
  CHECK(!activateQuestion(snapshot, face(QStringLiteral("F2.b0"), QStringLiteral("O1")), QString(), true).ask);

  // A face of the active component's child occurrence: asked, by the
  // occurrence picked and the active one's above it.
  const ActivateQuestion child =
      activateQuestion(snapshot, face(QStringLiteral("F4.b0"), QStringLiteral("O1/O2")), QString(), true);
  CHECK(child.ask);
  CHECK(child.component == QStringLiteral("C2"));
  CHECK(child.occurrence == QStringLiteral("O1/O2"));
  CHECK(child.picked == QStringLiteral("M6 rod bracket:1"));
  CHECK(child.active == QStringLiteral("Corner brace:1"));
  CHECK(child.kind == QStringLiteral("face"));
  CHECK(child.text() == QStringLiteral("The face belongs to M6 rod bracket:1, but Corner brace:1 is active."));

  // The same component's other occurrence: that one is named, and the
  // active component by its first occurrence.
  const ActivateQuestion second =
      activateQuestion(snapshot, face(QStringLiteral("F4.b0"), QStringLiteral("O4")), QString(), true);
  CHECK(second.ask);
  CHECK(second.occurrence == QStringLiteral("O4"));
  CHECK(second.picked == QStringLiteral("M6 rod bracket:2"));
  CHECK(second.active == QStringLiteral("Corner brace:1"));

  // Another top-level component's face: asked.
  const ActivateQuestion other =
      activateQuestion(snapshot, face(QStringLiteral("F6.b0"), QStringLiteral("O3")), QString(), true);
  CHECK(other.ask);
  CHECK(other.component == QStringLiteral("C3"));
  CHECK(other.picked == QStringLiteral("Plate:1"));
  CHECK(other.active == QStringLiteral("Corner brace:1"));

  // The root's own body while a component is active: the root is named.
  const ActivateQuestion root = activateQuestion(snapshot, face(QStringLiteral("F9.b0"), QString()), QString(), true);
  CHECK(root.ask);
  CHECK(root.component == QStringLiteral("C0"));
  CHECK(root.occurrence.isEmpty());
  CHECK(root.picked == QStringLiteral("Root"));

  // An origin plane is the active component's own: nothing asked.
  CHECK(!activateQuestion(snapshot, datum(QStringLiteral("xy"), QString()), QString(), true).ask);
  CHECK(!activateQuestion(snapshot, datum(QStringLiteral("xz"), QStringLiteral("O1")), QString(), true).ask);

  // A construction plane of the child occurrence's component: asked as a
  // plane.
  const ActivateQuestion plane =
      activateQuestion(snapshot, datum(QStringLiteral("F7"), QStringLiteral("O1/O2")), QString(), true);
  CHECK(plane.ask);
  CHECK(plane.picked == QStringLiteral("M6 rod bracket:1"));
  CHECK(plane.text() == QStringLiteral("The plane belongs to M6 rod bracket:1, but Corner brace:1 is active."));
  // Picked where no occurrence path was given (a browser row): its
  // component's first occurrence.
  const ActivateQuestion row = activateQuestion(snapshot, datum(QStringLiteral("F7"), QString()), QString(), true);
  CHECK(row.ask);
  CHECK(row.occurrence == QStringLiteral("O1/O2"));

  // An edge says so.
  SelectionItem edge = face(QStringLiteral("F6.b0"), QStringLiteral("O3"));
  edge.kind = SelectKind::Edge;
  CHECK(activateQuestion(snapshot, edge, QString(), true).text() ==
        QStringLiteral("The edge belongs to Plate:1, but Corner brace:1 is active."));

  // A face the design policy refuses: refused as before, nothing asked.
  CHECK(!activateQuestion(snapshot, face(QStringLiteral("F6.b0"), QStringLiteral("O3")),
                          QStringLiteral("Plate refuses attachments from other components."), true)
             .ask);

  // The preference off: nothing asked.
  CHECK(!activateQuestion(snapshot, face(QStringLiteral("F4.b0"), QStringLiteral("O1/O2")), QString(), false).ask);

  // The root active: its child occurrence's face asks, the root by its
  // component's name.
  DocumentSnapshot rootActive = design();
  rootActive.activeComponent = QStringLiteral("C0");
  const ActivateQuestion fromRoot =
      activateQuestion(rootActive, face(QStringLiteral("F2.b0"), QStringLiteral("O1")), QString(), true);
  CHECK(fromRoot.ask);
  CHECK(fromRoot.text() == QStringLiteral("The face belongs to Corner brace:1, but Root is active."));
  CHECK(!activateQuestion(rootActive, face(QStringLiteral("F9.b0"), QString()), QString(), true).ask);

  // Geometry of no component (a preview's body): nothing asked.
  CHECK(!activateQuestion(snapshot, face(QStringLiteral("F99.b0"), QStringLiteral("O9")), QString(), true).ask);

  // The second line names what the command makes: the sketch (Create
  // Sketch, the default), or a Construct command's plane, axis or point.
  ActivateQuestion made = activateQuestion(snapshot, face(QStringLiteral("F4.b0"), QStringLiteral("O1/O2")),
                                           QString(), true);
  CHECK(made.detail() == QStringLiteral("If you activate it, the sketch goes into M6 rod bracket:1. If you keep "
                                        "Corner brace:1 active, the sketch goes into Corner brace:1."));
  CHECK(madeBy(QStringLiteral("construction_plane")) == QStringLiteral("plane"));
  CHECK(madeBy(QStringLiteral("construction_axis")) == QStringLiteral("axis"));
  CHECK(madeBy(QStringLiteral("construction_point")) == QStringLiteral("point"));
  CHECK(madeBy(QString()) == QStringLiteral("sketch"));
  made.made = madeBy(QStringLiteral("construction_axis"));
  CHECK(made.detail() == QStringLiteral("If you activate it, the axis goes into M6 rod bracket:1. If you keep "
                                        "Corner brace:1 active, the axis goes into Corner brace:1."));
}

// Where the question goes: clear of the pick (80 px around it) inside the
// window; above the pick first, else below, right or left of it.
void placeTests() {
  const QRect window(0, 0, 1280, 800);
  const QSize box(460, 160);
  const QRect keep(500 - 80, 450 - 80, 161, 161);
  // A pick in the middle: above it, centred on it.
  const QRect middle = questionPlace(box, QPoint(500, 450), window);
  CHECK(middle == QRect(QPoint(270, 450 - 80 - 160), box));
  CHECK(!middle.intersects(keep));
  // A pick near the top: below it.
  const QRect top = questionPlace(box, QPoint(500, 100), window);
  CHECK(top.top() == 100 + 81);
  CHECK(!top.intersects(QRect(420, 20, 161, 161)));
  // Near the window's edge: moved inside the window, still clear.
  const QRect corner = questionPlace(box, QPoint(1250, 450), window);
  CHECK(window.contains(corner));
  CHECK(!corner.intersects(QRect(1170, 370, 161, 161)));
  // A window too small to keep clear: inside it, covering the least.
  const QRect small(0, 0, 500, 200);
  const QRect crowded = questionPlace(box, QPoint(250, 100), small);
  CHECK(crowded == QRect(QPoint(20, 0), box)); // above, pushed down into it
}

// The message box: its texts, Activate the default, and the answers.
void dialogTests() {
  const ActivateQuestion question =
      activateQuestion(design(), face(QStringLiteral("F4.b0"), QStringLiteral("O1/O2")), QString(), true);
  const auto answerWith = [&question](bool activate, bool dontAsk) {
    bool seen = false;
    QTimer::singleShot(0, [&seen, activate, dontAsk] {
      auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget());
      CHECK(box != nullptr);
      if (box == nullptr) {
        return;
      }
      seen = true;
      CHECK(box->objectName() == QStringLiteral("activateComponentQuestion"));
#ifdef Q_OS_MACOS
      // QMessageBox ignores window titles on macOS, including with the
      // offscreen plugin; keep checking the platform's exact behavior.
      CHECK(box->windowTitle().isEmpty());
#else
      CHECK(box->windowTitle() == QStringLiteral("Activate Component"));
#endif
      CHECK(box->icon() == QMessageBox::Question);
      CHECK(box->text() == QStringLiteral("The face belongs to M6 rod bracket:1, but Corner brace:1 is active."));
      CHECK(box->informativeText() ==
            QStringLiteral("If you activate it, the sketch goes into M6 rod bracket:1. If you keep Corner brace:1 "
                           "active, the sketch goes into Corner brace:1."));
      QPushButton* activateButton = nullptr;
      QPushButton* keepButton = nullptr;
      for (QAbstractButton* button : box->buttons()) {
        if (button->text() == QStringLiteral("&Activate M6 rod bracket:1")) {
          activateButton = qobject_cast<QPushButton*>(button);
        } else if (button->text() == QStringLiteral("&Keep in Corner brace:1")) {
          keepButton = qobject_cast<QPushButton*>(button);
        }
      }
      CHECK(activateButton != nullptr && keepButton != nullptr);
      CHECK(box->defaultButton() == activateButton);
      CHECK(box->escapeButton() == keepButton);
      CHECK(box->checkBox() != nullptr && box->checkBox()->text() == QStringLiteral("&Don't ask again"));
      CHECK(box->checkBox() != nullptr && !box->checkBox()->isChecked());
      if (box->checkBox() != nullptr) {
        box->checkBox()->setChecked(dontAsk);
      }
      QPushButton* pressed = activate ? activateButton : keepButton;
      if (pressed != nullptr) {
        pressed->click();
      } else {
        box->reject();
      }
    });
    const ActivateAnswer answer = askToActivate(nullptr, question);
    CHECK(seen);
    return answer;
  };
  const ActivateAnswer activated = answerWith(true, false);
  CHECK(activated.activate && !activated.dontAskAgain);
  const ActivateAnswer kept = answerWith(false, true);
  CHECK(!kept.activate && kept.dontAskAgain);
}

// The settings file the app reads in a settings folder (QSettings's INI
// files of the user scope: <folder>/<organization>/<application>.ini).
QString settingsFile(const QString& folder) {
  return QDir(folder).filePath(QCoreApplication::organizationName() + QLatin1Char('/') +
                               QCoreApplication::applicationName() + QStringLiteral(".ini"));
}

// A restart of the app with the same settings file: the file as saved,
// copied into a fresh folder that the settings are then read from. A new
// folder, because QSettings keeps a file it has read in memory for the
// rest of the process; a new start reads it from the disk.
void restart(const QString& from, const QString& to) {
  QSettings().sync();
  const QString saved = settingsFile(from);
  const QString copied = settingsFile(to);
  CHECK(QFileInfo::exists(saved));
  CHECK(QDir().mkpath(QFileInfo(copied).path()));
  CHECK(QFile::copy(saved, copied));
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, to);
  CHECK(QSettings().fileName() == copied);
}

// The preference is read back after a restart (mitcad#1190: saved as
// "general/askActivateComponent", an INI file read it back as unset), and
// a value saved under the old key is moved to its own group once.
void settingsTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("activate_question"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir first;
  QTemporaryDir second;
  QTemporaryDir third;
  CHECK(first.isValid() && second.isValid() && third.isValid());

  // Don't ask again, then a restart: still off; turned back on, then a
  // restart: on.
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, first.path());
  saveAskToActivateComponent(false);
  CHECK(!askToActivateComponent());
  restart(first.path(), second.path());
  CHECK(!askToActivateComponent());
  {
    const QSettings saved(settingsFile(second.path()), QSettings::IniFormat);
    CHECK(saved.contains(QStringLiteral("components/askActivate")));
    CHECK(!saved.childGroups().contains(QStringLiteral("General")));
    CHECK(!saved.childGroups().contains(QStringLiteral("general")));
  }
  saveAskToActivateComponent(true);
  restart(second.path(), third.path());
  CHECK(askToActivateComponent());

  // The file an earlier version saved on Linux: the group "general" as the
  // section [%General], read back as "General/askActivateComponent".
  QTemporaryDir old;
  QTemporaryDir oldRestarted;
  CHECK(old.isValid() && oldRestarted.isValid());
  const QString oldFile = settingsFile(old.path());
  CHECK(QDir().mkpath(QFileInfo(oldFile).path()));
  {
    QFile file(oldFile);
    CHECK(file.open(QIODevice::WriteOnly));
    file.write("[%General]\naskActivateComponent=false\n");
  }
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, old.path());
  CHECK(!askToActivateComponent());
  restart(old.path(), oldRestarted.path());
  CHECK(!askToActivateComponent());
  {
    const QSettings moved(settingsFile(oldRestarted.path()), QSettings::IniFormat);
    CHECK(moved.contains(QStringLiteral("components/askActivate")));
    CHECK(!moved.contains(QStringLiteral("General/askActivateComponent")));
    CHECK(!moved.childGroups().contains(QStringLiteral("General")));
  }

  // The old key read back as written (the Windows registry, the macOS
  // preferences): moved; a value already under the new key stays.
  QTemporaryDir registry;
  CHECK(registry.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, registry.path());
  QSettings().setValue(QStringLiteral("general/askActivateComponent"), false);
  CHECK(!askToActivateComponent());
  CHECK(QSettings().contains(QStringLiteral("components/askActivate")));
  CHECK(!QSettings().contains(QStringLiteral("general/askActivateComponent")));
  saveAskToActivateComponent(true);
  QSettings().setValue(QStringLiteral("general/askActivateComponent"), false);
  CHECK(askToActivateComponent());
  CHECK(!QSettings().contains(QStringLiteral("general/askActivateComponent")));

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
}

} // namespace

int activateQuestionTests() {
  failures = 0;
  decisionTests();
  placeTests();
  dialogTests();
  settingsTests();
  return failures;
}
