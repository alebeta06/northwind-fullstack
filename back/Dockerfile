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
COPY northwind.db ./northwind.db

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

# 🇪🇸 NOTA (este ENV es coherencia, no el mecanismo): el puerto real lo fija main.rs
# con `Config::figment().merge(("port", PORT))`, y en Figment un `merge` tiene MÁS
# precedencia que las variables de entorno — o sea que ROCKET_PORT no puede mover el
# puerto por sí solo. Se declara igualmente porque documenta el puerto en el que
# escucha la imagen y porque tiene que coincidir con `internal_port` de fly.toml.
# Para cambiar el puerto de verdad hay que tocar la constante PORT de main.rs.
ENV ROCKET_PORT=8001

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
