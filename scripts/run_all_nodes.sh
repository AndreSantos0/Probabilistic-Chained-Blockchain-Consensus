#!/bin/bash

# chmod +x run_all_nodes.sh
# ./run_all_nodes.sh prob

# Exit on error
set -e

# Optional mode argument (e.g., "prob")
MODE="$1"

# Check that we're in the repo root
if [ ! -f "Cargo.toml" ]; then
  echo "Please run this script from the root of the cloned repository."
  exit 1
fi

CSV_FILE="./shared/nodes.csv"

if [ ! -f "$CSV_FILE" ]; then
  echo "nodes.csv not found in shared/. Please generate it first."
  exit 1
fi

# Read and run for each node
tail -n +2 "$CSV_FILE" | while IFS=',' read -r id host port; do
  echo "Starting node $id on $host:$port..."

  if [ "$MODE" == "prob" ]; then
    cargo run --package simplex --bin simplex "$id" probabilistic &
  else
    cargo run --package simplex --bin simplex "$id" &
  fi
done

echo "All nodes started."