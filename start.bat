@echo off
cd /d "%~dp0"
echo [discord-toolbox] Starting bot...
if exist ".venv\Scripts\vc-keeper.exe" (
    ".venv\Scripts\vc-keeper.exe"
) else (
    python -m vc_keeper
)
pause