#!/bin/bash

# Check all library files for #![forbid(unsafe_code)]
echo "Checking library files for #![forbid(unsafe_code)]..."
missing_forbid=$(grep -rL "#!\[forbid(unsafe_code)\]" prompt-hub/src/ --include="*.rs")

if [ -n "$missing_forbid" ]; then
    echo "ERROR: The following files are missing #![forbid(unsafe_code)]:"
    echo "$missing_forbid"
    exit 1
else
    echo "SUCCESS: All library files have #![forbid(unsafe_code)]."
fi

# Check for any usage of the `unsafe` keyword in actual code.
# `#![forbid(unsafe_code)]` already makes real `unsafe` a compile error, so this
# is a belt-and-suspenders check — it must ignore the word appearing in comments
# or doc-comments (e.g. "unsafe patterns", "safe replacement for `unsafe impl`")
# and must not trip over `unsafe_code` in the forbid attribute itself.
echo "Checking for 'unsafe' keyword usage..."
unsafe_usage=$(
    grep -rn --include="*.rs" --exclude-dir=tests -e 'unsafe' prompt-hub/src/ \
        | sed -E 's://.*$::' \
        | grep -E '\bunsafe[[:space:]]*(\{|fn|impl|trait|extern|\()'
)

if [ -n "$unsafe_usage" ]; then
    echo "ERROR: Unsafe code detected in the library:"
    echo "$unsafe_usage"
    exit 1
else
    echo "SUCCESS: No unsafe code detected in the library."
fi

exit 0
