#!/bin/bash

# ./measure_throughput.sh 3 prob

# Exit on error
set -e

# --------- Configuration ---------
RUNTIME=60
CSV_FILE="./shared/nodes.csv"
BLOCK_FILE="./FinalizedBlocks_0.ndjson"
# ---------------------------------

# --------- Input Arguments ---------
if [ -z "$1" ]; then
  echo "Usage: $0 <txs_per_block> [prob]"
  exit 1
fi

TXS_PER_BLOCK=$1
MODE=${2:-normal}
# -----------------------------------

# Check working directory
if [ ! -f "Cargo.toml" ]; then
  echo "Please run this from the root of the cloned repository"
  exit 1
fi

# Clean previous output
rm -f "$BLOCK_FILE"

# Start nodes
echo "Starting nodes in '$MODE' mode..."
PIDS=()
tail -n +2 "$CSV_FILE" | while IFS=',' read -r id host port; do
  if [ "$MODE" == "prob" ]; then
    cargo run --package simplex --bin simplex "$id" probabilistic > "node_${id}.log" 2>&1 &
  else
    cargo run --package simplex --bin simplex "$id" > "node_${id}.log" 2>&1 &
  fi
  PIDS+=($!)
done

# Wait for protocol to run
echo "Running for $RUNTIME seconds..."
sleep "$RUNTIME"

# Kill all node processes
echo "Stopping nodes..."
for pid in "${PIDS[@]}"; do
  kill "$pid" 2>/dev/null || true
done

# Wait for files to flush
sleep 2

# Calculate throughput
BLOCK_COUNT=$(wc -l < "$BLOCK_FILE" 2>/dev/null || echo 0)
TOTAL_TXS=$((BLOCK_COUNT * TXS_PER_BLOCK))
THROUGHPUT=$(echo "$TOTAL_TXS / $RUNTIME" | bc -l)

echo "----------------------------------------"
echo "Mode:           $MODE"
echo "Blocks written: $BLOCK_COUNT"
echo "Txs/block:      $TXS_PER_BLOCK"
echo "Total txs:      $TOTAL_TXS"
printf "Throughput:     %.2f txs/sec\n" "$THROUGHPUT"
echo "----------------------------------------"