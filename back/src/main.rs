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

// 🇪🇸 NOTA (`#[allow(dead_code)]` con fecha de caducidad): `models.rs` define
// `NewCustomer`, `UpdateCustomer`, `Paginated` y `Customer::from_row`, y todavía nadie
// los usa — el CRUD llega en el siguiente paso. Sin este atributo, el build escupe cinco
// warnings de código muerto que ahogarían cualquier warning REAL que aparezca mientras
// trabajamos.
//
// Poner el `#[allow]` aquí, sobre la declaración del módulo, y no dentro de `models.rs`,
// es intencionado: así el "silenciador" queda a la vista en el archivo que se lee cada
// día, y quitarlo es un borrado de una línea. Un `#![allow(dead_code)]` enterrado en la
// cabecera de `models.rs` se queda ahí para siempre y acaba tapando errores de verdad.
//
// ⚠️ Esta línea debe desaparecer en cuanto existan las rutas CRUD.
#[allow(dead_code)]
mod models;

use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::serde::json::{json, Json, Value};
use rocket::State;

use db::Db;

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
        .mount("/", routes![health])
}
