# TASK 0282b - the unblock-restores-reach break

`throwaway-break.patch` is the diff that was applied to a *copy* of this tree at
`/tmp/osl-0282b-copy` to prove that
`crates/ipc/tests/task_0282_unblock_lets_request_through.rs` can go red. It makes
`cmd_osl_block_friend_request` remember the allowed places it removes and
`cmd_osl_unblock_person` add them straight back, so an unblock hands the person
their old reach along with the ability to ask again.

It was never applied to this tree, and it must not be. To replay it:

```
rsync -a --exclude=node_modules --exclude=.git --exclude=target . /tmp/osl-0282b-copy/
cd /tmp/osl-0282b-copy
patch -p0 < docs/evidence/task-0282b-unblock-restore-break/throwaway-break.patch
cargo test -p ipc --test task_0282_unblock_lets_request_through -- --test-threads=1 --nocapture
```

0282 then fails at `:190` with `left: 3 / right: 0` and prints one
`TASK_0282B_RESTORED ... stable_id=discord:account-{a,b,c}:direct_message:900000000000002820`
line per place that came back. Full run: `OSL-AUDITS/evidence/0282b.md`.
