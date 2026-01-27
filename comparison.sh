#!/bin/sh
cargo bench --color=always --features "std,comparison-bench,solana" \
  --bench codec_bench --bench solana_bench 2>&1 | grep -E 'rank|\[size\]'
