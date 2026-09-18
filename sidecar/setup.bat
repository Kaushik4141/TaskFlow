@echo off
setlocal
for %%I in ("%~dp0..") do set "ROOT=%%~fI"
set "VENV=%ROOT%\.venv-sidecar"

py -3.11 -m venv "%VENV%"
if errorlevel 1 python -m venv "%VENV%"
if errorlevel 1 exit /b 1

"%VENV%\Scripts\python.exe" -m pip install -r "%ROOT%\sidecar\requirements.txt"
if errorlevel 1 exit /b 1

echo Sidecar setup complete
