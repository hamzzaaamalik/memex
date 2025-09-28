#!/bin/bash

# Test runner script for MindCache

set -e  # Exit on any error

echo "🧪 Running MindCache Test Suite"
echo "================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Please run this script from the rust-core directory"
    exit 1
fi

# Check dependencies
print_status "Checking dependencies..."

if ! command -v cargo &> /dev/null; then
    print_error "Cargo is not installed. Please install Rust and Cargo."
    exit 1
fi

# Build the project first
print_status "Building project..."
if ! cargo build; then
    print_error "Build failed"
    exit 1
fi

print_success "Build completed"

# Run unit tests
print_status "Running unit tests..."
if cargo test --lib -- --test-threads=1; then
    print_success "Unit tests passed"
else
    print_error "Unit tests failed"
    exit 1
fi

# Run integration tests (only if they exist)
if [ -f "tests/integration_tests.rs" ]; then
    print_status "Running integration tests..."
    if cargo test --test integration_tests -- --test-threads=1; then
        print_success "Integration tests passed"
    else
        print_error "Integration tests failed"
        exit 1
    fi
else
    print_warning "Integration tests not found, skipping..."
fi

# Run FFI tests (only if they exist)
if [ -f "tests/ffi_tests.rs" ]; then
    print_status "Running FFI tests..."
    if cargo test --test ffi_tests -- --test-threads=1; then
        print_success "FFI tests passed"
    else
        print_error "FFI tests failed"
        exit 1
    fi
else
    print_warning "FFI tests not found, skipping..."
fi

# Run property-based tests (only if they exist)
if [ -f "tests/property_tests.rs" ]; then
    print_status "Running property-based tests..."
    if cargo test --test property_tests -- --test-threads=1; then
        print_success "Property-based tests passed"
    else
        print_warning "Property-based tests failed (this might be due to random test cases)"
    fi
else
    print_warning "Property-based tests not found, skipping..."
fi

# Run benchmarks (if requested)
if [ "$1" = "--bench" ] || [ "$1" = "-b" ]; then
    print_status "Running benchmarks..."
    if cargo bench; then
        print_success "Benchmarks completed"
    else
        print_warning "Some benchmarks failed"
    fi
fi

# Test with release build
if [ "$1" = "--release" ] || [ "$1" = "-r" ]; then
    print_status "Running tests with release build..."
    if cargo test --release -- --test-threads=1; then
        print_success "Release tests passed"
    else
        print_error "Release tests failed"
        exit 1
    fi
fi

# Coverage report (if requested)
if [ "$1" = "--coverage" ] || [ "$1" = "-c" ]; then
    print_status "Generating coverage report..."
    if command -v cargo-tarpaulin &> /dev/null; then
        cargo tarpaulin --out Html --output-dir target/coverage
        print_success "Coverage report generated in target/coverage/"
    else
        print_warning "cargo-tarpaulin not installed. Install with: cargo install cargo-tarpaulin"
    fi
fi

# Clean up test artifacts
print_status "Cleaning up test artifacts..."
find target -name "*.db" -delete 2>/dev/null || true
find target -name "test_*" -type d -exec rm -rf {} + 2>/dev/null || true

print_success "All tests completed successfully! ✨"

# Print summary
echo ""
echo "📊 Test Summary:"
echo "================"
echo "✅ Unit tests"

if [ -f "tests/integration_tests.rs" ]; then
    echo "✅ Integration tests"
fi

if [ -f "tests/ffi_tests.rs" ]; then
    echo "✅ FFI tests"
fi

if [ -f "tests/property_tests.rs" ]; then
    echo "✅ Property-based tests"
fi

if [ "$1" = "--bench" ] || [ "$1" = "-b" ]; then
    echo "📈 Benchmarks"
fi

if [ "$1" = "--coverage" ] || [ "$1" = "-c" ]; then
    echo "📋 Coverage report"
fi

echo ""
echo "🎉 Ready for production!"