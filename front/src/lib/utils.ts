import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Une clases condicionales resolviendo conflictos de Tailwind.
 *
 * `clsx` aplana condicionales; `twMerge` resuelve el conflicto de verdad: si un
 * componente trae `px-4` y quien lo usa pasa `px-2`, sin merge quedarían las dos y
 * ganaría la que el CSS ponga después — es decir, el orden del archivo generado, no
 * el de la intención. `twMerge` deja solo la última.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
