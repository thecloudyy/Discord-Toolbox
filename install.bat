@echo off
setlocal
cd /d "%~dp0"

python --version >nul 2>&1
if %errorLevel% neq 0 (
    py --version >nul 2>&1
    if %errorLevel% neq 0 (
        echo [discord-toolbox] Error: Python 3.12+ is required.
        pause
        exit /b 1
    )
    set "PYTHON_CMD=py"
) else (
    set "PYTHON_CMD=python"
)

echo [discord-toolbox] Running installer...
%PYTHON_CMD% install.py
pause