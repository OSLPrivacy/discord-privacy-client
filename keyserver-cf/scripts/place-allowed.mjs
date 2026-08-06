import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";

const REQUIRED_ARGS = ["db", "app", "account", "kind", "stable-id"];
const SUPPORTED_KIND = "account";

export function stableIdSha256(stableId) {
  return createHash("sha256").update(stableId, "utf8").digest("hex");
}

export function parsePlaceAllowedArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error("usage: place-allowed.mjs --db <sqlite.db> --app <app> --account <account> --kind <kind> --stable-id <stable-id>");
    }
    args[key.slice(2)] = value;
  }
  for (const key of REQUIRED_ARGS) {
    if (typeof args[key] !== "string" || args[key].length === 0) {
      throw new Error(`missing --${key}`);
    }
  }
  for (const key of ["app", "kind"]) {
    if (!/^[a-z][a-z0-9_-]{0,63}$/.test(args[key])) {
      throw new Error(`invalid --${key}`);
    }
  }
  if (args.account.length > 256) throw new Error("invalid --account");
  if (args["stable-id"].length > 512) throw new Error("invalid --stable-id");
  return {
    db: args.db,
    app: args.app,
    account: args.account,
    kind: args.kind,
    stableId: args["stable-id"],
  };
}

export function placeAllowed(db, { app, account, kind, stableId }) {
  const capability = db.prepare(
    `SELECT version
       FROM worker_schema_capabilities
      WHERE capability = 'account_ownership_binding_account_unique'`,
  ).get();
  if (!capability || capability.version !== 1) {
    throw new Error("account ownership binding contract is not applied");
  }

  if (kind !== SUPPORTED_KIND) return false;

  const row = db.prepare(
    `SELECT 1 AS allowed
       FROM account_ownership_proof_bindings
      WHERE service = ?
        AND owner_user_id = ?
        AND service_account_sha256 = ?
      LIMIT 1`,
  ).get(app, account, stableIdSha256(stableId));

  return !!row;
}

function main(argv) {
  let args;
  try {
    args = parsePlaceAllowedArgs(argv);
  } catch (error) {
    console.error(error.message);
    return 2;
  }

  let db;
  try {
    db = new DatabaseSync(args.db, { readOnly: true });
    const allowed = placeAllowed(db, args);
    console.log(allowed ? "allowed" : "not allowed");
    return 0;
  } catch (error) {
    console.error(error.message);
    return 2;
  } finally {
    db?.close();
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exit(main(process.argv));
}
