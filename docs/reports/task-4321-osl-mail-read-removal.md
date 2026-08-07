# Task 4321 - OSL Mail read removal audit

## Change found

Command:

```sh
git show -s --format='TASK4321_CHANGE=%H %s' f1e2e5aeb
```

Output:

```text
TASK4321_CHANGE=f1e2e5aeb76208a62a194a36676ca5a3e68da8c9 Demote unavailable OSL Mail read bridge
```

The removal change is `f1e2e5aeb76208a62a194a36676ca5a3e68da8c9`
(`Demote unavailable OSL Mail read bridge`).

## Recovery list

| # | piece | action | evidence |
|---|---|---|---|
| 1 | `main.ts` import of `acknowledgeOslMailRetrieval` | brought back | deleted line exists in `git diff --unified=0 f1e2e5aeb^ f1e2e5aeb -- apps/osl-hub-ui/src/main.ts` |
| 2 | `main.ts` import of `listOslMailThreads` | brought back | deleted line exists in the same diff |
| 3 | `main.ts` import of `retrieveOslMailThread` | brought back | deleted line exists in the same diff |
| 4 | `refreshOslMail()` call that filled `oslMailThreads` from `listOslMailThreads()` | brought back | deleted line: `if (status?.provisioned) oslMailThreads = await listOslMailThreads() ?? [];` |
| 5 | thread button handler reading `button.dataset.mailThread` | brought back | deleted line exists in the same diff |
| 6 | thread button handler calling `retrieveOslMailThread(threadId)` | brought back | deleted line exists in the same diff |
| 7 | thread button handler setting `"Message retrieval was refused"` on failure | brought back | deleted line exists in the same diff |
| 8 | acknowledge button guard `if (!oslMailActiveThread) return;` | brought back | deleted line exists in the same diff |
| 9 | acknowledge button call to `acknowledgeOslMailRetrieval(...)` | brought back | deleted line exists in the same diff |
| 10 | acknowledge button error text after local retrieval acknowledgment | brought back | deleted line exists in the same diff |
| 11 | adapter export `listOslMailThreads()` and its parser guarded array handling | brought back | deleted 8-line function exists in `git diff --unified=0 ... -- apps/osl-hub-ui/src/osl-mail-adapter.ts` |
| 12 | adapter export `retrieveOslMailThread(threadId)` | brought back | deleted export exists in the same diff |
| 13 | adapter export `acknowledgeOslMailRetrieval(retrievalId, messageIds)` | brought back | deleted export exists in the same diff |
| 14 | backend command implementation for `osl_mail_list_threads` | written from nothing | backend history commits matching the command: `0` |
| 15 | backend command implementation for `osl_mail_retrieve_thread` | written from nothing | backend history commits matching the command: `0` |
| 16 | backend command implementation for `osl_mail_acknowledge_retrieval` | written from nothing | backend history commits matching the command: `0` |

The deleted frontend pieces can be brought back from the parent of
`f1e2e5aeb`. The three backend commands cannot be brought back because they
never existed in `apps/osl-hub/src` history; they have to be written from
nothing.

## Commands that never existed

Command:

```sh
for cmd in osl_mail_list_threads osl_mail_retrieve_thread osl_mail_acknowledge_retrieval; do
  hits=$(git log --all --format=%H -G"$cmd" -- apps/osl-hub/src | wc -l)
  printf 'TASK4321_NEVER_EXISTED_COMMAND command=%s backend_history_commits=%s\n' "$cmd" "$hits"
done
```

Output:

```text
TASK4321_NEVER_EXISTED_COMMAND command=osl_mail_list_threads backend_history_commits=0
TASK4321_NEVER_EXISTED_COMMAND command=osl_mail_retrieve_thread backend_history_commits=0
TASK4321_NEVER_EXISTED_COMMAND command=osl_mail_acknowledge_retrieval backend_history_commits=0
```

The commands it was asking for that never existed were:

- `osl_mail_list_threads`
- `osl_mail_retrieve_thread`
- `osl_mail_acknowledge_retrieval`

## Diff evidence

Command:

```sh
git diff --unified=0 f1e2e5aeb^ f1e2e5aeb -- apps/osl-hub-ui/src/osl-mail-adapter.ts apps/osl-hub-ui/src/main.ts
```

Relevant output:

```text
-  acknowledgeOslMailRetrieval,
-  listOslMailThreads,
-  retrieveOslMailThread,
-  if (status?.provisioned) oslMailThreads = await listOslMailThreads() ?? [];
-    const threadId = button.dataset.mailThread ?? "";
-    oslMailActiveThread = await retrieveOslMailThread(threadId);
-    oslMailError = oslMailActiveThread ? null : "Message retrieval was refused";
-    if (!oslMailActiveThread) return;
-    oslMailDeleteReceipt = await acknowledgeOslMailRetrieval(oslMailActiveThread.retrievalId, oslMailActiveThread.messages.map((message) => message.messageId));
-    oslMailError = oslMailDeleteReceipt ? null : "Retrieval acknowledged locally; server deletion was not requested or confirmed by this build";
-export async function listOslMailThreads(): Promise<OslMailThreadSummary[] | null> {
-    const value = await invoke<unknown>("osl_mail_list_threads");
-export const retrieveOslMailThread = (threadId: string): Promise<OslMailRetrievedThread | null> => OSL_MAIL_ID.test(threadId)
-  ? call("osl_mail_retrieve_thread", { threadId }, parseOslMailRetrievedThread)
-export const acknowledgeOslMailRetrieval = (retrievalId: string, messageIds: string[]): Promise<OslMailDeleteReceipt | null> => OSL_MAIL_ID.test(retrievalId)
-  ? call("osl_mail_acknowledge_retrieval", { retrievalId, messageIds }, parseOslMailDeleteReceipt)
```

Command:

```sh
git grep -n -e 'osl_mail_list_threads' -e 'osl_mail_retrieve_thread' -e 'osl_mail_acknowledge_retrieval' f1e2e5aeb^ -- apps/osl-hub/src apps/osl-hub-ui/src
```

Output:

```text
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:32:      case "osl_mail_list_threads":
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:34:      case "osl_mail_retrieve_thread":
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:36:      case "osl_mail_acknowledge_retrieval":
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:79:      ["osl_mail_list_threads"],
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:80:      ["osl_mail_retrieve_thread", { threadId: ID }],
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter-ipc.test.ts:81:      ["osl_mail_acknowledge_retrieval", { retrievalId: ID, messageIds: [ID] }],
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter.ts:226:    const value = await invoke<unknown>("osl_mail_list_threads");
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter.ts:234:  ? call("osl_mail_retrieve_thread", { threadId }, parseOslMailRetrievedThread)
f1e2e5aeb^:apps/osl-hub-ui/src/osl-mail-adapter.ts:239:  ? call("osl_mail_acknowledge_retrieval", { retrievalId, messageIds }, parseOslMailDeleteReceipt)
```

Command:

```sh
git log --all --oneline -G'async fn osl_mail_(list_threads|retrieve_thread|acknowledge_retrieval)|osl_mail_(list_threads|retrieve_thread|acknowledge_retrieval),' -- apps/osl-hub/src
```

Output: no lines.

## Test evidence

Command:

```sh
npx vitest run src/osl-mail-adapter.test.ts src/osl-mail-adapter-ipc.test.ts src/osl-mail-integration.test.ts src/osl-mail-view.test.ts
```

Output summary:

```text
src/osl-mail-adapter.test.ts (13 tests) passed
src/osl-mail-adapter-ipc.test.ts (3 tests) passed
src/osl-mail-view.test.ts (9 tests) passed
src/osl-mail-integration.test.ts failed before tests ran because current dirty main.ts cannot transform:
The symbol "coverInsertion" has already been declared
The symbol "setupScreen" has already been declared
The symbol "setupNavigation" has already been declared
Expected ":" but found ";"
```

Focused rerun not importing the unrelated broken `main.ts`:

```sh
npx vitest run src/osl-mail-adapter.test.ts src/osl-mail-adapter-ipc.test.ts src/osl-mail-view.test.ts
```

Output:

```text
Test Files  3 passed (3)
Tests  25 passed (25)
```

## Finish line

Mechanical check command:

```sh
awk '/^\| [0-9]+ \|/ { total++; if ($0 !~ /brought back|written from nothing/) bad++ } END { printf "TASK4321_RECOVERY_LINES=%d\nTASK4321_LINES_WITH_NEITHER_WORD=%d\n", total, bad }' /home/liamw/osl-plan/OSL-AUDITS/evidence/4321.md
```

Output:

```text
TASK4321_RECOVERY_LINES=16
TASK4321_LINES_WITH_NEITHER_WORD=0
```

- The list names the change: `f1e2e5aeb76208a62a194a36676ca5a3e68da8c9 Demote unavailable OSL Mail read bridge`
- One line per deleted piece with `brought back` or `written from nothing` beside it: `16`
- Commands that never existed: `osl_mail_list_threads`, `osl_mail_retrieve_thread`, `osl_mail_acknowledge_retrieval`
- Count of list lines with neither word: `0`
