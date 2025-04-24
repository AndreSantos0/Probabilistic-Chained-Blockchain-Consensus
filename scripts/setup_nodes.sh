#!/bin/bash

# chmod +x scripts/setup_nodes.sh
# ./scripts/setup_nodes.sh 10
# source shared/set_keys.env

set -e

NUM_NODES=${1:-4}
SHARED_DIR="shared"
KEYS_FILE="public_keys.toml"

cd "$SHARED_DIR" || { echo "Shared directory not found."; exit 1; }

# Step 0: Clear previous key and env files
echo "" > "$KEYS_FILE"
echo "" > set_keys.env

# Step 1: Generate nodes.csv
echo "📄 Generating nodes.csv for $NUM_NODES nodes..."
echo "id,host,port" > nodes.csv
for ((i=0; i<NUM_NODES; i++)); do
  PORT=$((8081 + i))
  echo "$i,127.0.0.1,$PORT" >> nodes.csv
done

# Step 2: Generate Ed25519 PKCS#8 v1 keys (compatible with from_pkcs8_maybe_unchecked)
echo "🔐 Generating Ed25519 key pairs..."

for ((i=0; i<NUM_NODES; i++)); do
  TMP_DIR=$(mktemp -d)

  # Private key in PEM format
  openssl genpkey -algorithm ED25519 -out "$TMP_DIR/private.pem"

  # Private key as DER (binary), which is what ring expects
  openssl pkcs8 -topk8 -inform PEM -outform DER -nocrypt -in "$TMP_DIR/private.pem" -out "$TMP_DIR/private.der"

  # Export public key (DER, then base64)
  PUB_BASE64=$(openssl pkey -in "$TMP_DIR/private.pem" -pubout -outform DER | base64 -w 0)
  PRIV_BASE64=$(base64 -w 0 "$TMP_DIR/private.der")

  rm -rf "$TMP_DIR"

  echo "[$i]" >> "$KEYS_FILE"
  echo "public_key = \"$PUB_BASE64\"" >> "$KEYS_FILE"
  echo "" >> "$KEYS_FILE"

  export "PRIVATE_KEY_$i=$PRIV_BASE64"
  echo "export PRIVATE_KEY_$i=\"$PRIV_BASE64\"" >> set_keys.env
done

echo "✅ Finished: public_keys.toml and env variables for private keys."
echo "ℹ️ Load private keys into your shell with: source shared/set_keys.env"