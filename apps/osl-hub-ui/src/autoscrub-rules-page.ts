/// TASK 1461: connect the AutoScrub rules page -- rule switches, private
/// words, and "Test these rules". Reuses the [[rule_name, private_word]]
/// shape TASK 1459 gave AutoScrub (`cmd_osl_list_autoscrub_bad_message_rules`
/// / `cmd_osl_save_autoscrub_bad_message_rule`), and previews matches the
/// same find-only way TASK 1460 established: the connection this page is
/// handed to "Test these rules" exposes a read method and nothing else --
/// there is no delete method in `AutoScrubPreviewConnection` for the preview
/// to call even by mistake, so the page has zero deletion buttons to render.

export interface AutoScrubBadMessageRule {
  ruleName: string;
  privateWord: string;
}

/// One row on the page: a switch (on/off) plus the editable private word for
/// that rule name. `enabled` is a page-local concept -- a rule is "on" when
/// it is present in AutoScrub's saved rule map (TASK 1459's independent
/// `autoscrub_bad_message_rules` switch), "off" when it has been removed
/// from that map without touching normal Scrub's rules.
export interface AutoScrubRuleSwitch {
  ruleName: string;
  enabled: boolean;
  privateWord: string;
}

export interface AutoScrubPreviewMessage {
  messageId: string;
  channelId: string;
  body: string;
}

/// A connection "Test these rules" reads through. Deliberately has no
/// delete/remove method of any kind -- the preview cannot call a capability
/// that does not exist on the type it is handed.
export interface AutoScrubPreviewConnection {
  readMessages(): Promise<AutoScrubPreviewMessage[]>;
}

export interface AutoScrubRuleMatch {
  messageId: string;
  channelId: string;
  ruleName: string;
  matchedWord: string;
}

export interface AutoScrubRulePreview {
  scannedMessageCount: number;
  matches: AutoScrubRuleMatch[];
  matchCount: number;
  /// Structurally always 0 -- there is no code path in this module that can
  /// set it to anything else, since nothing here ever calls a delete method.
  deletedCount: 0;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/// Only enabled switches with a non-empty private word are tested -- an
/// off switch or a blank word cannot produce a match.
function activeSwitches(switches: AutoScrubRuleSwitch[]): AutoScrubRuleSwitch[] {
  return switches.filter((rule) => rule.enabled && rule.privateWord.trim().length > 0);
}

/// Find-only preview: reads every message once through `connection`, marks
/// every case-insensitive substring match against each active rule's private
/// word, and never calls anything but `readMessages`. `deletedCount` is a
/// literal `0`, not a counter that happens to land on zero.
export async function previewAutoScrubBadMessageRules(
  switches: AutoScrubRuleSwitch[],
  connection: AutoScrubPreviewConnection,
): Promise<AutoScrubRulePreview> {
  const rules = activeSwitches(switches);
  const messages = await connection.readMessages();

  const matches: AutoScrubRuleMatch[] = [];
  for (const message of messages) {
    const bodyLower = message.body.toLowerCase();
    for (const rule of rules) {
      if (bodyLower.includes(rule.privateWord.trim().toLowerCase())) {
        matches.push({
          messageId: message.messageId,
          channelId: message.channelId,
          ruleName: rule.ruleName,
          matchedWord: rule.privateWord,
        });
      }
    }
  }

  return {
    scannedMessageCount: messages.length,
    matches,
    matchCount: matches.length,
    deletedCount: 0,
  };
}

/// Renders the rule switches, their private-word inputs, and the "Test
/// these rules" button. `preview` is `null` before the first test run.
export function renderAutoScrubRulesPage(
  switches: AutoScrubRuleSwitch[],
  preview: AutoScrubRulePreview | null,
): string {
  const switchRows = switches
    .map((rule) => {
      const safeName = escapeHtml(rule.ruleName);
      const slug = rule.ruleName.replace(/\s+/g, "-");
      return `<div class="autoscrub-rule-row" data-autoscrub-rule-row="${slug}"><label class="autoscrub-rule-switch"><input type="checkbox" data-autoscrub-rule-switch="${slug}" ${rule.enabled ? "checked" : ""}/><span>${safeName}</span></label><input type="text" class="autoscrub-rule-word" data-autoscrub-rule-word="${slug}" value="${escapeHtml(rule.privateWord)}" placeholder="Private word" ${rule.enabled ? "" : "disabled"}/></div>`;
    })
    .join("");

  const previewSection = renderAutoScrubRulePreview(preview);

  return `<section class="autoscrub-rules-page" aria-labelledby="autoscrub-rules-heading"><header><h2 id="autoscrub-rules-heading">AutoScrub rules</h2><p>Turn a rule on and give it a private word AutoScrub should flag. Testing a rule only reads messages -- it never deletes anything.</p></header><div class="autoscrub-rule-list">${switchRows}</div><button class="button compact" id="autoscrub-test-rules" type="button">Test these rules</button>${previewSection}</section>`;
}

/// The preview list itself: every matched item, with the exact count in the
/// heading. No delete affordance is ever emitted here -- there is no
/// markup path in this function that produces a `<button>` per match.
export function renderAutoScrubRulePreview(preview: AutoScrubRulePreview | null): string {
  if (!preview) return "";

  if (preview.matches.length === 0) {
    return `<section class="autoscrub-rule-preview" aria-live="polite"><p class="autoscrub-rule-preview-count">0 matches out of ${preview.scannedMessageCount} scanned.</p><p>Nothing has been deleted.</p></section>`;
  }

  const items = preview.matches
    .map(
      (match) =>
        `<li data-autoscrub-match="${escapeHtml(match.messageId)}"><strong>${escapeHtml(match.channelId)}</strong><code>${escapeHtml(match.messageId)}</code><span>${escapeHtml(match.ruleName)}</span><span>${escapeHtml(match.matchedWord)}</span></li>`,
    )
    .join("");

  return `<section class="autoscrub-rule-preview" aria-live="polite"><p class="autoscrub-rule-preview-count">${preview.matchCount} match${preview.matchCount === 1 ? "" : "es"} out of ${preview.scannedMessageCount} scanned.</p><ul class="autoscrub-rule-preview-list">${items}</ul><p><strong>Nothing has been deleted.</strong> This is a read-only preview.</p></section>`;
}
