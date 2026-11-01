@echo off
setlocal
echo ========================================================
echo Restoring Antigravity Conversation Index
echo ========================================================
echo.

echo Checking if Antigravity is running...
tasklist /FI "IMAGENAME eq Antigravity.exe" 2>NUL | find /I /N "Antigravity.exe">NUL
if "%ERRORLEVEL%"=="0" (
    echo [WARNING] Antigravity is still running.
    echo Please close the Antigravity IDE window completely first, then run this again.
    echo.
    pause
    exit /b 1
)

tasklist /FI "IMAGENAME eq language_server.exe" 2>NUL | find /I /N "language_server.exe">NUL
if "%ERRORLEVEL%"=="0" (
    echo Stopping background language server process...
    taskkill /F /IM language_server.exe >NUL 2>&1
    timeout /t 1 /nobreak >NUL
)

echo Running restore script...
python "%~dp0restore.py"
if errorlevel 1 (
    echo [ERROR] Restore failed.
    pause
    exit /b 1
)

pause
