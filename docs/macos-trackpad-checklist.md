# macOS trackpad and Magic Mouse checklist

A manual test of the view's trackpad navigation (`NavigationScheme::Trackpad`,
`OcctViewer::trackpadScroll` and `OcctViewer::nativeGesture`) on a Mac. Open a
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
- [ ] Alt (Option) + two-finger scroll orbits about the same centre a mouse
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
- [ ] Alt + left drag orbits; Shift + Alt + left drag pans; a middle drag (if
      there is a mouse) pans; a right drag orbits.
- [ ] Left click selects; left drag selects with a window; Shift and
      Command + click add to the selection as in the other schemes.
- [ ] Two-finger click (right click) and Control + click on a body, a face
      and an empty spot open the context menu on release; after a drag of
      more than a few pixels they do not.
- [ ] In a sketch with a sketch tool active: two-finger scroll pans,
      Option + scroll orbits, pinch zooms, and the left button keeps
      drawing.
- [ ] The Keyboard and Mouse Overview (Help) lists the Trackpad rows with
      the Option and Command glyphs.

## MacBook trackpad, the other schemes

- [ ] Choose "Middle pans, Shift + middle orbits": pinch zooms, rotation
      rolls, a double tap fits; a two-finger scroll zooms as it did before
      (it is not a pan); nothing else changed.
- [ ] Same with Alt + left orbits, and with right orbits.

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
      grey out, the menu bar stays Mitcad's.
- [ ] Keyboard shortcuts work while a field in a card has the focus (E, S, L
      to start a command, Cmd+Z, Ctrl+Enter to accept), and while no field
      has it.
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
- [ ] Switching light and dark appearance (System Settings, or Auto at sunset)
      while the application runs: the cards, ribbon, icons and the view
      background (Follow System) change at once.
- [ ] Settings (Cmd+,): the window opens with the panes General (with the
      3D Print slicer), Navigation, Display; changes apply at once; closing it and reopening keeps
      them; it comes to the front instead of opening a second one.
- [ ] Sheets: Save before closing, the Parameters dialog and message boxes
      hang from the title bar and block only that window; the cards cannot
      be used meanwhile, the others can.
- [ ] Mitcad › About Mitcad shows the standard About panel with the version
      and the credits.
- [ ] The menu bar's menus have no icons, while the ribbon's group menus
      do.
- [ ] `--no-glass`: the same cards paint their own translucent material, with
      the same behaviour as above.
