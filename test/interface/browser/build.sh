#!/usr/bin/env bash
set -euo pipefail
exec bash "$(dirname "$0")/../dom/browser/build.sh" corner-listener
