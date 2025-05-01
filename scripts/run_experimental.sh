#!/bin/bash

# chmod +x run_experimental.sh

set -e

NODES_FILE="myNodes"
SSH_USER="root"
REMOTE_DIR="Probabilistic-Chained-Blockchain-Consensus"
LOCAL_RESULTS_DIR="results"
SHARED_DIR="$REMOTE_DIR/shared"
SET_KEYS_FILE="$SHARED_DIR/set_keys.env"

ARGS=()
if [[ "$1" == "test" ]]; then
  ARGS+=("test")
fi
if [[ "$2" == "prob" ]] || [[ "$1" == "prob" ]]; then
  ARGS+=("probabilistic")
fi

mkdir -p "$LOCAL_RESULTS_DIR"

echo "🚀 Running simplex on all nodes in parallel for 60 seconds with args: ${ARGS[*]}"

node_id=0
pids=()

while read -r node; do
  if [ -z "$node" ]; then
    continue
  fi

  FILE_NAME="FinalizedBlocks_${node_id}"
  echo "🔧 Starting simplex on node $node (ID: $node_id)..."

  ssh -n "$SSH_USER@$node" "
    source $SET_KEYS_FILE
    cd $REMOTE_DIR || { echo '❌ Repo dir not found'; exit 1; }
    source \$HOME/.cargo/env
    timeout 60s cargo run --package simplex --bin simplex $node_id ${ARGS[*]} > /tmp/simplex_$node_id.log 2>&1
  " &

  pids+=($!)  # Store PID for wait
  ((node_id++))
done < "$NODES_FILE"

# Wait for all to finish
for pid in "${pids[@]}"; do
  wait "$pid"
done

echo "✅ All remote executions complete."

# Now retrieve result files
node_id=0
while read -r node; do
  FILE_NAME="FinalizedBlocks_${node_id}"
  echo "📥 Retrieving result file $FILE_NAME from node $node..."
  scp "$SSH_USER@$node:$REMOTE_DIR/$FILE_NAME" "$LOCAL_RESULTS_DIR/$FILE_NAME" || echo "⚠️ Failed to retrieve $FILE_NAME from $node"
  ((node_id++))
done < "$NODES_FILE"

echo "✅ All results collected in '$LOCAL_RESULTS_DIR/'"
