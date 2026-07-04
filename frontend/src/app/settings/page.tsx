'use client';

import { useEffect, useState } from 'react';
import { useLanguage } from '@/lib/i18n/context';
import { useConnection } from '@/lib/connection-context';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Settings as SettingsIcon,
  Globe,
  Moon,
  Sun,
  Key,
  Server,
  Save,
  RefreshCw,
  CheckCircle,
  XCircle,
  Eye,
  EyeOff
} from 'lucide-react';

interface ExchangeConfig {
  id: string;
  name: string;
  apiKey: string;
  apiSecret: string;
  passphrase: string;
  testnet: boolean;
}

interface ServerConfig {
  host: string;
  port: number;
  protocol: string;
}

export default function Settings() {
  const { language, setLanguage, t } = useLanguage();
  const { tradingCore, database, checkConnection } = useConnection();
  const [isDark, setIsDark] = useState(true);

  // Exchange API 配置
  const [exchanges, setExchanges] = useState<ExchangeConfig[]>([
    { id: 'binance', name: 'Binance', apiKey: '', apiSecret: '', passphrase: '', testnet: true },
    { id: 'okx', name: 'OKX', apiKey: '', apiSecret: '', passphrase: '', testnet: true },
  ]);
  const [selectedExchange, setSelectedExchange] = useState('binance');
  const [showSecrets, setShowSecrets] = useState(false);

  // 服务器配置
  const [serverConfig, setServerConfig] = useState<ServerConfig>({
    host: 'localhost',
    port: 8080,
    protocol: 'http',
  });

  // 保存状态
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // 读取 localStorage 中的配置
  useEffect(() => {
    const savedTheme = localStorage.getItem('theme');
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const dark = savedTheme === 'dark' || (!savedTheme && prefersDark);
    setIsDark(dark);

    // 读取保存的配置
    const savedExchanges = localStorage.getItem('exchange_configs');
    if (savedExchanges) {
      try {
        setExchanges(JSON.parse(savedExchanges));
      } catch {}
    }

    const savedServer = localStorage.getItem('server_config');
    if (savedServer) {
      try {
        setServerConfig(JSON.parse(savedServer));
      } catch {}
    }
  }, []);

  // 切换主题
  const setTheme = (dark: boolean) => {
    setIsDark(dark);
    document.documentElement.classList.toggle('dark', dark);
    localStorage.setItem('theme', dark ? 'dark' : 'light');
  };

  // 保存配置
  const saveConfig = async () => {
    setSaving(true);
    try {
      localStorage.setItem('exchange_configs', JSON.stringify(exchanges));
      localStorage.setItem('server_config', JSON.stringify(serverConfig));
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (err) {
      console.error('Failed to save config:', err);
    } finally {
      setSaving(false);
    }
  };

  // 更新交易所配置
  const updateExchange = (id: string, field: keyof ExchangeConfig, value: string | boolean) => {
    setExchanges(prev => prev.map(ex =>
      ex.id === id ? { ...ex, [field]: value } : ex
    ));
  };

  // 获取当前交易所配置
  const currentExchange = exchanges.find(ex => ex.id === selectedExchange) || exchanges[0];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <SettingsIcon className="w-6 h-6" />
          <h1 className="text-2xl font-bold">{t.settingsPage.title}</h1>
        </div>
        <Button onClick={saveConfig} disabled={saving}>
          {saving ? (
            <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
          ) : saved ? (
            <CheckCircle className="w-4 h-4 mr-2 text-emerald-500" />
          ) : (
            <Save className="w-4 h-4 mr-2" />
          )}
          {saved ? '已保存' : '保存配置'}
        </Button>
      </div>

      {/* 服务状态 */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center justify-between text-lg">
            <div className="flex items-center gap-2">
              <Server className="w-5 h-5" />
              服务状态
            </div>
            <Button variant="outline" size="sm" onClick={checkConnection}>
              <RefreshCw className="w-4 h-4 mr-1" />
              刷新
            </Button>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center justify-between p-3 rounded-lg border">
              <span className="text-sm font-medium">Trading Core</span>
              {tradingCore === 'connected' ? (
                <Badge variant="default" className="bg-emerald-500">
                  <CheckCircle className="w-3 h-3 mr-1" />
                  已连接
                </Badge>
              ) : tradingCore === 'connecting' ? (
                <Badge variant="outline">
                  <RefreshCw className="w-3 h-3 mr-1 animate-spin" />
                  连接中
                </Badge>
              ) : (
                <Badge variant="destructive">
                  <XCircle className="w-3 h-3 mr-1" />
                  未连接
                </Badge>
              )}
            </div>
            <div className="flex items-center justify-between p-3 rounded-lg border">
              <span className="text-sm font-medium">Database</span>
              {database === 'connected' ? (
                <Badge variant="default" className="bg-emerald-500">
                  <CheckCircle className="w-3 h-3 mr-1" />
                  已连接
                </Badge>
              ) : (
                <Badge variant="destructive">
                  <XCircle className="w-3 h-3 mr-1" />
                  未连接
                </Badge>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 服务器配置 */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-lg">
            <Server className="w-5 h-5" />
            服务器配置
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-3 gap-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">主机地址</label>
              <Input
                value={serverConfig.host}
                onChange={(e) => setServerConfig(prev => ({ ...prev, host: e.target.value }))}
                placeholder="localhost"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">端口</label>
              <Input
                type="number"
                value={serverConfig.port}
                onChange={(e) => setServerConfig(prev => ({ ...prev, port: parseInt(e.target.value) || 8080 }))}
                placeholder="8080"
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium">协议</label>
              <select
                value={serverConfig.protocol}
                onChange={(e) => setServerConfig(prev => ({ ...prev, protocol: e.target.value }))}
                className="w-full h-10 px-3 rounded-md border border-input bg-background text-sm"
              >
                <option value="http">HTTP</option>
                <option value="https">HTTPS</option>
              </select>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 交易所 API 配置 */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-lg">
            <Key className="w-5 h-5" />
            交易所 API 配置
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* 交易所选择 */}
          <div className="flex gap-2">
            {exchanges.map(ex => (
              <Button
                key={ex.id}
                variant={selectedExchange === ex.id ? "default" : "outline"}
                onClick={() => setSelectedExchange(ex.id)}
              >
                {ex.name}
              </Button>
            ))}
          </div>

          {/* API 配置表单 */}
          <div className="space-y-4 p-4 rounded-lg border">
            <div className="flex items-center justify-between">
              <h3 className="font-medium">{currentExchange.name} API</h3>
              <div className="flex items-center gap-2">
                <label className="text-sm text-muted-foreground">Testnet</label>
                <input
                  type="checkbox"
                  checked={currentExchange.testnet}
                  onChange={(e) => updateExchange(selectedExchange, 'testnet', e.target.checked)}
                  className="h-4 w-4"
                />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <label className="text-sm font-medium">API Key</label>
                <Input
                  type={showSecrets ? "text" : "password"}
                  value={currentExchange.apiKey}
                  onChange={(e) => updateExchange(selectedExchange, 'apiKey', e.target.value)}
                  placeholder="输入 API Key"
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">API Secret</label>
                <Input
                  type={showSecrets ? "text" : "password"}
                  value={currentExchange.apiSecret}
                  onChange={(e) => updateExchange(selectedExchange, 'apiSecret', e.target.value)}
                  placeholder="输入 API Secret"
                />
              </div>
            </div>

            {selectedExchange === 'okx' && (
              <div className="space-y-2">
                <label className="text-sm font-medium">Passphrase</label>
                <Input
                  type={showSecrets ? "text" : "password"}
                  value={currentExchange.passphrase}
                  onChange={(e) => updateExchange(selectedExchange, 'passphrase', e.target.value)}
                  placeholder="输入 Passphrase"
                />
              </div>
            )}

            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowSecrets(!showSecrets)}
            >
              {showSecrets ? (
                <>
                  <EyeOff className="w-4 h-4 mr-1" />
                  隐藏密钥
                </>
              ) : (
                <>
                  <Eye className="w-4 h-4 mr-1" />
                  显示密钥
                </>
              )}
            </Button>
          </div>
        </CardContent>
      </Card>

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

      {/* Theme Settings */}
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
