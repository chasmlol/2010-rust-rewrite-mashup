@echo off
setlocal
cd /d "%~dp0"

set "IW4L_EXE="
if exist "%~dp0iw4l.exe" set "IW4L_EXE=%~dp0iw4l.exe"
if not defined IW4L_EXE if exist "%~dp0target\play\iw4l.exe" set "IW4L_EXE=%~dp0target\play\iw4l.exe"

if not defined IW4L_EXE (
    echo Could not find iw4l.exe.
    echo Build it with: cargo build --profile play -p launcher
    pause
    exit /b 1
)

set "IW4L_SKATE=off"
start "" "%IW4L_EXE%" map minecraft:overworld
