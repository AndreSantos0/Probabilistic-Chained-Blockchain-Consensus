#!/bin/bash

# ssh -oHostKeyAlgorithms=+ssh-rsa -oPubkeyAcceptedAlgorithms=+ssh-rsa andresantos@quinta.navigators.di.fc.ul.pt
# chmod +x config.sh
# kadeploy3 -e ubuntu-20.04 -f myNodes -s config.sh

set -e

# Variables
REPO_URL="https://github.com/AndreSantos0/Probabilistic-Chained-Blockchain-Consensus.git"
REPO_DIR="Probabilistic-Chained-Blockchain-Consensus"
NODES_FILE="myNodes"   # The file listing your nodes (one node name per line)
KEYS_FILE="keys"       # The file with private:public keypairs (one per line)
SHARED_DIR="Probabilistic-Chained-Blockchain-Consensus/shared"
PUBLIC_KEYS_FILE="$SHARED_DIR/public_keys.toml"
SET_KEYS_FILE="$SHARED_DIR/set_keys.env"

# Clone or pull the repo
if [ ! -d "$REPO_DIR" ]; then
  echo "Cloning repository..."
  git clone "$REPO_URL"
  if [ $? -ne 0 ]; then
    echo "Failed to clone the repository."
    exit 1
  fi
else
  echo "Repository already exists. Updating..."
  cd "$REPO_DIR" || exit 1
  git pull origin main || git pull origin master
  cd ..
fi

echo "Repository is up to date."

# Check if cargo is installed
check_cargo() {
  if ! command -v cargo &> /dev/null
  then
    echo "Cargo is not installed. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source $HOME/.cargo/env
  fi
}

check_cargo

# Step 0: Clear previous files
echo "🧹 Cleaning old files..."
echo "" > "$PUBLIC_KEYS_FILE"
echo "" > "$SET_KEYS_FILE"

# Step 1: Generate nodes.csv
echo "📄 Generating nodes.csv..."
echo "id,host,port" > "$SHARED_DIR/nodes.csv"
node_id=0
PORT=8081

while read -r node; do
  # Get IP address of the node
  ip=$(getent hosts "$node" | awk '{ print $1 }')

  if [ -z "$ip" ]; then
    echo "❌ Could not resolve IP for node: $node"
    exit 1
  fi

  # Write to CSV
  echo "$node_id,$ip,$PORT" >> "$SHARED_DIR/nodes.csv"
  ((node_id++))
done < "$NODES_FILE"

# Step 2: Read keys and populate public_keys.toml and set_keys.env
echo "🔐 Setting up keys..."

node_id=0
while IFS= read -r line; do
  PRIVATE_KEY=$(echo "$line" | cut -d ':' -f1)
  PUBLIC_KEY=$(echo "$line" | cut -d ':' -f2)

  # Add to public_keys.toml
  {
    echo "[$node_id]"
    echo "public_key = \"$PUBLIC_KEY\""
    echo ""
  } >> "$PUBLIC_KEYS_FILE"

  # Add to set_keys.env
  {
    echo "export PRIVATE_KEY_$node_id=\"$PRIVATE_KEY\""
  } >> "$SET_KEYS_FILE"

  # Also export it in the current shell
  export "PRIVATE_KEY_$node_id"="$PRIVATE_KEY"

  ((node_id++))
done < "$KEYS_FILE"

echo "✅ Done!"
echo "Public keys written to $PUBLIC_KEYS_FILE"
echo "Private keys ready in $SET_KEYS_FILE (use: source $SET_KEYS_FILE)"