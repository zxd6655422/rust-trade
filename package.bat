@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo 🚀 开始打包...

:: 清理
if exist dist rmdir /s /q dist
mkdir dist

:: ==================== trading-core ====================
echo 📦 编译 trading-core...
cargo build --release -p trading-core
if errorlevel 1 (
    echo ❌ 编译 trading-core 失败
    exit /b 1
)

set CORE_DIR=dist\trading-core
mkdir %CORE_DIR%\config

copy target\release\trading-core.exe %CORE_DIR%\
copy config\development.toml %CORE_DIR%\config\
copy config\production.toml %CORE_DIR%\config\

:: 创建启动脚本
(
echo @echo off
echo cd /d "%%~dp0"
echo set RUN_MODE=%%RUN_MODE%%^&if "%%RUN_MODE%%"=="" set RUN_MODE=production
echo echo 🚀 Starting Trading Core ^(mode: %%RUN_MODE%%^)...
echo trading-core.exe service
) > %CORE_DIR%\start.bat

:: ==================== trading-engine ====================
echo 📦 编译 trading-engine...
cargo build --release -p trading-engine
if errorlevel 1 (
    echo ❌ 编译 trading-engine 失败
    exit /b 1
)

set ENGINE_DIR=dist\trading-engine
mkdir %ENGINE_DIR%\config

copy target\release\trading-engine.exe %ENGINE_DIR%\
copy config\engine-development.toml %ENGINE_DIR%\config\
copy config\engine-production.toml %ENGINE_DIR%\config\

:: 创建启动脚本
(
echo @echo off
echo cd /d "%%~dp0"
echo set RUN_MODE=%%RUN_MODE%%^&if "%%RUN_MODE%%"=="" set RUN_MODE=production
echo echo 🚀 Starting Trading Engine ^(mode: %%RUN_MODE%%^)...
echo trading-engine.exe
) > %ENGINE_DIR%\start.bat

echo.
echo ✅ 打包完成!
echo.
echo 📦 生成目录:
echo   - dist\trading-core\
echo   - dist\trading-engine\
echo.
echo 📋 部署步骤:
echo   1. 复制 dist\trading-core 和 dist\trading-engine 到服务器
echo   2. 运行 start.bat 启动服务
