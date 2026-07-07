# 外部市场API
- F:\rust_projects\rust-trade\api-docs  包含了binance、okx相关的api接口

# 文档结构
- F:\rust_projects\rust-trade\README_CN.md 应用说明
- F:\rust_projects\rust-trade\version\CHANGELOG.md 版本更新记录
- F:\rust_projects\rust-trade\version\v1.0\PLAN.md 版本开发计划
- F:\rust_projects\rust-trade\version\v1.0\README.md 版本要实现的目标
- F:\rust-projects\rust-trade\sql 所有的表结构脚本

# 开发约束
- 前端展示的地方都需要满足中英文切换
- 每个功能点要有注释，每个方法要有注释
- 功能实现避免过度设计，需要结合项目实际拥有的资源来实现
- 每次完成功能点开发都需要更新开发计划与进度文件，每一次新的需求都要记录到开发计划
- 每次完成一个开发计划都要按照时间线更新 CHANGELOG.md
- 数据库永远不要使用视图或者函数，如果有表结构变更，则需要记录到version/版本/sql 目录中，不要在单个文件中追加脚本。 应该以日期时间戳、更新目的加以区分
