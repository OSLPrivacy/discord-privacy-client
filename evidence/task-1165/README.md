# TASK 1165 Messenger browser-machine receipt

This receipt records the non-secret state observed after provisioning the
dedicated Windows VM.  The machine is deliberately left running because this
task is a start/provision task.  It has no Messenger session: the current Azure
subscription exposes zero Key Vaults, so the documented disposable Messenger
account credentials are unavailable.  No substitute account or credential was
created.

The VM image includes Microsoft Edge.  Azure Run Command can launch a process
only in session 0, not an interactive desktop, so it cannot truthfully prove a
visible signed-in browser window.  The stored state is consequently a partial
provisioning receipt, not a claim that the task finish line passed.
