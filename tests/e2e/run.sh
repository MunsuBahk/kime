#!/bin/sh
# Build the frontends, then run the full e2e suite serially.
set -eu
cd "$(dirname "$0")/../.."
ninja -C build
exec cargo test -p kime-e2e -- --ignored --test-threads=1
