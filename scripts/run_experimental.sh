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

ARGS=()
if [[ "$2" == "test" ]] || [[ "$1" == "test" ]]; then
  ARGS+=("test")
fi
if [[ "$2" == "prob" ]] || [[ "$1" == "prob" ]]; then
  ARGS+=("probabilistic")
fi

# Clean local results directory
echo "🧹 Cleaning local results directory..."
rm -rf "$LOCAL_RESULTS_DIR"
mkdir -p "$LOCAL_RESULTS_DIR"

echo "🚀 Running simplex on all nodes in parallel for 60 seconds with args: ${ARGS[*]}"

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
    timeout 65s cargo run --package simplex --bin simplex $node_id ${ARGS[*]} > /tmp/simplex_$node_id.log 2>&1
  " &

  pids+=($!)
  ((node_id++))
done < "$NODES_FILE"

set +e  # Allow failures temporarily

# Wait for all to finish
for pid in "${pids[@]}"; do
  wait "$pid"
done

set -e  # Re-enable strict mode

echo "✅ All remote executions complete."

# Now retrieve result files
node_id=0
read -r node < "$NODES_FILE"  # Read only the first line from the file

if [[ -n "$node" ]]; then
  FILE_NAME="FinalizedBlocks_0.ndjson"
  echo "📥 Retrieving result file $FILE_NAME from node $node..."
  scp "$SSH_USER@$node:$REMOTE_DIR/$FILE_NAME" "$LOCAL_RESULTS_DIR/$FILE_NAME" || echo "⚠️ Failed to retrieve $FILE_NAME from $node"
  ((node_id++))
else
  echo "❌ No node found in $NODES_FILE"
fi

echo "✅ All results collected in '$LOCAL_RESULTS_DIR/'"

# Get the last line of the file
last_line=$(tail -n 1 "$NDJSON_FILE")

# Extract the value of "length" using grep and sed
length=$(echo "$last_line" | grep -o '"length":[0-9]*' | sed 's/[^0-9]*//')

if [ -n "$length" ]; then
  echo "Length field value: $length"
else
  echo "Could not find length field."
fi
