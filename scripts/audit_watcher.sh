#!/bin/bash
# Local watcher for audit files
# Usage: bash scripts/audit_watcher.sh

AUDIT_DIR="docs/audits"
TODO_SCRIPT="scripts/update_todo_from_audit.py"

echo "Watching $AUDIT_DIR for new audits..."

# Initial state: list of files
files=$(ls $AUDIT_DIR/*.json 2>/dev/null)

while true; do
    sleep 5
    new_files=$(ls $AUDIT_DIR/*.json 2>/dev/null)
    
    for file in $new_files; do
        if [[ ! $files =~ $file ]]; then
            echo "New audit detected: $file"
            python3 "$TODO_SCRIPT" "$file"
        fi
    done
    files="$new_files"
done
