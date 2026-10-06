// SPDX-License-Identifier: MIT
#pragma once

// Thin OCCT facade used by the Rust model through the CXX bridge
// (core/ffi/src/kernel/) and by the application for display. One header per
// operation family; lengths are millimetres and angles radians. All
// operations throw std::exception subclasses on invalid input or kernel
// failure.

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/datum.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
// Transforms, patterns, mirrors, combine and primitives (F4).
#include "mitcad/geometry/pattern.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/transform.hpp"
// Face operations (F2).
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/split.hpp"
#include "mitcad/geometry/tool.hpp"
// Profile features (F1).
#include "mitcad/geometry/hole.hpp"
#include "mitcad/geometry/reference.hpp"
#include "mitcad/geometry/revolve.hpp"
#include "mitcad/geometry/target.hpp"
#include "mitcad/geometry/thread.hpp"
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/rib.hpp"
