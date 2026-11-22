<!-- SPDX-License-Identifier: MIT -->
# A view on a window-less EGL context: 2031

mitcad#2031: MitCad running natively on Wayland (OCCT's OpenGL driver
through EGL). On a GNOME Wayland session with an NVIDIA driver the first
frame of the view, and the first after every resize, logged "MSAA texture
… cannot be created with error GL_INVALID_OPERATION" and "Main FBO …
initialization has failed".
Back to the [port's README](../README.md).

## Overview

- **2031** makes `OpenGl_Window::init()` select the colour buffer of the
  offscreen default FBO it makes for a window-less EGL context, instead of
  `GL_BACK`, which such a context does not have. GLX, WGL, EGL with a
  surface and OpenGL ES are unchanged.

## The case

On Wayland, Qt makes `QOpenGLWidget`'s context current on its offscreen
surface, which is window-less there (`EGL_KHR_surfaceless_context`, no
`EGLSurface`). MitCad's view names Qt's EGL display and context in its
driver (`QtEglGraphicDriver` in `app/OcctViewer.cpp`) with no EGL config,
so OCCT makes no pbuffer: `OpenGl_Window::init()` makes an offscreen
default FBO and binds it. It then called `SetDrawBuffer(GL_BACK)`, which
binds framebuffer 0 again and calls `glDrawBuffer(GL_BACK)`. A
window-less context has no window framebuffer, so NVIDIA's driver
(580.178.04) raises `GL_INVALID_OPERATION`.

OCCT does not read that error there. It stays pending until the next
`glGetError()`, which is the one in `OpenGl_Texture::Init2DMultisample`
after `glTexStorage2DMultisample`: that call succeeds (the same call on a
fresh window-less context gives no error, `GL_SRGB8_ALPHA8` with 4
samples included), but the pending error makes OCCT drop the 4-sample
scene FBO for that frame. `OpenGl_Window::Resize` runs `init()` again, so
every resize repeated it. The next frame made the FBO, and the rest of
the window (drawn by Qt's own context, which OCCT does not touch) was not
affected.

## 2031: the window-less default FBO's colour buffer

### 2031-tkopengl-window-less-egl-context-draws-into-its-default-fbo-not-the-back-buffer

`OpenGl_Window::init()` remembers that it made the offscreen default FBO
(the window-less EGL branch) and then selects `GL_COLOR_ATTACHMENT0`,
which keeps that FBO bound; otherwise `GL_BACK` as before. Upstream's
comment says the branch is never called there, because OCCT makes a
pbuffer when it has an EGL config; an application that brings its own
window-less context without a config reaches it.

## Reports and tests

- No test with OCCT alone in `geometry/tests/`: the defect needs a
  window-less EGL context of a driver that refuses `GL_BACK` there, which
  those tests do not create (Mesa on a virtual X display gives Qt a
  pbuffer).
- Checked with a reproducer of OCCT alone on NVIDIA 580.178.04 (an EGL
  display of a Wayland compositor, a desktop OpenGL 4.6 compatibility
  context current with `EGL_NO_SURFACE`, an `OpenGl_GraphicDriver` naming
  that display and context, a `V3d_View` with 4 MSAA samples on a virtual
  `Aspect_NeutralWindow`, an application FBO wrapped as the default one).
  Without the patch: `GL_INVALID_OPERATION` pending after `SetWindow`,
  framebuffer 0 bound, a `KHR_debug` callback placing it in `glDrawBuffer`
  from `OpenGl_Context::SetDrawBuffer` from `OpenGl_Window::init`, and both
  messages at the first frame and after a resize. With it: no error and
  the scene FBO made at both sizes.
- MitCad on a headless GNOME Shell Wayland compositor on the same GPU:
  the messages gone, the view and the window drawn.
