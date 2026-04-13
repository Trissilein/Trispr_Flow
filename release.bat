@echo off
::  release.bat — Trispr Flow auto-release script
::
::  What it does (silently):
::    1. Bumps the patch version in package.json / tauri.conf.json / Cargo.toml
::    2. Builds all installer variants (vulkan · cuda-lite · cuda-complete)
::    3. Commits the version bump and creates a git tag
::    4. Pushes commits + tag to remote
::    5. Creates a GitHub release and uploads the installers
::
::  Requires: node, git, gh (GitHub CLI)
::  Output:   release-build.log  (full build log)
::
setlocal EnableDelayedExpansion

:: ── Configuration ─────────────────────────────────────────────────────────
set REPO=Trissilein/Trispr_Flow
set VARIANTS=vulkan cuda-lite cuda-complete
set SCRIPT_DIR=%~dp0
set LOG=%SCRIPT_DIR%release-build.log

:: ── Fresh log file ────────────────────────────────────────────────────────
echo Trispr Flow Release Log > "%LOG%"
echo Started: %DATE% %TIME% >> "%LOG%"
echo. >> "%LOG%"

echo.
echo  ============================================
echo   Trispr Flow -- Auto Release
echo  ============================================
echo.

:: ── Prerequisite checks ───────────────────────────────────────────────────
where node >nul 2>&1
if errorlevel 1 ( echo  [ERROR] node.js not found in PATH. & goto :fail )

where gh >nul 2>&1
if errorlevel 1 ( echo  [ERROR] GitHub CLI ^(gh^) not found in PATH. & echo         Install from: https://cli.github.com & goto :fail )

where git >nul 2>&1
if errorlevel 1 ( echo  [ERROR] git not found in PATH. & goto :fail )

gh auth status >nul 2>&1
if errorlevel 1 ( echo  [ERROR] Not authenticated with GitHub. Run: gh auth login & goto :fail )

:: ── 1. Bump patch version ─────────────────────────────────────────────────
echo  [1/5] Bumping patch version ...
for /f "delims=" %%v in ('node "%SCRIPT_DIR%scripts\bump-version.mjs" 2>>"%LOG%"') do set NEW_VERSION=%%v
if errorlevel 1 ( echo  [ERROR] Version bump failed. & goto :fail )
echo         ^> v!NEW_VERSION!

:: ── 2. Build all installer variants ──────────────────────────────────────
echo  [2/5] Building installers ^(may take 10-20 min^) ...
call "%SCRIPT_DIR%scripts\windows\build-installers.bat" %VARIANTS% >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [ERROR] Installer build failed. & goto :fail_restore )
echo         Installers built OK.

:: ── 3. Git commit + tag ───────────────────────────────────────────────────
echo  [3/5] Committing version bump and tagging v!NEW_VERSION! ...
git -C "%SCRIPT_DIR%." add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [ERROR] git add failed. & goto :fail )
git -C "%SCRIPT_DIR%." commit -m "chore: release v!NEW_VERSION!" >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [ERROR] git commit failed. & goto :fail )
git -C "%SCRIPT_DIR%." tag "v!NEW_VERSION!" >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [ERROR] git tag failed. Check if tag already exists. & goto :fail )
echo         Tagged v!NEW_VERSION!.

:: ── 4. Push commits and tag ───────────────────────────────────────────────
echo  [4/5] Pushing to remote ...
git -C "%SCRIPT_DIR%." push >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [WARNING] git push failed -- push manually: git push & echo. )
git -C "%SCRIPT_DIR%." push origin "v!NEW_VERSION!" >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [WARNING] Tag push failed -- push manually: git push origin v!NEW_VERSION! & echo. )

:: ── 5. Upload to GitHub Releases ─────────────────────────────────────────
echo  [5/5] Creating GitHub release and uploading assets ...
powershell -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\windows\upload-release-assets.ps1" ^
  -Tag "v!NEW_VERSION!" ^
  -Repo "%REPO%" ^
  -CreateReleaseIfMissing ^
  -Latest ^
  -Clobber >>"%LOG%" 2>&1
if errorlevel 1 ( echo  [ERROR] Upload failed. Assets may be partially uploaded. & goto :fail )
echo         Assets uploaded to GitHub.

:: ── Success ───────────────────────────────────────────────────────────────
echo.
echo  ============================================
echo   SUCCESS  --  v!NEW_VERSION! released
echo   https://github.com/%REPO%/releases
echo   Log: %LOG%
echo  ============================================
echo.
goto :end

:: ── Failure with version rollback ─────────────────────────────────────────
:fail_restore
echo  [INFO] Rolling back version files ...
git -C "%SCRIPT_DIR%." checkout -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml >>"%LOG%" 2>&1

:fail
echo.
echo  ============================================
echo   FAILED  --  see release-build.log
echo  ============================================
echo.
endlocal
pause
exit /b 1

:end
endlocal
pause
