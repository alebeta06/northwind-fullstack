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

// 🇪🇸 NOTA (aquí hubo un `#[allow(dead_code)]`, y ya no queda ninguno en el proyecto):
// silenciaba los cinco warnings de `models.rs` mientras el CRUD no existía. Según fueron
// llegando las rutas, el atributo se fue mudando a los structs que aún no se usaban
// —primero a `NewCustomer` y `UpdateCustomer`, luego solo a `UpdateCustomer`— hasta
// desaparecer con el PUT.
//
// Que ese recorrido termine en cero es el punto: un `allow` con una condición de salida
// escrita al lado se acaba borrando; uno puesto "por ahora" en la cabecera del módulo se
// queda para siempre y, el día que de verdad sobre código, nadie se entera.
mod models;

use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::request::Request;
use rocket::response::status::{Created, NoContent};
use rocket::response::{self, Responder};
use rocket::serde::json::{json, Json, Value};
use rocket::{FromForm, State};
use rusqlite::{params, Connection, ErrorCode, ToSql};

use db::Db;
use models::{Customer, NewCustomer, Paginated, UpdateCustomer};

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
// 🇪🇸 NOTA (por qué el error ya no es `Status` a secas): devolver `Status` funcionaba —
// ahora incluso saldría en JSON, porque el catcher del 500 lo recogería—, pero pierde por
// el camino lo único que distingue un fallo de otro: qué consulta se rompió. Con
// `ApiError` cada punto de fallo loguea su propio contexto y el cliente sigue recibiendo
// el mismo cuerpo genérico. El catcher es la red de seguridad, no la primera opción.
#[get("/customers?<q..>")]
fn list_customers(db: &State<Db>, q: ListQuery) -> Result<Json<Paginated<Customer>>, ApiError> {
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
    .map_err(|e| ApiError::internal("GET /customers · count query", e))?;

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
        // 🇪🇸 NOTA: al log va la sentencia entera, que es lo que de verdad se necesita para
        // depurar un `prepare` fallido. Al cliente no: la consulta enseña el esquema.
        ApiError::internal(&format!("GET /customers · prepare ({select_sql})"), e)
    })?;

    // 🇪🇸 NOTA (`query_map` + `collect`): `query_map` devuelve un iterador perezoso de
    // `Result<Customer>`. El truco del `collect::<rusqlite::Result<Vec<_>>>()` es que
    // Rust sabe convertir un iterador de Results en un Result de Vec: si alguna fila
    // falla, se queda con el primer error y descarta el resto; si todas van bien, obtienes
    // el Vec. Ahorra el bucle con `match` fila a fila.
    let data = stmt
        .query_map(rusqlite::params_from_iter(params), Customer::from_row)
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<Customer>>>())
        .map_err(|e| ApiError::internal("GET /customers · row query", e))?;

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
/// llegar aquí (un JSON malformado, una ruta inexistente) no pasan por aquí: los atienden
/// los `#[catch]` de más abajo, que construyen este MISMO tipo para que el formato de
/// error sea uno solo en toda la API.
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

    /// 404 — the id is well-formed but no such customer exists.
    ///
    /// 🇪🇸 NOTA: el id se refleja en el mensaje, y es seguro: llega aquí después de pasar
    /// por `normalize_customer_id`, así que son exactamente 5 caracteres alfanuméricos
    /// ASCII. No hay nada que escapar porque no hay nada que se pueda colar.
    fn not_found(id: &str) -> Self {
        ApiError::new(
            Status::NotFound,
            "not_found",
            format!("no customer with id '{id}'"),
        )
    }

    /// 500 — logs the real cause and returns a deliberately vague message.
    ///
    /// 🇪🇸 NOTA (por qué el error real NO viaja al cliente): el mensaje de rusqlite dice
    /// cosas como `no such column: Fax` o `UNIQUE constraint failed: Customers.CustomerID`.
    /// Eso es un mapa del esquema servido gratis a cualquiera que sepa provocar un fallo:
    /// nombres de tabla, de columna y de índice. El detalle va al log del servidor, donde
    /// lo necesito yo para depurar; al cliente le llega que algo falló y que no es culpa
    /// suya. La asimetría es deliberada.
    ///
    /// 🇪🇸 NOTA: `context` dice QUÉ operación falló ("GET /customers · count query"), y lo
    /// pone quien llama porque es el único que lo sabe. Sin ese dato, el log de un 500
    /// sería una línea de rusqlite sin ninguna pista de por dónde entró la petición.
    fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        eprintln!("[api error] {context}: {error}");
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
//  Catchers — la otra mitad de la historia de errores
// ═══════════════════════════════════════════════════════════════════
//
// 🇪🇸 NOTA — POR QUÉ HACEN FALTA CATCHERS SI YA EXISTE `ApiError`
//
// `ApiError` solo puede cubrir los fallos que ocurren DENTRO del handler. Y hay una clase
// entera de fallos que ocurre ANTES de que el handler llegue a ejecutarse.
//
// Recordemos el orden en que Rocket atiende una petición:
//
//   1. ROUTING      — busca una ruta cuyo método, path y `format` casen.
//   2. REQUEST GUARDS — resuelve los parámetros de la firma (`&State<Db>`, `ListQuery`…).
//   3. DATA GUARD   — lee el cuerpo y lo convierte al tipo pedido (`Json<NewCustomer>`).
//   4. HANDLER      — ← recién AQUÍ empieza mi código, y por tanto `ApiError`.
//   5. RESPONDER    — convierte lo devuelto en una respuesta HTTP.
//
// Un JSON con una coma de más revienta en el paso 3. Una ruta mal escrita ni siquiera pasa
// del 1. En ambos casos el cuerpo de `create_customer` NO SE EJECUTA: no hay ningún punto
// del programa donde yo pueda construir un `ApiError`, porque el flujo nunca entra en mi
// función. Da igual lo bien escrito que esté el handler.
//
// Cuando eso pasa, Rocket se queda con un `Status` y busca un CATCHER para él. Si no hay
// ninguno registrado, usa el suyo, que responde con una página HTML — la que veíamos en
// las pruebas del POST. El catcher es el ÚNICO gancho que el framework ofrece para ese
// tramo del ciclo de vida.
//
// Dicho de otra forma: `ApiError` protege la salida de mi código; los catchers protegen
// todo lo que rodea a mi código. Hacen falta los dos para poder afirmar, sin asteriscos,
// que esta API responde siempre en JSON.
//
// ⚠️ Los catchers NO interceptan un `Status` que forme parte de una respuesta ya
// construida. Si el handler devuelve un `ApiError` con 400, eso es una respuesta COMPLETA
// (código, `Content-Type` y cuerpo) y sale tal cual: el catcher del 400 no se entera. Solo
// se invoca cuando Rocket tiene un status "suelto", sin cuerpo. Por eso el 400 de
// `invalid_customer_id` conserva su mensaje específico en lugar de convertirse en el
// genérico "malformed_json" de aquí abajo.

/// 400 — the body could not be parsed at all (broken JSON syntax).
///
/// 🇪🇸 NOTA: los catchers devuelven `ApiError`, el MISMO tipo que usan las rutas. No es
/// pereza: es la garantía de que existe UN solo formato de error en toda la API. Si aquí
/// se construyera un `json!` a mano, nada impediría que dentro de un mes este dijera
/// `{"code": …}` y el handler `{"error": …}`, y el frontend tendría que probar las dos
/// formas en cada `catch`. Con un único tipo, el formato se cambia en un sitio o no se
/// cambia en ninguno.
#[catch(400)]
fn catch_bad_request() -> ApiError {
    ApiError::new(
        Status::BadRequest,
        "malformed_json",
        "the request body is not valid JSON — check for trailing commas, unquoted keys \
         or truncated content",
    )
}

/// 422 — syntactically valid JSON that does not match the expected shape.
///
/// 🇪🇸 NOTA (la diferencia 400 vs 422, que la impone `Json<T>` y no yo): el data guard de
/// Rocket distingue dos fallos distintos y les da códigos distintos.
///
///   · 400 Bad Request         → el texto NI SIQUIERA ES JSON. `serde_json` no pasa del
///                               analizador léxico.
///   · 422 Unprocessable Entity → es JSON perfectamente formado, pero no encaja con el
///                               tipo: falta `companyName`, o `customerId` viene como
///                               número en vez de string.
///
/// La distinción es útil para quien depura: un 400 apunta a cómo se construyó la cadena
/// (una concatenación, un template roto); un 422 apunta a QUÉ campos se mandaron. Son dos
/// bugs de naturaleza distinta en el cliente, y merecen mensajes distintos.
#[catch(422)]
fn catch_unprocessable() -> ApiError {
    ApiError::new(
        Status::UnprocessableEntity,
        "invalid_payload",
        "the JSON is well-formed but does not match the expected shape — check that every \
         required field is present and has the right type",
    )
}

/// 404 — no route matched.
///
/// 🇪🇸 NOTA (por qué el mensaje habla también del método y del `Content-Type`): un 404 en
/// Rocket no significa "ese path no existe", sino "ninguna ruta CASA con esta petición", y
/// el path es solo uno de los criterios. Con `format = "json"` en el POST, mandar el
/// cuerpo correcto al path correcto pero sin la cabecera `Content-Type: application/json`
/// también acaba aquí. El mensaje lo dice para que quien lo lea no pierda media hora
/// mirando la URL, que está bien.
#[catch(404)]
fn catch_not_found(req: &Request<'_>) -> ApiError {
    ApiError::new(
        Status::NotFound,
        "not_found",
        // 🇪🇸 NOTA: reflejar la URI que pidió el cliente es información SUYA, no mía — no
        // filtra nada del servidor. Y va dentro de un valor JSON generado por `serde`, que
        // escapa comillas y barras invertidas, así que no puede romper la estructura del
        // documento ni inyectar nada. Reflejar la entrada es seguro cuando sabes en qué
        // contexto se serializa; el problema aparece al meterla en HTML sin escapar, que
        // es justo lo que esta API nunca hace.
        format!(
            "no route matches {} {} — check the path, the HTTP method and, for requests \
             with a body, the Content-Type header",
            req.method(),
            req.uri()
        ),
    )
}

/// 500 — something broke inside Rocket or in a handler that returned a bare status.
///
/// 🇪🇸 NOTA: en la práctica este catcher casi no salta, porque los 500 de las rutas ya
/// salen como `ApiError::internal` (respuesta completa, no un status suelto). Cubre lo que
/// queda: un panic dentro de un handler, que Rocket captura y convierte en un 500 sin
/// cuerpo. Que sea raro no lo hace prescindible — es precisamente el caso en el que menos
/// ganas tienes de descubrir que la API contesta HTML.
#[catch(500)]
fn catch_internal() -> ApiError {
    ApiError::new(
        Status::InternalServerError,
        "internal_error",
        "the request could not be completed due to an internal error",
    )
}

/// Fallback for every status without a dedicated catcher.
///
/// 🇪🇸 NOTA (por qué un `default` además de los cuatro anteriores): los específicos cubren
/// lo que sé que va a pasar. El `default` cubre lo que no. Hoy mismo ya tiene trabajo: el
/// 503 de `/health` cuando la base no responde, y el 405 que devolvería un `DELETE` sobre
/// una ruta que solo acepta GET. Sin él, cada status nuevo que aparezca en el futuro
/// —porque se añada una ruta, un guard o una versión de Rocket— vuelve a salir en HTML sin
/// que nadie se dé cuenta. Una lista de casos concretos siempre está incompleta; la
/// pregunta no es si aparecerá uno nuevo, sino cuándo.
///
/// 🇪🇸 NOTA (la firma la fija Rocket): un catcher `default` recibe `(Status, &Request)` —
/// necesita el status porque, a diferencia de los específicos, no lo conoce de antemano.
/// Los específicos pueden tomar `(&Request)`, `()` o también el status; aquí el primero se
/// deja sin argumentos porque el mensaje es fijo.
///
/// ⚠️ El `code` no puede derivarse del status: es `&'static str` a propósito (ver el
/// struct `ApiError`), y un slug como `"service_unavailable"` habría que construirlo en
/// tiempo de ejecución. Así que el default clasifica por FAMILIA —`client_error` o
/// `server_error`—, que es una distinción estable y honesta, y deja el detalle numérico
/// para el `message`. Prefiero un código impreciso pero cierto a uno inventado.
#[catch(default)]
fn catch_default(status: Status, req: &Request<'_>) -> ApiError {
    let code = if status.code < 500 {
        "client_error"
    } else {
        "server_error"
    };

    ApiError::new(
        status,
        code,
        format!(
            "{} {} failed with status {} {}",
            req.method(),
            req.uri(),
            status.code,
            status.reason_lossy()
        ),
    )
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
/// 🇪🇸 NOTA (`format = "json"`, que antes NO estaba): este atributo exige que la petición
/// llegue con `Content-Type: application/json`; si no, la ruta no se selecciona siquiera.
/// No se puso en su momento a propósito: sin catchers, olvidar esa cabecera terminaba en
/// un 404 con la página HTML de Rocket — un error que contradecía el contrato de "errores
/// siempre en JSON" y encima desorientaba (un 404 en una ruta que existe). Ahora que
/// `catch_not_found` responde en JSON y explica que el `Content-Type` es uno de los
/// criterios de enrutado, el filtro sale gratis: rechaza cuerpos que no dicen ser JSON
/// antes de intentar parsearlos, y el fallo se explica solo.
#[post("/customers", format = "json", data = "<payload>")]
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
    let created = find_customer(&conn, &customer_id)
        .map_err(|e| ApiError::internal("POST /customers · re-read", e))?;

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
//  Operaciones sobre UN cliente — GET / PUT / DELETE por id
// ═══════════════════════════════════════════════════════════════════

/// Reads a single customer, using the caller's already-acquired lock.
///
/// 🇪🇸 NOTA (por qué recibe `&Connection` y no `&State<Db>`): si esta función pidiera el
/// estado, tendría que hacer el `.lock()` ella misma — y entonces sería IMPOSIBLE usarla
/// dentro de una operación que ya tiene el lock cogido. Peor: `std::sync::Mutex` no es
/// reentrante, así que un segundo `.lock()` desde el mismo hilo no da error, se queda
/// colgado para siempre. Un interbloqueo de una sola línea.
///
/// Recibiendo la conexión prestada, quien manda sobre el lock es SIEMPRE la ruta, que es
/// la que sabe cuánto tiene que durar la sección crítica. Esta función solo lee. El
/// `MutexGuard` se convierte en `&Connection` solo con pasarlo, por el `Deref` de la
/// guarda.
///
/// 🇪🇸 NOTA: devuelve `rusqlite::Result` y no `Result<_, ApiError>` a propósito. "No hay
/// fila" significa cosas distintas según quién pregunte: en el `GET /customers/<id>` es un
/// 404 legítimo; justo después de un INSERT o un UPDATE con éxito es un 500, porque
/// implica que la fila que acabo de escribir se ha esfumado bajo mi propio lock. Traducir
/// el error aquí obligaría a todos a compartir la misma interpretación, y no la comparten.
fn find_customer(conn: &Connection, id: &str) -> rusqlite::Result<Customer> {
    let sql = format!(
        "SELECT {} FROM Customers WHERE CustomerID = ?1",
        Customer::COLUMNS
    );
    conn.query_row(&sql, params![id], Customer::from_row)
}

/// Fetches one customer by id.
///
/// `GET /customers/<id>` → 200 + the customer, or 404.
///
/// 🇪🇸 NOTA (`<id>` es un "parameter guard"): Rocket saca el segmento de la URL y lo
/// convierte al tipo del parámetro con el trait `FromParam`. Con `&str` acepta cualquier
/// segmento; si el tipo fuera `usize`, una URL con letras simplemente no casaría con la
/// ruta (y acabaría en el catcher del 404). Aquí se usa `&str` porque los ids de Northwind
/// son texto, y la validación de forma la hace `normalize_customer_id`, que da un mensaje
/// mucho más útil que un 404 mudo.
///
/// ⚠️ Un id mal formado (`/customers/xx`) devuelve 400, no 404. Es deliberado: "lo que
/// pides no puede existir" y "lo que pides no está aquí" son diagnósticos distintos, y al
/// cliente le sirve más el primero. Un 404 le haría buscar el cliente; el 400 le dice que
/// mire cómo construye la URL.
#[get("/customers/<id>")]
fn get_customer(db: &State<Db>, id: &str) -> Result<Json<Customer>, ApiError> {
    // 🇪🇸 NOTA: la MISMA normalización que el POST, y por el mismo motivo. Si el POST
    // guarda siempre en mayúsculas pero el GET busca tal cual, `/customers/alfki` daría
    // 404 sobre un cliente que existe — un bug desconcertante que además solo aparece
    // según cómo escriba el usuario la URL. Las dos rutas tienen que compartir la idea de
    // qué es "el mismo id"; compartir la función es la forma de que no se separen.
    let id = normalize_customer_id(id)?;

    let conn = db
        .0
        .lock()
        .expect("the SQLite mutex was poisoned by a panicking thread");

    // 🇪🇸 NOTA (POR QUÉ SE DISTINGUE `QueryReturnedNoRows` DEL RESTO): `query_row` devuelve
    // `Err` en los dos casos, pero significan cosas opuestas.
    //
    //   · `QueryReturnedNoRows` → la consulta funcionó PERFECTAMENTE. Simplemente no hay
    //     ningún cliente con ese id. Eso es un 404: la API está bien, el recurso no está.
    //   · Cualquier otro error   → la consulta no llegó a ejecutarse o falló a medias
    //     (disco, esquema, corrupción). Eso sí es un 500.
    //
    // Meter los dos en el mismo saco es el error clásico de este endpoint, y tiene
    // consecuencias reales: un 500 dispara alertas, se reintenta y hace pensar que el
    // servidor está roto, cuando lo único que pasaba es que el cliente pidió un id que no
    // existe. El `match` sobre la variante concreta del error es lo que separa "no hay
    // nada" de "algo va mal".
    find_customer(&conn, &id).map(Json).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => ApiError::not_found(&id),
        other => ApiError::internal("GET /customers/<id> · select", other),
    })
}

/// Replaces a customer.
///
/// `PUT /customers/<id>` → 200 + the updated customer, or 404.
///
/// ═══════════════════════════════════════════════════════════════════
/// 🇪🇸 NOTA — SEMÁNTICA DE REEMPLAZO TOTAL (PUT ≠ PATCH)
/// ═══════════════════════════════════════════════════════════════════
///
/// Esto es lo más importante que hay que entender de esta ruta, y lo que más veces se
/// implementa mal: **PUT sustituye el recurso ENTERO por lo que mandas**. No fusiona.
///
/// Un campo que no venga en el cuerpo no se queda "como estaba": se queda en NULL. Si el
/// cliente ALFKI tiene teléfono y mandas un PUT con solo `companyName`, el teléfono
/// DESAPARECE. No es un bug de esta implementación, es la definición de PUT en HTTP: el
/// cuerpo es el nuevo estado completo del recurso, y lo que no está en él, no está.
///
/// La operación de "cambia solo estos campos y deja el resto" es PATCH, que es un verbo
/// distinto y no está en el enunciado. Que `UpdateCustomer` tenga los campos como
/// `Option<String>` puede despistar: ese `Option` distingue "null" de "un texto", no
/// "ausente" de "presente".
///
/// ⚠️ Consecuencia PRÁCTICA para el frontend, que hay que decir en voz alta porque es
/// donde se pierden datos de verdad: el formulario de edición debe cargarse con TODOS los
/// campos del cliente (un GET previo) y mandarlos TODOS de vuelta, incluidos los que el
/// usuario no tocó. Un formulario que solo envíe los campos modificados irá borrando el
/// resto del registro en cada guardado, silenciosamente y sin un solo error.
///
/// 🇪🇸 NOTA (por qué `format = "json"`, igual que el POST): el mismo filtro por el mismo
/// motivo — rechaza cuerpos que no dicen ser JSON antes de intentar parsearlos, y ahora
/// que hay catchers, el rechazo sale en JSON y se explica solo.
#[put("/customers/<id>", format = "json", data = "<payload>")]
fn update_customer(
    db: &State<Db>,
    id: &str,
    payload: Json<UpdateCustomer>,
) -> Result<Json<Customer>, ApiError> {
    let id = normalize_customer_id(id)?;
    let body = payload.into_inner();

    // ─── 1. Validar, con el mismo rasero que el POST ───
    //
    // 🇪🇸 NOTA: un UPDATE persiste igual que un INSERT, así que la validación tiene que ser
    // idéntica de estricta. Sería absurdo blindar la puerta de entrada y dejar abierta la
    // de al lado: quien quisiera dejar una empresa sin nombre solo tendría que crearla bien
    // y editarla mal. Se reutilizan LAS MISMAS funciones, no unas parecidas — dos
    // validaciones que "hacen lo mismo" acaban divergiendo en cuanto una de las dos cambia.
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

    // ─── 2. UPDATE y SELECT bajo el mismo lock ───
    let conn = db
        .0
        .lock()
        .expect("the SQLite mutex was poisoned by a panicking thread");

    // 🇪🇸 NOTA: las diez columnas del SET se listan explícitamente y en el mismo orden que
    // los `?N` y que los parámetros de abajo. `CustomerID` NO está entre ellas: la clave
    // primaria no se toca en un PUT. Cambiarla no sería "actualizar este cliente" sino
    // "crear otro y borrar este", que es una operación distinta y con otras consecuencias
    // (los pedidos apuntan a la clave vieja). El id de la URL solo aparece en el WHERE.
    const UPDATE_SQL: &str = "UPDATE Customers SET \
         CompanyName = ?1, ContactName = ?2, ContactTitle = ?3, Address = ?4, City = ?5, \
         Region = ?6, PostalCode = ?7, Country = ?8, Phone = ?9, Fax = ?10 \
         WHERE CustomerID = ?11";

    // 🇪🇸 NOTA (`execute` devuelve el NÚMERO DE FILAS AFECTADAS): ese contador es la forma
    // de saber si el cliente existía, y evita la consulta previa de "¿existe?" — que
    // además sería incorrecta en un sistema concurrente: entre el SELECT de comprobación y
    // el UPDATE, otra petición podría borrar la fila. Aquí no hay ventana: la pregunta y
    // la acción son la MISMA sentencia. Cero filas = no había nada que actualizar.
    let affected = conn
        .execute(
            UPDATE_SQL,
            params![
                company_name,
                contact_name,
                contact_title,
                address,
                city,
                region,
                postal_code,
                country,
                phone,
                fax,
                id
            ],
        )
        .map_err(|e| ApiError::internal("PUT /customers/<id> · update", e))?;

    if affected == 0 {
        return Err(ApiError::not_found(&id));
    }

    // 🇪🇸 NOTA: se relee bajo el MISMO lock, por lo mismo que en el POST: sin él, entre el
    // UPDATE y esta lectura cabría un DELETE de otra petición, y el 200 devolvería un error
    // al no encontrar la fila que acaba de escribir. Aquí `QueryReturnedNoRows` sí sería un
    // 500 de pleno derecho —significaría que el mutex no está haciendo su trabajo—, y por
    // eso todos los errores de esta lectura caen en el mismo saco.
    let updated = find_customer(&conn, &id)
        .map_err(|e| ApiError::internal("PUT /customers/<id> · re-read", e))?;

    Ok(Json(updated))
}

/// Deletes a customer.
///
/// `DELETE /customers/<id>` → 204 No Content, 404, or 409 if it has orders.
///
/// 🇪🇸 NOTA (`status::NoContent`, verificado en la documentación de Rocket 0.5.1): es un
/// struct unitario cuyo `Responder` pone el código 204 y deja el cuerpo VACÍO. Devolver
/// `Json(json!({"deleted": true}))` con un 204 sería una respuesta contradictoria: el 204
/// significa literalmente "no hay contenido", y un cuerpo ahí es algo que los
/// intermediarios pueden descartar. Si se quisiera devolver algo, el código correcto sería
/// 200, no 204.
///
/// El tipo en la firma documenta la respuesta: quien lee `Result<NoContent, ApiError>` sabe
/// que en el camino feliz no hay cuerpo, sin tener que leer el cuerpo de la función.
#[delete("/customers/<id>")]
fn delete_customer(db: &State<Db>, id: &str) -> Result<NoContent, ApiError> {
    let id = normalize_customer_id(id)?;

    let conn = db
        .0
        .lock()
        .expect("the SQLite mutex was poisoned by a panicking thread");

    // ⚠️ El id va parametrizado, como todo lo demás. Un DELETE con el id interpolado es el
    // ejemplo de libro de por qué la regla no admite excepciones "porque este valor ya
    // está validado": aquí el precio de equivocarse es una tabla vacía.
    let affected = conn
        .execute("DELETE FROM Customers WHERE CustomerID = ?1", params![id])
        .map_err(|e| match e {
            // 🇪🇸 NOTA (POR QUÉ ESTE 409 EXISTE, Y POR QUÉ ES EL CASO NORMAL):
            //
            // `db.rs` activa `PRAGMA foreign_keys = ON` en cada conexión. Gracias a eso,
            // borrar un cliente que tiene pedidos falla con `ConstraintViolation` en vez de
            // dejar filas de `Orders` apuntando a un cliente inexistente. Sin ese PRAGMA
            // —que SQLite trae DESACTIVADO por defecto— este brazo del `match` no saltaría
            // nunca y la base se corrompería en silencio.
            //
            // Comprobado en el esquema: `Orders.CustomerID → Customers.CustomerID` con
            // `ON DELETE NO ACTION`, es decir, la restricción se aplica y no cascadea.
            // (`CustomerCustomerDemo` también referencia a `Customers`.)
            //
            // ⚠️ Consecuencia que conviene anticipar antes de que alguien la reporte como
            // bug: los 93 clientes originales de Northwind TIENEN pedidos —ALFKI tiene
            // 163—, así que el 409 es la respuesta NORMAL al intentar borrar cualquiera de
            // ellos. No es que el borrado esté roto: es que borrar un cliente con historial
            // de pedidos es precisamente lo que la integridad referencial impide. Un
            // cliente creado por el POST nace sin pedidos y se borra sin problema.
            //
            // Y por eso es 409 y no 500: la base hizo su trabajo. El conflicto está entre
            // lo que pide el cliente y el estado actual de los datos, no en el servidor.
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == ErrorCode::ConstraintViolation =>
            {
                ApiError::new(
                    Status::Conflict,
                    "has_orders",
                    format!(
                        "customer '{id}' cannot be deleted because it has associated orders \
                         — delete or reassign them first"
                    ),
                )
            }
            other => ApiError::internal("DELETE /customers/<id> · delete", other),
        })?;

    // 🇪🇸 NOTA: mismo truco que en el PUT — el contador de filas responde a "¿existía?" sin
    // una consulta previa y sin ventana de carrera.
    //
    // ⚠️ Un DELETE sobre algo que no existe podría defenderse como 204 (el borrado es
    // idempotente: el resultado final es el mismo, no hay cliente con ese id). Aquí se
    // devuelve 404 porque el enunciado lo pide y porque, para un panel de administración,
    // "el cliente que ibas a borrar ya no estaba" es información útil: probablemente
    // signifique que otro operador se te adelantó, o que la lista que estás viendo está
    // desactualizada.
    if affected == 0 {
        return Err(ApiError::not_found(&id));
    }

    Ok(NoContent)
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
        .mount(
            "/",
            routes![
                health,
                list_customers,
                get_customer,
                create_customer,
                update_customer,
                delete_customer
            ],
        )
        // 🇪🇸 NOTA (`.register()` y no `.mount()`): las rutas se MONTAN, los catchers se
        // REGISTRAN. Son dos tablas distintas dentro de Rocket, y el primer argumento
        // significa cosas distintas en cada una: en `mount` es el prefijo del path; en
        // `register` es el ámbito en el que ese catcher aplica. Con `"/"` cubrimos toda la
        // aplicación; si mañana hubiera un `/api` con otro formato de error, se le podría
        // registrar el suyo y ganaría por ser más específico.
        .register(
            "/",
            catchers![
                catch_bad_request,
                catch_unprocessable,
                catch_not_found,
                catch_internal,
                catch_default
            ],
        )
}
