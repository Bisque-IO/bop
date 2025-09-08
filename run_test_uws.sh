#!/bin/bash
max_attempts=10000
success_count=0
error_count=0

for ((i=1; i<=max_attempts; i++)); do
    echo "Attempt $i/$max_attempts"
    
    ./build/linux/x86_64/debug/test-uws --test-case="Test Client and Server"
    exit_code=$?
    
    if [ $exit_code -eq 0 ]; then
        ((success_count++))
        echo "✓ Success (exit code: $exit_code)"
    else
        ((error_count++))
        echo "✗ Error (exit code: $exit_code)"
        echo "Stopping after $i attempts"
        break
    fi
done

echo "Summary: $success_count successful, $error_count failed"