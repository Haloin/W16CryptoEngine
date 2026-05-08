@echo off
echo Building Polymarket Engine Kernel Driver...
echo.

REM Check if WDK is installed
if not exist "C:\Program Files (x86)\Windows Kits\10\Include\*" (
    echo ERROR: Windows Driver Kit (WDK) not found!
    echo Please install WDK 10 or later from Microsoft.
    pause
    exit /b 1
)

REM Set up WDK build environment
call "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\setenv.bat" fre

REM Build the driver
echo Compiling driver...
build -cZ

if %ERRORLEVEL% EQU 0 (
    echo.
    echo SUCCESS: Driver compiled successfully!
    echo Output files should be in obj\fre_win7_amd64\ directory
) else (
    echo.
    echo ERROR: Compilation failed!
    echo Check the build log for errors.
)

pause
