import { AlertCircle, Info } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * Aviso en línea dentro de un diálogo.
 *
 * ═══════════════════════════════════════════════════════════════════
 * 🇪🇸 NOTA — POR QUÉ HAY DOS VARIANTES Y NO UNA "DE ERROR" PARA TODO
 * ═══════════════════════════════════════════════════════════════════
 *
 * El color aquí no es decoración: es la respuesta a "¿he roto algo?".
 *
 *   · `error` (rojo)  → algo FALLÓ. El servidor no responde, la petición se cayó.
 *                       Reintentar tiene sentido: puede que a la segunda funcione.
 *
 *   · `info` (azul)   → nada ha fallado. El sistema funcionó perfectamente y la
 *                       respuesta es "no, y por esto". Reintentar NO sirve: el
 *                       resultado será idéntico las mil veces siguientes.
 *
 * El caso que obliga a distinguirlo es el 409 al borrar un cliente con pedidos. En
 * rojo, el usuario lee "he roto la aplicación" y vuelve a pulsar. En azul, lee una
 * regla del negocio —los clientes con historial no se borran— y entiende que la
 * aplicación acaba de protegerle de dejar 163 pedidos huérfanos.
 *
 * Y en Northwind ese 409 es el camino NORMAL: los 93 clientes originales tienen
 * pedidos. Pintar de rojo el resultado más frecuente de una operación entrena al
 * usuario a ignorar el color rojo, que es justo lo que no se quiere el día que algo
 * se rompa de verdad.
 */
export function Notice({
  variant,
  title,
  children,
}: {
  variant: "error" | "info";
  title?: string;
  children: React.ReactNode;
}) {
  const Icon = variant === "error" ? AlertCircle : Info;

  return (
    <div
      role={variant === "error" ? "alert" : "status"}
      className={cn(
        "flex gap-2.5 rounded-md border p-3 text-sm",
        variant === "error"
          ? "border-destructive/30 bg-destructive/5 text-destructive"
          : "border-info-border bg-info-muted text-info-foreground",
      )}
    >
      <Icon className="mt-0.5 size-4 shrink-0" />
      <div className="min-w-0 space-y-0.5">
        {title ? <p className="font-medium">{title}</p> : null}
        <p className="text-pretty leading-relaxed">{children}</p>
      </div>
    </div>
  );
}
