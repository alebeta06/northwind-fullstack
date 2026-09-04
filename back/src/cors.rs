//! CORS support, implemented as a response fairing plus a preflight route.
//!
//! 🇪🇸 NOTA (por qué un fairing propio y NO el crate `rocket_cors`):
//!
//! `rocket_cors` existe y funciona, pero para lo que necesita este proyecto —un origen,
//! cuatro cabeceras fijas— trae un coste que no se recupera:
//!
//!   1. Una dependencia más que hay que mantener sincronizada con la versión de Rocket.
//!      `rocket_cors` ha ido por detrás de Rocket en cada release importante, y quedarse
//!      esperando a que un crate de terceros soporte la 0.5 por cuatro cabeceras es un
//!      mal negocio.
//!   2. Configurar `rocket_cors` (con sus `AllowedOrigins`, `AllowedHeaders` y un
//!      `to_cors()?` que puede fallar) ocupa parecido a lo que ocupa esto, pero sin que se
//!      vea lo que hace por debajo.
//!
//! Escribirlo a mano son ~40 líneas de código real y deja a la vista el mecanismo: dónde
//! se enganchan las cabeceras en el ciclo de vida y por qué el preflight necesita una ruta
//! propia. Para un módulo de curso, ese es justamente el material.

use std::env;

use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::response::status::NoContent;
use rocket::{Request, Response};

// ═══════════════════════════════════════════════════════════════════
//  Configuración
// ═══════════════════════════════════════════════════════════════════

/// Environment variable holding the browser origin allowed to call this API.
const ORIGIN_ENV: &str = "CORS_ALLOWED_ORIGIN";

/// Origin used when the variable is not set: the Next.js dev server.
const DEFAULT_ORIGIN: &str = "http://localhost:3000";

/// 🇪🇸 NOTA: los métodos y las cabeceras son literales porque los decide el SERVIDOR, no el
/// despliegue: son exactamente los verbos que la API implementa y la única cabecera que el
/// frontend necesita mandar. El origen, en cambio, cambia entre desarrollo y producción
/// sin que cambie ni una línea de esta API — y por eso ese sí sale del entorno.
const ALLOWED_METHODS: &str = "GET, POST, PUT, DELETE, OPTIONS";
const ALLOWED_HEADERS: &str = "Content-Type";

/// 24 h — how long the browser may cache the preflight result.
///
/// 🇪🇸 NOTA: sin esto, el navegador repite el OPTIONS antes de CADA petición con JSON:
/// dos viajes de red por cada guardado. Con el `Max-Age`, pregunta una vez al día. Es el
/// máximo que respetan Chrome (que además lo recorta a 2 h por su cuenta) y Firefox.
const MAX_AGE: &str = "86400";

// ═══════════════════════════════════════════════════════════════════
//  El fairing
// ═══════════════════════════════════════════════════════════════════

/// Adds the `Access-Control-*` headers to every outgoing response.
pub struct Cors {
    /// 🇪🇸 NOTA (por qué se guarda el origen y no se lee en cada respuesta): `env::var`
    /// recorre el entorno del proceso y devuelve un `String` NUEVO en cada llamada. Hacerlo
    /// en `on_response` sería una búsqueda y una asignación de memoria por cada petición
    /// servida, para obtener siempre el mismo valor.
    ///
    /// Pero el motivo de peso no es el rendimiento, es CUÁNDO se descubre un fallo de
    /// configuración. Resolviendo la variable al construir el fairing, el valor queda
    /// fijado en el arranque: todas las respuestas de ese proceso dicen lo mismo. Si se
    /// leyera por respuesta, un cambio del entorno a mitad de vida del proceso produciría
    /// respuestas incoherentes entre sí, y ese es un bug para volverse loco.
    origin: String,
}

impl Cors {
    /// Reads the allowed origin from the environment, once.
    ///
    /// 🇪🇸 NOTA (por qué el origen NO se escribe en el código): en desarrollo el frontend
    /// vive en `http://localhost:3000`; en producción, en una URL de Vercel. Si el valor
    /// estuviera incrustado en el binario, desplegar exigiría recompilar — y recompilar
    /// para cambiar una cadena de texto significa que la configuración se ha colado dentro
    /// del artefacto. El binario debe ser el mismo en las dos máquinas; lo que cambia es
    /// el entorno.
    ///
    /// El default hace que `cargo run` funcione sin exportar nada, que es lo que se quiere
    /// para desarrollar. En producción se define la variable.
    pub fn from_env() -> Self {
        let origin = env::var(ORIGIN_ENV).unwrap_or_else(|_| DEFAULT_ORIGIN.to_string());
        Cors { origin }
    }
}

/// 🇪🇸 NOTA (`#[rocket::async_trait]`): los métodos de `Fairing` son `async`, y hasta hace
/// poco Rust no permitía funciones async en traits. La macro reescribe cada método para que
/// devuelva un `Pin<Box<dyn Future>>`, que es lo que el lenguaje sí sabía expresar. Hay que
/// ponerla tanto en la definición del trait (eso lo hace Rocket) como en CADA
/// implementación — si se olvida, el error de compilación habla de tipos que no encajan y
/// no menciona la macro por ningún lado.
#[rocket::async_trait]
impl Fairing for Cors {
    /// 🇪🇸 NOTA (`Kind::Response`): declara EN QUÉ momentos del ciclo de vida quiere
    /// engancharse este fairing, y Rocket solo lo llama en esos. Aquí basta con
    /// `Response`: no hace falta `Kind::Request`, porque las cabeceras se ponen a la
    /// salida y no hay que inspeccionar nada a la entrada. Pedir menos ganchos de los
    /// necesarios rompe el fairing; pedir de más lo hace correr sin motivo.
    ///
    /// El `name` es lo que Rocket imprime en el log de arranque y en los errores. Vale la
    /// pena que diga algo: es lo que se ve cuando algo va mal.
    fn info(&self) -> Info {
        Info {
            name: "CORS headers",
            kind: Kind::Response,
        }
    }

    /// ═══════════════════════════════════════════════════════════════════
    /// 🇪🇸 NOTA — POR QUÉ UN FAIRING Y NO CABECERAS RUTA A RUTA
    /// ═══════════════════════════════════════════════════════════════════
    ///
    /// Se podría devolver un responder con cabeceras desde cada handler. Sería peor por
    /// dos motivos, y el segundo es el que de verdad decide:
    ///
    /// 1. Habría que acordarse en las seis rutas, y en la séptima que se añada dentro de
    ///    seis meses. Un olvido no rompe nada en `curl` y rompe UNA pantalla del frontend.
    ///
    /// 2. **`on_response` también se ejecuta sobre las respuestas de los CATCHERS**, y eso
    ///    no se puede conseguir ruta a ruta, porque un catcher salta precisamente cuando
    ///    NO se ha ejecutado ninguna ruta.
    ///
    /// El punto 2 merece detalle, porque es un fallo que cuesta horas: si el 404 en JSON
    /// saliera SIN cabeceras CORS, el navegador bloquearía la respuesta al leerla. El
    /// `fetch` no rechazaría con el 404 y su cuerpo — rechazaría con un `TypeError:
    /// Failed to fetch`, un error de red genérico, SIN status y SIN cuerpo. El frontend no
    /// podría distinguir "el cliente no existe" de "el servidor está caído", y el
    /// `{"error":"not_found","message":"..."}` que tanto cuidamos sería invisible. Es
    /// decir: justo cuando el usuario necesita un mensaje útil, es cuando no lo habría.
    ///
    /// 🇪🇸 NOTA (sobre `Shield`, el otro fairing que ya está puesto): Rocket 0.5 adjunta
    /// por defecto un fairing llamado `Shield`, que añade cabeceras de seguridad —son las
    /// `x-content-type-options: nosniff`, `x-frame-options: SAMEORIGIN` y
    /// `permissions-policy` que aparecen en cualquier respuesta de esta API. `Shield` NO
    /// toca ninguna cabecera `Access-Control-*`, así que los dos fairings escriben en
    /// conjuntos disjuntos y no hay conflicto ni orden de adjuntado que importe. Conviene
    /// saberlo antes de perder tiempo buscando quién pisa a quién.
    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        // 🇪🇸 NOTA (`set_header` y no `adjoin_header`): `set` REEMPLAZA cualquier valor
        // previo con ese nombre; `adjoin` añadiría uno más. Para CORS hay que reemplazar:
        // dos cabeceras `Access-Control-Allow-Origin` en la misma respuesta hacen que el
        // navegador la rechace entera, porque la especificación exige exactamente una.
        //
        // ⚠️ Se ponen en TODAS las respuestas, también en las de `curl`, que no manda
        // `Origin`. Es inofensivo: un cliente que no es un navegador ignora estas
        // cabeceras, y no hay nada que decidir en función de quién pregunta.
        response.set_header(Header::new(
            "Access-Control-Allow-Origin",
            self.origin.clone(),
        ));
        response.set_header(Header::new("Access-Control-Allow-Methods", ALLOWED_METHODS));
        response.set_header(Header::new("Access-Control-Allow-Headers", ALLOWED_HEADERS));
        response.set_header(Header::new("Access-Control-Max-Age", MAX_AGE));
    }
}

// ═══════════════════════════════════════════════════════════════════
//  El preflight
// ═══════════════════════════════════════════════════════════════════

/// Catch-all route answering the browser's preflight `OPTIONS` request.
///
/// ═══════════════════════════════════════════════════════════════════
/// 🇪🇸 NOTA — POR QUÉ HACE FALTA ESTA RUTA (Y POR QUÉ SU AUSENCIA VUELVE LOCO A CUALQUIERA)
/// ═══════════════════════════════════════════════════════════════════
///
/// Antes de mandar un PUT, un DELETE o cualquier POST con `Content-Type: application/json`,
/// el navegador NO envía esa petición. Envía primero otra, distinta, que el código del
/// frontend nunca escribió:
///
///     OPTIONS /customers/ALFKI
///     Origin: http://localhost:3000
///     Access-Control-Request-Method: PUT
///     Access-Control-Request-Headers: content-type
///
/// Es el "preflight": una pregunta previa —¿me dejas hacer esto?— que el navegador hace por
/// su cuenta para no ejecutar una operación con efectos secundarios contra un servidor que
/// quizá no la esperaba. Solo si la respuesta trae las cabeceras adecuadas manda la
/// petición real.
///
/// ⚠️ Sin esta ruta, ese OPTIONS no casa con ninguna de las montadas, cae en el catcher del
/// 404, y el navegador aborta ahí. **El PUT nunca llega a salir del navegador.**
///
/// Y aquí está lo que hace que este bug sea el más desconcertante del proyecto: `curl`
/// **no hace preflight**. No es un navegador, no aplica la política del mismo origen, manda
/// el PUT directamente y recibe su 200. Así que se da esta situación:
///
///     · `curl -X PUT …`     → 200 OK, perfecto, el backend funciona.
///     · El frontend         → falla siempre, con un error de CORS en la consola que
///                             habla del PUT, no del OPTIONS que realmente falló.
///
/// Se pierden horas depurando el PUT, que está bien, en vez de la petición que de verdad se
/// rompió y que ni siquiera aparece en el código. Por eso se prueba con `-X OPTIONS`
/// explícitamente: es la única forma de ver desde `curl` lo que ve el navegador.
///
/// 🇪🇸 NOTA (`/<_..>`): captura CUALQUIER número de segmentos. El `_` dice que el valor no
/// se usa —sin él, Rust avisaría de una variable sin utilizar—. Con una sola ruta quedan
/// cubiertos `/customers`, `/customers/ALFKI` y todo lo que se añada después, que es lo
/// correcto: el preflight no depende del recurso concreto, y una ruta OPTIONS por endpoint
/// sería una lista que tarde o temprano se queda corta.
///
/// El 204 va sin cuerpo porque no hay nada que decir: toda la respuesta al preflight son
/// cabeceras, y las pone el fairing al pasar por `on_response`. Esta función solo existe
/// para que haya una ruta que case y, por tanto, una respuesta que el fairing pueda tocar.
#[options("/<_..>")]
pub fn preflight() -> NoContent {
    NoContent
}
