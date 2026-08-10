#!/bin/sh

# Keep the task gate's public failure contract at exit 1 even though Cargo
# reports a failing Rust test as 101. The underlying command remains the
# required focused package/test invocation with one test thread.
osl-cargo test -p task-4613-metering-privacy-driver --lib -- --test-threads=1 --nocapture
status=$?
if [ "$status" -ne 0 ]; then
  exit 1
fi
