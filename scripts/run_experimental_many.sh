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
  echo "Usage: $0 <transaction_size> <n_transactions> <n_processes> [test] [prob]"
  exit 1
fi

transaction_size="$1"
n_transactions="$2"
TOTAL_PROCESSES="$3"
shift 3

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

# Read nodes into an array
nodes=()
while IFS= read -r line || [[ -n "$line" ]]; do
  nodes+=("$line")
done < "$NODES_FILE"

NUM_NODES=${#nodes[@]}

if [ $NUM_NODES -eq 0 ]; then
  echo "❌ No nodes found in $NODES_FILE"
  exit 1
fi

BASE=$(( TOTAL_PROCESSES / NUM_NODES ))
REMAINDER=$(( TOTAL_PROCESSES % NUM_NODES ))

node_id=0
pids=()

for index in "${!nodes[@]}"; do
  node="${nodes[$index]}"
  if [ -z "$node" ]; then
    continue
  fi

  COUNT=$BASE
  if [ $index -lt $REMAINDER ]; then
    COUNT=$(( COUNT + 1 ))
  fi

  echo "🔧 Starting simplex on node $node for $COUNT processes..."

  # Build remote command string
  remote_cmds=""
  for ((i=0; i<COUNT; i++)); do
    remote_cmds+="rm -f FinalizedBlocks_$node_id.ndjson; "
    remote_cmds+="nohup cargo run --release --package simplex --bin simplex $node_id ${ARGS[*]} > /tmp/simplex_$node_id.log 2>&1 & "
    ((node_id++))
  done

  ssh -n "$SSH_USER@$node" "
    source $SET_KEYS_FILE
    cd $REMOTE_DIR || { echo '❌ Repo dir not found'; exit 1; }
    source \$HOME/.cargo/env
    pkill simplex || true
    $remote_cmds
  " &

  pids+=($!)
done

# Wait for all ssh launches to complete (not the cargo processes themselves)
for pid in "${pids[@]}"; do
  wait "$pid"
done

echo "✅ All remote executions complete."
