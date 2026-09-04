"use client";

import { useCallback, useMemo } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

import {
  SORTABLE_COLUMNS,
  type ListParams,
  type SortableColumn,
  type SortDirection,
} from "./api";

export const PAGE_SIZES = [10, 25, 50] as const;

/** Mismos valores por defecto que el backend, para que la URL limpia y la
 *  respuesta del servidor coincidan. */
const DEFAULTS: ListParams = {
  page: 1,
  pageSize: 10,
  companyName: "",
  sortBy: "companyName",
  sortDir: "asc",
};

function readPage(raw: string | null): number {
  const value = Number(raw);
  return Number.isInteger(value) && value >= 1 ? value : DEFAULTS.page;
}

function readPageSize(raw: string | null): number {
  const value = Number(raw);
  // Solo se aceptan los tamaños que ofrece el selector: si la URL trae 7, el
  // desplegable no podría representarlo y quedaría en blanco. El backend lo
  // aceptaría, pero la UI mentiría sobre su propio estado.
  return (PAGE_SIZES as readonly number[]).includes(value)
    ? value
    : DEFAULTS.pageSize;
}

function readSortBy(raw: string | null): SortableColumn {
  return SORTABLE_COLUMNS.includes(raw as SortableColumn)
    ? (raw as SortableColumn)
    : DEFAULTS.sortBy;
}

function readSortDir(raw: string | null): SortDirection {
  return raw === "desc" ? "desc" : DEFAULTS.sortDir;
}

/**
 * Mantiene el estado de la vista EN LA URL, no en `useState`.
 *
 * 🇪🇸 NOTA (por qué esto vale la pena): un `useState` vive en memoria y muere con
 * la pestaña. Poner página, filtro y orden en la query string convierte la vista en
 * una DIRECCIÓN, y eso arregla cuatro cosas de golpe:
 *
 *   · F5 no te devuelve a la página 1 — el estado sobrevive a la recarga.
 *   · La vista se puede pegar en un chat: "mira estos, página 3 filtrando por
 *     'restaurant'". Con estado en memoria, el enlace lleva a otro sitio.
 *   · Los botones atrás/adelante del navegador funcionan como el usuario espera,
 *     gratis: cada cambio es una entrada del historial.
 *   · Al depurar, la URL DICE el estado. No hay que reconstruirlo a mano.
 *
 * ⚠️ Se usa `router.replace` y no `push`: escribir en el buscador genera un cambio
 * de URL cada 300 ms, y con `push` cada letra sería una entrada del historial —
 * volver atrás obligaría a pulsar quince veces para deshacer una búsqueda.
 */
export function useListParams() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();

  const params = useMemo<ListParams>(
    () => ({
      page: readPage(searchParams.get("page")),
      pageSize: readPageSize(searchParams.get("pageSize")),
      companyName: searchParams.get("companyName") ?? DEFAULTS.companyName,
      sortBy: readSortBy(searchParams.get("sortBy")),
      sortDir: readSortDir(searchParams.get("sortDir")),
    }),
    [searchParams],
  );

  const setParams = useCallback(
    (patch: Partial<ListParams>) => {
      const next: ListParams = { ...params, ...patch };
      const query = new URLSearchParams();

      // Solo se escriben los valores que NO son el defecto. Así la URL de la vista
      // inicial es `/` a secas en vez de `/?page=1&pageSize=10&sortBy=…`, que es
      // ruido que nadie necesita leer ni compartir.
      if (next.page !== DEFAULTS.page) query.set("page", String(next.page));
      if (next.pageSize !== DEFAULTS.pageSize)
        query.set("pageSize", String(next.pageSize));
      if (next.companyName) query.set("companyName", next.companyName);
      if (next.sortBy !== DEFAULTS.sortBy) query.set("sortBy", next.sortBy);
      if (next.sortDir !== DEFAULTS.sortDir)
        query.set("sortDir", next.sortDir);

      const qs = query.toString();
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    },
    [params, pathname, router],
  );

  /**
   * Alterna la ordenación de una columna.
   *
   * Primer clic en una columna nueva: ascendente. Clic en la que ya está activa:
   * invierte. Y siempre vuelve a la página 1 — seguir en la 4 tras reordenar
   * enseñaría un tramo arbitrario de una lista distinta.
   */
  const toggleSort = useCallback(
    (column: SortableColumn) => {
      setParams({
        sortBy: column,
        sortDir:
          params.sortBy === column && params.sortDir === "asc" ? "desc" : "asc",
        page: 1,
      });
    },
    [params.sortBy, params.sortDir, setParams],
  );

  return { params, setParams, toggleSort };
}
