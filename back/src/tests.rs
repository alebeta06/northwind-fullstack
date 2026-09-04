//! Smoke tests for the read-only endpoints.
//!
//! 🇪🇸 NOTA (ALCANCE, dicho por delante): esto es cobertura de HUMO, no una suite. Cada
//! test comprueba que un endpoint responde y que lo que devuelve tiene la forma pactada;
//! ninguno explora casos límite ni combinaciones. Lo que compran estos once tests es
//! detectar en segundos que algo se rompió del todo — una ruta desmontada, un catcher mal
//! registrado, el fairing de CORS caído— sin tener que arrancar el servidor y repasar los
//! `curl` a mano.
//!
//! ⚠️ NINGÚN test escribe en la base. Son todos GET contra `northwind.db` REAL, así que se
//! pueden ejecutar mil veces seguidas sin dejar rastro. Probar POST/PUT/DELETE exige otra
//! conversación: o se acepta que los tests ensucien el archivo (y hay que limpiar, y un
//! test que falle a medias deja basura), o se monta una copia temporal de la base por
//! test, que obliga a que la ruta del archivo sea configurable — es decir, a tocar el
//! arranque. Hoy no toca.
//!
//! 🇪🇸 NOTA (POR QUÉ ESTOS TESTS VIVEN EN `src/` Y NO EN `tests/`): la convención de Rust
//! es que los tests de integración van en un directorio `tests/` en la raíz del paquete.
//! Aquí NO se puede, y el motivo es de fondo: `back` es un crate BINARIO. Un `tests/foo.rs`
//! es un crate aparte que hace `use back::…`, y un binario no se puede enlazar como
//! biblioteca — no hay nada que importar.
//!
//! Las dos salidas son: (a) partir el proyecto en `src/lib.rs` + un `main.rs` fino que lo
//! use, que es lo correcto en un proyecto grande y es una reestructuración; o (b) declarar
//! el módulo de tests DENTRO del crate, que es esto. La (b) cuesta una línea en `main.rs`,
//! da acceso directo a `super::rocket()` sin exponer nada en una API pública, y no toca el
//! arranque. Para once tests de humo, la (b) gana.

use std::path::Path;

use rocket::http::{Header, Status};
use rocket::local::blocking::Client;
use serde_json::Value;

/// Builds a test client against the real Rocket instance.
///
/// 🇪🇸 NOTA (`super::rocket()` con `#[launch]`): la macro genera el `fn main()`, pero
/// CONSERVA la función tal como está escrita. Por eso se puede llamar desde aquí y obtener
/// la aplicación ya montada —con sus rutas, sus catchers y sus dos fairings— sin abrir un
/// puerto ni lanzar un proceso. Es la razón por la que estos tests no necesitan que nadie
/// arranque nada a mano: `Client` habla con la instancia por dentro.
///
/// Y es también lo que hace que sirvan de algo: al construir la aplicación DE VERDAD,
/// cualquier fallo de cableado (una ruta sin montar, un fairing que aborta la ignición) se
/// manifiesta aquí igual que se manifestaría en producción.
fn client() -> Client {
    // 🇪🇸 NOTA (por qué se comprueba el archivo ANTES de construir el cliente): si la base
    // no está, `Client::tracked` falla porque el fairing `try_on_ignite` aborta el
    // arranque, y el test muere con un `expect` sobre un error de ignición — un mensaje que
    // habla de fairings y no dice ni que falta un archivo ni cuál. Comprobarlo aquí cuesta
    // tres líneas y convierte un panic críptico en una instrucción.
    //
    // La ruta es relativa y se resuelve contra el directorio de trabajo, que en `cargo
    // test` es la raíz del paquete (`back/`) — la misma que en `cargo run`. Por eso se
    // imprime el cwd: si algún día no coinciden, el mensaje lo dice en vez de dejarlo
    // adivinar.
    let db_path = Path::new(super::DB_PATH);
    if !db_path.exists() {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());

        panic!(
            "\n\nthe Northwind database is missing, so these tests cannot run.\n\
             \x20 expected file    : {}\n\
             \x20 working directory: {}\n\
             \x20 resolved to      : {}/{}\n\n\
             It is git-ignored on purpose; download it as described in the README and \
             place it in the `back/` directory.\n",
            super::DB_PATH,
            cwd,
            cwd,
            super::DB_PATH
        );
    }

    // 🇪🇸 NOTA: cada test llama a esto y se queda con SU propia instancia y SU propia
    // conexión. Sale más caro que compartir una, y a cambio los tests son independientes:
    // ninguno puede dejar al siguiente un estado raro, y `cargo test` puede correrlos en
    // paralelo (que es lo que hace por defecto) sin que se estorben. Con once tests de
    // solo lectura, ese coste no se nota.
    Client::tracked(super::rocket()).expect("the Rocket instance should be valid")
}

// 🇪🇸 NOTA (por qué cada test hace `let client = client();` en vez de encadenar
// `client().get(...)`): la respuesta que devuelve `dispatch()` es un `LocalResponse` que
// PRESTA el cliente — lee el cuerpo de él. Encadenando, el `Client` sería un temporal que
// muere al final de esa sentencia y dejaría la respuesta apuntando a algo liberado; el
// compilador lo rechaza con un E0716. Es el borrow checker haciendo su trabajo: en otro
// lenguaje esto sería un use-after-free silencioso en un test que "pasa".

/// Parses a response body as JSON, failing with the body itself if it is not.
///
/// 🇪🇸 NOTA: si la respuesta no fuera JSON —el fallo que precisamente arreglaron los
/// catchers—, el mensaje de error enseña el cuerpo recibido. Un `unwrap()` pelado diría
/// "expected value at line 1 column 1", que es la forma más larga de no decir "te ha
/// llegado un HTML".
fn json(body: Option<String>) -> Value {
    let body = body.expect("the response should have a body");
    serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("the response body is not JSON ({e}):\n{body}"))
}

// ═══════════════════════════════════════════════════════════════════
//  1-2. Cableado y listado por defecto
// ═══════════════════════════════════════════════════════════════════

#[test]
fn health_reports_the_93_customers() {
    let client = client();
    let response = client.get("/health").dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = json(response.into_string());
    // 🇪🇸 NOTA: este número no es un capricho — es el contenido conocido del dataset de
    // Northwind. Si algún día un test escribiera en la base y no limpiara, ESTE test sería
    // el primero en cantarlo.
    assert_eq!(body["customers"], 93, "the Northwind dataset has 93 customers");
    assert_eq!(body["status"], "ok");
}

#[test]
fn list_defaults_to_ten_per_page() {
    let client = client();
    let response = client.get("/customers").dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = json(response.into_string());
    assert_eq!(body["total"], 93);
    assert_eq!(body["page"], 1);
    assert_eq!(body["pageSize"], 10);
    assert_eq!(
        body["data"].as_array().expect("data should be an array").len(),
        10,
        "the default page size is 10"
    );
}

// ═══════════════════════════════════════════════════════════════════
//  3-6. Los parámetros de la query
// ═══════════════════════════════════════════════════════════════════

#[test]
fn page_size_is_honoured() {
    let client = client();
    let response = client.get("/customers?pageSize=2").dispatch();

    assert_eq!(response.status(), Status::Ok);
    let body = json(response.into_string());
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["pageSize"], 2);
}

#[test]
fn company_name_filters_case_insensitively() {
    // 🇪🇸 NOTA: se busca en minúsculas contra "Alfreds Futterkiste", que está en la base
    // con mayúscula inicial. Que encuentre justo uno demuestra las dos cosas a la vez: que
    // el LIKE no distingue mayúsculas y que el filtro llega hasta el COUNT (si el `total`
    // ignorara el WHERE, aquí saldría 93).
    let client = client();
    let response = client.get("/customers?companyName=alfreds").dispatch();

    assert_eq!(response.status(), Status::Ok);
    let body = json(response.into_string());
    assert_eq!(body["total"], 1);
    assert_eq!(body["data"][0]["customerId"], "ALFKI");
}

#[test]
fn unknown_sort_by_falls_back_to_the_default() {
    // 🇪🇸 NOTA: el valor no está en la whitelist. Lo que se comprueba NO es el orden, sino
    // que la petición responde 200 en lugar de romperse: la degradación al orden por
    // defecto es una decisión de diseño del GET, y este test la fija para que nadie la
    // cambie por un 400 sin darse cuenta.
    let client = client();
    let response = client.get("/customers?sortBy=NoExiste").dispatch();

    assert_eq!(response.status(), Status::Ok);
    let body = json(response.into_string());
    assert_eq!(body["total"], 93);
    assert_eq!(body["data"].as_array().unwrap().len(), 10);
}

#[test]
fn page_size_is_capped() {
    // 🇪🇸 NOTA: el test del tope no es cosmético — es el que impide que alguien "arregle"
    // el clamp para complacer a un frontend que quiere pedirlo todo de una vez, y reabra
    // el DoS de amplificación por el camino.
    let client = client();
    let response = client.get("/customers?pageSize=999999").dispatch();

    assert_eq!(response.status(), Status::Ok);
    let body = json(response.into_string());
    assert_eq!(body["pageSize"], 100, "pageSize must be capped at 100");
    assert_eq!(body["data"].as_array().unwrap().len(), 93);
}

// ═══════════════════════════════════════════════════════════════════
//  7-9. Un cliente concreto
// ═══════════════════════════════════════════════════════════════════

#[test]
fn get_by_id_returns_the_customer() {
    let client = client();
    let response = client.get("/customers/ALFKI").dispatch();

    assert_eq!(response.status(), Status::Ok);
    let body = json(response.into_string());
    assert_eq!(body["customerId"], "ALFKI");
    assert_eq!(body["companyName"], "Alfreds Futterkiste");
}

#[test]
fn get_by_id_normalises_the_case() {
    let client = client();

    let lowercase = json(client.get("/customers/alfki").dispatch().into_string());
    let uppercase = json(client.get("/customers/ALFKI").dispatch().into_string());

    // 🇪🇸 NOTA: se comparan los dos cuerpos ENTEROS, no solo el id. Comparar solo el id
    // dejaría pasar el bug que de verdad importaría —devolver otro cliente— siempre que
    // ese otro cliente trajera el id pedido.
    assert_eq!(lowercase, uppercase, "/customers/alfki must be the same resource as /customers/ALFKI");
    assert_eq!(lowercase["customerId"], "ALFKI");
}

#[test]
fn unknown_id_returns_a_json_404() {
    let client = client();
    let response = client.get("/customers/NOEXI").dispatch();

    assert_eq!(response.status(), Status::NotFound);

    let body = json(response.into_string());
    assert_eq!(body["error"], "not_found");
    // 🇪🇸 NOTA: se comprueba que `message` existe y no está vacío, pero NO su texto exacto.
    // Un test que fija la redacción se rompe cada vez que alguien mejora un mensaje, y esa
    // clase de fallo enseña a ignorar los tests. El contrato con el frontend es el `error`,
    // que es el campo estable; el `message` es para humanos.
    assert!(
        body["message"].as_str().is_some_and(|m| !m.is_empty()),
        "the error body must carry a human-readable message"
    );
}

// ═══════════════════════════════════════════════════════════════════
//  10-11. Lo que rodea a las rutas: catchers y CORS
// ═══════════════════════════════════════════════════════════════════

#[test]
fn unknown_route_is_caught_as_json() {
    // 🇪🇸 NOTA: este test vigila el catcher, no una ruta. Antes de que existieran, esto
    // devolvía la página HTML de Rocket, y el frontend recibía un `<!DOCTYPE html>` donde
    // esperaba un objeto. Es exactamente la clase de regresión que nadie nota probando a
    // mano, porque el status (404) sigue siendo el correcto: lo único que cambia es el
    // cuerpo.
    let client = client();
    let response = client.get("/rutaquenoexiste").dispatch();

    assert_eq!(response.status(), Status::NotFound);

    let content_type = response
        .headers()
        .get_one("content-type")
        .expect("the response must declare a content type")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "catcher responses must be JSON, got: {content_type}"
    );

    let body = json(response.into_string());
    assert_eq!(body["error"], "not_found");
}

#[test]
fn responses_carry_the_cors_header() {
    let client = client();
    let response = client
        .get("/customers")
        .header(Header::new("Origin", "http://localhost:3000"))
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    // 🇪🇸 NOTA (por qué se comprueba que la cabecera EXISTE y no su valor): el origen sale
    // de `CORS_ALLOWED_ORIGIN`, así que su valor depende del entorno en el que se ejecuten
    // los tests. Fijar "http://localhost:3000" haría que la suite fallara en cualquier
    // máquina que tenga la variable definida — un test que falla por el entorno y no por el
    // código es peor que no tenerlo. Lo que este test protege es que el fairing SIGUE
    // ADJUNTADO; el valor concreto ya lo decide la configuración.
    let origin = response
        .headers()
        .get_one("access-control-allow-origin")
        .expect("the CORS fairing must set access-control-allow-origin");

    assert!(!origin.is_empty(), "the allowed origin must not be empty");
}
