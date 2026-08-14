#!/usr/bin/env python3
"""Measure how far an app screen is from its design page.

Two numbers, because they mean different things:
  raw       - every differing pixel. Contaminated by CONTENT: the design
              prototypes ship their own demo data (Mara, Kit, Frontier) and the
              app has different fixtures, so different words count as difference.
  structure - both images blurred hard first, so glyphs dissolve into blocks.
              What survives is WHERE things are and WHAT COLOUR they are.
              This is the honest measure of layout and palette conformance.

Optimise `structure`. A screen whose raw is high but structure is low is
telling you the words differ, not the design.

    python3 uidiff.py                # every paired screen, worst first
    python3 uidiff.py home           # one screen
"""
import sys, os, json
from PIL import Image, ImageChops, ImageFilter
HERE = os.path.dirname(os.path.abspath(__file__))
CANON = '/home/liamw/osl-plan/OSL-AUDITS/reference/canon-1280x800'
APP = os.path.join(HERE, 'all-routes')
PAIRS = [
 ('Home','home'),('Settings','settings'),('OSLChats','osl-chat'),('OSLServers','osl-servers'),
 ('Scrub','scrub'),('Sign_In_Final','onboarding-unlock'),('Key_Lost','onboarding-keylost'),
 ('Create_Account','onboarding-welcome'),('Create_Password','onboarding-create'),
 ('Recovery_Kit','onboarding-recovery'),('Restore_Account','onboarding-import'),
 ('Pro_Code','onboarding-pro'),('Mullvad','onboarding-mullvad'),('Connection_Choice','onboarding-tor'),
 ('Send_Mode','onboarding-sending'),('Send_Checks','onboarding-privacy'),
 ('Cover_Insertion','onboarding-cover'),('Old_Messages','onboarding-forward-secrecy'),
 ('Device_Storage','onboarding-defaults'),('Stealth_Password','onboarding-passwords'),
 ('Burn_Password','onboarding-burnpass'),('Forgot_Password','onboarding-account-recovery'),
 # Settings has one canonical composition; each shipped section is independently
 # captured so section changes cannot disappear behind the default Account view.
 ('Settings_Account','settings-account'),('Settings_Apps','settings-apps'),('Settings_Privacy','settings-privacy'),
 ('Settings_Whitelisting','settings-whitelisting'),('Settings_Scrub','settings-scrub'),('Settings_Cleanup','settings-cleanup'),
 ('Settings_Notifications','settings-notifications'),('Settings_Appearance','settings-appearance'),('Settings_About','settings-about'),
 # Interaction surfaces use state-specific canons so their measurements are
 # not contaminated by a different popup or by the parent page's base state.
 ('Home_notifs','home-notifications-popover'),('Home_edit_mode','home-edit-mode'),
 ('Home_friends_collapsed','home-friends-collapsed'),
 ('OSLChats_settings','osl-chat-settings-popup'),('OSLChats_key_changed','osl-chat-key-changed'),
 ('OSLChats_profile_appearance','osl-chat-profile-appearance'),('OSLChats_new','osl-chat-start-something'),
 ('OSLChats_safety_number','osl-chat-safety-number'),('OSLServers_card','osl-servers-member-profile'),
 ('Strip_timer','strip-timer-popup'),('Strip_whitelist','strip-whitelist-popup'),
 ('Strip_quick','strip-quick-settings-popup'),('Strip','strip-unprovable-room'),('Strip_burn','strip-burn-dialog'),
]

# The capture harness deliberately covers product surfaces for which the
# supplied design export has no corresponding page.  Keeping this exception
# list next to the pair table makes a newly captured screen a review failure
# until it is either paired with a canon or given a specific, written reason.
NO_DESIGN_COUNTERPART = {
    'activity': 'Activity has no page in the supplied design export.',
    'connections': 'Connections has no page in the supplied design export.',
    'inbox': 'Inbox has no page in the supplied design export.',
    'mullvad': 'The workspace Mullvad hand-off is not the onboarding Mullvad design page.',
    'onboarding-apps': 'The later Connect your apps step has no supplied design page.',
    'onboarding-browser': 'The browser-account discovery step has no supplied design page.',
    'onboarding-decoy': 'The decoy workspace step has no supplied design page.',
    'onboarding-detected': 'The installed-app detection step has no supplied design page.',
    'onboarding-install': 'The missing-app installation step has no supplied design page.',
    'onboarding-silent-visible': 'The display-mode step has no supplied design page.',
    'onboarding-tutorial': 'The tutorial lock step has no supplied design page.',
    'onboarding-visibility': 'The visibility step has no supplied design page.',
    'osl-mail': 'OSL Mail has no page in the supplied design export.',
    'people': 'People has no page in the supplied design export.',
    'privacy': 'Privacy has no page in the supplied design export.',
    'service': 'The external-service hand-off has no page in the supplied design export.',
    'signal-qa': 'Signal QA is a verification fixture, not a supplied design page.',
}


def capture_manifest():
    """Return capture records, or a validation error for an unusable manifest."""
    path = os.path.join(APP, 'manifest.json')
    try:
        with open(path, encoding='utf-8') as handle:
            screens = json.load(handle)['screens']
    except (OSError, ValueError, TypeError, KeyError) as error:
        return None, [f"cannot read capture manifest {path}: {error}"]
    if not isinstance(screens, list):
        return None, [f"capture manifest {path} has a non-list screens field"]
    errors = []
    records = {}
    for entry in screens:
        route = entry.get('route') if isinstance(entry, dict) else None
        if not isinstance(route, str) or not route:
            errors.append(f"capture manifest {path} has a screen without a route")
        elif route in records:
            errors.append(f"capture manifest repeats screen: {route}")
        else:
            records[route] = entry
    return records, errors


def validate_coverage(records):
    """Return every condition that would otherwise make UI coverage silent."""
    errors = []
    paired = {}
    for canon, app in PAIRS:
        if app in paired:
            errors.append(f"pairs table repeats capture: {app}")
        paired[app] = canon
        canon_path = os.path.join(CANON, f'{canon}.png')
        if not os.path.isfile(canon_path):
            errors.append(f"missing paired canon page: {canon_path} (screen {app})")

        # A manifest UNREACHABLE record is an intentional absence, with its
        # own reason recorded by the capture harness.  Every expected capture
        # must exist on disk; this is deliberately checked before measuring.
        record = records.get(app)
        unreachable = isinstance(record, dict) and record.get('status') == 'UNREACHABLE'
        if unreachable:
            reason = record.get('reason')
            if not isinstance(reason, str) or not reason.strip():
                errors.append(f"unreachable paired capture has no reason: {app}")
        else:
            capture_path = os.path.join(APP, f'{app}.png')
            if not os.path.isfile(capture_path):
                errors.append(f"missing paired capture: {capture_path} (screen {app})")

    for app in sorted(records):
        if app in paired:
            continue
        reason = NO_DESIGN_COUNTERPART.get(app)
        if not isinstance(reason, str) or not reason.strip():
            errors.append(f"unpaired capture manifest screen has no design counterpart reason: {app}")

    # A stale exception is just as dangerous as an omitted manifest route:
    # require the list to be a precise account of this capture run.
    for app in sorted(set(NO_DESIGN_COUNTERPART) - set(records)):
        errors.append(f"no-design-counterpart entry is not in capture manifest: {app}")
    return errors

def measure(canon, app):
    cp, ap = f'{CANON}/{canon}.png', f'{APP}/{app}.png'
    if not (os.path.exists(cp) and os.path.exists(ap)):
        return None
    A = Image.open(cp).convert('RGB'); B = Image.open(ap).convert('RGB')
    if A.size != B.size: B = B.resize(A.size)
    n = A.size[0] * A.size[1]
    raw = ImageChops.difference(A, B).convert('L')
    rawpct = 100.0 * sum(1 for v in raw.getdata() if v > 28) / n
    ka = A.filter(ImageFilter.GaussianBlur(9)); kb = B.filter(ImageFilter.GaussianBlur(9))
    st = ImageChops.difference(ka, kb).convert('L')
    stpct = 100.0 * sum(1 for v in st.getdata() if v > 18) / n
    amp = st.point(lambda v: min(255, v * 6))
    out = os.path.join(HERE, 'reference/pixel-diff')
    os.makedirs(out, exist_ok=True)
    Image.merge('RGB', (amp, amp.point(lambda v: int(v*0.2)), amp.point(lambda v: int(v*0.45)))) \
         .save(f'{out}/{app}-struct.png')
    return rawpct, stpct

want = sys.argv[1] if len(sys.argv) > 1 else None
manifest_records, errors = capture_manifest()
if manifest_records is not None:
    errors.extend(validate_coverage(manifest_records))
if errors:
    print('UI DIFF COVERAGE ERRORS', file=sys.stderr)
    for error in errors:
        print(f'ERROR: {error}', file=sys.stderr)
    raise SystemExit(1)

rows = []
missing = []
for c, a in PAIRS:
    if want and want not in (c, a): continue
    m = measure(c, a)
    if m:
        rows.append((m[1], m[0], c, a))
    else:
        missing.append((c, a))
rows.sort(reverse=True)
print(f"{'screen':<26}{'raw%':>8}{'STRUCTURE%':>13}")
for st, raw, c, a in rows:
    print(f"{a:<26}{raw:>7.1f}{st:>12.1f}")
if rows:
    print(f"\nmean structure: {sum(r[0] for r in rows)/len(rows):.1f}%   worst: {rows[0][3]} at {rows[0][0]:.1f}%")

if not want:
    print(f"paired captures: {len(rows)}/{len(PAIRS)}")
    if missing:
        print("\nUNREACHABLE PAIRED CAPTURES (manifest reason)")
        for canon, app in missing:
            print(f"{app:<26} {manifest_records[app]['reason']}")

    # The manifest is the only capture authority.  Do not fall back to stale
    # PNGs in the directory: that fallback is how unpaired screens went quiet.
    app_screens = list(manifest_records)
    paired_apps = {app for _, app in PAIRS}
    unpaired = sorted(set(app_screens) - paired_apps)
    print("\nUNPAIRED (no canon design page)")
    if unpaired:
        for app in unpaired:
            print(f"{app:<30} {NO_DESIGN_COUNTERPART[app]}")
    else:
        print("none")
