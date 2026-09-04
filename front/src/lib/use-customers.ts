"use client";

import { useCallback, useEffect, useState } from "react";

import { listCustomers, type Customer, type ListParams } from "./api";

interface CustomersState {
  page: { data: Customer[]; total: number; pageSize: number } | null;
  loading: boolean;
  error: Error | null;
}

/**
 * Carga la página de clientes que describen `params` y la recarga cuando cambian.
 *
 * 🇪🇸 NOTA (`AbortController`): al teclear en el buscador se lanza una petición cada
 * 300 ms. Sin cancelar la anterior, dos respuestas pueden llegar DESORDENADAS —la
 * de "al" después de la de "alf"— y la tabla acabaría mostrando el resultado de una
 * búsqueda que el usuario ya no está haciendo. Es la "condición de carrera del
 * autocompletado", y no se arregla con un debounce más largo: se arregla
 * cancelando. El `return` del efecto aborta la petición en vuelo antes de lanzar la
 * siguiente.
 */
export function useCustomers(params: ListParams) {
  const [state, setState] = useState<CustomersState>({
    page: null,
    loading: true,
    error: null,
  });

  // Contador que fuerza una recarga sin cambiar los parámetros: lo usan el botón
  // "Reintentar" y los diálogos tras crear, editar o borrar.
  const [reloadToken, setReloadToken] = useState(0);
  const reload = useCallback(() => setReloadToken((n) => n + 1), []);

  const { page, pageSize, companyName, sortBy, sortDir } = params;

  useEffect(() => {
    const controller = new AbortController();

    setState((previous) => ({ ...previous, loading: true, error: null }));

    listCustomers(
      { page, pageSize, companyName, sortBy, sortDir },
      controller.signal,
    )
      .then((result) => {
        setState({
          page: {
            data: result.data,
            total: result.total,
            pageSize: result.pageSize,
          },
          loading: false,
          error: null,
        });
      })
      .catch((error: unknown) => {
        // La cancelación es una operación normal, no un fallo: se ignora en
        // silencio y el efecto siguiente ya está pintando su propio "cargando".
        if (error instanceof DOMException && error.name === "AbortError") return;

        setState({
          page: null,
          loading: false,
          error: error instanceof Error ? error : new Error(String(error)),
        });
      });

    return () => controller.abort();
  }, [page, pageSize, companyName, sortBy, sortDir, reloadToken]);

  return { ...state, reload };
}
