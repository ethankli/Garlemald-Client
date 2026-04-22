@echo off
rem Package garlemald-client as a distributable Windows folder + .zip.
rem
rem Usage:
rem   scripts\package-windows.cmd [--debug] [--target TRIPLE]
rem
rem   --debug         Use the dev profile instead of release.
rem   --target TRIPLE Override the build target. Defaults to whatever the
rem                   per-repo .cargo\config.toml pins (i686-pc-windows-msvc),
rem                   which is required for the PE patcher to work against
rem                   the 32-bit ffxivgame.exe. Pass x86_64-pc-windows-msvc
rem                   for a launcher-only x64 build (no game launching).
rem
rem Output: target\windows-package\garlemald-client\
rem         target\windows-package\garlemald-client-<version>-<target>.zip
rem
rem Re-run-safe: the package dir is wiped before each build.
setlocal EnableExtensions EnableDelayedExpansion

rem Capture script dir before any shift -- plain `shift` rewrites %0 too,
rem which would silently corrupt %~dp0 below.
set "SCRIPT_DIR=%~dp0"
set "PROJECT_DIR=%SCRIPT_DIR%.."
for %%i in ("%PROJECT_DIR%") do set "PROJECT_DIR=%%~fi"

set "BINARY_NAME=garlemald-client"
set "PROFILE=release"
set "PROFILE_FLAG=--release"
rem Default to i686 -- matches .cargo\config.toml and is required for the
rem PE patcher to work against the 32-bit ffxivgame.exe. Override with --target.
set "TARGET=i686-pc-windows-msvc"

:parse
if "%~1"=="" goto parsed
if /i "%~1"=="--debug" (
    set "PROFILE=debug"
    set "PROFILE_FLAG="
    shift
    goto parse
)
if /i "%~1"=="--release" (
    set "PROFILE=release"
    set "PROFILE_FLAG=--release"
    shift
    goto parse
)
if /i "%~1"=="--target" (
    if "%~2"=="" (
        1>&2 echo --target requires a triple argument
        exit /b 2
    )
    set "TARGET=%~2"
    shift
    shift
    goto parse
)
if /i "%~1"=="-h"     goto :usage
if /i "%~1"=="--help" goto :usage
1>&2 echo unknown flag: %~1
1>&2 echo see --help
exit /b 2

:parsed
cd /d "%PROJECT_DIR%"

rem Pull the package version out of Cargo.toml. The /b anchor + literal "version ="
rem prefix avoids matching dependency tables like `serde = { version = "1" }`.
set "VERSION="
for /f "tokens=2 delims== " %%v in ('findstr /b /c:"version =" Cargo.toml') do (
    if not defined VERSION set "VERSION=%%~v"
)
if not defined VERSION (
    1>&2 echo error: could not parse version from Cargo.toml
    exit /b 1
)

echo ==^> Packaging garlemald-client v%VERSION% ^(%PROFILE%, %TARGET%^)

set "TARGET_FLAG=--target %TARGET%"
set "BUILT_DIR=%PROJECT_DIR%\target\%TARGET%\%PROFILE%"
echo ==^> rustup target add %TARGET%
rustup target add %TARGET% >nul 2>&1

echo ==^> cargo build %PROFILE_FLAG% %TARGET_FLAG%
cargo build %PROFILE_FLAG% %TARGET_FLAG%
if errorlevel 1 exit /b 1

if not exist "%BUILT_DIR%\%BINARY_NAME%.exe" (
    1>&2 echo    X built binary not found at %BUILT_DIR%\%BINARY_NAME%.exe
    exit /b 1
)

set "PKG_ROOT=%PROJECT_DIR%\target\windows-package"
set "PKG_DIR=%PKG_ROOT%\%BINARY_NAME%"
set "ZIP_PATH=%PKG_ROOT%\%BINARY_NAME%-%VERSION%-%TARGET%.zip"

if exist "%PKG_DIR%" rmdir /s /q "%PKG_DIR%"
if exist "%ZIP_PATH%" del /q "%ZIP_PATH%"
mkdir "%PKG_DIR%"
mkdir "%PKG_DIR%\configs"

echo ==^> Copying files
copy /y "%BUILT_DIR%\%BINARY_NAME%.exe" "%PKG_DIR%\" >nul
if exist "%PROJECT_DIR%\configs\garlemald-client.toml" (
    copy /y "%PROJECT_DIR%\configs\garlemald-client.toml" "%PKG_DIR%\configs\" >nul
)
if exist "%PROJECT_DIR%\README.md" copy /y "%PROJECT_DIR%\README.md" "%PKG_DIR%\" >nul
if exist "%PROJECT_DIR%\LICENSE"   copy /y "%PROJECT_DIR%\LICENSE"   "%PKG_DIR%\" >nul
rem icon.ico is already embedded into the exe via build.rs; we also ship
rem a sidecar copy so users can pin a custom shortcut to it if they want.
if exist "%PROJECT_DIR%\assets\icon.ico" copy /y "%PROJECT_DIR%\assets\icon.ico" "%PKG_DIR%\" >nul

echo ==^> Creating zip at %ZIP_PATH%
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command ^
    "Compress-Archive -Path '%PKG_DIR%\*' -DestinationPath '%ZIP_PATH%' -Force"
if errorlevel 1 (
    1>&2 echo    X Compress-Archive failed
    exit /b 1
)

for %%f in ("%ZIP_PATH%") do set "ZIP_BYTES=%%~zf"
echo.
echo Built %PKG_DIR%\%BINARY_NAME%.exe
echo   target: %TARGET%
echo   zip:    %ZIP_PATH% ^(%ZIP_BYTES% bytes^)
echo.
echo Launch with:
echo   "%PKG_DIR%\%BINARY_NAME%.exe"
exit /b 0

:usage
echo Usage: %~nx0 [--debug^|--release] [--target TRIPLE]
echo.
echo Builds garlemald-client and packages it under target\windows-package\
echo as both a folder and a versioned .zip. Default profile is release;
echo default target is whatever .cargo\config.toml pins ^(i686-pc-windows-msvc
echo for the PE-patcher path to work against the 32-bit FFXIV 1.x binary^).
exit /b 0
