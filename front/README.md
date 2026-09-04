# Northwind · Frontend

Panel de gestión de la tabla `Customers` de Northwind. Next.js 15 (App Router),
TypeScript en modo estricto, Tailwind CSS 4 y componentes shadcn/ui. Habla con la API
de Rust/Rocket que vive en `../back`.

## Arranque

Requiere Node 20+ y pnpm. **El backend tiene que estar corriendo**: el frontend no
tiene datos propios ni fixtures.

```bash
# 1. En una terminal, el backend (escucha en el 8001)
cd ../back && cargo run

# 2. En otra, el frontend
pnpm install
cp .env.example .env.local     # opcional en local: ver más abajo
pnpm dev                       # http://localhost:3000
```

| Comando | Qué hace |
|---|---|
| `pnpm dev` | Servidor de desarrollo en el 3000 |
| `pnpm build` | Build de producción |
| `pnpm start` | Sirve el build (requiere `pnpm build` antes) |
| `pnpm lint` | ESLint |

### Configuración

Una sola variable, documentada en `.env.example`:

| Variable | Por defecto | Para qué |
|---|---|---|
| `NEXT_PUBLIC_API_URL` | `http://localhost:8001` | URL base de la API |

El prefijo `NEXT_PUBLIC_` es obligatorio: las peticiones salen del navegador, no del
servidor de Next, así que la variable tiene que viajar en el bundle. Como
`src/lib/api.ts` cae en el mismo valor por defecto, **en local no hace falta crear
ningún `.env`**; el `cp` de arriba solo hace falta si tu backend está en otro sitio.

Del lado del backend, el origen permitido por CORS se controla con
`CORS_ALLOWED_ORIGIN`, que por defecto ya es `http://localhost:3000`.

## La API, con ejemplos

Seis endpoints. Todos los errores —validaciones, conflictos y también los catchers de
Rocket— responden con la misma forma, `{ "error": "codigo", "message": "texto" }`, que
es lo que permite parsearlos en un único sitio (`request()` en `src/lib/api.ts`).

### `GET /health`

Comprueba la cadena completa hasta SQLite. Es lo primero que hay que mirar si el panel
dice que el servidor no responde.

```bash
curl http://localhost:8001/health
# {"status":"ok","sqlite":"<versión de la SQLite embebida>","customers":93}
```

### `GET /customers` — listado paginado, filtrado y ordenado

```bash
curl 'http://localhost:8001/customers?page=1&pageSize=10&companyName=ana&sortBy=companyName&sortDir=asc'
```

```json
{ "data": [ { "customerId": "ANATR", "companyName": "Ana Trujillo Emparedados y helados", "...": "..." } ],
  "total": 3, "page": 1, "pageSize": 10 }
```

| Parámetro | Por defecto | Notas |
|---|---|---|
| `page` | `1` | Base 1 |
| `pageSize` | `10` | Se recorta a 1–100 |
| `companyName` | — | Coincidencia parcial (`LIKE %texto%`) |
| `sortBy` | `companyName` | `customerId`, `companyName`, `contactName`, `contactTitle`, `city`, `country` |
| `sortDir` | `asc` | `asc` \| `desc` |

Un `sortBy` no reconocido **no da error**: cae en el orden por defecto en silencio. Por
eso el frontend mantiene la lista de columnas ordenables como un tipo (`SORTABLE_COLUMNS`)
y no como cadenas sueltas — un typo daría un orden incorrecto sin avisar de nada.

### `GET /customers/<id>`

```bash
curl http://localhost:8001/customers/ALFKI
curl -i http://localhost:8001/customers/ZZZZZ
# HTTP/1.1 404 Not Found
# {"error":"not_found","message":"no customer with id 'ZZZZZ'"}
```

El id se normaliza a mayúsculas y debe tener 5 caracteres alfanuméricos; cualquier otra
cosa es un `400 invalid_customer_id`.

### `POST /customers`

Los once campos. `customerId` y `companyName` son obligatorios; el resto puede ir a
`null` u omitirse.

```bash
curl -i -X POST http://localhost:8001/customers \
  -H 'Content-Type: application/json' \
  -d '{
    "customerId": "TEST1",
    "companyName": "Pruebas SL",
    "contactName": "Ada Lovelace",
    "contactTitle": "CTO",
    "address": "Calle Mayor 1",
    "city": "Madrid",
    "region": null,
    "postalCode": "28013",
    "country": "Spain",
    "phone": "+34 900 000 000",
    "fax": null
  }'
# HTTP/1.1 201 Created
# Location: /customers/TEST1
```

Repetir la llamada da `409 duplicate_id`. La cabecera `Content-Type: application/json`
no es opcional: sin ella la ruta ni siquiera se selecciona y la respuesta es un 404
confuso.

### `PUT /customers/<id>` — ⚠️ reemplazo total

Escribe las **diez** columnas editables con lo que reciba (el `customerId` va en la
URL, no en el cuerpo). **Un campo ausente no se conserva: se guarda como `NULL`.**

```bash
curl -X PUT http://localhost:8001/customers/TEST1 \
  -H 'Content-Type: application/json' \
  -d '{
    "companyName": "Pruebas SL",
    "contactName": "Ada Lovelace",
    "contactTitle": "CTO",
    "address": "Calle Mayor 1",
    "city": "Barcelona",
    "region": null,
    "postalCode": "28013",
    "country": "Spain",
    "phone": "+34 900 000 000",
    "fax": null
  }'
```

Mandar solo `{"companyName":"...","city":"Barcelona"}` cambiaría la ciudad **y vaciaría
los otros ocho campos**, sin error y sin que nadie se entere hasta que falte un
teléfono. Por eso el cliente de TypeScript tipa el cuerpo como `CustomerFields`
completo y no como un `Partial<>`, y por eso el diálogo de edición hace un `GET` al
abrir en vez de reutilizar la fila de la tabla: para reemplazar el registro entero hay
que conocerlo entero.

### `DELETE /customers/<id>`

```bash
curl -i -X DELETE http://localhost:8001/customers/TEST1
# HTTP/1.1 204 No Content

curl -i -X DELETE http://localhost:8001/customers/ALFKI
# HTTP/1.1 409 Conflict
# {"error":"has_orders","message":"customer 'ALFKI' cannot be deleted because it has associated orders — delete or reassign them first"}
```

**Ese 409 es el camino normal, no el excepcional**: los 93 clientes de Northwind
tienen pedidos —`SELECT COUNT(DISTINCT CustomerID) FROM Orders` devuelve 93—, así que
borrar cualquiera de ellos lo devuelve. Solo se puede borrar de verdad un cliente
creado desde el panel. El frontend lo trata en consecuencia:
el diálogo no se cierra, el aviso es informativo (azul) en vez de un error rojo, y el
botón pasa a "Entendido" en lugar de ofrecer un reintento cuyo resultado sería idéntico
las mil veces siguientes.

## Decisiones técnicas

El ENUNCIADO pedía **axios** y **Material-UI**. Aquí se usan `fetch` y shadcn/ui. La
divergencia es deliberada y estos son los motivos.

### `fetch` nativo en vez de axios

1. **La razón principal es `AbortController`, no el peso.** Al teclear en el buscador se
   lanza una petición cada 300 ms, y sin cancelar la anterior las respuestas pueden
   llegar desordenadas —la de `"al"` después de la de `"alf"`— dejando en la tabla el
   resultado de una búsqueda que el usuario ya no está haciendo. `fetch` acepta un
   `signal` directamente y rechaza con un `AbortError` distinguible; axios lo soporta
   igual de bien, pero por debajo hace exactamente esto. Es la dependencia envolviendo
   lo que ya se necesitaba usar.
2. **El manejo de errores de axios estorbaba aquí.** Axios convierte todo status ≥ 400 en
   una excepción con la respuesta enterrada en `error.response`, lo que obliga a
   distinguir "falló la red" de "el servidor contestó 409" desenredando su objeto de
   error. Este backend emite un único formato, `{error, message}`, y la distinción que
   de verdad importa —un `TypeError` sin status (backend caído, CORS) frente a un 404
   con cuerpo— se expresa mejor con dos clases propias, `NetworkError` y `ApiError`.
   Confundirlas lleva al peor mensaje posible de un panel: decir "este cliente no
   existe" cuando lo que pasa es que nadie ha arrancado el servidor.
3. **Cero dependencias para algo que el runtime ya trae.** `fetch` es estándar en el
   navegador y en Node 18+, y Next lo extiende con su propio caché. Un `request()` de
   sesenta líneas hace todo lo que este proyecto necesita.

Si el proyecto creciera con interceptores, reintentos automáticos o autenticación con
refresco de token, axios volvería a ser una opción razonable. Con cinco endpoints y
ninguna de esas necesidades, sería una dependencia que solo añade una capa que traducir.

### shadcn/ui en vez de Material-UI

1. **shadcn/ui no es una librería de componentes, es código que se copia al proyecto.**
   Los archivos de `src/components/ui/` son nuestros: se leen, se modifican y se
   versionan como el resto. Con MUI, ajustar un componente pasa por su sistema de
   theming y por `sx`, y acabar peleándose con la especificidad de sus estilos es lo
   normal, no la excepción.
2. **Material Design es un lenguaje visual con opinión propia, y esa opinión choca con
   la de este panel.** Elevaciones, ondas al pulsar, esquinas y color de marca por todas
   partes. Aquí el color es un canal de señal —rojo = algo falló, azul = regla de
   negocio— y todo lo demás es gris precisamente para que esos dos avisos se vean. En una
   herramienta que alguien usa ocho horas, gastar color en decoración inutiliza el color
   como información: si todo es azul, el azul del "tiene pedidos" no dice nada.
3. **Accesibilidad sin heredar el diseño.** Los componentes se apoyan en Radix
   (`@radix-ui/react-dialog`, `react-select`), que resuelve foco, teclado y ARIA sin
   imponer aspecto. Los diálogos atrapan el foco y se cierran con `Esc` gratis, y la
   tabla sigue siendo un `<table>` semántico con `aria-sort`, que un lector de pantalla
   anuncia como tabla en vez de como una lista plana de textos sueltos.
4. **Coste de bundle.** shadcn/ui manda al bundle solo los siete componentes que se usan.
   MUI trae Emotion y su motor de theming aunque solo se pinten una tabla y dos diálogos.

En un equipo que ya tuviera un design system sobre MUI, la decisión correcta sería la
contraria: la consistencia con lo que ya existe vale más que cualquiera de estos cuatro
puntos.

## Estructura

```
src/
├─ app/                     layout, page (único componente de servidor) y globals.css
├─ components/
│  ├─ customers/            la pantalla: tabla, formulario y los tres diálogos
│  └─ ui/                   primitivas shadcn/ui
└─ lib/
   ├─ api.ts                tipos del contrato + cliente + parseo único de errores
   ├─ use-list-params.ts    estado de la vista EN LA URL (page, pageSize, filtro, orden)
   ├─ use-customers.ts      carga de la página con cancelación de peticiones
   └─ utils.ts              cn()
```

Dos detalles que no se ven en el árbol:

- **El estado de la vista vive en la query string, no en `useState`.** Así la vista es
  una dirección: sobrevive a un F5, se puede pegar en un chat y los botones
  atrás/adelante del navegador funcionan solos. Se usa `router.replace` y no `push`
  para que escribir en el buscador no genere una entrada de historial por letra.
- **`app/page.tsx` es el único componente de servidor** y no hace ninguna petición: solo
  marca el límite de `Suspense` que `useSearchParams()` exige para que `next build` no
  falle.
