# OSL Logo Lab

A standalone, local-only editor for the public OSL SVG mark at
`apps/osl-hub-ui/src/assets/logo-mark.svg`.

```bash
cd tools/logo-lab
npm run dev
```

Open `http://127.0.0.1:4177`. The server binds only to localhost, serves an
explicit file allowlist, disables caching, and applies a restrictive Content
Security Policy. There are no dependencies, remote requests, telemetry,
uploads, or persisted edits.

Run checks with:

```bash
npm test
```
