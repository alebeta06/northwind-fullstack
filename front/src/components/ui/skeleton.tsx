import { cn } from "@/lib/utils";

/**
 * Bloque gris animado que ocupa el sitio de un contenido que aún no ha llegado.
 *
 * 🇪🇸 NOTA (por qué skeletons y no un spinner centrado): un spinner sustituye a la
 * tabla, así que la página COLAPSA a la altura del spinner y vuelve a crecer cuando
 * llegan los datos. Ese salto mueve el paginador y el buscador bajo el cursor justo
 * cuando el usuario iba a hacer clic. Los skeletons mantienen la altura exacta de la
 * tabla —una fila por cada fila que va a haber— y nada se desplaza al llegar los
 * datos. Además comunican la FORMA de lo que viene, no solo que hay que esperar.
 */
export function Skeleton({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("animate-pulse rounded-md bg-muted", className)}
      {...props}
    />
  );
}
