@echo off
setlocal
cd /d "%~dp0"
carbon.exe --config Carbon.toml %*
exit /b %ERRORLEVEL%
