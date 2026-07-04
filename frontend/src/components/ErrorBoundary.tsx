'use client';

import React, { Component, ErrorInfo, ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { AlertCircle, RefreshCw } from 'lucide-react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error, errorInfo: null };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
    this.setState({ errorInfo });
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null, errorInfo: null });
  };

  handleReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="flex items-center justify-center min-h-[400px] p-6">
          <div className="text-center space-y-4 max-w-md">
            <div className="flex justify-center">
              <div className="p-3 rounded-full bg-red-100 dark:bg-red-900/30">
                <AlertCircle className="w-8 h-8 text-red-500" />
              </div>
            </div>
            <h2 className="text-xl font-semibold">出现错误</h2>
            <p className="text-sm text-muted-foreground">
              页面遇到了一个意外错误。请尝试刷新页面或联系技术支持。
            </p>
            {process.env.NODE_ENV === 'development' && this.state.error && (
              <div className="p-3 rounded-lg bg-muted text-left text-xs font-mono overflow-auto max-h-[200px]">
                <p className="font-bold text-red-500 mb-1">{this.state.error.message}</p>
                <p className="text-muted-foreground whitespace-pre-wrap">
                  {this.state.error.stack}
                </p>
              </div>
            )}
            <div className="flex justify-center gap-2">
              <Button variant="outline" onClick={this.handleReset}>
                <RefreshCw className="w-4 h-4 mr-2" />
                重试
              </Button>
              <Button onClick={this.handleReload}>
                刷新页面
              </Button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

/**
 * 包装组件的高阶函数
 */
export function withErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  fallback?: ReactNode
) {
  const WrappedComponent = (props: P) => (
    <ErrorBoundary fallback={fallback}>
      <Component {...props} />
    </ErrorBoundary>
  );

  WrappedComponent.displayName = `withErrorBoundary(${Component.displayName || Component.name})`;

  return WrappedComponent;
}
