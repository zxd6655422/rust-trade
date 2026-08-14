# 研究记录索引

> 约定：每次研究/测试在此目录下建立独立子目录，包含 `report.md`（完整记录）+ 脚本引用 + `results/`（生成结果）。
> 脚本统一放在 `src/`，报告与结果放在 `studies/<编号>-<主题>/`。

## 研究列表（按时间倒序）

| 编号 | 日期 | 主题 | 关键结论 | 报告 |
|---|---|---|---|---|
| 002 | 2026-08-14 | 过滤的收益稳定性（风险调整视角） | 过滤核心价值是抗灾：6币种回撤砍半、灾年消除；**修正001：BNB应启用过滤** | [report](002-filter-risk-adjust/report.md) |
| 001 | 2026-08-14 | 逐币种 vol 阈值标定 + 样本外验证 | 高波动过滤不是跨币种通用阈值；SUI/HYPE 需单独标定（BNB 结论已被002修正） | [report](001-per-coin-vol-threshold/report.md) |

## 历史成果索引（本目录建立之前的结论，详见对应文件）

| 主题 | 关键结论 | 文件 |
|---|---|---|
| 基线回测（3币） | 2020/2021 大幅亏损，简单收益正但复利为负 | `src/backtest_report.md` |
| 多维度指标分析（27指标） | 亏损集中在「高波动+宽箱体+过度分离」，非「交织/窄箱」 | `src/feature_report/feature_analysis_report.md` |
| 样本外验证（时间/滚动/跨币种） | 高波动过滤通过四重检验 | `src/feature_report/oos_validation_report.md` |
| 新币参数矩阵 | 原模板不可迁移，新币需更紧止损+更高激活线 | `src/param_matrix_report.md` |
| 基线 vs 过滤对比 | 全局 0.522 误伤 SUI（暴露问题，引出研究001） | `src/feature_report/filter_compare.md` |
| 分币种×分年度 | 各币灾年错开，无法互相冲 | `src/feature_report/per_year_summary.md` |
| 全量结论汇总 | 综合结论 + 风险提醒 | `结论汇总.md` |
