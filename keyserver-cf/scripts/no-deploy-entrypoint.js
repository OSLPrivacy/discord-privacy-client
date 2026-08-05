// Entrypoint referenced by `wrangler.contract.toml`, which exists only so that
// `wrangler d1 migrations apply --config wrangler.contract.toml` can select the
// `migrations-contract/` directory.  Wrangler requires `main` to resolve even
// for commands that never build a Worker.
//
// If this module is ever actually served, something deployed the migration
// config by mistake.  It answers 500 and says so rather than pretending to be
// the keyserver.
export default {
  fetch() {
    return new Response(
      "oslprivacy-keyserver-migrations-contract is not a Worker. " +
        "wrangler.contract.toml exists only to select migrations-contract/ for " +
        "`d1 migrations apply`. Deploy keyserver-cf/wrangler.toml instead.",
      { status: 500, headers: { "content-type": "text/plain; charset=utf-8" } },
    );
  },
};
