"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth";
import { Logo } from "@/components/logo";

export default function Home() {
  const router = useRouter();
  const { user, loading, needsSetup } = useAuth();

  useEffect(() => {
    if (!loading) {
      if (!user) {
        router.replace("/login");
      } else if (needsSetup) {
        router.replace("/setup");
      } else {
        router.replace("/home");
      }
    }
  }, [user, loading, needsSetup, router]);

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="flex items-center justify-center gap-2">
          <Logo size={80} animate />
          <span className="text-3xl font-bold text-text-primary tracking-wide" style={{ fontFamily: "var(--font-brand)" }}>FRONA</span>
        </div>
    </div>
  );
}
