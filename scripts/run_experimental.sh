#!/bin/bash

# chmod +x run_experimental.sh

set -e

NODES_FILE="myNodes"
SSH_USER="root"
REMOTE_DIR="Probabilistic-Chained-Blockchain-Consensus"
LOCAL_RESULTS_DIR="results"
SHARED_DIR="$REMOTE_DIR/shared"
SET_KEYS_FILE="$SHARED_DIR/set_keys.env"
NDJSON_FILE="$LOCAL_RESULTS_DIR/FinalizedBlocks_0.ndjson"

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <transaction_size> <n_transactions> [test] [prob]"
  exit 1
fi

transaction_size="$1"
n_transactions="$2"
shift 2

ARGS=("$transaction_size" "$n_transactions")

for arg in "$@"; do
  case "$arg" in
    test)
      ARGS+=("test")
      ;;
    prob)
      ARGS+=("probabilistic")
      ;;
    *)
      echo "Unknown mode: '$arg'. Supported optional modes are 'test' and 'prob'."
      exit 1
      ;;
  esac
done

echo "🧹 Cleaning local results directory..."
rm -rf "$LOCAL_RESULTS_DIR"
mkdir -p "$LOCAL_RESULTS_DIR"

echo "🚀 Running simplex on all nodes in parallel with args: ${ARGS[*]}"

node_id=0
pids=()

while read -r node; do
  if [ -z "$node" ]; then
    continue
  fi

  echo "🔧 Starting simplex on node $node (ID: $node_id)..."

  ssh -n "$SSH_USER@$node" "
    source $SET_KEYS_FILE
    cd $REMOTE_DIR || { echo '❌ Repo dir not found'; exit 1; }
    rm -f FinalizedBlocks_$node_id.ndjson
    source \$HOME/.cargo/env
    cargo run --release --package simplex --bin simplex $node_id ${ARGS[*]} > /tmp/simplex_$node_id.log 2>&1
  " &

  pids+=($!)
  ((node_id++))
done < "$NODES_FILE"

set +e

for pid in "${pids[@]}"; do
  wait "$pid"
done

set -e

echo "✅ All remote executions complete."