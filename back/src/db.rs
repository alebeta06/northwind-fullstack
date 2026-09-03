//! Database access layer.
//!
//! Owns the SQLite connection and exposes it to Rocket as managed state.

use rusqlite::{Connection, OpenFlags};
use std::sync::Mutex;

// ═══════════════════════════════════════════════════════════════════
//  El newtype Db
// ═══════════════════════════════════════════════════════════════════

/// Thread-safe wrapper around the SQLite connection.
///
/// 🇪🇸 NOTA (por qué un "newtype" y no `Mutex<Connection>` a secas):
///
/// Rocket guarda el estado compartido en un mapa indexado POR TIPO. Si
/// mañana quisieras gestionar dos cosas del mismo tipo (por ejemplo, una
/// conexión de lectura y otra de escritura), `Mutex<Connection>` sería
/// ambiguo y Rocket entregaría cualquiera de las dos.
///
/// Envolver en un tipo propio le da nombre e identidad. Es el patrón
/// "newtype": una tupla de un solo campo que no añade coste en tiempo de
/// ejecución (el compilador la elimina) pero sí seguridad de tipos.
pub struct Db(pub Mutex<Connection>);

// ═══════════════════════════════════════════════════════════════════
//  Por qué el Mutex es OBLIGATORIO (no una recomendación)
// ═══════════════════════════════════════════════════════════════════
//
// 🇪🇸 NOTA: esto es lo que el curso no explica y el enunciado sí exige
// ("thread-safe database access with Mutex" — 20% de la nota).
//
// Rocket es multihilo: atiende varias peticiones a la vez, cada una en
// un hilo distinto del pool de Tokio. Para que un valor pueda ser
// COMPARTIDO entre hilos, Rust exige en TIEMPO DE COMPILACIÓN que ese
// tipo implemente el trait `Sync`.
//
//   - `Send`  → el valor se puede MOVER a otro hilo.
//   - `Sync`  → se puede acceder por referencia (`&T`) desde varios
//               hilos a la vez. Formalmente: `T: Sync` si y solo si
//               `&T: Send`.
//
// `rusqlite::Connection` implementa `Send` pero NO `Sync`, porque por
// dentro guarda un puntero a la estructura de C de SQLite y varias
// llamadas simultáneas sobre el mismo handle corromperían el estado.
//
// Resultado: si intentas `.manage(connection)` directamente, el código
// NO COMPILA. El error dice algo como:
//     `*mut sqlite3` cannot be shared between threads safely
//     the trait `Sync` is not implemented for `Connection`
//
// `Mutex<T>` implementa `Sync` siempre que `T: Send`. Al envolver la
// conexión, el Mutex garantiza que solo un hilo la toca a la vez, y eso
// es exactamente lo que le faltaba para ser compartible.
//
// Es decir: el Mutex no es una precaución que tomas por prudencia. Es
// el requisito que el compilador te impone para que el programa exista.
//
// ── El coste, que conviene documentar en el README ──
//
// Un único Mutex SERIALIZA todos los accesos a la base. Con 50
// peticiones concurrentes, 49 esperan en cola. Para 93 filas y un
// proyecto de curso es irrelevante; en producción se usa un pool de
// conexiones (`r2d2_sqlite`), que mantiene N conexiones abiertas y
// reparte.
//
// ── Un segundo matiz, más sutil ──
//
// Usamos `std::sync::Mutex`, no `tokio::sync::Mutex`. La regla general
// es: `std` para secciones críticas cortas que no cruzan un `.await`;
// `tokio` cuando sí lo hacen. Aquí `rusqlite` es una API bloqueante
// (no async), así que nunca hay un `.await` dentro del lock y `std` es
// la elección correcta.
//
// Lo que sí ocurre es que una consulta lenta bloquea un hilo del
// ejecutor async. La solución canónica sería envolver las consultas en
// `rocket::tokio::task::spawn_blocking`. No lo hacemos porque añade
// complejidad sin beneficio medible a esta escala — pero es una
// decisión consciente, no un descuido, y como tal va documentada.

// ═══════════════════════════════════════════════════════════════════
//  Apertura de la conexión
// ═══════════════════════════════════════════════════════════════════

/// Opens the Northwind database and applies runtime configuration.
///
/// 🇪🇸 NOTA: devuelve `rusqlite::Result<Db>`, no `Db` a secas. Abrir un
/// archivo puede fallar (no existe, permisos, archivo corrupto) y en
/// Rust eso se modela con `Result`, nunca con excepciones. Quien llama
/// decide qué hacer con el error.
pub fn open(path: &str) -> rusqlite::Result<Db> {
    // 🇪🇸 NOTA (por qué NO usamos `Connection::open`, que sería lo obvio):
    //
    // `Connection::open(path)` abre con las flags por defecto de SQLite, y entre ellas
    // está `SQLITE_OPEN_CREATE`. Traducido: si el archivo no existe, NO da error — crea
    // una base de datos nueva y vacía, y devuelve `Ok`.
    //
    // Eso convierte el fallo más probable de este proyecto (lanzar el binario desde un
    // directorio equivocado, o no haber descargado la BD) en un fallo SILENCIOSO: el
    // servidor arranca, `/customers` devuelve "no such table: Customers", y te pasas
    // media hora buscando el error en el SQL cuando el problema era la ruta.
    //
    // Comprobado: ejecutando `./target/debug/back` desde `/tmp`, `open` creaba un
    // `/tmp/northwind.db` de 0 bytes y Rocket levantaba el puerto sin una queja.
    //
    // Quitando `SQLITE_OPEN_CREATE` de las flags, abrir un archivo inexistente devuelve
    // `Err(SqliteFailure(... 14, "unable to open database file"))`, que es lo que
    // `main.rs` necesita para abortar el arranque con un mensaje útil.
    //
    // Las otras tres flags son las mismas que pone `open()` por defecto:
    //   - READ_WRITE : necesitamos escribir (POST/PUT/DELETE).
    //   - NO_MUTEX   : desactiva el mutex INTERNO de SQLite. Es correcto porque la
    //                  exclusión ya la garantiza nuestro `Mutex<Connection>` de Rust;
    //                  duplicarla solo añadiría coste.
    //   - URI        : permite rutas con forma de URI (`file:...?mode=ro`). Inofensivo,
    //                  y lo mantenemos para no desviarnos del comportamiento estándar.
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(path, flags)?;

    // 🇪🇸 NOTA (LA LÍNEA MÁS IMPORTANTE DE ESTE ARCHIVO):
    //
    // SQLite trae las claves foráneas DESACTIVADAS por defecto, por
    // compatibilidad histórica. Y hay que activarlas EN CADA CONEXIÓN:
    // no es una propiedad del archivo, es de la sesión.
    //
    // Sin esta línea, `DELETE FROM Customers WHERE CustomerID = 'ALFKI'`
    // borra el cliente y deja sus pedidos apuntando a un cliente que ya
    // no existe. Sin error. Sin aviso. Datos corruptos en silencio.
    //
    // Con esta línea, ese DELETE devuelve un error de constraint que
    // podemos traducir a un 409 Conflict con un mensaje útil.
    //
    // Dato del dataset: los 93 clientes de Northwind tienen pedidos, así
    // que TODO borrado de un cliente preexistente dará 409. Eso es
    // correcto, no un bug. Un cliente creado por el POST sí se puede
    // borrar, porque nace sin pedidos.
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(Db(Mutex::new(conn)))
}

// ═══════════════════════════════════════════════════════════════════
//  Cómo se usa desde una ruta
// ═══════════════════════════════════════════════════════════════════
//
// 🇪🇸 NOTA: en `main.rs` se registra con `.manage(db)`, y en cada ruta
// se pide como parámetro:
//
//     #[get("/customers/<id>")]
//     fn get_customer(db: &State<Db>, id: &str) -> ... {
//         let conn = db.0.lock().unwrap();
//         // ...usar conn...
//     }                          // ← el lock se libera AQUÍ, solo
//                                //   porque `conn` sale de ámbito
//
// Tres cosas que merecen atención:
//
// 1. `&State<Db>` es un "request guard": Rocket ve ese tipo en la firma
//    y le inyecta el estado automáticamente. No hay variables globales
//    ni singletons.
//
// 2. `.lock()` devuelve un `Result` porque puede fallar si otro hilo
//    entró en pánico mientras tenía el lock (el Mutex queda
//    "envenenado"). `.unwrap()` propaga ese pánico, que es razonable:
//    si la base quedó en estado inconsistente, seguir es peor.
//
// 3. El lock se libera SOLO al salir de ámbito, por el Drop de la
//    guarda. No hay `unlock()`. Esto es RAII, y es la razón por la que
//    en Rust es difícil olvidarse de liberar un mutex — pero también
//    significa que si mantienes `conn` viva más de lo necesario,
//    bloqueas a todos los demás sin darte cuenta.
