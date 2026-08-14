# OSL release gap list

Status: shipped-release honesty list for the merged 48-hour plan.

This list records features kept in the product direction but absent from this
release build. A feature that does not exist in this build is absent from the UI;
it is not represented by a disabled or greyed control.

## Machine-readable gap list

```json
{
  "version": 1,
  "entries": [
    {
      "id": "voice-client-absent",
      "capability": "voice",
      "releaseState": "absent",
      "clientSurface": "absent",
      "serverWork": "unaffected",
      "references": [
        "Ruling 10 keeps voice in the product direction.",
        "File 21 keeps building the server side.",
        "PLAN-48H.md merged plan ships this release without a voice client."
      ],
      "uiRule": "No shipped screen may show a live or greyed voice control; a greyed control implies the feature exists in this build."
    }
  ]
}
```
