import type { Metadata } from "next";

import "./globals.css";

export const metadata: Metadata = {
  title: "Northwind · Clientes",
  description: "Panel de gestión de clientes sobre la base de datos Northwind.",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="es">
      <body className="antialiased">{children}</body>
    </html>
  );
}
