/**
 * Cliente de la API de clientes (Northwind).
 *
 * Los tipos de este archivo son la transcripción del contrato REAL del backend
 * (`back/src/models.rs` y `back/src/main.rs`), no una aproximación:
 *
 *   · `Customer`      ← struct Customer, con `#[serde(rename_all = "camelCase")]`.
 *                       Los `Option<String>` de Rust llegan como `string | null`.
 *   · `Paginated<T>`  ← struct Paginated { data, total, page, pageSize }.
 *   · `ApiErrorBody`  ← { error, message }, el ÚNICO formato de error que emite la
 *                       API, catchers incluidos.
 */

export const API_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8001";

// ═══════════════════════════════════════════════════════════════════
//  Tipos del dominio
// ═══════════════════════════════════════════════════════════════════

/** Campos opcionales: en la base son NULL-ables y el backend los normaliza. */
export interface CustomerFields {
  companyName: string;
  contactName: string | null;
  contactTitle: string | null;
  address: string | null;
  city: string | null;
  region: string | null;
  postalCode: string | null;
  country: string | null;
  phone: string | null;
  fax: string | null;
}

export interface Customer extends CustomerFields {
  customerId: string;
}

export interface Paginated<T> {
  data: T[];
  total: number;
  page: number;
  pageSize: number;
}

/**
 * Columnas por las que el backend acepta ordenar.
 *
 * ⚠️ Esta lista NO es decorativa: replica la whitelist del `match` de
 * `sort_column()` en el backend. Cualquier otro valor allí cae al orden por
 * defecto en silencio, así que un typo aquí no daría error — daría un orden que
 * no es el que el usuario pidió. Tenerla como tipo hace que TypeScript avise.
 */
export const SORTABLE_COLUMNS = [
  "customerId",
  "companyName",
  "city",
  "country",
] as const;

export type SortableColumn = (typeof SORTABLE_COLUMNS)[number];
export type SortDirection = "asc" | "desc";

export interface ListParams {
  page: number;
  pageSize: number;
  companyName: string;
  sortBy: SortableColumn;
  sortDir: SortDirection;
}

// ═══════════════════════════════════════════════════════════════════
//  Errores — un solo formato, dos naturalezas
// ═══════════════════════════════════════════════════════════════════

/**
 * Error CON respuesta del servidor: hay status HTTP y hay cuerpo `{error, message}`.
 *
 * `code` es el campo estable contra el que programa la UI (`has_orders`,
 * `duplicate_id`, `not_found`…); `message` es el texto para el humano. Esa
 * separación es del backend, y aquí se respeta: NUNCA se decide comportamiento
 * mirando el texto del mensaje, que puede cambiar de redacción.
 */
export class ApiError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/**
 * Error SIN respuesta: la petición no llegó o el navegador la bloqueó.
 *
 * ⚠️ Este es el caso que hay que distinguir sí o sí. `fetch` rechaza con un
 * `TypeError` pelado —sin status, sin cuerpo— cuando el servidor no está
 * escuchando, cuando se cae la red, o cuando la respuesta llega pero le faltan
 * las cabeceras CORS. Para el navegador los tres son lo mismo: "no hay
 * respuesta legible".
 *
 * Confundirlo con un 404 lleva al peor mensaje posible de un panel: decirle al
 * usuario "el cliente no existe" cuando lo que pasa es que el backend está
 * caído. Son diagnósticos opuestos y llevan a acciones opuestas — uno se
 * resuelve buscando otro cliente, el otro arrancando un servidor.
 */
export class NetworkError extends Error {
  constructor(cause?: unknown) {
    super(
      "No se pudo contactar con el servidor. Comprueba que el backend está en marcha.",
    );
    this.name = "NetworkError";
    this.cause = cause;
  }
}

interface ApiErrorBody {
  error: string;
  message: string;
}

function isApiErrorBody(value: unknown): value is ApiErrorBody {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as ApiErrorBody).error === "string" &&
    typeof (value as ApiErrorBody).message === "string"
  );
}

/**
 * Ejecuta una petición y traduce CUALQUIER fallo a `ApiError` o `NetworkError`.
 *
 * 🇪🇸 NOTA (por qué una sola función y no un parseo por llamada): el backend
 * garantiza un único formato de error para todo — validaciones, conflictos y
 * también los catchers de Rocket, que son los que responden cuando la petición ni
 * siquiera llega a una ruta. Como el formato es uno, el parseo tiene que ser uno.
 * Repartirlo por los componentes acabaría con dos o tres interpretaciones distintas
 * del mismo cuerpo, y con una de ellas quedándose obsoleta sin que nadie lo note.
 */
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;

  try {
    response = await fetch(`${API_URL}${path}`, {
      ...init,
      headers: {
        // Solo se declara JSON cuando hay cuerpo: el POST y el PUT del backend
        // llevan `format = "json"`, y sin esta cabecera la ruta ni se selecciona
        // (contesta 404, no 415).
        ...(init?.body ? { "Content-Type": "application/json" } : {}),
        ...init?.headers,
      },
    });
  } catch (cause) {
    // ⚠️ `fetch` rechaza con AbortError cuando NOSOTROS cancelamos la petición —
    // algo que pasa continuamente al teclear en el buscador, donde cada pulsación
    // aborta la búsqueda anterior. Envolverlo en `NetworkError` haría que el panel
    // gritara "el servidor no responde" mientras el usuario escribe, con el
    // servidor perfectamente vivo. Se deja pasar tal cual para que quien llama lo
    // reconozca y lo ignore.
    if (cause instanceof DOMException && cause.name === "AbortError") {
      throw cause;
    }
    throw new NetworkError(cause);
  }

  // 204 No Content: el DELETE con éxito. No hay cuerpo que leer, e intentar
  // parsearlo sería un error inventado.
  if (response.status === 204) {
    return undefined as T;
  }

  const text = await response.text();
  let parsed: unknown;

  try {
    parsed = JSON.parse(text);
  } catch {
    // No debería ocurrir —los catchers garantizan JSON— pero si ocurre, el
    // mensaje dice lo que pasó de verdad en vez de un "unexpected token <".
    throw new ApiError(
      "unexpected_response",
      `El servidor respondió algo que no es JSON (HTTP ${response.status}).`,
      response.status,
    );
  }

  if (!response.ok) {
    if (isApiErrorBody(parsed)) {
      throw new ApiError(parsed.error, parsed.message, response.status);
    }
    throw new ApiError(
      "unexpected_response",
      `El servidor devolvió un error HTTP ${response.status}.`,
      response.status,
    );
  }

  return parsed as T;
}

// ═══════════════════════════════════════════════════════════════════
//  Operaciones
// ═══════════════════════════════════════════════════════════════════

export function listCustomers(
  params: ListParams,
  signal?: AbortSignal,
): Promise<Paginated<Customer>> {
  const query = new URLSearchParams({
    page: String(params.page),
    pageSize: String(params.pageSize),
    sortBy: params.sortBy,
    sortDir: params.sortDir,
  });

  // El filtro vacío no se manda: el backend trata "ausente" y "vacío" igual, pero
  // una URL sin ruido es más fácil de leer y de compartir.
  if (params.companyName) query.set("companyName", params.companyName);

  return request<Paginated<Customer>>(`/customers?${query}`, { signal });
}

export function getCustomer(
  id: string,
  signal?: AbortSignal,
): Promise<Customer> {
  return request<Customer>(`/customers/${encodeURIComponent(id)}`, { signal });
}

export function createCustomer(customer: Customer): Promise<Customer> {
  return request<Customer>("/customers", {
    method: "POST",
    body: JSON.stringify(customer),
  });
}

/**
 * ⚠️ REEMPLAZO TOTAL. El PUT del backend escribe las diez columnas con lo que
 * reciba: un campo ausente en el cuerpo NO se conserva, se queda en NULL.
 *
 * Por eso el parámetro es `CustomerFields` completo y no un `Partial<>`: el tipo
 * impide, en tiempo de compilación, el bug que vaciaría registros en silencio.
 */
export function updateCustomer(
  id: string,
  fields: CustomerFields,
): Promise<Customer> {
  return request<Customer>(`/customers/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(fields),
  });
}

export function deleteCustomer(id: string): Promise<void> {
  return request<void>(`/customers/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}
