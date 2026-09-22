# syntax=docker/dockerfile:1

# ═══════════════════════════════════════════════════════════════════
#  STAGE 1 — compilación
# ═══════════════════════════════════════════════════════════════════
#
# 🇪🇸 NOTA (versión fijada, no `rust:latest`): el toolchain con el que se desarrolla
# es 1.98 y la edition está clavada en 2021 a propósito (ver Cargo.toml). Un
# `latest` convertiría cada build en una tirada de dados sobre la versión del
# compilador, que es justo lo que no se quiere de un artefacto de despliegue.
FROM rust:1.98-bookworm AS builder

WORKDIR /build

# ── Caché de capas: primero los manifiestos, después el código ──────
#
# 🇪🇸 NOTA (por qué este baile de un main.rs falso): Docker cachea capa a capa e
# invalida una capa y TODAS las siguientes en cuanto cambia algo de lo que copia.
# Si se copiara `src/` antes de compilar, tocar una línea de un handler tiraría la
# caché de las dependencias y volvería a compilar el árbol entero — que aquí
# incluye compilar SQLite desde su código C, la parte más lenta con diferencia.
#
# Copiando SOLO Cargo.toml y Cargo.lock, esta capa sobrevive mientras no cambien
# las dependencias, y un cambio de código recompila únicamente el crate propio.
COPY Cargo.toml Cargo.lock ./

RUN mkdir src \
 && echo "fn main() {}" > src/main.rs \
 && cargo build --release \
 && rm -rf src

# Ahora sí, el código real. A partir de aquí la caché se invalida en cada cambio,
# que es exactamente lo que se quiere.
COPY src ./src

# ⚠️ El `touch` no es decorativo: en target/ quedó el binario del main.rs falso con
# el mismo nombre, y Cargo decide qué recompilar por huella. Forzar la marca de
# tiempo garantiza que el crate propio se reconstruya sin invalidar la huella de
# las dependencias, que es lo que se acaba de cachear.
#
# `strip` quita los símbolos de depuración: el binario pasa de decenas de MB a unos
# pocos, y en una imagen de despliegue no hay nadie que vaya a leer un backtrace
# simbolizado.
RUN touch src/main.rs \
 && cargo build --release \
 && strip target/release/back

# ═══════════════════════════════════════════════════════════════════
#  STAGE 2 — imagen final
# ═══════════════════════════════════════════════════════════════════
#
# 🇪🇸 NOTA (por qué NO se instala libsqlite3-dev ni nada de SQLite): rusqlite se
# compila con la feature `bundled`, que compila SQLite desde su código C y lo enlaza
# ESTÁTICAMENTE dentro del binario. No hay ninguna librería de SQLite que buscar en
# tiempo de ejecución. Lo único que el binario pide del sistema es la libc de Debian
# y un par de librerías básicas, que bookworm-slim ya trae.
#
# Se comprueba, no se supone:
#   docker run --rm --entrypoint ldd northwind-api /usr/local/bin/back
# y en la lista no aparece ningún libsqlite3.
FROM debian:bookworm-slim

WORKDIR /app

COPY --from=builder /build/target/release/back /usr/local/bin/back

# ── La base de datos viaja DENTRO de la imagen ──────────────────────
#
# 🇪🇸 DECISIÓN CONSCIENTE: no hay volumen persistente. La imagen es autocontenida y
# la base de datos es parte del artefacto.
#
# La consecuencia hay que tenerla clara: **lo que se escriba en producción se pierde
# en el siguiente despliegue**, porque cada deploy trae de vuelta el northwind.db
# original. Para esta demo eso es una VENTAJA — cualquiera puede crear, editar y
# borrar clientes sin miedo, y un redeploy devuelve la base a su estado conocido de
# 93 clientes, sin ningún paso manual de limpieza.
#
# Si esto tuviera que guardar datos de verdad, esta línea sería el error más grave
# del archivo y habría que montar un volumen de Fly en /app (o, más honestamente,
# dejar SQLite y usar una base gestionada).
#
# La ruta importa: main.rs abre "northwind.db" RELATIVO al directorio de trabajo,
# así que el archivo tiene que estar en el WORKDIR y no en cualquier otro sitio.
#
# ── Por qué se DESCARGA y no se copia del contexto ──────────────────
#
# 🇪🇸 `northwind.db` está en .gitignore: no viaja en el repositorio. Un `COPY
# northwind.db` obliga a que el archivo exista en el disco de quien construye, lo
# que rompe cualquier build hecho a partir de un clon limpio — que es exactamente
# lo que hace una plataforma que construye desde GitHub. Descargándolo aquí, la
# imagen se puede construir desde el repo y nada más.
#
# ⚠️ El `--checksum` NO es opcional: sin él esto sería "ejecuta en producción el
# archivo que hoy haya en una rama de un tercero". Con el sha256 fijado, si el
# archivo remoto cambia —actualización del upstream, rama movida, respuesta
# manipulada— el build FALLA en esta línea en vez de desplegar una base distinta a
# la que se probó. Cambiar de versión de la base es entonces un cambio explícito de
# esta línea, revisable en el diff.
#
# El hash se calculó sobre una descarga fresca de esta URL, NO sobre el
# northwind.db local: ese ya recibió escrituras de las pruebas de CRUD y tiene otro
# sha256 (una base SQLite cambia de bytes en cuanto se escribe en ella).
ADD --checksum=sha256:2f4f5c68dfcd33ba27373eae48c7a4869800c68095ee0f9f0da494f83382a877 \
    https://github.com/jpwhite3/northwind-SQLite3/raw/main/dist/northwind.db \
    ./northwind.db

# ═══════════════════════════════════════════════════════════════════
#  Variables de entorno
# ═══════════════════════════════════════════════════════════════════
#
# ⚠️⚠️ ESTA LÍNEA ES LA QUE HACE QUE EL CONTENEDOR SEA ALCANZABLE ⚠️⚠️
#
# Rocket escucha en 127.0.0.1 por defecto. Dentro de un contenedor eso es el
# loopback DEL CONTENEDOR: el servidor arranca, imprime su "Rocket has launched
# from http://127.0.0.1:8001" tan contento, responde perfectamente a un curl hecho
# desde dentro... y rechaza toda conexión que venga de fuera.
#
# El síntoma es especialmente cruel porque NO HAY NINGÚN ERROR EN LOS LOGS: los
# logs dicen que todo va bien, porque desde el punto de vista de Rocket todo va
# bien. Desde fuera solo se ve "connection refused", y en Fly.io, health checks que
# fallan sin explicación.
#
# 0.0.0.0 significa "escucha en todas las interfaces", que es lo que hace falta para
# que el port forwarding de Docker y el proxy de Fly puedan entrar.
ENV ROCKET_ADDRESS=0.0.0.0

# 🇪🇸 NOTA (por qué PORT y NO ROCKET_PORT): main.rs lee `PORT` y lo fusiona en el
# figment, y en Figment un `merge` tiene MÁS precedencia que las variables de entorno
# — así que `ROCKET_PORT` NO movería el puerto: el merge lo pisaría. `PORT` es la única
# variable que decide el puerto, y además es la convención que usan las plataformas de
# despliegue para comunicar el puerto que han asignado.
#
# Este valor es solo el DEFAULT de la imagen, y coincide con el que main.rs trae por su
# cuenta: sirve para documentar en qué puerto escucha si nadie dice nada. Una plataforma
# que inyecte su propio `PORT` lo sobreescribe sin reconstruir la imagen.
ENV PORT=8001

# 🇪🇸 CORS_ALLOWED_ORIGIN se deja DELIBERADAMENTE sin definir aquí.
#
# Si tuviera un valor por defecto en la imagen, cambiar el origen permitido —al
# desplegar el frontend, al cambiar de dominio— obligaría a reconstruir y volver a
# desplegar la imagen entera para cambiar una cadena de texto. Se configura como
# secreto de Fly, que se puede cambiar en caliente:
#
#   fly secrets set CORS_ALLOWED_ORIGIN=https://mi-app.vercel.app
#
# Sin definirla, cors.rs cae en su propio valor por defecto, http://localhost:3000,
# que es justo lo que hace falta para desarrollo local.

# Documenta el puerto para quien lea la imagen; no publica nada por sí solo.
EXPOSE 8001

CMD ["back"]
