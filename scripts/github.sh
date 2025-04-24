#!/bin/bash

REPO_URL="https://github.com/AndreSantos0/Probabilistic-Chained-Blockchain-Consensus.git"
REPO_DIR="Probabilistic-Chained-Blockchain-Consensus"

if [ ! -d "$REPO_DIR" ]; then
  echo "📦 Cloning repository..."
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