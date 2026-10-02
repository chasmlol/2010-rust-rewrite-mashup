@echo off
setlocal EnableExtensions
set "ROOT=%~dp0"
for %%I in ("%ROOT%.") do set "ROOT=%%~fI"
cd /d "%ROOT%"
if errorlevel 1 goto failed

echo IW4L Windows build
echo.

where cargo >nul 2>&1
if errorlevel 1 (
    echo Rust Cargo was not found. Install Rust with rustup, then reopen this script.
    goto failed
)

if not exist "%ROOT%\Cargo.toml" (
    echo Cargo.toml was not found beside this script.
    goto failed
)

set "OUT=%ROOT%\target\playable"
set /p "OUT=Folder for the built game [ %ROOT%\target\playable ]: "
if not defined OUT set "OUT=%ROOT%\target\playable"
set "OUT=%OUT:"=%"
for %%I in ("%OUT%") do set "OUT=%%~fI"
if /I "%OUT%"=="%ROOT%" (
    echo Choose a separate output folder, not the source repository folder.
    goto failed
)

set "CONVERTER=%ROOT%\skate\iw4l-skate-convert.exe"
if not exist "%CONVERTER%" (
    echo.
    echo The converter is not checked into the repository because it is built
    echo separately. Provide your updated iw4l-skate-convert.exe file.
    set /p "CONVERTER=Converter executable path: "
)
set "CONVERTER=%CONVERTER:"=%"
if not exist "%CONVERTER%" (
    echo Converter executable not found: "%CONVERTER%"
    goto failed
)

"%CONVERTER%" --help 2>&1 | findstr /c:"--props-only" >nul
if errorlevel 1 (
    echo This converter is outdated. It must support --props-only.
    echo Build or obtain the current converter before running this script.
    goto failed
)

echo.
echo Building the launcher and its Cargo dependencies.
echo Cargo will compile crates in dependency order.
cargo build --locked --profile play -p launcher
if errorlevel 1 goto failed

set "GAME=%ROOT%\target\play\iw4l.exe"
if not exist "%GAME%" (
    echo Build completed but the launcher was not found at "%GAME%".
    goto failed
)

if not exist "%OUT%" mkdir "%OUT%"
if errorlevel 1 goto failed
if not exist "%OUT%\skate" mkdir "%OUT%\skate"
if errorlevel 1 goto failed

if /I not "%GAME%"=="%OUT%\iw4l.exe" copy /y "%GAME%" "%OUT%\iw4l.exe" >nul
if errorlevel 1 goto failed
if /I not "%CONVERTER%"=="%OUT%\skate\iw4l-skate-convert.exe" copy /y "%CONVERTER%" "%OUT%\skate\iw4l-skate-convert.exe" >nul
if errorlevel 1 goto failed

for %%F in ("Minecraft World.bat" "Extract Skate Props.bat" "LICENSE" "NOTICE" "crates\ui\assets\OFL-Oxanium.txt" "crates\console\assets\COPYING-FreeFont.txt") do (
    if not exist "%ROOT%\%%~F" (
        echo Required release file is missing: "%ROOT%\%%~F"
        goto failed
    )
)

copy /y "%ROOT%\Minecraft World.bat" "%OUT%\Minecraft World.bat" >nul
if errorlevel 1 goto failed
copy /y "%ROOT%\Extract Skate Props.bat" "%OUT%\Extract Skate Props.bat" >nul
if errorlevel 1 goto failed
copy /y "%ROOT%\LICENSE" "%OUT%\LICENSE" >nul
if errorlevel 1 goto failed
copy /y "%ROOT%\NOTICE" "%OUT%\NOTICE" >nul
if errorlevel 1 goto failed
copy /y "%ROOT%\crates\ui\assets\OFL-Oxanium.txt" "%OUT%\OFL-Oxanium.txt" >nul
if errorlevel 1 goto failed
copy /y "%ROOT%\crates\console\assets\COPYING-FreeFont.txt" "%OUT%\COPYING-FreeFont.txt" >nul
if errorlevel 1 goto failed

echo.
echo Build complete: "%OUT%"
echo Double-click iw4l.exe there. First-run setup will ask for your MW2 folder
echo and whether to select your Skate 3 default.xex. Game files are not copied.
echo.
pause
exit /b 0

:failed
echo.
echo Build/package failed. Review the message above.
echo.
pause
exit /b 1
