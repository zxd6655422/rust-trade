'use client';

import React from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { DollarSign, TrendingUp, BarChart3, Target, Radio } from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

interface StatCardsProps {
  totalAssets: string;
  totalAssetsChange?: string;
  todayPnl: string;
  todayPnlCount?: string;
  positionCount: number;
  positionSymbols?: string;
  winRate: string;
  winRatePeriod?: string;
  signalCount: number;
  pendingCount?: number;
}

const StatCards: React.FC<StatCardsProps> = ({
  totalAssets,
  totalAssetsChange,
  todayPnl,
  todayPnlCount,
  positionCount,
  positionSymbols,
  winRate,
  winRatePeriod,
  signalCount,
  pendingCount,
}) => {
  const { t } = useLanguage();

  const pnlValue = parseFloat(todayPnl.replace(/[^-\d.]/g, ''));
  const isPnlPositive = pnlValue >= 0;

  const cards = [
    {
      icon: DollarSign,
      label: t.overview.totalAssets,
      value: totalAssets,
      sub: totalAssetsChange || '',
      color: 'text-blue-500',
      bgColor: 'bg-blue-500/10',
    },
    {
      icon: TrendingUp,
      label: t.overview.todayPnl,
      value: todayPnl,
      sub: todayPnlCount || '',
      color: isPnlPositive ? 'text-emerald-500' : 'text-red-500',
      bgColor: isPnlPositive ? 'bg-emerald-500/10' : 'bg-red-500/10',
      valueColor: isPnlPositive ? 'text-emerald-600 dark:text-emerald-400' : 'text-red-600 dark:text-red-400',
    },
    {
      icon: BarChart3,
      label: t.overview.positionCount,
      value: String(positionCount),
      sub: positionSymbols || '',
      color: 'text-violet-500',
      bgColor: 'bg-violet-500/10',
    },
    {
      icon: Target,
      label: t.overview.winRate,
      value: winRate,
      sub: winRatePeriod || t.overview.thisWeek,
      color: 'text-amber-500',
      bgColor: 'bg-amber-500/10',
    },
    {
      icon: Radio,
      label: t.overview.signalCount,
      value: String(signalCount),
      sub: pendingCount ? `${pendingCount} ${t.overview.pendingCount}` : '',
      color: 'text-cyan-500',
      bgColor: 'bg-cyan-500/10',
    },
  ];

  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
      {cards.map((card) => {
        const Icon = card.icon;
        return (
          <Card key={card.label}>
            <CardContent className="p-4">
              <div className="flex items-center gap-3">
                <div className={`p-2 rounded-lg ${card.bgColor}`}>
                  <Icon className={`w-4 h-4 ${card.color}`} />
                </div>
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-muted-foreground truncate">{card.label}</p>
                  <p className={`text-lg font-semibold ${card.valueColor || ''}`}>
                    {card.value}
                  </p>
                  {card.sub && (
                    <p className="text-[10px] text-muted-foreground truncate">{card.sub}</p>
                  )}
                </div>
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
};

export default StatCards;
