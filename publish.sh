#!/bin/sh
set -ex
cargo check --workspace
cargo check --workspace --all-features
cargo test --workspace
cargo test --workspace --all-features
cargo bench --check
cargo bench --all-features --check
cargo doc --workspace
cargo doc --workspace --all-features
cd macros
cargo publish
cd ..
cargo publish
