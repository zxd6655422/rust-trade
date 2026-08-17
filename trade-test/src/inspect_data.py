"""快速检查 data_2026-08-13 下所有 30m CSV 的条数、日期范围、时间顺序与列名。"""
import csv
import os
from datetime import datetime

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# 数据已移到仓库外：rust-projects/data_2026-08-13
DATA = os.path.join(os.path.dirname(os.path.dirname(BASE)), "data_2026-08-13")

files = sorted(f for f in os.listdir(DATA) if f.startswith("kline_30m_"))
print(f"{'file':45} {'rows':>7}  {'min_open_time':24}  {'max_open_time':24}  order")
print("-" * 120)
for f in files:
    p = os.path.join(DATA, f)
    with open(p, "r", encoding="utf-8", newline="") as fh:
        r = csv.DictReader(fh)
        cols = r.fieldnames
        rows = list(r)
    if not rows:
        print(f"{f:45} {0:>7}")
        continue
    def ts(s):
        return datetime.strptime(s, "%Y-%m-%d %H:%M:%S.%f %z")
    times = [ts(x["open_time"]) for x in rows]
    order = "asc" if times[0] < times[-1] else "desc"
    lo = min(times).strftime("%Y-%m-%d %H:%M")
    hi = max(times).strftime("%Y-%m-%d %H:%M")
    print(f"{f:45} {len(rows):>7}  {lo:16}  {hi:16}  {order}")
    if f.endswith("_BNB.csv") or f.endswith("_SUI.csv") or f.endswith("_HYPE.csv"):
        print(f"    cols={cols}  sample_vol={rows[0].get('volume')}")
