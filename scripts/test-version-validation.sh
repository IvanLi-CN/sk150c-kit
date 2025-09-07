#!/bin/bash

# Test script for version validation logic used in GitHub Actions workflow
# This script tests the same validation logic that runs in the workflow

set -e

echo "🧪 Testing Version Validation Logic"
echo "=================================="

# Test function for semantic version validation
test_version_format() {
    local version="$1"
    local expected="$2"
    
    if echo "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        result="valid"
    else
        result="invalid"
    fi
    
    if [ "$result" = "$expected" ]; then
        echo "✅ $version -> $result (expected: $expected)"
    else
        echo "❌ $version -> $result (expected: $expected)"
        exit 1
    fi
}

# Test function for tag existence checking
test_tag_existence() {
    local version="$1"
    local expected="$2"
    
    if git tag -l | grep -q "^v$version$"; then
        result="exists"
    else
        result="not_exists"
    fi
    
    if [ "$result" = "$expected" ]; then
        echo "✅ Tag v$version -> $result (expected: $expected)"
    else
        echo "❌ Tag v$version -> $result (expected: $expected)"
        exit 1
    fi
}

echo ""
echo "📋 Testing Version Format Validation"
echo "-----------------------------------"

# Valid versions
test_version_format "1.0.0" "valid"
test_version_format "0.1.0" "valid"
test_version_format "10.20.30" "valid"
test_version_format "999.999.999" "valid"

# Invalid versions
test_version_format "1.0" "invalid"
test_version_format "v1.0.0" "invalid"
test_version_format "1.0.0-beta" "invalid"
test_version_format "1.0.0.1" "invalid"
test_version_format "" "invalid"
test_version_format "1.0.0-rc.1" "invalid"
test_version_format "1.0.0-alpha" "invalid"

echo ""
echo "🏷️  Testing Tag Existence Checking"
echo "--------------------------------"

# Get current tags for testing
EXISTING_TAGS=$(git tag -l)

if [ -z "$EXISTING_TAGS" ]; then
    echo "ℹ️  No existing tags found - testing with hypothetical versions"
    test_tag_existence "1.0.0" "not_exists"
    test_tag_existence "0.1.0" "not_exists"
else
    echo "ℹ️  Found existing tags: $EXISTING_TAGS"
    # Test with existing tags
    for tag in $EXISTING_TAGS; do
        if echo "$tag" | grep -qE '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
            version=$(echo "$tag" | sed 's/^v//')
            test_tag_existence "$version" "exists"
        fi
    done
    
    # Test with non-existing version
    test_tag_existence "999.999.999" "not_exists"
fi

echo ""
echo "🔄 Testing Fallback Logic"
echo "------------------------"

# Test fallback version generation
test_fallback() {
    local input_version="$1"
    local expected_output="$2"
    
    if [ -z "$input_version" ] || ! echo "$input_version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+'; then
        output_version="0.1.0"
    else
        output_version="$input_version"
    fi
    
    if [ "$output_version" = "$expected_output" ]; then
        echo "✅ Fallback: '$input_version' -> '$output_version' (expected: '$expected_output')"
    else
        echo "❌ Fallback: '$input_version' -> '$output_version' (expected: '$expected_output')"
        exit 1
    fi
}

test_fallback "" "0.1.0"
test_fallback "invalid" "0.1.0"
test_fallback "1.0" "0.1.0"
test_fallback "1.0.0" "1.0.0"
test_fallback "2.1.3" "2.1.3"

echo ""
echo "🎉 All tests passed!"
echo "==================="
echo ""
echo "The version validation logic is working correctly and ready for use in the GitHub Actions workflow."
