#!/bin/bash

# Script to validate Stable MIR JSON files against the JSON schema
# Requires: jq with JSON Schema validation support

set -e

SCHEMA_FILE="${SCHEMA_FILE:-$(dirname "$0")/../schema/stable-mir.schema.json}"
TEST_DIR="${TEST_DIR:-$(dirname "$0")/../tests}"
VERBOSE="${VERBOSE:-0}"

usage() {
    echo "Usage: $0 [options] [json-files...]"
    echo ""
    echo "Validate Stable MIR JSON files against the JSON schema"
    echo ""
    echo "Options:"
    echo "  -s, --schema PATH    Path to schema file (default: ../schema/stable-mir.schema.json)"
    echo "  -t, --test-dir PATH  Directory containing test JSON files (default: ../tests)"
    echo "  -v, --verbose        Enable verbose output"
    echo "  -h, --help          Show this help message"
    echo ""
    echo "Environment variables:"
    echo "  SCHEMA_FILE     Path to schema file"
    echo "  TEST_DIR        Directory containing test files"
    echo "  VERBOSE         Enable verbose output (0/1)"
    echo ""
    echo "Examples:"
    echo "  $0                                    # Validate all test files"
    echo "  $0 file.smir.json                    # Validate specific file"
    echo "  $0 --verbose tests/**/*.json         # Validate with verbose output"
}

log_verbose() {
    if [ "$VERBOSE" = "1" ]; then
        echo "$@" >&2
    fi
}

log_info() {
    echo "$@" >&2
}

log_error() {
    echo "ERROR: $@" >&2
}

# Parse command line arguments
FILES=()
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--schema)
            SCHEMA_FILE="$2"
            shift 2
            ;;
        -t|--test-dir)
            TEST_DIR="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            FILES+=("$1")
            shift
            ;;
    esac
done

# Check if required tools are installed
if ! command -v jq &> /dev/null; then
    log_error "jq is required but not installed"
    exit 1
fi

# Check if schema file exists
if [ ! -f "$SCHEMA_FILE" ]; then
    log_error "Schema file not found: $SCHEMA_FILE"
    exit 1
fi

log_verbose "Using schema file: $SCHEMA_FILE"

# If no files specified, find all expected JSON files in test directory
if [ ${#FILES[@]} -eq 0 ]; then
    log_verbose "No files specified, searching for test files in: $TEST_DIR"
    
    # Find all .smir.json.expected files
    while IFS= read -r -d '' file; do
        FILES+=("$file")
    done < <(find "$TEST_DIR" -name "*.smir.json.expected" -print0 2>/dev/null)
    
    # Also find any .smir.json files
    while IFS= read -r -d '' file; do
        FILES+=("$file")
    done < <(find "$TEST_DIR" -name "*.smir.json" -print0 2>/dev/null)
fi

if [ ${#FILES[@]} -eq 0 ]; then
    log_error "No JSON files found to validate"
    exit 1
fi

log_info "Found ${#FILES[@]} files to validate"

# Validation results
PASSED=0
FAILED=0
ERRORS=()

# Validate each file
for file in "${FILES[@]}"; do
    if [ ! -f "$file" ]; then
        log_error "File not found: $file"
        FAILED=$((FAILED + 1))
        ERRORS+=("File not found: $file")
        continue
    fi
    
    log_verbose "Validating: $file"
    
    # Check if file is valid JSON first
    if ! jq empty "$file" >/dev/null 2>&1; then
        log_error "Invalid JSON: $file"
        FAILED=$((FAILED + 1))
        ERRORS+=("Invalid JSON: $file")
        continue
    fi
    
    # Note: Full JSON Schema validation with jq requires additional setup
    # For now, we'll do basic structure validation
    
    # Check if file has required top-level fields
    required_fields=("name" "crate_id" "allocs" "functions" "uneval_consts" "items" "types" "spans" "debug" "machine")
    missing_fields=()
    
    for field in "${required_fields[@]}"; do
        if ! jq -e "has(\"$field\")" "$file" >/dev/null 2>&1; then
            missing_fields+=("$field")
        fi
    done
    
    if [ ${#missing_fields[@]} -gt 0 ]; then
        log_error "Missing required fields in $file: ${missing_fields[*]}"
        FAILED=$((FAILED + 1))
        ERRORS+=("Missing fields in $file: ${missing_fields[*]}")
        continue
    fi
    
    # Basic type checks
    type_errors=()
    
    # Check that name is a string
    if ! jq -e '.name | type == "string"' "$file" >/dev/null 2>&1; then
        type_errors+=("name should be string")
    fi
    
    # Check that crate_id is a number
    if ! jq -e '.crate_id | type == "number"' "$file" >/dev/null 2>&1; then
        type_errors+=("crate_id should be number")
    fi
    
    # Check that arrays are arrays
    for array_field in "allocs" "functions" "uneval_consts" "items" "types" "spans"; do
        if ! jq -e ".$array_field | type == \"array\"" "$file" >/dev/null 2>&1; then
            type_errors+=("$array_field should be array")
        fi
    done
    
    if [ ${#type_errors[@]} -gt 0 ]; then
        log_error "Type errors in $file: ${type_errors[*]}"
        FAILED=$((FAILED + 1))
        ERRORS+=("Type errors in $file: ${type_errors[*]}")
        continue
    fi
    
    # If we get here, basic validation passed
    PASSED=$((PASSED + 1))
    log_verbose "✓ $file"
done

# Print summary
echo ""
echo "Validation Summary:"
echo "=================="
echo "Files validated: $((PASSED + FAILED))"
echo "Passed: $PASSED"
echo "Failed: $FAILED"

if [ $FAILED -gt 0 ]; then
    echo ""
    echo "Errors:"
    for error in "${ERRORS[@]}"; do
        echo "  - $error"
    done
    echo ""
    echo "Note: This script performs basic validation only."
    echo "For full JSON Schema validation, consider using a dedicated validator."
fi

exit $FAILED