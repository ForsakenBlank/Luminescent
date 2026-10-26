@echo off
rem Starts Luminescent on Windows without a console window.
rem The first run makes .venv and installs requirements.txt (that one shows a console while it works).
cd /d "%~dp0"
if not exist ".venv\Scripts\pythonw.exe" (
    echo Setting up Luminescent for the first time...
    py -3 -m venv .venv || python -m venv .venv || goto :nopython
    ".venv\Scripts\python.exe" -m pip install --upgrade pip
    ".venv\Scripts\python.exe" -m pip install -r requirements.txt || goto :failed
)
start "" ".venv\Scripts\pythonw.exe" Launcher.py %*
exit /b 0

:nopython
echo Python 3.11 or newer is needed: https://www.python.org/downloads/ (tick "Add python.exe to PATH").
pause
exit /b 1

:failed
echo Installing the Python packages failed (see above).
pause
exit /b 1
