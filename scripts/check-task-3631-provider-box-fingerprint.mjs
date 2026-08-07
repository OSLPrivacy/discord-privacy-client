import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.cwd();
const ADAPTERS = path.join(ROOT, 'apps/osl-hub/src/adapters');

function read(relative) {
  return fs.readFileSync(path.join(ROOT, relative), 'utf8');
}

function fail(message) {
  console.error(`TASK3631_FAIL=${message}`);
  process.exit(1);
}

function fingerprint(providerId) {
  return crypto
    .createHash('sha256')
    .update('OSL/provider-message-box-fingerprint/v1')
    .update(Buffer.from([0]))
    .update(providerId)
    .digest('hex');
}

function extractProviderInventory(source) {
  const match = source.match(
    /SUPPORTED_MESSAGE_BOX_PROVIDERS:\s*\[&str;\s*SUPPORTED_MESSAGE_BOX_PROVIDER_COUNT\]\s*=\s*\[([\s\S]*?)\];/,
  );
  if (!match) {
    fail('missing_supported_provider_inventory');
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
}

function bodyAfter(source, anchor, functionNeedle) {
  const anchorIndex = source.indexOf(anchor);
  if (anchorIndex < 0) {
    fail(`missing_anchor:${anchor}`);
  }
  const fnIndex = source.indexOf(functionNeedle, anchorIndex);
  if (fnIndex < 0) {
    fail(`missing_function:${functionNeedle}`);
  }
  const openIndex = source.indexOf('{', fnIndex);
  if (openIndex < 0) {
    fail(`missing_function_body:${functionNeedle}`);
  }
  let depth = 0;
  for (let index = openIndex; index < source.length; index += 1) {
    const char = source[index];
    if (char === '{') {
      depth += 1;
    } else if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(openIndex, index + 1);
      }
    }
  }
  fail(`unterminated_function_body:${functionNeedle}`);
}

function assertGuardBeforeBackendPlace(relative, anchor, backendCall) {
  const source = read(relative);
  const body = bodyAfter(source, anchor, 'fn place(');
  const guardIndex = body.indexOf('same_scope_and_message_box(binding,');
  const backendIndex = body.indexOf(backendCall);
  if (guardIndex < 0) {
    fail(`missing_message_box_guard:${relative}`);
  }
  if (backendIndex < 0) {
    fail(`missing_backend_place:${relative}`);
  }
  if (guardIndex > backendIndex) {
    fail(`message_box_guard_after_backend_place:${relative}`);
  }
  guardedPlacementCount += 1;
}

const adaptersMod = fs.readFileSync(path.join(ADAPTERS, 'mod.rs'), 'utf8');
if (!adaptersMod.includes('message_box_fingerprint: MessageBoxFingerprint')) {
  fail('placement_authorization_missing_fingerprint');
}
if (!adaptersMod.includes('pub(crate) fn same_scope_and_message_box')) {
  fail('missing_shared_preplacement_guard');
}

const providers = extractProviderInventory(adaptersMod);
if (providers.length !== 17) {
  fail(`provider_count_${providers.length}`);
}
if (new Set(providers).size !== providers.length) {
  fail('duplicate_provider_fingerprint_inventory');
}

let guardedPlacementCount = 0;
assertGuardBeforeBackendPlace(
  'apps/osl-hub/src/adapters/discord.rs',
  'impl<B: DiscordBackend> SurfaceAdapter',
  'self.backend.place(binding, carrier)',
);
assertGuardBeforeBackendPlace(
  'apps/osl-hub/src/adapters/telegram.rs',
  'impl<B: TelegramBackend> SurfaceAdapter',
  'self.backend.place(binding, carrier)',
);
assertGuardBeforeBackendPlace(
  'apps/osl-hub/src/adapters/signal.rs',
  'impl<B: SignalBackend> SurfaceAdapter',
  'self.backend.place_without_submit(binding, carrier)',
);
assertGuardBeforeBackendPlace(
  'apps/osl-hub/src/adapters/whatsapp.rs',
  'impl<B: WhatsAppBackend> SurfaceAdapter',
  'self.backend.place(binding, carrier)',
);
assertGuardBeforeBackendPlace(
  'apps/osl-hub/src/web_surface_adapter/mod.rs',
  'impl<B: WebSurfaceBackend> SurfaceAdapter',
  'self.backend.place(binding, carrier)',
);

console.log('TASK3631_DIRECT_COMMAND=check-task-3631-provider-box-fingerprint');
providers.forEach((providerId, index) => {
  console.log(
    `TASK3631_PROVIDER_MESSAGE_BOX_FINGERPRINT[${String(index).padStart(2, '0')}]=${providerId}:${fingerprint(providerId)}`,
  );
});
console.log(`TASK3631_PROVIDER_FINGERPRINT_COUNT=${providers.length}`);
console.log(`TASK3631_PREPLACEMENT_GUARDED_ADAPTER_COUNT=${guardedPlacementCount}`);

let backendPlaceCount = 0;
const discordFingerprint = fingerprint('discord');
function discordPlacementCheck(authorizationFingerprint) {
  if (authorizationFingerprint !== discordFingerprint) {
    return 'NotPlaced';
  }
  backendPlaceCount += 1;
  return 'Placed';
}

const matchingPlacement = discordPlacementCheck(fingerprint('discord'));
const changedPlacement = discordPlacementCheck(fingerprint('telegram'));
const refusedBeforeText = changedPlacement === 'NotPlaced' && backendPlaceCount === 1;

console.log(`TASK3631_DISCORD_MATCHING_PLACEMENT=${matchingPlacement}`);
console.log(
  `TASK3631_CHANGED_FINGERPRINT_REFUSED_BEFORE_TEXT=${refusedBeforeText} backend_place_count=${backendPlaceCount}`,
);

if (matchingPlacement !== 'Placed') {
  fail('matching_discord_fingerprint_not_permitted');
}
if (!refusedBeforeText) {
  fail('changed_fingerprint_not_refused_before_text');
}
