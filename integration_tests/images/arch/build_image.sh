#!/usr/bin/env bash

set -ex

podman build . --build-context binaries=../../../target/debug --tag test_img
