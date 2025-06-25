#!/bin/bash

# Create test directories
mkdir -p tests/integration
mkdir -p tests/unit
mkdir -p tests/lua
mkdir -p tests/protocol
mkdir -p tests/performance
mkdir -p tests/features
mkdir -p tests/scripts

# Make sure the script is executable
chmod +x create_test_dirs.sh

echo "Test directories created successfully."