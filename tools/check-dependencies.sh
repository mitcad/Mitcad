#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Checks the Rust dependencies against core/deny.toml: known vulnerabilities
# (fetches the RustSec advisory database), licences, banned crates and sources.
set -euo pipefail
cd "$(dirname "$0")/../core"
cargo deny --locked check
