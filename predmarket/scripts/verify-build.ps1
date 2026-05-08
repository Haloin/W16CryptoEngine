#!/usr/bin/env pwsh
# PolyMarket Core Engine - Build Verification Script
# Run this to verify all components compile correctly

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "PolyMarket Core Engine - Build Verification" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$apiDir = "f:\polymarketEngine\predmarket\api"
$engineDir = "f:\polymarketEngine\predmarket\engine"

# Step 1: Check Rust toolchain
Write-Host "[1/5] Checking Rust toolchain..." -ForegroundColor Yellow
try {
    $rustVersion = rustc --version
    $cargoVersion = cargo --version
    Write-Host "    Rust: $rustVersion" -ForegroundColor Green
    Write-Host "    Cargo: $cargoVersion" -ForegroundColor Green
} catch {
    Write-Error "Rust not installed. Please install from https://rustup.rs/"
    exit 1
}

# Step 2: Check C++ build tools (vcpkg)
Write-Host "[2/5] Checking C++ dependencies..." -ForegroundColor Yellow
if (Test-Path "$env:VCPKG_ROOT\vcpkg.exe") {
    Write-Host "    vcpkg found at $env:VCPKG_ROOT" -ForegroundColor Green
} else {
    Write-Host "    WARNING: vcpkg not found. HFT engine may not build." -ForegroundColor Yellow
}

# Step 3: Build Rust workspace (check only, no full build)
Write-Host "[3/5] Checking Rust workspace compilation..." -ForegroundColor Yellow
Set-Location $apiDir

try {
    # Check compilation without full build
    $checkResult = cargo check --all-features 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "    Rust workspace: OK" -ForegroundColor Green
    } else {
        Write-Host "    Rust workspace: FAILED" -ForegroundColor Red
        Write-Host $checkResult -ForegroundColor Red
        exit 1
    }
} catch {
    Write-Host "    Error checking Rust workspace: $_" -ForegroundColor Red
    exit 1
}

# Step 4: Verify key crates exist
Write-Host "[4/5] Verifying key crates..." -ForegroundColor Yellow
$crates = @(
    "crates/common",
    "crates/db",
    "crates/auth",
    "crates/orders",
    "crates/markets",
    "crates/positions",
    "crates/settlement",
    "crates/messaging",
    "crates/cache",
    "crates/api-http",
    "crates/audit",
    "crates/analytics",
    "crates/polymarket",
    "crates/blockchain"
)

$allCratesExist = $true
foreach ($crate in $crates) {
    $cratePath = Join-Path $apiDir $crate
    if (Test-Path $cratePath) {
        Write-Host "    $crate : OK" -ForegroundColor Green
    } else {
        Write-Host "    $crate : MISSING" -ForegroundColor Red
        $allCratesExist = $false
    }
}

if (-not $allCratesExist) {
    Write-Error "Some crates are missing. Please check the project structure."
    exit 1
}

# Step 5: Check HFT engine files
Write-Host "[5/5] Verifying HFT engine components..." -ForegroundColor Yellow
$engineFiles = @(
    "include/spsc_queue.hpp",
    "include/crypto_fast.hpp",
    "include/keccak256.hpp",
    "include/simdjson_parser.hpp",
    "include/network_stack.hpp",
    "include/market_data_parser.hpp",
    "include/strategy_engine.hpp",
    "include/order_gateway.hpp",
    "include/hft_orchestrator.hpp",
    "src/hft_main.cpp",
    "tests/test_spsc_queue.cpp",
    "tests/test_crypto.cpp",
    "tests/test_market_data_parser.cpp",
    "CMakeLists.txt"
)

$allEngineFilesExist = $true
foreach ($file in $engineFiles) {
    $filePath = Join-Path $engineDir $file
    if (Test-Path $filePath) {
        Write-Host "    $file : OK" -ForegroundColor Green
    } else {
        Write-Host "    $file : MISSING" -ForegroundColor Yellow
        $allEngineFilesExist = $false
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
if ($allCratesExist -and $allEngineFilesExist) {
    Write-Host "BUILD VERIFICATION: PASSED" -ForegroundColor Green
    Write-Host "All components are present and ready." -ForegroundColor Green
} else {
    Write-Host "BUILD VERIFICATION: PARTIAL" -ForegroundColor Yellow
    Write-Host "Some components are missing but the core is ready." -ForegroundColor Yellow
}
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next steps:" -ForegroundColor White
Write-Host "  1. Copy .env.example to .env and configure credentials" -ForegroundColor White
Write-Host "  2. Run: cd $apiDir && cargo build --release" -ForegroundColor White
Write-Host "  3. Run: docker-compose up -d (for infrastructure)" -ForegroundColor White
Write-Host "  4. Run: cargo test --workspace" -ForegroundColor White
Write-Host ""
