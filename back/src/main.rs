//! Northwind Customer Management API.
//!
//! At this stage the application wires together its three layers — models, database
//! access and HTTP — but exposes no CRUD routes yet. The only endpoint, `/health`, is a
//! deliberate end-to-end smoke test: answering it requires Rocket's codegen, the managed
//! state, the `Mutex`, the bundled SQLite and a real query against the Northwind file.
//!
//! 🇪🇸 NOTA: si `/health` responde con el número correcto de clientes, entonces la cadena
//! completa (State → Mutex → Connection → SQLite → JSON) está funcionando. Cuando después
//! escribamos el CRUD, cualquier fallo será de la lógica de las rutas, no del cableado.
//! Verificar el cableado por separado es lo que hace que el siguiente paso sea depurable.

#[macro_use]
extern crate rocket;

// ═══════════════════════════════════════════════════════════════════
//  Declaración de módulos
// ═══════════════════════════════════════════════════════════════════
//
// 🇪🇸 NOTA: `mod db;` NO importa nada: le dice al compilador "existe un módulo llamado
// `db`, búscalo en `src/db.rs`". Sin esta línea, el archivo `db.rs` sencillamente no
// forma parte del programa — Rust no compila los archivos de un directorio por el hecho
// de estar ahí, a diferencia de lo que hacen Node o Python. Sí el árbol de módulos que
// tú declaras explícitamente, empezando por la raíz (`main.rs`).

mod db;

// 🇪🇸 NOTA (el `#[allow(dead_code)]` que había aquí ya no está): cubría a `Customer`,
// `Paginated`, `Customer::from_row` y `Customer::COLUMNS`, que `GET /customers` ya usa.
// Un silenciador vive exactamente lo que dura su motivo; el de este módulo se ha quedado
// sin motivo y por eso desaparece.
//
// ⚠️ `NewCustomer` y `UpdateCustomer` siguen sin usarse hasta que lleguen POST y PUT, así
// que el `#[allow]` no se ha borrado: se ha MUDADO a esos dos structs concretos, en
// `models.rs`. Es un cambio a mejor, aunque quede menos a la vista: un `allow` sobre el
// módulo entero tapa también el código muerto que aparezca por accidente; uno sobre cada
// struct solo tapa lo que nombra, y desaparece con el commit que implemente su ruta.
mod models;

use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::request::Request;
use rocket::response::status::Created;
use rocket::response::{self, Responder};
use rocket::serde::json::{json, Json, Value};
use rocket::{FromForm, State};
use rusqlite::{params, ErrorCode, ToSql};

use db::Db;
use models::{Customer, NewCustomer, Paginated};

// ═══════════════════════════════════════════════════════════════════
//  Constantes de configuración
// ═══════════════════════════════════════════════════════════════════

/// Port mandated by ENUNCIADO.md. The Next.js frontend will point here.
const PORT: u16 = 8001;

/// Path to the Northwind database file.
///
/// 🇪🇸 NOTA: es una ruta RELATIVA, y se resuelve contra el directorio de trabajo del
/// proceso, no contra la ubicación del binario. `cargo run` ejecuta desde la raíz del
/// paquete (`back/`), que es justo donde está `northwind.db`, así que funciona.
///
/// ⚠️ Pero `./target/debug/back` lanzado desde otro directorio NO la encontrará. Es el
/// fallo de arranque más probable de este proyecto, y por eso el mensaje de error de
/// abajo imprime el directorio de trabajo: sin ese dato, "unable to open database file"
/// es un error mudo.
const DB_PATH: &str = "northwind.db";

// ═══════════════════════════════════════════════════════════════════
//  Rutas
// ═══════════════════════════════════════════════════════════════════

/// Liveness probe.
///
/// Reports the linked SQLite version and the live row count of the `Customers` table,
/// which proves the managed connection is usable from inside a request handler.
///
/// 🇪🇸 NOTA (el request guard `&State<Db>`): Rocket mira la firma de la función, ve el
/// tipo `&State<Db>` y le inyecta el valor que registramos con `.manage()`. La búsqueda
/// es POR TIPO, que es exactamente la razón por la que `db.rs` define el newtype `Db` en
/// lugar de gestionar un `Mutex<Connection>` pelado.
///
/// ⚠️ Si te olvidas del `.manage()`, esto NO da error de compilación: Rocket lo detecta
/// al arrancar y aborta el lanzamiento diciendo que falta el estado. Es un error de
/// arranque, no de tipos.
#[get("/health")]
fn health(db: &State<Db>) -> Result<Json<Value>, Status> {
    // 🇪🇸 NOTA: `.lock()` devuelve un `Result` porque el Mutex puede estar "envenenado"
    // (otro hilo entró en pánico mientras lo tenía). Aquí propagamos ese pánico con
    // `expect`: si la base quedó en un estado indeterminado, seguir sirviendo peticiones
    // es peor que caerse. Es la única situación de este archivo donde entrar en pánico
    // es la respuesta correcta.
    let conn = db.0.lock().expect("the SQLite mutex was poisoned by a panicking thread");

    // 🇪🇸 NOTA (`query_row` vs `prepare` + `query_map`): `query_row` es el atajo para una
    // consulta que devuelve exactamente UNA fila. Da error si devuelve cero. Un
    // `COUNT(*)` siempre devuelve una fila (con el valor 0 si la tabla está vacía), así
    // que aquí es seguro.
    //
    // La anotación `: i64` no es decorativa: `row.get(0)` es genérico sobre el tipo de
    // destino, y sin ella el compilador no puede inferir a qué convertir la columna.
    // Es i64 y no i32 porque el INTEGER de SQLite son 64 bits.
    let customers: i64 = conn
        .query_row("SELECT COUNT(*) FROM Customers", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("[health] count query failed: {e}");
            // 🇪🇸 NOTA (503 y no 500): 500 significa "he tenido un fallo interno";
            // 503 significa "estoy en pie, pero una dependencia mía no responde". Para
            // un health check lo segundo es más preciso y más accionable para quien
            // monitoriza: distingue "la API está rota" de "la base de datos está caída".
            Status::ServiceUnavailable
        })?;

    Ok(Json(json!({
        "status": "ok",
        // Reading the version through the FFI forces the linker to actually resolve the
        // bundled SQLite symbols.
        "sqlite": rusqlite::version(),
        "customers": customers,
    })))
}

// ═══════════════════════════════════════════════════════════════════
//  GET /customers — listado con paginación, filtro y ordenación
// ═══════════════════════════════════════════════════════════════════

/// Valores por defecto y cotas de la paginación.
///
/// 🇪🇸 NOTA (por qué `MAX_PAGE_SIZE` no es opcional): `pageSize` sale del cliente, y sin
/// tope superior `?pageSize=999999` hace que el servidor materialice en memoria toda la
/// tabla — y con ella, todo el JSON de respuesta — por una petición de 30 bytes. Es la
/// asimetría que define un DoS de amplificación: barato de pedir, caro de servir. Con 93
/// filas no duele; el hábito de acotar TODA cantidad que venga de fuera, sí importa.
const DEFAULT_PAGE: u32 = 1;
const DEFAULT_PAGE_SIZE: u32 = 10;
const MAX_PAGE_SIZE: u32 = 100;

/// Column used when `sortBy` is absent or not in the whitelist.
const DEFAULT_SORT_COLUMN: &str = "CompanyName";

/// Query string of `GET /customers`. Todos los campos son opcionales.
///
/// 🇪🇸 NOTA (por qué un struct `FromForm` y no cinco parámetros sueltos en la ruta):
/// Rocket permite escribir `#[get("/customers?<page>&<page_size>")]`, pero entonces el
/// nombre del parámetro en la URL ES el identificador de Rust — y el enunciado pide
/// `pageSize` en camelCase, que no es un identificador válido por convención (dispararía
/// `non_snake_case`). Con un struct que deriva `FromForm` se puede renombrar cada campo
/// con `#[field(name = "...")]`: la URL habla camelCase (convención de TypeScript, que es
/// quien va a consumir esto) y Rust habla snake_case. El mismo desdoblamiento que ya hace
/// `#[serde(rename_all = "camelCase")]` en `models.rs`, pero para la query string.
///
/// 🇪🇸 NOTA (`Option<u32>` y la entrada basura): el `FromForm` de `Option<T>` devuelve
/// `None` tanto si el campo FALTA como si está pero no parsea. Es decir, `?page=abc` no
/// produce un 422: se comporta igual que no mandar `page`, y cae al valor por defecto.
/// Es coherente con lo que el enunciado pide para `sortBy` (una entrada inválida degrada
/// al orden por defecto, no rompe la petición) y hace que TODO parámetro de esta ruta se
/// comporte igual: si no se entiende, se ignora.
#[derive(FromForm)]
struct ListQuery {
    page: Option<u32>,
    #[field(name = "pageSize")]
    page_size: Option<u32>,
    #[field(name = "companyName")]
    company_name: Option<String>,
    #[field(name = "sortBy")]
    sort_by: Option<String>,
    #[field(name = "sortDir")]
    sort_dir: Option<String>,
}

/// Maps the client-supplied `sortBy` to a column name, or to the default.
///
/// ═══════════════════════════════════════════════════════════════════
/// 🇪🇸 NOTA — POR QUÉ EL `ORDER BY` NECESITA WHITELIST Y NO SE PUEDE PARAMETRIZAR
/// ═══════════════════════════════════════════════════════════════════
///
/// La regla que uno aprende primero es "nunca concatenes entrada del usuario en el SQL,
/// usa parámetros". Correcta, pero incompleta: los parámetros (`?1`, `?2`…) NO sirven
/// aquí, y conviene entender por qué.
///
/// Un placeholder de SQLite ocupa la posición de un VALOR, no la de un IDENTIFICADOR. El
/// motor prepara la sentencia ANTES de conocer el valor: analiza la sintaxis, resuelve
/// los nombres de tabla y columna y elige el plan de ejecución. Para eso, la estructura
/// de la consulta tiene que estar fijada en el momento del `prepare`. Un nombre de
/// columna es estructura; un literal es dato. Por eso `WHERE CompanyName LIKE ?1` sí
/// funciona y `ORDER BY ?1` no.
///
/// Y el detalle que lo hace peligroso: `ORDER BY ?1` NO da error. Es SQL válido. Lo que
/// hace es ordenar por una EXPRESIÓN CONSTANTE — el string "City" — que vale lo mismo
/// para todas las filas. Resultado: cero ordenación, cero mensajes, y un bug que solo se
/// ve comparando el orden de salida a ojo. Un fallo silencioso, que es la peor clase.
///
/// Quedan entonces dos caminos, y solo uno es aceptable:
///
///   ✗ Interpolar el nombre: `format!("ORDER BY {sort_by}")`. Es inyección SQL de
///     manual: `?sortBy=CompanyName; DROP TABLE Customers--`. Que `rusqlite::execute`
///     rechace múltiples sentencias mitiga ESE payload concreto, pero no el problema:
///     con una subconsulta en el ORDER BY se puede exfiltrar contenido de otras tablas
///     sin usar un solo `;`. Depender de esa mitigación es apostar a que el atacante
///     tenga menos imaginación que tú.
///
///   ✓ Whitelist: la entrada del usuario no viaja al SQL, solo lo SELECCIONA. El `match`
///     compara el texto recibido contra una lista cerrada y devuelve un `&'static str`
///     —un literal incrustado en el binario, escrito por mí en tiempo de compilación—.
///     La cadena que acaba en la consulta existía antes de que el usuario abriera el
///     navegador. Lo que él manda solo decide CUÁL de mis literales se usa.
///
/// El tipo de retorno `&'static str` no es decorativo: es la garantía, comprobada por el
/// compilador, de que ningún byte de la petición puede acabar en el SQL. Si alguien
/// intentara devolver aquí un `String` construido con la entrada, el tipo no encajaría.
/// La invariante de seguridad queda expresada en la firma, no en un comentario que
/// alguien puede ignorar.
///
/// Lo mismo aplica a la dirección (`ASC`/`DESC`), que también es sintaxis, no valor.
fn sort_column(requested: Option<&str>) -> &'static str {
    // 🇪🇸 NOTA: comparamos en minúsculas para que `?sortBy=companyname` y
    // `?sortBy=CompanyName` sean lo mismo (los nombres de columna de SQLite no distinguen
    // mayúsculas). Es `to_ascii_lowercase` y no `to_lowercase` a propósito: los nombres
    // de columna son ASCII puro, y la versión ASCII no arrastra las reglas Unicode
    // dependientes de locale (el clásico de la `I` turca).
    match requested.unwrap_or_default().to_ascii_lowercase().as_str() {
        "customerid" => "CustomerID",
        "companyname" => "CompanyName",
        "contactname" => "ContactName",
        "contacttitle" => "ContactTitle",
        "city" => "City",
        "country" => "Country",
        // Cualquier otra cosa —vacío, un typo, o un intento de inyección— cae aquí.
        // El enunciado lo pide así: degradar al orden por defecto, no devolver 400.
        _ => DEFAULT_SORT_COLUMN,
    }
}

/// Maps the client-supplied `sortDir` to `ASC`/`DESC`.
///
/// 🇪🇸 NOTA: misma lógica que `sort_column` y mismo `&'static str`. Solo hay dos valores
/// posibles, así que la "whitelist" es un `match` de dos brazos, pero la propiedad es la
/// misma: lo que sale de aquí lo escribí yo, no el cliente.
fn sort_direction(requested: Option<&str>) -> &'static str {
    match requested.unwrap_or_default().to_ascii_lowercase().as_str() {
        "desc" => "DESC",
        _ => "ASC",
    }
}

/// Lists customers with pagination, partial company-name filtering and sorting.
///
/// `GET /customers?page=1&pageSize=10&companyName=ab&sortBy=city&sortDir=desc`
#[get("/customers?<q..>")]
fn list_customers(db: &State<Db>, q: ListQuery) -> Result<Json<Paginated<Customer>>, Status> {
    // ─── 1. Normalizar la entrada ───
    //
    // 🇪🇸 NOTA: `clamp(1, MAX_PAGE_SIZE)` hace las dos cotas de golpe. Un `pageSize=0`
    // daría `LIMIT 0` (una página siempre vacía, y el frontend en bucle infinito pidiendo
    // la siguiente); un `pageSize=999999` es el DoS descrito arriba.
    let page = q.page.unwrap_or(DEFAULT_PAGE).max(1);
    let page_size = q
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);

    let sort_by = sort_column(q.sort_by.as_deref());
    let sort_dir = sort_direction(q.sort_dir.as_deref());

    // 🇪🇸 NOTA: `?companyName=` (vacío) o `?companyName=%20%20` se tratan como "sin
    // filtro". Si no, el patrón sería `%%`, que no filtra nada pero sí obliga a SQLite a
    // recorrer la tabla comparando — y, sobre todo, deja fuera cualquier fila con
    // CompanyName NULL, porque `NULL LIKE '%%'` es NULL, no true. Distinguir "filtro
    // ausente" de "filtro vacío" evita esa diferencia sutil entre las dos consultas.
    let pattern = q
        .company_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        // 🇪🇸 NOTA (LIKE y los comodines del usuario): un `%` o un `_` dentro de la
        // búsqueda actúa como comodín en lugar de como carácter literal. No es un agujero
        // de seguridad —el patrón sigue siendo un VALOR parametrizado, nunca SQL— sino
        // una peculiaridad de la búsqueda. Se documenta y se deja así; escaparlo exigiría
        // un `ESCAPE '\'` y no aporta nada en un buscador de nombres de empresa.
        .map(|s| format!("%{s}%"));

    let offset = i64::from(page - 1) * i64::from(page_size);
    let limit = i64::from(page_size);

    // ─── 2. Un solo lock para el COUNT y el SELECT ───
    //
    // 🇪🇸 NOTA (POR QUÉ LAS DOS CONSULTAS VAN BAJO EL MISMO LOCK): son dos consultas
    // distintas, y entre una y otra otro hilo podría ejecutar el POST o el DELETE. Si
    // soltáramos el mutex en medio, la respuesta podría decir `total: 93` y traer una
    // página calculada sobre 92 filas: el paginador del frontend dibujaría una página que
    // ya no existe, o se saltaría un registro al pasar de página. Mantener la guarda viva
    // durante ambas convierte el par (total, página) en una lectura coherente.
    //
    // Es, de hecho, la única transacción de solo lectura que necesita este endpoint: no
    // hace falta un BEGIN explícito porque el mutex ya excluye a todo el mundo — con UNA
    // conexión, que es justo el diseño de `db.rs`.
    let conn = db
        .0
        .lock()
        .expect("the SQLite mutex was poisoned by a panicking thread");

    // ─── 3. COUNT ───
    //
    // 🇪🇸 NOTA: el `WHERE` se monta con `format!`, pero fíjate en QUÉ se interpola: un
    // literal fijo elegido por un `if`, nunca datos. El valor buscado viaja aparte, como
    // parámetro `?1`.
    let where_clause = if pattern.is_some() {
        "WHERE CompanyName LIKE ?1"
    } else {
        ""
    };

    let count_sql = format!("SELECT COUNT(*) FROM Customers {where_clause}");

    let total: i64 = match &pattern {
        Some(p) => conn.query_row(&count_sql, [p], |row| row.get(0)),
        None => conn.query_row(&count_sql, [], |row| row.get(0)),
    }
    .map_err(|e| {
        eprintln!("[GET /customers] count query failed: {e}");
        Status::InternalServerError
    })?;

    // ─── 4. SELECT de la página ───
    //
    // 🇪🇸 NOTA (los índices de los placeholders se desplazan): sin filtro, LIMIT y OFFSET
    // son `?1` y `?2`; con filtro, el patrón ocupa el `?1` y pasan a ser `?2` y `?3`. Por
    // eso las posiciones se eligen en el mismo `match` que construye la lista de
    // parámetros: si estuvieran en sitios distintos, sería cuestión de tiempo que se
    // desincronizaran.
    //
    // ⚠️ LIMIT y OFFSET van parametrizados aunque sean números que YA hemos validado y
    // acotado. Interpolarlos "porque son u32 y no pueden contener comillas" funcionaría,
    // pero deja el hábito instalado: el día que el valor interpolado sea un String, el
    // patrón ya está normalizado en el archivo y nadie lo mira dos veces. La regla útil
    // es la simple: los valores SIEMPRE se parametrizan.
    let (limit_ph, offset_ph): (&str, &str) = if pattern.is_some() {
        ("?2", "?3")
    } else {
        ("?1", "?2")
    };

    let select_sql = format!(
        "SELECT {} FROM Customers {} ORDER BY {} {} LIMIT {} OFFSET {}",
        Customer::COLUMNS,
        where_clause,
        sort_by,
        sort_dir,
        limit_ph,
        offset_ph,
    );

    // 🇪🇸 NOTA: `Vec<&dyn ToSql>` es una lista de parámetros de longitud VARIABLE (con o
    // sin patrón). Los macros `params![...]` construyen un array de tamaño fijo y no
    // sirven cuando el número de parámetros se decide en tiempo de ejecución; para eso
    // está `params_from_iter`. `dyn` es despacho dinámico: guardamos punteros a valores de
    // tipos distintos (`String`, `i64`) unificados por el trait que todos implementan.
    let mut params: Vec<&dyn ToSql> = Vec::with_capacity(3);
    if let Some(p) = &pattern {
        params.push(p);
    }
    params.push(&limit);
    params.push(&offset);

    let mut stmt = conn.prepare(&select_sql).map_err(|e| {
        eprintln!("[GET /customers] prepare failed ({select_sql}): {e}");
        Status::InternalServerError
    })?;

    // 🇪🇸 NOTA (`query_map` + `collect`): `query_map` devuelve un iterador perezoso de
    // `Result<Customer>`. El truco del `collect::<rusqlite::Result<Vec<_>>>()` es que
    // Rust sabe convertir un iterador de Results en un Result de Vec: si alguna fila
    // falla, se queda con el primer error y descarta el resto; si todas van bien, obtienes
    // el Vec. Ahorra el bucle con `match` fila a fila.
    let data = stmt
        .query_map(rusqlite::params_from_iter(params), Customer::from_row)
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<Customer>>>())
        .map_err(|e| {
            eprintln!("[GET /customers] row query failed: {e}");
            Status::InternalServerError
        })?;

    Ok(Json(Paginated {
        data,
        total,
        page,
        page_size,
    }))
}

// ═══════════════════════════════════════════════════════════════════
//  ApiError — un cuerpo JSON para los errores, nunca la página HTML
// ═══════════════════════════════════════════════════════════════════

/// An API error serialised as `{"error": "...", "message": "..."}`.
///
/// 🇪🇸 NOTA (por qué no basta con devolver `Status`): si una ruta devuelve `Status`, Rocket
/// invoca su "catcher" por defecto, que responde con una PÁGINA HTML. Para un endpoint que
/// consume un frontend con Axios eso es lo peor de dos mundos: el cliente hace
/// `response.data.message` sobre un string que empieza por `<!DOCTYPE html>` y lo que ve
/// el usuario es "undefined". Un contrato de API serio dice que TODA respuesta, incluidos
/// los fallos, viene en el mismo formato.
///
/// ⚠️ Esto cubre los errores que produce ESTE código. Los que produce Rocket ANTES de
/// llegar aquí (un JSON malformado, una ruta inexistente) siguen saliendo en HTML, porque
/// los genera el catcher por defecto. Arreglarlo son unos `#[catch]` propios; queda
/// pendiente y documentado, que es distinto de estar tapado.
pub struct ApiError {
    status: Status,
    /// 🇪🇸 NOTA: `&'static str` otra vez, y por el mismo motivo que en `sort_column`: el
    /// código de error es un identificador ESTABLE contra el que el frontend programa
    /// (`if (err.error === "duplicate_id")`). Que sea un literal de compilación garantiza
    /// que nunca se cuela ahí un fragmento de la entrada del usuario ni del error interno.
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: Status, code: &'static str, message: impl Into<String>) -> Self {
        ApiError { status, code, message: message.into() }
    }

    /// 400 — the request itself is wrong; retrying it unchanged will fail again.
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::new(Status::BadRequest, code, message)
    }

    /// 500 — logs the real cause and returns a deliberately vague message.
    ///
    /// 🇪🇸 NOTA (por qué el error real NO viaja al cliente): el mensaje de rusqlite dice
    /// cosas como `no such column: Fax` o `UNIQUE constraint failed: Customers.CustomerID`.
    /// Eso es un mapa del esquema servido gratis a cualquiera que sepa provocar un fallo:
    /// nombres de tabla, de columna y de índice. El detalle va al log del servidor, donde
    /// lo necesito yo para depurar; al cliente le llega que algo falló y que no es culpa
    /// suya. La asimetría es deliberada.
    fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        eprintln!("[POST /customers] {context}: {error}");
        ApiError::new(
            Status::InternalServerError,
            "database_error",
            "the request could not be completed due to an internal error",
        )
    }
}

/// 🇪🇸 NOTA (`Responder`): este trait es lo que convierte un valor de Rust en una respuesta
/// HTTP. Implementarlo para `ApiError` es lo que permite escribir
/// `Result<_, ApiError>` como tipo de retorno de la ruta y que Rocket sepa qué hacer con
/// la variante `Err`.
///
/// La conversión se apoya en una propiedad que verifiqué en la documentación de Rocket
/// 0.5.1 antes de escribir esto: **las tuplas `(Status, R)` son ellas mismas `Responder`**
/// cuando `R` lo es. Así que no hace falta construir un `Response` a mano campo a campo —
/// basta con delegar en `(self.status, Json(body))`, que ya sabe poner el código, el
/// `Content-Type: application/json` y el cuerpo.
impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let body = json!({ "error": self.code, "message": self.message });
        (self.status, Json(body)).respond_to(req)
    }
}

// ═══════════════════════════════════════════════════════════════════
//  POST /customers — validación y alta
// ═══════════════════════════════════════════════════════════════════

/// Length of a Northwind customer id (`ALFKI`, `ANATR`, …).
const CUSTOMER_ID_LEN: usize = 5;

// ═══════════════════════════════════════════════════════════════════
// 🇪🇸 NOTA — POR QUÉ AQUÍ SE VALIDA CON DUREZA Y EN EL GET NO
// ═══════════════════════════════════════════════════════════════════
//
// En `GET /customers`, un `sortBy` inválido cae al orden por defecto y un `pageSize`
// absurdo se recorta. Aquí, un `customerId` inválido es un 400 y se acabó. Parece
// incoherente y no lo es: son dos operaciones con consecuencias distintas.
//
//   · El GET PRESENTA datos. Su efecto dura lo que la respuesta, y no deja rastro. Ante
//     una entrada rara, la opción amable —enseñar algo razonable— no tiene coste: nadie
//     se queda con datos peores por ello. Rechazar la petición solo serviría para romper
//     un enlace compartido al que le sobra un parámetro.
//
//   · El POST PERSISTE datos. Su efecto sobrevive a la petición, a la sesión y
//     probablemente a mí. Un "arreglo amable" aquí —aceptar un id de 3 letras, guardar
//     una empresa sin nombre— no se queda en esta respuesta: se queda EN LA TABLA, y
//     cada lectura futura arrastra esa basura. Y a diferencia de una respuesta mal
//     ordenada, no se corrige recargando la página.
//
// La regla, que conviene poder decir en voz alta: SÉ PERMISIVO CON LO QUE MUESTRAS,
// ESTRICTO CON LO QUE GUARDAS. El coste de un error es asimétrico, así que la tolerancia
// también debe serlo.
//
// ⚠️ Y hay un motivo que no es filosófico sino literal, y es el que decide el asunto:
// el esquema de Northwind NO declara NOT NULL en ninguna columna. Ni siquiera en la clave
// primaria — en SQLite, un `PRIMARY KEY` sobre una columna que no es INTEGER (y fuera de
// las tablas STRICT) NO implica NOT NULL; es un bug histórico que se mantiene por
// compatibilidad. Comprobado con `PRAGMA table_info(Customers)`: las once columnas dan
// `notnull = 0`.
//
// Traducido: la base aceptará encantada un cliente sin id y sin nombre. La ÚNICA barrera
// que existe entre la entrada del usuario y una tabla corrupta son las funciones que
// vienen a continuación. No hay una segunda línea de defensa esperando abajo.

/// Trims, upper-cases and validates a customer id.
///
/// 🇪🇸 NOTA (por qué se NORMALIZA A MAYÚSCULAS y no solo se valida): la PK de SQLite
/// compara TEXT byte a byte por defecto, así que `alfki` y `ALFKI` son claves DISTINTAS.
/// Sin normalizar, el POST aceptaría los dos y la tabla acabaría con dos filas para el
/// mismo cliente real: la unicidad seguiría siendo cierta para el motor y falsa para el
/// negocio. Normalizar ANTES de insertar hace que la restricción de la base signifique lo
/// que la gente cree que significa.
///
/// El orden importa: primero `trim`, luego mayúsculas, luego validar. Validar antes de
/// recortar rechazaría `" alfki "` por tener 7 caracteres.
fn normalize_customer_id(raw: &str) -> Result<String, ApiError> {
    let id = raw.trim().to_ascii_uppercase();

    // 🇪🇸 NOTA (`chars().count()` y no `len()`): `len()` devuelve BYTES, no caracteres. Un
    // id como "ÑUÑEZ" son 5 caracteres pero 7 bytes en UTF-8, y `len() == 5` lo rechazaría
    // por el motivo equivocado — dando un mensaje de error que miente. Aquí acaba
    // rechazado igualmente por `is_ascii_alphanumeric`, que es la razón correcta y la que
    // el mensaje explica.
    //
    // Y es `is_ascii_alphanumeric` en vez del `is_alphanumeric` de Unicode a propósito:
    // este último acepta `٣` (el tres árabe) o `ｱ`, que son alfanuméricos de pleno derecho
    // y no tienen nada que hacer en una clave de cinco letras de Northwind.
    let valid = id.chars().count() == CUSTOMER_ID_LEN
        && id.chars().all(|c| c.is_ascii_alphanumeric());

    if valid {
        Ok(id)
    } else {
        Err(ApiError::bad_request(
            "invalid_customer_id",
            "customerId must be exactly 5 alphanumeric ASCII characters (e.g. \"ALFKI\")",
        ))
    }
}

/// Trims a required text field and rejects it if nothing is left.
fn require_non_empty(raw: &str, code: &'static str, field: &str) -> Result<String, ApiError> {
    let value = raw.trim();
    if value.is_empty() {
        Err(ApiError::bad_request(
            code,
            format!("{field} is required and cannot be empty or whitespace-only"),
        ))
    } else {
        Ok(value.to_string())
    }
}

/// Trims an optional text field, collapsing "present but empty" into `None`.
///
/// 🇪🇸 NOTA (POR QUÉ `""` SE GUARDA COMO NULL): un formulario web manda todos sus campos,
/// también los que el usuario no rellenó. Sin esta función, la tabla acabaría con dos
/// representaciones distintas de la misma idea: unas filas con `Region = NULL` (las 93
/// originales) y otras con `Region = ''` (las creadas desde el formulario).
///
/// El daño no es estético. `WHERE Region IS NULL` no encuentra las cadenas vacías y
/// `WHERE Region = ''` no encuentra los NULL, así que cualquier consulta de "clientes sin
/// región" devuelve la mitad de la respuesta — y ninguna de las dos falla, que es lo que
/// lo hace difícil de ver. Los agregados y los `COUNT(Region)` cuentan distinto según por
/// dónde entró la fila.
///
/// "No hay dato" y "el dato es la cadena vacía" son cosas diferentes; mezclarlas en la
/// misma columna es perder información que ya no se recupera. Se decide UNA representación
/// —NULL, la que ya usan las 93 filas existentes— y se normaliza en la frontera, aquí.
fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Creates a customer.
///
/// `POST /customers` → `201 Created`, `Location: /customers/<id>`, body = the new row.
///
/// 🇪🇸 NOTA (por qué el 201 lleva CUERPO y no solo la cabecera): el servidor normaliza —
/// mayúsculas en el id, recortes, cadenas vacías convertidas en NULL—, así que lo que se
/// ha guardado NO es literalmente lo que se mandó. Devolver la fila tal como quedó ahorra
/// al cliente un GET inmediato para enterarse, y le enseña qué transformaciones aplica la
/// API. El `Location` es la otra mitad del contrato: dice DÓNDE vive ahora el recurso.
///
/// 🇪🇸 NOTA (por qué NO se pone `format = "json"` en el atributo): sería lo canónico, pero
/// en Rocket un `format` que no casa hace que la ruta no se seleccione, y la petición
/// termina en un **404 con la página HTML** de Rocket. Es decir: hoy, olvidar la cabecera
/// `Content-Type` daría un error que contradice el contrato de "errores siempre en JSON",
/// y encima uno desorientador (404 en una ruta que existe). En cuanto haya `#[catch]`
/// propios que devuelvan JSON, añadir `format = "json"` pasa a ser gratis y correcto.
/// Es una decisión con fecha, no un olvido.
#[post("/customers", data = "<payload>")]
fn create_customer(
    db: &State<Db>,
    payload: Json<NewCustomer>,
) -> Result<Created<Json<Customer>>, ApiError> {
    // 🇪🇸 NOTA: `into_inner()` saca el `NewCustomer` del envoltorio `Json`. A partir de
    // aquí trabajamos con datos de Rust normales; el wrapper solo servía para que Rocket
    // supiera cómo leer el cuerpo de la petición.
    let body = payload.into_inner();

    // ─── 1. Validar y normalizar ANTES de tocar la base ───
    //
    // 🇪🇸 NOTA: el `?` corta la ejecución en el primer campo inválido y devuelve el
    // `ApiError` correspondiente. Nada llega a la conexión hasta que TODOS los campos
    // están limpios: la base nunca ve una entrada a medio validar, y no hace falta
    // deshacer nada porque no se empezó.
    let customer_id = normalize_customer_id(&body.customer_id)?;
    let company_name = require_non_empty(&body.company_name, "invalid_company_name", "companyName")?;

    let contact_name = optional_text(body.contact_name);
    let contact_title = optional_text(body.contact_title);
    let address = optional_text(body.address);
    let city = optional_text(body.city);
    let region = optional_text(body.region);
    let postal_code = optional_text(body.postal_code);
    let country = optional_text(body.country);
    let phone = optional_text(body.phone);
    let fax = optional_text(body.fax);

    // ─── 2. INSERT y SELECT bajo el MISMO lock ───
    //
    // 🇪🇸 NOTA (mismo argumento que en el GET, con más filo): entre el INSERT y el SELECT
    // que lee la fila recién creada, otra petición podría hacer un PUT sobre ese mismo id
    // —o un DELETE—. Soltando el mutex en medio, el 201 podría devolver datos que el
    // cliente nunca mandó, o fallar al leer una fila que acaba de crear. Con la guarda
    // viva durante ambas, "lo que devuelvo" es literalmente "lo que acabo de escribir".
    let conn = db
        .0
        .lock()
        .expect("the SQLite mutex was poisoned by a panicking thread");

    // 🇪🇸 NOTA: la lista de columnas sale de `Customer::COLUMNS`, la MISMA constante que usa
    // el SELECT del GET y que define el orden que espera `Customer::from_row`. Los once
    // `?N` van en ese orden, y por eso el orden de `params![...]` de abajo no es
    // negociable. Reutilizar la constante evita el fallo clásico de este endpoint: añadir
    // una columna al modelo y que el INSERT siga con diez, metiendo el teléfono en el país.
    let insert_sql = format!(
        "INSERT INTO Customers ({}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        Customer::COLUMNS
    );

    // ⚠️ Nada de lo que viene del usuario se interpola: `format!` solo monta la parte fija
    // de la sentencia con una constante mía. Los once valores viajan como parámetros, y un
    // `Option::None` se convierte solo en NULL — rusqlite implementa `ToSql` para
    // `Option<T>`, así que no hay que escribir un caso especial para los campos ausentes.
    conn.execute(
        &insert_sql,
        params![
            customer_id,
            company_name,
            contact_name,
            contact_title,
            address,
            city,
            region,
            postal_code,
            country,
            phone,
            fax
        ],
    )
    .map_err(|e| match e {
        // 🇪🇸 NOTA (por qué un 409 y no un 500): el `match guard` (`if err.code == ...`)
        // distingue el ÚNICO error de base que no es culpa del servidor. Un id repetido
        // significa "tu petición choca con el estado actual del recurso", que es
        // exactamente la definición de 409 Conflict. Un 500 diría "me he roto", y sería
        // mentira: la base funcionó perfectamente e hizo su trabajo.
        //
        // La diferencia es operativa, no cosmética. Un 500 despierta a alguien de guardia
        // y no se debe reintentar; un 409 lo resuelve el propio cliente eligiendo otro id.
        //
        // ⚠️ Que esto funcione depende de que la tabla tenga PRIMARY KEY sobre CustomerID.
        // Comprobado en el esquema: `PRIMARY KEY (CustomerID)`, con su
        // `sqlite_autoindex_Customers_1`. Si no lo tuviera, el segundo POST con el mismo id
        // NO daría error: crearía una fila duplicada en silencio, y este brazo del `match`
        // sería código muerto que da una falsa sensación de seguridad. Nunca des por
        // supuesta una restricción que no has mirado.
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == ErrorCode::ConstraintViolation =>
        {
            ApiError::new(
                Status::Conflict,
                "duplicate_id",
                format!("a customer with id '{customer_id}' already exists"),
            )
        }
        other => ApiError::internal("insert failed", other),
    })?;

    // ─── 3. Releer la fila para devolverla ───
    //
    // 🇪🇸 NOTA: se relee en vez de reconstruir el `Customer` en memoria a partir de las
    // variables. Es una consulta más, y a cambio lo que devuelve la API es lo que
    // REALMENTE hay en disco, no lo que yo creo que escribí. Si un DEFAULT, un trigger o
    // una conversión de tipo cambiara algo por el camino, el cliente lo vería. Es la misma
    // idea que verificar `/health` en vez de fiarse de que el arranque fue bien.
    let select_sql = format!(
        "SELECT {} FROM Customers WHERE CustomerID = ?1",
        Customer::COLUMNS
    );

    let created = conn
        .query_row(&select_sql, params![customer_id], Customer::from_row)
        .map_err(|e| ApiError::internal("re-reading the created row failed", e))?;

    // 🇪🇸 NOTA (`Created`, verificado en la documentación de Rocket 0.5.1): se construye con
    // `Created::new(location)` —el string se pone TAL CUAL en la cabecera `Location`— y el
    // cuerpo se adjunta con `.body(responder)`. El responder envuelto es quien fija el
    // `Content-Type`, y por eso pasamos `Json<Customer>` y no el `Customer` pelado.
    //
    // Existe también `.tagged_body()`, que además calcula un `ETag` con el hash del cuerpo.
    // No se usa aquí porque exige `R: Hash` y `Json<Customer>` no lo implementa; tendría
    // sentido el día que haya cachés o peticiones condicionales de por medio.
    Ok(Created::new(format!("/customers/{customer_id}")).body(Json(created)))
}

// ═══════════════════════════════════════════════════════════════════
//  Arranque
// ═══════════════════════════════════════════════════════════════════

/// Builds and configures the Rocket instance.
///
/// 🇪🇸 NOTA (`rocket::custom` vs `rocket::build`): `rocket::build()` usa la configuración
/// por defecto (puerto 8000). Como el enunciado exige el 8001, partimos de esa misma
/// configuración por defecto (`Config::figment()`) y le fusionamos el puerto, lo que
/// obliga a usar `rocket::custom(figment)`.
///
/// Usamos `.merge()` sobre `Config::figment()` en lugar de construir un `Config` a mano
/// porque `merge` conserva la cadena de proveedores de Rocket: `Rocket.toml` y las
/// variables `ROCKET_*` siguen funcionando, y `ROCKET_PORT` puede sobreescribir esto en
/// despliegue sin recompilar. Un `Config { port: 8001, ..Default::default() }` mataría
/// esa capacidad.
///
/// 🇪🇸 NOTA (`#[launch]`): esta macro genera el `fn main()` y arranca el runtime async de
/// Tokio. En Rocket 0.4 se escribía `fn main() { rocket::ignite()...launch(); }`; en 0.5
/// `ignite()` ya no existe y el arranque es asíncrono.
#[launch]
fn rocket() -> _ {
    let figment = rocket::Config::figment().merge(("port", PORT));

    rocket::custom(figment)
        // 🇪🇸 NOTA (POR QUÉ UN FAIRING Y NO `db::open(DB_PATH).unwrap()`):
        //
        // Lo obvio sería `.manage(db::open(DB_PATH).unwrap())`. Funciona, pero si el
        // archivo no está, el programa muere con un panic: un volcado con "called
        // `Result::unwrap()` on an `Err` value", el nombre del archivo fuente y un número
        // de línea. Eso le dice algo a quien escribió el código y nada a quien lo ejecuta.
        //
        // `AdHoc::try_on_ignite` es el mecanismo que Rocket 0.5 ofrece para esto:
        // se ejecuta en la fase de "ignition", ANTES de abrir el puerto, y si devuelve
        // `Err(rocket)` aborta el lanzamiento de forma ordenada. Ganamos tres cosas:
        //
        //   1. Un mensaje propio, con el diagnóstico que de verdad importa (la ruta
        //      absoluta que se intentó abrir y el cwd desde el que se resolvió).
        //   2. Rocket añade su propio error nombrando el fairing que falló, así que en el
        //      log queda claro QUÉ parte del arranque se cayó.
        //   3. El puerto NUNCA llega a abrirse. Con `unwrap()` en `.manage()` el orden
        //      también sería ese, pero aquí queda garantizado por el ciclo de vida de
        //      Rocket, no por dónde casualmente pusimos la llamada.
        //
        // ⚠️ Comprobado, y conviene no vender más de lo que hay: `#[launch]` sigue
        // terminando en un panic de la propia Rocket ("aborting due to fairing
        // failure(s)", exit code 101). Es decir, ganamos el diagnóstico y el orden de
        // arranque, NO una salida limpia por código de retorno. Para eso habría que
        // sustituir `#[launch]` por un `#[rocket::main]` manual que capture el error y
        // llame a `std::process::exit(1)`. No compensa a esta escala.
        //
        // El coste es que la conexión ya no se abre en `main`, sino dentro de un closure
        // async. A cambio, el fallo se comunica en lugar de simplemente ocurrir.
        .attach(AdHoc::try_on_ignite("Northwind SQLite", |rocket| async {
            match db::open(DB_PATH) {
                // `.manage()` guarda el valor en el mapa de estado de Rocket, indexado
                // por tipo. A partir de aquí cualquier ruta puede pedir `&State<Db>`.
                Ok(db) => Ok(rocket.manage(db)),
                Err(e) => {
                    // Diagnostics go to stderr unconditionally rather than through
                    // Rocket's `error!` macro: a startup failure must stay visible even
                    // if the configured log level would filter it out.
                    let cwd = std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "<unknown>".to_string());

                    eprintln!("\n╭─ FATAL: could not open the Northwind database ─────────");
                    eprintln!("│ path (as configured) : {DB_PATH}");
                    eprintln!("│ working directory    : {cwd}");
                    eprintln!("│ resolved to          : {cwd}/{DB_PATH}");
                    eprintln!("│ sqlite error         : {e}");
                    eprintln!("│");
                    eprintln!("│ Hint: run the server with `cargo run` from the `back/`");
                    eprintln!("│ directory, or download the database as described in the");
                    eprintln!("│ README (it is git-ignored and does not ship with the repo).");
                    eprintln!("╰────────────────────────────────────────────────────────\n");

                    // 🇪🇸 NOTA: devolvemos `Err(rocket)` — el propio Rocket, no el error.
                    // La firma del fairing es `Result<Rocket<Build>, Rocket<Build>>`: el
                    // tipo es el mismo en ambos lados y solo la variante indica éxito o
                    // fallo. Rocket lo pide así para poder inspeccionar la instancia y
                    // decir qué fairing abortó el arranque.
                    Err(rocket)
                }
            }
        }))
        .mount("/", routes![health, list_customers, create_customer])
}
