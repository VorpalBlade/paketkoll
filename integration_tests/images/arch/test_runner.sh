#!/usr/bin/env bash

set -ex

test_dir="$1"

podman run --mount type=bind,src=$test_dir,target=/test_dir test_img /test_dir/test_runner_inner.sh
