#!/usr/bin/env bash
set -euo pipefail

# ── ZeroClaw Minimal Docker Setup ──────────────────────────────
# Builds the scratch-based image and starts the container.
#
# Usage:
#   ./run-minimal.sh                         # Ollama (local, free)
#   API_KEY=sk-... ./run-minimal.sh          # OpenRouter/cloud
#   PROVIDER=anthropic API_KEY=sk-... ./run-minimal.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Defaults: local Ollama
export PROVIDER="${PROVIDER:-ollama}"
export API_KEY="${API_KEY:-http://host.docker.internal:11434}"

echo "──────────────────────────────────────────────"
echo "  ZeroClaw Minimal Docker Setup"
echo "  Provider: $PROVIDER"
echo "──────────────────────────────────────────────"

# 1. Build
echo ""
echo "▸ Building scratch image (this takes a few minutes first time)..."
docker build -f Dockerfile.scratch -t zeroclaw:scratch .

# 2. Show image size
echo ""
echo "▸ Image size:"
docker images zeroclaw:scratch --format "  {{.Repository}}:{{.Tag}}  {{.Size}}"

# 3. Stop old container if running
if docker ps -a --format '{{.Names}}' | grep -q '^zeroclaw-minimal$'; then
    echo ""
    echo "▸ Stopping existing zeroclaw-minimal container..."
    docker rm -f zeroclaw-minimal >/dev/null 2>&1 || true
fi

# 4. Start
echo ""
echo "▸ Starting container..."
docker compose -f docker-compose.minimal.yml up -d

# 5. Wait for health
echo ""
echo "▸ Waiting for health check..."
for i in $(seq 1 15); do
    if docker inspect --format='{{.State.Health.Status}}' zeroclaw-minimal 2>/dev/null | grep -q healthy; then
        echo "  ✓ Healthy!"
        break
    fi
    if [ "$i" -eq 15 ]; then
        echo "  ⚠ Not healthy yet (may still be starting). Check: docker logs zeroclaw-minimal"
        break
    fi
    sleep 2
    printf "  waiting... (%ds)\n" "$((i * 2))"
done

# 6. Summary
echo ""
echo "──────────────────────────────────────────────"
echo "  ✓ ZeroClaw running on http://localhost:42617"
echo ""
echo "  Useful commands:"
echo "    docker logs -f zeroclaw-minimal    # follow logs"
echo "    docker stats zeroclaw-minimal      # resource usage"
echo "    docker compose -f docker-compose.minimal.yml down  # stop"
echo "──────────────────────────────────────────────"
