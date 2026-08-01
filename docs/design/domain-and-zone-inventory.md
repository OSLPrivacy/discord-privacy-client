# Domain and zone inventory

## Hostnames

| hostname | decision | verification |
|---|---|---|
| `www.oslprivacy.com` | deliberately absent | `node scripts/test-www-domain.mjs` must report NXDOMAIN. |

`www.oslprivacy.com` is intentionally not a public hostname. The canonical
website hostname is `https://oslprivacy.com`; do not add website copy,
redirects, or CORS consumers that imply `www` works. If this decision changes,
provision a proxied DNS record and an HTTPS redirect to the apex in the zone,
then change this row to `redirect to apex` before deployment.
