#!/usr/bin/env bash
# Remote quiet-box lane — run oracle dumps and perf work on a Linux box over
# ssh instead of bogging down the local machine.
#
#   ./scripts/bench-remote.sh sync            # rsync the repo (fast, repeatable)
#   ./scripts/bench-remote.sh sync-corpora    # rsync corpora/ (~3.2 GB, once)
#   ./scripts/bench-remote.sh blobs           # build small+wa corpus blobs remotely
#   ./scripts/bench-remote.sh oracle <tag>    # pin all four full-fleet dumps remotely
#   ./scripts/bench-remote.sh oracle-diff <a> <b>   # diff two remote pin sets
#   ./scripts/bench-remote.sh ladder          # warm ladder, both configs, 3 books
#   ./scripts/bench-remote.sh exec '<cmd>'    # arbitrary command in the remote repo
#
# THE RULES THAT MAKE THIS SOUND:
# - Baseline and candidate ALWAYS run on the same box. The remote box is its
#   own oracle series: pin there, diff there. Never diff a remote dump against
#   a local (macOS) pin — scores flow through libm (`ln`) and float formatting,
#   which may differ across platform/libc, so cross-machine byte-diffs are not
#   guaranteed meaningful even when behavior is identical.
# - Perf numbers likewise: x86 absolutes are their own series; ratios transfer,
#   milliseconds don't.
# - Remote pins live OUTSIDE the synced repo dir (sync uses --delete) in
#   $REMOTE_PINS, so a re-sync can never eat them.
#
# Non-interactive ssh shells don't source ~/.bashrc, so cargo (rustup installs
# to ~/.cargo/bin) isn't on PATH by default — every remote command exports it.
set -euo pipefail

HOST="${SSC_REMOTE_HOST:-kellyw@172.19.144.192}"
RDIR="${SSC_REMOTE_DIR:-/home/kellyw/projects/sous-chef}"
REMOTE_PINS="${SSC_REMOTE_PINS:-/home/kellyw/projects/sous-chef-oracle}"
LOCAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

cmd="${1:?usage: bench-remote.sh <sync|sync-corpora|blobs|oracle|oracle-diff|ladder|exec> [args...]}"
shift || true

# Run a command remotely with cargo on PATH, cwd = the remote repo.
rexec() {
  ssh "$HOST" "bash -euo pipefail -s" <<REMOTE
export PATH="\$HOME/.cargo/bin:\$PATH"
cd "$RDIR"
$*
REMOTE
}

case "$cmd" in
  sync)
    # Code + git history (so remote checkouts of pinned refs work). Excluded
    # build dirs and data are protected on the receiver: --delete does not
    # remove excluded paths, so remote target/ and corpora/ survive re-syncs.
    rsync -az --delete \
      --exclude '/target/' \
      --exclude '/spike-bench/target/' \
      --exclude '/node_modules/' \
      --exclude '/corpora' \
      --exclude '/oracle-blobs' \
      --exclude '/.claude/' \
      --exclude '.DS_Store' \
      --exclude 'dhat-heap.json' \
      "$LOCAL_DIR/" "$HOST:$RDIR/"
    echo "synced -> $HOST:$RDIR"
    ;;
  sync-corpora)
    rsync -az "$LOCAL_DIR/corpora/" "$HOST:$RDIR/corpora/"
    echo "corpora synced"
    ;;
  blobs)
    rexec 'cargo build --release -p ssc-core --example calibrate --features "serde parallel"
mkdir -p oracle-blobs
./target/release/examples/calibrate --build-blob corpora/vref small oracle-blobs/small.blob
./target/release/examples/calibrate --build-blob corpora/vref wa oracle-blobs/wa.blob
ls -lh oracle-blobs/'
    ;;
  oracle)
    tag="${1:?usage: bench-remote.sh oracle <tag>}"
    rexec "cargo build --release -p ssc-core --example calibrate --features \"serde parallel\"
cargo build --release -p ssc-galley --example transcript_oracle
mkdir -p '$REMOTE_PINS/$tag'
./target/release/examples/calibrate --dump-findings corpora/vref '$REMOTE_PINS/$tag/findings.default.tsv' default full 2>/dev/null
./target/release/examples/calibrate --dump-findings corpora/vref '$REMOTE_PINS/$tag/findings.all.tsv' all full 2>/dev/null
./target/release/examples/transcript_oracle --dump-incremental corpora/vref '$REMOTE_PINS/$tag/inc.default.tsv' default full 2>/dev/null
./target/release/examples/transcript_oracle --dump-incremental corpora/vref '$REMOTE_PINS/$tag/inc.all.tsv' all full 2>/dev/null
sha256sum '$REMOTE_PINS/$tag/'*.tsv"
    ;;
  oracle-diff)
    a="${1:?usage: bench-remote.sh oracle-diff <tagA> <tagB>}"
    b="${2:?}"
    rexec "for f in findings.default findings.all inc.default inc.all; do
  if diff -q '$REMOTE_PINS/$a/'\$f.tsv '$REMOTE_PINS/$b/'\$f.tsv >/dev/null; then echo \"OK  \$f\"; else echo \"DIFF \$f\"; fi
done"
    ;;
  ladder)
    rexec 'cd spike-bench && cargo build --release 2>&1 | tail -1
for cfg in default all; do for bk in 3JN MAT PSA; do
  ./target/release/warm_ladder_profile ../corpora/vref/WA-en-ulb.txt $bk --config $cfg 2>/dev/null | grep "batch"
done; done
uptime'
    ;;
  exec)
    rexec "$@"
    ;;
  *)
    echo "unknown subcommand: $cmd" >&2
    exit 2
    ;;
esac
