import json
from collections import Counter

d = json.load(open('feature_report/trade_features.json', encoding='utf-8'))
print('total trades:', len(d))
print()
print('=== all-coins exit reason distribution ===')
c = Counter(t['reason'] for t in d)
for k, v in c.most_common():
    print(f'  {k}: {v}')
print()
print('=== per-coin ===')
for sym in sorted(set(t['symbol'] for t in d)):
    ts = [t for t in d if t['symbol'] == sym]
    print(sym, len(ts), dict(Counter(t['reason'] for t in ts)))
