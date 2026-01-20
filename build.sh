#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "=== Agent Session Recorder Build ==="
echo

# Run tests first
echo "Running tests..."
docker build -f docker/Dockerfile --target test -t asr-test . || {
    echo "Tests failed!"
    exit 1
}
echo "Tests passed!"
echo

# Build release
echo "Building release..."
docker build -f docker/Dockerfile --target final -t asr-build .

# Extract binary
mkdir -p dist
CONTAINER_ID=$(docker create asr-build)
docker cp "$CONTAINER_ID:/asr" dist/asr
docker rm "$CONTAINER_ID" > /dev/null

chmod +x dist/asr

if [ -f "dist/asr" ]; then
    echo
    echo "Build successful!"
    echo "Binary: dist/asr"
    ls -lh dist/asr
else
    echo "Build failed - binary not found"
    exit 1
fi
