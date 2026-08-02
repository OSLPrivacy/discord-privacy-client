# Deletion durability evidence

Run this only against a freshly deployed `workers.dev` preview. The probe rejects custom domains: Cloudflare documents that cached custom-domain reads can continue to serve a deleted R2 object, so they are not deletion evidence.

```sh
cd cipher-store-cf
CLOUDFLARE_API_TOKEN=… node scripts/deletion-durability-probe.mjs \
  --worker https://<preview>.workers.dev \
  --account-id <cloudflare-account-id> \
  --bucket osl-cipher-payloads-prod --yes
```

The token needs Workers R2 read access and permission to read bucket lock configuration. The probe uploads one 16-byte Padmé-valid payload, acknowledges it, and performs an immediate `HEAD` through Cloudflare's uncached R2 control API for the precise `PAYLOADS` binding key (`SHA-256(fetch_cap)`). It passes only if that response is `404`, the target bucket is the requested bucket, and its lock-rule list is empty. Its cleanup uses the independent manage capability if an earlier step fails.

R2 does not offer bucket versioning: Cloudflare's current S3 compatibility matrix marks `GetBucketVersioning` unsupported. The receipt records that fact as `versioning=unsupported-by-r2`; if Cloudflare adds versioning, this probe must be revised before relying on a deletion claim.

The probe is evidence for deletion from the serving bucket. It does not claim protection against an already completed provider backup, a compromised account, or a forward-looking legal order.
