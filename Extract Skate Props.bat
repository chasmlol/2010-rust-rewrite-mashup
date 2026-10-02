@echo off
setlocal
set "ROOT=%~dp0"
set "CONVERTER=%ROOT%skate\iw4l-skate-convert.exe"
set "GAME_ROOT="

if not exist "%CONVERTER%" (
    echo Missing converter: "%CONVERTER%"
    echo Put iw4l-skate-convert.exe in the skate folder first.
    goto failed
)

set /p "GAME_ROOT=Enter the decompiled Skate 3 folder containing data\content\parkassets.big: "
set "GAME_ROOT=%GAME_ROOT:"=%"
if not exist "%GAME_ROOT%\data\content\parkassets.big" (
    echo Could not find data\content\parkassets.big under:
    echo "%GAME_ROOT%"
    goto failed
)

set "ASSETS=%ROOT%skate-data\assets"
if exist "%ROOT%.env" (
    for /f "usebackq tokens=1,* delims==" %%A in ("%ROOT%.env") do (
        if "%%A"=="IW4L_SKATE_ASSETS" set "ASSETS=%%B"
    )
)
set "ASSETS=%ASSETS:"=%"
set "HELP_FILE=%TEMP%\iw4l-skate-converter-help-%RANDOM%-%RANDOM%.txt"
"%CONVERTER%" --help > "%HELP_FILE%" 2>&1
findstr /c:"--props-only" "%HELP_FILE%" >nul
if not errorlevel 1 (
    del "%HELP_FILE%" >nul 2>&1
    "%CONVERTER%" --props-only --game-root "%GAME_ROOT%" --out "%ASSETS%"
    if errorlevel 1 goto failed
) else (
    del "%HELP_FILE%" >nul 2>&1
    echo.
    echo This converter is too old: it does not support --props-only.
    echo Its full conversion also does not extract the prop catalog.
    echo Build or install the updated converter, then run this script again.
    goto failed
)

echo.
echo Props are installed under "%ASSETS%\private\park-props".
echo The script uses the IW4L_SKATE_ASSETS path from .env when it is set.
echo.
pause
exit /b 0

:failed
echo.
echo Prop extraction failed. Check the folder and converter path above.
echo.
pause
exit /b 1
