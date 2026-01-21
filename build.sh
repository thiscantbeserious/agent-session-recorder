#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "=== Agent Session Recorder Build ==="
echo

# Run tests first
echo "Running tests..."
docker build -f docker/Dockerfile --target test -t agr-test . || {
    echo "Tests failed!"
    exit 1
}
echo "Tests passed!"
echo

# Build release
echo "Building release..."
docker build -f docker/Dockerfile --target final -t agr-build .

# Extract binary
mkdir -p dist
CONTAINER_ID=$(docker create agr-build)
docker cp "$CONTAINER_ID:/agr" dist/agr
docker rm "$CONTAINER_ID" > /dev/null

chmod +x dist/agr

if [ -f "dist/agr" ]; then
    echo
    echo "Build successful!"
    echo "Binary: dist/agr"
    ls -lh dist/agr
else
    echo "Build failed - binary not found"
    exit 1
fi
