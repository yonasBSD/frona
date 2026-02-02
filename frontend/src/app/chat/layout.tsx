"use client";

import { Suspense } from "react";
import { AuthGuard } from "@/components/auth/auth-guard";
import { NavigationProvider } from "@/lib/navigation-context";
import { SessionProvider } from "@/lib/session-context";
import { NavigationPanel } from "@/components/layout/navigation-panel";
import { ConversationPanel } from "@/components/chat/conversation-panel";

export default function ChatLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <AuthGuard>
      <NavigationProvider>
        <Suspense>
          <SessionProvider>
            <div className="flex h-screen">
              <NavigationPanel />
              <ConversationPanel>{children}</ConversationPanel>
            </div>
          </SessionProvider>
        </Suspense>
      </NavigationProvider>
    </AuthGuard>
  );
}
