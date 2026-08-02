# T13-G7 unencrypted send boundary blocker

`sensitive_warning::before_unencrypted_send` is implemented and tested as a
non-blocking decision gate. This checkout has no application-owned unencrypted
send command or adapter boundary to invoke it: shipping send paths prepare and
place protected carriers, while the browser/hosted adapters are protected-path
only.

Adding a new public-send path merely to call this warning would introduce new
product behaviour and a new exposure surface. Calling it in any existing
protected path would train users to ignore a warning for data that is already
protected, directly violating T13-G7. The module therefore remains available
for the first genuine unencrypted send boundary; it must be wired there along
with its command ACL and integration test.
