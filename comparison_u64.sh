#!/bin/sh
set -e
LENCODE_CODEC_FILTER=regular_u64 \
  cargo bench --color=always --features "std,comparison-bench" \
  --bench codec_bench -- regular_u64 2>&1 | grep -E 'rank|\[size\]|error|warning'

cargo bench --color=always --features "std,comparison-bench" \
  --bench varint_bench -- u64 2>&1 | grep -E 'rank|\[size\]|error|warning'
