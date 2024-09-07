#!/usr/bin/env bash
# Expects a runnable debug build in target/debug

set -ex
exit_code=0

# Build image(s)
echo "::group::Build image(s)"
(cd images/arch && ./build_image.sh)
echo "::endgroup::"

# Run tests

for test in arch/*/; do
    echo "::group::Test: $test"
    if ! ./images/arch/test_runner.sh "$test" "../target/debug"; then
        exit_code=1
        echo "::error::FAILED: $test"
    fi
    echo "::endgroup::"
done

exit $exit_code
