#!/usr/bin/env bash
set -euo pipefail

echo "No automated test suite is configured for LANDrop." >&2
echo "Add a Qt Test target (or another real suite) and update scripts/test.sh before treating tests as available." >&2
exit 2
