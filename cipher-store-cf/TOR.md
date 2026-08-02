# T1-74 — Cloudflare Onion Routing and Tor-exit runbook

**Status:** configuration prepared; production verification is pending the
owner-provided custom-domain zone and a Tor-capable verification host.  Do not
apply this to `*.workers.dev`: Onion Routing is a **zone** setting, so the
Worker must first be reachable through the production custom domain.

## Required zone settings

1. In the custom domain's Cloudflare dashboard, open **Network** and set
   **Onion Routing** to **On**.  The API equivalent is the zone setting
   `opportunistic_onion=on`:

   ```sh
   curl --fail-with-body -X PATCH \
     "https://api.cloudflare.com/client/v4/zones/$OSL_ZONE_ID/settings/opportunistic_onion" \
     -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
     -H 'Content-Type: application/json' \
     --data '{"value":"on"}'
   ```

   The token must be limited to the production zone's **Zone Settings: Edit**
   permission.  Record the API result (without its token) with the deployment
   change.

2. In **Security → WAF → Custom rules**, add a zone-scoped rule named
   `osl-tor-exit-skip` with expression:

   ```text
   ip.src.country eq "T1"
   ```

   Use **Skip** only for the application-specific custom/rate-limit rules that
   are known to challenge Tor exits; do not skip Managed Rules.  This is a
   narrow fallback for the requests that still use an exit before the browser
   follows Onion Routing.  It is not an allow rule and it must not disable DDoS
   protection, Bot Fight Mode, or Managed Rules.

## T1-T74 deployment proof

From a host using Tor Browser (or an equivalent Tor SOCKS client), fetch the
custom-domain health endpoint twice.  The first normal-domain response must
advertise an `alt-svc` value containing an `.onion` authority; after discovery,
the subsequent connection must use Onion Routing without hardcoding that
authority.  Save the redacted response headers and the timestamp in the
deployment record.

```sh
curl --proxy socks5h://127.0.0.1:9150 -sS -D - -o /dev/null \
  https://$OSL_CIPHER_STORE_HOST/v1/healthz | grep -i '^alt-svc:'
```

Also confirm a request from a Tor exit does not receive the targeted custom or
rate-limit challenge, while a deliberately managed-rule-triggering request is
still blocked.  That second check proves the skip is not broader than intended.

## Owner decision required

The evidence above must be supplied to resolve OQ-8 (whether Tor is default
on).  Onion Routing removes the exit hop, but does not hide that the client
holds a persistent connection and does not itself settle the bandwidth/latency
trade.

## References

- <https://developers.cloudflare.com/network/onion-routing/>
- <https://developers.cloudflare.com/waf/custom-rules/skip/>
