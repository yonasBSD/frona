"use client";

import { useState, useRef, useEffect } from "react";
import { BellIcon } from "@heroicons/react/24/solid";
import { useNotifications } from "@/lib/notification-context";
import { API_URL } from "@/lib/api-client";
import type { Notification, NotificationLevel } from "@/lib/types";

function levelColor(level: NotificationLevel): string {
  switch (level) {
    case "success":
      return "text-green-400";
    case "error":
      return "text-red-400";
    case "warning":
      return "text-yellow-400";
    default:
      return "text-blue-400";
  }
}

function levelDot(level: NotificationLevel): string {
  switch (level) {
    case "success":
      return "bg-green-400";
    case "error":
      return "bg-red-400";
    case "warning":
      return "bg-yellow-400";
    default:
      return "bg-blue-400";
  }
}

function timeAgo(dateStr: string): string {
  const seconds = Math.floor(
    (Date.now() - new Date(dateStr).getTime()) / 1000,
  );
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function getNotificationHref(notification: Notification): string | null {
  if (notification.data.type !== "App") return null;
  const { action } = notification.data;
  if (action === "stop" || notification.level === "error") return null;
  return `${API_URL}/apps/${notification.data.app_id}/`;
}

function NotificationItem({
  notification,
  onRead,
  onClose,
}: {
  notification: Notification;
  onRead: (id: string) => void;
  onClose: () => void;
}) {
  const href = getNotificationHref(notification);

  const handleClick = () => {
    if (!notification.read) onRead(notification.id);
    if (href) {
      window.open(href, "_blank");
      onClose();
    }
  };

  return (
    <button
      onClick={handleClick}
      className={`w-full text-left px-4 py-3 flex gap-3 items-start transition hover:bg-surface-tertiary ${
        notification.read ? "opacity-60" : ""
      } ${href ? "cursor-pointer" : ""}`}
    >
      <span
        className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${notification.read ? "bg-transparent" : levelDot(notification.level)}`}
      />
      <div className="flex-1 min-w-0">
        <p
          className={`text-sm font-medium truncate ${levelColor(notification.level)}`}
        >
          {notification.title}
        </p>
        {notification.body && (
          <p className="text-xs text-text-secondary truncate mt-0.5">
            {notification.body}
          </p>
        )}
        <p className="text-xs text-text-tertiary mt-1">
          {timeAgo(notification.created_at)}
        </p>
      </div>
    </button>
  );
}

export function NotificationDropdown() {
  const { notifications, unreadCount, markRead, markAllRead } =
    useNotifications();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div ref={ref} className="relative flex items-center">
      <button
        onClick={() => setOpen((v) => !v)}
        className="relative flex items-center justify-center h-10 w-10 rounded-full bg-surface-tertiary text-text-secondary hover:brightness-125 transition cursor-pointer"
        title="Notifications"
      >
        <BellIcon className="h-5 w-5" />
        {unreadCount > 0 && (
          <span className="absolute -top-2 -right-2 flex items-center justify-center h-5 min-w-5 px-1 rounded-full bg-red-600 text-white text-[11px] font-bold leading-none ring-2 ring-surface-nav">
            {unreadCount > 99 ? "99+" : unreadCount}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-full z-20 mt-2 w-80 max-h-96 rounded-lg border border-border bg-surface-secondary shadow-lg flex flex-col overflow-hidden">
          <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0">
            <span className="text-sm font-medium text-text-primary">
              Notifications
            </span>
            {unreadCount > 0 && (
              <button
                onClick={() => markAllRead()}
                className="text-xs text-accent hover:text-accent-hover transition cursor-pointer"
              >
                Mark all read
              </button>
            )}
          </div>
          <div className="overflow-y-auto flex-1">
            {notifications.length === 0 ? (
              <p className="px-4 py-8 text-sm text-text-tertiary text-center">
                No notifications yet
              </p>
            ) : (
              notifications.map((n) => (
                <NotificationItem
                  key={n.id}
                  notification={n}
                  onRead={markRead}
                  onClose={() => setOpen(false)}
                />
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
