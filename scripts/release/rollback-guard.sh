#!/usr/bin/env bash
# Decide whether a proposed rollback of the OSL Privacy update feed is allowed.
#
#   scripts/release/rollback-guard.sh <target-tag> <target-is-draft> \
#                                     <current-feed-version> <target-version>
#
# Pure decision logic, no network and no side effects, so the rules that
# protect the update feed can be tested directly rather than only being
# exercised the one time somebody is already having a bad day.
#
# Exit 0 = rollback permitted. Any other exit = refused, with the reason on
# stderr. The caller is responsible for actually moving the feed.
set -uo pipefail

usage() {
  echo "usage: rollback-guard.sh <target-tag> <target-is-draft> <current-feed-version> <target-version>" >&2
  exit 2
}

[ "$#" -eq 4 ] || usage

target_tag="$1"
target_is_draft="$2"
current_feed_version="$3"
target_version="$4"

refuse() {
  echo "refused: $1" >&2
  exit 1
}

# A rollback target must name a real, well-formed candidate tag. This mirrors
# the pattern the promote workflow enforces so the two cannot drift apart.
printf '%s' "$target_tag" | grep -Eq '^hub-v[0-9A-Za-z.+-]{1,64}$' \
  || refuse "target tag '$target_tag' is not a bounded hub-v* tag"

# You can only roll back TO something users were actually given. A draft was
# never published, never passed the two-VM gate as a release, and pointing the
# update feed at one would ship an untested build to every installed client.
case "$target_is_draft" in
  false) ;;
  true) refuse "target release $target_tag is still a draft; a draft was never published and must not become the update feed" ;;
  *) refuse "target draft state must be exactly 'true' or 'false', got '$target_is_draft'" ;;
esac

[ -n "$current_feed_version" ] || refuse "current feed version is unknown; refusing to overwrite a feed we cannot identify"
[ -n "$target_version" ] || refuse "target version is empty"

# Rolling back to whatever is already live is always an operator error, and it
# would overwrite the feed for no reason while looking like a successful action.
[ "$current_feed_version" != "$target_version" ] \
  || refuse "the update feed already serves $target_version; this rollback is a no-op"

echo "permitted: roll the update feed from $current_feed_version back to $target_version ($target_tag)"
