#!/bin/sh
# Renders all three directions × five surfaces × two themes with headless Chrome.
# Resolutions follow the brief: 1440×900 for the main window, 400×600 for the menu
# bar panel. The combined "panels" shot is 1440×900 so Peek + Menu bar + Assistant
# fit side by side. All renders are 2x (--force-device-scale-factor=2).
#
# Usage: ./render.sh [direction] [surface]
#   ./render.sh          — render all
#   ./render.sh a        — render only Direction A
#   ./render.sh b main   — render only Direction B main shot

set -e
cd "$(dirname "$0")"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
mkdir -p out

one() { # name html surface theme w h
  local out="out/$1.png"
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=2 --window-size="$5","$6" \
    --virtual-time-budget=1600 \
    --screenshot="$PWD/$out" "file://$PWD/$2?s=$3&t=$4" >/dev/null 2>&1
  echo "  $out"
}

render_direction() {
  local v=$1 # a|b|c
  local only=$2 # optional single surface
  local html="$v.html"
  local wants="main panels arcade settings translucent"
  [ -n "$only" ] && wants="$only"
  for s in $wants; do
    for t in light dark; do
      local w=1440 h=900
      one "$v-$s-$t" "$html" "$s" "$t" "$w" "$h"
    done
  done
}

VS="${1:-a b c}"
ONLY="${2:-}"
for v in $VS; do
  echo "Direction $v:"
  render_direction "$v" "$ONLY"
done
echo "Done. See out/"
