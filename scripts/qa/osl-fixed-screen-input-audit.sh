#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

SCREEN="${OSL_FIXED_SCREEN_SIZE:-1440x900x24}"
DISPLAY_ID="${OSL_FIXED_SCREEN_DISPLAY:-:$((90 + ($$ % 20)))}"
OUT_DIR="${OSL_INPUT_AUDIT_OUT:-$REPO_ROOT/evidence/task-3546-early-screen-input/out}"
RUN_DIR="${OSL_INPUT_AUDIT_RUN_DIR:-$REPO_ROOT/evidence/task-3546-early-screen-input/run}"
WINDOW_NAME="${OSL_FIXED_SCREEN_WINDOW_NAME:-OSL Privacy}"
WAIT_SECONDS="${OSL_INPUT_AUDIT_WAIT_SECONDS:-15}"
SKIP_AFTER="${OSL_INPUT_AUDIT_SKIP_AFTER:-0}"
EARLY_WRONG="${OSL_INPUT_AUDIT_EARLY_WRONG:-0}"
LOADING_MS="${OSL_INPUT_AUDIT_LOADING_MS:-1500}"
AFTER_MS="${OSL_INPUT_AUDIT_AFTER_MS:-4500}"

STATUS_PATH="$RUN_DIR/input-audit.status"
START_PATH="$RUN_DIR/input-audit.start"
RECORDS_PATH="$OUT_DIR/input-audit-records.tsv"
REPORT_PATH="$OUT_DIR/input-audit-report.md"
FIXTURE_SOURCE="$RUN_DIR/input-audit-fixture.c"
FIXTURE_BIN="$RUN_DIR/input-audit-fixture"

XVFB_PID=""
WM_PID=""
APP_PID=""
CLEANED_UP=0

die() {
  printf 'TASK3546_ERROR %s\n' "$*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"
}

cleanup() {
  if [ "$CLEANED_UP" -eq 1 ]; then
    return
  fi
  CLEANED_UP=1
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    if [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
  done
  for pid in "$APP_PID" "$WM_PID" "$XVFB_PID"; do
    if [ -n "$pid" ]; then
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
  if [ -n "$XVFB_PID" ] && kill -0 "$XVFB_PID" >/dev/null 2>&1; then
    fake_screen_alive=yes
  else
    fake_screen_alive=no
  fi
  printf 'TASK3546_CLEANUP fake_screen_alive=%s display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
    "$fake_screen_alive" "$DISPLAY_ID" "${XVFB_PID:-none}" "${WM_PID:-none}" "${APP_PID:-none}"
}
trap cleanup EXIT

require_tool Xvfb
require_tool matchbox-window-manager
require_tool xdotool
require_tool xwininfo
require_tool gcc
require_tool python3

rm -rf -- "$OUT_DIR" "$RUN_DIR"
mkdir -p -- "$OUT_DIR" "$RUN_DIR"

cat > "$FIXTURE_SOURCE" <<'C'
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <unistd.h>

typedef struct {
  const char *id;
  const char *title;
  const char *target_id;
  const char *target_label;
  const char *sequence;
} ScreenSpec;

typedef struct {
  char early_control[64];
  char early_text[128];
  char after_control[64];
  char after_text[128];
  int early_actions;
} ScreenRecord;

static const ScreenSpec screens[] = {
  {"welcome", "Welcome", "welcome-safe-entry", "Welcome setup input", "osl3546welcomea1"},
  {"choose-protection", "Choose protection", "choose-protection-preset", "Protection preset input", "osl3546protectb2"},
  {"choose-apps", "Choose apps", "choose-apps-picker", "App picker input", "osl3546appsc3"},
  {"choose-send", "Choose how Send works", "choose-send-mode", "Send mode input", "osl3546sendd4"},
  {"review-defaults", "Review defaults", "review-defaults-policy", "Defaults review input", "osl3546defaultse5"},
  {"secure-recovery", "Secure recovery", "secure-recovery-proof", "Recovery proof input", "osl3546recoveryf6"},
};

static const int screen_count = (int)(sizeof(screens) / sizeof(screens[0]));

static long now_ms(void) {
  struct timeval tv;
  gettimeofday(&tv, NULL);
  return (long)tv.tv_sec * 1000L + (long)tv.tv_usec / 1000L;
}

static unsigned long color(Display *display, const char *name) {
  XColor exact;
  XColor screen;
  Colormap map = DefaultColormap(display, DefaultScreen(display));
  if (!XAllocNamedColor(display, map, name, &screen, &exact)) {
    fprintf(stderr, "TASK3546_FIXTURE_ERROR color allocation failed: %s\n", name);
    exit(1);
  }
  return screen.pixel;
}

static XFontStruct *load_font(Display *display, const char *pattern) {
  XFontStruct *font = XLoadQueryFont(display, pattern);
  return font ? font : XLoadQueryFont(display, "fixed");
}

static void draw_text(Display *display, Window window, GC gc, XFontStruct *font, int x, int y, const char *value) {
  XSetFont(display, gc, font->fid);
  XDrawString(display, window, gc, x, y, value, (int)strlen(value));
}

static void put_status(const char *path, const ScreenSpec *spec, const char *phase) {
  FILE *file = fopen(path, "w");
  if (!file) return;
  fprintf(file, "%s\t%s\t%s\n", spec->id, phase, spec->sequence);
  fclose(file);
}

static void append_char(char *dest, size_t size, char c) {
  size_t len = strlen(dest);
  if (len + 1 >= size) return;
  dest[len] = c;
  dest[len + 1] = '\0';
}

static void draw(Display *display, Window window, GC gc, const ScreenSpec *spec, const ScreenRecord *record, bool loaded) {
  unsigned long bg = color(display, "#080c0d");
  unsigned long panel = color(display, "#11181d");
  unsigned long field = color(display, loaded ? "#f4f7f8" : "#26323a");
  unsigned long line = color(display, loaded ? "#2ac0f0" : "#66727a");
  unsigned long text = color(display, "#f4f7f8");
  unsigned long muted = color(display, "#a6b2b8");
  unsigned long ink = color(display, "#080c0d");

  XFontStruct *title = load_font(display, "-adobe-helvetica-bold-r-normal--48-*-*-*-*-*-iso8859-1");
  XFontStruct *body = load_font(display, "-adobe-helvetica-medium-r-normal--22-*-*-*-*-*-iso8859-1");
  XFontStruct *mono = load_font(display, "fixed");

  XSetForeground(display, gc, bg);
  XFillRectangle(display, window, gc, 0, 0, 1440, 900);
  XSetForeground(display, gc, panel);
  XFillRectangle(display, window, gc, 120, 95, 1200, 710);
  XSetForeground(display, gc, text);
  draw_text(display, window, gc, title, 170, 190, spec->title);
  XSetForeground(display, gc, muted);
  draw_text(display, window, gc, body, 170, 245, loaded ? "Loading finished. The documented target owns keyboard input." : "Loading. Keyboard input must not start an action or land in another control.");
  draw_text(display, window, gc, body, 170, 295, spec->target_label);

  XSetForeground(display, gc, field);
  XFillRectangle(display, window, gc, 170, 330, 760, 72);
  XSetForeground(display, gc, line);
  XDrawRectangle(display, window, gc, 170, 330, 760, 72);
  XSetForeground(display, gc, loaded ? ink : muted);
  draw_text(display, window, gc, mono, 190, 374, loaded ? record->after_text : "target not ready");

  XSetForeground(display, gc, muted);
  draw_text(display, window, gc, body, 170, 475, "Documented target id:");
  draw_text(display, window, gc, mono, 420, 475, spec->target_id);
  draw_text(display, window, gc, body, 170, 525, "Audit sequence:");
  draw_text(display, window, gc, mono, 420, 525, spec->sequence);

  XFreeFont(display, title);
  XFreeFont(display, body);
  XFreeFont(display, mono);
  XFlush(display);
}

static void write_records(const char *path, const ScreenRecord *records) {
  FILE *file = fopen(path, "w");
  if (!file) {
    fprintf(stderr, "TASK3546_FIXTURE_ERROR cannot write records\n");
    exit(1);
  }
  fprintf(file, "screen\ttitle\ttarget_id\ttarget_label\tsequence\tearly_control\tearly_text\tafter_control\tafter_text\tearly_actions\n");
  for (int i = 0; i < screen_count; i++) {
    fprintf(file, "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%d\n",
      screens[i].id,
      screens[i].title,
      screens[i].target_id,
      screens[i].target_label,
      screens[i].sequence,
      records[i].early_control,
      records[i].early_text,
      records[i].after_control,
      records[i].after_text,
      records[i].early_actions);
  }
  fclose(file);
}

int main(int argc, char **argv) {
  if (argc != 7) {
    fprintf(stderr, "usage: fixture STATUS START RECORDS EARLY_WRONG LOADING_MS AFTER_MS\n");
    return 2;
  }
  const char *status_path = argv[1];
  const char *start_path = argv[2];
  const char *records_path = argv[3];
  const bool early_wrong = strcmp(argv[4], "1") == 0;
  const long loading_ms = atol(argv[5]);
  const long after_ms = atol(argv[6]);

  ScreenRecord records[sizeof(screens) / sizeof(screens[0])];
  memset(records, 0, sizeof(records));
  for (int i = 0; i < screen_count; i++) {
    strcpy(records[i].early_control, "absent");
    strcpy(records[i].after_control, "none");
  }

  Display *display = XOpenDisplay(NULL);
  if (!display) {
    fprintf(stderr, "TASK3546_FIXTURE_ERROR cannot open X display\n");
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
  XSelectInput(display, window, ExposureMask | StructureNotifyMask | KeyPressMask | FocusChangeMask);
  XMapRaised(display, window);
  GC gc = XCreateGC(display, window, 0, NULL);

  while (access(start_path, F_OK) != 0) {
    while (XPending(display)) {
      XEvent event;
      XNextEvent(display, &event);
    }
    usleep(20000);
  }

  for (int index = 0; index < screen_count; index++) {
    bool loaded = false;
    long phase_start = now_ms();
    put_status(status_path, &screens[index], "loading");
    while (now_ms() - phase_start < loading_ms) {
      while (XPending(display)) {
        XEvent event;
        XNextEvent(display, &event);
        if (event.type == Expose || event.type == MapNotify || event.type == ConfigureNotify) {
          draw(display, window, gc, &screens[index], &records[index], loaded);
        }
        if (event.type == KeyPress) {
          char buf[8] = {0};
          KeySym sym = NoSymbol;
          XLookupString(&event.xkey, buf, (int)sizeof(buf), &sym, NULL);
          if (sym == XK_Return || sym == XK_space) records[index].early_actions++;
          if (buf[0] >= 32 && buf[0] <= 126) {
            if (early_wrong) {
              strcpy(records[index].early_control, "loading-search");
              append_char(records[index].early_text, sizeof(records[index].early_text), buf[0]);
            }
          }
        }
      }
      draw(display, window, gc, &screens[index], &records[index], loaded);
      usleep(20000);
    }

    loaded = true;
    strcpy(records[index].after_control, screens[index].target_id);
    phase_start = now_ms();
    put_status(status_path, &screens[index], "loaded");
    while (now_ms() - phase_start < after_ms && strlen(records[index].after_text) < strlen(screens[index].sequence)) {
      while (XPending(display)) {
        XEvent event;
        XNextEvent(display, &event);
        if (event.type == Expose || event.type == MapNotify || event.type == ConfigureNotify) {
          draw(display, window, gc, &screens[index], &records[index], loaded);
        }
        if (event.type == KeyPress) {
          char buf[8] = {0};
          KeySym sym = NoSymbol;
          XLookupString(&event.xkey, buf, (int)sizeof(buf), &sym, NULL);
          if (buf[0] >= 32 && buf[0] <= 126 && strlen(records[index].after_text) < strlen(screens[index].sequence)) {
            append_char(records[index].after_text, sizeof(records[index].after_text), buf[0]);
          }
        }
      }
      draw(display, window, gc, &screens[index], &records[index], loaded);
      usleep(20000);
    }
  }

  put_status(status_path, &screens[screen_count - 1], "done");
  write_records(records_path, records);
  XCloseDisplay(display);
  return 0;
}
C

gcc "$FIXTURE_SOURCE" -o "$FIXTURE_BIN" -lX11

printf 'TASK3546_SWITCHES display=%s screen=%s window=%q out=%q skip_after=%s early_wrong=%s loading_ms=%s after_ms=%s\n' \
  "$DISPLAY_ID" "$SCREEN" "$WINDOW_NAME" "$OUT_DIR" "$SKIP_AFTER" "$EARLY_WRONG" "$LOADING_MS" "$AFTER_MS"

Xvfb "$DISPLAY_ID" -screen 0 "$SCREEN" -nolisten tcp &
XVFB_PID=$!
sleep 0.4
if ! kill -0 "$XVFB_PID" >/dev/null 2>&1; then
  die "Xvfb exited before the input audit could run"
fi

DISPLAY="$DISPLAY_ID" matchbox-window-manager -use_titlebar no &
WM_PID=$!
sleep 0.2

DISPLAY="$DISPLAY_ID" "$FIXTURE_BIN" "$STATUS_PATH" "$START_PATH" "$RECORDS_PATH" "$EARLY_WRONG" "$LOADING_MS" "$AFTER_MS" &
APP_PID=$!
printf 'TASK3546_LAUNCH display=%s xvfb_pid=%s wm_pid=%s app_pid=%s\n' \
  "$DISPLAY_ID" "$XVFB_PID" "$WM_PID" "$APP_PID"

WINDOW_ID=""
deadline=$((SECONDS + WAIT_SECONDS))
while [ "$SECONDS" -lt "$deadline" ]; do
  if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
    die "input audit fixture exited before creating the fixed-screen window"
  fi
  WINDOW_ID="$(DISPLAY="$DISPLAY_ID" xdotool search --name "$WINDOW_NAME" 2>/dev/null | head -n 1 || true)"
  if [ -n "$WINDOW_ID" ]; then
    break
  fi
  sleep 0.1
done
[ -n "$WINDOW_ID" ] || die "fixed-screen window not found: $WINDOW_NAME"
DISPLAY="$DISPLAY_ID" xdotool windowfocus --sync "$WINDOW_ID"

XWININFO_OUT="$(DISPLAY="$DISPLAY_ID" xwininfo -id "$WINDOW_ID")"
WIDTH="$(printf '%s\n' "$XWININFO_OUT" | awk '/Width:/ {print $2; exit}')"
HEIGHT="$(printf '%s\n' "$XWININFO_OUT" | awk '/Height:/ {print $2; exit}')"
printf 'TASK3546_GEOMETRY width=%s height=%s\n' "${WIDTH:-unknown}" "${HEIGHT:-unknown}"
touch "$START_PATH"

last_phase_key=""
while kill -0 "$APP_PID" >/dev/null 2>&1; do
  if [ -s "$STATUS_PATH" ]; then
    IFS=$'\t' read -r screen_id phase sequence < "$STATUS_PATH" || true
    phase_key="${screen_id:-}:${phase:-}"
    if [ -n "${screen_id:-}" ] && [ "$phase" != "done" ] && [ "$phase_key" != "$last_phase_key" ]; then
      last_phase_key="$phase_key"
      if [ "$phase" = "loading" ]; then
        DISPLAY="$DISPLAY_ID" xdotool windowfocus --sync "$WINDOW_ID"
        DISPLAY="$DISPLAY_ID" xdotool type --clearmodifiers --delay 0 "$sequence"
        printf 'TASK3546_SENT screen=%s phase=loading sequence=%s\n' "$screen_id" "$sequence"
      elif [ "$phase" = "loaded" ]; then
        if [ "$SKIP_AFTER" = "1" ]; then
          printf 'TASK3546_SENT screen=%s phase=loaded skipped=true sequence=%s\n' "$screen_id" "$sequence"
        else
          DISPLAY="$DISPLAY_ID" xdotool windowfocus --sync "$WINDOW_ID"
          DISPLAY="$DISPLAY_ID" xdotool type --clearmodifiers --delay 0 "$sequence"
          printf 'TASK3546_SENT screen=%s phase=loaded sequence=%s\n' "$screen_id" "$sequence"
        fi
      fi
    fi
  fi
  sleep 0.03
done
wait "$APP_PID"
APP_PID=""

python3 - "$RECORDS_PATH" "$REPORT_PATH" <<'PY'
import csv
import sys
from pathlib import Path

records_path = Path(sys.argv[1])
report_path = Path(sys.argv[2])
rows = list(csv.DictReader(records_path.open(encoding="utf-8"), delimiter="\t"))
screen_count = len(rows)
after_landed = 0
early_absent = 0
early_target = 0
early_other = 0
early_actions = 0
lines = [
    "# TASK 3546 fixed-screen early input audit",
    "",
    "| screen | sequence | documented target | early focused control | early text | after focused control | after text |",
    "| --- | --- | --- | --- | --- | --- | --- |",
]
for row in rows:
    target = row["target_id"]
    sequence = row["sequence"]
    early_control = row["early_control"]
    early_text = row["early_text"]
    after_control = row["after_control"]
    after_text = row["after_text"]
    actions = int(row["early_actions"])
    early_actions += actions
    if after_control == target and after_text == sequence:
        after_landed += 1
    if early_control == "absent" and early_text == "":
        early_absent += 1
        early_ok = True
    elif early_control == target and early_text == sequence:
        early_target += 1
        early_ok = True
    else:
        early_other += 1
        early_ok = False
    print(
        "TASK3546_SCREEN "
        f"screen={row['screen']} sequence={sequence} target={target} "
        f"early_control={early_control} early_text={early_text!r} early_ok={str(early_ok).lower()} "
        f"after_control={after_control} after_text={after_text!r} "
        f"after_landed={str(after_control == target and after_text == sequence).lower()} "
        f"early_actions={actions}"
    )
    lines.append(
        f"| {row['screen']} | `{sequence}` | `{target}` | `{early_control}` | `{early_text}` | `{after_control}` | `{after_text}` |"
    )

status = (
    "ok"
    if screen_count > 0
    and after_landed == screen_count
    and early_other == 0
    and early_actions == 0
    else "fail"
)
lines.extend([
    "",
    f"- screen_count: {screen_count}",
    f"- after_landed_count: {after_landed}",
    f"- early_absent_count: {early_absent}",
    f"- early_target_count: {early_target}",
    f"- early_other_count: {early_other}",
    f"- early_actions_started: {early_actions}",
    f"- status: {status}",
])
report_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(
    "TASK3546_SUMMARY "
    f"screen_count={screen_count} after_landed_count={after_landed} "
    f"early_absent_count={early_absent} early_target_count={early_target} "
    f"early_other_count={early_other} early_actions_started={early_actions} status={status}"
)
print(f"TASK3546_REPORT path={report_path}")
if status != "ok":
    raise SystemExit(1)
PY
