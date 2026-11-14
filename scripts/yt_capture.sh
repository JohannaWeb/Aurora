#!/usr/bin/env bash
# Record the scorecard pages (scripts/scorecard/pages.txt) from the live site.
#
#   scripts/yt_capture.sh [name...]
#
# Each page is recorded logged out with AURORA_RECORD into
# $AURORA_CAPTURES/<name>/ (default .captures/), replacing an older capture.
# Recordings hold anonymous visitor cookies and IP-derived locale; keep them
# out of the repository.
set -euo pipefail
cd "$(dirname "$0")/.."
captures=${AURORA_CAPTURES:-.captures}
binary=${AURORA_BIN:-target/release/aurora}
[ -x "$binary" ] || cargo build --release

grep -vE '^\s*(#|$)' scripts/scorecard/pages.txt | while read -r name url; do
    if [ $# -gt 0 ] && [[ ! " $* " == *" $name "* ]]; then
        continue
    fi
    dir="$captures/$name"
    rm -rf "$dir"
    mkdir -p "$dir"
    printf '{"url": "%s", "recorded": "%s"}\n' "$url" "$(date -u +%FT%TZ)" > "$dir/capture.json"
    echo "recording $name: $url"
    AURORA_RECORD="$dir" AURORA_SCREENSHOT="$dir/live.png" \
        timeout "${AURORA_CAPTURE_TIMEOUT:-300}" "$binary" "$url" > "$dir/record.log" 2>&1 \
        || echo "  $name exited with status $? (see $dir/record.log)"
    echo "  $(wc -l < "$dir/index.jsonl") requests, $(du -sh "$dir" | cut -f1)"
done
