#!/usr/bin/env bash

set -ex

export PATH="/bin_dir:$PATH"

exec /test_dir/test.sh
