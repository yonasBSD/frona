"use client";

import { Suspense } from "react";
import { AuthGuard } from "@/components/auth/auth-guard";
import { NavigationProvider } from "@/lib/navigation-context";
import { SessionProvider } from "@/lib/session-context";
import { TopBar } from "@/components/layout/top-bar";

export default function MainLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <AuthGuard>
      <NavigationProvider>
        <Suspense>
          <SessionProvider>
            <div className="flex flex-col h-screen">
              <TopBar />
              <div className="flex-1 overflow-hidden">
                {children}
              </div>
            </div>
          </SessionProvider>
        </Suspense>
      </NavigationProvider>
    </AuthGuard>
  );
}
