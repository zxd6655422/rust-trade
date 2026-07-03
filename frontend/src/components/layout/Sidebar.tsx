'use client';

import React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { LayoutDashboard, Settings, Activity, Zap } from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

const Sidebar = () => {
  const pathname = usePathname();
  const { t } = useLanguage();

  const menuItems = [
    { label: t.sidebar.dashboard, path: '/', icon: LayoutDashboard },
    { label: t.sidebar.trading, path: '/trading', icon: Activity },
    { label: t.sidebar.settings, path: '/settings', icon: Settings },
  ];

  return (
    <aside className="w-56 h-full bg-slate-900 text-white flex flex-col">
      {/* Logo Area */}
      <div className="p-4 border-b border-slate-700">
        <div className="flex items-center gap-2">
          <div className="w-8 h-8 bg-blue-600 rounded-lg flex items-center justify-center">
            <Zap className="w-4 h-4" />
          </div>
          <div>
            <h2 className="text-sm font-bold">Rust Trade</h2>
            <p className="text-[10px] text-slate-400">{t.sidebar.quantitativeTrading}</p>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav className="flex-1 p-3">
        <ul className="space-y-1">
          {menuItems.map((item) => {
            const Icon = item.icon;
            const isActive = pathname === item.path ||
              (item.path !== '/' && pathname.startsWith(item.path));

            return (
              <li key={item.path}>
                <Link
                  href={item.path}
                  className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all ${
                    isActive
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'text-slate-300 hover:bg-slate-800 hover:text-white'
                  }`}
                >
                  <Icon className="w-4 h-4" />
                  {item.label}
                </Link>
              </li>
            );
          })}
        </ul>
      </nav>

      {/* Status Footer */}
      <div className="p-4 border-t border-slate-700">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 bg-emerald-500 rounded-full animate-pulse" />
          <span className="text-xs text-slate-400">{t.common.systemOnline}</span>
        </div>
      </div>
    </aside>
  );
};

export default Sidebar;
