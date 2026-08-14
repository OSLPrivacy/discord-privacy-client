# TASK 1100 X browser machine receipt

This deployment created exactly one isolated Azure Windows 11 machine for the
X browser lane. It is intentionally an empty profile: no personal credential
or pre-existing browser session was used.

- Resource group: `OSL-X-BROWSER-1100`
- Machine: `oslx1100`
- VM region: `polandcentral` (Poland Central)
- VM size: `Standard_B2s_v2`
- Browser: Microsoft Edge `151.0.4129.72`
- Browser result: Edge remained running and loaded `https://x.com/`, with title
  `X. It's what's happening`; `signedIn=false`.

The signed-in requirement is not claimed as satisfied. The disposable X test
account could not be retrieved: the existing test Key Vault rejected the
authenticated subscription token's issuer, and the available local X logins
are imported browser entries rather than designated disposable test accounts.
