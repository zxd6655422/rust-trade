'use client';

import React, { useEffect, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Moon, Sun, Wifi, WifiOff, Globe, RefreshCw } from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';
import { useConnection } from '@/lib/connection-context';

const Header = () => {
  const [isDark, setIsDark] = useState(false);
  const { language, setLanguage, t } = useLanguage();
  const { tradingCore, database, lastCheck, error, checkConnection } = useConnection();

  useEffect(() => {
    const saved = localStorage.getItem('theme');
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const dark = saved === 'dark' || (!saved && prefersDark);
    setIsDark(dark);
    document.documentElement.classList.toggle('dark', dark);
  }, []);

  const toggleTheme = () => {
    const newDark = !isDark;
    setIsDark(newDark);
    document.documentElement.classList.toggle('dark', newDark);
    localStorage.setItem('theme', newDark ? 'dark' : 'light');
  };

  const toggleLanguage = () => {
    setLanguage(language === 'en' ? 'zh' : 'en');
  };

  const isConnected = tradingCore === 'connected';
  const isConnecting = tradingCore === 'connecting';

  return (
    <header className="h-14 bg-background border-b flex items-center justify-between px-6">
      <div className="flex items-center gap-4">
        <h1 className="text-lg font-semibold">{t.header.title}</h1>
        <Badge variant="outline" className="text-[10px]">v1.0</Badge>
      </div>

      <div className="flex items-center gap-3">
        {/* Connection Status */}
        <div className="flex items-center gap-1.5" title={error || undefined}>
          {isConnecting ? (
            <>
              <RefreshCw className="w-3.5 h-3.5 text-yellow-500 animate-spin" />
              <span className="text-xs text-yellow-500">{t.common.connecting || 'Connecting'}</span>
            </>
          ) : isConnected ? (
            <>
              <Wifi className="w-3.5 h-3.5 text-emerald-500" />
              <span className="text-xs text-emerald-500">{t.common.connected}</span>
            </>
          ) : (
            <>
              <WifiOff className="w-3.5 h-3.5 text-red-500" />
              <span className="text-xs text-red-500">{t.common.disconnected}</span>
            </>
          )}
        </div>

        {/* Refresh Button */}
        <Button
          variant="ghost"
          size="sm"
          onClick={checkConnection}
          disabled={isConnecting}
          className="gap-1"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${isConnecting ? 'animate-spin' : ''}`} />
        </Button>

        {/* Language Toggle */}
        <Button variant="ghost" size="sm" onClick={toggleLanguage} className="gap-1.5">
          <Globe className="w-4 h-4" />
          <span className="text-xs font-medium">{language === 'en' ? '中' : 'EN'}</span>
        </Button>

        {/* Theme Toggle */}
        <Button variant="ghost" size="sm" onClick={toggleTheme}>
          {isDark ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </Button>
      </div>
    </header>
  );
};

export default Header;
