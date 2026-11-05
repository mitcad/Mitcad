# macOS trackpad and Magic Mouse checklist

A manual test of the view's trackpad navigation (`NavigationScheme::Trackpad`,
`OcctViewer::trackpadScroll` and `OcctViewer::nativeGesture`) on a Mac. Run
first with the default Qt frontend (`--ui=qt`), then repeat with the opt-in
native frontend (`--ui=native`). Record the effective frontend and Navigation
preset for each run. Open a
design with a few bodies and a zoom level where you can see the model's edges
(Fit first). Preferences > Navigation: "Trackpad ..." (the default on macOS;
delete the `view/navigationScheme` setting to see the default again). Try it
with System Settings > Trackpad > Natural scrolling on and off: the content
should follow the fingers when it is on, and move the other way when it is
off.

## MacBook trackpad, Trackpad scheme

- [ ] Two-finger scroll pans in all directions; the content keeps pace with
      the fingers and does not stutter or lag behind.
- [ ] Lifting the fingers at speed: the pan coasts (momentum) and stops
      without a jump; touching the trackpad stops it.
- [ ] Option + two-finger scroll orbits about the same centre a mouse
      orbit uses. Check that Option does not swap the horizontal and the
      vertical direction (if it does, the orbit goes sideways for a vertical
      scroll); horizontal scroll turns about the vertical axis.
- [ ] Orbit with "Constrained orbit (Z stays up)" on: Z stays up.
- [ ] Letting go of Option while the orbit coasts: the coasting continues as
      an orbit, it does not turn into a pan.
- [ ] Shift + two-finger scroll zooms (up: in, down: out, with the
      reverse-zoom setting flipping it); Command + scroll the same. Zoom
      about the cursor follows the "Zoom about the cursor" setting.
- [ ] Pinch zooms about the fingers; spreading zooms in. One full pinch
      across the trackpad zooms by a factor of about 2-3, not by 10 or by
      1.01 (the scale is `kPinchZoomSteps`). The reverse-zoom setting does
      not change it.
- [ ] Two-finger rotation rolls the view and the model turns with the fingers
      (clockwise fingers, clockwise model; flip `kRollDirection` if not). Not
      with the constrained orbit.
- [ ] Two-finger double tap fits the whole design.
- [ ] In the native frontend, a pinch starting on the canvas keeps zooming
      when the pointer crosses a floating card. A pinch starting on a card
      leaves the canvas unchanged, even if the pointer crosses onto it.
- [ ] Option + left drag orbits; Shift + Option + left drag pans; a middle drag (if
      there is a mouse) pans; a right drag orbits.
- [ ] Left click selects; left drag selects with a window; Shift and
      Command + click add to the selection as in the other schemes.
- [ ] Two-finger click (right click) and Control + click on a body, a face
      and an empty spot open the context menu on release; after a drag of
      more than a few pixels they do not.
- [ ] Dismiss context menus on the canvas, orientation cube, Browser and
      history with Escape or an outside click, and choose an action from
      each. Moving the pointer afterwards leaves the camera still; the
      next right drag orbits normally. Repeat after a drag crosses a card
      and after switching applications while holding the mouse button.
- [ ] In a sketch with a sketch tool active: two-finger scroll pans,
      Option + scroll orbits, pinch zooms, and the left button keeps
      drawing.
- [ ] The Keyboard and Mouse Overview (Help) lists the Trackpad rows with
      the Option and Command glyphs.

## Command-click release ordering

Use two distinct visible edges on a simple solid. These checks require an
actual trackpad: the synthetic release-order tests exercise application
events and do not establish physical-input acceptance.

- [ ] Command-click the two edges before starting Fillet, then open Fillet:
      both edges are retained. Cancel and repeat with Chamfer.
- [ ] Start Fillet with no selection and Command-click both edges. Repeat
      with Chamfer. Release Command before the trackpad click in one run,
      and release the click before Command in another.
- [ ] Repeat immediately after an intentional orbit (Option-drag and the
      preset's two-finger orbit), both after an ordinary stop and after
      momentum. Lifting the fingers ends the gesture; after both the click
      and Command are released, pointer motion leaves the camera still and
      the next click selects normally.
- [ ] Repeat with Trackpad, Shift orbit, and a saved Custom navigation table.
      Intentional orbit and the saved bindings keep working. Record any
      failing modifier/button/scroll sequence separately from a screenshot.

## MacBook trackpad, Shift orbit scheme

- [ ] Choose "Trackpad (two-finger scroll pans, pinch zooms, Shift orbits)"
      in Navigation, close Settings and reopen it: the choice is retained,
      including after restarting the app.
- [ ] Shift + two-finger scroll orbits in both directions; releasing Shift
      during momentum keeps orbiting. Repeat with constrained orbit on.
- [ ] Option or Command + scroll zooms; plain scroll pans and pinch zooms.
      Mouse button shortcuts and Shift + click selection still work as in
      the default Trackpad scheme.
- [ ] The Keyboard and Mouse Overview lists Shift for scroll orbit and
      Option or Command for scroll zoom.
- [ ] Switching to the default Trackpad scheme restores Option orbit and
      Shift zoom, including when switching during momentum.

## MacBook trackpad, the other schemes

- [ ] Choose "Middle pans, Shift + middle orbits": pinch zooms, rotation
      rolls, a double tap fits; a two-finger scroll zooms as it did before
      (it is not a pan); nothing else changed.
- [ ] Same with Option + left orbits, and with right orbits.

## Magic Mouse

- [ ] Trackpad scheme: a one-finger swipe along the surface pans (it is a
      continuous scroll with phases); Option + swipe orbits; Shift + swipe
      zooms.
- [ ] A double tap with one finger fits (smart zoom), where the mouse sends it.
- [ ] Other schemes: the swipe zooms as before.

## Ordinary mouse with a wheel

- [ ] Trackpad scheme: the wheel zooms about the cursor in steps, as in the
      other schemes (a wheel is not mistaken for a trackpad scroll); the
      reverse-zoom setting works; buttons: middle pans, right orbits.
- [ ] Switching schemes in Preferences while a pan is coasting does not
      leave the view stuck in a drag.

## Glass interface (floating chrome)

Needs hardware (the build VM has no GPU) and macOS 26 or later for the glass;
repeat the items with `--no-glass` where it says so. Open a design with a few
bodies.

- [ ] The glass cards (Browser, Command, Timeline) blur the real,
      GPU-rendered model behind them; the blur follows the model while
      orbiting, panning and zooming, without flicker, tearing or lag behind
      the model, and the frame rate is as with `--chrome=docked` (compare by
      eye, or with `MITCAD_LOG_TIMING=1`).
- [ ] Clicking a card (its header, a tree row, a field) keeps the main
      window active: the traffic lights stay coloured, the title row does not
      grey out, the menu bar stays MitCad's.
- [ ] With viewport focus, the configured command shortcuts work (defaults:
      E, S and L to start commands, Command+Z to undo, Command+Return to
      Finish Sketch). In a text editor, ordinary typing and text editing
      retain focus; command keys never delete or replace model items.
- [ ] Reassign Finish Sketch and clear its binding: sketch hints show the
      current native key name, or just the command name when cleared.
      Restore defaults and accept without a conflict warning.
- [ ] Model and sketch Move/Copy share M in their separate contexts.
      Assigning M to Extrude or Revolve is rejected because they also run
      from a sketch. Command+S/W, F6 while Fit keeps its default, and
      Command+M/H cannot be assigned to unrelated commands.
- [ ] Native Browser and history: F2 starts rename, Return retains its
      activation/edit action, Backspace and forward Delete delete the
      selection only while the view has focus. In the rename editor those
      keys edit text. Command+F opens search; Escape restores the preceding
      selection and scroll position.
- [ ] Save with Description: Command+Return saves; plain Return inserts a
      newline. Its Save tooltip uses the native key sequence.
- [ ] In the Command card: Tab moves between the fields, Return accepts (OK),
      Escape cancels; the typed value is not lost when the focus leaves.
- [ ] A click on the view just outside a card's rounded corner and on its
      shadow reaches the view (it picks, a drag orbits); a click inside the
      corner does not.
- [ ] Minimizing the window (yellow button, Cmd+M) takes the cards with it,
      and restoring brings them back in the same places; none stays behind.
- [ ] Full screen (green button) and back: the cards follow, are not
      duplicated, and are in the same Space as the window.
- [ ] Spaces and Mission Control: the cards move with the window; Mission
      Control and Cmd+` list the window once (no card windows), and the Window
      menu has no card entries.
- [ ] Moving and resizing the window keeps the cards in place relative to it
      (no trailing). Collapsing the Browser and showing/hiding the cards from
      View keep the model's fit and the orientation cube out from under them.
- [ ] The native feature timeline uses coloured CAD icons in a compact
      horizontal row. Its marker sits between features and rolls the model
      back and forward; one drag creates one undo step. Rolled-back and
      suppressed features stay identifiable. Horizontal scrolling keeps
      long histories usable at compact window sizes.
- [ ] Timeline selection, Command/Shift selection, double-click editing,
      context menus, feature reordering and keyboard marker movement work
      with the native history card. Sheets and active commands block model
      changes without leaving a marker drag transaction open.
- [ ] Switching light and dark appearance (System Settings, or Auto at sunset)
      while the application runs: the cards, ribbon, icons and the view
      background (Follow System) change at once.
- [ ] Settings (Cmd+,): the window opens with the panes General (with the
      3D Print slicer), Navigation, Display; changes apply at once; closing it and reopening keeps
      them; it comes to the front instead of opening a second one.
- [ ] Sheets: Save before closing, the Parameters dialog and message boxes
      hang from the title bar and block only that window; the cards cannot
      be used meanwhile, the others can.
- [ ] MitCad › About MitCad shows the standard About panel with the version
      and the credits.
- [ ] The menu bar's menus have no icons, while the ribbon's group menus
      do.
- [ ] `--no-glass`: the same cards paint their own translucent material, with
      the same behaviour as above.
