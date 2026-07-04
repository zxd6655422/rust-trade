'use client';

import { useEffect, useState } from 'react';
import { useLanguage } from '@/lib/i18n/context';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Settings as SettingsIcon, Globe, Moon, Sun } from 'lucide-react';

export default function Settings() {
  const { language, setLanguage, t } = useLanguage();
  const [isDark, setIsDark] = useState(true);

  // 读取 localStorage 中的主题偏好
  useEffect(() => {
    const saved = localStorage.getItem('theme');
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const dark = saved === 'dark' || (!saved && prefersDark);
    setIsDark(dark);
  }, []);

  // 切换主题并同步到 localStorage 和 DOM
  const setTheme = (dark: boolean) => {
    setIsDark(dark);
    document.documentElement.classList.toggle('dark', dark);
    localStorage.setItem('theme', dark ? 'dark' : 'light');
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <SettingsIcon className="w-6 h-6" />
        <h1 className="text-2xl font-bold">{t.settingsPage.title}</h1>
      </div>

      {/* Language Settings */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-lg">
            <Globe className="w-5 h-5" />
            {t.settingsPage.language}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-4">
            {t.settingsPage.languageDesc}
          </p>
          <div className="flex gap-3">
            <button
              onClick={() => setLanguage('zh')}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border-2 transition-all ${
                language === 'zh'
                  ? 'border-primary bg-primary/10 text-primary font-medium'
                  : 'border-muted hover:border-muted-foreground/50 text-muted-foreground hover:text-foreground'
              }`}
            >
              <span className="text-lg">🇨🇳</span>
              {t.settingsPage.chinese}
              {language === 'zh' && <Badge variant="default" className="ml-2 text-xs">当前</Badge>}
            </button>
            <button
              onClick={() => setLanguage('en')}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border-2 transition-all ${
                language === 'en'
                  ? 'border-primary bg-primary/10 text-primary font-medium'
                  : 'border-muted hover:border-muted-foreground/50 text-muted-foreground hover:text-foreground'
              }`}
            >
              <span className="text-lg">🇺🇸</span>
              {t.settingsPage.english}
              {language === 'en' && <Badge variant="default" className="ml-2 text-xs">Current</Badge>}
            </button>
          </div>
        </CardContent>
      </Card>

      {/* Theme Settings (placeholder) */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-lg">
            <Moon className="w-5 h-5" />
            {t.settingsPage.theme}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-4">
            {t.settingsPage.themeDesc}
          </p>
          <div className="flex gap-3">
            <button
              onClick={() => setTheme(false)}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border-2 transition-all ${
                !isDark
                  ? 'border-primary bg-primary/10 text-primary font-medium'
                  : 'border-muted hover:border-muted-foreground/50 text-muted-foreground hover:text-foreground'
              }`}
            >
              <Sun className="w-4 h-4" />
              {t.settings.lightMode}
              {!isDark && <Badge variant="default" className="ml-2 text-xs">当前</Badge>}
            </button>
            <button
              onClick={() => setTheme(true)}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border-2 transition-all ${
                isDark
                  ? 'border-primary bg-primary/10 text-primary font-medium'
                  : 'border-muted hover:border-muted-foreground/50 text-muted-foreground hover:text-foreground'
              }`}
            >
              <Moon className="w-4 h-4" />
              {t.settings.darkMode}
              {isDark && <Badge variant="default" className="ml-2 text-xs">当前</Badge>}
            </button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
