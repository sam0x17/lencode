#!/bin/sh
set -e
cargo bench --bench varint_bench 2>&1 \
  | grep --line-buffered -E 'rank' \
  | grep --line-buffered 'lencode' \
  | awk '{
      if ($0 ~ /^ *1st/) printf "\033[32m%s\033[0m\n", $0;
      else printf "\033[31m%s\033[0m\n", $0;
      fflush();
    }'
