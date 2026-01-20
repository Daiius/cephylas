#!/bin/bash

set -e

if [ $# -eq 0 ]; then
  echo "Usage: $0 <worktree1> [worktree2] ..."
  echo "Example: $0 main feat-axum-migration feat-hyper-migration"
  exit 1
fi

WORKTREES=("$@")
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
BASE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

mkdir -p "$RESULTS_DIR"

# Build TypeScript
echo "Building TypeScript..."
cd "$SCRIPT_DIR"
pnpm run build

for worktree in "${WORKTREES[@]}"; do
  echo ""
  echo "=== Testing $worktree ==="
  echo ""

  WORKTREE_PATH="$BASE_DIR/$worktree"

  if [ ! -d "$WORKTREE_PATH" ]; then
    echo "Warning: Worktree $WORKTREE_PATH does not exist, skipping..."
    continue
  fi

  # Start server (docker-compose)
  cd "$WORKTREE_PATH"
  echo "Starting server..."
  docker-compose up -d cephylas
  sleep 5  # Wait for startup

  # Run benchmark
  echo "Running benchmark..."
  k6 run --out json="$RESULTS_DIR/${worktree}.json" \
         -e API_URL=http://localhost:7878 \
         "$SCRIPT_DIR/dist/load-test.js"

  # Stop server
  echo "Stopping server..."
  docker-compose down
done

echo ""
echo "=== Comparison complete ==="
echo "Results saved to: $RESULTS_DIR"
