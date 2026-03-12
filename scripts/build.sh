#!/usr/bin/env bash
# Build AZCOIN pool release binary
set -e
cargo build --release -p azcoin-pool
