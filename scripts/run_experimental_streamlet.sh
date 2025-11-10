#!/bin/bash

# chmod +x run_experimental.sh

set -e

NODES_FILE="myNodes"
SSH_USER="root"
REMOTE_DIR="Probabilistic-Chained-Blockchain-Consensus"
LOCAL_RESULTS_DIR="results"
SHARED_DIR="$REMOTE_DIR/shared"
SET_KEYS_FILE="$SHARED_DIR/set_keys.env"

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <transaction_size> <n_transactions> <epoch_time> [test]"
  exit 1
fi

transaction_size="$1"
n_transactions="$2"
epoch_time="$3"
shift 3

# Validate epoch_time is a valid float
if ! [[ "$epoch_time" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "Error: epoch_time must be a float64 number."
  exit 1
fi

ARGS=("$transaction_size" "$n_transactions" "$epoch_time")

for arg in "$@"; do
  case "$arg" in
    test)
      ARGS+=("test")
      ;;
    *)
      echo "Unknown mode: '$arg'. Supported optional mode is 'test'."
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
    pkill simplex
    source $SET_KEYS_FILE
    cd $REMOTE_DIR || { echo '❌ Repo dir not found'; exit 1; }
    rm -f FinalizedBlocks_$node_id.ndjson
    source \$HOME/.cargo/env
    cargo run --release --package streamlet --bin streamlet $node_id ${ARGS[*]} > /tmp/simplex_$node_id.log 2>&1
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
