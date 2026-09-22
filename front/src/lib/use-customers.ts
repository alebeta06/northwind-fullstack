"use client";

import { useCallback, useEffect, useRef, useState } from "react";

import {
  listCustomers,
  NetworkError,
  type Customer,
  type ListParams,
} from "./api";

interface CustomersState {
  page: { data: Customer[]; total: number; pageSize: number } | null;
  loading: boolean;
  error: Error | null;
  /** Seguimos esperando la primera respuesta y ya ha pasado tiempo suficiente
   *  para avisar de que el servidor puede estar arrancando. */
  waking: boolean;
}

// ═══════════════════════════════════════════════════════════════════
//  Arranque en frío
// ═══════════════════════════════════════════════════════════════════
//
// 🇪🇸 EL PROBLEMA: el backend vive en un plan gratuito que SUSPENDE la instancia
// cuando no recibe tráfico. La primera petición después de un rato no falla porque
// algo esté roto: falla —o se queda esperando— porque la máquina está encendiéndose,
// y tarda entre unos segundos y cerca de un minuto.
//
// Sin reintento, quien abre el panel ve "El servidor no responde" y un botón
// Reintentar que, pulsado a los dos segundos, funciona. Es decir: la aplicación le
// pide al usuario que haga a mano el reintento que puede hacer ella, y le enseña un
// error rojo por una situación que no es un error.

/** Cuánto tiempo se insiste antes de admitir que el servidor no está. */
const COLD_START_BUDGET_MS = 90_000;

/**
 * Cuánto se espera antes de explicar la demora.
 *
 * 🇪🇸 NOTA (por qué hay una espera y no se avisa desde el primer momento): en local,
 * y con el servidor ya despierto, la lista llega en decenas de milisegundos. Un aviso
 * inmediato parpadearía en cada carga y acabaría siendo ruido que nadie lee. A los
 * 3 segundos sin datos la demora ya es perceptible, y entonces el aviso informa en
 * vez de interrumpir.
 */
const WAKING_NOTICE_AFTER_MS = 3_000;

const FIRST_RETRY_DELAY_MS = 500;
const MAX_RETRY_DELAY_MS = 5_000;

/**
 * Espera `ms`, o termina antes si se cancela.
 *
 * 🇪🇸 NOTA: resuelve —no rechaza— al cancelarse. Quien llama comprueba el `signal`
 * justo después, que es una condición explícita en el flujo; un reject obligaría a un
 * try/catch cuyo error no significa "ha fallado algo" sino "ya no hace falta".
 */
function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    // `done` cita a `timer` antes de su declaración, y es correcto: solo se ejecuta
    // desde el temporizador o desde el evento de cancelación, los dos posteriores a
    // la asignación. Así el `clearTimeout` cancela el reloj si la espera termina por
    // cancelación, y el `removeEventListener` no deja un oyente colgado del signal si
    // termina por tiempo.
    const done = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    };

    const timer = setTimeout(done, ms);
    signal.addEventListener("abort", done, { once: true });
  });
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
    waking: false,
  });

  // Contador que fuerza una recarga sin cambiar los parámetros: lo usan el botón
  // "Reintentar" y los diálogos tras crear, editar o borrar.
  const [reloadToken, setReloadToken] = useState(0);
  const reload = useCallback(() => setReloadToken((n) => n + 1), []);

  // 🇪🇸 NOTA (por qué un ref y no un estado): marca si el servidor ya contestó alguna
  // vez, y solo sirve para decidir dentro del efecto. En un estado, cambiarlo
  // provocaría un render de más por cada carga y tendría que entrar en las
  // dependencias del efecto, que volvería a lanzarse al primer éxito.
  const answeredOnce = useRef(false);

  const { page, pageSize, companyName, sortBy, sortDir } = params;

  useEffect(() => {
    const controller = new AbortController();
    const startedAt = Date.now();

    // 🇪🇸 DECISIÓN (el reintento cubre SOLO las cargas anteriores al primer éxito):
    // una vez que el servidor ha contestado, está despierto, y un fallo de red
    // posterior ya no es un arranque en frío — es la red del usuario, o el servidor
    // caído de verdad. Insistir 90 segundos ahí dejaría la tabla en un "cargando"
    // larguísimo en lugar de decir lo que pasa, que es peor que el error.
    const coldStartPossible = !answeredOnce.current;

    setState((previous) => ({
      ...previous,
      loading: true,
      error: null,
      waking: false,
    }));

    // El aviso se dispara por TIEMPO, no por número de intentos: la instancia
    // suspendida puede fallar al instante (nadie escuchando todavía) o dejar la
    // petición colgada mientras arranca. Para quien mira la pantalla los dos casos
    // son el mismo —no hay datos y la espera se alarga—, así que el mismo reloj los
    // cubre a los dos.
    const noticeTimer = coldStartPossible
      ? setTimeout(() => {
          if (controller.signal.aborted) return;
          setState((previous) =>
            previous.loading ? { ...previous, waking: true } : previous,
          );
        }, WAKING_NOTICE_AFTER_MS)
      : undefined;

    const load = async () => {
      for (let attempt = 0; ; attempt += 1) {
        try {
          const result = await listCustomers(
            { page, pageSize, companyName, sortBy, sortDir },
            controller.signal,
          );

          answeredOnce.current = true;
          setState({
            page: {
              data: result.data,
              total: result.total,
              pageSize: result.pageSize,
            },
            loading: false,
            error: null,
            waking: false,
          });
          return;
        } catch (error: unknown) {
          // La cancelación es una operación normal, no un fallo: se ignora en
          // silencio y el efecto siguiente ya está pintando su propio "cargando".
          if (controller.signal.aborted) return;
          if (error instanceof DOMException && error.name === "AbortError") {
            return;
          }

          // ⚠️ La condición que separa "espera, que está arrancando" de "esto está
          // roto": solo se reintenta un `NetworkError`, que es el fallo SIN respuesta
          // del servidor. Un 4xx o un 5xx llega como `ApiError`, con status y cuerpo
          // — significa que hay alguien al otro lado y que ha contestado eso, así que
          // repetir la misma petición daría la misma respuesta 90 segundos seguidos y
          // solo retrasaría el diagnóstico.
          const retryable =
            coldStartPossible &&
            error instanceof NetworkError &&
            Date.now() - startedAt < COLD_START_BUDGET_MS;

          if (!retryable) {
            setState({
              page: null,
              loading: false,
              error: error instanceof Error ? error : new Error(String(error)),
              waking: false,
            });
            return;
          }

          // Backoff exponencial con techo: los primeros intentos van seguidos
          // —porque el servidor puede estar ya listo— y luego se espacian para no
          // castigar a una máquina que está arrancando con una petición cada 500 ms.
          await sleep(
            Math.min(MAX_RETRY_DELAY_MS, FIRST_RETRY_DELAY_MS * 2 ** attempt),
            controller.signal,
          );

          if (controller.signal.aborted) return;
        }
      }
    };

    void load();

    return () => {
      clearTimeout(noticeTimer);
      controller.abort();
    };
  }, [page, pageSize, companyName, sortBy, sortDir, reloadToken]);

  return { ...state, reload };
}
