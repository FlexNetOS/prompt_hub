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

# Check for any usage of 'unsafe' keyword
echo "Checking for 'unsafe' keyword usage..."
unsafe_usage=$(grep -r "unsafe " prompt-hub/src/ --exclude-dir=tests --include="*.rs")

if [ -n "$unsafe_usage" ]; then
    echo "ERROR: Unsafe code detected in the library:"
    echo "$unsafe_usage"
    exit 1
else
    echo "SUCCESS: No unsafe code detected in the library."
fi

exit 0
