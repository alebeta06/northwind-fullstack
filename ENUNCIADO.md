# ENUNCIADO — Customer Management (Rust Rocket + Next.js)

## Qué tienes que construir

Una aplicación full-stack de **gestión de clientes**: API REST en **Rust con Rocket** sobre la base de datos Northwind (SQLite) y frontend en **Next.js**. El `README.md` de este directorio describe la aplicación de referencia.

## Backend (carpeta `back/`) — Rust + Rocket

Endpoints CRUD sobre la tabla `Customers` de Northwind:

| Método | Ruta | Función |
|---|---|---|
| GET | `/customers` | Listado con paginación, filtro por nombre de empresa y ordenación |
| GET | `/customers/<id>` | Un cliente por ID |
| POST | `/customers` | Crear cliente |
| PUT | `/customers/<id>` | Actualizar cliente |
| DELETE | `/customers/<id>` | Borrar cliente |

Requisitos técnicos:
- `rusqlite` para SQLite con acceso thread-safe (Mutex).
- Serialización JSON con `serde`.
- CORS habilitado; servidor en el puerto 8001.
- Validación de entrada y manejo de errores (404, 400, 500 coherentes).

## Frontend (carpeta `front/`) — Next.js

- Listado de clientes con paginación.
- Búsqueda por nombre de empresa y ordenación por columnas.
- Alta, edición y borrado con formularios validados.
- Axios contra el backend, URL configurable por `.env`.
- Material-UI y diseño responsive.

## Plan de trabajo

- [ ] Descargar la BD Northwind SQLite y colocarla en la raíz del backend.
- [ ] Backend: modelo Customer + GET con paginación/filtro/orden.
- [ ] Backend: POST/PUT/DELETE con validaciones.
- [ ] Frontend: tabla con paginación, búsqueda y ordenación.
- [ ] Frontend: formularios de alta/edición y confirmación de borrado.
- [ ] Prueba end-to-end del CRUD completo.

## Entregables

1. Repositorio con `back/` (`cargo run` en :8001) y `front/` (`npm run dev` en :3000).
2. CRUD completo funcionando contra Northwind.
3. README propio con instrucciones de arranque y ejemplos de las llamadas API (curl).

## Evaluación

| Criterio | Peso |
|---|---|
| API REST completa y correcta (paginación, filtro, orden) | 40% |
| Calidad del Rust (errores, tipos, thread-safety) | 20% |
| Frontend funcional y validado | 30% |
| Documentación | 10% |
