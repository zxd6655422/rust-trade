// src/app/layout.tsx
import './globals.css'
import Header from '@/components/layout/Header'
import Sidebar from '@/components/layout/Sidebar'
import { LanguageProvider } from '@/lib/i18n/context'

export const metadata = {
  title: 'Rust Trading System',
  description: 'Advanced trading system built with Rust, Tauri and Next.js',
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>
        <LanguageProvider>
          <div className="flex flex-col h-screen">
            <Header />
            <div className="flex flex-1 overflow-hidden">
              <Sidebar />
              <main className="flex-1 overflow-auto p-6">
                {children}
              </main>
            </div>
          </div>
        </LanguageProvider>
      </body>
    </html>
  )
}
