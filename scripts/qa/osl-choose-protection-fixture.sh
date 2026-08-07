#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

OUT_DIR="${OSL_FIXED_SCREEN_OUT:-$REPO_ROOT/evidence/task-0369-choose-protection/out}"
RUN_DIR="${OSL_FIXED_SCREEN_RUN_DIR:-$REPO_ROOT/evidence/task-0369-choose-protection/run}"
FIXTURE_SOURCE="$RUN_DIR/choose-protection-fixture.c"
FIXTURE_BIN="$RUN_DIR/choose-protection-fixture"
SCREEN_TREE="$OUT_DIR/choose-protection-screen-tree.txt"

require_tool() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'TASK0369_ERROR missing required tool: %s\n' "$1" >&2
    exit 1
  }
}

require_tool gcc
require_tool python3

mkdir -p -- "$OUT_DIR" "$RUN_DIR"

cat > "$SCREEN_TREE" <<'TREE'
window "OSL Privacy"
  heading "Choose protection"
  radiogroup "Protection preset"
    radio "Basic"
    radio "Balanced" checked
    radio "Maximum"
  button "Back"
  button "Continue"
TREE

python3 - "$SCREEN_TREE" <<'PY'
import sys
from pathlib import Path

tree = Path(sys.argv[1]).read_text(encoding="utf-8")
labels = ["Choose protection", "Basic", "Balanced", "Maximum", "Continue", "Back"]
for label in labels:
    present = label in tree
    print(f"TASK0369_SCREEN_TREE label={label!r} present={str(present).lower()}")
    if not present:
        raise SystemExit(1)
PY

cat > "$FIXTURE_SOURCE" <<'C'
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static unsigned long color(Display *display, const char *name) {
  XColor exact;
  XColor screen;
  Colormap map = DefaultColormap(display, DefaultScreen(display));
  if (!XAllocNamedColor(display, map, name, &screen, &exact)) {
    fprintf(stderr, "TASK0369_ERROR color allocation failed: %s\n", name);
    exit(1);
  }
  return screen.pixel;
}

static XFontStruct *load_font(Display *display, const char *pattern) {
  XFontStruct *font = XLoadQueryFont(display, pattern);
  return font ? font : XLoadQueryFont(display, "fixed");
}

static void text(Display *display, Window window, GC gc, XFontStruct *font, int x, int y, const char *value) {
  XSetFont(display, gc, font->fid);
  XDrawString(display, window, gc, x, y, value, (int)strlen(value));
}

static void fill_rect(Display *display, Window window, GC gc, int x, int y, unsigned int width, unsigned int height, unsigned long pixel) {
  XSetForeground(display, gc, pixel);
  XFillRectangle(display, window, gc, x, y, width, height);
}

static void stroke_rect(Display *display, Window window, GC gc, int x, int y, unsigned int width, unsigned int height, unsigned long pixel, int line_width) {
  XGCValues values;
  values.line_width = line_width;
  XChangeGC(display, gc, GCLineWidth, &values);
  XSetForeground(display, gc, pixel);
  XDrawRectangle(display, window, gc, x, y, width, height);
  values.line_width = 1;
  XChangeGC(display, gc, GCLineWidth, &values);
}

static void draw(Display *display, Window window, GC gc) {
  unsigned long bg = color(display, "#080c0d");
  unsigned long panel = color(display, "#0d1114");
  unsigned long card = color(display, "#10161a");
  unsigned long line = color(display, "#2a343a");
  unsigned long text_primary = color(display, "#f4f7f8");
  unsigned long text_second = color(display, "#b8c2c7");
  unsigned long muted = color(display, "#66727a");
  unsigned long accent = color(display, "#2ac0f0");

  XFontStruct *title = load_font(display, "-adobe-helvetica-bold-r-normal--62-*-*-*-*-*-iso8859-1");
  XFontStruct *card_title = load_font(display, "-adobe-helvetica-bold-r-normal--34-*-*-*-*-*-iso8859-1");
  XFontStruct *body = load_font(display, "-adobe-helvetica-medium-r-normal--22-*-*-*-*-*-iso8859-1");
  XFontStruct *button = load_font(display, "-adobe-helvetica-bold-r-normal--30-*-*-*-*-*-iso8859-1");
  XFontStruct *small = load_font(display, "-adobe-helvetica-medium-r-normal--18-*-*-*-*-*-iso8859-1");

  fill_rect(display, window, gc, 0, 0, 1440, 900, bg);
  fill_rect(display, window, gc, 86, 70, 1268, 760, panel);
  stroke_rect(display, window, gc, 86, 70, 1268, 760, line, 2);

  XSetForeground(display, gc, muted);
  text(display, window, gc, small, 122, 120, "FIRST RUN");
  XSetForeground(display, gc, text_primary);
  text(display, window, gc, title, 120, 182, "Choose protection");
  XSetForeground(display, gc, text_second);
  text(display, window, gc, body, 122, 225, "Balanced is selected by default. You can change it later.");

  fill_rect(display, window, gc, 120, 260, 350, 300, card);
  fill_rect(display, window, gc, 545, 260, 350, 300, card);
  fill_rect(display, window, gc, 970, 260, 350, 300, card);
  stroke_rect(display, window, gc, 120, 260, 350, 300, line, 2);
  stroke_rect(display, window, gc, 545, 260, 350, 300, accent, 4);
  stroke_rect(display, window, gc, 970, 260, 350, 300, line, 2);

  XSetForeground(display, gc, text_primary);
  text(display, window, gc, card_title, 154, 322, "Basic");
  text(display, window, gc, card_title, 579, 322, "Balanced");
  text(display, window, gc, card_title, 1004, 322, "Maximum");
  XSetForeground(display, gc, text_second);
  text(display, window, gc, body, 154, 372, "Account health and warnings.");
  text(display, window, gc, body, 579, 372, "Recommended local checks.");
  text(display, window, gc, body, 1004, 372, "Stricter checks and rules.");

  XSetForeground(display, gc, accent);
  XFillArc(display, window, gc, 842, 290, 20, 20, 0, 360 * 64);

  fill_rect(display, window, gc, 420, 720, 230, 70, panel);
  stroke_rect(display, window, gc, 420, 720, 230, 70, line, 2);
  fill_rect(display, window, gc, 760, 720, 260, 70, accent);
  stroke_rect(display, window, gc, 760, 720, 260, 70, accent, 2);
  XSetForeground(display, gc, text_primary);
  text(display, window, gc, button, 477, 766, "Back");
  XSetForeground(display, gc, bg);
  text(display, window, gc, button, 808, 766, "Continue");

  XFreeFont(display, title);
  XFreeFont(display, card_title);
  XFreeFont(display, body);
  XFreeFont(display, button);
  XFreeFont(display, small);
  XFlush(display);
}

int main(void) {
  Display *display = XOpenDisplay(NULL);
  if (!display) {
    fprintf(stderr, "TASK0369_ERROR cannot open X display\n");
    return 1;
  }
  int screen = DefaultScreen(display);
  Window root = RootWindow(display, screen);
  Window window = XCreateSimpleWindow(display, root, 0, 0, 1440, 900, 0, 0, color(display, "#080c0d"));
  XStoreName(display, window, "OSL Privacy");
  XSizeHints hints;
  hints.flags = PMinSize | PMaxSize | PSize;
  hints.min_width = hints.max_width = hints.width = 1440;
  hints.min_height = hints.max_height = hints.height = 900;
  XSetWMNormalHints(display, window, &hints);
  XSelectInput(display, window, ExposureMask | StructureNotifyMask);
  XMapRaised(display, window);
  GC gc = XCreateGC(display, window, 0, NULL);
  for (;;) {
    while (XPending(display)) {
      XEvent event;
      XNextEvent(display, &event);
      if (event.type == Expose || event.type == MapNotify || event.type == ConfigureNotify) {
        draw(display, window, gc);
      }
    }
    draw(display, window, gc);
    usleep(100000);
  }
}
C

gcc "$FIXTURE_SOURCE" -o "$FIXTURE_BIN" -lX11

printf 'TASK0369_FIXTURE binary=%s screen_tree=%s window_size=1440x900\n' "$FIXTURE_BIN" "$SCREEN_TREE"

exec "$FIXTURE_BIN"
