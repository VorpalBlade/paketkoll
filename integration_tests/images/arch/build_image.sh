#!/usr/bin/env bash

set -ex

podman build . --tag test_img
