import type { Metadata, Viewport } from "next";
import { Exo_2 } from "next/font/google";
import { AuthProvider } from "@/lib/auth";
import { ThemeProvider } from "@/lib/theme";
import "./globals.css";

const exo2 = Exo_2({
  subsets: ["latin"],
  variable: "--font-brand",
  weight: ["700"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Frona",
  description: "AI Agentic Assistant",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  maximumScale: 1,
  userScalable: false,
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${exo2.variable} bg-surface text-text-primary antialiased`}>
        <ThemeProvider>
          <AuthProvider>{children}</AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
