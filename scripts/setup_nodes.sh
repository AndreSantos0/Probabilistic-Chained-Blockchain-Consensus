#!/bin/bash

# chmod +x scripts/setup_nodes.sh
# ./scripts/setup_nodes.sh 10
# source shared/set_keys.env
# sudo pkill -f 'target/debug/simplex'

set -e

# Number of nodes, default to 4
NUM_NODES=${1:-4}

# Directory for shared files
SHARED_DIR="shared"
KEYS_FILE="public_keys.toml"

# Path to the keygen-pkcs8 binary
KEYGEN_BIN="../target/release/keygen-pkcs8"

# Function to check if cargo is installed
check_cargo() {
  if ! command -v cargo &> /dev/null
  then
    echo "Cargo is not installed. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source $HOME/.cargo/env
  fi
}

# Ensure cargo is installed
check_cargo

# Navigate to the shared directory or exit if not found
cd "$SHARED_DIR" || { echo "Shared directory not found."; exit 1; }

# Step 0: Clear previous files
echo "🧹 Removing old files..."
echo "" > "$KEYS_FILE"
echo "" > set_keys.env

# Step 1: Generate nodes.csv
echo "📄 Generating nodes.csv for $NUM_NODES nodes..."
echo "id,host,port" > nodes.csv
for ((i=0; i<NUM_NODES; i++)); do
  PORT=$((8081 + i))
  echo "$i,127.0.0.1,$PORT" >> nodes.csv
done

# Step 2: Build keygen-pkcs8 if not already built
if [ ! -f "$KEYGEN_BIN" ]; then
  echo "🔨 Building keygen-pkcs8..."
  cargo build --release -p keygen-pkcs8
fi

# Step 3: Generate PKCS#8 v2 Ed25519 keypairs
echo "🔐 Generating Ed25519 PKCS#8 key pairs..."
for ((i=0; i<NUM_NODES; i++)); do
  OUTPUT=$($KEYGEN_BIN)
  PRIVATE_BASE64=$(echo "$OUTPUT" | cut -d ':' -f1)
  PUBLIC_BASE64=$(echo "$OUTPUT" | cut -d ':' -f2)

  # Write public key to the TOML file
  echo "[$i]" >> "$KEYS_FILE"
  echo "public_key = \"$PUBLIC_BASE64\"" >> "$KEYS_FILE"
  echo "" >> "$KEYS_FILE"

  # Store private key in environment file
  export "PRIVATE_KEY_$i=$PRIVATE_BASE64"
  echo "export PRIVATE_KEY_$i=\"$PRIVATE_BASE64\"" >> set_keys.env
done

echo "✅ Done. Public keys in $KEYS_FILE. Run:"
echo "   source $SHARED_DIR/set_keys.env"