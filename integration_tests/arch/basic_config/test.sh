#!/usr/bin/env bash

set -ex

rm -rf /test_dir/actual
mkdir /test_dir/actual

compare() {
    local ret=0
    if [[ -f /test_dir/unsorted.rn ]]; then
        mv /test_dir/unsorted.rn /test_dir/actual/${1}_unsorted.rn
    fi
    # Remove timestamps
    sed -e 's/^20[^ ]* *//' -e 's/20[-0-9]* [0-9:.]*//' -i /test_dir/actual/${1}_output.txt
    # Compare
    echo "--- Diff of ${1}_output.txt ---"
    if ! diff -Naur /test_dir/expected/${1}_output.txt /test_dir/actual/${1}_output.txt; then
        ret=1
    fi
    echo "--- Diff of ${1}_unsorted.rn ---"
    if [[ -f /test_dir/expected/${1}_unsorted.rn ]] && \
        ! diff -Naur /test_dir/expected/${1}_unsorted.rn /test_dir/actual/${1}_unsorted.rn; then
        ret=1
    fi
    return $ret
}

exit_code=0

echo "# HI!" >> /etc/ld.so.conf

echo "1: Save"
konfigkoll -c /test_dir --trust-mtime -p dry-run save 2>&1 | tee /test_dir/actual/1_output.txt
compare 1 || exit_code=1

echo "2: Diff"
konfigkoll -c /test_dir --trust-mtime -p dry-run diff /etc/passwd 2>&1 | tee /test_dir/actual/2_output.txt
compare 2 || exit_code=1

echo "3: Apply (dry-run)"
konfigkoll -c /test_dir --trust-mtime -p dry-run apply 2>&1 | tee /test_dir/actual/3_output.txt
compare 3 || exit_code=1

exit $exit_code
