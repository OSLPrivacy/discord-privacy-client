#!/usr/bin/env node

const HALF_GIB = 536870912;
const WARNING =
  'WARNING: during probe A the global multipart reservation pool is deliberately held full for a few seconds, so other users cannot START a new multipart upload in that window. Direct uploads and existing attachments are unaffected. Run in a maintenance window.';

function parseArgs(argv) {
  const args = { host: null, yes: false };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];

    if (arg === '--host') {
      args.host = argv[i + 1] ?? null;
      i += 1;
      continue;
    }

    if (arg === '--yes') {
      args.yes = true;
      continue;
    }

    throw new Error(`unknown argument: ${arg}`);
  }

  if (!args.host) {
    throw new Error('missing required --host');
  }

  const host = new URL(args.host);
  host.pathname = host.pathname.replace(/\/+$/, '');
  host.search = '';
  host.hash = '';
  args.host = host.toString().replace(/\/$/, '');

  return args;
}

function printUsage() {
  console.log('Usage: node scripts/post-deploy-probe.mjs --host https://ciphers.oslprivacy.com [--yes]');
}

function printDryRunPlan(host) {
  console.log('Post-deploy probe plan');
  console.log(`Host: ${host}`);
  console.log('');
  console.log(WARNING);
  console.log('');
  console.log('No requests were sent because --yes was not provided.');
  console.log('');
  console.log('With --yes this will run:');
  console.log("A. DECISIVE: reservation pool is isolated from stored content");
  console.log("B. REGRESSION GUARD, NOT decisive: multipart receipts agree and content is retrievable");
  console.log("C. DECISIVE, runs last: mutation rate limiting is atomic and exhausts this address's hourly session budget");
}

function capability() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function attachmentHeaders({ ttl, cap, size }) {
  return {
    'x-osl-ttl-seconds': String(ttl),
    'x-osl-fetch-token': cap,
    'x-osl-size-bytes': String(size),
    'content-length': '0',
  };
}

function bodyHeaders({ ttl, cap, size }) {
  return {
    'x-osl-ttl-seconds': String(ttl),
    'x-osl-fetch-token': cap,
    'content-length': String(size),
  };
}

function tokenHeaders(cap) {
  return {
    'x-osl-fetch-token': cap,
  };
}

async function readJson(response) {
  try {
    return await response.json();
  } catch {
    return null;
  }
}

async function createSession(host, { ttl, cap, size }) {
  const response = await fetch(`${host}/v1/attachment/session`, {
    method: 'POST',
    headers: attachmentHeaders({ ttl, cap, size }),
  });
  const json = await readJson(response);
  return { response, json, status: response.status, cap };
}

async function deleteAttachment(host, item) {
  const response = await fetch(`${host}/v1/attachment/${item.id}`, {
    method: 'DELETE',
    headers: tokenHeaders(item.cap),
  });
  return response.status;
}

async function cleanup(host, items, label) {
  const failures = [];

  for (const item of items) {
    try {
      const status = await deleteAttachment(host, item);
      if (status !== 204) {
        failures.push(`${item.id}: HTTP ${status}`);
      }
    } catch (error) {
      failures.push(`${item.id}: ${error.message}`);
    }
  }

  if (failures.length > 0) {
    console.log(`CLEANUP FAIL ${label}: ${failures.join('; ')}`);
  }
}

async function probeA(host) {
  const cleanupItems = [];

  try {
    const firstFour = [];
    for (let i = 0; i < 4; i += 1) {
      const cap = capability();
      const result = await createSession(host, { ttl: 3600, cap, size: HALF_GIB });
      if (result.status === 201 && result.json?.id) {
        cleanupItems.push({ id: result.json.id, cap });
      }
      firstFour.push(result);
    }

    const fifthCap = capability();
    const fifth = await createSession(host, { ttl: 3600, cap: fifthCap, size: HALF_GIB });
    if (fifth.status === 201 && fifth.json?.id) {
      cleanupItems.push({ id: fifth.json.id, cap: fifthCap });
    }

    const directCap = capability();
    const directBody = new Uint8Array([112, 114, 111, 98]);
    const direct = await fetch(`${host}/v1/attachment`, {
      method: 'POST',
      headers: bodyHeaders({ ttl: 3600, cap: directCap, size: directBody.byteLength }),
      body: directBody,
    });
    const directJson = await readJson(direct);
    if (direct.status === 201 && directJson?.id) {
      cleanupItems.push({ id: directJson.id, cap: directCap });
    }

    const createdAllReservations = firstFour.every((result) => result.status === 201);
    const fifthRejectedByCapacity = fifth.status === 503 && fifth.json?.error === 'storage_capacity';
    const directUploadAccepted = direct.status === 201;
    const pass = createdAllReservations && fifthRejectedByCapacity && directUploadAccepted;

    const details = [
      `first4=${firstFour.map((result) => result.status).join(',')}`,
      `fifth=${fifth.status}${fifth.json?.error ? `/${fifth.json.error}` : ''}`,
      `direct=${direct.status}`,
    ].join(' ');

    console.log(
      `${pass ? 'PASS' : 'FAIL'} Probe A DECISIVE reservation pool is isolated from stored content (${details})`,
    );

    return pass;
  } catch (error) {
    console.log(`FAIL Probe A DECISIVE reservation pool is isolated from stored content (${error.message})`);
    return false;
  } finally {
    await cleanup(host, cleanupItems, 'Probe A');
  }
}

async function probeB(host) {
  const cleanupItems = [];

  try {
    const cap = capability();
    const session = await createSession(host, { ttl: 604800, cap, size: 1024 });
    if (session.status === 201 && session.json?.id) {
      cleanupItems.push({ id: session.json.id, cap });
    }

    if (session.status !== 201 || !session.json?.id || !session.json?.expires_at) {
      console.log(`FAIL Probe B REGRESSION GUARD, NOT decisive multipart receipt/retrieval (session=${session.status})`);
      return false;
    }

    const id = session.json.id;
    const body = new Uint8Array(1024);
    crypto.getRandomValues(body);

    const part = await fetch(`${host}/v1/attachment/${id}/part/1`, {
      method: 'PUT',
      headers: {
        'x-osl-fetch-token': cap,
        'content-length': String(body.byteLength),
      },
      body,
    });

    const complete = await fetch(`${host}/v1/attachment/${id}/complete`, {
      method: 'POST',
      headers: {
        'x-osl-fetch-token': cap,
        'content-length': '0',
      },
    });
    const completeJson = await readJson(complete);

    const got = await fetch(`${host}/v1/attachment/${id}`, {
      method: 'GET',
      headers: tokenHeaders(cap),
    });
    const gotBytes = got.status === 200 ? new Uint8Array(await got.arrayBuffer()) : new Uint8Array(0);

    // `expires_at` is unix-epoch SECONDS as a JSON number, not an ISO string --
    // see the `json({ ... expires_at: contentExpiresAt })` receipts in
    // src/endpoints/attachment.ts. Date.parse() on it yields NaN, which would
    // make this probe fail permanently and look like a deploy regression.
    const sessionExpiresMs = Number(session.json.expires_at) * 1000;
    const expectedExpiresMs = Date.now() + 604800 * 1000;
    const expiresWithinWindow =
      Number.isFinite(sessionExpiresMs)
      && Math.abs(sessionExpiresMs - expectedExpiresMs) <= 120000;
    const receiptsAgree = completeJson?.expires_at === session.json.expires_at;
    const bytesMatch =
      gotBytes.byteLength === body.byteLength && gotBytes.every((byte, index) => byte === body[index]);

    const pass =
      part.status === 201 &&
      complete.status === 201 &&
      receiptsAgree &&
      expiresWithinWindow &&
      got.status === 200 &&
      bytesMatch;

    const details = [
      `session=${session.status}`,
      `part=${part.status}`,
      `complete=${complete.status}`,
      `expires_agree=${receiptsAgree}`,
      `expires_window=${expiresWithinWindow}`,
      `get=${got.status}/${gotBytes.byteLength}b`,
      `bytes_match=${bytesMatch}`,
    ].join(' ');

    console.log(
      `${pass ? 'PASS' : 'FAIL'} Probe B REGRESSION GUARD, NOT decisive multipart receipts agree and content is retrievable (${details})`,
    );

    return pass;
  } catch (error) {
    console.log(
      `FAIL Probe B REGRESSION GUARD, NOT decisive multipart receipts agree and content is retrievable (${error.message})`,
    );
    return false;
  } finally {
    await cleanup(host, cleanupItems, 'Probe B');
  }
}

async function probeC(host) {
  const cleanupItems = [];

  try {
    const attempts = Array.from({ length: 40 }, async () => {
      const cap = capability();
      const result = await createSession(host, { ttl: 3600, cap, size: 1024 });
      if (result.status === 201 && result.json?.id) {
        cleanupItems.push({ id: result.json.id, cap });
      }
      return result;
    });

    const results = await Promise.all(attempts);
    const nonRateLimited = results.filter((result) => result.status !== 429).length;
    const rateLimited = results.filter((result) => result.status === 429).length;
    const pass = nonRateLimited <= 24 && rateLimited > 0;

    console.log(
      `${pass ? 'PASS' : 'FAIL'} Probe C DECISIVE mutation rate limiting is atomic (non_429=${nonRateLimited} 429=${rateLimited})`,
    );
    console.log('Probe C exhausted this address session budget for the rest of the clock hour.');

    return pass;
  } catch (error) {
    console.log(`FAIL Probe C DECISIVE mutation rate limiting is atomic (${error.message})`);
    return false;
  } finally {
    await cleanup(host, cleanupItems, 'Probe C');
  }
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    printUsage();
    console.error(error.message);
    process.exitCode = 1;
    return;
  }

  if (!args.yes) {
    printDryRunPlan(args.host);
    return;
  }

  console.log('Running production post-deploy probes');
  console.log(`Host: ${args.host}`);
  console.log(WARNING);

  const results = [];
  results.push(await probeA(args.host));
  results.push(await probeB(args.host));
  results.push(await probeC(args.host));

  console.log(
    'NOTE: the generic-blob aggregate quota is deliberately NOT probed, because proving it would mean filling 2 GiB of production storage.',
  );

  const passed = results.filter(Boolean).length;
  const failed = results.length - passed;
  console.log(`SUMMARY ${failed === 0 ? 'PASS' : 'FAIL'} passed=${passed} failed=${failed} skipped=0`);

  process.exitCode = failed === 0 ? 0 : 1;
}

await main();
