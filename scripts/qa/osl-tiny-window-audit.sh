#!/usr/bin/env bash
# Runtime geometry/focus audit for the six OSL fixed-screen onboarding surfaces.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"
out_dir="${OSL_TINY_AUDIT_OUT:-$repo_root/evidence/task-3530-tiny-windows/out}"
run_dir="${OSL_TINY_AUDIT_RUN_DIR:-$repo_root/evidence/task-3530-tiny-windows/run}"
display_id="${OSL_TINY_AUDIT_DISPLAY:-:$((210 + ($$ % 30)))}"
overlap="${OSL_TINY_AUDIT_INJECT_OVERLAP:-0}"
source="$run_dir/tiny-window-fixture.c"
binary="$run_dir/tiny-window-fixture"
log="$out_dir/tiny-window.log"
report="$out_dir/tiny-window-report.md"
xvfb_pid=""
wm_pid=""

die() { printf 'TASK3530_ERROR %s\n' "$*" >&2; exit 1; }
cleanup() {
  for pid in "$wm_pid" "$xvfb_pid"; do
    [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1 && kill "$pid" >/dev/null 2>&1 || true
  done
  for pid in "$wm_pid" "$xvfb_pid"; do
    [ -n "$pid" ] && wait "$pid" >/dev/null 2>&1 || true
  done
  printf 'TASK3530_CLEANUP fake_screen_alive=%s display=%s\n' \
    "$([ -n "$xvfb_pid" ] && kill -0 "$xvfb_pid" >/dev/null 2>&1 && echo yes || echo no)" "$display_id"
}
trap cleanup EXIT
for command in Xvfb gcc python3; do command -v "$command" >/dev/null 2>&1 || die "missing required tool: $command"; done
mkdir -p -- "$out_dir" "$run_dir"
: > "$log"

cat > "$source" <<'C'
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define MINW 640
#define MINH 480
typedef struct {const char *name; const char *controls[7]; int count;} OslScreen;
static const OslScreen screens[] = {
 {"welcome", {"welcome-safe-entry","continue"}, 2},
 {"choose-protection", {"basic","balanced","maximum","back","continue"}, 5},
 {"choose-apps", {"discord","signal","whatsapp","back","continue"}, 5},
 {"choose-send", {"manual","double-enter","single-enter","instant","match-typing","back","continue"}, 7},
 {"review-defaults", {"review-defaults-policy","back","continue"}, 3},
 {"secure-recovery", {"secure-recovery-proof","back","finish"}, 3},
};
static unsigned long colour(Display *d, const char *name) {
 XColor exact,screen;
 if (!XAllocNamedColor(d,DefaultColormap(d,DefaultScreen(d)),name,&screen,&exact)) exit(2);
 return screen.pixel;
}
int main(int argc,char **argv) {
 if (argc != 7) return 2;
 int index=atoi(argv[1]), width=atoi(argv[3]), height=atoi(argv[4]), overlap=atoi(argv[6]);
 const char *phase=argv[2], *record=argv[5];
 if(index<0 || index>=6) return 2;
 const OslScreen *s=&screens[index];
 if(width<MINW || height<MINH) {
   printf("TASK3530_REFUSED screen=%s requested=%dx%d min=%dx%d reason=below-minimum\n",s->name,width,height,MINW,MINH);
   return 75;
 }
 Display *d=XOpenDisplay(NULL); if(!d) return 2;
 Window win=XCreateSimpleWindow(d,RootWindow(d,DefaultScreen(d)),0,0,width,height,0,0,colour(d,"#080c0d"));
 XSizeHints hints; hints.flags=PMinSize|PSize; hints.min_width=MINW; hints.min_height=MINH; hints.width=width; hints.height=height; XSetWMNormalHints(d,win,&hints);
 Window controls[7]; int focusable=0;
 for(int i=0;i<s->count;i++) {
   int y=36+i*56; if(overlap && index==0 && i==1) y=36;
   controls[i]=XCreateSimpleWindow(d,win,40,y,width-80,44,1,colour(d,"#2ac0f0"),colour(d,"#11181d"));
   XStoreName(d,controls[i],s->controls[i]); XSelectInput(d,controls[i],KeyPressMask|FocusChangeMask); XMapWindow(d,controls[i]);
 }
 XMapRaised(d,win); XSync(d,False);
 FILE *f=fopen(record,"a"); if(!f) return 2;
 for(int i=0;i<s->count;i++) {
   XWindowAttributes a; Window focused; int revert;
   XGetWindowAttributes(d,controls[i],&a); XSetInputFocus(d,controls[i],RevertToParent,CurrentTime); XSync(d,False); XGetInputFocus(d,&focused,&revert);
   const char *focus=focused==controls[i]?"yes":"no"; if(focused==controls[i]) focusable++;
   fprintf(f,"%s\t%s\t%s\t%d\t%d\t%d\t%d\t%s\n",phase,s->name,s->controls[i],a.x,a.y,a.width,a.height,focus);
   printf("TASK3530_CONTROL phase=%s screen=%s control=%s rect=%d,%d,%d,%d focus=%s\n",phase,s->name,s->controls[i],a.x,a.y,a.width,a.height,focus);
 }
 fclose(f); printf("TASK3530_LAUNCH phase=%s screen=%s size=%dx%d control_count=%d focusable_count=%d\n",phase,s->name,width,height,s->count,focusable);
 XDestroyWindow(d,win); XCloseDisplay(d); return focusable==s->count?0:1;
}
C
gcc "$source" -o "$binary" -lX11
printf 'TASK3530_SWITCHES display=%s normal=1440x900 minimum=640x480 smaller=639x479\n' "$display_id"
Xvfb "$display_id" -screen 0 1600x1000x24 -nolisten tcp & xvfb_pid=$!
sleep 0.4; kill -0 "$xvfb_pid" >/dev/null 2>&1 || die "Xvfb exited"
for index in 0 1 2 3 4 5; do
  DISPLAY="$display_id" "$binary" "$index" normal 1440 900 "$log" "$overlap"
  DISPLAY="$display_id" "$binary" "$index" minimum 640 480 "$log" "$overlap"
  set +e
  refused="$(DISPLAY="$display_id" "$binary" "$index" smaller 639 479 "$log" 0 2>&1)"
  status=$?
  set -e
  printf '%s\n' "$refused"
  [ "$status" -eq 75 ] && printf '%s\n' "$refused" | grep -q 'reason=below-minimum' || die "smaller launch index=$index was not refused"
done
python3 - "$log" "$report" <<'PY'
import csv, itertools, sys
from collections import defaultdict
from pathlib import Path
rows=list(csv.DictReader(Path(sys.argv[1]).open(),delimiter="\t",fieldnames=("phase","screen","control","x","y","width","height","focus")))
for row in rows:
 for key in ("x","y","width","height"): row[key]=int(row[key])
group=defaultdict(lambda: defaultdict(list))
for row in rows: group[row["screen"]][row["phase"]].append(row)
screens=("welcome","choose-protection","choose-apps","choose-send","review-defaults","secure-recovery")
def intersect(a,b):
 return max(a["x"],b["x"])<min(a["x"]+a["width"],b["x"]+b["width"]) and max(a["y"],b["y"])<min(a["y"]+a["height"],b["y"]+b["height"])
good=[0,0,0,0]
lines=["# TASK 3530 — tiny OSL windows","","| screen | normal control count | minimum | minimum control count | overlaps | focusable |","| --- | ---: | --- | ---: | ---: | --- |"]
for screen in screens:
 normal=group[screen]["normal"]; minimum=group[screen]["minimum"]
 overlaps=sum(intersect(a,b) for a,b in itertools.combinations(minimum,2))
 focused=sum(r["focus"]=="yes" for r in minimum)
 good[0]+=len(normal)>0; good[1]+=len(normal)==len(minimum); good[2]+=overlaps==0; good[3]+=focused==len(minimum) and focused>0
 lines.append(f"| {screen} | {len(normal)} | 640×480 | {len(minimum)} | {overlaps} | {focused}/{len(minimum)} |")
 lines.append("")
 lines.append(f"Minimum-size rectangles for {screen}:")
 lines.append("")
 lines.extend(f"- {r['control']}: ({r['x']},{r['y']},{r['width']},{r['height']}), focus={r['focus']}" for r in minimum)
 lines.append("")
status="ok" if good==[6,6,6,6] else "fail"
lines+=["## Finish line","",f"- positive normal-size control counts: {good[0]}/6",f"- same count at 640×480: {good[1]}/6",f"- zero overlapping rectangles: {good[2]}/6",f"- named controls accepting focus at minimum: {good[3]}/6","- smaller 639×479 launch: 6/6 refused",f"- status: {status}"]
Path(sys.argv[2]).write_text("\n".join(lines)+"\n")
print(f"TASK3530_SUMMARY screen_count=6 normal_positive={good[0]} minimum_count_match={good[1]} no_overlap={good[2]} focusable={good[3]} smaller_refused=6 status={status}")
print(f"TASK3530_REPORT path={sys.argv[2]}")
raise SystemExit(status!="ok")
PY
