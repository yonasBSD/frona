"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  createElement,
} from "react";
import { api, setAccessToken } from "./api-client";
import type {
  UserInfo,
  AuthResponse,
  LoginRequest,
  RegisterRequest,
  SsoStatus,
} from "./types";

const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001";

interface AuthContextValue {
  user: UserInfo | null;
  loading: boolean;
  ssoStatus: SsoStatus | null;
  login: (req: LoginRequest) => Promise<void>;
  register: (req: RegisterRequest) => Promise<void>;
  logout: () => void;
  revalidate: () => Promise<void>;
  initiateSso: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1000;

async function fetchCurrentUser(): Promise<UserInfo | null> {
  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      return await api.get<UserInfo>("/api/auth/me");
    } catch (err: unknown) {
      const isAuthError =
        err instanceof Error &&
        "status" in err &&
        (err as { status: number }).status === 401;

      if (isAuthError) return null;

      if (attempt < MAX_RETRIES) {
        await new Promise((r) => setTimeout(r, RETRY_DELAY_MS));
        continue;
      }
      return null;
    }
  }
  return null;
}

async function fetchSsoStatus(): Promise<SsoStatus | null> {
  try {
    const res = await fetch(`${API_URL}/api/auth/sso`);
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<UserInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [ssoStatus, setSsoStatus] = useState<SsoStatus | null>(null);

  useEffect(() => {
    Promise.all([fetchCurrentUser(), fetchSsoStatus()])
      .then(([u, sso]) => {
        setUser(u);
        setSsoStatus(sso);
      })
      .finally(() => setLoading(false));
  }, []);

  const login = useCallback(async (req: LoginRequest) => {
    const res = await api.post<AuthResponse>("/api/auth/login", req);
    if (res.token) {
      setAccessToken(res.token);
    }
    setUser(res.user);
  }, []);

  const register = useCallback(async (req: RegisterRequest) => {
    const res = await api.post<AuthResponse>("/api/auth/register", req);
    if (res.token) {
      setAccessToken(res.token);
    }
    setUser(res.user);
  }, []);

  const logout = useCallback(async () => {
    await api.post("/api/auth/logout", {}).catch(() => {});
    setAccessToken(null);
    setUser(null);
  }, []);

  const revalidate = useCallback(async () => {
    const u = await fetchCurrentUser();
    if (u) setUser(u);
  }, []);

  const initiateSso = useCallback(() => {
    window.location.href = `${API_URL}/api/auth/sso/authorize`;
  }, []);

  return createElement(
    AuthContext.Provider,
    {
      value: {
        user,
        loading,
        ssoStatus,
        login,
        register,
        logout,
        revalidate,
        initiateSso,
      },
    },
    children,
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
