//! Data models for the Customers domain.
//!
//! 🇪🇸 NOTA: `//!` es un comentario de documentación "interior": documenta
//! el módulo que lo contiene. `///` documenta el elemento que va DEBAJO.
//! Ambos los recoge `cargo doc` para generar documentación HTML.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════
//  Customer — el registro completo, tal como sale de la base de datos
// ═══════════════════════════════════════════════════════════════════

/// A customer record from the Northwind `Customers` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    // 🇪🇸 NOTA: la columna en SQLite se llama `CustomerID` (PascalCase),
    // el campo en Rust se llama `customer_id` (snake_case, la convención
    // del lenguaje), y en el JSON sale como `customerId` (camelCase, la
    // convención de TypeScript). Tres nombres para lo mismo:
    //   - el de SQL lo controlas tú al escribir la consulta
    //   - el de Rust lo exige el linter del lenguaje
    //   - el de JSON lo produce `rename_all = "camelCase"` de arriba
    pub customer_id: String,

    // 🇪🇸 NOTA: `String` sin Option porque en las 93 filas actuales no hay
    // ningún NULL aquí, y porque es el campo por el que se busca y ordena.
    // ⚠️ Ojo: el esquema NO declara NOT NULL. Esto es una garantía que
    // impones tú validando en el POST/PUT, no algo que la base te asegure.
    pub company_name: String,

    // ─── Campos que SÍ pueden ser NULL ───
    //
    // 🇪🇸 NOTA: aquí está el bug número uno de este proyecto. Si declaras
    // `fax: String` en vez de `Option<String>`, rusqlite falla en tiempo
    // de EJECUCIÓN al leer la primera fila con NULL — y hay 24 clientes
    // sin fax. El error es `InvalidColumnType`, y desconcierta porque el
    // código compila perfectamente.
    //
    // `Option<T>` es el enum que viste en la lección 240:
    //     enum Option<T> { Some(T), None }
    // Es la forma que tiene Rust de representar "puede no haber valor"
    // sin punteros nulos. El compilador te obliga a manejar el caso None.
    //
    // En el JSON, `None` se serializa como `null` y `Some("x")` como "x".
    pub contact_name: Option<String>,
    pub contact_title: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
}

impl Customer {
    /// Builds a `Customer` from a database row.
    ///
    /// 🇪🇸 NOTA: `impl Customer { ... }` es el bloque de implementación
    /// que viste en la lección 365. Aquí van los métodos asociados al tipo.
    ///
    /// Esta función NO recibe `&self` — no opera sobre una instancia
    /// existente, sino que crea una nueva. Eso la convierte en una
    /// "función asociada" (equivalente a un método estático). Se llama
    /// con `Customer::from_row(row)`, no con `algo.from_row()`.
    ///
    /// El tipo de retorno `rusqlite::Result<Self>` es azúcar para
    /// `Result<Customer, rusqlite::Error>`. `Self` es un alias del tipo
    /// que estamos implementando.
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Customer {
            // 🇪🇸 NOTA: `row.get(0)?` intenta leer la columna 0 y
            // convertirla al tipo del campo destino. El `?` al final es
            // el operador de propagación de errores que viste en la
            // lección 250: si el Result es Err, sale de la función
            // devolviendo ese error; si es Ok, extrae el valor.
            //
            // Sin `?` tendrías que escribir:
            //     let id = match row.get(0) {
            //         Ok(v) => v,
            //         Err(e) => return Err(e),
            //     };
            //
            // ⚠️ El índice es POSICIONAL: 0 es la primera columna del
            // SELECT, no de la tabla. Si cambias el orden de las columnas
            // en la consulta y no cambias estos números, los datos se
            // mezclan silenciosamente (el city acaba en country, etc.)
            // y no hay error. Por eso conviene mantener el SELECT y este
            // bloque juntos y en el mismo orden.
            customer_id: row.get(0)?,
            company_name: row.get(1)?,
            contact_name: row.get(2)?,
            contact_title: row.get(3)?,
            address: row.get(4)?,
            city: row.get(5)?,
            region: row.get(6)?,
            postal_code: row.get(7)?,
            country: row.get(8)?,
            phone: row.get(9)?,
            fax: row.get(10)?,
        })
    }

    /// Column list in the exact order expected by `from_row`.
    ///
    /// 🇪🇸 NOTA: tener la lista de columnas en UN solo sitio evita que el
    /// SELECT y `from_row` se desincronicen. Cualquier consulta que
    /// devuelva Customers debe usar esta constante.
    ///
    /// `&'static str` significa: una referencia a un string que vive
    /// durante todo el programa (lección 450, lifetimes). Los literales
    /// de texto en Rust son siempre `'static` porque están incrustados
    /// en el binario.
    pub const COLUMNS: &'static str = "CustomerID, CompanyName, ContactName, ContactTitle, \
         Address, City, Region, PostalCode, Country, Phone, Fax";
}

// ═══════════════════════════════════════════════════════════════════
//  NewCustomer — el payload de entrada del POST
// ═══════════════════════════════════════════════════════════════════

/// Payload for creating a customer (`POST /customers`).
///
/// 🇪🇸 NOTA: ¿por qué un struct separado en vez de reutilizar `Customer`?
///
/// Es una decisión de diseño que conviene poder defender:
///
/// 1. Solo deriva `Deserialize`, no `Serialize`. Este tipo únicamente
///    ENTRA a la API; nunca sale. El compilador impide que lo devuelvas
///    por error en una respuesta.
///
/// 2. Deja explícito el contrato de entrada. Si mañana el modelo de
///    lectura gana un campo calculado (por ejemplo, número de pedidos),
///    ese campo no debe poder mandarse en un POST. Con structs separados
///    eso es imposible por construcción, no por disciplina.
///
/// 3. Aquí el ID viene del cliente porque en Northwind `CustomerID` es
///    texto de 5 caracteres elegido por el usuario (ALFKI, ANATR), no un
///    autoincremental. Si la tabla usara un ID generado por la base, este
///    struct simplemente no tendría ese campo — y ahí se vería aún más
///    claro por qué conviene separarlos.
///
/// ⚠️ 🇪🇸 NOTA (`#[allow(dead_code)]` con fecha de caducidad): este struct todavía no lo
/// usa nadie — su ruta llega en el siguiente paso. El atributo evita que un warning de
/// código muerto, que ya sabemos que está ahí, ahogue a los warnings REALES. Está sobre
/// el struct concreto y no sobre el módulo a propósito: así solo silencia lo que nombra,
/// y se borra en el mismo commit que implemente la ruta.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCustomer {
    pub customer_id: String,
    pub company_name: String,
    pub contact_name: Option<String>,
    pub contact_title: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
//  UpdateCustomer — el payload de entrada del PUT
// ═══════════════════════════════════════════════════════════════════

/// Payload for updating a customer (`PUT /customers/<id>`).
///
/// 🇪🇸 NOTA: no lleva `customer_id`. El ID viene en la URL, no en el
/// cuerpo. Si estuviera en ambos sitios tendrías que decidir qué hacer
/// cuando no coinciden — un caso borde que se evita no creándolo.
///
/// ⚠️ 🇪🇸 NOTA (`#[allow(dead_code)]` con fecha de caducidad): este struct todavía no lo
/// usa nadie — su ruta llega en el siguiente paso. El atributo evita que un warning de
/// código muerto, que ya sabemos que está ahí, ahogue a los warnings REALES. Está sobre
/// el struct concreto y no sobre el módulo a propósito: así solo silencia lo que nombra,
/// y se borra en el mismo commit que implemente la ruta.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomer {
    pub company_name: String,
    pub contact_name: Option<String>,
    pub contact_title: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
//  Respuesta paginada
// ═══════════════════════════════════════════════════════════════════

/// Paginated response wrapper for `GET /customers`.
///
/// 🇪🇸 NOTA: el frontend necesita saber cuántos registros hay EN TOTAL
/// para dibujar el paginador ("página 3 de 10"). Si el endpoint
/// devolviera solo el array de 10 clientes, la tabla de Material-UI no
/// podría calcular cuántas páginas mostrar.
///
/// `<T>` lo hace genérico (lección 380): esta misma estructura sirve
/// para paginar clientes, pedidos o cualquier otra cosa. La restricción
/// `T: Serialize` dice "solo acepto tipos que sepan convertirse a JSON".
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paginated<T: Serialize> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}
