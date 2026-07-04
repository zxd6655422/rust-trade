@echo off 
cd /d "%~dp0" 
set RUN_MODE=production 
echo Starting Trading Engine... 
trading-engine.exe 
