#!/bin/bash

# chmod +x setup_nodes.sh
# ./setup_nodes.sh 5
# source setup_nodes.sh

set -e

NUM_NODES=${1:-4}
REPO_DIR="Probabilistic-Chained-Blockchain-Consensus"
SHARED_DIR="$REPO_DIR/shared"
KEYS_FILE="$SHARED_DIR/public_keys.toml"

cd "$SHARED_DIR" || { echo "Shared directory not found."; exit 1; }

# Step 1: Generate nodes.csv
echo "📄 Generating nodes.csv for $NUM_NODES nodes..."
echo "id,host,port" > nodes.csv
for ((i=0; i<NUM_NODES; i++)); do
  PORT=$((8081 + i))
  echo "$i,127.0.0.1,$PORT" >> nodes.csv
done

# Step 2: Generate Ed25519 keypairs and export
echo "🔐 Generating Ed25519 key pairs in memory..."
echo -n "" > "$KEYS_FILE"
echo -n "" > set_keys.env

for ((i=0; i<NUM_NODES; i++)); do
  # Generate keypair in memory using ssh-keygen and temp files
  TMP_DIR=$(mktemp -d)
  ssh-keygen -t ed25519 -f "$TMP_DIR/key" -N "" -q

  # Get base64 public key
  PUB_BASE64=$(cut -d' ' -f2 < "$TMP_DIR/key.pub")

  # Get base64-encoded private key (raw)
  PRIV_BASE64=$(base64 -w 0 "$TMP_DIR/key")

  # Clean up temp files
  rm -rf "$TMP_DIR"

  # Store public key in TOML
  echo "[$i]" >> "$KEYS_FILE"
  echo "public_key = \"$PUB_BASE64\"" >> "$KEYS_FILE"
  echo "" >> "$KEYS_FILE"

  # Export private key
  export "PRIVATE_KEY_$i=$PRIV_BASE64"
  echo "export PRIVATE_KEY_$i=\"$PRIV_BASE64\"" >> set_keys.env
done

echo "Finished: nodes.csv and public_keys.toml generated."
echo "To load private keys into your session, run: source set_keys.env"