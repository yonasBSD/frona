"use client";

import { useState, useRef, useEffect } from "react";
import { usePathname, useRouter } from "next/navigation";
import { Cog6ToothIcon, ArrowRightStartOnRectangleIcon } from "@heroicons/react/24/outline";
import { useAuth } from "@/lib/auth";
import { useSession } from "@/lib/session-context";
import { Logo } from "../logo";
import { NotificationDropdown } from "./notification-dropdown";

const topTabs = [
  { id: "chat", label: "Assistant", href: "/chat" },
  { id: "files", label: "Files", href: "/files" },
] as const;

export function TopBar() {
  const router = useRouter();
  const pathname = usePathname();
  const { user, logout } = useAuth();
  const { inferring } = useSession();

  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  const activeTab = topTabs.find((t) => pathname.startsWith(t.href))?.id ?? null;

  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  const handleLogout = () => {
    logout();
    router.replace("/login");
  };

  const initial = user?.email?.charAt(0)?.toUpperCase() ?? "?";

  return (
    <div className="flex items-stretch h-20 pr-5 bg-surface-nav border-b border-border shrink-0">
      {/* Left: Logo + brand — matches nav panel width */}
      <div className="flex items-center justify-center shrink-0" style={{ width: 288 }}>
        <button
          onClick={() => router.push("/chat")}
          className="flex items-center gap-1 cursor-pointer"
        >
          <Logo size={68} animate={inferring} />
          <span
            className="text-2xl font-bold text-text-primary tracking-wide"
            style={{ fontFamily: "var(--font-brand)" }}
          >
            FRONA
          </span>
        </button>
      </div>

      {/* Top-level tabs — bottom-aligned with tab shape */}
      <div className="flex items-end self-end gap-1">
        {topTabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => router.push(tab.href)}
            className={`w-28 text-center py-2 text-base font-medium transition rounded-t-lg border border-b-0 ${
              activeTab === tab.id
                ? "bg-surface border-border text-text-primary"
                : "bg-transparent border-transparent text-text-secondary hover:text-text-primary hover:bg-surface-tertiary/50"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Right: Notifications + User profile */}
      <div className="flex items-center gap-2">
      <NotificationDropdown />
      <div ref={menuRef} className="relative flex items-center">
        <button
          onClick={() => setMenuOpen((v) => !v)}
          className="flex items-center justify-center h-10 w-10 rounded-full bg-accent text-surface text-sm font-semibold cursor-pointer hover:bg-accent-hover transition"
          title={user?.email ?? "User"}
        >
          {initial}
        </button>

        {menuOpen && (
          <div className="absolute right-0 top-full z-20 mt-2 w-52 rounded-lg border border-border bg-surface-secondary shadow-lg py-1">
            {user && (
              <div className="px-4 py-2 border-b border-border">
                <p className="text-sm font-medium text-text-primary truncate">{user.email}</p>
              </div>
            )}
            <button
              onClick={() => { router.push("/settings"); setMenuOpen(false); }}
              className="w-full flex items-center gap-2 px-4 py-2 text-sm text-text-secondary hover:bg-surface-tertiary hover:text-text-primary transition"
            >
              <Cog6ToothIcon className="h-4 w-4" />
              Settings
            </button>
            <button
              onClick={() => { handleLogout(); setMenuOpen(false); }}
              className="w-full flex items-center gap-2 px-4 py-2 text-sm text-text-secondary hover:bg-surface-tertiary hover:text-text-primary transition"
            >
              <ArrowRightStartOnRectangleIcon className="h-4 w-4" />
              Logout
            </button>
          </div>
        )}
      </div>
      </div>
    </div>
  );
}
