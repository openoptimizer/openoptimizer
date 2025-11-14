@echo off
REM Build and run script for Cutting Optimizer

echo ========================================
echo Cutting Stock Optimizer - Build Script
echo ========================================
echo.

REM Check if Rust is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [ERROR] Cargo not found. Please install Rust from https://rustup.rs/
    pause
    exit /b 1
)

echo [INFO] Rust version:
cargo --version
echo.

REM Parse command line argument
set ACTION=%1

if "%ACTION%"=="" (
    echo Usage: build.bat [build^|cli^|api^|test^|clean]
    echo.
    echo Commands:
    echo   build  - Build all crates in release mode
    echo   cli    - Build and run CLI with example
    echo   api    - Build and run API server
    echo   test   - Run tests
    echo   clean  - Clean build artifacts
    echo.
    pause
    exit /b 0
)

if "%ACTION%"=="build" goto BUILD
if "%ACTION%"=="cli" goto CLI
if "%ACTION%"=="api" goto API
if "%ACTION%"=="test" goto TEST
if "%ACTION%"=="clean" goto CLEAN

echo [ERROR] Unknown command: %ACTION%
echo Use: build.bat [build^|cli^|api^|test^|clean]
pause
exit /b 1

:BUILD
echo [INFO] Building all crates in release mode...
echo.
cargo build --release
if %errorlevel% neq 0 (
    echo [ERROR] Build failed
    pause
    exit /b 1
)
echo.
echo [SUCCESS] Build complete!
echo.
echo Binaries are in: target\release\
echo   - optimizer.exe        (CLI tool)
echo   - optimizer-api.exe    (API server)
echo.
pause
exit /b 0

:CLI
echo [INFO] Building CLI tool...
cargo build --bin optimizer --release
if %errorlevel% neq 0 (
    echo [ERROR] Build failed
    pause
    exit /b 1
)
echo.
echo [INFO] Running CLI with simple example...
echo.
target\release\optimizer.exe optimize -i examples\simple.yaml
echo.
pause
exit /b 0

:API
echo [INFO] Building API server...
cargo build --bin optimizer-api --release
if %errorlevel% neq 0 (
    echo [ERROR] Build failed
    pause
    exit /b 1
)
echo.
echo [INFO] Starting API server...
echo.
echo The server will start on http://localhost:3000
echo Press Ctrl+C to stop the server
echo.
target\release\optimizer-api.exe
pause
exit /b 0

:TEST
echo [INFO] Running tests...
echo.
cargo test
if %errorlevel% neq 0 (
    echo [ERROR] Tests failed
    pause
    exit /b 1
)
echo.
echo [SUCCESS] All tests passed!
pause
exit /b 0

:CLEAN
echo [INFO] Cleaning build artifacts...
echo.
cargo clean
echo.
echo [SUCCESS] Clean complete!
pause
exit /b 0
