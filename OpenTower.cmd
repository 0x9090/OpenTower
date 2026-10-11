@echo off
setlocal

rem OpenTower source-tree launcher for Windows.
rem The game executable uses the Windows GUI subsystem, so no command prompt
rem remains open after this build launcher hands off to OpenTower.

cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo OpenTower requires the Rust toolchain ^(cargo^) to build from source.
    pause
    exit /b 1
)

taskkill /F /IM simtower.exe >nul 2>nul

echo Building OpenTower...
cargo build --release --package simtower-desktop
if errorlevel 1 (
    echo.
    echo OpenTower could not be built.
    pause
    exit /b 1
)

start "OpenTower" "%CD%\target\release\simtower.exe" %*
exit /b 0
