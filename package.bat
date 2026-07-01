@echo off
chcp 65001 >nul

echo Starting build...

if exist dist rmdir /s /q dist
mkdir dist

echo Building trading-core...
cargo build --release -p trading-core
if errorlevel 1 goto error

mkdir dist\trading-core\config
copy target\release\trading-core.exe dist\trading-core\
copy config\development.toml dist\trading-core\config\
copy config\production.toml dist\trading-core\config\

echo @echo off > dist\trading-core\start.bat
echo cd /d "%%~dp0" >> dist\trading-core\start.bat
echo set RUN_MODE=production >> dist\trading-core\start.bat
echo echo Starting Trading Core... >> dist\trading-core\start.bat
echo trading-core.exe service >> dist\trading-core\start.bat

echo Building trading-engine...
cargo build --release -p trading-engine
if errorlevel 1 goto error

mkdir dist\trading-engine\config
copy target\release\trading-engine.exe dist\trading-engine\
copy config\engine-development.toml dist\trading-engine\config\
copy config\engine-production.toml dist\trading-engine\config\

echo @echo off > dist\trading-engine\start.bat
echo cd /d "%%~dp0" >> dist\trading-engine\start.bat
echo set RUN_MODE=production >> dist\trading-engine\start.bat
echo echo Starting Trading Engine... >> dist\trading-engine\start.bat
echo trading-engine.exe >> dist\trading-engine\start.bat

echo.
echo Build complete!
echo.
echo Output:
echo   dist\trading-core\
echo   dist\trading-engine\
echo.
echo Deploy: Copy dist folder to server, run start.bat
goto end

:error
echo Build failed!
exit /b 1

:end
