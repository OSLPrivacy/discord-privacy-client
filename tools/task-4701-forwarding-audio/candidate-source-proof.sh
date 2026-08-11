#!/usr/bin/env bash
set -euo pipefail
# Preserve upstream line numbers while normalizing whitespace for a clean,
# reviewable evidence file.
exec > >(sed -u 's/[[:space:]]\+$//')

root="${1:-/tmp/osl-task-4701-candidates}"
mkdir -p "$root"

fetch() {
  local name="$1" repo="$2" revision="$3"
  if [[ ! -d "$root/$name/.git" ]]; then
    git clone --filter=blob:none --no-checkout "$repo" "$root/$name"
  fi
  git -C "$root/$name" fetch --depth=1 origin "$revision"
  git -C "$root/$name" checkout --detach "$revision"
  printf 'CANDIDATE %s REVISION %s\n' "$name" "$(git -C "$root/$name" rev-parse HEAD)"
}

fetch livekit https://github.com/livekit/livekit.git 3b9f118327b257301083a7c4aa46076c8012918a
fetch mediasoup https://github.com/versatica/mediasoup.git f8b20b9de5831cb9cd5e2f51c2138129fd61b94f
fetch ion-sfu https://github.com/ionorg/ion-sfu.git a970af33ddc3bf8782bf49d1de4006180e3e1c08
fetch janus-gateway https://github.com/meetecho/janus-gateway.git 07c61050038c7d745013fae8bc8e99d7365c31f1

echo 'PROOF livekit forwarding and unchanged encrypted payload'
git -C "$root/livekit" show HEAD:pkg/sfu/downtrack.go | nl -ba | sed -n '1011,1044p'
git -C "$root/livekit" show HEAD:pkg/rtc/wrappedreceiver.go | nl -ba | sed -n '93,131p'
echo 'PROOF livekit mandatory media encryption and TLS-fronting requirement'
git -C "$root/livekit" show HEAD:config-sample.yaml | nl -ba | sed -n '15,17p;53,63p'
echo 'PROOF livekit one whole room selects one node'
git -C "$root/livekit" show HEAD:pkg/service/roomallocator.go | nl -ba | sed -n '134,168p'
echo 'PROOF livekit recording is a separate, omitted Egress service'
git -C "$root/livekit" show HEAD:README.md | nl -ba | sed -n '26,40p;55,60p'

echo 'PROOF mediasoup forwarding, encrypted WebRTC, clear PlainTransport, and app-owned room mapping'
git -C "$root/mediasoup" show HEAD:worker/src/RTC/Router.cpp | nl -ba | sed -n '652,688p'
git -C "$root/mediasoup" show HEAD:worker/src/RTC/Codecs/Opus.cpp | nl -ba | sed -n '88,107p'
git -C "$root/mediasoup" show HEAD:worker/src/RTC/WebRtcTransport.cpp | nl -ba | sed -n '755,790p;1027,1064p'
git -C "$root/mediasoup" show HEAD:node/src/Router.ts | nl -ba | sed -n '517,535p'
git -C "$root/mediasoup" show HEAD:node/src/WorkerTypes.ts | nl -ba | sed -n '263,276p'
git -C "$root/mediasoup" show HEAD:README.md | nl -ba | sed -n '30,43p'

echo 'PROOF ion-sfu unchanged payload, one-process sessions, clear HTTP fallback, recorder hook'
git -C "$root/ion-sfu" show HEAD:pkg/sfu/downtrack.go | nl -ba | sed -n '346,384p'
git -C "$root/ion-sfu" show HEAD:pkg/sfu/session.go | nl -ba | sed -n '220,236p'
git -C "$root/ion-sfu" show HEAD:pkg/sfu/sfu.go | nl -ba | sed -n '76,83p;190,238p'
git -C "$root/ion-sfu" show HEAD:cmd/signal/json-rpc/main.go | nl -ba | sed -n '184,191p'
git -C "$root/ion-sfu" show HEAD:README.md | nl -ba | sed -n '14,29p;88,93p'

echo 'PROOF janus VideoRoom forwarding/E2EE, clear-media switch, local room, recorder reachability'
git -C "$root/janus-gateway" show HEAD:src/plugins/janus_videoroom.c | nl -ba | sed -n '2327,2370p;8915,8929p;13333,13352p;13501,13513p;7070,7105p'
git -C "$root/janus-gateway" show HEAD:conf/janus.plugin.videoroom.jcfg.sample | nl -ba | sed -n '35,46p'
git -C "$root/janus-gateway" show HEAD:src/options.c | nl -ba | sed -n '56,61p'
git -C "$root/janus-gateway" show HEAD:src/dtls.c | nl -ba | sed -n '803,850p'
