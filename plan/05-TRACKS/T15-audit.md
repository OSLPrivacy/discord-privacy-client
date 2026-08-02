# T15-G5 — independent audit of onboarding, lifecycle, and recovery

**Audit date:** 2026-08-01
**Disposition:** **not ready to certify.** The checkout contains useful pieces of
T15, but several owner-required flows are either not wired into the shipping
surface or conflict with later owner decisions. A unit-tested helper is not
evidence that a user can reach it.

## Method

This audit re-derived the result from the current checkout, rather than using
the task statuses in `T15-onboarding-lifecycle.md`. It checked the frozen
`03-CONTRACTS/lifecycle.md` against the command surface, UI route graph, and
native implementation. Owner decisions D59, D71, and D80 were applied where
they supersede the earlier track wording. Windows-VM evidence was not inferred
from Linux source review.

## What is substantively present

| Area | Source evidence | Audit result |
| --- | --- | --- |
| Phrase-wrapped file key | `crates/ipc/src/main_password.rs` creates both phrase-wrap fields and recovery unwraps then rotates state before writing the new marker. | Present; this closes the original silent-orphan failure for new markers. |
| Capture dead-end exit | `recovery-kit.ts` has retry, typed `show-anyway`, and remind-later states; `main.ts` renders the corresponding exits. | Present in the web UI. |
| Single unlock credential field | `main.ts` renders one unlock password input and the burn-code input is absent. | Present and consistent with D80. |
| Fresh Start honesty | `fresh-start.ts` is imported by `main.ts`; incomplete cleanup is not presented as complete. | Present. |
| USB removal primitive | `crates/runtime/src/usb.rs` includes a volume-removal event and callback. | Primitive present only; it is not a dead-man feature. |

## Certification blockers

| ID | Disagreement / missing end-to-end behaviour | Evidence | Required owner |
| --- | --- | --- |
| A-1 | The password-recovery screen is a static shell. It has no event binding in `main.ts`, and the adapter exposes only `view_hub_recovery_phrase`; the verify-phrase and set-password commands are not registered in `hub_command_surface.rs`. A forgotten-password user cannot complete the advertised reset. | `account-recovery.ts`; `main.ts`; `adapters.ts`; `apps/osl-hub/src/hub_command_surface.rs` | T15-A3–A6 |
| A-2 | “Recovery kit not saved” is still controlled by `localStorage`. The backend has encrypted status helpers, but there is no command registration or UI invocation for them. Clearing web storage therefore clears the state that decides whether onboarding is resumed at recovery. | `account_recovery.rs`; `onboarding-resume.ts`; `main.ts`; command surface | T15-A9 |
| A-3 | Legacy-marker recovery has no honest migration surface. `recovery-migration.ts` and its test are absent. | File absence; lifecycle contract §3 | T15-A10 |
| B-1 | D71 is not implemented: the destructive wrong-password setting is a fixed `DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT = 10`. There is no onboarding choice, persisted configuration, opt-out, or route/UI for it. | `crates/ipc/src/main_password.rs`; `onboarding-sequence.ts`; `main.ts` routes | T15-B6 |
| C-1 | D59 is not implemented: canonical onboarding jumps from `privacy` to `defaults`; `tor-choice` is absent from both the sequence and the runtime route union. | `onboarding-sequence.ts`; `main.ts` `OnboardingRoute` | T15-C4 (with T7-55) |
| C-2 | The documentation is only partly reconciled. `docs/ONBOARDING.md` correctly removed the old third-party sign-in, and `unlock-and-duress.md` now names the file-storage-key consequence; however neither documents the frozen D59 Tor choice or D71 configurable automatic-erase choice. `unlock-and-duress.md` also still contains historical Discord-webview architecture text. | `docs/ONBOARDING.md`; `docs/design/unlock-and-duress.md`; lifecycle contract §§5–6 | T15-C2; T15-C3 follow-up cleanup |
| C-3 | The required non-dismissable notice after the T5 verification migration has no implementation: `breaking-change-notice.ts` is absent. | File absence; lifecycle contract §3.5 | T15-C7 (after T5 migration) |
| D-1 | Component picker and AutoScrub-consent helpers are isolated modules. No shipping import reaches either, and the required “add features later” manager is absent. | `component-picker.ts`; `component-consent.ts`; import graph; absent `component-manager.ts` | T15-D4–D6 |
| E-1 | Device transfer is not the contract protocol. The backend reseals only `identity.json` with a code and identifier; it has no destination key proof, signed authorization, full payload/manifest, anchor adoption, replay floor, or command/UI reachability. The UI manifest/source-choice modules are likewise unimported by `main.ts`. | `device_transfer.rs`; `hub_command_surface.rs`; `device-transfer*.ts`; lifecycle contract §4 | T15-E2–E5 |
| F-1 | The dead-man feature is not wired. `deadman.ts` is an isolated UI helper and there is no `apps/osl-hub/src/deadman.rs` or runtime use of `UsbMonitor` from the hub. Removal therefore cannot lock or wipe an account. | `deadman.ts`; `apps/osl-hub/src`; `crates/runtime/src/usb.rs` | T15-F4–F5 |
| G-1 | The required in-app lifecycle state table is absent (`recovery-states.ts` does not exist), so users cannot inspect the recoverable/lost outcomes frozen in the contract. | File absence; lifecycle contract §2 | T15-G4 |

## Evidence still required

No T15 Windows-VM evidence artefacts are present for recovery, the full password
set, transfer, or dead-man removal. Those checks are explicitly platform and
hardware dependent; source inspection cannot certify them. The corresponding
T15-A1, B7, E6, and F6 evidence tasks remain open.

## Re-audit gate

Do not certify T15 until every blocker above is reachable from the shipping
command and UI surfaces, each has its specified focused red/green test, and the
four Windows-VM evidence artefacts exist. In particular, a recovery UI must
perform a real phrase verification and password reset, the durable unsaved-kit
state must survive web-storage clearing, and D59/D71 must be implemented as
owner-selected onboarding behaviour rather than documented intent.
