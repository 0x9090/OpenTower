@echo off
setlocal

rem Project-relative shortcut to the latest compiled Windows release.
set "PROJECT_DIR=%~dp0"
set "BINARY=%PROJECT_DIR%target\release\simtower.exe"

if not exist "%BINARY%" (
    echo OpenTower has not been compiled yet.
    echo Run OpenTower.cmd once to create the latest release build.
    pause
    exit /b 1
)

cd /d "%PROJECT_DIR%"
start "OpenTower" "%BINARY%" %*
exit /b 0
