import type { Metadata } from "next";

import { SiteFooter } from "@/components/site-footer";

import "./globals.css";

export const metadata: Metadata = {
  title: "Northwind · Customer Management",
  description:
    "Aplicación full-stack de gestión de clientes sobre la base de datos Northwind: API REST en Rust con Rocket y panel administrativo en Next.js.",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="es">
      {/* 🇪🇸 NOTA: el pie va en el layout y no dentro de la página, que es donde
          le corresponde: es cromo del sitio, va FUERA del <main>, y así una página
          futura lo hereda sin tener que acordarse de incluirlo. */}
      <body className="antialiased">
        {children}
        <SiteFooter />
      </body>
    </html>
  );
}
